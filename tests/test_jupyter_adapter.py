"""Optional real Jupyter client attachment; the adapter never starts an ipykernel."""

import json
import os
import subprocess
import sys
import time
import uuid
from pathlib import Path

import pytest

from kernelledger.cli import ensure_supervisor, rpc
from kernelledger.jupyter_adapter import connection_info, execution_key, kernelspec

pytest_plugins = ["test_shell"]


def test_execution_identity_is_stable_and_session_scoped():
    message = {"header": {"session": "client", "msg_id": "execute"}}
    assert uuid.UUID(execution_key(message))
    assert execution_key(message) == execution_key(message)
    assert execution_key(message) != execution_key(
        {"header": {"session": "other", "msg_id": "execute"}}
    )


def test_connection_refuses_remote_and_unsigned(tmp_path):
    path = tmp_path / "connection.json"
    path.write_text(json.dumps({"ip": "0.0.0.0", "key": ""}))
    path.chmod(0o600)
    with pytest.raises(ValueError):
        connection_info(path)


def test_kernelspec_preview_does_not_start_or_install(notebook):
    spec = kernelspec(notebook.root, "notebook-test")
    assert spec["installed_path"] is None
    assert "jupyter-adapter" in spec["kernel_spec"]["argv"]
    assert not (notebook.root / ".kl/supervisor.json").exists()
    with pytest.raises(ValueError):
        kernelspec(notebook.root, "../escape", install=True)


def test_real_jupyter_client_shared_namespace_completion_and_detach(notebook, tmp_path):
    jupyter_client = pytest.importorskip("jupyter_client")
    from jupyter_client.connect import write_connection_file

    ensure_supervisor(notebook.root)
    assert rpc(notebook.root, {"command": "run", "source": "shared = 40"})["status"] == "ok"
    connection = tmp_path / "kernel.json"
    write_connection_file(str(connection), key=b"kernelledger-test-connection-key-32")
    env = {**os.environ, "PYTHONPATH": str(Path(__file__).resolve().parents[1] / "src")}
    process = subprocess.Popen(
        [
            sys.executable,
            "-m",
            "kernelledger",
            "-C",
            str(notebook.root),
            "jupyter-adapter",
            "-f",
            str(connection),
        ],
        env=env,
        stdin=subprocess.DEVNULL,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        creationflags=0x08000000 if os.name == "nt" else 0,
    )
    client = jupyter_client.BlockingKernelClient(connection_file=str(connection))
    client.load_connection_file()
    client.start_channels()

    def shell_reply(request_id):
        deadline = time.monotonic() + 15
        while True:
            reply = client.get_shell_msg(timeout=max(0.1, deadline - time.monotonic()))
            if reply["parent_header"].get("msg_id") == request_id:
                return reply
            assert time.monotonic() < deadline, reply

    try:
        deadline = time.monotonic() + 20
        while True:
            try:
                client.wait_for_ready(timeout=max(0.1, deadline - time.monotonic()))
                break
            except RuntimeError:
                # The bare attachment client initially assumes its heartbeat
                # is beating, before the adapter has bound sockets. Retry only
                # the read-only readiness handshake, never execute requests.
                if process.poll() is not None or time.monotonic() >= deadline:
                    raise
                time.sleep(0.05)
        message_id = client.execute("shared += 2\nshared")
        reply = shell_reply(message_id)
        assert reply["parent_header"]["msg_id"] == message_id, reply
        assert reply["content"]["status"] == "ok", reply
        assert (
            rpc(notebook.root, {"command": "run", "source": "assert shared == 42"})["status"]
            == "ok"
        )
        complete_id = client.complete("sha", 3)
        reply = shell_reply(complete_id)
        assert reply["parent_header"]["msg_id"] == complete_id
        assert "shared" in reply["content"]["matches"]
        input_id = client.execute("answer = input('Value: ')\nassert answer == 'hello'")
        prompt = client.get_stdin_msg(timeout=10)
        assert prompt["content"]["prompt"] == "Value: "
        client.input("hello")
        reply = shell_reply(input_id)
        assert (
            reply["parent_header"]["msg_id"] == input_id and reply["content"]["status"] == "ok"
        ), reply
        history_id = client.history(hist_access_type="tail", n=10)
        history = shell_reply(history_id)
        assert any("shared" in item[2] for item in history["content"]["history"])
        interrupted = client.execute("import time\ntime.sleep(30)")
        deadline = time.monotonic() + 10
        while not rpc(notebook.root, {"command": "status"})["in_flight"]:
            assert time.monotonic() < deadline
            time.sleep(0.01)
        client.control_channel.send(client.session.msg("interrupt_request"))
        assert client.get_control_msg(timeout=10)["content"]["status"] == "ok"
        reply = shell_reply(interrupted)
        assert (
            reply["parent_header"]["msg_id"] == interrupted
            and reply["content"]["status"] == "error"
        ), reply
        client.shutdown(restart=False)
        process.wait(timeout=10)
        assert (
            rpc(notebook.root, {"command": "run", "source": "assert shared == 42"})["status"]
            == "ok"
        )
    finally:
        client.stop_channels()
        if process.poll() is None:
            process.terminate()
            process.wait(timeout=5)
        if process.returncode:
            print(process.stderr.read().decode(errors="replace"))
