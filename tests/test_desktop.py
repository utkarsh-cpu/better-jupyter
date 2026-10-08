import concurrent.futures
import json
import socket
import sqlite3
import struct
import time
import uuid

import pytest

from kernelledger.cli import alive, ensure_supervisor, rpc
from kernelledger.desktop.helper import Helper
from kernelledger.desktop.protocol import API, LIMIT, encode
from kernelledger.ledger import DDL, Ledger
from kernelledger.notebook import Notebook
from kernelledger.source import SourceError, SourceService, source_hash
from kernelledger.supervisor import interpreter


class Client:
    def __init__(self, root, role="control", name="Test", client_id=None):
        info = json.loads((root / ".kl/supervisor.json").read_text())
        self.sock = socket.create_connection(("127.0.0.1", info["desktop_port"]), timeout=5)
        self.file = self.sock.makefile("rb")
        self.client_id = client_id or str(uuid.uuid4())
        self.sock.sendall(
            encode(
                {
                    "kind": "hello",
                    "api": API,
                    "versions": [1],
                    "token": (root / ".kl/token").read_text(),
                    "client_id": self.client_id,
                    "client_name": name,
                    "role": role,
                }
            )
        )
        self.welcome = self.read()
        assert self.welcome["kind"] == "welcome", self.welcome

    def read(self):
        size = struct.unpack(">I", self.file.read(4))[0]
        return json.loads(self.file.read(size))

    def call(self, method, **fields):
        self.sock.sendall(encode({"method": method, **fields}))
        response = self.read()
        if response["kind"] == "error":
            raise SourceError(response["error"]["code"], response["error"]["message"])
        return response["result"]

    def submit(self, method, **fields):
        return self.call(method, client_request_id=str(uuid.uuid4()), **fields)

    def wait(self, oid):
        deadline = time.monotonic() + 10
        while time.monotonic() < deadline:
            result = self.call("operation.get", operation_id=oid)
            if result["state"] in ("completed", "failed", "unknown", "canceled"):
                return result
            time.sleep(0.02)
        pytest.fail("operation did not finish")

    def close(self):
        self.file.close()
        self.sock.close()


@pytest.fixture
def project(tmp_path):
    notebook = Notebook(tmp_path)
    notebook.init()
    ensure_supervisor(tmp_path)
    yield notebook
    if alive(tmp_path):
        rpc(tmp_path, {"command": "stop"})
    for _ in range(100):
        if not (tmp_path / ".kl/supervisor.json").exists():
            break
        time.sleep(0.02)


def mutation(service, **fields):
    current = service.read()
    return {
        "manifest_revision": current["manifest_revision"],
        "cell_revisions": {c["id"]: c["revision"] for c in current["cells"]},
        **fields,
    }


def test_source_concurrent_writers_and_external_change(tmp_path):
    notebook = Notebook(tmp_path)
    notebook.init()
    notebook.add("code", "old", "a")
    service = SourceService(tmp_path)
    request = mutation(service, action="edit", cell_id="a", source="new")

    def write():
        try:
            return service.mutate(request)["state"]
        except SourceError as error:
            return error.code

    with concurrent.futures.ThreadPoolExecutor() as pool:
        assert sorted(pool.map(lambda _: write(), range(2))) == ["ok", "source_conflict"]
    stale = mutation(service, action="edit", cell_id="a", source="mine")
    (tmp_path / "cells/a.py").write_text("external")
    with pytest.raises(SourceError, match="changed"):
        service.mutate(stale)
    assert (tmp_path / "cells/a.py").read_text() == "external"


@pytest.mark.parametrize(
    "stage,new",
    [
        ("prepared", False),
        ("committed", True),
        ("replaced:cells/b.py", True),
        ("replaced:notebook.json", True),
    ],
)
def test_source_journal_recovery(tmp_path, stage, new):
    notebook = Notebook(tmp_path)
    notebook.init()
    service = SourceService(tmp_path)
    request = mutation(service, action="add", type="code", new_id="b", source="value = 1")

    def crash(point):
        if point == stage:
            raise RuntimeError("crash")

    service.hook = crash
    with pytest.raises(RuntimeError):
        service.mutate(request)
    recovered = SourceService(tmp_path).recover()
    assert recovered["state"] == "ok"
    assert bool(recovered["cells"]) == new
    assert not list((tmp_path / ".kl/source-journal").glob("*"))


def test_migration_preserves_history_and_backup(tmp_path):
    path = tmp_path / "ledger.sqlite"
    db = sqlite3.connect(path)
    db.executescript(DDL)
    db.execute("PRAGMA user_version=1")
    db.commit()
    db.close()
    ledger = Ledger(path)
    sid = ledger.session(str(uuid.uuid4()), {"test": True}, state="closed")
    backup = tmp_path / "ledger.sqlite.schema1.bak"
    assert ledger.schema == 1
    ledger.migrate(backup)
    assert ledger.schema == 2
    assert ledger.one("SELECT * FROM sessions WHERE session_id=?", (sid,))["state"] == "closed"
    ledger.close()
    old = sqlite3.connect(backup)
    assert old.execute("PRAGMA user_version").fetchone()[0] == 1
    assert old.execute("PRAGMA quick_check").fetchone()[0] == "ok"
    old.close()


def test_private_interpreter_never_falls_back(tmp_path, monkeypatch):
    monkeypatch.delenv("VIRTUAL_ENV", raising=False)
    with pytest.raises(SourceError) as error:
        interpreter(tmp_path, None, {}, "private", "desktop")
    assert error.value.code == "interpreter_required"


def test_offline_helper_has_no_side_effects(tmp_path):
    notebook = Notebook(tmp_path)
    notebook.init()
    import shutil

    shutil.rmtree(tmp_path / ".kl")
    helper = Helper(tmp_path)
    assert helper.dispatch({"method": "source.read"})["state"] == "ok"
    assert helper.dispatch({"method": "sessions.list"})["items"] == []
    assert not (tmp_path / ".kl").exists()


def test_dropped_reply_idempotency_and_feed(project):
    project.add("code", "counter = globals().get('counter', 0) + 1\nprint(counter)", "a")
    first = Client(project.root)
    client_id, key = first.client_id, str(uuid.uuid4())
    request = {
        "method": "run",
        "client_request_id": key,
        "cell_id": "a",
        "expected_sha256": source_hash(project.source(project.cell("a"))),
    }
    first.sock.sendall(encode(request))
    first.close()
    client = Client(project.root, client_id=client_id)
    try:
        deadline = time.monotonic() + 5
        while True:
            try:
                operation = client.call("operation.get", client_request_id=key)
                break
            except SourceError:
                assert time.monotonic() < deadline
                time.sleep(0.02)
        assert client.wait(operation["operation_id"])["state"] == "completed"
        # A retry returns the operation even after the cell changes.
        (project.root / "cells/a.py").write_text("raise Exception('changed')")
        again = client.call("run", **{k: v for k, v in request.items() if k != "method"})
        assert again["operation_id"] == operation["operation_id"]
        with pytest.raises(SourceError) as error:
            client.call(
                "run",
                **{
                    **{k: v for k, v in request.items() if k != "method"},
                    "expected_sha256": "0" * 64,
                },
            )
        assert error.value.code == "key_reused"
        changes = client.call("changes.list", after_change_seq=0)
        assert any(c["kind"] == "execution_status" for c in changes["items"])
        helper = Helper(project.root)
        executions = helper.dispatch({"method": "executions.list"})["items"]
        assert len(executions) == 1
        view = helper.dispatch({"method": "fold.get", "exec_id": executions[0]["exec_id"]})
        assert any(e.get("text") == "1\n" for e in view["outputs"])
    finally:
        client.close()


def test_batch_stop_on_error_and_frozen_source(project):
    project.add("code", "import time\ntime.sleep(.15)\nx=1", "a")
    project.add("code", "raise ValueError('stop')", "b")
    project.add("code", "x=3", "c")
    client = Client(project.root)
    try:
        accepted = client.submit("batch", cell_ids="all")
        (project.root / "cells/b.py").write_text("x=99")
        operation = client.wait(accepted["operation_id"])
        assert operation["state"] == "completed", operation
        assert [e["status"] for e in operation["result"]["executions"]] == ["ok", "error"]
        executions = Helper(project.root).dispatch({"method": "executions.list"})["items"]
        assert [e["status"] for e in executions] == ["ok", "error", "canceled"]
    finally:
        client.close()


def test_cancel_queued_run_cancels_its_execution(project):
    client = Client(project.root)
    try:
        blocking = "input('Wait: ')"
        first = client.submit("run", source=blocking, expected_sha256=source_hash(blocking))
        deadline = time.monotonic() + 5
        while not client.call("runtime.status")["pending_inputs"]:
            assert time.monotonic() < deadline
            time.sleep(0.02)
        queued = "raise AssertionError('canceled code executed')"
        second = client.submit("run", source=queued, expected_sha256=source_hash(queued))
        client.call("operation.cancel", operation_id=second["operation_id"])
        operation = client.wait(second["operation_id"])
        assert operation["state"] == "canceled"
        executions = Helper(project.root).dispatch({"method": "executions.list"})["items"]
        assert executions[-1]["status"] == "canceled"
        client.call("operation.cancel", operation_id=first["operation_id"])
        client.wait(first["operation_id"])
    finally:
        client.close()


def test_input_claim_handoff_and_redaction(project):
    client = Client(project.root, name="First")
    second = Client(project.root, name="Second")
    source = "import getpass\nsecret = getpass.getpass('Secret: ')\nprint('done')"
    try:
        accepted = client.submit("run", source=source, expected_sha256=source_hash(source))
        deadline = time.monotonic() + 5
        while True:
            inputs = client.call("runtime.status")["pending_inputs"]
            if inputs:
                break
            assert time.monotonic() < deadline
            time.sleep(0.02)
        iid = inputs[0]["input_id"]
        client.call("input.claim", input_id=iid)
        with pytest.raises(SourceError) as error:
            second.call("input.claim", input_id=iid)
        assert error.value.code == "input_claimed"
        client.close()
        deadline = time.monotonic() + 5
        while True:
            try:
                second.call("input.claim", input_id=iid)
                break
            except SourceError:
                assert time.monotonic() < deadline
                time.sleep(0.02)
        second.call("input.reply", input_id=iid, text="never-record-me")
        operation = second.wait(accepted["operation_id"])
        assert operation["state"] == "completed", operation
        eid = operation["result"]["exec_id"]
        events = Helper(project.root).dispatch({"method": "events.list", "exec_id": eid})["items"]
        reply = next(e for e in events if e["kind"] == "stdin_reply")
        assert reply["redacted"] and "text" not in reply
        assert "never-record-me" not in json.dumps(events)
    finally:
        client.close()
        second.close()


def test_oversized_frame_rejected_before_body(project):
    client = Client(project.root)
    try:
        client.sock.sendall(struct.pack(">I", LIMIT + 1))
        assert client.read()["error"]["code"] == "too_large"
    finally:
        client.close()


def test_subscription_follow_and_verified_blob(project):
    project.add("code", "print('committed')", "a")
    control = Client(project.root)
    feed = Client(project.root, role="subscription")
    blobs = Client(project.root, role="blob")
    try:
        feed.sock.sendall(encode({"method": "subscribe"}))
        snapshot = feed.read()
        assert snapshot["kind"] == "snapshot"
        accepted = control.submit(
            "run", cell_id="a", expected_sha256=source_hash("print('committed')")
        )
        operation = control.wait(accepted["operation_id"])
        seqs = []
        while True:
            change = feed.read()
            assert change["kind"] == "change", change
            seqs.append(change["change_seq"])
            if (
                change["ref"].get("operation_id") == accepted["operation_id"]
                and change["ref"].get("state") == "completed"
            ):
                break
        assert seqs == sorted(set(seqs))
        eid = operation["result"]["exec_id"]
        follow = Client(project.root, role="subscription")
        try:
            follow.sock.sendall(
                encode({"method": "execution.follow", "exec_id": eid, "after_seq": 0})
            )
            events = []
            while True:
                event = follow.read()
                assert event["kind"] == "event"
                events.append(event["event"])
                if event["event"].get("state") == "idle":
                    break
            expected = Helper(project.root).dispatch({"method": "events.list", "exec_id": eid})[
                "items"
            ]
            assert events == expected
        finally:
            follow.close()
        digest = source_hash("print('committed')")
        blobs.sock.sendall(encode({"method": "blob.read", "sha256": digest}))
        header = blobs.read()
        payload = blobs.file.read(header["byte_size"])
        assert payload == b"print('committed')"
        assert header["verified"] and header["final"]
        with pytest.raises(SourceError) as error:
            blobs.call("blob.read", sha256=digest, offset=len(payload) + 1)
        assert error.value.code == "invalid_request"
    finally:
        control.close()
        feed.close()
        blobs.close()


def test_stop_preempts_busy_work_and_cancels_queued_runs(project):
    source = "import time\ntime.sleep(30)"
    project.add("code", source, "slow")
    control = Client(project.root)
    other = Client(project.root)
    try:
        first = control.submit("run", cell_id="slow", expected_sha256=source_hash(source))
        second = control.submit("run", cell_id="slow", expected_sha256=source_hash(source))
        deadline = time.monotonic() + 5
        while control.call("runtime.status")["worker_state"] != "running":
            assert time.monotonic() < deadline
            time.sleep(.02)
        stopped = other.submit("stop")
        deadline = time.monotonic() + 8
        while (project.root / ".kl/supervisor.json").exists():
            assert time.monotonic() < deadline, "Stop queued behind the 30-second cell"
            time.sleep(.02)
        with sqlite3.connect(project.root / ".kl/ledger.sqlite") as ledger:
            states = dict(ledger.execute("SELECT operation_id,state FROM operations"))
            assert states[stopped["operation_id"]] == "completed"
            assert states[first["operation_id"]] in ("failed", "completed", "canceled")
            assert states[second["operation_id"]] == "canceled"
            assert not ledger.execute("SELECT 1 FROM executions WHERE status IN ('running','queued')").fetchall()
    finally:
        control.close()
        other.close()
