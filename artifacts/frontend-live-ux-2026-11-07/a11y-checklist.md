# Accessibility checklist (live UX workstream E)

Run on a **qualified** host. Software-render Linux preview is not an AT pass.

## Windows (required before release)

| # | Journey | Narrator | NVDA | Result |
| --- | --- | --- | --- | --- |
| 1 | Read rendered Markdown without entering Edit | | | |
| 2 | Announce cell count, Ready/Saved, runtime state changes | | | |
| 3 | Settings section + theme selection patterns | | | |
| 4 | Tab to contextual cell actions; they become visible on focus | | | |
| 5 | Open/dismiss Commands, cell More, Settings; focus returns | | | |
| 6 | Failed cell: Failed badge + Explain error named; traceback disclosure | | | |
| 7 | Disabled Interrupt / Markdown Run / untrusted Restore report disabled | | | |
| 8 | Destination change clears menus; no stale tooltip speech | | | |

## Linux (preview after Workstream G hardware path)

| # | Journey | Orca | Result |
| --- | --- | --- | --- |
| 1 | Keyboard-only: open fixture, run cell, open History, Esc restore | | |
| 2 | Help search + About | | |

## Code already in place for these journeys

- Element-owned tooltips (`WorkbenchButton::hint`)
- Destination change clears menus/palette (`select_surface`)
- Failed cell a11y includes `, failed`
- Product wordmark / status wordmark labeled `KernelLedger`
- History Filters control has explicit show/hide names
