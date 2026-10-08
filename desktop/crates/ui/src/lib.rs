mod outputs;
mod interchange;
mod accessibility;
mod checkpoints;
mod components;
mod history;
mod help;
mod notebook;
mod settings;
mod shell;
mod surfaces;
use components::*;
mod theme;
use theme::*;
// Views hold presentation and actions; services own I/O and persistence.
use gpui_kit::assets::IconName as Glyph;
use gpui_kit::component::{
    ActiveTheme, Disableable, Icon, Selectable, Sizable, Theme, ThemeMode, ThemeTokens,
    button::{Button, ButtonVariants},
    checkbox::Checkbox,
    h_flex,
    input::{Editor, EditorState, Input, InputEvent, InputState},
    kbd::Kbd,
    resizable::{h_resizable, resizable_panel},
    text::TextView,
    v_flex,
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use kernelledger_client::{
    output::{bounded, safe_markdown},
    service::{Command, Event, NotebookService},
};
use kernelledger_models::{Mode, NotebookModel, SaveState, SourceSnapshot};
use serde_json::{Value, json};
use std::{collections::BTreeMap, path::PathBuf, time::Duration};

actions!(
    kernelledger,
    [Save, RunCell, RunAll, Palette, OpenNotebook, CommandMode]
);
const SURFACES: [(&str, Glyph, &str); 7] = [
    ("Notebook", Glyph::NotebookPen, ""),
    (
        "Help",
        Glyph::Info,
        "Shortcuts, terminology, and build information",
    ),
    (
        "History",
        Glyph::Clock,
        "Sessions and executions recorded in the ledger",
    ),
    (
        "Checkpoints",
        Glyph::Flag,
        "Saved namespaces you can restore or continue from",
    ),
    (
        "Runtimes",
        Glyph::Cpu,
        "Workers and interpreters known to this project",
    ),
    (
        "Diagnostics",
        Glyph::Stethoscope,
        "Health checks, operations in flight, and recovery tools",
    ),
    (
        "Settings",
        Glyph::Settings,
        "Appearance and interpreter preferences",
    ),
];
const COMMANDS: [(&str, &str, &str, Glyph); 20] = [
    ("export-ipynb", "Export notebook…", "Save current source as a Jupyter notebook", Glyph::FileText),
    ("export-bundle", "Export bundle…", "Save source and recorded history in a portable bundle", Glyph::Database),
    (
        "open",
        "Open notebook…",
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
        "checkpoint",
        "Checkpoint",
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
];

struct Tab {
    model: NotebookModel,
    service: NotebookService,
    list: ListState,
    editors: BTreeMap<String, Entity<EditorState>>,
    subscriptions: Vec<Subscription>,
    runtime: Value,
    operations: BTreeMap<String, Value>,
    inspections: BTreeMap<String, Value>,
    inspection_method: BTreeMap<String, String>,
    scrolls: BTreeMap<String, ScrollHandle>,
    parents: Vec<(String, String, Value, ScrollHandle, usize)>,
    raw_details: bool,
    reveal: Option<String>,
    uninitialized: bool,
    selections: BTreeMap<String, usize>,
    drafts: Vec<Value>,
    history_filter: String,
    checkpoint_notice: Option<String>,
    error: Option<String>,
}
pub struct Workspace {
    tabs: Vec<Tab>,
    active: usize,
    focus: FocusHandle,
    surface: String,
    palette: bool,
    query: Entity<InputState>,
    palette_query: Entity<InputState>,
    prompts: BTreeMap<String, Entity<InputState>>,
    tick: Task<()>,
    python: PathBuf,
    dark: bool,
    high_contrast: bool,
    confirmation: Option<(String, Value)>,
    palette_active: usize,
    palette_filter: String,
    palette_scroll: ScrollHandle,
    return_focus: Option<FocusHandle>,
    sidebar_visible: bool,
    app_menu: bool,
    runtime_menu: bool,
    preferences: kernelledger_client::preferences::Preferences,
    preference_status: Option<String>,
    preference_saving: bool,
    compact: bool,
    outline_drawer: bool,
    follow_system: bool,
    history_query: Entity<InputState>,
    help_query: Entity<InputState>,
    record_active: usize,
    sidebar_focus: FocusHandle,
    confirm_focus: FocusHandle,
    dialog_focus: FocusHandle,
    dialog_open: bool,
    keys: Option<Subscription>,
    recent: Vec<PathBuf>,
}
impl Workspace {
    pub fn new(
        project: Option<PathBuf>,
        appearance: &str,
        recent: Vec<PathBuf>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        cx.bind_keys([
            KeyBinding::new("ctrl-s", Save, Some("KernelLedger")),
            KeyBinding::new("ctrl-enter", RunCell, Some("KernelLedger")),
            KeyBinding::new("shift-enter", RunCell, Some("KernelLedger")),
            KeyBinding::new("ctrl-shift-enter", RunAll, Some("KernelLedger")),
            KeyBinding::new("ctrl-shift-p", Palette, Some("KernelLedger")),
            KeyBinding::new("ctrl-o", OpenNotebook, Some("KernelLedger")),
            KeyBinding::new("escape", CommandMode, Some("KernelLedger")),
        ]);
        let follow_system = appearance == "system";
        let dark = if follow_system {
            matches!(
                window.appearance(),
                WindowAppearance::Dark | WindowAppearance::VibrantDark
            )
        } else {
            appearance == "dark"
        };
        let high_contrast = appearance == "contrast" || (follow_system && system_high_contrast());
        apply_theme(dark, high_contrast, window, cx);
        let loaded_preferences = kernelledger_client::preferences::Preferences::load(
            &kernelledger_client::profile_path(),
        );
        let preference_status = loaded_preferences
            .as_ref()
            .err()
            .map(|e| format!("Preferences could not be loaded: {e}"));
        let preferences = loaded_preferences.unwrap_or_default();
        let mut view = Self {
            tabs: vec![],
            active: 0,
            focus: cx.focus_handle(),
            surface: "Notebook".into(),
            palette: false,
            query: cx
                .new(|cx| InputState::new(window, cx).placeholder("Search notebook cells only")),
            palette_query: cx.new(|cx| InputState::new(window, cx).placeholder("Type a command")),
            prompts: BTreeMap::new(),
            tick: Task::ready(()),
            python: std::env::var_os("KL_RUNTIME_PYTHON")
                .map(PathBuf::from)
                .unwrap_or_else(|| "python".into()),
            dark,
            high_contrast,
            confirmation: None,
            palette_active: 0,
            palette_filter: String::new(),
            palette_scroll: ScrollHandle::default(),
            return_focus: None,
            sidebar_visible: preferences.outline_visible,
            app_menu: false,
            runtime_menu: false,
            preferences,
            preference_status,
            preference_saving: false,
            compact: false,
            outline_drawer: false,
            follow_system,
            history_query: cx.new(|cx| {
                InputState::new(window, cx).placeholder(
                    "Search history | cell:... outcome:error session:... from:YYYY-MM-DD to:YYYY-MM-DD",
                )
            }),
            help_query: cx.new(|cx| InputState::new(window, cx).placeholder("Search Help")),
            record_active: 0,
            sidebar_focus: cx.focus_handle(),
            confirm_focus: cx.focus_handle(),
            dialog_focus: cx.focus_handle(),
            dialog_open: false,
            keys: None,
            recent,
        };
        let weak = cx.entity().downgrade();
        let handle = window.window_handle();
        view.keys = Some(cx.intercept_keystrokes(move |event, window, cx| {
            if window.window_handle() == handle {
                let _ = weak.update(cx, |view, cx| {
                    view.key_down(
                        &KeyDownEvent {
                            keystroke: event.keystroke.clone(),
                            is_held: false,
                            prefer_character_input: false,
                        },
                        window,
                        cx,
                    )
                });
            }
        }));
        if let Some(project) = project {
            view.open(project, cx);
        }
        view.tick = cx.spawn_in(window, async move |view, window| {
            loop {
                smol::Timer::after(Duration::from_millis(50)).await;
                if view
                    .update_in(window, |view, window, cx| view.drain(window, cx))
                    .is_err()
                {
                    break;
                }
            }
        });
        view
    }
    fn open(&mut self, project: PathBuf, cx: &mut Context<Self>) {
        if let Some(i) = self.tabs.iter().position(|t| t.model.project == project) {
            self.active = i;
            return;
        }
        self.tabs.push(Tab {
            service: NotebookService::start(project.clone(), self.python.clone()),
            model: NotebookModel {
                project,
                ..Default::default()
            },
            list: ListState::new(0, ListAlignment::Top, px(200.)),
            editors: BTreeMap::new(),
            subscriptions: vec![],
            runtime: Value::Null,
            operations: BTreeMap::new(),
            inspections: BTreeMap::new(),
            inspection_method: BTreeMap::new(),
            scrolls: BTreeMap::new(),
            parents: vec![],
            raw_details: false,
            reveal: None,
            uninitialized: false,
            selections: BTreeMap::new(),
            drafts: vec![],
            history_filter: String::new(),
            checkpoint_notice: None,
            error: None,
        });
        self.active = self.tabs.len() - 1;
        cx.notify();
    }
    fn send(&self, command: Command) {
        if let Some(tab) = self.tabs.get(self.active) {
            let _ = tab.service.commands.send(command);
        }
    }
    fn runtime(&self, method: &str, fields: Value) {
        self.send(Command::Runtime {
            method: method.into(),
            fields,
        });
    }
    fn inspect(&self, method: &str, fields: Value) {
        self.send(Command::Inspect {
            method: method.into(),
            fields,
        });
    }
    fn open_dialog(&mut self, _: &OpenNotebook, window: &mut Window, cx: &mut Context<Self>) {
        let paths = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some("Open notebook directory".into()),
        });
        cx.spawn_in(window, async move |view, window| {
            if let Ok(Ok(Some(paths))) = paths.await
                && let Some(path) = paths.into_iter().next()
            {
                let _ = view.update_in(window, |view, _, cx| view.open(path, cx));
            }
        })
        .detach();
    }
    fn create_dialog(&mut self, sample: bool, window: &mut Window, cx: &mut Context<Self>) {
        let paths = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some("Choose a directory to initialize as a notebook".into()),
        });
        cx.spawn_in(window, async move |view, window| {
            if let Ok(Ok(Some(paths))) = paths.await
                && let Some(path) = paths.into_iter().next()
            {
                let _ = view.update_in(window, |view, _, cx| {
                    view.open(path, cx);
                    view.send(Command::Initialize { sample });
                });
            }
        })
        .detach();
    }
    fn drain(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.follow_system {
            let dark = matches!(
                window.appearance(),
                WindowAppearance::Dark | WindowAppearance::VibrantDark
            );
            let high_contrast = system_high_contrast();
            if self.dark != dark || self.high_contrast != high_contrast {
                self.dark = dark;
                self.high_contrast = high_contrast;
                apply_theme(self.dark, self.high_contrast, window, cx);
            }
        }
        if self.confirmation.is_some() && !self.dialog_open {
            self.return_focus = window.focused(cx);
            self.confirm_focus.focus(window, cx);
            self.dialog_open = true;
        } else if self.confirmation.is_none() && self.dialog_open {
            self.dialog_open = false;
            if let Some(focus) = self.return_focus.take() {
                focus.focus(window, cx);
            }
        }
        let mut changed = false;
        for tab in &mut self.tabs {
            while let Ok(event) = tab.service.events.try_recv() {
                changed = true;
                match event {
                    Event::Source(snapshot) => {
                        if let Ok(snapshot) = serde_json::from_value::<SourceSnapshot>(snapshot) {
                            tab.uninitialized = snapshot.state == "missing"
                                && !snapshot.details.contains_key("manifest");
                            let order = tab.model.order.clone();
                            tab.model.apply_snapshot(snapshot);
                            if tab.model.source_state == "ok"
                                && !self.recent.contains(&tab.model.project)
                            {
                                self.recent.insert(0, tab.model.project.clone());
                                self.recent.truncate(10);
                                let recent = self.recent.clone();
                                cx.background_executor()
                                    .spawn(async move {
                                        let path =
                                            kernelledger_client::profile_path().join("recent.json");
                                        if let Some(parent) = path.parent() {
                                            let _ = std::fs::create_dir_all(parent);
                                        }
                                        let _ = std::fs::write(
                                            path,
                                            serde_json::to_vec(&recent).unwrap_or_default(),
                                        );
                                    })
                                    .detach();
                            }
                            if order != tab.model.order {
                                tab.list.reset(tab.model.order.len());
                            }
                            for (id, editor) in &tab.editors {
                                if let Some(cell) = tab.model.cells.get(id)
                                    && cell.save == SaveState::Saved
                                    && editor.read(cx).value().as_ref() != cell.text
                                {
                                    editor.update(cx, |s, cx| {
                                        s.set_value(cell.text.clone(), window, cx)
                                    });
                                }
                            }
                        }
                    }
                    Event::Saved {
                        cell_id,
                        submitted,
                        snapshot,
                    } => {
                        if let Ok(snapshot) = serde_json::from_value::<SourceSnapshot>(snapshot) {
                            tab.model.manifest_revision = snapshot.manifest_revision;
                            if let Some(source) =
                                snapshot.cells.into_iter().find(|c| c.id == cell_id)
                                && let Some(cell) = tab.model.cells.get_mut(&cell_id)
                            {
                                cell.acknowledge(source, &submitted);
                            }
                        }
                    }
                    Event::Runtime(value) => {
                        if tab
                            .inspections
                            .get("Technical error")
                            .is_some_and(|v| v["code"] == "transport")
                        {
                            tab.error = None;
                            tab.inspections.remove("Technical error");
                        }
                        tab.runtime = value;
                    }
                    Event::Operation(value) => {
                        if value["state"] == "completed" {
                            if let Some(id) = value["result"]["checkpoint_id"].as_str() {
                                tab.checkpoint_notice = Some(id.to_owned());
                                let _ = tab.service.commands.send(Command::Inspect { method: "checkpoints.list".into(), fields: json!({}) });
                            }
                        }
                        tab.operations.insert(
                            value["operation_id"].as_str().unwrap_or("pending").into(),
                            value,
                        );
                    }
                    Event::Output(value) => {
                        if let Some(id) = value["cell_id"].as_str()
                            && let Some(cell) = tab.model.cells.get_mut(id)
                        {
                            cell.output = value["outputs"].as_array().cloned().unwrap_or_default();
                            cell.exec_id = value["exec_id"].as_str().map(str::to_owned);
                            cell.executed_revision =
                                value["source_sha256"].as_str().map(str::to_owned);
                            cell.ordinal = value["ordinal"].as_u64();
                            cell.outcome = value["status"].as_str().unwrap_or("unknown").into();
                            if let Some(i) = tab.model.order.iter().position(|cid| cid == id) {
                                tab.list.splice(i..i + 1, 1);
                            }
                        }
                    }
                    Event::Inspection { method, value } => {
                        let surface = inspection_surface(&method).to_string();
                        if matches!(
                            method.as_str(),
                            "executions.list"
                                | "execution.get"
                                | "checkpoint.get"
                                | "training.inspect"
                        ) && let Some(previous) = tab.inspections.get(&surface)
                        {
                            let scroll = tab.scrolls.get(&surface).cloned().unwrap_or_default();
                            tab.parents.push((
                                surface.clone(),
                                tab.inspection_method
                                    .get(&surface)
                                    .cloned()
                                    .unwrap_or_default(),
                                previous.clone(),
                                scroll,
                                self.record_active,
                            ));
                            tab.scrolls.insert(surface.clone(), ScrollHandle::new());
                        }
                        tab.inspection_method.insert(surface.clone(), method);
                        tab.inspections.insert(surface, value);
                        tab.raw_details = false;
                    }
                    Event::Draft(value) => tab.drafts.push(value),
                    Event::Error { code, message } => {
                        if code == "transport" {
                            tab.runtime["connection_lost"] = json!(true);
                        }
                        tab.inspections.insert(
                            "Technical error".into(),
                            json!({"code":code,"message":message}),
                        );
                        tab.error = Some(error_message(&code).into());
                    }
                }
            }
        }
        if let Some(tab) = self.tabs.get_mut(self.active)
            && let Some(id) = tab.reveal.clone()
            && let Some(index) = tab.model.order.iter().position(|c| c == &id)
        {
            tab.model.selected = Some(id.clone());
            tab.model.mode = Mode::Edit;
            tab.list.scroll_to_reveal_item(index);
            tab.reveal = None;
            let editor = self.editor(self.active, &id, window, cx);
            editor.update(cx, |s, cx| s.focus(window, cx));
            changed = true;
        }
        if changed {
            cx.notify();
        }
    }
    fn back(&mut self) {
        if let Some(tab) = self.tabs.get_mut(self.active)
            && let Some(index) = tab
                .parents
                .iter()
                .rposition(|(surface, ..)| surface == &self.surface)
        {
            let (_, method, value, scroll, selected) = tab.parents.remove(index);
            self.record_active = selected;
            tab.inspections.insert(self.surface.clone(), value);
            tab.inspection_method.insert(self.surface.clone(), method);
            tab.scrolls.insert(self.surface.clone(), scroll);
            tab.raw_details = false;
        }
    }
    fn target_summary(&self, fields: &Value) -> String {
        let Some(tab) = self.tabs.get(self.active) else {
            return "notebook".into();
        };
        if let Some(id) = fields["cell_id"].as_str()
            && let Some(cell) = tab.model.cells.get(id)
        {
            return cell_title(&cell.text, &cell.disk.kind);
        }
        for key in ["checkpoint_id", "session_id", "snapshot"] {
            if let Some(id) = fields[key].as_str() {
                for value in tab
                    .inspections
                    .values()
                    .chain(tab.parents.iter().map(|(_, _, value, _, _)| value))
                {
                    if let Some(items) = value["items"].as_array()
                        && let Some(record) = items.iter().find(|row| row[key] == id)
                    {
                        return record_label(record, &tab.model);
                    }
                }
                return if key == "snapshot" {
                    format!("training snapshot {}", &id[..id.len().min(12)])
                } else if key == "checkpoint_id" {
                    "the selected recorded checkpoint".into()
                } else {
                    "the selected recorded session".into()
                };
            }
        }
        tab.model
            .project
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned()
    }
    fn support_info(&self) -> String {
        let version = self
            .tabs
            .get(self.active)
            .and_then(|tab| tab.inspections.get("Help"))
            .and_then(|value| value["identity"]["kernelledger_version"].as_str())
            .unwrap_or("unavailable (no helper attached)");
        build_info(version)
    }
    fn key_down(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
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
                || event.keystroke.key == "f1"
            {
                cx.stop_propagation();
            }
            return;
        }
        let key = event.keystroke.key.as_str();
        let m = event.keystroke.modifiers;
        if self.app_menu || self.runtime_menu {
            if key == "escape" {
                self.close_menu(window, cx);
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
                if m.control || m.alt || key == "f1" {
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
                if m.control || m.alt || key == "f1" {
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
            cx.stop_propagation();
            cx.notify();
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
                                &self.history_query.read(cx).value().to_lowercase(),
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
    fn save(&mut self, _: &Save, _: &mut Window, _: &mut Context<Self>) {
        self.send(Command::Save);
    }
    fn run(&mut self, _: &RunCell, _: &mut Window, _: &mut Context<Self>) {
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
    fn all(&mut self, _: &RunAll, _: &mut Window, _: &mut Context<Self>) {
        self.batch("all");
    }
    fn batch(&self, scope: &str) {
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
    fn palette(&mut self, _: &Palette, window: &mut Window, cx: &mut Context<Self>) {
        self.return_focus = window.focused(cx);
        self.palette_active = 0;
        self.palette = !self.palette;
        if self.palette {
            self.palette_query.update(cx, |s, cx| {
                s.set_value("", window, cx);
                s.focus(window, cx);
            });
        } else {
            self.focus.focus(window, cx);
        }
        cx.notify();
    }
    fn command_mode(&mut self, _: &CommandMode, window: &mut Window, cx: &mut Context<Self>) {
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
    fn mutate(&mut self, mut request: Value) {
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
    fn toggle_theme(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.follow_system = false;
        self.high_contrast = false;
        self.dark = !self.dark;
        persist_appearance(if self.dark { "dark" } else { "light" }, cx);
        apply_theme(self.dark, self.high_contrast, window, cx);
        cx.notify();
    }
    fn select_surface(&mut self, surface: &str, cx: &mut Context<Self>) {
        if let Some(tab) = self.tabs.get_mut(self.active) {
            tab.selections
                .insert(self.surface.clone(), self.record_active);
            self.record_active = tab.selections.get(surface).copied().unwrap_or(0);
        }
        self.surface = surface.into();
        if self
            .tabs
            .get(self.active)
            .is_some_and(|t| t.inspections.contains_key(surface))
        {
            cx.notify();
            return;
        }
        match surface {
            "History" => self.inspect("sessions.list", json!({})),
            "Checkpoints" => self.inspect("checkpoints.list", json!({})),
            "Diagnostics" => self.inspect("doctor", json!({})),
            "Runtimes" => self.inspect("runtimes.list", json!({})),
            _ => {}
        }
        cx.notify();
    }
    fn run_command(&mut self, method: &str, window: &mut Window, cx: &mut Context<Self>) {
        if self.command_reason(method).is_some() { return; }
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
            "export-ipynb" => self.export_dialog(false, window, cx),
            "export-bundle" => self.export_dialog(true, window, cx),
            "stop" | "restart" => self.confirmation = Some((method.into(), json!({}))),
            "run-all" => self.batch("all"),
            "above" | "below" => self.batch(method),
            "settings" => self.select_surface("Settings", cx),
            "diagnostics" => self.select_surface("Diagnostics", cx),
            "runtimes" => self.select_surface("Runtimes", cx),
            "checkpoints" => self.select_surface("Checkpoints", cx),
            "help" => self.select_surface("Help", cx),
            "outline" => {
                if self.compact {
                    self.outline_drawer = !self.outline_drawer;
                    self.palette = false;
                    if let Some(focus) = self.return_focus.take() { focus.focus(window, cx); }
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
    fn command_reason(&self, method: &str) -> Option<&'static str> {
        if matches!(method, "open" | "help") { return None; }
        let Some(tab) = self.tabs.get(self.active) else { return Some("Open a notebook to use this action"); };
        if method == "outline" && self.surface != "Notebook" { return Some("Outline is available in Notebook"); }
        action_reason(method, &tab.runtime)
    }
    fn choose_interpreter(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let paths = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("Select CPython 3.11 or newer".into()),
        });
        cx.spawn_in(window, async move |view, window| {
            if let Ok(Ok(Some(paths))) = paths.await
                && let Some(path) = paths.into_iter().next()
            {
                let _ = view.update_in(window, |v, _, _| {
                    v.send(Command::SelectInterpreter {
                        path,
                        remember: false,
                    })
                });
            }
        })
        .detach();
    }
    fn sidebar(&self, cx: &mut Context<Self>) -> Stateful<Div> {
        let theme = cx.theme();
        let (border, bar, muted_fg, primary, active_bg, hover_bg, radius, warning, danger, info) = (
            theme.sidebar_border,
            theme.sidebar,
            theme.muted_foreground,
            theme.primary,
            theme.list_active,
            theme.list_hover,
            theme.radius,
            theme.warning,
            theme.danger,
            theme.info,
        );
        let mono = theme.mono_font_family.clone();
        let tab = &self.tabs[self.active];
        let query = self.query.read(cx).value().to_lowercase();
        let mut list = v_flex()
            .id("cell-list")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .px_2()
            .pb_2()
            .gap_px();
        let mut shown = 0;
        for (i, id) in tab.model.order.iter().enumerate() {
            let cell = &tab.model.cells[id];
            if !query.is_empty()
                && !cell.text.to_lowercase().contains(&query)
                && !id.to_lowercase().contains(&query)
            {
                continue;
            }
            shown += 1;
            let selected = tab.model.selected.as_deref() == Some(id.as_str());
            let preview = cell
                .text
                .lines()
                .map(str::trim)
                .find(|l| !l.is_empty())
                .map(|l| l.chars().take(80).collect::<String>());
            let marker = if matches!(cell.save, SaveState::Conflict | SaveState::Failed)
                || matches!(cell.outcome.as_str(), "error" | "failed")
            {
                Some(danger)
            } else if cell.stale() {
                Some(warning)
            } else if cell.save == SaveState::Dirty {
                Some(info)
            } else {
                None
            };
            let id = id.clone();
            list = list.child(
                h_flex()
                    .id(("select", i))
                    .role(Role::ListBoxOption)
                    .aria_label(format!(
                        "Cell {}: {}",
                        i + 1,
                        cell_title(&cell.text, &cell.disk.kind)
                    ))
                    .aria_selected(selected)
                    .gap_2()
                    .px_2()
                    .py_1p5()
                    .items_center()
                    .rounded(radius)
                    .cursor_pointer()
                    .when(selected, |d| d.bg(active_bg))
                    .when(!selected, |d| d.hover(move |s| s.bg(hover_bg)))
                    .child(
                        div()
                            .flex_none()
                            .text_color(if selected { primary } else { muted_fg })
                            .child(Icon::new(kind_icon(&cell.disk.kind)).small()),
                    )
                    .child(
                        v_flex()
                            .flex_1()
                            .min_w_0()
                            .child(
                                div()
                                    .truncate()
                                    .text_sm()
                                    .when(preview.is_none(), |d| d.italic().text_color(muted_fg))
                                    .child(preview.unwrap_or_else(|| {
                                        format!("Empty {} cell", cell.disk.kind)
                                    })),
                            )
                            .child(
                                div()
                                    .truncate()
                                    .font_family(mono.clone())
                                    .text_size(px(10.5))
                                    .text_color(muted_fg)
                                    .child(format!(
                                        "{} ? {}",
                                        kind_label(&cell.disk.kind),
                                        cell.ordinal
                                            .map(|n| format!("Run {n}"))
                                            .unwrap_or_else(|| "Not run".into())
                                    )),
                            ),
                    )
                    .when_some(marker, |d, color| d.child(dot(color)))
                    .on_click(cx.listener(move |v, _, window, cx| {
                        v.surface = "Notebook".into();
                        let t = &mut v.tabs[v.active];
                        t.model.selected = Some(id.clone());
                        t.list.scroll_to_reveal_item(i);
                        t.reveal = Some(id.clone());
                        v.focus.focus(window, cx);
                        cx.notify();
                    })),
            );
        }
        if shown == 0 {
            list = list.child(
                div()
                    .px_2()
                    .py_6()
                    .text_center()
                    .text_sm()
                    .text_color(muted_fg)
                    .child(if tab.model.order.is_empty() {
                        "No cells yet"
                    } else {
                        "No cells match"
                    }),
            );
        }
        let mut add = h_flex()
            .flex_wrap()
            .flex_none()
            .gap_1()
            .p_2()
            .border_t_1()
            .border_color(border);
        for (kind, label) in [("code", "Code"), ("markdown", "Markdown"), ("raw", "Raw")] {
            add =
                add.child(
                    Button::new(kind)
                        .small()
                        .ghost()
                        .icon(Glyph::Plus)
                        .label(label)
                        .tooltip(format!("Add {kind} cell"))
                        .flex_1()
                        .on_click(cx.listener(move |v, _, _, _| {
                            v.mutate(json!({"action":"add","type":kind}))
                        })),
                );
        }
        v_flex()
            .id("sidebar")
            .debug_selector(|| "notebook-outline".into())
            .track_focus(&self.sidebar_focus)
            .size_full()
            .bg(bar)
            .child(
                h_flex()
                    .flex_none()
                    .px_3()
                    .pt_3()
                    .pb_2()
                    .justify_between()
                    .items_center()
                    .child(
                        div()
                            .text_xs()
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(muted_fg)
                            .child("CELLS"),
                    )
                    .child(badge(tab.model.order.len().to_string(), muted_fg)),
            )
            .child(
                div().flex_none().px_2().pb_2().child(
                    Input::new(&self.query)
                        .small()
                        .cleanable(true)
                        .prefix(Icon::new(Glyph::Search).small().text_color(muted_fg)),
                ),
            )
            .child(list)
            .child(add)
    }
    fn empty_notebook(&self, cx: &mut Context<Self>) -> AnyElement {
        let muted_fg = cx.theme().muted_foreground;
        v_flex()
            .size_full()
            .items_center()
            .justify_center()
            .gap_4()
            .child(icon_tile(Glyph::NotebookPen, 48., cx))
            .child(
                v_flex()
                    .items_center()
                    .gap_1()
                    .child(
                        div()
                            .text_lg()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child("This notebook is empty"),
                    )
                    .child(
                        div()
                            .text_sm()
                            .text_color(muted_fg)
                            .child("Add a cell to start writing. Nothing runs until you ask."),
                    ),
            )
            .child(
                h_flex()
                    .gap_2()
                    .child(
                        Button::new("empty-code")
                            .primary()
                            .icon(Glyph::Code)
                            .label("Add code cell")
                            .on_click(cx.listener(|v, _, _, _| {
                                v.mutate(json!({"action":"add","type":"code"}))
                            })),
                    )
                    .child(
                        Button::new("empty-markdown")
                            .outline()
                            .icon(Glyph::Type)
                            .label("Add markdown cell")
                            .on_click(cx.listener(|v, _, _, _| {
                                v.mutate(json!({"action":"add","type":"markdown"}))
                            })),
                    ),
            )
            .into_any_element()
    }
    fn welcome(&self, cx: &mut Context<Self>) -> Div {
        let theme = cx.theme();
        let (border, muted_fg, radius) = (theme.border, theme.muted_foreground, theme.radius_lg);
        let mut shortcuts = v_flex()
            .w(px(340.))
            .rounded(radius)
            .border_1()
            .border_color(border)
            .overflow_hidden();
        for (label, keys) in [
            ("Open notebook", "ctrl-o"),
            ("Command palette", "ctrl-shift-p"),
            ("Run selected cell", "shift-enter"),
            ("Run all cells", "ctrl-shift-enter"),
            ("Save", "ctrl-s"),
            ("Command mode", "escape"),
        ] {
            shortcuts = shortcuts.child(
                h_flex()
                    .px_3()
                    .py_2()
                    .justify_between()
                    .items_center()
                    .border_b_1()
                    .border_color(border)
                    .text_sm()
                    .child(div().text_color(muted_fg).child(label))
                    .children(kbd(keys)),
            );
        }
        let recent = self.recent.iter().take(5).enumerate().map(|(i, path)| {
            let path = path.clone();
            Button::new(("recent", i))
                .ghost()
                .label(
                    path.file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .to_string(),
                )
                .tooltip(path.display().to_string())
                .on_click(cx.listener(move |v, _, _, cx| v.open(path.clone(), cx)))
        });
        v_flex()
            .flex_1()
            .items_center()
            .justify_center()
            .gap_6()

            .child(
                v_flex()
                    .items_center()
                    .gap_2()
                    .child(
                        div()
                            .text_2xl()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child("KernelLedger"),
                    )
                    .child(
                        div()
                            .text_color(muted_fg)
                            .child("Create or open a notebook. Your source and recorded results stay in local files."),
                    ),
            )
            .child(
                Button::new("welcome-open")
                    .primary()
                    .large()
                    .icon(Glyph::FolderOpen)
                    .label("Open notebook")
                    .on_click(
                        cx.listener(|v, _, window, cx| v.open_dialog(&OpenNotebook, window, cx)),
                    ),
            )
            .child(
                h_flex()
                    .gap_2()
                    .flex_wrap()
                    .child(
                        Button::new("create")
                            .outline()
                            .label("Create notebook")
                            .on_click(cx.listener(|v, _, w, cx| v.create_dialog(false, w, cx))),
                    )
                    .child(
                        Button::new("sample")
                            .outline()
                            .label("Create sample notebook")
                            .on_click(cx.listener(|v, _, w, cx| v.create_dialog(true, w, cx))),
                    )
                    .child(
                        Button::new("welcome-help")
                            .ghost()
                            .label("Help | F1")
                            .on_click(cx.listener(|v, _, _, cx| {
                                v.surface = "Help".into();
                                cx.notify();
                            })),
                    ),
            )
            .child(h_flex().gap_2().flex_wrap().children(recent))
            .child(shortcuts)
    }
    fn banners(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Div {
        let theme = cx.theme();
        let (danger, warning, info, muted_fg) = (
            theme.danger,
            theme.warning,
            theme.info,
            theme.muted_foreground,
        );
        let mut banners = v_flex().flex_none();
        if let Some(id) = self.tabs[self.active].checkpoint_notice.clone() {
            banners = banners.child(h_flex().px_3().py_1().gap_2().items_center()
                .child("Checkpoint saved")
                .child(Button::new("view-saved-checkpoint").ghost().small().label("View checkpoint")
                    .on_click(cx.listener(move |v, _, _, cx| {
                        v.select_surface("Checkpoints", cx);
                        v.inspect("checkpoint.get", json!({"checkpoint_id":id}));
                        v.tabs[v.active].checkpoint_notice = None;
                    })))
                .child(Button::new("dismiss-checkpoint-notice").ghost().small().label("Dismiss")
                    .on_click(cx.listener(|v, _, _, cx| { v.tabs[v.active].checkpoint_notice = None; cx.notify(); }))));
        }
        if let Some(error) = self.tabs[self.active].error.clone() {
            banners = banners.child(
                banner(danger, Glyph::CircleAlert)
                    .child(div().flex_1().min_w_0().text_sm().child(error))
                    .child(
                        Button::new("diagnostics")
                            .small()
                            .outline()
                            .label("Diagnostics")
                            .on_click(
                                cx.listener(|v, _, _, cx| v.select_surface("Diagnostics", cx)),
                            ),
                    )
                    .child(
                        Button::new("dismiss-error")
                            .small()
                            .ghost()
                            .icon(Glyph::X)
                            .tooltip("Dismiss")
                            .accessibility_label("Dismiss")
                            .on_click(cx.listener(|v, _, _, cx| {
                                v.tabs[v.active].error = None;
                                cx.notify();
                            })),
                    ),
            );
        }
        let source_state = self.tabs[self.active].model.source_state.clone();
        if self.tabs[self.active].uninitialized {
            banners = banners.child(banner(warning, Glyph::Info).flex_wrap()
                .child(div().min_w_0().flex_1().whitespace_normal().child("This directory is not an initialized notebook. Initialize it to add cells, or choose another directory."))
                .child(Button::new("initialize").outline().label("Initialize as notebook").on_click(cx.listener(|v, _, _, _| v.send(Command::Initialize { sample: false }))))
                .child(Button::new("choose-another").ghost().label("Choose another").on_click(cx.listener(|v, _, w, cx| v.open_dialog(&OpenNotebook, w, cx)))));
        } else if source_state != "ok" {
            banners = banners.child(banner(warning, Glyph::TriangleAlert).child(
                div().text_sm().child(format!(
                    "Source state: {source_state} — history remains available"
                )),
            ));
        }
        let drafts = self.tabs[self.active].drafts.clone();
        for (index, draft) in drafts.iter().enumerate() {
            let id = draft["cell_id"].as_str().unwrap_or("unknown").to_owned();
            banners = banners.child(
                banner(warning, Glyph::FileText)
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_sm()
                            .child(format!("Recovered an unsaved draft for {id}")),
                    )
                    .child(
                        Button::new(("restore-draft", index))
                            .small()
                            .primary()
                            .label("Restore")
                            .on_click(cx.listener({
                                let draft = draft.clone();
                                move |v, _, _, cx| {
                                    v.send(Command::RestoreDraft(draft.clone()));
                                    cx.notify();
                                }
                            })),
                    )
                    .child(
                        Button::new(("compare-draft", index))
                            .small()
                            .outline()
                            .label("Compare")
                            .on_click(cx.listener({
                                let draft = draft.clone();
                                move |v, _, _, cx| {
                                    v.tabs[v.active]
                                        .inspections
                                        .insert("Diagnostics".into(), draft.clone());
                                    v.surface = "History".into();
                                    cx.notify();
                                }
                            })),
                    )
                    .child(
                        Button::new(("discard-draft", index))
                            .small()
                            .ghost()
                            .label("Discard")
                            .on_click(cx.listener(move |v, _, _, cx| {
                                v.send(Command::DiscardDraft(id.clone()));
                                v.tabs[v.active].drafts.retain(|d| d["cell_id"] != id);
                                cx.notify();
                            })),
                    ),
            );
        }
        let inputs = self.tabs[self.active].runtime["pending_inputs"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        for input in inputs {
            let iid = input["input_id"].as_str().unwrap_or_default().to_string();
            let password = input["password"] == true;
            let state = self
                .prompts
                .entry(iid.clone())
                .or_insert_with(|| cx.new(|cx| InputState::new(window, cx).masked(password)))
                .clone();
            let notice =
                if password && self.tabs[self.active].runtime["record_secret_input"] != true {
                    "Not recorded"
                } else {
                    "Your reply is recorded in history and included in bundles"
                };
            banners = banners.child(
                banner(info, Glyph::Terminal)
                    .child(
                        v_flex()
                            .flex_none()
                            .max_w(px(320.))
                            .child(
                                div()
                                    .text_sm()
                                    .font_weight(FontWeight::MEDIUM)
                                    .truncate()
                                    .child(input["prompt"].as_str().unwrap_or("Input").to_string()),
                            )
                            .child(div().text_xs().text_color(muted_fg).child(notice)),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(160.))
                            .child(Input::new(&state).small()),
                    )
                    .child(
                        Button::new(SharedString::from(format!("claim-{iid}")))
                            .small()
                            .ghost()
                            .label("Claim")
                            .on_click(cx.listener({
                                let iid = iid.clone();
                                move |v, _, _, _| v.runtime("input.claim", json!({"input_id":iid}))
                            })),
                    )
                    .child(
                        Button::new(SharedString::from(format!("reply-{iid}")))
                            .small()
                            .primary()
                            .label("Reply")
                            .on_click(cx.listener(move |v, _, window, cx| {
                                let text = state.read(cx).value().to_string();
                                v.runtime("input.reply", json!({"input_id":iid,"text":text}));
                                state.update(cx, |s, cx| s.set_value("", window, cx));
                            })),
                    ),
            );
        }
        banners
    }
    fn status_bar(&self, cx: &mut Context<Self>) -> Div {
        let theme = cx.theme();
        let (border, bar, muted_fg, primary) = (
            theme.status_bar_border,
            theme.status_bar,
            theme.muted_foreground,
            theme.primary,
        );
        let tab = &self.tabs[self.active];
        let state = tab.runtime["worker_state"].as_str();
        let state_color = state.map(|s| tone(s, cx)).unwrap_or(muted_fg);
        let edit = tab.model.mode == Mode::Edit;
        h_flex()
            .flex_none()
            .h(px(26.))
            .px_3()
            .gap_4()
            .items_center()
            .bg(bar)
            .border_t_1()
            .border_color(border)
            .text_xs()
            .text_color(muted_fg)
            .child(badge(
                format!("{:?} mode", tab.model.mode).to_uppercase(),
                if edit { primary } else { muted_fg },
            ))
            .child(format!(
                "{} cell{}",
                tab.model.order.len(),
                if tab.model.order.len() == 1 { "" } else { "s" }
            ))
            .child(
                h_flex()
                    .gap_1p5()
                    .items_center()
                    .child(dot(state_color))
                    .child(if tab.runtime["connection_lost"] == true { "Connection lost — execution state unknown".into() } else { worker_label(state) }),
            )
            .child(if tab.model.cells.values().any(|c| matches!(c.save, SaveState::Failed | SaveState::Conflict)) {
                "Edits retained — save needs attention"
            } else if tab.model.cells.values().any(|c| c.save != SaveState::Saved) { "Saving…" } else { "Saved" })
            .child(div().flex_1())
            .child(
                h_flex()
                    .gap_1p5()
                    .items_center()
                    .child(Icon::new(Glyph::Info).xsmall())
                    .child(if self.compact { "Closing detaches" } else { "Closing detaches; computations continue" }),
            )
    }
}
impl Render for Workspace {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.confirmation.is_some() && !self.dialog_open {
            self.return_focus = window.focused(cx);
            self.confirm_focus.focus(window, cx);
            self.dialog_open = true;
        } else if self.confirmation.is_none() && self.dialog_open {
            self.dialog_open = false;
            if let Some(focus) = self.return_focus.take() {
                focus.focus(window, cx);
            }
        }
        self.compact = window.viewport_size().width < px(1024.);
        let title = self
            .tabs
            .get(self.active)
            .map(|t| format!("KernelLedger - {}", t.model.project.display()))
            .unwrap_or_else(|| "KernelLedger".into());
        window.set_window_title(&title);
        self.palette_scroll.scroll_to_item(self.palette_active);
        let filter = self.palette_query.read(cx).value().to_lowercase();
        if filter != self.palette_filter {
            self.palette_filter = filter;
            self.palette_active = 0;
        }
        let mut root = v_flex()
            .id("workspace")
            .key_context("KernelLedger")
            .track_focus(&self.focus)
            .relative()
            .size_full()
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .text_size(px(if self.preferences.comfortable {
                15.
            } else {
                13.
            }))
            .on_action(cx.listener(Self::save))
            .on_action(cx.listener(Self::run))
            .on_action(cx.listener(Self::all))
            .on_action(cx.listener(Self::palette))
            .on_action(cx.listener(Self::open_dialog))
            .on_action(cx.listener(Self::command_mode))
            .child(self.header(cx));
        if self.tabs.is_empty() {
            if self.surface == "Help" {
                root = root.child(
                    div()
                        .id("welcome-help-content")
                        .flex_1()
                        .overflow_y_scroll()
                        .p_6()
                        .child(
                            self.help_content(cx),
                        )
                        .child(Button::new("back-welcome").ghost().label("Back").on_click(
                            cx.listener(|v, _, _, cx| {
                                v.surface = "Notebook".into();
                                cx.notify();
                            }),
                        )),
                );
            } else {
                root = root.child(self.welcome(cx));
            }
        } else {
            let center = if self.surface != "Notebook" {
                self.panel(cx)
            } else if self.tabs[self.active].model.order.is_empty() {
                self.empty_notebook(cx)
            } else {
                let state = self.tabs[self.active].list.clone();
                let view = cx.entity();
                list(state, move |i, window, cx| {
                    view.update(cx, |view, cx| view.cell(i, window, cx))
                })
                .size_full()
                .into_any_element()
            };
            root = root
                .when(self.surface == "Notebook", |d| d.child(self.toolbar(cx)))
                .child(self.banners(window, cx))
                .child(
                    h_flex()
                        .flex_1()
                        .min_h_0()
                        .relative()
                        .child(
                            h_resizable(SharedString::from(format!(
                                "notebook-layout-v5-{}-{}-{}",
                                self.preferences.outline_width, self.surface, self.sidebar_visible && !self.compact
                            )))
                            .when(
                                self.surface == "Notebook" && !self.compact && self.sidebar_visible,
                                |d| {
                                    d.child(
                                        resizable_panel()
                                            .size(px(self.preferences.outline_width))
                                            .size_range(px(180.)..px(300.))
                                            .child(self.sidebar(cx)),
                                    )
                                },
                            )
                            .child(resizable_panel().child(center))
                            .on_resize(cx.listener(
                                |v,
                                 state: &Entity<gpui_kit::component::resizable::ResizableState>,
                                 _,
                                 cx| {
                                    if v.surface == "Notebook" && !v.compact && v.sidebar_visible {
                                        if let Some(width) = state.read(cx).sizes().first() {
                                            v.preferences.outline_width =
                                                f32::from(*width).clamp(180., 300.);
                                            v.save_preferences(cx);
                                        }
                                    }
                                },
                            )),
                        )
                        .when(
                            self.surface == "Notebook" && self.compact && self.outline_drawer,
                            |d| {
                                d.child(
                                    div()
                                        .absolute()
                                        .left_0()
                                        .top_0()
                                        .bottom_0()
                                        .w(px(220.))
                                        .border_r_1()
                                        .border_color(cx.theme().border)
                                        .child(self.sidebar(cx)),
                                )
                            },
                        ),
                )
                .child(self.status_bar(cx));
        }
        if self.app_menu || self.runtime_menu {
            root = root.child(self.menu_overlay(cx));
        }
        if self.palette {
            root = root.child(self.palette_overlay(cx));
        }
        if let Some((method, fields)) = self.confirmation.clone() {
            root = root.child(self.confirm_overlay(method, fields, window, cx));
        }
        root.into_any_element()
    }
}
fn kind_label(kind: &str) -> &'static str {
    match kind {
        "code" => "Python",
        "markdown" => "Markdown",
        _ => "Raw",
    }
}
fn kind_icon(kind: &str) -> Glyph {
    match kind {
        "code" => Glyph::Code,
        "markdown" => Glyph::Type,
        _ => Glyph::FileText,
    }
}
fn editor_lines(text: &str) -> usize {
    text.split('\n').count().clamp(3, 30)
}
fn tone(status: &str, cx: &App) -> Hsla {
    let theme = cx.theme();
    match status {
        "ok" | "success" | "succeeded" | "completed" | "complete" | "idle" | "ready"
        | "verified" => theme.success,
        "error" | "failed" | "crashed" | "conflict" | "corrupt" => theme.danger,
        "interrupted" | "cancelled" | "canceled" | "interrupting" | "partial" | "stale" => {
            theme.warning
        }
        "running" | "starting" | "queued" | "accepted" | "pending" => theme.info,
        _ => theme.muted_foreground,
    }
}
fn worker_label(state: Option<&str>) -> String {
    match state {
        None | Some("absent") => "Not running".into(),
        Some("idle") => "Ready".into(),
        Some("stopped") => "Stopped | run to start".into(),
        Some(s) => {
            let mut chars = s.chars();
            chars
                .next()
                .map(|c| c.to_uppercase().chain(chars).collect())
                .unwrap_or_default()
        }
    }
}
fn kbd(keys: &str) -> Option<Kbd> {
    Keystroke::parse(keys).ok().map(Kbd::new)
}
fn safe_link(url: &SharedString, _: &ClickEvent, _: &mut Window, cx: &mut App) {
    if url.starts_with("https://") || url.starts_with("http://") {
        cx.open_url(url);
    }
}
fn output(event: &Value, id: String, _cx: &App) -> AnyElement {
    outputs::OutputBlock { id: id.into(), event: event.clone() }.into_any_element()
}

fn inspection_surface(method: &str) -> &'static str {
    match method {
        "sessions.list" | "executions.list" | "execution.get" => "History",
        "checkpoints.list" | "checkpoint.get" | "training.list" | "training.inspect" => {
            "Checkpoints"
        }
        "runtimes.list" => "Runtimes",
        "interpreter.validate" => "Settings",
        "build.identity" => "Help",
        _ => "Diagnostics",
    }
}
fn cell_title(source: &str, kind: &str) -> String {
    source
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map(|line| line.chars().take(64).collect())
        .unwrap_or_else(|| format!("Empty {kind} cell"))
}
fn confirmation_copy(method: &str) -> (&'static str, &'static str, &'static str) {
    match method {
        "training-restore" => (
            "Restore training for",
            "Executes the captured reconstruction factory in a fresh worker. After success it replaces your live namespace. Registered training state is restored; notebook variables and native process state are excluded. Failed reconstruction preserves the current worker.",
            "Trust and restore training",
        ),
        "delete" => (
            "Delete",
            "This build has no structural undo. Deleting removes this cell and its source from the notebook. Recorded executions remain available in History.",
            "Delete cell",
        ),
        "stop" => (
            "Stop runtime for",
            "Stops computation and shuts down the supervisor and worker for all attached clients. Unsaved live variables are lost. Saved source and recorded history remain available. Running again starts a runtime.",
            "Stop runtime",
        ),
        "restart" => (
            "Restart runtime for",
            "Discards the live namespace and starts an empty worker. Saved source and history remain available.",
            "Restart empty",
        ),
        "restore" => (
            "Restore",
            "Restores recorded checkpoint values into a new session. Checkpoint omissions remain absent. Review the checkpoint report before restoring.",
            "Restore checkpoint",
        ),
        "continue" => (
            "Continue from",
            "Restores the checkpoint and runs later saved code. Code may repeat file and network side effects.",
            "Continue",
        ),
        "replay" => (
            "Replay",
            "Runs recorded code in a new session. File and network side effects can repeat; secret input must be supplied again.",
            "Replay session",
        ),
        "migrate" => (
            "Migrate ledger for",
            "Creates a schema 1 backup before migration. Older clients cannot open the migrated ledger.",
            "Migrate ledger",
        ),
        _ => (
            "Apply action to",
            "Review the target before applying this action.",
            "Apply action",
        ),
    }
}
fn error_message(code: &str) -> &'static str {
    match code {
        "source_conflict" | "conflict" => {
            "The source changed on disk. Your draft is retained; resolve the cell conflict before running."
        }
        "permission_denied" => {
            "The notebook could not be written. Your draft is retained; choose a writable location or correct its permissions."
        }
        "unsupported_schema" => {
            "This notebook uses a newer schema. Inspection may be limited; open it with a compatible KernelLedger version."
        }
        "migration_required" => {
            "The ledger needs migration. Open Diagnostics to migrate it with a backup."
        }
        "too_large" => {
            "This record exceeds the viewer limit. Recorded data is retained; inspect it with the CLI."
        }
        "not_found" => {
            "The requested record or file is unavailable. Reload the notebook to refresh its records."
        }
        _ => {
            "The operation could not finish. Your source and drafts are retained; open Diagnostics and reconnect or reload. Technical details can be copied there."
        }
    }
}
fn build_info(version: &str) -> String {
    format!(
        "KernelLedger desktop {}\nKernelLedger runtime: {version} (development Python environment; private bundle not packaged)\nRuntime plan: 5\nFrontend plan: 4\nDesktop protocol: kernelledger.desktop v1",
        env!("CARGO_PKG_VERSION")
    )
}
fn persist_appearance(value: &str, cx: &mut App) {
    let path = kernelledger_client::profile_path().join("appearance.txt");
    let value = value.to_owned();
    cx.background_executor()
        .spawn(async move {
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(path, value)
        })
        .detach();
}

fn record_label(record: &Value, _model: &NotebookModel) -> String {
    let title = if record.get("exec_id").is_some() {
        cell_title(
            record["source"]
                .as_str()
                .unwrap_or("Recorded execution (source unavailable)"),
            "code",
        )
    } else if record.get("checkpoint_id").is_some() {
        "Checkpoint".into()
    } else if record.get("project_path").is_some() {
        format!(
            "{} | {}",
            record["project_path"].as_str().unwrap_or("Notebook"),
            record["runtime"]["interpreter"]["executable"]
                .as_str()
                .unwrap_or("interpreter unavailable")
        )
    } else {
        "Session".into()
    };
    let time = ["timestamp", "start", "started", "accepted", "start_time"]
        .iter()
        .find_map(|key| record[*key].as_str())
        .unwrap_or("time unavailable");
    let time_label = relative_time(time);
    let duration = record["duration"]
        .as_f64()
        .or_else(|| {
            let start = chrono::DateTime::parse_from_rfc3339(record["start"].as_str()?).ok()?;
            let end = chrono::DateTime::parse_from_rfc3339(record["end"].as_str()?).ok()?;
            Some((end - start).num_milliseconds().max(0) as f64 / 1000.)
        })
        .map(|d| format!(" | {d:.2}s"))
        .unwrap_or_default();
    format!("{title} | {time_label}{duration}")
}

fn runtime_action_available(method: &str, runtime: &Value, _surface: &str) -> bool {
    action_reason(method, runtime).is_none()
}
fn action_reason(method: &str, runtime: &Value) -> Option<&'static str> {
    kernelledger_models::presentation::RuntimeAction::parse(method)
        .and_then(|action| action.unavailable_reason(runtime))
}

fn history_matches(item: &Value, filter: &str, model: &NotebookModel) -> bool {
    let label = record_label(item, model).to_lowercase();
    filter.split_whitespace().all(|part| {
        if let Some((field, value)) = part.split_once(':') {
            match field {
                "cell" => {
                    item["cell_id"]
                        .as_str()
                        .is_some_and(|s| s.to_lowercase().contains(value))
                        || label.contains(value)
                }
                "outcome" => item["status"].as_str().is_some_and(|s| s == value),
                "session" => item["session_id"]
                    .as_str()
                    .is_some_and(|s| s.contains(value)),
                "from" | "after" => item["start"]
                    .as_str()
                    .or(item["timestamp"].as_str())
                    .is_some_and(|s| s >= value),
                "to" | "before" => item["start"]
                    .as_str()
                    .or(item["timestamp"].as_str())
                    .is_some_and(|s| s.get(..10).unwrap_or(s) <= value),
                _ => label.contains(part),
            }
        } else {
            label.contains(part)
                || item["source"]
                    .as_str()
                    .is_some_and(|s| s.to_lowercase().contains(part))
        }
    })
}

#[cfg(test)]
mod tests;

fn local_time(value: &str) -> String {
    chrono::DateTime::parse_from_rfc3339(value)
        .map(|date| {
            date.with_timezone(&chrono::Local)
                .format("%d %b %Y, %H:%M:%S %Z")
                .to_string()
        })
        .unwrap_or_else(|_| value.to_owned())
}
fn relative_time(value: &str) -> String {
    let Ok(date) = chrono::DateTime::parse_from_rfc3339(value) else {
        return value.to_owned();
    };
    let seconds = (chrono::Utc::now() - date.with_timezone(&chrono::Utc)).num_seconds();
    match seconds {
        ..=59 => "just now".into(),
        60..=3599 => format!("{} min ago", seconds / 60),
        3600..=86399 => format!("{} h ago", seconds / 3600),
        86400..=604799 => format!("{} days ago", seconds / 86400),
        _ => local_time(value),
    }
}

#[cfg(target_os = "windows")]
fn system_high_contrast() -> bool {
    #[repr(C)]
    struct HighContrast {
        size: u32,
        flags: u32,
        scheme: *mut u16,
    }
    #[link(name = "user32")]
    unsafe extern "system" {
        fn SystemParametersInfoW(
            action: u32,
            param: u32,
            data: *mut std::ffi::c_void,
            flags: u32,
        ) -> i32;
    }
    let mut state = HighContrast {
        size: std::mem::size_of::<HighContrast>() as u32,
        flags: 0,
        scheme: std::ptr::null_mut(),
    };
    // SPI_GETHIGHCONTRAST reads a fixed-size structure; it never changes OS preferences.
    unsafe {
        SystemParametersInfoW(
            0x0042,
            state.size,
            (&mut state as *mut HighContrast).cast(),
            0,
        ) != 0
            && state.flags & 1 != 0
    }
}
#[cfg(not(target_os = "windows"))]
fn system_high_contrast() -> bool {
    false
}

fn operation_label(op: &Value, model: &NotebookModel) -> String {
    let kind = op["kind"]
        .as_str()
        .or(op["method"].as_str())
        .unwrap_or("Operation");
    let target = op["request"]["cell_id"]
        .as_str()
        .and_then(|id| model.cells.get(id))
        .map(|cell| cell_title(&cell.text, &cell.disk.kind))
        .unwrap_or_else(|| {
            model
                .project
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string()
        });
    format!(
        "{kind} | {target} | {}",
        op["accepted"]
            .as_str()
            .map(relative_time)
            .unwrap_or_else(|| "this session".into())
    )
}
