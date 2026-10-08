# KernelLedger UI/UX audit and scoring system

**Observed score: 51.7/100. Weighted evidence coverage: 88.5%. 56/64 criteria rated.**

The native notebook is visually coherent and useful for small Python workflows. Running code, saving edits, interrupting work, restoring a checkpoint, and rendering a table/chart succeeded. Keyboard execution, deletion recovery, adaptive layout, accessible navigation and the presentation of recorded work need attention before a broad user release. This is a structured expert assessment of a development build, not a participant study or accessibility certification.

## Environment and method

- Audit date: 4 October 2026. Windows 11; Python 3.12.10; KernelLedger 0.1.0 native Rust/GPUI UI.
- Executable: `desktop/target/debug/kernelledger-desktop.exe`, 51,521,024 bytes, last built 4 October 2026 11:48 local time. UI source timestamp: 11:47:56. Existing binary was used; no rebuild or product-code changes.
- Desktop mouse, Unicode keyboard input and screenshot capture used Windows APIs. The collaborative browser was initialized but the product is a native app, so its UI was audited directly.
- Display: 2880 x 1800 physical pixels at observed 200% scale. Main window tested at 2560 x 1600, 2048 x 1400 and 1600 x 1200 physical pixels, approximately 1280 x 800, 1024 x 700 and 800 x 600 logical outer dimensions. Content area is smaller due to window chrome.
- Isolated fixture: `artifacts/ui-ux-audit/notebook`. Existing `analysis/` was not used. Initial fixture had Markdown, two successful code cells, a deliberate ZeroDivisionError and raw notes. A temporary sleep cell was added/deleted and a rich-output cell was added for pandas/PNG testing.
- Evidence combines observed screenshots, saved source, runtime ledger output, native UI Automation tree and targeted implementation/document review. Source-backed unfinished features are labeled in criterion notes; they were not treated as successful live tests.
- An interrupted first launch was resumed. The app remained responsive during the completed audit journeys. Startup and performance were not benchmarked.
- Runtime was stopped after testing; app left open on the isolated notebook for review. Temporary always-on-top positioning used for screenshots was removed.

## Scoring rules

Sixteen categories carry weights totaling 100. Each has four equally weighted criteria. This rubric covers visual design, task efficiency, navigation, authoring, execution, feedback, scientific output, provenance, recovery, safety, keyboard use, accessibility, resizing, scale, preferences and help/platform integration.

| Rating | Meaning |
| --- | --- |
| 0 | Absent or unusable for the assessed task |
| 1 | Severe friction, blocked path or major missing capability |
| 2 | Partial; repeated workarounds or substantial ambiguity |
| 3 | Usable with noticeable gaps |
| 4 | Strong observed behavior with minor limitations |
| 5 | Excellent, repeatably verified behavior across relevant states |
| NT | Not tested; no pass/fail inference |

`Category rating = mean of its rated criteria.`

`Category points = category weight × category rating / 5.`

`Observed total = sum of category points.`

`Weighted evidence coverage = sum(category weight × rated criteria / all category criteria).`

NT criteria are excluded from a category mean and reduce coverage. If an entire category is NT, exclude its weight and renormalize the observed score over rated category weights. This prevents untested features being scored as failures. It also means the total is provisional: a high category score with low coverage must not be mistaken for demonstrated readiness. The performance category, for example, has only 25% coverage.

Proposed interpretation bands for this project: 90–100 excellent; 80–89 strong; 70–79 usable with gaps; 60–69 substantial improvement needed; below 60 major workflow/qualification gaps. These are custom rubric bands, not industry certification thresholds. Differences of a point or two are not meaningful without repeat assessment.

Proposed release gate: observed score at least 80, weighted coverage at least 90%, no unresolved P1 findings, and direct verification of keyboard-only, screen-reader and safe-recovery journeys. This gate is a recommendation, not a claim that a score alone establishes release readiness.

Usability criteria are informed by [Nielsen’s usability heuristics](https://www.nngroup.com/articles/ten-usability-heuristics/). Accessibility checks use [WCAG 2.2](https://www.w3.org/TR/WCAG22/) as a reference for keyboard, focus, contrast and target-size concerns; the audited app is native Windows software, so this is not a WCAG conformance determination.

## Weighted scorecard

| Category | Weight | Rating / 5 | Points | Coverage |
| --- | ---: | ---: | ---: | ---: |
| Visual hierarchy and consistency | 8 | 3.75 | 6.00 | 100% |
| Navigation and information architecture | 7 | 2.75 | 3.85 | 100% |
| Onboarding and first use | 5 | 2.75 | 2.75 | 100% |
| Editing and cell authoring | 10 | 3.00 | 6.00 | 100% |
| Execution and runtime control | 9 | 3.00 | 5.40 | 100% |
| Feedback and state communication | 6 | 2.75 | 3.30 | 100% |
| Output readability and scientific content | 6 | 3.00 | 3.60 | 100% |
| History and provenance usability | 6 | 1.75 | 2.10 | 100% |
| Checkpoints, diagnostics and recovery | 7 | 2.67 | 3.73 | 75% |
| Safety, prevention and user control | 7 | 2.25 | 3.15 | 100% |
| Keyboard efficiency and focus | 7 | 1.67 | 2.33 | 75% |
| Accessibility and inclusive interaction | 7 | 2.00 | 2.80 | 50% |
| Adaptive layout and window sizing | 6 | 1.33 | 1.60 | 75% |
| Perceived performance and scale | 4 | 4.00 | 3.20 | 25% |
| Preferences and personalization | 3 | 2.00 | 1.20 | 100% |
| Help, terminology and platform polish | 2 | 1.75 | 0.70 | 100% |
| **Total** | **100** | | **51.7/100** | **88.5% weighted** |

## Task outcomes

| Task | Outcome | Evidence |
| --- | --- | --- |
| Launch with initialized notebook | Pass: offline inspection, source/history available | [Initial screen](01-notebook-initial-view.png) |
| Edit and save source | Pass: comment persisted on disk, stale-output feedback shown | [Edit evidence](03-edit-keyboard-run-view.png), `notebook/cells/sales.py` |
| Run from cell button | Pass: stdout and result 675 | [Single-cell run](02-run-cell-view.png) |
| Run from focused editor using advertised shortcuts | Fail: inserts newline | [Ctrl+Enter](04-ctrl-enter-view.png) |
| Escape then Shift+Enter | Pass: execution ordinal advanced | [Command-mode run](05-command-mode-run-view.png) |
| Run all including deliberate error | Pass for visible execution/error retention; batch stop-on-error beyond the error was not separately tested | [Batch](06-run-all-error-view.png) |
| Inspect Python traceback | Pass: red error panel shows exception and source | [Traceback](30-bottom-cell-view.png) |
| Inspect history/session/execution | Partial: paths work, identity/detail presentation is difficult | [Execution details](10-execution-detail-view.png) |
| Create and restore checkpoint | Pass: new session has restored_checkpoint_id | [Dialog](12-restore-confirm-view.png), [ledger](runtime-evidence.json) |
| Escape confirmation | Pass | [Dismissed dialog](13-dialog-escape-view.png) |
| Run Doctor | Partial: result available as JSON | [Doctor](16-doctor-view.png) |
| Switch light/dark theme | Pass within session; resets after restart | [Dark settings](19-settings-top-view.png), [Startup](39-startup-view.png) |
| Filter cells/no matches | Pass: outline reports no matches | [Empty filter](22-filter-empty-view.png) |
| Palette search + keyboard activation | Partial: search works; Arrow Down/Enter does not execute | [Palette](21-palette-keyboard-view.png) |
| Add a code cell | Partial: added, but not immediately revealed/focused | [Add](28-add-cell-view.png) |
| Run and interrupt 30-second sleep | Pass: Running → interrupted → Idle | [Interrupt](32-interrupt-view.png), [ledger](runtime-evidence.json) |
| Delete a saved cell then Ctrl+Z | Fail for safe recovery: immediate delete and no structural undo | [After undo attempt](34-delete-undo-view.png) |
| Resize to compact desktop windows | Fail for containment/reachability of some controls | [1024](24-checkpoints-1024-view.png), [800](26-notebook-800-view.png) |
| Render pandas table and PNG chart | Pass: both are visible | [Rich output](42-rich-output-finished-view.png) |
| Stop runtime and retain offline UI | Partial: shutdown succeeded, but final UI showed a transport error and stale operation state | [Stop failure state](43-final-offline-view.png), [Follow-up](46-final-review-view.png), [CLI status](final-runtime-status.json) |
| Native accessibility inspection | Partial: named controls exist; navigation/output missing in captured tree; window title empty | [UIA tree](accessibility-tree.json) |

## Prioritized findings

P1 = serious task, safety, reachability or accessibility issue; address before broad release. P2 = substantial usability improvement; schedule next. P3 = polish/terminology. No observed P0 runtime data-loss/crash incident is claimed. Source deletion without immediate undo is a separate P1 safety concern.

### F01 · P1 · Run shortcuts are swallowed by the editor

**Observed:** Focus code editor; press Shift+Enter or Ctrl+Enter. A newline is inserted. Escape, then Shift+Enter runs.

**Evidence:** [03-edit-keyboard-run](03-edit-keyboard-run-view.png).

**Change:** Route notebook run bindings before editor newline handlers; keep focus behavior explicit and consistent.

**Acceptance:** Both shortcuts execute once from editor and command mode without changing source; define Shift+Enter run-next behavior.

### F02 · P1 · Command palette cannot activate results with keys

**Observed:** Open palette, type checkpoint, press Arrow Down and Enter. The palette remains and no operation starts.

**Evidence:** [21-palette-keyboard](21-palette-keyboard-view.png).

**Change:** Maintain active result index, support arrows/Enter, visible selection and focus restoration.

**Acceptance:** Open, filter, select and execute each command entirely by keyboard; Escape dismisses.

### F03 · P1 · Cell deletion has no immediate recovery

**Observed:** Delete a saved cell. It disappears immediately. Ctrl+Z does not bring it back.

**Evidence:** [34-delete-undo](34-delete-undo-view.png).

**Change:** Add structural undo and an Undo notification; use a clear confirmation until reliable undo exists.

**Acceptance:** Undo restores the exact cell source, kind, position and identity; failed delete preserves source.

### F04 · P1 · Important controls clip in normal and compact windows

**Observed:** Open Checkpoints at 1280, 1024 and 800 logical pixels. Restore/status rows clip; small notebook windows clip add-cell/runtime controls.

**Evidence:** [24-checkpoints-1024](24-checkpoints-1024-view.png).

**Change:** Make inspection rows shrink/wrap; add toolbar overflow and a collapsible outline; set and communicate minimum supported dimensions.

**Acceptance:** All primary actions and status are reachable at documented minimum size in both themes.

### F05 · P1 · Critical dialog text overflows and uses generic wording

**Observed:** Open Restore or Stop confirmation; description extends outside the dialog. Stop warns about running saved code and shows {}.

**Evidence:** [37-stop-confirm](37-stop-confirm-view.png).

**Change:** Wrap description; use action-specific consequences and button names; show a useful target summary.

**Acceptance:** Every confirmation stays contained at minimum size; Stop says Stop runtime and Restore identifies its checkpoint.

### F06 · P1 · Native accessibility identity is incomplete

**Observed:** UI Automation shows an unnamed app window and named controls, but no six navigation destinations or output semantics in the captured tree.

**Evidence:** [accessibility-tree.json](accessibility-tree.json).

**Change:** Expose native navigation roles/names/current state, contextual control names and accessible output content; set app/notebook window title.

**Acceptance:** NVDA can identify the window, navigate all destinations, read source/output/status and operate dialogs without a mouse.

### F13 · P1 · Normal runtime stop is displayed as a connection failure

**Observed:** After rich-output execution, Stop shut down the runtime, confirmed by CLI status, but UI showed Disconnected and raw OS error 10053. Reconnect/reload did not clear that state in the follow-up capture; a running operation label remained.

**Evidence:** [43-final-offline](43-final-offline-view.png).

**Change:** Handle intentional shutdown as an expected lifecycle event; reconcile operation completion and refresh connection state; distinguish runtime stopped from transport failure.

**Acceptance:** Repeated Stop from idle/busy states gives a stopped/offline state without an OS error; history stays readable and no completed operation remains running.

### F07 · P2 · Recorded work is presented as identifiers and JSON

**Observed:** History and Runtimes list UUIDs; execution detail and doctor results are raw JSON.

**Evidence:** [09-executions](09-executions-view.png).

**Change:** Show cell name/source preview, local timestamp, duration, interpreter/path and status; place raw JSON in an optional details disclosure.

**Acceptance:** A user can locate a failed cell or current runtime without reading UUIDs/JSON.

### F08 · P2 · Settings inherits unrelated diagnostic state

**Observed:** Scroll Doctor results down, switch to Settings. Old scroll position and doctor JSON remain.

**Evidence:** [18-settings-dark](18-settings-dark-view.png).

**Change:** Scope inspection data and scroll position per destination; reset new-view scroll or restore only its own prior state.

**Acceptance:** Settings opens on preferences and does not display results from Doctor or another destination.

### F09 · P2 · Adding/selecting a cell does not reliably reveal its editor

**Observed:** Add code cell at bottom; existing selection and viewport remain. Select new outline entry; editor still requires extra scrolling.

**Evidence:** [29-added-selected](29-added-selected-view.png).

**Change:** Select, reveal and focus newly added cell after mutation acknowledgement; scroll using measured card bounds.

**Acceptance:** One add action yields a visible focused editor without extra navigation, including after large output.

### F10 · P2 · Theme preference resets on relaunch

**Observed:** Switch to dark, close and relaunch. Startup returns to light; source hardcodes initial light theme.

**Evidence:** [39-startup](39-startup-view.png).

**Change:** Persist appearance selection and offer Follow system with accessible high-contrast support.

**Acceptance:** Relaunch restores selection and system mode follows OS preference.

### F11 · P2 · First use assumes an initialized notebook directory

**Observed:** Launch without a path. Only Open notebook is offered.

**Evidence:** [39-startup](39-startup-view.png).

**Change:** Offer Create notebook, recent notebooks and a small sample; explain invalid/uninitialized directory recovery.

**Acceptance:** A new user can create, edit, run and save a first notebook through the UI.

### F12 · P3 · Session and worker states can look contradictory

**Observed:** History labels the open session running while runtime chip reads Idle.

**Evidence:** [08-history](08-history-view.png).

**Change:** Use Active session for session lifecycle and reserve Running for computation; add a short state explanation.

**Acceptance:** Idle worker plus active session has clearly distinct labels and contextual help.

## Detailed rubric: all 64 criteria

### Visual hierarchy and consistency · 8 points

| Criterion | Score | Evidence and rationale |
| --- | ---: | --- |
| Hierarchy, grouping and primary actions | 4 | Clear app header, navigation rail, outline, cell cards and primary Run all action. [01-notebook-initial](01-notebook-initial-view.png) |
| Typography, spacing and alignment | 4 | Readable code and comfortable card spacing; secondary labels are small. [27-notebook-reset](27-notebook-reset-view.png) |
| Theme and component consistency | 4 | Light and dark themes share a coherent component vocabulary. [19-settings-top](19-settings-top-view.png) |
| Information density and restraint | 3 | Notebook is balanced; repeated operation records and JSON clutter supporting screens. [08-history](08-history-view.png) |

### Navigation and information architecture · 7 points

| Criterion | Score | Evidence and rationale |
| --- | ---: | --- |
| Primary destinations and current location | 4 | Six clearly labeled destinations and visible active selection. [15-diagnostics](15-diagnostics-view.png) |
| Cell outline, selection and filtering | 3 | Filtering and no-match feedback work; filtering affects only the outline and selection can remain offscreen. [22-filter-empty](22-filter-empty-view.png) |
| History drill-down and return path | 2 | Sessions to executions to detail works, but no visible breadcrumb or dedicated back action. [10-execution-detail](10-execution-detail-view.png) |
| Context isolation across screens | 2 | Settings retains doctor JSON and a prior panel's scroll position; source confirms one shared inspection value. [18-settings-dark](18-settings-dark-view.png) |

### Onboarding and first use · 5 points

| Criterion | Score | Evidence and rationale |
| --- | ---: | --- |
| Initial value proposition and first action | 4 | Clear welcome text, Open notebook CTA and statement that opening starts no worker. [39-startup](39-startup-view.png) |
| Create-notebook journey | 2 | Startup only offers opening a directory; creating a notebook requires CLI knowledge. [39-startup](39-startup-view.png) |
| Recent work and example discoverability | 2 | No visible recent notebook list, guided example or sample notebook action. [39-startup](39-startup-view.png) |
| Shortcut introduction and mental model | 3 | Shortcut list is helpful, but run shortcut behavior depends on focus in ways the list does not explain. [39-startup](39-startup-view.png) |

### Editing and cell authoring · 10 points

| Criterion | Score | Evidence and rationale |
| --- | ---: | --- |
| Code readability and editing affordances | 4 | Python syntax highlighting, line numbers and labeled cell editors are present. [40-rich-before](40-rich-before-view.png) |
| Persistence and stale-source awareness | 4 | An edit was saved to cells/sales.py; stale output badge and outline marker appeared. [03-edit-keyboard-run](03-edit-keyboard-run-view.png) |
| Add-cell focus and placement | 2 | Adding succeeds but keeps the old viewport and selection rather than placing the new editor in view. [28-add-cell](28-add-cell-view.png) |
| Structural editing and undo | 2 | Add/delete work; delete is immediate and Ctrl+Z did not restore the cell. Structural undo is documented as unfinished. [34-delete-undo](34-delete-undo-view.png) |

### Execution and runtime control · 9 points

| Criterion | Score | Evidence and rationale |
| --- | ---: | --- |
| Single-cell and batch execution | 4 | Mouse run and Run all executed code and retained output; batch error was visible. [06-run-all-error](06-run-all-error-view.png) |
| Running state and interruption | 4 | A 30-second sleep showed Running, then Interrupted and Idle after Interrupt. [32-interrupt](32-interrupt-view.png) |
| Runtime lifecycle and offline inspection | 2 | Open was offline and first execution started a worker. Final Stop shut down the runtime but left a Disconnected chip and OS error 10053; reopening is needed for a clean offline state. [43-final-offline](43-final-offline-view.png) |
| Capability gating and action scope | 2 | Global execution controls remain on inspection/settings screens; idle interrupt and destructive controls have little state-specific guidance. [15-diagnostics](15-diagnostics-view.png) |

### Feedback and state communication · 6 points

| Criterion | Score | Evidence and rationale |
| --- | ---: | --- |
| Execution result and global status | 4 | Runtime chip, cell status and execution ordinal make completion and running state visible. [31-running](31-running-view.png) |
| Save, stale and empty-result feedback | 4 | Stale output, outline markers and No cells match provide useful feedback. [22-filter-empty](22-filter-empty-view.png) |
| Operation identity and progress meaning | 1 | Operations show UUID/state without task names or timestamps; after Stop, a running operation label remained despite runtime shutdown. [46-final-review](46-final-review-view.png) |
| Session versus worker state terminology | 2 | History session says running while the worker chip says Idle; this distinction is not explained. [08-history](08-history-view.png) |

### Output readability and scientific content · 6 points

| Criterion | Score | Evidence and rationale |
| --- | ---: | --- |
| Text, expressions and Python errors | 4 | Text/result separation and a prominent traceback preserve useful detail. [30-bottom-cell](30-bottom-cell-view.png) |
| Tables and image rendering | 4 | A four-row pandas table and matplotlib PNG rendered correctly in the native UI. [42-rich-output-finished](42-rich-output-finished-view.png) |
| Markdown and source/output relationship | 3 | Markdown renders with emphasis and headings, but source and preview occupy space simultaneously. [01-notebook-initial](01-notebook-initial-view.png) |
| Output tooling and rich-content coverage | 1 | MIME chooser/export, output search, full attachment preview and bounded large-output paging remain unfinished; not all MIME types were exercised. [docs/desktop/coverage.md](../../docs/desktop/coverage.md) |

### History and provenance usability · 6 points

| Criterion | Score | Evidence and rationale |
| --- | ---: | --- |
| Human-readable record summaries | 2 | Execution rows primarily contain UUIDs; cell names, time and duration are missing from the list. [09-executions](09-executions-view.png) |
| Detail presentation and source comparison | 2 | Data is available as raw JSON; source comparison and a formatted execution detail view are missing. [10-execution-detail](10-execution-detail-view.png) |
| Find, filter and sort recorded work | 2 | No visible history search or time/status filtering; the left search is cell filtering. [08-history](08-history-view.png) |
| Runtime identity and actionable discovery | 1 | Runtime rows show IDs and verified badges rather than useful path/interpreter/activity summaries. [14-runtimes](14-runtimes-view.png) |

### Checkpoints, diagnostics and recovery · 7 points

| Criterion | Score | Evidence and rationale |
| --- | ---: | --- |
| Checkpoint creation and restoration | 4 | A complete checkpoint was created and restored; ledger records a new session linked to it. [12-restore-confirm](12-restore-confirm-view.png) |
| Diagnostics and reconnect tools | 2 | Doctor/reconnect tools exist but use raw JSON. Reconnect/reload was clicked after Stop; the Disconnected chip and error remained in the follow-up capture, although later history inspection worked. [45-reconnected](45-reconnected-view.png) |
| Recovery guidance and preflight clarity | 2 | Trust/mismatch options and warning exist; no concise summary of restored bindings or typed recovery guidance. [12-restore-confirm](12-restore-confirm-view.png) |
| Crash, conflict and draft recovery | NT | Draft/conflict/recovery features are documented but crashes, simultaneous edits and draft replay were not induced. Stop-related disconnect behavior is rated in the lifecycle/diagnostics criteria. [docs/desktop/coverage.md](../../docs/desktop/coverage.md) |

### Safety, prevention and user control · 7 points

| Criterion | Score | Evidence and rationale |
| --- | ---: | --- |
| Deletion protection and recovery | 1 | One click removes the source cell from the notebook immediately; no confirmation, Undo action or working structural Ctrl+Z observed. This does not claim execution history or all possible CLI recovery paths are erased. [34-delete-undo](34-delete-undo-view.png) |
| Namespace-changing action confirmation | 3 | Restore and Stop ask for confirmation; Escape dismisses and trust/mismatch are explicit. [12-restore-confirm](12-restore-confirm-view.png) |
| Action-specific warning and button copy | 2 | Stop reuses a warning about executing saved code, target is {}, and the action label is generic Proceed. [37-stop-confirm](37-stop-confirm-view.png) |
| Trust, attachment and close semantics | 3 | Trust options and closing-detaches message are visible. This is a UX assessment, not a security audit. [43-final-offline](43-final-offline-view.png) |

### Keyboard efficiency and focus · 7 points

| Criterion | Score | Evidence and rationale |
| --- | ---: | --- |
| Run shortcuts from editor focus | 1 | Shift+Enter and Ctrl+Enter inserted newlines in editor focus; Escape followed by Shift+Enter executed the selected cell. [04-ctrl-enter](04-ctrl-enter-view.png) |
| Command-palette keyboard activation | 1 | Filtering checkpoint worked, but Arrow Down + Enter did not select or execute it; implementation has click handlers only. [21-palette-keyboard](21-palette-keyboard-view.png) |
| Escape and mode visibility | 3 | Escape dismisses confirmations and returns to command mode; status bar distinguishes edit/command mode. [13-dialog-escape](13-dialog-escape-view.png) |
| Complete tab journey and modal focus containment | NT | Focusable controls exist, but a full Tab/Shift+Tab journey, focus restoration and modal trap were not tested. [accessibility-tree.json](accessibility-tree.json) |

### Accessibility and inclusive interaction · 7 points

| Criterion | Score | Evidence and rationale |
| --- | ---: | --- |
| Native semantics and accessible labels | 3 | UI Automation exposes 29 descendants including named buttons/inputs/editors. Navigation destinations and rendered outputs were absent from this snapshot. [accessibility-tree.json](accessibility-tree.json) |
| Window and contextual accessible identity | 1 | Native window has an empty title/name and repeated Run/Delete labels lack a cell name in the exposed tree. [accessibility-tree.json](accessibility-tree.json) |
| Measured text, nontext and focus contrast | NT | Visual inspection alone cannot certify contrast. No pixel/token contrast calculation was performed. [19-settings-top](19-settings-top-view.png) |
| Screen reader, high contrast, IME and text scaling | NT | NVDA, system high contrast, IME, mixed DPI and enlarged text remain qualification work; no conformance claim. [docs/desktop/platforms.md](../../docs/desktop/platforms.md) |

### Adaptive layout and window sizing · 6 points

| Criterion | Score | Evidence and rationale |
| --- | ---: | --- |
| Standard-width containment | 2 | Notebook is usable at about 1280 logical pixels, but checkpoint row actions already clip on the right. [11-checkpoints](11-checkpoints-view.png) |
| Compact-window controls | 1 | At 800 logical pixels, runtime state and cell-add actions are clipped; no adaptive toolbar overflow was visible. [26-notebook-800](26-notebook-800-view.png) |
| Dialog wrapping and inspection reflow | 1 | Confirmation description paints past the right edge; checkpoint actions clip at 1024 and 800. [12-restore-confirm](12-restore-confirm-view.png) |
| Mixed DPI, secondary monitor and extreme sizing | NT | Observed on one Windows display at 200% scaling; other DPI configurations and monitors were not tested. [docs/desktop/platforms.md](../../docs/desktop/platforms.md) |

### Perceived performance and scale · 4 points

| Criterion | Score | Evidence and rationale |
| --- | ---: | --- |
| Small-notebook responsiveness | 4 | Observed interactions completed and app stayed responsive while code ran; no app crash during completed workflows. [42-rich-output-finished](42-rich-output-finished-view.png) |
| Measured startup and interaction latency | NT | No repeatable timing harness was run. Screenshot-tool time is not an application latency measurement. [docs/desktop/platforms.md](../../docs/desktop/platforms.md) |
| Large notebooks, history and output | NT | Six-cell fixture only; no large history, 100 MiB output, image-cache or long-session stress test. [docs/desktop/coverage.md](../../docs/desktop/coverage.md) |
| Resource use and rendering stability | NT | Memory, frame times, GPU fallback and long-running resource behavior were not measured. [docs/desktop/platforms.md](../../docs/desktop/platforms.md) |

### Preferences and personalization · 3 points

| Criterion | Score | Evidence and rationale |
| --- | ---: | --- |
| Theme discoverability and operation | 4 | Header and Settings theme switches work. [19-settings-top](19-settings-top-view.png) |
| Interpreter clarity | 2 | Choose project interpreter exists, but current selected path/version is not summarized in Settings; picker was not exercised. [19-settings-top](19-settings-top-view.png) |
| Preference persistence and system integration | 1 | Dark theme returned to light on relaunch; source initializes apply_theme(false) and dark:false. [39-startup](39-startup-view.png) |
| Customization depth | 1 | System/high-contrast preference, key rebinding and full config fields remain unfinished. [docs/desktop/coverage.md](../../docs/desktop/coverage.md) |

### Help, terminology and platform polish · 2 points

| Criterion | Score | Evidence and rationale |
| --- | ---: | --- |
| Contextual help and documentation access | 2 | Startup shortcuts and tooltips help, but no visible in-app help, glossary or documentation entry point. [39-startup](39-startup-view.png) |
| Language and domain concepts | 2 | Ordinary notebook labels are clear; namespace, runtime, interpreter and Continue/Restore need explanation. [37-stop-confirm](37-stop-confirm-view.png) |
| Native shell identity and release information | 1 | Window title is empty and version/About is not visible. Installer, signing and update UX were not tested. [accessibility-tree.json](accessibility-tree.json) |
| Discovery of incomplete workflows | 2 | Coverage is honestly documented in the repo, but UI offers little guidance about unimplemented exchange/publish workflows. [docs/desktop/coverage.md](../../docs/desktop/coverage.md) |

## Remaining qualification matrix

| Area | Verification to perform before assigning a final score |
| --- | --- |
| Keyboard/focus | Full mouse-free open/edit/run/history/checkpoint journey; Tab order; focus trap and return; focus visibility in both themes |
| Screen reader | NVDA reads window, navigation, selected cell, code, outputs, execution state, input prompts and errors; repeat with native UIA tooling |
| Visual accessibility | Measure normal/large text, control/focus contrast; check target sizes/spacing, color-independent meaning, high contrast and larger fonts |
| Editor | Undo/redo after autosave and run; large cells; clipboard; find/replace; IME; selection retention during virtualization |
| Recovery | Concurrent external edit; save conflict; helper/worker crash; reconnect; retained drafts; interrupted batch; reopen unsaved work |
| Runtime/input | input() ownership and handoff, getpass behavior, interpreter picker validation, long queue, run-above/below ordering, terminate/restart confirmations |
| Rich content | JPEG, JSON/Markdown MIME, unsupported MIME fallback, image failures, wide/large tables, HTML safety, LaTeX fallback, output hide/show and persistence |
| Scale/performance | Measured cold/warm startup; typing/scroll latency; hundreds/thousands of cells; large history; 100 MiB output; long-session memory/GPU use |
| Platform | 100/125/150/200% DPI; mixed monitors; GPU/driver variants; clean machine; signed installer/update/uninstall; Linux qualification where supported |
| Product completion | Create/import/export, recent notebooks, keyboard guide/help, source comparison, publish/pin, multi-notebook and detached-window journeys |
| Human validation | Observe novice and experienced notebook users; measure task success, errors, time and satisfaction rather than inferring them from expert review |

## Recommended implementation order

1. Fix editor shortcuts and keyboard palette activation; add structural undo and contextual accessible names/navigation.
2. Fix dialog wrapping/copy, row shrink/wrap behavior and compact toolbar overflow.
3. Replace UUID-first record views with meaningful summaries; separate screen data/scroll state; focus new cells.
4. Persist preferences, add create/recent/help flows, then complete accessibility/recovery/performance qualification.

## Reuse and artifacts

- `scorecard.html`: standalone interactive scorecard; category filtering, score editing, evidence links, and JSON export. Edits are local to the page until exported.
- `scorecard.json` and `scorecard.csv`: machine-readable category weights, criterion ratings and evidence.
- `build_scorecard.py`: reproducibly generates all reports from the embedded audit data.
- `computer.py`: native audit capture/input helper, restricted to the app PID stored in `app.pid`. Coordinates are logical half-resolution coordinates for this 200% display; text/key delivery requires app focus.
- Numbered PNG files are evidence; `-view.png` versions are downsampled for easier reading. UI Automation and runtime snapshots are stored separately.
- Re-score fixed criteria using the same fixture and rubric. Preserve original evidence and record build/environment changes; add measurements for NT criteria rather than converting them to arbitrary estimates.

No product source files were changed. The audit fixture and artifacts are the only workspace additions. This repository snapshot has no `.git` directory, so no commit or PR was created.

## Artifact verification

The generated JSON/CSV/report contain 16 categories with weights totaling 100 and 64 criteria. All evidence links were checked for existing files. The interactive scorecard showed 51.7/100 and 88.5% coverage; changing a criterion updated the score, Reset restored it, and category filtering showed the selected category. No horizontal document overflow was found at 1280-pixel desktop or 390-pixel compact browser widths. These browser checks apply to the report artifact, not to the native app.
