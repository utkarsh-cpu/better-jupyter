"""Cooperative plugin qualification with a queued request and donor exit.

Run in the GPU environment with --device cuda:0 --out <fresh directory>.
"""

import argparse
import json
import os
import subprocess
import sys
import threading
import time
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[2] / "src"))

import torch  # noqa: E402
from portable_training import build, train  # noqa: E402

from kernelledger.artifacts import ArtifactStore  # noqa: E402
from kernelledger.experiments import materialize_source  # noqa: E402
from kernelledger.gpu.cooperative import (  # noqa: E402
    CheckpointBusy,
    CooperativeCheckpointPlugin,
)


def donor(root, device):
    objects = build()
    objects["model"].to(device)
    timing = {}

    def drain():
        # This fixture has no asynchronous loader; real applications must drain theirs.
        timing["capture_start"] = time.perf_counter()

    plugin = CooperativeCheckpointPlugin(
        ArtifactStore(root / "donor"),
        objects=objects,
        source_root=Path(__file__).parent,
        files=["cooperative_training.py", "portable_training.py"],
        recipe_source="portable_training.py",
        factory="build",
        cursor={"batch": 0},
        drain=drain,
    )
    requests = []
    with plugin.training_step(cursor={"batch": 1}) as registered:
        train(registered)
        control = threading.Thread(target=lambda: requests.append(plugin.request_checkpoint()))
        control.start()
        control.join(timeout=10)
        assert not control.is_alive() and len(requests) == 1
        assert not requests[0].done()
        try:
            plugin.checkpoint()
        except CheckpointBusy:
            pass
        else:
            raise AssertionError("capture entered an active training step")
    capture_seconds = time.perf_counter() - timing["capture_start"]
    digest = requests[0].result(timeout=10)
    captured_gradients = {
        name: None if parameter.grad is None else parameter.grad.tolist()
        for name, parameter in objects["model"].named_parameters()
    }
    plugin.store.export(digest, root / "export")
    with plugin.training_step(cursor={"batch": 2}) as registered:
        loss = train(registered)
    expected = {
        "snapshot": digest,
        "loss": loss,
        "parameters": {
            name: tensor.tolist() for name, tensor in objects["model"].state_dict().items()
        },
        "gradients_at_capture": captured_gradients,
        "capture_seconds": capture_seconds,
        "stored_bytes": plugin.store._usage(),
    }
    (root / "expected.json").write_text(json.dumps(expected), encoding="utf-8")
    plugin.close()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--device", default="cpu")
    parser.add_argument("--out", required=True, type=Path)
    parser.add_argument("--donor", action="store_true", help=argparse.SUPPRESS)
    args = parser.parse_args()
    root = args.out.resolve()
    if args.donor:
        donor(root, args.device)
        return
    root.mkdir(parents=True, exist_ok=False)
    subprocess.run(
        [
            sys.executable,
            str(Path(__file__).resolve()),
            "--device",
            args.device,
            "--out",
            str(root),
            "--donor",
        ],
        check=True,
        creationflags=0x08000000 if os.name == "nt" else 0,
    )
    # Donor has exited. Import, verify and restore without any live donor handles.
    expected = json.loads((root / "expected.json").read_text(encoding="utf-8"))
    store = ArtifactStore(root / "recipient")
    digest = store.import_directory(root / "export")
    materialize_source(store, digest, root / "source")
    start = time.perf_counter()
    restored = CooperativeCheckpointPlugin.from_snapshot(
        store,
        digest,
        build,
        source_root=root / "source",
        trust=True,
        device=args.device,
    )
    restore_seconds = time.perf_counter() - start
    assert restored.status["step"] == 1 and restored.status["cursor"] == {"batch": 1}
    for name, parameter in restored.objects["model"].named_parameters():
        actual = None if parameter.grad is None else parameter.grad.tolist()
        assert actual == expected["gradients_at_capture"][name]
    with restored.training_step(cursor={"batch": 2}) as registered:
        loss = train(registered)
    assert loss == expected["loss"]
    for name, tensor in restored.objects["model"].state_dict().items():
        assert tensor.tolist() == expected["parameters"][name]
    clone = CooperativeCheckpointPlugin.from_snapshot(
        store,
        digest,
        build,
        source_root=root / "source",
        trust=True,
        device=args.device,
    )
    original = next(restored.objects["model"].parameters()).detach().clone()
    with clone.training_step(cursor={"batch": 2}):
        with torch.no_grad():
            next(clone.objects["model"].parameters()).add_(1)
    assert torch.equal(original, next(restored.objects["model"].parameters()))
    report = {
        "plugin": "kernelledger.gpu.cooperative",
        "mode": "portable",
        "device": args.device,
        "torch": torch.__version__,
        "hip": torch.version.hip,
        "snapshot": digest,
        "donor_exited": True,
        "queued_safe_point": True,
        "busy_capture_refused": True,
        "gradients_equal": True,
        "training_continuation_equal": True,
        "independent_clone": True,
        "step": restored.status["step"],
        "cursor": restored.status["cursor"],
        "loss": loss,
        "capture_seconds": expected["capture_seconds"],
        "restore_seconds": restore_seconds,
        "stored_bytes": expected["stored_bytes"],
        "native_qualified": False,
    }
    (root / "result.json").write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
    print(json.dumps(report, indent=2))
    restored.close()
    clone.close()


if __name__ == "__main__":
    main()
