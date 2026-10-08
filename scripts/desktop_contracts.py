"""Regenerate desktop schemas from the runtime-owned request catalog."""

import json
from pathlib import Path

from kernelledger.desktop.contracts import schema

root = Path(__file__).resolve().parents[1]
for helper in (False, True):
    name = "desktop-helper" if helper else "desktop-protocol"
    (root / "schemas" / (name + ".schema.json")).write_text(
        json.dumps(schema(helper), indent=2) + "\n", encoding="utf-8"
    )
