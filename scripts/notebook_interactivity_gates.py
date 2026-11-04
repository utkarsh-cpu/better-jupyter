"""Report interactivity release gates without treating implementation as qualification."""

import argparse
import json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def blockers(record, through=10):
    if record.get("schema") != "kernelledger.interactivity-gates/1":
        raise ValueError("Unsupported interactivity gates schema")
    phases = record.get("phases", [])
    if sorted(phase["phase"] for phase in phases) != list(range(11)):
        raise ValueError("Expected N0–N10 exactly once")
    result = []
    for phase in phases:
        if phase["status"] not in ("partial", "passed", "blocked"):
            raise ValueError("Invalid gate status")
        if phase["phase"] <= through and (
            phase["status"] != "passed" or phase.get("remaining") or not phase.get("evidence")
        ):
            result.append(
                {
                    "phase": phase["phase"],
                    "remaining": phase.get("remaining") or ["No exit evidence"],
                }
            )
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--record", type=Path, default=ROOT / "docs/notebook-interactivity-gates.json"
    )
    parser.add_argument("--through", type=int, choices=range(11), default=10)
    parser.add_argument("--require-passed", action="store_true")
    args = parser.parse_args()
    record = json.loads(args.record.read_text(encoding="utf-8"))
    outstanding = blockers(record, args.through)
    if args.through == 10:
        from native_gates import blockers as native_blockers

        inherited = json.loads((ROOT / "docs/runtime-v6-gates.json").read_text(encoding="utf-8"))
        outstanding.extend(
            {"phase": "inherited-native", "remaining": [item]}
            for item in native_blockers(inherited, 8)
        )
    print(
        json.dumps(
            {
                "default_owner": record["default_owner"],
                "default_engine": record["default_engine"],
                "through": args.through,
                "passed": not outstanding,
                "blockers": outstanding,
            },
            indent=2,
        )
    )
    if args.require_passed and outstanding:
        raise SystemExit(1)


if __name__ == "__main__":
    main()
