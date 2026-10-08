# Keyboard guide

Run shortcuts work in edit and command mode. Ctrl+Enter runs and keeps selection;
Shift+Enter runs and selects the next cell; Alt+Enter runs and inserts a cell below.
Plain Enter inserts a newline while editing. During IME composition, application
run shortcuts are suppressed. Escape leaves editing without changing source.

Ctrl+O opens a directory, Ctrl+S saves, Ctrl+Shift+Enter runs all, Ctrl+Shift+P
opens Commands, and F1 opens Help. Alt+Left returns from recorded details.
The Kit editor supplies selection, indentation, undo/redo and cell search.
Structural undo and shortcut rebinding are unavailable in this development build;
deleting a cell asks for confirmation before changing source.

In Commands, type to filter, use Up/Down or Home/End to select, Enter to activate,
and Escape to return focus. Unavailable runtime actions explain why. Sidebar and
record lists support arrow navigation and Enter. Confirmations start on Cancel,
trap Tab and Shift+Tab within the dialog, and Escape cancels and returns focus.

Closing detaches. Stop runtime is a separate named action. After stopping, source
and history remain available; the next run starts a runtime. See [Help](help.md)
for the glossary and [coverage](coverage.md) for qualification limits.
