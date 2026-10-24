"""Fixed read-only boundary and validated proposal contracts."""

from typing import Any

from ..desktop.helper import Helper
from ..source import SourceError

ALLOWLIST = frozenset(
    {
        "source.read",
        "sessions.list",
        "executions.list",
        "execution.get",
        "events.list",
        "fold.get",
        "checkpoints.list",
        "checkpoint.get",
        "training.list",
        "training.inspect",
        "native.status",
        "doctor",
        "interpreter.candidates",
        "blob.read",
    }
)


class ReadTools:
    def __init__(self, root: Any):
        self._helper = Helper(root)

    def read(self, method: str, **fields: Any) -> Any:
        if method not in ALLOWLIST:
            raise SourceError("forbidden", "assistant tool is read-only")
        # SourceService.read can recover pending transactions. Assistant reads never do.
        if method == "source.read":
            return self._helper.source.read(inspect_only=True)
        if method == "doctor":
            return {
                "source": self._helper.source.read(inspect_only=True),
                "native": self.read("native.status"),
                "checks": self._helper.notebook.doctor(),
            }
        value = self._helper.dispatch({**fields, "method": method})
        if method in ("execution.get", "fold.get"):
            value["outputs"] = [self.output(event) for event in value.get("outputs", [])]
        if method == "events.list":
            value["items"] = [self.output(event) for event in value.get("items", [])]
        return value

    def output(self, event: Any) -> Any:
        event = dict(event)
        if event.get("kind") in ("stdout", "stderr") and event.get("blob"):
            event["text"] = self.text_blob(event["blob"])
        if "bundle" in event:
            # Images and active MIME bodies are never silently selected as text context.
            bundle = {}
            for mime, item in event["bundle"].items():
                if mime not in ("text/plain", "application/json"):
                    bundle[mime] = {
                        "omitted": "explicit image selection and qualified vision model required"
                    }
                elif item.get("blob"):
                    bundle[mime] = {"text": self.text_blob(item["blob"])}
                else:
                    bundle[mime] = item
            event["bundle"] = bundle
        return event

    def text_blob(self, digest: str) -> str:
        head = self.read("blob.read", sha256=digest, length=8192)
        text = head["data"].decode("utf-8", "replace")
        if head["total_size"] > 8192:
            tail = self.read(
                "blob.read", sha256=digest, offset=max(8192, head["total_size"] - 8192), length=8192
            )
            text += "\n[TRUNCATED OUTPUT]\n" + tail["data"].decode("utf-8", "replace")
        return text


def object_schema(properties: dict[str, Any]) -> dict[str, Any]:
    return {
        "type": "object",
        "properties": properties,
        "required": list(properties),
        "additionalProperties": False,
    }


STRING = {"type": "string"}
CITATION_SCHEMA = object_schema(
    {
        "kind": {"type": "string", "enum": ["cell", "execution", "checkpoint", "source"]},
        "id": STRING,
    }
)
PROPOSAL_SCHEMAS = [
    object_schema(
        {
            "kind": {"const": "edit_cell", "type": "string"},
            "cell_id": STRING,
            "source_sha256": STRING,
            "source": STRING,
            "rationale": STRING,
        }
    ),
    object_schema(
        {
            "kind": {"const": "insert_cells", "type": "string"},
            "index": {"type": "integer", "minimum": 0},
            "cells": {
                "type": "array",
                "items": object_schema(
                    {
                        "type": {"type": "string", "enum": ["code", "markdown", "raw"]},
                        "source": STRING,
                    }
                ),
            },
            "rationale": STRING,
        }
    ),
    object_schema(
        {
            "kind": {"const": "run_cells", "type": "string"},
            "cells": {
                "type": "array",
                "items": object_schema({"cell_id": STRING, "source_sha256": STRING}),
            },
            "rationale": STRING,
        }
    ),
    object_schema(
        {"kind": {"const": "command", "type": "string"}, "text": STRING, "rationale": STRING}
    ),
]
RESPONSE_SCHEMA = object_schema(
    {
        "answer": STRING,
        "citations": {"type": "array", "items": CITATION_SCHEMA},
        "proposals": {"type": "array", "items": {"anyOf": PROPOSAL_SCHEMAS}},
    }
)


def check_schema(value: Any, schema: Any) -> None:
    """Validate our closed schema subset without adding a runtime dependency."""
    if "anyOf" in schema:
        for variant in schema["anyOf"]:
            try:
                check_schema(value, variant)
                return
            except ValueError:
                pass
        raise ValueError("invalid proposal shape")
    kind = schema["type"]
    if not {
        "object": isinstance(value, dict),
        "array": isinstance(value, list),
        "string": isinstance(value, str),
        "integer": type(value) is int,
    }[kind]:
        raise ValueError(f"expected {kind}")
    if (
        "const" in schema
        and value != schema["const"]
        or "enum" in schema
        and value not in schema["enum"]
    ):
        raise ValueError("invalid enum")
    if kind == "object":
        if set(value) != set(schema["properties"]):
            raise ValueError("missing or unknown response fields")
        for key, item in value.items():
            check_schema(item, schema["properties"][key])
    if kind == "array":
        if len(value) > 100:
            raise ValueError("too many response items")
        for item in value:
            check_schema(item, schema["items"])
    if kind == "integer" and value < schema.get("minimum", value):
        raise ValueError("integer below minimum")


def validate_proposal(proposal: Any, snapshot: Any) -> None:
    check_schema(proposal, {"anyOf": PROPOSAL_SCHEMAS})
    cells = {c["id"]: c for c in snapshot.get("cells", [])}
    targets = (
        [proposal]
        if proposal["kind"] == "edit_cell"
        else proposal.get("cells", [])
        if proposal["kind"] == "run_cells"
        else []
    )
    for target in targets:
        cell = cells.get(target["cell_id"])
        if cell is None or cell["revision"] != target["source_sha256"]:
            raise SourceError("stale", "proposal source has changed or cell is missing")
        if proposal["kind"] == "run_cells" and cell["type"] != "code":
            raise SourceError("invalid_request", "only code cells can run")
    if proposal["kind"] == "run_cells" and not targets:
        raise SourceError("invalid_request", "run proposal is empty")
    if proposal["kind"] == "insert_cells" and (
        not proposal["cells"] or proposal["index"] > len(cells)
    ):
        raise SourceError("stale", "invalid insertion position or empty cell set")
