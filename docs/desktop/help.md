# Help and shortcuts

Run bindings work in both edit and command mode and leave source unchanged. Plain Enter in edit mode inserts a newline.

| Action | Shortcut |
| --- | --- |
| Run cell, select next | Shift+Enter |
| Run cell, keep selection | Ctrl+Enter |
| Run cell, insert below | Alt+Enter |
| Run all | Ctrl+Shift+Enter |
| Save | Ctrl+S |
| Open notebook | Ctrl+O |
| Command palette | Ctrl+Shift+P |
| Command mode / cancel | Escape |
| Back from recorded details | Alt+Left |
| Help | F1 |

In the command palette, type to filter, use Up/Down or Home/End to select, Enter to activate, and Escape to return focus.

## Training checkpoints

Training checkpoint is a separate experimental action for an idle notebook cell.
Explicitly import `kernelledger.gpu.notebook.register` and register a model,
optimizer, reconstruction factory file and next-batch cursor. Mutate registered
objects only inside `training.training_step`; provide a loader drain callback
when needed. Registration does not discover arbitrary notebook variables or
external data-loader state.

Checkpoints → Training snapshots shows saved coverage. Restore training asks you
to trust the captured factory, reconstructs in a fresh candidate worker and
replaces the current worker only after success. A failed candidate preserves the
live worker. The reconstructed plugin is available as `training`.

Namespace checkpoints, portable training checkpoints and native process snapshots
have different coverage. Training saves exclude the namespace and native driver
context. Native snapshot availability in the palette reports the current platform
limits; Windows and the local AMD WSL target have no supported native backend.

## Glossary

- **Session**: recorded work sharing one namespace. An open session is Active; Running describes computation.
- **Worker**: the Python process executing notebook code.
- **Runtime**: a supervisor and its worker for a notebook.
- **Supervisor**: manages execution, durable records, and clients.
- **Namespace**: the live Python variables in a worker.
- **Checkpoint**: published values with a report of omissions.
- **Restore**: load checkpoint values into a new session.
- **Continue**: restore, then run later saved source.
- **Replay**: run recorded source again; side effects may repeat.
- **Trust**: permission to use imported saved code or values for the selected operation.

## About this build

KernelLedger desktop 0.1.0. Runtime plan 5; frontend plan 4 baseline with the frontend v6 draft implementation under development; desktop protocol v1. This development build uses the configured Python environment rather than a bundled private distribution.

Structural undo, multiple-cell selection, key rebinding, detached windows, import review dialogs, and publish/pin are not available in this build. Use `kl import` and `kl bundle` for import workflows. See the shipped keyboard guide, desktop README and platform qualification document in docs/desktop.

Minimum window: 800 x 600 logical pixels. Below 1,024 pixels the notebook outline becomes a drawer. The Outline button opens it. A manually collapsed outline stays collapsed when resizing. Settings offers interface sizes from 100% to 200%, independent editor text size, Compact or Comfortable spacing, outline width, and System, Light, Dark or High contrast appearance. Larger interface sizes also enlarge controls and spacing; scroll supporting pages to reach later settings.

## Navigate the workbench

Use Notebook, History, and Checkpoints in the header. Select the notebook name to switch open notebooks. Menu contains Settings, Help, Diagnostics, and all runtimes. At compact widths, Commands and Runtime move into Menu. Runtime shows the current interpreter and relevant runtime actions. Commands searches common and advanced actions and explains unavailable commands.

## Edit cell structure

Add cell in the toolbar offers Code, Markdown, or Raw. Each boundary has one insertion control, visible on hover or keyboard focus; the selected boundary and Comfortable spacing keep it visible. Select a cell and open More for Duplicate, Delete, Move up, and Move down. Commands also offers these actions, Split cell at cursor, and Merge with next cell. These require saved source with no unresolved conflicts; deletion and merge require confirmation, and merge requires matching cell types. Structural undo is not yet available. Recorded executions remain separate from current source. Markdown has Edit/Preview controls and no Run action.

## Inspect results and history

Output tools reveals Copy, Find, Hide output, and display-size controls. Copy and Find operate on loaded output; viewer limits do not remove recorded data. Copy briefly acknowledges success. Show traceback appears beside the exception summary. Images offer fit and actual size, and tables offer a plain-text fallback. Arbitrary HTML and scripts are not executed.

History provides text search, outcome buttons, and Cell, Session, From, and Through fields. Dates use YYYY-MM-DD; Apply stays unavailable for invalid dates or a reversed range. Advanced cell:, session:, after:YYYY-MM-DD, and before:YYYY-MM-DD queries remain supported. Filters apply to loaded records; Search executions loads all recorded executions, which is not yet bounded for large histories. Next page follows the recorded list cursor; Back restores the previous page or list. Details show recorded source once. Changed source reveals a comparison; unchanged source can be compared on request. Return to cell returns to the notebook editor. Filters and list scroll belong to each notebook.

## Save and restore a checkpoint

Choose Create checkpoint on Checkpoints when the runtime is ready. After completion, View checkpoint opens the coverage report. Review exclusions before Restore or Continue. Missing saved-value counts and creation context are reported as unknown. These actions change live state and require confirmation. Coverage is not proof of complete process recovery or environment compatibility. Trust remains an explicit decision; a Python version override appears only after a reported mismatch for that checkpoint. Successful restore identifies the new session and offers Return to notebook. Technical details retain the raw report and identifiers. Advanced checkpoint types contains experimental training actions.

## Export and diagnose

Commands offers Export notebook and Export bundle, each with a native save dialog. Source is saved through the notebook service before export. Bundles require recorded history. The result appears in Diagnostics for the originating notebook. Run health check refreshes findings; Copy redacted diagnostics excludes source text, paths, environment values, and raw errors.
