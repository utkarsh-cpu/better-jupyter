"""Generate the checked-in schema 1 contracts from shared primitives."""

import json
from pathlib import Path

HASH = {"type": "string", "pattern": "^[0-9a-f]{64}$"}
ID = {"type": "string", "format": "uuid"}
STRING = {"type": "string"}
ARRAY = {"type": "array"}
OBJECT = {"type": "object"}


def schema(name, fields, required=None):
    result = {
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "title": f"KernelLedger {name} schema 1",
        "type": "object",
        "properties": {"schema": {"const": 1}, **fields},
        "required": ["schema", *(required if required is not None else fields)],
        "additionalProperties": True,
    }
    Path("schemas").mkdir(exist_ok=True)
    Path(f"schemas/{name}.schema.json").write_text(
        json.dumps(result, indent=2) + "\n", encoding="utf-8"
    )


schema(
    "notebook",
    {
        "id": ID,
        "metadata": OBJECT,
        "cells": {
            "type": "array",
            "items": {
                "type": "object",
                "required": ["id", "type"],
                "properties": {
                    "id": {"type": "string", "pattern": "^[A-Za-z0-9][A-Za-z0-9_-]{0,63}$"},
                    "type": {"enum": ["code", "markdown", "raw"]},
                    "metadata": OBJECT,
                    "attachments": {
                        "type": "array",
                        "uniqueItems": True,
                        "items": {"type": "string", "pattern": "^[^/\\\\:]+$"},
                    },
                },
            },
        },
    },
)
schema(
    "checkpoint",
    {
        "python": STRING,
        "platform": STRING,
        "codecs": OBJECT,
        "bindings": OBJECT,
        "objects": {
            "type": "object",
            "additionalProperties": {
                "type": "object",
                "required": ["k"],
                "properties": {
                    "k": {
                        "enum": [
                            "list",
                            "tuple",
                            "dict",
                            "set",
                            "frozenset",
                            "bytearray",
                            "module",
                            "import",
                            "function",
                            "class",
                            "instance",
                            "ndarray",
                            "table",
                            "numpy_generator",
                        ]
                    }
                },
            },
        },
        "excluded": {
            "type": "array",
            "items": {"type": "object", "required": ["name", "type", "path", "reason"]},
        },
        "notes": ARRAY,
        "imports_reimported": {"type": "boolean"},
    },
)
schema(
    "descriptor",
    {
        "checkpoint_id": ID,
        "manifest_sha256": HASH,
        "blob_hashes": {"type": "array", "items": HASH, "uniqueItems": True},
        "status": {"enum": ["complete", "partial"]},
        "excluded": ARRAY,
        "notes": ARRAY,
    },
)
schema(
    "bundle",
    {
        "notebook_id": ID,
        "session_ids": {"type": "array", "items": ID},
        "checkpoint_ids": {"type": "array", "items": ID},
        "blob_hashes": {"type": "array", "items": HASH},
        "external": {"type": "array", "items": STRING},
        "files": {"type": "object", "additionalProperties": HASH},
    },
)
schema(
    "protocol",
    {
        "kind": {
            "enum": [
                "auth",
                "request",
                "response",
                "notice",
                "blob",
                "envelope",
                "hello",
                "ready",
                "execute",
                "stdin_reply",
                "interrupt",
                "checkpoint",
                "restore",
                "shutdown",
                "stdout",
                "stderr",
                "display",
                "display_update",
                "result",
                "error",
                "clear",
                "stdin_request",
                "status",
                "checkpoint_offer",
                "restore_result",
            ]
        }
    },
    ["kind"],
)
schema(
    "ledger",
    {
        "tables": {
            "type": "object",
            "required": [
                "sessions",
                "executions",
                "events",
                "checkpoints",
                "blobs",
                "pins",
                "session_events",
            ],
            "additionalProperties": {"type": "array", "items": OBJECT},
        }
    },
)


# Make the contracts useful to another implementation: specify each graph kind,
# protocol payload, and ledger column, rather than only top-level envelopes.
def enrich(name, update):
    path = Path(f"schemas/{name}.schema.json")
    document = json.loads(path.read_text())
    update(document)
    path.write_text(json.dumps(document, indent=2) + "\n", encoding="utf-8")


def conditional(kind, fields):
    return {
        "if": {"properties": {"k": {"const": kind}}},
        "then": {"required": list(fields), "properties": fields},
    }


def checkpoint_contract(doc):
    value = {"$ref": "#/$defs/value"}
    integer_string = {"type": "string", "pattern": "^-?[0-9]+$"}
    binary = {
        "oneOf": [
            {"required": ["b64"], "not": {"required": ["blob"]}},
            {"required": ["blob"], "not": {"required": ["b64"]}},
        ],
        "properties": {"b64": STRING, "blob": HASH},
    }
    kinds = {
        "ref": {"id": STRING},
        "int": {"v": integer_string},
        "float": {"v": {"oneOf": [{"type": "number"}, {"enum": ["nan", "inf", "-inf"]}]}},
        "complex": {"re": value, "im": value},
        "str": {},
        "bytes": {},
        "date": {"v": STRING, "tz": {"type": ["object", "null"]}, "fold": {"enum": [0, 1]}},
        "time": {"v": STRING, "tz": {"type": ["object", "null"]}, "fold": {"enum": [0, 1]}},
        "datetime": {"v": STRING, "tz": {"type": ["object", "null"]}, "fold": {"enum": [0, 1]}},
        "timedelta": {name: {"type": "integer"} for name in ("days", "seconds", "microseconds")},
        "Decimal": {"v": STRING},
        "Fraction": {"num": integer_string, "den": integer_string},
        "UUID": {"v": ID},
        "range": {name: integer_string for name in ("start", "stop", "step")},
        "path": {
            "class": {"enum": ["PosixPath", "WindowsPath", "PurePosixPath", "PureWindowsPath"]},
            "v": STRING,
        },
        "enum": {"class": value, "name": STRING},
    }
    tagged = {
        "type": "object",
        "required": ["k"],
        "properties": {"k": {"enum": list(kinds)}},
        "allOf": [conditional(k, fields) for k, fields in kinds.items()],
    }
    tagged["allOf"].append({"if": {"properties": {"k": {"const": "bytes"}}}, "then": binary})
    tagged["allOf"].append(
        {
            "if": {"properties": {"k": {"const": "str"}}},
            "then": {
                "oneOf": [
                    {"required": ["v"], "properties": {"v": STRING}, "not": {"required": ["blob"]}},
                    {
                        "required": ["blob"],
                        "properties": {"blob": HASH},
                        "not": {"required": ["v"]},
                    },
                ]
            },
        }
    )
    doc["$defs"] = {"value": {"oneOf": [{"type": ["null", "boolean"]}, tagged]}}
    doc["properties"]["bindings"]["additionalProperties"] = value
    fields = {
        k: {"items": {"type": "array", "items": value}}
        for k in ("list", "tuple", "set", "frozenset")
    }
    fields.update(
        {
            "dict": {
                "items": {
                    "type": "array",
                    "items": {
                        "type": "array",
                        "prefixItems": [value, value],
                        "minItems": 2,
                        "maxItems": 2,
                    },
                }
            },
            "bytearray": {},
            "module": {"module": STRING},
            "import": {"module": STRING, "qualname": STRING},
            "function": {
                "source": HASH,
                "ordinal": {"type": "integer"},
                "line": {"type": "integer"},
                "name": STRING,
                "qualname": STRING,
                "closure": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "required": ["name"],
                        "properties": {
                            "name": STRING,
                            "empty": {"type": "boolean"},
                            "value": value,
                        },
                    },
                },
                "defaults": value,
                "kwdefaults": value,
                "attrs": value,
            },
            "class": {
                "source": HASH,
                "ordinal": {"type": "integer"},
                "line": {"type": "integer"},
                "name": STRING,
            },
            "instance": {"class": value, "attrs": value},
            "ndarray": {
                "blob": HASH,
                "dtype": STRING,
                "shape": {"type": "array", "items": {"type": "integer", "minimum": 0}},
            },
            "table": {
                "blob": HASH,
                "codec": {"const": "arrow"},
                "series": {"type": "boolean"},
                "name": value,
            },
            "numpy_generator": {"class": STRING, "state": value},
        }
    )
    node = doc["properties"]["objects"]["additionalProperties"]
    node["allOf"] = [conditional(k, f) for k, f in fields.items()] + [
        {"if": {"properties": {"k": {"const": "bytearray"}}}, "then": binary}
    ]


def protocol_contract(doc):
    fields = {
        "auth": {"token": STRING},
        "request": {"command": STRING},
        "notice": {"message": STRING},
        "blob": {
            "sha256": HASH,
            "byte_size": {"type": "integer", "minimum": 0, "maximum": 2147483648},
        },
        "envelope": {"blob": HASH},
        "hello": {
            "token": STRING,
            "python": STRING,
            "implementation": STRING,
            "executable": STRING,
            "platform": STRING,
            "cwd": STRING,
        },
        "execute": {
            "exec_id": ID,
            "cell_id": {"type": ["string", "null"]},
            "source_sha256": HASH,
            "source": STRING,
        },
        "restore": {"manifest": OBJECT},
        "display": {"bundle": OBJECT},
        "display_update": {"bundle": OBJECT, "display_id": {"type": ["string", "null"]}},
        "result": {"bundle": OBJECT},
        "error": {
            "ename": STRING,
            "evalue": STRING,
            "traceback": {"type": "array", "items": STRING},
        },
        "clear": {"wait": {"type": "boolean"}},
        "stdin_request": {"prompt": STRING, "password": {"type": "boolean"}},
        "status": {"state": {"enum": ["running", "idle"]}},
        "restore_result": {"installed": ARRAY, "excluded": ARRAY, "notes": ARRAY},
    }
    doc["allOf"] = [
        {
            "if": {"properties": {"kind": {"const": kind}}},
            "then": {"required": list(properties), "properties": properties},
        }
        for kind, properties in fields.items()
    ]
    for kind, options in {
        "response": ["result", "error"],
        "stdin_reply": ["text", "error"],
        "checkpoint_offer": ["manifest", "refused", "failed"],
        "stdout": ["text", "blob"],
        "stderr": ["text", "blob"],
    }.items():
        doc["allOf"].append(
            {
                "if": {"properties": {"kind": {"const": kind}}},
                "then": {"oneOf": [{"required": [field]} for field in options]},
            }
        )


def ledger_contract(doc):
    import sqlite3
    from kernelledger.ledger import DDL, TABLES

    db = sqlite3.connect(":memory:")
    db.executescript(DDL)
    tables = doc["properties"]["tables"]
    tables["additionalProperties"] = False
    tables["properties"] = {}
    for table in TABLES:
        columns = db.execute("PRAGMA table_info(" + table + ")").fetchall()
        properties = {
            column[1]: {
                "type": (["integer"] if column[2] == "INTEGER" else ["string"])
                + ([] if column[3] or column[5] else ["null"])
            }
            for column in columns
        }
        tables["properties"][table] = {
            "type": "array",
            "items": {
                "type": "object",
                "properties": properties,
                "required": list(properties),
                "additionalProperties": False,
            },
        }
    db.close()


enrich("checkpoint", checkpoint_contract)
enrich("protocol", protocol_contract)
enrich("ledger", ledger_contract)
