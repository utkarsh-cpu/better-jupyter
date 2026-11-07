# KernelLedger screenshot-based UI/UX polish plan

| Item | Decision |
| --- | --- |
| Date | 2026-11-01 |
| Status | Implemented 2026-11-01. Notes and acceptance evidence: [screenshot polish](../docs/desktop/frontend-screenshot-polish.md). |
| Track | Frontend; no frontend version number reserved. |
| Baseline | Native captures in `artifacts/ui-components`, modified 2026-10-27 through 2026-10-29, and the current working-tree implementation. |
| Requires | Existing runtime v5 contracts and the implemented notebook/assistant capabilities; no new runtime behavior is assumed. |
| Coordination | Refines the [frontend clarity draft](frontend-v6-draft.md), [assistant draft](ai-assistant-draft.md), and [notebook interactivity draft](notebook-interactivity-draft.md). Does not supersede [FRONTEND_PLAN.md](../FRONTEND_PLAN.md). |

## 1. Outcome and boundaries

Improve daily notebook use through eight changes: compact cell spacing, editing-first
cell menus, assistant setup guidance, clear global toolbar actions, scannable History,
readable checkpoint reports, organized Settings, and identifiable recent notebooks.

Keep the native GPUI interface, warm light palette, blue accent, restrained selection,
existing themes, independent editor font size, and Compact/Comfortable preferences.
Retain execution, trust, context approval, source revision, and recovery semantics.
This is a focused polish increment, not a new visual identity or runtime redesign.

Structural undo, execution queues, new checkpoint compatibility contracts, output
engine changes, and large-history pagination remain in their existing plans. This
increment must not imply that those capabilities have been delivered.

## 2. Evidence and initial verification

The reviewed files are real native captures, not mockups:

| Surface | Evidence | File modification date |
| --- | --- | --- |
| Notebook | [notebook.png](../artifacts/ui-components/notebook.png) | 2026-10-28 |
| Cell actions | [overlay-cell-actions.png](../artifacts/ui-components/overlay-cell-actions.png) | 2026-10-29 |
| Assistant | [assistant.png](../artifacts/ui-components/assistant.png) | 2026-10-29 |
| History | [history-list.png](../artifacts/ui-components/history-list.png) | 2026-10-28 |
| Checkpoint details | [checkpoints-inspect.png](../artifacts/ui-components/checkpoints-inspect.png) | 2026-10-27 |
| Settings | [settings.png](../artifacts/ui-components/settings.png) | 2026-10-28 |
| Welcome | [welcome.png](../artifacts/ui-components/welcome.png) | 2026-10-28 |

These files span multiple capture runs. Modification dates and the shared manifest
do not establish a single build identity for all screenshots. Before implementation:

- Build the current working tree and capture the seven affected surfaces into a new
  dated directory using `scripts/desktop_ui_components.py` and its disposable fixture.
- Record source revision plus dirty-tree status, executable hash, theme, window size,
  DPI, UI scale, editor size, and density alongside the captures.
- Preserve the existing evidence. Use separate `before` and `after` directories.
- Confirm each visible issue still exists; record any already-resolved item rather
  than reimplementing it. Preserve unrelated working-tree changes.

## 3. Requirements and acceptance criteria

### F-UXP-01: Compact notebook spacing

**Observed:** Run controls, short source blocks, and output are separated by large
vertical gaps even in the captured Compact setting.

**Change:** Reduce header-to-source and source-to-output padding in Compact mode.
Use shared spacing tokens and preserve a stronger boundary between successive cells.
Keep run targets, line height, caret, focus indication, and Comfortable targets usable.
Do not reduce editor font size to achieve density.

**Acceptance:** The same short-cell fixture occupies measurably less vertical space
at identical window/font/scale settings; record before/after cell heights. Selection,
multiline editing, Markdown, error output, rich output, and insertion boundaries remain
clear. No clipped source, output, or controls at supported scales. Comfortable mode
retains its existing target-size guarantees.

**Likely files:** `desktop/crates/ui/src/notebook.rs`, `theme.rs`, `components.rs`.

### F-UXP-02: Editing-first cell actions

**Observed:** Five assistant actions precede Duplicate and Move in the cell menu.

**Change:** Place available editing operations first; group assistant operations under
`Assistant`; retain a separate destructive-action group for Delete. Preserve shortcuts,
contextual availability, stable cell targeting, and existing delete confirmation.

**Acceptance:** Duplicate/Move are reachable without traversing AI actions. Every
existing assistant action remains reachable. Keyboard users can enter/leave the
assistant group, dismiss with Escape, and return focus to the invoking cell control.
Menus fit near window edges and under enlarged text. If native submenus cannot meet
these requirements, use an accessible secondary action panel with an explicit Back.

**Likely files:** `desktop/crates/ui/src/shell.rs`, `assistant.rs`, `components.rs`.

### F-UXP-03: Actionable assistant setup and context

**Observed:** A large blank assistant panel has a small Setup badge; the composer uses
an opaque `@rich` context label and an unexplained Review context action.

**Change:** Show an introductory state with `Connect a provider`, a short explanation,
and a direct link to provider settings. Distinguish unconfigured, configured/untested,
connecting, ready, and failed states using actual available settings/request results.
Render selected cells as removable chips with recognizable titles and count. Explain
that Review context previews information before a request is sent.

**Acceptance:** A first-time user has one clear next step. Connection failures show
a useful recovery action without exposing credentials. Setup preserves the draft
and selected context. Context chips match the receipt; removing one updates scope.
No new remote request or approval bypass is introduced. Existing conversation,
loading, empty, and error states remain usable at narrow panel widths.

**Likely files:** `desktop/crates/ui/src/assistant.rs`, `settings.rs`.

### F-UXP-04: Clear global toolbar actions

**Observed:** Global Run and per-cell Run both appear as play icons; Export uses a
document-shaped icon. The code already supports a Run all label conditionally.

**Change:** Adjust adaptive layout so `Run all` appears at the reviewed desktop width;
prefer a labeled `Export` action there. Keep secondary controls in overflow when
needed. Preserve icon-only fallbacks with precise tooltips and accessible names at
narrow widths. Use existing glyphs before introducing new assets.

**Acceptance:** Global execution scope is visible at the baseline size. Run all invokes
the existing batch behavior and cell Run remains cell-scoped. Labels do not overlap
runtime status, notebook navigation, or window controls at 800 x 600 or 200% UI scale.
Disabled/busy states retain their existing explanation and action availability.

**Likely files:** `desktop/crates/ui/src/shell.rs`, `components.rs`, `interchange.rs`.

### F-UXP-05: Scannable History and explicit filter state

**Observed:** Source, relative time, and duration are one bold sentence. Show JSON is
a prominent standalone control; filter application state is not obvious visually.

**Change:** Align Cell, Outcome, Run time, and Duration fields on wide layouts; use
stacked labeled rows on narrow layouts. Keep source/title primary and metadata quieter.
Provide exact timestamps alongside relative time, and show missing duration as unknown.
Retain explicit Apply behavior: indicate pending filter changes and distinguish them
from applied filters; Enter applies valid search, Clear resets both states. Put raw
records under a `Technical details` disclosure.

**Acceptance:** Users can scan failures and durations without reading whole sentences.
Invalid filters stay editable and show a useful error. Pending edits cannot appear
to describe the displayed result set. Back navigation preserves applied filters,
selection, and scroll. Raw JSON and copy access remain available. Do not add sorting,
grouping, or searches that imply complete results from a partial loaded page.

**Likely files:** `desktop/crates/ui/src/history.rs`, `surfaces.rs`.

### F-UXP-06: Readable checkpoint details

**Observed:** Main content exposes `source_unavailable` and JSON coverage notes.

**Change:** Map known reasons to plain-language explanations while retaining value
names. Explain Partial next to the status, distinguish unknown coverage from zero,
and move structured payloads to Technical details. Give Restore and Continue short,
visible descriptions: restore supported values in a new session versus restore and
replay later recorded executions. Preserve the namespace-replacement warning.

**Acceptance:** Known reasons are readable without opening raw details. Unknown reason
codes retain their original diagnostic information with an honest fallback. No saved
counts or compatibility claims are inferred from incomplete data. Trust checks,
confirmation, exclusion visibility, and runtime action semantics remain unchanged.

**Likely files:** `desktop/crates/ui/src/checkpoints.rs`, `history.rs`; presentation
helpers in `desktop/crates/models/src/presentation.rs` if shared formatting is useful.

### F-UXP-07: Navigable Settings with preview

**Observed:** One long settings card mixes unrelated categories and places controls
far from their labels.

**Change:** Add section navigation for Appearance, Editor, Layout & accessibility,
Runtime, and Assistant. Use a bounded label/control layout. At narrow widths replace
the section rail with an accessible selector. Include a small representative code
preview for font and density choices. Preserve existing immediate preference saving;
avoid implying an unsaved Apply workflow. Searchable settings are a later extension.

**Acceptance:** All existing settings remain reachable with pointer and keyboard.
Changing sections preserves appropriate focus/scroll. Preview updates agree with the
actual notebook and never execute code. Preferences survive relaunch; save failures
remain visible. Controls stay reachable at 200% UI scale and in high contrast.

**Likely files:** `desktop/crates/ui/src/settings.rs`, `history.rs` (page container),
`desktop/crates/client/src/preferences.rs` only if persistence changes are needed.

### F-UXP-08: Identifiable recent notebooks and accurate welcome copy

**Observed:** Recent notebooks are short name-only buttons. Welcome copy incorrectly
describes execution history as being stored in plain files.

**Change:** Use recent-notebook rows with name, folder, and last-opened time where
recorded. Handle long paths without losing the distinguishing part; provide full-path
access. Use copy such as: `Local-first Python notebooks. Source stays in ordinary
files; execution history is recorded locally in SQLite.` Keep Open/Create prominent,
and sample/help secondary.

**Acceptance:** Same-name notebooks can be distinguished and opened correctly. Missing
locations show a useful recovery message. Empty recents and long names work. If old
preferences contain paths only, omit unknown timestamps; record future opens through
a backward-compatible preference update. Never use filesystem modification time as
last-opened time. All existing recent paths survive any migration.

**Likely files:** Welcome/recent rendering in `desktop/crates/ui/src/lib.rs` or its
surface helper, and the actual recent-history persistence owner located during setup.
Update related welcome/help copy wherever the same storage claim is repeated.

## 4. Delivery sequence

| Phase | Work | Exit condition |
| --- | --- | --- |
| 0: Establish baseline | Reconcile captures with current source; capture controlled fixtures; inspect recent-history storage and menu capabilities. | Evidence identifies the actual before state and implementation dependencies. |
| 1: Daily editing polish | F-UXP-01, F-UXP-02, F-UXP-04, plus the welcome wording correction. | Notebook is denser, cell menu is editing-first, toolbar scope is clear, copy is accurate. |
| 2: Setup and workspace organization | F-UXP-07, then F-UXP-03; finish F-UXP-08. | Provider setup links to stable Settings navigation; context/drafts survive the round trip; recents are identifiable. |
| 3: Record readability | F-UXP-05 and F-UXP-06, sharing technical-detail presentation where useful. | History and checkpoint journeys meet their acceptance criteria without runtime-contract changes. |
| 4: Integrated verification | Native captures and focused regression journeys across affected surfaces. | All eight requirements have linked evidence, outstanding limitations, and pass/fail status. |

Each phase is independently reviewable. Complete shared control and navigation work
before dependent surfaces. Do not bundle unrelated runtime or assistant feature work.

## 5. Validation and completion gates

### Behavioral checks

- Run targeted Rust tests for menu focus/action targeting, global versus cell Run,
  pending/applied filters, checkpoint reason fallback, assistant context preservation,
  and any preference migration introduced by the implementation.
- Add tests only for changed behavior and data invariants; do not snapshot every
  spacing constant or write tests that merely duplicate rendering code.
- Run workspace formatting, relevant tests, and Clippy using the repository's existing
  configuration. Finish with a desktop build. Python checks are needed only if Python
  code/contracts are actually changed; no such changes are expected here.

### Native visual and interaction checks

- Use `scripts/desktop_ui_components.py` with a separately built executable and new
  output directory. Review notebook, cell menu, assistant setup, History, checkpoint
  detail, Settings, and Welcome. Extend disposable fixtures only where needed.
- Baseline: 1280 x 850 logical size, light theme, Compact density, 100% UI scale,
  fixed editor size. Compare with the same fixture and recorded DPI after changes.
- Check all affected surfaces in dark and high contrast; repeat constrained layouts
  at 800 x 600 and 200% UI scale. Exercise Comfortable density for notebook/menu controls.
- Traverse setup, menu, filter, checkpoint, and settings journeys with keyboard only;
  inspect accessible names, state, focus return, and clipping. Perform native screen
  reader checks for new navigation, context chips, and grouped menus. Report untested
  assistive-technology behavior explicitly rather than treating screenshots as proof.
- Include unknown metadata, missing recent paths, failed connection, long titles,
  partial checkpoint, and filter-no-results states. Use disposable data and do not send
  real assistant requests merely to capture the setup flow.

### Definition of done

Every F-UXP requirement has implementation notes, before/after captures where relevant,
and acceptance evidence. No new clipping, inaccessible controls, misleading state,
lost preferences/context, or action-scope regression remains. Preserve all existing
unrelated work. Document residual platform or runtime limitations separately; passing
this polish plan is not a declaration of full application release qualification.
