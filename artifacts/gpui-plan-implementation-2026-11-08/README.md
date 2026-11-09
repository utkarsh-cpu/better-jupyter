# Native implementation evidence — 8 November 2026

**Update:** [Resumed native qualification](../../docs/desktop/reviews/gpui-native-followup-2026-11-08.md) closes the earlier screen-lock-related live gaps and records the venv-picker and source-clipping fixes. The initial-round notes below are historical.

These are unmodified screenshots of the Rust/GPUI application on macOS Apple Silicon, using a disposable notebook and desktop profile. The [implementation report](../../docs/desktop/reviews/gpui-plan-implementation-2026-11-08.md) distinguishes native observations from automated coverage and unverified release gates.

| File | Observation |
| --- | --- |
| [01-auto-discovery-notebook.png](01-auto-discovery-notebook.png) | No application Python override; source loaded through environment discovery. Markdown preview selection uses a rail. 1280 × 800 logical; 2560 × 1600 backing pixels. |
| [11-final-checkpoint-shortcut.png](11-final-checkpoint-shortcut.png) | Native CGEvent Command+Shift+S saved a checkpoint in an idle runtime. |
| [12-final-after-open-cancel.png](12-final-after-open-cancel.png) | Canceling the native Open picker restored the workbench; another native checkpoint chord increased the disposable ledger checkpoint count from 2 to 3. |
| [13-final-history-page-one.png](13-final-history-page-one.png) | Bounded populated execution page. |
| [14-final-history-empty-candidates.png](14-final-history-empty-candidates.png) | Search for `needle`; first 100 candidates have no match, and Next page remains available. Backend parity tests find execution 225 after continuing two pages. This is not evidence of a full native paging journey. |
| [16-final-runtime-settings-800.png](16-final-runtime-settings-800.png) | Application Python is distinct from project interpreter, with Choose/Retry controls. 800 × 600 logical. Captured before the final version-field fallback correction. |
| [17-final-provider-basic-800.png](17-final-provider-basic-800.png) | Basic provider form at 800 × 600 logical, with advanced settings collapsed. |

The previous audit's [evidence](../gpui-design-audit-2026-11-08/README.md) supplies baseline and light/dark/high-contrast size captures. This follow-up did not repeat the complete matrix. The desktop locked during later testing (`CGSSessionScreenIsLocked=1`); subsequent capture/input attempts were discarded. No screenshot of the login screen or other applications is included. Screen-lock behavior must not be diagnosed as an application accessibility defect.

`key.swift` posts native key events to a tracked process. It was used because System Events delivery of shifted shortcuts was inconsistent. This is native event verification, not a physical-keyboard qualification. `capture.swift` captures the app's main layer-zero window through macOS screencapture. Native file pickers have their own window IDs and must be inspected separately; a screenshot of the main window alone cannot prove that a picker has closed.

`native_fixture.py` launches an isolated notebook with 240 recorded executions, no provider credentials, and a separate profile. It supports reproducible startup/discovery and later native checks. It does not automate unsafe confirmations or contact a provider. Run after building the desktop/daemon and installing the project into its venv:

```sh
.venv/bin/python artifacts/gpui-plan-implementation-2026-11-08/native_fixture.py launch
```

The printed session JSON identifies only the disposable app/project. After live testing, clean up its app and worker with:

```sh
.venv/bin/python artifacts/gpui-plan-implementation-2026-11-08/native_fixture.py stop
```

Remaining native checks: failed helper → choose valid application Python → loaded source; failed selection while edits/worker remain; expanded provider settings; physical keyboard, real clipboard/IME, screen readers, full revised theme/size/DPI matrix, long-duration rendering, target hardware and signed packaging. Automated tests cover the implemented recovery state transitions and the data-safety contracts; they do not close those live gates.

Final regressions: **138 Rust tests passed; 302 Python tests passed, 142 skipped.** Clippy, formatting, Ruff, and Pyright pass. Command transcripts are saved alongside these images.

## Resumed native checks

Latest regression: **140 Rust tests pass**; Clippy, formatting and debug builds pass. The previous application Python result remains 302 passed / 142 skipped; Python application code did not change in this resumed pass.

| Evidence | Observation |
| --- | --- |
| [Helper failure](live-01-helper-failure.png) → [recovered](live-02-helper-recovered.png) → [after restart](live-03-helper-remembered-after-restart.png) | Choosing the venv folder preserves its Python environment and is remembered. |
| [Basic provider](live-04-provider-basic-800.png), [reachable toggle](live-05-provider-advanced-toggle.png), [expanded settings](live-06-provider-advanced-expanded-800.png) | Native 800 × 600 form, collapsed and expanded, with vertical scrolling. |
| [First empty search page](live-08-history-first-empty.png), [second](live-09-history-second-empty.png), [later match](live-10-history-later-match.png) | Two Next actions reach execution 225. |
| [Unicode editor](live-11-unicode-clipboard-editor.png), [structural Undo](live-12-native-structural-undo.png) | Clipboard round trip; command-mode move/Undo/Redo preserves IDs. |
| [Runtime version](live-13-runtime-version-ready.png) | Worker version is correctly displayed. |
| [Confirmation](live-14-destructive-confirmation.png), [Tab focus](live-15-modal-tab-focus.png) | Danger action and focus are visible; Escape cancels without deleting. |
| [Earlier clipped cell](live-16-invalid-python-keeps-worker.png) → [fixed after text undo](live-18-short-cell-after-text-undo-fixed.png) | Disabling trailing editor scroll space keeps the first source line visible. |
| [Replacement error feedback](live-21-invalid-replacement-clear-feedback.png) | An invalid selection explicitly retains the current connection. A subsequent checkpoint succeeds. |
| [200% Settings](live-19-settings-200-percent.png), [scrolled](live-20-settings-200-percent-scrolled.png) | Compact chrome and scrollable content at minimum window size. |
| [Light](live-theme-light-800.png), [Dark](live-theme-dark-800.png), [High contrast](live-theme-contrast-800.png) | Final notebook rendering at 800 × 600 after the source-clipping fix. |

Window captures: [800 × 600](live-size-800x600.png), [1024 × 768](live-size-1024x768.png), [1280 × 800](live-size-1280x800.png), [1440 × 900](live-size-1440x900.png), [1920 × 1080 request](live-size-1920x1080.png). The last request is actually **1512 × 950 logical pixels**, as recorded in [window metadata](live-window-metadata.json).

[Validation observations](live-validation-results.json), [image sizes and hashes](live-image-manifest.json), and [bounded AX snapshot](live-accessibility-tree.json) preserve scope. The AX snapshot exposes native window/caption nodes; it does not qualify workbench screen-reader behavior. Real IME, assistive technology, mixed DPI, configured providers, sustained performance, target hardware and signed packages remain release gates.
