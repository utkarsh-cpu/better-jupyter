# Frontend live UX (2026-11-07)

Working notes for the [live UX draft plan](../../plans/frontend-live-ux-draft.md).

## What was run

- Built `kernelledger-desktop` on Ubuntu with Rust 1.99.0 (fontconfig, dbus, Mesa Vulkan/GL deps).
- Opened a populated fixture notebook on `DISPLAY=:1` (XFCE, Mesa llvmpipe software render).
- Interacted with Notebook, History, Checkpoints, Commands, Settings, error traceback, and Assistant.
- Captures from that session: see agent artifacts under `ui-live/` when retained; Windows release evidence remains in [frontend-v6 native](../../artifacts/frontend-v6/native/).

## Live heuristic (not a release score)

Overall **7.8 / 10**. Strongest: error/state (~9), interaction clarity, restraint. Weakest: brand/distinctiveness (~6), delight (~6), unqualified AT.

## Plan coverage

The draft rolls every open thread into workstreams A–J: identity, notebook density, History/Checkpoints/Settings craft, error/recovery honesty, accessibility, performance, Linux platform qualification, assistant reliability, interchange/help polish, and studies/scorecard/plan publication.

It does **not** replace `FRONTEND_PLAN.md`. Publish a frontend version only after requirements stabilize (see plans README).
