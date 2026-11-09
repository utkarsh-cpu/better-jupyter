# Native GPUI audit evidence — 8 November 2026

See the [complete audit](../../docs/desktop/reviews/gpui-design-audit-2026-11-08.md) for the scorecard, prioritized defect register, source inventory, implementation, and limits.

These are actual macOS window captures from a locally rebuilt Rust GPUI application. They are not browser mockups. A disposable notebook contains Markdown, a mean calculation, an intentional ValueError, and empty cells inserted while checking authoring. A separate disposable empty notebook distinguishes valid emptiness from failed source loading. No remote assistant context was sent. Existing notebooks and credentials were not modified.

## Evidence to use

| Check | Captures |
| --- | --- |
| Confirmed Settings obstruction | `before-settings-800.png` → `final-settings-800.png`, `final-provider-settings-800.png` |
| Assistant state restoration | `final-assistant-800.png`; headless test also checks retained question/context |
| History freshness | `before-history-stale.png` → `after-history-refreshed.png` (new runs without Apply) |
| Source geometry | Intermediate `after-notebook-800x600.png` shows wrapped-row clipping; `final-notebook-800x600.png` shows horizontal source scrolling from the leading edge |
| Themes | `final-notebook-dark.png`, `final-notebook-light.png`, `final-notebook-contrast.png` |
| Real empty / failed source | `final-empty-notebook.png`, `final-source-unavailable.png` |
| Returning-user welcome | `final-welcome.png` |
| Checkpoint creation and coverage | `final-checkpoint-click.png`, `final-checkpoints-list.png`, `final-checkpoint-detail.png` |
| Commands and native picker | `final-palette.png`, `final-open-dialog.png` |
| Canceled deletion | `final-delete-confirmation.png`; forward Delete sent, Escape canceled; no deletion |
| Secondary surfaces | `after-diagnostics.png`, `after-all-runtimes.png`, `after-help.png`, `after-variables.png`, `after-console.png`, `final-debugger-disabled.png` (inactive debugger, no stepping) |
| Size/scale | `final-notebook-800x600.png`, `final-notebook-1024x768.png`, `final-notebook-1280x800.png`, `final-notebook-1440x900.png`, `final-settings-200-percent.png` |
| Export choices | `after-export-menu.png`; format choice/actual export not repeated in this pass |

The baseline contains compositor shadow padding; later captures use `screencapture -o` to omit it. Compare content areas rather than raw screenshot dimensions. Shadowless captures are 2× the logical dimensions on this display. Native metadata records requested and actual window sizes. A request for 1920×1080 was clamped to **1512×950** by the host display. Do not treat the intermediate filename `after-notebook-1920x1080.png` as proof of native 1920×1080 testing. Headless geometry tests cover 1920 logical pixels.

`after-*` images are intermediate post-change captures and can predate final font/tab/source refinements. Prefer `final-*` for final appearance. The fixture gained cells and executions during testing; comparisons are behavioral, not identical-data pixel diffs.

## Excluded captures and automation limits

`after-checkpoint-saved.png` and `final-checkpoint-created.png` did **not** show a saved checkpoint. `after-delete-confirmation.png` did **not** show a dialog. They are not success evidence and are not curated for version control. The actual saved checkpoint and confirmation appear in the explicit files listed above. Native Command+Shift+S still needs a focused physical-keyboard check: a scripted attempt did not create a checkpoint, while GPUI command-dispatch regression passes. Pointer creation succeeded.

Successful input delivery and capture do not assert a surface's usability. Every curated capture was inspected. Light/contrast and scale screenshots qualify those specific views, not every state. VoiceOver, real IME, clipboard round trips, mixed-monitor DPI, sustained rendering stability, large datasets, full exports/imports, active debugger/widgets, assistant providers, and destructive recovery commitments remain unqualified.

## Reproduction and checks

`capture.swift` finds the target PID's normal window and asks macOS for a shadowless capture. Build it with `swiftc capture.swift -o /tmp/kl-design-capture`. `native_review.py` and `final_review.py` record the keyboard/pointer sequences. They require prior macOS Accessibility/Screen Recording authorization, the existing `/tmp/kl-click` CoreGraphics click helper, an isolated fixture/profile, and `/tmp/kl-design-audit-session.json` containing `root` and the launched app's `pid`. They are session drivers, not standalone assertions or release tests. They intentionally use only local notebook data.

The root folder contains `Precision notebook` and `profile/kernelledger/desktop`; the main executable is launched with `XDG_STATE_HOME=<root>/profile` and `KL_RUNTIME_PYTHON=<repo>/.venv/bin/python` (preserving the venv path). Failure injection uses `/usr/bin/false`. `final_review.py` adds a separate `Empty notebook` and tests welcome without a project argument. Do not reuse a personal profile/notebook for this sequence.

Final check logs are `cargo-tests.txt`, `cargo-clippy.txt`, `cargo-build.txt`, and `cargo-fmt.txt`. Workspace: **134 tests passed**; build, Clippy with warnings denied, formatting, and diff whitespace checks passed. Existing local toolchain/cache/optional stripping warnings are explained in the audit. No Python/runtime implementation changed, so this run does not claim a complete Python/runtime qualification.
