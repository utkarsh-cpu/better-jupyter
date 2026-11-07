# GPT 6.1 Sol xhigh vs Grok 4.7 xhigh: implemented KernelLedger UIs

**Recommendation: use Sol's candidate as the starting point, with remaining fixes before merge.**
My change-focused UI ratings are **Sol 8.2/10**, **Grok 8.0/10**, and **current baseline 7.4/10**.
The 0.2-point candidate gap is subjective and small. The recommendation rests on notebook identification
and conservative large-text behavior, not that decimal difference alone.

Grok is stronger on consistent cell/error boundaries and leaves a slightly wider editor. Both implement
substantial improvements and both introduce a horizontal-space regression. Neither is fully qualified
for screen readers or ready for unconditional acceptance.

## Implementations and provenance

| | Current common base | Candidate A: Sol | Candidate B: Grok |
|---|---|---|---|
| Model | Existing app | GPT 6.1 Sol xhigh via Codex | Grok 4.7 xhigh via Cursor |
| Source base/HEAD | dd68f53c96f85299a19851db7ae6804eb2a05560 | Same; dirty local changes, no commit | Same; dirty local changes, no commit |
| Build | dev/unoptimized | dev/unoptimized | release |
| Binary SHA-256 | 54CDEEB07EE7BF332C191542962410D38475BD479D579EB640C754F588185E08 | F9EF37BC4E19CE24E160AAF2AADFA0AC39AB8ABEA919DD9640035B59D9B5AB7B | FF454958E8823E72B6842676577117C085F4E721175D67A93486DAD3A65EA1DC |

- Sol worktree: `C:/Users/UTKARSH KAUSHIK/.t3/worktrees/KernelLedger-v6/t3-562311ac`, branch `t3/implement-kernelledger-ui-plan`.
- Grok worktree: `C:/Users/UTKARSH KAUSHIK/.t3/worktrees/KernelLedger-v6/t3-3cca6db7`, branch `t3/implement-kernelledger-ui-plan-1`.
- [Sol implementation report](<C:/Users/UTKARSH KAUSHIK/.t3/worktrees/KernelLedger-v6/t3-562311ac/artifacts/ui-plan-implementation/implementation-report.md>).
- [Grok implementation report](<C:/Users/UTKARSH KAUSHIK/.t3/worktrees/KernelLedger-v6/t3-3cca6db7/artifacts/ui-plan-implementation/implementation-report.md>).
- [Sol executable](<C:/Users/UTKARSH KAUSHIK/.t3/worktrees/KernelLedger-v6/t3-562311ac/desktop/target/debug/kernelledger-desktop.exe>).
- [Grok executable](<C:/Users/UTKARSH KAUSHIK/.t3/worktrees/KernelLedger-v6/t3-3cca6db7/desktop/target/release/kernelledger-desktop.exe>).

Both agents received the identical full implementation brief/plan and isolated managed worktrees/target
directories. Parent modifications to documentation/evidence were not included in their base checkout.
The parent independently built the common base with `cargo build --locked -p kernelledger-desktop`.
Its hash is identical to the November frozen executable. Historical screenshots were not substituted
for the new matched native captures in this comparison.

**Release vs debug is a confound for speed/memory. Neither is scored here.** Source layouts and
native interaction results can still be compared under the same fixture/window settings.

## Capture and verification method

[Capture driver](capture_candidates.py) creates owned notebooks and profiles, runs one app at a time,
and closes its own window and runtime afterward. Screens are actual Windows Graphics Capture output.
All matched cases use the same heading, prose, four-line sales source, two output lines and division-by-zero
cell. Native DPI is 192. Window dimensions are logical; manifests record actual client pixels.

The default case is light, Compact, editor 14 px, UI scale 1, standard runtime without experimental
debugging, initial Markdown selection, source visible for code, and traceback collapsed. Additional
matched states include 800x600, dark, Appearance/Layout settings, expanded tracebacks, Comfortable,
and a high-contrast stress case at UI scale 2/editor 24 px. That stress case needs scrolling; it is not
supposed to fit an entire notebook into the viewport.

A 32-cell source fixture was also opened at both widths. Only summary/error outputs were seeded there;
this is an opening/rendering smoke check, **not** a completed 32-cell run/edit/performance study.
Recorded ordinals and durations naturally vary after the scripted reruns; they are not benchmark results.

UIA dumps include Name, Role, Enabled, Offscreen, focus, supported patterns and Text/Value/Toggle/
Selection data when available. Some synthetic nodes have non-finite bounds; their names alone do not
establish full navigation or speech behavior.

## Ratings

Scale: 1 poor, 5 workable with substantial friction, 7 good, 9 excellent for the tested scope.
Half-points and averages express judgment, not measurement precision. These criteria explicitly split
vertical density from horizontal editor width and include implementation access evidence; consequently
these averages should not be compared numerically with the earlier Marimo review's different criteria.

| Parameter | Baseline | Sol | Grok | Basis |
|---|---:|---:|---:|---|
| Visual consistency | 7.5 | 8.0 | 8.5 | Grok gives resting/success/error cells a consistent container and stronger failure boundary. Sol is quieter, with more open canvas. |
| Text readability | 8.5 | 8.5 | 8.5 | Both preserve the 14 px editor default and approximately 21 px code pitch; no evidence of a new font-size advantage. |
| Notebook identification | 6.0 | 9.0 | 7.5 | Sol shows the full fixture name at the roomy size. Grok retains middle ellipsis there; both improve path context and suffix discovery over baseline. |
| Navigation / Run clarity | 7.5 | 8.5 | 8.5 | Both keep Run all labeled at 800 and secondary destinations reachable. Sol uses a current-destination dropdown; Grok adds an explicit Views menu. |
| Compact vertical space | 6.5 | 8.5 | 8.5 | Both reduce the defined source/output extent from 204 to 172 logical px (15.7%), with matched content and typography. |
| Editor horizontal space | 9.0 | 6.5 | 7.0 | At 1280 the editable rectangle shrinks from 964 logical px to 760 in Sol and 788 in Grok. Grok is 28 px wider; both sacrifice room to gutters/action columns. |
| Responsive / large text | 7.5 | 9.0 | 7.5 | Both pass the 800-wide header checks. At UI scale 2 with 24 px code, Sol uses a wider header fallback; Grok keeps the gutter and wraps substantially more source. |
| Error presentation | 8.0 | 8.0 | 8.5 | Both preserve readable summaries/disclosure and wrap the tested traceback. Grok adds a coherent danger border/rail and explicit alert icon. |
| Keyboard / access evidence | 6.0 | 7.5 | 7.0 | Both pass contextual focus/menu return and expose Markdown names. Sol provides a document/heading structure; footer/save-state exposure remains incomplete or unverified in both. |
| Execution feedback | 7.5 | 8.5 | 8.5 | Both retain Ready/Saved/outcomes and offer off-by-default recorded durations verified after rerun. No startup-speed, live-queue or memory ranking is implied. |
| **Total /100** | **74** | **82** | **80** | Ten equal weights. |
| **Mean /10** | **7.4** | **8.2** | **8.0** | No performance or model-capability ranking. |

## Measured tradeoffs

| Matched geometry, logical px | Baseline | Sol | Grok |
|---|---:|---:|---:|
| Source/output extent, 1280-wide default | 204 | 172 | 172 |
| Editable source width, 1280-wide default | 964 | 760 | 788 |
| Editable source width, 800-wide default | 740 | 536 | 564 |
| Editable source width, 800-wide scale-2/font-24 stress | 684 | 684 | 448 |

The source/output extent is **Run-button top to recorded-output bottom**, read from UIA at DPI192.
It includes the source and its output, not the whole next-cell slot. Both candidates save **32 px (15.7%)**
on this defined four-line fixture. Sol's child-reported101 vs132 headless measurement used a different,
single-line fixture; do not compare those numbers directly.

At the roomy default, Sol sacrifices204px of editor width (21.2%); Grok sacrifices176px (18.3%).
Grok's editor is28px wider than Sol's. Output retains its broader reading width. This is the most important
regression shared by both and should be corrected before merging a Compact redesign.

At the scale/font stress setting, Sol deliberately returns to a header. Grok keeps a gutter and much narrower
source area at the same180px editor height, so more source wraps and needs scrolling. The full source remains
in the Value pattern; this capture does not establish unreachable text. It does establish the width/reading cost.

## What is better in each candidate

### Sol

- The complete `Sales-analysis-notebook` name is visible at the wide baseline, with full path in the
  accessible switcher name and a measured, responsive width allocation.
- The current destination becomes a dropdown at narrow widths; native checks found History and
  Checkpoints reachable, while Run all remains labeled.
- Compact Run/ordinal sit beside source, Markdown no longer pays for a dedicated empty toolbar row,
  and the normal cell's source/output block is shorter.
- Large editor/UI sizes and debugger-capable runtimes take a conservative header fallback. This means
  the density improvement is intentionally unavailable in some contexts, but reduces gutter crowding.
- Rendered Markdown has a Document, named heading and named paragraph child structure in the live UIA
  dump. It is a concrete improvement over baseline absence, not proof of Narrator/NVDA qualification.

### Grok

- Resting cells use a consistent quiet border and failures gain a danger edge/rail plus alert icon.
  This makes source, output and failure read as one cell more clearly.
- Run and ordinal stack vertically in a narrower leading gutter, leaving28px more source width at normal scale.
- A labeled Views menu exposes secondary navigation at800px. Native opening/Escape checks passed.
- The tested expanded traceback remains readable and wraps. Sol's tested traceback also wraps, so this
  is not claimed as a uniquely delivered Grok feature despite its changed rendering implementation.
- Rendered Markdown is exposed as one named Group; the live status summary names the cell count and Ready.
  That is useful, although less structured than Sol's content nodes.

## Remaining issues before choosing a merge

1. **Fix horizontal allocation in either candidate.** A hidden action column still reserves substantial
   space. Keep stable targets and focus reveal, but reduce needless reservation or use a safer responsive
   action layout. Validate with long source lines and a mixed notebook.
2. **Complete native accessibility validation.** Sol's added footer Text nodes have empty names/no
   Text/Value patterns in this dump; its named runtime status is present. Grok's zero-size status summary
   is named `3 cells. Ready.` but ordinary Saved is absent from that summary. No screen reader was run.
   Verify the actual content/save-state speech and synthetic-node navigation before declaring P0 complete.
3. **Improve Grok's roomy name handling.** Its screenshot still reads `Sales-analysi...notebook` while
   Sol shows the complete fixture name. Full accessible path is present in both. Treat visible truncation
   and accessible names as different requirements.
4. **Add a large-type fallback or validated wrapping strategy to Grok.** A30px Run target alone does not
   ensure the source row remains practical.
5. **Preserve Sol's calmer typography while considering Grok's error grouping.** A stronger failure
   container may help; do not import every border by taste or add nested boxes.
6. **Keep timing optional and metadata-based.** Both candidates showed it after rerun and after History
   inspection. Do not infer runtime speed from the displayed hundredths of a second.

## Checks performed

| Check | Baseline | Sol | Grok |
|---|---|---|---|
| Native run-one records a successful execution | Pass | Pass | Pass |
| Native Run all records both code-cell runs | Pass | Pass | Pass |
| Traceback expand/collapse | Pass | Pass | Pass |
| Focus More, Space opens menu, Escape restores invoker | Pass | Pass | Pass |
| Selected theme exposes Toggle On/Off | Pass | Pass | Pass |
| Labeled Run all at800 / secondary menu reachability | Baseline uses icon | Pass | Pass |
| Enable optional duration, rerun, inspect History and return | Not available | Pass | Pass |
| Title-bar controls, drag region and native caption hit tests at1280/800 | Existing baseline evidence | Pass | Pass |

Native caption checks used the repo's existing technique: prime GPUI's window-local mouse state before
WM_NCHITTEST. Header controls return HTCLIENT; blank drag region HTCAPTION; minimize/maximize/close
return their corresponding codes. `*-caption-verified/` holds the valid results. The first
`*-caption/` attempt omitted that priming and returned stale HTCLIENT everywhere; that was a probe
error, not a candidate regression, and is superseded.

Parent independently reran the same UI/client/model subset in both worktrees with `--locked`:
**Sol97 tests** (59UI+31client+7models) and **Grok99 tests** (62UI+30client+7models), all passed.
Parent also checked both diffs for whitespace errors. Sol's archived full workspace run reports123
passed; Grok's child reports clean lint checks on changed crates and the built release executable.
Test counts are scope descriptions, not scores. No Narrator/NVDA, full user study, comprehensive IME
or mixed-DPI qualification, full conflict journey or performance benchmark is claimed.

## Evidence and disposition

- [Interactive screenshot comparison and scores](comparison.html).
- [Exact score data and metrics](scores.json).
- [Verified executable hashes](verified-builds.json).
- `common-base/`, `candidate-a/`, `candidate-b/`: matched main suites and UIA.
- `*-comfortable/`, `*-scaled-contrast/`, `*-mixed/`: additional fixture/configuration cases.
- `candidate-a-timing/`, `candidate-b-timing/`: option/rerun/History verification.
- `candidate-a-narrow/`, `candidate-b-narrow/`: navigation menu and expanded800px traceback.
- `*-caption-verified/`: valid window hit tests.

Both implementations remain independent and dirty in their managed worktrees. Parent has not changed
either implementation, merged either branch, or pushed anything. All owned capture apps/runtimes were
closed. Parent changes are evidence/capture/report files; unrelated existing user changes were preserved.
