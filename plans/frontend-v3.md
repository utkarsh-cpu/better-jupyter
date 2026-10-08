# Archived plan version 3

**Status:** Superseded on 2026-10-08 by [FRONTEND_PLAN.md](../FRONTEND_PLAN.md)

**Archived summary:** Historical local notebook and desktop contracts before opt-in training checkpoint integration.

The text below is the version 3 plan as it stood when version 4 replaced it.

---

# KernelLedger desktop frontend plan

| Item | Decision |
| --- | --- |
| Frontend plan version | 3 |
| Date | 2026-10-05 |
| Status | Current. Partially implemented in an unqualified development build; [implementation coverage](../docs/desktop/coverage.md) lists what exists. |
| Supersedes | [Frontend plan version 2](frontend-v2.md) |
| Requires | [Runtime plan](v4.md) version 4 or later, sections 3 (locks, registry), 5.1, 6, 8.1–8.8, 9.1, 10, 12, 15, and 17. |
| Versioning | [Plan versioning rules](README.md). Requirement ids `F-<AREA>-<nn>`, scenarios `FS-<nn>`, open decisions `FD-<nn>`. |
| Primary stack | Rust, GPUI, GPUI Kit, as a client of the Python supervisor and desktop helper. |
| Initial platforms | Windows 11 x64 first. Linux x64 after its platform gates pass. |

This plan specifies the desktop application only. Every runtime behavior, persisted format, protocol message, and CLI behavior it relies on is defined in the runtime plan and cited by section, written "runtime §8.3". When this plan needs a contract the runtime plan lacks, the runtime plan publishes a new version first (versioning rule 4). Nothing here describes an API that does not exist as current runtime behavior or as a **(v4)** runtime addition.

## Version history

| Version | Date | Status | Shape |
| --- | --- | --- | --- |
| 1 | Archived 2026-10-04 | Superseded | GPUI notebook plan that also specified the source service, desktop protocol, and ledger changes. |
| 2 | Archived 2026-10-05 | Superseded | UI-only plan over runtime plan 4. Adds requirement ids, an open-decision register, Git, runtime-manager, interpreter, secret-input, table and LaTeX output, and clear-output coverage, a vertical slice, and a timeboxed feasibility gate with a named fallback. |
| 3 | 2026-10-05 | Current | Adds requirements and scenarios from the 2026-10-04 UI/UX audit of the development build ([report](../artifacts/ui-ux-audit/REPORT.md)): window-size and reflow rules, confirmation and error-message standards, readable record labels, a Stopped runtime state, keyboard precedence and list navigation, per-surface state, cell reveal, delete safety, history search, help and identity, measurable accessibility thresholds, and a UX qualification gate with a scorecard and usability study. |

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
- **F-IA-23** Each surface owns its data, selection, and scroll position. Switching surfaces never shows another surface's results; for example, Settings never shows Doctor output. Returning to a surface restores only that surface's own prior state.
- **F-IA-24** History has search and filters by cell, outcome, session, and time range, combinable and cleared in one action. The notebook sidebar search filters cells only and says so in its placeholder.
- **F-IA-25** Every drill-down (sessions, executions, execution detail, checkpoint detail) has a visible Back action and a keyboard equivalent (F-KEY-01). Back restores the parent list's selection and scroll position.
- **F-IA-26** Opening a directory that is not an initialized notebook explains why and offers "Initialize as notebook" (F-IO-01), Choose another, and Cancel. The Welcome surface also offers "Create sample notebook", which uses the same create path and contains Markdown, a code cell, and a deliberate error.

## 4. Visual design and interaction

- **F-UX-01** Shared tokens for spacing, type, color, radius, elevation, focus rings, and state indicators. Light, dark, system, and high-contrast themes. Status always uses text or an icon in addition to color.
- **F-UX-02** Default density suits long notebooks: compact headers, expandable output, resizable panels, persistent scroll position, and a clearly visible focused cell.
- **F-UX-03** Lists show relative times. Details show absolute local times and the stored UTC value on hover.
- **F-UX-04** Every data surface specifies and implements these states: empty, loading, ready, partial, disconnected, permission denied, read-only snapshot, corrupt data, unsupported (newer) schema, migration required, Git conflict markers, and retryable error. Each state uses the runtime's error codes and validation states (runtime §5.1, §8.1) and names one precise recovery action.
- **F-UX-05** A toast is supplementary. An error that blocks work stays visible where it happened.
- **F-UX-06** Every confirmation dialog has a title naming the verb and its target ("Stop runtime for sales?"), a description of that action's specific consequence, a human-readable summary of the target, and a confirm button that repeats the verb. Generic labels such as "Proceed" and raw values such as `{}` or JSON are not used. Initial focus is on the non-destructive choice, Escape cancels, and all text wraps inside the dialog at the minimum window size (F-LAY-01).
- **F-UX-07** Raw OS, socket, and transport errors are never the primary message. Every error says what happened, the effect on the user's work, and one recovery action. The raw detail sits in a "Technical details" disclosure that can be copied. Transport and OS failures get entries in the F-CL-07 table alongside runtime error codes.
- **F-UX-08** Every record row (session, execution, operation, checkpoint, runtime, doctor finding) leads with a human-readable label: cell title or source preview, outcome, local time (F-UX-03), duration, and for runtimes the notebook, interpreter path, and version. Ids and raw JSON appear only in a "Raw details" disclosure or on hover. A user can find a failed cell or the current runtime without reading an id.
- **F-UX-09** After insert, paste, duplicate, move, selecting an outline entry, or selecting a search result, the target cell is scrolled fully into view using measured bounds (F-OUT-21) and receives focus. A newly inserted cell opens in edit mode. This holds when large outputs sit above the target.
- **F-UX-10** Session lifecycle and computation use different words. "Running" and "Busy" describe computation only. A session that is open but not computing is "Active". Each runtime-related status offers a short explanation on hover and focus (F-HELP-02).

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
| Back from a drill-down (F-IA-25) | Command | Alt+Left |
| Help and shortcut sheet (F-HELP-01) | Both | F1 |

- **F-KEY-02** Run all, Run above, Run below, Checkpoint, Interrupt, Restart, and Stop runtime are named actions in the palette.
- **F-KEY-03** No shortcut fires during IME composition, and no binding overrides standard text-editing keys in edit mode, except the run bindings in F-KEY-05.
- **F-KEY-04** Every structural action (insert, move, change type, delete, split, merge, output expansion, panel focus, dialog dismissal) works without a pointer. Dragging is an optional equivalent of keyboard reordering.
- **F-KEY-05** In edit mode, Shift+Enter, Ctrl+Enter, and Alt+Enter are handled by the notebook before the editor's newline handling. Each runs the cell exactly once and leaves the source unchanged. Plain Enter inserts a newline. The startup shortcut list and keyboard guide describe this behavior.
- **F-KEY-06** Every list surface (command palette, notebook sidebar, History, Checkpoints, Runtimes, Diagnostics) uses one keyboard model: Up and Down move a visible active item, Home and End jump, Enter activates, Escape closes or clears, and focus returns to where it was before the surface opened. In the palette, typing filters and the first match becomes active.

**Window size and reflow.**

- **F-LAY-01** The application documents a minimum supported window size (FD-16). At that size, in every theme, every primary action and status indicator on every surface is reachable without horizontal scrolling. The window cannot be resized below it.
- **F-LAY-02** Below documented width thresholds, toolbar actions move into an overflow menu in priority order, and the notebook sidebar and inspector collapse to toggles. Runtime state and Interrupt never move into the overflow.
- **F-LAY-03** List rows shrink, wrap, or truncate with the full text on hover and focus. Row actions never clip. Dialog and banner text wraps within its container.
- **F-LAY-04** Layouts are verified at 100, 125, 150, and 200% display scaling, with Windows text size at 200%, and when moving the window between monitors with different scaling (F-STACK-11).

**Help and terminology.**

- **F-HELP-01** A Help entry in the navigation rail and the palette opens a shortcut sheet generated from the live key map, including rebindings, plus links to the shipped keyboard guide, first-run instructions, and troubleshooting (F-PKG-07).
- **F-HELP-02** A short in-app glossary defines session, worker, runtime, supervisor, namespace, checkpoint, restore, continue, replay, and trust. Status labels and dialogs that use these terms link to their entries.
- **F-HELP-03** An About dialog shows the application version, the bundled KernelLedger version, the runtime and frontend plan versions, and the desktop protocol versions (F-PKG-06), with a copy action for support.
- **F-HELP-04** A capability that this build does not implement is either hidden or shown as "Not available in this build" with a pointer to the CLI equivalent where one exists. This applies to development builds as well as releases.

## 5. Source editing, saving, Git, and conflicts

All tracked-source writes go through the helper's `source.mutate` (runtime §5.1, §8.7). The application has no serializer for `notebook.json` or cell files.

- **F-ED-01** Code, Markdown, and raw cells. Stable ids survive edits and reordering.
- **F-ED-02** Add, duplicate with a new id, delete with undo, move, convert type, split, merge compatible cells, and copy and paste cells with attachment remapping. Each structural action is one journaled mutation.
- **F-ED-03** Supported metadata is preserved. Unsupported or malformed data shows a validation error and is never erased.
- **F-ED-04** Cell-local undo for text and notebook-level undo for structure. Undoing an action that has been saved is a new mutation with its own expected revisions, so it can conflict like any other.
- **F-ED-05** Markdown preview with attachments, safe links, code fences, lists, tables, and inline and display math (FD-05). Raw cells have no run action.
- **F-ED-06** Search and replace in the current cell, and notebook-wide with a preview. Historical source search is a separate History feature.
- **F-ED-07** Multi-cursor, indentation, bracket matching, folding, line numbers, configurable tab width, and grapheme-aware selection.
- **F-ED-08** Deleting a cell shows an Undo notification naming the cell, in addition to Z and Ctrl+Z (F-KEY-01). Undo restores the cell's id, kind, position, source, and metadata. In any build where structural undo (F-ED-04) is not available, deleting a cell first asks for confirmation under F-UX-06. A failed delete leaves the source untouched.
- **F-ED-09** Markdown cells show the rendered view by default. Enter or double-click opens the source in place, and Escape or running the cell returns to the rendered view. Source and preview are not shown side by side unless the user turns that on.

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
| Stopped | F-RT-13 | After a `stop` the user requested (runtime §8.2) succeeds, the notebook shows Stopped, not Disconnected, and returns to offline inspection (F-RT-01). The client reconciles every operation it was tracking through `operation.get` before showing Stopped, so no finished operation remains labeled running. An expected connection close during the stop is not reported as an error. A stop by another client or the CLI is shown as "Stopped by another client". |

- **F-RT-14** Operations are labeled by their `kind` and target (cell title, batch size, or checkpoint) and by accepted time and elapsed or total duration (runtime §9.1 `operations`). The operation id is secondary under F-UX-08.
- **F-RT-20** Run all, Run above, and Run below each submit one batch (runtime §8.4) with expected hashes for every included cell. Stop on error is the default. "Continue on error" is an explicit per-run choice and a setting.
- **F-RT-21** The queue shows the frozen batch. A cell edited after acceptance shows "running accepted version" in the queue. "Cancel remaining" is separate from Interrupt.
- **F-RT-22** Restart and restore create new sessions. The action is labeled "Restart (empty namespace)". Stop and Restart show the attached client count first and confirm when other clients are attached.
- **F-RT-23** Closing a window never interrupts or stops work (F-GOAL-02). Quitting with runtimes still running lists them once, offering keep running (the default) or stop selected.
- **F-RT-24** Runtime controls are scoped to the active notebook. On surfaces with no active notebook (Welcome, Runtimes, Settings) they are hidden. On History, Checkpoints, and Diagnostics they name the notebook they act on. Every disabled control states why on hover and focus, as in the palette (F-IA-11).

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
- **F-SET-05** Application preferences are restored on the next launch before the first frame paints. The theme choice is Light, Dark, or Follow system, and Follow system is the default. Follow system tracks OS light, dark, and contrast-theme changes while the application runs.
- **F-SET-06** Settings summarizes the active notebook's interpreter (path, version, and the rule that selected it, F-SET-03) before offering to change it.

## 12. Accessibility and international text

- **F-A11Y-01** Accessibility is a release gate. Controls, the editor, the cell list, runtime status, dialogs, and output are in the accessibility tree. GPUI Kit's upstream support is verified in the composed application.
- **F-A11Y-02** Every core journey can be completed by keyboard alone, with visible focus, logical focus restoration, labeled icon buttons, sufficient contrast, text scaling, reduced motion, and the high-contrast theme.
- **F-A11Y-03** NVDA on Windows and Orca on Linux, before each platform qualifies. Execution transitions, mode changes, and errors are announced without reading output token by token. Output updates never steal focus.
- **F-A11Y-04** IME composition, combining characters, emoji, bidirectional prose, non-ASCII paths and identifiers, clipboard line endings, and font fallback are tested.
- **F-A11Y-05** Text offsets are explicit types: UTF-8 bytes, graphemes, and UTF-16 for LSP.
- **F-A11Y-06** The first release UI is English. All strings live in one catalog so translation is a later addition. Dates and numbers use locale formatting. Canonical source and stored timestamps never change.
- **F-A11Y-07** Measured thresholds, using [WCAG 2.2](https://www.w3.org/TR/WCAG22/) as the reference without claiming conformance: text contrast at least 4.5:1, and 3:1 for large text; non-text elements and focus indicators at least 3:1 against adjacent colors; pointer targets at least 24 by 24 logical pixels or adequately spaced. A test computes these from the theme tokens for every theme.
- **F-A11Y-08** Tab and Shift+Tab visit every interactive control in a documented order on every surface. Dialogs and menus contain focus while open and return it to the invoking control when closed. Focus is visible in every theme.
- **F-A11Y-09** The window title and accessible name are "<notebook> — KernelLedger", or "KernelLedger" on Welcome. Navigation destinations are exposed with their roles, names, and current-item state. Repeated controls carry their context, such as "Run cell sales". Rendered output text, tables, and image alternative text are reachable in the accessibility tree.

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
- **F-PERF-23** A repeatable timing harness and fixtures (a 1,000-cell notebook, a large history, a 100 MiB stream, and an image-heavy notebook) exist by F2 exit. Section 13 targets are measured and recorded at every phase exit from F2, not first in F6.

**UX qualification.** The 2026-10-04 audit of the development build ([report](../artifacts/ui-ux-audit/REPORT.md)) scored 51.7 out of 100 with 88.5% weighted evidence coverage. That audit's method becomes a standing phase gate.

- **F-QA-01** A versioned UX scorecard is kept under `docs/desktop/`, generated from one machine-readable rubric. It starts from the audit's 16 categories and 64 criteria and adds two categories: "Interchange and Git" (J5, J7) and "Input and multiple runtimes" (F-RT-30 to -33, J8, tabs and detached windows). Every criterion lists the requirement ids it tests, and every journey J1–J8 has at least one criterion. Weights total 100 (FD-17). Ratings run 0–5. Untested criteria are recorded as not tested, excluded from the score, and reported as reduced coverage, never estimated.
- **F-QA-02** The scorecard is rescored with a fixed fixture notebook at every phase exit from F2, preserving earlier evidence and recording the build and environment. Each phase exit reports the score, the weighted coverage, and open findings by priority.
- **F-QA-03** Each audit finding has an automated regression test where the behavior is testable: key routing, list navigation, and undo in GPUI's test application context; clipping through screenshots at the minimum size (F-LAY-01) in every theme; accessibility identity through a UI Automation tree check (FS-25). Screenshots never gate correctness alone (F-ENG-03).
- **F-QA-04** Usability studies run with five to eight participants, a mix of new and experienced notebook users, on journeys J1–J8. A formative round at F2 exit closes FD-06. A summative round in F6 records task success, errors, time on task, and a System Usability Scale score. Expert review never substitutes for these results.
- **F-QA-05** The release gate (FD-17) requires a scorecard total of at least 80, weighted coverage of at least 90%, no open P1 finding, and direct verification of keyboard-only, screen-reader, and recovery journeys. A score alone never establishes release readiness. A P1 finding is a serious task, safety, reachability, or accessibility issue.

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
| FD-06 | Final command-mode key map | As F-KEY-01, or adjustments from the F2 formative usability study (F-QA-04) | F2 | F-KEY-01 |
| FD-07 | Helper scope | One per notebook, one per window, one per application | F1 | One per notebook |
| FD-08 | Autosave debounce | 250 to 1000 ms | F2 | 500 ms |
| FD-09 | Performance targets | Confirm or revise section 13 | F6 | Section 13 |
| FD-10 | Linux distributions | Named list | F6 | Ubuntu 24.04 LTS only |
| FD-11 | Windows versions beyond 11 | Windows 10 22H2 or not | F6 | Windows 11 only |
| FD-12 | Draft retention | Days before prompting | F2 | 30 days |
| FD-13 | Installer and updates | MSIX, MSI, or NSIS; manual or automatic updates | F6 | MSI, manual updates |
| FD-14 | Code signing | Provider and certificate type | F6 | Must be decided; no default |
| FD-15 | F0 budget | Engineer-weeks | Before F0 | Four |
| FD-16 | Minimum window size and collapse thresholds (F-LAY-01, -02) | Logical pixels | F2 | 800 × 600 minimum. Sidebar collapses below 1,024 wide. Inspector collapses below 1,280 wide. |
| FD-17 | Scorecard weights and release thresholds (F-QA-01, -05) | Weights per category, release score, coverage | F2 | The audit's 16 weights scaled by 0.9, plus 5 for each new category. Release at 80 points and 90% coverage. |

Runtime open choices are in runtime §21 and are not repeated here.

## 17. Delivery phases

Phases run in order. Accessibility checks and output renderer samples start in F0 and continue throughout. Dates follow F0 results.

| Phase | Needs runtime phase | Deliverables | Exit gate |
| --- | --- | --- | --- |
| F0: Feasibility and UX spec | — (parallel with 6) | Editor and virtualized-list spike, platform probes, native MIME samples including the HTML table, webview experiment, annotated screens, action and state matrix | F-STACK-10 to -13 proven within budget, or FD-01 chosen. Dependencies pinned. FD-02, -03 closed. |
| F1: Vertical slice | 7 (designed together) | Open a notebook through the helper, edit and autosave one cell, run with an expected hash, stream committed text output, survive a dropped reply and a reconnect | FS-03 and FS-10 pass end to end. Protocol issues found are fixed in a runtime plan version before runtime phase 7 completes. FD-07 closed. |
| F2: Editing shell | 6, 7 | Welcome, tabs, all cell types, structural editing, autosave and drafts, external and Git changes, keyboard model, Markdown preview, window reflow, help, scorecard and timing harness | FS-01, FS-02, FS-13, FS-16, FS-20 to FS-23, FS-27, FS-29 pass. Editing works with no runtime. First scorecard rescore (F-QA-02) and formative study (F-QA-04) recorded. FD-06, -16, -17 closed. |
| F3: Execution and outputs | 8 | Interpreter selection, batches and queue, runtime controls, input prompts, stream, error, image, table, and JSON renderers, stale badges, hide outputs | FS-03 to FS-08, FS-17, FS-24 pass. FD-04, -05 closed. |
| F4: Durable workflows | 9 | History and comparison, history search, checkpoint reports, restore, continue, replay, trust, blob recovery | FS-09 to FS-11, FS-28 pass. |
| F5: Interchange and full UX | 9 | ipynb, bundles, descriptors, pins and GC, diagnostics, migration, Runtimes surface, settings, full accessibility and MIME behavior | FS-12, FS-14, FS-18, FS-19, FS-25, FS-26 pass. Every row in section 10 is covered or documented as deferred. |
| F6: Qualification and release | 9 | Performance work, clean-machine packages, signing, version compatibility, update and recovery, user docs, summative study | FS-15 and every section 13 target pass. F-QA-05 release gate met. FD-09 to -14 closed. |

Every phase from F2 also rescores the UX scorecard (F-QA-02) and records section 13 measurements (F-PERF-23).

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
| FS-14 | Complete the edit, run, history, checkpoint, and restore journeys by keyboard and screen reader alone, with IME and non-ASCII paths. Tab order, dialog focus containment, and focus return hold on every surface. Measured contrast and target sizes pass in every theme. | F-KEY-*, F-A11Y-* |
| FS-15 | Install and update on clean systems, attach to an older running supervisor, negotiate, and keep user data on uninstall. | F-PKG-*, F-CL-02 |
| FS-16 | Switch Git branches with one cell edited locally. One banner appears. Unedited cells reload. The edited cell becomes a conflict. Merge markers block that cell only. | F-GIT-02 to -04 |
| FS-17 | Answer a `getpass` prompt. Confirm nothing is recorded, replay stops at it, and an `input()` prompt shows the recording notice. | F-RT-31, -32, F-CK-04 |
| FS-18 | Start a runtime with no project interpreter, pick a candidate, and remember it. Then list runtimes from two notebooks and stop one from the Runtimes surface. | F-RT-02, -40, F-SET-03 |
| FS-19 | Open a schema 1 ledger, see migration required, migrate, and confirm the backup exists and history is intact. Then open a notebook with a newer schema and see the unsupported state. | F-RT-04, F-UX-04 |
| FS-20 | From edit mode and from command mode, press Shift+Enter, Ctrl+Enter, and Alt+Enter. Each runs the cell exactly once, the source is unchanged, and selection follows F-KEY-01. Plain Enter in edit mode inserts a newline. | F-KEY-01, -03, -05 |
| FS-21 | Without a pointer, open the palette, filter, move through results, run a command, and confirm focus returns. Repeat the list model in the sidebar, History, Checkpoints, Runtimes, and Diagnostics. | F-KEY-06, F-IA-11 |
| FS-22 | Delete a saved cell, then undo with the notification, with Z, and with Ctrl+Z. The id, kind, position, source, and metadata are restored each time. In a build without structural undo, delete asks for confirmation first. A delete that fails leaves the source untouched. | F-ED-02, -04, -08 |
| FS-23 | At the minimum window size and at each scaling level in F-LAY-04, in every theme, open every surface and every confirmation dialog. No primary action or status clips, dialog text wraps, and toolbar overflow and panel collapse work. Restore and Stop confirmations name their action and target. | F-LAY-01 to -04, F-UX-06 |
| FS-24 | Stop the runtime from idle, from busy, and from another client. Each time the notebook shows Stopped, shows no OS or transport error, and no operation remains labeled running. Inspection keeps working, and the next run starts a runtime. Operations show kind, target, and time. | F-RT-01, -13, -14, F-UX-07 |
| FS-25 | An automated UI Automation check finds the window name, navigation destinations with current-item state, contextual names on repeated cell controls, and rendered output text. | F-A11Y-01, -09 |
| FS-26 | Choose Dark, quit, and relaunch: Dark is restored before the first frame. Choose Follow system and switch the OS theme and contrast theme: the application follows while running. | F-SET-05, F-UX-01 |
| FS-27 | On a clean profile, with no CLI, create a notebook, add and edit a cell, run it, and reopen it with the edit saved. Open an uninitialized directory and initialize it from the prompt. Create the sample notebook. | F-IA-01, -26, F-IO-01 |
| FS-28 | Run Doctor, scroll, and switch to Settings: no Doctor content or scroll position appears. Drill from sessions to an execution detail and press Back: the parent selection and scroll return. Find a failed execution by cell and outcome filter, and identify the current runtime, without reading an id. | F-IA-23 to -25, F-UX-08 |
| FS-29 | Below a large output, insert a cell, select another from the sidebar, and open a search result. Each target is fully visible and focused, and the inserted cell is in edit mode. | F-UX-09, F-OUT-21 |

**Release checklist.** Release requires recorded answers for the platform and graphics matrix, dependency pins, every open decision due by F6, the runtime plan version and protocol versions shipped, packaging provenance, performance baselines, the final UX scorecard and summative usability results (F-QA-05). An unfinished capability is hidden or labeled unavailable (F-HELP-04). No success state stands in for an unknown outcome.

## 19. Withdrawn requirements

None. No requirement has been withdrawn in versions 2 or 3.

## 20. Changes from version 2

Every change below comes from the 2026-10-04 [UI/UX audit](../artifacts/ui-ux-audit/REPORT.md) of the development build. Finding numbers refer to that report. No runtime contract changes; new requirements cite existing runtime sections. The `Requires` row is unchanged.

| Area | Version 2 | Version 3 |
| --- | --- | --- |
| Header | Status "Not implemented" | Status states partial implementation in an unqualified development build and links the coverage document |
| Window size and reflow | Not covered | New F-LAY-01 to -04: minimum size, toolbar overflow, panel collapse, row and dialog wrapping, scaling matrix (F04, F05). FD-16 sets defaults. |
| Confirmations | Only Restart labeling (F-RT-22) | New F-UX-06: verb and target title, specific consequence, verb button, safe initial focus, wrapping (F05) |
| Error messages | Runtime error codes mapped to actions | New F-UX-07: no raw OS or transport error as the primary message. F-CL-07 table covers transport errors (F13). |
| Record presentation | Fields listed per surface | New F-UX-08 and F-RT-14: human-readable primary labels, ids and JSON behind disclosure, labeled operations (F07, operation identity criterion) |
| Runtime states | No state for an intentional stop | New F-RT-13 Stopped state with operation reconciliation (F13) |
| Action scope | Unavailable actions explained in the palette | New F-RT-24: runtime controls scoped to the active notebook, reasons on every disabled control |
| Keyboard | Shift+Enter "Both" modes, with F-KEY-03 forbidding overrides of editing keys | F-KEY-03 amended. New F-KEY-05 run-binding precedence and F-KEY-06 list keyboard model (F01, F02). Alt+Left Back and F1 Help added to F-KEY-01. |
| Navigation | Breadcrumbs only | New F-IA-23 per-surface state (F08), F-IA-24 history search and filters, F-IA-25 Back action |
| First use | Welcome actions listed | New F-IA-26: initialize an uninitialized directory, sample notebook (F11) |
| Editing | Delete with undo | New F-ED-08 Undo notification and confirm-until-undo rule (F03). New F-ED-09 Markdown rendered by default. |
| Cell reveal | "Clearly visible focused cell" | New F-UX-09: insert, move, outline, and search targets scroll into view and take focus (F09) |
| Terminology and help | Docs shipped with the package only | New F-UX-10 session versus computation wording (F12). New F-HELP-01 to -04: shortcut sheet, glossary, About, unavailable-capability labeling. |
| Preferences | Theme listed as an application preference | New F-SET-05 restore on launch and Follow system default (F10). New F-SET-06 interpreter summary. |
| Accessibility | "Sufficient contrast", unmeasured | New F-A11Y-07 measured thresholds, F-A11Y-08 focus order and containment, F-A11Y-09 window and contextual identity (F06). FS-14 extended. |
| Performance | Targets confirmed in F6 | New F-PERF-23: timing harness by F2, measured at every phase exit |
| UX qualification | None. FD-06 cited an undefined UX study. | New F-QA-01 to -05: versioned scorecard with two added categories, rescoring at every phase from F2, regression tests per finding, usability studies, release gate. FD-17 sets defaults. |
| Open decisions | FD-06 closed in F0 | FD-06 closes in F2 from the formative study. New FD-16 and FD-17. |
| Scenarios | FS-01 to FS-19 | New FS-20 to FS-29 |
| Phases | F2 to F6 gates | Gates list the new scenarios, the scorecard rescore, measurements, studies, and the F-QA-05 release gate |
| Release checklist | Platform, pins, decisions, versions, provenance, baselines | Adds the final scorecard and summative usability results |
