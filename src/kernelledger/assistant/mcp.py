"""Read-only MCP stdio facade. No provider, assistant store, or proposal tools."""

import json
import sys
from pathlib import Path
from typing import Any

from .analysis import diff_executions, reproducibility, stale_cells
from .redact import clean
from .settings import policy
from .tools import ReadTools

CATALOG: dict[str, tuple[str, dict[str, Any]]] = {
    "read_cells": ("source.read", {}),
    "list_executions": ("executions.list", {}),
    "get_execution": ("execution.get", {"exec_id": {"type": "string"}}),
    "get_output": ("fold.get", {"exec_id": {"type": "string"}}),
    "list_checkpoints": ("checkpoints.list", {}),
    "get_checkpoint": ("checkpoint.get", {"checkpoint_id": {"type": "string"}}),
    "inspect_training": ("training.inspect", {"snapshot": {"type": "string"}}),
    "environment_report": ("doctor", {}),
    "stale_cells": ("stale", {}),
    "reproducibility": ("reproducibility", {}),
    "diff_executions": ("diff", {"first": {"type": "string"}, "second": {"type": "string"}}),
}


def dispatch(root: Path, request: Any) -> Any:
    method = request["method"]
    if method == "initialize":
        return {
            "protocolVersion": "2024-11-05",
            "capabilities": {"tools": {"listChanged": False}},
            "serverInfo": {"name": "kernelledger", "version": "1"},
        }
    if method == "ping":
        return {}
    if method == "tools/list":
        return {
            "tools": [
                {
                    "name": name,
                    "description": "Read recorded notebook data; no code execution.",
                    "inputSchema": {
                        "type": "object",
                        "properties": fields,
                        "required": list(fields),
                        "additionalProperties": False,
                    },
                    "annotations": {"readOnlyHint": True, "destructiveHint": False},
                }
                for name, (_, fields) in CATALOG.items()
            ]
        }
    if method != "tools/call":
        raise ValueError("unknown MCP method")
    params = request["params"]
    target, fields = CATALOG[params["name"]]
    args = params.get("arguments", {})
    if set(args) != set(fields) or any(not isinstance(v, str) for v in args.values()):
        raise ValueError("invalid tool arguments")
    tools = ReadTools(root)
    result = (
        stale_cells(tools)
        if target == "stale"
        else reproducibility(tools)
        if target == "reproducibility"
        else diff_executions(tools, **args)
        if target == "diff"
        else tools.read(target, **args)
    )
    text = json.dumps(
        clean(result, policy(root)[1].get("checkpoint_denylist", [])), ensure_ascii=False
    )
    return {
        "content": [
            {
                "type": "text",
                "text": text[:131072] + ("\n[TRUNCATED]" if len(text) > 131072 else ""),
            }
        ],
        "isError": False,
    }


def main(root: Path) -> None:
    while True:
        raw = sys.stdin.buffer.readline(1048577)
        if not raw:
            return
        if len(raw) > 1048576:
            return
        request: Any = {}
        try:
            request = json.loads(raw)
            if "id" not in request:
                continue
            response = {"jsonrpc": "2.0", "id": request["id"], "result": dispatch(root, request)}
        except Exception:
            response = {
                "jsonrpc": "2.0",
                "id": request.get("id"),
                "error": {
                    "code": -32602,
                    "message": "invalid or unavailable read-only notebook tool",
                },
            }
        print(json.dumps(response), flush=True)
