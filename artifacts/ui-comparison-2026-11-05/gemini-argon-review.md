# Independent UI/UX Review & Methodological Critique: Marimo 0.25.1 vs KernelLedger

- **Reviewer:** `Gemini 4 Argon High` (running in Antigravity ACP)
- **Date:** 2026-11-05
- **Workspace:** `C:/Users/UTKARSH KAUSHIK/Documents/Projects/KernelLedger-v6`
- **Evidence Folder:** `C:/Users/UTKARSH KAUSHIK/Documents/Projects/KernelLedger-v6/artifacts/ui-comparison-2026-11-05`
- **Initial Score Record (written prior to reading any existing reviews):** `C:/Users/UTKARSH KAUSHIK/Documents/Projects/KernelLedger-v6/artifacts/ui-comparison-2026-11-05/gemini-argon-initial-scores.json`
- **Applicable Design Skill Consulted:** `.agents/skills/ui-ux-pro-max/SKILL.md` and `.agents/skills/ui-ux-pro-max/references/quick-reference.md`

---

## 1. Mandatory Disclosure: Raster Image Viewing Failed in Harness (No Invented Visual Inspection or Scores)

Per the explicit task rule (*"You MUST distinguish seeing images from reading textual descriptions; if image viewing fails, say so and do not invent a visual inspection or scores"*), **multimodal raster pixel viewing failed structurally in this Antigravity ACP (`antigravity-acp`) session**:

1. **`client_view_file` on PNGs failed outright:** Calling `client_view_file` on `marimo-notebook.png` threw `File '...\marimo-notebook.png' is not a UTF-8 text file.`
2. **`view_file` is shadowed by a text-only wrapper:** In `antigravity-acp`, `view_file` executes as an `AGENCY_TOOL_CALL` returning a JSON string (`{"result": "\ufffdPNG..."}`) decoded as UTF-8 with replacement characters rather than Antigravity's native multimodal `VIEW_FILE` attachment. Inspection of this session's own `transcript.jsonl` (steps 3, 4, 17) confirmed **zero `"media"` image blocks** were attached to model context.
3. **MCP `html_preview` image output was offloaded without multimodal attachment:** Rendering `marimo-notebook.png` via `t3-code`'s `html_preview` returned metadata (`1280×866`) and offloaded the image to `.system_generated/steps/15/media_0.png` (`[Resource offloaded to file:///...]`) with **zero `"media"` attachment** in `transcript.jsonl` (step 15); calling `view_file` on `media_0.png` hit the same text-only wrapper.

### Epistemic Consequence for Scoring
- **Phase-1 Initial Scores (`gemini-argon-initial-scores.json`):** Recorded strictly **before** reading `review.md`, `grok-review.md`, `grok-critique.md`, `prior-computed-style-note.json`, or `comparison.html`. All ten dimensions were recorded as `null` (**Unscored**) with `"confidence": "none"`, accompanied by the non-visual structural facts independently verifiable from `session.json`, `capture.py`, `sales_analysis.py`, PNG IHDR binary headers, and `kernel-*-uia.json`.
- **Post-Reading Revision Decision:** `gemini-argon-initial-scores.json` is retained unchanged, and **no "proposed numerical revision" is fabricated after reading prior reviews**. Translating `review.md` and `grok-review.md`'s prose descriptions into synthetic numbers would launder secondhand text as visual observation. Instead, Section 3 audits both prior score tables dimension-by-dimension against verified structural geometry, capture confounds, and `ui-ux-pro-max` criteria.

---

## 2. Separation of Evidence Classes

### A. Directly Verified Facts (Independently Audited by This Reviewer Without Vision)
- **Software Provenance (`session.json`, `capture.py`):** Installed Marimo `0.25.1` (`--headless --no-token --skip-update-check --no-sandbox`) vs local debug executable `desktop/target/debug/kernelledger-desktop.exe` (`SHA-256: 54cdeeb07ee7bf332c191542962410d38475bd479d579eb640c754f588185e08`). Neither represents current upstream/HEAD releases, performance benchmarks, or full editor capability.
- **Identical Notebook Payload (`capture.py`, `sales_analysis.py`):**
  - Cell 1 (Markdown): `# Sales analysis\nCompare a small dataset, inspect the result, and diagnose an error.`
  - Cell 2 (Code): `sales = [120, 180, 150, 210]`, `total = sum(sales)`, `print(f"Total sales: {total}")`, `print(f"Average: {total / len(sales):.1f}")` -> recorded output `Total sales: 660\nAverage: 165.0\n`.
  - Cell 3 (Code): `1 / 0` -> recorded error `ZeroDivisionError: division by zero`.
- **Raster Dimensions & DPI Normalization (24-byte PNG IHDR headers + `session.json`):**
  - Native Windows DPI = `192` (`2.0×` scale factor). `capture.py` called `MoveWindow` with `round(1280 * 192 / 96) = 2560` × `round(850 * 192 / 96) = 1700` physical px.
  - `marimo-notebook.png` & `marimo-settings.png`: **`1280 × 850`** physical px (`1.00×` raster-to-logical).
  - `marimo-narrow.png`: **`1280 × 960`** physical px (`1.60×` raster-to-logical for `800 × 600` logical).
  - `kernel-notebook.png`, `kernel-settings.png`, `kernel-dark.png`, `kernel-commands.png`: **`2538 × 1689`** physical px (`1.983×` × `1.987×` raster-to-logical, i.e., `~1269 × 844.5` logical visible bounds after Windows DWM invisible frame trim).
  - `kernel-narrow.png`: **`1604 × 1202`** physical px (`2.005×` × `2.003×` raster-to-logical for `800 × 600` logical).
- **Configured Density Asymmetry (`capture.py`, `sales_analysis.py`):** Marimo ran with `app = marimo.App(width="medium")`; KernelLedger ran with an isolated profile setting `appearance.txt = "light"` and `workbench.json = {"schema": 2, "ui_scale": 1, "comfortable": false}` (compact density).
- **Normalized KernelLedger Logical Geometry (`kernel-notebook-uia.json`, dividing 192-DPI physical coordinates by `2.0`):**
  - **Top bar (`y = 7.5..37.5` logical, height `30` logical px):** Left cluster holds `Outline` (`28×30`), `Notebook switcher: kernelledger-notebook` (`96×30`), `Notebook, current destination` (`83×30`), `History` (`64×30`), `Checkpoints` (`96×30`), followed by a **`417` logical px center gap** (`x = 399.5` to `816.5`), then right cluster `Commands` (`28×30`), `Runtime: Python (Ready)` (`74×30`), `Run all` (`78×30`), `Export notebook` (`76×30`), `Settings` (`28×30`), `Menu` (`28×30`).
  - **Desktop pointer target sizes & gaps:** Compact icon-only buttons across header and cell toolbars measure **`28 × 30` logical px** (`56 × 60` physical px) with **`4` logical px** (`8` physical px) horizontal gaps between adjacent buttons (e.g., `Settings` at `x=1103.5..1131.5`, `Menu` at `x=1135.5..1163.5`; cell toolbar icons at `x=1095.5, 1127.5, 1159.5, 1191.5`). These satisfy WCAG 2.2 AA `web-target-size` (`≥24 × 24` CSS/logical px), though the `4` logical px gap is tighter than touch guidelines (`≥8px`).
  - **Sidebar vs Editor Allocation:** Outline list (`x = 6.5, w = 217, h = 707` logical) leaves `w = 964` logical px (`75.3%` of `1280` logical width) for cell editors (`x = 255.5..1219.5`).
  - **Vertical Fit:** All 3 cells, outputs, error banner, and `Add cell at end of notebook` (`y = 565..595` logical) fit well within the `850` logical px viewport height, leaving `~218` logical px above the bottom-right `Assistant` button (`y = 813.5..843.5` logical).
  - **Contextual Error & Outline Semantics:** Cell 3's outline item exposes failure state in its accessible name (`Cell 3: 1 / 0, failed`), its cell header adds an explicit `Explain error` button (`115 × 30` logical px), and its output group (`Recorded execution error: ZeroDivisionError: division by zero`) exposes `Show full traceback for Output 1 for cell 3: 1 / 0` (`140 × 30` logical px).
- **Critical UIA Discovery Missed or Misdiagnosed in Prior Reviews (`capture.py:82` + `kernel-*-uia.json`):**
  - `capture.py` line 82 queries `$root.FindAll([TreeScope]::Descendants, [Condition]::TrueCondition)`—an **unfiltered** UIA descendant dump.
  - Across all four UIA dumps, there are **zero `ControlType.Text` elements anywhere** (only `Button`, `Edit`, `List`, `ListItem`, `Group`, `Pane`, `Window`).
  - Consequently, **rendered Markdown cell content in view mode** (`# Sales analysis` and `Compare a small dataset, inspect the result, and diagnose an error.`), Settings explanatory copy, and static footer status labels are **completely absent from KernelLedger's Windows UIA tree** in build `54cdeeb0...`! Only Cell 1's toolbar button names (`Ask Assistant about # Sales analysis`) hint at Cell 1's heading, while its body paragraph is nowhere in UIA.
- **Artifact Coverage Gaps & Duplicate Dump:**
  - `kernel-after-theme-uia.json` (`3403` bytes) is byte-for-byte identical in node structure and coordinates to `kernel-settings-uia.json` (`3403` bytes)—it captured the Settings screen, not `kernel-dark.png`'s notebook view.
  - No `kernel-narrow-uia.json` or Marimo DOM dump exists in the evidence folder. `prior-computed-style-note.json` explicitly labels a transcription from an earlier `800×600` live `preview_evaluate` call (`"Fira Mono", monospace`, `fontSize: "12.6px"`, `lineHeight: "17.64px"`).
  - `comparison.html` omits the unrounded score totals (`75.5`, `80.5`), omits disclosure/settings confounds, and styles its chart bars with `background:var(--chart-1)` / `var(--chart-2)` without CSS fallback colors—meaning bars render transparent if opened directly in a standalone browser outside the host theme stylesheet.

### B. Secondhand Visual Claims Reported by Prior Reviewers (Unseen by This Reviewer)
Both `review.md` and `grok-review.md` (which had image access) converge on the following visual observations, treated here strictly as **reported visual evidence from prior reviewers**, not direct inspection by Gemini 4 Argon High:
- Marimo displays Markdown source alongside rendered Markdown, an open traceback with a horizontal scrollbar on a long local file path, per-cell execution durations (`846ms`, `1ms`, `172ms`), an error count (`1`), `Launch debugger` / `Get help` actions, and icon-only left/right rails.
- KernelLedger displays rendered Markdown inside a bordered card while code cells sit on open bands, truncates the header notebook title (`kernelle…`) and Cell 2 outline preview (`sales = [120, 18…`), shows `[1]`/`[2]` ordinals and a `Failed` badge, collapses the outline at `800×600` while turning `Run all` into a play icon, and renders a non-interactive footer (`Cell navigation`, `3 cells`, `Ready`, `Saved`).
- `marimo-settings.png` shows the `Editor` category with helper text, version `0.25.1`, locale, and config path; `kernel-settings.png` shows the `Appearance` category (`System`, `Light`, `Dark`, `High contrast`) above an `800 × 253` logical live preview card with open space below.
- `kernel-commands.png` shows disabled debugger items mentioning `KL_EXPERIMENTAL_INTERACTIVITY=1`.

### C. Subjective Judgments vs D. Untested Hypotheses
- **Subjective judgments:** Whether uniform bordered cell cards (Marimo) look cleaner than card-for-Markdown + open-band code cells (KernelLedger); whether generous cell spacing reads as "calm" or "low-density"; how heavily first-time label clarity should be weighted against expert workspace compactness.
- **Untested hypotheses:** Hover/focus tooltip behavior on truncated KernelLedger labels and Marimo rail icons; behavior of Marimo's `"Toggle text wrapping"` button on tracebacks; quality of KernelLedger's expanded traceback or `Explain error` assistant response; contents of KernelLedger's unopened `Editor` and `Layout & accessibility` settings sections; live streaming/running execution feedback; keyboard-only tab order; and actual NVDA/Narrator screen-reader output.

---

## 3. Dimension-by-Dimension Score Table & Evidentiary Audit

**Gemini 4 Argon High Initial & Retained Scores:** `null` (Unscored across all 10 dimensions; image viewing failed in harness). Prior reviewer arithmetic was independently verified (`review.md`: `75.5 / 10 = 7.55 -> 7.6` vs `80.5 / 10 = 8.05 -> 8.1`; `grok-review.md`: `72.5 / 10 = 7.25 -> 7.3` vs `76.5 / 10 = 7.65 -> 7.7`, half-up to 1 decimal).

| Dimension | `review.md` (M / KL) | `grok-review.md` (M / KL) | Gemini 4 Argon High Initial / Retained | Max Defensible Confidence from This Evidence Set | Structural & Methodological Assessment |
|---|---:|---:|---:|---|---|
| 1. Visual polish | 8.0 / 7.5 | 7.5 / 7.5 | `null` / `null` | **Medium** (requires pixel vision) | Purely visual judgment; asymmetric defects reported by visual reviewers (Marimo in-cell horizontal scrollbar vs KernelLedger mixed card/band containers and truncated `kernelle…` title) make a narrow margin or tie more defensible than a firm winner. |
| 2. Text readability | 7.0 / 8.5 | 7.5 / 8.0 | `null` / `null` | **Medium** (requires pixel vision + matched CSS metrics) | `prior-computed-style-note.json` records Marimo `.cm-content` at `12.6px` / `17.64px` (`Fira Mono`), while KernelLedger's exact CSS/logical font-size was never dumped. Grok's DPI-normalized line-pitch check (`~18px` vs `~20–22px` logical) refutes `review.md`'s 1.5-point gap: `12.6px` monospace is standard IDE density, not a readability failure. |
| 3. Visual hierarchy | 8.0 / 8.0 | 7.5 / 8.0 | `null` / `null` | **Medium** (confounded by Markdown source state) | KernelLedger UIA confirms clear structural zoning (top bar `y=7.5..37.5`, outline `w=217`, editor `w=964`, distinct output/error groups). Penalizing Marimo because raw Markdown source appeared above `# Sales analysis` scores an open-editor session state rather than fixed hierarchy. |
| 4. Space efficiency | 8.0 / 7.0 | 6.5 / 7.0 | `null` / `null` | **Low** (heavily confounded) | Severely confounded by *both* configured settings (Marimo `width="medium"` vs KernelLedger `comfortable=false`, `ui_scale=1`) and disclosure states (Marimo open Markdown source + expanded traceback vs KernelLedger collapsed Markdown + collapsed traceback fitting within `y=595/850` logical px). Neither `8.0/7.0` nor `6.5/7.0` isolates product efficiency from capture state. |
| 5. Navigation clarity | 6.5 / 8.5 | 6.5 / 8.0 | `null` / `null` | **Medium-High** (for desktop sight-reading) | KernelLedger UIA confirms labeled top-level destinations (`Notebook`, `History`, `Checkpoints`), `Filter cells`, and status-tagged cell items (`Cell 3: 1 / 0, failed`), tempered by visible truncation inside the `96px` logical switcher and `201px` logical outline rows. Marimo's icon-only rail imposes higher first-use recall cost. |
| 6. Action discoverability | 6.5 / 8.0 | 7.0 / 8.0 | `null` / `null` | **Medium** (correlates with Navigation) | KernelLedger UIA exposes explicit text/named controls (`Run all`, `Export notebook`, `Edit`, `Explain error`, `Show full traceback...`), though secondary per-cell controls (`28×30` logical px) and `Commands` are icon-only visually. Grok rightly noted `review.md`'s `6.5` underweighted Marimo's visible `Launch debugger` and `Get help` actions. |
| 7. Execution feedback | 8.5 / 8.0 | 8.0 / 7.5 | `null` / `null` | **Low-Medium** (static post-run stills only) | Static post-run captures cannot evaluate in-flight progress, cancellation, or reactive DAG staleness cues. Within post-run state only, Marimo surfaces per-cell durations (`846ms`, `1ms`, `172ms`) + error count, whereas KernelLedger surfaces `Runtime: Python (Ready)` and execution ordinals. |
| 8. Error experience | 8.0 / 8.5 | 7.5 / 7.5 | `null` / `null` | **Low-Medium** (unmatched traceback disclosure) | KernelLedger's collapsed summary + `Explain error` (`115×30` logical) + `Show full traceback...` (`140×30` logical) + outline `, failed` tag demonstrate strong progressive disclosure, whereas Marimo surfaces `Launch debugger` + full stack trace immediately. Because KernelLedger's expanded traceback was never captured, declaring an overall error-workflow winner is unsupported. |
| 9. Settings usability | 8.0 / 8.5 | 8.0 / 7.5 | `null` / `null` | **Low** (mismatched categories) | Unmatched tasks: Marimo `Editor` settings (dense form with helper text) vs KernelLedger `Appearance` settings (4 theme buttons + `800×253` logical live preview). Both `review.md` (`8.0/8.5`) and `grok-review.md` (`8.0/7.5`) over-interpret an apples-to-oranges capture. |
| 10. 800×600 layout | 7.0 / 8.0 | 6.5 / 7.5 | `null` / `null` | **Medium** (partially confounded; no narrow UIA) | KernelLedger adapts responsively at `800×600` (`1604×1202` raster) by collapsing its `217px` outline, at the cost of demoting `Run all` to an icon. Marimo's vertical clipping of `division by zero` at `800×600` (`1280×960` raster) is partly inflated by open Markdown source + expanded traceback, though retaining dual side rails at `800px` genuinely narrows editor width. |
| **Sum /100** | **75.5 / 80.5** | **72.5 / 76.5** | **`null` / `null`** | — | Arithmetic verified for both prior reviews; **do not average** `review.md` and `grok-review.md` into a pseudo-precise composite. |
| **Mean /10 (rounded half-up)** | **7.6 / 8.1** (`7.55` / `8.05`) | **7.3 / 7.7** (`7.25` / `7.65`) | **`null` / `null`** | — | Both prior means reflect novice/onboarding discoverability weighting across correlated dimensions (`Navigation`, `Action discovery`, `800×600`). |

---

## 4. Critique of Prior Methods, Scores, and Recommendations

### 4.1 Critique of `review.md` (and `comparison.html`)
- **Strengths:** Honest version/hash/DPI scoping; explicit warning not to treat half-point differences as statistically meaningful or UIA/DOM checks as accessibility certification; accurate identification of KernelLedger's labeled orientation advantage and outline truncation defect.
- **Methodological Flaws:**
  1. **Confound leakage from method prose into numerical scores:** `review.md` explicitly states that Marimo's open Markdown source and expanded traceback are session states rather than fixed limitations, yet penalizes Marimo on `Error experience` (`8.0` vs `8.5`) and `800×600 layout` (`7.0` vs `8.0`) for consequences of that open state, while simultaneously awarding Marimo `Space efficiency` (`8.0` vs `7.0`) despite unmatched width/density modes.
  2. **One-sided precision on `Text readability` (`7.0` vs `8.5`):** Uses an unarchived `12.6 CSS px` measurement for Marimo without measuring KernelLedger's computed font size, turning a modest desktop code-size difference into a 1.5-point penalty.
  3. **Dimension multicollinearity (double-counting labels vs icons):** `Navigation clarity`, `Action discovery`, and `800×600 layout` heavily weight visible text labels over icon rails. Because the 10-dimension rubric is equally weighted, one design trade-off (explicit labels vs compact chrome) produces +4.5 cumulative points for KernelLedger across three dimensions.
  4. **Premature density recommendation (#2):** Recommending a "denser working layout" as KernelLedger's #2 priority overlooks that `capture.py` *already* ran KernelLedger with `comfortable: false` (`ui_scale: 1`) and did not inspect `Layout & accessibility settings` to check existing density options.
  5. **`comparison.html` presentation gaps:** Displays `7.6 / 10` and `8.1 / 10` headline cards without unrounded sums or disclosure-mismatch caveats, and relies on undeclared CSS custom properties (`var(--chart-1)`, `var(--chart-2)`) with no fallback values.

### 4.2 Critique of `grok-review.md`
- **Strengths:** Rigorous PNG header raster-to-logical normalization (`1.0×`, `1.6×`, `1.983×`, `2.005×`); right-sizing the `Text readability` delta via normalized line pitch (`~18px` vs `~21px` logical); catching `Run all` losing its text label at `800×600`; flagging raw `KL_EXPERIMENTAL_INTERACTIVITY=1` copy in `Commands`; and rightly warning that chasing higher density by shrinking type would destroy KernelLedger's readability edge.
- **Overreaches & Flaws:**
  1. **Reversing the `Settings usability` winner (`8.0` vs `7.5`) across unmatched screens:** After proving `Editor` vs `Appearance` is an unmatched capture, Grok still awards Marimo +0.5 (`Medium` confidence) because `Editor` shows more controls and helper text. More controls on a different settings category does not establish superior usability; the dimension warrants `Low` confidence.
  2. **Scoring capture state in `Space efficiency` (`6.5` vs `7.0`):** Dropping Marimo from `8.0` to `6.5` because Markdown source and traceback were open in that screenshot reverses `review.md`'s bias by penalizing Marimo for fixture setup rather than measuring matched cell states.
  3. **Treating horizontal scrollbars in tracebacks as an unconditional defect:** Long local file paths are unbroken tokens; forcing hard-wrap without testing Marimo's existing `"Toggle text wrapping"` control or providing one-click path copying can harm developer ergonomics.
  4. **Misdiagnosing UIA text absence as a dump-script omission:** Grok speculated *"The dump may have omitted text runs."* Direct inspection of `capture.py:82` refutes this: the script queried `TreeScope::Descendants` with `Condition::TrueCondition` (no role filter). The absence of `ControlType.Text` is a real property of KernelLedger's UIA tree—and affects **rendered Markdown cells** (`# Sales analysis` + prose), not just the footer.

### 4.3 Assessment of `grok-critique.md`
- **Where We Agree:** `grok-critique.md` models strong methodological restraint—accepting the readability overclaim, disclosure-state confounding, settings category mismatch, and unarchived DOM evidence (`prior-computed-style-note.json`), while rightly pushing back on Grok's reversed settings score, unconditional anti-scrollbar stance, correlated onboarding bias, and untested truncation tooltips.
- **One Technical Pushback:** `grok-critique.md` characterizes the missing UIA text as merely *"a hypothesis: absence from this UIA dump is not enough to establish missing screen-reader output."* While it is certainly not a full NVDA/Narrator certification, cross-referencing `capture.py:82` (`TrueCondition` across all descendants) against `kernel-notebook-uia.json` proves that KernelLedger's native UIA provider projects **zero** static text nodes—omitting both the rendered `# Sales analysis` Markdown prose and the footer status bar. Because Windows screen readers traverse UIA on custom native windows, this is concrete structural implementation evidence requiring direct engineering verification, not just a capture-script artifact.

### 4.4 Which Recommendations Are Supported vs Speculative vs Risky for Expert Users
- **Directly Supported by Evidence:**
  - Auditing/exposing missing non-interactive text (`Rendered Markdown` + `Footer status`) in KernelLedger's UIA provider (`High` structural support).
  - Improving disambiguation/keyboard-accessible reveal for truncated notebook titles and outline rows in KernelLedger without widening the `217px` sidebar by default (`Medium-High` support, verify current focus/hover reveal first).
  - Replacing raw `KL_EXPERIMENTAL_INTERACTIVITY=1` strings on disabled `Commands` items with plain-language status + docs/settings path (`Medium-High` support).
  - Auditing accessible names (`aria-label`) on Marimo's icon-only rail/toolbar buttons in a saved DOM audit (`Medium-High` support from `review.md` live note, needs saved verification).
- **Speculative Until Tested on Matched/Live Captures:**
  - Wholesale density reductions or font-size changes in either product (KernelLedger is already `comfortable: false`; Marimo's `12.6px` `Fira Mono` may already be user-configurable in `Editor` settings).
  - Redesigning either product's Settings architecture from a single unmatched tab.
  - Changing default traceback expansion state in either product without testing KernelLedger's expanded traceback and Marimo's `"Toggle text wrapping"` control.
- **Likely to Backfire for Expert Users if Applied Naively:**
  - **Shrinking KernelLedger fonts or compact button hitboxes (`28×30` logical px, `4px` gap):** Degrades long-session scan comfort and pushes pointer targets toward mis-click territory.
  - **Wrapping every KernelLedger code cell and output in heavy bordered cards:** Increases visual clutter ("boxes inside boxes") across 30+ cell analysis notebooks.
  - **Forcing permanent text labels onto Marimo's left rail or KernelLedger's per-cell icon tools:** Steals horizontal width needed for wide code/tables, especially in `800–1000px` split-screen tiling common among developers.
  - **Unconditionally hard-wrapping traceback paths or auto-collapsing open editors/tracebacks:** Breaks line-oriented stack scanning, regex copying, and intentional multi-cell debugging state.
  - **High-salience always-on millisecond badges:** Adds visual jitter and encourages over-reading noisy single-run Python timings (`1ms`).

---

## 5. Prioritized Recommendations (Concrete Change, Benefit, Risk/Tradeoff & Validation)

### 5.1 KernelLedger

| Priority | Concrete Change | Intended Benefit | Risk / Tradeoff & Required Validation |
|---|---|---|---|
| **P1** | **Project rendered Markdown blocks and footer status into Windows UIA:** Expose rendered Markdown cells (`# Sales analysis` + prose) as `Document`/`Text` nodes and footer status (`Saved`, cell count, `Ready`) as a non-intrusive status group in the native UIA tree (`voiceover-sr`). | Enables screen-reader and assistive-tech users to read rendered Markdown prose without switching into Edit mode, and confirms save state non-visually. | *Risk:* Granular per-span UIA nodes make screen-reader navigation chatty. Expose one coherent semantic block per Markdown cell and validate with Windows Narrator + NVDA before shipping. |
| **P2** | **Add keyboard- & pointer-accessible identifier disclosure for truncated header/outline labels:** Keep the `217` logical px outline width unchanged, but verify/add focusable + hover full-text reveal (`truncation-strategy`, `compact-label-overflow`) and smart middle-truncation or two-line expansion on the *currently selected* row when prefixes collide (`kernelledger-notebook`). | Prevents notebook/cell misidentification even on 3-cell notebooks where `sales = [120, 18…` clips mid-literal. | *Risk:* Multi-line wrapping on *all* outline items halves vertical list capacity for long notebooks. First test current hover/focus behavior in a live run. |
| **P3** | **Prototype preserving labeled `"Run all"` priority at `800×600`:** In narrow split-screen widths, test collapsing `History` / `Checkpoints` into the `Notebook` destination menu before stripping the `"Run all"` text label down to a lone play glyph. | Preserves clear sighted distinction between *"Run entire notebook"* and per-cell *"Run"* (`primary-action`, `nav-label-icon`). | *Risk:* Crowds the top bar if notebook switcher names or UI scale (`>1`) grow; test at `800×600` with `ui_scale` 1.0 and 1.25 before changing collapse order. |
| **P4** | **Humanize disabled experimental command explanations:** In `Commands`, replace raw `KL_EXPERIMENTAL_INTERACTIVITY=1` text on disabled debugger rows (`Toggle breakpoint at caret`, `Debug selected cell`) with concise product copy (*"Experimental debugger disabled — enable in Runtime settings"*) plus a copyable flag/help action (`error-clarity`). | Eliminates user confusion that the Python kernel is broken while retaining feature discoverability. | *Risk:* Hiding the rows entirely would bury debugger discoverability; keep `enabled: false` + clear enablement path. |
| **P5** | **Test optional muted per-cell duration (`tabular-nums`) and gap-only compact tuning:** First inspect unopened `Layout & accessibility settings` and expanded tracebacks; if adding higher density or timing, keep per-cell duration muted/optional beside `[1]` (`number-tabular`) and tune inter-cell vertical gaps only—**do not** shrink font size or `28×30` logical px (`4px`-gap) button bounds. | Adds profiling visibility for slow cells and optional vertical capacity without sacrificing readability or WCAG 2.2 AA `24×24px` pointer safety. | *Risk:* Always-on ms labels add visual noise; shrinking text or `28×30` targets harms both expert scan speed and pointer accuracy. Validate on a 20+ cell mixed notebook first. |

### 5.2 Marimo

| Priority | Concrete Change | Intended Benefit | Risk / Tradeoff & Required Validation |
|---|---|---|---|
| **P1** | **Verify & enforce accessible names (`aria-label`) + focusable tooltips on all icon-only controls:** Run and archive an automated DOM audit across left-rail, floating, and cell-action icon buttons (`aria-labels`, `icon-context`), adding deterministic `aria-label` + keyboard-focusable tooltip disclosures wherever missing. | Resolves assistive-tech unlabeled-button barriers noted in `review.md` and accelerates sighted icon learning with zero layout cost. | *Risk:* Minimal UI risk, but must be verified against current Marimo release with a saved DOM dump rather than assumed from `0.25.1` unarchived notes. |
| **P2** | **Ensure traceback path wrapping / prefix-folding + dedicated copy controls while keeping the exception line visible:** Verify Marimo's existing `"Toggle text wrapping"` behavior on tracebacks; support soft-wrapping or collapsible `<workspace>/…` path prefixes (`long-token-wrapping`) paired with one-click *Copy path* / *Copy traceback*, and anchor `ZeroDivisionError: division by zero` in immediate view at `800×600`. | Eliminates two-axis in-cell scrolling and prevents narrow viewports from clipping the actual exception cause (`division by zero`). | *Risk:* Unconditional hard-wrapping of stack frames frustrates experts scanning line numbers or selecting paths; preserve raw-copy fidelity and user-togglable wrap state. |
| **P3** | **Provide an optional/on-demand labeled rail mode and auto-collapse secondary rails at `≤800px`:** Allow toggling text labels on the left navigation rail (or holding a modifier key for shortcut/label badges) while collapsing side rails to overlays at `800×600` (`adaptive-navigation`, `nav-label-icon`). | Lowers first-session navigation friction without permanently taxing horizontal width for expert users. | *Risk:* Making a wide labeled rail permanent would penalize `width="medium"`/`full` DataFrame workflows; verify existing Marimo sidebar collapse shortcuts first. |
| **P4** | **Validate desktop code font-size presets before any default change:** Confirm whether `Editor` settings already exposes code font-size scaling above `12.6px` (`Fira Mono`), and test `12.6px` vs `13.5–14px` in a controlled reading/editing task before altering defaults. | Improves out-of-box comfort on large desktop monitors if validated, without guessing from stills. | *Risk:* Increasing default code font size reduces visible lines per screen for reactive multi-cell debugging; keep expert compact presets intact. |

---

## 6. Major Objections, Methodological Limitations & Next-Step Study Design

1. **No Direct Visual Inspection in Antigravity ACP:** Because `antigravity-acp` lacked a working multimodal image-attachment path (`client_view_file` rejected PNGs; `view_file` returned garbled UTF-8 text; `html_preview` offloaded `ImageContent` without attaching `media`), this review withholds numerical visual scores rather than fabricating them.
2. **State & Configuration Confounds Invalidate Naive Winner Claims:** Comparing Marimo (`width="medium"`, Markdown source open, traceback expanded, `Editor` settings) against KernelLedger (`comfortable=false`, `ui_scale=1`, Markdown source collapsed, traceback collapsed, `Appearance` settings) confounds `Space efficiency`, `Visual hierarchy`, `Error experience`, `Settings usability`, and `800×600 layout`.
3. **Single Tiny Fixture & Static Post-Run Stills:** A 3-cell, 4-line Python script (`1 / 0`) cannot evaluate real notebook scaling (30+ cells, wide DataFrames, plots), reactive vs imperative execution mental models, streaming output, autocomplete/editor ergonomics, keyboard navigation, or performance.
4. **How to Settle Remaining Disagreements Rigorously:** Capture matched states in both apps (Markdown rendered/closed, both collapsed and expanded tracebacks, matched `Editor` and `Appearance` settings tasks, `1280×850` and `800×600` viewports, plus saved DOM + UIA dumps), and run short timed/observed tasks (disambiguate notebooks, run single cell vs all cells, diagnose traceback, toggle theme/font size) before publishing updated comparative scores.
