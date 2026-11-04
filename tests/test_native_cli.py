"""Interactive legacy CLI over the native owner; no secret blob publication."""

import json
import os
import subprocess
import sys
import time
import uuid
from contextlib import contextmanager
from pathlib import Path

import pytest
from test_desktop import Client
from test_native_desktop import input_rows, wait_input
from test_native_supervisor import native_runtime as native_runtime

from kernelledger.cli import connect, rpc
from kernelledger.ledger import Ledger
from kernelledger.source import SourceError
from kernelledger.worker import receive


@contextmanager
def interactive(root, source, **fields):
    sock, channel = connect(root)
    sock.settimeout(15)
    try:
        channel.send("request", command="run", source=source, interactive=True, **fields)
        yield sock, channel
    finally:
        sock.close()


@pytest.mark.parametrize("idless", [False, True])
def test_native_cli_multiple_prompts_and_recorded_replies(native_runtime, idless):
    notebook, _ = native_runtime
    with interactive(notebook.root, "assert (input('One: '), input('Two: ')) == ('one','two')") as (
        sock,
        channel,
    ):
        for text in ("one", "two"):
            prompt = receive(sock)
            assert prompt["kind"] == "stdin_request" and prompt["recorded"]
            assert input_rows(notebook.root)[-1]["client_name"] == "CLI"
            channel.send(
                "stdin_reply", text=text, **({} if idless else {"input_id": prompt["input_id"]})
            )
        response = receive(sock)
        assert response["result"]["status"] == "ok", response
    assert all(row["state"] == "answered" for row in input_rows(notebook.root))


def test_native_cli_actual_process_input_and_eof(native_runtime):
    notebook, _ = native_runtime
    environment = dict(os.environ, PYTHONPATH=str(Path(__file__).resolve().parents[1] / "src"))
    flags = {"creationflags": 0x08000000} if os.name == "nt" else {}
    args = [
        sys.executable,
        "-m",
        "kernelledger",
        "--project",
        str(notebook.root),
        "run",
        "--source",
        "assert input('Value: ' + 'p' * 70000) == 'hello'",
    ]
    completed = subprocess.run(
        args, input="hello\n", capture_output=True, text=True, timeout=30, env=environment, **flags
    )
    assert completed.returncode == 0, completed.stderr
    assert "Reply recorded in history and bundles" in completed.stderr
    assert input_rows(notebook.root)[0]["state"] == "answered"
    # An actual EOF uses the same reply route and durable cancellation record.
    completed = subprocess.run(
        args, input="", capture_output=True, text=True, timeout=30, env=environment, **flags
    )
    assert "stdin is closed" in json.dumps(ledger_events(notebook.root))
    assert input_rows(notebook.root)[-1]["state"] == "canceled"


def ledger_events(root):
    ledger = Ledger(root / ".kl/ledger.sqlite", readonly=True)
    try:
        return ledger.rows("SELECT kind,payload_json FROM events ORDER BY exec_id,seq")
    finally:
        ledger.close()


def test_native_cli_large_secret_never_publishes_reply_envelope(native_runtime):
    notebook, _ = native_runtime
    secret = "legacy-private-" + "z" * 100_000
    with interactive(
        notebook.root, "import getpass\nassert len(getpass.getpass('Secret: ')) == 100015"
    ) as (sock, channel):
        prompt = receive(sock)
        assert prompt["password"] and not prompt["recorded"]
        channel.send("stdin_reply", text=secret, input_id=prompt["input_id"])
        response = receive(sock)
        assert response["result"]["status"] == "ok", response
    replies = [
        json.loads(row["payload_json"])
        for row in ledger_events(notebook.root)
        if row["kind"] == "stdin_reply"
    ]
    assert replies == [{"password": True, "redacted": True}]
    for path in notebook.root.rglob("*"):
        if path.is_file() and path != notebook.root / ".kl/lock":
            assert secret.encode() not in path.read_bytes(), path


def test_native_cli_disconnect_hands_prompt_to_desktop(native_runtime):
    notebook, _ = native_runtime
    desktop = Client(notebook.root)
    try:
        with interactive(notebook.root, "assert input('Handoff: ') == 'desktop'") as (sock, _):
            prompt = receive(sock)
            pending = wait_input(desktop)
            assert pending["client_name"] == "CLI"
            with pytest.raises(SourceError) as error:
                desktop.call("input.claim", input_id=prompt["input_id"])
            assert error.value.code == "input_claimed"
        deadline = time.monotonic() + 10
        while True:
            try:
                desktop.call("input.claim", input_id=prompt["input_id"])
                break
            except SourceError:
                assert time.monotonic() < deadline
                time.sleep(0.01)
        desktop.call("input.reply", input_id=prompt["input_id"], text="desktop")
        while rpc(notebook.root, {"command": "status"})["in_flight"]:
            assert time.monotonic() < deadline
            time.sleep(0.01)
        assert input_rows(notebook.root)[0]["state"] == "answered"
        assert rpc(notebook.root, {"command": "run", "source": "assert True"})["status"] == "ok"
    finally:
        desktop.close()


@pytest.mark.parametrize("command", ["interrupt", "stop"])
def test_native_cli_control_while_waiting_for_reply(native_runtime, command):
    notebook, process = native_runtime
    with interactive(notebook.root, "input('Wait: ')") as (sock, _):
        assert receive(sock)["kind"] == "stdin_request"
        rpc(notebook.root, {"command": command})
        response = receive(sock)
        if command == "interrupt":
            assert response["result"]["status"] == "interrupted", response
        else:
            assert "error" in response
            process.wait(timeout=15)
    assert input_rows(notebook.root)[0]["state"] == "canceled"


@pytest.mark.parametrize("invalid", ["oversized", "corrupt", "wrong_id"])
def test_native_cli_bad_reply_releases_claim_without_delivery(native_runtime, invalid):
    notebook, _ = native_runtime
    desktop = Client(notebook.root)
    try:
        with interactive(notebook.root, "input('Limit: ')") as (sock, channel):
            prompt = receive(sock)
            if invalid == "oversized":
                channel.send("blob", sha256="a" * 64, byte_size=1024 * 1024 + 1)
                expected = "input envelope exceeds 1 MiB"
            elif invalid == "corrupt":
                channel.send("blob", sha256="a" * 64, byte_size=3)
                sock.sendall(b"bad")
                expected = "blob hash mismatch"
            else:
                channel.send("stdin_reply", input_id="wrong", text="must not reach worker")
                expected = "stdin reply does not match"
            response = receive(sock)
            assert expected in response["error"]
        deadline = time.monotonic() + 10
        while True:
            try:
                desktop.call("input.claim", input_id=prompt["input_id"])
                break
            except SourceError:
                assert time.monotonic() < deadline
                time.sleep(0.01)
        desktop.call("input.cancel", input_id=prompt["input_id"])
    finally:
        desktop.close()


def test_native_cli_keyed_retry_returns_result_without_prompt_or_execution(native_runtime):
    notebook, _ = native_runtime
    fields = {"client_id": str(uuid.uuid4()), "client_request_id": str(uuid.uuid4())}
    source = "counter = globals().get('counter',0) + 1\nassert input('Once: ') == 'yes'"
    with interactive(notebook.root, source, **fields) as (sock, channel):
        prompt = receive(sock)
        channel.send("stdin_reply", text="yes", input_id=prompt["input_id"])
        result = receive(sock)["result"]
        assert result["status"] == "ok"
    retry = rpc(notebook.root, {"command": "run", "source": source, "interactive": True, **fields})
    assert retry == result
    assert len(input_rows(notebook.root)) == 1
    assert rpc(notebook.root, {"command": "run", "source": "assert counter == 1"})["status"] == "ok"
