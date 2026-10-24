"""Protocol state the desktop shell reads after runtime operations.

There is no ``ui.query`` method. These tests use ``runtime.status``, durable
operation results, and the offline helper's history and checkpoint reads.
"""

import time

import pytest

from kernelledger.cli import alive, ensure_supervisor, rpc
from kernelledger.desktop.helper import Helper
from kernelledger.notebook import Notebook
from kernelledger.source import source_hash
from tests.test_desktop import Client


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


def test_runtime_status_reports_shell_fields(project):
    client = Client(project.root)
    try:
        status = client.call("runtime.status")
        assert status["notebook_id"] == project.read()["id"]
        assert status["project_path"] == str(project.root)
        assert status["worker_state"] == "absent"
        assert status["session_id"] is None
        assert status["attached_client_count"] >= 1
        assert status["active_execution"] is None
        assert status["pending_inputs"] == []
    finally:
        client.close()


def test_completed_cell_output_is_visible_offline(project):
    source = "print(42)"
    client = Client(project.root)
    try:
        accepted = client.submit("run", source=source, expected_sha256=source_hash(source))
        finished = client.wait(accepted["operation_id"])
        assert finished["state"] == "completed"
        assert finished["result"]["status"] == "ok"
        status = client.call("runtime.status")
        assert status["worker_state"] == "idle"
        assert status["active_execution"] is None
        exec_id = finished["result"]["exec_id"]
    finally:
        client.close()

    view = Helper(project.root).dispatch({"method": "fold.get", "exec_id": exec_id})
    assert view["status"] == "ok"
    assert any(
        item["kind"] == "stdout" and "42" in item.get("text", "") for item in view["outputs"]
    )


def test_checkpoint_appears_in_helper_listing(project):
    source = "value = 7"
    client = Client(project.root)
    try:
        started = client.submit("run", source=source, expected_sha256=source_hash(source))
        assert client.wait(started["operation_id"])["result"]["status"] == "ok"
        accepted = client.submit("checkpoint")
        finished = client.wait(accepted["operation_id"])
        assert finished["state"] == "completed"
        checkpoint_id = finished["result"]["checkpoint_id"]
        assert finished["result"]["status"] in {"complete", "partial"}
    finally:
        client.close()

    listed = Helper(project.root).dispatch({"method": "checkpoints.list", "page_size": 10})
    match = next(item for item in listed["items"] if item["checkpoint_id"] == checkpoint_id)
    assert match["status"] in {"complete", "partial"}
    assert match["session_id"]
    detail = Helper(project.root).dispatch(
        {"method": "checkpoint.get", "checkpoint_id": checkpoint_id}
    )
    assert detail["checkpoint_id"] == checkpoint_id
