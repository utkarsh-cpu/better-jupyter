# KernelLedger frontend v5: a deliberate desktop redesign

| Item | Decision |
| --- | --- |
| Version | Frontend 5 proposal |
| Date | 2026-10-08 |
| Status | Implementation in progress. See the [implementation and qualification ledger](../docs/desktop/frontend-v5.md). Not release-qualified. |
| Baseline | [Current frontend v4](../FRONTEND_PLAN.md), [runtime v5](../PLAN.md) |
| Evidence | [Fresh native UI/UX review](../artifacts/uiux-fresh-2026-10-08/REVIEW.md) and the user's explicit rejection of the current visual design |
| Direction | A restrained scientific workbench: precise, quiet, content-first, native to the desktop |
| Visual concept | [Interactive composition study](frontend-v5-concept.html); illustrative sample data, not an implementation |
| Stack | Retain Rust, GPUI, GPUI Kit, and the existing Python client/runtime separation |
| Scope | Complete application design, interaction, states, accessibility, implementation sequence, migration, and qualification |

## 1. What this redesign must achieve

The user does not want the existing interface polished into a slightly better version of itself. Replace its visual composition and interaction hierarchy. The fresh review's 6.6/10 is a behavioral and heuristic baseline, not acceptance of its appearance. Functional success does not establish visual quality.

The application should feel like a carefully made tool for sustained work: code, results, and provenance have priority. Chrome should be compact and predictable. Every visible element should help orientation, manipulation, or understanding. Personality comes from typography, proportion, and thoughtful interaction rather than decoration.

Success means that a new user can create and run a notebook without knowing the runtime architecture, while an experienced user can edit, compare executions, and restore state efficiently. Source, recorded execution, live Python state, and saved checkpoints must remain distinguishable.

This draft adds proposed `F-RD-01` through `F-RD-20`, `FS-30` through `FS-41`, and `FD-20` through `FD-23`. Existing requirement/scenario identifiers are retained. Before publishing v5 as Current, archive v4 and reconcile the changes below into its owning sections under [the versioning rules](README.md). Unmentioned v4 requirements, runtime guarantees, and release gates remain part of the baseline; this proposal does not silently withdraw them.

## 2. Visual direction and boundaries

**F-RD-01 — Visual language.** Build a quiet, compact desktop workbench. Use flat surfaces, thin separators, a disciplined type scale, and one restrained interaction accent. Favor alignment and grouping over boxes. Reserve elevation for actual overlays. A notebook is a document, not a collection of dashboard cards.

Remove from the current composition:

- Large rounded cards around every cell, pronounced cell shadows, oversized section emblems, and excessive blank vertical space.
- The persistent two-level navigation/outline arrangement across unrelated pages.
- Full-width Back and Raw details buttons, repetitive headings, and generic RECORDS labels.
- A permanent toolbar containing every destructive, experimental, and maintenance operation.
- Repeated status badges, decorative icon tiles, long technical paths, and identifiers presented as primary labels.

Avoid generic generated-design patterns: gradient heroes, glowing borders, glass panels, pill-shaped everything, decorative charts, invented statistics, emoji icons, marketing copy inside the workspace, unnecessary animations, and uniformly spacious cards. Do not merely recolor the existing widgets or use a fashionable font to disguise unchanged composition.

Keep useful foundations: visible execution counts, source/output distinctions, deliberate confirmation for namespace replacement, offline inspection, keyboard execution, and explicit errors. A cleaner interface must not hide essential truth.

## 3. Design foundations

**F-RD-02 — Shared tokens.** Define semantic Rust tokens, consumed by all surfaces; no screen-specific magic colors or spacing. The values below are starting specifications to validate in native rendering, not certified contrast results.

| Foundation | Proposed specification |
| --- | --- |
| Light surfaces | Canvas `#FAFAF8`, chrome `#F1F2EF`, elevated surface `#FFFFFF` |
| Dark surfaces | Canvas `#15181B`, chrome `#1C2024`, elevated surface `#23282D`; avoid pure black outside high contrast |
| Text | Light primary `#20262A`, secondary `#59636B`; dark primary `#E6E9EB`, secondary `#A8B0B7` |
| Accent | Muted blue: light `#315E83`, dark `#8AB5D6`; use for selection, links, and focus, not large decorative fills |
| Semantic colors | Separate success, warning, error, and informational tokens; always accompanied by text/icon meaning |
| UI type | System UI family, Segoe UI on Windows; 13 px labels/controls, 14 px body, 18 px page titles, 24 px welcome title |
| Code type | Cascadia Mono when installed, otherwise Consolas/system monospace; 14 px starting size, 1.5 line height; configurable size |
| Spacing | 4/8/12/16/24/32 logical-pixel scale; 16–24 px content margins; 8–12 px control grouping |
| Corners | 4 px controls, 6 px menus/dialogs; no enclosing rounded frame around the application |
| Borders | 1 logical pixel where needed; verify pixel alignment at fractional scaling |
| Controls | 30–32 px regular height; pointer targets at least 28×28 logical px for compact icon actions, with comfortable-density option |
| Focus | Clearly visible 2 px outline, distinct from selection and hover; never rely on a faint tint alone |
| Motion | 100–150 ms for necessary transitions; reduced-motion support; no animated layout while typing |

Provide light, dark, follow-system, and high-contrast appearances using the same semantic system. Measure rendered text contrast: target at least 4.5:1 for normal text and 3:1 for large text and meaningful component boundaries. Validate syntax, selection, disabled-state readability, and focus independently. Audit appearance changes before the first frame and live OS changes. Do not add a font distribution/license obligation merely for styling.

## 4. Information architecture and workspace

**F-RD-03 — One workspace header.** Use a compact 40–44 px workspace strip: notebook switcher/name at left, Notebook / History / Checkpoints as the primary destinations, and search/commands plus runtime access at right. Window controls stay native. Settings, Help, Diagnostics, and global runtime management live in a labeled application menu or runtime popover and remain searchable through Commands. Do not replace labels with unexplained icons.

**F-RD-04 — Contextual panes.** Show a 220 px default cell outline only in Notebook. Make it resizable within approximately 180–300 px, collapsible at every size, and restorable through a visible Outline control. History and Checkpoints use their own lists and details; neither inherits a notebook outline. Store pane preferences per surface. No unrelated page inherits another page's selection, filters, or scroll position.

**F-RD-05 — Sizing.** All dimensions refer to logical pixels. Preserve the supported 800×600 minimum. At 800–1023 px collapse the outline and optional inspector into explicit drawers. At 1024 px and above show an outline only when the content retains at least 560 px. At larger sizes allow an optional 280–340 px inspector only when the canvas remains usable. Container constraints take precedence over a single global breakpoint.

Repeatedly shrinking and expanding must preserve clamped pane widths, never expand the outline to fill half the window. A manual collapse persists through resizing. Dialogs fit available height and scroll their content while actions stay reachable. Toolbar actions overflow by priority; Run and Interrupt remain reachable. Labels wrap or truncate intentionally with a way to recover the full value. Horizontal scrolling is permitted inside code/tables, not as a workaround for clipped application chrome.

Use the full useful width for code and data; cap long-form prose to a readable measure. Avoid an enormous empty centered canvas on wide monitors. Support native maximization, snap layouts, mixed-DPI moves, 100/125/150/175/200% scale, and UI zoom/text-size increases.

## 5. Notebook experience

**F-RD-06 — Continuous document.** Replace heavy cell cards with a continuous canvas separated by whitespace and subtle rules. Use a stable left gutter for execution number, run affordance, cell selection, and drag/reorder handle. A thin accent rule identifies the selected cell. Editor focus is distinct from cell selection. Cell-type labels and secondary actions are subdued; show actions on focus/selection as well as hover so keyboard and touch access remain possible.

Keep code, outputs, and their provenance together. Display a concise `Edited since run` indicator with a Compare action. A cell's last execution time/duration is available without making every header a row of badges. Saving status belongs near the document title or status area, with per-cell failures only where relevant.

Provide visible Add cell insertion affordances between cells and a compact end-of-document control. Code/Markdown/Raw choices should not depend on the outline being open. Support multiple-cell selection, move, duplicate, split/merge, and structural undo according to existing requirements and actual source-service capabilities. Reordering must have a keyboard alternative to dragging.

Preserve Ctrl+Enter, Shift+Enter, Alt+Enter, edit/command mode precedence, IME composition, native selection, undo/redo, find, and indentation. Run saves acknowledged source first; failed/conflicted saves block execution with a local corrective action. Reveal and focus newly added cells and search targets, including below large outputs. Virtualization must not drop caret, selection, composition, or editor undo state.

**F-RD-07 — Output design.** Render streams compactly with copy, find, collapse, and output-size controls. Show result values without unnecessary containers. Errors start with a readable exception and relevant user-source location; expandable details contain the full traceback and internal runtime frames. Never discard recorded error information. Tables need stable headers, bounded rendering, useful column sizing, keyboard navigation, and a plain-text fallback. Images need fit/actual-size controls and a bounded cache. Unsupported MIME shows an honest fallback/export action; arbitrary HTML/JavaScript is not executed. Cover stdin, secret input, ownership conflicts, truncation, missing blobs, and stale outputs.

## 6. Primary and secondary surfaces

**F-RD-08 — Purpose-built surfaces.**

| Surface | New composition | Required interactions and states |
| --- | --- | --- |
| Welcome | Small product title, clear Create/Open actions, recent notebooks, optional sample; concise explanation of local files | No CLI needed; keyboard open, missing recent paths, empty directory initialization, invalid notebook, permission error, loading, sample completion |
| Notebook | Continuous document, optional outline, concise execution controls | Editing, saving, stale outputs, external change, conflicts, draft recovery, busy runtime, input request, selection and focus |
| History | Filterable execution list with date/session grouping and a purposeful details area | Outcome/date/cell/session controls, filter chips, advanced query option, pagination, zero results, source/output detail, compare current source, replay preflight |
| Checkpoints | List with useful name/time/type/coverage; details for saved state and omissions | Create from page/empty state, namespace vs training distinction, progress, cancel where supported, report, restore/continue preflight, success link, failure recovery |
| Runtime popover | Interpreter, notebook runtime status, current work, a few relevant actions | Start/run, input needed, interrupt, confirmed restart/stop, exact unavailable reason, link to all runtimes |
| All runtimes | Readable notebook names, interpreter, activity, attached clients | Current vs other notebook, open notebook, stop the correct target, partial discovery, stale registry, offline inspection |
| Diagnostics | Plain-language issue summary, recommended action, expandable technical details | Reconnect/reload, copy redacted diagnostics, operation reconciliation, doctor findings, migration with backup; no false green success |
| Settings | Categorized forms with current values and explanatory labels | Appearance/density/editor size, interpreter, shortcuts where implemented, output/resource preferences; validation and persistence feedback |
| Help | Searchable task guide with real shortcut bindings, glossary, About | First run, checkpoints, recovery, keyboard map, actual capability limits, version/copy information; readable text measure |
| Commands | Small searchable command dialog grouped by task and context | Useful common actions first, clear current selection, shortcuts, unavailable reasons, no-results state, focus return and accessible rows |

History and checkpoint rows prioritize human-readable cell/notebook labels; long identifiers and full paths belong in details with Copy. Prefer compact back navigation/breadcrumbs that preserve list context. Never mix a historical execution outcome with the current live worker state.

## 7. State model and trustworthy feedback

**F-RD-09 — Separate state dimensions.** Present connection, worker activity, source save state, and operation outcome independently. Build one typed presentation selector for toolbar, palette, context menus, and page actions so they cannot disagree.

| Situation | User-facing behavior |
| --- | --- |
| Notebook open, no worker | `Not running`; Run starts it. Source/history remain available. |
| Connected and idle | `Ready`; explain interpreter on demand. Checkpoint availability depends on capability and state, not the current page. |
| Starting/connecting | Name the stage; show liveness. Keep editing available. |
| Running/queued | Show active cell/work and elapsed time; keep Interrupt reachable; explain queued work. |
| Waiting for input | Reveal a labeled input request with ownership and secret-input treatment. |
| Connection lost | `Connection lost — execution state unknown`; offer Reconnect and preserve drafts. Do not declare a running job failed. |
| Explicitly stopped | `Stopped`; clear stale running labels. Next Run can restart. |
| Operation accepted, result unknown | Reconcile by durable operation id; retries do not duplicate work. |
| Operation finished with code error | `Execution failed` for code; technical operation completion is secondary. |
| Save failure or external conflict | Local explanation plus retry/compare/recovery; show that edits are retained. |

**F-RD-10 — Feedback placement.** Inline feedback for local problems, a quiet status area for ongoing work, a persistent banner only for workspace-level attention, and transient notifications for completed reversible actions. A successful checkpoint includes View checkpoint; creation refreshes the list. Reconnection has an observable result or a specific next step. Persistent errors survive until resolved or explicitly dismissed; dismissal does not falsify the underlying status.

Confirmations name the action and target, explain consequences in ordinary language, and start focus on the safe choice. Prefer Undo for reversible edits. Never put internal transport terms, undefined abbreviations, or source paths in routine user-facing error copy. Technical details remain available separately.

## 8. Checkpoints, provenance, interchange, and advanced capabilities

**F-RD-11 — Checkpoint task flow.** Create → progress → saved checkpoint → coverage report → restore/continue preflight → confirmation → new session/result. Offer creation where users look for saved state. Differentiate namespace snapshots, portable training checkpoints, and native process snapshots. Avoid language suggesting complete process recovery when only supported values were saved.

Preflight presents provenance, source relationship, included/excluded values, environment compatibility, and impact on the current worker. Trust controls appear only when the operation requires that decision and provenance can be established; never infer trust just because a path is local. If existing contracts cannot establish the distinction, retain conservative trust confirmation and record a runtime dependency. Version overrides require an actual mismatch. Never hide a required safeguard for aesthetic simplicity.

**F-RD-12 — Advanced features.** Training actions live in the checkpoint type selector and contextual commands when supported, with an Experimental label and accurate scope. They do not dominate the standard notebook toolbar. Unsupported native capture is explained through capabilities, never shown as functioning. Respect runtime v5 section 24 and its candidate-worker failure behavior.

Carry forward import/export/bundle, Git/worktree change, interpreter, multi-runtime, and draft/conflict requirements. Design import review, provenance/trust, source comparisons, merge conflicts, migration, and export progress even if implementation follows the core slice. Clearly label or omit unimplemented actions. No fake controls in a shipped UI.

## 9. Accessibility and input

**F-RD-13 — Complete interaction semantics.** Every interactive element has an accessible role, name, state, and stable identity: outline rows, palette results, search-clear controls, gutter actions, splitters, filter chips, tables, and dialogs. Expose selection and disabled explanations. Dialog focus is trapped and returned; background content is not offered as the active interaction context. Narrate status changes sparingly without flooding speech with stream output.

Verify keyboard-only traversal and NVDA with actual tasks, not only a UIA element count. Test high contrast, reduced motion, text zoom, keyboard pane resizing, tab order, focus visibility, non-color outcome cues, touch targets, IME, and non-ASCII paths. Hover-revealed actions must appear on keyboard focus. Preserve familiar native text editing behavior.

## 10. Implementation architecture and contract dependencies

**F-RD-14 — Build composition, not a second monolith.** Split the current large UI module incrementally into `theme`, `components`, `shell`, `notebook`, `history`, `checkpoints`, `runtime`, `settings`, `diagnostics`, and `accessibility` modules. Proposed names are organization guidance, not new crates by default. Keep the client/models crates independent of GPUI. Views render typed state and dispatch existing commands; they do not write notebook files or own transport retries.

Shared components: labeled icon button, menu, segmented destination control, filter bar/chip, empty/loading/error state, inline notice, status indicator, list row, splitter, source diff, and confirmation dialog. Specify all states before spreading a component across screens. Use Kit primitives where they meet the design and semantics; wrap or extend deliberately when they do not.

**F-RD-15 — Dependency ledger.** Verify each feature against the runtime contract before implementation. This frontend draft creates no protocol or persistence guarantees.

| Area | Plan |
| --- | --- |
| Visual shell, tokens, panes, labels, existing filters | Frontend work using existing actions; no runtime format changes intended |
| Global action availability | Centralize existing model/capability checks; verify freshness and page-independent dispatch |
| Trust/provenance preflight | Inspect existing helper fields; missing authoritative data is an explicit runtime-plan prerequisite |
| Structural undo | Verify inverse mutations preserve ids, order, kind, source, metadata and revisions; do not claim it complete without runtime-backed tests |
| Checkpoint naming, pin/publish, interchange | Verify supported contracts; unsupported persistence/actions require a runtime change, not a local UI-only invention |
| Large history/output | Existing coverage lists pagination/large-response gaps; baseline UI cannot certify scale until those contracts are ready |
| Reconnection/subscriptions | Existing polling/feed integration gaps need client/runtime evidence; cosmetic status changes do not solve them |

Keep existing persisted notebooks/history unchanged. Version local UI preferences and migrate safely. Clamp stale pane sizes, preserve appearance/recents/drafts, and avoid resetting preferences during redesign. A temporary internal switch may retain the old view during development; both use the same client/model paths. Remove duplicate UI paths after parity and rollback validation.

## 11. Phased execution and exit gates

**F-RD-16 — Delivery sequence.** Estimate after the first native slice; do not substitute an arbitrary deadline for the qualification gates.

| Phase | Work and deliverables | Exit evidence |
| --- | --- | --- |
| R0: Design baseline | Inventory every current action/state; capture reference fixtures; finalize tokens, shell, notebook, History, checkpoint preflight and compact layouts; create a visual prototype | Coverage checklist, explicit runtime dependency list, native font/layout spike, visual direction checked against the user's rejection of the existing design |
| R1: Foundations and shell | Typed tokens/components, theme variants, new navigation, constrained split panes, command semantics, preferences migration | Native screenshots and keyboard/UIA checks; repeated compact↔wide resizing; no unreachable controls |
| R2: Notebook vertical slice | Continuous cells, editor integration, insertion, save/run state, text/results/errors, stale compare, input | Create → edit → run → error → correct → rerun → reopen; real runtime; focus/IME/virtualization checks; first formative user study |
| R3: History and checkpoints | Filters, list/detail navigation, compare, snapshot creation/report/restore/continue, provenance preflight | Recorded source is distinct from current source; end-to-end checkpoint restoration; correct trust/capability gates and saved navigation context |
| R4: Supporting workflows | Runtime manager, Diagnostics/recovery, Settings/Help, interchange/Git conflicts, structural undo as supported, advanced training presentation | Workflow parity matrix with no silent omissions; explicit blocked dependencies; accurate failure/recovery messages |
| R5: Qualification and release | Native visuals, accessibility, scale/performance, clean installation, theme/DPI matrix, regression closure, docs and old-view retirement | Existing F0/F6 and F-QA release gates satisfied; no P1s; measured results and limitations published |

For R0–R2 review real populated screens, not just empty-state mockups. Include long notebook names, mixed cell types, long code, many executions, failure states, disabled commands, multi-client prompts, and multiline paths. Refine the component system before expanding it to every page.

## 12. Verification plan

**F-RD-17 — Behavioral gates.** Reuse existing tests and add focused regressions for changed behavior. Pure visual token edits do not need tests that merely restate constants. Model tests validate state and capability decisions; integration tests validate source/operation guarantees; GPUI/OS tests validate interaction; screenshots validate composition. None replaces the others.

| Scenario | Acceptance |
| --- | --- |
| FS-30 | All destinations at 800×600, 1024×768, 1280×850, and 1440×900 logical sizes; no clipped primary control; repeat shrink/expand at least ten times, with user-collapsed and user-resized panes |
| FS-31 | Light/dark/system/high contrast, all DPI levels above, mixed-monitor move and increased text size: legible content, stable alignment, reachable dialogs |
| FS-32 | Create/sample/open, edit, save, all three run shortcuts, insert/reorder/delete/undo where implemented, reopen: source and focus are correct |
| FS-33 | Long stream/table/image, error, stdin/secret input, stale output and source comparison: bounded rendering and intact recorded data |
| FS-34 | Search/filter history, open details, back, change destinations: filters, selection, page and scroll restore to the right surface |
| FS-35 | Create from Checkpoints, inspect coverage, restore/continue/replay with applicable preflight: accurate result, no silent namespace loss or duplicate execution |
| FS-36 | Every shared action across toolbar/palette/context/page uses identical capability/state logic and truthful disabled reasons |
| FS-37 | Disconnect during save/run, reconnect, stop from another client, failed interpreter, failed save: drafts survive and durable operations reconcile |
| FS-38 | Keyboard and NVDA complete open→edit→run→history→checkpoint→restore; announce palette/outline selection and every repeated control clearly |
| FS-39 | External source edits, Git changes, uninitialized directory, unsupported schema, migration, imported bundle trust: accurate context and recoverable decisions |
| FS-40 | Performance fixtures: 1,000 cells, large history, 100 MiB output, images, flood; record frame time, memory, responsiveness and recovery against existing F-PERF budgets |
| FS-41 | Preference migration and packaged clean-machine launch preserve drafts/data, use the correct runtime, and pass existing platform/release gates |

**F-RD-18 — Visual quality gate.** A separate visual review must check hierarchy, proportion, typography, density, alignment, restraint, and coherence. Every core screen must look designed with real content at normal size. No clipped text, accidental overflow, placeholder-looking controls, excessive card treatment, ornamental badges, or inconsistent spacing. The notebook should dominate its workspace. A passing functional checklist does not satisfy this gate.

Target a fresh expert review of at least 8/10 for visual quality and core task UX, while retaining the current formal release rubric (at least 80/100, at least 90% weighted coverage, no P1, verified keyboard/screen-reader/recovery journeys). These are targets, not forecast results; the 6.6/10 review uses a different heuristic rubric and must not be presented as the formal release score. Conduct the existing five-to-eight-person formative/summative task studies; report actual task success, errors, time and SUS. Include the user's visual judgment in direction review rather than replacing it with a computed average.

## 13. Priorities, risks, and decisions

**F-RD-19 — Priority order.** First fix composition, resizing, accessible semantics, and action truthfulness. Next complete editing/output and checkpoint/history journeys. Then supporting workflows and visual polish. Animation and decorative refinement never precede correctness, readability, or task reachability.

Risks: GPUI primitive behavior may constrain styling or accessible semantics; editor virtualization can regress input; pane persistence can recreate the resize defect; hiding advanced actions can harm discoverability; backend gaps can leave polished but incomplete workflows. Address these through the R0/R2 native slices and the contract ledger. A toolkit rewrite is not assumed; revisit it only with measured blockers against the existing feasibility requirements.

| Decision | Working choice | Resolve by |
| --- | --- | --- |
| FD-20 | Quiet neutral workbench, muted blue accent, system typography, light/dark parity | R0 populated visual review |
| FD-21 | Workspace header with three primary destinations; contextual outline; runtime popover | R1 compact/keyboard validation |
| FD-22 | Continuous notebook canvas with gutter actions and optional detail inspector | R2 real editor and output slice |
| FD-23 | Scope/contract readiness of structural undo, provenance preflight, interchange and scale | R0 dependency inventory; any runtime-plan changes precede dependent shipping |

**F-RD-20 — Completion definition.** The redesign is complete when the native application implements the approved composition across every supported surface and state; core and supporting journeys are accounted for; relevant existing guarantees still hold; accessibility/performance/platform evidence passes; new screenshots and the scorecard are published; and no mock, hidden failure, unsupported action, or untested gate is represented as complete.

## 14. Proposed changes from frontend v4

Replace the visual shell, cell-card styling, navigation composition and broad runtime toolbar with the workbench design above. Add explicit tokens, contextual pane rules, resize-transition acceptance, page-independent action selectors, native accessibility coverage, and visual quality gates. Redesign every existing surface rather than stopping at Notebook. Retain source/history/runtime separation, offline access, trust and recovery guarantees, experimental training boundaries, performance targets, and packaging requirements. New runtime-dependent capabilities remain explicitly gated. No frontend requirement is withdrawn merely because its implementation is unfinished.
