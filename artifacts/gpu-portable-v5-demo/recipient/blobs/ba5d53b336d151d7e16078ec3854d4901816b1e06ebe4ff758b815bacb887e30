"""CPU v5 prototype: donor exits, exported state continues in a fresh process.

Run: python examples/gpu/portable_training.py --out artifacts/gpu-portable-demo
Requires PyTorch and NumPy in this interpreter; creates no v4 runtime ledger.
"""

import argparse
import json
import os
import random
import subprocess
import sys
import time
from pathlib import Path

import numpy as np
import torch

sys.path.insert(0, str(Path(__file__).resolve().parents[2] / "src"))

from kernelledger.artifacts import ArtifactStore  # noqa: E402
from kernelledger.experiments import (  # noqa: E402
    capture_training,
    materialize_source,
    restore_training,
)


def build():
    model = torch.nn.Sequential(torch.nn.Linear(3, 4), torch.nn.Dropout(0.2), torch.nn.Linear(4, 1))
    optimizer = torch.optim.Adam(model.parameters(), lr=0.01)
    scheduler = torch.optim.lr_scheduler.StepLR(optimizer, step_size=1, gamma=0.95)
    return {"model": model, "optimizer": optimizer, "scheduler": scheduler}


def train(objects):
    batch = torch.randn(5, 3) + random.random() + float(np.random.rand())
    objects["optimizer"].zero_grad()
    loss = objects["model"](batch).square().mean()
    loss.backward()
    objects["optimizer"].step()
    objects["scheduler"].step()
    return loss.item()


def donor(root):
    objects = build()
    train(objects)
    store = ArtifactStore(root / "donor")
    start = time.perf_counter()
    digest = capture_training(
        store,
        **objects,
        step=1,
        cursor={"batch": 1},
        source_root=Path(__file__).parent,
        files=[Path(__file__).name],
        recipe_source=Path(__file__).name,
        factory="build",
        boundary="idle",
    )
    capture_seconds = time.perf_counter() - start
    store.export(digest, root / "export")
    expected = {
        "snapshot": digest,
        "loss": train(objects),
        "parameters": {
            name: tensor.tolist() for name, tensor in objects["model"].state_dict().items()
        },
        "capture_seconds": capture_seconds,
        "stored_bytes": store._usage(),
    }
    (root / "expected.json").write_text(json.dumps(expected), encoding="utf-8")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--donor", action="store_true", help=argparse.SUPPRESS)
    args = parser.parse_args()
    root = args.out.resolve()
    if args.donor:
        donor(root)
        return
    root.mkdir(parents=True, exist_ok=False)
    subprocess.run(
        [sys.executable, str(Path(__file__).resolve()), "--out", str(root), "--donor"],
        check=True,
        creationflags=0x08000000 if os.name == "nt" else 0,
    )
    # The donor process has terminated before reading or restoring anything.
    expected = json.loads((root / "expected.json").read_text(encoding="utf-8"))
    recipient = ArtifactStore(root / "recipient")
    digest = recipient.import_directory(root / "export")
    materialize_source(recipient, digest, root / "source")
    start = time.perf_counter()
    restored = restore_training(recipient, digest, build, trust=True)
    restore_seconds = time.perf_counter() - start
    loss = train(restored["objects"])
    if loss != expected["loss"] or any(
        tensor.tolist() != expected["parameters"][name]
        for name, tensor in restored["objects"]["model"].state_dict().items()
    ):
        raise AssertionError("CPU training continuation differs after donor exit")
    print(
        json.dumps(
            {
                "snapshot": digest,
                "donor_exited": True,
                "mode": restored["mode"],
                "training_continuation_equal": True,
                "loss": loss,
                "capture_seconds": expected["capture_seconds"],
                "restore_seconds": restore_seconds,
                "stored_bytes": expected["stored_bytes"],
                "native_qualified": False,
            },
            indent=2,
        )
    )


if __name__ == "__main__":
    main()
