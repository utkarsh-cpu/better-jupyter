# Rust UI and native backend qualification

The native startup selector and the current Rust UI/backend subset have been
tested together locally. This is not completion of phases 3–8 or release
qualification. Python remains the default; outstanding gates are recorded in
[runtime-v6-gates.json](runtime-v6-gates.json).

## Startup and rollback

Set `KERNELLEDGER_RUNTIME=native` and `KERNELLEDGER_DAEMON` to an existing absolute
daemon path before starting the CLI or desktop. The desktop also accepts a
bundled sibling daemon. `KERNELLEDGER_HELPER=native` selects the native source and
inspection helper, retaining the existing Python utility fallback for unsupported
helper methods. No binary is downloaded and an unavailable native binary is an
error. A running compatible owner is reused instead of being replaced.

Worker interpreter selection preserves explicit choices, project configuration,
project venvs, and the desktop's private-distribution refusal. Unix venv symlinks
are preserved so the worker retains that environment's packages. Startup readiness
requires an authenticated desktop-protocol handshake, rather than a readiness
file alone, and retries have a wall-clock deadline.

To roll back, stop the owner and wait for `supervisor.json` to disappear, then set
`KERNELLEDGER_RUNTIME=python`. Set `KERNELLEDGER_HELPER=python` for the Python helper.
Switching owners ends the live namespace; the existing source/history formats
remain compatible.

## Completed local checks

| Check | Result | Evidence under `artifacts/runtime-v6/` |
| --- | --- | --- |
| Complete Windows Python suite, with native tests enabled | 281 passed, 44 optional/platform skips | `native-ui-backend-full-windows312.xml` |
| Complete Windows Rust workspace | 80 passed, including 37 GPUI tests | `ui-backend/rust-workspace-windows.log` |
| Windows workspace Clippy | Passed with warnings denied | `ui-backend/rust-clippy-windows.log` |
| Full Python static checks | Pyright: zero errors; Ruff lint and all 78 formatted files pass | Local qualification tool output |
| Assistant regressions after type annotation cleanup | 54 passed | `ui-backend/assistant-static-cleanup.xml` |
| Linux headless Rust client/models/protocol/runtime | 43 passed | `ui-backend/rust-headless-linux.log` |
| Linux native owner/desktop/CLI/feed/launcher integration | 67 passed | `native-ui-backend-linux312.xml` |
| Linux launcher/venv regression suite | 7 passed, including isolated venv package import | `tests/test_runtime_launch.py` |
| Real Rust service with native helper and native owner | Passed on Windows and Linux | Windows `ui-backend/*-native.json`; Linux lifecycle output |
| Extracted Linux daemon archive | 20 passed | `native-ui-backend-package-linux312.xml` |
| Extracted Windows daemon archive | 19 passed, Unix venv test skipped | `native-ui-backend-package-windows312.xml` |

The Rust service scenarios cover revision-checked editing, execution, lost-client
reconnect, durable operation lookup, folded output, first-use sample creation,
stop while idle/busy, stop from another client, restart by running again, offline
source export, offline bundle export, and paged history. Windows runtime instance
records independently confirmed that the examples used the Rust owner.
The service examples also explicitly assert `implementation == "rust"` when
native startup is selected, so their CI checks cannot silently pass against Python.

The rendered Windows application was tested with the native helper and native
owner at 192 DPI. Light-mode checks passed real Run and Create checkpoint actions.
Dark and high-contrast checks additionally invoked checkpoint inspection, scrolled the owned
report, selected its trust checkbox, restored through the UI, verified a replacement
session, and checked `answer == 42` in the restored namespace.

The native probe checks UIA output and navigation, notebook/history/checkpoint and
settings destinations, four window sizes, caption hit-testing, minimum size, and
ten compact/wide resize cycles. Reports and captures are under
`ui-backend/native-light/`, `ui-backend/native-dark/`, and `ui-backend/native-contrast/`. Captures use a crop of the
owned foreground window and are not an occlusion-free screenshot certification.

The probe now waits for asynchronous controls and scrolls checkpoint content before
activating clipped actions. Its failed exploratory restore runs exposed probe
assumptions about offscreen controls, trust selection, and retained scroll position;
the successful dark report includes the complete restore journey.

Formatting-only differences in three existing UI files were normalized so the
whole-workspace formatting check passes. No visual design or interaction policy
was changed by that normalization. CI now runs Windows workspace tests and native
Rust service smoke/lifecycle scenarios; Linux client CI installs its D-Bus build
prerequisites. Graphical Windows probing remains a local display-dependent check.
Pre-existing assistant type inference errors were resolved with explicit types
for validated requests, approval maps, provider URLs, and resolved destinations;
all 54 assistant tests and full-project Pyright checks pass afterward. Two existing
test files were mechanically formatted for the whole-project Ruff check.

## Remaining qualification

No Linux graphical UI, NVDA/Orca journey, real IME journey, mixed-DPI matrix,
clean-machine installer/upgrade/uninstall, signed release, or two qualified release
candidates is claimed. Fault injection, sustained input/feed/process-tree stress,
complete numeric hashing, grouped stream commits and queue byte budgets, remaining
sharing/training mutations, cross-version/codecs, and measured benefit gates remain
open. Passing local UI/backend tests does not waive those exits.
