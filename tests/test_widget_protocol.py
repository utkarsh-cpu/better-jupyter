"""Native widget subset: accepted callbacks, retries, snapshots and stale handles."""

import uuid

import pytest
from test_desktop import Client
from test_shell import execute, successful

from kernelledger.cli import ensure_supervisor
from kernelledger.notebook import source_hash

pytest_plugins = ["test_shell"]


def test_widget_callbacks_are_durable_and_retry_deduplicated(notebook, monkeypatch):
    monkeypatch.setenv("KL_EXPERIMENTAL_INTERACTIVITY", "1")
    ensure_supervisor(notebook.root)
    events = successful(
        notebook,
        "clicks = []\ncontrol = widget('button', label='Count', callback=lambda value: clicks.append(1))\ncontrol",
    )
    model = next(
        event["bundle"]["application/vnd.kernelledger.widget+json"]["value"]
        for event in events
        if event.get("bundle", {}).get("application/vnd.kernelledger.widget+json")
    )
    source = f"__kl_widget_event__({model['worker_generation']!r}, {model['handle']!r}, None)"
    client = Client(notebook.root)
    try:
        key = str(uuid.uuid4())
        request = dict(
            source=source,
            expected_sha256=source_hash(source),
            origin="widget",
            client_request_id=key,
        )
        first = client.call("run", **request)
        second = client.call("run", **request)
        assert first["operation_id"] == second["operation_id"]
        assert client.wait(first["operation_id"])["result"]["status"] == "ok"
        successful(notebook, "assert clicks == [1]")
        successful(notebook, "control.close()")
        result, _ = execute(notebook, source)
        assert result["status"] == "error"
    finally:
        client.close()


@pytest.mark.parametrize(
    "source",
    [
        "widget('integer_slider', 0.5)",
        "widget('integer_slider', float('nan'))",
        "widget('checkbox', 1)",
        "widget('dropdown', 'missing', options=['yes'])",
        "widget('button', label='x' * 2000)",
    ],
)
def test_widget_subset_rejects_invalid_and_unqualified_values(notebook, source, monkeypatch):
    monkeypatch.setenv("KL_EXPERIMENTAL_INTERACTIVITY", "1")
    ensure_supervisor(notebook.root)
    assert execute(notebook, source)[0]["status"] == "error"
