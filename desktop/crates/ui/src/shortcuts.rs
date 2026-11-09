use super::*;

#[derive(Clone, PartialEq, ::gpui_kit::Action)]
#[action(namespace = kernelledger, no_json)]
pub(super) struct WorkbenchCommand {
    pub method: String,
}

pub(super) fn bind_commands(cx: &mut App) {
    for (method, _, keys) in SHORTCUTS {
        for keys in [platform_binding(keys), *keys] {
            cx.bind_keys([KeyBinding::new(
                keys,
                WorkbenchCommand {
                    method: (*method).into(),
                },
                Some("KernelLedger"),
            )]);
        }
    }
}

/// Application commands follow the host platform; notebook execution keeps
/// the established Control/Shift+Enter combinations on every platform.
pub(super) fn platform_binding(keys: &'static str) -> &'static str {
    if !(cfg!(target_os = "macos") && keys.starts_with("ctrl-") && !keys.ends_with("enter")) {
        return keys;
    }
    match keys {
        "ctrl-z" => "cmd-z",
        "ctrl-shift-z" => "cmd-shift-z",
        "ctrl-1" => "cmd-1",
        "ctrl-2" => "cmd-2",
        "ctrl-3" => "cmd-3",
        "ctrl-," => "cmd-,",
        "ctrl-b" => "cmd-b",
        "ctrl-s" => "cmd-s",
        "ctrl-o" => "cmd-o",
        "ctrl-shift-f" => "cmd-shift-f",
        "ctrl-shift-p" => "cmd-shift-p",
        "ctrl-shift-n" => "cmd-shift-n",
        "ctrl-shift-m" => "cmd-shift-m",
        "ctrl-shift-d" => "cmd-shift-d",
        "ctrl-shift-delete" => "cmd-shift-delete",
        "ctrl-alt-up" => "cmd-alt-up",
        "ctrl-alt-down" => "cmd-alt-down",
        "ctrl-shift-s" => "cmd-shift-s",
        "ctrl-shift-i" => "cmd-shift-i",
        "ctrl-shift-e" => "cmd-shift-e",
        _ => keys,
    }
}

pub(super) fn shortcut_text(text: &str) -> String {
    if !cfg!(target_os = "macos") {
        return text.to_owned();
    }
    let mut parts = text.split("Ctrl+");
    let mut result = parts.next().unwrap_or_default().to_owned();
    for part in parts {
        result.push_str(
            if part.starts_with("Enter") || part.starts_with("Shift+Enter") {
                "Ctrl+"
            } else {
                "Cmd+"
            },
        );
        result.push_str(part);
    }
    result
}

pub(super) const SHORTCUTS: &[(&str, &str, &str)] = &[
    ("undo-source", "Undo cell structure", "ctrl-z"),
    ("redo-source", "Redo cell structure", "ctrl-shift-z"),
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
            // Keep the existing Control aliases for established workflows.
            [platform_binding(keys), *keys].into_iter().any(|keys| {
                Keystroke::parse(keys).is_ok_and(|binding| {
                    binding.key.eq_ignore_ascii_case(&event.keystroke.key)
                        && binding.modifiers == event.keystroke.modifiers
                })
            })
        }) else {
            return false;
        };
        if matches!(*method, "undo-source" | "redo-source") && !self.focus.is_focused(window) {
            return false;
        }
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
