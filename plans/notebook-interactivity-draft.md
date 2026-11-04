# KernelLedger notebook interactivity plan

| Item | Decision |
| --- | --- |
| Date | 2026-10-25 |
| Status | Development implementation across N0–N10; supported subsets and remaining release gates are recorded in [the implementation report](../docs/notebook-interactivity.md). |
| Tracks | Python worker, Python/Rust supervisors, desktop protocols, GPUI desktop, CLI and interoperability. |
| Baseline | Current [runtime v5](../PLAN.md), [frontend v4](../FRONTEND_PLAN.md), implemented working tree, and the evidence linked below. |
| Coordination | Combine with the [Rust migration draft](v6-draft.md), [assistant draft](ai-assistant-draft.md), and [frontend clarity draft](frontend-v6-draft.md) before publication. This document reserves no runtime or frontend version number. |
| Default | Python supervisor and the custom CPython worker remain the default. Rust stays opt-in. IPython is an optional execution-engine candidate, not an adopted dependency. |
| Delivery | Eleven increments, N0–N10, with frontend, backend and acceptance work in each. A phase is complete only when its backend and user journey pass. |

## 1. Outcome and scope

Make KernelLedger comfortable for everyday interactive Python work while retaining
its durable execution history, ordinary source files, typed checkpoints and native
desktop. Cover all twelve feature groups discussed: completion/documentation,
inspection, timing/profiling, asynchronous cells, input/output shortcuts, cell
magics, debugging, widgets, richer rendering, a shared console, notebook workflow,
and Jupyter interoperability.

This is a delivery plan, not a claim of IPython or Jupyter compatibility. Some
features belong to IPython, others to ipykernel, Jupyter frontends or widget
libraries. Adding IPython alone does not provide debugger UI, widgets, a Jupyter
transport, notebook persistence or KernelLedger checkpoint semantics.

Keep GPUI and the existing process separation. Multiple-language execution,
collaborative editing, remote hosting and native process/GPU snapshots require
separate plans. Preserve existing assistant and experimental training work.

## 2. Starting point and unfinished work

| Area | Implemented evidence | Work still required |
| --- | --- | --- |
| Execution | [worker.py](../src/kernelledger/worker.py) uses a persistent namespace, AST source indexing, `compile`/`exec`, and final-expression evaluation. Both supervisors use a standalone project Python worker. | Isolate the execution engine; add the features below without changing source provenance or restore guarantees. |
| Notebook syntax | Token-aware `!`, `!!`, and selected `%pip`, `%cd`, `%pwd` handling exists. | Timing, history shortcuts, suppression, inspection syntax and cell magics; test rather than assume IPython equivalence. |
| Output | Worker MIME hooks, display/update/clear events, streams and Matplotlib capture exist. GPUI has paging, images and structured output handling. | Audit renderer coverage; add missing accessible renderers and qualified interactive output. A worker MIME hook is not proof the UI can render that MIME. |
| Desktop | GPUI editor, source revisions, batches, history, checkpoints, runtime controls, interpreter selection and interchange surfaces exist. | Completion, help, live inspection, debugging, console and widget interactions. Audit existing shortcuts and workflow before adding duplicates. |
| Interchange | `.ipynb` import/export, document export and bundles already have contracts and implementations. | Stronger round-trip fixtures, new-output fallbacks and an optional Jupyter kernel bridge. |
| Assistant | Provider/service, context/proposal review and UI code exist in the checkout. | Reconcile draft language claiming no AI code exists; preserve explicit proposal application and execution acceptance. Live namespace data stays outside automatic assistant context. |
| Native runtime | Durable operations, bounded admission, input claims, feeds, verified storage, native launch and initial optimization/qualification exist. | Outstanding [native gates](../docs/runtime-v6-gates.json) remain binding. No default switch follows from this plan. |
| Numeric requests | [Boundary and raw-wire tests](../docs/runtime-native-numeric.md) cover exact integer hashes and refusal/recovery. | Full Python numeric/hash and malformed-input error parity remain open. Widget floats must not bypass durable hash qualification. |

The [optimization report](../docs/runtime-native-optimization.md) and
[UI/backend report](../docs/runtime-v6-ui-backend.md) contain local measurements
and tests. They do not qualify the new features. Current root plans exclude
several features now partly implemented or proposed here; resolve those conflicts
through the [versioning process](README.md), rather than silently editing current
guarantees. The live namespace inspector and any embedded webview remain gated
until their new contracts and platform checks are adopted.

## 3. Architecture and execution-engine decision

Keep this ownership chain:

```text
GPUI desktop / CLI / optional future Jupyter adapter
                  |
           selected supervisor             offline helper
           Python or Rust                  source + recorded history
                  |
         standalone project Python worker
                  |
       custom engine / experimental IPython adapter
                  |
            notebook user namespace
```

The supervisor implementation and Python execution engine are independent
choices. A Rust supervisor still runs Python code in the Python worker. Engine
selection is fixed for a worker session; changing it starts a new session and
never silently changes a running namespace.

### 3.1 Common worker boundary

Introduce a worker-side engine interface for execute, complete, inspect,
namespace metadata, lifecycle and optional debug/comm hooks. Keep source indexing,
input mediation, event transport, checkpoint encoding and restore admission
outside engine-specific code. Preserve standalone execution without importing the
installed KernelLedger package or receiving `.kl` paths. If code is split into
support files, update resource extraction, build identity and packaging together.

N0 compares two implementations:

1. **Custom engine, retained default.** Extend the existing implementation for
   bounded completion/inspection, execution conveniences and defined async
   behavior. Maintain an honest compatibility subset.
2. **Optional IPython adapter, experimental.** Prototype `InteractiveShell`
   integration inside the isolated worker, initially behind an explicit startup
   option. Route its display publisher, display hook, input, errors and execution
   counts through KernelLedger. Avoid a second history authority. Test source
   transformations, future imports, async, functions/classes and checkpoint
   reconstruction. Select dependency versions compatible with each supported
   project Python version; do not require users to install an incompatible latest
   IPython or change their environment automatically.

Produce an ADR with measured startup/RSS, compatibility results, dependency and
license costs, and the maintenance cost of each path. Prefer the adapter for broad
IPython compatibility only if it passes provenance, input, output, interrupt and
checkpoint tests. Otherwise ship the custom subset and keep the adapter gated.
Do not maintain two competing parsers for the same engine. Implement features
against the shared interface so the decision does not require a frontend rewrite.

Checkpoint manifests continue to describe supported user objects, not an entire
IPython shell or process. Internal engine objects, history caches, debugger
frames, tasks and widget transports are not silently serialized. Record the
engine/version/transformer identity in session provenance with a reviewed schema
change where needed. Cross-engine restore is gated by tests; default refusal with
a useful explanation is preferable to silently restoring different semantics.

### 3.2 Frontend and backend module boundaries

| Layer | Existing starting points | Planned change |
| --- | --- | --- |
| Python execution | `src/kernelledger/worker.py`, `src/kernelledger/execution/` | Engine boundary, completion/inspection handlers, async runner, magic registry, transient caches and debug/widget adapters. Keep notebook execution on its execution thread. |
| Python owner | `src/kernelledger/supervisor/`, `desktop/server.py`, `desktop/contracts.py` | Capability routing, worker-generation checks, read request limits, durable side effects and new lifecycle states. |
| Rust owner | `desktop/crates/runtime/src/{supervisor,desktop,inputs,feeds,writer,resources}.rs` | Matching routing, byte budgets, cancellation and commits; embedded-worker packaging parity. Never implement Python object semantics in Rust. |
| Shared contracts | `schemas/`, protocol/models crates, Python catalogs and fixtures | Publish each request/result/error/state change once; generate/check schemas and fixtures. |
| Desktop client | `desktop/crates/client/src/`, models crate | Typed commands/events, capabilities, correlation, cancellation, deadlines and stale-result suppression. |
| Editor/UI | `desktop/crates/ui/src/{notebook,shortcuts,runtime,outputs,history,settings}.rs` | Small completion/help, variables, console, debugger and widget modules; shared theme, focus and surface state. Avoid expanding `lib.rs` into every feature owner. |
| Offline services | `src/kernelledger/desktop/helper.py`, native helper, interchange/documents | Recorded inspection/export only. They never query live Python objects or start a worker. |

## 4. Common runtime contracts required before new UI

The method names below are proposals for catalog/schema review, not implemented
APIs. Keep desktop protocol 1 only for genuinely additive negotiated changes;
publish a new version for incompatible framing, semantics or state assumptions.
Apply the same rule to worker messages and artifact schemas.

### 4.1 Capabilities, request correlation and generations

Advertise worker-derived features through the owner: completion, documentation,
namespace metadata, async cells, timing, console, debugging, comms and optional
Jupyter compatibility. The UI checks the connected owner's capabilities, engine
and version, not the presence of a button or a local binary. An unsupported
feature explains its prerequisite and does not trigger an owner replacement.

Proposed transient reads: `code.complete`, `code.inspect`, `namespace.list` and
`namespace.preview`. Requests carry request ID, session/worker generation, source
revision and cursor range where relevant. Replies include those identifiers,
namespace epoch, completeness/truncation and structured errors. Define offsets
as Unicode code points with explicit conversion to GPUI editor byte positions;
test emoji, combining marks and non-Latin identifiers. Cancellation is advisory;
late replies are ignored, including replies arriving after stop or restore.

Do not persist keystrokes, partial drafts, hover text or completion traffic as
execution history. Transient reads cannot consume durable operation keys or
block interrupt/stop behind normal work. Idle live reads run on the worker
execution thread. While busy, use static editor results or explicitly labeled
cached metadata; never inspect arbitrary objects from the protocol thread.

### 4.2 Passive inspection and explicit evaluation

Initial inspection reads names, exact built-in metadata, static attributes and
bounded previews for explicitly supported types. It does not call arbitrary
`repr`, `dir`, properties, descriptors, indexing overloads or evaluate expressions.
Avoid describing unrestricted Python introspection as side-effect-free.

Any later opt-in evaluation, custom representation, debugger expression or widget
callback is potentially executing user code. Route it through an explicit
accepted execution/operation with source/provenance, outcome and interrupt
behavior. No automatic retries after an uncertain outcome. Live preview is not a
checkpoint and does not imply that a displayed object can be restored.

### 4.3 Initial bounds and lifecycle

These are proposed starting budgets, to freeze against reference hardware in N0:

| Resource | Initial target and required behavior |
| --- | --- |
| Transient reads | At most 4 pending per connection and 32 per owner; latest completion supersedes older work. Per-client fairness and cancellation. |
| Completion/help reply | At most 100 candidates and 256 KiB total; docs at most 64 KiB. Explicit truncation. Existing 1 MiB desktop frame cap remains. |
| Namespace pages | At most 100 bindings per page; use generation/epoch-bound cursors. Preview at most 64 KiB per object without scanning the full object. |
| Output history cache | At most 100 retained results and an 8 MiB accounted-content budget; unknown-size objects are not retained by default. Include reference graphs/native-buffer accounting or refuse admission; `sys.getsizeof` alone cannot enforce this budget. |
| Widget traffic | Initially 64 KiB JSON plus at most 1 MiB binary per event, bounded aggregate queues and per-client rates. Reject oversize messages; coalesce only declared replaceable state updates. |
| Control responsiveness | Proposed p95 owner status/interrupt acknowledgement under bulk work at most 100 ms on the recorded reference workload. Measure completion of cancellation separately. |

State budgets in bytes as well as item counts. All socket writes have deadlines.
Reserve capacity for stop/interrupt and terminal events. Detached/background
output remains attributed by policy; it is not relabeled as the next cell's
output. No partial results may be presented as durable before commit.

Define idle, executing, waiting input, paused debugging, stopping and disconnected
behavior across both owners. Async tasks and widgets can outlive a cell, so idle
alone is not sufficient evidence for checkpoint eligibility. Refuse checkpoint or
restore while paused, while a callback runs, or while non-allowlisted background
work can mutate the graph. N0 specifies state transitions and recovery before
adding any persisted status or debug event kinds.

## 5. Feature coverage: frontend and backend work

Requirement IDs here are draft-only `NI-*`; frontend IDs use the distinct
`F-NI-*` namespace. Reconcile existing IDs and scenarios on publication.

### NI-01 / F-NI-01: completion and documentation — N2

**Backend:** Provide lexical/static completion first, then idle namespace names
and safe static attributes. Evaluate an optional Jedi/LSP process for imports,
signatures and diagnostics; bound its CPU/memory, cancel obsolete requests and
stop it with the project. Distinguish static suggestions from live availability.
Do not import notebook modules just to complete an identifier. Advanced dynamic
completion is gated as potentially executing Python.

**Frontend:** Completion popup anchored to the caret, signature help, explicit
documentation action and loading/empty/stale states. Define Tab versus indent,
Enter versus run, Escape dismissal and screen-reader announcements. Applying a
completion edits the correct revision through normal source-save/conflict paths.
Assistant inline completion remains separately labeled and gated.

**Exit:** Correct Unicode replacement ranges, no stale edits after typing/tab
switch/restore, no secret draft logging, keyboard-only operation. Warm local
static completion target p95 at most 200 ms; live help may time out with a useful
state. Measure on the supported Windows/Linux reference environments.

### NI-02 / F-NI-02: object help and variable explorer — N3

**Backend:** Add bounded namespace pages with type, safe shape/length metadata,
origin epoch and checkpoint support information only when known. Stable preview
handles expire on namespace change or worker replacement. Add `name?`/`name??`
through the chosen engine with precise syntax/provenance; source lookup is
read-only and never runs an expression. Table/array previews page explicit
supported codecs and report errors without materializing the full object.

**Frontend:** Optional inspector with searchable names, type/shape columns,
on-demand preview and documentation. Distinguish live metadata, cached metadata
and recorded checkpoint data. Refresh on demand or after completed executions,
not by an unrestricted polling loop. Initial version has no variable editing or
delete action. Explain busy, paused, stale, unsupported and disconnected states.

**Exit:** Hostile `__repr__`, `__getattr__`, property and container implementations
are never called in passive mode. Test huge arrays, cycles, missing codecs,
concurrent background mutation, restart and aliasing. Merely opening history or
the inspector does not start a runtime. An assistant reads no live preview unless
the user explicitly selects it under a separately adopted context contract.

### NI-03 / F-NI-03: timing and profiling — N1

**Backend:** Add `%time`, `%%time`, then `%timeit`/`%%timeit` and `%prun` with a
bounded result format. Record original cell and engine/transformer provenance.
Preserve exceptions, streams, input, interrupt and final-expression behavior.
Repeated benchmarks run user code repeatedly; expose repeat/count settings and
do not label them single executions. Bound profile size and overhead.

**Frontend:** Human-readable wall/CPU time and benchmark repeat counts; expandable
profile table with function/source navigation. Distinguish execution duration,
queue wait, worker compute and end-to-end latency. Keep timing attached to its
execution rather than overwriting another run's output.

**Exit:** Tests for mutations repeated by timeit, cancellation, errors and source
locations; native/Python owners fold the same results. No performance claim from
a single sample or a benchmark running alongside builds/tests.

### NI-04 / F-NI-04: asynchronous cells — N4

**Backend:** Add top-level `await`, `async for` and `async with` through a defined
worker event-loop strategy. Preserve namespace bindings, final expressions and
source indexing. Track tasks, output attribution, exceptions and cancellation;
define whether user-created tasks may survive a cell and expose them if allowed.
Restart/stop cancels and tears down the loop; restore starts a clean candidate.
Do not attempt to serialize tasks, loops or active network connections.

**Frontend:** Show foreground async execution and any surviving background work.
Interrupt targets the foreground operation; a separate explicit action handles
allowed background tasks. Explain why a checkpoint is refused. A completed cell
does not falsely imply all its background work has stopped.

**Exit:** Awaited results, async iterators, task errors, stdin, repeated interrupt,
cancel-resistant coroutines, late output, stop and failed restore tests. Healthy
workers survive failed candidates; forced process termination remains available.

### NI-05 / F-NI-05: result shortcuts and history — N1

**Backend:** Add final-result suppression with trailing `;` and bounded optional
`_`, `__`, `___`, `In` and `Out` conveniences. Token/AST-aware suppression must
ignore semicolons in strings/comments and leave side effects unchanged. Ledger
history remains authoritative; `Out` is an ephemeral cache, not durable history.
Reserve injected names under an explicit policy and handle pre-existing user
bindings without silent overwrite. Distinguish injected caches from user aliases
in checkpoints; never exclude a genuine user binding just because of its spelling.

**Frontend:** Search/reuse previous inputs from the existing History surface,
show exact source before inserting it, and expose cache clearing and limits in
settings. Reusing input edits or runs through normal revision/acceptance paths.
Output suppression is visible as an execution choice, not missing history.

**Exit:** Bounded retention, no hidden strong-reference leaks, clear-cache behavior,
restore/collision tests and correct alias preservation. Test a large object
referenced both by a user variable and by the transient output cache.

### NI-06 / F-NI-06: magic registry and notebook commands — N1, N4, N9

**Backend:** Centralize supported line/cell commands and advertise their help.
Ship timing plus `%%capture` first; add `%%writefile` and platform shell cells
after execution/provenance tests. Keep `%pip` tied to the selected interpreter,
record environment drift and never install automatically on notebook open.
`%run` must record the file bytes/hash actually executed, working directory and
namespace rules; `%load` is a source edit with a preview. Gate `%reset`, autoreload,
custom extensions and arbitrary magics until their effects on namespace epochs,
source indexing and checkpoints are specified. Never promise `%%bash` on Windows
unless that shell is present and explicitly selected.

**Frontend:** Supported-command help, command completion, bounded captured-output
view, and clear shell/interpreter context. Destructive namespace reset uses the
existing confirmation standards. Show original commands in history and offer
transformed source as diagnostic detail.

**Exit:** Strings/comments/indentation, cell magic parsing, mixed line endings,
shell quoting, process-tree interrupt, file mutation and replays have explicit
tests. Replaying a file/shell/install command never implies external side effects
can be undone by restoring a checkpoint.

### NI-07 / F-NI-07: debugger — N7

**Backend:** Evaluate a debugpy/DAP adapter in the worker with source mapping to
immutable executed cell filenames. Add control routing that works while paused,
breakpoints bound to source revisions, stack/scopes/variable pages, continue,
step and stop. Define paused-owner crash recovery and live frame lifetime.
Expression evaluation is explicit execution, logged and never silently retried.
Disable checkpoint/restore while paused. Keep debugger frame references bounded.

**Frontend:** Gutter breakpoints, current execution line, call stack, variables,
step controls and post-error entry. Edited cells show breakpoint/source mismatch.
Distinguish "paused" from "waiting for input" and show a working stop action.
The debug console obeys the same execution/privacy rules as the shared console.

**Exit:** Windows/Linux real-process stepping, nested functions, edited source,
exceptions, stdout, two clients competing for control, disconnect, stop and
debugger crash. Qualification must include paused UI behavior, not just DAP frames.

### NI-08 / F-NI-08: widgets — N8

**Backend:** Implement authenticated comm open/message/close routing only after
choosing the widget renderer. Bind handles to session/generation and client role;
bound models, buffers, callbacks and queued bytes. Serialize callbacks with user
execution and durably admit side effects. Use event IDs for retry deduplication;
do not coalesce clicks or replay callbacks when reopening recorded output.
Qualify finite floating widget values in canonical requests before native durable
admission; reject non-finite values according to a published policy. Saving
widget presentation/state does not save Python callbacks or kernel state.

**Frontend:** Start with sliders, buttons, checkboxes, dropdowns and progress.
A small native widget set may ship as a declared subset. General `ipywidgets`
compatibility requires a qualified widget manager, compatible models/modules,
binary buffers and typically an isolated web renderer. State the supported
library/model versions; never claim compatibility from matching controls alone.
Historical output shows an inert snapshot and "reconnect/recreate" guidance.

**Exit:** Reconnect, repeated clicks, two windows, large buffers, stale handles,
worker restart, callback failure, queue pressure and malicious model messages.
Widget/HTML content never receives the runtime token or unrestricted native bridge.

### NI-09 / F-NI-09: richer output — N6

**Backend:** Audit MIME selection, metadata, binary storage, folding,
`display_id` lifetime and `clear_output(wait=True)`. Preserve immutable raw events
while maintaining the current folded view. Add missing table/array pagination,
audio/video attachments and interactive-chart outputs through typed adapters.
Content limits and fallback text apply before UI rendering.

**Frontend:** Accessible paged tables, equations, image scaling, media controls,
copy/save actions and predictable display updates. Test existing HTML/SVG handling;
use sanitized/static rendering by default. Gate active HTML/JavaScript and chart
renderers behind an isolated, platform-qualified webview with network off by
default and no token, filesystem or privileged bridge access. Preserve visible
fallbacks for unavailable renderers and offline exports.

**Exit:** MIME priority/fallback fixtures, corrupt and oversized outputs, scripts,
links, frame navigation, autoplay, repeated updates, large histories, keyboard
access, contrast themes and untrusted imported notebooks. No full blob read just
to display a small page or thumbnail.

### NI-10 / F-NI-10: console sharing the notebook worker — N5

**Backend:** Reuse durable `run` admission with explicit console origin and
unattributed source provenance. Attach to the existing session, never spawn a
second worker or bypass the queue. Console cells participate in execution
ordering, input claims, interrupts, history, checkpoint boundaries and failures.
Persist origin metadata only through an adopted schema change.

**Frontend:** Optional console surface/drawer with input, output and history,
clear attached notebook/interpreter/session identity, and "insert into notebook"
as a revision-checked edit. Hiding the console clears no namespace. Define focus
and shortcut precedence so Enter, Shift+Enter and Escape do not run the wrong cell.

**Exit:** Notebook/console share variables and ordering; two windows cannot double
execute a retry; prompts go to the claiming client; disconnected/unknown outcomes
are recoverable through operation lookup. Recorded console code is distinguishable.

### NI-11 / F-NI-11: notebook editing and workflow — every UI phase

**Backend:** Reuse source locks, revisions, frozen batches, durable events and
existing interpreter selection. Add missing runtime metadata only through the
catalogs. Keep source order, execution order and result freshness distinct.

**Frontend:** Audit current shortcuts, cell folding, output search, run above/below,
run all, execution labels/timing, interrupt/restart and environment picker. Fill
gaps without creating parallel menus or a second history surface. Keep new panels
optional, remember per-notebook layout, and preserve scroll/caret positions.
Show stale results after edits or namespace replacement. Separate static editor
diagnostics from runtime errors and assistant suggestions.

**Exit:** Keyboard-only journeys, IME, international identifiers, light/dark/high
contrast, reduced motion, minimum supported window size and OS accessibility
inspection. Use existing theme tokens. Focus stays visible; dismissing help,
completion or debug overlays restores editor focus without executing code.

### NI-12 / F-NI-12: interchange and external Jupyter clients — N9

**Backend:** Extend existing `.ipynb` converters with explicit fixtures for MIME
metadata, display updates, attachments, raw/Markdown/code cells, execution labels,
widget snapshots and unknown fields. Preserve or report losses; never execute
imports to produce previews. Ordinary notebooks do not carry a live namespace.
Design an optional Jupyter kernel adapter mapping execute, complete, inspect,
history, stdin, interrupt, shutdown and, when qualified, debug/comm messages to
the existing owner. The adapter holds no ledger-writing authority and launches
no competing ipykernel execution process. Map each execute message to a stable
client/idempotency identity; uncertain side effects are not retried automatically.

**Frontend:** Import/export preview with compatibility warnings and usable
fallbacks. Optional kernelspec/adapter setup explains the project interpreter,
supported clients and current limitations. An external client's shutdown policy
must distinguish detaching its adapter from stopping a shared notebook runtime.

**Exit:** Round trips through selected Jupyter Notebook/JupyterLab/VS Code
versions, mixed frontend attachment, stdin, cancellation, comms, stale connection
files and shutdown. Initially loopback/local only. Jupyter authentication and
connection-file permissions must be specified; no claim of an external sandbox.

## 6. Delivery sequence and exit gates

Each increment contains vertical frontend/backend work. Python can deliver a
feature while native remains gated, provided the capability difference is clear.
No incomplete Rust feature is implemented by silently switching a live owner.

| Phase | Deliverable and dependency | Backend changes | Frontend changes | Exit gate |
| --- | --- | --- | --- | --- |
| N0 | Contracts, inventory and engine decision; first | ADR/spike, parity matrix, generations/states, budgets, schema proposals, provenance and checkpoint rules | Capability/state map, panel placement, shortcut/focus map and accessibility feasibility | Engine chosen for the next increment; contracts and fixtures reviewed; current plans/drafts reconciled for publication. |
| N1 | Small execution conveniences; N0 | NI-03/05 and initial NI-06; transient caches, timing, suppression and capture | Timing/profile output, command help, history reuse/cache settings | No provenance, checkpoint, retention or interrupt regression; new syntax tested through both advertised owners. |
| N2 | Completion and documentation; N0, N1 engine boundary | Static/live read routing and optional bounded analysis process | Caret popup, signatures, docs and cancellation | Unicode, stale reply, busy worker and keyboard tests; measured latency. |
| N3 | Variable inspector; N2 read infrastructure | Passive metadata, paged previews and epoch-bound handles | Variables/help inspector and explicit refresh | No arbitrary user code in passive mode, large-object bounds and restore invalidation. |
| N4 | Async and remaining execution semantics; N1 | Event-loop/task lifecycle, attribution, cancellation and checkpoint refusal | Foreground/background work and cancellation states | No leaked tasks/processes or false checkpoint eligibility; platform lifecycle tests. |
| N5 | Shared console; N1, N2; async enabled only after N4 | Accepted console runs and origin metadata | Console/history/input ownership and focus | Same worker/queue; durable retry and multi-client ordering pass. |
| N6 | Rich rendering; N0; independent of N4/N5 where practical | MIME/event/page/export audit and missing typed adapters | Tables/math/media and qualified interactive renderer spike | Bounded rendering, safe fallbacks, update/clear semantics and OS journeys. |
| N7 | Debugger; N3, N4 | Debug adapter, paused states and priority control | Breakpoints/stack/locals/stepping | Paused, edited-source, disconnect and candidate-state tests pass. |
| N8 | Widgets; N4, N6; numeric contract first | Comms, callback admission, deduplication and versioned state | Core widgets or qualified widget manager | Reconnect/load/trust tests; supported subset declared, inert history. |
| N9 | Interoperability; N1–N6; debug/widgets exposed only after N7/N8 | Converter coverage, `%run`/`%load` provenance and optional Jupyter adapter | Import/export review and explicit external-client setup | External client and notebook round-trip matrix; no duplicate writer/worker. |
| N10 | Integrated qualification and release | Performance/fault/parity/platform/package evidence; migration gates | Full workflow/accessibility/theme/platform qualification | Feature gates plus applicable inherited release gates pass; release notes state actual support. |

Do not estimate calendar completion until N0 fixes the engine path, renderer
feasibility, platform matrix and staffing. N1–N3 are the first product milestone;
N4–N6 the second; N7–N9 remain separately gated advanced features. Independent
features can ship earlier behind truthful capabilities without waiting for every
advanced feature or authorizing the Rust default switch.

## 7. Rust/Python parity, existing work and migration dependencies

Maintain a matrix per feature across Python owner/custom engine, native
owner/custom engine and any adopted engine adapter. Native unsupported methods
must return a useful error and advertise no capability. A startup dependency
failure names the selected interpreter and recovery action; no silent engine
fallback inside an existing session.

Retain all open migration work from `runtime-v6-gates.json`:

| Existing phase | Carry-forward work and relationship to this plan |
| --- | --- |
| 3, readers/helper | Malformed-input parity, large-history memory and desktop browsing. New previews/outputs must use bounded paging; offline browsing remains worker-free. |
| 4, durability | Real disk-full/permission failures, crash publication boundaries, Windows reparse/long-path cases and schema-1 backup/migration. New metadata/events require crash and rollback tests. |
| 5, supervisor | Admission/cancellation load, complete numeric hashes, input handoff/delivery, feed fan-out/reconnect/heartbeat/slow readers, background output/process-tree stress, grouped stream commits and end-to-end byte budgets. New transient/debug/widget channels must honor these controls. |
| 6, parity | Replay/continue, pins/GC/bundle/import mutations, training capture/restore, assistant/export inventory, cross-version codecs. Engine changes must preserve source reconstruction and training exclusions/registrations. |
| 7, distribution | Clean-machine Windows/Linux install/upgrade/uninstall, full license texts, capability rollback, signing/integrity and coexistence. Include analysis/debug/widget/engine dependencies and extracted worker assets. |
| 8, default switch | Every inherited exit, two qualified opt-in release candidates, material benefit without unexplained regressions, plan/assistant reconciliation and retained Python rollback. This plan neither closes nor bypasses these gates. |

Native float/hash compatibility needs Python-authoritative golden encodings,
raw-wire parsing and exact retry tests; merely serializing a Rust `f64` is not
sufficient. Engine/timing/widget settings must not introduce unqualified numeric
forms into accepted durable requests. Refusal is supported until parity is proven.

Preserve the assistant provider/credential process and existing proposal review.
Completion, inspection and debug content must not enter cloud prompts
automatically. Preserve export utilities, training registration and checkpoint
candidate admission across both engine paths. Existing source files, sessions
and checkpoints remain inspectable without installing new interactive extras.

## 8. Test and measurement plan

Add focused test modules as features ship; names below are planned locations.
Reuse current suites rather than replacing their invariants with mocks.

| Layer | Planned evidence |
| --- | --- |
| Worker | `tests/test_interactive_worker.py`, `test_worker_completion.py`, `test_worker_inspection.py`, `test_worker_async.py`: syntax, namespace, hostile objects, source mapping, cancellation, task and reference lifetime. |
| Durable storage | Existing checkpoint/boundary/runtime suites plus history-cache, engine-provenance, crash-publication, partial/failed restore and no-user-binding-loss cases. |
| Owners/protocol | Shared Python/Rust contract fixtures plus desktop/native suites: negotiated features, Unicode offsets, stale generations, repeated keys, queue byte pressure, read cancellation, input claims and stop priority. |
| Advanced adapters | `test_worker_debug.py`, `test_widget_protocol.py`, `test_jupyter_adapter.py`: real process control, callback side effects/deduplication, binary buffers, client shutdown and authentication. |
| Frontend | Rust model/client/UI tests plus real Windows UIA and available Linux OS automation: edit → complete → run → inspect → checkpoint → restore; console/debug/widget journeys when enabled. Screenshots supplement behavior tests. |
| Interchange | Existing document/import/export suites expanded with round-trip/golden fixtures and explicit unsupported-output reports. Test optional dependencies absent. |
| Distribution | Wheel/daemon/UI/adapter packages, resource identities, full license texts and extracted-archive tests; clean-machine install/upgrade/uninstall and rollback. |

Cover supported Python 3.11–3.13 where each optional dependency supports them;
report narrower adapter support explicitly rather than implying the whole matrix
passed. Windows/Linux headless tests and graphical qualification are separate
evidence. GPUI compilation alone does not qualify Linux desktop behavior.

Measure release builds against a frozen baseline with identical interpreters,
payloads and hardware/environment records. Interleave trials; keep raw samples,
hashes and idle/loaded workloads. Cover cold/warm/changing graph checkpoints,
32/256 MiB and 1 GiB where hardware allows, owner/worker/sidecar/UI memory,
output retention, 10,000-cell/history browsing, completion latency and control p95
under bulk transfer. Report skips and peak/private/RSS definitions. Do not infer
a tail-latency improvement from a better median or sum shared RSS as unique memory.

Run the applicable full Python suite, Rust workspace tests, static checks,
catalog/schema drift checks and real UI/backend journeys before qualifying a
milestone. Features that introduce persistent formats must include upgrade,
read-only newer-version refusal and rollback behavior before release.

## 9. Decision register and risks

| ID | Decision | Default until resolved | Evidence needed |
| --- | --- | --- | --- |
| NI-D01 | Custom engine versus optional IPython adapter | Current custom engine | N0 ADR, source/checkpoint tests, startup/RSS and maintenance comparison. |
| NI-D02 | Completion provider | Lightweight static + bounded idle names | Editor feasibility; Jedi/LSP dependency/process lifecycle and Unicode tests. |
| NI-D03 | Namespace preview scope | Passive supported metadata only | Hostile-object tests and bounded array/table previews; explicit evaluation separate. |
| NI-D04 | Async background tasks | No claim that idle implies checkpointable | Task policy, ownership, interrupt/stop and candidate safety matrix. |
| NI-D05 | History name collisions and retention | No silent overwrite or unbounded `Out` | Injection-origin tracking, user alias preservation and real retained-memory accounting. |
| NI-D06 | Widget/active-output renderer | Static native fallback | Native subset versus isolated webview feasibility on each OS; compatible model versions. |
| NI-D07 | Debugger integration/control owner | Disabled until N7 | Debugpy/DAP compatibility, frame lifetime and multi-client ownership. |
| NI-D08 | Jupyter adapter lifecycle | Optional local adapter, never second writer | Connection/auth policy, kernelspec packaging, shared-runtime shutdown and retry mapping. |
| NI-D09 | Format/protocol versions and rollout | Existing formats/defaults | Compatibility tables, generated schemas and reconciled runtime/frontend publication. |

Most material risks are transformed source no longer reconstructing functions,
completion/preview accidentally running user code, caches retaining large arrays,
async/widget callbacks mutating during checkpoint, debug control blocking behind
execution, and new client retry paths repeating side effects. Each has a gate
above. Adding a library does not discharge those gates.

## 10. First implementation slice and definition of done

Start with N0, followed by a narrow N1 vertical slice:

1. Freeze baseline worker/owner/UI journeys and engine fixtures, including a
   function/class checkpoint, aliases, final expressions, errors and interrupts.
2. Define the engine adapter boundary and produce the optional IPython spike/ADR
   without changing the default engine or requiring new packages on notebook open.
3. Publish the approved source/provenance/cache contracts and implement trailing
   `;` plus `%time`/`%%time`, with results shown in the existing output surface.
4. Add optional bounded result history only after collision and retention rules
   pass. Follow with N2 completion/signatures and N3 passive inspector.
5. Record per-backend capability, test and measurement evidence; keep later
   debugger/widget/Jupyter stages disabled until their own gates pass.

The plan artifact is complete when every discussed feature has an owner,
frontend/backend change, dependency, acceptance scenario and delivery phase.
Implementation is complete only when those scenarios run successfully, documented
limits match advertised capabilities, both selected owners are qualified for the
features they expose, and applicable platform/release gates are satisfied.

## 11. Primary references

These references inform compatibility design; KernelLedger behavior is specified
by its adopted contracts and tests.

- [IPython interactive shell API](https://ipython.readthedocs.io/en/stable/api/generated/IPython.core.interactiveshell.html): execution/engine adapter surface.
- [IPython magics](https://ipython.readthedocs.io/en/stable/interactive/magics.html): timing, capture, file and shell command semantics.
- [IPython autoawait](https://ipython.readthedocs.io/en/stable/interactive/autoawait.html): asynchronous cell behavior and integration differences.
- [Jupyter messaging](https://jupyter-client.readthedocs.io/en/stable/messaging.html): completion, inspection, Unicode positions, stdin, display, comms and debug transport.
- [Jupyter widget protocol](https://github.com/jupyter-widgets/ipywidgets/blob/main/packages/schema/messages.md): model/state synchronization and binary buffers.
- [JupyterLab debugger](https://jupyterlab.readthedocs.io/en/stable/user/debugger.html): frontend debugger capabilities and kernel requirements.
