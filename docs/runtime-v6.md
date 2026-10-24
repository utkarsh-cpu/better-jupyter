# Runtime migration implementation evidence

The v6 draft is a gated migration, not a release declaration. `PLAN.md` remains
the current runtime plan. The assistant work already in this working tree remains
part of the compatibility surface. No backend default or artifact schema changes
are authorized by performance measurements alone.

The experimental native owner currently supports durable operations, frozen
batches, input claims, interactive CLI input, verified blob ranges, and committed
change/execution feeds. Python remains the default; the continuation sections below
record focused evidence, and `runtime-v6-gates.json` tracks the incomplete exits.
The [UI/backend qualification report](runtime-v6-ui-backend.md) covers explicit
native startup, Rust workspace tests, real service lifecycles, and rendered
Windows Run/checkpoint/restore journeys.
The [native release optimization report](runtime-native-optimization.md) records
the release baseline, stage profiling, validated-result and hash-pass reuse,
event-driven acceptance, final measurements, and regression evidence.

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

## Phase 2 decision (baseline, before the opt-in continuation)

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

## Opt-in native continuation, 2026-10-24

The subsequent request to implement draft phases 3–8 on `runtime/v6-native`
authorizes development beyond the original Phase 2 stop decision. It does not
qualify a native release or change the default backend. The Python helper and
supervisor remain the defaults.

`kernelledger-daemon helper --project PATH` now implements ledger inspection,
output folding, paginated history, verified blob ranges, descriptor inspection,
training-root listing, and source read/watch/recovery/mutation. It does not start
a worker for these methods. Execution history is opened read-only; source
transactions use the Python-compatible source lock and journal. Blob publication
exists as a library primitive; there is no native ledger writer or production
supervisor yet.

To opt in from the desktop, set `KERNELLEDGER_HELPER=native` and point
`KERNELLEDGER_DAEMON` at the built daemon, or place it beside the desktop
executable. Missing daemon paths and unknown helper selectors fail explicitly.
Unset the selector or use `KERNELLEDGER_HELPER=python` to return to Python.
The helper chooses method ownership at startup. With `--python INTERPRETER`,
unsupported utility methods use one lazily started Python helper; without it,
their capabilities are not advertised. Source mutations always stay native.

Local Windows/Python 3.12 verification includes native/Python read parity,
active-WAL visibility, unchanged execution history, empty history, corruption
refusal before returning a blob range, source byte parity, Unicode and numeric
serialization, attachments, split/merge, generated cell IDs, invalid-request
atomicity, stale revisions, concurrent Python/Rust writers, and source journal
recovery in both directions. Test results are recorded in
`artifacts/runtime-v6/native-helper-windows312.xml`. Rust protocol/runtime/client
tests and strict Clippy also pass. The qualification workflow runs helper tests
alongside the existing probe tests on Windows and Linux; a configured missing
binary is a failure, while an unbuilt optional local binary is a skip.

This initial helper/source layer does not qualify a native release. The later
owner implementation below extends it; the machine-readable
[phase gates](runtime-v6-gates.json) track the current remaining work for every
phase from 3 through 8. No existing Python service is retired.

## Experimental native owner continuation

`kernelledger-daemon serve-experimental --project ABSOLUTE_PROJECT --python
ABSOLUTE_INTERPRETER` explicitly starts a native legacy-protocol owner. It does
not change CLI or desktop defaults. Its dedicated ledger thread holds the
common runtime lock, opens schema 2 with WAL/FULL synchronization and foreign
keys, and accepts domain commands through a 64-slot, 8 MiB estimated-memory
budget. Schema 1 is refused with instructions to migrate using Python first.
The generated `schema.sql` comes from Python's DDL and has a drift check.

The native coordinator supports status, run, interruption, hard kill, restart,
stop, checkpoint, restore, and operation lookup. Status and interruption use
separate connection handlers while execution is running. There are at most
32 connection handlers and 16 queued commands. Worker/client envelopes are
limited to 16 MiB; binary payloads retain their existing separate 2 GiB limit
and stream in 1 MiB buffers. These are experimental native limits, not a claim
of full protocol parity. Direct worker pipe bytes are captured in bounded chunks,
as described in the capture and recovery continuation below. The owner remains
experimental.

Legacy effect retries can supply both `client_id` and `client_request_id`.
Acceptance and results are committed before replies; retries reuse the saved
result, and a changed request under the same key is refused. Execution IDs are
recorded as operation progress before dispatch. Interrupted owner recovery
retains unknown outcomes. The desktop continuation below adds durable admission
and frozen batches through a separate protocol surface.

Checkpoint encoding/reconstruction remains Python's worker implementation.
The trusted embedded worker resource also supplies the graph validator in a
separate Python utility process. Native storage verifies the referenced closure
before the checkpoint transaction; corrupt/missing payloads prevent publication.
Restore validates before replacing a worker and preserves imported-source trust
and Python-minor mismatch checks. The Windows token ACL utility remains in the
installed Python package. Training and sharing mutations have not moved to the
native owner, and native GPU snapshots remain unavailable.

The worker is embedded in the daemon and extracted outside `.kl`; execution
needs no repository lookup. `scripts/native_package.py` creates and verifies an
experimental opt-in archive with file hashes, worker identity, source fingerprint,
license-expression inventory, and rollback instructions. A build-time source
fingerprint prevents packaging an old binary as the current source. This is
local packaging evidence, not clean-machine installation or complete license-text
qualification. No executable is downloaded automatically.

To roll back, stop the native owner with the existing CLI, wait for
`.kl/supervisor.json` to disappear, and start the regular Python CLI. The common
lock still decides ownership. Switching ends the live namespace; checkpoint
first where possible. Helper rollback remains `KERNELLEDGER_HELPER=python`.

`scripts/native_gates.py --require-passed` fails while any phase exit requirement
is outstanding and requires two distinct qualified Windows/Linux candidates for
the default switch. `runtime-v6-gates.json` currently records partial phases
3–7 and a blocked phase 8. It explicitly retains the remaining input/feed,
integration, platform fault-injection, performance, and release work. These
requirements are not waived by passing the focused native tests.

### Continuation qualification evidence

The Windows full-suite run on CPython 3.12.10 passed 221 tests with 43 optional
lanes skipped (`artifacts/runtime-v6/native-continuation-windows312.xml`).
The focused WSL2/Linux run passed 34 native probe/helper/supervisor/tooling tests
(`artifacts/runtime-v6/native-owner-linux312.xml`). Rust protocol/runtime/client
tests passed 30 tests on Windows; the headless Rust modules were also exercised
under WSL2. Strict Clippy, formatting, Ruff, schema drift, and hash-fixture drift
checks pass. Skips are not qualification for the unavailable lanes.

The local Windows archive passed seven supervisor tests after extraction outside
the checkout (`artifacts/runtime-v6/native-package-smoke-windows312.xml`). Local
Windows and Linux archives are under `artifacts/runtime-v6/native-opt-in-*.zip`;
both verify their member hashes and carry the same build-source fingerprint.
They are debug-build evaluation artifacts, not signed releases. The Linux
archive's Git identity was read through Windows Git because this checkout's
worktree metadata uses Windows absolute paths; the packager supports `--git`
for this explicit cross-environment case. No Git metadata was rewritten.

## Capture, retry, and discovery continuation

Native pipe readers now preserve stdout/stderr from extensions and child
processes using 16 KiB reads and incremental UTF-8 decoding. Split Unicode
characters survive read boundaries. A bounded 64 KiB startup buffer is retained
as session history once the worker is ready. Pipe and protocol text share the
configured per-execution output limit and one truncation count. Text received
outside an active execution is retained in `session_events` with
`cell_attribution: "unattributed"`, under a separate per-session limit. Idle
protocol messages are drained in bounded passes so background `print()` output
is retained too. Session truncation counts are recorded when capture closes.

Socket and OS-pipe events have no shared total order. Completion allows a bounded
20 ms drain window; later pipe text becomes unattributed history. Overlapping
background writers and process-tree stress remain qualification gates. Closing
capture does not wait indefinitely for an inherited writer handle to reach EOF.
Capture persistence errors fail the execution/worker instead of reporting success.

Keyed retries are checked before reading mutable source. A completed cell run can
therefore return its original result after that cell's source has been removed;
changed request content still fails with `key_reused`. Restore now reconstructs a
candidate before closing the healthy worker. Failed validation, version checks,
or candidate startup/reconstruction preserve the previous namespace and control
route. Partial restore reports keep their existing meaning, and restore outcomes
are recorded as session events.

The native owner now publishes the same hashed registry path as Python. Registry
publication precedes the project readiness hint. Both hints carry a unique runtime
instance ID, and cleanup only removes hints belonging to that instance. The main
owner retains the runtime lock until worker/capture shutdown and metadata cleanup
finish, preventing cleanup from racing a successor's startup. Native launcher
selection remains an open gate. The following continuation adds a desktop subset.

This continuation passed 76 focused tests on Windows CPython 3.12 and 40 on
WSL2/Linux CPython 3.12 (`native-capture-recovery-windows312.xml` and
`native-capture-recovery-linux312.xml` under `artifacts/runtime-v6`). Rust tests
passed 34 Windows protocol/runtime/client cases and 19 Linux headless cases;
strict Clippy, formatting, Ruff, and contract-drift checks are clean. Refreshed
Windows and Linux archives each passed all 13 supervisor integration cases after
extraction outside the checkout (`native-capture-package-*.xml`). These focused
results extend the prior evidence without completing the remaining phase gates.

## Desktop durable admission continuation

The experimental owner now publishes a desktop protocol 1 port and advertises
only `operations`, `batches`, and `blob_ranges`. It supports durable run/batch,
checkpoint/restore/continue, restart/stop, interruption, operation lookup and
cancellation, change pagination, runtime status, and verified blob ranges.
This desktop subset was subsequently extended with input ownership and replies
in the continuation below. Streaming feeds, training, and sharing mutations
remain unavailable. Python remains the default runtime.

Admission freezes source under the source lock and commits the operation,
execution rows, batch row, and source metadata in one FULL transaction before
replying. Request-key retries recover the original operation even after source
changes or a lost reply. Sixteen queue reservations bound accepted and running
work across both protocols. Unsupported numeric request-hash forms are refused
before admission; complete numeric compatibility remains a gate.

Queued cancellation prevents dispatch through an atomic ledger claim. Running
cancellation waits for the worker's running acknowledgment and delivers one
interrupt per execution, avoiding both a missed early interrupt and duplicate
interrupts during Python cleanup. Worker writes are serialized across envelope
uploads and control messages. Durable stop bypasses the admission lock and full
queue, cancels queued work, and completes after worker shutdown. Client sockets
are woken during shutdown before handlers are joined.

The focused Windows suite passed 90 tests and WSL2/Linux passed 54 tests on
CPython 3.12 (`native-desktop-windows312.xml` and `native-desktop-linux312.xml`).
Coverage includes frozen batches, lost replies, key reuse, Python takeover after
crash, queued/running cancellation, large worker envelopes, overload, authentication,
frame limits, blob corruption, and stop while busy. Rust Windows tests passed 36
cases, and strict Clippy passed. These are focused local checks; sustained load,
platform fault injection, complete desktop parity, and release gates remain open.
Both debug evaluation archives were rebuilt from this continuation's source.

The refreshed archives each passed 27 supervisor/desktop integration tests after
extraction outside the checkout (`native-desktop-package-windows312.xml` and
`native-desktop-package-linux312.xml`). Linux headless Rust tests passed 21 cases.
Scoped Rust formatting, Ruff, schema drift, and diff whitespace checks pass.
Workspace-wide Rust formatting still reports existing UI formatting differences
outside this continuation. The phase-gate command correctly refuses qualification.

## Native input ownership continuation

The native desktop surface now advertises `input_claims` and supports
`input.claim`, `input.release`, `input.reply`, and `input.cancel`. Runtime status
reports `waiting_input` and pending prompt views. Claims belong to the authenticated
connection, even when another connection uses the same client ID. Release and
connection cleanup make a claim available to another client without ending the
execution. Claim changes are committed before acknowledgment.

A prompt event and its input request row commit together. An answer event, terminal
input state, and input-state change reference also commit in one transaction before
worker delivery. Failed publication rolls back the complete answer, and repeated
cleanup cannot create a second answer. Native takeover cancels pending input rows
and publishes their final changes. Python takeover cancels them through its existing
recovery path.

Secret replies default to `{password: true, redacted: true}` in durable history.
`record_secret_input` in `.kl/config.json`, read when the owner starts, retains the
existing explicit recording choice. Large input envelopes travel directly from
memory to the worker; they never pass through blob publication. The input tests
inspect ledger/WAL files, blob files, and the daemon log for the supplied secret.

Native execution opts into worker request IDs and per-prompt reply queues. Late
replies to canceled prompts are discarded rather than consumed by a later prompt;
Python executions retain their existing idless input protocol. Large reply
envelopes are decoded into the input queue and discarded from the worker's envelope
cache. Running acknowledgment is inside the execution exception handler so an
immediate cancellation is reported as an execution outcome.

Cancellation and soft interruption also unblock pending input waits. Hard kill,
stop, and execution failure finalize pending rows before owner shutdown. Legacy
noninteractive runs retain closed-stdin behavior. The CLI continuation below adds
interactive relay; streaming subscriptions remain open work. Python remains the default.

Qualification for this continuation: 110 focused Windows CPython 3.12 tests passed;
WSL2/Linux passed 90 with one Windows-only venv-redirector test skipped
(`native-input-windows312.xml`, `native-input-linux312.xml`). The native desktop
suite now has 27 cases, including two-client ownership/handoff, large secret
recording policy, all cancellation paths, reply/cancel races, and Python takeover
with a claimed pending prompt. The Rust Windows suite passed 37 cases and Linux
headless suite passed 22, including failed answer publication, retry deduplication,
redaction, and native pending-input recovery. Scoped formatting, strict Clippy,
Ruff, worker Pyright, schema drift, and contract-fixture drift checks pass.

Both refreshed debug archives passed all 40 supervisor/desktop cases after
extraction outside the checkout (`native-input-package-windows312.xml` and
`native-input-package-linux312.xml`). These are local evaluation results, not
clean-machine release qualification. The phase record continues to require
expanded delivery/disconnect failures, subscriptions,
and the remaining release gates.

## Interactive CLI continuation

Native legacy `run` now honors `interactive: true`. The existing CLI receives
prompts, reads ordinary or password input, and returns replies through the native
owner. Each prompt is automatically claimed by its CLI connection and recorded
with `client_name: "CLI"`. A bounded one-prompt channel connects the execution
coordinator to the client handler; status and control use independent connections.
Noninteractive execution retains closed-stdin behavior.

Disconnect releases the CLI claim for a desktop client to take over, while the
accepted execution continues. Disconnect before a prompt is delivered also leaves
it available for a desktop claim. Interrupt and stop continue to unblock the worker
and finalize pending input. A keyed retry of a completed interactive run returns
the saved result without executing the cell or prompting again.

Reply blobs and envelopes are decoded and verified in memory with a 1 MiB bound
checked before reading the binary body. Malformed envelopes, corruption, and a
mismatched prompt ID release the connection's claim without delivering the reply.
Secret replies use the same atomic redacted answer transaction as desktop replies;
large CLI secret envelopes never enter the project's blob store. Large outgoing
prompts use memory-only envelopes too. Updated CLI clients echo the optional input
ID; existing clients with idless replies remain compatible.

`tests/test_native_cli.py` adds 11 cases, including actual CLI subprocess input
and EOF, long prompt envelopes, two sequential prompts, legacy idless replies,
large secret redaction, disconnect-to-desktop handoff, interrupt/stop, malformed
reply handling, and idempotent retry. The native qualification workflow now runs
these tests on both Windows and Linux. Subscriptions and the remaining phase gates
remain open; Python remains the default backend.

Both refreshed debug evaluation archives passed all 51 supervisor/desktop/CLI
integration cases after extraction outside the checkout
(`native-cli-package-windows312.xml`, `native-cli-package-linux312.xml`). The
broader WSL2/Linux run passed 101 tests with one Windows-only venv-redirector test
skipped (`native-cli-linux312.xml`). Rust tests passed 37 Windows cases and 22
Linux headless cases. Strict Clippy, scoped Rust formatting, Ruff, CLI Pyright,
schema drift, contract-fixture drift, and diff whitespace checks pass. The release
qualification command still refuses the default switch on the remaining gates.
The corresponding broader Windows CPython 3.12 run passed all 121 tests
(`native-cli-windows312.xml`). No optional lane was skipped in that run.

## Committed subscription continuation

The native owner now advertises `change_feed` and supports `subscribe` and
`execution.follow` on subscription connections. An initial subscription reads its
watermark, queued executions, active operations, pending inputs, and active execution
from one SQLite snapshot. A short lifecycle guard keeps the selected worker stable
while that snapshot is assembled. Socket writes begin only after the read transaction
and runtime locks have been released.

Change streams replay strictly after `after_change_seq`; execution streams replay
strictly after `after_seq` and can continue while execution is waiting for input.
All payloads come from committed read-only ledger snapshots. Input claims and
redacted answers appear through the existing input-state changes and execution
events. Reconnecting with the last processed cursor replays remaining records.

Each connection admits at most eight streams. Pages contain at most 64 records,
and each stream sends at most 256 KiB per pass (or one larger valid frame).
Backlogs exceeding 10,000 rows or 8 MiB, and records that cannot fit a 1 MiB frame,
produce `resync_required` with the last sent cursor. The affected stream ends;
the connection remains available for a new request. Queue snapshots beyond 1,000
queued executions also require resynchronization. Clients can use the existing
paged inspection methods to recover large histories.

Frames have an absolute five-second write deadline across partial writes, rather
than restarting the timeout for every partial write. Shutdown is checked between
streams and frames. A slow subscriber holds its own bounded handler and page,
without retaining a read transaction or a coordinator/writer lock. Heartbeats and
client inactivity cleanup remain enabled. Expanded fan-out, heartbeat/reconnect,
and sustained slow-reader qualification remain recorded phase gates.

Ten feed integration cases cover the shared Python feed contract, uncommitted and
rolled-back rows, cursor replay, pending-input/queue snapshots, secret redaction,
live execution output, row/byte/frame resync limits, stream/role/cursor validation,
and control/stop while a subscriber stops reading. The qualification workflow now
runs this suite on both operating systems. Python remains the default runtime.

The broader WSL2/Linux CPython 3.12 run passed 111 tests with one Windows-only
venv-redirector test skipped (`native-feeds-linux312.xml`). Its refreshed debug
archive passed 61 supervisor/desktop/CLI/feed cases after extraction outside the
checkout (`native-feeds-package-linux312.xml`). Rust protocol/runtime/client tests
passed 37 cases on Windows and 22 headless cases on Linux; strict Clippy, scoped
formatting, Ruff, schema drift, contract-fixture drift, and diff whitespace checks
pass. These local results do not satisfy the remaining phase exit or release gates.
The corresponding Windows CPython 3.12 run passed all 131 tests
(`native-feeds-windows312.xml`).
The refreshed Windows archive also passed all 61 extracted-package cases
(`native-feeds-package-windows312.xml`). Both archives remain debug evaluation
artifacts; Python remains the default backend.
