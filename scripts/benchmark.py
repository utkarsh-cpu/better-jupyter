"""Measure checkpoint walk, restoration, payload growth, and deduplication."""

import argparse
import json
import platform
import tempfile
import time
from pathlib import Path

import numpy as np

from kernelledger.worker import ExecutionIndex, GraphEncoder, GraphLoader


def main():
    p = argparse.ArgumentParser()
    p.add_argument("--mib", type=int, default=256)
    p.add_argument("--out", default="benchmarks/latest.json")
    p.add_argument("--supervised", action="store_true")
    args = p.parse_args()
    if args.supervised:
        return supervised(args)
    namespace = {"array": np.arange(args.mib * 1024**2 // 8, dtype=np.float64)}
    try:
        import pandas as pd
        import pyarrow  # noqa: F401

        namespace["table"] = pd.DataFrame(
            {"value": namespace["array"][: len(namespace["array"]) // 4]}
        )
    except ImportError:
        pass
    started = time.perf_counter()
    encoder = GraphEncoder(ExecutionIndex())
    manifest = encoder.checkpoint(namespace)
    walk_s = time.perf_counter() - started
    first_bytes = sum(map(len, encoder.blobs.values()))
    restored = {}
    started = time.perf_counter()
    report = GraphLoader(manifest, encoder.blobs, restored).load()
    restore_s = time.perf_counter() - started
    assert np.array_equal(namespace["array"], restored["array"])
    if "table" in namespace:
        assert namespace["table"].equals(restored["table"])
    second = GraphEncoder(ExecutionIndex())
    started = time.perf_counter()
    second.checkpoint(namespace)
    second_s = time.perf_counter() - started
    new_bytes = sum(len(v) for h, v in second.blobs.items() if h not in encoder.blobs)
    result = {
        "python": platform.python_version(),
        "platform": platform.platform(),
        "array_mib": args.mib,
        "walk_seconds": walk_s,
        "checkpoint_seconds": walk_s,
        "restore_seconds": restore_s,
        "first_blob_bytes": first_bytes,
        "unchanged_walk_seconds": second_s,
        "unchanged_new_blob_bytes": new_bytes,
        "excluded": report["excluded"],
    }
    Path(args.out).parent.mkdir(parents=True, exist_ok=True)
    Path(args.out).write_text(json.dumps(result, indent=2) + "\n", encoding="utf-8")
    print(json.dumps(result, indent=2))


def supervised(args):
    from kernelledger.cli import ensure_supervisor, rpc
    from kernelledger.notebook import Notebook

    with tempfile.TemporaryDirectory() as tmp:
        root = Path(tmp)
        Notebook(root).init()
        ensure_supervisor(root)
        try:
            source = f"import numpy as np\narray=np.arange({args.mib}*1024**2//8,dtype=np.float64)\nimport pandas as pd\ntable=pd.DataFrame({{'value':array[:len(array)//4]}})"
            assert rpc(root, {"command": "run", "source": source})["status"] == "ok"
            started = time.perf_counter()
            first = rpc(root, {"command": "checkpoint"})
            checkpoint_s = time.perf_counter() - started
            started = time.perf_counter()
            second = rpc(root, {"command": "checkpoint"})
            unchanged_s = time.perf_counter() - started
            started = time.perf_counter()
            restored = rpc(root, {"command": "restore", "checkpoint_id": first["checkpoint_id"]})
            restore_s = time.perf_counter() - started
            assert (
                rpc(
                    root,
                    {
                        "command": "run",
                        "source": "assert array[-1]==len(array)-1\nassert table['value'].iloc[-1]==len(table)-1",
                    },
                )["status"]
                == "ok"
            )
            result = {
                "mode": "supervised",
                "python": platform.python_version(),
                "platform": platform.platform(),
                "array_mib": args.mib,
                "checkpoint_seconds": checkpoint_s,
                "walk_seconds": first["walk_seconds"],
                "restore_seconds": restore_s,
                "first_blob_bytes": first["new_blob_bytes"],
                "unchanged_checkpoint_seconds": unchanged_s,
                "unchanged_new_blob_bytes": second["new_blob_bytes"],
                "excluded": restored["excluded"],
            }
            Path(args.out).parent.mkdir(parents=True, exist_ok=True)
            Path(args.out).write_text(json.dumps(result, indent=2) + "\n", encoding="utf-8")
            print(json.dumps(result, indent=2))
        finally:
            rpc(root, {"command": "stop"})
            for _ in range(100):
                if not (root / ".kl/supervisor.json").exists():
                    break
                time.sleep(0.02)


if __name__ == "__main__":
    main()
