"""Generate the native ledger schema from the Python contract without importing it."""

import argparse
import ast
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
OUTPUT = ROOT / "desktop/crates/runtime/src/schema.sql"


def schema() -> str:
    tree = ast.parse((ROOT / "src/kernelledger/ledger.py").read_text(encoding="utf-8"))
    values = {
        node.targets[0].id: ast.literal_eval(node.value)
        for node in tree.body
        if isinstance(node, ast.Assign)
        and isinstance(node.targets[0], ast.Name)
        and node.targets[0].id in ("DDL", "DDL2")
    }
    return (
        "-- Generated from kernelledger.ledger DDL/DDL2 by scripts/runtime_schema.py.\n"
        + values["DDL"]
        + values["DDL2"]
    )


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args()
    text = schema()
    if args.check:
        if OUTPUT.read_text(encoding="utf-8") != text:
            raise SystemExit("native ledger schema drift")
    else:
        OUTPUT.write_text(text, encoding="utf-8")
