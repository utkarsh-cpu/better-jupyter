import os
import time

import pytest

from kernelledger.blobs import BlobStore
from kernelledger.cli import alive, ensure_supervisor, rpc
from kernelledger.ledger import Ledger, fold
from kernelledger.notebook import Notebook
from kernelledger.sharing import Bundle
from kernelledger.supervisor.platform import DirectoryLock


@pytest.fixture
def runtime(tmp_path):
    notebook = Notebook(tmp_path)
    notebook.init()
    ensure_supervisor(tmp_path)
    try:
        yield notebook
    finally:
        if alive(tmp_path):
            rpc(tmp_path, {"command": "stop"})
        for _ in range(100):
            if not (tmp_path / ".kl/supervisor.json").exists():
                break
            time.sleep(0.02)


def run(notebook, source):
    result = rpc(notebook.root, {"command": "run", "source": source})
    assert result["status"] == "ok", result
    return result


def test_history_restore_and_bundle(runtime, tmp_path):
    n = runtime
    cid = n.add("code", "a = []; b = a; a.append(a)\ng = 3\ndef f():\n    return g\nf()", "main")
    tracked = n.path.read_bytes(), n.cell_path(n.cell(cid)).read_bytes()
    first = rpc(n.root, {"command": "run", "cell_id": cid})
    second = rpc(n.root, {"command": "run", "cell_id": cid})
    assert first["exec_id"] != second["exec_id"]
    assert tracked == (n.path.read_bytes(), n.cell_path(n.cell(cid)).read_bytes())
    checkpoint = rpc(n.root, {"command": "checkpoint"})
    n.cell_path(n.cell(cid)).write_text("raise ValueError('edited')")
    restored = rpc(n.root, {"command": "restore", "checkpoint_id": checkpoint["checkpoint_id"]})
    assert restored["session_id"] != first["session_id"]
    run(n, "assert a is b and a[0] is a\ng = 8\nassert f() == 8")
    bundle_path = tmp_path / "bundle.zip"
    rpc(n.root, {"command": "bundle_export", "path": str(bundle_path)})
    bundle = Bundle(bundle_path)
    try:
        assert not bundle.inspect()["hash_mismatches"]
    finally:
        bundle.close()
    destination = Notebook(tmp_path / "other")
    destination.init()
    ensure_supervisor(destination.root)
    try:
        rpc(destination.root, {"command": "bundle_import", "path": str(bundle_path)})
        with pytest.raises(ValueError, match="trust"):
            rpc(
                destination.root,
                {"command": "restore", "checkpoint_id": checkpoint["checkpoint_id"]},
            )
        rpc(
            destination.root,
            {"command": "restore", "checkpoint_id": checkpoint["checkpoint_id"], "trust": True},
        )
        run(destination, "assert f() == 3 and a is b")
    finally:
        rpc(destination.root, {"command": "stop"})


def test_display_clear_input_and_pipe(runtime):
    result = run(
        runtime,
        "display('one', display_id='d')\nupdate_display('two', 'd')\nclear_output(wait=True)\nprint('after')\nimport os\nos.write(1, b'pipe output')",
    )
    ledger = Ledger(runtime.root / ".kl/ledger.sqlite", readonly=True)
    try:
        events = ledger.events(result["exec_id"])
        assert any(e["kind"] == "display_update" for e in events)
        assert not any(e["kind"] == "display" for e in fold(events))
        assert any(e.get("capture") == "pipe" for e in events)
    finally:
        ledger.close()
    response = rpc(runtime.root, {"command": "run", "source": "input('required')"})
    assert response["status"] == "error"


def test_thread_refusal(runtime):
    run(
        runtime,
        "import threading\nstop_event = threading.Event()\nt = threading.Thread(target=stop_event.wait, name='user_thread')\nt.start()",
    )
    refusal = rpc(runtime.root, {"command": "checkpoint"})
    assert refusal["refused"] == "background_threads"
    run(runtime, "stop_event.set(); t.join()")


def test_lock(runtime):
    lock = DirectoryLock(runtime.root / ".kl")
    with pytest.raises(ValueError, match="locked"):
        lock.acquire()


def test_replay_deleted_cell_and_gc(runtime):
    cid = runtime.add("code", "x=5\nprint(x)", "old")
    result = rpc(runtime.root, {"command": "run", "cell_id": cid})
    runtime.remove(cid)
    replay = rpc(runtime.root, {"command": "replay", "session_id": result["session_id"]})
    assert replay["executions"][0]["status"] == "ok"
    run(runtime, "assert x==5")
    blobs = BlobStore(runtime.root / ".kl")
    orphan = blobs.put(b"orphan")
    report = rpc(runtime.root, {"command": "gc"})
    assert orphan in report["removed"]


def test_crash_recovery(runtime):
    import concurrent.futures

    with concurrent.futures.ThreadPoolExecutor() as pool:
        future = pool.submit(
            rpc, runtime.root, {"command": "run", "source": "import time\ntime.sleep(60)"}
        )
        for _ in range(100):
            status = alive(runtime.root)
            if status and status["in_flight"]:
                break
            time.sleep(0.03)
        else:
            pytest.fail("execution did not start")
        pid = status["pid"]
        if os.name == "nt":
            import ctypes
            from ctypes import wintypes

            k = ctypes.WinDLL("kernel32")
            k.OpenProcess.restype = wintypes.HANDLE
            k.OpenProcess.argtypes = [wintypes.DWORD, wintypes.BOOL, wintypes.DWORD]
            k.TerminateProcess.argtypes = [wintypes.HANDLE, wintypes.UINT]
            k.CloseHandle.argtypes = [wintypes.HANDLE]
            handle = k.OpenProcess(1, False, pid)
            assert k.TerminateProcess(handle, 1)
            k.CloseHandle(handle)
        else:
            import signal

            os.kill(pid, signal.SIGKILL)
        with pytest.raises((OSError, EOFError, ValueError)):
            future.result(timeout=10)
    ensure_supervisor(runtime.root)
    ledger = Ledger(runtime.root / ".kl/ledger.sqlite", readonly=True)
    try:
        row = ledger.one("SELECT * FROM executions WHERE exec_id=?", (status["in_flight"],))
        assert row["status"] == "incomplete"
    finally:
        ledger.close()


def test_binary_display(runtime):
    result = run(
        runtime,
        "class Rich:\n    def _repr_png_(self):\n        return b'x'*100000\ndisplay(Rich())",
    )
    ledger = Ledger(runtime.root / ".kl/ledger.sqlite", readonly=True)
    try:
        events = ledger.events(result["exec_id"])
        item = next(e for e in events if e["kind"] == "display")
        assert (
            BlobStore(runtime.root / ".kl").get(item["bundle"]["image/png"]["blob"])
            == b"x" * 100000
        )
    finally:
        ledger.close()
