"""Shared DAG spec: the Python analyzer must match tests/fixtures/dag_cases.json.

The same fixture drives the Rust implementation in desktop/crates/ui/src/dag.rs.
"""

from __future__ import annotations

import hashlib
import json
import sys
from pathlib import Path
from typing import Any

import pytest

from kernelledger.dag import (
    analyze_cells,
    analyze_python_cell,
    analyze_sql_cell,
    downstream_cells,
    notebook_stale_plan,
    stale_plan,
)
from kernelledger.ledger import Ledger
from kernelledger.notebook import Notebook

FIXTURE = json.loads(
    (Path(__file__).parent / "fixtures" / "dag_cases.json").read_text(encoding="utf-8")
)


def _ids(cases: list[dict[str, Any]]) -> list[str]:
    return [case["name"] for case in cases]


def test_fixture_is_well_formed() -> None:
    names = [c["name"] for group in FIXTURE.values() for c in group]
    assert len(names) == len(set(names))
    assert len(FIXTURE["python_cells"]) >= 60
    assert len(FIXTURE["sql_cells"]) >= 12
    assert len(FIXTURE["notebooks"]) >= 8


@pytest.mark.parametrize("case", FIXTURE["python_cells"], ids=_ids(FIXTURE["python_cells"]))
def test_python_cell(case: dict[str, Any]) -> None:
    result = analyze_python_cell(case["source"])
    assert sorted(set(result["defs"])) == case["defs"]
    assert sorted(set(result["refs"])) == case["refs"]
    assert (result["syntax_error"] is not None) == case.get("error", False)


@pytest.mark.parametrize("case", FIXTURE["sql_cells"], ids=_ids(FIXTURE["sql_cells"]))
def test_sql_cell(case: dict[str, Any]) -> None:
    result = analyze_sql_cell(case["source"], output_var=case["output_var"])
    assert sorted(set(result["defs"])) == case["defs"]
    assert sorted(set(result["refs"])) == case["refs"]


def _notebook_cells(case_cells: list[dict[str, Any]]) -> list[dict[str, Any]]:
    cells: list[dict[str, Any]] = []
    for c in case_cells:
        meta: dict[str, Any] = {"disabled": bool(c.get("disabled", False))}
        if c.get("output_var"):
            meta["output_var"] = c["output_var"]
        cells.append({"id": c["id"], "type": c["type"], "source": c["source"], "metadata": meta})
    return cells


@pytest.mark.parametrize("case", FIXTURE["notebooks"], ids=_ids(FIXTURE["notebooks"]))
def test_notebook(case: dict[str, Any]) -> None:
    graph = analyze_cells(_notebook_cells(case["cells"]))
    edges = sorted([e["from"], e["to"], sorted(e["vars"])] for e in graph["edges"])
    assert edges == case["edges"]
    assert sorted(sorted(c) for c in graph["cycles"]) == case["cycles"]
    if "topological_order" in case:
        assert graph["topological_order"] == case["topological_order"]


# ---------------------------------------------------------------------------
# Staleness rules
# ---------------------------------------------------------------------------


def _rev(source: str) -> str:
    return hashlib.sha256(source.encode("utf-8")).hexdigest()


def _cells(*specs: tuple[str, str], disabled: frozenset[str] = frozenset()) -> list[dict[str, Any]]:
    return [
        {
            "id": cid,
            "type": "code",
            "source": src,
            "revision": _rev(src),
            "metadata": {"disabled": cid in disabled},
        }
        for cid, src in specs
    ]


def _run(
    cells: list[dict[str, Any]], *order: str, status: dict[str, str] | None = None
) -> list[dict[str, Any]]:
    by_id = {c["id"]: c for c in cells}
    return [
        {
            "cell_id": cid,
            "source_sha256": by_id[cid]["revision"],
            "ordinal": i + 1,
            "status": (status or {}).get(f"{cid}@{i + 1}", "ok"),
        }
        for i, cid in enumerate(order)
    ]


def _states(plan: dict[str, Any]) -> dict[str, str]:
    return {c["cell_id"]: c["state"] for c in plan["cells"]}


CHAIN = (("c1", "a = 1\n"), ("c2", "b = a + 1\n"), ("c3", "c = b + 1\n"))


def test_never_run_cells_need_run_in_topological_order() -> None:
    cells = _cells(("c1", "y = x\n"), ("c2", "x = 1\n"))
    plan = stale_plan(cells, executions=[])
    assert _states(plan) == {"c1": "never_run", "c2": "never_run"}
    assert plan["run_stale_ids"] == ["c2", "c1"]
    assert plan["never_run_ids"] == ["c2", "c1"]
    assert all(c["upstream_stale_from"] == [] for c in plan["cells"])


def test_all_current_after_running_in_order() -> None:
    cells = _cells(*CHAIN)
    plan = stale_plan(cells, executions=_run(cells, "c1", "c2", "c3"))
    assert set(_states(plan).values()) == {"current"}
    assert plan["run_stale_ids"] == []


def test_edited_cell_is_direct_stale_and_descendants_upstream_stale() -> None:
    cells = _cells(*CHAIN)
    executions = _run(cells, "c1", "c2", "c3")
    cells[0]["source"] = "a = 2\n"
    cells[0]["revision"] = _rev("a = 2\n")
    plan = stale_plan(cells, executions=executions)
    assert _states(plan) == {"c1": "stale", "c2": "upstream_stale", "c3": "upstream_stale"}
    by_id = {c["cell_id"]: c for c in plan["cells"]}
    assert by_id["c3"]["upstream_stale_from"] == ["c1"]
    assert plan["direct_stale_ids"] == ["c1"]
    assert plan["upstream_stale_ids"] == ["c2", "c3"]
    assert plan["run_stale_ids"] == ["c1", "c2", "c3"]


def test_failed_last_run_needs_run() -> None:
    cells = _cells(*CHAIN)
    executions = _run(cells, "c1", "c2", "c3", "c2", status={"c2@4": "error"})
    plan = stale_plan(cells, executions=executions)
    states = _states(plan)
    assert states["c1"] == "current"
    assert states["c2"] == "failed"
    assert states["c3"] == "upstream_stale"
    assert plan["failed_ids"] == ["c2"]
    assert plan["run_stale_ids"] == ["c2", "c3"]


def test_interrupted_run_is_failed() -> None:
    cells = _cells(("c1", "a = 1\n"))
    plan = stale_plan(cells, executions=_run(cells, "c1", status={"c1@1": "interrupted"}))
    assert _states(plan) == {"c1": "failed"}
    assert plan["run_stale_ids"] == ["c1"]


def test_queued_and_running_executions_do_not_count() -> None:
    cells = _cells(("c1", "a = 1\n"))
    executions = _run(cells, "c1", "c1", status={"c1@2": "running"})
    assert _states(stale_plan(cells, executions=executions)) == {"c1": "current"}
    executions = _run(cells, "c1", status={"c1@1": "queued"})
    assert _states(stale_plan(cells, executions=executions)) == {"c1": "never_run"}


def test_upstream_reran_after_downstream() -> None:
    cells = _cells(*CHAIN)
    plan = stale_plan(cells, executions=_run(cells, "c1", "c2", "c3", "c1"))
    by_id = {c["cell_id"]: c for c in plan["cells"]}
    assert by_id["c1"]["state"] == "current"
    assert by_id["c2"]["state"] == "upstream_stale"
    assert by_id["c3"]["state"] == "upstream_stale"
    assert by_id["c2"]["upstream_stale_from"] == ["c1"]
    assert by_id["c3"]["upstream_stale_from"] == ["c1"]
    assert plan["run_stale_ids"] == ["c2", "c3"]


def test_downstream_run_after_upstream_is_current() -> None:
    cells = _cells(*CHAIN)
    plan = stale_plan(cells, executions=_run(cells, "c1", "c2", "c3", "c1", "c2", "c3"))
    assert set(_states(plan).values()) == {"current"}


def test_redefinition_chain_staleness_has_no_cycle() -> None:
    cells = _cells(("c1", "x = 0\n"), ("c2", "x += 1\n"), ("c3", "x += 1\n"), ("c4", "print(x)\n"))
    plan = stale_plan(cells, executions=_run(cells, "c1", "c2", "c3", "c4"))
    assert plan["graph"]["cycles"] == []
    assert plan["run_stale_ids"] == []
    plan = stale_plan(cells, executions=_run(cells, "c1", "c2", "c3", "c4", "c3"))
    assert plan["run_stale_ids"] == ["c4"]


def test_blocked_by_disabled_cell() -> None:
    cells = _cells(*CHAIN, ("c4", "d = 4\n"), disabled=frozenset({"c2"}))
    plan = stale_plan(cells, executions=[])
    by_id = {c["cell_id"]: c for c in plan["cells"]}
    assert by_id["c2"]["state"] == "disabled"
    assert by_id["c2"]["blocked"] and by_id["c3"]["blocked"]
    assert by_id["c3"]["blocked_by"] == ["c2"]
    assert not by_id["c1"]["blocked"] and not by_id["c4"]["blocked"]
    assert plan["blocked_ids"] == ["c2", "c3"]
    assert plan["run_stale_ids"] == ["c1", "c4"]
    assert plan["never_run_ids"] == ["c1", "c4"]
    # downstream_cells stays pure graph traversal, independent of blocking.
    assert downstream_cells(plan["graph"], "c1") == ["c2", "c3"]


def test_run_stale_ids_follow_topological_not_notebook_order() -> None:
    cells = _cells(("c1", "z = y + 1\n"), ("c2", "w = 5\n"), ("c3", "y = w\n"))
    plan = stale_plan(cells, executions=[])
    assert plan["run_stale_ids"] == ["c2", "c3", "c1"]


def test_legacy_executed_revisions_argument() -> None:
    cells = _cells(*CHAIN)
    plan = stale_plan(cells, {"c1": cells[0]["revision"], "c2": "old"})
    assert _states(plan) == {"c1": "current", "c2": "stale", "c3": "never_run"}
    assert plan["run_stale_ids"] == ["c2", "c3"]


def test_notebook_stale_plan_counts_only_current_session(tmp_path: Path) -> None:
    nb = Notebook(tmp_path)
    nb.init()
    nb.add("code", "a = 1\n", "c1")
    nb.add("code", "b = a + 1\n", "c2")
    ledger = Ledger(tmp_path / "ledger.sqlite")
    try:
        old = ledger.session("nb", {}, state="closed")
        for cid, src in (("c1", "a = 1\n"), ("c2", "b = a + 1\n")):
            eid = ledger.execution(old, cid, _rev(src))
            ledger.write("UPDATE executions SET status='ok' WHERE exec_id=?", (eid,))
        current = ledger.session("nb", {})
        eid = ledger.execution(current, "c1", _rev("a = 1\n"))
        ledger.write("UPDATE executions SET status='ok' WHERE exec_id=?", (eid,))

        plan = notebook_stale_plan(nb, ledger)
        assert plan["session_id"] == current
        assert _states(plan) == {"c1": "current", "c2": "never_run"}
        assert plan["run_stale_ids"] == ["c2"]

        # Explicit session selection.
        plan = notebook_stale_plan(nb, ledger, session_id=old)
        assert _states(plan) == {"c1": "current", "c2": "current"}

        # No live kernel session: nothing ran in the (empty) namespace.
        ledger.write("UPDATE sessions SET state='closed' WHERE session_id=?", (current,))
        plan = notebook_stale_plan(nb, ledger)
        assert plan["session_id"] is None
        assert plan["run_stale_ids"] == ["c1", "c2"]
    finally:
        ledger.close()


@pytest.mark.skipif(sys.version_info < (3, 12), reason="PEP 695 syntax needs Python 3.12+")
@pytest.mark.parametrize(
    ("source", "defs", "refs"),
    [
        ("def f[T](a: T) -> T:\n    return a", ["f"], []),
        ("class C[T](Base[T]):\n    pass", ["C"], ["Base"]),
        ("type Alias[T] = list[T] | Other", ["Alias"], ["Other"]),
        ("def g[T: Bound](x: T):\n    pass", ["g"], ["Bound"]),
    ],
)
def test_pep695_type_params_are_local(source: str, defs: list[str], refs: list[str]) -> None:
    # Mirrors the Rust analyzer: type parameters live in their own annotation scope.
    result = analyze_python_cell(source)
    assert sorted(result["defs"]) == defs
    assert sorted(result["refs"]) == refs
