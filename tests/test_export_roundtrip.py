"""Content-level `.py` / `.ipynb` round trips and code hiding in document exports."""

from __future__ import annotations

import json
import os
import subprocess
import sys
import time
from pathlib import Path
from typing import Any

import pytest

from kernelledger.documents import export_document, notebook_blocks
from kernelledger.notebook import Notebook, sql_output_var

ROOT = Path(__file__).resolve().parents[1]


def _notebook(path: Path) -> Notebook:
    notebook = Notebook(path)
    notebook.init()
    notebook.add("markdown", "# Report\n\nSome *text*.\n", "m1")
    notebook.add("code", "rows = [{'a': 1}, {'a': 2}, {'a': 3}]\n", "c1")
    notebook.add(
        "sql",
        "SELECT a, 'it''s' AS s\nFROM rows -- \"quoted\" comment\nWHERE a > 1\n",
        "s1",
        {"output_var": "big_rows"},
    )
    notebook.add("code", "total = len(big_rows)\nprint(total)\n", "c2")
    notebook.add(
        "code", "raise SystemExit('disabled cells must not run')\n", "d1", {"disabled": True}
    )
    notebook.add("sql", "SELECT 1\n", "d2", {"disabled": True, "output_var": "never"})
    notebook.add("code", "secret = 'hidden code'\n", "h1", {"hide_code": True})
    notebook.add("raw", "raw text\n\n  indented\n", "r1")
    return notebook


def _cells(notebook: Notebook) -> list[tuple[str, str, dict[str, Any]]]:
    return [
        (c["type"], notebook.source(c), c.get("metadata", {})) for c in notebook.read()["cells"]
    ]


def test_python_export_runs_sql_and_round_trips_cells(tmp_path: Path) -> None:
    original = _notebook(tmp_path / "nb")
    script = tmp_path / "script.py"
    export_document(original, str(script), "py")
    text = script.read_text()
    assert "# %% [sql] big_rows id=s1" in text
    assert "# %% id=d1 disabled=true" in text
    assert "# raise SystemExit" in text  # disabled code is commented out

    # The script is runnable: the SQL cell binds its output variable (DuckDB or SQLite).
    env = {**os.environ, "PYTHONPATH": str(ROOT / "src")}
    completed = subprocess.run(
        [sys.executable, str(script)], capture_output=True, text=True, env=env, timeout=120
    )
    assert completed.returncode == 0, completed.stderr
    assert completed.stdout.strip() == "2"

    reimported = Notebook(tmp_path / "back")
    reimported.init()
    reimported.import_script(script)
    assert [c["id"] for c in reimported.read()["cells"]] == [
        c["id"] for c in original.read()["cells"]
    ]
    assert _cells(reimported) == _cells(original)
    sql = reimported.cell("s1")
    assert sql["type"] == "sql" and sql_output_var(sql) == "big_rows"
    assert reimported.cell("d1")["metadata"] == {"disabled": True}


def test_python_export_falls_back_to_repr_for_awkward_sql(tmp_path: Path) -> None:
    notebook = Notebook(tmp_path / "nb")
    notebook.init()
    notebook.add("sql", "SELECT 'a\\b' AS x, '\"\"\"' AS y\n", "s1", {"output_var": "odd"})
    script = tmp_path / "odd.py"
    export_document(notebook, str(script), "py")
    back = Notebook(tmp_path / "back")
    back.init()
    back.import_script(script)
    assert _cells(back) == _cells(notebook)


def test_legacy_python_export_markers_still_import(tmp_path: Path) -> None:
    script = tmp_path / "legacy.py"
    script.write_text(
        "# %% [sql] res\n# SQL query bound to `res`\n_kl_sql = 'SELECT 1 AS one'\n\n"
        "# %% id=c1\n# [disabled]\n# x = 1\n"
    )
    notebook = Notebook(tmp_path / "nb")
    notebook.init()
    notebook.import_script(script)
    sql, code = notebook.read()["cells"]
    assert sql["type"] == "sql" and notebook.source(sql) == "SELECT 1 AS one\n"
    assert sql["metadata"] == {"output_var": "res"}
    assert notebook.source(code) == "x = 1\n" and code["metadata"] == {"disabled": True}


def test_ipynb_sql_cells_round_trip_through_metadata(tmp_path: Path) -> None:
    original = _notebook(tmp_path / "nb")
    path = tmp_path / "out.ipynb"
    original.export_ipynb(path)
    exported = json.loads(path.read_text())
    sql_item = next(c for c in exported["cells"] if c["id"] == "s1")
    assert sql_item["cell_type"] == "code"
    # Plain `%%sql`: jupysql would read `%%sql big_rows` as a connection string.
    assert sql_item["source"].splitlines()[0] == "%%sql"
    assert sql_item["metadata"]["kernelledger"] == {"type": "sql", "output_var": "big_rows"}

    back = Notebook(tmp_path / "back")
    back.init()
    back.import_ipynb(path)
    cells = {c["id"]: c for c in back.read()["cells"]}
    assert cells["s1"]["type"] == "sql"
    assert back.source(cells["s1"]) == original.source(original.cell("s1"))
    assert cells["s1"]["metadata"] == {"output_var": "big_rows"}
    assert cells["d2"]["type"] == "sql" and cells["d2"]["metadata"] == {
        "disabled": True,
        "output_var": "never",
    }
    assert cells["h1"]["metadata"] == {"hide_code": True}


@pytest.mark.parametrize("fmt", ["html", "pdf", "docx"])
def test_document_exports_honor_hide_code_and_report_mode(tmp_path: Path, fmt: str) -> None:
    notebook = _notebook(tmp_path / "nb")
    texts = [b.text for b in notebook_blocks(notebook)]
    assert not any("hidden code" in t for t in texts)
    assert any("SELECT a" in t for t in texts)
    assert any("disabled" in t for t in texts)
    report = [b.text for b in notebook_blocks(notebook, report=True)]
    assert not any(b.kind == "code" for b in notebook_blocks(notebook, report=True))
    assert "Some text." in report
    out = tmp_path / f"report.{fmt}"
    export_document(notebook, str(out), fmt, report=True)
    assert out.stat().st_size > 0
    if fmt == "html":
        html = out.read_text()
        assert "SELECT a" not in html and "rows = [" not in html
        assert "<h2>Report</h2>" in html and "<p>Some text.</p>" in html


def test_disabled_cell_outputs_are_omitted_from_documents(tmp_path: Path) -> None:
    from kernelledger.blobs import BlobStore
    from kernelledger.cli import alive, ensure_supervisor, rpc
    from kernelledger.ledger import Ledger
    from kernelledger.source import SourceService

    notebook = Notebook(tmp_path)
    notebook.init()
    notebook.add("code", "print('stale <output> & more')\n", "c1")
    ensure_supervisor(tmp_path)
    try:
        assert rpc(tmp_path, {"command": "run", "cell_id": "c1"})["status"] == "ok"
    finally:
        if alive(tmp_path):
            rpc(tmp_path, {"command": "stop"})
        for _ in range(200):
            if not (tmp_path / ".kl/supervisor.json").exists():
                break
            time.sleep(0.02)
    ledger = Ledger(tmp_path / ".kl/ledger.sqlite", readonly=True)
    blobs = BlobStore(tmp_path / ".kl", readonly=True)
    try:
        shown = [b.text for b in notebook_blocks(notebook, ledger, blobs)]
        assert any("stale <output> & more" in t for t in shown)
        out = tmp_path / "live.html"
        export_document(notebook, str(out), "html", ledger, blobs)
        assert "stale &lt;output&gt; &amp; more" in out.read_text()  # escaping intact

        service = SourceService(tmp_path)
        snapshot = service.read()
        service.mutate(
            {
                "action": "metadata",
                "cell_id": "c1",
                "metadata": {"disabled": True},
                "manifest_revision": snapshot["manifest_revision"],
                "cell_revisions": {"c1": snapshot["cells"][0]["revision"]},
            }
        )
        hidden = notebook_blocks(notebook, ledger, blobs)
        assert not any("stale <output>" in b.text for b in hidden if b.kind == "output")
        assert any("disabled" in b.text and b.kind == "warning" for b in hidden)
    finally:
        ledger.close()
