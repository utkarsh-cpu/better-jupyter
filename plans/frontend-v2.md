# KernelLedger desktop frontend plan, version 2

**Status:** Superseded on 2026-10-05 by [frontend plan version 3](../FRONTEND_PLAN.md).

**Archived summary:** UI-only GPUI plan over runtime plan version 4, with requirement IDs, an open-decision register, a vertical slice and a timeboxed feasibility gate. Version 3 keeps all of it and adds requirements, scenarios and a UX qualification gate derived from the 2026-10-04 UI/UX audit of the development build: window-size and reflow rules, confirmation and error-message standards, readable record labels, a Stopped runtime state, keyboard precedence and list navigation, per-surface state, cell reveal, delete safety, history search, help and identity, measurable accessibility thresholds, and a repeatable scorecard and usability study.

The text below is the version 2 frontend plan as it stood when version 3 replaced it.

---

# KernelLedger desktop frontend plan

| Item | Decision |
| --- | --- |
| Frontend plan version | 2 |
| Date | 2026-10-04 |
| Status | Current. Not implemented. |
| Supersedes | [Frontend plan version 1](frontend-v1.md) |
| Requires | [Runtime plan](../PLAN.md) version 4 or later, sections 3 (locks, registry), 5.1, 6, 8.1–8.8, 9.1, 10, 12, 15, and 17. |
| Versioning | [Plan versioning rules](README.md). Requirement ids `F-<AREA>-<nn>`, scenarios `FS-<nn>`, open decisions `FD-<nn>`. |
| Primary stack | Rust, GPUI, GPUI Kit, as a client of the Python supervisor and desktop helper. |
| Initial platforms | Windows 11 x64 first. Linux x64 after its platform gates pass. |

This plan specifies the desktop application only. Every runtime behavior, persisted format, protocol message, and CLI behavior it relies on is defined in the runtime plan and cited by section, written "runtime §8.3". When this plan needs a contract the runtime plan lacks, the runtime plan publishes a new version first (versioning rule 4). Nothing here describes an API that does not exist as current runtime behavior or as a **(v4)** runtime addition.

## Version history

| Version | Date | Status | Shape |
| --- | --- | --- | --- |
| 1 | Archived 2026-10-04 | Superseded | GPUI notebook plan that also specified the source service, desktop protocol, and ledger changes. |
| 2 | 2026-10-04 | Current | UI-only plan over runtime plan 4. Adds requirement ids, an open-decision register, Git, runtime-manager, interpreter, secret-input, table and LaTeX output, and clear-output coverage, a vertical slice, and a timeboxed feasibility gate with a named fallback. |

## 1. Product goals and boundaries

Build a native desktop notebook for editing ordinary source files, running Python in a project environment, inspecting durable history, and continuing from typed checkpoints. At every moment a user can tell apart source on disk, the source an execution recorded, the live namespace, and a published checkpoint.

**Core journeys.** Every journey has acceptance scenarios in section 18.

| Id | Journey |
| --- | --- |
| J1 | Open or create a notebook, choose an interpreter, edit cells, run them, and read results. |
| J2 | Reopen the application and inspect previous executions without starting Python. |
| J3 | See why output is stale after an edit, and compare executed with current source. |
| J4 | Publish a checkpoint, understand its omissions, restore into a new session, and continue. |
| J5 | Inspect and import a shared bundle, review provenance, and trust it explicitly before restore or replay. |
| J6 | Recover from an application crash, worker crash, supervisor restart, interrupted operation, or external edit without guessing what was saved or run. |
| J7 | Pull, merge, or switch Git branches underneath an open notebook and resolve the result. |
| J8 | See every running runtime on the machine and stop the ones no longer needed. |

**Requirements.**

- **F-GOAL-01** Opening a notebook, previewing Markdown, viewing history, inspecting a bundle, and showing checkpoint metadata execute no notebook code. They use the desktop helper (runtime §8.7).
- **F-GOAL-02** Closing a window saves or reconciles drafts and detaches. It never stops a supervisor or interrupts a computation. Stopping a runtime is a separate, named action.
- **F-GOAL-03** Editing stays available while the worker is busy, starting, disconnected, or absent.
- **F-GOAL-04** Out of scope for the first release: remote execution, collaborative editing, cloud accounts, a browser frontend, IPython magics, widgets, top-level await, a debugger, arbitrary output JavaScript, macOS, ARM64, and UI translations. A live variable inspector, LSP support, and an embedded webview are separately gated additions (section 15) and do not block the release.

## 2. Stack and feasibility

- **F-STACK-01** A Rust workspace under `desktop/` uses GPUI and GPUI Kit. Pin an exact compatible dependency set, commit `Cargo.lock`, and pin `rust-toolchain.toml`. GPUI Kit pins exact `gpui-pre` snapshot versions in its [workspace manifest](https://raw.githubusercontent.com/longbridge/gpui-kit/main/Cargo.toml). Upgrade the whole family together, as a deliberate change with UI regression checks. GPUI is pre-1.0 ([GPUI README](https://github.com/zed-industries/zed/blob/main/crates/gpui/README.md)).
- **F-STACK-02** The render thread never blocks on file I/O, process startup, hashing, sockets, or image decoding. Use GPUI's executors. Add another async runtime only if an integration requires it.
- **F-STACK-03** The `client` and `models` crates (section 14) do not depend on GPUI. This keeps the protocol client, state machines, and their tests reusable if the UI stack changes (FD-01).
- **F-STACK-04** Syntax highlighting is whatever the GPUI Kit editor supports well for Python. Its manifest carries both `syntect` and tree-sitter. Phase F0 confirms which one the editor uses and whether incremental highlighting stays within the typing budget (FD-02).

**Phase F0 feasibility gate.** F0 has a fixed budget, agreed before it starts (FD-15, default four engineer-weeks). It must prove:

- **F-STACK-10** A real Python editor inside a virtualized cell list keeps selection, focus, undo, and IME composition when cells mount and unmount.
- **F-STACK-11** On Windows 11: graphics on integrated and discrete GPUs, font rendering, clipboard, IME, NVDA, and mixed-DPI moves and resizes.
- **F-STACK-12** A packaged Rust application starts the private KernelLedger distribution (runtime §6) and a separate project interpreter for the worker.
- **F-STACK-13** Native output rendering handles a large stream, a large image, and a 1,000-row HTML table without freezing the editor.
- **F-STACK-14** The webview experiment (focus, clipping, scrolling, resizing, DPI, accessibility, composition) is run and recorded. Its result does not block F0, because native rendering is the baseline (section 8).

Record the OS build, graphics backend, GPU and driver, toolchain, and dependency revisions. If F-STACK-10, -11, or -12 fails within the budget, F0 ends with a recorded failure and FD-01 chooses the fallback. The fallback order is (a) Tauri 2 with CodeMirror 6, for mature IME, accessibility, and editor behavior, with the Rust `client` and `models` crates kept and HTML output in a sandboxed iframe; then (b) Qt 6. A new frontend plan version records the switch before any screen is built on it.

## 3. Layout and information architecture

The main window has a navigation rail, a resizable notebook sidebar, a central notebook editor, an optional right inspector, and a bottom status bar. Panels can be hidden and restored. Regions are fixed, without free docking.

| Surface | Requirement | Content and behavior |
| --- | --- | --- |
| Welcome | F-IA-01 | Recent notebooks, open directory, create, import ipynb, inspect bundle. A missing directory is a recoverable entry. |
| Notebook sidebar | F-IA-02 | Ordered cells with type, title, search, and running, queued, error, stale, and conflict indicators. Select and reveal. |
| Notebook editor | F-IA-03 | Code, Markdown, and raw cells. Editor, cell toolbar, per-session execution label `[n]` with the immutable execution id on hover, output, inline errors, insert handles, and collapsible input and output. |
| Runtime toolbar | F-IA-04 | Interpreter, session, runtime state, Run cell, Run above, Run below, Run all, Interrupt, Restart, Checkpoint. |
| History | F-IA-05 | Sessions and executions, paginated, with outcome, timestamps, duration, source hash, provenance, inputs (redacted where recorded so), and source and output views. |
| Checkpoints | F-IA-06 | Outcome, time, session, interpreter, roots, exclusions, notes, pins, restore and continue. |
| Inspector | F-IA-07 | Selected execution or checkpoint details and source comparison. |
| Runtimes | F-IA-08 | Every registry entry (runtime §3) with project, notebook, verified state, uptime, worker state, attached client count, and Stop. Stale entries are labeled and can be pruned. |
| Diagnostics | F-IA-09 | Doctor results, startup and connection problems, interpreter candidates, migration status, repair previews, logs. |
| Settings | F-IA-10 | Section 11. |
| Command palette | F-IA-11 | Searchable actions with shortcuts, an explanation for each unavailable action, recent notebooks, navigation. |

- **F-IA-20** Each notebook opens in a tab with its own drafts, history selection, connection, and runtime state. A tab can be detached into a window without starting another worker.
- **F-IA-21** A notebook is identified by its canonical project path plus file identity where the OS provides it, never by notebook UUID, which clones share (runtime §5). Opening the same notebook twice attaches both tabs to the same supervisor and the same helper state.
- **F-IA-22** Historical views carry breadcrumbs and a persistent "recorded" label. History opens read-only. "Copy recorded source into current cell" is an explicit edit with a preview.

## 4. Visual design and interaction

- **F-UX-01** Shared tokens for spacing, type, color, radius, elevation, focus rings, and state indicators. Light, dark, system, and high-contrast themes. Status always uses text or an icon in addition to color.
- **F-UX-02** Default density suits long notebooks: compact headers, expandable output, resizable panels, persistent scroll position, and a clearly visible focused cell.
- **F-UX-03** Lists show relative times. Details show absolute local times and the stored UTC value on hover.
- **F-UX-04** Every data surface specifies and implements these states: empty, loading, ready, partial, disconnected, permission denied, read-only snapshot, corrupt data, unsupported (newer) schema, migration required, Git conflict markers, and retryable error. Each state uses the runtime's error codes and validation states (runtime §5.1, §8.1) and names one precise recovery action.
- **F-UX-05** A toast is supplementary. An error that blocks work stays visible where it happened.

**Keyboard model.** The notebook has an edit mode and a command mode, as in Jupyter. The mode is always shown in the status bar and announced to screen readers when it changes.

- **F-KEY-01** Defaults, all rebindable:

| Action | Mode | Default |
| --- | --- | --- |
| Run cell, select next | Both | Shift+Enter |
| Run cell, keep selection | Both | Ctrl+Enter |
| Run cell, insert below | Both | Alt+Enter |
| Enter edit mode / command mode | — | Enter / Escape |
| Insert above / below | Command | A / B |
| Cell type code / Markdown / raw | Command | Y / M / R |
| Delete cell / undo cell deletion | Command | D, D / Z |
| Move cell up / down | Command | Ctrl+Shift+Up / Ctrl+Shift+Down |
| Toggle output collapse | Command | O |
| Save now | Both | Ctrl+S |
| Command palette | Both | Ctrl+Shift+P |
| Find in cell / notebook search | Both | Ctrl+F / Ctrl+Shift+F |
| Undo / redo | Both | Ctrl+Z / Ctrl+Shift+Z and Ctrl+Y |

- **F-KEY-02** Run all, Run above, Run below, Checkpoint, Interrupt, Restart, and Stop runtime are named actions in the palette.
- **F-KEY-03** No shortcut fires during IME composition, and no binding overrides standard text-editing keys in edit mode.
- **F-KEY-04** Every structural action (insert, move, change type, delete, split, merge, output expansion, panel focus, dialog dismissal) works without a pointer. Dragging is an optional equivalent of keyboard reordering.

## 5. Source editing, saving, Git, and conflicts

All tracked-source writes go through the helper's `source.mutate` (runtime §5.1, §8.7). The application has no serializer for `notebook.json` or cell files.

- **F-ED-01** Code, Markdown, and raw cells. Stable ids survive edits and reordering.
- **F-ED-02** Add, duplicate with a new id, delete with undo, move, convert type, split, merge compatible cells, and copy and paste cells with attachment remapping. Each structural action is one journaled mutation.
- **F-ED-03** Supported metadata is preserved. Unsupported or malformed data shows a validation error and is never erased.
- **F-ED-04** Cell-local undo for text and notebook-level undo for structure. Undoing an action that has been saved is a new mutation with its own expected revisions, so it can conflict like any other.
- **F-ED-05** Markdown preview with attachments, safe links, code fences, lists, tables, and inline and display math (FD-05). Raw cells have no run action.
- **F-ED-06** Search and replace in the current cell, and notebook-wide with a preview. Historical source search is a separate History feature.
- **F-ED-07** Multi-cursor, indentation, bracket matching, folding, line numbers, configurable tab width, and grapheme-aware selection.

**Saving.**

- **F-ED-10** Autosave sends a mutation after a short debounce (FD-08, default 500 ms). The cell shows Saving, Saved, Conflict, or Save failed.
- **F-ED-11** The draft store holds only content the source service has not yet acknowledged: pending, failed, or conflicted edits. A draft is deleted when its mutation is acknowledged. It lives in the application profile, keyed by notebook identity (F-IA-21), cell id, base revision, and draft revision, and never holds tokens. Drafts unresolved after FD-12's retention period prompt for a decision and are never deleted silently.
- **F-ED-12** After a crash, the application compares each draft with disk and offers restore, compare, or discard. It never overwrites a changed file automatically.
- **F-ED-13** After a lost mutation reply, the application re-reads and compares with the intended result (runtime §8.7), and only retries when the change is absent.

**External changes and Git.**

- **F-GIT-01** File watching only invalidates caches. Revisions decide conflicts (runtime §5.1).
- **F-GIT-02** A change that touches many files at once, such as a checkout, pull, or merge, produces one notebook-level banner, "N cells changed on disk", with Reload and Review. It never produces one dialog per cell. Cells without local edits reload silently and keep their stale-output badges. Cells with unacknowledged edits become per-cell conflicts.
- **F-GIT-03** A per-cell conflict keeps both versions and offers compare, take disk, take mine (an explicit overwrite with fresh expectations), or copy mine to a new cell.
- **F-GIT-04** The `conflict_markers` state shows the affected files and offers Open in external editor, Show markers, and Run doctor after resolution. Execution and mutation of affected cells stay blocked, and history stays readable.
- **F-GIT-05** Handle rename and deletion of the notebook directory, read-only directories (runtime §8.7 `read_only_snapshot`), a full disk, antivirus locks, and a partial manifest, each with its F-UX-04 state.
- **F-GIT-06** The status bar shows the current Git branch when the directory is a repository, read without running Git commands that change state. Git operations themselves are out of scope.

**Run integrity.**

- **F-ED-20** Running an edited cell first waits for its mutation to be acknowledged, then sends `run` with that `expected_sha256` (runtime §8.2). A `source_conflict` blocks the run and opens the conflict UI. The UI never shows one revision while running another.

## 6. Runtime states and execution controls

Connection, worker, operation, and execution states are tracked separately in `models`.

| State | Requirement | User-visible behavior |
| --- | --- | --- |
| Offline inspection | F-RT-01 | Source, history, and checkpoints readable through the helper. Running or any runtime action starts the supervisor explicitly. |
| Interpreter required | F-RT-02 | Shown when start fails with `interpreter_required` (runtime §6). Lists candidates with version and codecs, plus Browse. Offers "remember for this notebook", which writes `.kl/config.json`. Never chooses automatically. |
| Starting or connecting | F-RT-03 | The actual stage and interpreter, Cancel, and Detach. Editing continues. |
| Migration required | F-RT-04 | Explains schema 2 (runtime §9.1), the backup, and that version 3 `kl` will refuse the migrated ledger. Migrate is an explicit action. Inspection still works. |
| Idle | F-RT-05 | Run, restart, checkpoint, restore, and replay available, subject to trust and compatibility. |
| Queued or executing | F-RT-06 | Queue with cell, client, and batch. Executing cell highlighted. Interrupt available. Checkpoint disabled with the reason. |
| Waiting for input | F-RT-07 | See F-RT-30. |
| Interrupting | F-RT-08 | Requested interrupt, elapsed time, and an explicit "Terminate worker" escalation (`interrupt_kill`). |
| Checkpointing, restoring, replaying, migrating | F-RT-09 | Backend stages and operation id. No invented percentages. |
| Disconnected | F-RT-10 | Last known data with a stale indicator. Reconnect, resubscribe from the stored cursor, and look up pending operations before enabling dependent actions. |
| Crashed or incomplete | F-RT-11 | Recorded outcome and environment. Restart empty, or restore from a checkpoint. |
| Unknown outcome | F-RT-12 | An operation in state `unknown` (runtime §8.3) shows what is known and is never presented as success or failure. |

- **F-RT-20** Run all, Run above, and Run below each submit one batch (runtime §8.4) with expected hashes for every included cell. Stop on error is the default. "Continue on error" is an explicit per-run choice and a setting.
- **F-RT-21** The queue shows the frozen batch. A cell edited after acceptance shows "running accepted version" in the queue. "Cancel remaining" is separate from Interrupt.
- **F-RT-22** Restart and restore create new sessions. The action is labeled "Restart (empty namespace)". Stop and Restart show the attached client count first and confirm when other clients are attached.
- **F-RT-23** Closing a window never interrupts or stops work (F-GOAL-02). Quitting with runtimes still running lists them once, offering keep running (the default) or stop selected.

**Input.**

- **F-RT-30** An input request appears inline at its execution as a focusable prompt. It shows which client holds the claim (runtime §8.5). Interrupt and navigation stay responsive.
- **F-RT-31** An ordinary `input()` prompt states "Your reply is recorded in history and included in bundles" before the user types.
- **F-RT-32** A `getpass` prompt is masked. It states "Not recorded" when the runtime advertises `secret_input_redaction` and `record_secret_input` is false, and "Recorded" otherwise.
- **F-RT-33** A prompt claimed elsewhere is read-only with the claimant's name. It becomes claimable if that claim is released.

**Runtime manager.**

- **F-RT-40** The Runtimes surface (F-IA-08) reads the registry, verifies each entry by authenticating (runtime §3), and refreshes on focus and on demand. Stop uses the runtime's `stop` operation with the F-RT-22 confirmation.
- **F-RT-41** The status bar shows the count of running runtimes and opens the Runtimes surface.

## 7. Client architecture over the runtime contracts

This section describes how the application uses runtime §8. It defines no messages.

- **F-CL-01** Per notebook, the client opens one helper process (FD-07) and, when a supervisor runs, the control, subscription, and blob connections (runtime §8.1). It starts a supervisor with the private distribution and an explicit `--python` when one has been chosen.
- **F-CL-02** The client negotiates the desktop protocol version and reads capabilities. A missing capability hides or labels its feature, and the label names the runtime version that adds it. With no common version, the client shows both ranges and offers "Restart runtime with the bundled version" at a safe time, never automatically.
- **F-CL-03** Every side-effecting request gets a `client_request_id` stored before sending. After a timeout or disconnect the client calls `operation.get`. It never resubmits under a new id. A request whose outcome cannot be determined is shown as F-RT-12.
- **F-CL-04** The client persists its last `change_seq` per notebook and resubscribes from it. On `resync_required` it pages `changes.list` before re-enabling actions. Duplicates are discarded by `change_seq` and `(exec_id, seq)`.
- **F-CL-05** Output display may fold incrementally but reconciles with `fold.get` when an execution reaches a final status. Shared fixtures (runtime §8.8) test the incremental fold.
- **F-CL-06** Blob reads stream with bounded memory. Only verified content is cached. A missing or corrupt blob is a visible failure.
- **F-CL-07** Error-to-recovery-action mapping lives in one table in `models`, keyed by runtime error code.
- **F-CL-08** The client never writes SQLite, blobs, or tracked source directly, never manufactures events, never decodes checkpoint objects, and never logs or displays the token.
- **F-CL-09** A crashed helper is restarted with backoff. Its pending mutations are resolved through F-ED-13.

## 8. Output rendering and large notebooks

- **F-OUT-01** Folding follows runtime §10. Rendered output shows its execution label and, on demand, the execution id and source hash.

| MIME or content | Requirement | First-release treatment |
| --- | --- | --- |
| stdout, stderr, `text/plain` | F-OUT-10 | Selectable, copyable, searchable. Bounded ANSI styling, carriage-return handling, virtualized large streams. |
| Python errors | F-OUT-11 | Traceback with recorded source context. Links open the recorded revision. |
| `image/png`, `image/jpeg` | F-OUT-12 | Lazy decode, fit and zoom, save original, dimension and memory limits. |
| `image/svg+xml` | F-OUT-13 | Sanitized native rendering without scripts or external resources, or the source and export fallback. |
| `text/markdown` | F-OUT-14 | Native safe Markdown, including math (FD-05). |
| `text/latex` | F-OUT-15 | Native math rendering when FD-05 selects a renderer. Otherwise source with copy. |
| `application/json` | F-OUT-16 | Collapsible bounded-depth tree and raw view. Large values load incrementally. |
| `text/html` table subset | F-OUT-17 | **Required.** Native virtualized table for HTML whose structure is a `table` with `thead`, `tbody`, `tr`, `th`, and `td`, plus the pandas DataFrame `to_html` wrapper and its `style` block, which is ignored. Supports header rows, a multi-level index, truncation markers, copy as TSV, and the raw view. Scope is FD-04. |
| Other `text/html` | F-OUT-18 | The richest non-HTML MIME type in the bundle, then sanitized text, then source and export. The webview applies only after its gate (section 15). |
| Other MIME types | F-OUT-19 | MIME chooser, metadata, text fallback, save original. No implicit external opener. |

- **F-OUT-20** Output JavaScript never runs. Remote resources never load by default. Opening a link externally is an explicit action. Sanitization covers URL schemes, frames, scripts, obscuring styles, and resource limits.
- **F-OUT-21** Cell virtualization with stable ids, measured-height caching, and scroll anchoring. Editor state outlives the view.
- **F-OUT-22** Images decode in background tasks. Offscreen textures are released within a configurable budget.
- **F-OUT-23** Viewer truncation is separate from the runtime output cap. Both are shown, with access to stored data where it exists.
- **F-OUT-24** Output selection: the latest execution of the cell in the chosen session, with a history selector. Badge outputs whose source hash differs from current source. Mark imported and unattributed executions.
- **F-OUT-25** "Hide outputs" (per cell and notebook-wide) is view state, as runtime §10 defines. It is stored in application state per notebook, labeled "Hidden, still in history", and never sent to the runtime as a deletion.
- **F-OUT-26** Notebook search can include rendered text output, as a toggle.

## 9. Checkpoints, restore, replay, and trust

- **F-CK-01** Checkpoint results use exactly the four runtime outcomes (runtime §1). The id appears only after commit. Exclusions show binding, type, path, and reason. Notes include semantic limits.
- **F-CK-02** Before restore or continue, show provenance, roots and exclusions, interpreter compatibility, environment drift, and the new-session effect. A Python major.minor mismatch is refused unless the user chooses the recorded override (`allow_python_mismatch`).
- **F-CK-03** After restore, show the per-binding restore report.
- **F-CK-04** The replay preview shows the recorded cell sequence, source differences, drift, which inputs will be reused, and which are redacted and would stop the replay (runtime §8.5). It states that side effects repeat. Replay needs an explicit confirm and is never a navigation shortcut.
- **F-CK-05** Foreign artifacts are inspectable without trust. Before restore or replay, show identity, hash, origin, the relevant code, and why trust is needed. Trust covers one artifact and one operation. It is a workflow control, not a sandbox (runtime §15).
- **F-CK-06** Pins are named checkpoint references. GC shows the `dry_run` preview with bytes before deleting. The UI offers no history deletion (runtime §9).

## 10. Import, export, and CLI parity

| Runtime capability | Requirement | Frontend coverage |
| --- | --- | --- |
| init, add, rm, source edits | F-IO-01 | Create and open, cell editing, structural commands. |
| run cell, run all, run source | F-IO-02 | Cell and batch actions, plus an optional scratch run labeled unattributed. |
| status, serve, stop, interrupt, restart, runtimes | F-IO-03 | Runtime toolbar and the Runtimes surface. |
| operation, cancel | F-IO-04 | Queue and operation details. Cancel remaining. |
| log, show, events, MIME extraction | F-IO-05 | History, event inspector, save original output. |
| checkpoint, restore, continue, replay | F-IO-06 | Section 9 workflows. |
| import and export ipynb | F-IO-07 | File dialogs, preview, destination constraints, selected-session export, mismatch report. |
| bundle show, import, export | F-IO-08 | Inspect first. Choose destination. Integrity check. Tracked source is never overwritten on history import. |
| publish, fetch | F-IO-09 | Publish descriptor. Inspect descriptor and missing blobs. No remote download. |
| pin, unpin, gc | F-IO-10 | Section 9. |
| doctor, repair ids, retarget, migrate | F-IO-11 | Diagnostics with previews before each explicit action. |

- **F-IO-20** Bundle export previews what it includes and warns that source, history, outputs, recorded inputs, and checkpoint values can contain secrets. It names recorded password replies from schema 1 history (runtime §8.5). Redaction is offered only when the bundle format supports it.
- **F-IO-21** Invalid or unsupported artifacts give actionable validation details and never partially replace the current notebook.

## 11. Settings and environments

- **F-SET-01** Application preferences (theme, fonts, layout, shortcuts, viewer limits, draft retention, recent notebooks, hidden-output state) are separate from notebook-local `.kl/config.json` and from tracked metadata. Machine-local settings are never bundled.
- **F-SET-02** The notebook settings page edits every `.kl/config.json` key in runtime §4, including `record_secret_input` with its consequence explained. It validates before writing.
- **F-SET-03** The interpreter selector shows the executable, the resolution rule that chose it, the Python version, codec availability, and the distribution of the running supervisor. A change applies to the next worker and never restarts active work silently.
- **F-SET-04** The application shows install instructions and never installs packages into a project environment.

## 12. Accessibility and international text

- **F-A11Y-01** Accessibility is a release gate. Controls, the editor, the cell list, runtime status, dialogs, and output are in the accessibility tree. GPUI Kit's upstream support is verified in the composed application.
- **F-A11Y-02** Every core journey can be completed by keyboard alone, with visible focus, logical focus restoration, labeled icon buttons, sufficient contrast, text scaling, reduced motion, and the high-contrast theme.
- **F-A11Y-03** NVDA on Windows and Orca on Linux, before each platform qualifies. Execution transitions, mode changes, and errors are announced without reading output token by token. Output updates never steal focus.
- **F-A11Y-04** IME composition, combining characters, emoji, bidirectional prose, non-ASCII paths and identifiers, clipboard line endings, and font fallback are tested.
- **F-A11Y-05** Text offsets are explicit types: UTF-8 bytes, graphemes, and UTF-16 for LSP.
- **F-A11Y-06** The first release UI is English. All strings live in one catalog so translation is a later addition. Dates and numbers use locale formatting. Canonical source and stored timestamps never change.

## 13. Performance, reliability, and observability

Targets are initial and instrumented. They are measured on a documented mid-range Windows 11 machine and confirmed in F6 (FD-09).

| Scenario | Requirement | Initial target |
| --- | --- | --- |
| Cold start to interactive window, last notebook | F-PERF-01 | Under 1.5 s. Cached source visible under 2 s. |
| Start to first output of `print(1)`, runtime not running | F-PERF-02 | Under 3 s excluding the project interpreter's own import time. Each stage is shown while waiting. |
| Cached navigation | F-PERF-03 | Controls respond within 100 ms. Large loads stay asynchronous. |
| Typing and scrolling | F-PERF-04 | p95 frame work under 16.7 ms. No sustained freeze during an output flood. |
| Large notebook | F-PERF-05 | 1,000 cells open without 1,000 live editors. Source loads incrementally. |
| Large text output | F-PERF-06 | A 100 MiB stream is inspected through bounded paging. |
| Output flood | F-PERF-07 | Interrupt and input stay responsive. Overflow resyncs without losing history. |
| Images | F-PERF-08 | The decoded and GPU image cache stays within its configured budget. |
| Memory | F-PERF-09 | Idle with a 1,000-cell notebook under 500 MiB, excluding the image cache. |
| Reconnection | F-PERF-10 | Cursor and operation recovery without duplicate work. |

- **F-PERF-20** Timeouts apply to connection and startup only. Execution may run indefinitely. Checkpoint and restore show liveness and stages. A timeout never means failure (F-CL-03).
- **F-PERF-21** Structured local logs carry request ids, operation ids, and stage durations. No source, input, namespace values, tokens, or payloads are logged by default. A diagnostic export previews its files and redacts identifiable secrets. Crash reports and telemetry are opt-in.
- **F-PERF-22** Recovery tests: termination around mutation acknowledgment, operation acceptance, checkpoint publication, and event commit; stale endpoints, token rotation, port reuse, locked files, unavailable interpreter, full disk, bad blobs, a corrupt ledger, and supervisor takeover. Damaged state opens read-only first.

## 14. Packaging, release, and repository

- **F-PKG-01** The desktop package contains the application and a private, versioned CPython with one KernelLedger release, which is the `private` distribution (runtime §6). It never depends on a global install. Scientific packages stay in the project environment.
- **F-PKG-02** Windows first: clean-machine install and uninstall, paths with spaces and non-ASCII, token ACLs, file dialogs, Job Objects, code signing (FD-14), antivirus interaction, and the installer technology and update mechanism (FD-13). Windows 11 x64 initially.
- **F-PKG-03** Linux: declared graphics and system libraries, X11 and Wayland, permissions, process groups, and clean-machine launch on listed distributions (FD-10).
- **F-PKG-04** Install and update are atomic and recoverable. An update never replaces an attached supervisor or kills another client's session. A running older supervisor is handled by F-CL-02.
- **F-PKG-05** Notebooks, history, drafts, registry, and preferences live outside the install directory. Uninstall removes binaries. Removing user data is a separate explicit choice.
- **F-PKG-06** The desktop application has its own semantic version. Release notes state it, the bundled KernelLedger version, the runtime and frontend plan versions, and the desktop protocol versions spoken (versioning rule 7).
- **F-PKG-07** Ship a license inventory, a platform matrix, a keyboard guide, first-run instructions, and troubleshooting, including graphics startup failures. No software renderer is promised.

**Repository structure.**

```text
desktop/
  Cargo.toml, Cargo.lock, rust-toolchain.toml
  crates/app/        # bootstrap, actions, window and tab lifecycle (GPUI)
  crates/ui/         # notebook, editor, panels, tokens, accessible controls (GPUI)
  crates/client/     # desktop protocol and helper clients; no GPUI dependency
  crates/models/     # presentation state machines, error-to-action table; no GPUI dependency
  assets/            # licensed fonts, icons, themes
docs/desktop/        # UX spec, keyboard guide, platform and packaging notes
```

Runtime-owned pieces (`src/kernelledger/desktop/`, `schemas/desktop-*.schema.json`, `tests/fixtures/desktop/`) are specified in runtime §8.8 and §18.

- **F-ENG-01** Views hold no transport or persistence logic. Models expose typed states and actions. Ids identify everything. List indices never identify cells or operations.
- **F-ENG-02** `client` is tested against a fake service and against the runtime fixtures. `models` is tested headlessly. UI behavior is tested with GPUI's test application context before screenshots.
- **F-ENG-03** CI runs Rust format, lints, and unit tests, shared fixture tests, and packaging smoke tests on supported platforms. Targeted screenshots cover layout and themes. Real OS automation covers dialogs, focus, clipboard, accessibility, and process lifecycle. Screenshots never gate correctness alone.

## 15. Separately gated additions

| Addition | Gate |
| --- | --- |
| Embedded webview for HTML | F-STACK-14 passes on the platform, no token, bridge, or file system access, and network off by default, verified per platform. |
| LSP | Separate process with explicit selection, resource limits, cancellation, and shutdown. Diagnostics are labeled as editor diagnostics, not execution results. |
| Live namespace inspector | A runtime plan version adds a safe inspection contract for idle workers. Until then, nothing. |
| Provisional live output | Runtime deferred choice. Only if committed-event latency proves too slow in F3 measurements. |

## 16. Open decisions

Each decision has a phase that must close it and a default that applies if it is still open at that phase's exit. Closing a decision in a way that changes a requirement publishes a new frontend version.

| Id | Decision | Options | Closes in | Default |
| --- | --- | --- | --- | --- |
| FD-01 | UI stack if F0 fails | Tauri 2 with CodeMirror 6, Qt 6 | F0 exit | Tauri 2 |
| FD-02 | Python highlighter | Tree-sitter, syntect, as the GPUI Kit editor supports | F0 | Whatever the editor ships |
| FD-03 | Webview for HTML | Ship behind gate, defer | F0 | Defer |
| FD-04 | HTML table subset | DataFrame `to_html` only, or general simple tables, or Styler output | F3 | General simple tables, Styler ignored |
| FD-05 | Math rendering | Native renderer crate, source fallback only | F3 | Source fallback |
| FD-06 | Final command-mode key map | As F-KEY-01, or adjustments from the UX study | F0 | F-KEY-01 |
| FD-07 | Helper scope | One per notebook, one per window, one per application | F1 | One per notebook |
| FD-08 | Autosave debounce | 250 to 1000 ms | F2 | 500 ms |
| FD-09 | Performance targets | Confirm or revise section 13 | F6 | Section 13 |
| FD-10 | Linux distributions | Named list | F6 | Ubuntu 24.04 LTS only |
| FD-11 | Windows versions beyond 11 | Windows 10 22H2 or not | F6 | Windows 11 only |
| FD-12 | Draft retention | Days before prompting | F2 | 30 days |
| FD-13 | Installer and updates | MSIX, MSI, or NSIS; manual or automatic updates | F6 | MSI, manual updates |
| FD-14 | Code signing | Provider and certificate type | F6 | Must be decided; no default |
| FD-15 | F0 budget | Engineer-weeks | Before F0 | Four |

Runtime open choices are in runtime §21 and are not repeated here.

## 17. Delivery phases

Phases run in order. Accessibility checks and output renderer samples start in F0 and continue throughout. Dates follow F0 results.

| Phase | Needs runtime phase | Deliverables | Exit gate |
| --- | --- | --- | --- |
| F0: Feasibility and UX spec | — (parallel with 6) | Editor and virtualized-list spike, platform probes, native MIME samples including the HTML table, webview experiment, annotated screens, action and state matrix | F-STACK-10 to -13 proven within budget, or FD-01 chosen. Dependencies pinned. FD-02, -03, -06 closed. |
| F1: Vertical slice | 7 (designed together) | Open a notebook through the helper, edit and autosave one cell, run with an expected hash, stream committed text output, survive a dropped reply and a reconnect | FS-03 and FS-10 pass end to end. Protocol issues found are fixed in a runtime plan version before runtime phase 7 completes. FD-07 closed. |
| F2: Editing shell | 6, 7 | Welcome, tabs, all cell types, structural editing, autosave and drafts, external and Git changes, keyboard model, Markdown preview | FS-01, FS-02, FS-13, FS-16 pass. Editing works with no runtime. |
| F3: Execution and outputs | 8 | Interpreter selection, batches and queue, runtime controls, input prompts, stream, error, image, table, and JSON renderers, stale badges, hide outputs | FS-03 to FS-08 and FS-17 pass. FD-04, -05 closed. |
| F4: Durable workflows | 9 | History and comparison, checkpoint reports, restore, continue, replay, trust, blob recovery | FS-09 to FS-11 pass. |
| F5: Interchange and full UX | 9 | ipynb, bundles, descriptors, pins and GC, diagnostics, migration, Runtimes surface, settings, full accessibility and MIME behavior | FS-12, FS-14, FS-18, FS-19 pass. Every row in section 10 is covered or documented as deferred. |
| F6: Qualification and release | 9 | Performance work, clean-machine packages, signing, version compatibility, update and recovery, user docs | FS-15 and every section 13 target pass. FD-09 to -14 closed. |

## 18. Acceptance scenarios

Each scenario has an automated model or service test where possible, plus manual OS validation where native behavior matters. The last column traces to requirements.

| Id | Scenario | Covers |
| --- | --- | --- |
| FS-01 | Create a notebook, add each cell type, reorder, split, merge, attach an image, reopen with the CLI, and get identical ids, order, and canonical source. | F-ED-01, -02, -03 |
| FS-02 | Autosave, crash during a structural mutation, reopen, recover a draft, and resolve an external edit without losing either version. | F-ED-10 to -13, F-GIT-03 |
| FS-03 | Run an unsaved edit. The recorded hash equals the acknowledged revision. Edit during execution and see a stale badge after. | F-ED-20, F-OUT-24 |
| FS-04 | Run all, stop on error, cancel remaining, and submit a CLI run that executes after the batch. | F-RT-20, -21 |
| FS-05 | Flood stdout, emit display updates and clear events, request input, disconnect and reconnect. The final fold equals `fold.get`. | F-CL-04, -05, F-PERF-07 |
| FS-06 | Interrupt a loop, escalate to termination, and confirm the descendants are gone and the outcome is recorded correctly. | F-RT-08 |
| FS-07 | Close the application during a long run, reopen, reattach, and recover output without rerunning. | F-GOAL-02, F-RT-10 |
| FS-08 | Kill the supervisor during work. Takeover shows incomplete or crashed and an `unknown` operation honestly. | F-RT-11, -12 |
| FS-09 | Produce complete, partial, refused, and failed checkpoints. Restore with matching and mismatched interpreters and inspect binding failures. | F-CK-01 to -03 |
| FS-10 | Drop the reply after an operation commits, reconnect, and find it by id without rerunning. | F-CL-03 |
| FS-11 | Inspect foreign artifacts without execution. Reject malicious HTML, SVG, and oversized frames. Trust one artifact for one restore. | F-CK-05, F-OUT-20 |
| FS-12 | Import and export ipynb and bundles with stale outputs, absent codecs, corrupt or missing blobs, and recorded input. Destination source is preserved. | F-IO-07, -08, -20, -21 |
| FS-13 | Open on a read-only directory with no runtime and malformed source, and still read valid history. | F-GIT-05, F-UX-04 |
| FS-14 | Complete the edit, run, history, checkpoint, and restore journeys by keyboard and screen reader alone, with IME and non-ASCII paths. | F-KEY-*, F-A11Y-* |
| FS-15 | Install and update on clean systems, attach to an older running supervisor, negotiate, and keep user data on uninstall. | F-PKG-*, F-CL-02 |
| FS-16 | Switch Git branches with one cell edited locally. One banner appears. Unedited cells reload. The edited cell becomes a conflict. Merge markers block that cell only. | F-GIT-02 to -04 |
| FS-17 | Answer a `getpass` prompt. Confirm nothing is recorded, replay stops at it, and an `input()` prompt shows the recording notice. | F-RT-31, -32, F-CK-04 |
| FS-18 | Start a runtime with no project interpreter, pick a candidate, and remember it. Then list runtimes from two notebooks and stop one from the Runtimes surface. | F-RT-02, -40, F-SET-03 |
| FS-19 | Open a schema 1 ledger, see migration required, migrate, and confirm the backup exists and history is intact. Then open a notebook with a newer schema and see the unsupported state. | F-RT-04, F-UX-04 |

**Release checklist.** Release requires recorded answers for the platform and graphics matrix, dependency pins, every open decision due by F6, the runtime plan version and protocol versions shipped, packaging provenance, and performance baselines. An unfinished capability is hidden or labeled unavailable. No success state stands in for an unknown outcome.

## 19. Withdrawn requirements

None. Version 1 had no requirement ids.

## 20. Changes from version 1

| Area | Version 1 | Version 2 |
| --- | --- | --- |
| Scope | Specified the source service, desktop protocol, operations, change feed, and ledger migration itself | UI only. Those contracts moved to runtime plan version 4 (§5.1, §8.1–8.8, §9.1) and are cited. |
| Traceability | Prose requirements, numbered scenarios | Requirement ids, scenario ids, a coverage column, and a withdrawn table |
| Open questions | Spread through the text as "must be defined" | Open-decision register with closing phase and default |
| Feasibility | Untimed gate, "revise the stack" on failure | Budgeted F0 with a named fallback order. `client` and `models` independent of GPUI. |
| Phasing | Backend contracts built before any UI used them | F1 vertical slice developed with runtime phase 7. Each phase names the runtime phase it needs. |
| Interpreter | "Preserve discovery order" | Interpreter-required state with candidates. The bundled interpreter never runs user code (runtime §6). |
| Secret input | Vague non-recording condition | Masked `getpass` with an explicit recorded or not-recorded notice, and recording notice for `input()` |
| Output | HTML fallback to text, no LaTeX | Required native HTML table for DataFrames. `text/latex` and Markdown math row. Hide outputs defined as view state. Output search. |
| Git | Generic external edits | Bulk-change banner, conflict-marker state, branch indicator |
| Runtimes | Not covered | Runtimes surface, status count, quit-time prompt |
| Format states | Not covered | Unsupported schema, migration required, read-only snapshot, conflict markers |
| Notebook features | Missing | Run above and below, execution labels, collapsible input and output |
| Keyboard | Edit-only bindings with an undefined Escape | Explicit edit and command modes, Jupyter-style command keys, Ctrl+Y redo |
| Drafts | Overlapped with autosave | Hold only unacknowledged edits |
| Performance | No startup or memory targets | Cold start, first output, and memory targets |
| Highlighting | Tree-sitter assumed | Verified against the GPUI Kit editor in F0 |
| Language | Unstated | English first release, externalized strings |
| Versioning | "Keep revisions in plans/frontend-vN.md" | [plans/README.md](README.md) rules, `Requires` row, independent application version |
