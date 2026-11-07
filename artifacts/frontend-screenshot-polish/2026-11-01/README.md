# Screenshot polish evidence, 2026-11-01

Native GPUI captures and UI Automation reports for the
[screenshot polish notes](../../../docs/desktop/frontend-screenshot-polish.md).

`before/` is the pre-change executable (`9165d0980ac5`): compact baseline and
comfortable density. `after/` is executable `54cdeeb07ee7`: the same baseline,
dark, high contrast, comfortable density, 800×600, 200% UI scale, and 200% at
800×600. Each directory has a `manifest.json` with the source revision, dirty
tree, executable hash, DPI, scale, density, cell pitch, menu keyboard result,
and journey failures.

Captures use Windows Graphics Capture on the disposable probe window only.
No mockups are used as evidence.
