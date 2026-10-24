"""Deterministic notebook facts, independent of models and provider configuration."""

import difflib
import statistics
from typing import Any

from .tools import ReadTools


def stale_cells(tools: ReadTools) -> Any:
    snapshot = tools.read("source.read")
    latest = tools.read("executions.list", latest_per_cell=True, page_size=500)["items"]
    by_id = {r["cell_id"]: r for r in latest}
    result: list[Any] = []
    for cell in snapshot.get("cells", []):
        if cell["type"] != "code":
            continue
        row = by_id.get(cell["id"])
        result.append(
            {
                "cell_id": cell["id"],
                "state": "never_run"
                if not row
                else "stale"
                if row["source_sha256"] != cell["revision"]
                else "current",
                "exec_id": row["exec_id"] if row else None,
                "source_sha256": cell["revision"],
            }
        )
    return {
        "cells": result,
        "dependency_order": "unknown; execution order is not a dependency graph",
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
