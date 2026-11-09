# Keyboard guide

On macOS, application commands below use Command instead of Control (for example,
Command+O/S/comma and Command+Shift+P). Run bindings retain Control+Enter and
Control+Shift+Enter; Shift+Enter and Option+Enter also retain their notebook meaning.
Existing Control application shortcuts remain aliases. In-app Help, menus, and
tooltips display the platform's primary binding. Delete means forward Delete
(Fn+Delete on compact Mac keyboards).

Run shortcuts work in edit and command mode. Ctrl+Enter runs and keeps selection;
Shift+Enter runs and selects the next cell; Alt+Enter runs and inserts a cell below.
Plain Enter inserts a newline while editing. During IME composition, application
run shortcuts are suppressed. Escape leaves editing without changing source.

Ctrl+O opens a directory, Ctrl+S saves, Ctrl+Shift+Enter runs all, Ctrl+Shift+P
opens Commands, and F1 opens Help. Alt+Left returns from recorded details.

Additional workbench shortcuts:

| Action | Shortcut |
| --- | --- |
| Notebook / History / Checkpoints | Ctrl+1 / Ctrl+2 / Ctrl+3 |
| Settings | Ctrl+, |
| Show or hide the outline | Ctrl+B |
| Find notebook cells | Ctrl+Shift+F |
| Insert code / Markdown below the selection | Ctrl+Shift+N / Ctrl+Shift+M |
| Duplicate selected cell | Ctrl+Shift+D |
| Move selected cell up / down | Ctrl+Alt+Up / Ctrl+Alt+Down |
| Delete selected cell (asks for confirmation) | Ctrl+Shift+Delete |
| Create a namespace checkpoint | Ctrl+Shift+S |
| Undo / redo cell structure in command mode | Ctrl+Z / Ctrl+Shift+Z |
| Interrupt execution | Ctrl+Shift+I |
| Export notebook | Ctrl+Shift+E |

Cell structure shortcuts apply in Notebook after source is saved and conflicts
are resolved. Runtime shortcuts use the same availability checks as their buttons.
Menus and confirmations block background shortcuts; Escape closes them and returns
focus. These shortcuts are also shown beside their commands in Commands.

The **Export** button opens PDF, Word (.docx), Jupyter (.ipynb), and portable bundle
(.zip) choices. On compact windows it uses a document icon; Export is also in the
workspace menu. PDF and Word save current source and the latest recorded outputs
for each cell without running code. Edited cells with older outputs carry a warning.
Exports support Markdown headings, lists and tables, code, text, tracebacks, and
recorded PNG/JPEG plots. Inline Markdown styling is flattened to text. HTML-only,
SVG-only, and interactive outputs are identified as omitted; use a portable bundle
to preserve their original data. External Markdown images are not fetched.
Long cells paginate in PDF; Word content stays editable. Completion offers **Open
file** and **Show in folder**. Closing the save dialog cancels the export.

The Kit editor supplies selection, indentation, undo/redo and cell search.
Notebook source uses horizontal scrolling for long lines, keeping one visual row
per source line as panels and windows resize. In compact windows with the
Assistant open, cell controls move above the source so the editor uses the full
cell width.
In command mode, Undo cell structure and Redo cell structure restore stable cell
IDs, source, order, and attachments. In an editor, the same keys keep their text
undo meaning. Structural undo is local to the notebook and survives reopening;
it retains at most 20 operations and 16 MiB of patches, with a 4 MiB limit per
operation. Larger changes clear structural undo history. Undo refuses to overwrite
affected files changed since that operation; save or resolve edits first. A new
source change clears redo. Deletion and merge still ask for confirmation.
Shortcut rebinding remains unavailable.

In Commands, type to filter, use Up/Down or Home/End to select, Enter to activate,
and Escape to return focus. Unavailable runtime actions explain why. Sidebar and
record lists support arrow navigation and Enter. Confirmations start on Cancel,
trap Tab and Shift+Tab within the dialog, and Escape cancels and returns focus.

Closing detaches. Stop runtime is a separate named action. After stopping, source
and history remain available; the next run starts a runtime. See [Help](help.md)
for the glossary and [coverage](coverage.md) for qualification limits.
