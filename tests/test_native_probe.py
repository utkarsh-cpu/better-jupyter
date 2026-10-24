"""Cross-language feasibility tests; never select the probe as a real backend."""

import concurrent.futures
import ctypes
import json
import os
import subprocess
import sys
import time
from pathlib import Path

import pytest

from kernelledger.source import SourceService
from kernelledger.supervisor.platform import DirectoryLock

ROOT = Path(__file__).resolve().parents[1]


@pytest.fixture
def daemon():
    executable = os.environ.get("KL_NATIVE_DAEMON")
    if not executable:
        pytest.skip("native probe requires an explicit KL_NATIVE_DAEMON build")
    path = Path(executable).resolve()
    assert path.is_file(), path
    return path


def run(daemon, *args, **kwargs):
    return subprocess.run(
        [str(daemon), *map(str, args)], capture_output=True, text=True, timeout=90, **kwargs
    )


@pytest.mark.parametrize("source", [False, True])
def test_python_owner_blocks_native_lock(daemon, tmp_path, source):
    state = tmp_path / ".kl"
    path = state / ("source.lock" if source else "lock")
    if source:
        with SourceService(tmp_path).locked():
            assert run(daemon, "lock-probe", "--path", path).returncode != 0
    else:
        lock = DirectoryLock(state).acquire()
        try:
            assert run(daemon, "lock-probe", "--path", path).returncode != 0
        finally:
            lock.close()
    assert run(daemon, "lock-probe", "--path", path, input="\n").returncode == 0


def test_native_owner_blocks_python_and_releases_on_exit(daemon, tmp_path):
    process = subprocess.Popen(
        [str(daemon), "lock-probe", "--path", str(tmp_path / "lock")],
        stdin=subprocess.PIPE,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
    )
    try:
        assert json.loads(process.stdout.readline()) == {"locked": True}
        with pytest.raises(ValueError, match="locked"):
            DirectoryLock(tmp_path).acquire()
    finally:
        process.communicate("\n", timeout=10)
    lock = DirectoryLock(tmp_path).acquire()
    lock.close()


def test_native_source_lock_blocks_python_until_owner_crash(daemon, tmp_path):
    path = tmp_path / ".kl/source.lock"
    process = subprocess.Popen(
        [str(daemon), "lock-probe", "--path", str(path)],
        stdin=subprocess.PIPE,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
    )

    def edit_lock():
        with SourceService(tmp_path).locked():
            return "acquired"

    with concurrent.futures.ThreadPoolExecutor(max_workers=1) as pool:
        try:
            assert json.loads(process.stdout.readline()) == {"locked": True}
            waiting = pool.submit(edit_lock)
            time.sleep(0.1)
            assert not waiting.done()
        finally:
            process.kill()
            process.communicate(timeout=10)
        assert waiting.result(timeout=5) == "acquired"


def process_alive(pid):
    if os.name != "nt":
        path = Path(f"/proc/{pid}/stat")
        return path.exists() and path.read_text().split(") ", 1)[1][0] != "Z"
    from ctypes import wintypes as w

    kernel = ctypes.WinDLL("kernel32", use_last_error=True)
    kernel.OpenProcess.argtypes = [w.DWORD, w.BOOL, w.DWORD]
    kernel.OpenProcess.restype = w.HANDLE
    kernel.GetExitCodeProcess.argtypes = [w.HANDLE, ctypes.POINTER(w.DWORD)]
    kernel.CloseHandle.argtypes = [w.HANDLE]
    handle = kernel.OpenProcess(0x1000, False, pid)
    if not handle:
        return False
    try:
        code = w.DWORD()
        assert kernel.GetExitCodeProcess(handle, ctypes.byref(code))
        return code.value == 259
    finally:
        kernel.CloseHandle(handle)


def test_native_worker_blob_and_child_cleanup(daemon, tmp_path):
    child = tmp_path / "child.pid"
    result = run(
        daemon,
        "probe",
        "--project",
        tmp_path,
        "--python",
        sys.executable,
        "--worker",
        ROOT / "src/kernelledger/worker.py",
        "--mib",
        "8",
        "--child-pid-file",
        child,
    )
    assert result.returncode == 0, result.stderr
    report = json.loads(result.stdout)
    assert report["production_backend"] is False
    assert report["received_bytes"] >= 8 * 1024**2
    assert report["process_tree_terminated"] is True
    assert not (tmp_path / ".kl/ledger.sqlite").exists()
    assert not process_alive(report["worker_pid"])
    pid = int(child.read_text())
    deadline = time.monotonic() + 5
    while process_alive(pid) and time.monotonic() < deadline:
        time.sleep(0.02)
    assert not process_alive(pid)


def test_probe_refuses_production_modes_and_bad_interpreter(daemon, tmp_path):
    assert run(daemon, "supervisor").returncode != 0
    assert run(daemon, "helper").returncode != 0
    result = run(
        daemon,
        "probe",
        "--project",
        tmp_path,
        "--python",
        tmp_path / "missing",
        "--worker",
        ROOT / "src/kernelledger/worker.py",
        "--mib",
        1,
    )
    assert result.returncode != 0
    lock = DirectoryLock(tmp_path / ".kl").acquire()
    lock.close()


def test_native_parent_crash_owns_worker_descendants(daemon, tmp_path):
    child = tmp_path / "child.pid"
    process = subprocess.Popen(
        [
            str(daemon),
            "probe",
            "--project",
            str(tmp_path),
            "--python",
            sys.executable,
            "--worker",
            str(ROOT / "src/kernelledger/worker.py"),
            "--mib",
            "256",
            "--child-pid-file",
            str(child),
        ],
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
    )
    try:
        deadline = time.monotonic() + 30
        while (not child.exists() or not child.read_text()) and time.monotonic() < deadline:
            assert process.poll() is None
            time.sleep(0.005)
        pid = int(child.read_text())
        assert process.poll() is None, "probe finished before crash injection"
    finally:
        if process.poll() is None:
            process.kill()
        process.communicate(timeout=10)
    deadline = time.monotonic() + 5
    while process_alive(pid) and time.monotonic() < deadline:
        time.sleep(0.02)
    assert not process_alive(pid)
