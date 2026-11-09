use super::*;

pub(super) const COMMANDS: [(&str, &str, &str, Glyph); 56] = [
    (
        "undo-source",
        "Undo cell structure change",
        "Restore cell structure; source revisions are checked before applying",
        Glyph::RotateCcw,
    ),
    (
        "redo-source",
        "Redo cell structure change",
        "Reapply the last undone source transaction",
        Glyph::RefreshCw,
    ),
    (
        "debugger",
        "Debugger",
        "Return to the paused stack and stepping controls",
        Glyph::Code,
    ),
    (
        "load-source",
        "Load source file\u{2026}",
        "Preview a local Python file before inserting a new cell",
        Glyph::FileText,
    ),
    (
        "toggle-breakpoint",
        "Toggle breakpoint at caret",
        "Bind a line breakpoint to this saved source revision",
        Glyph::Flag,
    ),
    (
        "debug-cell",
        "Debug selected cell",
        "Run accepted source with local stepping enabled",
        Glyph::Code,
    ),
    (
        "variables",
        "Variables",
        "Inspect passive live namespace metadata",
        Glyph::Code,
    ),
    (
        "console",
        "Console",
        "Use the same notebook worker and execution queue",
        Glyph::Code,
    ),
    (
        "ai-explain",
        "Explain cell error",
        "Prepare recorded error context for review",
        Glyph::Sparkles,
    ),
    (
        "ai-edit",
        "Suggest cell edit",
        "Propose a revision-checked source edit",
        Glyph::Sparkles,
    ),
    (
        "ai-refactor",
        "Refactor cell",
        "Propose a cleaner cell refactoring with base source revision",
        Glyph::Sparkles,
    ),
    (
        "ai-document",
        "Document this cell",
        "Propose documentation for the selected cell",
        Glyph::Sparkles,
    ),
    (
        "ai-tests",
        "Generate tests for cell",
        "Propose a verification test cell",
        Glyph::Sparkles,
    ),
    (
        "ai-summary",
        "Resume-work summary",
        "Summarize notebook state with evidence",
        Glyph::Sparkles,
    ),
    (
        "ai-repro",
        "Reproducibility review",
        "Audit stale cells, execution order, and checkpoints",
        Glyph::Sparkles,
    ),
    (
        "ai-report",
        "Generate notebook report",
        "Draft a publication-ready narrative with recorded evidence",
        Glyph::Sparkles,
    ),
    (
        "ai-environment",
        "Explain environment",
        "Review interpreter and GPU reports",
        Glyph::Sparkles,
    ),
    (
        "assistant",
        "Ask assistant",
        "Review notebook context and ask a configured model",
        Glyph::Sparkles,
    ),
    (
        "notebook",
        "Notebook",
        "Return to notebook cells",
        Glyph::NotebookPen,
    ),
    (
        "history",
        "History",
        "Browse recorded executions",
        Glyph::Clock,
    ),
    (
        "find-cells",
        "Find notebook cells",
        "Search source in the notebook outline",
        Glyph::Search,
    ),
    (
        "export-menu",
        "Export notebook\u{2026}",
        "Choose PDF, Word, Jupyter, or a portable bundle",
        Glyph::FileText,
    ),
    (
        "export-pdf",
        "Export PDF\u{2026}",
        "Save source and recorded outputs as a paginated document",
        Glyph::FileText,
    ),
    (
        "export-docx",
        "Export Word document\u{2026}",
        "Save source and recorded outputs as an editable .docx document",
        Glyph::FileText,
    ),
    (
        "open",
        "Open notebook\u{2026}",
        "Choose a notebook directory",
        Glyph::FolderOpen,
    ),
    (
        "run-all",
        "Run all cells",
        "Execute every code cell in order",
        Glyph::Play,
    ),
    (
        "above",
        "Run above",
        "Run code cells above the selection",
        Glyph::ChevronsUp,
    ),
    (
        "below",
        "Run below",
        "Run code cells below the selection",
        Glyph::ChevronsDown,
    ),
    (
        "split-cell",
        "Split cell at cursor",
        "Split saved source at the editor cursor",
        Glyph::FileText,
    ),
    (
        "merge-cell",
        "Merge with next cell",
        "Combine adjacent cells of the same type",
        Glyph::FileText,
    ),
    (
        "checkpoint",
        "Create checkpoint",
        "Snapshot the live namespace",
        Glyph::BookmarkPlus,
    ),
    (
        "interrupt",
        "Interrupt",
        "Interrupt the running cell",
        Glyph::Square,
    ),
    (
        "checkpoints",
        "Checkpoints",
        "Inspect saved state",
        Glyph::Flag,
    ),
    (
        "outline",
        "Toggle notebook sidebar",
        "Show or hide cell navigation",
        Glyph::BookOpen,
    ),
    (
        "settings",
        "Settings",
        "Appearance and interpreter",
        Glyph::Settings,
    ),
    (
        "help",
        "Help and shortcuts",
        "Keyboard guide, glossary and About",
        Glyph::Info,
    ),
    (
        "runtimes",
        "All runtimes",
        "Manage notebook runtimes",
        Glyph::Cpu,
    ),
    (
        "diagnostics",
        "Diagnostics",
        "Recovery and technical details",
        Glyph::Stethoscope,
    ),
    (
        "export-ipynb",
        "Export Jupyter notebook\u{2026}",
        "Save current source as a Jupyter notebook",
        Glyph::FileText,
    ),
    (
        "export-bundle",
        "Export bundle\u{2026}",
        "Save source and recorded history in a portable bundle",
        Glyph::Database,
    ),
    (
        "inspect-bundle",
        "Inspect bundle\u{2026}",
        "Review a portable bundle without running it or replacing source",
        Glyph::Database,
    ),
    (
        "restart",
        "Restart empty",
        "Discard the live namespace",
        Glyph::RotateCcw,
    ),
    (
        "stop",
        "Stop runtime",
        "Stop computation and shut the worker down",
        Glyph::Power,
    ),
    (
        "interrupt_kill",
        "Terminate worker\u{2026}",
        "Force-stop an unresponsive worker; live variables are lost",
        Glyph::OctagonX,
    ),
    (
        "training-checkpoint",
        "Training checkpoint (Experimental)",
        "Save registered training state at an idle safe point",
        Glyph::BookmarkPlus,
    ),
    (
        "training-list",
        "Training snapshots",
        "Inspect saved training state without executing source",
        Glyph::Flag,
    ),
    (
        "native-status",
        "Native snapshot availability",
        "Inspect native process capture capability",
        Glyph::Cpu,
    ),
    (
        "gc",
        "Preview garbage collection",
        "Dry run; nothing is deleted",
        Glyph::Trash,
    ),
    (
        "run-selected",
        "Run selected cell",
        "Execute the selected code cell",
        Glyph::Play,
    ),
    (
        "add-code",
        "Add code cell",
        "Insert code after the selection",
        Glyph::Plus,
    ),
    (
        "add-markdown",
        "Add Markdown cell",
        "Insert prose after the selection",
        Glyph::Plus,
    ),
    (
        "add-raw",
        "Add Raw cell",
        "Insert plain source after the selection",
        Glyph::Plus,
    ),
    (
        "duplicate-cell",
        "Duplicate cell",
        "Copy the selected cell",
        Glyph::FileText,
    ),
    (
        "delete-cell",
        "Delete cell\u{2026}",
        "Confirm removal of the selected cell",
        Glyph::Trash,
    ),
    (
        "move-up",
        "Move cell up",
        "Move the selected cell one place earlier",
        Glyph::ChevronsUp,
    ),
    (
        "move-down",
        "Move cell down",
        "Move the selected cell one place later",
        Glyph::ChevronsDown,
    ),
];

/// Per-cell assistant actions, grouped behind the cell menu's Assistant entry.
pub(super) const CELL_ASSISTANT_ACTIONS: [(&str, &str); 5] = [
    ("assistant", "Ask about this cell"),
    ("ai-edit", "Suggest edit"),
    ("ai-refactor", "Refactor cell"),
    ("ai-document", "Document"),
    ("ai-tests", "Generate tests"),
];

impl Workspace {
    pub(super) fn key_down(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // Intercept before the editor's Enter/newline action. Never consume an IME commit.
        let composing = self
            .tabs
            .get(self.active)
            .and_then(|t| t.model.selected.as_ref().and_then(|id| t.editors.get(id)))
            .cloned()
            .is_some_and(|editor| {
                editor.update(cx, |s, cx| s.marked_text_range(window, cx).is_some())
            });
        if composing {
            if event.keystroke.modifiers.control
                || event.keystroke.modifiers.alt
                || event.keystroke.modifiers.shift
                || event.keystroke.modifiers.platform
                || event.keystroke.key == "f1"
            {
                cx.stop_propagation();
            }
            return;
        }
        let key = event.keystroke.key.as_str();
        let m = event.keystroke.modifiers;
        if self.app_menu
            || self.export_menu
            || self.runtime_menu
            || self.notebook_menu
            || self.cell_menu.is_some()
            || self.insert_menu.is_some()
        {
            if key == "escape" {
                self.close_menu(window, cx);
            } else if self.cell_menu.is_some() && matches!(key, "left" | "right") {
                self.cell_menu_assistant = key == "right";
                self.dialog_focus.focus(window, cx);
                cx.notify();
            } else if key == "tab" {
                if m.shift {
                    window.focus_prev(cx);
                } else {
                    window.focus_next(cx);
                }
                if !self.dialog_focus.contains_focused(window, cx) {
                    self.dialog_focus.focus(window, cx);
                }
            } else {
                if m.control || m.alt || m.platform || key == "f1" {
                    cx.stop_propagation();
                }
                return;
            }
            cx.stop_propagation();
            return;
        }
        if self.confirmation.is_some() {
            if key == "enter" && self.confirm_focus.is_focused(window) {
                self.command_mode(&CommandMode, window, cx);
                cx.stop_propagation();
                return;
            }
            if key == "escape" {
                self.command_mode(&CommandMode, window, cx);
            } else if key == "tab" {
                if m.shift {
                    window.focus_prev(cx);
                } else {
                    window.focus_next(cx);
                }
                if !self.dialog_focus.contains_focused(window, cx) {
                    self.confirm_focus.focus(window, cx);
                }
            } else {
                if m.control || m.alt || m.platform || key == "f1" {
                    cx.stop_propagation();
                }
                return;
            }
            cx.stop_propagation();
            return;
        }
        if self.palette {
            let matches = COMMANDS
                .iter()
                .filter(|(_, label, description, _)| {
                    label.to_lowercase().contains(&self.palette_filter)
                        || description.to_lowercase().contains(&self.palette_filter)
                })
                .collect::<Vec<_>>();
            match key {
                "down" => {
                    self.palette_active =
                        (self.palette_active + 1).min(matches.len().saturating_sub(1))
                }
                "up" => self.palette_active = self.palette_active.saturating_sub(1),
                "home" => self.palette_active = 0,
                "end" => self.palette_active = matches.len().saturating_sub(1),
                "enter" => {
                    if let Some((method, ..)) = matches.get(self.palette_active) {
                        self.run_command(method, window, cx);
                    }
                }
                "escape" => {
                    self.palette = false;
                    if let Some(focus) = self.return_focus.take() {
                        focus.focus(window, cx);
                    }
                }
                _ => return,
            }
            if matches!(key, "down" | "up" | "home" | "end") {
                // Reveal keyboard selections without undoing manual scrolling on redraw.
                self.palette_scroll
                    .scroll_to_item(self.palette_active, ScrollStrategy::Nearest);
            }
            cx.stop_propagation();
            cx.notify();
            return;
        }
        // Dismiss before the editor's Escape action propagates to CommandMode.
        // The popup may already be closed by the time that parent action runs.
        if key == "escape"
            && !m.control
            && !m.alt
            && !m.shift
            && !m.platform
            && self.surface == "Notebook"
            && self.tabs.get(self.active).is_some_and(|tab| {
                tab.model
                    .selected
                    .as_ref()
                    .and_then(|id| tab.editors.get(id))
                    .is_some_and(|editor| {
                        let editor = editor.read(cx);
                        editor.focus_handle(cx).is_focused(window)
                            && (editor.completion_menu_state().open
                                || editor.hover_popover().is_some())
                    })
            })
        {
            self.command_mode(&CommandMode, window, cx);
            cx.stop_propagation();
            return;
        }
        if self.project_shortcut(event, window, cx) {
            return;
        }
        if key == "f1" {
            self.surface = "Help".into();
            cx.stop_propagation();
            cx.notify();
            return;
        }
        if key == "left" && m.alt {
            self.back();
            cx.stop_propagation();
            cx.notify();
            return;
        }
        if self.surface != "Notebook" {
            if matches!(key, "up" | "down" | "home" | "end" | "enter")
                && self.focus.is_focused(window)
            {
                let items = self
                    .tabs
                    .get(self.active)
                    .and_then(|t| t.inspections.get(&self.surface))
                    .and_then(|v| v["items"].as_array())
                    .cloned()
                    .unwrap_or_default();
                let items = items
                    .into_iter()
                    .filter(|item| {
                        self.surface != "History"
                            || history_matches(
                                item,
                                &self.tabs[self.active].history_applied.to_lowercase(),
                                &self.tabs[self.active].model,
                            )
                    })
                    .collect::<Vec<_>>();
                match key {
                    "down" => {
                        self.record_active =
                            (self.record_active + 1).min(items.len().saturating_sub(1))
                    }
                    "up" => self.record_active = self.record_active.saturating_sub(1),
                    "home" => self.record_active = 0,
                    "end" => self.record_active = items.len().saturating_sub(1),
                    "enter" => {
                        if let Some(item) = items.get(self.record_active) {
                            if let Some(id) = item["exec_id"].as_str() {
                                self.inspect("execution.get", json!({"exec_id":id}));
                            } else if let Some(id) = item["checkpoint_id"].as_str() {
                                self.inspect("checkpoint.get", json!({"checkpoint_id":id}));
                            } else if let Some(id) = item["snapshot"].as_str() {
                                self.inspect("training.inspect", json!({"snapshot":id}));
                            } else if let Some(id) = item["session_id"].as_str() {
                                self.inspect("executions.list", json!({"session_id":id}));
                            } else if let Some(path) = item["project_path"].as_str() {
                                self.open(PathBuf::from(path), cx);
                                self.surface = "Notebook".into();
                            }
                        }
                    }
                    _ => {}
                }
                cx.stop_propagation();
                cx.notify();
            }
            return;
        }
        if self.sidebar_focus.contains_focused(window, cx) {
            let tab = &mut self.tabs[self.active];
            let query = self.query.read(cx).value().to_lowercase();
            let ids = tab
                .model
                .order
                .iter()
                .filter(|id| tab.model.cells[*id].text.to_lowercase().contains(&query))
                .cloned()
                .collect::<Vec<_>>();
            let mut index = ids
                .iter()
                .position(|id| Some(id) == tab.model.selected.as_ref())
                .unwrap_or(0);
            match key {
                "up" => index = index.saturating_sub(1),
                "down" => index = (index + 1).min(ids.len().saturating_sub(1)),
                "home" => index = 0,
                "end" => index = ids.len().saturating_sub(1),
                "enter" => {
                    tab.reveal = ids.get(index).cloned();
                    self.focus.focus(window, cx);
                }
                _ => return,
            }
            if let Some(id) = ids.get(index) {
                tab.model.selected = Some(id.clone());
            }
            cx.stop_propagation();
            cx.notify();
            return;
        }
        if key == "enter"
            && !m.control
            && !m.shift
            && !m.alt
            && let Some(tab) = self.tabs.get_mut(self.active)
            && tab.model.mode == Mode::Command
            && !tab
                .model
                .selected
                .as_ref()
                .and_then(|id| tab.editors.get(id))
                .is_some_and(|editor| editor.read(cx).focus_handle(cx).is_focused(window))
        {
            tab.reveal = tab.model.selected.clone();
            cx.stop_propagation();
            cx.notify();
            return;
        }
        if key == "enter" && (m.control || m.shift || m.alt) && !m.platform {
            if event.is_held {
                cx.stop_propagation();
                return;
            }
            if m.control && m.shift {
                self.batch("all");
                cx.stop_propagation();
                return;
            }
            self.run(&RunCell, window, cx);
            if m.alt {
                let index = self
                    .tabs
                    .get(self.active)
                    .and_then(|t| {
                        t.model
                            .selected
                            .as_ref()
                            .and_then(|id| t.model.order.iter().position(|c| c == id))
                    })
                    .unwrap_or(0)
                    + 1;
                self.mutate(json!({"action":"add","type":"code","index":index}));
            } else if m.shift
                && !m.control
                && let Some(tab) = self.tabs.get_mut(self.active)
                && let Some(index) = tab
                    .model
                    .selected
                    .as_ref()
                    .and_then(|id| tab.model.order.iter().position(|c| c == id))
                && let Some(next) = tab.model.order.get(index + 1)
            {
                tab.reveal = Some(next.clone());
            }
            cx.stop_propagation();
            cx.notify();
        }
    }

    pub(super) fn save(&mut self, _: &Save, _: &mut Window, _: &mut Context<Self>) {
        self.send(Command::Save);
    }

    pub(super) fn run(&mut self, _: &RunCell, _: &mut Window, _: &mut Context<Self>) {
        if self.surface != "Notebook" {
            return;
        }
        if let Some(id) = self
            .tabs
            .get(self.active)
            .and_then(|t| t.model.selected.clone())
        {
            if self.tabs[self.active].model.cells[&id].disk.kind != "code" {
                return;
            }
            self.send(Command::Run {
                cell_ids: vec![id],
                on_error: "stop".into(),
            });
        }
    }

    pub(super) fn all(&mut self, _: &RunAll, _: &mut Window, _: &mut Context<Self>) {
        if self.surface != "Notebook" {
            return;
        }
        self.batch("all");
    }

    pub(super) fn batch(&self, scope: &str) {
        if let Some(tab) = self.tabs.get(self.active) {
            let selected = tab
                .model
                .selected
                .as_ref()
                .and_then(|id| tab.model.order.iter().position(|c| c == id))
                .unwrap_or(0);
            let ids = tab
                .model
                .order
                .iter()
                .enumerate()
                .filter(|(i, id)| {
                    tab.model.cells[*id].disk.kind == "code"
                        && match scope {
                            "above" => *i < selected,
                            "below" => *i > selected,
                            _ => true,
                        }
                })
                .map(|(_, id)| id.clone())
                .collect();
            self.send(Command::Run {
                cell_ids: ids,
                on_error: "stop".into(),
            });
        }
    }

    pub(super) fn palette(&mut self, _: &Palette, window: &mut Window, cx: &mut Context<Self>) {
        self.return_focus = window.focused(cx);
        self.palette_active = 0;
        self.palette = !self.palette;
        if self.palette {
            self.palette_scroll.scroll_to_item(0, ScrollStrategy::Top);
            self.palette_query.update(cx, |s, cx| {
                s.set_value("", window, cx);
                s.focus(window, cx);
            });
        } else {
            self.focus.focus(window, cx);
        }
        cx.notify();
    }

    pub(super) fn command_mode(
        &mut self,
        _: &CommandMode,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.surface == "Console" && !self.palette && self.confirmation.is_none() {
            let editor = self.console_editor(window, cx);
            editor.update(cx, |editor, cx| {
                editor.dismiss_lsp_overlays(cx);
                editor.focus(window, cx);
            });
            return;
        }
        if let Some(editor) = self
            .tabs
            .get(self.active)
            .and_then(|tab| {
                tab.model
                    .selected
                    .as_ref()
                    .and_then(|id| tab.editors.get(id))
            })
            .cloned()
            && (editor.read(cx).completion_menu_state().open
                || editor.read(cx).hover_popover().is_some())
        {
            editor.update(cx, |editor, cx| editor.dismiss_lsp_overlays(cx));
            editor.update(cx, |editor, cx| editor.focus(window, cx));
            return;
        }
        self.palette = false;
        self.confirmation = None;
        if let Some(tab) = self.tabs.get_mut(self.active) {
            tab.model.mode = Mode::Command;
        }
        if let Some(focus) = self.return_focus.take() {
            focus.focus(window, cx);
        } else {
            self.focus.focus(window, cx);
        }
        cx.notify();
    }

    pub(super) fn mutate(&mut self, mut request: Value) {
        if matches!(
            request["action"].as_str(),
            Some("add" | "duplicate" | "split")
        ) {
            let id = uuid::Uuid::new_v4().simple().to_string();
            request["new_id"] = json!(id);
            if let Some(tab) = self.tabs.get_mut(self.active) {
                tab.reveal = Some(id);
            }
        }
        if request["action"] == "move"
            && let Some(tab) = self.tabs.get_mut(self.active)
        {
            tab.reveal = request["cell_id"].as_str().map(str::to_string);
        }
        if let Some(tab) = self.tabs.get(self.active) {
            request["manifest_revision"] = json!(tab.model.manifest_revision);
            request["cell_revisions"] = json!(
                tab.model
                    .cells
                    .iter()
                    .map(|(id, c)| (id.clone(), c.disk.revision.clone()))
                    .collect::<BTreeMap<_, _>>()
            );
            self.send(Command::Mutate(request));
        }
    }

    pub(super) fn select_surface(&mut self, surface: &str, cx: &mut Context<Self>) {
        // Drop destination-scoped chrome so hover/tooltips and menus cannot
        // linger over an unrelated page (live UX E / apple-design live finding).
        self.app_menu = false;
        self.export_menu = false;
        self.runtime_menu = false;
        self.notebook_menu = false;
        self.cell_menu = None;
        self.insert_menu = None;
        self.palette = false;
        self.menu_anchor.clear();
        self.menu_bounds.clear();
        if let Some(tab) = self.tabs.get_mut(self.active) {
            tab.selections
                .insert(self.surface.clone(), self.record_active);
            self.record_active = tab.selections.get(surface).copied().unwrap_or(0);
        }
        self.surface = surface.into();
        if let Some(tab) = self.tabs.get_mut(self.active) {
            tab.checkpoint_notice = None;
        }
        if surface == "History"
            && let Some((method, fields)) = self.history_refresh_request()
        {
            self.inspect(&method, fields);
        }
        if self
            .tabs
            .get(self.active)
            .is_some_and(|t| t.inspections.contains_key(surface))
        {
            cx.notify();
            return;
        }
        match surface {
            "History" => self.inspect("executions.list", json!({})),
            "Checkpoints" => self.inspect("checkpoints.list", json!({})),
            "Diagnostics" => self.inspect("doctor", json!({})),
            "Runtimes" => self.inspect("runtimes.list", json!({})),
            _ => {}
        }
        cx.notify();
    }

    pub(super) fn run_command(
        &mut self,
        method: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.command_reason(method).is_some() {
            return;
        }
        if matches!(
            method,
            "stop" | "interrupt" | "restart" | "checkpoint" | "training-checkpoint"
        ) && !self
            .tabs
            .get(self.active)
            .is_some_and(|t| runtime_action_available(method, &t.runtime, &self.surface))
        {
            return;
        }
        if matches!(method, "run-all" | "above" | "below") && self.tabs.is_empty() {
            return;
        }
        match method {
            "undo-source" | "redo-source" => self
                .mutate(json!({"action": if method == "undo-source" { "undo" } else { "redo" }})),
            "ai-explain" | "ai-edit" | "ai-refactor" | "ai-document" | "ai-tests"
            | "ai-summary" | "ai-repro" | "ai-report" | "ai-environment" => {
                self.tabs[self.active].assistant.open = true;
                self.tabs[self.active].assistant.selected_cells = self.tabs[self.active]
                    .model
                    .selected
                    .clone()
                    .into_iter()
                    .collect();
                let (task, question) = match method {
                    "ai-explain" => ("explain", "Explain the recorded error and propose a fix."),
                    "ai-edit" => (
                        "edit",
                        "Suggest an improvement to this cell. Propose an edit with its base source hash.",
                    ),
                    "ai-refactor" => (
                        "refactor",
                        "Refactor this cell for readability and modularity. Propose an edit with its base source hash.",
                    ),
                    "ai-document" => ("document", "Propose documentation for this cell."),
                    "ai-tests" => (
                        "tests",
                        "Propose a new code cell with unit tests verifying the selected cell.",
                    ),
                    "ai-summary" => (
                        "summary",
                        "Summarize where I left off, citing recorded evidence.",
                    ),
                    "ai-repro" => (
                        "reproducibility",
                        "Review stale cells, execution order, and checkpoints for reproducibility.",
                    ),
                    "ai-report" => (
                        "report",
                        "Draft a narrative notebook report citing recorded cells, executions, and checkpoints.",
                    ),
                    _ => (
                        "environment",
                        "Explain recorded environment and GPU reports. Show installation commands only as copyable proposals.",
                    ),
                };
                self.ai_preview(task, question.into(), cx);
            }
            "assistant" => self.open_assistant(cx),
            "run-selected" => self.run(&RunCell, window, cx),
            "add-code" | "add-markdown" | "add-raw" => {
                let tab = &self.tabs[self.active];
                let index = tab
                    .model
                    .selected
                    .as_ref()
                    .and_then(|id| tab.model.order.iter().position(|c| c == id))
                    .map_or(tab.model.order.len(), |i| i + 1);
                self.mutate(json!({"action":"add","type":method.strip_prefix("add-").unwrap(),"index":index}));
            }
            "duplicate-cell" | "delete-cell" | "move-up" | "move-down" => {
                let tab = &self.tabs[self.active];
                let id = tab.model.selected.clone().unwrap();
                let index = tab.model.order.iter().position(|c| *c == id).unwrap();
                match method {
                    "duplicate-cell" => self.mutate(json!({"action":"duplicate","cell_id":id})),
                    "delete-cell" => {
                        self.confirmation = Some(("delete".into(), json!({"cell_id":id})))
                    }
                    _ => self.mutate(json!({
                        "action":"move",
                        "cell_id":id,
                        "index":if method=="move-up"{index-1}else{index+1}
                    })),
                }
            }
            "split-cell" => {
                let tab = &self.tabs[self.active];
                let id = tab.model.selected.clone().unwrap();
                if let Some(editor) = tab.editors.get(&id) {
                    let text = &tab.model.cells[&id].text;
                    let offset = text[..editor.read(cx).cursor().min(text.len())]
                        .chars()
                        .count();
                    self.mutate(json!({"action":"split","cell_id":id,"offset":offset}));
                }
            }
            "merge-cell" => {
                let tab = &self.tabs[self.active];
                let id = tab.model.selected.clone().unwrap();
                let index = tab.model.order.iter().position(|c| *c == id).unwrap();
                self.confirmation = Some((
                    "merge".into(),
                    json!({"cell_id":id,"other_id":tab.model.order[index + 1]}),
                ));
            }
            "export-menu" => {
                self.palette = false;
                self.toggle_export(window, cx);
                return;
            }
            "export-pdf" => self.export_dialog("pdf", window, cx),
            "export-docx" => self.export_dialog("docx", window, cx),
            "export-ipynb" => self.export_dialog("ipynb", window, cx),
            "export-bundle" => self.export_dialog("zip", window, cx),
            "inspect-bundle" => {
                self.palette = false;
                self.inspect_bundle_dialog(window, cx);
                return;
            }
            "stop" | "restart" | "interrupt_kill" => {
                self.confirmation = Some((method.into(), json!({})))
            }
            "run-all" => self.batch("all"),
            "above" | "below" => self.batch(method),
            "settings" => self.select_surface("Settings", cx),
            "notebook" => {
                self.select_surface("Notebook", cx);
                self.focus.focus(window, cx);
            }
            "history" => {
                self.select_surface("History", cx);
                self.focus.focus(window, cx);
            }
            "find-cells" => {
                self.palette = false;
                self.return_focus = None;
                self.select_surface("Notebook", cx);
                if self.compact {
                    self.outline_drawer = true;
                } else {
                    self.sidebar_visible = true;
                }
                self.query.update(cx, |query, cx| query.focus(window, cx));
                cx.notify();
                return;
            }
            "diagnostics" => self.select_surface("Diagnostics", cx),
            "variables" => self.select_surface("Variables", cx),
            "console" => self.select_surface("Console", cx),
            "debugger" => self.select_surface("Debugger", cx),
            "debug-cell" => {
                if let Some(cell_id) = self.tabs[self.active].model.selected.clone() {
                    self.send(Command::DebugRun { cell_id });
                    self.select_surface("Debugger", cx);
                }
            }
            "toggle-breakpoint" => self.toggle_breakpoint(window, cx),
            "load-source" => self.load_source_dialog(window, cx),
            "runtimes" => self.select_surface("Runtimes", cx),
            "checkpoints" => self.select_surface("Checkpoints", cx),
            "help" => self.select_surface("Help", cx),
            "outline" => {
                if self.compact {
                    self.outline_drawer = !self.outline_drawer;
                    self.palette = false;
                    if let Some(focus) = self.return_focus.take() {
                        focus.focus(window, cx);
                    }
                    cx.notify();
                    return;
                }
                self.sidebar_visible = !self.sidebar_visible;
                self.preferences.outline_visible = self.sidebar_visible;
                self.save_preferences(cx);
            }
            "open" => self.open_dialog(&OpenNotebook, window, cx),
            "gc" => self.runtime(method, json!({"dry_run":true})),
            "training-list" => {
                self.surface = "Checkpoints".into();
                self.inspect("training.list", json!({}));
            }
            "native-status" => {
                self.surface = "Diagnostics".into();
                self.inspect("native.status", json!({}));
            }
            _ => self.runtime(method, json!({})),
        }
        self.palette = false;
        if let Some(focus) = self.return_focus.take() {
            focus.focus(window, cx);
        }
        cx.notify();
    }

    pub(super) fn command_reason(&self, method: &str) -> Option<&'static str> {
        if matches!(method, "undo-source" | "redo-source") {
            let Some(tab) = self.tabs.get(self.active) else {
                return Some("Open a notebook first");
            };
            if self.surface != "Notebook"
                || tab
                    .model
                    .cells
                    .values()
                    .any(|cell| cell.save != SaveState::Saved)
            {
                return Some("Return to Notebook and save source before changing cell structure");
            }
            let key = if method == "undo-source" {
                "undo"
            } else {
                "redo"
            };
            if tab
                .inspections
                .get("Source history")
                .is_none_or(|history| history[key] != true)
            {
                return Some("No cell structure change is available");
            }
        }
        if method == "toggle-breakpoint"
            && self
                .tabs
                .get(self.active)
                .is_some_and(|tab| tab.runtime["worker_state"] != "idle")
        {
            return Some("Set breakpoints in a saved cell while the worker is idle");
        }
        if matches!(method, "debug-cell" | "toggle-breakpoint")
            && !self.tabs.get(self.active).is_some_and(|tab| {
                tab.runtime["interactive"]["features"]
                    .as_array()
                    .is_some_and(|features| features.iter().any(|feature| feature == "debugger"))
            })
        {
            return Some(
                "Experimental debugging is disabled for this runtime. See Help for technical setup.",
            );
        }
        if matches!(method, "open" | "help") {
            return None;
        }
        let Some(tab) = self.tabs.get(self.active) else {
            return Some("Open a notebook to use this action");
        };
        if method.starts_with("ai-") {
            if tab.assistant.settings["policy"].as_str() == Some("off") {
                return Some("Notebook AI policy is Off (change in Settings \u{2192} Assistant)");
            }
            if tab.assistant.settings["settings"]["enabled"].as_bool() == Some(false) {
                return Some("Enable the Assistant in Settings \u{2192} Assistant");
            }
            if matches!(
                method,
                "ai-explain" | "ai-edit" | "ai-refactor" | "ai-document" | "ai-tests"
            ) && tab.model.selected.is_none()
            {
                return Some("Select a cell first");
            }
        }
        if method == "inspect-bundle"
            && tab
                .inspections
                .get("Bundle review")
                .is_some_and(|review| review["pending"] == true)
        {
            return Some("A bundle inspection is already in progress");
        }
        if matches!(
            method,
            "export-menu" | "export-pdf" | "export-docx" | "export-ipynb" | "export-bundle"
        ) && tab
            .model
            .cells
            .values()
            .any(|cell| matches!(cell.save, SaveState::Conflict | SaveState::Failed))
        {
            return Some("Resolve source conflicts before exporting");
        }
        if method == "outline" && self.surface != "Notebook" {
            return Some("Outline is available in Notebook");
        }
        if method == "run-all" {
            if !tab.model.cells.values().any(|c| c.disk.kind == "code") {
                return Some("Add a code cell first");
            }
            if tab.runtime["connection_lost"] == true {
                return Some("Reconnect before running code");
            }
            if tab.model.cells.values().any(|c| {
                c.disk.kind == "code" && matches!(c.save, SaveState::Conflict | SaveState::Failed)
            }) {
                return Some("Resolve source conflicts before running");
            }
        }
        if method == "run-selected" {
            if self.surface != "Notebook" {
                return Some("Select a code cell in Notebook");
            }
            let Some(cell) = tab
                .model
                .selected
                .as_ref()
                .and_then(|id| tab.model.cells.get(id))
            else {
                return Some("Select a code cell first");
            };
            if cell.disk.kind != "code" {
                return Some("Only code cells can run");
            }
            if matches!(cell.save, SaveState::Conflict | SaveState::Failed) {
                return Some("Resolve the source conflict before running");
            }
        }
        if matches!(
            method,
            "add-code"
                | "add-markdown"
                | "add-raw"
                | "duplicate-cell"
                | "delete-cell"
                | "move-up"
                | "move-down"
        ) {
            if self.surface != "Notebook" {
                return Some("Select a cell in Notebook");
            }
            if tab.model.cells.values().any(|c| c.save != SaveState::Saved) {
                return Some("Save source and resolve conflicts before changing cell structure");
            }
            if !method.starts_with("add-") {
                let Some(index) = tab
                    .model
                    .selected
                    .as_ref()
                    .and_then(|id| tab.model.order.iter().position(|c| c == id))
                else {
                    return Some("Select a cell first");
                };
                if method == "move-up" && index == 0 {
                    return Some("This cell is already first");
                }
                if method == "move-down" && index + 1 == tab.model.order.len() {
                    return Some("This cell is already last");
                }
            }
        }
        if matches!(method, "split-cell" | "merge-cell") {
            if self.surface != "Notebook" {
                return Some("Select a cell in Notebook");
            }
            let Some(id) = &tab.model.selected else {
                return Some("Select a cell first");
            };
            if tab.model.cells.values().any(|c| c.save != SaveState::Saved) {
                return Some(
                    "Save all source and resolve conflicts before changing cell structure",
                );
            }
            if method == "split-cell" && !tab.editors.contains_key(id) {
                return Some("Open the cell editor and place the cursor first");
            }
            if method == "merge-cell" {
                let next = tab
                    .model
                    .order
                    .iter()
                    .position(|c| c == id)
                    .and_then(|i| tab.model.order.get(i + 1));
                let Some(next) = next else {
                    return Some("There is no following cell");
                };
                if tab.model.cells[id].disk.kind != tab.model.cells[next].disk.kind {
                    return Some("Merge requires adjacent cells of the same type");
                }
            }
        }
        action_reason(method, &tab.runtime)
    }
}
