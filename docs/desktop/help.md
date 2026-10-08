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

KernelLedger desktop 0.1.0. Runtime plan 4; frontend plan 3; desktop protocol v1. This development build uses the configured Python environment rather than a bundled private distribution.

Structural undo, key rebinding, detached windows, import/export dialogs, publish/pin are not available in this build. Use `kl import`, `kl export`, and `kl bundle` for interchange workflows. See the shipped keyboard guide, desktop README and platform qualification document in docs/desktop.

Minimum window: 800 x 600 logical pixels. Below 1,024 pixels the sidebar collapses; More > Toggle notebook sidebar restores it. Runtime state and Interrupt stay in the toolbar.
