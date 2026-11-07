# Independent UI/UX review: Marimo 0.25.1 and KernelLedger (Claude Opus 5.5 xhigh, Cursor)

Reviewed 2026-11-05 from the saved captures in this folder. No app was launched or re-captured, and no settings were changed. No source code, notebook or existing artifact was edited. Scores were frozen in [opus-cursor-initial-scores.json](opus-cursor-initial-scores.json) before I opened any earlier review, score file or comparison HTML.

**Result: Marimo 71.0/100, mean 7.1. KernelLedger 78.0/100, mean 7.8.** Means are rounded half-up to one decimal. Image viewing was verified.

In these captured states, KernelLedger is easier to orient in, and it adapts far better to an 800×600 window. Marimo fits the same code cell into about 20% less height. It shows per-cell timing, and the Editor settings page that was open explains itself well. My KernelLedger lead (+7.0) is the largest of the three scored reviews. Section 7 explains why it should be read as one reviewer's judgement with halo risk.

## 1. Image viewing and provenance

**Status: verified.** Cursor's file reader rendered every PNG as a visible image: all six matched captures, plus `kernel-dark.png` and `kernel-commands.png`. The viewer downsampled each image to about 1024 px wide. For fine detail I wrote native-resolution crops and 4× nearest-neighbour zooms to `%TEMP%\opus-cursor-crops`. That folder is outside the repository and is not archived evidence.

| File | Raster px | Logical px | Raster ÷ logical |
|---|---:|---:|---:|
| `marimo-notebook.png`, `marimo-settings.png` | 1280×850 | 1280×850 | 1.0 |
| `marimo-narrow.png` | 1280×960 | 800×600 | 1.6 |
| `kernel-notebook.png`, `-settings`, `-dark`, `-commands` | 2538×1689 | ≈1280×850 (frame-trimmed) | ≈1.98 |
| `kernel-narrow.png` | 1604×1202 | 800×600 | ≈2.0 |

The viewer showed both desktop captures at the same 0.8× logical scale and both narrow captures at 1.28×, so side-by-side judgements were made at equal logical size. Screenshot sharpness was not scored. Section 7 discusses the residual risk that Kernel's 2× raster still looks crisper.

Phase 1 inputs were the eight PNGs, `session.json`, `capture.py`, `sales_analysis.py` and the four `kernel-*-uia.json` dumps. SHA-256 of the frozen initial file is `DE8A99BABAC27DA81B8DCC71B63790151C5F8B51AD9B2AF442BD030B1CBE05BD`.

`capture.py` launches both apps and captures the KernelLedger window, but it contains no Marimo screenshot code. The Marimo capture method (browser tool, viewport emulation, device scale) therefore cannot be reproduced from this folder.

## 2. Initial scores (frozen, unchanged)

Scale: 5 is workable with clear friction, 7 is good, 8 is strong for this fixture, and 9 is excellent with few reservations. Δ is KernelLedger minus Marimo. The full reasoning for each row is in the JSON.

| Dimension | Marimo | KernelLedger | Δ | Confidence | Main evidence |
|---|---:|---:|---:|---|---|
| Visual polish | 7.0 | 7.5 | +0.5 | low | Marimo: a red trash icon on every cell, a heavy in-cell scrollbar, a floating stack that mixes square and round buttons, and a Mac ⌘ glyph on Windows. Kernel: the name is truncated to "kernelle…" despite free top-bar space, an empty 31 px Markdown toolbar strip, an uncarded resting cell next to carded ones, and an active-line band shown in two editors at once. |
| Text readability | 7.5 | 8.0 | +0.5 | low | Prose and heading glyphs are not larger in Kernel. Kernel's advantages are looser leading (code pitch ≈20.8 vs ≈17.5 logical px) and darker, heavier text. Marimo uses more light-gray secondary text. These are perceptual judgements, not contrast measurements. |
| Visual hierarchy | 7.0 | 8.0 | +1.0 | medium | Kernel has one filled primary action and states failure three ways. Marimo leads with a gray file path, gives every cell the same weight, and has no primary action. Its Markdown source and open traceback are confounds. |
| Space efficiency | 7.5 | 7.0 | −0.5 | medium | The same cell in the same state measures ≈196 vs ≈240 logical px (Kernel +22%, in compact mode). Kernel also fits a 222 px outline into the same width. Marimo's settings page is denser. |
| Navigation clarity | 6.5 | 8.5 | +2.0 | medium | Kernel: labeled Notebook / History / Checkpoints, an open outline with failure dot, and a labeled settings list. Marimo: an 8-icon rail and 3 top-right icons, none with visible labels. |
| Action discoverability | 7.0 | 7.5 | +0.5 | low | Kernel shows about 7 actions in words, but 3 cell-1 buttons and 1 Markdown-card button exist in UIA without being painted. Marimo shows every per-cell control, but almost all are icon-only. |
| Execution feedback | 7.5 | 8.0 | +0.5 | low | Marimo: timings (848ms / 1ms / 172ms), an error count, and memory and CPU meters, with runtime state shown only as an icon. Kernel: [n] ordinals, Failed, Ready and Saved, but no timings. No running state was captured. |
| Error experience | 7.5 | 8.0 | +0.5 | low | Marimo: a full traceback with debugger and help actions, but one path line forces a horizontal scrollbar. Kernel: a one-line summary with Explain error and Show full traceback. The two are in opposite disclosure states. |
| Settings usability | 7.5 | 7.5 | 0 | low | Different pages were open. Marimo has rich helper text but trailing checkboxes at ragged x positions. Kernel has a clear section list, a 4-way theme control and a live preview, but only one control is visible. |
| 800×600 layout | 6.0 | 8.0 | +2.0 | medium | Kernel collapses its outline, keeps the full error sentence on screen and has no overflow. Marimo clips the path without an ellipsis, its floating stack crowds the timings, and the error message sits below the fold (partly a source-visible confound). |
| **Total / mean** | **71.0 / 7.1** | **78.0 / 7.8** | **+7.0** | | |

## 3. Evidence, by class

### 3.1 Measured: pixels or UIA rectangles, in logical px

- **Line pitch.** Code lines are ≈17.5 (Marimo) vs ≈20.8 (Kernel). Output lines are ≈17.5 vs ≈21. This agrees with the transcribed Marimo line-height of 17.64px.
- **Like-for-like cell size.** The "sales" cell has the same state in both apps: source visible, output visible.
  - Marimo's card runs from y 307 to 468 (161 px). Its slot, from the end of the Markdown card to the next card, is 289 to 485 (196 px).
  - Kernel's slot runs from 190 to 430 (240 px), with the run button and `[1]` on a separate header row.
  - This was measured with `workbench.json` set to `comfortable:false` and `ui_scale:1`.
- **Kernel's Markdown card** has a toolbar strip from y 59 to 90 (31 px) that contains only "Edit" and "…".
- **First-glyph ink heights** (±1 px; these are not font sizes):
  - Prose "C": Marimo ≈10.6–11, Kernel ≈10.0.
  - Heading "S": Marimo ≈24, Kernel ≈20.7.
  - Error "Z": Marimo ≈9 (monospace), Kernel ≈10.1 (sans).
  - Nothing here supports "Kernel's text is larger".
- **Kernel top bar (UIA).** The notebook switcher is 96 px wide and shows "kernelle…". About 417 px of empty bar lies between Checkpoints (right edge ≈400) and Commands (≈816).
- **Kernel control sizes.** Icon buttons are 28×30 with 4 px gaps. "Explain error" is 115×30 and "Show full traceback" is 140×30.
- **Present but unpainted controls.** UIA lists "Output tools", "Ask Assistant" and "More actions" for cell 1, and "Ask Assistant" for the Markdown card. None are painted at those coordinates in `kernel-notebook.png`; I checked native crops. Cell 3's corresponding buttons are painted.
- **State in UIA names is inconsistent.** Some names carry state: "Notebook, current destination", "Debugger, selected", "Cell 3: 1 / 0, failed", "…, unavailable". The selected settings section ("Appearance settings") and the selected theme ("Light") carry no state in the dumped fields.
- **UIA element counts.** The notebook dump has 29 Buttons, 3 Edits, 1 List, 3 ListItems and 2 Groups, and no Text nodes. No element at all represents the rendered Markdown cell's content area. Outputs, by contrast, have named Groups.
- **Duplicate dump.** `kernel-after-theme-uia.json` and `kernel-settings-uia.json` are byte-identical (SHA-256 `4791364B…1847`).
- **Marimo settings layout.** Checkboxes trail their labels at x ≈ 598–669, a spread of about 70 px.
- **Marimo first-cell timing** reads "848ms", confirmed in a 4× zoom.

### 3.2 Observed visually (seen, not measured)

- Marimo at 800×600:
  - The path header is clipped mid-word ("…ui-comparison-20") with no ellipsis.
  - The floating action stack (save, layout, ⌘, stop, run) sits beside the timing column; "172ms" nearly touches the stop circle.
  - The first cell's source soft-wraps.
  - Only the "ZeroDivisionError" line is above the fold.
- Kernel at 800×600:
  - The outline is hidden.
  - "Run all" becomes a filled icon-only play button.
  - No Export button is visible.
  - Notebook / History / Checkpoints keep their words.
  - All three cells, the error sentence and the status bar fit.
- "Ready" appears twice in Kernel, in the top bar and in the status bar.
- Marimo's runtime state is shown only by a lightning icon with a dropdown whose meaning I cannot read from the still.
- Kernel's "Output tools" glyph resembles a copy icon, and "Hide output" uses an eye-slash.
- The Marimo settings modal uses pastel colored icon tiles, blue uppercase section headers and helper text under nearly every option. It shows the version and locale and points to `marimo.toml`.

### 3.3 Aesthetic preference (my taste, low weight)

- Kernel's restrained warm-neutral palette and single accent read as calmer than Marimo's many small round icon buttons.
- Marimo's serif Markdown heading is attractive.
- The light Kernel settings page feels sparse rather than clean.

### 3.4 Untested hypotheses (need live validation)

- Kernel's resting-hidden toolbar buttons may receive keyboard focus without becoming visible.
- Rendered Markdown prose and footer status may be unreachable by screen readers.
- Settings and theme selection may not be exposed through `SelectionItemPattern`.
- Tooltips may mitigate both apps' icon-only controls.
- Marimo's "Toggle text wrapping" control may fix the traceback overflow. That claim comes from an earlier note and has not been verified.
- Whether Kernel's Export moved into the "…" overflow menu at narrow width.

## 4. Strongest findings

1. **Kernel's density gap is chrome, not type.**
   - The same cell is about 22% taller in Kernel's compact mode. The causes are a dedicated header row for the run button and ordinal, a 31 px mostly empty Markdown toolbar strip, and looser leading.
   - Earlier reviews treated density and readability as one tradeoff. In fact, most of the extra height can be recovered without touching text size or line pitch.
2. **Kernel's notebook-name truncation at 1280 px is not forced by space.** The switcher is fixed at about 96 px while about 417 px of the same bar is empty. This makes the case stronger than "add a reveal".
3. **Kernel hides some per-cell controls at rest while keeping them in the accessibility tree.** That is reasonable for visual calm. It still needs a live check that keyboard focus reveals them and that tab order does not land on invisible buttons.
4. **At 800×600, Kernel adapts and Marimo mostly does not.** Kernel removes the outline and shortens the toolbar. Marimo keeps the left rail, the right gutter and the floating stack, and clips the path. Marimo's visible Markdown source inflates the vertical cost (≈95 px here), but the horizontal crowding does not depend on that state.
5. **Labeled navigation is Kernel's most robust advantage.** All three scored reviews agree on its direction, by +1.5 to +2.0.
6. **The two apps report different kinds of execution state.** Marimo shows how long each cell took and how many errors exist. Kernel shows execution order, save state and runtime state in words. Neither has a running or queued state in these stills.
7. **Accessibility leads, not findings.**
   - The dump has no element for rendered Markdown content and no names for footer text.
   - Selection state is missing from names for settings sections and theme options.
   - Some controls are present but unpainted.
   - The script serialized only Name, Role, Enabled and Rectangle. The code editors' source text is also absent from every Name, even though editors normally expose it through Value or Text patterns. Absence in this dump is therefore not proof of a screen-reader failure.

## 5. Prioritized recommendations

No implementation is implied. Priority reflects strength of evidence × likely benefit ÷ risk.

### 5.1 KernelLedger

| # | Change | Intended benefit | Risk / tradeoff | Validate live |
|---|---|---|---|---|
| 1 | Run a live accessibility audit of what the dump could not show:<br>• rendered Markdown prose<br>• footer status (Ready, Saved, 3 cells)<br>• selected state of settings sections and theme buttons<br>• whether resting-hidden toolbar buttons become visible on keyboard focus<br>Fix only what fails. | Screen-reader and keyboard users get the same content and state that sighted users see. Focus never lands on an invisible control. | One UIA node per text span makes navigation chatty. Announcing every save is noise; announce state changes only. | Accessibility Insights for Windows (TextPattern, SelectionItemPattern, IsOffscreen, focus tracking), plus Narrator and NVDA, tabbing through a cell. |
| 2 | Let the notebook switcher use free top-bar width before truncating. Cap it at about 240 px and use a middle ellipsis so distinguishing suffixes survive. Show the full name on focus. Keep outline rows single-line, but reveal full source on focus, or as a second line for the selected row only. | Users can identify the notebook and cell without widening the outline. | Long names can push the destinations at narrow widths, so the cap is needed. Hover-only tooltips fail keyboard users. | 800×600 and 1280 windows, long names, similar-prefix names, ui_scale 1.25. |
| 3 | Make compact mode recover chrome, not type:<br>• fold the run button and `[n]` into the editor's left gutter, removing the header row<br>• turn the Markdown "Edit / …" strip into a corner overlay shown on hover, focus or selection<br>Keep code line pitch near 21 px. | About 30 px per code cell and about 31 px per Markdown cell, which closes most of the ≈44 px slot gap with Marimo. Readability is unaffected. | First-time users lose an always-visible per-cell run affordance. An overlay can cover the first heading line. Gutter targets must stay at least 28×30. | Task timing and mis-click rate on a 30+ cell mixed notebook. Confirm comfortable mode is unchanged. |
| 4 | At 800 px, test keeping the words "Run all" while History and Checkpoints collapse first. | Distinguishes run-all from per-cell run for sighted users. The secondary destinations currently keep their words while the primary action loses them. | The bar is already full, so something must move to overflow. | Run-one vs run-all tasks at 800 px, with long notebook names. |
| 5 | Make the resting state consistent. Use one container treatment for unselected cells and reserve the card for the selected cell. Show the active-line band only in the focused editor. | Removes the "why does only this cell have a card?" question in stills, and the double caret-line cue. | A weaker selection cue. The double band may be deliberate caret memory. | Keyboard cell navigation, the high-contrast theme, and selection visibility at a glance. |
| 6 | Optional, low-salience per-cell duration: on the `[n]` tooltip, or shown only above a threshold such as 1 s. Off or quiet by default. | Locates slow cells. This is the useful part of Marimo's gutter. | Always-on milliseconds add jitter and invite over-reading. Marimo's 848 ms import cell is mostly first-import cost, not slow user code. | Users' interpretation of single-run timings, and whether thresholding hides useful data. |
| 7 | Replace `KL_EXPERIMENTAL_INTERACTIVITY=1` in the disabled palette rows with a product sentence, and keep the variable under "Technical details". | The debugger stops looking broken. | Experts lose a one-glance fix unless the details stay one step away. Hiding the rows loses discovery. | First-run users opening Commands. |
| 8 | Review icon meaning in the cell toolbar: "Output tools" looks like copy, and the eye-slash is ambiguous between "hide" and "hidden". Show "Ready" in one place only, or make the two instances clearly different (menu vs status). | Fewer misreads. | Changing glyphs that experts have learned. | Tooltip check, then a 5-second icon-recognition test. |

### 5.2 Marimo

| # | Change | Intended benefit | Risk / tradeoff | Validate live |
|---|---|---|---|---|
| 1 | At 800 px or less, merge the floating action stack into the footer or a single overflow button, and fold the right gutter (timing, drag, expand) into the cell toolbar. Show the filename first in the path header, truncate from the left, and reveal the full path on focus or hover. | Removes the crowding at "172ms" and the mid-word path clip. Returns width to the code. | Experts lose one-click floating save and run, so keep shortcuts and keep the stack on wider windows. | 800×600 and about 1000 px split-screen tiling. |
| 2 | Check that the existing wrap toggle applies to tracebacks. Display traceback links as workspace-relative paths, keeping the full `marimo://` URI on hover and copy. | Removes the horizontal scrollbar caused by one path line. The exception sentence itself was already readable. | Copy fidelity, and ambiguity between same-named files. | Copy-paste and click-through on a deep path. |
| 3 | Labels on demand for the icon rail: verify focusable tooltips, and add an optional expanded labeled mode. Do not make labels permanent. | Faster first session. | A permanent labeled rail costs width that is already tight at 800 px. | First-session navigation tasks, then a returning-user check. |
| 4 | In settings, align controls in one column, either leading checkboxes or a right-aligned control column, and keep the helper text. | Faster scanning. The current spread is about 70 px. | Low. | Scan and toggle tasks across long sections. |
| 5 | Calm the per-cell chrome: keep the delete icon neutral until hover or focus, with the same position and hit target. | Red is reserved for errors, so the error cell stands out more. | Delete becomes slightly less discoverable. Only do this if undo exists. | Accidental-delete rate and undo discoverability. |
| 6 | Show runtime state in words in the footer (verify first what the lightning icon means). Use a Windows-appropriate glyph or "Ctrl" instead of ⌘. | Kernel and connection state readable at a glance, and platform fit. | Small footer crowding. | Disconnect and restart scenarios. |
| 7 | Run an accessible-name audit of icon-only controls, with a saved DOM dump. | Settles the earlier, unarchived claim that some icons lack names. | None for the UI itself. | axe or DevTools accessibility tree, plus NVDA in browser. |

### 5.3 Recommendations that could backfire for expert users

- **Heavy card borders.** Wrapping every cell and output in a bordered card ("boxes inside boxes") adds noise across long notebooks. Marimo's uniform cards are part of why its chrome reads busy, with three icon clusters per cell. Fix Kernel's inconsistency through the resting state and spacing, not more borders.
- **Forced wider rails.** Widening Kernel's 222 px outline to stop truncation, or making Marimo's rail permanently labeled, costs editor width exactly where it is scarcest (800–1000 px tiling). Use free top-bar space, focus reveal and labels on demand instead.
- **Smaller text or hitboxes.** Kernel's measurable readability edge is leading (≈21 vs ≈17.5 px). Its 28×30 targets with 4 px gaps are already compact for desktop pointing. Shrinking either to win density spends the advantage and raises mis-clicks. Recommendation 3 above shows density can come from chrome instead.
- **Default density.** I disagree with Grok here. "Don't chase density" is right for type and leading, but Kernel's compact setting is still about 22% taller per cell than Marimo, so compact should actually be compact. Leave the comfortable or default mode alone until a long-notebook test shows a need.
- **Always-on millisecond badges.** Single-run timings are noisy: 848 ms for `import marimo` is import cost. Always-on badges add visual jitter and invite benchmarking from one sample. Prefer a tooltip, a threshold or a setting.
- **My own recommendations carry risks too.** Folding Kernel's run button into the gutter can hurt first-run discovery. Hover-only Markdown "Edit" can hide editing; keep it visible on the selected cell. A neutral delete icon in Marimo needs undo.

## 6. Critique of earlier work

### 6.1 Scores side by side (arithmetic checked)

| Dimension | review.md M / K | grok-review.md M / K | This review M / K |
|---|---:|---:|---:|
| Visual polish | 8.0 / 7.5 | 7.5 / 7.5 | 7.0 / 7.5 |
| Text readability | 7.0 / 8.5 | 7.5 / 8.0 | 7.5 / 8.0 |
| Visual hierarchy | 8.0 / 8.0 | 7.5 / 8.0 | 7.0 / 8.0 |
| Space efficiency | 8.0 / 7.0 | 6.5 / 7.0 | 7.5 / 7.0 |
| Navigation clarity | 6.5 / 8.5 | 6.5 / 8.0 | 6.5 / 8.5 |
| Action discoverability | 6.5 / 8.0 | 7.0 / 8.0 | 7.0 / 7.5 |
| Execution feedback | 8.5 / 8.0 | 8.0 / 7.5 | 7.5 / 8.0 |
| Error experience | 8.0 / 8.5 | 7.5 / 7.5 | 7.5 / 8.0 |
| Settings usability | 8.0 / 8.5 | 8.0 / 7.5 | 7.5 / 7.5 |
| 800×600 layout | 7.0 / 8.0 | 6.5 / 7.5 | 6.0 / 8.0 |
| **Sum** | **75.5 / 80.5** | **72.5 / 76.5** | **71.0 / 78.0** |
| **Mean (half-up)** | **7.6 / 8.1** | **7.3 / 7.7** | **7.1 / 7.8** |

The earlier arithmetic is correct. All three reviews favour KernelLedger on readability, navigation, action discoverability and 800×600, so those directions are the robust part. They disagree on direction for polish, space, execution feedback and settings, so those rows should not carry conclusions. The two Antigravity score files are all-null. They are not additional votes, and I have not counted them.

### 6.2 Original review (`review.md`) and `comparison.html`

**Holds up**
- The scope section: version, hash, DPI, medium vs compact, the disclosure confound, and no accessibility or performance certification.
- The labeled-orientation advantage.
- The outline-truncation and per-cell-duration recommendations.

**Does not hold up**
- *"Kernel's larger text is easier to scan."* My glyph measurements show Kernel's prose and heading are not larger; Marimo's heading is about 15% taller. The real differences are leading and weight. The 1.5-point readability gap rested on a size claim, plus a 12.6px figure that was not archived at the time.
- **Space 8.0 vs 7.0.** The direction matches my like-for-like cell measurement. The justification ("compact code and narrow icon rail") is not isolated from state, and a 1.0 gap is more than one cell measurement supports.
- **Hierarchy tie.** This underweights Kernel's single primary action and its triple failure cue. Marimo's source-above-heading state also clouds this row.
- **Settings 8.0 vs 8.5 for Kernel.** This ranks one Appearance control above a full Editor page with helper text. The pages are unmatched.
- **Execution 8.5 vs 8.0.** Above 8 is over-precise for finished stills.

**Its recommendations**
- #1, "consistent boundaries … without excessive borders": the constraint is right. The inconsistency I see is state-driven (selected or hovered cells get cards), so the fix is the resting state.
- #2, the denser layout: more defensible than Grok allowed, once scoped to chrome within compact mode.
- #3, outline labels: this should have included the switcher, which truncates despite free space.

**`comparison.html` and `reviewer-comparison.html`**
- They headline one-decimal means without sums, confidence or confounds.
- Their bar colors come only from host-theme CSS variables (`var(--chart-1)` and `var(--chart-2)`, with no fallback). Outside the host renderer the bars may not be colored. This is a presentation issue, not a scoring one.

### 6.3 Grok review (`grok-review.md`)

**Strengths**
- Raster normalization.
- Line-pitch measurement (≈18 vs ≈21).
- Catching the narrow "Run all" icon.
- The raw env-var copy.
- The footer-name absence.
- Rejecting type shrinkage.
- A clean split between measured facts, preferences and gaps in evidence.

**Factual slip**
- "846ms": the capture reads 848ms. This is trivial, and the Antigravity Opus draft repeated it as "≈846–848".

**Disagreements**
- **Space, 6.5 vs 7.0 for Kernel.** It awards space to Kernel from the whole frame, which is dominated by Marimo's open states. The one like-for-like element, the "sales" cell, favours Marimo by about 20%. I would not give Kernel this row.
- **Settings, 8.0 vs 7.5 for Marimo.** This picks a winner across unmatched pages, and it missed Marimo's ragged checkbox column. I tie them.
- **"A denser layout is the recommendation most likely to backfire."** True for type and leading. Not true for chrome, as shown in 5.3.
- **"The dump may have omitted text runs."** The accurate wording is that the dump serialized four properties from every descendant. Text can live in patterns that were not serialized; the editors' code is the clearest example.

**Missed**
- Resting-hidden but present toolbar buttons.
- The empty Markdown toolbar strip.
- That the switcher truncation happens with free space.
- Marimo's floating-stack crowding at 800 px.
- The checkbox raggedness.
- Inconsistent state in UIA names.

### 6.4 Parent critique (`grok-critique.md`)

**Agree**
- Accepting the readability overclaim, the disclosure confounds and the settings mismatch, and archiving evidence.
- "A horizontal scrollbar is not automatically a defect." In this capture the exception sentence needs no scrolling. I deducted the scrollbar as polish and scan cost, not as a blocking error defect.

**Partly disagree**
- *"Truncation can be appropriate."* True in general, but here it occurs with about 417 px of free bar. That is not space-forced, so the evidence is stronger than "check the reveal".

**Correlated-scoring point**
- This is correct for the earlier tables. In mine, the lead survives collapsing navigation and discoverability into one row (Section 7). Correlation does not explain my gap; halo might.

### 6.5 Antigravity audits

**Opus draft (`opus-5-5-review.md`)**
- It correctly withheld scores, and its measurements match mine: pitch 17.7 vs 21, both truncations, and the narrow fold.
- The parent's correction stands: "Kernel's text is not larger" should read "no evidence Kernel's text is larger". My glyph heights agree in direction but are not font sizes.
- Its "space near parity" estimate is a whole-frame result. It hides a per-cell gap that favours Marimo, offset by Kernel's collapsed defaults. Parity in the frame is not parity in design.
- Its contrast proxy (3.3 vs 5.6) cannot rank text contrast. My perceptual note about Marimo's lighter secondary text agrees in direction and has the same limits.

**Gemini (`gemini-argon-review.md`)**
- It correctly withheld scores. Its UIA geometry is accurate: the 28×30 targets, 4 px gaps and 417 px gap match my reading.
- Its central overreach is "zero Text nodes proves rendered Markdown is absent from UIA". The parent correctly limits this: Group names carry output text, and patterns were not dumped. The editors' code is also missing from all Names, and nobody suggests the code is inaccessible.
- The stronger, narrower lead is that no element at all represents the rendered Markdown cell, while outputs get named Groups. Audit that first.
- Its point about the duplicate `kernel-after-theme` dump is confirmed by hash.
- Its expert-backfire list is sound. Its touch-gap remark does not apply to desktop pointing.

**`antigravity-outcomes-and-critique.md`**
- I agree with its corrections, and I confirm its counts: 29 Buttons, 3 Edits, 1 List, 3 ListItems, 2 Groups.

### 6.6 Method-level critique, all reviews

1. **Whole-frame comparisons mix design with state.** Comparing like-for-like elements in the same state is cheap and separates the two. The "sales" cell is one example. No earlier review used it.
2. **Rows that need hover or focus captures were scored from resting stills.** This applies to discoverability, polish, and part of navigation and accessibility. A single hover or focus capture per app would settle several disagreements.
3. **Some rows should be withheld.** Settings (unmatched pages) and error experience (opposite disclosure states) are better withheld or reported separately than averaged in.
4. **The fixture's provenance is incomplete.** The Marimo capture method is not in `capture.py`, and `kernel-after-theme-uia.json` duplicates the settings dump rather than the dark notebook tree.

## 7. Self-critique and sensitivity (not revised scores)

- **Possible halo.** Kernel scores at or above Marimo on 9 of 10 rows, ahead on 8. My +7.0 lead is the largest of the three scored reviews.
  - Contributing risks: the workspace is KernelLedger's repository, and Kernel's ~2× raster still looks crisper after downsampling.
  - Both risks could inflate my polish and readability rows, which is why both are low confidence.
- **Execution feedback diverges from both earlier reviews.** I weighted state shown in words (ordinals, Failed, Ready, Saved) above Marimo's timings. A reasonable reviewer could reverse this row.
- **Error experience credits Kernel's collapsed summary.** That is a captured state. If both apps are scored on matched states, I would expect a tie.

Sensitivity of my own scores (half-up):

| View | Marimo | KernelLedger | Gap |
|---|---:|---:|---:|
| All 10 rows | 7.1 | 7.8 | 0.7 |
| Without the state- or page-confounded rows (space, error, settings) | 6.9 (48.5/7) | 7.9 (55.5/7) | 1.0 |
| Same, with navigation and discoverability merged into one "labeled orientation" row | 7.0 (41.75/6) | 7.9 (47.5/6) | 0.9 |
| Medium-confidence rows only (hierarchy, space, navigation, 800×600) | 6.8 (27.0/4) | 7.9 (31.5/4) | 1.1 |

**No revised score set is proposed.** Phase 2 supplied no new visual evidence. The only new product fact is an unverified note that Marimo has a "Toggle text wrapping" control. Changing scores now would be agreement or disagreement, not evidence.

Matched states would most likely move these rows:
- Error experience toward a tie.
- Hierarchy toward about +0.5.
- Marimo's 800×600 score up by about 0.5 with source hidden.

These are hypotheses, not numbers.

## 8. Limitations

- One three-cell fixture, light mode for the paired captures, finished post-run stills only. There are no running, queued or stale states, and no hover, focus or keyboard paths. History, Checkpoints and the unopened settings sections were not seen.
- Captured states differ:
  - Marimo is `width="medium"`, with Markdown source visible and the traceback expanded.
  - Kernel uses compact scale 1, with source and traceback collapsed.
  - Settings pages differ: Marimo Editor vs Kernel Appearance.
- Raster scales differ: Marimo 1.0× and 1.6×, Kernel about 2×. Glyph ink heights are ±1 px raster measures, not CSS or font sizes. No semantic contrast was measured.
- UIA covers Kernel only, and the dump records only Name, Role, Enabled and Rectangle. There is no Marimo DOM dump, and the 12.6px / 17.64px figure is a labeled transcription, not fresh evidence.
- No performance, full-editor-capability or accessibility certification is claimed.
- Versions: the installed Marimo 0.25.1 and the existing KernelLedger debug executable `54cdeeb07ee7bf332c191542962410d38475bd479d579eb640c754f588185e08`. These are not the latest releases or fresh builds.
- My crops and measurement scripts live in `%TEMP%\opus-cursor-crops`, not in this folder. To reproduce the key measurement, scan pixel column x=600 in `marimo-notebook.png` and x=700 (×1.983) in `kernel-notebook.png`. The cell boundaries are at the y values listed in 3.1.

## 9. Files

- Initial scores (frozen): `C:\Users\UTKARSH KAUSHIK\Documents\Projects\KernelLedger-v6\artifacts\ui-comparison-2026-11-05\opus-cursor-initial-scores.json`
- This report: `C:\Users\UTKARSH KAUSHIK\Documents\Projects\KernelLedger-v6\artifacts\ui-comparison-2026-11-05\opus-cursor-review.md`
- Evidence folder: `C:\Users\UTKARSH KAUSHIK\Documents\Projects\KernelLedger-v6\artifacts\ui-comparison-2026-11-05`
