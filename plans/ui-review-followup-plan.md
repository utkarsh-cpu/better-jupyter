# KernelLedger UI review follow-up: final implementation proposal

| Item | Decision |
| --- | --- |
| Date | 2026-11-05 |
| Status | Sol candidate integrated 2026-11-07; [implementation and qualification notes](../docs/desktop/ui-review-followup.md). Broader release gates remain open. |
| Scope | A focused native desktop UI increment after the November screenshot polish. |
| Relationship | Work plan under the existing frontend track; does not replace or publish a new version of `FRONTEND_PLAN.md`. |
| Evidence | [Captured comparison and first review](../artifacts/ui-comparison-2026-11-05/review.md), [Grok review](../artifacts/ui-comparison-2026-11-05/grok-review.md), [Cursor Opus review](../artifacts/ui-comparison-2026-11-05/opus-cursor-review.md), and the parent critiques in that directory. |
| Existing work | [Implemented screenshot polish](../docs/desktop/frontend-screenshot-polish.md). |

## 1. Intended result

Make the active notebook easier to identify, keep execution controls clear in small windows,
reduce unnecessary cell chrome in Compact mode, and verify that keyboard and screen-reader
users can reach the content and state shown visually.

Retain the native GPUI stack, warm light palette, blue accent, existing light/dark/high-contrast
themes, configurable editor size, current code line spacing, and Compact/Comfortable choice.
Use spacing and control placement to recover room rather than smaller text or smaller targets.
Preserve execution, source saving, trust, assistant approval, history and recovery semantics.

The scores inform priorities, not acceptance. All reviewers saw the same small fixture;
Markdown disclosure, traceback disclosure and settings categories were not matched.
The Antigravity audits supplied no numerical visual scores.

## 2. What the code already provides

- `WorkbenchButton::contextual` in `components.rs` reveals controls on group hover **and focus**.
  Check live behavior before adding any new reveal mechanism.
- The notebook switcher already has a full-name hint and accessible name in `shell.rs`.
  The current visible label uses a character limit; the responsive budget reserves a small slot.
  Improve that allocation and verify focus access to the full name.
- `cell_spacing` in `theme.rs` already separates Compact and Comfortable spacing and feeds the
  Settings preview. The previous pass reduced gaps; this increment concerns remaining rows
  and action placement, not repeating that adjustment.
- Settings already separates Appearance, Editor, Layout & accessibility, Runtime and Assistant.
- Recorded execution duration is already formatted for History. `CellModel` does not currently
  carry duration, so optional notebook timing needs explicit record-to-cell association.

## 3. Delivery order

### Step 0 — Establish a reproducible current baseline

Build the current working tree and capture owned, disposable notebooks with isolated preferences.
Keep existing evidence intact. Save new evidence under a new dated `before/` and `after/` directory.
Record source revision, dirty status, binary hash, window/client sizes, DPI, UI scale, editor size,
density, theme, selection/focus, source disclosure and traceback disclosure.

Use both the short comparison fixture and a 30+ cell mixed notebook with Markdown, short and
multiline code, printed output, a table/figure, an error, stale source and unsaved/conflict states.
Match disclosure states for before/after measurements. Save the capture commands/scripts and
UIA properties/patterns needed to reproduce the checks.

**Exit:** Every suspected issue is marked reproduced, already resolved, or unverified. Existing
user notebooks and profiles remain untouched. The old comparison executable is not treated as
a fresh build of current source.

### Step 1 — Verify and repair content, focus and state access

**Priority: P0.** Inspect the live Windows accessibility provider and test with Narrator and NVDA:

1. Read rendered Markdown heading/body without switching to Edit mode.
2. Read cell count, save state and runtime state; check actual state changes.
3. Verify selected settings section and theme via applicable selection/toggle patterns.
4. Tab to every contextual cell action; it must become visible when focused.
5. Open/dismiss cell actions, palette and settings; verify logical order and focus return.

The saved dump contains Name, Role, Enabled and Rectangle only. Absence of `ControlType.Text`
or a string in those fields is a lead, not proof of absent Text/Value patterns or speech.

Repair only confirmed gaps. Expose coherent Markdown blocks and useful status semantics,
rather than a node for every span. Announce meaningful transitions without repeatedly
announcing idle polling or every autosave. Keep existing focus-reveal behavior when it passes.

**Acceptance:** Content and selected states are readable non-visually; no focus lands on an
invisible control; menu dismissal returns to its invoker; status updates do not create chatter.
Record actual results and remaining limitations rather than claiming general certification.

**Likely files:** `notebook.rs`, `shell.rs`, `settings.rs`, `components.rs`, `outputs.rs`, and the
existing accessibility/provider integration where the live investigation locates the gap.

### Step 2 — Fix notebook identification and responsive action priority

**Priority: P1.** Treat the title bar as one measured width budget, not independent hard-coded
character limits and control estimates.

- Let the notebook switcher use available room, with a bounded preferred width around 240
  logical px at the standard UI scale. Scale the budget with the interface; shrink responsively.
  Use middle ellipsis only when the rendered label needs it, preserving a useful distinguishing suffix.
- Reuse the existing full-name hint; ensure full notebook name/path is available on keyboard focus
  or through the switcher. Add path context where identical basenames need disambiguation.
- Keep outline rows compact by default. Ensure full cell identification is available on focus
  or deliberate disclosure; do not wrap every source preview or widen the outline globally.
- At 800×600 and UI scale 1, keep **Run all** text-labeled. Keep the current destination visible;
  move secondary History/Checkpoints navigation into a labeled menu when needed. Preserve their
  shortcuts and current-location indication when one is active.
- Let Export move to the existing overflow path before sacrificing the primary run label.
  At larger scales where a label genuinely cannot fit, retain an accessible, discoverable Run all
  control and document that fallback instead of overlapping controls or shrinking targets.
- Preserve native caption-button exclusion zones, a usable window-drag region and alignment.

**Acceptance:** The existing comparison name is readable at the roomy baseline; long/similar
names remain distinguishable. Run all is labeled at 800×600/scale 1. All destinations and Export
remain reachable after collapse. Window drag, resize, maximize and caption buttons still work.
Fit estimation agrees with the rendered control widths at all tested scales.

**Likely files:** `shell.rs`, `components.rs`, navigation tests and native hit-test probes.

### Step 3 — Make Compact mode save rows, not text

**Priority: P1.** Prototype one compact code-cell layout with Run and execution ordinal in a
shared leading gutter beside the source. Keep Run visible, named and keyboard reachable.
Coordinate with existing line numbers and breakpoint/debug gutters; do not overlay those targets.

- Ordinary saved, successful cells should not reserve a full empty header row.
- Preserve visible Failed, Unsaved/Saving, Conflict and Edited since run information. Use a
  compact status/action row when those states need it; never hide them to meet a density target.
- Place Markdown Edit/More actions within the existing top padding or a reserved corner area,
  rather than a dedicated mostly empty strip. Show editing access on selection, focus and hover;
  it must not cover the heading or depend exclusively on hover.
- Preserve source/output grouping, clear insertion boundaries, output controls, assistant actions
  and existing cell-target routing. Keep Comfortable mode's layout and target-size guarantees.
- Update Settings' sample preview to show the real new layout. Font size and code line pitch
  remain independently controlled; the current roughly 21 px pitch at 14 px editor size stays.

**Acceptance:** The same source/output cell loses at least its redundant header row in Compact
mode, with before/after whole-cell slot heights recorded at matched settings. The old approximately
196-versus-240 px comparison is context, not an exact universal target. No clipping, gutter collision,
mis-targeted run or lost state labels. Keyboard and pointer run/edit tasks pass on the mixed notebook.

**Ship gate:** If the gutter prototype fails discovery, debugger, large-text or focus checks,
retain a visible compact header and ship the independently verified spacing/Markdown improvements.
Do not compensate by reducing text or hit targets. This is an implementation decision, not a new
permission step.

**Likely files:** `notebook.rs`, `theme.rs`, `components.rs`, `interactive.rs`, `settings.rs` and
cell-height/virtualized-list invalidation paths.

### Step 4 — Clarify cell state and error actions

**Priority: P1.** Define a consistent visual treatment for resting, hovered, selected, editing,
failed and stale cells across code and Markdown.

- Use a quiet common resting treatment; selection and keyboard focus carry explicit emphasis.
- Preserve selection independently of the focused editor. Show the active-line treatment only
  where it helps identify editing focus; verify the pinned editor API before changing its painting.
- Keep error summaries visible with text plus a failure cue. Keep full traceback as an explicit
  disclosure, preserve its state through redraws, and verify long-path reading/copying when expanded.
- Keep Explain error tied to the failing cell. Its placement must remain associated with the
  error, and unavailable assistant setup must remain clear.
- Clarify the Output tools glyph/hint so it does not promise an immediate copy action. Verify
  current copy/show/hide behavior before changing glyphs people already use.
- Avoid adding a separate bordered card around every nested source/output block.

**Acceptance:** A user can distinguish selected cell, editing focus and failure without relying
on color alone. Inactive editors do not misleadingly look focused. Traceback disclosure, copy,
cell selection, output tools and menu focus return still operate on the correct cell.

**Likely files:** `notebook.rs`, `outputs.rs`, `components.rs`, editor configuration and theme tokens.

### Step 5 — Add quiet execution context and plain-language guidance

**Priority: P2; independent follow-up after the core layout passes.**

- Offer optional cell duration under Layout & accessibility. Keep it off by default, use muted
  fixed-width numerals, and associate it with the last recorded execution and its `exec_id`.
- Reuse existing recorded duration or start/end metadata and the History formatter. Do not derive
  duration from UI polling or display unknown as zero. Edited cells identify it as the last run.
- Update the client-side presentation model using existing loaded records. Avoid one history fetch
  per visible cell or a new runtime/protocol contract just for a badge. If existing feeds lack the
  necessary metadata, defer the badge rather than broadening this UI increment.
- Replace the raw debugger environment-variable explanation with plain language such as
  "Experimental debugging is disabled for this runtime." Keep the exact opt-in setup under
  Technical details/help. Do not invent a Settings enable switch unless one actually exists.

**Acceptance:** Optional timing matches the corresponding History record, remains absent when
unknown, and survives selection/redraw without flicker. Disabled commands describe the real
condition and a valid setup path. Enabling the display does not generate per-frame record fetches.

**Likely files:** `lib.rs`, `history.rs`, `settings.rs`, presentation preferences, and models/client
snapshot handling if existing metadata needs to be carried through.

## 4. Verification and rollout

Deliver in small changes: (1) confirmed accessibility repairs; (2) header/identification;
(3) Compact cell structure; (4) state/error polish; (5) optional timing and guidance.
Each independently shippable change includes its relevant evidence and documentation.

### Required coverage

| Dimension | Cases |
| --- | --- |
| Window | 1280×850 and 800×600 logical; a split-screen width between them. |
| Appearance | Light, dark and high contrast. |
| Density | Compact and Comfortable. |
| Scaling | Standard UI scale plus 125% and 200%; larger editor size independently. |
| Input | Pointer, keyboard-only, Narrator and NVDA for changed content/focus paths. |
| Cell state | Ready, running/queued where supported, failed, stale, saving and conflict. |
| Content | Short and multiline code, rendered/edited Markdown, table/figure, long traceback. |

Use a reduced combination set for routine iteration and cover the risk-bearing combinations
before acceptance. If host display limits clamp a high-DPI/high-scale window, record the actual
client viewport and mark the intended combination untested; do not call a clamped capture a pass.

Extend existing behavioral tests for collapse priorities, cell routing, focus/reveal, selection
semantics, stale/error state, measured cell/list heights and duration association. Reuse native
capture and probe tooling; save full relevant UIA properties/patterns rather than names alone.
Run UI/model tests, formatting and lint checks appropriate to the touched code, then the native
run/edit/menu/navigation journeys. Broaden to client tests only when timing/presentation changes
touch that layer.

For human verification, use the same tasks before and after: identify the notebook/cell, run one
cell, run all, edit Markdown, find/read an error, expand/copy its traceback, and switch a theme.
Record completion and mistakes; do not infer usability from screenshot scores alone.

### Completion criteria

- Active notebook and cell identity remain discoverable with long or similar names.
- Run all and single-cell Run remain distinct and accessible at small widths.
- Compact mode demonstrably removes redundant chrome without reducing text or target sizes.
- Cell selection/focus/error state is coherent; keyboard focus is always visible.
- Confirmed accessibility gaps are fixed and verified; unresolved qualifications are explicit.
- Before/after evidence identifies the actual build, scale, state and measurement boundaries.
- No execution, source-saving, trust, approval or recovery regression.

## 5. Deferred work

Do not turn these reviews into a new brand/theme, a wholesale Settings/History/Checkpoints
redesign, new debugger/runtime capability, or a Marimo implementation project. The evidence
does not justify those changes. Marimo suggestions remain comparison notes for its maintainers.

This proposal becomes implementation work only when that work is requested. No product files
were changed while preparing the plan, and no new frontend version number is reserved.
