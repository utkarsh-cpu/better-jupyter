# Live native UI/UX review

Date: 2026-10-09. This review replaces the preliminary [screenshot/source assessment](apple-design-review-2026-10-09.md) as the current judgment of the development frontend.

## Judgment

**Visual quality: 4/10. Overall apple-design assessment: 4.9/10, provisional.** The current interface needs a substantial composition redesign. Editing and execution have useful working paths, but large areas of repeated controls, stretched layouts and weak hierarchy make the app feel unfinished. The previous 6.1/10 was too generous because it credited implementation foundations without inspecting the full native experience.

## What actually ran

- Built the current working tree with `cargo build --locked -p kernelledger-desktop`; passed. No frontend implementation changes were made during this review.
- Launched the native executable on Windows with an isolated temporary notebook and isolated desktop profile. Existing user notebooks and preferences were not used.
- Fixture: five cells, Markdown, baseline calculation, successful stdout, a failed forecast, a report and a namespace checkpoint. Real Python supervisor/worker supplied the results.
- Native display: 2880×1800 physical pixels at 192 DPI. Initial measured client viewport: 2834×1679 physical pixels (1417×839.5 logical). Compact measured client viewport: 1600×1200 physical pixels (800×600 logical).
- Interacted through Windows UI Automation, a real pointer click into the editor, and native keyboard input. Screenshots use the Windows compositor with the app in the foreground. UIA names/bounds/state were recorded beside each capture.
- Inspected Notebook, notebook switcher, History/session/execution detail, Checkpoints/report/restore confirmation, Commands, Settings, compact outline, runtime menu, Help and Diagnostics. Switched from light to dark and resized compact/wide.
- Edited the baseline with a comment and ran it using Ctrl+Enter. The result stayed `Mean: 16.5`, the ordinal advanced to `[4]`, and the comment was verified in the saved source file. Typed Settings and Help into Commands and activated them with Enter. Escape canceled restore. Ran the notebook health check. No app stderr was recorded.

The scripts and captures are in [live review artifacts](../../artifacts/apple-design-live-2026-10-09/). This is a bounded interactive review, not a formal performance or accessibility study. Input-to-paint latency was not measured; the capture script's waits cannot establish it. No NVDA, actual IME, mixed monitors, long-stream stress, import/export, full checkpoint restore or participant study was performed.

## Live findings

| Finding | Evidence | Consequence | Frontend change |
| --- | --- | --- | --- |
| Notebook chrome consumes the document | [Notebook](../../artifacts/apple-design-live-2026-10-09/notebook-live.png) | Five short cells require substantial scrolling; insertion triplets and per-output actions repeat. The error summary sits at the viewport's lower edge. | Consolidate insertion; compact source/results; scope tools to the selected/focused output. |
| Controls span the page without a reason | [Checkpoint report](../../artifacts/apple-design-live-2026-10-09/checkpoint-report-live.png), [Settings](../../artifacts/apple-design-live-2026-10-09/settings-live.png) | Create checkpoint, Copy checkpoint identifier, High contrast and About read like stretched placeholder controls. | Content-sized actions with explicit alignment; set layout ownership per view. |
| Notebook switcher opens opposite its trigger | [Switcher](../../artifacts/apple-design-live-2026-10-09/switcher-live.png) | Left-side action opens a right-side panel; the connection is unclear. | Trigger-anchored native menu, visible disclosure and proper menu semantics. |
| History changes destination content while keeping search-heavy chrome | [Execution detail](../../artifacts/apple-design-live-2026-10-09/execution-detail-live.png) | Roughly the upper third is occupied before the recorded source. Two unchanged source blocks add further scanning. | Dedicated list/detail composition; compact breadcrumb/context; reveal comparison when changed or requested. |
| Search instructions disagree | [History list](../../artifacts/apple-design-live-2026-10-09/executions-live.png) | Help says `after:`/`before:` while placeholder says `from:`/`to:`. | One supported filter vocabulary, verified against the parser; ordinary date controls. |
| Checkpoint report displays empty JSON | [Report](../../artifacts/apple-design-live-2026-10-09/checkpoint-report-live.png) | Two broad code blocks show `[]`; routine saved-state information feels like debugging output. | Show No excluded values / No coverage notes; put raw data in optional details. |
| Experimental checkpoint actions compete with standard work | [Checkpoints](../../artifacts/apple-design-live-2026-10-09/checkpoints-live.png) | Training controls and a long explanatory sentence sit above the standard saved checkpoint. | A type choice or advanced disclosure; keep namespace create/inspect/restore primary. |
| Settings has inconsistent grouping and measure | [Settings](../../artifacts/apple-design-live-2026-10-09/settings-live.png) | Plain numeric choices, full-width rows and a long interpreter path have no consistent form composition. | Labeled grouped sections, explicit selections, bounded text measure and copy/details for paths. |
| Compact mode retains disproportionate cell spacing | [800×600 dark](../../artifacts/apple-design-live-2026-10-09/notebook-compact-dark-live.png) | A heading and one four-line code/result pair occupy nearly the whole document area. | Measure content density and reduce metadata/insertion/editor padding at compact size. |
| Disabled controls are exposed as enabled to UIA | Captured `*.uia.json`: gray Interrupt, Markdown Run and unchecked-trust Restore checkpoint all report `enabled: true` | Assistive clients receive state inconsistent with the rendered control. No unsupported action execution was tested. | Wire disabled state to native accessible semantics and verify both UIA and NVDA. |
| Tooltip survives unrelated navigation | [Help](../../artifacts/apple-design-live-2026-10-09/help-live.png), [Diagnostics](../../artifacts/apple-design-live-2026-10-09/diagnostics-live.png) | Notebook's Hide output tooltip remained over other pages after keyboard navigation, with the pointer stationary. | Clear hover/tooltip ownership on destination, overlay, virtualized-element and focus changes. |
| Global edit-mode status persists on other pages | Settings/Help/Diagnostics captures | EDIT MODE describes the notebook editor while the user is elsewhere; stale context adds noise. | Scope mode to Notebook, retain source/runtime truth independently. |
| Commands layout leaves an oversized blank footer | [Commands](../../artifacts/apple-design-live-2026-10-09/commands-live.png) | Tall popup cuts a result description at its scroll edge while reserving a large footer beneath Esc. | Compact content-driven palette height, deliberate scroll edge and a small footer. |

These are observed design/semantic issues, not claims of data loss. Generic layout reuse is a likely source of the stretched controls, but exact fixes require tracing each GPUI composition.

## Re-scoring with the same rubric

The original weights remain unchanged. Weighted mean: 4.89, reported as 4.9. Visual 4/10 is a separate editorial judgment of hierarchy, proportion, density and coherence. Scores remain heuristic rather than participant results.

| Principle | Weight | Score /10 | Live basis |
| --- | ---: | ---: | --- |
| Purpose | 12% | 6.0 | Useful core destinations; experimental and repeated tools obscure their main tasks. |
| Agency | 14% | 5.5 | Edit/run and keyboard commands worked; structural recovery remains incomplete. |
| Responsibility | 12% | 6.5 | Trust and coverage limits are stated, but JSON and semantic-state inconsistencies undermine comprehension. |
| Familiarity | 10% | 6.0 | Native typography/frame help; wrong-side switcher and inconsistent form controls weaken predictability. |
| Flexibility | 12% | 5.5 | Dark and compact layouts function, but density and accessible disabled states fall short. |
| Simplicity | 14% | 3.5 | Repeated controls, redundant unchanged source, technical filter syntax and overexposed experimental work. |
| Craft | 18% | 3.5 | Stretched actions, weak measures, stale tooltip, mismatched filter copy and awkward palette geometry. |
| Delight | 8% | 3.0 | Working tasks do not overcome the visual friction and lingering interaction artifacts. |

## Revised frontend priority

First build one native vertical slice with deliberate layout: workspace header → selected code → result/error → contextual actions. In the same slice, build a genuine History detail view and a readable checkpoint summary. Stop expanding generic record/form renderers until those compositions pass live review. Fix disabled accessibility state and stale tooltip lifecycle alongside the visual work. Then propagate the component states and layout rules to Settings, Help and Diagnostics.

The [v6 draft](../../plans/frontend-v6-draft.md) now incorporates these findings. Its first gate requires actual native populated views and interactive validation. A polished HTML concept cannot establish GPUI visual quality or native usability.
