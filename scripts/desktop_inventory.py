"""Record locked dependency licenses and schema 2 SQL columns for review."""

import json
import shutil
import sqlite3
import subprocess
from pathlib import Path

from kernelledger.ledger import DDL, DDL2

ROOT = Path(__file__).resolve().parents[1]


def write(path, value):
    path.write_text(json.dumps(value, indent=2, sort_keys=True) + "\n", encoding="utf-8")


def ledger_schema():
    db = sqlite3.connect(":memory:")
    db.executescript(DDL + DDL2)
    tables = {}
    for (name,) in db.execute(
        "SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%' ORDER BY name"
    ).fetchall():
        fields = {}
        for _, column, sql_type, not_null, _, primary in db.execute(f'PRAGMA table_info("{name}")'):
            kinds = ["integer" if sql_type == "INTEGER" else "string"]
            if not not_null and not primary:
                kinds.append("null")
            fields[column] = {"type": kinds}
        tables[name] = {
            "type": "array",
            "items": {
                "type": "object",
                "properties": fields,
                "required": list(fields),
                "additionalProperties": False,
            },
        }
    db.close()
    return {
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "title": "KernelLedger ledger schema 2 inspection",
        "type": "object",
        "properties": {
            "schema": {"const": 2},
            "tables": {
                "type": "object",
                "properties": tables,
                "required": list(tables),
                "additionalProperties": False,
            },
        },
        "required": ["schema", "tables"],
        "additionalProperties": False,
    }


def main():
    cargo = shutil.which("cargo") or str(Path.home() / ".cargo/bin/cargo.exe")
    result = subprocess.run(
        [
            cargo,
            "metadata",
            "--locked",
            "--offline",
            "--format-version",
            "1",
            "--filter-platform",
            "x86_64-pc-windows-msvc",
        ],
        cwd=ROOT / "desktop",
        check=True,
        capture_output=True,
    )
    metadata = json.loads(result.stdout)
    packages = [
        {
            "name": package["name"],
            "version": package["version"],
            "license": package["license"],
            "license_file": package["license_file"],
            "repository": package["repository"],
            "source": package["source"],
        }
        for package in metadata["packages"]
        if package["source"] is not None
    ]
    packages.sort(key=lambda package: (package["name"], package["version"]))
    write(
        ROOT / "docs/desktop/dependency-licenses.json",
        {
            "target": "x86_64-pc-windows-msvc",
            "scope": "Cargo metadata declarations; license text and redistribution review remain required",
            "packages": packages,
        },
    )
    write(ROOT / "schemas/ledger-v2.schema.json", ledger_schema())
    print(f"Recorded {len(packages)} dependency license declarations and ledger schema 2")


if __name__ == "__main__":
    main()
