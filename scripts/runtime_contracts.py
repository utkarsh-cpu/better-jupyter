"""Freeze Python's existing operation-hash bytes for cross-language parity."""

import argparse
import hashlib
import json
from pathlib import Path

OUTPUT = Path(__file__).resolve().parents[1] / "tests/fixtures/runtime/request-hashes.json"


def fixtures():
    cases = [
        (
            "unicode",
            {
                "method": "run",
                "source": "é\n\t😀\u007f",
                "request_id": "excluded",
                "client_request_id": "kept",
            },
            True,
        ),
        (
            "ordering",
            {
                "z": {"β": None, "a": True},
                "a": [False, 0, -7, 9223372036854775807, 18446744073709551615],
            },
            True,
        ),
        (
            "transport_retry",
            {
                "method": "run",
                "source": "é\n\t😀\u007f",
                "request_id": "different",
                "client_request_id": "kept",
            },
            True,
        ),
        ("escapes", {"x": '\b\f\r\\"/\x00', "nested": {"request_id": "retained"}}, True),
        ("negative_zero", {"value": -0.0}, False),
        ("float", {"value": 1.0, "small": 1e-7, "large": 1e20}, False),
        ("big_integer", {"value": 2**100}, False),
    ]
    result = []
    for name, request, supported in cases:
        admitted = {k: v for k, v in request.items() if k != "request_id"}
        canonical = json.dumps(admitted, sort_keys=True, separators=(",", ":"))
        result.append(
            {
                "name": name,
                "request": request,
                "canonical": canonical,
                "sha256": hashlib.sha256(canonical.encode()).hexdigest(),
                "native_supported": supported,
            }
        )
    return result


if __name__ == "__main__":
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--check", action="store_true")
    args = p.parse_args()
    text = json.dumps(fixtures(), indent=2, ensure_ascii=True) + "\n"
    if args.check:
        if not OUTPUT.exists() or OUTPUT.read_text(encoding="utf-8") != text:
            raise SystemExit("runtime hash fixture drift")
    else:
        OUTPUT.parent.mkdir(parents=True, exist_ok=True)
        OUTPUT.write_text(text, encoding="utf-8")
