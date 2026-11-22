"""Deterministic notebook facts, independent of models and provider configuration."""

import difflib
import statistics
from typing import Any

from ..dag import stale_plan
from .tools import ReadTools


def _current_session_id(tools: ReadTools) -> str | None:
    """The live kernel session (most recently started `running` session), if any."""
    try:
        sessions = tools.read("sessions.list", page_size=500).get("items", [])
    except Exception:
        return None
    running = [s for s in sessions if s.get("state") == "running" and s.get("session_id")]
    return str(running[-1]["session_id"]) if running else None


def stale_cells(tools: ReadTools) -> Any:
    snapshot = tools.read("source.read")
    session_id = _current_session_id(tools)
    cells = snapshot.get("cells", [])
    executions: list[dict[str, Any]] = []
    if session_id is not None:
        # Only the live session's executions describe what is actually in the namespace.
        cursor: str | None = None
        while True:
            page = tools.read(
                "executions.list",
                session_id=session_id,
                page_size=500,
                **({"cursor": cursor} if cursor else {}),
            )
            executions.extend(page.get("items", []))
            cursor = page.get("cursor")
            if not cursor:
                break
    by_id: dict[str, Any] = {}
    for row in executions:
        if isinstance(row.get("cell_id"), str):
            by_id[row["cell_id"]] = row
    plan = stale_plan(cells, executions=executions)
    state_by_id = {item["cell_id"]: item for item in plan["cells"]}
    result: list[Any] = []
    for cell in cells:
        if cell.get("type") not in ("code", "sql"):
            continue
        cid = cell["id"]
        row = by_id.get(cid)
        planned = state_by_id.get(cid, {})
        result.append(
            {
                "cell_id": cid,
                "state": planned.get(
                    "state",
                    "never_run"
                    if not row
                    else "stale"
                    if row["source_sha256"] != cell["revision"]
                    else "current",
                ),
                "upstream_stale_from": planned.get("upstream_stale_from", []),
                "blocked_by": planned.get("blocked_by"),
                "defs": planned.get("defs", []),
                "refs": planned.get("refs", []),
                "exec_id": row["exec_id"] if row else None,
                "source_sha256": cell["revision"],
            }
        )
    graph = plan["graph"]
    return {
        "cells": result,
        "dependency_order": graph["topological_order"],
        "edges": graph["edges"],
        "multiple_definitions": graph["multiple_definitions"],
        "cycles": graph["cycles"],
        "run_stale_ids": plan["run_stale_ids"],
        "failed_ids": plan.get("failed_ids", []),
        "blocked_ids": plan.get("blocked_ids", []),
        "session_id": session_id,
    }


def diff_executions(
    tools: ReadTools, first: str, second: str, other: ReadTools | None = None
) -> Any:
    a = tools.read("execution.get", exec_id=first)
    b = (other or tools).read("execution.get", exec_id=second)
    return {
        "first": a,
        "second": b,
        "observed": {
            k: {"before": a.get(k), "after": b.get(k)}
            for k in sorted(a.keys() | b.keys())
            if a.get(k) != b.get(k)
        },
        "source_diff": "".join(
            difflib.unified_diff(
                a.get("source", "").splitlines(True),
                b.get("source", "").splitlines(True),
                fromfile=first,
                tofile=second,
            )
        ),
        "causes": "not established by recorded differences",
    }


def reproducibility(tools: ReadTools) -> Any:
    return {
        "staleness": stale_cells(tools),
        "checkpoints": tools.read("checkpoints.list"),
        "limits": [
            "Recorded hashes do not prove external data or environment equivalence.",
            "Checkpoint coverage exclusions require review.",
        ],
    }


def duration_statistics(rows: list[dict[str, Any]]) -> Any:
    durations = [
        r["duration"]
        for r in rows
        if isinstance(r.get("duration"), (int, float)) and r["duration"] >= 0
    ]
    if not durations:
        return {"count": 0, "flags": []}
    median = statistics.median(durations)
    mad = statistics.median(abs(v - median) for v in durations)
    return {
        "count": len(durations),
        "median": median,
        "mad": mad,
        "flags": [v for v in durations if v > median + max(3 * mad, median * 2)],
        "meaning": "duration outliers only; no inferred cause",
    }
