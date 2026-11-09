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
| Notebook / History / Checkpoints | Ctrl+1 / Ctrl+2 / Ctrl+3 |
| Settings | Ctrl+, |
| Toggle outline | Ctrl+B |
| Find notebook cells | Ctrl+Shift+F |
| Insert code / Markdown below | Ctrl+Shift+N / Ctrl+Shift+M |
| Duplicate selected cell | Ctrl+Shift+D |
| Move cell up / down | Ctrl+Alt+Up / Ctrl+Alt+Down |
| Delete cell (confirmation) | Ctrl+Shift+Delete |
| Create checkpoint | Ctrl+Shift+S |
| Interrupt execution | Ctrl+Shift+I |
| Export notebook | Ctrl+Shift+E |

In the command palette, type to filter, use Up/Down or Home/End to select, Enter to activate, and Escape to return focus.

## Export a document

Choose **Export** in the title bar (a document icon on compact windows), or press
**Ctrl+Shift+E**. Choose **PDF document** for reading and sharing or **Word document**
for an editable .docx file, then choose a save location. Jupyter and portable bundle
exports are also available. The completion notice offers **Open file** and **Show
in folder**.

PDF and Word include current source and the latest recorded outputs per cell;
export never runs code. Outputs from an older source revision are labeled. Markdown
headings, lists and tables, code, text, errors and recorded PNG/JPEG plots are
included. Inline Markdown styling becomes plain text. Interactive and HTML/SVG-only
outputs are marked as omitted; use portable bundle export to preserve them.
Remote Markdown images are not downloaded. Resolve source conflicts before
exporting; pending edits are saved automatically.

## Inspect a bundle

**Menu → Inspect bundle** or **Commands → Inspect bundle** reads a portable
bundle without running code and without replacing notebook source. Diagnostics
shows the bundle name, recorded session and checkpoint counts, named external
files, and file-hash mismatches. A mismatch blocks import. Git conflict markers
and external source conflicts stay listed and unchanged.

**Import recorded history** copies sessions, executions, and checkpoints into
the open notebook after you confirm trust. Local records with the same
identifiers are kept. Saved code in the bundle does not run during import.

## Install packages and run shell commands

Code cells accept notebook shell escapes alongside Python:

```python
!pip install pandas
import pandas as pd
pd.DataFrame({"value": [1, 2, 3]})
```

`%pip install pandas` also works. `!pip`, `!pip3`, `!python`, and `!python3` use
the selected notebook Python environment. Package output appears in the cell;
newly installed packages can be imported in subsequent Python statements. An
already imported package may require a runtime restart after upgrading it.
If pip is missing, the notebook sets it up with Python's bundled `ensurepip`
before running the requested pip command.

Other `!` commands use the operating system's shell: `cmd.exe` on Windows and
`/bin/sh` on Linux/macOS. For example, `!dir` lists files on Windows, `!ls` lists
files on Linux/macOS, and `!echo hello` works on both. Pipes, redirects and quoted
arguments follow that shell's syntax.

Capture output into a Python list with `lines = !command`, or display the captured
list with `!!command`. A nonzero command exit marks the cell as failed and is
available as `_exit_code`. Interrupt stops a running command and its children;
the notebook worker remains available.

Use `%pwd` to get the current directory and `%cd "folder with spaces"` to change
it for later cells. `!cd` only changes the temporary shell's directory. Shell
commands run without interactive stdin; use Python `input()` for notebook input.
Background shell commands and IPython magics beyond `%pip`, `%cd`, and `%pwd` are
not supported. `!` inside Python strings and comments stays ordinary text.

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

## Still incomplete in this build

These workflows are named here so Help does not present them as finished:

- **Structural undo** retains at most 20 operations and 16 MiB of patches (4 MiB per operation). Larger changes clear this history. Undo refuses to overwrite newer changes to affected files. Recorded executions stay in History independently.
- **Checkpoint preflight** is not a verified compatibility report. Provenance, source, and environment counts stay labeled unknown until the service reports them.
- **History search** examines at most 100 candidate runs per request. Free text searches recorded sources up to 1 MiB each; larger or unavailable sources are flagged. An empty page can have a Next page because later candidates have not been searched yet.
- **NVDA and IME** are not qualified. Controls have keyboard names; a screen-reader and input-method pass is still required before release.

## About this build

KernelLedger desktop 0.1.0. Runtime plan 5; frontend plan 4 baseline with the frontend v6 draft implementation under development; desktop protocol v1. This development build uses the configured Python environment rather than a bundled private distribution.

Multiple-cell selection, key rebinding, detached windows, and publish/pin are not available in this build. Inspect bundle reviews a portable bundle before import; `kl import` remains available for scripted imports. See the shipped keyboard guide, desktop README and platform qualification document in docs/desktop.

Minimum window: 800 x 600 logical pixels. Below 1,024 pixels the notebook outline becomes a drawer. The Outline button opens it. A manually collapsed outline stays collapsed when resizing. Settings offers interface sizes from 100% to 200%, independent editor text size, Compact or Comfortable spacing, outline width, and System, Light, Dark or High contrast appearance. Larger interface sizes also enlarge controls and spacing; scroll supporting pages to reach later settings. Activating a settings control below the window scrolls that group into view. Reduce motion starts from the system animation setting until you change it.

## Navigate the workbench

Open **Settings** from the gear button at the top right. Wide windows also show
its text label. **Ctrl+,** and **⋯ → Settings** open the same page.

Use Notebook, History, and Checkpoints in the header. Select the notebook name to switch open notebooks. Menu contains Settings, Help, Diagnostics, and all runtimes. At compact widths, Commands and Runtime move into Menu. Runtime shows the current interpreter and relevant runtime actions. Commands searches common and advanced actions and explains unavailable commands.

## Edit cell structure

Add cell in the toolbar offers Code, Markdown, or Raw. Each boundary has one insertion control, visible on hover or keyboard focus; the selected boundary and Comfortable spacing keep it visible. Select a cell and open More for Duplicate, Delete, Move up, and Move down. Commands also offers these actions, Split cell at cursor, and Merge with next cell. These require saved source with no unresolved conflicts; deletion and merge require confirmation, and merge requires matching cell types. Undo cell structure and Redo cell structure are in Commands. Ctrl+Z / Ctrl+Shift+Z use structural undo in command mode; an editor retains its text undo. Save pending edits first. Undo preserves stable IDs and attachments and refuses affected files with newer changes. Recorded executions remain separate from current source. Markdown has Edit/Preview controls and no Run action.

## Inspect results and history

Output tools reveals Copy, Find, Hide output, and display-size controls. A large recorded stream is shown one page at a time. Previous page and Next page read that byte range from the recorded blob; the rest of the stream stays on disk. Copy and Find use the page on screen. Display-size controls request a new range instead of only hiding already loaded text. Copy briefly acknowledges success. Show traceback appears beside the exception summary. Images offer fit and actual size. The decoded image cache drops figures that scrolled away and stays within its byte budget; Show again reloads a figure that was evicted. Tables offer a plain-text fallback. Cell attachments show a bounded text or image preview. Arbitrary HTML and scripts are not executed.

History provides text search, outcome buttons, and Cell, Session, From, and Through fields. Dates use YYYY-MM-DD; Apply stays unavailable for invalid dates or a reversed range. Advanced cell:, session:, after:YYYY-MM-DD, and before:YYYY-MM-DD queries remain supported. Cell and Session match exact IDs. Apply requests a bounded page of at most 100 candidates; outcome, cell, session, and date filters run in the service. Free text searches supported source blobs up to 1 MiB per execution, returning a preview of at most 1 KiB. A notice identifies sources that could not be searched. An empty page can still offer Next page because later candidates have not been searched yet. Next page follows a stable row cursor; Back restores the previous page or list. Details show recorded source once. Changed source reveals a comparison; unchanged source can be compared on request. Return to cell returns to the notebook editor. Filters and list scroll belong to each notebook.

`display(value, display_id='status')` gives an output a session-scoped ID.
`update_display(value, 'status')` updates matching displays across cells in the
current notebook view. Updated outputs name the updating execution; History
retains each execution's own evidence. Rerunning a cell replaces its old targets,
and restarting/restoring isolates the new session. `.ipynb` export includes the
updated view and its provenance. If replay or cache limits leave outputs stale,
the notice offers Reload outputs without running code.

## Save and restore a checkpoint

Choose Create checkpoint on Checkpoints when the runtime is ready. After completion, View checkpoint opens the coverage report. Review exclusions before Restore or Continue. Missing saved-value counts and creation context are reported as unknown. These actions change live state and require confirmation. Coverage is not proof of complete process recovery or environment compatibility. Trust remains an explicit decision; a Python version override appears only after a reported mismatch for that checkpoint. Successful restore identifies the new session and offers Return to notebook. Technical details retain the raw report and identifiers. Advanced checkpoint types contains experimental training actions.

## Export and diagnose

Commands offers Export notebook and Export bundle, each with a native save dialog. Source is saved through the notebook service before export. Bundles require recorded history. The result appears in Diagnostics for the originating notebook. Run health check refreshes findings; Copy redacted diagnostics excludes source text, paths, environment values, and raw errors.

## Interactive Python

The editor offers completion and passive documentation. Source names remain
available offline; live names require an idle worker. Escape dismisses suggestions
and help without running the cell. Commands → Variables opens an inspector:
Refresh live variables reads a bounded snapshot, and selecting a variable opens
its first value page. Refresh after running or restoring. Passive previews do not
call user properties, `repr`, or `dir`; unsupported types show their type only.

Commands → Console opens an input in the same notebook session. Its Run button
submits ordinary recorded work. Enter edits the input. History labels console
runs and can insert recorded source into a new cell for review before execution.
Commands → Load source file previews up to 64 KiB before insertion.

End an expression with `;` to suppress its result. `%time`, `%%time`, `%timeit`
and `%prun` report bounded timings/profiles. Repeated timing repeats side effects.
`%%capture [fresh_name]` captures text. `%history on` enables the optional bounded
`In`, `Out`, `_`, `__`, `___` cache; it refuses existing names. `%history clear`
and `%history off` release cached references. Settings has the same controls.
`%commands` lists supported commands. `name?` shows help; `name??` adds recorded
source when available. Top-level `await` works; tasks are canceled after each cell.
A task that refuses cancellation requires a runtime restart before more execution
or checkpoints. Future-import settings reset on namespace restore.

`%run path.py` records the file bytes, path, hash and working directory, then
executes in this namespace. `%%writefile path` writes the exact cell body; `-a`
appends. Shell cells use `%%sh`/`%%bash` on POSIX or `%%cmd` on Windows. These
commands have normal external effects. Restoring a namespace does not undo files
or packages.

Debugger and native widgets are experimental. Start the desktop/owner with
`KL_EXPERIMENTAL_INTERACTIVITY=1` to enable them. Commands → Debug selected cell
pauses at entry. Gutter controls and Toggle breakpoint at caret bind to the saved
cell revision while the worker is idle. The paused line has an arrow and highlight.
Commands → Debugger shows stack/locals and stepping controls; after an exception,
post-mortem inspection offers Continue or Stop. Checkpoints are unavailable while
paused. One client owns controls until execution ends; terminate remains available
if that client disconnects.

Stack and profile source links return to the matching cell and caret. When that
cell has changed, they open a read-only recorded source window and preserve your
edits. Previous/next controls page the recorded source.

`widget('button', label='Run', callback=lambda value: print(value))` creates a
native control. Supported kinds are button, checkbox, dropdown, integer_slider
(stepper) and read-only progress. Callbacks are recorded executions. Historical
and imported widget snapshots stay inert. This subset does not load ipywidgets or
execute JavaScript. Static tables use `display_table(rows, columns=...)`; media
attachments use `display_media(data, mime)`. Save chooses a new filename, verifies
the complete content and offers Open file in the system application. Files over
32 MiB and existing destinations are refused. Nothing plays automatically.
Equations support native fractions, roots, scripts and common symbols; unsupported
TeX remains visible as source. See docs/notebook-interactivity.md for limits and
the optional local Jupyter adapter setup.


## Experimental debugging: technical details

Experimental debugging is disabled unless the runtime advertises debugger support.
To opt in, start KernelLedger and its runtime with `KL_EXPERIMENTAL_INTERACTIVITY=1`
in the launching environment. Restart an already running runtime after changing
that environment. Then use **Debug selected cell** in Commands. There is no
Settings switch for this runtime capability. Breakpoints require saved source.

## Recover notebook services

Application Python loads notebook source and runs the application services. It is separate from the project interpreter used to execute cells. KernelLedger tries the active environment, nearby `.venv` environments, and Python on PATH. `KL_RUNTIME_PYTHON` explicitly overrides discovery.

If notebook services cannot start, use Retry loading or Choose application Python. Select the environment folder (for example, `.venv`) or a Python executable with KernelLedger installed. On macOS, choose the folder for virtual environments so the file picker does not resolve the Python symlink out of that environment; a failed selection keeps any working services attached. Settings → Runtime also offers these controls. A successful selection is remembered for this desktop profile unless the environment override is set. Retrying does not resubmit executions. Technical error in Diagnostics retains the detailed startup failure.

Assistant Settings shows Model, Base URL, provider save actions, and API key first. Advanced settings reveals coding model, request limits, schema, and inference options. Save configuration before using Test saved connection; that action tests the saved endpoint and model.
