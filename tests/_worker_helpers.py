"""In-process Worker helpers shared by tests (not collected: no `test_` prefix)."""

from __future__ import annotations

import ast
import hashlib
from typing import Any

from kernelledger.worker import Worker


class FakeChannel:
    def __init__(self) -> None:
        self.events: list[dict[str, Any]] = []
        self.blobs: dict[str, bytes] = {}

    def send(self, kind: str, **fields: Any) -> None:
        self.events.append({"kind": kind, **fields})

    def blob(self, data: bytes) -> str:
        digest = hashlib.sha256(data).hexdigest()
        self.blobs[digest] = data
        return digest


def new_worker(*, advanced: bool = False) -> tuple[Worker, FakeChannel]:
    channel = FakeChannel()
    worker = Worker(channel)
    worker.advanced = advanced
    return worker, channel


def run(
    worker: Worker, channel: FakeChannel, source: str, cell_type: str = "code", **extra: Any
) -> tuple[dict[str, Any], list[dict[str, Any]]]:
    start = len(channel.events)
    worker.execute(
        {
            "exec_id": f"e{worker.ordinal + 1}",
            "source": source,
            "source_sha256": hashlib.sha256(source.encode()).hexdigest(),
            "cell_type": cell_type,
            **extra,
        }
    )
    emitted = channel.events[start:]
    idle = next(
        (e for e in reversed(emitted) if e.get("kind") == "status" and e.get("state") == "idle"),
        {},
    )
    return idle, emitted


def errors(events: list[dict[str, Any]]) -> list[str]:
    return [f"{e['ename']}: {e['evalue']}" for e in events if e.get("kind") == "error"]


def interactive(worker: Worker, channel: FakeChannel, method: str, **fields: Any) -> dict[str, Any]:
    start = len(channel.events)
    worker.transient(
        {
            "request_id": "req",
            "request": {"method": method, "worker_generation": worker.generation, **fields},
        }
    )
    return next(e for e in reversed(channel.events[start:]) if e["kind"] == "interactive_reply")


def run_snippet(snippet: str, namespace: dict[str, Any]) -> Any:
    """Execute a generated snippet the way a notebook cell would; return its last value."""
    tree = ast.parse(snippet)
    last = tree.body.pop() if tree.body and isinstance(tree.body[-1], ast.Expr) else None
    exec(compile(tree, "<snippet>", "exec"), namespace)
    if last is None:
        return None
    return eval(compile(ast.Expression(last.value), "<snippet>", "eval"), namespace)
