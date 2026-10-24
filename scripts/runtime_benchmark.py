"""Reproducible, opt-in runtime measurements (no UI and no user notebook).

Run with the interpreter being qualified. Every sample uses a disposable notebook.
RSS is sampled per process, not summed with the worker or a desktop process.
"""

import argparse
import concurrent.futures
import ctypes
import hashlib
import importlib.metadata
import json
import os
import platform
import sqlite3
import statistics
import subprocess
import sys
import tempfile
import threading
import time
import zipfile
from contextlib import closing, contextmanager
from pathlib import Path

from kernelledger.cli import ensure_supervisor, rpc
from kernelledger.notebook import Notebook

REPO = Path(__file__).resolve().parents[1]


def identity(snapshot=None):
    def git(*args):
        return subprocess.check_output(["git", *args], cwd=REPO).decode().strip()

    paths = git("ls-files", "--cached", "--others", "--exclude-standard", "-z").split("\0")
    files = {}
    for name in sorted(set(paths)):
        if not name or name.startswith(("artifacts/", "benchmarks/", ".agents/")):
            continue
        path = REPO / name
        if path.is_file():
            files[name] = hashlib.sha256(path.read_bytes()).hexdigest()
    if snapshot:
        Path(snapshot).parent.mkdir(parents=True, exist_ok=True)
        with zipfile.ZipFile(snapshot, "w", zipfile.ZIP_DEFLATED) as archive:
            for name in files:
                archive.write(REPO / name, name)
    versions = {}
    for name in ("numpy", "pandas", "pyarrow", "matplotlib", "torch"):
        try:
            versions[name] = importlib.metadata.version(name)
        except importlib.metadata.PackageNotFoundError:
            versions[name] = None
    return {
        "commit": git("rev-parse", "HEAD"),
        "dirty": bool(git("status", "--porcelain")),
        "source_sha256": hashlib.sha256(json.dumps(files, sort_keys=True).encode()).hexdigest(),
        "source_files": files,
        "snapshot": str(snapshot) if snapshot else None,
        "python": sys.version,
        "executable": sys.executable,
        "gil_enabled": getattr(sys, "_is_gil_enabled", lambda: True)(),
        "platform": platform.platform(),
        "machine": platform.machine(),
        "cpu": platform.processor(),
        "cpu_count": os.cpu_count(),
        "dependencies": versions,
        "sqlite": sqlite3.sqlite_version,
        "sqlite_compile_options": sqlite_options(),
        "storage_path": tempfile.gettempdir(),
        "storage_device": "not detected; record device/cache conditions with qualification results",
    }


def sqlite_options():
    with closing(sqlite3.connect(":memory:")) as db:
        return [r[0] for r in db.execute("pragma compile_options")]


def resident(pid):
    if os.name != "nt":
        return int(Path(f"/proc/{pid}/statm").read_text().split()[1]) * os.sysconf("SC_PAGE_SIZE")
    from ctypes import wintypes as w

    class Counters(ctypes.Structure):
        _fields_ = [("cb", w.DWORD), ("faults", w.DWORD)] + [
            (name, ctypes.c_size_t)
            for name in ("peak", "rss", "qpp", "qp", "qnpp", "qnp", "pagefile", "peakfile")
        ]

    kernel = ctypes.WinDLL("kernel32", use_last_error=True)
    kernel.OpenProcess.argtypes = [w.DWORD, w.BOOL, w.DWORD]
    kernel.OpenProcess.restype = w.HANDLE
    kernel.CloseHandle.argtypes = [w.HANDLE]
    psapi = ctypes.WinDLL("psapi", use_last_error=True)
    psapi.GetProcessMemoryInfo.argtypes = [w.HANDLE, ctypes.c_void_p, w.DWORD]
    handle = kernel.OpenProcess(0x0400 | 0x0010, False, pid)
    if not handle:
        raise OSError("process unavailable")
    try:
        counters = Counters()
        counters.cb = ctypes.sizeof(counters)
        if not psapi.GetProcessMemoryInfo(handle, ctypes.byref(counters), counters.cb):
            raise OSError("memory query failed")
        return counters.rss
    finally:
        kernel.CloseHandle(handle)


def summary(values):
    result = {
        "n": len(values),
        "median": statistics.median(values),
        "min": min(values),
        "max": max(values),
    }
    if len(values) >= 30:
        result["p95"] = sorted(values)[int(0.95 * (len(values) - 1))]
    return result


def wait_exit(pid):
    if os.name == "nt":
        from ctypes import wintypes as w

        kernel = ctypes.WinDLL("kernel32", use_last_error=True)
        kernel.OpenProcess.argtypes = [w.DWORD, w.BOOL, w.DWORD]
        kernel.OpenProcess.restype = w.HANDLE
        kernel.WaitForSingleObject.argtypes = [w.HANDLE, w.DWORD]
        kernel.CloseHandle.argtypes = [w.HANDLE]
        handle = kernel.OpenProcess(0x00100000, False, pid)
        if handle:
            try:
                if kernel.WaitForSingleObject(handle, 15000) != 0:
                    raise TimeoutError("supervisor did not exit")
            finally:
                kernel.CloseHandle(handle)
    else:
        deadline = time.monotonic() + 15
        while time.monotonic() < deadline:
            try:
                os.kill(pid, 0)
            except ProcessLookupError:
                return
            time.sleep(0.02)
        raise TimeoutError("supervisor did not exit")


@contextmanager
def notebook_directory():
    directory = tempfile.TemporaryDirectory(prefix="kl-benchmark-")
    try:
        yield directory.name
    finally:
        # A Windows venv redirector may still be releasing inherited log handles
        # briefly after the actual supervisor has exited.
        target = Path(directory.name).resolve()
        assert target.parent == Path(tempfile.gettempdir()).resolve()
        for attempt in range(100):
            try:
                directory.cleanup()
                break
            except PermissionError:
                if attempt == 99:
                    raise
                time.sleep(0.05)


def measure(args):
    result = {
        "schema": 1,
        "backend": "python",
        "environment": identity(args.snapshot),
        "parameters": vars(args),
        "cache": "OS cache uncontrolled; first and repeated checkpoints separated",
        "samples": [],
    }
    for sample in range(args.repetitions):
        with notebook_directory() as temp:
            root = Path(temp)
            Notebook(root).init()
            row = {"sample": sample, "rss_peak_bytes": {}, "memory_sample_errors": 0}
            done = threading.Event()
            pids = {}

            def sample_memory():
                while not done.wait(0.02):
                    for role, pid in list(pids.items()):
                        try:
                            value = resident(pid)
                            row["rss_peak_bytes"][role] = max(
                                value, row["rss_peak_bytes"].get(role, 0)
                            )
                        except (OSError, ValueError):
                            row["memory_sample_errors"] += 1

            sampler = threading.Thread(target=sample_memory, daemon=True)
            sampler.start()

            def timed(name, request):
                start = time.perf_counter()
                reply = rpc(root, request)
                row[name] = time.perf_counter() - start
                return reply

            try:
                start = time.perf_counter()
                ensure_supervisor(root, sys.executable)
                row["startup_seconds"] = time.perf_counter() - start
                status = rpc(root, {"command": "status"})
                pids["supervisor"] = json.loads((root / ".kl/supervisor.json").read_text())["pid"]
                # The worker PID is refreshed after restore, which creates a new worker.
                if status.get("worker_pid"):
                    pids["worker"] = status["worker_pid"]
                source = f"import numpy as np\narray=np.arange({args.mib}*1024**2//8,dtype=np.float64)\nalias=array\n__import__('pathlib').Path('worker.pid').write_text(str(__import__('os').getpid()))"
                assert (
                    timed("array_execute_seconds", {"command": "run", "source": source})["status"]
                    == "ok"
                )
                status = rpc(root, {"command": "status"})
                pids["worker"] = int((root / "worker.pid").read_text())
                if status.get("worker_pid"):
                    pids["worker"] = status["worker_pid"]
                row["idle_rss_bytes"] = {role: resident(pid) for role, pid in pids.items()}
                row["control_seconds"] = []
                with concurrent.futures.ThreadPoolExecutor(max_workers=1) as pool:
                    start = time.perf_counter()
                    pending = pool.submit(rpc, root, {"command": "checkpoint"})
                    while not pending.done():
                        tick = time.perf_counter()
                        rpc(root, {"command": "status"})
                        row["control_seconds"].append(time.perf_counter() - tick)
                        time.sleep(0.01)
                    first = pending.result()
                    row["checkpoint_seconds"] = time.perf_counter() - start
                assert first.get("checkpoint_id"), first
                second = timed("unchanged_seconds", {"command": "checkpoint"})
                assert second["new_blob_bytes"] == 0, second
                restored = timed(
                    "restore_seconds",
                    {"command": "restore", "checkpoint_id": first["checkpoint_id"]},
                )
                assert not restored.get("excluded"), restored
                status = rpc(root, {"command": "status"})
                rpc(
                    root,
                    {
                        "command": "run",
                        "source": "__import__('pathlib').Path('worker.pid').write_text(str(__import__('os').getpid()))",
                    },
                )
                pids["worker"] = int((root / "worker.pid").read_text())
                if status.get("worker_pid"):
                    pids["worker"] = status["worker_pid"]
                assert (
                    rpc(
                        root,
                        {
                            "command": "run",
                            "source": "assert array is alias\nassert array[-1] == len(array)-1",
                        },
                    )["status"]
                    == "ok"
                )
                row["short_cell_seconds"] = []
                for _ in range(args.control_repetitions):
                    start = time.perf_counter()
                    assert rpc(root, {"command": "run", "source": "pass"})["status"] == "ok"
                    row["short_cell_seconds"].append(time.perf_counter() - start)
                row["first_blob_bytes"] = first["new_blob_bytes"]
                row["unchanged_new_blob_bytes"] = second["new_blob_bytes"]
                row["checkpoint_stages"] = first.get(
                    "timings", {"walk_seconds": first["walk_seconds"]}
                )
                row["unchanged_stages"] = second.get(
                    "timings", {"walk_seconds": second["walk_seconds"]}
                )
                with closing(sqlite3.connect(root / ".kl/ledger.sqlite")) as db:
                    row["sqlite_journal_mode"] = db.execute("pragma journal_mode").fetchone()[0]
                wal = root / ".kl/ledger.sqlite-wal"
                row["wal_bytes"] = wal.stat().st_size if wal.exists() else 0
            finally:
                done.set()
                sampler.join()
                if (root / ".kl/supervisor.json").exists():
                    rpc(root, {"command": "stop"})
                    deadline = time.monotonic() + 15
                    while (root / ".kl/supervisor.json").exists() and time.monotonic() < deadline:
                        time.sleep(0.02)
                if "supervisor" in pids:
                    wait_exit(pids["supervisor"])
            result["samples"].append(row)
            print(
                f"sample {sample + 1}/{args.repetitions}: checkpoint={row['checkpoint_seconds']:.3f}s restore={row['restore_seconds']:.3f}s",
                flush=True,
            )
    result["summary"] = {
        key: summary([row[key] for row in result["samples"]])
        for key in ("startup_seconds", "checkpoint_seconds", "unchanged_seconds", "restore_seconds")
    }
    for key in ("control_seconds", "short_cell_seconds"):
        values = [value for row in result["samples"] for value in row[key]]
        if values:
            result["summary"][key] = summary(values)
    out = Path(args.out)
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text(json.dumps(result, indent=2) + "\n", encoding="utf-8")
    return result


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--mib", type=int, default=256)
    parser.add_argument("--repetitions", type=int, default=5)
    parser.add_argument("--control-repetitions", type=int, default=30)
    parser.add_argument("--out", default="benchmarks/runtime-v6/latest.json")
    parser.add_argument("--snapshot", default=None)
    options = parser.parse_args()
    if min(options.mib, options.repetitions, options.control_repetitions) < 1:
        parser.error("sizes and repetition counts must be positive")
    measure(options)
