"""Native legacy owner integration; uses durable artifacts and the public protocol."""

import concurrent.futures
import json
import os
import socket
import subprocess
import sys
import time
import uuid
from pathlib import Path

import pytest

from kernelledger.blobs import BlobStore
from kernelledger.cli import ensure_supervisor, rpc
from kernelledger.ledger import Ledger
from kernelledger.notebook import Notebook
from kernelledger.registry import entry_path
from kernelledger.supervisor.platform import DirectoryLock
from kernelledger.worker import Channel, receive

ROOT = Path(__file__).resolve().parents[1]


@pytest.fixture
def native_runtime(tmp_path, request):
    binary = os.environ.get("KL_NATIVE_DAEMON") or os.environ.get("KERNELLEDGER_DAEMON")
    if not binary:
        pytest.skip("native supervisor requires an explicit daemon build")
    assert Path(binary).is_file(), binary
    notebook = Notebook(tmp_path)
    notebook.init()
    seed = getattr(request, "param", {})
    if seed.get("config"):
        config_path = tmp_path / ".kl/config.json"
        config = json.loads(config_path.read_text()) if config_path.exists() else {}
        config.update(seed["config"])
        config_path.write_text(json.dumps(config))
    if seed.get("checkpoint"):
        manifest = {
            "schema": 1,
            "bindings": {},
            "objects": {},
            "codecs": {},
            "excluded": [],
            "notes": [],
            "python": "0.0.0",
            "platform": "test",
        }
        blobs = BlobStore(tmp_path / ".kl")
        digest = blobs.put(json.dumps(manifest).encode())
        ledger = Ledger(tmp_path / ".kl/ledger.sqlite")
        session = ledger.session(notebook.read()["id"], {}, state="closed")
        try:
            ledger.write(
                "INSERT INTO checkpoints VALUES (?,?,?,?,?,?,?)",
                (
                    "seeded-checkpoint",
                    session,
                    None,
                    "complete",
                    digest,
                    "2026-10-24T00:00:00Z",
                    seed.get("origin", "local"),
                ),
            )
        finally:
            ledger.close()
    flags = {"creationflags": 0x08000000} if os.name == "nt" else {}
    with (tmp_path / "native.log").open("wb") as log:
        process = subprocess.Popen(
            [binary, "serve-experimental", "--project", str(tmp_path), "--python", sys.executable],
            stdin=subprocess.DEVNULL,
            stdout=log,
            stderr=log,
            cwd=tmp_path,
            env={**os.environ, **seed.get("env", {})},
            **flags,
        )
        deadline = time.monotonic() + 20
        while not (tmp_path / ".kl/supervisor.json").exists():
            if process.poll() is not None:
                pytest.fail((tmp_path / "native.log").read_text(errors="replace"))
            assert time.monotonic() < deadline, "native supervisor startup timed out"
            time.sleep(0.02)
        try:
            yield notebook, process
        finally:
            if process.poll() is None:
                try:
                    rpc(tmp_path, {"command": "stop"})
                    process.wait(timeout=15)
                finally:
                    if process.poll() is None:
                        process.kill()
                        process.wait(timeout=5)


def test_native_run_history_and_python_reopen(native_runtime):
    notebook, process = native_runtime
    info = json.loads((notebook.root / ".kl/supervisor.json").read_text())
    registry = entry_path(notebook.root)
    assert json.loads(registry.read_text()) == info
    assert rpc(notebook.root, {"command": "status"})["instance_id"] == info["instance_id"]
    first = rpc(notebook.root, {"command": "run", "source": "x = 41\nprint('native')"})
    second = rpc(notebook.root, {"command": "run", "source": "assert x == 41\nx += 1"})
    assert first["status"] == second["status"] == "ok"
    assert first["session_id"] == second["session_id"]
    assert first["new_empty_session"] and not second["new_empty_session"]
    ledger = Ledger(notebook.root / ".kl/ledger.sqlite", readonly=True)
    try:
        assert ledger.schema == 2
        assert [
            r["status"] for r in ledger.rows("SELECT status FROM executions ORDER BY ordinal")
        ] == ["ok", "ok"]
        events = ledger.events(first["exec_id"])
        assert "native" in "".join(e.get("text", "") for e in events)
    finally:
        ledger.close()
    with pytest.raises(Exception):
        DirectoryLock(notebook.root / ".kl").acquire()
    assert rpc(notebook.root, {"command": "stop"}) == {"stopped": True}
    assert process.wait(timeout=10) == 0
    assert not (notebook.root / ".kl/supervisor.json").exists()
    assert not registry.exists()
    lock = DirectoryLock(notebook.root / ".kl").acquire()
    try:
        ledger = Ledger(notebook.root / ".kl/ledger.sqlite")
        ledger.takeover()
        ledger.close()
    finally:
        lock.close()


def test_native_captures_pipe_output_and_incremental_unicode(native_runtime):
    notebook, _ = native_runtime
    result = rpc(
        notebook.root,
        {
            "command": "run",
            "source": (
                "import os,time\n"
                "for byte in 'pipe🙂é'.encode():\n"
                "    os.write(1, bytes([byte])); time.sleep(0.005)\n"
                "os.write(2, b'pipe-error')\n"
                "print('protocol-output')\n"
            ),
        },
    )
    assert result["status"] == "ok"
    ledger = Ledger(notebook.root / ".kl/ledger.sqlite", readonly=True)
    try:
        events = ledger.events(result["exec_id"])
        stdout = "".join(e.get("text", "") for e in events if e["kind"] == "stdout")
        stderr = "".join(e.get("text", "") for e in events if e["kind"] == "stderr")
        assert "pipe🙂é" in stdout
        assert "protocol-output" in stdout
        assert "pipe-error" in stderr
        assert "\ufffd" not in stdout
        assert any(e.get("capture") == "pipe" for e in events)
    finally:
        ledger.close()


def test_native_pipe_and_protocol_share_the_output_cap(native_runtime):
    notebook, _ = native_runtime
    (notebook.root / ".kl/config.json").write_text(json.dumps({"output_limit_bytes": 64}))
    result = rpc(
        notebook.root,
        {
            "command": "run",
            "source": ("import os\nos.write(1,b'a'*80)\nos.write(2,b'b'*80)\nprint('c'*80,end='')"),
        },
    )
    assert result["status"] == "ok"
    ledger = Ledger(notebook.root / ".kl/ledger.sqlite", readonly=True)
    try:
        events = ledger.events(result["exec_id"])
        recorded = sum(
            len(e.get("text", "").encode()) for e in events if e["kind"] in ("stdout", "stderr")
        )
        dropped = [e["dropped_bytes"] for e in events if e["kind"] == "truncated"]
        assert recorded == 64
        assert dropped == [176]
    finally:
        ledger.close()


def test_native_late_pipe_output_is_explicitly_unattributed(native_runtime):
    notebook, _ = native_runtime
    result = rpc(
        notebook.root,
        {
            "command": "run",
            "source": (
                "import os,threading,time\n"
                "def late():\n"
                "    time.sleep(0.3)\n"
                "    os.write(1,b'late-native-output')\n"
                "    print('late-protocol-output')\n"
                "threading.Thread(target=late).start()\n"
            ),
        },
    )
    assert result["status"] == "ok"
    deadline = time.monotonic() + 5
    while True:
        ledger = Ledger(notebook.root / ".kl/ledger.sqlite", readonly=True)
        try:
            rows = ledger.rows(
                "SELECT * FROM session_events WHERE session_id=? AND kind='stdout'",
                (result["session_id"],),
            )
            payloads = [json.loads(row["payload_json"]) for row in rows]
            texts = "".join(p.get("text", "") for p in payloads)
            if "late-native-output" in texts and "late-protocol-output" in texts:
                assert all(p["cell_attribution"] == "unattributed" for p in payloads)
                assert any(p.get("capture") == "pipe" for p in payloads)
                assert any(
                    "late-protocol-output" in p.get("text", "") and "capture" not in p
                    for p in payloads
                )
                assert "late-native-output" not in "".join(
                    e.get("text", "") for e in ledger.events(result["exec_id"])
                )
                break
        finally:
            ledger.close()
        assert time.monotonic() < deadline
        time.sleep(0.02)


def test_native_status_interrupt_and_restart(native_runtime):
    notebook, _ = native_runtime
    with concurrent.futures.ThreadPoolExecutor() as pool:
        running = pool.submit(
            rpc,
            notebook.root,
            {"command": "run", "source": "import time\nwhile True: time.sleep(0.01)"},
        )
        deadline = time.monotonic() + 20
        while not rpc(notebook.root, {"command": "status"})["in_flight"]:
            assert time.monotonic() < deadline
            time.sleep(0.01)
        started = time.monotonic()
        assert rpc(notebook.root, {"command": "status"})["in_flight"]
        assert time.monotonic() - started < 1
        assert rpc(notebook.root, {"command": "interrupt"})["interrupted"]
        assert running.result(timeout=15)["status"] == "interrupted"
    previous = rpc(notebook.root, {"command": "status"})["session_id"]
    assert rpc(notebook.root, {"command": "restart"})["session_id"] != previous
    assert (
        rpc(notebook.root, {"command": "run", "source": "assert 'x' not in globals()"})["status"]
        == "ok"
    )


def test_native_rejects_bad_auth_before_accepting_work(native_runtime):
    notebook, _ = native_runtime
    info = json.loads((notebook.root / ".kl/supervisor.json").read_text())
    with socket.create_connection(("127.0.0.1", info["port"]), timeout=5) as sock:
        Channel(sock).send("auth", token="wrong")
        assert "authentication failed" in receive(sock)["error"]
    ledger = Ledger(notebook.root / ".kl/ledger.sqlite", readonly=True)
    try:
        assert ledger.rows("SELECT * FROM sessions") == []
        assert ledger.rows("SELECT * FROM executions") == []
    finally:
        ledger.close()


def test_native_checkpoint_restore_preserves_aliases_and_python_reopens(native_runtime):
    notebook, _ = native_runtime
    assert (
        rpc(notebook.root, {"command": "run", "source": "a = []; b = a; a.append(a)\nx = 41"})[
            "status"
        ]
        == "ok"
    )
    checkpoint = rpc(notebook.root, {"command": "checkpoint"})
    assert checkpoint["status"] in ("complete", "partial")
    restored = rpc(
        notebook.root, {"command": "restore", "checkpoint_id": checkpoint["checkpoint_id"]}
    )
    assert restored["session_id"]
    assert (
        rpc(
            notebook.root,
            {"command": "run", "source": "assert a is b and a[0] is a\nassert x == 41"},
        )["status"]
        == "ok"
    )


def test_native_long_source_and_idempotent_legacy_retry(native_runtime):
    notebook, _ = native_runtime
    request = {
        "command": "run",
        "source": "# padding\n" * 10000 + "counter = globals().get('counter', 0) + 1",
        "client_id": str(uuid.uuid4()),
        "client_request_id": str(uuid.uuid4()),
    }
    first = rpc(notebook.root, request)
    assert first["status"] == "ok"
    assert rpc(notebook.root, request) == first
    operation = rpc(
        notebook.root,
        {
            "command": "operation",
            "client_id": request["client_id"],
            "client_request_id": request["client_request_id"],
        },
    )
    assert operation["state"] == "completed"
    assert operation["result"] == first
    assert rpc(notebook.root, {"command": "run", "source": "assert counter == 1"})["status"] == "ok"
    with pytest.raises(Exception, match="key_reused"):
        rpc(notebook.root, {**request, "source": "counter += 1"})


def test_native_keyed_cell_retry_uses_original_result_after_source_is_removed(native_runtime):
    notebook, _ = native_runtime
    cell = notebook.add("code", "counter = globals().get('counter', 0) + 1", "keyed")
    request = {
        "command": "run",
        "cell_id": cell,
        "client_id": str(uuid.uuid4()),
        "client_request_id": str(uuid.uuid4()),
    }
    first = rpc(notebook.root, request)
    assert first["status"] == "ok"
    notebook.cell_path(notebook.cell(cell)).unlink()
    assert rpc(notebook.root, request) == first
    with pytest.raises(Exception, match="key_reused"):
        rpc(notebook.root, {**request, "cell_id": "different"})
    assert rpc(notebook.root, {"command": "run", "source": "assert counter == 1"})["status"] == "ok"


@pytest.mark.parametrize(
    "native_runtime",
    [{"checkpoint": True}, {"checkpoint": True, "origin": "imported"}],
    indirect=True,
)
def test_native_failed_restore_preserves_the_healthy_namespace(native_runtime):
    notebook, _ = native_runtime
    original = rpc(notebook.root, {"command": "run", "source": "keep = 42"})
    ledger = Ledger(notebook.root / ".kl/ledger.sqlite", readonly=True)
    try:
        origin = ledger.rows(
            "SELECT origin FROM checkpoints WHERE checkpoint_id='seeded-checkpoint'"
        )[0]["origin"]
    finally:
        ledger.close()
    with pytest.raises(
        Exception, match="--trust" if origin == "imported" else "Python major.minor mismatch"
    ):
        rpc(notebook.root, {"command": "restore", "checkpoint_id": "seeded-checkpoint"})
    status = rpc(notebook.root, {"command": "status"})
    assert status["session_id"] == original["session_id"]
    assert status["worker_start_stage"] == "ready"
    assert rpc(notebook.root, {"command": "run", "source": "assert keep == 42"})["status"] == "ok"
    ledger = Ledger(notebook.root / ".kl/ledger.sqlite", readonly=True)
    try:
        running = ledger.rows("SELECT session_id FROM sessions WHERE state='running'")
        assert running == [{"session_id": original["session_id"]}]
    finally:
        ledger.close()


def test_native_hard_kill_records_aborted_and_allows_fresh_session(native_runtime):
    notebook, _ = native_runtime
    with concurrent.futures.ThreadPoolExecutor() as pool:
        running = pool.submit(
            rpc,
            notebook.root,
            {"command": "run", "source": "import time\nwhile True: time.sleep(0.01)"},
        )
        deadline = time.monotonic() + 20
        while not rpc(notebook.root, {"command": "status"})["in_flight"]:
            assert time.monotonic() < deadline
            time.sleep(0.01)
        assert rpc(notebook.root, {"command": "interrupt", "kill": True})["interrupted"]
        with pytest.raises(Exception, match="stopped|exited|disconnected"):
            running.result(timeout=15)
    result = rpc(notebook.root, {"command": "run", "source": "assert 'time' not in globals()"})
    assert result["status"] == "ok" and result["new_empty_session"]
    ledger = Ledger(notebook.root / ".kl/ledger.sqlite", readonly=True)
    try:
        assert len(ledger.rows("SELECT * FROM executions WHERE status='aborted'")) == 1
    finally:
        ledger.close()


def test_python_takes_over_native_crash_without_repeating_keyed_execution(native_runtime):
    notebook, process = native_runtime
    request = {
        "command": "run",
        "source": "import time\nwhile True: time.sleep(0.01)",
        "client_id": str(uuid.uuid4()),
        "client_request_id": str(uuid.uuid4()),
    }
    with concurrent.futures.ThreadPoolExecutor() as pool:
        running = pool.submit(rpc, notebook.root, request)
        deadline = time.monotonic() + 20
        while not rpc(notebook.root, {"command": "status"})["in_flight"]:
            assert time.monotonic() < deadline
            time.sleep(0.01)
        process.kill()
        process.wait(timeout=5)
        with pytest.raises(Exception):
            running.result(timeout=10)
    ensure_supervisor(notebook.root)
    try:
        operation = rpc(
            notebook.root,
            {
                "command": "operation",
                "client_id": request["client_id"],
                "client_request_id": request["client_request_id"],
            },
        )
        assert operation["state"] == "unknown"
        assert len(operation["result"]["exec_ids"]) == 1
        assert len(operation["result"]["executions"]) == 1
        ledger = Ledger(notebook.root / ".kl/ledger.sqlite", readonly=True)
        try:
            assert len(ledger.rows("SELECT * FROM executions")) == 1
            assert ledger.rows("SELECT status FROM executions")[0]["status"] == "incomplete"
        finally:
            ledger.close()
    finally:
        rpc(notebook.root, {"command": "stop"})
