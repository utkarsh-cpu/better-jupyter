"""Measure the isolated native spike; deliberately not a parity benchmark."""

import argparse
import json
import subprocess
import sys
import tempfile
import threading
from pathlib import Path

from runtime_benchmark import identity, resident, summary


def measure(args):
    daemon = Path(args.daemon).resolve()
    result = {
        "schema": 1,
        "mode": "feasibility_probe",
        "parity_comparison": False,
        "environment": identity(),
        "binary_bytes": daemon.stat().st_size,
        "samples": [],
        "limitations": "No ledger, fsync publication, admission, helper or subscription service; restore reuses probe worker. Timings cannot establish a production speedup.",
    }
    for _ in range(args.repetitions):
        with tempfile.TemporaryDirectory(prefix="kl-native-benchmark-") as root:
            process = subprocess.Popen(
                [
                    str(daemon),
                    "probe",
                    "--project",
                    root,
                    "--python",
                    sys.executable,
                    "--worker",
                    str(Path(__file__).resolve().parents[1] / "src/kernelledger/worker.py"),
                    "--mib",
                    str(args.mib),
                ],
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                text=True,
            )
            peaks = []
            done = threading.Event()

            def sample():
                while not done.wait(0.02):
                    try:
                        peaks.append(resident(process.pid))
                    except OSError:
                        pass

            sampler = threading.Thread(target=sample, daemon=True)
            sampler.start()
            try:
                stdout, stderr = process.communicate(timeout=120)
                if process.returncode:
                    raise RuntimeError(stderr)
                row = json.loads(stdout)
                row["daemon_rss_peak_bytes"] = max(peaks) if peaks else None
                result["samples"].append(row)
            finally:
                if process.poll() is None:
                    process.kill()
                    process.wait()
                done.set()
                sampler.join()
    result["summary"] = {
        key: summary([s[key] for s in result["samples"]])
        for key in (
            "ready_seconds",
            "checkpoint_seconds",
            "restore_seconds",
            "daemon_rss_peak_bytes",
        )
    }
    out = Path(args.out)
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text(json.dumps(result, indent=2) + "\n", encoding="utf-8")
    print(json.dumps(result["summary"], indent=2))


if __name__ == "__main__":
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--daemon", required=True)
    p.add_argument("--mib", type=int, default=256)
    p.add_argument("--repetitions", type=int, default=5)
    p.add_argument("--out", default="benchmarks/runtime-v6/native-probe.json")
    args = p.parse_args()
    if not 1 <= args.mib <= 1024 or args.repetitions < 1:
        p.error("MiB must be 1..1024 and repetitions positive")
    measure(args)
