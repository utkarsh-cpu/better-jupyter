# Frontend live UX plan (draft)

| Item | Decision |
| --- | --- |
| Date | 2026-11-07 |
| Status | Draft; not authoritative. Does not replace `FRONTEND_PLAN.md`. |
| Scope | Everything still open after the live Linux GPUI walkthrough, prior v6 gates, screenshot polish, UI follow-up, and assistant reliability work. |
| Evidence | Live build + interaction on Ubuntu XFCE (`DISPLAY=:1`, Mesa llvmpipe): [/opt/cursor/artifacts/ui-live/](../artifacts/) session captures under `ui-live/`; prior Windows native sets in `artifacts/frontend-v6/`, `artifacts/frontend-bars-polish/`, `artifacts/ui-candidates-2026-11-05/`. |
| Related | [frontend-v6](../docs/desktop/frontend-v6.md), [ui-review-followup-plan](ui-review-followup-plan.md), [ui-review-followup](../docs/desktop/ui-review-followup.md), [assistant-reliability-draft](assistant-reliability-draft.md), [platforms](../docs/desktop/platforms.md), [ux-scorecard](../docs/desktop/ux-scorecard.md). |

## 1. Why this plan exists

A fresh debug build of `kernelledger-desktop` was opened against a populated fixture notebook and reviewed by interacting with the running window (Notebook, History, Checkpoints, Commands, Settings, error disclosure, Assistant), not by judging archived screenshots alone.

Live heuristic scores (provisional, not a user study):

| Category | Live | Notes |
| --- | ---: | --- |
| Visual hierarchy | 8 | Tabs + primary Run all read clearly |
| Typography | 7 | Legible; Segoe/Consolas-default character |
| Color & theme | 8 | Quiet ledger works; light canvas near generic cream |
| Distinctiveness / brand | 6 | Title-bar identity; GPUI-kit/IDE-generic otherwise |
| Restraint / clutter | 8.5 | Strong progressive disclosure |
| Spacing & density | 8 | Compact/Comfortable available |
| Consistency / craft | 8 | Shared chrome and status |
| Interaction clarity | 8.5 | Palette, tooltips, Edit/preview |
| Error & state design | 9 | Best live strength |
| Copy / writing | 7.5 | Plain verbs; some technical density remains |
| Accessibility (observed) | 7 | Options exist; AT journeys unqualified |
| Delight | 6 | Calm, not memorable |
| **Overall** | **7.8** | Craft ahead of identity |

Earlier Windows apple-design heuristic in frontend-v6: **7.2 / 10** (target 8 / 10, no principle below 7). Release scorecard coverage remains largely `NT`.

This plan covers **all remaining workstreams** needed to close that gap without pretending the product is release-certified.

## 2. Intended result

1. Keep the working notebook UX strengths (error/state, Run clarity, restraint).
2. Raise distinctiveness, typography, and first-viewport brand without a theme costume change.
3. Finish open follow-up and v6 qualification gates (a11y, DPI, Linux, history bounds, contracts).
4. Produce evidence that updates the UX scorecard from `NT` toward measured scores.
5. Only then consider publishing a new authoritative frontend plan version.

Non-goals for this draft: Marimo reimplementation, new runtime protocols unless a gate truly requires them, wholesale History/Checkpoints IA redesign, decorative motion for its own sake, purple/SaaS-card restyles.

## 3. Workstreams (everything in scope)

### A — Brand, typography, and visual identity (P1)

**Problem.** Live UI is competent and quiet but interchangeable with other GPUI/IDE notebooks. Brand appears mainly in the window title. System faces and warm paper `#F3F2ED` read near common AI-default cream.

**Work.**

1. Define a KernelLedger token sheet (4–6 named colors, two type roles max) that stays “quiet ledger / precision cobalt” but is not cream+system-default by accident.
2. Ship one deliberate display/UI pairing (still desktop-native; no web font CDN). Prefer faces already licensed/bundled or OS-installed with documented fallbacks.
3. Put product identity in the first viewport: wordmark or distinctive app chrome treatment that survives removing the OS title text.
4. Audit ALL-CAPS chrome labels (e.g. section eyebrows) against the design skill: drop or sentence-case unless they encode real structure.
5. Keep light / dark / high-contrast; re-check syntax and non-text contrast after token changes.
6. Update Settings Appearance preview to the real tokens.

**Acceptance.** A stranger can name the product from a cropped notebook screenshot without the window title. Contrast gates from `F-A11Y-07` still pass. No principle-score regression on Simplicity.

**Likely files.** `theme.rs`, `shell.rs`, `settings.rs`, `components.rs`, asset/font packaging docs.

### B — Notebook chrome density and identification (P1)

**Problem.** Follow-up integration improved Compact gutters and title-bar naming, but narrow/assistant-split layouts still clip source/error rows; identification and Run priority need closed evidence on current tree.

**Work.**

1. Re-baseline current `master` with before/after captures (1280×850, 800×600, split width) light/dark/hi-c, Compact/Comfortable, 100/125/200% UI scale.
2. Confirm notebook switcher uses measured width budget, middle-ellipsis with useful suffix, full path on focus.
3. Confirm Run all stays text-labeled at 800×600 / scale 1; Export collapses before Run label; destinations remain reachable.
4. Verify Compact code cells: no empty header row when state allows; Failed / Unsaved / Conflict / Edited-since-run stay explicit.
5. Fix assistant-open narrow layout so source and error summaries remain readable (no silent clip of the failing line).

**Acceptance.** Criteria in [ui-review-followup-plan](ui-review-followup-plan.md) Steps 2–4, re-verified on the integrated Sol build. Failures filed as open P1s, not papered with screenshots of larger windows.

**Likely files.** `shell.rs`, `notebook.rs`, `theme.rs`, `components.rs`, `assistant.rs`.

### C — History, Checkpoints, Settings composition (P2)

**Problem.** Live History is cleaner than Oct captures, but filter chrome + technical details still consume first paint. Checkpoint advanced/experimental paths must stay secondary. Settings categories exist; measure and empty-state craft still vary.

**Work.**

1. History: default to a short filter strip (search + outcome chips); park advanced Cell/Session/date controls behind Filters until used; preserve query vocabulary consistency.
2. History detail: one unchanged-source presentation; compare only when changed or requested.
3. Checkpoints: keep Create / Inspect / Restore primary; Advanced + Technical details collapsed; no empty JSON blocks in ordinary reports.
4. Settings: content-sized controls, bounded path measure, honest interpreter summary before change.
5. Commands palette: content-driven height; disabled experimental debug commands keep plain-language reasons (already started).

**Acceptance.** First viewport of History shows results without scrolling past a form. Checkpoint report readable without expanding Technical details. No stretched full-bleed buttons.

**Likely files.** `history.rs`, `checkpoints.rs`, `settings.rs`, `shell.rs`, `shortcuts.rs`.

### D — Error, state, and recovery (maintain + close gaps) (P0/P1)

**Problem.** Live error UX scored ~9; remaining risk is consistency under redraw, AT announcement, and recovery contracts that UI cannot honestly claim yet.

**Work.**

1. Lock visual grammar for resting / hover / selected / editing / failed / stale / canceled across code and Markdown (text + non-color cue).
2. Preserve traceback disclosure across redraw; long-path copy; Explain error stays cell-tied; unavailable assistant setup stays honest.
3. Status bar: Ready / Saved / Editing scoped correctly; never leave EDIT MODE stale on non-Notebook destinations.
4. Do not claim structural Undo/Redo until runtime inverse contracts exist (v6 gate 1).
5. Do not claim verified restore preflight until checkpoint provenance/compatibility contracts exist (v6 gate 2).

**Acceptance.** Color-blind-safe distinction of selection vs failure. Trust-gated restore still requires explicit allow-saved-code. Any unavailable capability says so in UI and docs.

**Likely files.** `notebook.rs`, `outputs.rs`, `checkpoints.rs`, client/runtime contract docs as needed.

### E — Accessibility and keyboard (P0)

**Problem.** Live session observed shortcuts and Reduce motion only. NVDA/Narrator/Orca, full focus rings, and offscreen UIA truth remain open. Frontend-v6 explicitly blocks release on this.

**Work.**

1. Narrator + NVDA on Windows for: rendered Markdown without Edit; cell count/save/runtime; Settings selection; contextual cell actions on focus; palette/settings focus return.
2. Orca + keyboard-only pass on Linux once graphics path is qualified (Workstream G).
3. Repair confirmed gaps only; expose coherent Markdown blocks and status semantics; avoid chatter from autosave/polling.
4. Verify disabled UIA state matches visuals (Interrupt, Markdown Run, untrusted Restore).
5. Clear tooltips on destination/overlay/focus change (no stale hover).
6. Complete IME, 100/125/150/200% DPI, mixed-monitor, pointer-cancel checks from v6 gate 4.

**Acceptance.** Documented AT journeys with pass/fail and remaining limits. No invisible focused controls. Scorecard keyboard + a11y categories leave `NT`.

**Likely files.** `accessibility.rs`, `notebook.rs`, `shell.rs`, `components.rs`, `settings.rs`.

### F — Performance and large-data gates (P1)

**Problem.** v6 gate 3: unbounded helper history hydration; 10k-record and large-notebook targets unverified on current UI.

**Work.**

1. Bound history query/pagination; prove partial-page and offline behavior.
2. Re-run F-PERF style checks: cold start, first `print(1)`, 1000-cell open without 1000 live editors, 100 MiB stream paging, interrupt during flood.
3. Image cache and idle memory budgets under documented fixtures.
4. Record mid-range Windows **and** the Linux software-render path separately (do not mix).

**Acceptance.** Numbers with fixture hashes and machine notes. Failures become explicit release blockers or deferred with plan ids.

### G — Linux / platform qualification (P1)

**Problem.** `platforms.md` marks Linux packaging/graphics unqualified. This environment proved a window can open with Mesa llvmpipe after installing fontconfig/dbus/Vulkan ICDs, but that is not a ship matrix.

**Work.**

1. Document minimum Linux packages (fontconfig, dbus, xkbcommon, Vulkan/GL, X11/Wayland) and env (`XDG_STATE_HOME`, `KL_RUNTIME_PYTHON`).
2. Qualify at least one hardware-accelerated Linux path and one software-render path; record failures honestly.
3. Package/provenance notes for unsigned debug vs future installers; signing remains Windows-first.
4. Clipboard, IME, multi-monitor DPI move on each qualified OS.
5. Keep macOS/ARM out of scope unless a later plan opens them.

**Acceptance.** Update `platforms.md` with pass/fail table. Live UX claims cite the exact graphics backend used.

### H — Assistant reliability and evidence UX (P1)

**Problem.** Assistant panel is present live (“Connect a provider”, Explain error, Review context). Reliability draft and implementation notes still limit what may be claimed.

**Work.**

1. Execute remaining items in [assistant-reliability-draft](assistant-reliability-draft.md) that are not already shipped.
2. Keep approval / Apply / cancel / retry honest; never imply network providers are configured when they are not.
3. Narrow-layout coexistence with notebook (ties to Workstream B.5).
4. Preserve local-variable context labeling without leaking secrets into exports.

**Acceptance.** Reliability draft exit criteria; isolated native journeys recorded.

### I — Interchange, Git, help, and polish (P2)

**Work.**

1. Export hover/menus (PDF/Word/Jupyter) — verify destinations and failure copy.
2. Git conflict / disk-change banners per frontend plan when those journeys are in the release cut.
3. Help: one shortcut vocabulary matching Commands; build info; no contradictory filter syntax.
4. Diagnostics: ordinary empty states; doctor/health actions content-sized.
5. Optional cell duration off-by-default (follow-up Step 5) if metadata already available.

### J — Studies, scorecard, and plan publication (P0 for release)

**Work.**

1. Fill `ux-scorecard.json` per category as evidence lands; archive prior snapshots.
2. Formative study (5–8 participants, new + experienced) on identify notebook, run cell, run all, edit Markdown, find/read error, restore trust flow, switch theme.
3. Summative / SUS before release cut; no open P1.
4. When requirements change, publish frontend plan version via [plans/README.md](README.md) rules (archive → rewrite → Requires runtime sections).
5. Do not treat live agent heuristics (7.8) or apple-design 7.2 as release scores.

## 4. Delivery phases

| Phase | Name | Includes | Exit |
| --- | --- | --- | --- |
| 0 | Baseline | Current-tree native captures Linux+Windows, fixture set, scorecard `NT`→instrumented | Repro matrix checked in |
| 1 | P0 truth | D (contracts honesty), E (AT repairs), J (scorecard wiring) | No false capability claims; AT gaps listed |
| 2 | Identity + density | A, B, C.1–C.2 | Brand test pass; Compact/narrow evidence |
| 3 | Surfaces + assistant | C.3–C.5, H, I | History/Checkpoints/Settings/Help craft bar |
| 4 | Scale & platform | F, G, remaining E DPI/IME | Perf + platforms tables filled |
| 5 | Release review | J studies, no-P1, plan version if needed | Scorecard ≥ release thresholds in `ux-scorecard.md` |

Phases 1–2 may overlap on separate branches if they do not fight theme tokens. Theme/token work (A) lands before large screenshot campaigns in later phases.

## 5. Requirement mapping (existing ids)

This draft does not mint new `F-*` ids until a frontend version is published. Map work to current plan areas:

| Workstream | Existing anchors |
| --- | --- |
| A | F-UX-01, F-APL-04, design refresh notes |
| B | F-APL-01/02/03/10, ui-review-followup Steps 2–3 |
| C | F-APL-09/13, F-IA-23/24 |
| D | F-UX-04–07, F-APL-06/08, FS-45/48 |
| E | F-A11Y-*, F-KEY-*, F-APL-12, FS-49/50 |
| F | F-PERF-*, v6 gate 3 |
| G | platforms.md, packaging section of frontend plan |
| H | assistant reliability draft / AI draft |
| I | F-GIT-*, Help/Diagnostics surfaces |
| J | F-QA-*, ux-scorecard release rule |

## 6. Evidence rules

1. Prefer live GPUI window captures + UIA/AT notes over HTML mockups.
2. Record revision, binary hash, DPI, UI scale, density, theme, disclosure state.
3. Separate Windows hardware, Linux hardware, and Linux software-render results.
4. Screenshot scores inform priority only; acceptance is task completion + gates.
5. Store new campaigns under `artifacts/frontend-live-ux-YYYY-MM-DD/` with README.

## 7. Open decisions (draft)

| Id | Question | Default until decided |
| --- | --- | --- |
| LD-01 | Bundle a custom UI face vs document OS face + fallback only | Document + optional bundled face if license clear |
| LD-02 | How strong first-viewport brand (wordmark vs typographic title only) | Typographic wordmark in app chrome, no mascot |
| LD-03 | Linux release in same cut as Windows | Windows ships first; Linux “preview” until G exits |
| LD-04 | Whether delight work gets a dedicated spike | Only after A/B pass; max one restrained motion |

## 8. Explicitly deferred

- macOS / ARM64
- New charting library beyond existing MIME outputs
- Structural Undo/Redo before inverse contracts
- Verified checkpoint compatibility UI before provenance contracts
- Rewriting the product as a web app
- Theme directions called out as AI defaults in the design skill (purple SaaS, terracotta cream costume, broadsheet, acid-green dark)

## 9. Immediate next actions

1. Check this draft into `plans/` and index it (this document).
2. Phase 0: script reproducible Linux + Windows fixture launch (extend `artifacts/frontend-v6/native/launch.py` patterns; Linux launcher using `XDG_STATE_HOME` + `KL_RUNTIME_PYTHON`).
3. File Phase 1 AT investigation tasks against the integrated Sol build.
4. Sketch token/type options for Workstream A as a short design note before code.
5. Do not bump `FRONTEND_PLAN.md` until Phase 2+ requirements stabilize.
