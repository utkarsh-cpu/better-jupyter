# Runtime migration implementation evidence

The v6 draft is a gated migration, not a release declaration. `PLAN.md` remains
the current runtime plan. The assistant work already in this working tree remains
part of the compatibility surface. No backend default or artifact schema changes
are authorized by performance measurements alone.

## Baseline and acceptance scenarios

`scripts/runtime_inventory.py` generates `runtime-capabilities.json` from the CLI,
desktop/helper/assistant catalogs, worker AST, and schemas. Its suite references
are regression entry points, not claims of complete per-method qualification.
Use `--check` to detect inventory drift.

The initial Windows/Python 3.12.10 working-tree test run had **155 passed, 42
skipped, 3 failed, 4 errors**. The failures predate migration edits: selected
standalone interpreter identity fails in the standalone-worker and two shell
installation tests; the four untracked UI-state tests pass a `Notebook` where
their client requires a filesystem path. GPU/optional dependency skips do not
qualify those combinations. These results prevent claiming a clean Phase 0 exit.

| ID | Invariant and required acceptance | Existing regression entry point |
| --- | --- | --- |
| V6-01 | Python/native contend on byte 0, length 1 on Windows and flock on Linux; no PID-based stealing | `test_runtime.py::test_lock`; native contention suite |
| V6-02 | Standalone worker receives no `.kl` path, cannot publish runtime state | `test_boundaries.py::test_token_permissions_and_standalone_worker` |
| V6-03 | Crash before checkpoint commit publishes nothing; after commit all payloads exist | `test_boundaries.py::test_publish_crash_commit_point` |
| V6-04 | Lost acceptance/terminal reply does not repeat an effect; restart retains uncertain results | `test_desktop.py::test_dropped_reply_idempotency_and_feed`, `test_runtime.py::test_crash_recovery` |
| V6-05 | Offline inspection starts no worker; source editing uses source service without writing history | `test_desktop.py::test_offline_helper_has_no_side_effects`, assistant source proposal tests |
| V6-06 | Revision conflicts, source recovery, canonical bytes, frozen batch ordering | `test_desktop.py::test_source_concurrent_writers_and_external_change`, `test_source_journal_recovery`, `test_batch_stop_on_error_and_frozen_source` |
| V6-07 | Input claims redact secrets; capped output, ordered events and committed feeds | `test_desktop.py::test_input_claim_handoff_and_redaction`, `test_subscription_follow_and_verified_blob`, `test_boundaries.py::test_stream_volume_truncation_and_denylist` |
| V6-08 | Restore retains aliases, cycles, refusals, provenance and version/trust checks | `test_checkpoint.py`, `test_runtime.py::test_history_restore_and_bundle` |
| V6-09 | Job/process-group cleanup, hidden Windows launches and token ACLs | `test_boundaries.py::test_busy_and_interrupt_kill`, `test_token_permissions_and_standalone_worker` |
| V6-10 | Native GPU capture remains unqualified and unavailable | `test_gpu_capabilities.py::test_compute_and_tool_presence_never_qualify_native` |

Each native owner must pass these scenarios before replacing the corresponding
Python owner. Cross-backend recovery, disk-full, source path edge cases and
clean-machine installs remain additional release gates, not inferred passes.

## Measurement procedure

Run `python scripts/runtime_benchmark.py --out <result.json> --snapshot <source.zip>`
in the interpreter being qualified. The default is five 256 MiB NumPy array runs,
with aliased arrays, unchanged checkpoints, restore value/identity checks, thirty
short cells per run, and concurrent status requests during checkpoint transfer.
Use `--mib 1024` for the memory scaling comparison. Each sample uses a disposable
notebook. The output records source hashes, dirty state, dependency versions,
SQLite build options, first versus repeated checkpoints, WAL size, raw samples,
and separate sampled supervisor and worker RSS. RSS is not private resident memory.
The 20 ms sampling interval can miss shorter peaks. The storage device and OS
cache conditions need an accompanying environment note; first use is not a claim
of a cold filesystem cache. Startup samples below thirty report median/range only.

The snapshot includes current source/configuration and the existing assistant/UI
changes. Snapshots exclude artifacts and benchmarks; output files retain their
per-file hashes. Never run the two backend writers on one notebook to compare them.

## Compatibility decisions

Worker/legacy frame headers remain 65,536 bytes; desktop/helper JSON remains
1,048,576 bytes. Binary worker payloads are limited to 2 GiB. A native prototype
must not advertise production supervisor/helper capabilities.

Operation hashes are SHA-256 over UTF-8 bytes of Python
`json.dumps(request, sort_keys=True, separators=(",", ":"))`. Desktop admission
removes only the top-level `request_id` before hashing. Defaults include ASCII
escaping and Python's float spelling; `client_request_id` and extra fields remain
part of the hash. Shared fixtures must cover Unicode/surrogate pairs, integer
boundaries, negative zero, exponent formatting and booleans/null. Unsupported
native numeric forms must be rejected before side-effect admission until exact
parity exists; serde's default serializer is not a compatible substitute.

No live backend switch, auto fallback after acceptance, or persisted backend
selector is introduced before a qualified native supervisor exists. The native
spike is invoked explicitly as a separate command. The Python CLI and helper keep
their current selection semantics.

## Measured Python storage changes

All figures below are five disposable notebooks on Windows 11, Python 3.12.10,
dirty tree `f995161`, from `benchmarks/runtime-v6/`. RSS is the 20 ms sampled
peak, not private resident memory. `python-streaming.json` and
`python-streaming-1gib.json` are copies of the after files. They predate the
later move of blob writes and `fsync` onto a two-slot blocking pool, so they do
not measure that pool. The pool is covered by
`test_sync_keeps_control_responsive_and_cancellation_waits_for_io`.

256 MiB NumPy array:

| Quantity | Before median (min–max) | After median (min–max) |
| --- | --- | --- |
| Checkpoint | 1.498 s (1.442–1.542) | 1.237 s (1.219–2.033) |
| Unchanged checkpoint | 1.627 s (1.595–2.229) | 1.422 s (1.378–2.155) |
| Restore | 2.383 s (2.337–2.693) | 2.450 s (2.404–2.533) |
| Startup | 2.000 s (1.197–2.012) | 2.113 s (1.356–2.120) |
| Supervisor RSS | 452.71 MiB (401.82–531.00) | 35.86 MiB (35.00–36.57) |
| Worker RSS | 807.4 MiB (781.7–807.6) | 732.3 MiB (669.6–770.7) |

Startup and restore medians moved by less than 10%. Checkpoint and unchanged
checkpoint medians improved. Supervisor RSS fell because checkpoint metadata no
longer rereads a blob to learn its size, and restore sends the stored file in
bounded chunks.

The same after build at 1 GiB, before blob writes moved to the blocking pool,
kept supervisor RSS at a 36.68 MiB median (35.97–37.09) while worker RSS reached
a 2770.5 MiB median. Checkpoint median was 6.341 s (4.992–7.807), unchanged
checkpoint 5.811 s, and restore 5.394 s. Control acknowledgments during that
transfer had median 20.7 ms and p95 26.0 ms, with a maximum of 3.097 s.

`python-blocking-pool-1gib.json` repeats that 1 GiB workload after the two-slot
blocking pool. Supervisor RSS median was 36.46 MiB (35.60–36.77) and worker RSS
median was 2799.7 MiB. Checkpoint median was 5.101 s (5.049–5.218), unchanged
checkpoint 5.853 s (5.788–5.993), and restore 5.358 s (5.347–5.390). Across
1,061 status calls during checkpoint transfer, control latency median was
17.2 ms, p95 25.9 ms, and maximum 31.2 ms. The 3.097 s synchronization stall
does not appear in this run. These figures are one Windows machine with an
uncontrolled OS cache.

## Qualification record

Recorded on this machine, 2026-10-24. Optional-dependency skips are unsupported
lanes for that environment, not passes.

| Run | Result |
| --- | --- |
| Windows 3.12.10, full suite after the venv launcher fix (`python312.xml`) | 183 passed, 47 skipped |
| Windows 3.14.7, full suite before `Path.resolve()` on the base interpreter (`python314.xml`) | 176 passed, 45 skipped, 1 failed |
| Windows 3.14.7, full suite after that fix (`python314-after.xml`) | 189 passed, 48 skipped |
| Windows 3.14.7, `test_windows_venv_skips_redirector_and_keeps_site_packages` after that fix | 1 passed |
| Linux 3.12, full suite before shell-cleanup used the original `sleep` (`linux312.xml`) | 183 passed, 46 skipped, 1 failed (`test_interrupt_stops_shell_child_and_preserves_worker`, `TimeoutError`) |
| Linux, runtime, boundaries, blob streaming, and shell after that fix (`linux-storage-final.xml`) | 54 passed, 1 skipped |
| Windows, runtime, boundaries, and blob streaming after the blocking pool (`storage-final.xml`) | 46 passed, 1 skipped |
| Windows native probe, Python 3.12 and 3.14 workers | 7 passed each |
| WSL2 native probe, Python 3.12.3, Rust 1.99.0 debug daemon (`linux-native-probe.xml`) | 7 passed |
| Python 3.14 supervisor with the 3.12 worker | 1 passed |
| Pyright on `blobs.py`, `worker.py`, and `supervisor/__init__.py`; Ruff on the touched modules | clean |

The 3.14 failure compared uv's `cpython-3.14-...` junction with the resolved
`cpython-3.14.7-...` interpreter. The full 3.14 suite was repeated after the
fix: 189 passed and 48 skipped, including the repaired UI-state tests. The
full Linux suite was not repeated after the shell fix; the previously
failing shell test is inside the later 54-pass run. Python 3.15 final is not
installed here. The qualification workflow requests it with prereleases disabled,
and no local 3.15 result is claimed. GPU combinations were not requalified.
`tests/test_ui_state.py` now reads `runtime.status`, `fold.get`, and
`checkpoints.list` through a project path. It no longer calls a `ui.query`
method or passes a `Notebook` object where a path is required. The three tests
passed on Python 3.12.10 and are part of the later 3.14 suite.

## Phase 2 decision

Keep the Python supervisor, helper, ledger, and source service. Leave
`kernelledger-daemon` as an explicit feasibility probe. Do not start the native
read-only helper, storage port, or supervisor replacement described in later
draft phases.

The release probe binary is 588,800 bytes. On five 256 MiB runs its sampled RSS
median was 7.34 MiB, with checkpoint median 0.542 s and restore median 0.475 s.
Each sample records `parity_comparison: false`: the probe does not write a
ledger, publish with `fsync`, admit operations, or serve the helper. Restore
reuses the probe's worker. Those timings are not a production comparison with
the optimized Python supervisor. Windows tests cover lock contention in both
directions, including release after the native owner exits or crashes, a worker
blob round trip, and child cleanup. The same seven tests passed on WSL2 with a
debug daemon built by Rust 1.99.0. That is a Linux execution of the probe, not
a clean-machine install or a release-binary comparison.

The Python supervisor's sampled RSS staying near 36 MiB from 256 MiB through
1 GiB is the measured memory result. A clean-machine install comparison was not
run, so packaging does not by itself authorize the port. Backend selection stays
on the Python CLI and helper.

## Remaining gates

- Run 3.11, 3.13, and 3.15, with and without codecs, on Windows and Linux.
  Python 3.12 and 3.14 have Windows results above; 3.14 is not a codecs or
  Linux run. Record a missing wheel as an unsupported combination.
- Any later native owner must pass scenarios V6-01–V6-10, including crash
  recovery across Python and Rust writers. The feasibility probe itself has
  Windows and WSL2 results above.
- Qualify GPU combinations separately before claiming those lanes.
- Ship a packaged daemon only through an explicit desktop or platform artifact,
  with a clean-machine install, upgrade, and rollback rehearsal.
