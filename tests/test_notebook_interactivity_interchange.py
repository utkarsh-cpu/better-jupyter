"""Interchange keeps data, reports unknown output, and never executes an import."""
import base64
import json

from kernelledger.blobs import BlobStore
from kernelledger.ledger import Ledger
from kernelledger.notebook import Notebook
from kernelledger.sharing import mutate, notebook_outputs


def test_roundtrip_unknown_fields_media_and_bounded_imported_outputs(tmp_path):
    notebook = Notebook(tmp_path / "notebook")
    notebook.init()
    audio = b"RIFF-test-media-attachment"
    unknown = {"output_type": "vendor_unknown", "payload": "private" * 10000}
    metadata = {"vendor": "m" * 70000}
    original = {
        "nbformat": 4, "nbformat_minor": 5, "metadata": {}, "vendor_top": {"keep": True},
        "cells": [{"id": "example", "cell_type": "code", "source": "raise RuntimeError('must never run on import')",
                   "metadata": {}, "execution_count": 12, "vendor_cell": [1, 2], "outputs": [
                       {"output_type": "stream", "name": "stdout", "text": "x" * 100000},
                       {"output_type": "display_data", "data": {"audio/wav": base64.b64encode(audio).decode(), "text/plain": "Audio"}, "metadata": {}},
                       {"output_type": "display_data", "data": {"application/vnd.test+json": [1, {"a": 2}], "text/plain": "JSON"}, "metadata": metadata},
                       unknown,
                   ]}]
    }
    path = tmp_path / "input.ipynb"
    path.write_text(json.dumps(original), encoding="utf-8")
    rows = notebook.import_ipynb(path)
    ledger = Ledger(notebook.root / ".kl/ledger.sqlite")
    blobs = BlobStore(notebook.root / ".kl")
    try:
        result = mutate("import_outputs", {"rows": rows}, notebook, ledger, blobs)
        execution = ledger.rows("SELECT * FROM executions")[0]
        events = ledger.events(execution["exec_id"])
        assert events[0].get("blob") and "text" not in events[0]
        assert events[1]["imported"] is True
        outputs = notebook_outputs(ledger, blobs, result["session_id"])
        destination = tmp_path / "output.ipynb"
        notebook.export_ipynb(destination, outputs)
        restored = json.loads(destination.read_text(encoding="utf-8"))
        assert restored["vendor_top"] == original["vendor_top"]
        cell = restored["cells"][0]
        assert cell["vendor_cell"] == [1, 2]
        assert cell["metadata"]["kernelledger"]["imported_execution_count"] == 12
        assert cell["outputs"][0]["text"] == "x" * 100000
        assert base64.b64decode(cell["outputs"][1]["data"]["audio/wav"]) == audio
        assert cell["outputs"][2]["data"]["application/vnd.test+json"] == [1, {"a": 2}]
        assert cell["outputs"][2]["metadata"] == metadata
        assert cell["outputs"][3]["data"]["application/vnd.kernelledger.imported-output+json"] == unknown
        assert not (notebook.root / ".kl/supervisor.json").exists()
    finally:
        ledger.close()
