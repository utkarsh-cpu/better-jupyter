# Screenshot-based UI polish

Implements the eight requirements in the
[screenshot polish plan](../../plans/frontend-screenshot-polish-draft.md).
This pass changes presentation and navigation only. Execution, trust, context
approval, source revision, and recovery behavior are unchanged.

## What changed

1. **Compact spacing.** Compact mode uses a 2px run-to-source gap. Comfortable
   mode keeps the previous 8px gap and its target sizes.
2. **Editing-first cell menu.** Duplicate and Move come first. Assistant actions
   sit in a group entered with Right and left with Left. Escape closes the menu
   and returns focus to the invoking More actions control. Delete stays in its
   own group.
3. **Assistant setup.** An unconfigured assistant shows Connect a provider, a
   short explanation, and Open provider settings. Selected cells appear as
   removable chips. Review context states that nothing is sent until approval.
4. **Toolbar scope.** The baseline width shows a labeled Run all and Export.
   At 800×600 and at 200% UI scale those actions fall back to icons with their
   existing accessible names. Cell Run stays on the cell.
5. **History.** Wide layouts use Cell, Outcome, Run time, and Duration columns,
   with the exact timestamp under the relative time. Narrow layouts stack those
   fields as labeled rows. Apply, Clear, and pending filter state stay explicit.
   Raw records sit under Technical details. The detail subtitle uses the same
   fields as the list instead of one sentence.
6. **Checkpoints.** Known reasons use plain language and keep the reason code.
   Partial explains that only saved values return. A missing count is unknown,
   not zero. Restore and Continue describe what they do. Structured payloads
   stay under Technical details.
7. **Settings.** Appearance, Editor, Layout & accessibility, Runtime, and
   Assistant are separate sections. A sample preview shows font and density
   choices and does not execute code.
8. **Welcome and recents.** Recent rows show the name, path, and recorded open
   time. A missing path says it was not found and offers Remove. The welcome
   line says source stays in ordinary files and execution history is recorded
   locally in SQLite.

## Acceptance evidence

Captures are native windows of a disposable fixture, written by
`scripts/desktop_ui_components.py` on 2026-11-01. The host is Windows 11 at
192 DPI. Before and after sets live under
[`artifacts/frontend-screenshot-polish/2026-11-01`](../../artifacts/frontend-screenshot-polish/2026-11-01).

| Set | Executable | Result |
| --- | --- | --- |
| Before, compact and comfortable | `9165d0980ac5` | Confirms the old spacing, copy, menu, and single settings card. New section and assistant-group names are absent, so those journeys fail. |
| After baseline, dark, contrast, comfortable | `54cdeeb07ee7` | 44 screens each, no journey failures. |
| After 800×600 | same | Outline is hidden. The outline crop has no bounds. Other journeys pass. |
| After 200% at 1280×850 | same | Outline is hidden. The later Test connection step is not in the tree. |
| After 200% at 800×600 | same | The window cannot show the full notebook. History, diagnostics, help, the error crop, and Test connection are not reachable in this capture. |

Both executables were built from dirty revision `6a62fa8`. That work is in
`d350595`. The after binary is the one named in each after `manifest.json`.

Compact cell pitch dropped from 177px and 168px to 161px and 152px. The
run-to-source gap dropped from 8px to 2px. Comfortable pitch stayed 209px and
214px with an 8px gap. High contrast measures 162px and 153px with a 3px gap.

On every after configuration the cell menu opens with editing actions before
Assistant, Right enters that group, Left returns, and Escape restores focus to
`More actions for print('Hello, KernelLedger')`. The More actions control is
hidden until hover or focus, so the probe parks the pointer on it and waits
until that name is focused before it invokes the menu.

Reviewed screens match the requirements: labeled Run all and Export, the
editing-first menu, the Connect a provider card, columnar history, the split
detail subtitle, plain checkpoint copy, sectioned Settings with a preview, and
recent rows that distinguish two notebooks named `notebook` plus a missing path.
At 800×600, Run all is icon-only, Export moves into overflow, history uses
stacked labels, and those labels do not overlap the title bar.

UI tests covering the changed behavior include
`cell_menu_puts_editing_first_and_assistant_actions_target_the_invoking_cell`,
`history_pending_edits_never_describe_the_list_and_clear_resets_both`,
`assistant_setup_follows_settings_and_test_results_without_leaking_keys`,
`provider_setup_round_trip_keeps_the_question_and_context`, and
`caption_gutter_collapses_export_before_settings_at_compact_200_percent`.

## Residual limits

This host's display cannot show a full 1280×850 or 800×600 window once UI scale
is 200% on top of 192 DPI. The 800×600 plus 200% capture is evidence of that
clamp. It is not evidence that those controls overlap or that 200% alone hides
them. The separate 800×600 and 200% captures are the ones that match the plan.

The outline crop fails whenever the outline is collapsed. That is the adaptive
layout, not a missing control.

Keyboard focus return and accessible names were checked through UI Automation.
A screen reader was not run. These captures do not qualify NVDA, IME, mixed
DPI, macOS, or Linux, and they are not a release qualification.
