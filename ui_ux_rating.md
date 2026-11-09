# KernelLedger UI/UX Rating for Data Scientists & Data Engineers

**Project**: KernelLedger (desktop notebook application)
**Rating Scale**: 1-10 (1 = Poor, 10 = Excellent)
**Audience**: Data Scientists & Data Engineers

---

## Rating Categories

### 1. Workflow Efficiency for Rapid Iteration ⭐⭐⭐⭐⭐⭐⭐⭐ 8/10

**Data Scientist perspective**: Quick cell execution, immediate output feedback, seamless Markdown preview, easy checkpointing.

**Data Engineer perspective**: Git branch switching, conflict resolution, batch operations, pipeline-like workflows.

**Strengths (from FRONTEND_PLAN)**:
- F-ED-10: Autosave with debounce (500ms default)
- F-RT-20: Run all/above/below with expected hashes
- F-ED-01-04: Structural editing with undo/redo
- F-IA-21: Notebooks identified by path, not UUID
- F-IO-02: Run cell, run all, run source actions

**Weaknesses**:
- No formative usability study results yet (F-QA-04 mentions formative round closes FD-06)
- F0 budget default is 4 engineer-weeks — may limit polish

**Rating**: 8/10 — Strong vertical slice (F2) with editing, autosave, and execution. Gaps in formative validation.

---

### 2. Data Exploration & Visualization ⭐⭐⭐⭐⭐⭐ 7/10

**Data Scientist perspective**: Easy Markdown rendering, image output (lazy decode, fit/zoom), table rendering (F-OUT-17: virtualized HTML table), LaTeX math (F-OUT-15).

**Data Engineer perspective**: Large stream handling (F-OUT-23: 100 MiB bounded paging), JSON tree views (F-OUT-16), output selection with history.

**Strengths**:
- F-OUT-12: Image lazy decode, fit and zoom, dimension/memory limits
- F-OUT-17: Virtualized HTML table with thead/tbody/tr/th/td, pandas DataFrame wrapper
- F-OUT-14: Native Markdown with math (FD-05)
- F-OUT-21: Cell virtualization with stable ids and measured-height caching

**Weaknesses**:
- No native charting library mentioned (F-TRAIN-01 is experimental)
- Large notebook performance (F-PERF-05: 1,000 cells open without 1,000 live editors) is a target, not proven
- No embedded webview for rich HTML (deferred to section 15 gate)

**Rating**: 7/10 — Strong MIME output rendering, but limited charting/interactive visualization out of the box.

---

### 3. Debugging & Error Clarity ⭐⭐⭐⭐⭐⭐⭐⭐ 8/10

**Data Scientist perspective**: Clear tracebacks with source context (F-OUT-11), error recovery actions, stale badge indicators.

**Data Engineer perspective**: Error-to-recovery-action mapping (F-CL-07), technical details disclosures, runtime error codes.

**Strengths**:
- F-UX-04: All data surfaces specify states: empty, loading, ready, partial, disconnected, permission denied, read-only snapshot, corrupt data, unsupported schema, migration required, Git conflict markers, retryable error
- F-UX-05: Errors that block work stay visible where they happened; toasts are supplementary
- F-UX-06: Confirmation dialogs with verb + target + consequence + human-readable summary
- F-UX-07: Raw OS/transport errors never primary message; "Technical details" disclosure
- F-CL-07: Error-to-recovery-action mapping in one table keyed by runtime error code

**Weaknesses**:
- Formative usability study (F-QA-04) hasn't run yet to validate error UX
- F0 phase — may not have full regression testing

**Rating**: 8/10 — Excellent error state design per the plan, awaiting usability validation.

---

### 4. Git & Version Control ⭐⭐⭐⭐⭐⭐ 7/10

**Data Scientist perspective**: Less critical but helpful for sharing notebooks, tracking experiments.

**Data Engineer perspective**: Essential — branch switching, conflict resolution, per-cell conflict UI.

**Strengths**:
- F-GIT-01 to F-GIT-06: Comprehensive Git integration plan
- F-GIT-02: One banner "N cells changed on disk" vs per-cell dialogs
- F-GIT-03: Per-cell conflict UI with compare/take disk/take mine/copy mine
- F-GIT-04: Conflict markers UI with external editor, show markers, run doctor
- F-IA-24: History search/filter by cell, outcome, session, time range (combinable)

**Weaknesses**:
- No mention of Git blame/annotation in the cell editor
- F-GIT-05: Handles rename/delete/disk-full/antivirus locks — but not yet proven
- F0 phase — Git integration is a core journey (J7) but unvalidated

**Rating**: 7/10 — Well-planned Git integration, but real-world validation pending F2 exit.

---

### 5. Accessibility & Learnability ⭐⭐⭐⭐⭐⭐⭐⭐ 8/10

**Data Scientist perspective**: Easy to pick up, keyboard-navigable for long sessions, clear status indicators.

**Data Engineer perspective**: Keyboard-driven workflows, screen-reader compatibility for pipeline debugging.

**Strengths**:
- F-A11Y-01 to F-A11Y-09: Comprehensive accessibility requirements
- Keyboard model (F-KEY-01 to F-KEY-06) with default shortcuts (Shift+Enter, Ctrl+Enter, Alt+Enter)
- F-A11Y-02: Core journeys completable by keyboard alone
- F-A11Y-07: WCAG 2.2 measured thresholds (4.5:1 text contrast, 3:1 large text, 3:1 non-text elements, 24x24 pointer targets)
- F-A11Y-08: Tab order, focus containment, focus return
- F-A11Y-09: Window title "<notebook> — KernelLedger", accessible names, alt text for rendered output
- Mode indicators in status bar (command vs edit mode announced to screen readers)

**Weaknesses**:
- First-release UI is English only (F-A11Y-06) — no i18n yet
- NVDA/Orca testing "before each platform qualifies" — may not be fully validated
- IME composition, combining characters, emoji testing ongoing

**Rating**: 8/10 — One of the strongest areas; thorough accessibility spec with measured WCAG thresholds.

---

### 6. Performance & Responsiveness ⭐⭐⭐⭐⭐⭐ 7/10

**Data Scientist perspective**: Fast startup, quick output for small experiments, no freezes during output flood.

**Data Engineer perspective**: Large data handling, 1,000-cell notebooks, memory efficiency.

**Strengths**:
- F-PERF-01: Cold start to interactive under 1.5s (cached source under 2s)
- F-PERF-02: First output of `print(1)` under 3s (excluding project interpreter import)
- F-PERF-03: Controls respond within 100ms; large loads asynchronous
- F-PERF-04: p95 frame work under 16.7ms; no sustained freeze during output flood
- F-PERF-05: 1,000 cells open without 1,000 live editors; source loads incrementally
- F-PERF-06: 100 MiB stream inspected through bounded paging
- F-PERF-07: Interrupt and input stay responsive during output flood; overflow resyncs without losing history

**WeaknessES**:
- Targets confirmed on "documented mid-range Windows 11 machine" — may vary on other platforms
- F-PERF-23: Repeatable timing harness and fixtures exist by F2 exit, but F0 may not have full measurements
- Image cache budget (F-PERF-08) and memory target (F-PERF-09: <500MiB idle with 1,000 cells) are initial targets

**Rating**: 7/10 — Ambitious performance targets well-specified; F0/F2 validation pending.

---

### 7. Visual Design & Readability ⭐⭐⭐⭐⭐⭐⭐⭐ 8/10

**Data Scientist perspective**: Clear distinction between source, executed source, live namespace, published checkpoints (F-GOAL-01); compact headers, expandable output (F-UX-02); readable record labels (F-UX-03).

**Data Engineer perspective**: Dark/system/high-contrast themes; status always uses text+icon alongside color (F-UX-01); clear hierarchy in Runtimes, Diagnostics surfaces.

**Strengths**:
- F-UX-01: Shared tokens for spacing, type, color, radius, elevation, focus rings, state indicators; light/dark/system/Hi-C themes; status uses text+icon + color
- F-UX-02: Default density for long notebooks: compact headers, expandable output, resizable panels, persistent scroll position, clearly visible focused cell
- F-UX-03: Lists show relative times; details show absolute local times + stored UTC on hover
- F-UX-04: All 13 data surface states specified and implemented with precise recovery actions
- F-UX-09: After insert/paste/duplicate/move/search, target cell scrolls fully into view with measured bounds
- F-LAY-01 to F-LAY-04: Window size/reflow verified at 100/125/150/200% scaling, 200% Windows text size, multi-monitor DPI moves
- F-IA-23: Each surface owns its data/selection/scroll position; switching surfaces never shows another's results

**Weaknesses**:
- "A missing directory is a recoverable entry" (F-IA-01) — error state not fully tested
- High-contrast theme verification across all surfaces

**Rating**: 8/10 — Excellent visual design tokens and state management; strong layout reflow logic.

---

### 8. Customization & Settings ⭐⭐⭐⭐⭐⭐ 7/10

**Data Scientist perspective**: Theme switching, font size, output limits to match workflow.

**Data Engineer perspective**: Interpreter selector, draft retention, recent notebooks, shortcut customization.

**Strengths**:
- F-SET-01 to F-SET-06: Settings separate from notebook-local config; theme (Light/Dark/Follow system); Follow system tracks OS changes; interpreter selector shows executable, resolution rule, Python version, codec availability; settings summarize active notebook interpreter before offering to change

**Weaknesses**:
- Scientific packages stay in project environment, not global install (F-PKG-01) — limiting for some workflows
- Application never installs packages into project environment (F-SET-04) — may require CLI workflow
- Draft retention (F-D-12) default is 30 days with FD-12 phase gate

**Rating**: 7/10 — Good settings separation, but some workflow constraints (no global installs, project-environment-only packages).

---

## OVERALL COMPOSITE SCORE: 7.6 / 10

---

## Summary by User Persona

### 🧑‍🔬 Data Scientist: 7.8 / 10
**Strengths**: Rapid cell iteration, Markdown/LaTeX output, image visualization, keyboard accessibility, strong error clarity.
**Growth areas**: Charting/visualization libraries, formative usability validation, interactive HTML output.

### 👨‍💻 Data Engineer: 7.4 / 10
**Strengths**: Git integration design, large data stream handling, performance targets, accessibility for keyboard-driven workflows, clear error recovery.
**Growth areas**: Real-world Git conflict validation, package management boundaries, formative usability studies.

---

## Key Recommendations

1. **Run formative usability study (F-QA-04)** with 5-8 participants (mix of new/experienced) to validate the 8/10 ratings
2. **Add charting library option** (F-TRAIN-01 experimental) or integrate with external tools for data scientist visualization needs
3. **Prove F-PERF-05 (1,000 cells)** and F-PERF-06 (100 MiB stream) with real notebooks before F2 exit
4. **Validate F-GIT-02 to F-GIT-04** with real branch-switching scenarios involving edited cells
5. **Consider charting/styling integration** for pandas DataFrame output beyond the basic F-OUT-17 table wrapper

---

*Ratings based on FRONTEND_PLAN.md v4 (2026-10-08) requirements, acceptance scenarios FS-01 to FS-29, and UX qualification gate F-QA-01 to F-QA-05. Untested criteria recorded as "not tested, excluded from score." Scores may increase after F2/F6 validation.*