# Native runtime release profiling and optimization

The release baseline, targeted optimizations, correctness tests, and real Rust UI
journey are complete for this continuation. Python remains the default. This does
not close the remaining phase exits or qualify a production release.

## Changes

* Replaced five-millisecond polling of public listeners with two blocking acceptor
  threads feeding a bounded channel. Incoming connections wake dispatch directly.
  Stop wakes and joins both acceptors, including when no clients are connecting.
  Authentication, handler limits, durable admission, and the runtime lock remain
  on their existing paths.
* Added opt-in `KERNELLEDGER_PROFILE=1` checkpoint/restore stage durations. These
  record timings, not cell contents or variable values.
* Loads the exact `validate_checkpoint` and `json_bytes` AST functions from the
  trusted worker resource without importing unrelated worker/codec modules. It
  still runs validation in an isolated Python utility. Unknown future function
  dependencies fail closed.
* Added an owner-local cache for successful validation of byte-identical manifest
  inputs. It is limited to 64 entries and 4 MiB of input/output bytes and is
  bypassed for a custom `--worker`. Large manifests bypass caching. This is a
  cache of a pure validation result, not a payload-integrity cache.
* Reuses operation-local closure verification evidence instead of hashing every
  checkpoint payload twice before publication. Every identity is still checked
  inside the publication transaction. Restore still hashes storage for each
  request, verifies identities around transfer, hashes bytes during transfer, and
  retains worker-side hash verification. No cross-request payload hash cache was
  added. FULL transactions, fsync publication, and corruption refusal remain.

## Profiling evidence

The initial release stage sample spent about 195 ms starting/running graph
validation and 42 ms in checkpoint ledger publication/closure verification.
After the changes, the latter stage was about 20–22 ms. The exact-manifest cache
reduced repeated validation to roughly 0.04–0.05 ms in the final stage sample.
Cold validation still launches Python; a final cold sample was 215 ms. These
single stage samples diagnose costs; they are not percentile estimates.

Restore stage samples also show worker-start variability. The large end-to-end
restore gain below is observed, but no claim assigns every millisecond to one
individual optimization. In particular, cache-hit restores avoid a separate
validator process immediately before starting the candidate worker.

## Quiet interleaved comparison

Five disposable notebooks per backend, with Python, a frozen pre-optimization
native release executable, and the optimized native release interleaved in one
run. Each uses the same interpreter, 32 MiB bytes payload and alias, 30 status
requests and 30 empty cells. Restored identity/content are checked. No test or
build jobs were running during this final measurement.

| Median | Python | Rust release before | Rust release after |
| --- | ---: | ---: | ---: |
| Status round trip | 1.52 ms | 5.56 ms | 1.53 ms |
| Empty cell round trip | 32.26 ms | 30.06 ms | 26.39 ms |
| First checkpoint | 268 ms | 443 ms | 322 ms |
| Unchanged checkpoint | 307 ms | 466 ms | 197 ms |
| Restore after checkpoint | 1,414 ms | 1,629 ms | 395 ms |
| Idle owner RSS, before checkpoint | 31.98 MiB | 8.19 MiB | 8.23 MiB |
| Idle Python worker RSS | 60.81 MiB | 60.96 MiB | 60.80 MiB |

Status p95 did not improve in this sample: 19.12 ms before and 22.10 ms after
(Python: 22.69 ms). The median gain must not be described as a tail-latency win.
First checkpoints remain about 20% slower than Python in this run. Unchanged
checkpoint and restore rows have an already validated manifest; cold restore or
continually changing large graphs need separate qualification.

Startup had only five samples per backend and varied substantially: Python
809–1,807 ms, baseline Rust 1,625–2,629 ms, optimized Rust 618–1,622 ms. These
ranges do not establish a general startup-speed claim. Earlier baseline samples
also differed, so the final interleaved run is the primary before/after evidence.

After checkpointing, optimized owner RSS was about 8.55–8.62 MiB versus
8.54–8.64 MiB for the frozen release. This workload's manifests are small; the
cache's input/output buffers remain capped at 4 MiB, plus entry metadata. RSS is working
set per process, not private or peak memory. UI and transient utilities are not
included; shared pages must not be summed as unique application memory.

Both owners still execute cells and encode/reconstruct objects in Python. OS
cache and background host/storage conditions were uncontrolled. No GPU, Linux
performance, loaded-control, 256 MiB/1 GiB, or release-default claim is made.

## Verification

* Complete Windows Python suite against the optimized release: **288 passed,
  44 optional/platform skips**.
* Complete Windows Rust workspace: **83 passed**, including **37 GPUI tests**.
* Linux headless Rust tests: **46 passed**; native integration suite against the
  release: **75 passed**.
* Extracted Windows and Linux release archives: **20 supervisor/integrity tests
  passed on each**.
* Real Windows Rust UI, native helper and optimized release owner: Run,
  checkpoint, trust confirmation, restore into a replacement session, namespace
  verification, navigation, resizing, and caption hit-testing passed.
* Full Python lint/format/Pyright, Rust formatting/Clippy, and schema/fixture drift
  checks pass.

New regression cases warm validation before testing malformed graphs; verify
missing fields, dangling references, invalid kinds/base64 and forbidden bindings;
corrupt a stored payload after a successful checkpoint and verify restore refusal
without losing the healthy namespace; check bounded validation-result reuse;
and verify blocked acceptors wake and join on idle shutdown.

## Artifacts and reproduction

All paths below are under `artifacts/runtime-v6/`:

* `backend-release-before-after-windows312.json`: primary interleaved samples,
  build hashes, source/environment identity, ranges and percentiles.
* `backend-release-baseline-windows312.json`: initial release baseline.
* `backend-release-profile-before.json`, `backend-release-profile-final.json`:
  diagnostic stage samples.
* `backend-release-optimized-under-test-load-windows312.json`: a diagnostic run
  that overlapped tests/builds; explicitly excluded from the final comparison.
* `native-optimized-full-windows312.xml`, `native-optimized-release-linux312.xml`,
  `native-optimized-package-*.xml`: test evidence.
* `optimized-rust-workspace-windows.log`, `optimized-rust-clippy-windows.log` and
  `optimized-ui/native-dark/probe-dark.json`: Rust/UI evidence.
* `native-release-opt-in-*.zip`: optimized local release-build evaluation archives.
  These remain experimental artifacts, not signed qualified releases.

```text
cargo build --release --locked -p kernelledger-daemon
python scripts/runtime_compare.py --daemon /absolute/optimized-daemon --baseline-daemon /absolute/frozen-baseline-daemon --build-label "release before/after" --repetitions 5 --controls 30 --mib 32 --out artifacts/runtime-v6/comparison.json
```

Keep Python as the default while remaining durability/load, numeric compatibility,
grouped stream/queue budgets, capability parity, clean-machine distribution and
release-candidate gates are open. Next performance qualification should exercise
cold restores, changing graphs, larger payloads, and control p95 under bulk work.
