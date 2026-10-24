"""Report native phase exit gates; fail closed when a release/default is requested."""

import argparse
import json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def blockers(record: dict, through: int) -> list[str]:
    if record.get("schema") != "kernelledger.native-gates/1":
        raise ValueError("unsupported native gate schema")
    phases = record.get("phases", [])
    if sorted(p.get("phase") for p in phases) != list(range(3, 9)):
        raise ValueError("phase gates must cover phases 3–8 exactly once")
    result = []
    for phase in phases:
        if phase["status"] not in ("partial", "blocked", "passed"):
            raise ValueError("invalid phase gate status")
        if phase["phase"] > through:
            continue
        remaining = phase.get("remaining", [])
        if phase["status"] != "passed" or remaining:
            result.append(f"Phase {phase['phase']}: " + "; ".join(remaining or [phase["status"]]))
        elif not phase.get("evidence"):
            result.append(f"Phase {phase['phase']}: no exit evidence recorded")
    if through >= 8:
        candidates = record.get("release_candidates", [])
        qualified = {
            c.get("id")
            for c in candidates
            if c.get("id")
            and c.get("status") == "passed"
            and c.get("platforms") == ["linux", "windows"]
            and c.get("evidence")
        }
        if len(qualified) < 2:
            result.append(
                "Default switch: two distinct qualified Windows/Linux opt-in candidates required"
            )
    return result


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--record", type=Path, default=ROOT / "docs/runtime-v6-gates.json")
    parser.add_argument("--through", type=int, choices=range(3, 9), default=8)
    parser.add_argument("--require-passed", action="store_true")
    args = parser.parse_args()
    record = json.loads(args.record.read_text(encoding="utf-8"))
    outstanding = blockers(record, args.through)
    print(
        json.dumps(
            {
                "default_backend": record["default_backend"],
                "through": args.through,
                "passed": not outstanding,
                "blockers": outstanding,
            },
            indent=2,
        )
    )
    if args.require_passed and outstanding:
        raise SystemExit(1)
