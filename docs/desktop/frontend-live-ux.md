# Frontend live UX (2026-11-07)

Working notes for the [live UX draft plan](../../plans/frontend-live-ux-draft.md).

## What was run

- Built `kernelledger-desktop` on Ubuntu with Rust 1.99.0 (fontconfig, dbus, Mesa Vulkan/GL deps).
- Opened a populated fixture notebook on `DISPLAY=:1` (XFCE, Mesa llvmpipe software render).
- Interacted with Notebook, History, Checkpoints, Commands, Settings, error traceback, and Assistant.
- Captures from that session: see agent artifacts under `ui-live/` when retained; Windows release evidence remains in [frontend-v6 native](../../artifacts/frontend-v6/native/).
- Reproducible Linux launcher: [artifacts/frontend-live-ux-2026-11-07/launch_linux.py](../../artifacts/frontend-live-ux-2026-11-07/launch_linux.py).

## Live heuristic (not a release score)

Overall **7.8 / 10**. Strongest: error/state (~9), interaction clarity, restraint. Weakest: brand/distinctiveness (~6), delight (~6), unqualified AT.

## Plan coverage

The draft rolls every open thread into workstreams A–J: identity, notebook density, History/Checkpoints/Settings craft, error/recovery honesty, accessibility, performance, Linux platform qualification, assistant reliability, interchange/help polish, and studies/scorecard/plan publication.

It does **not** replace `FRONTEND_PLAN.md`. Publish a frontend version only after requirements stabilize (see plans README).

## Implementation status (this branch)

| Workstream | Status | What landed |
| --- | --- | --- |
| A Identity | In progress | Cool slate paper tokens (`#EEF1F5` / cobalt); Ubuntu fonts on Linux; always-visible `KernelLedger`/`KL` wordmark; Settings preview brand line; sentence-case “Context receipt” |
| B Density / narrow | In progress | Wordmark collapse budget before Run label; narrower assistant panel (280–560, default 320/400); compact overlay width capped |
| C Surfaces | Partial | History advanced filters already disclosed; no further IA rewrite |
| D State grammar | In progress | Failed cells: danger border + left rail + a11y “failed”; Editing remains Notebook-scoped |
| E Accessibility | Open | Needs NVDA/Narrator/Orca on qualified hosts |
| F Performance | Open | Measurement campaign not run |
| G Linux platform | In progress | Package/env docs + `launch_linux.py`; hardware GL still untested |
| H Assistant | Partial | Narrow coexistence with B; reliability draft remainder open |
| I Interchange/help | Open | |
| J Studies/scorecard | Open | |

## Token sheet (A.1)

| Name | Light | Dark |
| --- | --- | --- |
| Paper | `#EEF1F5` | `#101317` |
| Ink | `#161B21` | `#ECEEF1` |
| Mist | `#E2E6EC` | `#171B21` |
| Card | `#F7F9FC` | `#1A1F25` |
| Cobalt | `#0B5CAB` | `#6CB6FF` |
