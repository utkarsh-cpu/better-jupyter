"""Runtime-owned assistant method catalog and generated schema."""

from typing import Any, cast

from ..source import SourceError

METHODS = {
    "status": [],
    "settings.get": [],
    "settings.update": ["settings"],
    "policy.update": ["policy"],
    "providers.list": [],
    "providers.test": [],
    "models.list": [],
    "context.preview": ["question"],
    "respond": ["receipt_hash"],
    "cancel": [],
    "conversations.list": [],
    "conversation.get": ["conversation_id"],
    "proposals.list": [],
    "proposal.resolve": ["proposal_id", "state"],
    "application.record": ["proposal_id", "result"],
    "search": ["query"],
    "purge": [],
}


def validate(request: Any) -> None:
    if not isinstance(request, dict):
        raise SourceError("invalid_request", "request must be an object")
    request = cast(dict[str, Any], request)
    if request.get("method") not in METHODS:
        raise SourceError("invalid_request", "unknown assistant method")
    for key in METHODS[request["method"]]:
        if key not in request:
            raise SourceError("invalid_request", f"{key} is required")
    for key in (
        "question",
        "receipt_hash",
        "approved_receipt",
        "conversation_id",
        "proposal_id",
        "state",
        "query",
        "policy",
        "task",
    ):
        if key in request and not isinstance(request[key], str):
            raise SourceError("invalid_request", f"{key} must be a string")
    for key in ("cell_ids", "exec_ids", "checkpoint_ids"):
        if key in request and (
            not isinstance(request[key], list)
            or len(cast(list[Any], request[key])) > 100
            or any(not isinstance(v, str) for v in cast(list[Any], request[key]))
        ):
            raise SourceError("invalid_request", f"{key} must be a bounded list of ids")
    for key in ("settings", "result"):
        if key in request and not isinstance(request[key], dict):
            raise SourceError("invalid_request", f"{key} must be an object")


def schema() -> Any:
    fields: dict[str, Any] = {
        key: {"type": "string"}
        for key in (
            "request_id",
            "question",
            "receipt_hash",
            "approved_receipt",
            "conversation_id",
            "proposal_id",
            "state",
            "query",
            "policy",
            "task",
        )
    }
    fields.update(
        {
            key: {"type": "array", "maxItems": 100, "items": {"type": "string"}}
            for key in ("cell_ids", "exec_ids", "checkpoint_ids")
        }
    )
    fields.update({key: {"type": "object"} for key in ("settings", "result")})
    return {
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "title": "KernelLedger assistant v1",
        "oneOf": [
            {
                "type": "object",
                "properties": {"method": {"const": method}, **fields},
                "required": ["method", *required],
            }
            for method, required in METHODS.items()
        ],
    }
