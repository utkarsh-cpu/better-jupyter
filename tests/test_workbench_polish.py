"""Source-history safety and bounded desktop search regressions."""

import json

import pytest

from kernelledger.blobs import BlobStore
from kernelledger.desktop.helper import Helper
from kernelledger.ledger import Ledger
from kernelledger.notebook import Notebook
from kernelledger.source import SourceError, SourceService


def mutate(service, action, **fields):
    current = service.read()
    return service.mutate(
        {
            "action": action,
            "manifest_revision": current["manifest_revision"],
            "cell_revisions": {c["id"]: c["revision"] for c in current["cells"]},
            **fields,
        }
    )


def source_files(root):
    return {
        p.relative_to(root).as_posix(): p.read_bytes()
        for p in root.rglob("*")
        if p.is_file() and ".kl" not in p.relative_to(root).parts and p.name != ".gitignore"
    }


@pytest.mark.parametrize(
    "action,fields",
    [
        ("add", {"type": "code", "new_id": "stable", "index": 1}),
        ("duplicate", {"cell_id": "a", "new_id": "stable"}),
        ("delete", {"cell_id": "a"}),
        ("move", {"cell_id": "a", "index": 2}),
        ("split", {"cell_id": "a", "offset": 4, "new_id": "stable"}),
        ("merge", {"cell_id": "a", "other_id": "b"}),
        ("convert", {"cell_id": "a", "type": "raw"}),
    ],
)
def test_structure_undo_redo_restores_exact_files_and_ids(tmp_path, action, fields):
    notebook = Notebook(tmp_path)
    notebook.init()
    notebook.add("code", "x=1\ny=2", "a")
    notebook.add("code", "z=3", "b")
    notebook.add("code", "w=4", "c")
    service = SourceService(tmp_path)
    before = source_files(tmp_path)
    result = mutate(service, action, **fields)
    assert result["history"]["undo"]
    after = source_files(tmp_path)
    assert before != after
    # Undo survives a helper restart; no generated replacement cell IDs.
    service = SourceService(tmp_path)
    mutate(service, "undo")
    assert source_files(tmp_path) == before
    mutate(service, "redo")
    assert source_files(tmp_path) == after


def test_undo_restores_attachments_and_rejects_newer_source(tmp_path):
    notebook = Notebook(tmp_path)
    notebook.init()
    notebook.add("markdown", "![plot](attachment:plot.png)", "a")
    service = SourceService(tmp_path)
    payload = tmp_path.parent / (tmp_path.name + "-plot.png")
    payload.write_bytes(b"recorded attachment")
    mutate(service, "attach", cell_id="a", filename="plot.png", file_path=str(payload))
    before = source_files(tmp_path)
    mutate(service, "delete", cell_id="a")
    mutate(service, "undo")
    assert source_files(tmp_path) == before
    (tmp_path / "cells/a.md").write_text("newer external edit")
    with pytest.raises(SourceError, match="Source changed"):
        mutate(service, "redo")
    assert (tmp_path / "cells/a.md").read_text() == "newer external edit"


def test_move_undo_preserves_unrelated_edits_and_new_edits_clear_redo(tmp_path):
    notebook = Notebook(tmp_path)
    notebook.init()
    notebook.add("code", "a=1", "a")
    notebook.add("code", "b=2", "b")
    service = SourceService(tmp_path)
    mutate(service, "move", cell_id="a", index=1)
    (tmp_path / "cells/b.py").write_text("b=999")
    mutate(service, "undo")
    assert notebook.source(notebook.cell("b")) == "b=999"
    assert [c["id"] for c in notebook.read()["cells"]] == ["a", "b"]
    mutate(service, "edit", cell_id="b", source="b=1000")
    assert not service.read()["history"]["redo"]


def test_undo_budget_and_tampered_history_cannot_escape_notebook(tmp_path):
    Notebook(tmp_path).init()
    service = SourceService(tmp_path)
    for i in range(24):
        mutate(service, "add", type="code", new_id=f"cell{i}")
    index = tmp_path / ".kl/source-history/index.json"
    state = json.loads(index.read_text())
    assert len(state["undo"]) == 20
    assert len(list(index.parent.glob("*.json"))) == 21
    entry = index.parent / (state["undo"][-1]["id"] + ".json")
    entry.write_text(json.dumps({"paths": {"../escape": {"before": None, "after": None}}}))
    with pytest.raises(SourceError):
        mutate(service, "undo")
    assert not (tmp_path.parent / "escape").exists()


def test_search_pages_past_empty_matches_and_limits_preview_memory(tmp_path):
    notebook = Notebook(tmp_path)
    notebook.init()
    ledger = Ledger(tmp_path / ".kl/ledger.sqlite")
    blobs = BlobStore(tmp_path / ".kl")
    ledger.db.execute(
        "INSERT INTO sessions(session_id,notebook_id,state,start,environment_json,supervisor_json) VALUES ('s','n','ready','2026-11-08T00:00:00Z','{}','{}')"
    )
    for i in range(240):
        text = "needle\n" + "x" * 6000 if i == 225 else f"value={i}"
        digest = blobs.put(text.encode())
        ledger.db.execute(
            "INSERT INTO executions VALUES (?,?,?,?,?,?,?,?)",
            (f"exec{i}", "s", i + 1, "a", digest, "ok", "2026-11-08T00:00:00Z", None),
        )
    ledger.db.commit()
    helper = Helper(tmp_path)
    request = {"method": "executions.list", "query": "needle cell:a outcome:ok", "page_size": 100}
    first = helper.dispatch(request)
    assert first["items"] == [] and first["cursor"] and first["scanned"] == 100
    second = helper.dispatch({**request, "cursor": first["cursor"]})
    assert second["items"] == [] and second["cursor"]
    third = helper.dispatch({**request, "cursor": second["cursor"]})
    assert [item["exec_id"] for item in third["items"]] == ["exec225"]
    assert third["cursor"] is None and third["scanned"] == 40
    assert third["items"][0]["source_preview_truncated"]
    assert len(third["items"][0]["source"].encode()) <= 1024
    with pytest.raises(SourceError):
        helper.dispatch({"method": "executions.list", "query": "x" * 513})
