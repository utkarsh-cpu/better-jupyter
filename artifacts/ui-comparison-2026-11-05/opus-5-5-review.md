# Independent review: Marimo 0.25.1 and KernelLedger (Claude Opus 5.5 High)

Reviewed 2026-11-05 from the saved captures in this folder. No app was launched, nothing was re-captured, and no product code or notebook was edited.

> [!WARNING]
> **I could not view the screenshots as images.** I tried every image path available to me:
> - `view_file` returned the raw PNG bytes as text.
> - `client_view_file` rejected the PNG as a non-UTF-8 file.
> - `html_preview`, with the PNG inlined at 1280px and again at 480px, saved its screenshot to disk as a file reference that was never shown to me.
>
> As the brief requires, I made **no visual inspection and gave no scores**. [opus-5-5-initial-scores.json](opus-5-5-initial-scores.json) records `null` for every dimension, total and mean. It was written before I read any prior review, and it is unchanged.
>
> What follows is a **measurement audit**. It uses programmatic pixel statistics, Windows built-in OCR (text plus bounding boxes) and the KernelLedger UIA dumps. It checks the factual claims behind the earlier scores. Read it as a third data source, not a third visual rating.

## 1. Method

| Source | What it gives | What it cannot give |
|---|---|---|
| Pixel statistics (PIL/numpy), after resizing each raster to its logical size | Dominant colours, ink coverage, share of pale-red surface, a rough ink-vs-background luminance ratio | Polish, grouping, hierarchy. The contrast figure is a crude proxy that anti-aliasing distorts. It is not a WCAG measurement. |
| Windows.Media.Ocr, with boxes divided by the raster scale | Which words are on screen, where they sit, word-box heights, line pitch | Icons, white-on-fill text (it missed "Run all" even at full width), whether something is clipped or scrolled, colour meaning |
| `kernel-*-uia.json` | Kernel control names and bounds (physical px ÷ 2 = logical) | No Marimo equivalent exists here. The dump may also omit text runs. |
| `session.json`, `capture.py`, `sales_analysis.py` | Versions, DPI 192, executable hash, fixture text, Kernel profile `ui_scale: 1, comfortable: false` | — |

Raster scales I measured: Marimo notebook/settings 1.0, Marimo narrow 1.6, Kernel 1.983 (narrow 2.005). These match Grok's table exactly. I scored no sharpness. All positions below are **logical px**.

Scripts and raw output are in this conversation's scratch folder (`pixel_stats.py`, `pixel_stats.json`, `ocr.ps1`, `ocr_part1.txt`). They are not product files.

## 2. Observed facts (measured, not seen)

### Notebook, 1280×850

| Fact | Marimo | KernelLedger |
|---|---|---|
| Disclosure state (confound) | Markdown source visible (y≈116–147) above the rendered heading. Traceback expanded. | Rendered Markdown only. Traceback collapsed behind "Show full traceback". |
| H1 word-box height | ≈34 | ≈28 |
| Prose word-box height | ≈14 | ≈13.6 |
| Code word-box heights (lowercase words / with brackets) | ≈10 / 13–14 | ≈9.6 / 12.6 |
| Output word-box heights | 9–12 | 9–11 |
| **Code line pitch** | **≈17.7** (y 318, 336, 353, 371). This matches the prior note's 17.64px line-height. | **≈21** |
| Code text starts at x | ≈169 (centred `medium` column) | ≈307 (after a ≈250px cell outline) |
| Last content line | y≈748 ("Launch debugger", "Get help") | y≈580 ("+ Add cell"). Then about 240px empty above the status bar at y≈831. |
| Error text | "ZeroDivisionError", "division by zero", "Traceback (most recent call last)", "Cell marimo://C:…" with "line 1" pushed to x≈1089, the sentence repeated, then "Launch debugger" and "Get help" | "Failed" label, "Explain error", "ZeroDivisionError: division by zero", "Show full traceback" |
| Pale-red surface / saturated red | 2.6% / 0.43% (red text) | 4.6% / 0.09% (a tinted error band) |
| Run-state text | Per-cell times ≈846–848ms, 1ms, 172ms | Footer "Ready", "Saved", "3 cells", "Cell navigation". Top-bar UIA "Runtime: Python (Ready)". UIA cell list "Cell 3: 1 / 0, failed". No timing text. |
| Chrome text | OCR found no text labels in Marimo's rail or toolbar. That absence is not proof. | "Notebook", "History", "Checkpoints", "Export", "Edit". The notebook switcher reads **"kernelle…"**. Cell 2's preview reads **"= [120, 18…"**, while the UIA name is the full source. |
| Crude contrast proxy (median high-contrast ink : bg) | ≈3.3:1 | ≈5.6:1 |

### 800×600

| Fact | Marimo (1.6× raster) | KernelLedger (2.0× raster) |
|---|---|---|
| Top line | Full file path, clipped to "…ui-comparison-2…" | Labelled top bar. "Export" text is no longer detected. "Run all" is unreadable by OCR at both widths, so I can't confirm its narrow form. |
| Side columns | Both persist: icon glyphs bottom-left at x≈15, timing column at x≈713. Code starts at x≈154. | No outline text is detected, so the outline appears collapsed. Code starts at x≈86. |
| Error on the first screen | Only the "ZeroDivisionError" title, at y≈554 of 600. The message and traceback are below the fold. | Full sentence plus "Show full traceback" at y≈526. Footer at y≈579. |

**Untested estimate:** hiding Marimo's Markdown source would lift the content roughly 60–80px. That would likely bring "division by zero" above the fold at 800×600. It would also bring the 1280×850 frame's last line to roughly the same height as Kernel's. Most of the measured space and narrow-layout gap is disclosure state.

### Settings (different panes: Marimo Editor vs Kernel Appearance)

- **Marimo** is a centred modal about 900px wide (ink x≈194–1096).
  - Nav: Editor, Display, Packages & Data, Runtime, one entry OCR didn't read, Optional Dependencies, Labs.
  - Also shows version 0.25.1, locale and the config-file location.
  - Editor pane: AUTOSAVE, FORMATTING, AUTOCOMPLETE and LANGUAGE SERVERS, about 12 controls, each with helper text (word boxes ≈11–12px at 1×).
- **Kernel** is a full page.
  - Nav: Appearance, Editor, Layout & accessibility, Runtime, Assistant.
  - Appearance pane: one 4-way control (System / Light / Dark / High contrast), a one-line explanation, and a live preview labelled "Sample cells. Nothing here runs."
- **Density:** 76% of Marimo's rows contain high-contrast ink versus 30% of Kernel's.

### Supplementary Kernel captures (unpaired, not comparative)

- **Commands palette:** about 544×348, over a scrim that covers 62% of the frame with grey.
  - Rows have two lines: title and description.
  - The disabled rows are named "…, unavailable" in UIA and show the visible reason **"Debugger requires an opt-in runtime started with KL_EXPERIMENTAL_INTERACTIVITY=1"**.
  - An "Esc to close" hint is present.
- **Dark theme:** background (20,20,20), with the same structure as light.
- **Footer accessibility:** "Ready", "Saved", "3 cells" and "Cell navigation" are not names in `kernel-notebook-uia.json`. This is a fact about the dump. It does not prove what a screen reader would announce.

## 3. Scores

**None.** [opus-5-5-initial-scores.json](opus-5-5-initial-scores.json) is all-null, so there is no sum, mean or half-up rounding to report. I am also **not proposing a revised score set** after reading the prior reviews. Turning their descriptions into my numbers would be scoring from text, which the brief rules out.

Instead, this table checks each prior score against what the measurements can verify:

| Dimension | review.md M / K | Grok M / K | What my measurements say |
|---|---:|---:|---|
| Visual polish | 8.0 / 7.5 | 7.5 / 7.5 | **Can't check.** Pixels only confirm that Kernel layers white surfaces on an off-white base, while Marimo is a single near-white field. |
| Text readability | 7.0 / 8.5 | 7.5 / 8.0 | **Contradicts review.md's premise.** Kernel's glyph boxes are not larger: prose, code and output heights are equal or slightly smaller than Marimo's, and the H1 is smaller. The measured difference is line pitch (≈21 vs ≈17.7), plus a crude contrast proxy that favours Kernel. A 1.5-point gap built on "Kernel's larger text" is unsupported. Grok's half-point (spacing, not size) is consistent with the data. A tie would be too. |
| Visual hierarchy | 8.0 / 8.0 | 7.5 / 8.0 | **Can't check.** Only head-size is measured, and Marimo's H1 is larger. Grok's half-point leans partly on the source-visible state. |
| Space efficiency | 8.0 / 7.0 | 6.5 / 7.0 | **Opposite winners, both confounded.** In this state Marimo fills more of the frame, but with source and traceback open. Kernel ends earlier, but spends about 250px of width on the outline at 1280. My estimate with matched disclosure is near parity. **Either winner is unsupported.** |
| Navigation clarity | 6.5 / 8.5 | 6.5 / 8.0 | **Partly supports.** Kernel's chrome carries text labels and Marimo's OCR found none. The "kernelle…" and "= [120, 18…" truncations are confirmed, which backs Grok trimming Kernel from 8.5. |
| Action discoverability | 6.5 / 8.0 | 7.0 / 8.0 | **Partly supports.** Kernel exposes "Explain error", "Show full traceback" and "Edit" as words, plus named icon buttons in UIA. Marimo shows "Launch debugger" and "Get help" as words on the error, which supports Grok raising Marimo. |
| Execution feedback | 8.5 / 8.0 | 8.0 / 7.5 | **Supports a different signal, not a winner.** Marimo shows timings. Kernel shows Ready / Saved / Failed. Stills can't show running, queued or stale states, so anything above 8 is over-precise. |
| Error experience | 8.0 / 8.5 | 7.5 / 7.5 | **Confounded.** Both name the exception on the failing cell. Kernel's tinted band is larger. Marimo's long path line is real. Preferring Kernel's collapsed summary uses the disclosure asymmetry, so Grok's tie is the more defensible of the two. |
| Settings usability | 8.0 / 8.5 | 8.0 / 7.5 | **Different panes, inconclusive.** The data confirms Marimo's pane is denser and self-explaining, and Kernel's is one control plus a preview. Neither winner follows. I agree with the parent critique. |
| 800×600 | 7.0 / 8.0 | 6.5 / 7.5 | **Supports Kernel in this state, but fragile.** Kernel keeps the error sentence above the fold and drops the outline. Marimo keeps both side columns plus a clipped path title. Part of the gap disappears if Marimo's source is hidden. The persistent columns and path title would not. |

Arithmetic check: review.md sums to 75.5 / 80.5 (means 7.55 → 7.6 and 8.05 → 8.1). Grok sums to 72.5 / 76.5 (7.25 → 7.3 and 7.65 → 7.7). Both are correct.

## 4. Critique of the earlier work

### What holds up

- **Grok's factual layer is corroborated** by my independent measurements:
  - raster scales;
  - line pitch (≈18 vs ≈21);
  - the "kernelle…" and Cell 2 truncations;
  - the clipped Marimo path at 800×600;
  - Marimo's side columns persisting at 800px;
  - the error sentence above the fold in Kernel;
  - the env-var text in the palette;
  - footer names missing from the UIA dump.

  I can't verify that Grok actually saw the images, but every checkable fact matched.
- **review.md's scope section is good.** It names the version, hash, DPI, density settings and disclosure confounds, and it declines accessibility and performance certification.
- **The parent critique's corrections are right:** the settings mismatch, "a scrollbar is not automatically a defect", truncation is acceptable when a reveal exists, and the correlated-dimension warning.

### Where I disagree, and how it changes the conclusions

1. **The readability claim is wrong at the glyph level, not just over-scored.** review.md says Kernel's "larger text is easier to scan". The OCR word boxes do not show larger text; they show looser leading. That matters for the recommendations. "Kernel is readable because it is bigger" points to keeping the type size. "Kernel is readable because of its leading" points to line-height as the variable to protect in any density change. The parent's acceptance ("roomier, a preference") is closer, but still doesn't name the variable.
2. **Correlated dimensions drive the lead.** In review.md, Kernel leads by 5.0 points total. Navigation (+2.0), action discovery (+1.5) and 800×600 (+1.0) are all largely the same construct: visible text labels. In Grok, those three give +3.5 of the +4.0 lead. If you remove the three confounded rows (space, error, settings), Grok's remaining lead is about +3.5 over seven dimensions, almost all from labels.

   So both averages say one thing: **Kernel is easier to orient in on a first visit.** They do not support "Kernel has better UI/UX" overall. Expert-session efficiency was never observed.
3. **Grok still picks winners in two confounded rows.** It gives space to Kernel 7.0 vs 6.5, and settings to Marimo 8.0 vs 7.5, while its own text says both are confounded. Abstaining, or flagging those rows as low confidence and leaving them out of the mean, would be more honest than a half-point either way.
4. **Hierarchy (Grok +0.5 Kernel) partly relies on the visible red Markdown source.** That is a state, not a design. Marimo's H1 is also measurably larger. I'd treat this row as unresolved.
5. **Publishing one-decimal means (7.6 / 8.1 and 7.3 / 7.7) invites false precision.** Two of ten rows are disclosure-confounded and one is pane-mismatched. A direction plus "first-visit orientation" is the defensible finding.

### Which prior recommendations are supported, speculative or risky

| Recommendation | Status | Why |
|---|---|---|
| Kernel: fix identifier truncation / provide a reveal | **Supported** | Both truncations are OCR-confirmed. Whether a reveal already exists is untested. |
| Kernel: plain-language reason instead of the env var | **Supported** | The text is OCR-confirmed. |
| Kernel: optional per-cell duration | **Supported as optional** | Marimo's timings are confirmed. The benefit is untested. |
| Kernel: keep "Run all" labelled at 800px | **Speculative from my data** | OCR can't read "Run all" at either width, so the narrow icon form is Grok's observation, not mine. It is a design hypothesis to test. |
| Kernel: footer exposed to UIA | **Hypothesis** | Absence from the dump is confirmed. Absence from a screen reader is not. |
| Kernel: denser layout (review.md #2) | **Risky for readability; test first** | Glyphs are already the same size as Marimo's. Density can only come from leading and gaps, which are the measured readability difference. Expert users may still want it as an option. |
| Kernel: unify cell boundaries | **Can't evaluate** | It is purely visual. |
| Marimo: wrap the traceback path | **Plausible, verify first** | A long path line is OCR-supported. The prior DOM note mentions a "Toggle text wrapping" control whose behaviour is untested. Wrapping by default could hurt copying. |
| Marimo: labelled rail on demand | **Plausible** | No chrome text was found. That doesn't prove there are no tooltips or labels elsewhere. A persistent labelled rail would annoy experts. |
| Marimo: larger code font | **Not supported by comparison** | Marimo's code glyph boxes match Kernel's. 12.6px is small but common for code. Any change should be about leading, not size. |

## 5. My recommendations

Priority reflects the evidence strength and the expected benefit. Each item is a proposal, not an implementation.

### KernelLedger

| # | Change | Intended benefit | Risk / tradeoff |
|---|---|---|---|
| 1 | Give truncated identifiers a reliable reveal: show the full notebook name in the native window title and in a focusable tooltip on the switcher, and let outline rows show a second line or a full preview on focus. Check first whether a reveal already exists. | Users can tell notebooks and cells apart. "kernelle…" and "= [120, 18…" happen even on a three-cell fixture. | Widening the outline by default costs editor width at 1280. Hover-only tooltips fail for keyboard users. |
| 2 | Replace `KL_EXPERIMENTAL_INTERACTIVITY=1` in the disabled palette rows with a product sentence ("Debugger needs an experimental runtime"). Put the variable under a "Technical details" disclosure. | First-time users don't read a raw env var as "broken" or "go edit my environment". | Experts lose a one-glance fix unless the details stay one step away. Hiding the rows entirely would reduce discovery. |
| 3 | Verify the footer status ("Ready", "Saved", "3 cells") with Narrator or Accessibility Insights. If it isn't exposed, give it named text peers and announce changes through a polite live region. | Status the eye sees becomes reachable for screen-reader users. | Announcing every save would be noisy, so announce on state change only. The dump may simply omit text runs, so verify before fixing. |
| 4 | Add an optional, visually secondary per-cell duration next to the existing state. | Context for slow cells, matching a Marimo signal that was observed. | Always-on milliseconds add noise and invite performance conclusions from single runs. Keep it off or quiet by default. |
| 5 | Before any density work, A/B test line-height and inter-cell gaps (not font size) on a notebook with 30+ mixed cells at 1280 and 800px. Keep the current spacing as an option. | It protects the variable that measurably differs (≈21 vs ≈17.7 code pitch) while answering the expert demand for more lines on screen. | A tighter default could erase the readability edge both prior reviewers credited. A looser one wastes space on long notebooks. |
| 6 | Test the narrow-width primary action: labelled "Run all" vs an icon, with run-one vs run-all tasks at 800px. | It resolves whether the icon form causes real ambiguity. | The top bar is already full, so another action must move to overflow. |

### Marimo

| # | Change | Intended benefit | Risk / tradeoff |
|---|---|---|---|
| 1 | At narrow widths, lead the title with the notebook filename and show the full path on focus or hover. | The 800px title line currently spends its width on "C:…KAUSHIK\Documents\…" and clips the part that identifies the notebook. | Same-named notebooks become ambiguous unless the path stays one step away. |
| 2 | Check that the existing wrap toggle applies to traceback path lines. Consider wrap-on by default for expanded tracebacks, paired with a copy action. | The long path line ("line 1" at x≈1089) reads without a second scroll axis. | Wrapped paths are harder to read as one unit. Experts copying paths want fidelity. |
| 3 | Keep the exception message in the first screen on short windows, for example by pinning the error header when output is clipped. | At 800×600 only "ZeroDivisionError" is visible. | It is partly caused by the visible source, and auto-collapsing source would fight users who opened it deliberately. |
| 4 | Offer labels on demand for the icon rail (expanded mode or visible tooltips). | Faster first-session orientation. No chrome text was detected. | A persistent labelled rail costs width and annoys experts who already know the icons. |
| 5 | Audit secondary and syntax-token contrast against computed colours. | My crude proxy put Marimo's median high-contrast ink at ≈3.3:1 vs ≈5.6:1. If real, some secondary text may be near or below 4.5:1. | The proxy is anti-aliasing-sensitive at a 1× raster and may be an artefact. Don't act without computed-style data. |

### For the review process

1. Recapture with matched disclosure: Markdown source hidden in Marimo, tracebacks collapsed in both, then expanded in both. Use the same settings pane (Editor in both).
2. Save computed-style, DOM and UIA dumps next to the PNGs at capture time.
3. Score first-visit orientation and expert efficiency as separate constructs, so labelled-chrome rows don't stack.

## 6. Limitations and major objections

- **No image viewing.** Polish, hierarchy, grouping, iconography, colour use and clipping-vs-scrolling are all outside this review. My measurements can corroborate or contradict factual claims. They cannot support or refute aesthetic ones.
- OCR misses white-on-fill text and icons, so its silence about Marimo's rail labels and Kernel's narrow "Run all" means "not detected", not "absent".
- My contrast figures are a crude proxy, not WCAG. OCR box heights at a 1× raster are quantised to about ±1px.
- One fixture, light mode, finished stills. No running states, keyboard paths, History or Checkpoints contents, or unopened settings categories. Marimo's dark theme is unseen.
- Versions: installed Marimo 0.25.1 and Kernel debug executable `54cdeeb07ee7bf332c191542962410d38475bd479d579eb640c754f588185e08`. Not latest releases, not performance, not the full editor, not an accessibility certification.

## 7. Summary

- **Scores:** none (`null` × 20), because image viewing failed. The initial-score file is unchanged, and no revised score set is proposed.
- **Strongest measured findings:**
  1. Kernel's text is not larger than Marimo's. Its measured difference is looser code leading (≈21 vs ≈17.7 logical px).
  2. Kernel truncates the notebook name and cell previews on a three-cell notebook.
  3. At 800×600, Kernel keeps the full error sentence above the fold while Marimo shows only the exception title. This is partly due to Marimo's visible source.
  4. Kernel's palette exposes a raw env var to users.
  5. Kernel's footer status text is absent from the UIA dump.
- **On the prior scores:** both rank Kernel ahead, mostly on correlated "visible labels" dimensions. That supports "easier first-visit orientation", not overall superiority. review.md's 1.5-point readability gap rests on a size claim my OCR contradicts. Grok's factual observations all checked out, but it still picks winners in rows it calls confounded.

Report: `C:\Users\UTKARSH KAUSHIK\Documents\Projects\KernelLedger-v6\artifacts\ui-comparison-2026-11-05\opus-5-5-review.md`
Initial scores: `C:\Users\UTKARSH KAUSHIK\Documents\Projects\KernelLedger-v6\artifacts\ui-comparison-2026-11-05\opus-5-5-initial-scores.json`
