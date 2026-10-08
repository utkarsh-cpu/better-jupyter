# KernelLedger desktop frontend plan, version 1

**Status:** Superseded on 2026-10-04 by [frontend plan version 2](../FRONTEND_PLAN.md).

**Archived summary:** GPUI/GPUI Kit desktop notebook over the Python supervisor. Version 1 mixed UI decisions with runtime and protocol contracts. Version 2 moves those contracts into runtime plan version 4, adds requirement IDs, an open-decision register, Git, runtime-manager, secret-input, interpreter, table-output and clear-output coverage, a vertical slice, and a timeboxed feasibility gate with a fallback stack.

The text below is the version 1 frontend plan as it stood when version 2 replaced it.

---

# KernelLedger desktop frontend plan

| Item | Decision |
| --- | --- |
| Frontend plan version | 1 |
| Date | 2026-10-04 |
| Status | Implementation plan; frontend and desktop API are not implemented yet. |
| Runtime baseline | [Python runtime plan version 3](v3.md), [implementation notes](../docs/implementation.md). |
| Primary stack | Rust, GPUI, GPUI Kit; existing Python supervisor and worker. |
| Initial platforms | Windows x64 first, Linux x64 after its platform gates pass. |

This document supplements the runtime plan. It does not claim that planned APIs exist or change the current artifact contracts. Before implementation changes a runtime decision or persisted schema, archive runtime plan version 3 and publish the corresponding versioned runtime plan and migration specification. Keep frontend plan revisions in `plans/frontend-vN.md` when decisions change.

## 1. Product goals and boundaries

Build a native desktop notebook for editing ordinary source files, executing Python in a project environment, inspecting durable history, and continuing from typed checkpoints. A user must always be able to distinguish source on disk, an execution's recorded source, the live namespace, and a published checkpoint.

The core journeys are:

1. Open or create a notebook, select an interpreter, edit cells, run them, and read results.
2. Reopen the application and inspect previous executions without starting Python execution.
3. See why output is stale after editing source, and compare the executed version with the current one.
4. Publish a checkpoint, understand any omissions, restore into a new session, and continue work.
5. Inspect and import a shared bundle, review its provenance, and explicitly trust it before restore or replay.
6. Recover from an editor crash, worker crash, supervisor restart, interrupted operation, or external source edit without guessing what was saved or executed.

Opening a notebook, previewing Markdown, viewing history, inspecting a bundle, and showing checkpoint metadata execute no notebook code. Closing a window detaches from an existing supervisor; stopping the runtime is a separate action. Editing remains available while the worker is busy.

The first release is local and single-user. Remote execution, collaborative editing, cloud accounts, a browser frontend, IPython magics, widgets, top-level await, debugger integration, and arbitrary JavaScript output are outside its scope. macOS and ARM64 require separate runtime and packaging qualification. A live variable inspector and LSP support are later additions unless their individual gates are completed; neither blocks the core notebook release.

## 2. Stack and feasibility decisions

Use a Rust workspace under `desktop/`, with a GPUI application, GPUI Kit controls/editor, typed service client, and presentation models. Use GPUI's asynchronous integration for background work; do not block the render thread on file I/O, Python startup, hashing, SQLite, network frames, or image decoding. Add a separate asynchronous runtime only if an integration requires it.

GPUI describes itself as a GPU UI framework with retained and immediate rendering, and its API remains pre-1.0. Pin a verified compatible dependency set, commit `Cargo.lock`, and pin `rust-toolchain.toml`. Treat framework upgrades as deliberate changes with UI regression checks. See the [official GPUI README](https://github.com/zed-industries/zed/blob/main/crates/gpui/README.md).

GPUI Kit provides controls, an editor, layout facilities, and accessibility integration. Its workspace explicitly coordinates exact GPUI snapshot versions; use its compatible dependency family rather than independently upgrading GPUI crates. See [GPUI Kit](https://github.com/longbridge/gpui-kit) and its [workspace manifest](https://raw.githubusercontent.com/longbridge/gpui-kit/main/Cargo.toml). Tree-sitter syntax highlighting is baseline; Python diagnostics, completion, and navigation through an optional LSP come later.

Embedded HTML is a feasibility decision, not an assumed dependency. The current [GPUI Kit webview example](https://raw.githubusercontent.com/longbridge/gpui-kit/main/examples/webview/src/main.rs) contains an unfinished Linux integration path and a Windows composition setting. Prototype focus, clipping, scrolling, resizing, DPI, accessibility, and composition before shipping it. Native text, Markdown, images, JSON, and constrained tables are the required output baseline. Unsupported HTML remains inspectable as source and exportable. If the webview gate fails, ship this explicit fallback rather than delaying all notebook features.

Phase 0 must prove:

- A real Python editor inside a virtualized cell list preserves selections, focus, undo, and composition when cells mount and unmount.
- Windows graphics, font rendering, clipboard, IME, screen reader behavior, and mixed-DPI resizing work on supported hardware.
- Linux X11 and Wayland pass equivalent checks before Linux is advertised as supported.
- A packaged Rust application can invoke a private Python service and use a separate project interpreter for the worker.
- Native output rendering can safely handle large streams and images without freezing the editor.

Record the tested OS, graphics backend, GPU/driver, Rust toolchain, and dependency revisions. If GPUI or its editor cannot meet a core gate, document the failure and revise the stack decision before building the remaining screens.

## 3. Application layout and information architecture

The main window has a narrow navigation rail, a resizable notebook sidebar, a central notebook editor, an optional right inspector, and a bottom status area. Keep execution controls close to the notebook and its cells. Panels can be hidden and restored; start with predictable fixed regions rather than unrestricted docking.

| Surface | Required content and behavior |
| --- | --- |
| Welcome | Recent notebooks, open directory, create notebook, import ipynb, inspect bundle; missing directories are recoverable entries. |
| Notebook sidebar | Ordered cells, type, concise title, search/filter, running/queued/error/stale indicators; select and reveal cell. |
| Notebook editor | Code/Markdown/raw cells; source editor, cell controls, execution identity, output, inline errors, insert handles. |
| Runtime toolbar | Interpreter, session, runtime state, Run cell, Run all, Interrupt, Restart, checkpoint action. |
| History | Sessions and executions with pagination, outcome, timestamps, duration, source hashes, provenance, recorded inputs, source and output views. |
| Checkpoints | Outcome, publication time, originating session, interpreter/environment, roots, exclusions, notes, pins, restore/continue actions. |
| Inspector | Selected execution/checkpoint details and source comparison; optional safe live namespace summaries in a later phase. |
| Diagnostics | Doctor results, connection/startup problems, repair proposals, interpreter resolution, runtime logs. |
| Settings | Appearance, editor, accessibility, project runtime configuration, output viewer limits, local storage and diagnostics preferences. |
| Command palette | Searchable actions with shortcuts, availability explanations, recent notebooks, navigation commands. |

Use tabs for notebooks within a window. Each notebook has independent drafts, history selection, connection state, and runtime state. Resolve directory identity using canonical paths/file identity where possible; UUID alone cannot identify a runtime because cloned notebooks share UUIDs. Opening the same notebook twice attaches to the same supervisor and coordinates source edits. A user can detach a tab into a window without starting another worker.

Use breadcrumbs and persistent labels for historical views. History opens read-only by default. “Copy recorded source into current cell” is an explicit editing action with a preview; historical navigation never silently replaces the source or the live namespace.

## 4. Visual design and interaction specification

Create shared tokens for spacing, typography, colors, border radii, elevation, focus rings, and state indicators. Provide light, dark, and system themes. Use readable monospaced source/output fonts and distinct text for cell titles and navigation. Status must use text or icons in addition to color.

Default density should support long notebooks: compact headers, expandable outputs, resizable panels, persistent scroll position, and a visible focused cell. Display session/execution identity without overwhelming the editing surface. Show absolute timestamps in details and concise relative times in lists.

Specify and implement empty, loading, ready, partial-data, disconnected, permission-denied, corrupt-data, and retryable-error variants for every data surface. A toast is supplementary; an error that blocks work remains visible where it occurred, with a precise recovery action.

Baseline keyboard bindings, configurable where practical:

| Action | Default |
| --- | --- |
| Run cell and select next | Shift+Enter |
| Run cell and retain selection | Ctrl+Enter |
| Run cell and insert below | Alt+Enter |
| Save current notebook | Ctrl+S |
| Command palette | Ctrl+Shift+P |
| Find in active editor / notebook search | Ctrl+F / Ctrl+Shift+F |
| Undo / redo | Ctrl+Z / Ctrl+Shift+Z |
| Leave editing / cancel transient UI | Escape |

Run all, checkpoint, and runtime shutdown must also be accessible through named actions. Do not intercept shortcuts during IME composition or steal expected text editing behavior. Support keyboard cell insertion, movement, type change, deletion, output expansion, panel navigation, and dialog dismissal without requiring a pointer. Dragging is an optional equivalent of keyboard reorder.

## 5. Source editing, saving, and conflicts

Retain the existing directory format and canonical source rules. The Python source module owns ID validation, UTF-8/LF normalization, notebook serialization, attachment validation, and canonical hashes. The Rust UI must not introduce a competing serializer.

Use a Python source service with short-lived source locks, separate from the supervisor's lifetime ledger lock. Route GUI and CLI source mutations through the same implementation. It may read/write tracked source and source recovery journals, but never execution history or checkpoint blobs. Lock ordering must be documented so source edits cannot deadlock execution acceptance.

Every source mutation carries the expected manifest revision and expected cell hash. Under the source lock, re-read, validate, reject conflicts, and atomically publish the change. Multi-file operations use a recovery journal and explicit commit point; individual atomic replacements alone do not make reorder, conversion, or deletion transactional. Recover unfinished source transactions before the next mutation. Remove obsolete files only after successful publication and preserve recoverable content until the transaction completes.

Editing requirements:

- Code, Markdown, and raw cells; stable IDs survive edits and reordering.
- Add, duplicate with a new ID, delete with undo, move, convert type, split and merge compatible cells, and copy/paste cells with attachment remapping.
- Preserve supported metadata; show validation errors for unsupported or malformed data instead of erasing it.
- Cell-local undo for text and notebook-level undo for structural operations; undo obeys external-edit conflict checks.
- Markdown preview with attachment resolution, safe links, code fences, lists, and tables; raw cells have no execution action.
- Search and replace in current source, with previews for notebook-wide replacement; historical source search is separate.
- Multi-cursor editing, indentation, bracket matching, folding, line numbers, configurable tab width, and Unicode-aware selections.

Autosave defaults to a short debounce, initially 500 ms, with visible Saving/Saved/Conflict/Save failed states. Persist recoverable drafts in the application profile using notebook directory identity, cell ID, base hash, and draft revision. Do not store tokens there. On recovery, compare drafts with disk and offer restore, comparison, or discard; never overwrite external edits automatically.

File watchers invalidate cached source but are only hints: verify hashes before saving and running. On conflicts, preserve both versions and offer compare, reload, or explicit overwrite. Handle notebook renames, deletion, read-only directories, disk full, antivirus interference, and partial manifests. An invalid cell file should not prevent reading valid history.

Run an edited cell only after its intended source is committed and accepted as an immutable execution snapshot. The run request includes the expected canonical hash. A conflict blocks execution with a clear choice; the UI never displays a draft while silently running a different file revision.

## 6. Runtime states and execution controls

Separate connection state, worker state, active operation state, and execution state. A single “busy” Boolean is insufficient.

| State | User-visible behavior |
| --- | --- |
| Offline inspection | Read source/history/checkpoints; no worker. Run or a runtime mutation can explicitly start the supervisor. |
| Starting / connecting | Show the actual startup stage, interpreter and retry/cancel-detach actions; editing continues. |
| Idle | Run, restart, checkpoint, restore, and replay are available subject to trust and compatibility checks. |
| Queued / executing | Show queue and executing cell; allow editing and interrupt; explain why checkpoint is unavailable. |
| Waiting for input | Focusable input prompt tied to execution and request IDs; keep interrupt and navigation responsive. |
| Interrupting | Show requested interrupt, elapsed time, and an explicit terminate-worker escalation. |
| Checkpointing / restoring / replaying | Show backend stages and operation identity; do not invent percentage progress. |
| Disconnected | Retain last known data with a stale indicator; reconnect and reconcile before enabling dependent actions. |
| Crashed / incomplete | Show recorded outcome and environment; restart empty or restore from a published checkpoint. |

Run all captures a batch plan in manifest order with cell IDs, source hashes, and frozen source. Execute code cells sequentially, excluding Markdown/raw cells. Default to stopping on error; continuing after errors is an explicit setting/action. Show queued cells and allow canceling remaining work separately from interrupting the current cell. The batch contract defines behavior for cells edited or deleted after acceptance. Queues must be owned and coordinated by the supervisor so GUI and CLI clients cannot interleave unpredictably.

Restart and restore create new sessions. Label “Restart empty” clearly. Show whether other clients are attached before stopping a shared runtime. A close-window action saves/reconciles drafts and detaches; it never interrupts a long computation implicitly.

Input prompts retain a durable indication of what was requested and whether it was answered or canceled. An initiating client can disconnect without blocking the worker event reader. One eligible client claims each input request; other clients see its ownership. Secret-entry mode is offered only when the runtime supports an explicit non-recording policy; ordinary Python input is currently recorded and the UI must say so.

## 7. Desktop service and protocol work

The current service is not yet a desktop API: the CLI uses one authenticated framed request per connection, most mutations await completion, history is read separately, and input replies are tied to the initiating connection. Do not build the frontend around polling CLI stdout or parsing human-readable output.

Introduce a separately identified desktop protocol, initially `api = kernelledger.desktop`, version 1, with its own schema and capability negotiation. Preserve legacy CLI and worker schema 1 behavior. The exact listener/handshake routing must be documented and tested before implementation; do not silently send new message kinds to an old parser. A separate authenticated loopback listener advertised alongside the existing supervisor endpoint is the preferred initial route.

Provide a read-only Python inspection helper for offline source/history/checkpoint/bundle queries. It opens the ledger read-only and launches no supervisor or worker. Use structured framed stdio with bounded messages and explicit versioning, never shell-interpolated commands. Online runtime mutations and event subscriptions go to the sole-writer supervisor. Source editing uses the shared source service from section 5.

| API group | Required contracts |
| --- | --- |
| Discovery | Protocol/capabilities, runtime identity, notebook identity, versions, authenticated attachment. |
| Source | Read manifest/cell/attachments, expected-revision mutations, transaction recovery, source change notification. |
| History | Paginated sessions/executions, recorded source, events, input metadata, output fold, environment and provenance. |
| Operations | Accepted operation ID, start/status/result, cancel semantics, run/batch/interrupt/restart/checkpoint/restore/replay/stop. |
| Events | Initial snapshot, durable catch-up cursors, execution lifecycle changes, output events, input requests, connection health. |
| Artifacts | Checkpoint reports/descriptors, safe blob streaming, bundle inspection/import/export, publish, pins, orphan GC. |
| Diagnostics | Doctor, proposed repair, interpreter discovery/config validation, supported settings. |

Protocol requirements:

- Authenticate with the existing protected token mechanism on loopback; never pass tokens to a webview, expose them in logs, or treat a PID alone as endpoint identity.
- Separate control requests from subscriptions and large blob transfers. An input prompt, image transfer, or output flood cannot block interrupt requests.
- Use typed request/response payloads, explicit error codes, size limits, timeouts, and capability flags. Validate untrusted lengths before allocation.
- Assign client request IDs and durable operation IDs. Persist acceptance/outcomes before acknowledging side-effecting operations; repeated requests with the same key return the same operation. Never blindly retry run, replay, restore, import, or publication after a timeout.
- Define retention of deduplication records and lookup after reconnection. Cancellation distinguishes pending cancellation, completed cancellation, committed success, and unknown outcome.
- Define snapshot-and-subscribe consistency: an atomic snapshot watermark plus a durable change sequence, replay after that watermark, bounded queues, and explicit resynchronization on cursor expiry. Deliver at least once and deduplicate by stable IDs; do not claim exactly-once transport.
- Preserve execution event order and committed sequence numbers. Speculative live output, if added, is marked separately and reconciled to durable events.
- Stream blobs with verified hash, byte count and bounded memory. Cache only verified content; missing/corrupt blobs remain visible failures rather than empty output.
- Add persistent operation/change records only through a versioned ledger migration with backup, failure recovery, and old-client behavior defined. Never perform an irreversible upgrade merely by opening a notebook.
- Keep blocking disk work and expensive output folding outside the asynchronous event reader; batch notifications and enforce backpressure without dropping durable history.

Publish JSON schemas, Rust/Python shared fixtures, protocol error examples, state transition diagrams, and compatibility tests before connecting the UI. If new worker commands are needed, version that protocol independently. The frontend does not write SQLite, manufacture execution events, deserialize Python checkpoint objects, or implement checkpoint encoding.

## 8. Output rendering and large notebooks

Use the runtime's authoritative event-folding semantics for display updates, clear-output behavior, streams, results, errors, and truncation. If Rust maintains an incremental fold for responsiveness, verify it against shared Python-generated fixtures and reconcile with the authoritative final fold. Show the immutable execution ID and source hash alongside rendered output.

| MIME/content | First-release treatment |
| --- | --- |
| stdout/stderr, text/plain | Searchable/copyable text, bounded ANSI styling, carriage-return handling, virtualized large streams. |
| Python errors | Traceback with recorded source context; links open that recorded revision, not unrelated current lines. |
| image/png, image/jpeg | Lazy decode, zoom, fit, save original, dimension/memory limits. |
| image/svg+xml | Validated/sanitized renderer or source/export fallback; no scripts or external resources. |
| text/markdown | Native safe Markdown; constrained links and attachments. |
| application/json | Collapsible bounded-depth tree and raw view; large values loaded incrementally. |
| text/html | Sanitized supported subset if validated; source/text/export fallback. Optional sandboxed webview only after its gate. |
| Other MIME types | MIME chooser, metadata, text fallback if present, save original blob; no implicit executable opener. |

Do not execute output JavaScript. Disable remote resource loading by default. Opening a link externally is an explicit user action. Sanitization covers URL schemes, embedded frames, scripts, styles that obscure UI, and resource limits. A webview has no notebook service bridge, token, filesystem access, or unrestricted network access; verify its platform capabilities before promising this boundary.

Render long notebooks with cell virtualization, stable IDs, measured height caching, and scroll anchoring. Preserve editor state independently of view lifetime. Decode images in background tasks and release offscreen GPU textures. Viewer truncation is separate from the runtime's output cap: show both with access to stored data where it exists. UI limits never delete ledger events or blobs.

Output selection follows a documented policy: latest execution for the selected cell in the chosen session, with an explicit history selector. Badge results whose source hash differs from current source. Preserve outputs for deleted cells in history; mark imported and unattributed executions. A changing display can update visible output without rewriting the original event log.

## 9. Checkpoints, restore, replay, and trust

Checkpoint UI must show all four outcomes: complete, partial, refused, and failed. Show progress stages provided by the backend, then the published ID only after ledger commit. Present exclusions with binding name, type, path, and reason; notes include semantic limitations such as reimported modules, rerun decorators, copied array views, and split closure cells. A partial checkpoint is useful but visibly incomplete.

Before restore/continue, show checkpoint provenance, roots/exclusions, interpreter compatibility, environment drift, and the new-session effect. Reject Python major/minor mismatches by default, matching the runtime. Any supported override is deliberate and recorded. After restore, show the actual per-binding restore report; checkpoint publication does not guarantee every binding restores successfully.

Replay previews the recorded cell sequence, source differences, interpreter/environment drift, and whether recorded inputs will be reused. It executes recorded source in a new session and can repeat external side effects. Require an explicit action after this preview; never launch replay as a history navigation shortcut.

Foreign artifacts remain inspectable without trust. Before executing their saved definitions or source, show the artifact identity/hash, origin, relevant code, and reason trust is required. Scope trust to the selected artifact/operation; importing a bundle does not blanket-trust its contents or future replacements. This trust decision is a workflow control, not a Python sandbox.

Pins are named checkpoint references. GC previews removable orphan blobs and reports reclaimed bytes. Existing retained history/checkpoints are not deleted by schema-1 GC; do not offer “clear all history” until a separate deletion contract exists.

## 10. Import, export, and CLI parity

| Existing capability | Frontend coverage |
| --- | --- |
| init, add/rm, source editing | Create/open notebook; cell editing and structural commands with recoverable saves. |
| run cell/all/source | Cell/batch actions; optional scratch execution labeled unattributed. |
| status, serve, stop, interrupt, restart | Runtime status and explicit lifecycle actions. |
| log/show/source/events/MIME extraction | History viewer, recorded source and event inspector, save original output. |
| checkpoint, restore, continue, replay | Reviewed workflows from section 9 with progress and final reports. |
| import/export ipynb | File dialogs, source/output preview, import conflict handling, selected-session export, mismatch metadata. |
| bundle show/import/export | Inspect before importing; choose destination; verify integrity and preserve tracked source on history import. |
| publish/fetch | Publish descriptor; inspect descriptor and missing local blobs. No claimed remote download service. |
| pin/unpin/gc | Checkpoint pins and orphan cleanup preview. |
| doctor, repair IDs, retarget | Diagnostics; preview changed IDs/references or new notebook UUID before explicit repair. |

Bundle export previews what it includes: source snapshots, history, recorded inputs, outputs, and checkpoint values can contain secrets. Export exclusion/redaction is offered only once the artifact format supports it correctly; never advertise stripping secrets with a cosmetic UI filter. Bundle import verifies hashes/references and does not overwrite tracked source. Use a separate explicit source-copy workflow when needed.

Import ipynb follows the runtime's destination constraints. Explain whether outputs are retained as imported history or omitted. Export chooses a session and reports source mismatches. Invalid or unsupported artifacts produce actionable validation details without partially replacing the current notebook.

## 11. Settings, environments, and optional tools

Separate application preferences from notebook-local `.kl/config.json` and tracked notebook metadata. App preferences include theme, font, panel layout, shortcuts, viewer limits, draft recovery, and recent directories. Notebook settings include interpreter, environment allowlist, checkpoint denylist, thread allowlist, and runtime output limit. Never bundle machine-local settings.

The interpreter selector shows the resolved executable, resolution rule, Python version, and optional codec availability. Validate with a bounded diagnostic subprocess. Changing selection applies to the next worker and must not silently restart active work. Show installation instructions rather than automatically installing packages into a project environment.

The packaged service's private Python interpreter is distinct from the worker interpreter. Preserve the runtime's project-environment discovery order and CPython >=3.11 requirement. Optional LSP is a separate process with explicit selection/install configuration, resource limits, cancellation, and shutdown; editor diagnostics are not runtime execution results.

A later safe namespace inspector queries only an idle worker through a new explicit contract. Summaries are bounded and avoid arbitrary `repr`, properties, getters, iteration of unknown objects, or evaluation. Expand only supported built-in data shapes. Inspection itself can have side effects if user hooks are invoked; unsupported objects therefore show type/identity metadata, not automatic previews.

## 12. Accessibility and international text

Accessibility is a release gate, not a final cosmetic pass. Map controls, editor, cell list, runtime status, dialogs, and output to the accessibility tree. Validate GPUI Kit's integration in the actual application; upstream support does not prove our composed notebook is accessible.

Require keyboard-only completion of core journeys, visible focus, logical focus restoration, labels for icon buttons, sufficient contrast, text scaling, reduced motion, high-contrast themes, and useful error descriptions. Test Windows with NVDA and Linux with Orca before qualifying each platform. Announce execution transitions and errors without reading every output token. Output updates must not steal focus or overwhelm a screen reader.

Test IME composition, combining characters, emoji, bidirectional prose, non-ASCII paths/identifiers, clipboard line endings, and font fallback. Keep internal text offsets explicit: UTF-8 byte positions, grapheme selections, and LSP UTF-16 positions require conversion. Use locale-aware display formatting while preserving canonical source and stable machine timestamps.

## 13. Performance, reliability, and observability

Use instrumented target budgets, not unsupported promises. Establish baselines on a documented mid-range Windows machine and repeat on qualified Linux hardware.

| Scenario | Initial acceptance target |
| --- | --- |
| Cached source/history navigation | Interactive controls respond within 100 ms; large data loads remain asynchronous. |
| Typing and scrolling | p95 UI frame work below 16.7 ms under normal notebook load; no sustained freeze during output flood. |
| Large notebook | 1,000 cells open without constructing 1,000 live editors; source loads incrementally. |
| Large text output | 100 MiB stored stream can be inspected with bounded paging, not a single giant string/widget. |
| Output flood | Control/input/interrupt remain responsive; dropped subscriber notifications force catch-up, not lost history. |
| Image-heavy notebook | GPU/decoded image cache has a configurable bounded budget and evicts offscreen data. |
| Reconnection | Recover cursor/snapshot state and operation outcomes without submitting duplicate work. |

Define timeout policy by operation: connection/startup timeouts are bounded, execution can legitimately run indefinitely, and checkpoint/restore show liveness plus known stages rather than a generic short HTTP-style timeout. A timeout never proves the operation failed.

Keep structured local logs with request/operation IDs and stage durations. Do not log source, input, namespace values, token material, or raw artifact payloads by default. A diagnostic export previews included files and redacts secrets it can reliably identify. Crash reports and usage telemetry are opt-in; the application works without external services.

Recovery tests cover power-loss-style termination around save commit, operation acceptance, checkpoint publication, and event commit. Exercise stale endpoints, token rotation, port reuse, locked files, unavailable interpreter, disk exhaustion, bad blobs, database corruption, and supervisor takeover. Prefer read-only recovery inspection when state is damaged.

## 14. Packaging and release operation

Produce a desktop executable plus a private, versioned CPython/service distribution. Do not depend on the developer's globally installed package. Pin the service/runtime version compatible with the Rust client. Keep optional scientific packages in the selected project environment; do not ship them as an implicit namespace guarantee.

The Windows build is the first supported package: test clean-machine launch, application install/uninstall, spaces/non-ASCII paths, token ACLs, file dialogs, Job Object behavior, code signing, and antivirus interactions. Qualify Windows 11 x64 initially; claim additional versions only after testing. Linux packaging must declare graphics/system library requirements and test X11/Wayland, permissions, process groups, and clean-machine launch on explicitly listed distributions.

Install/update steps are atomic and recoverable. Never hot-replace an attached supervisor or kill another client's session during update. Handle a service/client version mismatch with compatibility negotiation or an explicit restart at a safe time. Keep user notebooks, history, drafts, and app preferences outside the installation directory. Uninstall removes application binaries; removing user data is a separate explicit choice.

Ship a license/dependency inventory, supported-platform matrix, keyboard guide, first-run instructions, and troubleshooting guide. Document graphics startup failures with useful diagnostics. No alternative software renderer is promised until proven by the selected stack.

## 15. Repository structure and engineering rules

Proposed structure:

```text
desktop/
  Cargo.toml, Cargo.lock, rust-toolchain.toml
  crates/app/                 # application bootstrap, actions, window/tab lifecycle
  crates/ui/                  # notebook/editor/panels, tokens, accessible controls
  crates/client/              # desktop protocol, inspection/source helper clients
  crates/models/              # source/runtime/history presentation state
  assets/                     # licensed fonts, icons, themes
src/kernelledger/desktop/     # Python service facade, operations, subscriptions
schemas/desktop-protocol.schema.json
tests/desktop/                # protocol, recovery, shared folding fixtures
docs/desktop/                 # UX spec, shortcuts, platform/packaging notes
```

Keep views free of transport and persistence logic. Models expose typed states/actions. The client is independently testable against a fake service. Backend validation remains authoritative. Centralize error-to-recovery-action mapping, cancellation, and request identity. Use stable IDs throughout; list indices never identify cells or operations.

CI runs Rust formatting/lints/unit tests, Python checks, shared protocol fixtures, and packaging smoke tests on supported platforms. Add a headless model test layer before relying on expensive visual tests. Use targeted screenshots for layout/theme regressions and real OS automation for dialogs, focus, clipboard, accessibility, and process lifecycle. Do not gate correctness solely on screenshots.

## 16. Delivery phases and exit gates

| Phase | Deliverables | Exit gate |
| --- | --- | --- |
| 0: Feasibility and UX specification | GPUI/editor spike, virtualized notebook, platform probes, native MIME samples, webview experiment, annotated screens and action matrix. | Core editor/input/accessibility/platform behavior proven; output fallback chosen; dependency set pinned. |
| 1: Backend contracts | Versioned desktop protocol, inspection/source helpers, accepted operations, durable dedupe and catch-up, input ownership, migrations and revised runtime plan where required. | GUI and CLI coexist; offline inspection executes nothing; lost-ack/reconnect tests produce no duplicate side effects. |
| 2: Editing shell | Welcome, tabs, source cells, structural editing, autosave/drafts, external change conflicts, keyboard commands, Markdown preview. | Crash-safe saves and multi-client conflict tests pass; editing works without a worker. |
| 3: Execution and outputs | Environment picker, execution/batch queue, runtime controls, stdin, stream/error/image renderers, stale-source badges. | Output flood, interrupted run, worker crash, input disconnect and source-race scenarios pass. |
| 4: Durable workflows | History/source comparison, checkpoint reports, restore/continue/replay, trust workflow, blob recovery. | A reopen/restore/continue journey preserves prior sessions and accurately reports omissions and drift. |
| 5: Interchange and complete UX | ipynb/bundles/descriptors, pins/GC, diagnostics/repair, settings, full accessibility and supported MIME behavior. | CLI parity table is satisfied or explicitly documented as deferred; keyboard/screen-reader journeys pass. |
| 6: Qualification and release | Performance tuning, clean-machine packages, signing, version compatibility, update/recovery, user docs. | Platform matrix, security/output boundary tests, packaging and acceptance suite all pass. |

Build phases in order; output renderer samples and accessibility checks start in phase 0 and continue throughout. Optional LSP, namespace inspection, and embedded webview are separately gated additions. Estimates follow the feasibility results; do not commit to dates before discovering the editor and service costs.

## 17. Acceptance scenarios and release checklist

Each scenario has an automated model/service test where possible plus manual OS validation where behavior depends on native integration.

1. Create a notebook, add each cell type, reorder/split/merge, attach an image, save, reopen with the CLI, and verify identical IDs/order/canonical source.
2. Edit with autosave, crash during a structural commit, reopen, recover a draft, and resolve an external edit without losing either version.
3. Run an unsaved edit, verify the committed hash equals recorded source, edit during execution, and observe a stale-output badge afterward.
4. Run all, stop on error, cancel the remaining queue, and attach a CLI client without violating accepted ordering.
5. Flood stdout, emit display updates and clear-output events, request input, disconnect/reconnect, and verify the final fold matches the Python fold exactly.
6. Interrupt a loop; escalate to worker termination; verify descendants are gone and the recorded execution outcome is correct.
7. Close the application during a long run, reopen, reconnect to the same supervisor, and recover durable output without repeating the run.
8. Crash the supervisor during work; takeover reports incomplete/crashed history and never invents a successful execution.
9. Publish complete/partial/refused/failed checkpoints, inspect every omission, restore with matching and mismatched interpreters, and inspect binding failures.
10. Lose the response after an operation commits; reconnect and look it up by ID without rerunning or republishing it.
11. Inspect foreign artifacts without execution; reject malicious HTML/SVG/oversized frames; trust only the selected artifact before restore/replay.
12. Import/export ipynb and bundles, including stale outputs, absent codecs, corrupt/missing blobs, and recorded input; preserve destination tracked source during history import.
13. Open a notebook on a read-only directory, with unavailable runtime and malformed source, and still inspect valid persisted history.
14. Complete the primary edit/run/history/checkpoint/restore journeys using only keyboard and a screen reader; verify IME and non-ASCII paths.
15. Install and update on clean supported systems, attach to an older running supervisor, handle the version difference, and retain user data on uninstall.

Release requires explicit answers for the supported OS/graphics matrix, dependency pins, source transaction recovery, API/ledger migration compatibility, event cursor retention, operation dedupe retention, input ownership, HTML fallback, packaging provenance, and performance baselines. Any unfinished capability is hidden or labeled unavailable; no success state may substitute for an unknown backend outcome.
