"""Numeric hash boundaries must preserve durable admission and retry identity."""

import hashlib
import json
import struct
import uuid
from pathlib import Path

import pytest
from test_desktop import Client
from test_native_supervisor import native_runtime as native_runtime

from kernelledger.cli import rpc
from kernelledger.ledger import Ledger
from kernelledger.notebook import source_hash
from kernelledger.source import SourceError


def test_numeric_admission_and_retry_boundaries(native_runtime):
    notebook, _ = native_runtime
    fixtures = json.loads(
        (Path(__file__).parent / "fixtures/runtime/request-hashes.json").read_text()
    )
    client = Client(notebook.root)
    source = "numeric_boundary_runs = globals().get('numeric_boundary_runs', 0) + 1"
    admitted = 0
    try:
        for case in fixtures:
            if not case["name"].startswith(("exact_integer_", "unsupported_numeric_")):
                continue
            key = str(uuid.uuid4())
            fields = {
                "client_request_id": key,
                "source": source,
                "expected_sha256": source_hash(source),
                "numeric_metadata": case["request"],
            }
            ledger = Ledger(notebook.root / ".kl/ledger.sqlite", readonly=True)
            try:
                before = ledger.rows("SELECT operation_id FROM operations")
                if not case["native_supported"]:
                    with pytest.raises(SourceError) as error:
                        client.call("run", **fields)
                    assert error.value.code == "invalid_request", case["name"]
                    assert "numeric form is not qualified" in str(error.value)
                    assert ledger.rows("SELECT operation_id FROM operations") == before
                    # A refused request must not consume its idempotency key.
                    fields["numeric_metadata"] = {"value": 0}
                accepted = client.call("run", request_id="first", **fields)
                result = client.wait(accepted["operation_id"])
                assert result["state"] == "completed", result
                admitted += 1
                retry = client.call("run", request_id="retry", **fields)
                assert retry["operation_id"] == accepted["operation_id"]
                with pytest.raises(SourceError) as conflict:
                    client.call("run", **{**fields, "numeric_metadata": {"value": 1}})
                assert conflict.value.code == "key_reused"
                request = {"method": "run", **fields}
                digest = hashlib.sha256(
                    json.dumps(request, sort_keys=True, separators=(",", ":")).encode()
                ).hexdigest()
                rows = ledger.rows(
                    "SELECT request_sha256 FROM operations WHERE operation_id=?",
                    (accepted["operation_id"],),
                )
                assert rows[0]["request_sha256"] == digest, case["name"]
                assert len(ledger.rows("SELECT operation_id FROM operations")) == len(before) + 1
            finally:
                ledger.close()
        assert (
            rpc(
                notebook.root,
                {"command": "run", "source": f"assert numeric_boundary_runs == {admitted}"},
            )["status"]
            == "ok"
        )
    finally:
        client.close()


def test_raw_numeric_refusal_and_reconnect(native_runtime):
    notebook, _ = native_runtime
    # Parsed but unqualified numbers keep the control connection usable.
    unqualified = [
        "-0",
        "0.0",
        "1e0",
        "1E+0",
        "-0e0",
        "1e-999",
        "18446744073709551616",
        "-9223372036854775809",
    ]
    # Invalid JSON or an unrepresentable number ends this framed connection.
    invalid = ["NaN", "Infinity", "-Infinity", "1e999", "01", "+1", "1.", ".1", "1e", "--1"]
    source = "raw_numeric_runs = globals().get('raw_numeric_runs', 0) + 1"
    for token in unqualified + invalid:
        client = Client(notebook.root)
        key = str(uuid.uuid4())
        fields = {
            "client_request_id": key,
            "source": source,
            "expected_sha256": source_hash(source),
        }
        # Construct the wire token directly; json.dumps would normalize it.
        prefix = json.dumps({"method": "run", **fields})[:-1]
        data = (prefix + ',"numeric_metadata":[' + token + "]}").encode()
        ledger = Ledger(notebook.root / ".kl/ledger.sqlite", readonly=True)
        try:
            before = ledger.rows("SELECT operation_id FROM operations")
            executions = ledger.rows("SELECT exec_id FROM executions")
            client.sock.sendall(struct.pack(">I", len(data)) + data)
            response = client.read()
            assert response["kind"] == "error", (token, response)
            assert response["error"]["code"] == "invalid_request", (token, response)
            assert ledger.rows("SELECT operation_id FROM operations") == before
            assert ledger.rows("SELECT exec_id FROM executions") == executions
            if token in invalid:
                assert client.file.read(1) == b"", token
                client.close()
                # The failed connection must not poison this client's retry key.
                client = Client(notebook.root, client_id=client.client_id)
            else:
                client.call("runtime.status")
            accepted = client.call("run", **fields)
            result = client.wait(accepted["operation_id"])
            assert result["state"] == "completed", (token, result)
            assert len(ledger.rows("SELECT operation_id FROM operations")) == len(before) + 1
        finally:
            ledger.close()
            client.close()
    assert (
        rpc(
            notebook.root,
            {
                "command": "run",
                "source": f"assert raw_numeric_runs == {len(unqualified + invalid)}",
            },
        )["status"]
        == "ok"
    )
