# Bars and buttons implementation evidence

Actual native GPUI window captures and UI Automation reports accompany the
[implementation checklist](../../docs/desktop/frontend-bars-polish.md).

`native/` contains the four requested outer window sizes in light, dark, and
high-contrast appearance, plus Settings captures. `native/comfortable-200/`
contains high-contrast evidence at 200% interface size with comfortable targets.
Reports include vertical center measurements and Windows hit-test results.

`capture/` is a separate, pinned Windows Graphics Capture utility used only for
the owned disposable probe window. Its Cargo dependencies are independent of
the product workspace; build artifacts remain ignored. No mockups are used as
implementation evidence.
