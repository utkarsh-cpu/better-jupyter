# Resumed native qualification — 8 November 2026

This follow-up supersedes the live-check gaps recorded when the desktop locked in the [implementation report](gpui-plan-implementation-2026-11-08.md). It uses a fresh disposable notebook, separate application/configuration profiles, and the latest native Rust/GPUI build. No provider request was sent and no destructive confirmation was committed.

## Findings corrected during the resumed checks

| Severity | Confirmed defect and cause | Correction | Verification |
| --- | --- | --- | --- |
| P1 · Python recovery | macOS resolves a venv's Python symlink in the native file picker, selecting the base interpreter and losing the environment containing KernelLedger. | Both Python choosers accept an environment folder or executable. A selected folder maps to `bin/python` on Unix or `Scripts/python.exe` on Windows, with root `python.exe` supported. The venv executable path is retained rather than canonicalized. `ui/src/lib.rs` | The chooser accepted `.venv`, loaded source in the same application process, saved the correct venv path, and loaded source again after relaunch. A GPUI chooser regression covers folder selection. Windows mapping is source-reviewed and still requires target-platform testing. |
| P2 · short cell source | A short cell could retain a vertical scroll position after a longer edit followed by text undo, hiding line one. The full-file editor's default trailing scroll space is inappropriate for a cell sized to its source. | Disable scroll beyond the last line for notebook editors using the installed `scroll_beyond_last_line(Some(0))` API. Horizontal scrolling and the 30-row cap remain. `ui/src/notebook.rs` | A geometry regression shrinks a scrolled 45-line cell and verifies scroll clamping, visible first row and unchanged caret. Native paste → text undo shows both original source lines fully visible. |
| P2 · replacement failure copy | The banner said notebook services could not start even though an invalid replacement correctly retained the working connection. | Distinguish initial startup from replacement validation; the latter explicitly says the current connection is unchanged. Keep technical diagnostics out of the generic startup summary. `ui/src/lib.rs` | GPUI failure-state test checks the copy and retained edits/session; native invalid selection leaves Ready visible and a subsequent checkpoint succeeds. |

The Python chooser prompt now names environment folders. Help recommends selecting `.venv` on macOS to preserve its packages. These are focused native fixes; no dependencies or renderer/framework forks were added.

## Native checks completed

| Check | Result and evidence |
| --- | --- |
| Initial helper failure and recovery | Pass. `live-01-helper-failure.png` → `live-02-helper-recovered.png`. The application PID stays the same through selection and recovery. |
| Remember successful Python | Pass. Profile stores the `.venv/bin/python` path; relaunch loads the notebook. `live-03-helper-remembered-after-restart.png`. |
| Invalid replacement while worker is ready | Pass. Working helper/worker remain usable; creating a checkpoint after failed validation increments the disposable ledger count. `live-21-invalid-replacement-clear-feedback.png`; `live-validation-results.json`. |
| Provider basic and advanced states | Pass at 800 × 600. The advanced toggle is reachable by scrolling, opens coding-model/inference controls and retains alignment. `live-04-provider-basic-800.png`, `live-05-provider-advanced-toggle.png`, `live-06-provider-advanced-expanded-800.png`. No connection test or provider save was submitted. |
| History paging beyond empty matches | Pass. Search `needle`, follow Next twice, reach execution 225. `live-08-history-first-empty.png`, `live-09-history-second-empty.png`, `live-10-history-later-match.png`. |
| Unicode clipboard | Pass. Paste and copy a source cell containing Greek text and an emoji, compare clipboard content, save and compare source bytes. The previous clipboard was restored without logging its contents. `live-11-unicode-clipboard-editor.png`. This is not an IME test. |
| Structural undo/redo | Pass for moving a cell, entering command mode with Escape, Undo, Redo and Undo again. Manifest cell IDs/order match the expected states. `live-12-native-structural-undo.png`. Editor text undo remains distinct. |
| Runtime version | Pass. Runtime Settings displays worker version `3.14.3`. `live-13-runtime-version-ready.png`. |
| Destructive confirmation | Pass for opening, Tab traversal and Escape cancellation at minimum size; danger action is visible and source is retained. `live-14-destructive-confirmation.png`, `live-15-modal-tab-focus.png`. Delete was not committed. |
| Short-cell text undo | Pass after the fix. `live-18-short-cell-after-text-undo-fixed.png` shows line one and line two fully visible; the earlier clipped state is visible in `live-16-invalid-python-keeps-worker.png`. |
| Window dimensions | 800 × 600, 1024 × 768, 1280 × 800 and 1440 × 900 logical dimensions reached. A 1920 × 1080 request is clamped to 1512 × 950 by this display/window manager. `live-window-metadata.json` records actual bounds. |
| Themes | Light, Dark and High contrast inspected in native screenshots at minimum size. `live-theme-*.png`. This is not every component/theme combination. |
| UI scale | 100% and 200% checked. At 800 × 600 and 200%, compact chrome uses Menu and Settings content remains scrollable. `live-19-settings-200-percent.png`, `live-20-settings-200-percent-scrolled.png`. The 200% fixture preference was applied before launch and then restored. |

The images are unmodified native window captures at 2× backing resolution. Logical window size and backing pixels are separate. Keyboard checks use native CGEvents posted to the application; this does not certify every physical keyboard/layout. Settings navigation scrolls with the page, so the review scrolled back to its section selector before changing sections.

## Accessibility and remaining qualification

A bounded macOS accessibility-tree read returned seven native window/caption nodes. This does not verify workbench control naming, announcements, reading order or screen-reader operation. No VoiceOver setting was changed. The accessibility score remains **Not Tested**, and assistive-technology qualification remains a release gate.

Still unqualified: real IME/composition, a physical keyboard/layout matrix, screen-reader journeys, mixed DPI/multiple monitors, sustained large-notebook/output performance, configured provider requests, target Windows/Linux hardware, and signed/distributable packages. The host cannot provide 1920 × 1080 logical window verification on this display. These limits are not inferred passes.

The design scorecard in the implementation report otherwise remains applicable. The resumed evidence increases confidence in the corrected flows; it does not establish accessibility compliance or sustained rendering stability. Production release still requires qualification of those gates.

## Regression and implementation record

- `cargo test --locked --workspace`: **140 passed** — client 31, models 7, protocol 3, runtime 24, UI 75.
- Clippy with warnings denied, formatting and desktop/daemon build pass.
- The previously completed Python suite remains **302 passed, 142 skipped**. Application Python code is unchanged in this resumed pass; that suite was not needlessly rerun.
- New regressions cover environment-folder selection and short-cell scroll clamping. Existing recovery coverage now also checks that validation failure describes the retained connection.
- Changed application files: `desktop/crates/ui/src/lib.rs`, `notebook.rs`, and `tests.rs`; Help documents the environment-folder choice. Earlier implementation changes remain in the working tree.

The current [evidence index](../../../artifacts/gpui-plan-implementation-2026-11-08/README.md) links the screenshots, actual window bounds, validation observations and command transcripts. The isolated application and worker are stopped after checks; the disposable notebook remains available for inspection.
