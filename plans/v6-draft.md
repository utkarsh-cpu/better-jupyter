# Runtime v6 draft: backend migration

Status: Draft, written 2026-10-24. Baseline, Python storage, and native-probe evidence is in [the runtime migration record](../docs/runtime-v6.md). That record keeps the Python supervisor and leaves the native daemon as an explicit probe. This draft is not a replacement for the current runtime plan.

Owner: Runtime track. Requires coordination with the desktop client and packaging. The current authority remains [runtime v5](../PLAN.md). Publication must follow [plan versioning](README.md), reconcile the [assistant draft](ai-assistant-draft.md), and assign the next available runtime version. Both drafts currently propose v6; neither independently reserves that number.

## 1. Decision and scope

Move toward a Rust supervisor, durable storage services, and offline helper while retaining a standalone Python worker for notebook execution, live-object checkpoint encoding, reconstruction, and scientific/GPU integrations. Improve and measure the current Python implementation before authorizing the full port on technical grounds. Python version qualification and Rust migration are separate workstreams with separate release decisions.

The intended outcome is predictable responsiveness during large transfers, bounded backend memory, reliable process lifecycle management, and simpler native desktop distribution. A Rust rewrite does not inherently accelerate SQLite, SHA-256, NumPy, PyTorch, or GPU computation; these already perform substantial work in native code. Improvements must come from reduced copying, less repeated work, better ownership, and measured scheduling changes.

Delivery includes the legacy CLI protocol, desktop protocol, offline helper, source transactions, execution history, checkpoints, bundles, registry, Windows/Linux process control, and existing experimental training integration. Preserve assistant interoperability, but leave its provider service and credential lifecycle in their existing separate process.

Out of scope: replacing Python execution, embedding CPython into the desktop process, distributed scheduling, a cloud database, collaborative editing, native GPU process snapshots, a new checkpoint format, or a frontend redesign. Supporting additional notebook languages would require a separate worker and checkpoint contract.

## 2. Evidence and starting point

The following are code observations and recorded measurements, not a new benchmark or compatibility certification:

- [Project configuration](../pyproject.toml) permits Python 3.11+, targets 3.11 for Ruff/Pyright, and pins the GPU extra to PyTorch 2.12.0. [CI](../.github/workflows/tests.yml) tests Python 3.11–3.13 on Windows and Ubuntu, with and without optional codecs.
- The [supervisor](../src/kernelledger/supervisor/__init__.py) already separates Python execution into another process, batches stream events, commits periodically, and uses durable completion boundaries.
- [Blob storage](../src/kernelledger/blobs.py) streams incoming payloads, but performs file writes and synchronization within async processing. `record_blob()` obtains byte size by reading and hashing the entire blob. Restore also reads whole blobs before sending them.
- [The worker](../src/kernelledger/worker.py) concatenates frame headers with payload bytes when sending and collects received binary chunks into complete byte strings. Improving supervisor memory alone will not eliminate worker allocations.
- [The ledger](../src/kernelledger/ledger.py) uses SQLite WAL, full synchronization, and one writer. Keep these foundations.
- [The source service](../src/kernelledger/source.py) has a separate source lock, revision checks, transaction recovery, and canonical serialization. Porting it requires filesystem parity, not merely matching JSON fields.
- The [recorded supervised benchmark](../benchmarks/supervised.json), using Python 3.12.10 on Windows, reports 3.51 seconds to checkpoint a 256 MiB array workload, 4.50 seconds to restore, and 4.68 seconds for an unchanged checkpoint with zero new blob bytes. This suggests investigation of repeated work; it does not identify a proven bottleneck.
- The [recorded footprint](../artifacts/footprint-release-2026-10-18/README.md) reports 71.02 MiB total private resident memory across four processes. It includes the UI and cannot serve as a backend-only baseline.
- The workspace already contains changes to runtime, desktop, source handling, exports, and the assistant. Implementation must inventory and preserve that work rather than assuming a clean historical baseline.

## 3. Language and runtime choices

### Rust for native runtime services

Rust is the preferred implementation language because the desktop and client already use it. Reuse contract fixtures and model definitions where they are transport-neutral. Keep backend crates independent of GPUI so headless builds never compile a graphical runtime unnecessarily.

Proposed workspace location: add crates under the existing `desktop/crates/` Cargo workspace initially, despite its name. This avoids a simultaneous workspace relocation. A later repository-wide Cargo reorganization is an independent housekeeping change.

Start with three new crates, using internal modules rather than immediately creating one crate per subsystem:

- `kernelledger-protocol`: framing, request/response types, validation, error codes, and compatible canonical request hashing. Reuse or extract appropriate parts of `kernelledger-models`; avoid creating two competing definitions.
- `kernelledger-runtime`: blob store, ledger owner, source transactions, operation state machines, platform adapters, worker transport, and supervisor services.
- `kernelledger-daemon`: a headless executable with supervisor and offline-helper modes. It is a separate process from the UI and exposes only the capabilities appropriate to its mode.

Evaluate Tokio for async process/socket/timer management and rusqlite on a dedicated database thread. These are candidates, not frozen dependencies. A short platform spike must settle Windows process creation, existing smol client coexistence across process boundaries, SQLite build configuration, licensing, toolchain support, and binary size. Do not move the frontend to another executor as part of this work.

### Python remains a first-class runtime

Keep the worker runnable by absolute path in a project interpreter without importing the installed KernelLedger package. Preserve its isolation flags, notebook import behavior, optional-codec discovery, and absence of `.kl` paths. Keep AST/source provenance and object-graph semantics in Python.

Retain Python implementations of document conversion/export, assistant providers, and framework adapters initially. Native services may call a narrowly scoped Python utility process for conversion, but that process must not acquire ledger-writing authority or execute notebook code as part of inspection. The normal helper path must not eagerly import Torch or document-export dependencies.

### Alternatives considered

- Go is a viable separate remote-service choice, but adds a third language here and does not reuse the desktop's Rust models. Revisit only if remote fleet operation becomes the primary product requirement.
- C++ is reserved for a concrete native API integration that cannot reasonably be handled in Rust. It offers no demonstrated advantage for the present orchestration workload.
- An optimized Python-only backend remains a valid outcome if it meets responsiveness, memory, reliability, and packaging goals and the Rust prototype offers insufficient additional benefit.
- Free-threaded Python and subinterpreters are experiments, not substitutes for process isolation. Subinterpreters share a process and extension compatibility varies. They cannot preserve independent worker process termination semantics.

## 4. Invariants that every migration phase must preserve

1. Exactly one active ledger writer per canonical notebook. Both implementations must contend on the same OS lock, including the same byte range on Windows. A PID or registry entry is never proof that a lock can be stolen.
2. The worker does not receive runtime state paths and cannot directly publish ledger rows or blobs into `.kl`.
3. A checkpoint becomes published at the ledger transaction commit, after its referenced durable payloads and manifest exist. Failure before that point leaves no published checkpoint. Orphan payloads remain eligible for explicit GC.
4. Acknowledged durable operation acceptance and terminal results survive restart. Losing a reply never justifies repeating a side effect with a new request key. Recovery preserves `unknown` and `incomplete` outcomes where appropriate.
5. Inspection does not execute saved Python or start a worker. The offline helper may mutate tracked source through the source transaction service; it cannot write execution history. This distinction must be reflected in tests and documentation.
6. Source updates remain revision checked, recoverable, and byte-compatible with canonical file serialization. Frozen batches retain their admitted source and ordering.
7. Input ownership, secret handling, output limits, truncation notices, event ordering guarantees, and committed-only subscriptions remain intact.
8. Restore retains complete/partial/refused/failed distinctions, alias/cycle behavior, exclusions, source provenance, trust checks, and Python-version mismatch protection.
9. Windows Job Object behavior, hidden process launches, token ACLs, Linux process groups, and permission checks retain their existing semantics. A worker process is not a security sandbox for malicious notebook code.
10. Native process capture stays unavailable unless separately qualified. Rust migration must not accidentally advertise unsupported GPU capabilities.

Translate each invariant into a named acceptance scenario before replacing its owner.

## 5. Ownership and execution model

The desktop and Python CLI connect to the selected supervisor through existing protocols. The supervisor starts the project Python worker and owns the notebook runtime lock. Offline inspection and source editing use helper mode. Closing the UI does not implicitly stop the supervisor or erase the live namespace.

Within the native supervisor:

- An operation coordinator owns execution, batch, input, and lifecycle state. It serializes conflicting effects and handles durable idempotency lookup before admission.
- A dedicated ledger thread owns its SQLite connection. It accepts bounded, domain-level transaction requests, not arbitrary closures from network handlers. Completion replies are sent after the required commit.
- Blob I/O uses a bounded blocking pool or dedicated streaming workers. Queue limits account for bytes as well as task counts. Control messages must remain serviceable while bulk I/O is active.
- Read-only query connections serve committed history using short-lived snapshots. No reader uses SQLite's immutable flag on a live database that may change.
- Transport tasks parse bounded frames and forward validated messages. They do not mutate lifecycle state independently.
- The worker transport preserves one admitted execution at a time. Source snapshots and worker readiness are verified before dispatch.

Do not hold the ledger transaction or coordinator mutex across a long blob transfer, a worker reply, or a source-lock wait. Define lock ordering for operations touching source and ledger state, and test it with two clients.

Assign proposed queue budgets during baseline work. On overload, backpressure transfers, reject work before durable acceptance with a documented retryable response where compatible, or retain already accepted work durably. Never silently discard accepted operations. Slow subscription consumers may be disconnected and resynchronized using existing cursor semantics.

## 6. Storage and checkpoint optimization

### Blob path

Introduce an internal verified-blob result containing hash, byte size, and publication outcome. Only the storage verifier can construct it. Use it to record metadata without rereading payload bytes merely to learn their size.

Stream large files through bounded buffers during receive, verification, and restore transmission. Avoid concatenating headers and multi-megabyte payloads into a second allocation. Preserve exact framing and transfer hashes. Audit both worker and supervisor allocations before claiming bounded end-to-end memory.

Verification remains explicit: validate incoming/untrusted bytes, verify existing referenced content according to the current contract, and detect corruption during restore/import/doctor. Do not replace verification with `stat()` or an indefinitely trusted hash cache. Operation-local verification reuse must account for file replacement and mutation; choose and document the integrity policy before optimizing repeated reads.

Durable publication retains temporary-file writes, flush/sync, rename, and the appropriate platform directory durability behavior. Interrupted transfers cannot expose partial blobs. GC must not race in-progress publication; coordinate it through the runtime owner and existing lock rules.

### Ledger and feeds

Keep schema 2 and WAL/full synchronization initially. Preserve transactions, triggers, uniqueness rules, and foreign-key behavior. Batch stream event writes within the existing durability window, while maintaining immediate lifecycle commits required by the contract.

Record commit latency and WAL growth. Define busy handling, bounded retries, and maintenance checkpoint policy without starving control traffic. Validate backup and offline behavior with WAL state present; copying only the database file while it is active is not an acceptable backup strategy.

Use query plans to decide indexes for operation status, execution status, and history queries. Retain current cursor semantics for the parity release. Any move from offset to keyset pagination requires an explicit compatible extension or protocol revision, not reinterpretation of existing cursors.

### Repeated checkpoints

Measure graph walk, codec encode, hash, IPC, duplicate verification, filesystem synchronization, and ledger publication separately. Remove redundant work before considering incremental formats.

Do not treat unchanged Python object identity as evidence of unchanged content. Mutable arrays, aliased buffers, containers, and framework state can change without identity changes. Incremental checkpointing requires a separately reviewed mutation/digest strategy and is deferred beyond the parity migration.

## 7. Python version qualification

Qualify standard CPython 3.14 and 3.15 in addition to the existing supported versions. Python 3.15 is released, but release status alone does not establish compatibility with the project's pinned GPU build or checkpoint semantics. Use the latest maintained patch release of each chosen minor at implementation time and record exact interpreter/build identities in results.

Keep the worker's 3.11 syntax floor during the migration unless an independently approved compatibility change raises it. Updating the development interpreter does not require changing `requires-python`, Ruff target, or worker syntax. Split tooling configuration by component if native utilities or Python utility services later require a newer minimum.

Qualification sequence:

1. Run core tests on Windows and Linux for 3.11–3.15, with isolated environments and explicit dependency resolution reports.
2. Add NumPy, pandas/PyArrow, and plotting lanes on each claimed supported combination. A missing wheel is a recorded unsupported combination, not a silently successful test skip.
3. Run the existing Python supervisor with each supported worker minor, prioritizing the newly selected supervisor version with every retained worker version. Then run the native supervisor against those same worker builds.
4. Exercise functions, classes, closures, dataclasses, cycles, random state, AST handling, encoding, subprocess output, interruption, and restore provenance. New Python built-ins or syntax must be explicitly supported or yield the existing documented exclusion/refusal behavior.
5. Retain same-minor restoration as the default policy. Test cross-minor restore only under the existing explicit override and report compatibility failures honestly. Preserve access to old environments so existing checkpoints remain usable.
6. Qualify GPU combinations separately: pinned Torch version, Python minor, OS, accelerator backend, driver, architecture, and training example. Do not upgrade Torch and Python simultaneously when diagnosing regressions.
7. Compare startup, idle memory, checkpoint/restore, and responsiveness before selecting a newer Python utility/supervisor default. 3.14 is the conservative first candidate; 3.15 can become the choice when its gates pass.

Free-threaded and JIT builds are optional benchmark lanes after standard builds pass. Record actual GIL/build status, extension behavior, correctness, and memory. Do not claim that parallel graph walking is safe without a consistent namespace snapshot and explicit thread-safety design.

## 8. Compatibility across implementations

### Protocols

Retain worker, legacy, desktop, and helper protocol versions for the first native release wherever behavior is unchanged. Preserve their distinct frame limits: do not apply the desktop's 1 MiB JSON limit to the worker's 64 KiB inline-header contract. Cover envelopes, binary frames, partial reads, EOF, oversized lengths, invalid UTF-8, unsupported versions, and unknown methods.

Use the existing Python request catalog and generated schemas as the initial contract authority. Add Rust types and validators against shared fixtures; only change the generation authority in a separate reviewed step.

Request hashing requires exact parity for previously accepted requests. Python and Rust JSON serializers can differ in whitespace, escaping, numeric representation, booleans, null handling, and map ordering. Define and test the existing hashing algorithm byte-for-byte, including exclusion of transport request IDs. If exact parity cannot be achieved, design an explicit versioned hash compatibility mechanism before native operation admission; do not invalidate old idempotency keys.

### Files and databases

Use existing notebook, checkpoint, descriptor, bundle, and ledger formats. Keep source serialization bytes, SHA-256 references, relative path rules, UUID handling, timestamps, and operation outcomes compatible. Check integer ranges and Unicode behavior at JSON/database boundaries.

A backend implementation choice is machine-local configuration, never tracked notebook metadata. Proposed values are `python`, `rust`, and later `auto`; the name and location must be frozen during contract design. Startup metadata must identify the implementation/build without breaking old readers.

### Backend switching and rollback

Switch only through stop, recovery verification, and restart under the common runtime lock. Changing supervisors ends the live worker namespace; do not imply live handoff. Offer a checkpoint beforehand where possible, with explicit partial/refused outcomes.

Preserve the Python backend during rollout. Verify that it can reopen ledgers written by Rust, and that Rust can reopen Python-produced ledgers, after clean shutdown and simulated crashes. Never run both writers on the same notebook, including during differential testing.

Automatic fallback is allowed only before acquiring ownership and accepting side effects, and only when compatibility is known. After acceptance or an uncertain outcome, recover and query the durable operation; never replay automatically on another backend.

If a later change needs a new schema, stop assuming downgrade compatibility. Require explicit migration, verified backup, version checks, and a documented data-preserving rollback boundary. Do not restore an old backup over newer user history as an automatic fallback.

## 9. Delivery phases and exit gates

### Phase 0 — Freeze the baseline and contracts

Deliver a tracked capability inventory mapping every CLI command, desktop/helper method, worker message, persisted format, and platform behavior to its implementation and test. Include source editing, export, bundles, training, assistant integration, and stopped-runtime commands.

Capture fresh performance results at an identified commit or reproducible working-tree snapshot. Profile checkpoint and restore; measure backend-only memory separately from UI and Python worker. List discrepancies between code and current documentation for explicit resolution.

Exit: reproducible workloads and environment manifests; invariant acceptance scenarios; an agreed feature matrix; no unexplained baseline test failures.

### Phase 1 — Improve Python and qualify newer interpreters

Introduce verified blob metadata, remove avoidable allocations, instrument stage timings, and strengthen high-value protocol/state types. Move blocking work only where measurements justify it, respecting SQLite thread ownership. Add the Python qualification lanes from section 7.

Exit: targeted crash/corruption tests pass, fresh results show the effect of each optimization, and each Python/dependency combination has an explicit status. This optimized implementation becomes the Rust comparison baseline.

### Phase 2 — Native feasibility and decision gate

Build a headless prototype that authenticates, launches the existing worker, executes a trivial cell, streams output and a large blob, and terminates its process tree on Windows/Linux. Demonstrate common-lock contention between Python and Rust implementations, including source locks. Build with no GPUI dependency.

Settle executor, SQLite adapter, platform APIs, dependency versions, and distribution approach in short decision records. Benchmark against Phase 1.

Exit: continue to a full Rust port only if the prototype demonstrates a credible, measured resource/latency or packaging benefit with manageable compatibility cost. If it does not, retain the Python improvements and stop native expansion. Document the evidence either way.

### Phase 3 — Native read-only helper

Implement history/status inspection, output folding, blob ranges, checkpoint/descriptor inspection, and pagination. Keep the Python helper as fallback selected at process startup for methods not yet implemented. Do not silently route a side effect to a different owner.

Use the Python helper as a reference over identical immutable test copies. Test active-WAL reads separately with the writer running. Ensure inspection starts no worker and makes no execution-history writes.

Exit: all designated read-only methods match fixtures and integration behavior; large histories remain bounded; desktop browsing works without a Python helper for the native-supported subset. Unsupported capabilities are accurately advertised.

### Phase 4 — Native storage and source transactions

Implement blob verification/publication, ledger transactions and recovery, and source serialization/locking/transaction recovery. Run storage write tests only on isolated copies until the supervisor owns these services.

Port source mutation before replacing helper mode wholesale. Verify Python/Rust concurrent edit conflicts and interrupted transaction recovery in both directions. Exercise Windows case-insensitive paths, non-ASCII names, long paths, symlinks/reparse points, and path traversal defenses according to current supported behavior.

Exit: corruption, disk-full, permission, crash-publication, source recovery, and bidirectional reopen tests pass. No schema bump is required for parity.

### Phase 5 — Native supervisor and durable operations

Integrate worker startup/readiness, execution, status, interruption, hard kill, restart, stop, batches, input ownership, output limits, operations, and committed subscriptions. Support CLI and desktop paths, including lost replies and reconnects.

Preserve platform-specific process lifetime guarantees and registry behavior. Cancel/stop tests include unresponsive workers and child processes. Readiness failure must never advertise a usable worker.

Exit: execution/lifecycle suites run against both backends; accepted requests have correct recovery outcomes; queue overload and slow subscribers cannot stall unrelated control indefinitely.

### Phase 6 — Checkpoints, sharing, and integration parity

Integrate checkpoint publication, restore/replay/continue, pins, GC, bundle import/export, descriptors, source conversion, and document-export utility dispatch. Reuse Python object encoding and reconstruction. Preserve portable training APIs and all refusal paths for unsupported native GPU capture.

Test assistant read tools and revision-checked proposal application against the native helper/source service. Keep assistant records and credentials separate from runtime state.

Exit: every capability in the Phase 0 inventory is implemented, explicitly retained in a compatible Python utility, or formally deferred without being advertised. Cross-backend round trips and real training scenarios pass for each claimed target.

### Phase 7 — Packaging and opt-in release

Ship a versioned daemon and worker resource with desktop builds. Define worker-resource lookup, integrity/version validation, installed-app paths, and updates while an older daemon is running. Never depend on a repository checkout at runtime.

Keep `pip`/`uv` Python-only installs functional during transition. Native daemon acquisition must be explicit: package it through the desktop distribution or documented optional platform artifacts; do not silently download executables at first launch. Keep the existing `kl` entry point as a compatible launcher/client initially.

Provide diagnostics for selected backend, daemon/worker versions, interpreter, protocol capabilities, SQLite build, lock owner, and actionable compatibility errors. Keep secrets, cell contents, and variable values out of ordinary diagnostics.

Exit: clean-machine install/upgrade/uninstall smoke tests on Windows/Linux, license inventory, release artifact verification, rollback rehearsal, and an opt-in end-to-end release with accurate limitations.

### Phase 8 — Default switch and retirement

Switch the desktop's default only after the release gates below pass and at least two opt-in release candidates complete the qualification workloads without unresolved durability regressions. This is a proposed release policy, not a calendar promise.

Retain explicit Python selection for at least one subsequent supported release. Remove duplicated supervisor/helper code only after usage and compatibility evidence supports it, docs are updated, and the CLI distribution story is settled. Python worker and utility code remain supported.

Exit: native default, documented recovery/rollback, and no unowned capability or silently unsupported command.

## 10. Test strategy

Parameterize behavioral integration tests by backend. Avoid coupling assertions to Python private classes where a protocol or durable artifact can express the contract. Keep implementation-specific unit tests where they verify algorithms or platform adapters.

Required suites:

- Protocol golden fixtures shared across Rust/Python; malformed and fragmented frames; canonical request hashes; numeric boundaries; unknown/extra fields and exact error behavior.
- Existing graph/property tests for Python checkpoint semantics and cross-backend artifact round trips. Check mutable aliases, unsupported descendants, corrupted blobs, source provenance, and version mismatch.
- Fault injection before/after payload rename, manifest rename, acceptance commit, checkpoint commit, terminal result commit, and reply delivery. Restart under the other backend and check recovery.
- Process tests for authentication failures, worker launch errors, crashes, soft interruption, hard kill, detached lifetime, parent exit, and owned child cleanup.
- Two-client scenarios for frozen batches, cancel races, input claims, disconnect/reconnect, lost replies, source conflicts, feed cursor replay, and resynchronization.
- Storage tests for partial reads/writes, disk-full, read-only locations, WAL readers, large histories, malformed bundles, unsafe paths, corrupt hashes, and GC/publication coordination.
- Desktop/CLI/helper end-to-end tests, including opening source/history without a worker and source edits with the supervisor stopped.
- Qualified GPU scenarios and assistant regression tests, with unavailable hardware clearly distinguished from a pass.

Run Python tests, Ruff/Pyright, Rust format/clippy/tests, schema generation drift checks, and package smoke tests as appropriate. Pull requests run a focused stable matrix; nightly/release jobs run the expanded interpreter, failure-injection, performance, and hardware matrix. Compatibility claims require passing qualification, even when a lane is too expensive for every PR.

## 11. Measurement and release criteria

Build a reusable benchmark command producing machine-readable results with commit/build identity, dirty-tree status, Python/codec versions, OS, CPU, memory, storage, SQLite settings, and workload parameters. Record cold versus warm runs separately. Use monotonic timing; collect enough samples for the statistic being claimed. Use at least 30 repetitions for short control/startup operations and at least five for expensive payload runs; report median and spread for small samples rather than a misleading p95.

Workloads: empty startup; idle runtime; short cell; many small outputs; sustained stdout at the configured cap; 256 MiB and 1 GiB array checkpoints when hardware permits; repeated unchanged checkpoint; aliased/mutated arrays; large object graph; restore; large history pagination; checkpoint while another client queries status; slow subscriber; concurrent source edits; training capture/restore.

Proposed engineering targets, to calibrate in Phase 0 rather than present as achieved results:

- Zero loss of acknowledged durable operations/checkpoints in the defined crash suite; no duplicate effect caused by lost replies; no new trust/permission regressions.
- Status/control acknowledgment p95 below 100 ms on the documented local reference machine during bulk transfer. Measure interruption acknowledgment separately from actual termination of uninterruptible native code.
- Supervisor/helper memory overhead remains bounded by configured buffers and queues as blob size increases from 256 MiB to 1 GiB. Measure worker and total application peaks separately.
- No unexplained regression above 10% in baseline startup, checkpoint, restore, or history workloads. Investigate noisy measurements before making a decision.
- To justify the native default, demonstrate at least one material benefit over optimized Python: for example, 25% lower backend-only resident memory, 25% faster startup, or 25% better control latency under load, without sacrificing other release gates. Packaging benefits may justify proceeding instead, but require a documented clean-machine comparison and decision record.
- Full method/command inventory coverage, successful cross-backend rollback, and passing installation tests for every advertised OS/architecture.

Instrumentation records operation IDs, stage durations, bytes read/written/hashed/transferred, queue occupancy, database wait/commit duration, worker launch time, and recovery outcome. Diagnostic collection is local and opt-in for detailed traces; it does not include payload contents or credentials.

## 12. Suggested change sequence and effort

Each numbered item should be a reviewable change or small series with its own exit evidence:

1. Baseline benchmark/diagnostics and capability inventory.
2. Python blob metadata/copy optimizations with corruption and crash coverage.
3. Python 3.14/3.15 qualification and compatibility documentation.
4. Shared protocol fixtures, hashing specification, and backend test selection.
5. Native platform/worker prototype and feasibility decision.
6. Native read-only helper.
7. Blob/ledger storage implementation and cross-backend recovery tests.
8. Source transactions and helper mutation parity.
9. Supervisor lifecycle and legacy execution protocol.
10. Desktop durable operations, batches, input, and feeds.
11. Checkpoint/restore/sharing parity.
12. Training, assistant, and export integration.
13. Packaged opt-in release and rollback diagnostics.
14. Default switch after qualification; later retirement as a separate change.

Dependency order: baseline precedes optimization and interpreter qualification; contract fixtures precede native parity work; prototype decision precedes the full port; storage/source/lifecycle precede checkpoint and integration parity; packaging and default switching follow complete qualification. Python qualification can proceed alongside contract work, but do not mix language-port defects and interpreter upgrades in the same unisolated test result.

Planning estimate for one experienced maintainer: 1–2 engineer-weeks for baseline/contracts, 1–2 for Python improvements and interpreter qualification, 1–2 for the prototype, 2–4 for storage/helper/source parity, 3–5 for supervisor and integrations, and 2–3 for packaging/qualification. Total: roughly 10–18 engineer-weeks if the full port proceeds, excluding extended hardware acquisition or ecosystem blockers. Re-estimate after Phase 2; these are effort ranges, not promised completion dates.

## 13. Risks and decisions to resolve

- Durability divergence: control with identical crash points, verified backups, full synchronization, and a common writer lock. Treat any published row referencing missing payloads as a release blocker.
- Cross-language serialization differences: freeze exact hashes and source bytes before native writes. A semantically equivalent JSON object is insufficient for content-addressed compatibility.
- Windows process/lock differences: prove them in the earliest prototype rather than discovering them after storage is ported.
- Misattributed speed gains: compare native code to the optimized Python baseline using identical worker versions, dependencies, data, and storage settings.
- New-Python ecosystem gaps: select supported worker versions independently; preserve older project environments and qualify GPU paths explicitly.
- Assistant and frontend work in flight: keep the v6 draft coordinated with their contract changes. Native extraction must not drop newly added methods.
- Temporary dual maintenance: keep one contract catalog, shared fixtures, and a retirement gate. Avoid two independently evolving feature sets.
- Packaging complexity: choose executable delivery and worker-resource ownership in Phase 2, then validate on machines without a developer checkout.

Default decisions in this draft: Rust preferred; Python workers retained; existing protocols and schema retained; explicit opt-in native selection first; Python CLI retained; no remote service; no in-process CPython embedding. Dependency versions, initial buffer budgets, exact machine-local selector, and final default Python minor are settled by the designated measurement/prototype gates, not by user intervention for routine implementation choices.

## 14. Plan publication and definition of done

Before declaring the new runtime authoritative, merge the accepted decisions into the next current runtime plan, archive the prior plan verbatim according to repository rules, update its compatibility table and implementation stack, and reconcile the assistant draft. Update frontend dependencies only as required by actual contract changes. Plan, artifact, protocol, and package versions remain independent.

The migration is complete when the chosen backend meets all invariants, covers the declared capabilities, passes its supported-platform matrix, has reproducible performance evidence, ships through supported installation paths, and has a rehearsed recovery procedure. If the Phase 2 decision retains Python, completion instead means the measured Python improvements, interpreter qualification, and documented native feasibility result; do not report a Rust migration as completed.

## 15. References

- [Current runtime plan](../PLAN.md), especially sections 3, 5.1, 8, 9, 11, 12, 15, 17, 18, 20, and 24.
- [Implementation notes](../docs/implementation.md), [desktop protocol](../docs/desktop-protocol.md), and [desktop coverage](../docs/desktop/coverage.md).
- [Python 3.15 release information](https://www.python.org/downloads/release/python-3150/): release availability; not a project compatibility guarantee.
- [Python free-threading documentation](https://docs.python.org/3/howto/free-threading-python.html): extension compatibility and actual GIL status must be checked.
- [Python multiple-interpreter documentation](https://docs.python.org/3.14/library/concurrent.interpreters.html): process-level isolation is not supplied by subinterpreters.
- [SQLite WAL documentation](https://www.sqlite.org/wal.html): reader/writer behavior, checkpointing, and filesystem limitations. Keep active ledgers on supported local filesystems.
