# Native v6 evidence

These are compositor screenshots and UI Automation trees from the actual Windows
GPUI executable. They are not HTML mockups. `release-*` is the final implementation
capture set; `verified-*`, `final-*` and earlier names record development iterations.
The fixture and profile are isolated from the user's project and preferences.

| Journey | Final capture |
| --- | --- |
| Notebook, Light | [release-notebook-final.png](release-notebook-final.png) |
| Notebook, Dark | [release-notebook-dark.png](release-notebook-dark.png) |
| Compact High contrast, 200% interface size | [release-contrast-200-800.png](release-contrast-200-800.png) |
| History list | [release-history-list.png](release-history-list.png) |
| Recorded execution, unchanged source once | [release-history-detail.png](release-history-detail.png) |
| Checkpoint report | [release-checkpoint-report.png](release-checkpoint-report.png) |
| Copy acknowledgment | [release-copy-feedback.png](release-copy-feedback.png) |
| Restore before trust | [release-restore-untrusted.png](release-restore-untrusted.png) |
| Restore after trust | [release-restore-trusted.png](release-restore-trusted.png) |
| Actual successful new-session restore | [release-restored.png](release-restored.png) |
| Settings | [release-settings-final-100.png](release-settings-final-100.png) |
| Commands | [release-commands-final.png](release-commands-final.png) |
| Help | [release-help-final.png](release-help-final.png) |
| Diagnostics | [release-diagnostics-final.png](release-diagnostics-final.png) |
| Runtimes | [release-runtimes.png](release-runtimes.png) |

Each screenshot has a matching `.uia.json`. `launch.py` creates a populated
temporary fixture, starts the Python runtime and opens its native desktop window.
`reattach.py` reopens the same fixture against a rebuilt executable.
`inspect.ps1` restricts actions to the recorded process/window, measures actual
client bounds, and records accessible names, roles, availability and geometry.
Key/pointer/capture actions verify that the app owns the foreground.

Display DPI is 192. Native 800 x 600 and 1280 x 800 clients fit; Windows limits a
requested 1440 x 900 client to 1440 x 879.5 on this display. Some UIA descendants
outside scrolling viewports are reported as onscreen by the toolkit; do not use
that field as proof of visibility. See the [qualification ledger](../../../docs/desktop/frontend-v6.md)
for the exact limits of these checks and remaining release gates.
