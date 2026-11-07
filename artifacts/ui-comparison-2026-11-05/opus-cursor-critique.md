# Parent critique: Claude Opus 5.5 xhigh in Cursor

## Verified outcome

Opus reported working image viewing in Cursor and scored the eight PNGs before reading the earlier reviews. Its saved initial file says image_viewing_status=verified. The initial-score file SHA-256 matches the value in its final report: DE8A99BABAC27DA81B8DCC71B63790151C5F8B51AD9B2AF442BD030B1CBE05BD.

I checked all ten scores against the report and recalculated the sums:
- Marimo: 71/100, mean 7.1/10.
- KernelLedger: 78/100, mean 7.8/10.

These are heuristic ratings of saved local captures, not a fresh app run or task study. The Antigravity audits remain unscored and are not additional visual reviewers.

## What Opus adds usefully

1. **A better density comparison.** Comparing the same source-visible, output-visible sales cell avoids much of the whole-screen Markdown/traceback-state confound. Opus measures its complete vertical slot at approximately 196 logical px in Marimo versus 240 in KernelLedger: Kernel is about 22% taller, or Marimo about 18% shorter. The report separately gives Marimo's card body as about161px. Do not confuse card-body and whole-slot measurements. This is one four-line cell, not a measured saving for every notebook.
2. **Chrome and text spacing are separate design levers.** A dedicated control header and the Markdown edit strip consume height independently of code font size. That improves on the earlier blanket density-versus-readability framing.
3. **Truncation has a stronger local explanation.** The switcher occupies about96 logical px while the captured header has a much larger blank center. Allocating some of that space to the name is more promising than only adding a tooltip.
4. **Resting-hidden controls need a focus check.** UIA lists controls not painted at their coordinates in this resting screenshot. That is a legitimate reason to test keyboard reveal and focus order, not proof of a defect.
5. **Its accessibility restraint is appropriate.** It distinguishes missing serialized names from missing Text/Value/Selection patterns and actual screen-reader behavior.

## Where I disagree or would narrow the claims

- **The narrow-window score still mixes state and layout.** The two-point 6.0 versus8.0 gap includes Marimo's open Markdown source and traceback. Horizontal crowding and filename clipping are separate observed candidates; the vertical fit is not a clean responsive-design experiment. Treat the gap as a rating of these captures, not an intrinsic product difference.
- **It still scores rows it says should be withheld.** The initial error and settings scores were frozen before later criticism, which is good provenance. But those rows remain in the headline average even though the report recommends withholding them. Publishing low confidence helps; it does not resolve the mismatch.
- **Removing a header is not guaranteed to leave readability or usability unaffected.** A gutter run control and overlay Edit button may recover height while hiding action meaning or covering content. Prototype them with large text, keyboard focus and comfortable mode. Keep the existing target sizes and selected-cell action access.
- **The exact240px switcher cap is a proposal.** Blank title-bar space also supports window dragging. Allocate space responsively while keeping a reliable drag region and handling long names, rather than treating every blank pixel as waste.
- **Glyph ink measurements are not font sizes.** The evidence does not support my original large text-size advantage, but Opus's approximate glyph boxes cannot establish exact CSS/native font sizes either. Its roomier line-pitch observation is the stronger finding.
- **The848ms import attribution is unproven.** The cell includes importing marimo, but one displayed timing does not isolate import overhead from other initialization. Recommend quiet timing displays without asserting what dominated that run.
- **Its chart critique partly misreads the newer artifact.** reviewer-comparison.html already includes unrounded sums/means and disclosure/settings caveats; the older comparison.html lacks exact sums. A blanket statement that both omit them is inaccurate. Neither provides a validated confidence interval.
- **Sensitivity is not causal proof.** Its7.0 versus7.9 sensitivity row excludes three confounded dimensions and then merges two others. If only navigation/discoverability are merged while all other rows remain, the means are approximately7.14 versus7.78. Either way the lead persists numerically, but correlated judgments, narrow-layout state and reviewer taste are not eliminated.
- **Shared direction is not independent usability validation.** Three reviewers seeing the same small fixture can repeat the same bias. Their ratings should stay separate, not become a precise consensus average.

## Recommendations I would prioritize

| Priority | Action | Why | Validation |
|---|---|---|---|
| 1 | Test live native accessibility for rendered Markdown, footer status, settings selection and keyboard reveal of hidden toolbar controls. | Concrete gaps in the saved evidence affect content/focus access if real. | Inspect provider patterns and Narrator/NVDA behavior; fix only confirmed failures. |
| 2 | Let the notebook switcher use more available width at roomy sizes; provide focus-accessible full labels and disambiguation. | The captured truncation occurs despite ample center space. | Preserve a drag region; test long similar names at800px and large text. |
| 3 | Prototype compact cell headers and Markdown action placement while preserving text/line pitch. | Better supported than shrinking type to save space. | Use a30+cell mixed notebook and measure run/edit discovery, focus and pointer errors. |
| 4 | Test keeping Run all labeled at800px, moving a secondary destination to overflow if necessary. | Avoids ambiguity between running one cell and the notebook. | Compare tasks rather than assume the current icon fails. |
| 5 | Keep per-cell duration optional/quiet and translate raw experimental debugger guidance into plain language with technical details. | Adds context without clutter. | Check expert access and avoid interpreting one timing as a benchmark. |

For Marimo, the best candidates are filename-first path display, less floating-control crowding at narrow widths, and verification of existing traceback wrap/copy and icon-label behavior. Aligning settings checkboxes or neutralizing delete icons are lower-confidence polish ideas; preserve undo, focus visibility and task grouping.

No application changes were made. [Full Opus report](opus-cursor-review.md), [frozen scores](opus-cursor-initial-scores.json), and [reviewer chart](all-reviewer-comparison.html) remain separate from this critique.
