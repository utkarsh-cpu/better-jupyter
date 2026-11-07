"""Assistant method dispatch. Never imports an effectful client or writes source/ledger."""

import hashlib
import json
import sqlite3
import threading
import time
import uuid
from collections.abc import Callable
from pathlib import Path
from typing import Any, cast
from urllib.parse import urlsplit

from ..source import SourceError
from . import capabilities, settings
from .analysis import duration_statistics, reproducibility, stale_cells
from .context import packed, preview
from .profile import inference_profile, normalized_endpoint, profile_key
from .providers import Provider
from .redact import clean
from .store import Store
from .tools import CITATION_SCHEMA, RESPONSE_SCHEMA, ReadTools, check_schema, validate_proposal


class Assistant:
    def __init__(self, root: Path, credential: str = "", settings_file: Path | None = None):
        self.root = root.resolve()
        self.credential = credential
        self.settings_file = settings_file
        self.tools = ReadTools(root)
        self._store: Store | None = None
        self.provider: Provider | None = None
        self.cancelled = threading.Event()
        self.receipts: dict[str, Any] = {}
        self.index_signature: str | None = None

    @property
    def store(self) -> Store:
        if self._store is None:
            self._store = Store(self.root, settings.load(self.settings_file)["retention_days"])
        return self._store

    def close(self) -> None:
        if self._store:
            self._store.close()
            self._store = None

    def cancel(self) -> None:
        self.cancelled.set()
        if self.provider:
            self.provider.cancel()

    def configured(self, task: str = "ask") -> tuple[Any, Any, Any]:
        config = settings.load(self.settings_file)
        policy, notebook_config = settings.policy(self.root)
        active_model = config.get("task_models", {}).get(task) or config["model"]
        if policy == "off" or not config["provider"] or not active_model:
            raise SourceError(
                "disabled",
                "select a provider and model in Settings \u2192 AI; notebook policy may disable AI",
            )
        provider = Provider(
            config["provider"],
            config["providers"].get(config["provider"], {}),
            active_model,
            self.credential,
            config["timeout"],
            config["max_output_tokens"],
            self.cancelled,
        )
        if policy == "local_only" and not provider.destination["local"]:
            raise SourceError("policy", "this notebook allows local providers only")
        self.provider = provider
        return config, notebook_config, provider

    def is_auto_approved(
        self, config: dict[str, Any] | str, dest: dict[str, Any] | None = None
    ) -> bool:
        if isinstance(config, str):
            cfg = settings.load(self.settings_file)
            config = cfg
        destination: dict[str, Any] = (
            dest if dest is not None else self.status_summary(config)["destination"]
        )
        approvals: dict[str, Any] = config.get("approvals") or {}
        prefix = f"{self.root}::"
        endpoint = normalized_endpoint(destination.get("base_url", ""))
        return any(
            key.startswith(prefix)
            and key[len(prefix) :].lower().startswith(("http://", "https://"))
            and normalized_endpoint(key[len(prefix) :]) == endpoint
            and value is True
            for key, value in approvals.items()
        )

    def status_summary(self, config: dict[str, Any]) -> dict[str, Any]:
        prov_name = config.get("provider")
        prov_cfg = config.get("providers", {}).get(prov_name or "", {})
        dest_info: dict[str, Any] = {
            "host": "unconfigured",
            "local": True,
            "label": "Local",
            "base_url": "",
        }
        if prov_name == "fake":
            dest_info = {
                "host": "localhost",
                "local": True,
                "label": "Local",
                "base_url": "http://127.0.0.1/fake",
            }
        elif prov_name in ("openai", "openai-compatible"):
            base: str = prov_cfg.get(
                "base_url",
                "https://api.openai.com/v1"
                if prov_name == "openai"
                else "http://127.0.0.1:11434/v1",
            ).rstrip("/")
            host = urlsplit(base).hostname or ""
            is_local = host in ("localhost", "127.0.0.1", "::1")
            dest_info = {
                "host": host or base,
                "local": is_local,
                "label": "Local" if is_local else "Remote",
                "base_url": base,
            }
        used_today = 0
        cap_info: dict[str, Any] = {
            "streaming": None,
            "structured_output": None,
            "tool_calls": False,
            "vision": False,
            "fill_in_the_middle": False,
            "context_window": None,
            "reason": "Unknown: test this endpoint/model/profile to observe SSE and JSON connectivity.",
        }
        if prov_name and (self.root / ".kl/assistant/assistant.sqlite").exists():
            try:
                used_today = self.store.used(prov_name)
            except Exception:
                used_today = None
            try:
                rows = self.store.rows(
                    "SELECT result,created FROM capabilities WHERE key=?",
                    (profile_key(inference_profile(config)),),
                )
                if rows and time.time() - rows[0]["created"] < 86400:
                    cached = json.loads(rows[0]["result"])
                    if not isinstance(cached, dict):
                        raise ValueError("invalid capability cache shape")
                    cap_info = cast(dict[str, Any], cached)
            except (ValueError, OSError, sqlite3.Error, SourceError):
                cap_info["reason"] = "Capability cache unavailable; connectivity is unknown."
        analysis_info: dict[str, Any] = {}
        if (self.root / "notebook.json").exists() and (self.root / ".kl/ledger.sqlite").exists():
            try:
                recent = self.tools.read("executions.list", page_size=100)
                analysis_info = {
                    "stale_cells": stale_cells(self.tools),
                    "reproducibility": reproducibility(self.tools),
                    "duration_statistics": duration_statistics(recent.get("items", [])),
                }
            except Exception:
                analysis_info = {}
        return {
            "settings": config,
            "policy": settings.policy(self.root)[0],
            "notebook_root": str(self.root),
            "destination": dest_info,
            "capabilities": cap_info,
            "profile": inference_profile(config),
            "task_profiles": {
                task: inference_profile(config, task) for task in config.get("task_models", {})
            },
            "usage": {
                "used_today": used_today,
                "daily_token_cap": config["daily_token_cap"],
                "max_output_tokens": config["max_output_tokens"],
                "context_bytes": config["context_bytes"],
                "scope": "This notebook/provider, UTC day; reservations include failed requests and connection tests. Not a billing total.",
            },
            "analysis": analysis_info,
            "providers": ["openai-compatible", "openai"],
            "gates": {
                "chatgpt-plan": "client registration and desktop release required",
                "inline": "local latency qualification required",
                "agent": "effect and budget qualification required",
                "vision": "image input qualification required",
            },
        }

    def dispatch(self, request: Any, emit: Callable[[Any], None] = lambda _value: None) -> Any:
        from .contracts import validate

        validate(request)
        request = cast(dict[str, Any], request)
        method = request["method"]
        if method == "cancel":
            self.cancel()
            return {"cancelled": True}
        if method in ("status", "providers.list", "settings.get"):
            config = settings.load(self.settings_file)
            return self.status_summary(config)
        if method == "settings.update":
            config = {**settings.load(self.settings_file), **request["settings"]}
            settings.save(config, self.settings_file)
            self.receipts.clear()
            return self.status_summary(config)
        if method == "approval.revoke":
            config = settings.load(self.settings_file)
            destination = self.status_summary(config)["destination"]
            endpoint = normalized_endpoint(destination["base_url"])
            prefix = f"{self.root}::"
            for key in list(config["approvals"]):
                if (
                    key.startswith(prefix)
                    and key[len(prefix) :].lower().startswith(("http://", "https://"))
                    and normalized_endpoint(key[len(prefix) :]) == endpoint
                ):
                    del config["approvals"][key]
            settings.save(config, self.settings_file)
            self.receipts.clear()
            return self.status_summary(config)
        if method == "policy.update":
            from ..source import atomic

            _policy, config = settings.policy(self.root)
            value = request["policy"]
            if value not in ("off", "local_only", "allowed"):
                raise SourceError("invalid_request", "invalid notebook policy")
            config["assistant"] = value
            atomic(self.root / ".kl/config.json", json.dumps(config, indent=2).encode())
            self.receipts.clear()
            return {"policy": value}
        if method == "purge":
            if (self.root / ".kl/assistant/assistant.sqlite").exists():
                _ = self.store  # Acquire ownership before removing another process's files.
            self.close()
            directory = self.root / ".kl/assistant"
            # Only known assistant files, with no recursive deletion or symlink traversal.
            for name in (
                "assistant.sqlite",
                "assistant.sqlite-wal",
                "assistant.sqlite-shm",
                "assistant.sqlite-journal",
                "lock",
            ):
                (directory / name).unlink(missing_ok=True)
            self.receipts.clear()
            return {"purged": True}
        if method == "prune":
            config = settings.load(self.settings_file)
            with self.store.db:
                deleted = self.store.db.execute(
                    "DELETE FROM conversations WHERE pinned=0 AND created<?",
                    (time.time() - config["retention_days"] * 86400,),
                ).rowcount
            return {"pruned": deleted}
        if method == "conversations.list":
            return {
                "items": self.store.rows(
                    "SELECT c.* FROM conversations c WHERE EXISTS(SELECT 1 FROM requests r WHERE r.conversation=c.id AND r.task != 'probe') ORDER BY c.created DESC LIMIT 200"
                )
            }
        if method == "conversation.get":
            return self.store.conversation(request["conversation_id"])
        if method == "proposals.list":
            rows = self.store.rows(
                "SELECT p.*,i.state AS application_state FROM proposals p LEFT JOIN application_intents i ON p.id=i.proposal_id ORDER BY p.rowid DESC LIMIT 200"
            )
            for row in rows:
                row["payload"] = json.loads(row["payload"])
                if row["application_state"] == "unknown":
                    row["state"] = "unknown"
            return {"items": rows}
        if method in ("proposal.resolve", "application.record"):
            return self.resolve(request)
        if method == "search":
            self.index()
            return self.store.search(request["query"])
        task = request.get("task", "ask")
        config, notebook_config, provider = self.configured(task)
        if method == "models.list":
            return {"items": provider.models(), "destination": provider.destination}
        if method == "providers.test":
            profile = inference_profile(config, task)
            # Probes are inference too; reserve the bounded prompt and output before sending.
            receipt: dict[str, Any] = {
                "question": "Connection test (no notebook context)",
                "token_estimate": 4096,
                "provider": config["provider"],
                "model": provider.model,
                "destination": provider.destination,
                "prompt_version": "connectivity-2",
                "hash": profile_key(profile),
                "manifest_revision": "",
                "profile": profile,
            }
            self.admit(config, receipt["token_estimate"])
            rid = self.store.begin(
                {"task": "probe"}, receipt, str(self.root), config["max_output_tokens"]
            )
            try:
                result = capabilities.probe(provider, self.store, profile)
                with self.store.db:
                    self.store.db.execute(
                        "UPDATE requests SET status='done',usage=?,finished=? WHERE id=?",
                        (json.dumps(provider.usage), time.time(), rid),
                    )
                return result
            except Exception as error:
                self.store.fail(rid, getattr(error, "code", "provider_error"), provider.usage)
                raise
        if method == "context.preview":
            request = dict(request)
            # Conversation content comes from our store, never arbitrary client-supplied history.
            request.pop("conversation_messages", None)
            if request.get("conversation_id"):
                request["conversation_messages"] = self.store.conversation(
                    request["conversation_id"]
                )["messages"][-12:]
            receipt = preview(
                self.tools,
                request,
                config,
                provider.destination,
                notebook_config.get("checkpoint_denylist", []),
                (self.credential,),
            )
            if self.is_auto_approved(config, provider.destination):
                receipt["auto_approved"] = True
                receipt["hash"] = hashlib.sha256(
                    packed({k: v for k, v in receipt.items() if k != "hash"}).encode()
                ).hexdigest()
            # Prepared context is held in memory. Respond cannot substitute arbitrary receipt data.
            self.receipts = {
                receipt["hash"]: {
                    "receipt": receipt,
                    "request": dict(request),
                    "created": time.monotonic(),
                }
            }
            return receipt
        if method == "respond":
            prepared = self.receipts.pop(request.get("receipt_hash", ""), None)
            if not prepared or time.monotonic() - prepared["created"] > 600:
                raise SourceError("receipt_required", "preview context again before sending")
            receipt = prepared["receipt"]
            req_task = prepared["request"].get("task", "ask")
            config, notebook_config, provider = self.configured(req_task)
            expected_model = config.get("task_models", {}).get(req_task) or config["model"]
            if (
                receipt["destination"] != provider.destination
                or receipt["model"] != expected_model
                or receipt["provider"] != config["provider"]
                or receipt["profile"] != inference_profile(config, req_task)
            ):
                raise SourceError(
                    "receipt_changed",
                    "provider destination, model or inference settings changed; preview again",
                )
            if (
                not provider.destination["local"]
                and request.get("approved_receipt") != receipt["hash"]
                and not self.is_auto_approved(config, provider.destination)
            ):
                raise SourceError(
                    "approval_required",
                    "approve this exact context receipt before a remote request",
                )
            self.admit(config, receipt["token_estimate"])
            snapshot = self.tools.read("source.read")
            rid = self.store.begin(
                prepared["request"],
                receipt,
                snapshot["manifest"]["id"],
                config["max_output_tokens"],
            )
            try:
                # Never forward arbitrary provider fragments: credentials can cross SSE boundaries.
                # Emit only complete decoded answer strings, redacted across all received chunks.
                streamed: list[str] = []
                streamed_answer = ""

                def delta(text: str) -> None:
                    nonlocal streamed_answer
                    streamed.append(text)
                    raw = "".join(streamed)
                    import re

                    match = re.search(r'"answer"\s*:\s*', raw)
                    if match:
                        try:
                            answer, _ = json.JSONDecoder().raw_decode(raw[match.end() :])
                        except ValueError:
                            return
                        if isinstance(answer, str) and not streamed_answer:
                            streamed_answer = clean(
                                answer,
                                notebook_config.get("checkpoint_denylist", []),
                                (self.credential,),
                            )
                            emit({"kind": "delta", "text": streamed_answer, "unverified": True})

                output = provider.respond(
                    receipt["messages"],
                    delta,
                )
                response = clean(
                    json.loads(output),
                    notebook_config.get("checkpoint_denylist", []),
                    (self.credential,),
                )
                # Preserve a safe explanation when optional generated proposals are bad.
                envelope: Any = {
                    **RESPONSE_SCHEMA,
                    "properties": {
                        **RESPONSE_SCHEMA["properties"],
                        "proposals": {
                            "type": "array",
                            "items": {"type": "object", "properties": {}},
                        },
                    },
                }
                check_schema({**response, "proposals": []}, envelope)
                if not isinstance(response["proposals"], list) or len(response["proposals"]) > 100:
                    raise ValueError("invalid proposal list")
                snapshot = self.tools.read("source.read")
                verified: list[Any] = []
                for citation in response["citations"]:
                    check_schema(citation, CITATION_SCHEMA)
                    included = any(
                        (i["kind"] == citation["kind"] and i["id"] == citation["id"])
                        or (
                            citation["kind"] == "source"
                            and i["kind"] == "cell"
                            and json.loads(i["text"]).get("revision") == citation["id"]
                        )
                        for i in receipt["items"]
                        if not i["truncated"]
                    )
                    verified.append(
                        {
                            **citation,
                            "verified": self.verify_citation(citation, snapshot),
                            "in_context": included,
                        }
                    )
                response["citations"] = verified
                valid: list[Any] = []
                warnings: list[Any] = []
                supplied = {
                    i["id"]: json.loads(i["text"])["revision"]
                    for i in receipt["items"]
                    if i["kind"] == "cell" and not i["truncated"]
                }
                for index, proposal in enumerate(response["proposals"]):
                    try:
                        validate_proposal(proposal, snapshot, supplied)
                        valid.append(proposal)
                    except (SourceError, ValueError, KeyError, TypeError) as error:
                        code = getattr(error, "code", "invalid_proposal")
                        warnings.append(
                            {
                                "index": index,
                                "code": "source_changed" if code == "stale" else code,
                                "message": str(error)
                                if isinstance(error, SourceError)
                                else "Invalid generated proposal shape; explanation retained",
                            }
                        )
                response["proposals"] = valid
                response["warnings"] = warnings
                proposals = self.store.finish(rid, response, provider.usage)
                for proposal in proposals:
                    emit({"kind": "proposal", "proposal": proposal})
                return {
                    **response,
                    "proposals": proposals,
                    "request_id": rid,
                    "conversation_id": self.store.rows(
                        "SELECT conversation FROM requests WHERE id=?", (rid,)
                    )[0]["conversation"],
                    "receipt_hash": receipt["hash"],
                    "usage": provider.usage,
                    "evidence_notice": "Citation targets verified where marked. Uncited claims and suggested causes are unverified.",
                }
            except Exception as error:
                self.store.fail(rid, getattr(error, "code", "invalid_response"), provider.usage)
                if isinstance(error, (ValueError, KeyError, TypeError)) and not isinstance(
                    error, SourceError
                ):
                    raise SourceError(
                        "invalid_response", "model returned an invalid response; regenerate"
                    ) from None
                raise
        raise SourceError("invalid_request", "unknown assistant method")

    def admit(self, config: Any, input_estimate: int) -> None:
        if (
            self.store.used(config["provider"]) + input_estimate + config["max_output_tokens"]
            > config["daily_token_cap"]
        ):
            raise SourceError(
                "budget",
                "Daily notebook/provider token reservation budget would be exceeded; reservations include failed requests and connection tests",
            )

    def verify_citation(self, citation: Any, snapshot: Any) -> bool:
        kind, identity = citation["kind"], citation["id"]
        if kind in ("cell", "source"):
            return any(
                c["id" if kind == "cell" else "revision"] == identity for c in snapshot["cells"]
            )
        try:
            self.tools.read(
                "execution.get" if kind == "execution" else "checkpoint.get",
                **{"exec_id" if kind == "execution" else "checkpoint_id": identity},
            )
            return True
        except (SourceError, ValueError, KeyError, OSError):
            return False

    def resolve(self, request: Any) -> Any:
        rows = self.store.rows("SELECT * FROM proposals WHERE id=?", (request["proposal_id"],))
        if not rows:
            raise SourceError("not_found", "proposal not found")
        row = rows[0]
        proposal = json.loads(row["payload"])
        if request["method"] == "application.record":
            existing = self.store.rows(
                "SELECT result FROM applications WHERE proposal_id=?", (row["id"],)
            )
            if existing:
                outcome = (
                    "failed"
                    if json.loads(existing[0]["result"]).get("status") == "failed"
                    else "applied"
                )
                return {
                    "recorded": True,
                    "proposal_id": row["id"],
                    "state": outcome,
                    "duplicate": True,
                }
            if row["state"] != "accepted":
                raise SourceError("invalid_request", "application requires an accepted proposal")
            with self.store.db:
                self.store.db.execute(
                    "INSERT INTO applications VALUES(?,?,?,?)",
                    (
                        row["id"],
                        row["id"],
                        json.dumps(clean(request["result"], secrets=(self.credential,))),
                        time.time(),
                    ),
                )
                outcome = (
                    request["result"].get("status")
                    if request["result"].get("status") in ("failed", "unknown")
                    else "applied"
                )
                self.store.db.execute(
                    "UPDATE proposals SET state=? WHERE id=?", (outcome, row["id"])
                )
                self.store.db.execute(
                    "UPDATE application_intents SET state=? WHERE proposal_id=?",
                    (outcome, row["id"]),
                )
            return {"recorded": True, "proposal_id": row["id"], "state": outcome}
        if row["state"] != "proposed":
            raise SourceError("invalid_request", "proposal is already resolved")
        state = request["state"]
        if state not in ("accepted", "rejected"):
            raise SourceError("invalid_request", "choose accepted or rejected")
        mutation = None
        if state == "accepted":
            snapshot = self.tools.read("source.read")
            try:
                validate_proposal(proposal, snapshot)
                if (
                    proposal["kind"] == "insert_cells"
                    and snapshot["manifest_revision"] != row["manifest_revision"]
                ):
                    raise SourceError("stale", "notebook cell order changed")
            except SourceError:
                with self.store.db:
                    self.store.db.execute(
                        "UPDATE proposals SET state='stale' WHERE id=?", (row["id"],)
                    )
                raise
            if proposal["kind"] == "edit_cell":
                mutation = {
                    "action": "edit",
                    "cell_id": proposal["cell_id"],
                    "source": proposal["source"],
                    "manifest_revision": snapshot["manifest_revision"],
                    "cell_revisions": {proposal["cell_id"]: proposal["source_sha256"]},
                }
            if proposal["kind"] == "insert_cells":
                mutation = {
                    "action": "insert_cells",
                    "index": proposal["index"],
                    "cells": proposal["cells"],
                    "manifest_revision": snapshot["manifest_revision"],
                }
        with self.store.db:
            self.store.db.execute("UPDATE proposals SET state=? WHERE id=?", (state, row["id"]))
            if state == "accepted":
                self.store.db.execute(
                    "INSERT INTO application_intents VALUES(?,?,?,?,?)",
                    (
                        row["id"],
                        uuid.uuid4().hex,
                        "applying",
                        json.dumps({"mutation": mutation, "payload": proposal}),
                        time.time(),
                    ),
                )
        intent = self.store.rows(
            "SELECT id FROM application_intents WHERE proposal_id=?", (row["id"],)
        )
        return {
            "proposal_id": row["id"],
            "state": state,
            "payload": proposal,
            "mutation": mutation,
            "application_id": intent[0]["id"] if intent else None,
        }

    def index(self) -> None:
        policy, notebook_config = settings.policy(self.root)
        if policy == "off" or not settings.load(self.settings_file)["provider"]:
            raise SourceError("disabled", "select a provider before creating an assistant index")
        items: list[Any] = []
        cursor = None
        while True:
            result = self.tools.read(
                "executions.list", page_size=500, **({"cursor": cursor} if cursor else {})
            )
            for row in result["items"]:
                value = row  # List includes recorded source; no N+1 execution queries.
                items.append(
                    (
                        "execution",
                        row["exec_id"],
                        json.dumps(
                            clean(
                                value,
                                notebook_config.get("checkpoint_denylist", []),
                                (self.credential,),
                            )
                        ),
                    )
                )
            cursor = result.get("cursor")
            if not cursor:
                break
        signature = hashlib.sha256(json.dumps(items).encode()).hexdigest()
        if signature == self.index_signature:
            return
        with self.store.db:
            self.store.db.execute("DELETE FROM history_index")
            self.store.db.executemany("INSERT INTO history_index VALUES(?,?,?)", items)
        self.index_signature = signature
