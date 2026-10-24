"""Native desktop protocol, durable admission, frozen batches, and rollback."""

import concurrent.futures
import json
import socket
import struct
import time
import uuid
from contextlib import contextmanager

import pytest
from test_desktop import Client
from test_desktop import test_cancel_queued_run_cancels_its_execution as check_cancel_queued_input
from test_desktop import test_input_claim_handoff_and_redaction as check_input_handoff
from test_native_supervisor import native_runtime as native_runtime

from kernelledger.blobs import BlobStore
from kernelledger.cli import ensure_supervisor, rpc
from kernelledger.desktop.protocol import API, LIMIT, encode
from kernelledger.ledger import Ledger
from kernelledger.notebook import source_hash
from kernelledger.source import SourceError
from kernelledger.supervisor.platform import DirectoryLock


def wait_active(root):
    deadline = time.monotonic() + 15
    while not rpc(root, {"command": "status"})["in_flight"]:
        assert time.monotonic() < deadline
        time.sleep(0.01)


def wait_input(client):
    deadline = time.monotonic() + 15
    while True:
        status = client.call("runtime.status")
        if status["pending_inputs"]:
            assert status["worker_state"] == "waiting_input"
            return status["pending_inputs"][0]
        assert time.monotonic() < deadline, status
        time.sleep(0.01)


def input_rows(root):
    ledger = Ledger(root / ".kl/ledger.sqlite", readonly=True)
    try:
        return ledger.rows("SELECT * FROM input_requests ORDER BY created,input_id")
    finally:
        ledger.close()


def test_native_input_handoff_and_redaction(native_runtime):
    notebook, _ = native_runtime
    check_input_handoff(notebook)


def test_native_queued_cancel_while_waiting_input(native_runtime):
    notebook, _ = native_runtime
    check_cancel_queued_input(notebook)


@pytest.mark.parametrize(
    "native_runtime", [{}, {"config": {"record_secret_input": True}}], indirect=True
)
def test_native_input_large_secret_recording_policy(native_runtime):
    notebook, _ = native_runtime
    client = Client(notebook.root)
    secret = "private-input-" + "x" * 100_000
    try:
        source = (
            "import getpass\nanswer = getpass.getpass('Secret: ')\nassert len(answer) == 100014"
        )
        accepted = client.submit("run", source=source, expected_sha256=source_hash(source))
        pending = wait_input(client)
        assert pending["password"] and pending["client_name"] is None
        client.call("input.claim", input_id=pending["input_id"])
        client.call("input.reply", input_id=pending["input_id"], text=secret)
        result = client.wait(accepted["operation_id"])
        assert result["state"] == "completed" and result["result"]["status"] == "ok", result
        recorded = client.call("runtime.status")["record_secret_input"]
        rows = input_rows(notebook.root)
        assert rows[0]["state"] == "answered" and rows[0]["client_name"] == "Test"
        ledger = Ledger(notebook.root / ".kl/ledger.sqlite", readonly=True)
        try:
            reply = ledger.rows("SELECT payload_json FROM events WHERE kind='stdin_reply'")[0]
            payload = json.loads(reply["payload_json"])
            assert (payload.get("text") == secret) == recorded
            assert payload.get("redacted", False) == (not recorded)
        finally:
            ledger.close()
        if not recorded:
            # Include WAL, blobs, and native.log: large transport envelopes must
            # never publish a secret payload to the content-addressed store.
            for path in notebook.root.rglob("*"):
                if path.is_file() and path != notebook.root / ".kl/lock":
                    assert secret.encode() not in path.read_bytes(), path
    finally:
        client.close()


def test_native_input_claim_is_connection_owned_and_release_is_durable(native_runtime):
    notebook, _ = native_runtime
    first = Client(notebook.root, name="First")
    second = Client(notebook.root, name="Second", client_id=first.client_id)
    try:
        source = "first = input('One: ')\nsecond = input('Two: ')\nassert (first,second) == ('hello','world')"
        accepted = first.submit("run", source=source, expected_sha256=source_hash(source))
        pending = wait_input(first)
        iid = pending["input_id"]
        for method in ("input.reply", "input.release", "input.cancel"):
            with pytest.raises(SourceError) as error:
                second.call(
                    method, input_id=iid, **({"text": "wrong"} if method == "input.reply" else {})
                )
            assert error.value.code == "input_claimed"
        first.call("input.claim", input_id=iid)
        with pytest.raises(SourceError) as error:
            second.call("input.claim", input_id=iid)
        assert error.value.code == "input_claimed"
        first.call("input.release", input_id=iid)
        assert input_rows(notebook.root)[0]["client_name"] is None
        second.call("input.claim", input_id=iid)
        assert input_rows(notebook.root)[0]["client_name"] == "Second"
        with pytest.raises(SourceError) as error:
            second.call("input.reply", input_id=iid, text=123)
        assert error.value.code == "invalid_request"
        second.call("input.reply", input_id=iid, text="hello")
        with pytest.raises(SourceError) as error:
            second.call("input.reply", input_id=iid, text="duplicate")
        assert error.value.code == "not_found"
        following = wait_input(second)
        assert following["input_id"] != iid
        second.call("input.claim", input_id=following["input_id"])
        second.call("input.reply", input_id=following["input_id"], text="world")
        result = second.wait(accepted["operation_id"])
        assert result["result"]["status"] == "ok", result
        assert all(row["state"] == "answered" for row in input_rows(notebook.root))
    finally:
        first.close()
        second.close()


@pytest.mark.parametrize(
    "action",
    ["input.cancel", "operation.cancel", "interrupt", "legacy_interrupt", "interrupt_kill", "stop"],
)
def test_native_input_cancellation_and_next_prompt(native_runtime, action):
    notebook, process = native_runtime
    client = Client(notebook.root)
    try:
        source = "input('Cancel me: ')"
        accepted = client.submit("run", source=source, expected_sha256=source_hash(source))
        pending = wait_input(client)
        client.call("input.claim", input_id=pending["input_id"])
        if action == "input.cancel":
            client.call(action, input_id=pending["input_id"])
        elif action == "operation.cancel":
            client.call(action, operation_id=accepted["operation_id"])
        elif action == "legacy_interrupt":
            rpc(notebook.root, {"command": "interrupt"})
        elif action == "stop":
            client.submit("stop")
            process.wait(timeout=15)
        else:
            client.call(action)
        if action != "stop":
            result = client.wait(accepted["operation_id"])
            assert result["state"] in ("completed", "failed"), result
            assert not client.call("runtime.status")["pending_inputs"]
        assert input_rows(notebook.root)[0]["state"] == "canceled"
        if action != "stop":
            following = "assert input('Next: ') == 'fresh'"
            next_run = client.submit(
                "run", source=following, expected_sha256=source_hash(following)
            )
            prompt = wait_input(client)
            client.call("input.claim", input_id=prompt["input_id"])
            client.call("input.reply", input_id=prompt["input_id"], text="fresh")
            result = client.wait(next_run["operation_id"])
            assert result["result"]["status"] == "ok", result
    finally:
        client.close()


def test_native_input_reply_cancel_race_does_not_answer_next_prompt(native_runtime):
    notebook, _ = native_runtime
    owner = Client(notebook.root)
    control = Client(notebook.root)
    try:
        with concurrent.futures.ThreadPoolExecutor(max_workers=2) as pool:
            for _ in range(10):
                source = "input('Race: ')"
                accepted = owner.submit("run", source=source, expected_sha256=source_hash(source))
                pending = wait_input(owner)
                owner.call("input.claim", input_id=pending["input_id"])
                reply = pool.submit(
                    owner.call, "input.reply", input_id=pending["input_id"], text="old"
                )
                cancel = pool.submit(
                    control.call, "operation.cancel", operation_id=accepted["operation_id"]
                )
                try:
                    reply.result(timeout=10)
                except SourceError as error:
                    assert error.code == "not_found"
                cancel.result(timeout=10)
                result = owner.wait(accepted["operation_id"])
                assert result["state"] == "completed", result
                assert not owner.call("runtime.status")["pending_inputs"]
                following = "assert input('Fresh: ') == 'fresh'"
                next_run = owner.submit(
                    "run", source=following, expected_sha256=source_hash(following)
                )
                prompt = wait_input(owner)
                owner.call("input.claim", input_id=prompt["input_id"])
                owner.call("input.reply", input_id=prompt["input_id"], text="fresh")
                assert owner.wait(next_run["operation_id"])["result"]["status"] == "ok"
    finally:
        owner.close()
        control.close()


def test_native_pending_input_recovers_under_python(native_runtime):
    notebook, process = native_runtime
    client = Client(notebook.root)
    try:
        source = "import getpass\ngetpass.getpass('Pending: ')"
        accepted = client.submit("run", source=source, expected_sha256=source_hash(source))
        pending = wait_input(client)
        client.call("input.claim", input_id=pending["input_id"])
        process.kill()
        process.wait(timeout=10)
        client.close()
        ensure_supervisor(notebook.root)
        reopened = Client(notebook.root)
        try:
            assert not reopened.call("runtime.status")["pending_inputs"]
            assert (
                reopened.call("operation.get", operation_id=accepted["operation_id"])["state"]
                == "unknown"
            )
            row = input_rows(notebook.root)[0]
            assert row["state"] == "canceled" and row["client_name"] is None
        finally:
            reopened.close()
            rpc(notebook.root, {"command": "stop"})
    finally:
        client.close()


@contextmanager
def blocking(root):
    pool = concurrent.futures.ThreadPoolExecutor()
    future = pool.submit(
        rpc, root, {"command": "run", "source": "import time\nwhile True: time.sleep(0.01)"}
    )
    try:
        wait_active(root)
        yield future
    finally:
        if not future.done():
            try:
                rpc(root, {"command": "interrupt", "kill": True})
            except Exception:
                pass
        try:
            future.result(timeout=10)
        except Exception:
            pass
        pool.shutdown(wait=False, cancel_futures=True)


def test_native_desktop_run_retry_and_capability_subset(native_runtime):
    notebook, _ = native_runtime
    client = Client(notebook.root)
    try:
        assert set(client.welcome["capabilities"]) == {
            "operations",
            "batches",
            "blob_ranges",
            "input_claims",
            "change_feed",
        }
        assert not {"portable_training"} & set(client.welcome["capabilities"])
        source = "counter = globals().get('counter', 0) + 1"
        key = str(uuid.uuid4())
        fields = {
            "source": source,
            "expected_sha256": source_hash(source),
            "client_request_id": key,
        }
        first = client.call("run", **fields)
        assert first["kind"] == "accepted" and len(first["executions"]) == 1
        result = client.wait(first["operation_id"])
        assert result["state"] == "completed" and result["result"]["status"] == "ok"
        retry = client.call("run", **fields, request_id="new-transport-id")
        assert retry["operation_id"] == first["operation_id"]
        with pytest.raises(SourceError, match="request key"):
            client.call("run", **{**fields, "source": "counter += 1"})
        assert (
            rpc(notebook.root, {"command": "run", "source": "assert counter == 1"})["status"]
            == "ok"
        )
        status = client.call("runtime.status")
        assert status["interpreter"]["python"]
        changes = client.call("changes.list", page_size=1)
        assert len(changes["items"]) == 1 and changes["cursor"]
        assert changes["items"][0]["ref"] == json.loads(changes["items"][0]["ref_json"])
        next_page = client.call("changes.list", page_size=1, cursor=changes["cursor"])
        assert next_page["items"][0]["change_seq"] > changes["items"][0]["change_seq"]
        assert (
            status["worker_state"] == "idle"
            and status["session_id"] == result["result"]["session_id"]
        )
    finally:
        client.close()


def test_native_desktop_batch_is_durable_and_frozen_before_reply(native_runtime):
    notebook, _ = native_runtime
    notebook.add("code", "events = ['before']", "a")
    notebook.add("code", "events.append('second')", "b")
    client = Client(notebook.root)
    try:
        with blocking(notebook.root) as active:
            accepted = client.submit(
                "batch",
                cell_ids=["a", "b"],
                expected_sha256={
                    "a": source_hash("events = ['before']"),
                    "b": source_hash("events.append('second')"),
                },
            )
            ledger = Ledger(notebook.root / ".kl/ledger.sqlite", readonly=True)
            try:
                row = ledger.rows(
                    "SELECT * FROM operations WHERE operation_id=?", (accepted["operation_id"],)
                )[0]
                assert row["state"] == "accepted"
                ids = json.loads(row["result_json"])["exec_ids"]
                assert ids == accepted["executions"]
                assert (
                    len(
                        ledger.rows(
                            "SELECT * FROM batches WHERE operation_id=?",
                            (accepted["operation_id"],),
                        )
                    )
                    == 1
                )
                assert all(
                    ledger.rows("SELECT status FROM executions WHERE exec_id=?", (eid,))[0][
                        "status"
                    ]
                    == "queued"
                    for eid in ids
                )
            finally:
                ledger.close()
            notebook.cell_path(notebook.cell("a")).write_text(
                "raise RuntimeError('edited after admission')"
            )
            client.call("interrupt")
            assert active.result(timeout=10)["status"] == "interrupted"
            finished = client.wait(accepted["operation_id"])
            assert finished["state"] == "completed", finished
            assert [value["status"] for value in finished["result"]["executions"]] == ["ok", "ok"]
            assert (
                rpc(
                    notebook.root,
                    {"command": "run", "source": "assert events == ['before', 'second']"},
                )["status"]
                == "ok"
            )
    finally:
        client.close()


def test_native_desktop_cancels_queued_execution_without_running_it(native_runtime):
    notebook, _ = native_runtime
    client = Client(notebook.root)
    try:
        with blocking(notebook.root) as active:
            source = "forbidden_effect = True"
            accepted = client.submit("run", source=source, expected_sha256=source_hash(source))
            assert (
                client.call("operation.cancel", operation_id=accepted["operation_id"])["state"]
                == "canceled"
            )
            client.call("interrupt")
            active.result(timeout=10)
            assert client.wait(accepted["operation_id"])["state"] == "canceled"
            assert (
                rpc(
                    notebook.root,
                    {"command": "run", "source": "assert 'forbidden_effect' not in globals()"},
                )["status"]
                == "ok"
            )
    finally:
        client.close()


def test_native_desktop_lost_reply_and_removed_source_reconcile_by_key(native_runtime):
    notebook, _ = native_runtime
    notebook.add("code", "once = globals().get('once', 0) + 1", "a")
    first = Client(notebook.root)
    second = Client(notebook.root, client_id=first.client_id)
    request = {
        "method": "run",
        "cell_id": "a",
        "expected_sha256": source_hash("once = globals().get('once', 0) + 1"),
        "client_request_id": str(uuid.uuid4()),
    }
    try:
        with blocking(notebook.root) as active:
            first.sock.sendall(encode(request))
            deadline = time.monotonic() + 10
            while True:
                try:
                    operation = second.call(
                        "operation.get", client_request_id=request["client_request_id"]
                    )
                    break
                except SourceError as error:
                    assert error.code == "not_found"
                assert time.monotonic() < deadline
                time.sleep(0.01)
            first.close()
            notebook.cell_path(notebook.cell("a")).unlink()
            retry = second.call("run", **{k: v for k, v in request.items() if k != "method"})
            assert retry["operation_id"] == operation["operation_id"]
            second.call("interrupt")
            active.result(timeout=10)
            assert second.wait(operation["operation_id"])["state"] == "completed"
            assert (
                rpc(notebook.root, {"command": "run", "source": "assert once == 1"})["status"]
                == "ok"
            )
    finally:
        first.close()
        second.close()


def test_native_desktop_accepted_queue_reopens_in_python_after_crash(native_runtime):
    notebook, process = native_runtime
    client = Client(notebook.root)
    try:
        with blocking(notebook.root) as active:
            source = "should_not_run = True"
            accepted = client.submit("run", source=source, expected_sha256=source_hash(source))
            process.kill()
            process.wait(timeout=5)
            with pytest.raises(Exception):
                active.result(timeout=10)
    finally:
        client.close()
    ensure_supervisor(notebook.root)
    reopened = Client(notebook.root)
    try:
        operation = reopened.call("operation.get", operation_id=accepted["operation_id"])
        assert operation["state"] == "canceled"
        assert all(value["status"] == "canceled" for value in operation["result"]["executions"])
    finally:
        reopened.close()
        rpc(notebook.root, {"command": "stop"})


def test_native_desktop_stop_bypasses_running_work_and_commits_result(native_runtime):
    notebook, process = native_runtime
    client = Client(notebook.root)
    try:
        source = "import time\nwhile True: time.sleep(0.01)"
        running = client.submit("run", source=source, expected_sha256=source_hash(source))
        wait_active(notebook.root)
        queued_source = "never = 1"
        queued = client.submit(
            "run", source=queued_source, expected_sha256=source_hash(queued_source)
        )
        stopped = client.submit("stop")
        assert process.wait(timeout=15) == 0
        ledger = Ledger(notebook.root / ".kl/ledger.sqlite", readonly=True)
        try:
            rows = {row["operation_id"]: row for row in ledger.rows("SELECT * FROM operations")}
            assert rows[stopped["operation_id"]]["state"] == "completed"
            assert json.loads(rows[stopped["operation_id"]]["result_json"]) == {"stopped": True}
            assert rows[queued["operation_id"]]["state"] == "canceled"
            assert rows[running["operation_id"]]["state"] in ("completed", "failed", "unknown")
        finally:
            ledger.close()
        lock = DirectoryLock(notebook.root / ".kl").acquire()
        lock.close()
    finally:
        client.close()


def test_native_desktop_running_cancel_keeps_the_worker_usable(native_runtime):
    notebook, _ = native_runtime
    client = Client(notebook.root)
    try:
        source = "import time\nwhile True: time.sleep(0.01)"
        accepted = client.submit("run", source=source, expected_sha256=source_hash(source))
        wait_active(notebook.root)
        assert (
            client.call("operation.cancel", operation_id=accepted["operation_id"])["kind"]
            == "cancel_requested"
        )
        result = client.wait(accepted["operation_id"])
        assert result["state"] == "completed" and result["result"]["status"] == "interrupted"
        assert rpc(notebook.root, {"command": "run", "source": "assert True"})["status"] == "ok"
    finally:
        client.close()


def test_native_desktop_cancel_race_with_worker_envelopes(native_runtime):
    notebook, _ = native_runtime
    client = Client(notebook.root)
    source = "# envelope padding\n" * 5000 + "import time\nwhile True: time.sleep(0.01)"
    try:
        for _ in range(10):
            accepted = client.submit("run", source=source, expected_sha256=source_hash(source))
            client.call("operation.cancel", operation_id=accepted["operation_id"])
            result = client.wait(accepted["operation_id"])
            assert result["state"] in ("canceled", "completed"), json.dumps(result)
            if result["state"] == "completed":
                assert result["result"]["status"] in ("canceled", "interrupted")
        assert rpc(notebook.root, {"command": "run", "source": "pass"})["status"] == "ok"
    finally:
        client.close()


@pytest.mark.parametrize("policy", ["stop", "continue"])
def test_native_desktop_batch_error_policy(native_runtime, policy):
    notebook, _ = native_runtime
    notebook.add("code", "raise ValueError('expected')", "a")
    notebook.add("code", "continued = True", "b")
    client = Client(notebook.root)
    try:
        accepted = client.submit("batch", cell_ids=["a", "b"], on_error=policy)
        result = client.wait(accepted["operation_id"])
        assert result["state"] == "completed"
        expected = ["error", "ok"] if policy == "continue" else ["error"]
        assert [item["status"] for item in result["result"]["executions"]] == expected
        ledger = Ledger(notebook.root / ".kl/ledger.sqlite", readonly=True)
        try:
            status = ledger.rows(
                "SELECT status FROM executions WHERE exec_id=?", (accepted["executions"][1],)
            )[0]["status"]
            assert status == ("ok" if policy == "continue" else "canceled")
        finally:
            ledger.close()
    finally:
        client.close()


def test_native_desktop_checkpoint_refuses_busy_and_validates_source(native_runtime):
    notebook, _ = native_runtime
    client = Client(notebook.root)
    try:
        with pytest.raises(SourceError) as error:
            client.submit("run", source="wrong = True", expected_sha256=source_hash("different"))
        assert error.value.code == "source_conflict"
        notebook.add("code", "current = 1", "stale")
        with pytest.raises(SourceError) as error:
            client.submit("run", cell_id="stale", expected_sha256=source_hash("old = 1"))
        assert error.value.code == "source_conflict"
        with pytest.raises(SourceError) as error:
            client.submit("run", source="pass", expected_sha256=source_hash("pass"), extra=1.5)
        assert error.value.code == "invalid_request"
        assert client.call("runtime.status")["worker_state"] == "absent"
        with blocking(notebook.root) as active:
            accepted = client.submit("checkpoint")
            result = client.wait(accepted["operation_id"])
            assert result["state"] == "completed" and result["result"] == {"refused": "busy"}
            client.call("interrupt")
            active.result(timeout=10)
    finally:
        client.close()


def test_native_desktop_authentication_and_frame_limits(native_runtime):
    notebook, _ = native_runtime
    info = json.loads((notebook.root / ".kl/supervisor.json").read_text())
    with socket.create_connection(("127.0.0.1", info["desktop_port"]), timeout=5) as sock:
        sock.sendall(
            encode(
                {
                    "kind": "hello",
                    "api": API,
                    "versions": [1],
                    "role": "control",
                    "client_id": str(uuid.uuid4()),
                    "client_name": "bad-token",
                    "token": "wrong",
                }
            )
        )
        with sock.makefile("rb") as stream:
            size = struct.unpack(">I", stream.read(4))[0]
            error = json.loads(stream.read(size))
            assert error["kind"] == "error" and error["error"]["code"] == "unauthenticated"
    client = Client(notebook.root)
    try:
        client.sock.sendall(struct.pack(">I", LIMIT + 1))
        error = client.read()
        assert error["kind"] == "error" and error["error"]["code"] == "too_large"
    finally:
        client.close()
    assert rpc(notebook.root, {"command": "status"})["session_id"] is None


def test_native_desktop_blob_role_streams_multiple_verified_chunks(native_runtime):
    notebook, _ = native_runtime
    payload = b"x" * (2 * 1024 * 1024 + 123)
    digest = BlobStore(notebook.root / ".kl").put(payload)
    client = Client(notebook.root, role="blob")
    try:
        with pytest.raises(SourceError) as error:
            client.call("runtime.status")
        assert error.value.code == "invalid_request"
        client.sock.sendall(encode({"method": "blob.read", "sha256": digest, "request_id": "blob"}))
        received = bytearray()
        chunks = 0
        while True:
            frame = client.read()
            assert frame["kind"] == "blob" and frame["request_id"] == "blob"
            assert frame["offset"] == len(received)
            received.extend(client.file.read(frame["byte_size"]))
            chunks += 1
            if frame["final"]:
                assert frame["verified"]
                break
        assert bytes(received) == payload and chunks == 3
        (notebook.root / ".kl/blobs" / digest).write_bytes(b"y" * len(payload))
        with pytest.raises(SourceError) as error:
            client.call("blob.read", sha256=digest)
        assert error.value.code == "invalid_request"
    finally:
        client.close()


def test_native_desktop_queue_overload_is_rejected_before_acceptance(native_runtime):
    notebook, _ = native_runtime
    client = Client(notebook.root)
    try:
        with blocking(notebook.root) as active:
            accepted = [
                client.submit("run", source="pass", expected_sha256=source_hash("pass"))
                for _ in range(15)
            ]
            key = str(uuid.uuid4())
            fields = {
                "source": "pass",
                "expected_sha256": source_hash("pass"),
                "client_request_id": key,
            }
            with pytest.raises(SourceError) as error:
                client.call("run", **fields)
            assert error.value.code == "busy"
            with pytest.raises(SourceError) as error:
                client.call("operation.get", client_request_id=key)
            assert error.value.code == "not_found"
            for operation in accepted:
                client.call("operation.cancel", operation_id=operation["operation_id"])
            client.call("interrupt")
            active.result(timeout=10)
            deadline = time.monotonic() + 10
            while True:
                try:
                    retry = client.call("run", **fields)
                    break
                except SourceError as error:
                    assert error.code == "busy"
                assert time.monotonic() < deadline
                time.sleep(0.01)
            assert client.wait(retry["operation_id"])["state"] == "completed"
    finally:
        client.close()
