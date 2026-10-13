# KernelLedger frontend v6: clarity, control and responsive craft

| Item | Decision |
| --- | --- |
| Version | Frontend 6 proposal |
| Date | 2026-10-09 |
| Status | Draft; contract-independent implementation landed in the working tree; full qualification pending |
| Implementation | [v6 implementation and qualification ledger](../docs/desktop/frontend-v6.md) |
| Published baseline | [Frontend v4](../FRONTEND_PLAN.md) |
| Implementation baseline | [v5 draft](frontend-v5-draft.md) and [qualification ledger](../docs/desktop/frontend-v5.md) |
| Requires | [Runtime v5](../PLAN.md), including the published frontend baseline's dependencies and section 24; additional contracts below must be resolved before dependent shipping |
| Assessment | [Live native review: 4.9/10 overall, 4/10 visual](../docs/desktop/apple-design-live-review-2026-10-09.md); replaces the preliminary 6.1/10 judgment |
| Stack | Rust, GPUI, GPUI Kit; retain independent client/models and Python runtime |
| Direction | A quiet native notebook: clear purpose, reversible work, immediate feedback and disciplined composition |

## 1. Scope and relationship to existing plans

This is a new frontend proposal informed by the current v5 implementation, not a stack rewrite. It inherits published v4 requirements and v5 proposals unless explicitly changed below. Existing qualification gaps remain obligations. The v5 draft and uncommitted implementation remain intact. Publication must follow [plan versioning](README.md), archive the then-current plan, reconcile pending v5 changes and update the authoritative header/history. This draft does not publish v5 or v6 as Current.

New proposed requirement ids are F-APL-01–13; scenarios FS-42–50; decisions FD-24–26. No old identifier is reused or withdrawn. Runtime behavior, persisted source formats and protocol messages belong to the runtime plan. A frontend proposal cannot silently add their guarantees.

## 2. Intended experience

The user opens a notebook and immediately sees its name, location in the app, live runtime status and source. Run selected is the clearest notebook action. Editing does not cause surrounding controls to jump. A result appears next to its source; an error explains its cause and offers nearby traceback details. Creating a checkpoint ends at a readable coverage summary. Returning from History restores the notebook position and caret. Routine structural changes become reversible once a verified service contract supports them.

Use the apple-design skill's eight principles as review questions: Does this serve the task? Can the user recover? Is it truthful? Is it familiar on Windows? Does it adapt? Is the common path clear? Are the details deliberate? Does the experience leave the user calm and confident? Translate the skill to native GPUI behavior; CSS, browser APIs and web spring packages are examples, not dependencies for this app.

## 3. Workspace and hierarchy

**F-APL-01 — Fewer, clearer global controls.** Keep a 44-logical-pixel workspace header with a visibly disclosed notebook switcher, Notebook / History / Checkpoints destinations, Commands, live runtime access and an application menu. Keep a 36–40 px notebook action strip. Give Run selected a restrained primary treatment; Run all is secondary. Interrupt stays in a predictable position and is enabled/explained by the shared runtime selector. Label Run as Run selected where width permits. A single Add cell disclosure defaults to code and offers Markdown/Raw. Remove the outline-footer insertion triplet. Preserve all existing actions in contextual menus/Commands.

At narrow widths, shorten the notebook name first, combine nonessential utility entries into an explicitly labeled menu next, and collapse the outline when needed. Preserve destinations, Run and Interrupt access. Choose cutoffs from measured text/control widths rather than assuming a device class. No overlapping or silently truncated primary action at 800×600 logical size with increased text.

**F-APL-02 — Contextual actions with stable geometry.** Each inter-cell boundary has one insertion affordance, revealing its action on hover, keyboard focus, or touch interaction; Commands and Add cell provide permanent alternatives. Its hit region stays allocated so disclosure causes no scroll/caret jump. Selected/focused code cells expose Run and a More menu. Markdown exposes edit/preview and More; Raw exposes editing and More. Output controls appear only when output exists. Duplicate, move and delete remain discoverable in More and Commands, with contextual accessible names. Never make a hover-only action unreachable by keyboard or speech.

**F-APL-03 — Source and result read as one unit.** Align ordinal, source, output and error to a shared content grid. A code cell header is approximately 28–32 px in compact density. Use a source height that fits short content rather than a generous fixed line floor; retain a usable empty-cell target and cap long source with deliberate scrolling. Validate multiline paste, IME and virtualized height changes before adopting the calculation. Keep 12–16 px between source and result, and 16–24 px between cells; tune in native fixtures. Offer width limits for prose, while code/tables may use available width. Replace `ok` with Succeeded and `error` with Failed where an outcome label is necessary. Put Show traceback next to the error summary, left aligned and sized to its label.

## 4. Foundations and materials

**F-APL-04 — Semantic roles and scalable typography.** Extend the shared token layer with canvas, chrome, overlay, divider-subtle, control-border, focus-ring, selected-cell, selected-row and error-text roles. Decorative separators can be quieter than interactive boundaries. Never weaken focus or high-contrast boundaries to obtain a cleaner screenshot. Audit semantic/syntax colors supplied by GPUI Kit as well as app tokens.

Starting type scale: metadata 12 px, UI/body 14 px, page title 18–20 px, document heading 26–30 px; code uses the existing adjustable starting size. Use native system UI and an installed monospace fallback. Differentiate hierarchy through weight, spacing and leading; tighten large headings only where native font support permits it. Introduce a shared UI text-scale setting or honor the effective OS text scale, and scale spacing/control heights with it. Code font size alone is insufficient. Keep source data and code text unchanged.

Normal text must meet 4.5:1; large text and meaningful boundaries 3:1. Review syntax, disabled explanations, selected content and focused controls separately. Light/dark/high contrast have equivalent hierarchy. In high contrast, functional boundaries remain explicit. Surfaces stay solid by default. Reserve elevation for menus, dialogs and actual overlays. Any optional translucency requires supported native composition, solid fallbacks and measured legibility; it is not a prerequisite.

## 5. Feedback and motion

**F-APL-05 — Overlays belong to their trigger.** Place notebook switcher content under its trigger; runtime and application popovers under theirs, clamped to the usable window. Track trigger geometry after text-scale and window changes. Distinguish menu semantics from destructive confirmation dialogs. Preserve Escape, outside-click dismissal where appropriate, and focus return. A blocking confirmation dims the workspace and traps focus; a contextual nonblocking panel preserves parallel work. Validate toolkit menu behavior before replacing the current occluding dialog wrapper.

**F-APL-06 — Immediate, truthful feedback.** All shared buttons show visible pointer-down and keyboard-press feedback immediately, and a drag away cancels activation according to native control conventions. Save, connection, worker activity and execution outcome remain separate. Show Saving/Saved beside the relevant source state; show running cell and elapsed time with Interrupt; show queued work without pretending it has started. Connection lost means execution state unknown. Avoid transport names and ids in routine copy. Copy and successful reversible actions receive a short acknowledgment without a modal. Checkpoint success links directly to its saved result. Persistent failures include a concrete next action and keep drafts intact.

**F-APL-11 — Motion communicates state and remains interruptible.** Start with press feedback and correct anchored placement. Small opacity/color transitions may settle in 80–120 ms; users never wait for an animation before acting. If a panel moves, use a native spring retargeted from its presented position and current velocity; start with no overshoot and roughly 0.3–0.4 s response. Entry and exit use the same spatial path. New input cancels or redirects pending motion without a jump. Workspace switching and typing must stay responsive.

Do not add cell reordering gestures solely to satisfy a motion checklist. If later added, require pointer capture, original grab offset, continuous tracking, release-velocity handoff, projected snap targets, soft bounds, keyboard equivalents and auto-scroll. Keep edit/IME selection distinct from dragging. Reduced motion uses instant state changes or short fades, without sliding or bounce. Read supported OS preferences and retain an app override; unsupported detection must be documented. No sound/haptics are required. Profile any animation on actual GPUI rendering; web examples are not assumed to port directly.

## 6. Editing, recovery and saved state

**F-APL-07 — Forgiveness backed by the source service.** Before offering structural Undo/Redo, verify atomic inverse operations preserve stable cell ids, order, kind, source, metadata, attachments and revision/conflict rules for add/delete/duplicate/move/split/merge. Keep editor text undo distinct from structural history. On successful undo, reveal the affected cell and restore usable focus. Do not implement this by rewriting source files from a view. Until the contract is verified, preserve honest confirmations and do not advertise unsupported Undo. Later add multiple-cell selection with explicit count and predictable keyboard range selection.

**F-APL-08 — Checkpoint decisions in ordinary language.** The initial report shows checkpoint type, recorded outcome, saved coverage, excluded values with reasons, recorded creation context when available, and the primary next action. Replace raw exclusion JSON with labeled rows; Raw details and Copy remain available. If the helper supplies no reliable count or field, show Unknown rather than deriving a confident value from incomplete data.

Before Restore/Continue, summarize affected notebook, namespace replacement versus new session, excluded values, recorded source relationship, available environment checks and trust requirements. Distinguish recorded facts from checks not established. Keep conservative trust behavior until authoritative runtime fields exist. Override Python mismatch only after an actual mismatch result. Experimental training/native capabilities retain existing availability and labels. Successful restoration names the new session and reveals a safe next action; failure preserves the current worker per the existing runtime contract.

**F-APL-09 — History follows human tasks.** Provide outcome, session, cell and date controls alongside text search. Keep advanced query syntax as an optional path. List rows prioritize recognizable cell titles, outcome, time and duration; identifiers are secondary/copyable. Compare recorded source with current source and label each explicitly. Preserve filter, cursor, selection and scroll independently for each destination/notebook. Return to notebook restores its active cell and caret where valid. Group by session/date only with correct bounded loading; do not misrepresent partial pages as complete groups.

## 7. Accessibility, responsiveness and supporting surfaces

**F-APL-10 — Whole journeys remain accessible.** Shared controls, menus, insertion affordances, output actions, outline rows, splitters and tables expose correct roles/names/states and distinct cell context. All actions have keyboard access. Hover actions also appear on focus. Selection, editor focus and keyboard focus remain distinguishable. Announce meaningful state changes without narrating every streamed output chunk. Verify NVDA open → edit → run → inspect output → history → checkpoint → restore with actual task completion.

Validate logical windows 800×600, 1024×768, 1280×850 and 1440×900; light/dark/high contrast; 100/125/150/200% DPI; mixed-monitor changes; 100/125/150/200% UI text scale. At increased text scale, menus/popovers may scroll, utilities may consolidate and panes may collapse, but text and core actions remain reachable. Compact pointer controls retain v5's minimum target baseline; comfortable/touch mode uses at least 44×44 logical px for primary touch actions. Test real Windows IME and non-ASCII paths. Check the effective client viewport, not only outer-window dimensions.

**F-APL-12 — Complete the application composition.** Apply the same tokens, action hierarchy and state language to Welcome, runtime management, Settings, Help, Diagnostics and interchange. Welcome offers real open/create/import entry points with truthful availability. Settings groups appearance, typography, layout and accessibility with persistence feedback. Diagnostics shows actionable findings first, redacted copy and optional technical details. Runtime management names notebook/interpreter and explains unknown state. Import review separates source, recorded output and trust before commitment; unsupported actions stay explicitly unavailable. Supporting workflows retain the baseline's guarantees and release gates.

**F-APL-13 — Purpose-built native views and interaction cleanup.** The live review shows that shared generic layout is not producing acceptable screen composition. Extract dedicated History list/detail and checkpoint list/report compositions from `surfaces.rs`. Shared buttons must not stretch merely because their parent is a vertical flex: action rows own alignment, content-sized actions own width, and full-width controls require an explicit purpose. Put History filters in list context; detail uses compact breadcrumb/context and reveals current-source comparison when changed or requested rather than duplicating unchanged source by default. Render empty checkpoint arrays as ordinary text, and move training actions behind type selection/advanced disclosure. Group Settings into a bounded form with explicit appearance/density/size choices; expose interpreter path in details with Copy. Commands uses bounded result scrolling and a small content-sized footer.

Unify history placeholder/help/parser vocabulary. Scope command/edit mode to Notebook. Clear tooltip/hover ownership when changing destination, opening/closing overlays or unmounting virtualized controls. Expose actual disabled state to native accessibility APIs; a disabled-looking button reporting enabled is a defect even if dispatch remains guarded. Native state verification must cover Interrupt while idle, Run on Markdown and Restore before trust consent. Complete these changes in the first native slice before propagating more generic UI to other pages.

## 8. Implementation sequence

| Phase | Priority and work | Owning modules | Exit evidence |
| --- | --- | --- | --- |
| A0: Native design slice | P0: use the live review captures as baseline; inventory states; prototype header, selected code/Markdown, output/error, anchored menu, dedicated History detail and checkpoint summary | theme, components, shell, notebook, outputs, surfaces | Actual native light/dark/contrast views and interactive review at compact/wide widths; no lost actions; explicit judgment of composition and density |
| A1: Composition and scaling | P1: F-APL-01–05/13, shared tokens, adaptive shell, contextual insertion, compact cells, dedicated layouts, tooltip lifecycle, disabled semantics and UI scaling | theme, components, shell, notebook, settings, accessibility, surfaces | FS-42–44/50 pass; selection/caret/IME regression checks; baseline workflow parity |
| A2: Feedback and recovery | P1: F-APL-06–07, acknowledgments, source recovery, structural undo contract investigation | client/models, notebook, accessibility | FS-45–46; source-service integration evidence; blocked contracts named explicitly |
| A3: History and checkpoint comprehension | P1: F-APL-08–09, structured filters, readable coverage, safe restore preflight | history, checkpoints, surfaces, client/models | FS-47–48 with real runtime; all unknown/missing fields treated honestly |
| A4: Interaction and full-surface polish | P2: F-APL-10–12, press behavior, anchored menu semantics, accessible shared components and supporting screens | accessibility, shell, runtime, diagnostics, interchange, help | FS-49 plus populated captures of every destination; motion reversal/preference recordings only where applicable |
| A5: Qualification | Complete inherited v4/v5 performance, recovery, accessibility, packaging and participant gates | whole app | Published measured evidence, independent visual/UX review, no unresolved release blockers |

Estimate delivery after A0's native spike and the contract inventory. A0/A1 can proceed while recovery/preflight contracts are investigated. A2/A3 features dependent on missing contracts cannot be claimed complete because their UI is finished.

## 9. Acceptance scenarios

| ID | Scenario and pass condition |
| --- | --- |
| FS-42 | Populated notebook has one global Add cell entry, one contextual insertion entry per boundary, no outline insertion triplet, relevant selected-cell actions, an obvious Run action, and no lost Commands/menu actions. Native compact/wide captures establish improved hierarchy. |
| FS-43 | Two-line code and a short result occupy content-appropriate space; multiline paste/IME and virtualization preserve selection, source and scrolling. Keyboard disclosure of actions causes no caret or layout jump. |
| FS-44 | Width/theme/DPI/text-scale matrix from F-APL-10 keeps all core controls and dialogs reachable. Ten shrink/expand cycles preserve manual outline collapse/size; real client bounds are recorded. Syntax/focus/disabled and semantic-text contrast pass separately. |
| FS-45 | Press and drag-away cancellation work for pointer and keyboard; save/run/copy/checkpoint feedback is observable. Disconnect during an operation preserves drafts and reports unknown execution state; reconnection reconciles durable ids without duplicate work. |
| FS-46 | Service-backed structural Undo/Redo, if shipping, restores exact stable ids/source/metadata/attachments/order, respects external revision conflicts and retains editor focus. No failed mutation enters successful undo history. Unimplemented undo remains honestly unavailable. |
| FS-47 | Common history filters work without query syntax. Open recorded details → compare → return restores the right notebook/cell/caret and history filter/page/scroll, including offline inspection and partial pages. |
| FS-48 | Checkpoint report presents readable coverage and exclusions. Restore/Continue states consequences, trust and unknown checks accurately. Success identifies the result; failure preserves runtime guarantees. No missing metadata is presented as verified compatibility. |
| FS-49 | Real NVDA/keyboard completes the core journey; repeated controls identify their cell. Menus anchor to the right trigger at all sizes, Escape returns focus, reduced-motion behavior works, and moving panels can be reversed without blocking or jumping. Instant panels do not need a fabricated animation test. |
| FS-50 | With populated native fixtures, History details avoid redundant unchanged-source blocks and list filters; empty checkpoint exclusions/notes read as text; utility buttons are content-sized; Settings uses coherent groups; Commands has no oversized blank footer. Park the pointer over a notebook output action, navigate through Commands to Help/Diagnostics, and verify the old tooltip disappears. UIA reports disabled for idle Interrupt, Markdown Run and trust-gated Restore. Filter placeholder/help/parser agree and non-Notebook pages omit stale editor-mode status. |

These scenarios supplement FS-30–41 and existing platform/release gates; they do not replace them. Add meaningful automated regressions for behavior changes, not tests that merely restate visual constants. Native images assess composition; OS tests assess input/accessibility; service tests assess data integrity.

## 10. Dependencies, risks and decisions

| Area | Dependency and action |
| --- | --- |
| Structural undo | Audit existing source mutations; missing inverse/revision/attachment guarantees require a runtime-plan change and tested service implementation before shipping. |
| Checkpoint preflight | Present current coverage now; authoritative provenance/source/environment checks require documented runtime fields. Keep conservative confirmations. |
| History/output scale | Retain v5's bounded query, byte-range and client cache obligations. Filtering visible records does not certify large-history completeness. |
| Overlay semantics | GPUI Kit primitives may need adaptation; measure focus, trigger geometry and accessibility before introducing motion. |
| Typography/accessibility | Native text-scale/OS preference support requires a platform spike; editor-size preference does not prove whole-app support. |
| Capture/qualification | Existing screenshots are limited notebook fixtures. Fresh native captures of every surface and real participant/platform runs remain required. |

| Decision | Working choice | Resolve by |
| --- | --- | --- |
| FD-24 | Keep the v5 shell direction; consolidate insertion and introduce relevance-based cell actions, rather than reintroducing a navigation rail | A0 populated native comparison |
| FD-25 | Solid materials and minimal state transitions; springs only for meaningful moving panels/gestures | A4 platform/interaction evidence |
| FD-26 | Implement contract-independent clarity first; independently gate undo, authoritative preflight and scale | A0 dependency inventory and A2/A3 exit review |

## 11. Success and proposed changes from v5

Target at least **8/10** on the same eight-principle rubric, with no principle below 7, plus the inherited formal release score, weighted coverage and no-P1 requirements. Keep rubric weights stable for comparisons. Re-score after implementation using fresh evidence; 8/10 is a target, not a forecast. Run the inherited five-to-eight-participant studies, record task success/time/errors and SUS, and review the user's judgment of the actual native composition. Do not translate a heuristic score into participant satisfaction.

Compared with v5, this proposal consolidates insertion, makes cell actions relevant, tightens short-cell composition, separates decorative/functional tokens, scales the whole UI, anchors overlays, defines immediate/interruptible feedback, improves checkpoint/history comprehension and provides explicit verification for recovery and accessibility. It retains the stack, source/runtime separation, conservative trust and experimental capability boundaries. Completion means implemented native behavior plus validated core/supporting journeys and inherited performance/platform/release gates; a plan, concept or passing screenshot check alone is insufficient.
