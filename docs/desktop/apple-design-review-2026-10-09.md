# Apple-design UI/UX assessment

**Superseded assessment:** After running and interacting with the native app, the [live review](apple-design-live-review-2026-10-09.md) revises the overall judgment to 4.9/10 and visual quality to 4/10. The preliminary 6.1/10 below remains historical evidence, not the current rating.

Date: 2026-10-09. Scope: KernelLedger's current, uncommitted native GPUI frontend v5 implementation. Method: the repository's [apple-design skill](../../.agents/skills/apple-design/SKILL.md), applied to captured native views, source, and recorded qualification evidence.

## Verdict

**6.1/10, provisional expert assessment.** The app has a useful workbench structure and treats runtime truth seriously. It still feels assembled from controls rather than composed around editing and understanding results. The largest opportunity is clearer hierarchy and less repeated chrome, followed by stronger recovery and accessible interaction. Adding decorative motion or translucent surfaces would not resolve those problems.

This is a judgment rubric, not a standardized usability instrument, measured user satisfaction, or release certification. It is not directly comparable to the older 6.6/10 review or the formal F-QA score. Ratings estimate the current design from the evidence below; planned behavior earns no implementation credit.

## Evidence and limits

Visually inspected:

- [Light notebook, nominal 1440×900](../../artifacts/frontend-v5/native/notebook-light-1440x900.png).
- [Dark notebook, nominal 800×600](../../artifacts/frontend-v5/native/notebook-dark-800x600.png).
- [High-contrast notebook, nominal 1024×768](../../artifacts/frontend-v5/native/notebook-contrast-1024x768.png).

Source inspected: [theme](../../desktop/crates/ui/src/theme.rs), [shared controls](../../desktop/crates/ui/src/components.rs), [shell](../../desktop/crates/ui/src/shell.rs), [notebook](../../desktop/crates/ui/src/notebook.rs), [outputs](../../desktop/crates/ui/src/outputs.rs), [history filters](../../desktop/crates/ui/src/history.rs), [checkpoint report](../../desktop/crates/ui/src/checkpoints.rs), and supporting settings/accessibility/test code. The [qualification ledger](frontend-v5.md) establishes remaining workflow and platform gaps. The [native probe](../../artifacts/frontend-v5/native/probe-light.json) records 192 DPI, ten resize cycles and limited UIA checks; its window visibility measurement is not editor latency.

Screenshots are existing fixtures, not newly captured in this review. Their requested size names do not certify usable client area or absence of occlusion. The ledger says captures precede a subsequent cursor-navigation change. Only Notebook was visually reviewed; other surfaces were assessed from source and the ledger. No fresh application run, pointer-latency measurement, animation recording, NVDA, real IME, mixed-DPI, or participant study was performed. Static views cannot establish interruptibility or smoothness.

## Ratings against the eight principles

Scale: 1–3 obstructive; 4–5 substantial friction; 6–7 usable with visible weaknesses; 8–9 coherent and validated; 10 exceptional and repeatedly demonstrated. Weights reflect sustained notebook work. The weighted mean is 6.07, reported as 6.1; decimals are editorial estimates.

| Principle | Weight | Rating /10 | Current evidence and reason | Improvement |
| --- | ---: | ---: | --- | --- |
| Purpose | 12% | 7.0 | Notebook, History and Checkpoints match core tasks. Insertion actions and output toolbars consume attention repeatedly. | Give editing/results the largest share of the composition; reduce repeated controls. |
| Agency | 14% | 5.5 | Collapsible outline, editing shortcuts, source comparison and confirmations exist. Structural undo and multiple-cell operations remain open. | Complete source-backed recovery; preserve caret, selection and context across actions. |
| Responsibility | 12% | 7.0 | Unknown connection state and conservative checkpoint trust are explicit. Coverage is truthful, but much of the report is JSON and authoritative preflight is incomplete. | Translate coverage into readable decisions; expose exactly what replacement means and what remains unknown. |
| Familiarity | 10% | 7.5 | Native window frame, system typography, labeled destinations and standard run controls help orientation. Notebook switcher lacks a visible disclosure; unrelated menus share an overlay placement. | Use recognizable disclosures, trigger-anchored popovers, familiar keyboard behavior. |
| Flexibility | 12% | 6.5 | Theme variants, compact outline, density and editor-size settings exist. Full text scaling, real assistive technology and mixed-DPI qualification remain open. | Adapt layout to measured available width and text scale; qualify keyboard and NVDA tasks. |
| Simplicity | 14% | 5.0 | Flat cells are an improvement, but identical insertion triplets recur in the toolbar, outline footer and between cells. Selected Markdown exposes disabled Run and output controls. | One global Add cell action, contextual insertion, relevant cell actions and plain-language states. |
| Craft | 18% | 5.5 | Light/dark foundations and alignment are useful. Full-width rules, large minimum editor regions, bright semantic text and a centered traceback action weaken composition. | Separate decorative dividers from focus/control boundaries; tighten short cells; refine type and error placement. |
| Delight | 8% | 5.0 | Calm colors and quick task access provide some confidence. Recovery gaps and repetitive chrome interrupt that feeling. | Make edit → run → inspect → recover effortless; communicate success without interrupting work. |

## Findings by priority

| Finding | Severity | Evidence | User effect | Proposed requirement |
| --- | --- | --- | --- | --- |
| Repeated insertion controls compete with content | P1 design | Captured light view and shell/notebook composition | More scanning and less room for code/results | F-APL-01, F-APL-02 |
| Selected Markdown shows irrelevant Run/output actions | P2 | Notebook action construction and captured selected Markdown | Users must interpret disabled or irrelevant controls | F-APL-02 |
| Short source cells occupy generous minimum height | P2 | Captured two-line/one-line code; editor height uses `editor_lines` | Extra scrolling; harder comparison of adjacent cells | F-APL-03 |
| Structural recovery is incomplete | P1 workflow | v5 ledger: source-backed undo remains open | Confirmation replaces forgiveness for routine editing | F-APL-07 |
| Menus share top-right overlay placement | P2 | `menu_overlay` uses `justify_end`, including notebook switcher | Weak relationship between source and opened content | F-APL-05 |
| One border token serves many visual roles | P2 | Theme and component/shell usage | Decorative separators become as prominent as functional boundaries | F-APL-04 |
| Error and outcome presentation lack hierarchy | P2 | `ok`/`error` labels; bright error text; full-width traceback action | Outcome feels technical and controls feel detached from their content | F-APL-03, F-APL-06 |
| Checkpoint coverage relies on raw JSON | P1 comprehension | `checkpoint_report` renders exclusions/notes as JSON | Users must decode technical data before a consequential restore | F-APL-08 |
| History requires query syntax for common filters | P2 | `history_filters` help text | Higher discovery cost for dates, cells and sessions | F-APL-09 |
| Real accessibility and interaction qualification incomplete | P1 qualification | v5 ledger and probe limits | Keyboard/speech and input reliability cannot yet be certified | F-APL-10, F-APL-11 |

P1 here means high priority for the redesign; it does not assert a newly reproduced crash, data-loss incident, or formal release defect classification.

## Skill-specific interaction assessment

| Criterion | Assessment | Needed evidence |
| --- | --- | --- |
| Immediate response | Selection responds on mouse-down; theme includes active button colors. Actual primitive press behavior and latency are unmeasured. | Native press/cancel recording and event-to-paint timings. |
| Direct manipulation | Keyboard outline sizing and cell move actions exist. No complete drag interaction was demonstrated. | If added: pointer capture, grab offset, continuous tracking and keyboard parity. |
| Interruptibility, velocity, momentum | Not rated. Workspace transitions are currently described as instant; no gesture-driven spring is demonstrated. | Reversal recordings only for interactions that actually move. |
| Spatial consistency | Common menu placement weakens anchoring. | Trigger bounds, opening/closing path and focus return at each supported width. |
| Materials/depth | Solid surfaces suit a scientific desktop tool. The notebook does not need glass. | Clear overlay separation and contrast; compositor support before considering translucency. |
| Typography | System UI and monospace choices fit Windows; rendered hierarchy needs refinement. | Full UI scaling, syntax contrast and fallback-font review in native rendering. |
| Accessibility preferences | High contrast and a persisted reduced-motion option exist. Full behavior/OS-preference coverage is unverified. | OS and app preference matrix plus real NVDA/keyboard tasks. |
| Multimodal feedback | No sound/haptic requirement is justified for current desktop tasks. | Consider only if a demonstrated user need warrants it. |

## Recommended direction

Retain GPUI and the three primary destinations. Compose a quieter document workspace: one obvious run action, one global insertion entry, a readable outline, compact source/output pairs, and overlays attached to their triggers. Make freshness, saving and execution states separate but easy to understand. Complete recovery and checkpoint comprehension before expanding animation. The [frontend v6 draft](../../plans/frontend-v6-draft.md) turns these findings into implementation phases and acceptance scenarios.
