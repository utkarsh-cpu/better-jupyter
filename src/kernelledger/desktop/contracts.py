"""Protocol request catalog used for stdlib validation and published JSON schemas."""

import uuid
from typing import Any, cast

from ..source import SourceError

SUPERVISOR = {
    "runtime.status": [],
    "run": ["expected_sha256"],
    "batch": [],
    "interrupt": [],
    "interrupt_kill": [],
    "restart": [],
    "stop": [],
    "operation.get": [],
    "operation.list": [],
    "operation.cancel": ["operation_id"],
    "subscribe": [],
    "changes.list": [],
    "execution.follow": ["exec_id"],
    "input.claim": ["input_id"],
    "input.release": ["input_id"],
    "input.reply": ["input_id", "text"],
    "input.cancel": ["input_id"],
    "checkpoint": [],
    "training-checkpoint": [],
    "training-restore": ["snapshot"],
    "training.list": [],
    "training.inspect": ["snapshot"],
    "native.status": [],
    "restore": ["checkpoint_id"],
    "continue": ["checkpoint_id"],
    "replay": ["session_id"],
    "pin": ["name", "checkpoint_id"],
    "unpin": ["name"],
    "gc": [],
    "publish": ["checkpoint_id"],
    "import_ipynb": ["path"],
    "bundle_import": ["path"],
    "migrate": [],
    "blob.read": ["sha256"],
}
HELPER = {
    "training.list": [],
    "training.inspect": ["snapshot"],
    "native.status": [],
    "source.read": [],
    "source.mutate": ["action", "manifest_revision"],
    "source.recover": [],
    "source.watch": [],
    "sessions.list": [],
    "executions.list": [],
    "executions.watermark": [],
    "execution.get": ["exec_id"],
    "events.list": ["exec_id"],
    "fold.get": ["exec_id"],
    "checkpoints.list": [],
    "checkpoint.get": ["checkpoint_id"],
    "bundle.inspect": ["path"],
    "descriptor.inspect": ["path"],
    "export_ipynb": ["path"],
    "export_document": ["path", "format"],
    "bundle_export": ["path"],
    "doctor": [],
    "interpreter.candidates": [],
    "interpreter.validate": ["path"],
    "blob.read": ["sha256"],
    "ping": [],
}
EFFECTS = {
    "training-checkpoint",
    "training-restore",
    "run",
    "batch",
    "restart",
    "stop",
    "checkpoint",
    "restore",
    "continue",
    "replay",
    "pin",
    "unpin",
    "gc",
    "publish",
    "import_ipynb",
    "bundle_import",
    "migrate",
}


def validate(request: Any, helper: bool = False) -> None:
    if not isinstance(request, dict):
        raise SourceError("invalid_request", "request must be an object")
    request = cast(dict[str, Any], request)
    method = request.get("method")
    catalog = HELPER if helper else SUPERVISOR
    if not isinstance(method, str) or method not in catalog:
        raise SourceError("invalid_request", "unknown request method")
    for key in catalog[method]:
        if key not in request:
            raise SourceError("invalid_request", f"{key} is required")
    for key in (
        "exec_id",
        "session_id",
        "checkpoint_id",
        "operation_id",
        "cell_id",
        "input_id",
        "path",
        "name",
        "text",
        "source",
        "manifest_revision",
        "snapshot",
        "device",
        "format",
        "markdown",
    ):
        if key in request and not isinstance(request[key], str):
            raise SourceError("invalid_request", f"{key} must be a string")
    for key in ("offset", "length", "after_seq", "after_change_seq", "page_size"):
        if key in request and (type(request[key]) is not int or request[key] < 0):
            raise SourceError("invalid_request", f"{key} must be a nonnegative integer")
    if "page_size" in request and not 1 <= request["page_size"] <= 500:
        raise SourceError("invalid_request", "page_size must be between 1 and 500")
    for key in (
        "trust",
        "allow_python_mismatch",
        "reuse_input",
        "keep_outputs",
        "dry_run",
        "latest_per_cell",
    ):
        if key in request and type(request[key]) is not bool:
            raise SourceError("invalid_request", f"{key} must be a boolean")
    if not helper and method in EFFECTS:
        try:
            uuid.UUID(request["client_request_id"])
        except (ValueError, KeyError, TypeError, AttributeError):
            raise SourceError("invalid_request", "client_request_id must be a UUID") from None
    if method == "run" and (("source" in request) + ("cell_id" in request) != 1):
        raise SourceError("invalid_request", "run requires one cell_id or source")
    if "snapshot" in request:
        from ..blobs import check_hash

        try:
            check_hash(request["snapshot"])
        except ValueError:
            raise SourceError("invalid_request", "snapshot must be a SHA256 digest") from None


def schema(helper: bool = False) -> Any:
    catalog = HELPER if helper else SUPERVISOR
    variants: list[Any] = []
    for method, required in catalog.items():
        properties: dict[str, Any] = {
            "method": {"const": method},
            "request_id": {"type": ["string", "null"]},
        }
        keys = ["method", *required]
        if not helper and method in EFFECTS:
            keys.append("client_request_id")
            properties["client_request_id"] = {"type": "string", "format": "uuid"}
        for key in required:
            properties[key] = (
                {"type": "string"}
                if key != "expected_sha256"
                else {"type": "string", "pattern": "^[0-9a-f]{64}$"}
            )
        if helper and method == "export_document":
            properties["markdown"] = {"type": "string"}
        variants.append({"type": "object", "required": keys, "properties": properties})
    variants.extend(
        [
            {
                "type": "object",
                "required": ["kind", "api", "versions", "client_id", "client_name", "role"],
                "properties": {
                    "kind": {"const": "hello"},
                    "api": {"const": "kernelledger.desktop"},
                    "versions": {"type": "array", "items": {"type": "integer"}},
                    "client_id": {"type": "string", "format": "uuid"},
                    "client_name": {"type": "string", "maxLength": 256},
                    "role": {"enum": ["control", "subscription", "blob"]},
                    "token": {"type": "string"},
                },
            },
            {
                "type": "object",
                "required": ["kind", "version", "identity", "capabilities"],
                "properties": {
                    "kind": {"const": "welcome"},
                    "version": {"const": 1},
                    "identity": {"type": "object"},
                    "capabilities": {"type": "array", "items": {"type": "string"}},
                },
            },
            {
                "type": "object",
                "required": ["kind", "result"],
                "properties": {"kind": {"const": "response"}},
            },
            {
                "type": "object",
                "required": ["kind", "error"],
                "properties": {
                    "kind": {"const": "error"},
                    "error": {
                        "type": "object",
                        "required": ["code", "message", "retryable", "details"],
                        "properties": {
                            "code": {"type": "string"},
                            "message": {"type": "string"},
                            "retryable": {"type": "boolean"},
                            "details": {"type": "object"},
                        },
                    },
                },
            },
            {
                "type": "object",
                "required": ["kind", "change_seq", "runtime"],
                "properties": {
                    "kind": {"const": "snapshot"},
                    "change_seq": {"type": "integer", "minimum": 0},
                },
            },
            {
                "type": "object",
                "required": ["kind", "change_seq", "ref"],
                "properties": {
                    "kind": {"const": "change"},
                    "change_seq": {"type": "integer", "minimum": 1},
                },
            },
            {
                "type": "object",
                "required": ["kind", "exec_id", "event"],
                "properties": {
                    "kind": {"const": "event"},
                    "event": {"type": "object", "required": ["seq", "kind"]},
                },
            },
            {
                "type": "object",
                "required": ["kind", "after_change_seq"],
                "properties": {"kind": {"const": "resync_required"}},
            },
            {
                "type": "object",
                "required": ["kind", "sha256", "offset", "byte_size", "total_size", "verified"],
                "properties": {
                    "kind": {"const": "blob"},
                    "sha256": {"type": "string", "pattern": "^[0-9a-f]{64}$"},
                    "byte_size": {"type": "integer", "minimum": 0, "maximum": 1048576},
                },
            },
            {
                "type": "object",
                "required": ["kind"],
                "properties": {"kind": {"enum": ["ping", "pong"]}},
            },
        ]
    )
    return {
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "title": "KernelLedger desktop helper 1" if helper else "KernelLedger desktop protocol 1",
        "oneOf": variants,
    }
