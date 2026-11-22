"""Deterministic notebook facts, independent of models and provider configuration."""

import difflib
import statistics
from typing import Any

from ..dag import stale_plan
from .tools import ReadTools


def stale_cells(tools: ReadTools) -> Any:
    snapshot = tools.read("source.read")
    latest = tools.read("executions.list", latest_per_cell=True, page_size=500)["items"]
    by_id = {r["cell_id"]: r for r in latest}
    cells = snapshot.get("cells", [])
    executed_revisions = {
        cid: row.get("source_sha256") for cid, row in by_id.items() if isinstance(cid, str)
    }
    plan = stale_plan(cells, executed_revisions)
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
