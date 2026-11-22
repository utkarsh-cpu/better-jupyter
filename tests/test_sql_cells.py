"""SQL cells: visible names, DuckDB dtype fidelity, referenced-only SQLite copies, output_var."""

from __future__ import annotations

import sys
import time
from pathlib import Path
from typing import Any

import pytest
from _worker_helpers import errors, new_worker, run

from kernelledger import worker as worker_mod
from kernelledger.notebook import Notebook, cell_metadata, normalize_metadata, sql_output_var
from kernelledger.source import SourceService
from kernelledger.worker import SqlEngine, sql_visible_name


@pytest.fixture(params=["duckdb", "sqlite"])
def engine(request: pytest.FixtureRequest, monkeypatch: pytest.MonkeyPatch) -> str:
    if request.param == "duckdb":
        pytest.importorskip("duckdb")
    else:
        monkeypatch.setitem(sys.modules, "duckdb", None)  # import raises ImportError
    return request.param


def test_sql_visible_names_hide_internals_but_expose_single_underscore() -> None:
    for name in ("_sql_df", "_tmp", "df", "sales_2024"):
        assert sql_visible_name(name), name
    for name in ("_", "__", "___", "_i", "_ii", "_iii", "_oh", "_1", "_i3", "In", "Out"):
        assert not sql_visible_name(name), name
    for name in ("__name__", "__private", "_kl_helper", "class", "1abc", 3):
        assert not sql_visible_name(name), name


def test_sql_cells_chain_through_the_default_output_variable(engine: str) -> None:
    worker, channel = new_worker()
    run(worker, channel, "rows = [{'a': 1}, {'a': 2}, {'a': 3}]\n")
    first, events = run(worker, channel, "SELECT a FROM rows WHERE a > 1", "sql")
    assert first["outcome"] == "ok", errors(events)
    assert "_sql_df" in worker.namespace
    chained, events = run(worker, channel, "SELECT COUNT(*) AS n FROM _sql_df", "sql")
    assert chained["outcome"] == "ok", errors(events)
    result = worker.namespace["_sql_df"]
    value = result[0]["n"] if isinstance(result, list) else result["n"][0]
    assert int(getattr(value, "as_py", lambda: value)()) == 2
    # History shortcuts and other internals stay invisible even when they hold tables.
    worker.namespace["_1"] = [{"a": 1}]
    hidden, events = run(worker, channel, "SELECT * FROM _1", "sql")
    assert hidden["outcome"] == "error"


def test_duckdb_results_keep_dtypes_and_library_preference(monkeypatch: pytest.MonkeyPatch) -> None:
    pytest.importorskip("duckdb")
    pd = pytest.importorskip("pandas")
    frame = pd.DataFrame(
        {
            "a": [1, 2, 3],
            "t": pd.to_datetime(["2020-01-01", "2020-01-02", "2020-01-03"]),
            "b": [True, False, True],
            "n": pd.array([1, None, 3], dtype="Int64"),
        }
    )
    # An empty result used to become all-object columns (DataFrame(fetchall())).
    empty = SqlEngine({"df": frame}).run("SELECT * FROM df WHERE a > 100")
    assert len(empty) == 0
    assert str(empty["a"].dtype) == "int64" and str(empty["b"].dtype) == "bool"
    assert pd.api.types.is_datetime64_any_dtype(empty["t"])
    full = SqlEngine({"df": frame}).run("SELECT * FROM df")
    assert str(full["n"].dtype) == "Int64" and full["n"].isna().sum() == 1

    pl = pytest.importorskip("polars")
    pa = pytest.importorskip("pyarrow")
    namespace = {"p": pl.DataFrame({"x": [1, 2]}), "q": pa.table({"x": [3]})}
    monkeypatch.setattr(worker_mod, "_sql_result_library", lambda: "polars")
    as_polars = SqlEngine(namespace).run("SELECT x FROM p UNION ALL SELECT x FROM q ORDER BY x")
    assert type(as_polars) is pl.DataFrame and as_polars["x"].to_list() == [1, 2, 3]
    monkeypatch.setattr(worker_mod, "_sql_result_library", lambda: "pyarrow")
    as_arrow = SqlEngine(namespace).run("SELECT x FROM p")
    assert type(as_arrow) is pa.Table and as_arrow.column("x").to_pylist() == [1, 2]


def test_sqlite_fallback_copies_only_referenced_tables(monkeypatch: pytest.MonkeyPatch) -> None:
    pd = pytest.importorskip("pandas")
    monkeypatch.setitem(sys.modules, "duckdb", None)
    copied: list[str] = []
    original = pd.DataFrame.to_sql

    def spy(self: Any, name: str, *args: Any, **kwargs: Any) -> Any:
        copied.append(name)
        return original(self, name, *args, **kwargs)

    monkeypatch.setattr(pd.DataFrame, "to_sql", spy)
    namespace: dict[str, Any] = {
        "small": pd.DataFrame({"v": [1, 2, 3]}),
        "big": pd.DataFrame({"x": range(200_000)}),
        "other": pd.DataFrame({"y": [1]}),
    }
    started = time.perf_counter()
    result = SqlEngine(namespace).run("SELECT SUM(v) AS s FROM small -- not big")
    assert int(result["s"].iloc[0]) == 6
    assert copied == ["small"]
    assert time.perf_counter() - started < 2

    # DAG refs from the supervisor are honoured; a missing table retries with that binding.
    copied.clear()
    engine = SqlEngine(namespace)
    assert int(engine.run("SELECT COUNT(*) AS c FROM other", refs=[])["c"].iloc[0]) == 1
    assert copied == ["other"] and engine.registered == ["other"]

    # The worker forwards the message's `sql_refs`.
    worker, channel = new_worker()
    worker.namespace.update(namespace)
    copied.clear()
    idle, events = run(worker, channel, "SELECT v FROM small", "sql", sql_refs=["small"])
    assert idle["outcome"] == "ok", errors(events)
    assert copied == ["small"]


def test_sql_display_reports_total_rows_of_large_results(engine: str) -> None:
    worker, channel = new_worker()
    run(worker, channel, "rows = [{'i': i} for i in range(250)]\n")
    idle, events = run(worker, channel, "SELECT i FROM rows", "sql", output_var="out")
    assert idle["outcome"] == "ok", errors(events)
    table = next(
        e["bundle"]["application/vnd.kernelledger.table+json"]["value"]
        for e in events
        if e["kind"] == "display"
    )
    assert table["total_rows"] == 250 and len(table["rows"]) == 100 and table["truncated"]


def test_metadata_reader_tolerates_nested_payloads() -> None:
    assert sql_output_var({"metadata": {"output_var": "res"}}) == "res"
    nested = {"metadata": {"metadata": {"output_var": "res"}, "attachments": ["a.png"]}}
    assert sql_output_var(nested) == "res"
    assert cell_metadata(nested) == {"output_var": "res"}
    assert sql_output_var({"metadata": {"output_var": "not valid"}}) == "_sql_df"
    assert sql_output_var({}) == "_sql_df"
    flat = normalize_metadata(
        {
            "metadata": {"output_var": "old", "hide_code": True},
            "output_var": "new",
            "attachments": ["x.png"],
            "attachment_mimes": {"x.png": "image/png"},
            "id": "c1",
        }
    )
    assert flat == {"output_var": "new", "hide_code": True}


def test_metadata_mutation_flattens_nesting_and_drops_cell_keys(tmp_path: Path) -> None:
    notebook = Notebook(tmp_path)
    notebook.init()
    notebook.add("sql", "SELECT 1 AS one\n", "s1")
    service = SourceService(tmp_path)
    snapshot = service.read()
    revision = next(c["revision"] for c in snapshot["cells"] if c["id"] == "s1")
    service.mutate(
        {
            "action": "metadata",
            "cell_id": "s1",
            # The malformed shape the desktop sent: the whole cell copied into metadata.
            "metadata": {
                "metadata": {"output_var": "res"},
                "attachments": [],
                "attachment_mimes": {},
                "disabled": True,
            },
            "manifest_revision": snapshot["manifest_revision"],
            "cell_revisions": {"s1": revision},
        }
    )
    cell = notebook.cell("s1")
    assert cell["metadata"] == {"output_var": "res", "disabled": True}
    assert "attachments" not in cell and "attachment_mimes" not in cell
    assert service.execution_sources(["s1"])[0]["metadata"]["output_var"] == "res"


def test_supervisor_reads_output_var_from_cell_metadata(tmp_path: Path) -> None:
    from kernelledger.cli import alive, ensure_supervisor, rpc

    notebook = Notebook(tmp_path)
    notebook.init()
    notebook.add("code", "rows = [{'a': 1}, {'a': 2}]\n", "c1")
    # Stored with the legacy nested shape; the reader must still find `res`.
    notebook.add("sql", "SELECT a FROM rows\n", "s1", {"metadata": {"output_var": "res"}})
    notebook.add("code", "assert len(res) == 2, res\n", "c2")
    ensure_supervisor(tmp_path)
    try:
        for cid in ("c1", "s1", "c2"):
            result = rpc(tmp_path, {"command": "run", "cell_id": cid})
            assert result["status"] == "ok", (cid, result)
    finally:
        if alive(tmp_path):
            rpc(tmp_path, {"command": "stop"})
        for _ in range(100):
            if not (tmp_path / ".kl/supervisor.json").exists():
                break
            time.sleep(0.02)


def test_notebook_schema_models_sql_cells_and_metadata(tmp_path: Path) -> None:
    import json

    jsonschema = pytest.importorskip("jsonschema")
    schema = json.loads(
        (Path(__file__).resolve().parents[1] / "schemas/notebook.schema.json").read_text()
    )
    notebook = Notebook(tmp_path)
    notebook.init()
    notebook.add("sql", "SELECT 1\n", "s1", {"output_var": "res", "disabled": False})
    notebook.add("code", "x = 1\n", "c1", {"hide_code": True})
    jsonschema.validate(notebook.read(), schema)
    bad = notebook.read()
    bad["cells"][0]["metadata"]["output_var"] = "not an identifier"
    with pytest.raises(jsonschema.ValidationError):
        jsonschema.validate(bad, schema)
