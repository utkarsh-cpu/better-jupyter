"""Measure local transient reads and priority cancellation in a disposable notebook."""

import argparse
import hashlib
import json
import math
import os
import platform
import statistics
import subprocess
import sys
import tempfile
import time
import uuid
from pathlib import Path

from kernelledger.cli import alive, rpc
from kernelledger.jupyter_adapter import RuntimeConnection
from kernelledger.notebook import Notebook, source_hash
from kernelledger.source import SourceError

ROOT = Path(__file__).resolve().parents[1]


def summary(samples):
    ordered = sorted(samples)
    return {
        "samples_ms": samples,
        "median_ms": statistics.median(samples),
        "p95_ms": ordered[max(0, math.ceil(len(samples) * 0.95) - 1)],
    }


def measure(project, samples):
    assert (
        rpc(
            project,
            {
                "command": "run",
                "source": "answer = 42\n" + "\n".join(f"variable_{i} = {i}" for i in range(1000)),
            },
        )["status"]
        == "ok"
    )
    client = RuntimeConnection(project, str(uuid.uuid4()))
    try:
        status = client.call("runtime.status")
        generation = status["interactive"]["worker_generation"]
        results = {}
        for method, fields in [
            (
                "code.complete",
                {"source": "var", "cursor": 3, "source_revision": source_hash("var")},
            ),
            ("code.inspect", {"name": "answer"}),
            ("namespace.list", {"page_size": 100}),
        ]:
            timings = []
            for _ in range(samples):
                started = time.perf_counter()
                result = client.call(method, worker_generation=generation, **fields)
                assert not result.get("error"), result
                timings.append((time.perf_counter() - started) * 1000)
            results[method] = summary(timings)
        source = "import time\nprint('x' * 2_000_000)\ntime.sleep(30)"
        accepted = client.call(
            "run",
            source=source,
            expected_sha256=source_hash(source),
            origin="console",
            client_request_id=str(uuid.uuid4()),
        )
        deadline = time.monotonic() + 10
        while client.call("runtime.status")["worker_state"] != "running":
            if time.monotonic() > deadline:
                raise TimeoutError("Probe execution did not start")
            time.sleep(0.01)
        timings = []
        for _ in range(min(samples, 20)):
            started = time.perf_counter()
            try:
                reply = client.call(
                    "code.complete",
                    worker_generation=generation,
                    source="ans",
                    cursor=3,
                    source_revision=source_hash("ans"),
                )
                assert reply.get("error", {}).get("code") == "busy", reply
            except SourceError as error:
                assert error.code == "busy", error
            timings.append((time.perf_counter() - started) * 1000)
        results["busy_completion_refusal"] = summary(timings)
        started = time.perf_counter()
        client.call("operation.cancel", operation_id=accepted["operation_id"])
        while True:
            operation = client.call("operation.get", operation_id=accepted["operation_id"])
            if operation["state"] in ("completed", "canceled", "failed", "unknown"):
                break
            if time.perf_counter() - started > 10:
                raise TimeoutError("Probe cancellation did not finish")
            time.sleep(0.01)
        results["cancel_with_output_ms"] = (time.perf_counter() - started) * 1000
        results["terminal"] = operation["result"].get("status", operation["state"])
        results["engine"] = status["interactive"]["engine"]
        return results
    finally:
        client.close()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--native-daemon", type=Path)
    parser.add_argument("--samples", type=int, default=50)
    args = parser.parse_args()
    if not 5 <= args.samples <= 1000:
        parser.error("samples must be between 5 and 1000")
    with tempfile.TemporaryDirectory(prefix="kl-interactivity-probe-") as directory:
        project = Path(directory)
        Notebook(project).init()
        process = None
        try:
            if args.native_daemon:
                command = [
                    str(args.native_daemon.resolve()),
                    "serve-experimental",
                    "--project",
                    str(project),
                    "--python",
                    sys.executable,
                ]
            else:
                command = [sys.executable, "-m", "kernelledger", "-C", str(project), "serve"]
            process = subprocess.Popen(
                command,
                stdin=subprocess.DEVNULL,
                stdout=subprocess.DEVNULL,
                stderr=subprocess.DEVNULL,
                env={**os.environ, "PYTHONPATH": str(ROOT / "src")},
                creationflags=0x08000000 if os.name == "nt" else 0,
            )
            deadline = time.monotonic() + 20
            while not (project / ".kl/supervisor.json").exists():
                if process.poll() is not None or time.monotonic() > deadline:
                    raise RuntimeError("Probe owner failed startup")
                time.sleep(0.02)
            report = {
                "schema": "kernelledger.interactivity-probe/1",
                "platform": platform.platform(),
                "python": sys.version,
                "owner": "rust" if args.native_daemon else "python",
                "worker_sha256": hashlib.sha256(
                    (ROOT / "src/kernelledger/worker.py").read_bytes()
                ).hexdigest(),
                "measurements": measure(project, args.samples),
                "scope": "Local development build, 1000 names, one client. No baseline or release-performance claim.",
            }
        finally:
            if alive(project):
                rpc(project, {"command": "stop"})
            if process:
                process.wait(timeout=15)
            deadline = time.monotonic() + 5
            while alive(project) and time.monotonic() < deadline:
                time.sleep(0.02)
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
        print(args.output)


if __name__ == "__main__":
    main()
