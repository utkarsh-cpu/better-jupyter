# Native numeric boundary qualification

Python remains the default. Native durable requests support JSON integers from
`-2**63` through `2**64 - 1`. Floats and integers outside that range remain
unqualified and are refused before durable admission. This pass adds evidence
for that boundary; it does not implement full Python numeric compatibility.

`scripts/runtime_contracts.py` now generates 20 Python-authoritative golden
fixtures, including signed and unsigned endpoints, integers beyond double's
exact range, adjacent out-of-range integers, negative zero, subnormal floats,
and the largest finite double. Rust checks accepted digests against Python's
sorted, compact JSON encoding and checks refusal for unsupported forms.

`tests/test_native_numeric.py` exercises 13 added numeric cases over the real
desktop socket. It verifies that refusals create no operation row, the same
request key can then admit a valid request, stored digests match Python, changed
numeric content returns `key_reused`, and changing the transport request ID
reuses the original operation. A worker-side counter checks that retries and
refused requests do not execute extra cells.

Validation on Windows: protocol crate tests (3 passed), desktop plus numeric
integration suite (28 passed), fixture drift, Rust formatting and focused Python
lint/format. Linux: numeric admission integration passed against the existing
optimized release daemon. The runtime implementation is unchanged apart from
correcting an outdated request-hash comment. The new integration test is included
in the native qualification CI matrix.

Local JUnit evidence is under `artifacts/runtime-v6/native-numeric-windows312.xml`
and `native-numeric-linux312.xml`. These checks do not close the complete numeric
qualification gate: Python-compatible float encoding, arbitrary integers,
non-finite number policy, raw-wire alternate spellings and malformed-number
error parity remain open. No release-default change follows from this evidence.

## Raw-wire refusal and recovery

The numeric suite also sends 18 literal wire tokens without Python JSON
normalization. Parsed but unqualified tokens (`-0`, `0.0`, `1e0`, `1E+0`,
`-0e0`, underflow `1e-999`, and adjacent out-of-range integers) return
`invalid_request` while keeping the authenticated control connection usable.
In particular, native refuses integer-spelled `-0` conservatively; Python parses
it as integer zero. This is an explicit compatibility difference, not full parity.

Non-finite spellings (`NaN`, `Infinity`, `-Infinity`), overflow `1e999`, and
malformed tokens (`01`, `+1`, `1.`, `.1`, `1e`, `--1`) return `invalid_request`
and close the affected connection. This tests native policy; Python's JSON
decoder accepts some of these forms, so cross-backend error parity remains open.

For every refusal the test checks unchanged operation and execution rows, then
submits a valid request with the same client ID and idempotency key, reconnecting
when needed. The final worker counter checks exactly 18 successful executions.
Both numeric tests pass on Windows and Linux against the optimized release
daemon. Raw-wire JUnit reports are
`artifacts/runtime-v6/native-numeric-wire-windows312.xml` and
`native-numeric-wire-linux312.xml`. The existing CI numeric entry includes both
tests. Runtime behavior and the backend default are unchanged.
