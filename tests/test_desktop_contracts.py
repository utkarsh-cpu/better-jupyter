import json
from pathlib import Path

import jsonschema
import pytest

from kernelledger.desktop.contracts import schema, validate
from kernelledger.ledger import Ledger
from kernelledger.source import SourceError

ROOT = Path(__file__).resolve().parents[1]


def test_shared_protocol_fixtures_and_schema_generation():
    fixture = json.loads((ROOT / "tests/fixtures/desktop/contract.json").read_text())
    for helper in (False, True):
        name = "desktop-helper" if helper else "desktop-protocol"
        contract = json.loads((ROOT / "schemas" / (name + ".schema.json")).read_text())
        assert contract == schema(helper)
        jsonschema.Draft202012Validator.check_schema(contract)
        for frame in fixture["frames"]:
            jsonschema.validate(frame, contract)


@pytest.mark.parametrize(
    "message",
    [
        {"method": "run", "cell_id": "a", "expected_sha256": "0" * 64},
        {"method": "operation.cancel"},
        {"method": "blob.read", "sha256": "0" * 64, "offset": -1},
        {"method": "changes.list", "page_size": 501},
        {"method": "replay", "session_id": "s", "trust": "yes"},
    ],
)
def test_invalid_requests_are_typed_errors(message):
    with pytest.raises(SourceError) as error:
        validate(message)
    assert error.value.code == "invalid_request"


def test_ledger_v2_inspection_schema_matches_sql_columns(tmp_path):
    contract = json.loads((ROOT / "schemas/ledger-v2.schema.json").read_text())
    jsonschema.Draft202012Validator.check_schema(contract)
    ledger = Ledger(tmp_path / "ledger.sqlite")
    try:
        tables = {}
        for name, definition in contract["properties"]["tables"]["properties"].items():
            columns = ledger.rows(f'PRAGMA table_info("{name}")')
            assert {column["name"] for column in columns} == set(definition["items"]["properties"])
            tables[name] = ledger.rows(f'SELECT * FROM "{name}"')
        jsonschema.validate({"schema": ledger.schema, "tables": tables}, contract)
    finally:
        ledger.close()
