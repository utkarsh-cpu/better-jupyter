"""Rebuild the audit report, CSV, JSON and standalone interactive scorecard."""
import csv
import html
import json
from pathlib import Path

ROOT = Path(__file__).parent
CATEGORIES = []


def category(name, weight, criteria):
    CATEGORIES.append({"name": name, "weight": weight, "criteria": [
        {"name": n, "score": s, "evidence": e, "note": note}
        for n, s, e, note in criteria
    ]})


category("Visual hierarchy and consistency", 8, [
    ("Hierarchy, grouping and primary actions", 4, "01-notebook-initial", "Clear app header, navigation rail, outline, cell cards and primary Run all action."),
    ("Typography, spacing and alignment", 4, "27-notebook-reset", "Readable code and comfortable card spacing; secondary labels are small."),
    ("Theme and component consistency", 4, "19-settings-top", "Light and dark themes share a coherent component vocabulary."),
    ("Information density and restraint", 3, "08-history", "Notebook is balanced; repeated operation records and JSON clutter supporting screens."),
])
category("Navigation and information architecture", 7, [
    ("Primary destinations and current location", 4, "15-diagnostics", "Six clearly labeled destinations and visible active selection."),
    ("Cell outline, selection and filtering", 3, "22-filter-empty", "Filtering and no-match feedback work; filtering affects only the outline and selection can remain offscreen."),
    ("History drill-down and return path", 2, "10-execution-detail", "Sessions to executions to detail works, but no visible breadcrumb or dedicated back action."),
    ("Context isolation across screens", 2, "18-settings-dark", "Settings retains doctor JSON and a prior panel's scroll position; source confirms one shared inspection value."),
])
category("Onboarding and first use", 5, [
    ("Initial value proposition and first action", 4, "39-startup", "Clear welcome text, Open notebook CTA and statement that opening starts no worker."),
    ("Create-notebook journey", 2, "39-startup", "Startup only offers opening a directory; creating a notebook requires CLI knowledge."),
    ("Recent work and example discoverability", 2, "39-startup", "No visible recent notebook list, guided example or sample notebook action."),
    ("Shortcut introduction and mental model", 3, "39-startup", "Shortcut list is helpful, but run shortcut behavior depends on focus in ways the list does not explain."),
])
category("Editing and cell authoring", 10, [
    ("Code readability and editing affordances", 4, "40-rich-before", "Python syntax highlighting, line numbers and labeled cell editors are present."),
    ("Persistence and stale-source awareness", 4, "03-edit-keyboard-run", "An edit was saved to cells/sales.py; stale output badge and outline marker appeared."),
    ("Add-cell focus and placement", 2, "28-add-cell", "Adding succeeds but keeps the old viewport and selection rather than placing the new editor in view."),
    ("Structural editing and undo", 2, "34-delete-undo", "Add/delete work; delete is immediate and Ctrl+Z did not restore the cell. Structural undo is documented as unfinished."),
])
category("Execution and runtime control", 9, [
    ("Single-cell and batch execution", 4, "06-run-all-error", "Mouse run and Run all executed code and retained output; batch error was visible."),
    ("Running state and interruption", 4, "32-interrupt", "A 30-second sleep showed Running, then Interrupted and Idle after Interrupt."),
    ("Runtime lifecycle and offline inspection", 2, "43-final-offline", "Open was offline and first execution started a worker. Final Stop shut down the runtime but left a Disconnected chip and OS error 10053; reopening is needed for a clean offline state."),
    ("Capability gating and action scope", 2, "15-diagnostics", "Global execution controls remain on inspection/settings screens; idle interrupt and destructive controls have little state-specific guidance."),
])
category("Feedback and state communication", 6, [
    ("Execution result and global status", 4, "31-running", "Runtime chip, cell status and execution ordinal make completion and running state visible."),
    ("Save, stale and empty-result feedback", 4, "22-filter-empty", "Stale output, outline markers and No cells match provide useful feedback."),
    ("Operation identity and progress meaning", 1, "46-final-review", "Operations show UUID/state without task names or timestamps; after Stop, a running operation label remained despite runtime shutdown."),
    ("Session versus worker state terminology", 2, "08-history", "History session says running while the worker chip says Idle; this distinction is not explained."),
])
category("Output readability and scientific content", 6, [
    ("Text, expressions and Python errors", 4, "30-bottom-cell", "Text/result separation and a prominent traceback preserve useful detail."),
    ("Tables and image rendering", 4, "42-rich-output-finished", "A four-row pandas table and matplotlib PNG rendered correctly in the native UI."),
    ("Markdown and source/output relationship", 3, "01-notebook-initial", "Markdown renders with emphasis and headings, but source and preview occupy space simultaneously."),
    ("Output tooling and rich-content coverage", 1, "docs/desktop/coverage.md", "MIME chooser/export, output search, full attachment preview and bounded large-output paging remain unfinished; not all MIME types were exercised."),
])
category("History and provenance usability", 6, [
    ("Human-readable record summaries", 2, "09-executions", "Execution rows primarily contain UUIDs; cell names, time and duration are missing from the list."),
    ("Detail presentation and source comparison", 2, "10-execution-detail", "Data is available as raw JSON; source comparison and a formatted execution detail view are missing."),
    ("Find, filter and sort recorded work", 2, "08-history", "No visible history search or time/status filtering; the left search is cell filtering."),
    ("Runtime identity and actionable discovery", 1, "14-runtimes", "Runtime rows show IDs and verified badges rather than useful path/interpreter/activity summaries."),
])
category("Checkpoints, diagnostics and recovery", 7, [
    ("Checkpoint creation and restoration", 4, "12-restore-confirm", "A complete checkpoint was created and restored; ledger records a new session linked to it."),
    ("Diagnostics and reconnect tools", 2, "45-reconnected", "Doctor/reconnect tools exist but use raw JSON. Reconnect/reload was clicked after Stop; the Disconnected chip and error remained in the follow-up capture, although later history inspection worked."),
    ("Recovery guidance and preflight clarity", 2, "12-restore-confirm", "Trust/mismatch options and warning exist; no concise summary of restored bindings or typed recovery guidance."),
    ("Crash, conflict and draft recovery", None, "docs/desktop/coverage.md", "Draft/conflict/recovery features are documented but crashes, simultaneous edits and draft replay were not induced. Stop-related disconnect behavior is rated in the lifecycle/diagnostics criteria."),
])
category("Safety, prevention and user control", 7, [
    ("Deletion protection and recovery", 1, "34-delete-undo", "One click removes the source cell from the notebook immediately; no confirmation, Undo action or working structural Ctrl+Z observed. This does not claim execution history or all possible CLI recovery paths are erased."),
    ("Namespace-changing action confirmation", 3, "12-restore-confirm", "Restore and Stop ask for confirmation; Escape dismisses and trust/mismatch are explicit."),
    ("Action-specific warning and button copy", 2, "37-stop-confirm", "Stop reuses a warning about executing saved code, target is {}, and the action label is generic Proceed."),
    ("Trust, attachment and close semantics", 3, "43-final-offline", "Trust options and closing-detaches message are visible. This is a UX assessment, not a security audit."),
])
category("Keyboard efficiency and focus", 7, [
    ("Run shortcuts from editor focus", 1, "04-ctrl-enter", "Shift+Enter and Ctrl+Enter inserted newlines in editor focus; Escape followed by Shift+Enter executed the selected cell."),
    ("Command-palette keyboard activation", 1, "21-palette-keyboard", "Filtering checkpoint worked, but Arrow Down + Enter did not select or execute it; implementation has click handlers only."),
    ("Escape and mode visibility", 3, "13-dialog-escape", "Escape dismisses confirmations and returns to command mode; status bar distinguishes edit/command mode."),
    ("Complete tab journey and modal focus containment", None, "accessibility-tree.json", "Focusable controls exist, but a full Tab/Shift+Tab journey, focus restoration and modal trap were not tested."),
])
category("Accessibility and inclusive interaction", 7, [
    ("Native semantics and accessible labels", 3, "accessibility-tree.json", "UI Automation exposes 29 descendants including named buttons/inputs/editors. Navigation destinations and rendered outputs were absent from this snapshot."),
    ("Window and contextual accessible identity", 1, "accessibility-tree.json", "Native window has an empty title/name and repeated Run/Delete labels lack a cell name in the exposed tree."),
    ("Measured text, nontext and focus contrast", None, "19-settings-top", "Visual inspection alone cannot certify contrast. No pixel/token contrast calculation was performed."),
    ("Screen reader, high contrast, IME and text scaling", None, "docs/desktop/platforms.md", "NVDA, system high contrast, IME, mixed DPI and enlarged text remain qualification work; no conformance claim."),
])
category("Adaptive layout and window sizing", 6, [
    ("Standard-width containment", 2, "11-checkpoints", "Notebook is usable at about 1280 logical pixels, but checkpoint row actions already clip on the right."),
    ("Compact-window controls", 1, "26-notebook-800", "At 800 logical pixels, runtime state and cell-add actions are clipped; no adaptive toolbar overflow was visible."),
    ("Dialog wrapping and inspection reflow", 1, "12-restore-confirm", "Confirmation description paints past the right edge; checkpoint actions clip at 1024 and 800."),
    ("Mixed DPI, secondary monitor and extreme sizing", None, "docs/desktop/platforms.md", "Observed on one Windows display at 200% scaling; other DPI configurations and monitors were not tested."),
])
category("Perceived performance and scale", 4, [
    ("Small-notebook responsiveness", 4, "42-rich-output-finished", "Observed interactions completed and app stayed responsive while code ran; no app crash during completed workflows."),
    ("Measured startup and interaction latency", None, "docs/desktop/platforms.md", "No repeatable timing harness was run. Screenshot-tool time is not an application latency measurement."),
    ("Large notebooks, history and output", None, "docs/desktop/coverage.md", "Six-cell fixture only; no large history, 100 MiB output, image-cache or long-session stress test."),
    ("Resource use and rendering stability", None, "docs/desktop/platforms.md", "Memory, frame times, GPU fallback and long-running resource behavior were not measured."),
])
category("Preferences and personalization", 3, [
    ("Theme discoverability and operation", 4, "19-settings-top", "Header and Settings theme switches work."),
    ("Interpreter clarity", 2, "19-settings-top", "Choose project interpreter exists, but current selected path/version is not summarized in Settings; picker was not exercised."),
    ("Preference persistence and system integration", 1, "39-startup", "Dark theme returned to light on relaunch; source initializes apply_theme(false) and dark:false."),
    ("Customization depth", 1, "docs/desktop/coverage.md", "System/high-contrast preference, key rebinding and full config fields remain unfinished."),
])
category("Help, terminology and platform polish", 2, [
    ("Contextual help and documentation access", 2, "39-startup", "Startup shortcuts and tooltips help, but no visible in-app help, glossary or documentation entry point."),
    ("Language and domain concepts", 2, "37-stop-confirm", "Ordinary notebook labels are clear; namespace, runtime, interpreter and Continue/Restore need explanation."),
    ("Native shell identity and release information", 1, "accessibility-tree.json", "Window title is empty and version/About is not visible. Installer, signing and update UX were not tested."),
    ("Discovery of incomplete workflows", 2, "docs/desktop/coverage.md", "Coverage is honestly documented in the repo, but UI offers little guidance about unimplemented exchange/publish workflows."),
])

FINDINGS = [
    ("F01", "P1", "Run shortcuts are swallowed by the editor", "03-edit-keyboard-run", "Focus code editor; press Shift+Enter or Ctrl+Enter. A newline is inserted. Escape, then Shift+Enter runs.", "Route notebook run bindings before editor newline handlers; keep focus behavior explicit and consistent.", "Both shortcuts execute once from editor and command mode without changing source; define Shift+Enter run-next behavior."),
    ("F02", "P1", "Command palette cannot activate results with keys", "21-palette-keyboard", "Open palette, type checkpoint, press Arrow Down and Enter. The palette remains and no operation starts.", "Maintain active result index, support arrows/Enter, visible selection and focus restoration.", "Open, filter, select and execute each command entirely by keyboard; Escape dismisses."),
    ("F03", "P1", "Cell deletion has no immediate recovery", "34-delete-undo", "Delete a saved cell. It disappears immediately. Ctrl+Z does not bring it back.", "Add structural undo and an Undo notification; use a clear confirmation until reliable undo exists.", "Undo restores the exact cell source, kind, position and identity; failed delete preserves source."),
    ("F04", "P1", "Important controls clip in normal and compact windows", "24-checkpoints-1024", "Open Checkpoints at 1280, 1024 and 800 logical pixels. Restore/status rows clip; small notebook windows clip add-cell/runtime controls.", "Make inspection rows shrink/wrap; add toolbar overflow and a collapsible outline; set and communicate minimum supported dimensions.", "All primary actions and status are reachable at documented minimum size in both themes."),
    ("F05", "P1", "Critical dialog text overflows and uses generic wording", "37-stop-confirm", "Open Restore or Stop confirmation; description extends outside the dialog. Stop warns about running saved code and shows {}.", "Wrap description; use action-specific consequences and button names; show a useful target summary.", "Every confirmation stays contained at minimum size; Stop says Stop runtime and Restore identifies its checkpoint."),
    ("F06", "P1", "Native accessibility identity is incomplete", "accessibility-tree.json", "UI Automation shows an unnamed app window and named controls, but no six navigation destinations or output semantics in the captured tree.", "Expose native navigation roles/names/current state, contextual control names and accessible output content; set app/notebook window title.", "NVDA can identify the window, navigate all destinations, read source/output/status and operate dialogs without a mouse."),
    ("F07", "P2", "Recorded work is presented as identifiers and JSON", "09-executions", "History and Runtimes list UUIDs; execution detail and doctor results are raw JSON.", "Show cell name/source preview, local timestamp, duration, interpreter/path and status; place raw JSON in an optional details disclosure.", "A user can locate a failed cell or current runtime without reading UUIDs/JSON."),
    ("F08", "P2", "Settings inherits unrelated diagnostic state", "18-settings-dark", "Scroll Doctor results down, switch to Settings. Old scroll position and doctor JSON remain.", "Scope inspection data and scroll position per destination; reset new-view scroll or restore only its own prior state.", "Settings opens on preferences and does not display results from Doctor or another destination."),
    ("F09", "P2", "Adding/selecting a cell does not reliably reveal its editor", "29-added-selected", "Add code cell at bottom; existing selection and viewport remain. Select new outline entry; editor still requires extra scrolling.", "Select, reveal and focus newly added cell after mutation acknowledgement; scroll using measured card bounds.", "One add action yields a visible focused editor without extra navigation, including after large output."),
    ("F10", "P2", "Theme preference resets on relaunch", "39-startup", "Switch to dark, close and relaunch. Startup returns to light; source hardcodes initial light theme.", "Persist appearance selection and offer Follow system with accessible high-contrast support.", "Relaunch restores selection and system mode follows OS preference."),
    ("F11", "P2", "First use assumes an initialized notebook directory", "39-startup", "Launch without a path. Only Open notebook is offered.", "Offer Create notebook, recent notebooks and a small sample; explain invalid/uninitialized directory recovery.", "A new user can create, edit, run and save a first notebook through the UI."),
    ("F12", "P3", "Session and worker states can look contradictory", "08-history", "History labels the open session running while runtime chip reads Idle.", "Use Active session for session lifecycle and reserve Running for computation; add a short state explanation.", "Idle worker plus active session has clearly distinct labels and contextual help."),
    ("F13", "P1", "Normal runtime stop is displayed as a connection failure", "43-final-offline", "After rich-output execution, Stop shut down the runtime, confirmed by CLI status, but UI showed Disconnected and raw OS error 10053. Reconnect/reload did not clear that state in the follow-up capture; a running operation label remained.", "Handle intentional shutdown as an expected lifecycle event; reconcile operation completion and refresh connection state; distinguish runtime stopped from transport failure.", "Repeated Stop from idle/busy states gives a stopped/offline state without an OS error; history stays readable and no completed operation remains running."),
]

FINDINGS.sort(key=lambda f: (f[1], f[0]))

for cat in CATEGORIES:
    rated = [c["score"] for c in cat["criteria"] if c["score"] is not None]
    cat["rating"] = sum(rated) / len(rated) if rated else None
    cat["coverage"] = len(rated) / len(cat["criteria"])
    cat["points"] = cat["weight"] * cat["rating"] / 5 if rated else None
overall = sum(c["points"] or 0 for c in CATEGORIES)
coverage = sum(c["weight"] * c["coverage"] for c in CATEGORIES)
criteria_count = sum(len(c["criteria"]) for c in CATEGORIES)
rated_count = sum(c["score"] is not None for cat in CATEGORIES for c in cat["criteria"])
data = {"project": "KernelLedger", "date": "2026-10-04", "scale": "0-5", "overall": round(overall, 2), "weighted_coverage": coverage, "rated_criteria": rated_count, "total_criteria": criteria_count, "categories": CATEGORIES, "findings": [dict(zip(['id','priority','title','evidence','reproduction','recommendation','acceptance'], f)) for f in FINDINGS]}
(ROOT / 'scorecard.json').write_text(json.dumps(data, indent=2), encoding='utf-8')
with (ROOT / 'scorecard.csv').open('w', newline='', encoding='utf-8') as f:
    writer = csv.writer(f)
    writer.writerow(['category','category_weight','criterion','score_0_to_5','evidence','note'])
    for cat in CATEGORIES:
        for c in cat['criteria']:
            writer.writerow([cat['name'],cat['weight'],c['name'],'NT' if c['score'] is None else c['score'],c['evidence'],c['note']])


def evidence_link(ref):
    if ref.endswith('.json'):
        return f'[{ref}]({ref})'
    if ref.startswith('docs/'):
        return f'[{ref}](../../{ref})'
    return f'[{ref}]({ref}-view.png)'


lines = [
    '# KernelLedger UI/UX audit and scoring system',
    '',
    f'**Observed score: {overall:.1f}/100. Weighted evidence coverage: {coverage:.1f}%. {rated_count}/{criteria_count} criteria rated.**',
    '',
    'The native notebook is visually coherent and useful for small Python workflows. Running code, saving edits, interrupting work, restoring a checkpoint, and rendering a table/chart succeeded. Keyboard execution, deletion recovery, adaptive layout, accessible navigation and the presentation of recorded work need attention before a broad user release. This is a structured expert assessment of a development build, not a participant study or accessibility certification.',
    '',
    '## Environment and method',
    '',
    '- Audit date: 4 October 2026. Windows 11; Python 3.12.10; KernelLedger 0.1.0 native Rust/GPUI UI.',
    '- Executable: `desktop/target/debug/kernelledger-desktop.exe`, 51,521,024 bytes, last built 4 October 2026 11:48 local time. UI source timestamp: 11:47:56. Existing binary was used; no rebuild or product-code changes.',
    '- Desktop mouse, Unicode keyboard input and screenshot capture used Windows APIs. The collaborative browser was initialized but the product is a native app, so its UI was audited directly.',
    '- Display: 2880 x 1800 physical pixels at observed 200% scale. Main window tested at 2560 x 1600, 2048 x 1400 and 1600 x 1200 physical pixels, approximately 1280 x 800, 1024 x 700 and 800 x 600 logical outer dimensions. Content area is smaller due to window chrome.',
    '- Isolated fixture: `artifacts/ui-ux-audit/notebook`. Existing `analysis/` was not used. Initial fixture had Markdown, two successful code cells, a deliberate ZeroDivisionError and raw notes. A temporary sleep cell was added/deleted and a rich-output cell was added for pandas/PNG testing.',
    '- Evidence combines observed screenshots, saved source, runtime ledger output, native UI Automation tree and targeted implementation/document review. Source-backed unfinished features are labeled in criterion notes; they were not treated as successful live tests.',
    '- An interrupted first launch was resumed. The app remained responsive during the completed audit journeys. Startup and performance were not benchmarked.',
    '- Runtime was stopped after testing; app left open on the isolated notebook for review. Temporary always-on-top positioning used for screenshots was removed.',
    '',
    '## Scoring rules',
    '',
    'Sixteen categories carry weights totaling 100. Each has four equally weighted criteria. This rubric covers visual design, task efficiency, navigation, authoring, execution, feedback, scientific output, provenance, recovery, safety, keyboard use, accessibility, resizing, scale, preferences and help/platform integration.',
    '',
    '| Rating | Meaning |',
    '| --- | --- |',
    '| 0 | Absent or unusable for the assessed task |',
    '| 1 | Severe friction, blocked path or major missing capability |',
    '| 2 | Partial; repeated workarounds or substantial ambiguity |',
    '| 3 | Usable with noticeable gaps |',
    '| 4 | Strong observed behavior with minor limitations |',
    '| 5 | Excellent, repeatably verified behavior across relevant states |',
    '| NT | Not tested; no pass/fail inference |',
    '',
    '`Category rating = mean of its rated criteria.`',
    '',
    '`Category points = category weight × category rating / 5.`',
    '',
    '`Observed total = sum of category points.`',
    '',
    '`Weighted evidence coverage = sum(category weight × rated criteria / all category criteria).`',
    '',
    'NT criteria are excluded from a category mean and reduce coverage. If an entire category is NT, exclude its weight and renormalize the observed score over rated category weights. This prevents untested features being scored as failures. It also means the total is provisional: a high category score with low coverage must not be mistaken for demonstrated readiness. The performance category, for example, has only 25% coverage.',
    '',
    'Proposed interpretation bands for this project: 90–100 excellent; 80–89 strong; 70–79 usable with gaps; 60–69 substantial improvement needed; below 60 major workflow/qualification gaps. These are custom rubric bands, not industry certification thresholds. Differences of a point or two are not meaningful without repeat assessment.',
    '',
    'Proposed release gate: observed score at least 80, weighted coverage at least 90%, no unresolved P1 findings, and direct verification of keyboard-only, screen-reader and safe-recovery journeys. This gate is a recommendation, not a claim that a score alone establishes release readiness.',
    '',
    'Usability criteria are informed by [Nielsen’s usability heuristics](https://www.nngroup.com/articles/ten-usability-heuristics/). Accessibility checks use [WCAG 2.2](https://www.w3.org/TR/WCAG22/) as a reference for keyboard, focus, contrast and target-size concerns; the audited app is native Windows software, so this is not a WCAG conformance determination.',
    '',
    '## Weighted scorecard',
    '',
    '| Category | Weight | Rating / 5 | Points | Coverage |',
    '| --- | ---: | ---: | ---: | ---: |',
]
for cat in CATEGORIES:
    lines.append(f"| {cat['name']} | {cat['weight']} | {cat['rating']:.2f} | {cat['points']:.2f} | {cat['coverage']:.0%} |")
lines += [f'| **Total** | **100** | | **{overall:.1f}/100** | **{coverage:.1f}% weighted** |', '', '## Task outcomes', '',
    '| Task | Outcome | Evidence |', '| --- | --- | --- |',
    '| Launch with initialized notebook | Pass: offline inspection, source/history available | [Initial screen](01-notebook-initial-view.png) |',
    '| Edit and save source | Pass: comment persisted on disk, stale-output feedback shown | [Edit evidence](03-edit-keyboard-run-view.png), `notebook/cells/sales.py` |',
    '| Run from cell button | Pass: stdout and result 675 | [Single-cell run](02-run-cell-view.png) |',
    '| Run from focused editor using advertised shortcuts | Fail: inserts newline | [Ctrl+Enter](04-ctrl-enter-view.png) |',
    '| Escape then Shift+Enter | Pass: execution ordinal advanced | [Command-mode run](05-command-mode-run-view.png) |',
    '| Run all including deliberate error | Pass for visible execution/error retention; batch stop-on-error beyond the error was not separately tested | [Batch](06-run-all-error-view.png) |',
    '| Inspect Python traceback | Pass: red error panel shows exception and source | [Traceback](30-bottom-cell-view.png) |',
    '| Inspect history/session/execution | Partial: paths work, identity/detail presentation is difficult | [Execution details](10-execution-detail-view.png) |',
    '| Create and restore checkpoint | Pass: new session has restored_checkpoint_id | [Dialog](12-restore-confirm-view.png), [ledger](runtime-evidence.json) |',
    '| Escape confirmation | Pass | [Dismissed dialog](13-dialog-escape-view.png) |',
    '| Run Doctor | Partial: result available as JSON | [Doctor](16-doctor-view.png) |',
    '| Switch light/dark theme | Pass within session; resets after restart | [Dark settings](19-settings-top-view.png), [Startup](39-startup-view.png) |',
    '| Filter cells/no matches | Pass: outline reports no matches | [Empty filter](22-filter-empty-view.png) |',
    '| Palette search + keyboard activation | Partial: search works; Arrow Down/Enter does not execute | [Palette](21-palette-keyboard-view.png) |',
    '| Add a code cell | Partial: added, but not immediately revealed/focused | [Add](28-add-cell-view.png) |',
    '| Run and interrupt 30-second sleep | Pass: Running → interrupted → Idle | [Interrupt](32-interrupt-view.png), [ledger](runtime-evidence.json) |',
    '| Delete a saved cell then Ctrl+Z | Fail for safe recovery: immediate delete and no structural undo | [After undo attempt](34-delete-undo-view.png) |',
    '| Resize to compact desktop windows | Fail for containment/reachability of some controls | [1024](24-checkpoints-1024-view.png), [800](26-notebook-800-view.png) |',
    '| Render pandas table and PNG chart | Pass: both are visible | [Rich output](42-rich-output-finished-view.png) |',
    '| Stop runtime and retain offline UI | Partial: shutdown succeeded, but final UI showed a transport error and stale operation state | [Stop failure state](43-final-offline-view.png), [Follow-up](46-final-review-view.png), [CLI status](final-runtime-status.json) |',
    '| Native accessibility inspection | Partial: named controls exist; navigation/output missing in captured tree; window title empty | [UIA tree](accessibility-tree.json) |',
    '', '## Prioritized findings', '',
    'P1 = serious task, safety, reachability or accessibility issue; address before broad release. P2 = substantial usability improvement; schedule next. P3 = polish/terminology. No observed P0 runtime data-loss/crash incident is claimed. Source deletion without immediate undo is a separate P1 safety concern.', '']
for fid, priority, title, ev, reproduction, recommendation, acceptance in FINDINGS:
    lines += [f'### {fid} · {priority} · {title}', '', f'**Observed:** {reproduction}', '', f'**Evidence:** {evidence_link(ev)}.', '', f'**Change:** {recommendation}', '', f'**Acceptance:** {acceptance}', '']
lines += ['## Detailed rubric: all 64 criteria', '']
for cat in CATEGORIES:
    lines += [f"### {cat['name']} · {cat['weight']} points", '', '| Criterion | Score | Evidence and rationale |', '| --- | ---: | --- |']
    for c in cat['criteria']:
        lines.append(f"| {c['name']} | {'NT' if c['score'] is None else c['score']} | {c['note']} {evidence_link(c['evidence'])} |")
    lines.append('')
lines += ['## Remaining qualification matrix', '',
    '| Area | Verification to perform before assigning a final score |', '| --- | --- |',
    '| Keyboard/focus | Full mouse-free open/edit/run/history/checkpoint journey; Tab order; focus trap and return; focus visibility in both themes |',
    '| Screen reader | NVDA reads window, navigation, selected cell, code, outputs, execution state, input prompts and errors; repeat with native UIA tooling |',
    '| Visual accessibility | Measure normal/large text, control/focus contrast; check target sizes/spacing, color-independent meaning, high contrast and larger fonts |',
    '| Editor | Undo/redo after autosave and run; large cells; clipboard; find/replace; IME; selection retention during virtualization |',
    '| Recovery | Concurrent external edit; save conflict; helper/worker crash; reconnect; retained drafts; interrupted batch; reopen unsaved work |',
    '| Runtime/input | input() ownership and handoff, getpass behavior, interpreter picker validation, long queue, run-above/below ordering, terminate/restart confirmations |',
    '| Rich content | JPEG, JSON/Markdown MIME, unsupported MIME fallback, image failures, wide/large tables, HTML safety, LaTeX fallback, output hide/show and persistence |',
    '| Scale/performance | Measured cold/warm startup; typing/scroll latency; hundreds/thousands of cells; large history; 100 MiB output; long-session memory/GPU use |',
    '| Platform | 100/125/150/200% DPI; mixed monitors; GPU/driver variants; clean machine; signed installer/update/uninstall; Linux qualification where supported |',
    '| Product completion | Create/import/export, recent notebooks, keyboard guide/help, source comparison, publish/pin, multi-notebook and detached-window journeys |',
    '| Human validation | Observe novice and experienced notebook users; measure task success, errors, time and satisfaction rather than inferring them from expert review |',
    '', '## Recommended implementation order', '',
    '1. Fix editor shortcuts and keyboard palette activation; add structural undo and contextual accessible names/navigation.',
    '2. Fix dialog wrapping/copy, row shrink/wrap behavior and compact toolbar overflow.',
    '3. Replace UUID-first record views with meaningful summaries; separate screen data/scroll state; focus new cells.',
    '4. Persist preferences, add create/recent/help flows, then complete accessibility/recovery/performance qualification.',
    '', '## Reuse and artifacts', '',
    '- `scorecard.html`: standalone interactive scorecard; category filtering, score editing, evidence links, and JSON export. Edits are local to the page until exported.',
    '- `scorecard.json` and `scorecard.csv`: machine-readable category weights, criterion ratings and evidence.',
    '- `build_scorecard.py`: reproducibly generates all reports from the embedded audit data.',
    '- `computer.py`: native audit capture/input helper, restricted to the app PID stored in `app.pid`. Coordinates are logical half-resolution coordinates for this 200% display; text/key delivery requires app focus.',
    '- Numbered PNG files are evidence; `-view.png` versions are downsampled for easier reading. UI Automation and runtime snapshots are stored separately.',
    '- Re-score fixed criteria using the same fixture and rubric. Preserve original evidence and record build/environment changes; add measurements for NT criteria rather than converting them to arbitrary estimates.',
    '', 'No product source files were changed. The audit fixture and artifacts are the only workspace additions. This repository snapshot has no `.git` directory, so no commit or PR was created.', '']
(ROOT / 'REPORT.md').write_text('\n'.join(lines), encoding='utf-8')

payload = json.dumps(data).replace('<', '\\u003c')
page = '''<!doctype html><html lang="en"><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>KernelLedger UI/UX scorecard</title>
<style>:root{color-scheme:light dark;font-family:system-ui,sans-serif;background:#10131d;color:#e8eaf2}*{box-sizing:border-box}body{margin:0}main{max-width:1160px;margin:auto;padding:36px 24px}h1{font-size:32px;margin:0 0 8px}p{line-height:1.6;color:#b9c1d5}a{color:#aaaaff}button,select,input{font:inherit;background:#20283b;color:#eff1fa;border:1px solid #485470;border-radius:7px;padding:8px}button{cursor:pointer}button:focus-visible,select:focus-visible,a:focus-visible{outline:3px solid #babaff;outline-offset:3px}.summary{display:grid;grid-template-columns:repeat(3,1fr);gap:16px;margin:24px 0}.metric,article{background:#1b2232;border:1px solid #36415c;border-radius:12px;padding:20px}.metric strong{font-size:32px;display:block}.metric span{color:#b9c1d5}.tools{display:flex;gap:12px;align-items:center;flex-wrap:wrap;margin:22px 0}.categories{display:grid;grid-template-columns:repeat(2,minmax(0,1fr));gap:16px}h2{font-size:18px;margin:0 0 12px}.bar{background:#35415d;border-radius:20px;height:7px;margin:12px 0}.bar span{display:block;height:7px;background:#8e95ff;border-radius:20px}small{color:#b9c1d5}details{border-top:1px solid #36415c;margin-top:16px;padding-top:12px}summary{cursor:pointer}.criterion{display:grid;grid-template-columns:1fr 85px;gap:10px;padding:14px 0;border-bottom:1px solid #36415c}.criterion p{grid-column:1 / -1;font-size:14px;margin:0}.finding{margin:14px 0}.badge{font-size:13px;background:#3d3552;color:#ffc3d4;border-radius:5px;padding:3px 7px}.finding p{margin:8px 0}footer{margin-top:30px;border-top:1px solid #36415c;padding-top:20px}.note{background:#25283e;border-left:3px solid #9298ff;padding:12px 16px}@media(max-width:720px){.summary,.categories{grid-template-columns:1fr}main{padding:22px 16px}}</style>
<main><h1>KernelLedger UI/UX scorecard</h1><p>Native Windows app · 4 October 2026 · 16 weighted categories · 64 criteria</p><p class="note">Expert audit of observed workflows. Untested criteria remain NT and reduce evidence coverage. This score is provisional and does not certify accessibility, security or performance.</p>
<div class="summary"><div class="metric"><strong id="total"></strong><span>Observed score / 100</span></div><div class="metric"><strong id="coverage"></strong><span>Weighted evidence coverage</span></div><div class="metric"><strong id="count"></strong><span>Criteria rated / 64</span></div></div>
<div class="tools"><label for="filter">Category</label><select id="filter"><option value="all">All categories</option></select><button id="export">Export current JSON</button><button id="reset">Reset audit scores</button><a href="REPORT.md">Read full audit</a><a href="scorecard.csv">CSV rubric</a></div><p>Expand a category to see evidence or change its ratings. Changes stay in this page until exported; the original audit files are preserved.</p><div id="categories" class="categories"></div><h2 style="margin-top:32px">Prioritized findings</h2><div id="findings"></div>
<footer><p>References: <a href="https://www.nngroup.com/articles/ten-usability-heuristics/">Nielsen usability heuristics</a> and <a href="https://www.w3.org/TR/WCAG22/">WCAG 2.2</a>. Native accessibility requires direct Windows UIA/NVDA qualification. Performance, crash recovery, mixed DPI and advanced workflows remain partially untested.</p></footer></main>
<script>
const original=__DATA__;let data=structuredClone(original);
const escapeHtml=s=>String(s).replace(/[&<>"']/g,c=>({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[c]));
function evidence(ref){const href=ref.endsWith('.json')?ref:ref.startsWith('docs/')?'../../'+ref:ref+'-view.png';return `<a href="${escapeHtml(href)}">Evidence</a>`}
const filter=document.querySelector('#filter');original.categories.forEach((c,i)=>{const o=document.createElement('option');o.value=i;o.textContent=c.name;filter.append(o)});
function compute(){let points=0,coverage=0,count=0,ratedWeight=0;data.categories.forEach(c=>{const rated=c.criteria.filter(x=>x.score!==null);c.rating=rated.length?rated.reduce((n,x)=>n+x.score,0)/rated.length:null;c.coverage=rated.length/c.criteria.length;c.points=c.rating===null?null:c.weight*c.rating/5;points+=c.points||0;coverage+=c.weight*c.coverage;count+=rated.length;if(rated.length)ratedWeight+=c.weight});data.overall=ratedWeight?points*100/ratedWeight:null;data.weighted_coverage=coverage;data.rated_criteria=count;document.querySelector('#total').textContent=data.overall===null?'NT':data.overall.toFixed(1);document.querySelector('#coverage').textContent=coverage.toFixed(1)+'%';document.querySelector('#count').textContent=count+'/64';}
function render(){compute();document.querySelector('#categories').innerHTML=data.categories.map((c,i)=>{if(filter.value!=='all'&&Number(filter.value)!==i)return '';return `<article><h2>${escapeHtml(c.name)}</h2><small>${c.weight} point weight · ${c.rating===null?'NT':c.rating.toFixed(2)+'/5'} · ${c.points===null?'NT':c.points.toFixed(2)+' points'} · ${(c.coverage*100).toFixed(0)}% coverage</small><div class="bar"><span style="width:${c.rating===null?0:c.rating*20}%"></span></div><details><summary>Criteria, scores and evidence</summary>${c.criteria.map((x,j)=>`<div class="criterion"><label for="s-${i}-${j}">${escapeHtml(x.name)}</label><select id="s-${i}-${j}" data-i="${i}" data-j="${j}">${[null,0,1,2,3,4,5].map(v=>`<option value="${v===null?'NT':v}" ${v===x.score?'selected':''}>${v===null?'NT':v}</option>`).join('')}</select><p>${escapeHtml(x.note)} ${evidence(x.evidence)}</p></div>`).join('')}</details></article>`}).join('');document.querySelector('#findings').innerHTML=data.findings.map(f=>`<article class="finding"><h2><span class="badge">${f.priority}</span> ${f.id}: ${escapeHtml(f.title)}</h2><p>${escapeHtml(f.reproduction)} ${evidence(f.evidence)}</p><p><b>Change:</b> ${escapeHtml(f.recommendation)}</p><details><summary>Acceptance check</summary><p>${escapeHtml(f.acceptance)}</p></details></article>`).join('');}
document.querySelector('#categories').addEventListener('change',e=>{if(e.target.matches('select[data-i]')){data.categories[e.target.dataset.i].criteria[e.target.dataset.j].score=e.target.value==='NT'?null:Number(e.target.value);compute();const open=[...document.querySelectorAll('#categories details')].map(x=>x.open);render();document.querySelectorAll('#categories details').forEach((x,i)=>x.open=open[i]);}});filter.addEventListener('change',render);document.querySelector('#reset').onclick=()=>{data=structuredClone(original);render()};document.querySelector('#export').onclick=()=>{compute();const a=document.createElement('a');const url=URL.createObjectURL(new Blob([JSON.stringify(data,null,2)],{type:'application/json'}));a.href=url;a.download='kernelledger-scorecard.json';a.click();setTimeout(()=>URL.revokeObjectURL(url),1000)};render();
</script></html>'''.replace('__DATA__', payload)
(ROOT / 'scorecard.html').write_text(page, encoding='utf-8')
assert sum(c['weight'] for c in CATEGORIES) == 100
assert criteria_count == 64
print(json.dumps({'score':round(overall,2),'weighted_coverage':coverage,'rated':rated_count,'criteria':criteria_count}))
