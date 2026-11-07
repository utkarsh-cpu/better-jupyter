# Independent UI/UX review: Marimo 0.25.1 and KernelLedger

Reviewed 2026-11-05 from the captures in this folder. This report was written after looking at the screenshots, and only then reading `review.md` and `comparison.html`.

**Equal-weight averages: Marimo 7.3 / 10, KernelLedger 7.7 / 10.**

Unrounded means are 7.25 and 7.65. Rounded half-up to one decimal. The 0.4 gap is reviewer judgment on one fixture. It is not a measured product ranking.

## How this was inspected

All eight PNGs were opened in an image reader: `marimo-notebook.png`, `kernel-notebook.png`, `marimo-settings.png`, `kernel-settings.png`, `marimo-narrow.png`, `kernel-narrow.png`, `kernel-dark.png`, and `kernel-commands.png`. Scores below come from those images, from pixel measurements on the files, and from the KernelLedger UI Automation dumps. They do not come from the earlier write-up.

Raster sizes measured from the PNG headers:

| File | Raster pixels | Stated logical size | Raster ÷ logical |
|---|---:|---:|---:|
| Marimo notebook and settings | 1280×850 | 1280×850 | 1.00 × 1.00 |
| Marimo narrow | 1280×960 | 800×600 | 1.60 × 1.60 |
| KernelLedger notebook, settings, dark, commands | 2538×1689 | 1280×850 | 1.983 × 1.987 |
| KernelLedger narrow | 1604×1202 | 800×600 | 2.005 × 2.003 |

`session.json` records native DPI 192, so a factor near 2.0 for the KernelLedger window is expected. Marimo’s notebook and settings files match the logical size 1:1. The narrow Marimo file is 1.6×. Sharpness and “which PNG looks bigger” are not comparable across those scales. Layout was compared at the stated logical sizes.

Guidance used for the judgments, not as a certification checklist: errors sit on the failing cell and use text as well as color; essential text is not left on a horizontal scrollbar; truncated names need a way to read the whole value; icon controls need a name even when the glyph is recognizable; prose stays in a limited measure. The mobile 44×44px target was not applied to this desktop capture. KernelLedger’s compact header buttons are about 28×30 logical pixels (UIA bounds 56×60 device pixels at 192 DPI), which is a compact desktop target, not a phone target. `workbench.json` sets `ui_scale` 1 and `comfortable` false, so that density was chosen for the capture.

## What is evidence, and what is not

**Measured**

- Both apps show the same fixture: heading “Sales analysis”, the same paragraph, `sales = [120, 180, 150, 210]`, printed `Total sales: 660` and `Average: 165.0`, and `1 / 0`.
- Marimo code-line ink in `marimo-notebook.png` sits near y = 320–328, 340–346, 355–363, and 372–381. Center-to-center spacing is about 17–19 pixels in a file that is already at logical size. That agrees with a line height around 17.6px. It does not, by itself, prove a computed font-size of 12.6px.
- KernelLedger code-line ink centers are about 44, 40, and 44 raster pixels apart. Divided by 1.983, that is about 20–22 logical pixels. Glyph ink is on the order of 7–12 logical pixels in both apps. KernelLedger’s code is somewhat looser, not a different size class.
- Screenshot color samples, not a colorimeter: both headings are near-black on near-white, about 17:1. KernelLedger’s dominant code ink `(48,48,48)` on the gray code surface `(232,230,224)` is about 10.6:1. The green and blue code clusters in that block are about 5.7:1 and 6.1:1. The error banner’s main red cluster `(176,32,40)` on `(249,236,234)` is about 5.9:1, with a near-black cluster about 16:1. Dark-mode heading ink is about 13.5:1 on `(26,31,37)`. Small color clusters were ignored because they match antialiased edges.
- KernelLedger UIA names and bounds in `kernel-notebook-uia.json`, `kernel-settings-uia.json`, and `kernel-commands-uia.json`.

**Seen in the captures**

- Which words are on screen, what is clipped, where the error sits, and which regions are empty.
- Marimo shows Markdown source and an expanded traceback. KernelLedger shows rendered Markdown and a one-line error with “Show full traceback”. Those are states of this session.

**Preference**

- Whether KernelLedger’s extra cell padding looks calm or wasteful.
- Whether Marimo’s uniform cell cards look more finished than KernelLedger’s mix of a markdown card and open code rows.

**Not established**

- Hover tooltips, keyboard paths, running or streaming execution, the contents of History, Checkpoints, or unopened settings categories.
- Whether “Explain error” produces a useful explanation.
- Marimo’s dark theme. `kernel-dark.png` has no pair, so it is not a comparison win.
- A screen-reader pass. There is no Marimo DOM dump in this folder. The earlier claim that some Marimo icon controls lack an accessible name cannot be rechecked here.
- The figure 12.6 CSS px. It appears in `review.md` and is not stored beside a style dump. The line-pitch measurement is compatible with the stated 17.64px line height. Font-size was not recomputed.

## Scores

Scale used: 5 is workable with clear friction, 7 is good, 8 is strong for this fixture, 9 would need fewer reservations. Half-points are judgment, not instrument precision.

| Dimension | Marimo | KernelLedger | Confidence | What the captures support |
|---|---:|---:|---|---|
| Visual polish | 7.5 | 7.5 | Medium | Marimo’s cells share one bordered container. KernelLedger’s window chrome, accent, and status line are consistent, while the markdown cell is a card and the code cells are open bands. Marimo’s expanded traceback has a horizontal scrollbar. KernelLedger’s title reads “kernelle…”. Those offset. |
| Text readability | 7.5 | 8.0 | Medium | Primary heading, prose, code, output, and the error sentence are readable in both light captures. After DPI normalization, code line pitch is about 18px versus about 21px. KernelLedger’s main code ink measures well above 4.5:1. Marimo’s long traceback path is not readable without horizontal scrolling. KernelLedger’s outline clips cell text. The size gap is real and small. |
| Visual hierarchy | 7.5 | 8.0 | Medium | Both make “Sales analysis” the dominant heading and put output under the code. KernelLedger adds one filled “Run all”, a “Failed” word, and a separate error band. Marimo’s per-cell controls are similar weight, and in this capture the red Markdown string sits above the heading. That second point is the source-visible state, so it is only a half-point. |
| Space efficiency | 6.5 | 7.0 | Medium, unstable | Marimo’s icon rail is narrow, and the code itself is compact. This frame also shows source plus rendered Markdown, a right-hand tool column, and a tall open traceback. KernelLedger spends more padding, but the three results and the error sentence fit on the 1280×850 view. Captured density also differs: Marimo `width="medium"`, KernelLedger compact scale 1. |
| Navigation clarity | 6.5 | 8.0 | Medium-high | Marimo’s left rail is icons, and the location is a filesystem path. KernelLedger labels Notebook, History, and Checkpoints, lists three cells, and marks the third “failed” in UIA (`Cell 3: 1 / 0, failed`). The visible title and the outline previews are truncated, so this is not 8.5. |
| Action discoverability | 7.0 | 8.0 | Medium | Marimo shows a play control on each cell and the words “Launch debugger” and “Get help”. The rail and the right-hand tools do not show words. KernelLedger shows “Run all”, “Export”, “Edit”, “Explain error”, and “Show full traceback”. Several cell tools stay icons; UIA names them (“Hide output”, “Ask Assistant…”, “More actions…”). The header control named “Commands” looks like a search icon. |
| Execution feedback | 8.0 | 7.5 | Medium | These are finished stills, not a running capture. Marimo shows 846ms, 1ms, and 172ms plus an error count of 1. KernelLedger shows “Ready”, “Saved”, execution marks `[1]` and `[2]`, and a “Failed” badge. Timing is the extra fact Marimo puts on screen. |
| Error experience | 7.5 | 7.5 | Medium | Both name `ZeroDivisionError: division by zero` on the failing cell and use text, not color alone. Marimo also shows the traceback, “Launch debugger”, and “Get help”, and the path scrolls sideways. KernelLedger starts with a one-line summary, “Explain error”, and “Show full traceback”. The expanded KernelLedger traceback was not captured, so the cleaner summary is not scored as a better debugger. |
| Settings usability | 8.0 | 7.5 | Medium | Different pages were open. Marimo’s Editor page has grouped controls, helper text, version 0.25.1, locale, and the config-file location. KernelLedger’s Appearance page has System / Light / Dark / High contrast, a short explanation, and a live preview labeled as sample cells that do not run. The rest of that page is empty. Unopened categories were not treated as empty. |
| 800×600 layout | 6.5 | 7.5 | Medium-high | At 800×600, Marimo keeps both side columns, clips the path to “…ui-comparison-20”, wraps the Markdown source, and leaves the visible error as “ZeroDivisionError” at the bottom edge. KernelLedger hides the outline, keeps the heading, both outputs, and the full error sentence on screen, and turns “Run all” into a play icon. The Marimo clip is inflated by open source and an open traceback. The lost “Run all” label is not. |

**Sums:** Marimo 7.5+7.5+7.5+6.5+6.5+7.0+8.0+7.5+8.0+6.5 = 72.5. KernelLedger 7.5+8.0+8.0+7.0+8.0+8.0+7.5+7.5+7.5+7.5 = 76.5.

**Means:** 72.5/10 = 7.25, rounded to 7.3. 76.5/10 = 7.65, rounded to 7.7.

KernelLedger’s edge in this table is navigation, labeled actions, and the narrow window. Marimo’s edge is post-run timing and the density of the Editor settings page that was open. Polish and error handling are ties for different reasons.

`kernel-dark.png` is consistent with the light layout: same structure, a distinct error band, and heading contrast around 13.5:1 in the pixel sample. It is not in the average. `kernel-commands.png` shows a command list with “Debugger” selected, “Load source file…”, disabled “Toggle breakpoint at caret” and “Debug selected cell”, and “Variables”. The disabled rows say the debugger needs `KL_EXPERIMENTAL_INTERACTIVITY=1`. UIA also names an enabled “Console” row at the bottom of the palette bounds and marks the two debugger commands `enabled: false` with names ending in “unavailable”. That overlay was not given a separate score.

## Critique of the earlier review

`review.md` rates Marimo 7.6 and KernelLedger 8.1. The arithmetic is right: the ten Marimo scores sum to 75.5 (7.55 → 7.6) and the KernelLedger scores sum to 80.5 (8.05 → 8.1). `comparison.html` repeats those scores and does not add a measurement.

The scope section is careful in the right places. It names Marimo 0.25.1, the executable hash, DPI 192, the medium versus compact settings, the open versus collapsed traceback, and the refusal to certify accessibility or performance. The sentence that half a point is not statistical should also cover the published 7.6 versus 8.1.

### Where the conclusion holds

- KernelLedger is easier to orient in at first glance because Notebook, History, Checkpoints, Run all, and the cell list use words.
- Marimo’s icon rail and floating tools ask the viewer to already know the glyphs. That is visible even without a DOM dump.
- Marimo’s per-cell times and error count are the stronger completed-run feedback in these stills.
- The open traceback’s horizontal scrollbar is a concrete defect, not a style preference.
- KernelLedger’s outline already truncates this short notebook. That recommendation is the best of the four in the earlier review.
- Calling the UIA names “implementation evidence, not a certification” is the right limit. The names checked here match that description: “Run all”, “Outline”, “Settings”, “Runtime: Python (Ready)”, “Explain error”, “Show full traceback for Output 1 for cell 3: 1 / 0”.

### Where the scores overfit the captures

**Readability, 7.0 versus 8.5.** This is the weakest score in the earlier table. A 1.5 point gap needs a large, checked difference. The stated Marimo size is 12.6px, and no KernelLedger font size is reported, so the comparison is precise on one side only. Those 12.6 and 17.64 figures are not in a saved style dump. Raster line pitch, normalized by the scale factors above, is about 18px versus about 21px. Both primary texts were readable. A half-point, 7.5 versus 8.0, fits that evidence. A drop to 7.0 also treats 12.6px as a failure. The usual floor cited for body text is under 12px, and this is code, which is often set near that size. The number may be accurate. It is not, on these screens, a 1.5 point defect.

**Space, 8.0 versus 7.0.** This rewards Marimo’s compact glyphs and narrow rail, and charges KernelLedger for padding. The same review says the open traceback and visible Markdown source are not a fixed Marimo limitation, then still gives Marimo the space win in the frame those choices dominate. Compact code and an efficient screen are different observations. The score is also confounded by `width="medium"` versus compact scale 1. My 6.5 versus 7.0 only records which captured frame spent less of the viewport on duplicate or diagnostic chrome. It would move if both apps were captured with source hidden and tracebacks collapsed.

**Error experience, 8.0 versus 8.5.** The half-point prefers KernelLedger’s summary. That summary is what the collapsed state looks like. Marimo’s extra material (traceback, debugger, help) was on screen, along with the scrollbar. Scoring KernelLedger higher uses the asymmetry the method section told the reader not to treat as a product limit. Equal 7.5s leave the scrollbar as Marimo’s problem and leave KernelLedger’s uncaptured expanded traceback unscored.

**Settings, 8.0 versus 8.5.** Editor-with-help was compared with Appearance-with-preview. Both pages are clear. KernelLedger’s preview is a real advantage for a theme switch. It does not make a one-control page, with a large blank area under it, a better settings system than the page that explains autosave, formatting, and autocomplete. Giving it 8.5 is more precision than one unmatched screen can hold. I score 8.0 versus 7.5. A tie would also be honest.

**Polish, 8.0 versus 7.5.** “Uniform borders” versus “cards mixed with open canvas” is a fair description. It is not enough to rank Marimo higher once KernelLedger’s truncated title and Marimo’s in-cell scrollbar are in the same judgment. Tie at 7.5.

**Navigation, 6.5 versus 8.5.** Direction agreed. KernelLedger at 8.5 ignores the clipped window title and clipped outline rows. The UIA name `Notebook switcher: kernelledger-notebook` is complete while the visible title is not, which is exactly the sighted-label problem. 8.0.

**800×600, 7.0 versus 8.0.** Direction agreed. Calling KernelLedger’s shorter toolbar an unqualified improvement misses that “Run all” loses its words and becomes a play icon. That is the primary action. Marimo at 7.0 is slightly high for a first screen that does not show “division by zero”, but that clip is partly the open source and open traceback, so I do not score it as a permanent narrow-layout failure. 6.5 versus 7.5.

**Action discovery, 6.5 versus 8.0, and execution, 8.5 versus 8.0.** Direction agreed. Marimo 6.5 underweights “Launch debugger” and “Get help”, which are written on the error. Execution scores above 8 treat a finished still as if loading, streaming, and failure-during-run had been seen. They had not.

**Unstored measurements.** The accessible-name DOM result and the 12.6px figure are used as evidence but not saved next to the screenshots. A later reader cannot audit them. The line height is plausible from the PNG. The DOM claim should stay labeled as an unverified prior note.

The earlier averages and these averages point the same way, KernelLedger slightly ahead, for different internal reasons. Publishing either pair to one decimal is fine only beside the unrounded sum and the confounds. The chart in `comparison.html` does not do that extra step. It presents 7.6 and 8.1 as the result.

## Recommendations

No implementation is implied. Order is priority for the captured friction, not a roadmap.

### KernelLedger

| Priority | Change | Who it helps | Risk |
|---|---|---|---|
| 1 | Keep the full notebook name and a full cell preview available without widening the outline by default. The visible title is “kernelle…”. Cell 2 is cut mid-token (“sales = [120, 18…”) while its UIA name is `Cell 2: sales = [120, 180, 150, 210]`. Use a second line, horizontal fade plus an explicit disclosure, or a focusable full label. Do not rely on a hover-only tooltip. | Anyone scanning more than one notebook or cell. | A permanently wider outline costs editor width. At 800×600 the outline is already hidden, so disclosure beats a wider default. |
| 2 | At 800×600, keep the words “Run all” on the primary button, or put that labeled action in the first overflow item. History and Checkpoints can shorten before Run all does. | Narrow windows and anyone who does not read a lone play icon as “run the notebook”. | The top bar is already full. Something else has to yield. |
| 3 | When a traceback is expanded, wrap long paths inside the cell. Marimo’s open traceback is the example of what not to ship. Keep the one-line summary as the default. | People diagnosing errors on ordinary window widths. | A wrapped path is harder to copy as one line. A separate copy action covers that. Opening the traceback by default would throw away the summary that works on this screen. |
| 4 | Make a code cell and a markdown cell the same visual group: source, result, and actions as one unit. Do that with spacing and one container, not with a new border around every output. | Scanning a mixed notebook. | Extra boxes make a notebook feel like a stack of forms. Experts who want code on the canvas will dislike it. This needs a side-by-side check, not a default change on taste alone. |
| 5 | Add optional per-cell duration next to the execution ordinal, visually secondary, off or quiet by default. | People comparing slow cells. Marimo’s 846ms / 1ms / 172ms is the useful pattern. | Always-on milliseconds add noise and invite performance readings from one sample. |
| 6 | Replace `KL_EXPERIMENTAL_INTERACTIVITY=1` on the disabled debugger rows with a product sentence, or hide those commands until the runtime can run them. UIA already marks them unavailable. | Anyone who opens Commands and thinks the debugger is broken. | Hiding them reduces discovery. Showing a raw variable trains the wrong fix. |
| 7 | Do not chase a denser default next. This profile is already compact, and the readability advantage is the current size and gap. If density changes, change gaps and outline width before type size. Put any density control on the same live preview Appearance already uses. Layout & accessibility was not opened, so this does not say the control is missing. | A later density option, without damaging the current reading size. | Shipping “denser” as a fix for this review would contradict the readability finding. |
| 8 | Check the footer. “Cell navigation”, “3 cells”, “Ready”, and “Saved” are visible and are not names in `kernel-notebook-uia.json`. “Assistant” is. If the footer is drawn without text peers, expose the same sentence the eye sees. | Assistive-tech users, if the dump is complete. | The dump may have omitted text runs. Confirm before treating it as a defect. |

### Marimo

| Priority | Change | Who it helps | Risk |
|---|---|---|---|
| 1 | Keep traceback lines, especially the file path, inside the cell width. The scrollbar on `marimo-notebook.png` is the clearest defect in the set. | Reading a stack without a second scroll axis. | Wrapping can annoy people who copy the path. Pair it with copy. |
| 2 | Give the left rail a labeled mode that does not depend on already knowing the icons. These captures never show labels, including at 800×600, where the rail still occupies width. | First-time navigation. | A labeled rail costs the compactness the earlier review liked. It should collapse. This review did not prove that Marimo lacks a collapse control; it shows the control was not in effect here. |
| 3 | On a short window, keep the exception message in the first screen when a cell is expanded. The 800×600 capture shows the type and cuts “division by zero”. Part of that is the open source and open traceback, so the fix is wrapping and scroll position, not a claim that the layout cannot work. | Seeing the failure without hunting. | Auto-collapsing source would fight a user who opened it on purpose. |
| 4 | If the 12.6px code size is confirmed from computed style, a slightly larger desktop default is a low-risk option. It is not justified as a repair from these pixels alone. The code was readable, and the measured line pitch is about 18px. | People who find the code small on a desktop monitor. | Larger code brings back the vertical cost on short windows. |

Settings in the Marimo capture do not need a redesign. Helper text under each editor control is the pattern worth keeping. KernelLedger’s theme preview is the pattern worth copying onto other appearance choices, not a reason to thin Marimo’s form.

### Which earlier recommendations hold

The earlier KernelLedger list was: more consistent cell boundaries; a denser layout while keeping the readable one; better outline labels; optional per-cell duration.

- Outline labels are the strongest item and should be first, not third. The truncation is already visible on a three-cell notebook, and the accessible name shows the full string exists.
- Optional duration is a good, small idea. Keep it optional. It should not outrank the narrow “Run all” label or traceback wrapping.
- Consistent boundaries are worth a trial. “Without excessive borders” is the right constraint. It can still backfire for expert notebook users if every output grows a card. Validate with a working notebook, not this one screen.
- A denser layout is the recommendation most likely to backfire. The capture is already `comfortable: false` at scale 1. The earlier review’s own readability score depends on KernelLedger looking larger and easier to scan. Tightening type to win the space row would spend that advantage. Tuning gaps first is sensible. Ranking it second is not, until someone shows the current gaps blocking a real notebook.

Recommendations that sound helpful and are still risky:

- Labeling every KernelLedger icon. Run, Edit, Explain, and Show traceback are already words or a standard play glyph. The rest have UIA names. Adding visible labels to Assistant, Hide, and More would crowd the row that is currently quiet.
- Opening tracebacks by default to “match Marimo.” That would import the scrollbar problem and remove the summary that fits at 800×600.
- Treating the command palette as a substitute for a visible Run label. The palette is a good index. It is not how a person sees the primary action.
- Assuming Marimo’s rail cannot be learned. Expert users already map icons. The cost is the first session and the unlabeled 800px rail, not every session. A persistent labeled rail for everyone would annoy those users.

## Limits

- One notebook, one error, light mode for the paired shots, and still images. Nothing was re-run for this review.
- Markdown source and the traceback were open in Marimo and closed in KernelLedger. Compact scale was on for KernelLedger. Those settings move space, hierarchy, and the narrow score.
- Marimo narrow’s raster scale is 1.6×, while its other shots are 1×. KernelLedger is about 2×. Pixel sharpness was not scored.
- Contrast figures are cluster samples from screenshots. They are not a WCAG audit. No focus rings, hit testing, or screen reader were exercised.
- Dark mode and Commands inform KernelLedger notes only.
- Footer UIA absence and the earlier DOM / 12.6px notes are called out because a missing artifact is not the same as a failed control.
- Versions are the installed Marimo 0.25.1 and the debug executable `54cdeeb07ee7bf332c191542962410d38475bd479d579eb640c754f588185e08`. Not current release, not performance, not the full editor.

## Summary

Marimo **7.3** (72.5/10) and KernelLedger **7.7** (76.5/10) on ten equal weights. The useful difference is that KernelLedger labels destinations and the main actions, and this 800×600 example still shows the error sentence. Marimo tells you how long each cell took, and its open Editor settings explain themselves. Polish and the error screen are ties: Marimo’s traceback scrolls sideways, and KernelLedger’s expanded traceback was not shown.

The earlier 7.6 versus 8.1 is arithmetically correct and directionally similar, but the 7.0 versus 8.5 readability gap and the 8.0 versus 7.0 space gap are not justified by the normalized captures. Settings and error scores compare unmatched states. The best KernelLedger change is to stop clipping the notebook title and cell previews. Next is keeping “Run all” in words at 800×600. A denser default is the earlier idea most likely to undo the readability people can actually see here.

Report path: `C:\Users\UTKARSH KAUSHIK\Documents\Projects\KernelLedger-v6\artifacts\ui-comparison-2026-11-05\grok-review.md`
