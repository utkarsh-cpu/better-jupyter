"""Committed native feeds, replay, resync bounds, and slow socket isolation."""

import json
import select
import sqlite3
import time

import pytest
from test_desktop import Client
from test_desktop import test_subscription_follow_and_verified_blob as check_feed_contract
from test_native_desktop import wait_input
from test_native_supervisor import native_runtime as native_runtime

from kernelledger.cli import rpc
from kernelledger.desktop.protocol import encode
from kernelledger.ledger import Ledger
from kernelledger.notebook import source_hash
from kernelledger.source import SourceError


def send(client, method, **fields):
    client.sock.sendall(encode({"method": method, **fields}))


def sql(root):
    return sqlite3.connect(root / ".kl/ledger.sqlite", timeout=5)


def watermark(root):
    with sql(root) as db:
        return db.execute("SELECT COALESCE(MAX(change_seq),0) FROM changes").fetchone()[0]


def test_native_feed_matches_python_contract(native_runtime):
    notebook, _ = native_runtime
    check_feed_contract(notebook)


def test_native_feed_only_delivers_committed_rows_and_replays_cursor(native_runtime):
    notebook, _ = native_runtime
    feed = Client(notebook.root, role="subscription")
    try:
        cursor = watermark(notebook.root)
        send(feed, "subscribe", after_change_seq=cursor)
        db = sql(notebook.root)
        try:
            db.execute("INSERT INTO changes(kind,ref_json,timestamp) VALUES ('test','{}','now')")
            assert not select.select([feed.sock], [], [], 0.2)[0]
            db.commit()
            first = feed.read()
            assert first["kind"] == "change" and first["change_seq"] > cursor
            db.execute(
                "INSERT INTO changes(kind,ref_json,timestamp) VALUES ('rolled_back','{}','now')"
            )
            db.rollback()
            assert not select.select([feed.sock], [], [], 0.2)[0]
            db.execute("INSERT INTO changes(kind,ref_json,timestamp) VALUES ('test2','{}','now')")
            db.commit()
            second = feed.read()
            assert second["change_kind"] == "test2"
        finally:
            db.close()
        feed.close()
        feed = Client(notebook.root, role="subscription")
        send(feed, "subscribe", after_change_seq=first["change_seq"])
        assert feed.read() == second
    finally:
        feed.close()


def test_native_feed_snapshot_pending_claim_and_redacted_event_replay(native_runtime):
    notebook, _ = native_runtime
    control = Client(notebook.root, name="Owner")
    feed = Client(notebook.root, role="subscription")
    try:
        source = "import getpass\nanswer = getpass.getpass('Secret: ')"
        accepted = control.submit("run", source=source, expected_sha256=source_hash(source))
        pending = wait_input(control)
        control.call("input.claim", input_id=pending["input_id"])
        queued_source = "raise AssertionError('queued work must be canceled')"
        queued = control.submit(
            "run", source=queued_source, expected_sha256=source_hash(queued_source)
        )
        send(feed, "subscribe")
        snapshot = feed.read()
        assert snapshot["kind"] == "snapshot"
        runtime = snapshot["runtime"]
        assert runtime["worker_state"] == "waiting_input"
        assert runtime["pending_inputs"][0]["client_name"] == "Owner"
        assert runtime["active_execution"] == pending["exec_id"]
        assert [row["exec_id"] for row in runtime["queue"]] == queued["executions"]
        assert {row["operation_id"] for row in runtime["active_operations"]} == {
            accepted["operation_id"],
            queued["operation_id"],
        }
        with sql(notebook.root) as db:
            assert (
                db.execute(
                    "SELECT MAX(change_seq) FROM changes WHERE kind='input_state'"
                ).fetchone()[0]
                <= snapshot["change_seq"]
            )
        secret = "feed-secret-do-not-record"
        control.call("operation.cancel", operation_id=queued["operation_id"])
        control.call("input.reply", input_id=pending["input_id"], text=secret)
        result = control.wait(accepted["operation_id"])
        frames = []
        while True:
            frame = feed.read()
            frames.append(frame)
            if (
                frame["ref"].get("operation_id") == accepted["operation_id"]
                and frame["ref"].get("state") == "completed"
            ):
                break
        assert all(frame["change_seq"] > snapshot["change_seq"] for frame in frames)
        assert any(frame["ref"].get("state") == "answered" for frame in frames)
        feed.close()
        feed = Client(notebook.root, role="subscription")
        ledger = Ledger(notebook.root / ".kl/ledger.sqlite", readonly=True)
        try:
            expected = ledger.events(result["result"]["exec_id"])
        finally:
            ledger.close()
        cursor = expected[0]["seq"]
        send(feed, "execution.follow", exec_id=pending["exec_id"], after_seq=cursor)
        actual = [feed.read()["event"] for _ in expected[1:]]
        assert actual == expected[1:]
        assert any(event["kind"] == "stdin_reply" and event["redacted"] for event in actual)
        assert secret not in json.dumps(frames + actual + [snapshot])
    finally:
        control.close()
        feed.close()


@pytest.mark.parametrize("limit", ["rows", "bytes", "frame"])
def test_native_feed_resync_bounds_and_connection_reuse(native_runtime, limit):
    notebook, _ = native_runtime
    cursor = watermark(notebook.root)
    count, size = {"rows": (10001, 0), "bytes": (20, 450000), "frame": (1, 700000)}[limit]
    payload = json.dumps({"padding": "x" * size})
    with sql(notebook.root) as db:
        db.executemany(
            "INSERT INTO changes(kind,ref_json,timestamp) VALUES ('test',?,'now')",
            ((payload,) for _ in range(count)),
        )
    feed = Client(notebook.root, role="subscription")
    try:
        send(feed, "subscribe", after_change_seq=cursor)
        assert feed.read() == {"kind": "resync_required", "after_change_seq": cursor}
        send(feed, "ping")
        assert feed.read()["kind"] == "pong"
        after = watermark(notebook.root)
        send(feed, "subscribe", after_change_seq=after)
        with sql(notebook.root) as db:
            db.execute(
                "INSERT INTO changes(kind,ref_json,timestamp) VALUES ('after_resync','{}','now')"
            )
        assert feed.read()["change_kind"] == "after_resync"
    finally:
        feed.close()


def test_native_execution_feed_resync_for_oversized_event(native_runtime):
    notebook, _ = native_runtime
    execution = rpc(notebook.root, {"command": "run", "source": "pass"})["exec_id"]
    with sql(notebook.root) as db:
        seq = db.execute("SELECT MAX(seq) FROM events WHERE exec_id=?", (execution,)).fetchone()[0]
        db.execute(
            "INSERT INTO events VALUES (?,?,?,?,?,NULL)",
            (execution, seq + 1, "stdout", "now", json.dumps({"text": "x" * 1100000})),
        )
    feed = Client(notebook.root, role="subscription")
    try:
        send(feed, "execution.follow", exec_id=execution, after_seq=seq)
        assert feed.read() == {"kind": "resync_required", "exec_id": execution, "after_seq": seq}
    finally:
        feed.close()


def test_native_execution_follow_streams_before_completion(native_runtime):
    notebook, _ = native_runtime
    control = Client(notebook.root)
    feed = Client(notebook.root, role="subscription")
    try:
        source = "print('before')\ninput('Continue: ')\nprint('after')"
        accepted = control.submit("run", source=source, expected_sha256=source_hash(source))
        pending = wait_input(control)
        send(feed, "execution.follow", exec_id=pending["exec_id"])
        events = []
        while True:
            event = feed.read()["event"]
            events.append(event)
            if event["kind"] == "stdin_request":
                break
        assert any("before" in event.get("text", "") for event in events)
        assert (
            control.call("operation.get", operation_id=accepted["operation_id"])["state"]
            == "running"
        )
        control.call("input.claim", input_id=pending["input_id"])
        control.call("input.reply", input_id=pending["input_id"], text="continue")
        while True:
            event = feed.read()["event"]
            events.append(event)
            if event.get("state") == "idle":
                break
        assert any("after" in event.get("text", "") for event in events)
        assert control.wait(accepted["operation_id"])["result"]["status"] == "ok"
        ledger = Ledger(notebook.root / ".kl/ledger.sqlite", readonly=True)
        try:
            assert events == ledger.events(pending["exec_id"])
        finally:
            ledger.close()
    finally:
        feed.close()
        control.close()


def test_native_feed_role_stream_and_cursor_limits(native_runtime):
    notebook, _ = native_runtime
    control = Client(notebook.root)
    feed = Client(notebook.root, role="subscription")
    try:
        with pytest.raises(SourceError) as error:
            control.call("subscribe")
        assert error.value.code == "invalid_request"
        for request in [
            {
                "method": "run",
                "source": "pass",
                "expected_sha256": source_hash("pass"),
                "client_request_id": "00000000-0000-0000-0000-000000000000",
            },
            {"method": "subscribe", "after_change_seq": 2**63},
            {"method": "execution.follow"},
        ]:
            feed.sock.sendall(encode(request))
            assert feed.read()["error"]["code"] == "invalid_request"
        for _ in range(8):
            send(feed, "execution.follow", exec_id="absent")
        send(feed, "execution.follow", exec_id="overflow")
        assert feed.read()["error"]["code"] == "busy"
        assert control.call("runtime.status")["worker_state"] == "absent"
    finally:
        control.close()
        feed.close()


def test_native_slow_execution_feed_does_not_block_control_or_stop(native_runtime):
    notebook, process = native_runtime
    execution = rpc(notebook.root, {"command": "run", "source": "pass"})["exec_id"]
    with sql(notebook.root) as db:
        seq = db.execute("SELECT MAX(seq) FROM events WHERE exec_id=?", (execution,)).fetchone()[0]
        payload = json.dumps({"text": "x" * 30000})
        db.executemany(
            "INSERT INTO events VALUES (?,?,?,?,?,NULL)",
            ((execution, seq + i, "stdout", "now", payload) for i in range(1, 201)),
        )
    slow = Client(notebook.root, role="subscription")
    control = Client(notebook.root)
    try:
        import socket

        slow.sock.setsockopt(socket.SOL_SOCKET, socket.SO_RCVBUF, 1024)
        send(slow, "execution.follow", exec_id=execution, after_seq=seq)
        time.sleep(0.2)
        started = time.monotonic()
        for _ in range(3):
            assert control.call("runtime.status")["worker_state"] == "idle"
        assert time.monotonic() - started < 2
        assert rpc(notebook.root, {"command": "run", "source": "assert True"})["status"] == "ok"
        rpc(notebook.root, {"command": "stop"})
        process.wait(timeout=15)
        assert process.returncode == 0
    finally:
        slow.close()
        control.close()
