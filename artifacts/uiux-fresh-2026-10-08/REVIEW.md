# KernelLedger UI/UX review — fresh run, 8 October 2026

**Overall: 6.6/10.** A capable development notebook with a coherent visual language and working core interactions. It needs layout, discoverability, and accessibility work before it feels polished for everyday use.

This is a new hands-on review, not a reuse of the repository's scorecard. I launched the existing native debug executable, created isolated profiles and notebooks, interacted through Windows mouse/keyboard input, inspected UI Automation trees, and captured new screenshots. No application source was modified. Windows reported 192 DPI (200% scaling). Tested outer window sizes included 1626×1271, 1800×1400, and 2560×1700 physical pixels; the latter is approximately 1280×850 logical pixels. This was not a web/mobile review.

## Ratings

Scores are heuristic judgments, not user-study measurements. 5 means usable with substantial friction; 7 means good with noticeable gaps; 9 means highly polished. The overall score is the equal-weight average of these ten dimensions.

| Dimension | /10 | Evidence and judgment |
|---|---:|---|
| Onboarding | 8.0 | Clear welcome page, open/create/sample choices, shortcut guide. Creating a sample through the native folder picker succeeded. The sample explains running code and includes an intentional error. “Opening starts no worker” assumes runtime vocabulary. |
| Visual consistency | 7.5 | Consistent icons, rounded cards, spacing, indigo selection, status badges, and light/dark/high-contrast themes. Some secondary actions are oversized and compete with content. |
| Typography and readability | 7.0 | Code highlighting, line numbers, markdown, and traceback styling are readable. Long paths, command hints, and explanatory copy clip in constrained layouts. Light-theme success green deserves measured contrast checking. |
| Layout and resizing | 4.0 | Compact-to-wide resizing produced an oversized cell outline and squeezed the content pane. Checkpoints overflowed even with the compact outline hidden. Runtime actions and Help content also clipped. A newly opened wide sample had much healthier proportions. |
| Navigation and discoverability | 7.0 | Named destinations and current-page indication are clear; Ctrl+Shift+P and filtering work. The outline remains present in Settings/Help/History, consuming space without helping those tasks. More opens the command palette, whose initial entries emphasize advanced training actions. |
| Editing and execution | 7.5 | Editing, saving to the source file, Ctrl+Enter, click-to-run, Run all, stdout, results, errors, execution counts, and stale-output indicators worked. Adding cells is less discoverable when the compact layout hides the outline. |
| History and search | 6.5 | Session/execution lists, success/error badges, durations, source/output inspection, and outcome:error filtering worked. Filter syntax is packed into placeholder text; ordinary users would benefit from visible filter controls and chips. Large Back and Raw details controls dilute the hierarchy. |
| Checkpoint workflow | 6.0 | Namespace checkpoint creation and restore succeeded; the ledger confirmed a new session linked to the checkpoint. Empty states lack a contextual Create checkpoint action. The page's toolbar checkpoint action is disabled outside Notebook. Restore presents imported-code trust and Python mismatch controls even for this local checkpoint. |
| Status, feedback, and recovery | 6.0 | Idle/offline state, error panels, stale-output badges, and retention warnings help. Palette unavailability said to open/start the runtime while the notebook was open and status showed Idle; returning to Notebook enabled actions. Diagnostics marks a failed cell operation green “completed,” which is defensible transport terminology but confusing outcome feedback. After the interruption between review turns, Disconnected appeared; its cause was not established. Reconnect did not visibly clear it in the immediate spot-check. |
| Keyboard and accessibility | 6.5 | Ctrl+Enter, Ctrl+S, Ctrl+Shift+P, command filtering, Escape cancellation, named navigation/buttons/editors, and high contrast are useful foundations. The settled UIA tree exposed the palette input but not its command rows; outline entries were also absent, and the history clear icon had no name. No NVDA or full focus-order certification was performed. |

## Highest-priority changes

1. **P1 — Repair resizing and overflow.** Reproduce by opening a notebook, shrinking to the minimum, then expanding to 2560×1700 physical pixels at 200% scale. The outline grew to roughly half the workspace. Constrain/restore outline width, preserve usable content minimums, wrap long text, and keep row actions reachable. Independently fix the Checkpoints tab/helper row overflow. Validate repeated grow/shrink transitions, not just initial layouts.
2. **P1 — Make action availability truthful.** When runtime actions depend on the current destination, explain that dependency or make them work consistently across destinations. “Open the notebook and start its runtime” is misleading with an open, idle runtime.
3. **P1 — Expose interactive content to assistive technology.** Add semantics for command rows and cell outline entries, selected/disabled state, and an accessible name for the search-clear button. Verify with a screen reader after the UIA fixes.
4. **P2 — Give checkpoints a complete local workflow.** Put Create checkpoint in the page and empty state, show progress and success with a View action, and refresh the list when creation finishes. Surface the saved values/omissions report before restore. Make trust and version overrides conditional on actual need.
5. **P2 — Simplify history and diagnostics.** Offer outcome/date/session filters with visible labels; keep query syntax as an advanced option. Show execution outcome separately from operation completion. Reduce full-width secondary buttons and use compact back navigation.
6. **P2 — Improve reversibility and feedback.** The deletion dialog correctly explains that structural undo is unavailable and Escape canceled successfully. Add structural undo or a recoverable trash flow. Give save/checkpoint operations clearer completion feedback, and make reconnection status actionable and verifiable.

## Workflows actually exercised

- Fresh launch with an isolated notebook/profile; separate welcome launch with no notebook.
- Create sample notebook using the native folder picker; verified generated cells after loading settled.
- Run a code cell and inspect stdout/result; run deliberate division-by-zero and inspect its recorded error.
- Edit code, save, verify file contents, observe stale output, and execute using Ctrl+Enter.
- Run all sample cells; inspect History and filter for outcome:error.
- Create namespace checkpoints and restore one; read-only ledger inspection confirmed completion and restored-session lineage.
- Inspect all seven destinations: Notebook, Help, History, Checkpoints, Runtimes, Diagnostics, Settings.
- Open/filter the command palette; cancel a deletion confirmation with Escape.
- Switch light, dark, and high-contrast appearance; resize through compact and wide layouts.
- Inspect Windows UIA roles/names and save screenshots of tested states.

No crash was observed during active interaction. Responsiveness felt adequate for these tiny notebooks, but this run did not measure latency or test large notebooks, GPU training, rich scientific outputs, sustained jobs, interruption, crash recovery, IME, mixed-monitor DPI, or a screen reader. Those are unscored coverage gaps, not assumed passes. The transient empty sample screen during creation was allowed to settle and is not reported as a failed creation.

## Screenshot evidence

- [Welcome](welcome/welcome.png)
- [Fresh wide sample with healthy initial proportions](welcome/sample-settled.png)
- [Successful execution](executed-ready.png)
- [Edited cell and stale output](edited.png)
- [Oversized outline after resizing](settings.png)
- [Checkpoints overflow with outline hidden](checkpoints-empty.png)
- [Clipped runtime actions](runtimes.png)
- [Filtered history](welcome/search-results.png)
- [Runtime-unavailability wording](palette.png)
- [Restore confirmation](restore-confirm.png)
- [Delete confirmation](delete-confirm.png)
- [High contrast](contrast.png)
- [Disconnected recovery view after resuming review](welcome/recovery-settled.png)

Adjacent JSON files contain the UI Automation snapshots. Fixture scripts, profiles, notebooks, and screenshots are confined to this review directory. The extra audit window/runtime was closed; the sample notebook app was left open for inspection.
