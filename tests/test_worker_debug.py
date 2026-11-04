"""Real process pause/control and checkpoint exclusion."""

import time
import uuid

import pytest
from test_desktop import Client

from kernelledger.cli import ensure_supervisor, rpc
from kernelledger.notebook import source_hash
from kernelledger.source import SourceError

pytest_plugins = ["test_shell"]


def test_debug_pause_step_claim_and_interrupt(notebook, monkeypatch):
    monkeypatch.setenv("KL_EXPERIMENTAL_INTERACTIVITY", "1")
    ensure_supervisor(notebook.root)
    client = Client(notebook.root)
    try:
        source = "value = 40\nvalue += 2\nvalue"
        accepted = client.submit(
            "run", source=source, expected_sha256=source_hash(source), debug=True
        )

        def paused():
            deadline = time.monotonic() + 10
            while True:
                status = client.call("runtime.status")
                if status["worker_state"] == "paused":
                    return status
                assert time.monotonic() < deadline, status
                time.sleep(0.01)

        status = paused()
        generation = status["interactive"]["worker_generation"]
        assert status["debug"]["stack"][0]["line"] == 1
        with pytest.raises(SourceError) as error:
            client.submit("checkpoint")
        assert error.value.code == "busy"
        owner = str(uuid.uuid4())
        client.call(
            "debug.control", worker_generation=generation, command="next", controller_id=owner
        )
        status = paused()
        with pytest.raises(SourceError):
            # Worker errors are intentionally structured inside a correlated reply.
            reply = client.call(
                "debug.control",
                worker_generation=generation,
                command="continue",
                controller_id=str(uuid.uuid4()),
            )
            if reply.get("error"):
                raise SourceError(reply["error"]["code"], reply["error"]["message"])
        reply = client.call(
            "debug.control", worker_generation=generation, command="continue", controller_id=owner
        )
        assert not reply.get("error"), reply
        result = client.wait(accepted["operation_id"])
        assert result["result"]["status"] == "ok", result
    finally:
        client.close()


def test_breakpoints_bind_to_saved_source_revision(notebook, monkeypatch):
    monkeypatch.setenv("KL_EXPERIMENTAL_INTERACTIVITY", "1")
    source = "value = 40\nvalue += 2\nvalue\n"
    notebook.add("code", source, "stepper")
    ensure_supervisor(notebook.root)
    assert rpc(notebook.root, {"command": "run", "source": "1"})["status"] == "ok"
    client = Client(notebook.root)
    try:
        generation = client.call("runtime.status")["interactive"]["worker_generation"]
        configured = client.call(
            "debug.breakpoints",
            worker_generation=generation,
            breakpoints=[{"cell_id": "stepper", "source_revision": source_hash(source), "line": 3}],
        )
        assert not configured.get("error"), configured
        accepted = client.submit(
            "run", cell_id="stepper", expected_sha256=source_hash(source), debug=True
        )
        owner = str(uuid.uuid4())
        for target in (1, 3):
            deadline = time.monotonic() + 10
            while True:
                status = client.call("runtime.status")
                if (
                    status["worker_state"] == "paused"
                    and status["debug"]["stack"][0]["line"] == target
                ):
                    break
                assert time.monotonic() < deadline, status
                time.sleep(0.01)
            client.call(
                "debug.control",
                worker_generation=generation,
                controller_id=owner,
                command="continue",
            )
        assert client.wait(accepted["operation_id"])["result"]["status"] == "ok"
    finally:
        client.close()


def test_debug_post_mortem_and_prior_cell_breakpoint(notebook, monkeypatch):
    monkeypatch.setenv("KL_EXPERIMENTAL_INTERACTIVITY", "1")
    definition = "def fail():\n    local_value = 42\n    raise ValueError('post mortem')\n"
    notebook.add("code", definition, "definition")
    ensure_supervisor(notebook.root)
    assert rpc(notebook.root, {"command": "run", "cell_id": "definition"})["status"] == "ok"
    client = Client(notebook.root)
    try:
        generation = client.call("runtime.status")["interactive"]["worker_generation"]
        reply = client.call(
            "debug.breakpoints",
            worker_generation=generation,
            breakpoints=[
                {"cell_id": "definition", "source_revision": source_hash(definition), "line": 3}
            ],
        )
        assert not reply.get("error"), reply
        source = "fail()"
        accepted = client.submit(
            "run", source=source, expected_sha256=source_hash(source), debug=True
        )
        owner = str(uuid.uuid4())
        for phase in ("entry", "breakpoint", "failed"):
            deadline = time.monotonic() + 10
            while True:
                status = client.call("runtime.status")
                debug = status.get("debug") or {}
                stack = debug.get("stack") or []
                matches = (
                    (phase == "entry" and stack and stack[0]["function"] == "<module>")
                    or (
                        phase == "breakpoint"
                        and stack
                        and stack[0]["function"] == "fail"
                        and stack[0]["line"] == 3
                        and not debug.get("post_mortem")
                    )
                    or (phase == "failed" and debug.get("post_mortem"))
                )
                if matches:
                    break
                assert time.monotonic() < deadline, (phase, status)
                time.sleep(0.01)
            if phase != "entry":
                assert debug["source_revision"] == source_hash(definition)
            if phase == "failed":
                assert sum(line["current"] for line in debug["source_context"]) == 1
                assert len(debug["source_context"]) <= 7
                assert any(
                    item["name"] == "local_value" and item["preview"] == "42"
                    for item in debug["locals"]
                )
                refused = client.call(
                    "debug.control",
                    worker_generation=generation,
                    controller_id=owner,
                    command="next",
                )
                assert refused["error"]["code"] == "post_mortem"
            client.call(
                "debug.control",
                worker_generation=generation,
                controller_id=owner,
                command="continue",
            )
        assert client.wait(accepted["operation_id"])["result"]["status"] == "error"
        assert client.call("runtime.status").get("debug") is None
    finally:
        client.close()
