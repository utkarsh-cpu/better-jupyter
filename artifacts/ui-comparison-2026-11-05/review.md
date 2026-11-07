# Marimo and KernelLedger: observed UI/UX comparison

Captured on 2026-11-05. Marimo **7.6/10**, KernelLedger **8.1/10** in this desktop UI review. Scores are subjective heuristics, equally weighted; differences of half a point should not be treated as statistically meaningful.

## Scope and method

- Ran the installed Marimo **0.25.1** and the existing KernelLedger native debug executable. This is not a claim about the latest Marimo release or a fresh build of current source.
- KernelLedger executable SHA-256: `54cdeeb07ee7bf332c191542962410d38475bd479d579eb640c754f588185e08`. The executable matches the 2026-11-01 local capture evidence.
- Both used disposable notebooks with the same heading, paragraph, four lines of Python, successful output (`Total sales: 660`, `Average: 165.0`), and `1 / 0` error. Markdown implementation differs naturally between the products.
- Compared light appearance at **1280×850** and **800×600** logical pixels. Native screenshots include the window frame; browser screenshots include the editor viewport without browser chrome. Windows native DPI was 192; browser output raster scale differs. Compare logical layout, not raw pixel resolution.
- Marimo's notebook has `width="medium"`; KernelLedger uses compact density and interface scale 1 with an isolated profile. Marimo's first run required activating its run control. Error traceback was open in Marimo and collapsed in KernelLedger in these captured states; this influences space usage and is not a fixed limitation of Marimo.
- Inspected notebook, settings, native Commands overlay, native UI Automation names, Marimo DOM control labels and code font, and small-window behavior. Marimo Escape dismissed settings. This was not a complete keyboard, editor, accessibility, or user task study. No performance or full feature-parity score is assigned.
- Scale: 1 poor, 5 workable with substantial friction, 7 good, 9 excellent, 10 exceptional. Half-points convey reviewer judgment, not measurement precision.

## Ratings

| Property | Marimo /10 | KernelLedger /10 | Reason |
|---|---:|---:|---|
+| Visual polish | 8.0 | 7.5 | Marimo has more uniform cell borders and toolbars; Kernel mixes cards and open canvas. |
| Text readability | 7.0 | 8.5 | Kernel’s larger text is easier to scan; Marimo’s measured code size is 12.6 CSS px. |
| Visual hierarchy | 8.0 | 8.0 | Marimo groups cells consistently; Kernel clearly separates navigation, source, result and error. |
| Space efficiency | 8.0 | 7.0 | Marimo’s compact code and narrow icon rail leave more space for work; Kernel has roomier cell spacing. |
| Navigation clarity | 6.5 | 8.5 | Kernel labels Notebook, History and Checkpoints; Marimo relies heavily on sidebar icons. |
| Action discovery | 6.5 | 8.0 | Kernel exposes Run all, Edit, Export and error actions in words; Marimo hides more meaning in icons. |
| Execution feedback | 8.5 | 8.0 | Marimo shows per-cell timings and error counts; Kernel clearly shows Ready, Saved and execution ordinals. |
| Error experience | 8.0 | 8.5 | Marimo offers Launch debugger, but its open traceback includes a long scrolling path; Kernel starts with a concise expandable summary. |
| Settings usability | 8.0 | 8.5 | Both organize settings; Kernel’s appearance preview is especially clear, while Marimo’s editor settings are denser. |
| 800×600 layout | 7.0 | 8.0 | Kernel automatically collapses its outline and fits this example; Marimo wraps the Markdown source and requires more vertical scrolling. |
| **Equal-weight average, rounded** | **7.6** | **8.1** | Limited to the observed UI scope above. |

## Interpretation

KernelLedger is easier to understand at first glance. Its labeled destinations, clear Run all button, readable body/code text, Saved/Ready status and short error summary reduce interpretation. Its settings page is calm and the appearance preview helps users see the consequence of a change. The narrow window automatically removes the outline and shortens toolbar actions; this three-cell example remains visible.

Marimo has a consistent cell container and toolbar structure, compact code, per-cell execution timing, an error count and a direct Launch debugger action. Its richer editor settings are organized, but the main UI's icon-only rails and floating actions require more learning. Code was measured at 12.6 CSS px with a 17.64 px line height. The open traceback shows a long local path with horizontal scrolling. Showing Markdown source and rendered output together increases vertical space in this fixture; source and traceback can be collapsed, so the narrow screenshot should not be read as an optimized Marimo layout.

Marimo's baseline exposed multiple visible icon controls without an accessible name in this DOM sample. KernelLedger exposed named Run, Outline, Settings, cell and output controls in Windows UI Automation. This is useful implementation evidence, **not a screen-reader accessibility rating or certification for either app**.

## Highest-value KernelLedger improvements

1. Make inactive and selected cell boundaries more consistent; source, output and actions should read as one predictable unit without excessive borders.
2. Offer a denser working layout while retaining the current readable option. Tune gaps between cells and the width of the outline before shrinking text.
3. Provide more useful outline labels: this short notebook already truncates both its name and a cell's source preview.
4. Consider optional per-cell duration beside the execution ordinal, following Marimo's useful timing feedback.

## Evidence

- [Interactive screenshot comparison and chart](comparison.html)
- [Marimo notebook](marimo-notebook.png), [KernelLedger notebook](kernel-notebook.png)
- [Marimo settings](marimo-settings.png), [KernelLedger settings](kernel-settings.png)
- [Marimo at 800×600](marimo-narrow.png), [KernelLedger at 800×600](kernel-narrow.png)
- [KernelLedger dark appearance](kernel-dark.png), [KernelLedger Commands](kernel-commands.png)
- `kernel-*-uia.json`: native control evidence. `session.json`: versions, DPI, executable hash and owned process identifiers (processes have been stopped).
- `capture.py` and `sales_analysis.py`: fixture/capture source. The script creates new fixtures and is not intended to overwrite an existing run.

Both comparison apps, their owned runtimes and the browser tab were closed after capture. Application source and the user's existing notebooks/settings were not edited.
