# Native macOS UI test — 8 November 2026

Environment: macOS 26.3 Apple Silicon, native development executable, disposable notebook `/tmp/kernelledger-macos-screenshot-notebook`, explicitly configured KL_RUNTIME_PYTHON. Used actual keyboard and pointer events and native window screenshots; these are not mockups. Existing personal notebooks and credentials were not modified. Appearance was changed to Light during testing and window size returned to 1280×850.

## Findings

1. **P1: provider settings obscured by assistant at 800×600.** Open Assistant, Settings → Assistant, resize to 800×600. Right side of the form is covered by the panel; controls/content cannot all be read. Evidence: `17-narrow.png`. Use a full-width settings route, collapse the assistant, or make it an explicitly dismissible overlay with a usable underlying layout.
2. **P2: History stays empty after execution until Apply.** Open History before any execution; return to notebook and Run All; reopen History. It still says no recorded history. Press Apply and the successful record appears. Evidence: `06-history-populated.png` vs `20-history-apply.png`. Refresh on navigation or invalidate the loaded history after completion.
3. **P2: Runtime settings version status is misleading.** Runtime is Ready following successful execution, but Runtime settings says “Version available after runtime start.” Evidence: `15-runtime-settings.png`, `28-runtime-menu.png`. Update version metadata or explain that it could not be obtained.
4. **P2: checkpoint coverage lacks saved-value count.** A newly created checkpoint is Complete, but Inspect says count not reported despite the fixture containing `values`. Evidence: `21-checkpoint-detail.png`. Populate coverage metadata; distinguish count unavailable from capture completeness.
5. **P2: Mac commands remain Control-based.** Control navigation/save/run/palette works, and Help advertises those bindings. Native Command equivalents were not exhaustively checked. Evidence: `18-help.png`. Centralize platform bindings and labels.
6. **P3: success notification persists across unrelated tasks.** Checkpoint saved banner remains throughout settings, assistant, error and export journeys. It occupies substantial vertical space and competes with current state. Consider auto-dismiss or a notification center for non-actionable success feedback.
7. **Accessibility qualification gap:** System Events entire-contents inspection exposed only a group and native window buttons, rather than semantic app controls. This warrants VoiceOver testing; it does not prove VoiceOver is unusable. No screen-reader pass is claimed.

## Exercised results

| Surface | Actions and observed result | Evidence |
| --- | --- | --- |
| History empty state | Control-2 opened history; search/filter/actions visible | 01-history.png |
| Checkpoints empty state | Control-3 opened checkpoints; create disabled before runtime start | 02-checkpoints.png |
| Settings appearance | Control-comma opened settings; System and Light rendered; Light selection changed appearance | 03-settings.png, 12-light.png |
| Notebook execution | Control-Shift-Enter ran sample; Ready status and Mean: 21.0 displayed | 04-run.png |
| Checkpoint creation | Control-Shift-S completed; saved banner and list item appeared | 19-checkpoint-populated.png |
| Checkpoint inspection | Inspect opened coverage, exclusions, restore/continue explanations | 21-checkpoint-detail.png |
| Commands | Palette opened; typed diagnostics and Enter navigated correctly | 07-palette.png, 08-diagnostics.png |
| Diagnostics | Operations showed completed run/checkpoint; recovery tools visible. Buttons not all executed | 08-diagnostics.png |
| Export chooser | Four formats visible; Escape closed menu | 09-export-menu.png |
| Assistant | Footer opened panel; setup and composer visible; close button worked | 10-assistant.png |
| Editor settings | Section opened; screenshot captured, no editing preference change qualified | 13-editor-settings.png |
| Layout settings | Section opened; spacing, duration, outline width, Reduce motion visible. Toggle behavior not qualified | 14-layout-settings.png |
| Runtime settings | Section opened; interpreter chooser and result shortcuts visible | 15-runtime-settings.png |
| Provider settings | Section opened with endpoint/model/key/configuration controls; no API key or provider configuration stored | 16-provider-settings.png |
| Narrow layout | Resized to 800×600 with assistant open; overlap defect found | 17-narrow.png |
| Help | F1 opened shortcut table and help search | 18-help.png |
| History refresh | Apply populated successful execution record | 20-history-apply.png |
| Authoring | Control-Shift-N inserted cell, focused editor; typed Python and Control-S saved | 22-insert-cell.png |
| Error output | Control-Enter executed intentional ValueError; failure badge, outline dot and error text appeared | 23-error.png |
| Native save dialog | PDF choice opened native Save; entered unique filename and saved | 24-save-dialog.png |
| PDF completion | Export complete notification appeared; file exists; extracted text contains Mean: 21.0, intentional error, αβ and Привет | 25-export-result.png |
| Native open dialog | Control-O opened directory picker; Escape cancelled without replacing notebook | 26-open-dialog.png |
| Deletion confirmation | Control-Shift-Delete opened confirmation; Escape cancelled, cell retained. No deletion performed | 27-delete-confirmation.png |
| Runtime menu | Menu opened, interpreter path visible, idle Interrupt disabled, restart/stop/all-runtimes offered; Escape closed | 28-runtime-menu.png |

`05-create-checkpoint.png` captured a transient black frame and is not test evidence. `11-settings-light.png` predates the successful Light click; use `12-light.png`.

## Not qualified by this run

This is a broad interactive smoke pass, not every possible interaction. Live assistant requests/context approval/proposal apply need a configured service; no remote notebook data was sent. Restore/continue commitment, replay, conflict/draft recovery, runtime interruption while busy, actual deletion, structural reorder, all export formats/import, media/large outputs, debugger/variables/console, VoiceOver, IME, clipboard round trips, high contrast, reduced motion behavior, multi-monitor/Retina combinations, and large-notebook performance remain untested. No blanket “all UI passed” claim is appropriate.
