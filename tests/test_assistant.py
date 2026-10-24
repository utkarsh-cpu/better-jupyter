"""Offline end-to-end assistant contract and boundary regressions."""

import concurrent.futures
import json
import os
import struct
import subprocess
import sys
import threading
import time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path

import jsonschema
import pytest

from kernelledger.assistant import API
from kernelledger.assistant.analysis import duration_statistics, stale_cells
from kernelledger.assistant.contracts import METHODS, schema
from kernelledger.assistant.mcp import CATALOG
from kernelledger.assistant.mcp import dispatch as mcp_dispatch
from kernelledger.assistant.providers import Provider, destination
from kernelledger.assistant.redact import redact
from kernelledger.assistant.service import Assistant
from kernelledger.assistant.settings import DEFAULTS, load, save
from kernelledger.assistant.tools import ALLOWLIST, RESPONSE_SCHEMA, ReadTools, check_schema
from kernelledger.blobs import BlobStore
from kernelledger.desktop.helper import Helper
from kernelledger.desktop.protocol import encode
from kernelledger.ledger import Ledger
from kernelledger.notebook import Notebook
from kernelledger.source import SourceError, SourceService


@pytest.fixture
def notebook(tmp_path):
    notebook = Notebook(tmp_path / "notebook")
    notebook.init()
    notebook.add("code", "x = 1\n", "first")
    notebook.add("code", "print(x)\n", "second")
    return notebook


@pytest.fixture
def assistant(notebook, tmp_path):
    path = tmp_path / "config/assistant.json"
    save({**DEFAULTS, "provider": "fake", "model": "fake"}, path)
    service = Assistant(notebook.root, settings_file=path)
    yield service
    service.close()


def respond(assistant, **fields):
    receipt = assistant.dispatch(
        {"method": "context.preview", "question": "Explain this notebook", **fields}
    )
    return assistant.dispatch({"method": "respond", "receipt_hash": receipt["hash"]})


def model_response(monkeypatch, value):
    def scripted(self, messages, emit, schema=RESPONSE_SCHEMA):
        raw = json.dumps(value)
        for i in range(0, len(raw), 3):
            emit(raw[i : i + 3])
        return raw

    monkeypatch.setattr(Provider, "respond", scripted)


def proposal(assistant, monkeypatch, kind="edit_cell"):
    snapshot = assistant.tools.read("source.read")
    payload = {
        "kind": "edit_cell",
        "cell_id": "first",
        "source_sha256": snapshot["cells"][0]["revision"],
        "source": "x = 2\n",
        "rationale": "Update the value",
    }
    if kind == "insert_cells":
        payload = {
            "kind": kind,
            "index": 1,
            "cells": [
                {"type": "code", "source": "y = 2\n"},
                {"type": "markdown", "source": "# Results"},
            ],
            "rationale": "Add a section",
        }
    model_response(
        monkeypatch,
        {
            "answer": "Review the suggested change.",
            "citations": [{"kind": "cell", "id": "first"}],
            "proposals": [payload],
        },
    )
    return respond(assistant)["proposals"][0]


def test_import_status_and_disabled_requests_create_no_assistant_records(notebook, tmp_path):
    service = Assistant(notebook.root, settings_file=tmp_path / "missing.json")
    assert service.dispatch({"method": "status"})["settings"]["provider"] is None
    with pytest.raises(SourceError, match="select a provider"):
        service.dispatch({"method": "context.preview", "question": "test"})
    assert not (notebook.root / ".kl/assistant").exists()
    assert not (notebook.root / ".kl/supervisor.json").exists()


def test_receipt_exact_messages_are_sent_and_records_are_separate(assistant, notebook, monkeypatch):
    before = {
        p.relative_to(notebook.root): p.read_bytes() for p in (notebook.root / "cells").iterdir()
    }
    receipt = assistant.dispatch(
        {"method": "context.preview", "question": "what changed?", "cell_ids": ["first"]}
    )
    assert [i["id"] for i in receipt["items"]] == ["first"]
    sent = []
    original = Provider.respond

    def capture(self, messages, emit, schema=RESPONSE_SCHEMA):
        sent.append(messages)
        return original(self, messages, emit, schema)

    monkeypatch.setattr(Provider, "respond", capture)
    result = assistant.dispatch({"method": "respond", "receipt_hash": receipt["hash"]})
    assert sent == [receipt["messages"]]
    assert result["receipt_hash"] == receipt["hash"]
    conversation = assistant.dispatch(
        {"method": "conversation.get", "conversation_id": result["conversation_id"]}
    )
    assert len(conversation["messages"]) == 2
    assert before == {
        p.relative_to(notebook.root): p.read_bytes() for p in (notebook.root / "cells").iterdir()
    }
    assert not (notebook.root / ".kl/ledger.sqlite").exists()


@pytest.mark.parametrize(
    "method",
    [
        "source.mutate",
        "source.recover",
        "interpreter.validate",
        "run",
        "batch",
        "restart",
        "restore",
        "gc",
        "export_document",
        "bundle_export",
        "publish",
        "stop",
        "import_ipynb",
        "../token",
    ],
)
def test_effectful_helper_methods_are_unreachable(notebook, method):
    assert method not in ALLOWLIST
    with pytest.raises(SourceError, match="read-only"):
        ReadTools(notebook.root).read(method, source="injected instructions")


def test_source_reads_never_acquire_lock_or_recover(assistant, monkeypatch):
    def forbidden(*args, **kwargs):
        pytest.fail("assistant attempted source recovery")

    monkeypatch.setattr(SourceService, "locked", forbidden)
    assistant.tools.read("source.read")
    assistant.tools.read("doctor")


def test_remote_receipt_required_and_one_use(assistant, monkeypatch):
    config = load(assistant.settings_file)
    config.update(provider="openai-compatible", model="test")
    save(config, assistant.settings_file)
    monkeypatch.setattr(
        "kernelledger.assistant.providers.destination",
        lambda *a: {
            "host": "remote.test",
            "local": False,
            "label": "Remote",
            "base_url": "https://remote.test/v1",
        },
    )
    receipt = assistant.dispatch({"method": "context.preview", "question": "test"})
    with pytest.raises(SourceError, match="approve this exact"):
        assistant.dispatch({"method": "respond", "receipt_hash": receipt["hash"]})
    with pytest.raises(SourceError, match="preview context again"):
        assistant.dispatch(
            {
                "method": "respond",
                "receipt_hash": receipt["hash"],
                "approved_receipt": receipt["hash"],
            }
        )
    receipt = assistant.dispatch({"method": "context.preview", "question": "test"})
    model_response(monkeypatch, {"answer": "test", "citations": [], "proposals": []})
    assert (
        assistant.dispatch(
            {
                "method": "respond",
                "receipt_hash": receipt["hash"],
                "approved_receipt": receipt["hash"],
            }
        )["answer"]
        == "test"
    )


def test_notebook_policy_blocks_remote_and_all_requests(assistant, notebook, monkeypatch):
    assistant.dispatch({"method": "policy.update", "policy": "off"})
    with pytest.raises(SourceError, match="select a provider"):
        respond(assistant)
    assistant.dispatch({"method": "policy.update", "policy": "local_only"})
    config = load(assistant.settings_file)
    config.update(provider="openai-compatible", model="test")
    save(config, assistant.settings_file)
    monkeypatch.setattr("kernelledger.assistant.providers.destination", lambda *a: {"local": False})
    with pytest.raises(SourceError, match="local providers only"):
        respond(assistant)


def test_changed_destination_or_model_invalidates_receipt(assistant):
    receipt = assistant.dispatch({"method": "context.preview", "question": "test"})
    config = load(assistant.settings_file)
    config["model"] = "other"
    save(config, assistant.settings_file)
    with pytest.raises(SourceError, match="changed"):
        assistant.dispatch({"method": "respond", "receipt_hash": receipt["hash"]})


def test_budget_refuses_before_provider_and_purge_is_removable(assistant):
    config = load(assistant.settings_file)
    config["daily_token_cap"] = 10
    save(config, assistant.settings_file)
    with pytest.raises(SourceError, match="budget"):
        respond(assistant)
    assert assistant.store.rows("SELECT * FROM requests") == []
    assistant.dispatch({"method": "purge"})
    assert list((assistant.root / ".kl/assistant").iterdir()) == []


def test_only_one_assistant_owns_store(assistant):
    _ = assistant.store
    other = Assistant(assistant.root, settings_file=assistant.settings_file)
    with pytest.raises(SourceError, match="owns this notebook"):
        _ = other.store


def test_stale_edit_is_refused_and_persisted(assistant, monkeypatch, notebook):
    item = proposal(assistant, monkeypatch)
    (notebook.root / "cells/first.py").write_text("x = 3\n")
    with pytest.raises(SourceError, match="changed"):
        assistant.dispatch(
            {"method": "proposal.resolve", "proposal_id": item["id"], "state": "accepted"}
        )
    assert assistant.dispatch({"method": "proposals.list"})["items"][0]["state"] == "stale"


@pytest.mark.parametrize("kind", ["edit_cell", "insert_cells"])
def test_reviewed_proposals_apply_through_helper_atomically(assistant, monkeypatch, kind):
    item = proposal(assistant, monkeypatch, kind)
    assert assistant.tools.read("source.read")["cells"][0]["source"] == "x = 1\n"
    resolution = assistant.dispatch(
        {"method": "proposal.resolve", "proposal_id": item["id"], "state": "accepted"}
    )
    snapshot = Helper(assistant.root).dispatch(
        {"method": "source.mutate", **resolution["mutation"]}
    )
    if kind == "edit_cell":
        assert snapshot["cells"][0]["source"] == "x = 2\n"
    else:
        assert [c["source"] for c in snapshot["cells"]] == [
            "x = 1\n",
            "y = 2\n",
            "# Results",
            "print(x)\n",
        ]
    assistant.dispatch(
        {
            "method": "application.record",
            "proposal_id": item["id"],
            "result": {"manifest_revision": snapshot["manifest_revision"]},
        }
    )
    assert len(assistant.store.rows("SELECT * FROM applications")) == 1


def test_source_changed_after_acceptance_is_still_refused_by_helper(
    assistant, monkeypatch, notebook
):
    item = proposal(assistant, monkeypatch)
    resolution = assistant.dispatch(
        {"method": "proposal.resolve", "proposal_id": item["id"], "state": "accepted"}
    )
    (notebook.root / "cells/first.py").write_text("x = 9")
    with pytest.raises(SourceError, match="changed"):
        Helper(assistant.root).dispatch({"method": "source.mutate", **resolution["mutation"]})


def test_invalid_or_effectful_model_response_never_becomes_proposal(assistant, monkeypatch):
    model_response(
        monkeypatch, {"answer": "I applied it", "citations": [], "proposals": [{"kind": "restart"}]}
    )
    with pytest.raises(SourceError, match="invalid response"):
        respond(assistant)
    assert assistant.store.rows("SELECT * FROM proposals") == []
    assert assistant.store.rows("SELECT status FROM requests")[0]["status"] == "error"


def test_citations_resolve_or_are_marked_unverified(assistant, monkeypatch):
    model_response(
        monkeypatch,
        {
            "answer": "A claim",
            "citations": [{"kind": "cell", "id": "first"}, {"kind": "execution", "id": "invented"}],
            "proposals": [],
        },
    )
    assert [c["verified"] for c in respond(assistant)["citations"]] == [True, False]


def test_credential_is_redacted_across_stream_fragments_and_records(assistant, monkeypatch):
    secret = "test-secret-crossing-stream-fragments"
    assistant.credential = secret
    model_response(monkeypatch, {"answer": "Echo " + secret, "citations": [], "proposals": []})
    receipt = assistant.dispatch({"method": "context.preview", "question": secret})
    frames = []
    assistant.dispatch({"method": "respond", "receipt_hash": receipt["hash"]}, frames.append)
    assert secret not in json.dumps(frames)
    assert secret not in json.dumps(assistant.store.rows("SELECT * FROM requests"))
    assert secret not in json.dumps(assistant.store.rows("SELECT * FROM messages"))
    assistant.close()
    assert secret.encode() not in (assistant.root / ".kl/assistant/assistant.sqlite").read_bytes()


@pytest.mark.parametrize(
    "value",
    [
        "sk-proj-abcdefghijklmnopqrstuvwxyz",
        "Bearer xyzabcdef",
        "postgres://bob:password@host/db",
        "password = 'hunter22'",
        "-----BEGIN PRIVATE KEY-----\nabc\n-----END PRIVATE KEY-----",
    ],
)
def test_common_credential_redaction(value):
    assert "[REDACTED]" in redact(value)


def test_context_budget_selection_and_denylist(assistant, notebook):
    (notebook.root / "cells/first.py").write_text("a" * 40000)
    receipt = assistant.dispatch(
        {"method": "context.preview", "question": "test", "cell_ids": ["first"]}
    )
    assert receipt["items"][0]["truncated"]
    assert receipt["items"][0]["bytes"] <= 32768
    assert len(receipt["items"]) == 1
    (notebook.root / ".kl/config.json").write_text(
        json.dumps({"checkpoint_denylist": ["sensitive"]})
    )
    (notebook.root / "cells/first.py").write_text("sensitive = 42")
    receipt = assistant.dispatch(
        {"method": "context.preview", "question": "test", "cell_ids": ["first"]}
    )
    assert receipt["items"][0]["redacted"]
    assert "42" not in json.loads(receipt["items"][0]["text"])["source"]


def test_deterministic_staleness_and_search(assistant, notebook):
    ledger = Ledger(notebook.root / ".kl/ledger.sqlite")
    blobs = BlobStore(notebook.root / ".kl")
    session = ledger.session(notebook.read()["id"], {})
    digest = blobs.put(b"x = 0\n")
    eid = ledger.execution(session, "first", digest)
    ledger.close()
    assert stale_cells(assistant.tools)["cells"][0]["state"] == "stale"
    result = assistant.dispatch({"method": "search", "query": "x"})
    assert result["items"][0]["identity"] == eid
    assert duration_statistics([{"duration": d} for d in [1, 1, 1, 20]])["flags"] == [20]


def test_mcp_has_only_read_tools_and_starts_no_assistant_records(notebook):
    tools = mcp_dispatch(notebook.root, {"method": "tools/list"})["tools"]
    assert len(tools) == len(CATALOG)
    assert all(t["annotations"]["readOnlyHint"] for t in tools)
    assert (
        "first"
        in mcp_dispatch(
            notebook.root,
            {"method": "tools/call", "params": {"name": "read_cells", "arguments": {}}},
        )["content"][0]["text"]
    )
    with pytest.raises(KeyError):
        mcp_dispatch(
            notebook.root, {"method": "tools/call", "params": {"name": "run", "arguments": {}}}
        )
    assert not (notebook.root / ".kl/assistant").exists()


def test_generated_schema_matches_catalog():
    assert json.loads(Path("schemas/desktop-assistant.schema.json").read_text()) == schema()
    for method, fields in METHODS.items():
        jsonschema.validate(
            {
                "method": method,
                **{key: {} if key in ("settings", "result") else "test" for key in fields},
            },
            schema(),
        )
    check_schema({"answer": "test", "citations": [], "proposals": []}, RESPONSE_SCHEMA)
    fixture = json.loads(Path("tests/fixtures/desktop/assistant/frames.json").read_text())
    assert fixture["api"] == API
    for frame in fixture["frames"]:
        assert frame["request_id"] == "request-1"
        assert len(encode(frame)) <= 1048580


@pytest.fixture
def server():
    class Handler(BaseHTTPRequestHandler):
        response = {"answer": "Connected", "citations": [], "proposals": []}
        requests = []
        mode = "normal"

        def log_message(self, *args):
            pass

        def do_GET(self):
            self.send_response(200)
            self.end_headers()
            self.wfile.write(b'{"data":[{"id":"test"}]}')

        def do_POST(self):
            body = json.loads(self.rfile.read(int(self.headers["Content-Length"])))
            self.requests.append((self.path, body))
            if self.mode == "error":
                self.send_response(503)
                self.end_headers()
                return
            self.send_response(200)
            self.send_header("Content-Type", "text/event-stream")
            self.end_headers()
            if self.mode == "malformed":
                self.wfile.write(b"data: broken\n\n")
                return
            if self.mode == "timeout":
                time.sleep(1.5)
                return
            text = json.dumps(self.response)
            try:
                for i in range(0, len(text), 5):
                    event = (
                        {"type": "response.output_text.delta", "delta": text[i : i + 5]}
                        if self.path.endswith("responses")
                        else {"choices": [{"delta": {"content": text[i : i + 5]}}]}
                    )
                    self.wfile.write(("data: " + json.dumps(event) + "\n\n").encode())
                    self.wfile.flush()
                    if self.mode == "slow":
                        time.sleep(0.03)
                if self.mode != "incomplete":
                    self.wfile.write(
                        (
                            "data: "
                            + json.dumps(
                                {
                                    "type": "response.completed",
                                    "response": {"usage": {"input_tokens": 3, "output_tokens": 4}},
                                }
                            )
                            + "\n\n"
                        ).encode()
                        if self.path.endswith("responses")
                        else b'data: {"choices":[],"usage":{"prompt_tokens":3,"completion_tokens":4}}\n\ndata: [DONE]\n\n'
                    )
            except (BrokenPipeError, ConnectionResetError, OSError):
                pass

    httpd = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
    worker = threading.Thread(target=httpd.serve_forever, daemon=True)
    worker.start()
    yield f"http://127.0.0.1:{httpd.server_port}/v1", Handler
    httpd.shutdown()
    httpd.server_close()
    worker.join(2)


@pytest.mark.parametrize("name", ["openai", "openai-compatible"])
def test_real_http_adapters_stream_and_report_usage(server, name):
    url, handler = server
    adapter = Provider(name, {"base_url": url}, "test")
    assert adapter.models() == [{"id": "test"}]
    chunks = []
    output = adapter.respond([{"role": "user", "content": "test"}], chunks.append)
    assert json.loads(output) == handler.response
    assert len(chunks) > 1
    assert adapter.usage
    body = handler.requests[0][1]
    if name == "openai":
        assert body["store"] is False
        assert body["text"]["format"]["schema"] == RESPONSE_SCHEMA


@pytest.mark.parametrize("mode", ["malformed", "error", "incomplete", "timeout"])
def test_bounded_provider_failures(server, mode):
    url, handler = server
    handler.mode = mode
    adapter = Provider("openai-compatible", {"base_url": url}, "test", timeout=1)
    with pytest.raises(SourceError):
        adapter.respond([], lambda text: None)
    assert len(handler.requests) == (2 if mode == "error" else 1)


def test_mid_stream_cancellation(server):
    url, handler = server
    handler.mode = "slow"
    adapter = Provider("openai-compatible", {"base_url": url}, "test")
    chunks = []
    with concurrent.futures.ThreadPoolExecutor() as pool:
        future = pool.submit(adapter.respond, [], chunks.append)
        deadline = time.monotonic() + 2
        while not chunks and time.monotonic() < deadline:
            time.sleep(0.01)
        adapter.cancel()
        with pytest.raises(SourceError, match="cancelled"):
            future.result(timeout=2)


def test_remote_plain_http_is_refused(monkeypatch):
    monkeypatch.setattr("socket.getaddrinfo", lambda *a, **k: [(2, 1, 6, "", ("192.0.2.1", 80))])
    with pytest.raises(SourceError, match="HTTPS"):
        destination("http://example.test/v1")
    assert destination("http://example.test/v1", "example.test")["label"] == "Remote"


def test_framed_process_and_cli(notebook, tmp_path):
    env = {
        **os.environ,
        "LOCALAPPDATA": str(tmp_path / "profile"),
        "XDG_CONFIG_HOME": str(tmp_path / "profile"),
    }
    process = subprocess.Popen(
        [sys.executable, "-I", "-m", "kernelledger.assistant", "--project", str(notebook.root)],
        stdin=subprocess.PIPE,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        env=env,
    )

    def call(value):
        process.stdin.write(encode(value))
        process.stdin.flush()
        size = struct.unpack(">I", process.stdout.read(4))[0]
        return json.loads(process.stdout.read(size))

    try:
        assert call({"kind": "hello", "api": API, "versions": [1]})["kind"] == "welcome"
        assert (
            call(
                {
                    "method": "settings.update",
                    "request_id": "1",
                    "settings": {"provider": "fake", "model": "fake"},
                }
            )["kind"]
            == "done"
        )
        receipt = call({"method": "context.preview", "request_id": "2", "question": "test"})[
            "result"
        ]
        process.stdin.write(
            encode({"method": "respond", "request_id": "3", "receipt_hash": receipt["hash"]})
        )
        process.stdin.flush()
        while True:
            size = struct.unpack(">I", process.stdout.read(4))[0]
            frame = json.loads(process.stdout.read(size))
            assert frame["request_id"] == "3"
            if frame["kind"] == "done":
                assert frame["result"]["answer"]
                break
    finally:
        process.stdin.close()
        process.wait(timeout=5)
    result = subprocess.run(
        [sys.executable, "-m", "kernelledger", "-C", str(notebook.root), "ai", "status"],
        capture_output=True,
        text=True,
        env=env,
        timeout=10,
    )
    assert result.returncode == 0, result.stderr
    assert json.loads(result.stdout)["settings"]["provider"] == "fake"


@pytest.mark.parametrize("format", ["pdf", "docx"])
def test_assistant_markdown_report_exports_via_helper(notebook, tmp_path, format):
    path = tmp_path / f"report.{format}"
    assert Helper(notebook.root).dispatch(
        {
            "method": "export_document",
            "format": format,
            "path": str(path),
            "markdown": "# Experiment report\nEvidence: cell first.",
        }
    )["path"] == str(path)
    assert path.stat().st_size > 100


def test_context_hydrates_bounded_text_outputs_and_omits_images(assistant, notebook):
    ledger = Ledger(notebook.root / ".kl/ledger.sqlite")
    blobs = BlobStore(notebook.root / ".kl")
    session = ledger.session(notebook.read()["id"], {})
    digest = blobs.put(b"x = 1\n")
    eid = ledger.execution(session, "first", digest)
    output = blobs.put(b"START" + b"x" * 40000 + b"END")
    ledger.event(eid, "stdout", {"blob": output})
    ledger.event(
        eid,
        "display",
        {
            "bundle": {
                "image/png": {"value": "IMAGE_MUST_NOT_BE_SENT"},
                "text/plain": {"value": "Plot summary"},
            }
        },
    )
    ledger.close()
    value = assistant.tools.read("fold.get", exec_id=eid)
    assert value["outputs"][0]["text"].startswith("START")
    assert value["outputs"][0]["text"].endswith("END")
    receipt = assistant.dispatch(
        {"method": "context.preview", "question": "Explain output", "exec_ids": [eid]}
    )
    assert "IMAGE_MUST_NOT_BE_SENT" not in json.dumps(receipt)
    assert "Plot summary" in json.dumps(receipt)


def test_followup_history_is_explicit_receipted_context(assistant):
    first = respond(assistant)
    receipt = assistant.dispatch(
        {
            "method": "context.preview",
            "question": "What did I just ask?",
            "conversation_id": first["conversation_id"],
        }
    )
    assert any(item["kind"] == "conversation" for item in receipt["items"])
    assert first["answer"] in json.dumps(receipt["messages"])


def test_history_index_first_page_at_one_thousand_executions(assistant, monkeypatch):
    rows = [{"exec_id": f"exec-{i}", "source": f"loss_history = {i}"} for i in range(1000)]
    original = assistant.tools.read

    def read(method, **fields):
        if method == "executions.list":
            offset = int(fields.get("cursor", "0"))
            return {
                "items": rows[offset : offset + 500],
                "cursor": str(offset + 500) if offset + 500 < len(rows) else None,
            }
        return original(method, **fields)

    monkeypatch.setattr(assistant.tools, "read", read)
    result = assistant.dispatch({"method": "search", "query": "loss_history"})
    assert len(result["items"]) == 50
    started = time.perf_counter()
    result = assistant.store.search("loss_history")
    assert len(result["items"]) == 50
    assert time.perf_counter() - started < 0.1


def test_purge_refuses_another_assistant_owner(assistant):
    _ = assistant.store
    other = Assistant(assistant.root, settings_file=assistant.settings_file)
    with pytest.raises(SourceError, match="owns this notebook"):
        other.dispatch({"method": "purge"})
    assert assistant.store.path.exists()


def test_new_tasks_task_models_approvals_and_status_summary(assistant):
    status = assistant.dispatch({"method": "settings.get"})
    assert "destination" in status and "capabilities" in status and "gates" in status
    assert "analysis" in status and "usage" in status
    assert not (assistant.root / ".kl/ledger.sqlite").exists()

    updated = assistant.dispatch(
        {
            "method": "settings.update",
            "settings": {
                "task_models": {"refactor": "custom-refactor-model"},
                "approvals": {"create_cell": True, "edit_cell": False},
            },
        }
    )
    assert updated["settings"]["task_models"]["refactor"] == "custom-refactor-model"
    assert assistant.is_auto_approved("create_cell") is True
    assert assistant.is_auto_approved("edit_cell") is False

    for task in ("refactor", "stale", "anomaly", "checkpoint", "explore"):
        receipt = assistant.dispatch(
            {
                "method": "context.preview",
                "task": task,
                "question": f"Test {task}",
                "cell_ids": ["first"],
            }
        )
        assert receipt["task"] == task
        if task == "refactor":
            assert receipt["model"] == "custom-refactor-model"
