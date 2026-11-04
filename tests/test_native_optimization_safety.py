"""Integrity and namespace safety through the optimized checkpoint path."""

import json
import platform

import pytest
from test_native_supervisor import native_runtime as native_runtime

from kernelledger.blobs import BlobStore
from kernelledger.cli import rpc
from kernelledger.ledger import Ledger


@pytest.mark.parametrize("malformed", ["field", "reference", "kind", "base64", "binding"])
def test_native_exact_validator_rejects_bad_graph_before_replacing_worker(
    native_runtime, malformed
):
    notebook, _ = native_runtime
    original = rpc(notebook.root, {"command": "run", "source": "keep = 42"})
    assert rpc(notebook.root, {"command": "checkpoint"})["status"] == "complete"
    manifest = {
        "schema": 1,
        "bindings": {"x": {"k": "int", "v": "7"}},
        "objects": {},
        "codecs": {},
        "excluded": [],
        "notes": [],
        "python": platform.python_version(),
        "platform": platform.platform(),
    }
    if malformed == "field":
        manifest.pop("codecs")
    elif malformed == "reference":
        manifest["bindings"]["x"] = {"k": "ref", "id": "missing"}
    elif malformed == "kind":
        manifest["bindings"]["x"] = {"k": "unknown"}
    elif malformed == "base64":
        manifest["bindings"]["x"] = {"k": "bytes", "b64": "!invalid!"}
    else:
        manifest["bindings"]["__builtins__"] = {"k": "int", "v": "1"}
    digest = BlobStore(notebook.root / ".kl").put(json.dumps(manifest).encode())
    ledger = Ledger(notebook.root / ".kl/ledger.sqlite")
    try:
        ledger.write(
            "INSERT INTO checkpoints VALUES (?,?,?,?,?,?,?)",
            (
                "invalid-graph",
                original["session_id"],
                original["exec_id"],
                "complete",
                digest,
                "now",
                "local",
            ),
        )
    finally:
        ledger.close()
    with pytest.raises(Exception, match="checkpoint graph validation failed"):
        rpc(notebook.root, {"command": "restore", "checkpoint_id": "invalid-graph"})
    assert rpc(notebook.root, {"command": "status"})["session_id"] == original["session_id"]
    assert rpc(notebook.root, {"command": "run", "source": "assert keep == 42"})["status"] == "ok"


def test_native_restore_rehashes_payload_after_a_successful_checkpoint(native_runtime):
    notebook, _ = native_runtime
    original = rpc(
        notebook.root,
        {"command": "run", "source": "payload = b'x' * 131072\nalias = payload\nkeep = 42"},
    )
    checkpoint = rpc(notebook.root, {"command": "checkpoint"})
    ledger = Ledger(notebook.root / ".kl/ledger.sqlite", readonly=True)
    try:
        digest = ledger.rows(
            "SELECT manifest_sha256 FROM checkpoints WHERE checkpoint_id=?",
            (checkpoint["checkpoint_id"],),
        )[0]["manifest_sha256"]
    finally:
        ledger.close()
    manifest = json.loads(BlobStore(notebook.root / ".kl").get(digest))
    blob = manifest["bindings"]["payload"]["blob"]
    (notebook.root / ".kl/blobs" / blob).write_bytes(b"corrupt")
    with pytest.raises(Exception, match="hash mismatch"):
        rpc(notebook.root, {"command": "restore", "checkpoint_id": checkpoint["checkpoint_id"]})
    assert rpc(notebook.root, {"command": "status"})["session_id"] == original["session_id"]
    assert (
        rpc(notebook.root, {"command": "run", "source": "assert keep == 42 and payload is alias"})[
            "status"
        ]
        == "ok"
    )


@pytest.mark.parametrize("native_runtime", [{"env": {"KERNELLEDGER_PROFILE": "1"}}], indirect=True)
def test_native_opt_in_profile_reports_stage_durations_without_payloads(native_runtime):
    notebook, _ = native_runtime
    rpc(notebook.root, {"command": "run", "source": "payload = b'private value' * 10000"})
    checkpoint = rpc(notebook.root, {"command": "checkpoint"})
    restored = rpc(
        notebook.root, {"command": "restore", "checkpoint_id": checkpoint["checkpoint_id"]}
    )
    for result in (checkpoint, restored):
        timings = result["timings"]
        assert timings["total_seconds"] > 0
        assert all(isinstance(value, (float, int)) and value >= 0 for value in timings.values())
        assert "private" not in json.dumps(timings)
