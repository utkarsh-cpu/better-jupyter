"""Regressions for real store/dispatch boundaries and HTTP failure recovery."""

import json
import threading
import time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

import pytest
from test_assistant import assistant as assistant
from test_assistant import model_response, proposal, respond
from test_assistant import notebook as notebook

from kernelledger.assistant.profile import inference_profile, profile_key
from kernelledger.assistant.providers import Provider
from kernelledger.assistant.service import Assistant
from kernelledger.assistant.settings import load, save
from kernelledger.desktop.helper import Helper
from kernelledger.source import SourceError


def test_probe_cache_roundtrip_and_usage_fail_independently(assistant):
    before = assistant.dispatch({"method": "status"})
    assert before["capabilities"]["structured_output"] is None
    result = assistant.dispatch({"method": "providers.test"})
    assert result["task_qualified"] is False
    config = load(assistant.settings_file)
    key = profile_key(inference_profile(config))
    with assistant.store.db:
        assistant.store.db.execute(
            "UPDATE capabilities SET result=? WHERE key=?",
            (json.dumps({"structured_output": False, "streaming": True}), key),
        )
    used = assistant.store.used(config["provider"])
    assert used > 0
    state = assistant.dispatch({"method": "status"})
    assert state["capabilities"]["structured_output"] is False
    assert state["usage"]["used_today"] == used
    with assistant.store.db:
        assistant.store.db.execute("UPDATE capabilities SET result='invalid json'")
    state = assistant.dispatch({"method": "status"})
    assert state["capabilities"]["structured_output"] is None
    assert state["usage"]["used_today"] == used


def test_inference_settings_change_invalidates_approved_receipt(assistant):
    receipt = assistant.dispatch(
        {"method": "context.preview", "question": "Explain", "task": "refactor"}
    )
    config = load(assistant.settings_file)
    config["max_output_tokens"] += 100
    save(config, assistant.settings_file)
    with pytest.raises(SourceError, match="inference settings changed"):
        assistant.dispatch({"method": "respond", "receipt_hash": receipt["hash"]})


def test_partial_answer_keeps_valid_proposal_and_rejects_invented_hash(assistant, monkeypatch):
    cell = assistant.tools.read("source.read")["cells"][0]
    valid = {
        "kind": "edit_cell",
        "cell_id": cell["id"],
        "source_sha256": cell["revision"],
        "source": "x = 2\n",
        "rationale": "Example",
    }
    model_response(
        monkeypatch,
        {
            "answer": "Useful explanation",
            "citations": [{"kind": "cell", "id": cell["id"]}],
            "proposals": [valid, {**valid, "source_sha256": "invented"}, {"kind": "restart"}],
        },
    )
    result = respond(assistant)
    assert result["answer"] == "Useful explanation"
    assert len(result["proposals"]) == 1
    assert [w["code"] for w in result["warnings"]] == ["invalid_base_hash", "invalid_proposal"]
    assert len(assistant.store.rows("SELECT * FROM messages WHERE role='assistant'")) == 1
    assert (
        json.loads(assistant.store.rows("SELECT warnings FROM requests")[0]["warnings"])
        == result["warnings"]
    )
    assert assistant.tools.read("source.read")["cells"][0]["revision"] == cell["revision"]
    history = assistant.dispatch(
        {"method": "conversation.get", "conversation_id": result["conversation_id"]}
    )
    assert history["requests"][-1]["warnings"] == result["warnings"]


@pytest.mark.parametrize(
    "value",
    [
        {"answer": 17, "citations": [], "proposals": []},
        {"answer": "x", "citations": [], "proposals": {}, "unknown": True},
    ],
)
def test_invalid_envelope_still_fails_closed(assistant, monkeypatch, value):
    model_response(monkeypatch, value)
    with pytest.raises(SourceError, match="invalid response"):
        respond(assistant)
    assert assistant.store.rows("SELECT * FROM proposals") == []


def test_context_accounts_for_omitted_and_truncated_records(assistant, notebook):
    (notebook.root / "cells/first.py").write_text("# " + "a" * 4096)
    config = load(assistant.settings_file)
    config["context_bytes"] = 1024
    save(config, assistant.settings_file)
    receipt = assistant.dispatch({"method": "context.preview", "question": "Explain"})
    assert receipt["selection"]["included"] == len(receipt["items"])
    assert receipt["selection"]["truncated"] > 0
    assert receipt["omitted"] and receipt["selection"]["omitted"] == len(receipt["omitted"])


def test_known_context_limit_refuses_before_inference(assistant):
    config = load(assistant.settings_file)
    config["providers"]["fake"] = {"context_window": 256}
    save(config, assistant.settings_file)
    with pytest.raises(SourceError, match="context window"):
        assistant.dispatch({"method": "context.preview", "question": "Explain"})


def test_unrelated_existing_citation_is_not_approved_context(assistant, monkeypatch):
    cells = assistant.tools.read("source.read")["cells"]
    model_response(
        monkeypatch,
        {
            "answer": "Unverified claim",
            "citations": [{"kind": "cell", "id": cells[1]["id"]}],
            "proposals": [],
        },
    )
    receipt = assistant.dispatch(
        {"method": "context.preview", "question": "Explain", "cell_ids": [cells[0]["id"]]}
    )
    citation = assistant.dispatch({"method": "respond", "receipt_hash": receipt["hash"]})[
        "citations"
    ][0]
    assert citation["verified"] is True and citation["in_context"] is False


def test_probe_cannot_bypass_token_cap(assistant, monkeypatch):
    config = load(assistant.settings_file)
    config["daily_token_cap"] = 1
    save(config, assistant.settings_file)
    monkeypatch.setattr(
        Provider, "respond", lambda *args: pytest.fail("Inference was not admitted")
    )
    with pytest.raises(SourceError, match="budget"):
        assistant.dispatch({"method": "providers.test"})
    assert assistant.store.rows("SELECT * FROM requests") == []


def test_application_record_is_idempotent_and_conversation_rehydrates(assistant, monkeypatch):
    item = proposal(assistant, monkeypatch)
    resolution = assistant.dispatch(
        {"method": "proposal.resolve", "proposal_id": item["id"], "state": "accepted"}
    )
    assert resolution["application_id"]
    snapshot = Helper(assistant.root).dispatch(
        {"method": "source.mutate", **resolution["mutation"]}
    )
    request = {
        "method": "application.record",
        "proposal_id": item["id"],
        "result": {"manifest_revision": snapshot["manifest_revision"]},
    }
    assert assistant.dispatch(request)["state"] == "applied"
    assert assistant.dispatch(request)["duplicate"] is True
    assert len(assistant.store.rows("SELECT * FROM applications")) == 1
    cid = assistant.store.rows("SELECT id FROM conversations")[0]["id"]
    history = assistant.dispatch({"method": "conversation.get", "conversation_id": cid})
    assert isinstance(history["messages"][-1]["citations"], list)
    assert history["proposals"][0]["state"] == "applied"


def test_interrupted_application_is_unknown_and_cannot_reapply(assistant, monkeypatch):
    item = proposal(assistant, monkeypatch)
    assistant.dispatch(
        {"method": "proposal.resolve", "proposal_id": item["id"], "state": "accepted"}
    )
    assistant.close()
    restored = Assistant(assistant.root, settings_file=assistant.settings_file)
    try:
        assert restored.dispatch({"method": "proposals.list"})["items"][0]["state"] == "unknown"
        with pytest.raises(SourceError, match="already resolved"):
            restored.dispatch(
                {"method": "proposal.resolve", "proposal_id": item["id"], "state": "accepted"}
            )
    finally:
        restored.close()


def test_only_scoped_normalized_destination_grants_apply(assistant):
    config = load(assistant.settings_file)
    dest = {"base_url": "https://example.com/v1", "host": "example.com"}
    config["approvals"] = {"example.com": True, f"{assistant.root}::example.com": True}
    assert not assistant.is_auto_approved(config, dest)
    config["approvals"][f"{assistant.root}::https://EXAMPLE.com:443/v1/"] = True
    assert assistant.is_auto_approved(config, dest)
    config["approvals"].clear()
    assert not assistant.is_auto_approved(config, dest)


def test_revoke_removes_equivalent_scoped_urls(assistant):
    config = load(assistant.settings_file)
    config["approvals"] = {
        f"{assistant.root}::http://127.0.0.1/fake": True,
        f"{assistant.root}::http://127.0.0.1:80/fake/": True,
        "127.0.0.1": True,
    }
    save(config, assistant.settings_file)
    assistant.dispatch({"method": "approval.revoke"})
    config = load(assistant.settings_file)
    assert len(config["approvals"]) == 1  # Ignored legacy grant is not silently widened.
    assert not assistant.is_auto_approved(config, {"base_url": "http://127.0.0.1/fake"})


@pytest.fixture
def wire_server():
    state = {"mode": "length", "requests": []}

    class Handler(BaseHTTPRequestHandler):
        def log_message(self, *args):
            pass

        def do_POST(self):
            state["requests"].append(
                json.loads(self.rfile.read(int(self.headers["Content-Length"])))
            )
            self.send_response(200)
            self.send_header("Content-Type", "text/event-stream")
            self.end_headers()
            try:
                if state["mode"] == "trickle":
                    for _ in range(200):
                        self.wfile.write(b" ")
                        self.wfile.flush()
                        time.sleep(0.03)
                else:
                    for event in [
                        {
                            "choices": [
                                {
                                    "delta": {"reasoning_content": "diagnostic"},
                                    "finish_reason": None,
                                }
                            ]
                        },
                        {"choices": [{"delta": {}, "finish_reason": state["mode"]}]},
                        {
                            "choices": [],
                            "usage": {"completion_tokens": 32, **state.get("usage_extra", {})},
                        },
                    ]:
                        self.wfile.write(("data: " + json.dumps(event) + "\n\n").encode())
                    self.wfile.write(b"data: [DONE]\n\n")
                    self.wfile.flush()
            except (OSError, ValueError):
                pass

    server = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()
    yield f"http://127.0.0.1:{server.server_port}/v1", state
    server.shutdown()
    server.server_close()
    thread.join(timeout=2)


@pytest.mark.parametrize(
    "reason,code",
    [
        ("length", "output_limit"),
        ("content_filter", "refused"),
        ("tool_calls", "unsupported_capability"),
    ],
)
def test_finish_reasons_and_reasoning_metadata_are_distinct(wire_server, reason, code):
    url, state = wire_server
    state["mode"] = reason
    provider = Provider(
        "openai-compatible",
        {"base_url": url, "structured_output": True, "enable_thinking": False},
        "fixture",
        max_output=32,
    )
    with pytest.raises(SourceError) as error:
        provider.respond([], lambda _: pytest.fail("Raw reasoning leaked"))
    assert error.value.code == code
    assert provider.usage["completion_tokens"] == 32
    assert provider.usage["finish_reason"] == reason
    assert "diagnostic" not in json.dumps(provider.usage)
    assert state["requests"][0]["enable_thinking"] is False
    assert state["requests"][0]["response_format"]["type"] == "json_schema"


def test_total_deadline_interrupts_a_trickling_unterminated_line(wire_server):
    url, state = wire_server
    state["mode"] = "trickle"
    provider = Provider("openai-compatible", {"base_url": url}, "fixture", timeout=1)
    started = time.monotonic()
    with pytest.raises(SourceError) as error:
        provider.respond([], lambda _: None)
    assert error.value.code == "timeout"
    assert time.monotonic() - started < 2.5


def test_dns_resolution_has_a_bounded_wait(monkeypatch):
    from kernelledger.assistant.providers import destination

    def delayed(*args, **kwargs):
        time.sleep(0.3)
        return []

    monkeypatch.setattr("socket.getaddrinfo", delayed)
    started = time.monotonic()
    with pytest.raises(SourceError) as error:
        destination("https://slow.invalid/v1", timeout=0.05)
    assert error.value.code == "timeout"
    assert time.monotonic() - started < 0.25


def test_provider_usage_cannot_persist_arbitrary_secret_fields(wire_server):
    url, state = wire_server
    state["usage_extra"] = {
        "total_tokens": "sk-usage-secret-fixture",
        "prompt_tokens": 123,
        "headers": {"Authorization": "sk-usage-secret-fixture"},
        "prompt_tokens_details": {"cached_tokens": 4, "api_key": "sk-usage-secret-fixture"},
    }
    provider = Provider("openai-compatible", {"base_url": url}, "fixture", max_output=32)
    with pytest.raises(SourceError):
        provider.respond([], lambda _: None)
    assert provider.usage["prompt_tokens"] == 123
    assert provider.usage["prompt_tokens_details"] == {"cached_tokens": 4}
    assert "sk-usage-secret-fixture" not in json.dumps(provider.usage)


def test_autoapproved_receipt_hash_is_exact_and_revocation_consumes_it(assistant):
    import hashlib

    from kernelledger.assistant.context import packed

    config = load(assistant.settings_file)
    url = "http://127.0.0.1:12345/v1"
    config.update(
        provider="openai-compatible",
        model="fixture",
        providers={"openai-compatible": {"base_url": url}},
        approvals={f"{assistant.root}::{url}": True},
    )
    save(config, assistant.settings_file)
    receipt = assistant.dispatch({"method": "context.preview", "question": "Review only"})
    assert receipt["auto_approved"] is True
    assert (
        receipt["hash"]
        == hashlib.sha256(
            packed({k: v for k, v in receipt.items() if k != "hash"}).encode()
        ).hexdigest()
    )
    assistant.dispatch({"method": "approval.revoke"})
    with pytest.raises(SourceError, match="preview context again"):
        assistant.dispatch({"method": "respond", "receipt_hash": receipt["hash"]})


def test_legacy_application_record_is_reconciled_without_repeating_effect(assistant, monkeypatch):
    item = proposal(assistant, monkeypatch)
    resolution = assistant.dispatch(
        {"method": "proposal.resolve", "proposal_id": item["id"], "state": "accepted"}
    )
    snapshot = Helper(assistant.root).dispatch(
        {"method": "source.mutate", **resolution["mutation"]}
    )
    assistant.dispatch(
        {
            "method": "application.record",
            "proposal_id": item["id"],
            "result": {"manifest_revision": snapshot["manifest_revision"]},
        }
    )
    with assistant.store.db:
        assistant.store.db.execute("UPDATE proposals SET state='accepted'")
        assistant.store.db.execute("DELETE FROM application_intents")
    assistant.close()
    restored = Assistant(assistant.root, settings_file=assistant.settings_file)
    try:
        assert restored.dispatch({"method": "proposals.list"})["items"][0]["state"] == "applied"
        assert len(restored.store.rows("SELECT * FROM applications")) == 1
    finally:
        restored.close()
