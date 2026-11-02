"""Session overlays are paged and derived; raw execution evidence never changes."""

import json
import os

import pytest

from kernelledger.blobs import BlobStore
from kernelledger.desktop.contracts import validate
from kernelledger.desktop.helper import Helper
from kernelledger.display import apply_update, session_views
from kernelledger.ledger import Ledger, fold
from kernelledger.notebook import Notebook
from kernelledger.sharing import notebook_outputs, output_view
from kernelledger.source import SourceError

pytest_plugins = ["test_shell"]


def test_runtime_updates_are_shared_but_do_not_cross_worker_replacement(notebook):
    from kernelledger.cli import ensure_supervisor, rpc

    notebook.add("code", "display('before', display_id='shared')", "target")
    ensure_supervisor(notebook.root)
    first = rpc(notebook.root, {"command": "run", "cell_id": "target"})
    assert first["status"] == "ok"
    session = rpc(notebook.root, {"command": "status"})["session_id"]
    update = rpc(
        notebook.root,
        {"command": "run", "source": "update_display('after', 'shared')", "origin": "console"},
    )
    assert update["status"] == "ok"
    helper = Helper(notebook.root)
    recorded = helper.dispatch({"method": "fold.get", "exec_id": first["exec_id"]})
    assert recorded["outputs"][0]["bundle"]["text/plain"]["value"] == "'before'"
    path = notebook.root / ".kl/ledger.sqlite"
    ledger = Ledger(path, readonly=True)
    try:
        live = list(session_views(ledger, session))[0]
        assert live["outputs"][0]["bundle"]["text/plain"]["value"] == "'after'"
        assert live["outputs"][0]["display_update_origin"]["exec_id"] == update["exec_id"]
    finally:
        ledger.close()
    rpc(notebook.root, {"command": "restart"})
    replacement = rpc(notebook.root, {"command": "status"})["session_id"]
    assert replacement != session
    assert (
        rpc(
            notebook.root, {"command": "run", "source": "update_display('wrong worker', 'shared')"}
        )["status"]
        == "ok"
    )
    ledger = Ledger(path, readonly=True)
    try:
        assert (
            list(session_views(ledger, session))[0]["outputs"][0]["bundle"]["text/plain"]["value"]
            == "'after'"
        )
    finally:
        ledger.close()


def seed(tmp_path):
    notebook = Notebook(tmp_path)
    notebook.init()
    blobs = BlobStore(tmp_path / ".kl")
    source = blobs.put(b"# recorded source\n")
    ledger = Ledger(tmp_path / ".kl/ledger.sqlite")
    session = ledger.session(notebook.read()["id"], {})
    return notebook, blobs, ledger, session, source


def display(value, display_id="shared"):
    return {"display_id": display_id, "bundle": {"text/plain": {"value": value}}, "metadata": {}}


def test_session_view_broadcasts_without_rewriting_history(tmp_path):
    notebook, blobs, ledger, session, source = seed(tmp_path)
    try:
        a = ledger.execution(session, "a", source)
        ledger.event(a, "display", display("first"))
        ledger.event(a, "display", display("second"))
        b = ledger.execution(session, "b", source)
        ledger.event(b, "display", display("other cell"))
        c = ledger.execution(session, None, source)
        ledger.event(c, "display_update", display("updated"))
        before = [dict(row) for row in ledger.rows("SELECT * FROM events ORDER BY rowid")]
        views = list(session_views(ledger, session))
        assert [
            [event["bundle"]["text/plain"]["value"] for event in view["outputs"]] for view in views
        ] == [["updated", "updated"], ["updated"]]
        assert views[0]["outputs"][0]["display_update_origin"]["exec_id"] == c
        assert views[0]["outputs"][0]["seq"] == 1
        original = output_view(notebook, ledger, blobs, a)
        assert [event["bundle"]["text/plain"]["value"] for event in original["outputs"]] == [
            "first",
            "second",
        ]
        outputs = notebook_outputs(ledger, blobs, session)
        assert outputs["a"]["outputs"][0]["data"]["text/plain"] == "updated"
        assert (
            outputs["a"]["outputs"][0]["metadata"]["kernelledger_display_update_origin"]["exec_id"]
            == c
        )
        assert before == [dict(row) for row in ledger.rows("SELECT * FROM events ORDER BY rowid")]
        assert not (tmp_path / ".kl/supervisor.json").exists()
    finally:
        ledger.close()


def test_clear_wait_rerun_creation_order_and_sessions(tmp_path):
    notebook, _, ledger, session, source = seed(tmp_path)
    try:
        a = ledger.execution(session, "a", source)
        ledger.event(a, "display", display("gone"))
        ledger.event(a, "clear", {"wait": True})
        ledger.event(a, "display_update", display("not a new display"))
        assert fold(ledger.events(a)) == []
        b = ledger.execution(session, "b", source)
        ledger.event(b, "display", display("before"))
        ledger.event(b, "display_update", display("middle"))
        ledger.event(b, "display", display("after"))
        views = {view["cell_id"]: view for view in session_views(ledger, session)}
        assert views["a"]["outputs"] == []
        assert [event["bundle"]["text/plain"]["value"] for event in views["b"]["outputs"]] == [
            "middle",
            "after",
        ]
        rerun = ledger.execution(session, "b", source)
        ledger.event(rerun, "display", display("new target"))
        other_session = ledger.session(notebook.read()["id"], {})
        other = ledger.execution(other_session, "c", source)
        ledger.event(other, "display_update", display("different worker"))
        current = list(session_views(ledger, session))[-1]
        assert current["exec_id"] == rerun
        assert current["outputs"][0]["bundle"]["text/plain"]["value"] == "new target"
        update = {"session_id": other_session, "ordinal": 100, "seq": 1, "event": display("wrong")}
        assert not apply_update(current, update)
    finally:
        ledger.close()


def test_export_folds_one_read_snapshot_during_a_live_append(tmp_path, monkeypatch):
    _, _, writer, session, source = seed(tmp_path)
    original = writer.execution(session, "a", source)
    writer.event(original, "display", display("before"))
    updater = writer.execution(session, None, source)
    reader = Ledger(tmp_path / ".kl/ledger.sqlite", readonly=True)
    read_rows = reader.rows
    appended = False

    def rows(sql, args=()):
        nonlocal appended
        result = read_rows(sql, args)
        if sql.startswith("SELECT * FROM executions") and not appended:
            appended = True
            writer.event(updater, "display_update", display("after"))
        return result

    monkeypatch.setattr(reader, "rows", rows)
    try:
        views = list(session_views(reader, session))
        assert appended
        assert views[0]["outputs"][0]["bundle"]["text/plain"]["value"] == "before"
        assert list(session_views(writer, session))[0]["outputs"][0]["bundle"]["text/plain"]["value"] == "after"
    finally:
        reader.close()
        writer.close()


def test_replay_scan_payload_bounds_and_snapshot(tmp_path):
    _, _, ledger, session, source = seed(tmp_path)
    try:
        execution = ledger.execution(session, "a", source)
        for _ in range(130):
            ledger.event(execution, "stdout", {"text": "x" * 10000}, commit=False)
        for index in range(20):
            ledger.event(execution, "display_update", display(str(index)), commit=False)
        ledger.event(execution, "display_update", display("x" * 70000), commit=False)
        ledger.event(execution, "display_update", display("bad id", {"bad": "id"}), commit=False)
        ledger.event(execution, "display_update", display("unicode", "λ" * 128), commit=False)
        ledger.db.commit()
        helper = Helper(tmp_path)
        through = helper.dispatch({"method": "executions.watermark"})["event_rowid"]
        page = helper.dispatch(
            {"method": "display.updates", "after_rowid": 0, "through_rowid": through}
        )
        assert page["after_rowid"] == 128 and page["items"] == [] and page["more"]
        items = []
        while page["more"]:
            page = helper.dispatch(
                {
                    "method": "display.updates",
                    "after_rowid": page["after_rowid"],
                    "through_rowid": through,
                }
            )
            assert len(page["items"]) <= 16 and len(json.dumps(page).encode()) < 262144
            items.extend(page["items"])
        assert len(items) == 23
        assert items[20]["omitted"] == "oversized_payload"
        assert items[21]["omitted"] == "invalid_display_id"
        assert items[22]["event"]["display_id"] == "λ" * 128
        ledger.event(execution, "display_update", display("later"))
        assert (
            helper.dispatch(
                {"method": "display.updates", "after_rowid": through, "through_rowid": through}
            )["items"]
            == []
        )
        with pytest.raises(ValueError, match="Cannot fold"):
            list(session_views(ledger, session))
        for key in ("after_rowid", "through_rowid"):
            for invalid in (-1, False, 1.0, "1", 2**63):
                with pytest.raises(SourceError):
                    validate(
                        {
                            "method": "display.updates",
                            "after_rowid": 0,
                            "through_rowid": through,
                            key: invalid,
                        },
                        helper=True,
                    )
    finally:
        ledger.close()


def test_native_display_replay_matches_python(tmp_path):
    if not os.environ.get("KL_NATIVE_DAEMON"):
        pytest.skip("Requires an explicit native daemon build")
    from test_native_helper import NativeHelper, assert_match

    _, _, ledger, session, source = seed(tmp_path)
    for _ in range(17):
        ledger.event(ledger.execution(session, "a", source), "display_update", display("x" * 60000))
    execution = ledger.execution(session, "a", source)
    for payload in (
        "[]",
        '{"display_id":"x", "bundle":NaN}',
        '{"display_id":"\\ud800"}',
        "{}",
        '{"display_id":"x", "value":1e999}',
        '{"display_id":"x", "value":' + "9" * 400 + "}",
        '{"display_id":"x", "value":' + "[" * 65 + "0" + "]" * 65 + "}",
        json.dumps({"display_id": "x", "value": 'quoted [ { \\" value'}),
    ):
        ledger.db.execute(
            "INSERT INTO events VALUES (?,?,?,?,?,?)",
            (
                execution,
                ledger.db.execute(
                    "SELECT COALESCE(MAX(seq),0)+1 FROM events WHERE exec_id=?", (execution,)
                ).fetchone()[0],
                "display_update",
                "2026-10-26",
                payload,
                None,
            ),
        )
    ledger.event(execution, "display_update", display("oversized" * 10000))
    ledger.event(execution, "display_update", display("bounded metadata"))
    ledger.db.execute(
        "UPDATE events SET timestamp=? WHERE rowid=(SELECT MAX(rowid) FROM events)", ("x" * 100000,)
    )
    ledger.event(execution, "x" * 100000, {"text": "unrecognized event kinds are not replay data"})
    ledger.db.commit()
    through = Helper(tmp_path).dispatch({"method": "executions.watermark"})["event_rowid"]
    ledger.close()
    native = NativeHelper(tmp_path)
    try:
        after = 0
        while after < through:
            assert_match(
                native, tmp_path, "display.updates", after_rowid=after, through_rowid=through
            )
            page = Helper(tmp_path).dispatch(
                {"method": "display.updates", "after_rowid": after, "through_rowid": through}
            )
            after = page["after_rowid"]
        assert_match(
            native,
            tmp_path,
            "display.updates",
            after_rowid=0,
            through_rowid=through,
            session_id="missing",
        )
        for invalid in (False, -1, 1.0, "1", 2**63):
            assert_match(
                native, tmp_path, "display.updates", after_rowid=invalid, through_rowid=through
            )
    finally:
        native.close()
