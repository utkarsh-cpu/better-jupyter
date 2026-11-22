"""Comprehensive tests for marimo-inspired features (Milestones 1-4)."""

from __future__ import annotations

import hashlib
import json
from pathlib import Path
from typing import Any

import pytest

from kernelledger.assistant.analysis import stale_cells
from kernelledger.cache import cache, persistent_cache
from kernelledger.dag import analyze_cells, downstream_cells
from kernelledger.desktop.helper import Helper
from kernelledger.documents import export_document
from kernelledger.notebook import Notebook
from kernelledger.source import SourceError, SourceService
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


def _new_worker(*, advanced: bool = False) -> tuple[Worker, FakeChannel]:
    channel = FakeChannel()
    worker = Worker(channel)
    worker.advanced = advanced
    return worker, channel


def _run_worker(
    worker: Worker,
    channel: FakeChannel,
    source: str,
    cell_type: str = "code",
    output_var: str | None = None,
) -> tuple[dict[str, Any], list[dict[str, Any]]]:
    start = len(channel.events)
    req: dict[str, Any] = {
        "exec_id": f"e{worker.ordinal + 1}",
        "source": source,
        "source_sha256": hashlib.sha256(source.encode()).hexdigest(),
        "cell_type": cell_type,
    }
    if output_var is not None:
        req["output_var"] = output_var
    worker.execute(req)
    emitted = channel.events[start:]
    idle = next(
        (e for e in reversed(emitted) if e.get("kind") == "status" and e.get("state") == "idle"), {}
    )
    return idle, emitted


def _interactive(
    worker: Worker, channel: FakeChannel, method: str, **fields: Any
) -> dict[str, Any]:
    start = len(channel.events)
    worker.transient(
        {
            "request_id": "req-1",
            "request": {
                "method": method,
                "worker_generation": worker.generation,
                **fields,
            },
        }
    )
    emitted = channel.events[start:]
    return next(e for e in reversed(emitted) if e.get("kind") == "interactive_reply")


# ---------------------------------------------------------------------------
# M1: Static AST + SQL Dependency Graph & Cascade Stale Detection
# ---------------------------------------------------------------------------


def test_dag_analysis_defs_refs_downstream_cycles_and_multiple_defs() -> None:
    cells = [
        {"id": "c1", "type": "code", "source": "x = 10\ny = 20\n"},
        {"id": "c2", "type": "code", "source": "z = x + y\n"},
        {
            "id": "c3",
            "type": "sql",
            "source": "SELECT * FROM tbl WHERE a > 1\n",
            "metadata": {"output_var": "q_df"},
        },
        {"id": "c4", "type": "code", "source": "total = z + len(q_df)\n"},
        {"id": "c5", "type": "code", "source": "x = 99\n"},
    ]
    graph = analyze_cells(cells)
    by_id = {c["id"]: c for c in graph["cells"]}
    assert "x" in by_id["c1"]["defs"]
    assert "z" in by_id["c2"]["defs"]
    assert "x" in by_id["c2"]["refs"]
    assert "q_df" in by_id["c3"]["defs"]
    assert "tbl" in by_id["c3"]["refs"]
    assert "q_df" in by_id["c4"]["refs"]

    # Multiple definitions detected for x
    assert "x" in graph["multiple_definitions"]
    assert set(graph["multiple_definitions"]["x"]) == {"c1", "c5"}

    # Downstream of c1 includes c2 and c4 transitively
    ds = downstream_cells(graph, "c1")
    assert ds == ["c2", "c4"]

    # Cycles detection
    cyclic = analyze_cells(
        [
            {"id": "a", "type": "code", "source": "v1 = v2 + 1\n"},
            {"id": "b", "type": "code", "source": "v2 = v1 + 1\n"},
        ]
    )
    assert len(cyclic["cycles"]) == 1
    assert set(cyclic["cycles"][0]) == {"a", "b"}


def test_dag_stale_plan_and_assistant_stale_cascade(tmp_path: Path) -> None:
    nb = Notebook(tmp_path)
    nb.init()
    nb.add("code", "a = 1\n", "c1")
    nb.add("code", "b = a + 2\n", "c2")
    nb.add("code", "c = b * 3\n", "c3")
    nb.add("code", "d = 42\n", "c4")

    # Disable c4 via metadata
    svc = SourceService(tmp_path)
    snap = svc.read()
    c4_rev = next(c["revision"] for c in snap["cells"] if c["id"] == "c4")
    svc.mutate(
        {
            "action": "metadata",
            "cell_id": "c4",
            "metadata": {"disabled": True},
            "manifest_revision": snap["manifest_revision"],
            "cell_revisions": {"c4": c4_rev},
        }
    )

    from kernelledger.dag import notebook_stale_plan

    plan = notebook_stale_plan(nb, None)
    # c1, c2, c3 are never run (stale); c4 is disabled so excluded from run_stale_ids
    assert set(plan["run_stale_ids"]) == {"c1", "c2", "c3"}
    assert "c4" not in plan["run_stale_ids"]

    # Helper RPC dag.analyze returns the same structure offline
    helper = Helper(tmp_path)
    rpc_plan = helper.dispatch({"method": "dag.analyze"})
    assert rpc_plan["run_stale_ids"] == plan["run_stale_ids"]

    # assistant.analysis.stale_cells returns cascade stale cells
    from kernelledger.assistant.tools import ReadTools

    stale = stale_cells(ReadTools(tmp_path))
    assert stale["run_stale_ids"] == ["c1", "c2", "c3"]


# ---------------------------------------------------------------------------
# M2: Interactive DataFrame Explorer (Sort, Filter, Histograms, Snippets)
# ---------------------------------------------------------------------------


def test_dataframe_explorer_sort_filter_histograms_and_snippets() -> None:
    pytest.importorskip("pandas")
    worker, channel = _new_worker()
    _run_worker(
        worker,
        channel,
        "import pandas as pd\n"
        "df = pd.DataFrame({\n"
        "    'region': ['North', 'South', 'North', 'West', 'South', 'North'],\n"
        "    'amount': [10, 50, 30, 20, 40, 60],\n"
        "})\n",
    )
    listed = _interactive(worker, channel, "namespace.list", page_size=50)
    df_entry = next(item for item in listed["items"] if item["name"] == "df")
    handle = df_entry["handle"]

    # 1. Default preview with include_stats -> histograms & top_values
    preview = _interactive(worker, channel, "namespace.preview", handle=handle, include_stats=True)
    page = preview["page"]
    assert page["mode"] == "table"
    table = page["table"]
    assert table["total_rows"] == 6
    assert table["unfiltered_rows"] == 6
    schema_by_name = {c["name"]: c for c in page["schema"]}
    assert "histogram" in schema_by_name["amount"]
    assert len(schema_by_name["amount"]["histogram"]) == 8
    assert "top_values" in schema_by_name["region"]
    assert schema_by_name["region"]["top_values"][0]["value"] == "North"
    assert schema_by_name["region"]["top_values"][0]["count"] == 3

    # 2. Sort descending by amount
    sorted_prev = _interactive(
        worker, channel, "namespace.preview", handle=handle, sort_col="amount", sort_desc=True
    )
    amounts = [row[1]["text"] for row in sorted_prev["page"]["table"]["rows"]]
    assert amounts == ["60", "50", "40", "30", "20", "10"]
    assert "sort_values('amount', ascending=False)" in sorted_prev["page"]["snippet"]

    # 3. Filter numeric comparison + sort
    filtered = _interactive(
        worker,
        channel,
        "namespace.preview",
        handle=handle,
        filter_query="amount>=30",
        sort_col="amount",
        sort_desc=False,
    )
    ft = filtered["page"]["table"]
    assert ft["total_rows"] == 4
    assert ft["unfiltered_rows"] == 6
    assert [row[1]["text"] for row in ft["rows"]] == ["30", "40", "50", "60"]
    assert "df['amount'] >= 30" in filtered["page"]["snippet"]

    # 4. Text substring filter across all columns
    text_filtered = _interactive(
        worker, channel, "namespace.preview", handle=handle, filter_query="north"
    )
    assert text_filtered["page"]["table"]["total_rows"] == 3

    # 5. Original df in namespace was never mutated (strict passivity)
    assert list(worker.namespace["df"]["amount"]) == [10, 50, 30, 20, 40, 60]


# ---------------------------------------------------------------------------
# M3: SQL Cells, HTML/Python Export, Marimo & Jupytext Import
# ---------------------------------------------------------------------------


def test_sql_cells_and_magic_execution() -> None:
    worker, channel = _new_worker()
    _run_worker(
        worker,
        channel,
        "sales = [\n"
        "    {'region': 'East', 'rev': 100},\n"
        "    {'region': 'West', 'rev': 250},\n"
        "    {'region': 'East', 'rev': 150},\n"
        "]\n",
    )
    # First-class SQL cell
    res, events = _run_worker(
        worker,
        channel,
        "SELECT region, SUM(rev) AS total FROM sales GROUP BY region ORDER BY total DESC",
        cell_type="sql",
        output_var="by_region",
    )
    assert res["outcome"] == "ok"
    assert "by_region" in worker.namespace
    assert any(ev.get("kind") == "display" for ev in events)

    # %%sql magic inside a normal code cell
    res2, _ = _run_worker(
        worker,
        channel,
        "%%sql top_row\nSELECT region, total FROM by_region LIMIT 1\n",
    )
    assert res2["outcome"] == "ok"
    assert "top_row" in worker.namespace


def test_html_and_python_export_and_script_import(tmp_path: Path) -> None:
    nb_dir = tmp_path / "nb"
    nb = Notebook(nb_dir)
    nb.init()
    nb.add("markdown", "# Analysis Report\n", "m1")
    nb.add("code", "x = 42\nprint(x)\n", "c1")
    nb.add("sql", "SELECT 1 AS one\n", "s1")

    html_out = tmp_path / "report.html"
    py_out = tmp_path / "script.py"
    export_document(nb, html_out, "html")
    export_document(nb, py_out, "py")

    html_text = html_out.read_text()
    assert "<!DOCTYPE html>" in html_text
    assert "Analysis Report" in html_text
    assert "SELECT 1 AS one" in html_text

    py_text = py_out.read_text()
    assert "# %% [markdown]" in py_text
    assert "x = 42" in py_text
    assert "# %% [sql]" in py_text

    # Import Jupytext # %% script round-trip
    nb2_dir = tmp_path / "nb_jupytext"
    nb2 = Notebook(nb2_dir)
    nb2.init()
    created = nb2.import_script(py_out)
    assert len(created) == 3
    kinds = [c["type"] for c in nb2.read()["cells"]]
    assert kinds == ["markdown", "code", "sql"]

    # Import Marimo @app.cell script
    marimo_py = tmp_path / "app.py"
    marimo_py.write_text(
        "import marimo\n"
        "app = marimo.App()\n\n"
        "@app.cell\n"
        "def _(mo):\n"
        '    mo.md("# Hello Marimo")\n'
        "    return\n\n"
        "@app.cell(hide_code=True, disabled=True)\n"
        "def _():\n"
        "    x = 123\n"
        "    return (x,)\n\n"
        "@app.cell\n"
        "def _(mo):\n"
        '    q = mo.sql("SELECT 1 AS a")\n'
        "    return (q,)\n"
    )
    nb3_dir = tmp_path / "nb_marimo"
    nb3 = Notebook(nb3_dir)
    nb3.init()
    marimo_created = nb3.import_script(marimo_py)
    assert len(marimo_created) == 3
    cells3 = nb3.read()["cells"]
    assert [c["type"] for c in cells3] == ["markdown", "code", "sql"]
    assert cells3[1].get("metadata", {}).get("hide_code") is True
    assert cells3[1].get("metadata", {}).get("disabled") is True
    assert cells3[2].get("metadata", {}).get("output_var") == "q"


# ---------------------------------------------------------------------------
# M4: Expanded Native Widgets + kl.ui, @kl.cache, Helper RPCs, Telemetry
# ---------------------------------------------------------------------------


def test_expanded_widgets_kl_ui_and_form() -> None:
    worker, channel = _new_worker(advanced=True)
    res, events = _run_worker(
        worker,
        channel,
        "import kernelledger as kl\n"
        "fs = kl.ui.slider(0.0, 1.0, value=0.5, step=0.1, label='LR')\n"
        "txt = kl.ui.text('alice', label='Name', placeholder='Enter name')\n"
        "ms = kl.ui.multiselect(['a', 'b', 'c'], value=['a'], label='Tags')\n"
        "st = kl.ui.stat('98.4%', label='Accuracy', delta='+1.2%', caption='val split')\n"
        "co = kl.ui.callout('Model converged', kind='success', title='Done')\n"
        "fm = kl.ui.form({'lr': fs, 'name': txt}, submit_label='Apply')\n"
        "for w in (fs, txt, ms, st, co, fm):\n"
        "    display(w)\n",
    )
    assert res["outcome"] == "ok"
    widget_events = [
        ev["bundle"]["application/vnd.kernelledger.widget+json"]["value"]
        for ev in events
        if ev.get("kind") == "display"
        and "application/vnd.kernelledger.widget+json" in ev.get("bundle", {})
    ]
    kinds = {w["kind"] for w in widget_events}
    assert {"float_slider", "text", "multiselect", "stat", "callout", "form"} <= kinds

    # Update float_slider, multiselect, and form via widget_event
    fs_handle = next(w["handle"] for w in widget_events if w["kind"] == "float_slider")
    worker.widget_event(worker.generation, fs_handle, 0.8)
    assert worker.namespace["fs"].value == pytest.approx(0.8)

    ms_handle = next(w["handle"] for w in widget_events if w["kind"] == "multiselect")
    worker.widget_event(worker.generation, ms_handle, ["a", "c"])
    assert worker.namespace["ms"].value == ["a", "c"]

    form_handle = next(w["handle"] for w in widget_events if w["kind"] == "form")
    worker.widget_event(worker.generation, form_handle, {"lr": 0.25, "name": "bob"})
    assert worker.namespace["fm"].value == {"lr": 0.25, "name": "bob"}


def test_kl_cache_and_persistent_cache(tmp_path: Path) -> None:
    calls = 0

    @cache(maxsize=4)
    def expensive(n: int) -> int:
        nonlocal calls
        calls += 1
        return n * n

    assert expensive(5) == 25
    assert expensive(5) == 25
    assert calls == 1
    assert expensive.cache_info()["hits"] == 1
    expensive.cache_clear()
    assert expensive(5) == 25
    assert calls == 2

    disk_calls = 0

    @persistent_cache(cache_dir=tmp_path / "disk_cache")
    def disk_fn(x: str) -> dict[str, Any]:
        nonlocal disk_calls
        disk_calls += 1
        return {"val": x.upper()}

    assert disk_fn("hello") == {"val": "HELLO"}
    assert disk_fn("hello") == {"val": "HELLO"}
    assert disk_calls == 1

    # Different function name gets isolated key
    disk_calls2 = 0

    @persistent_cache(cache_dir=tmp_path / "disk_cache")
    def disk_fn_other(x: str) -> dict[str, Any]:
        nonlocal disk_calls2
        disk_calls2 += 1
        return {"val": x.upper()}

    assert disk_fn_other("hello") == {"val": "HELLO"}
    assert disk_calls2 == 1


def test_helper_packages_format_files_and_worker_rss(tmp_path: Path) -> None:
    nb = Notebook(tmp_path)
    nb.init()
    (tmp_path / "data.csv").write_text("a,b\n1,2\n")
    (tmp_path / "sub").mkdir()
    (tmp_path / "sub" / "notes.json").write_text(json.dumps({"ok": True}))

    helper = Helper(tmp_path)

    # files.list with loader snippets
    files = helper.dispatch({"method": "files.list"})
    by_path = {item["path"]: item for item in files["items"]}
    assert "data.csv" in by_path
    assert "pd.read_csv('data.csv')" in by_path["data.csv"]["loader_snippet"]
    assert "sub/notes.json" in by_path
    assert "json.loads" in by_path["sub/notes.json"]["loader_snippet"]

    # Path traversal blocked
    with pytest.raises(SourceError):
        helper.dispatch({"method": "files.list", "path": "../"})

    # code.format
    formatted = helper.dispatch({"method": "code.format", "source": "x=1+2\n"})
    assert "source" in formatted
    assert formatted["source"].strip() != ""

    # packages.list
    pkgs = helper.dispatch({"method": "packages.list"})
    assert pkgs["total"] >= 1
    assert any(item["name"].lower() == "pytest" for item in pkgs["items"])

    # packages.install rejects flags/injection
    with pytest.raises(SourceError):
        helper.dispatch({"method": "packages.install", "package": "--index-url http://bad"})

    # Worker status includes RSS telemetry
    worker, channel = _new_worker()
    res, _ = _run_worker(worker, channel, "buf = [0] * 1000\n")
    assert res["outcome"] == "ok"
    assert isinstance(res.get("rss_bytes"), int)
    assert res["rss_bytes"] > 0
    assert isinstance(res.get("memory_delta_bytes"), int)
