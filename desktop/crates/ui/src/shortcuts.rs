use super::*;

pub(super) const SHORTCUTS: &[(&str, &str, &str)] = &[
    ("notebook", "Notebook", "ctrl-1"),
    ("history", "History", "ctrl-2"),
    ("checkpoints", "Checkpoints", "ctrl-3"),
    ("settings", "Settings", "ctrl-,"),
    ("outline", "Toggle outline", "ctrl-b"),
    ("find-cells", "Find notebook cells", "ctrl-shift-f"),
    ("add-code", "Insert code cell below", "ctrl-shift-n"),
    ("add-markdown", "Insert Markdown cell below", "ctrl-shift-m"),
    ("duplicate-cell", "Duplicate selected cell", "ctrl-shift-d"),
    (
        "delete-cell",
        "Delete cell (confirmation)",
        "ctrl-shift-delete",
    ),
    ("move-up", "Move cell up", "ctrl-alt-up"),
    ("move-down", "Move cell down", "ctrl-alt-down"),
    ("checkpoint", "Create checkpoint", "ctrl-shift-s"),
    ("interrupt", "Interrupt execution", "ctrl-shift-i"),
    ("export-menu", "Export notebook", "ctrl-shift-e"),
];

pub(super) fn command_shortcut(method: &str) -> Option<&'static str> {
    SHORTCUTS
        .iter()
        .find(|(command, ..)| *command == method)
        .map(|(.., keys)| *keys)
        .or(match method {
            "open" => Some("ctrl-o"),
            "run-all" => Some("ctrl-shift-enter"),
            "run-selected" => Some("ctrl-enter"),
            "help" => Some("f1"),
            _ => None,
        })
}

impl Workspace {
    pub(super) fn project_shortcut(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some((method, ..)) = SHORTCUTS.iter().find(|(.., keys)| {
            Keystroke::parse(keys).is_ok_and(|binding| {
                binding.key == event.keystroke.key && binding.modifiers == event.keystroke.modifiers
            })
        }) else {
            return false;
        };
        if !event.is_held {
            self.run_command(method, window, cx);
        }
        cx.stop_propagation();
        true
    }

    pub(super) fn toggle_export(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let open = !self.export_menu;
        let origin = self.return_focus.clone().or_else(|| window.focused(cx));
        self.close_menu(window, cx);
        self.return_focus = origin;
        self.menu_anchor = "export".into();
        self.export_menu = open;
        self.dialog_focus.focus(window, cx);
        cx.notify();
    }
}
