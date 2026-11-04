"""Paired local supervisor comparison on disposable notebooks; not a release gate."""

import argparse
import hashlib
import json
import os
import sys
import time
from pathlib import Path

from runtime_benchmark import identity, notebook_directory, resident, summary, wait_exit

from kernelledger.cli import ensure_supervisor, rpc
from kernelledger.notebook import Notebook


def measure(mode, daemon, mib, controls):
    with notebook_directory() as temporary:
        root = Path(temporary)
        Notebook(root).init()
        row = {"backend": mode}
        pid = None
        try:
            os.environ["KERNELLEDGER_RUNTIME"] = "python" if mode == "python" else "native"
            os.environ["KERNELLEDGER_DAEMON"] = str(daemon)
            started = time.perf_counter()
            ensure_supervisor(root, sys.executable)
            row["startup_seconds"] = time.perf_counter() - started
            status = rpc(root, {"command": "status"})
            pid = status["pid"]
            actual = status.get("implementation", "python")
            assert actual == ("python" if mode == "python" else "rust"), status
            source = (
                f"payload = bytes(range(256)) * ({mib} * 4096)\nalias = payload\n"
                "__import__('pathlib').Path('worker.pid').write_text(str(__import__('os').getpid()))"
            )
            assert rpc(root, {"command": "run", "source": source})["status"] == "ok"
            row["idle_owner_rss_bytes"] = resident(pid)
            row["idle_worker_rss_bytes"] = resident(int((root / "worker.pid").read_text()))
            for label, request in (
                ("status_seconds", {"command": "status"}),
                ("short_cell_seconds", {"command": "run", "source": "pass"}),
            ):
                values = []
                for _ in range(controls):
                    started = time.perf_counter()
                    result = rpc(root, request)
                    values.append(time.perf_counter() - started)
                    if label == "short_cell_seconds":
                        assert result["status"] == "ok", result
                row[label] = values
            started = time.perf_counter()
            checkpoint = rpc(root, {"command": "checkpoint"})
            row["checkpoint_seconds"] = time.perf_counter() - started
            row["checkpoint_timings"] = checkpoint.get("timings")
            assert checkpoint.get("checkpoint_id") and checkpoint["status"] == "complete", (
                checkpoint
            )
            started = time.perf_counter()
            unchanged = rpc(root, {"command": "checkpoint"})
            row["unchanged_checkpoint_seconds"] = time.perf_counter() - started
            row["unchanged_checkpoint_timings"] = unchanged.get("timings")
            assert unchanged.get("checkpoint_id"), unchanged
            row["unchanged_new_blob_bytes"] = unchanged.get("new_blob_bytes")
            row["post_checkpoint_owner_rss_bytes"] = resident(pid)
            started = time.perf_counter()
            restored = rpc(
                root, {"command": "restore", "checkpoint_id": checkpoint["checkpoint_id"]}
            )
            row["restore_seconds"] = time.perf_counter() - started
            row["restore_timings"] = restored.get("timings")
            assert not restored.get("excluded"), restored
            checked = rpc(
                root,
                {
                    "command": "run",
                    "source": f"assert payload is alias\nassert len(payload) == {mib} * 1024**2\nassert payload[-1] == 255",
                },
            )
            assert checked["status"] == "ok", checked
            row["restore_verified"] = True
            row["post_restore_owner_rss_bytes"] = resident(pid)
        finally:
            if (root / ".kl/supervisor.json").exists():
                rpc(root, {"command": "stop"})
            if pid:
                wait_exit(pid)
        return row


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--daemon", required=True, type=Path)
    parser.add_argument("--repetitions", type=int, default=5)
    parser.add_argument("--controls", type=int, default=30)
    parser.add_argument("--mib", type=int, default=32)
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--build-label", default="unspecified local build")
    parser.add_argument(
        "--baseline-daemon",
        type=Path,
        help="Interleave a frozen earlier native binary with Python and the current native binary",
    )
    parser.add_argument("--profile", action="store_true")
    args = parser.parse_args()
    if args.repetitions < 1 or args.controls < 1 or not 1 <= args.mib <= 1024:
        parser.error("positive repetitions/controls and MiB 1..1024 required")
    daemon = args.daemon.resolve(strict=True)
    baseline = args.baseline_daemon.resolve(strict=True) if args.baseline_daemon else None
    modes = ["python", "native_baseline", "native"] if baseline else ["python", "native"]
    result = {
        "schema": "kernelledger.runtime-comparison/1",
        "environment": identity(),
        "daemon_sha256": hashlib.sha256(daemon.read_bytes()).hexdigest(),
        "baseline_daemon_sha256": hashlib.sha256(baseline.read_bytes()).hexdigest()
        if baseline
        else None,
        "daemon_build": args.build_label,
        "parameters": {
            "pairs": args.repetitions,
            "controls_per_pair": args.controls,
            "payload_mib": args.mib,
            "profile_requested": args.profile,
        },
        "limitations": [
            "Local microbenchmark; OS cache uncontrolled",
            "Interleaved backend order; no cold-cache claim",
            "Startup uses five samples by default: median/range only",
            "Idle RSS is per process, excludes transient utilities and is not peak or private memory",
            "Python worker used by both backends",
            "No UI/helper/feed/load/GPU or 256 MiB/1 GiB qualification",
        ],
        "samples": [],
    }
    previous = {
        key: os.environ.get(key)
        for key in ("KERNELLEDGER_RUNTIME", "KERNELLEDGER_DAEMON", "KERNELLEDGER_PROFILE")
    }
    try:
        if args.profile:
            os.environ["KERNELLEDGER_PROFILE"] = "1"
        for pair in range(args.repetitions):
            order = (
                modes[pair % len(modes) :] + modes[: pair % len(modes)] if baseline else modes[:]
            )
            if pair % 2:
                order.reverse()
            for mode in order:
                row = measure(
                    mode, baseline if mode == "native_baseline" else daemon, args.mib, args.controls
                )
                row["pair"] = pair
                result["samples"].append(row)
                args.out.parent.mkdir(parents=True, exist_ok=True)
                args.out.write_text(json.dumps(result, indent=2) + "\n")
                print(f"pair {pair + 1}/{args.repetitions}: {mode} verified", flush=True)
    finally:
        for key, value in previous.items():
            if value is None:
                os.environ.pop(key, None)
            else:
                os.environ[key] = value
    result["summary"] = {}
    for mode in modes:
        rows = [row for row in result["samples"] if row["backend"] == mode]
        result["summary"][mode] = {
            key: summary(
                [
                    value
                    for row in rows
                    for value in (row[key] if isinstance(row[key], list) else [row[key]])
                ]
            )
            for key in (
                "startup_seconds",
                "status_seconds",
                "short_cell_seconds",
                "checkpoint_seconds",
                "unchanged_checkpoint_seconds",
                "restore_seconds",
                "idle_owner_rss_bytes",
                "idle_worker_rss_bytes",
            )
        }
    args.out.write_text(json.dumps(result, indent=2) + "\n")
    print(json.dumps(result["summary"], indent=2))


if __name__ == "__main__":
    main()
