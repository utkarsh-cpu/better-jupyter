# Frontend live UX (2026-11-07)

Working notes for the [live UX draft plan](../../plans/frontend-live-ux-draft.md).

## What was run

- Built `kernelledger-desktop` on Ubuntu with Rust 1.99.0 (fontconfig, dbus, Mesa Vulkan/GL deps).
- Opened a populated fixture notebook on `DISPLAY=:1` (XFCE, Mesa llvmpipe software render).
- Interacted with Notebook, History, Checkpoints, Commands, Settings, error traceback, and Assistant.
- Captures from that session: see agent artifacts under `ui-live/` when retained; Windows release evidence remains in [frontend-v6 native](../../artifacts/frontend-v6/native/).
- Reproducible Linux launcher: [artifacts/frontend-live-ux-2026-11-07/launch_linux.py](../../artifacts/frontend-live-ux-2026-11-07/launch_linux.py).

## Live heuristic (not a release score)

Overall **7.8 / 10**. Strongest: error/state (~9), interaction clarity, restraint. Weakest: brand/distinctiveness (~6.5 after wordmark), delight (~6), unqualified AT.

Provisional JSON: [scorecard-provisional.json](../../artifacts/frontend-live-ux-2026-11-07/scorecard-provisional.json). Does not replace [ux-scorecard.md](ux-scorecard.md) NT ratings.

## Plan coverage

The draft rolls every open thread into workstreams A–J: identity, notebook density, History/Checkpoints/Settings craft, error/recovery honesty, accessibility, performance, Linux platform qualification, assistant reliability, interchange/help polish, and studies/scorecard/plan publication.

It does **not** replace `FRONTEND_PLAN.md`. Publish a frontend version only after requirements stabilize (see plans README).

## Implementation status (this branch)

| Workstream | Status | What landed |
| --- | --- | --- |
| A Identity | Done for draft slice | Cool slate paper; Ubuntu fonts on Linux; KL title mark + status wordmark; Settings brand preview; sentence-case “Context receipt” |
| B Density / narrow | Done for draft slice | Collapse budget with KL; narrower assistant; Linux caption gutter |
| C Surfaces | Done for draft slice | History Filters hint (after:/before:); Help subtitle restored; About/Copy build info craft |
| D State grammar | Done for draft slice | Failed border/rail/a11y; Editing Notebook-scoped |
| E Accessibility | Partial | Destination change clears menus/palette; [a11y-checklist.md](../../artifacts/frontend-live-ux-2026-11-07/a11y-checklist.md); AT host runs still open |
| F Performance | Scaffolded | [perf-campaign.md](../../artifacts/frontend-live-ux-2026-11-07/perf-campaign.md); measurements not run |
| G Linux platform | Partial | Packages/env + launcher; hardware GL untested |
| H Assistant | Partial | Narrow coexistence; Rebuild context already present; broader reliability gates open |
| I Interchange/help | Partial | Help/Diagnostics empty-state copy; export/Git journeys not re-qualified |
| J Studies/scorecard | Partial | Provisional heuristic filed; release scorecard still NT |

## Token sheet (A.1)

| Name | Light | Dark |
| --- | --- | --- |
| Paper | `#EEF1F5` | `#101317` |
| Ink | `#161B21` | `#ECEEF1` |
| Mist | `#E2E6EC` | `#171B21` |
| Card | `#F7F9FC` | `#1A1F25` |
| Cobalt | `#0B5CAB` | `#6CB6FF` |
