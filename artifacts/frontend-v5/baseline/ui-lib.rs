//! Views hold presentation and actions; services own I/O and persistence.
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
const COMMANDS: [(&str, &str, &str, Glyph); 14] = [
    (
        "training-checkpoint",
        "Training checkpoint",
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
        "restart",
        "Restart empty",
        "Discard the live namespace",
        Glyph::RotateCcw,
    ),
    (
        "help",
        "Help and shortcuts",
        "Keyboard guide, glossary and About",
        Glyph::Info,
    ),
    (
        "outline",
        "Toggle notebook sidebar",
        "Show or hide cell navigation",
        Glyph::BookOpen,
    ),
    (
        "run-all",
        "Run all cells",
        "Execute every code cell in order",
        Glyph::Play,
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
        "stop",
        "Stop runtime",
        "Stop computation and shut the worker down",
        Glyph::Power,
    ),
    (
        "gc",
        "Preview garbage collection",
        "Dry run; nothing is deleted",
        Glyph::Trash,
    ),
    (
        "open",
        "Open notebook…",
        "Choose a notebook directory",
        Glyph::FolderOpen,
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
    return_focus: Option<FocusHandle>,
    sidebar_visible: bool,
    compact: bool,
    follow_system: bool,
    history_query: Entity<InputState>,
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
            return_focus: None,
            sidebar_visible: false,
            compact: false,
            follow_system,
            history_query: cx.new(|cx| {
                InputState::new(window, cx).placeholder(
                    "Search history | cell:... outcome:error session:... from:YYYY-MM-DD to:YYYY-MM-DD",
                )
            }),
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
                        if value["worker_state"] == "stopped"
                            && tab
                                .inspections
                                .get("Technical error")
                                .is_some_and(|v| v["code"] == "transport")
                        {
                            tab.error = None;
                        }
                        tab.runtime = value;
                    }
                    Event::Operation(value) => {
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
        if matches!(method, "run-all" | "above" | "below")
            && (self.tabs.is_empty() || self.surface != "Notebook")
        {
            return;
        }
        match method {
            "stop" | "restart" => self.confirmation = Some((method.into(), json!({}))),
            "run-all" => self.batch("all"),
            "above" | "below" => self.batch(method),
            "help" => self.surface = "Help".into(),
            "outline" => self.sidebar_visible = !self.sidebar_visible,
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
    fn editor(
        &mut self,
        tab_index: usize,
        id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Entity<EditorState> {
        if let Some(editor) = self.tabs[tab_index].editors.get(id) {
            return editor.clone();
        }
        let cell = &self.tabs[tab_index].model.cells[id];
        let text = cell.text.clone();
        let kind = cell.disk.kind.clone();
        let editor = cx.new(|cx| {
            let mut s = EditorState::new(window, cx).language(if kind == "code" {
                "python"
            } else if kind == "markdown" {
                "markdown"
            } else {
                "text"
            });
            s.set_value(text, window, cx);
            s
        });
        let id = id.to_string();
        let subscription = cx.subscribe(&editor, {
            let id = id.clone();
            move |view, editor, event: &InputEvent, cx| {
                let tab = &mut view.tabs[tab_index];
                if matches!(event, InputEvent::Focus) {
                    view.active = tab_index;
                    tab.model.selected = Some(id.clone());
                    tab.model.mode = Mode::Edit;
                }
                if matches!(event, InputEvent::Change) {
                    let text = editor.read(cx).value().to_string();
                    if let Some(cell) = tab.model.cells.get_mut(&id)
                        && cell.text != text
                    {
                        let resized = editor_lines(&cell.text) != editor_lines(&text);
                        cell.edit(text);
                        if resized && let Some(i) = tab.model.order.iter().position(|c| *c == id) {
                            tab.list.splice(i..i + 1, 1);
                        }
                        if let Some(request) = tab.model.save_request(&id) {
                            let _ = tab.service.commands.send(Command::Edit {
                                cell_id: id.clone(),
                                request,
                            });
                        }
                    }
                }
                cx.notify();
            }
        });
        self.tabs[tab_index].subscriptions.push(subscription);
        self.tabs[tab_index].editors.insert(id, editor.clone());
        editor
    }
    fn cell(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let Some(id) = self.tabs[self.active].model.order.get(index).cloned() else {
            return div().into_any_element();
        };
        let editor = self.editor(self.active, &id, window, cx);
        let tab = &self.tabs[self.active];
        let cell = tab.model.cells[&id].clone();
        let selected = tab.model.selected.as_deref() == Some(id.as_str());
        let last = index + 1 == tab.model.order.len();
        let theme = cx.theme();
        let (border, muted, muted_fg, primary, background, radius) = (
            theme.border,
            theme.muted,
            theme.muted_foreground,
            theme.primary,
            theme.background,
            theme.radius_lg,
        );
        let mono = theme.mono_font_family.clone();
        let (warning, danger, info) = (theme.warning, theme.danger, theme.info);
        let save = match cell.save {
            SaveState::Saved => None,
            SaveState::Dirty => Some(("Unsaved", warning)),
            SaveState::Saving => Some(("Saving…", info)),
            SaveState::Conflict => Some(("Conflict", danger)),
            SaveState::Failed => Some(("Save failed", danger)),
        };
        let actions = h_flex()
            .flex_none()
            .gap_0p5()
            .child(
                Button::new(("run", index))
                    .ghost()
                    .small()
                    .icon(Glyph::Play)
                    .tooltip("Run cell · Shift+Enter")
                    .accessibility_label(format!("Run {}", cell_title(&cell.text, &cell.disk.kind)))
                    .disabled(cell.disk.kind != "code" || cell.save == SaveState::Conflict)
                    .on_click(cx.listener({
                        let id = id.clone();
                        move |v, _, _, _| {
                            v.send(Command::Run {
                                cell_ids: vec![id.clone()],
                                on_error: "stop".into(),
                            })
                        }
                    })),
            )
            .child(
                Button::new(("collapse", index))
                    .ghost()
                    .small()
                    .icon(if cell.collapsed {
                        Glyph::Eye
                    } else {
                        Glyph::EyeOff
                    })
                    .tooltip(if cell.collapsed {
                        "Show output"
                    } else {
                        "Hide output"
                    })
                    .accessibility_label(if cell.collapsed {
                        "Show output"
                    } else {
                        "Hide output"
                    })
                    .on_click(cx.listener({
                        let id = id.clone();
                        move |v, _, _, cx| {
                            let t = &mut v.tabs[v.active];
                            if let Some(c) = t.model.cells.get_mut(&id) {
                                c.collapsed = !c.collapsed;
                            }
                            t.list.splice(index..index + 1, 1);
                            cx.notify();
                        }
                    })),
            )
            .child(
                Button::new(("duplicate", index))
                    .ghost()
                    .small()
                    .icon(Glyph::Copy)
                    .tooltip("Duplicate")
                    .accessibility_label(format!(
                        "Duplicate {}",
                        cell_title(&cell.text, &cell.disk.kind)
                    ))
                    .on_click(cx.listener({
                        let id = id.clone();
                        move |v, _, _, _| v.mutate(json!({"action":"duplicate","cell_id":id}))
                    })),
            )
            .child(
                Button::new(("delete", index))
                    .ghost()
                    .small()
                    .icon(Glyph::Trash)
                    .tooltip("Delete")
                    .accessibility_label(format!(
                        "Delete {}",
                        cell_title(&cell.text, &cell.disk.kind)
                    ))
                    .on_click(cx.listener({
                        let id = id.clone();
                        move |v, _, window, cx| {
                            v.confirmation = Some(("delete".into(), json!({"cell_id":id})));
                            v.return_focus = window.focused(cx);
                            v.focus.focus(window, cx);
                            cx.notify();
                        }
                    })),
            );
        let header = h_flex()
            .h(px(36.))
            .px_3()
            .gap_2()
            .items_center()
            .bg(muted.opacity(0.45))
            .border_b_1()
            .border_color(border)
            .child(
                div()
                    .flex_none()
                    .font_family(mono.clone())
                    .text_xs()
                    .text_color(if selected { primary } else { muted_fg })
                    .child(
                        cell.ordinal
                            .map(|n| format!("[{n}]"))
                            .unwrap_or_else(|| "[ ]".into()),
                    ),
            )
            .child(badge(kind_label(&cell.disk.kind), muted_fg))
            .child(
                div()
                    .min_w_0()
                    .truncate()
                    .font_family(mono.clone())
                    .text_xs()
                    .text_color(muted_fg)
                    .child(cell_title(&cell.text, &cell.disk.kind)),
            )
            .when_some(save, |d, (label, color)| d.child(badge(label, color)))
            .when(!cell.outcome.is_empty(), |d| {
                d.child(badge(cell.outcome.clone(), tone(&cell.outcome, cx)))
            })
            .when(cell.stale(), |d| d.child(badge("Stale output", warning)))
            .child(div().flex_1())
            .child(actions);
        let mut card = v_flex()
            .id(SharedString::from(id.clone()))
            .w_full()
            .max_w(px(980.))
            .rounded(radius)
            .border_1()
            .border_color(if selected {
                primary.opacity(0.7)
            } else {
                border
            })
            .bg(background)
            .overflow_hidden()
            .when(selected, |d| d.shadow_md())
            .on_mouse_down(
                MouseButton::Left,
                cx.listener({
                    let id = id.clone();
                    move |v, _, _, cx| {
                        let t = &mut v.tabs[v.active];
                        if t.model.selected.as_deref() != Some(id.as_str()) {
                            t.model.selected = Some(id.clone());
                            cx.notify();
                        }
                    }
                }),
            )
            .child(header)
            .when(
                cell.disk.kind != "markdown" || (selected && tab.model.mode == Mode::Edit),
                |d| {
                    d.child(
                        div().px_1().py_1().child(
                            Editor::new(&editor)
                                .aria_label(format!("{} cell {id}", cell.disk.kind))
                                .h(editor_height(&cell.text))
                                .bordered(false),
                        ),
                    )
                },
            );
        if cell.save == SaveState::Conflict {
            card = card.child(
                v_flex()
                    .gap_2()
                    .px_4()
                    .py_3()
                    .border_t_1()
                    .border_color(danger.opacity(0.3))
                    .bg(danger.opacity(0.06))
                    .child(
                        h_flex()
                            .gap_2()
                            .items_center()
                            .text_sm()
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(danger)
                            .child(Icon::new(Glyph::TriangleAlert).small())
                            .child("Conflict — both versions are preserved"),
                    )
                    .child(
                        div()
                            .p_2()
                            .rounded(px(6.))
                            .bg(background)
                            .border_1()
                            .border_color(border)
                            .font_family(mono.clone())
                            .text_xs()
                            .child(bounded(&cell.disk.source, 4096)),
                    )
                    .child(
                        h_flex()
                            .gap_2()
                            .child(
                                Button::new(("disk", index))
                                    .small()
                                    .outline()
                                    .label("Take disk")
                                    .on_click(cx.listener({
                                        let id = id.clone();
                                        move |v, _, window, cx| {
                                            let t = &mut v.tabs[v.active];
                                            t.model.take_disk(&id);
                                            let _ = t
                                                .service
                                                .commands
                                                .send(Command::DiscardDraft(id.clone()));
                                            if let Some(e) = t.editors.get(&id) {
                                                e.update(cx, |s, cx| {
                                                    s.set_value(
                                                        t.model.cells[&id].text.clone(),
                                                        window,
                                                        cx,
                                                    )
                                                });
                                            }
                                            cx.notify();
                                        }
                                    })),
                            )
                            .child(
                                Button::new(("mine", index))
                                    .small()
                                    .primary()
                                    .label("Take mine")
                                    .on_click(cx.listener({
                                        let id = id.clone();
                                        move |v, _, _, _| {
                                            if let Some(r) =
                                                v.tabs[v.active].model.overwrite_request(&id)
                                            {
                                                v.send(Command::Mutate(r));
                                            }
                                        }
                                    })),
                            ),
                    ),
            );
        }
        if cell.disk.kind == "markdown" && !(selected && tab.model.mode == Mode::Edit) {
            card = card.child(
                div().px_4().py_3().border_t_1().border_color(border).child(
                    TextView::markdown(("preview", index), safe_markdown(&cell.text))
                        .on_link_click(safe_link),
                ),
            );
        }
        if !cell.output.is_empty() {
            card = card.child(if cell.collapsed {
                h_flex()
                    .px_4()
                    .py_2()
                    .gap_2()
                    .border_t_1()
                    .border_color(border)
                    .text_xs()
                    .text_color(muted_fg)
                    .child(Icon::new(Glyph::EyeOff).xsmall())
                    .child(format!(
                        "{} output{} hidden",
                        cell.output.len(),
                        if cell.output.len() == 1 { "" } else { "s" }
                    ))
            } else {
                v_flex()
                    .px_4()
                    .py_3()
                    .gap_2()
                    .border_t_1()
                    .border_color(border)
                    .bg(muted.opacity(0.25))
                    .children(
                        cell.output
                            .iter()
                            .enumerate()
                            .map(|(i, e)| output(e, format!("{id}-{i}"), cx)),
                    )
            });
        }
        div()
            .w_full()
            .flex()
            .justify_center()
            .px_6()
            .pt(px(if index == 0 { 20. } else { 6. }))
            .pb(px(if last { 32. } else { 6. }))
            .child(card)
            .into_any_element()
    }
    fn panel(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let value = self.tabs[self.active]
            .inspections
            .get(&self.surface)
            .cloned()
            .unwrap_or(Value::Null);
        let theme = cx.theme();
        let (muted_fg, primary) = (theme.muted_foreground, theme.primary);
        let mono = theme.mono_font_family.clone();
        let (glyph, description) = SURFACES
            .iter()
            .find(|(name, ..)| *name == self.surface)
            .map(|(_, glyph, description)| (*glyph, *description))
            .unwrap_or((Glyph::Info, ""));
        let mut body = v_flex().w_full().min_w_0().max_w(px(920.)).gap_6().child(
            h_flex()
                .gap_3()
                .items_center()
                .child(
                    div()
                        .flex_none()
                        .size(px(36.))
                        .rounded(px(8.))
                        .flex()
                        .items_center()
                        .justify_center()
                        .bg(primary.opacity(0.1))
                        .text_color(primary)
                        .child(Icon::new(glyph)),
                )
                .child(
                    v_flex()
                        .gap_0p5()
                        .child(
                            div()
                                .text_lg()
                                .font_weight(FontWeight::SEMIBOLD)
                                .child(self.surface.clone()),
                        )
                        .child(div().text_sm().text_color(muted_fg).child(
                            if matches!(self.surface.as_str(), "Settings" | "Help") {
                                description.to_string()
                            } else {
                                format!("{description} | recorded data")
                            },
                        )),
                ),
        );
        if self.surface == "Help" {
            body = body.child(
                Button::new("about")
                    .outline()
                    .label("About KernelLedger")
                    .on_click(cx.listener(|v, _, _, cx| {
                        v.confirmation = Some(("about".into(), json!({})));
                        cx.notify();
                    })),
            );
            body = body.child(
                TextView::markdown("help", include_str!("../../../../docs/desktop/help.md"))
                    .selectable(true),
            );
            body = body.child(
                Button::new("copy-about")
                    .outline()
                    .label("Copy build information")
                    .on_click(cx.listener(|v, _, _, cx| {
                        cx.write_to_clipboard(ClipboardItem::new_string(v.support_info()));
                    })),
            );
        }
        if matches!(self.surface.as_str(), "History" | "Checkpoints")
            && !self.tabs[self.active].parents.is_empty()
        {
            body = body.child(
                Button::new("back")
                    .outline()
                    .label("Back | Alt+Left")
                    .on_click(cx.listener(|v, _, _, cx| {
                        v.back();
                        cx.notify();
                    })),
            );
        }
        if self.surface == "Diagnostics"
            && let Some(detail) = self.tabs[self.active]
                .inspections
                .get("Technical error")
                .cloned()
        {
            body = body.child(
                Button::new("technical-details")
                    .ghost()
                    .label("Copy technical details")
                    .on_click(cx.listener(move |_, _, _, cx| {
                        cx.write_to_clipboard(ClipboardItem::new_string(detail.to_string()));
                    })),
            );
        }
        if self.surface == "Diagnostics" {
            let operations = self.tabs[self.active].runtime["active_operations"]
                .as_array()
                .cloned()
                .unwrap_or_default();
            let mut rows = card(cx);
            if operations.is_empty() {
                rows = rows.child(empty_row("No operations in flight", cx));
            }
            for (index, operation) in operations.into_iter().enumerate() {
                let oid = operation["operation_id"]
                    .as_str()
                    .unwrap_or_default()
                    .to_owned();
                let cancelable = operation["state"] == "accepted"
                    || operation["kind"] == "run"
                    || operation["kind"] == "batch";
                let state = operation["state"].as_str().unwrap_or("unknown").to_owned();
                rows =
                    rows.child(
                        row(cx)
                            .child(Icon::new(Glyph::Activity).small().text_color(muted_fg))
                            .child(div().font_weight(FontWeight::MEDIUM).child(
                                operation["kind"].as_str().unwrap_or("operation").to_owned(),
                            ))
                            .child(badge(state.clone(), tone(&state, cx)))
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .truncate()
                                    .font_family(mono.clone())
                                    .text_xs()
                                    .text_color(muted_fg)
                                    .child(
                                        operation["accepted"]
                                            .as_str()
                                            .map(relative_time)
                                            .unwrap_or_else(|| "this session".into()),
                                    ),
                            )
                            .child(
                                Button::new(("cancel-operation", index))
                                    .small()
                                    .outline()
                                    .label("Cancel operation")
                                    .disabled(!cancelable)
                                    .on_click(cx.listener(move |v, _, _, _| {
                                        v.runtime("operation.cancel", json!({"operation_id":oid}));
                                    })),
                            ),
                    );
            }
            let mut tools = h_flex().gap_2().flex_wrap();
            for (method, label, glyph) in [
                ("doctor", "Run doctor", Glyph::Stethoscope),
                (
                    "interpreter.candidates",
                    "Interpreter candidates",
                    Glyph::Cpu,
                ),
            ] {
                tools = tools.child(
                    Button::new(method)
                        .small()
                        .outline()
                        .icon(glyph)
                        .label(label)
                        .on_click(cx.listener(move |v, _, _, _| v.inspect(method, json!({})))),
                );
            }
            tools = tools
                .child(
                    Button::new("refresh")
                        .small()
                        .outline()
                        .icon(Glyph::RefreshCw)
                        .label("Reconnect / reload")
                        .on_click(cx.listener(|v, _, _, _| v.send(Command::Refresh))),
                )
                .child(
                    Button::new("migrate")
                        .small()
                        .outline()
                        .icon(Glyph::Database)
                        .label("Migrate ledger with backup")
                        .on_click(cx.listener(|v, _, _, cx| {
                            v.confirmation = Some(("migrate".into(), json!({})));
                            cx.notify();
                        })),
                );
            body = body
                .child(section("Active operations", cx).child(rows))
                .child(section("Tools", cx).child(tools));
        }
        if self.surface == "Settings" {
            let dark = self.dark;
            body = body.child(
                section("Preferences", cx).child(
                    card(cx)
                        .child(
                            row(cx)
                                .child(setting_label(
                                    "Appearance",
                                    if self.high_contrast {
                                        "High contrast theme"
                                    } else if dark {
                                        "Dark theme"
                                    } else {
                                        "Light theme"
                                    },
                                    cx,
                                ))
                                .child(
                                    Button::new("theme")
                                        .small()
                                        .outline()
                                        .icon(if dark { Glyph::Sun } else { Glyph::Moon })
                                        .label("Toggle light / dark")
                                        .on_click(cx.listener(|v, _, window, cx| {
                                            v.toggle_theme(window, cx)
                                        })),
                                ),
                        )
                        .child(
                            row(cx)
                                .child(setting_label(
                                    "Project interpreter",
                                    "CPython 3.11 or newer runs this notebook's worker",
                                    cx,
                                ))
                                .child(
                                    Button::new("interpreter")
                                        .small()
                                        .outline()
                                        .icon(Glyph::FolderOpen)
                                        .label("Choose project interpreter")
                                        .on_click(cx.listener(|v, _, window, cx| {
                                            v.choose_interpreter(window, cx)
                                        })),
                                ),
                        ),
                ),
            );
        }
        if self.surface == "Settings" {
            body = body.child(
                Button::new("high-contrast")
                    .outline()
                    .label("High contrast")
                    .on_click(cx.listener(|v, _, window, cx| {
                        v.follow_system = false;
                        v.high_contrast = true;
                        persist_appearance("contrast", cx);
                        apply_theme(v.dark, true, window, cx);
                        cx.notify();
                    })),
            );
        }
        if self.surface == "Settings" {
            body = body.child(
                Checkbox::new("system-theme")
                    .label("Follow system appearance")
                    .checked(self.follow_system)
                    .on_click(cx.listener(|v, checked: &bool, window, cx| {
                        v.follow_system = *checked;
                        v.high_contrast = *checked && system_high_contrast();
                        if *checked {
                            v.dark = matches!(
                                window.appearance(),
                                WindowAppearance::Dark | WindowAppearance::VibrantDark
                            );
                        }
                        persist_appearance(
                            if *checked {
                                "system"
                            } else if v.dark {
                                "dark"
                            } else {
                                "light"
                            },
                            cx,
                        );
                        apply_theme(v.dark, v.high_contrast, window, cx);
                        cx.notify();
                    })),
            );
            let interpreter = &self.tabs[self.active].runtime["interpreter"];
            let summary = format!(
                "Interpreter: {} | {}",
                interpreter["executable"]
                    .as_str()
                    .or(interpreter["python"].as_str())
                    .or(value["path"].as_str())
                    .unwrap_or("Not selected; choose a project interpreter"),
                interpreter["version"]
                    .as_str()
                    .unwrap_or("version available after start")
            );
            body = body.child(div().whitespace_normal().child(summary));
        }
        if self.surface == "Checkpoints" {
            body = body.child(h_flex().gap_2()
                .child(Button::new("namespace-list").ghost().label("Namespace checkpoints").on_click(cx.listener(|v, _, _, _| v.inspect("checkpoints.list", json!({})))))
                .child(Button::new("training-list").ghost().label("Training snapshots").on_click(cx.listener(|v, _, _, _| v.inspect("training.list", json!({})))))
                .child(div().whitespace_normal().child("Training snapshots save registered model/optimizer state. Namespace and native process state are excluded.")));
        }
        if value.get("snapshot").is_some() {
            let snapshot = value["snapshot"].as_str().unwrap_or_default().to_owned();
            let device = if value["requirements"]["device_count"].as_u64().unwrap_or(0) > 0 {
                "cuda:0"
            } else {
                "cpu"
            }
            .to_owned();
            body = body.child(
                section("Portable training coverage", cx)
                    .child(
                        div()
                            .whitespace_normal()
                            .child(bounded(&value["coverage"].to_string(), 4096)),
                    )
                    .child(div().whitespace_normal().child(format!(
                        "Source coherence: {}",
                        value["source_coherence"].as_str().unwrap_or("unverified")
                    )))
                    .child(
                        Button::new("restore-training")
                            .outline()
                            .label("Restore training in fresh worker")
                            .on_click(cx.listener(move |v, _, _, cx| {
                                v.confirmation = Some((
                                    "training-restore".into(),
                                    json!({"snapshot":snapshot,"device":device,"trust":false}),
                                ));
                                cx.notify();
                            })),
                    ),
            );
        }
        if value.get("native_qualified").is_some() && value.get("capabilities").is_some() {
            body = body.child(section("Native process snapshot availability", cx).child(
                div().whitespace_normal().child(bounded(
                    &value["capabilities"]["features"].to_string(),
                    8192,
                )),
            ));
        }
        if self.surface == "History" {
            body = body.child(Input::new(&self.history_query).cleanable(true));
            body = body.child(
                h_flex()
                    .gap_2()
                    .flex_wrap()
                    .child(
                        Button::new("search-executions")
                            .outline()
                            .label("Search executions")
                            .on_click(cx.listener(|v, _, _, _| {
                                v.inspect("executions.list", json!({"search_all":true}))
                            })),
                    )
                    .child(
                        Button::new("clear-history-filters")
                            .ghost()
                            .label("Clear all filters")
                            .on_click(cx.listener(|v, _, window, cx| {
                                v.history_query
                                    .update(cx, |s, cx| s.set_value("", window, cx));
                                cx.notify();
                            })),
                    ),
            );
        }
        if let Some(items) = value["items"].as_array() {
            let mut rows = card(cx);
            if items.is_empty() {
                rows = rows.child(empty_row("Nothing recorded yet", cx));
            }
            let history_filter = self.history_query.read(cx).value().to_lowercase();
            let items = items
                .iter()
                .filter(|item| {
                    self.surface != "History"
                        || history_matches(item, &history_filter, &self.tabs[self.active].model)
                })
                .collect::<Vec<_>>();
            self.record_active = self.record_active.min(items.len().saturating_sub(1));
            for (i, item) in items.into_iter().enumerate() {
                let status = item["status"]
                    .as_str()
                    .or(item["state"].as_str())
                    .unwrap_or("unknown");
                let status = if item.get("exec_id").is_none()
                    && item.get("session_id").is_some()
                    && status == "running"
                {
                    "Active"
                } else {
                    status
                }
                .to_owned();
                if let Some(snapshot) = item["snapshot"].as_str() {
                    let snapshot = snapshot.to_owned();
                    rows = rows.child(
                        record_row(
                            Glyph::Flag,
                            &format!("Training {}", &snapshot[..snapshot.len().min(12)]),
                            "Portable; native excluded",
                            cx,
                        )
                        .child(
                            Button::new(("training-inspect", i))
                                .small()
                                .label("Inspect")
                                .on_click(cx.listener(move |v, _, _, _| {
                                    v.inspect("training.inspect", json!({"snapshot":snapshot}))
                                })),
                        ),
                    );
                } else if let Some(eid) = item["exec_id"].as_str() {
                    let eid = eid.to_owned();
                    rows = rows.child(
                        record_row(
                            Glyph::Terminal,
                            &record_label(item, &self.tabs[self.active].model),
                            &status,
                            cx,
                        )
                        .when(i == self.record_active, |d| d.bg(cx.theme().list_active))
                        .child(
                            Button::new(("execution", i))
                                .small()
                                .ghost()
                                .label("View")
                                .on_click(cx.listener(move |v, _, _, _| {
                                    v.inspect("execution.get", json!({"exec_id":eid}))
                                })),
                        ),
                    );
                } else if let Some(id) = item["checkpoint_id"].as_str() {
                    let id = id.to_owned();
                    rows = rows.child(
                        record_row(
                            Glyph::Flag,
                            &record_label(item, &self.tabs[self.active].model),
                            &status,
                            cx,
                        )
                        .when(i == self.record_active, |d| d.bg(cx.theme().list_active))
                        .child(
                            Button::new(("report", i))
                                .small()
                                .ghost()
                                .label("Report")
                                .on_click(cx.listener({
                                    let id = id.clone();
                                    move |v, _, _, _| {
                                        v.inspect("checkpoint.get", json!({"checkpoint_id":id}))
                                    }
                                })),
                        )
                        .child(
                            Button::new(("continue", i))
                                .small()
                                .ghost()
                                .label("Continue")
                                .on_click(cx.listener({
                                    let id = id.clone();
                                    move |v, _, _, cx| {
                                        v.confirmation =
                                            Some(("continue".into(), json!({"checkpoint_id":id})));
                                        cx.notify();
                                    }
                                })),
                        )
                        .child(
                            Button::new(("restore", i))
                                .small()
                                .outline()
                                .label("Restore")
                                .on_click(cx.listener(move |v, _, _, cx| {
                                    v.confirmation =
                                        Some(("restore".into(), json!({"checkpoint_id":id})));
                                    cx.notify();
                                })),
                        ),
                    );
                } else if let Some(sid) = item["session_id"].as_str() {
                    let sid = sid.to_owned();
                    rows = rows.child(
                        record_row(
                            Glyph::Clock,
                            &record_label(item, &self.tabs[self.active].model),
                            &status,
                            cx,
                        )
                        .when(i == self.record_active, |d| d.bg(cx.theme().list_active))
                        .child(
                            Button::new(("session", i))
                                .small()
                                .ghost()
                                .label("Executions")
                                .on_click(cx.listener({
                                    let sid = sid.clone();
                                    move |v, _, _, _| {
                                        v.inspect("executions.list", json!({"session_id":sid}))
                                    }
                                })),
                        )
                        .child(
                            Button::new(("replay", i))
                                .small()
                                .outline()
                                .label("Replay session")
                                .on_click(cx.listener(move |v, _, _, cx| {
                                    v.confirmation =
                                        Some(("replay".into(), json!({"session_id":sid})));
                                    cx.notify();
                                })),
                        ),
                    );
                } else if let Some(path) = item["path"].as_str() {
                    let label = path.to_owned();
                    let path = PathBuf::from(path);
                    rows = rows.child(
                        row(cx)
                            .child(Icon::new(Glyph::Cpu).small().text_color(muted_fg))
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .truncate()
                                    .font_family(mono.clone())
                                    .text_sm()
                                    .child(label),
                            )
                            .child(
                                Button::new(("python", i))
                                    .small()
                                    .outline()
                                    .label("Use")
                                    .on_click(cx.listener(move |v, _, _, _| {
                                        v.send(Command::SelectInterpreter {
                                            path: path.clone(),
                                            remember: false,
                                        })
                                    })),
                            ),
                    );
                } else if let Some(label) = ["instance_id", "id", "name"]
                    .iter()
                    .find_map(|key| item[*key].as_str())
                {
                    let path = item["project_path"].as_str().map(PathBuf::from);
                    rows = rows.child(
                        row(cx).child(
                            Button::new(("runtime-stop", i))
                                .outline()
                                .label("Stop runtime")
                                .disabled(status != "verified")
                                .on_click(cx.listener(move |v, _, _, cx| {
                                    if let Some(path) = path.clone() {
                                        v.open(path, cx);
                                        v.confirmation = Some(("stop".into(), json!({})));
                                        cx.notify();
                                    }
                                })),
                        ),
                    );
                    let _ = label;
                    rows = rows.child(
                        record_row(
                            Glyph::Cpu,
                            &record_label(item, &self.tabs[self.active].model),
                            &status,
                            cx,
                        )
                        .when(i == self.record_active, |d| d.bg(cx.theme().list_active))
                        .child(
                            Button::new(("runtime-open", i))
                                .outline()
                                .label("Open notebook")
                                .disabled(item["project_path"].as_str().is_none())
                                .on_click(cx.listener({
                                    let path = item["project_path"].as_str().map(PathBuf::from);
                                    move |v, _, _, cx| {
                                        if let Some(path) = path.clone() {
                                            v.open(path, cx);
                                            v.surface = "Notebook".into();
                                        }
                                    }
                                })),
                        ),
                    );
                }
            }
            body = body.child(section("Records", cx).child(rows));
        }
        if (value.get("exec_id").is_some() || value.get("checkpoint_id").is_some())
            && let Some(time) = value["start"].as_str().or(value["timestamp"].as_str())
        {
            body = body.child(
                div()
                    .id("recorded-time")
                    .aria_label(format!(
                        "Recorded time: {}. Stored UTC: {time}",
                        local_time(time)
                    ))
                    .child(format!("Recorded: {}", local_time(time))),
            );
        }
        if value.get("exec_id").is_some() {
            body = body.child(
                section("Recorded execution", cx).child(
                    div()
                        .whitespace_normal()
                        .child(record_label(&value, &self.tabs[self.active].model)),
                ),
            );
            if let Some(source) = value["source"].as_str() {
                body = body.child(
                    TextView::markdown(
                        "recorded-source",
                        format!("```python\n{}\n```", bounded(source, 65536)),
                    )
                    .selectable(true),
                );
            }
            if let Some(outputs) = value["outputs"].as_array() {
                for (i, event) in outputs.iter().enumerate() {
                    body = body.child(output(event, format!("recorded-output-{i}"), cx));
                }
            }
        }
        if self.surface == "Diagnostics" && !value.is_null() {
            body = body.child(section("Doctor findings", cx).child(
                div().whitespace_normal().child(format!(
                    "Source: {} | {} findings",
                    value["source"]["state"].as_str().unwrap_or("unknown"),
                    value["issues"].as_array().map(Vec::len).unwrap_or(0)
                )),
            ));
        }
        let operations = &self.tabs[self.active].operations;
        if self.surface == "Diagnostics" && !operations.is_empty() {
            let mut rows = card(cx);
            for op in operations.values() {
                let state = op["state"].as_str().unwrap_or("unknown").to_owned();
                rows = rows.child(
                    row(cx)
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .truncate()
                                .font_family(mono.clone())
                                .text_xs()
                                .child(operation_label(op, &self.tabs[self.active].model)),
                        )
                        .child(badge(state.clone(), tone(&state, cx))),
                );
            }
            body = body.child(section("Operations this session", cx).child(rows));
        }
        if !value.is_null() {
            body = body.child(
                Button::new("raw-details")
                    .ghost()
                    .label(if self.tabs[self.active].raw_details {
                        "Hide raw details"
                    } else {
                        "Raw details"
                    })
                    .on_click(cx.listener(|v, _, _, cx| {
                        v.tabs[v.active].raw_details = !v.tabs[v.active].raw_details;
                        cx.notify();
                    })),
            );
        }
        if !value.is_null() && self.tabs[self.active].raw_details {
            body = body.child(
                section("Details", cx).child(
                    card(cx).child(
                        div().p_3().min_w_0().overflow_hidden().child(
                            TextView::markdown(
                                "details",
                                format!(
                                    "```json\n{}\n```",
                                    bounded(
                                        &serde_json::to_string_pretty(&value).unwrap_or_default(),
                                        65536
                                    )
                                ),
                            )
                            .selectable(true),
                        ),
                    ),
                ),
            );
        }
        div()
            .id(SharedString::from(format!("inspection-{}", self.surface)))
            .track_scroll(
                self.tabs[self.active]
                    .scrolls
                    .entry(self.surface.clone())
                    .or_default(),
            )
            .size_full()
            .overflow_y_scroll()
            .child(
                div()
                    .w_full()
                    .flex()
                    .justify_center()
                    .px_8()
                    .py_6()
                    .child(body),
            )
            .into_any_element()
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
    fn header(&self, cx: &mut Context<Self>) -> Div {
        let theme = cx.theme();
        let (border, bar, muted_fg, primary, primary_fg, background, ring, radius) = (
            theme.title_bar_border,
            theme.title_bar,
            theme.muted_foreground,
            theme.primary,
            theme.primary_foreground,
            theme.background,
            theme.ring,
            theme.radius,
        );
        let mut tabs = h_flex().gap_1().min_w_0().overflow_hidden();
        for (i, tab) in self.tabs.iter().enumerate() {
            tabs = tabs.child(
                Button::new(("tab", i))
                    .ghost()
                    .small()
                    .icon(Glyph::NotebookPen)
                    .label(
                        tab.model
                            .project
                            .file_name()
                            .map(|n| n.to_string_lossy().to_string())
                            .unwrap_or_else(|| "Notebook".into()),
                    )
                    .tooltip(tab.model.project.to_string_lossy().to_string())
                    .selected(i == self.active)
                    .on_click(cx.listener(move |v, _, _, cx| {
                        if let Some(tab) = v.tabs.get_mut(v.active) {
                            tab.selections.insert(v.surface.clone(), v.record_active);
                        }
                        v.active = i;
                        v.record_active =
                            v.tabs[i].selections.get(&v.surface).copied().unwrap_or(0);
                        cx.notify();
                    })),
            );
        }
        h_flex()
            .flex_none()
            .min_h(px(46.))
            .flex_wrap()
            .px_3()
            .gap_3()
            .items_center()
            .bg(bar)
            .border_b_1()
            .border_color(border)
            .child(
                h_flex()
                    .flex_none()
                    .gap_2()
                    .items_center()
                    .child(
                        div()
                            .size(px(24.))
                            .rounded(px(6.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .bg(primary)
                            .text_color(primary_fg)
                            .child(Icon::new(Glyph::BookOpen).small()),
                    )
                    .child(
                        div()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child("KernelLedger"),
                    ),
            )
            .when(!self.tabs.is_empty(), |d| d.child(vsep(border)))
            .child(tabs)
            .child(div().flex_1())
            .child(
                h_flex()
                    .id("palette-trigger")
                    .flex_none()
                    .h(px(28.))
                    .w(px(if self.compact { 150. } else { 240. }))
                    .px_2()
                    .gap_2()
                    .items_center()
                    .rounded(radius)
                    .border_1()
                    .border_color(border)
                    .bg(background)
                    .text_xs()
                    .text_color(muted_fg)
                    .cursor_pointer()
                    .hover(move |s| s.border_color(ring))
                    .child(Icon::new(Glyph::Search).xsmall())
                    .child(div().flex_1().child("Commands"))
                    .children(kbd("ctrl-shift-p"))
                    .on_click(cx.listener(|v, _, window, cx| v.palette(&Palette, window, cx))),
            )
            .child(
                Button::new("open")
                    .small()
                    .outline()
                    .icon(Glyph::FolderOpen)
                    .label("Open notebook")
                    .tooltip("Open a notebook directory · Ctrl+O")
                    .on_click(
                        cx.listener(|v, _, window, cx| v.open_dialog(&OpenNotebook, window, cx)),
                    ),
            )
            .child(
                Button::new("theme-toggle")
                    .small()
                    .ghost()
                    .icon(if self.dark { Glyph::Sun } else { Glyph::Moon })
                    .tooltip("Toggle light / dark")
                    .accessibility_label("Toggle light / dark")
                    .on_click(cx.listener(|v, _, window, cx| v.toggle_theme(window, cx))),
            )
    }
    fn toolbar(&self, cx: &mut Context<Self>) -> Div {
        let theme = cx.theme();
        let (border, muted_fg) = (theme.border, theme.muted_foreground);
        let state = self.tabs[self.active].runtime["worker_state"]
            .as_str()
            .map(str::to_owned);
        let state_color = state.as_deref().map(|s| tone(s, cx)).unwrap_or(muted_fg);
        let mut bar = h_flex()
            .flex_none()
            .min_h(px(42.))
            .flex_wrap()
            .px_3()
            .gap_1()
            .items_center()
            .border_b_1()
            .border_color(border)
            .child(
                Button::new("all")
                    .primary()
                    .small()
                    .icon(Glyph::Play)
                    .label("Run all")
                    .tooltip("Run every code cell · Ctrl+Shift+Enter")
                    .on_click(cx.listener(|v, _, _, _| v.batch("all"))),
            );
        for (scope, label, glyph) in [
            ("above", "Run above", Glyph::ChevronsUp),
            ("below", "Run below", Glyph::ChevronsDown),
        ] {
            if self.compact {
                continue;
            }
            bar = bar.child(
                Button::new(scope)
                    .ghost()
                    .small()
                    .icon(glyph)
                    .label(label)
                    .on_click(cx.listener(move |v, _, _, _| v.batch(scope))),
            );
        }
        bar = bar.child(vsep(border));
        for (method, label, glyph, tooltip) in [
            (
                "interrupt",
                "Interrupt",
                Glyph::Square,
                "Interrupt the running cell",
            ),
            (
                "interrupt_kill",
                "Terminate worker",
                Glyph::OctagonX,
                "Kill the worker process",
            ),
            (
                "checkpoint",
                "Checkpoint",
                Glyph::BookmarkPlus,
                "Snapshot the live namespace",
            ),
            (
                "training-checkpoint",
                "Training checkpoint",
                Glyph::BookmarkPlus,
                "Save explicitly registered model and optimizer; excludes namespace/native state",
            ),
            (
                "restart",
                "Restart empty",
                Glyph::RotateCcw,
                "Restart with an empty namespace",
            ),
        ] {
            if self.compact && method != "interrupt" {
                continue;
            }
            if method == "checkpoint" {
                bar = bar.child(vsep(border));
            }
            bar = bar.child(
                Button::new(method)
                    .ghost()
                    .small()
                    .icon(glyph)
                    .label(label)
                    .disabled(!runtime_action_available(
                        method,
                        &self.tabs[self.active].runtime,
                        &self.surface,
                    ))
                    .tooltip(
                        if runtime_action_available(
                            method,
                            &self.tabs[self.active].runtime,
                            &self.surface,
                        ) {
                            tooltip
                        } else {
                            "Open the active notebook and start its runtime by running a cell"
                        },
                    )
                    .on_click(cx.listener(move |v, _, _, cx| {
                        if method == "restart" {
                            v.confirmation = Some((method.into(), json!({})));
                            cx.notify();
                        } else {
                            v.runtime(method, json!({}));
                        }
                    })),
            );
        }
        bar = bar.child(
            Button::new("more")
                .ghost()
                .small()
                .label("More...")
                .on_click(cx.listener(|v, _, window, cx| v.palette(&Palette, window, cx))),
        );
        bar.child(div().flex_1()).child(
            h_flex()
                .flex_none()
                .gap_2()
                .px_2p5()
                .h(px(24.))
                .items_center()
                .rounded_full()
                .bg(state_color.opacity(0.1))
                .text_xs()
                .font_weight(FontWeight::MEDIUM)
                .text_color(state_color)
                .child(dot(state_color))
                .child(worker_label(state.as_deref())),
        )
    }
    fn rail(&self, cx: &mut Context<Self>) -> Div {
        let theme = cx.theme();
        let (border, bar, _muted_fg, _fg, _primary, _active_bg, _hover_bg, _radius) = (
            theme.sidebar_border,
            theme.sidebar,
            theme.muted_foreground,
            theme.foreground,
            theme.primary,
            theme.list_active,
            theme.list_hover,
            theme.radius,
        );
        let mut rail = v_flex()
            .flex_none()
            .w(px(144.))
            .h_full()
            .px_1p5()
            .py_2()
            .gap_1()
            .bg(bar)
            .border_r_1()
            .border_color(border);
        for (surface, glyph, _) in SURFACES {
            let active = self.surface == surface;
            rail = rail.child(
                Button::new(surface)
                    .ghost()
                    .small()
                    .w_full()
                    .icon(glyph)
                    .label(surface)
                    .selected(active)
                    .accessibility_label(format!(
                        "{surface}{}",
                        if active { ", current destination" } else { "" }
                    ))
                    .on_click(cx.listener(move |v, _, window, cx| {
                        v.select_surface(surface, cx);
                        v.focus.focus(window, cx);
                    })),
            );
        }
        rail
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
                                        "{}{id}",
                                        cell.ordinal.map(|n| format!("[{n}] ")).unwrap_or_default()
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
            .child(icon_tile(Glyph::BookOpen, 60., cx))
            .child(
                v_flex()
                    .items_center()
                    .gap_2()
                    .child(
                        div()
                            .text_2xl()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child("Source you can edit. History you can trust."),
                    )
                    .child(
                        div()
                            .text_color(muted_fg)
                            .child("Open a notebook directory to begin. Opening starts no worker."),
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
                    .child(worker_label(state)),
            )
            .child(div().flex_1())
            .child(
                h_flex()
                    .gap_1p5()
                    .items_center()
                    .child(Icon::new(Glyph::Info).xsmall())
                    .child("Closing detaches; computations continue"),
            )
    }
    fn palette_overlay(&self, cx: &mut Context<Self>) -> Stateful<Div> {
        let theme = cx.theme();
        let (border, popover, popover_fg, muted_fg, hover_bg, overlay, radius) = (
            theme.border,
            theme.popover,
            theme.popover_foreground,
            theme.muted_foreground,
            theme.list_hover,
            theme.overlay,
            theme.radius_lg,
        );
        let query = self.palette_query.read(cx).value().to_lowercase();
        let mut items = v_flex().p_1().gap_px();
        let mut shown = 0;
        for (method, label, description, glyph) in COMMANDS {
            if !query.is_empty()
                && !label.to_lowercase().contains(&query)
                && !description.to_lowercase().contains(&query)
            {
                continue;
            }
            shown += 1;
            let available = match method {
                "stop" | "restart" | "interrupt" | "checkpoint" | "training-checkpoint" => self
                    .tabs
                    .get(self.active)
                    .is_some_and(|t| runtime_action_available(method, &t.runtime, &self.surface)),
                "run-all" | "above" | "below" => {
                    !self.tabs.is_empty() && self.surface == "Notebook"
                }
                _ => true,
            };
            let description = if available {
                description
            } else {
                "Unavailable: open the notebook and start its runtime"
            };
            items = items.child(
                h_flex()
                    .id(SharedString::from(format!("cmd-{method}")))
                    .gap_3()
                    .px_3()
                    .py_2()
                    .items_center()
                    .rounded(px(6.))
                    .cursor_pointer()
                    .when(shown - 1 == self.palette_active, |d| d.bg(hover_bg))
                    .hover(move |s| s.bg(hover_bg))
                    .child(div().text_color(muted_fg).child(Icon::new(glyph)))
                    .child(
                        v_flex()
                            .flex_1()
                            .child(div().text_sm().font_weight(FontWeight::MEDIUM).child(label))
                            .child(div().text_xs().text_color(muted_fg).child(description)),
                    )
                    .on_click(
                        cx.listener(move |v, _, window, cx| v.run_command(method, window, cx)),
                    ),
            );
        }
        if shown == 0 {
            items = items.child(
                div()
                    .px_3()
                    .py_6()
                    .text_center()
                    .text_sm()
                    .text_color(muted_fg)
                    .child("No matching commands"),
            );
        }
        div()
            .id("palette-backdrop")
            .occlude()
            .absolute()
            .top_0()
            .left_0()
            .size_full()
            .flex()
            .justify_center()
            .pt(px(88.))
            .bg(overlay)
            .on_click(cx.listener(|v, _, window, cx| {
                v.palette = false;
                v.focus.focus(window, cx);
                cx.notify();
            }))
            .child(
                v_flex()
                    .id("palette")
                    .occlude()
                    .w(px(560.))
                    .h_auto()
                    .max_h(px(440.))
                    .rounded(radius)
                    .border_1()
                    .border_color(border)
                    .bg(popover)
                    .text_color(popover_fg)
                    .shadow_xl()
                    .overflow_hidden()
                    .child(
                        div().p_2().border_b_1().border_color(border).child(
                            Input::new(&self.palette_query)
                                .prefix(Icon::new(Glyph::Search).small().text_color(muted_fg))
                                .appearance(false),
                        ),
                    )
                    .child(items)
                    .child(
                        h_flex()
                            .px_3()
                            .py_1p5()
                            .gap_1p5()
                            .items_center()
                            .border_t_1()
                            .border_color(border)
                            .text_xs()
                            .text_color(muted_fg)
                            .children(kbd("escape"))
                            .child("to close"),
                    ),
            )
    }
    fn confirm_overlay(
        &self,
        method: String,
        fields: Value,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let theme = cx.theme();
        let (border, popover, popover_fg, muted, muted_fg, overlay, warning, radius) = (
            theme.border,
            theme.popover,
            theme.popover_foreground,
            theme.muted,
            theme.muted_foreground,
            theme.overlay,
            theme.warning,
            theme.radius_lg,
        );
        let mono = theme.mono_font_family.clone();
        let clients = self
            .tabs
            .get(self.active)
            .map(|t| t.runtime["attached_client_count"].clone())
            .unwrap_or(Value::Null);
        if method == "about" {
            return div()
                .id("about-backdrop")
                .occlude()
                .absolute()
                .top_0()
                .left_0()
                .size_full()
                .flex()
                .items_center()
                .justify_center()
                .bg(overlay)
                .child(
                    v_flex()
                        .id("about-dialog")
                        .role(Role::Dialog)
                        .aria_label("About KernelLedger")
                        .track_focus(&self.dialog_focus)
                        .w(px(480.))
                        .max_w_full()
                        .min_w_0()
                        .whitespace_normal()
                        .p_5()
                        .gap_3()
                        .bg(popover)
                        .text_color(popover_fg)
                        .rounded(radius)
                        .child(div().text_lg().child("About KernelLedger"))
                        .child(self.support_info())
                        .child(
                            Button::new("about-copy")
                                .outline()
                                .label("Copy support information")
                                .on_click(cx.listener(|v, _, _, cx| {
                                    cx.write_to_clipboard(ClipboardItem::new_string(
                                        v.support_info(),
                                    ))
                                })),
                        )
                        .child(
                            div()
                                .id("about-close-focus")
                                .role(Role::Button)
                                .aria_label("Close About")
                                .track_focus(&self.confirm_focus)
                                .child(
                                    Button::new("about-close")
                                        .primary()
                                        .label("Close")
                                        .on_click(cx.listener(|v, _, _, cx| {
                                            v.confirmation = None;
                                            cx.notify();
                                        })),
                                ),
                        ),
                );
        }
        let (title, consequence, verb) = confirmation_copy(&method);
        let target = self.target_summary(&fields);

        let mut content = v_flex()
            .gap_3()
            .child(
                h_flex()
                    .gap_3()
                    .items_start()
                    .child(
                        div()
                            .flex_none()
                            .size(px(36.))
                            .rounded_full()
                            .flex()
                            .items_center()
                            .justify_center()
                            .bg(warning.opacity(0.12))
                            .text_color(warning)
                            .child(Icon::new(Glyph::TriangleAlert)),
                    )
                    .child(
                        v_flex()
                            .flex_1()
                            .gap_1()
                            .min_w_0()
                            .whitespace_normal()
                            .child(
                                div()
                                    .text_base()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child(format!("{title} {target}?")),
                            )
                            .child(div().text_sm().text_color(muted_fg).child(consequence)),
                    ),
            )
            .child(
                v_flex()
                    .gap_1()
                    .p_3()
                    .rounded(px(6.))
                    .bg(muted.opacity(0.5))
                    .text_xs()
                    .child(
                        h_flex()
                            .gap_2()
                            .child(
                                div()
                                    .w(px(110.))
                                    .text_color(muted_fg)
                                    .child("Attached clients"),
                            )
                            .child(div().font_family(mono.clone()).child(clients.to_string())),
                    )
                    .child(
                        h_flex()
                            .gap_2()
                            .child(div().w(px(110.)).text_color(muted_fg).child("Target"))
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .font_family(mono.clone())
                                    .whitespace_normal()
                                    .child(target),
                            ),
                    ),
            );
        if matches!(
            method.as_str(),
            "restore" | "continue" | "replay" | "training-restore"
        ) {
            content = content.child(
                Checkbox::new("artifact-trust")
                    .label("Trust imported saved code")
                    .checked(fields["trust"] == true)
                    .on_click(cx.listener(|v, checked: &bool, _, cx| {
                        if let Some((_, fields)) = &mut v.confirmation {
                            fields["trust"] = json!(*checked);
                        }
                        cx.notify();
                    })),
            );
        }
        if matches!(method.as_str(), "restore" | "continue") {
            content = content
                .child(
                    Checkbox::new("python-mismatch")
                        .label("Override a Python version mismatch")
                        .checked(fields["allow_python_mismatch"] == true)
                        .on_click(cx.listener(|v, checked: &bool, _, cx| {
                            if let Some((_, fields)) = &mut v.confirmation {
                                fields["allow_python_mismatch"] = json!(*checked);
                            }
                            cx.notify();
                        })),
                )
                .child(div().text_xs().text_color(muted_fg).child(
                    "Review the checkpoint report before overriding a Python version mismatch.",
                ));
        }
        let destructive = matches!(method.as_str(), "restart" | "stop" | "migrate" | "restore");
        let proceed = Button::new("proceed")
            .label(verb)
            .disabled(method == "training-restore" && fields["trust"] != true);
        let proceed = if destructive {
            proceed.danger()
        } else {
            proceed.primary()
        };
        content = content.child(
            h_flex()
                .pt_2()
                .gap_2()
                .justify_end()
                .child(
                    div()
                        .id("safe-cancel")
                        .role(Role::Button)
                        .aria_label("Cancel")
                        .track_focus(&self.confirm_focus)
                        .child(Button::new("cancel").ghost().label("Cancel").on_click(
                            cx.listener(|v, _, _, cx| {
                                v.confirmation = None;
                                cx.notify();
                            }),
                        )),
                )
                .child(proceed.on_click(cx.listener(move |v, _, _, cx| {
                    if method == "delete" {
                        v.mutate(json!({"action":"delete","cell_id":fields["cell_id"]}));
                    } else {
                        v.runtime(&method, fields.clone());
                    }
                    v.confirmation = None;
                    cx.notify();
                }))),
        );
        div()
            .id("confirm-backdrop")
            .occlude()
            .absolute()
            .top_0()
            .left_0()
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .bg(overlay)
            .on_click(cx.listener(|v, _, _, cx| {
                v.confirmation = None;
                cx.notify();
            }))
            .child(
                div()
                    .id("confirm")
                    .role(Role::Dialog)
                    .aria_label(title)
                    .track_focus(&self.dialog_focus)
                    .occlude()
                    .w(px(480.))
                    .max_w_full()
                    .min_w_0()
                    .whitespace_normal()
                    .p_5()
                    .rounded(radius)
                    .border_1()
                    .border_color(border)
                    .bg(popover)
                    .text_color(popover_fg)
                    .shadow_xl()
                    .child(content),
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
            .text_sm()
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
                            TextView::markdown(
                                "welcome-help",
                                include_str!("../../../../docs/desktop/help.md"),
                            )
                            .selectable(true),
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
                .child(self.toolbar(cx))
                .child(self.banners(window, cx))
                .child(
                    h_flex().flex_1().min_h_0().child(self.rail(cx)).child(
                        h_resizable("layout")
                            .when(!self.compact || self.sidebar_visible, |d| {
                                d.child(resizable_panel().size(px(260.)).child(self.sidebar(cx)))
                            })
                            .child(resizable_panel().child(center)),
                    ),
                )
                .child(self.status_bar(cx));
        }
        if self.palette {
            root = root.child(self.palette_overlay(cx));
        }
        if let Some((method, fields)) = self.confirmation.clone() {
            root = root.child(self.confirm_overlay(method, fields, cx));
        }
        root.into_any_element()
    }
}
fn apply_theme(dark: bool, high_contrast: bool, window: &mut Window, cx: &mut App) {
    Theme::change(
        if dark || high_contrast {
            ThemeMode::Dark
        } else {
            ThemeMode::Light
        },
        Some(window),
        cx,
    );
    let accent = if dark {
        hsla(232. / 360., 0.86, 0.70, 1.)
    } else {
        hsla(232. / 360., 0.68, 0.54, 1.)
    };
    let on_accent = if dark {
        hsla(232. / 360., 0.45, 0.10, 1.)
    } else {
        hsla(0., 0., 1., 1.)
    };
    let shade = |delta: f32| Hsla {
        l: (accent.l + delta).clamp(0., 1.),
        ..accent
    };
    let theme = Theme::global_mut(cx);
    let colors = &mut theme.colors;
    colors.primary = accent;
    colors.primary_hover = shade(-0.05);
    colors.primary_active = shade(-0.10);
    colors.primary_foreground = on_accent;
    colors.button_primary = accent;
    colors.button_primary_hover = shade(-0.05);
    colors.button_primary_active = shade(-0.10);
    colors.button_primary_foreground = on_accent;
    colors.ring = accent;
    colors.link = accent;
    colors.list_active = accent.opacity(0.12);
    colors.list_active_border = accent;
    colors.sidebar_primary = accent;
    colors.sidebar_primary_foreground = on_accent;
    colors.overlay = hsla(0., 0., 0., if dark { 0.5 } else { 0.22 });
    if high_contrast {
        let black = hsla(0., 0., 0., 1.);
        let white = hsla(0., 0., 1., 1.);
        let yellow = hsla(1. / 6., 1., 0.5, 1.);
        colors.background = black;
        colors.foreground = white;
        colors.muted = black;
        colors.muted_foreground = white;
        colors.popover = black;
        colors.popover_foreground = white;
        colors.sidebar = black;
        colors.sidebar_foreground = white;
        colors.border = white;
        colors.sidebar_border = white;
        colors.primary = yellow;
        colors.primary_foreground = black;
        colors.button_primary = yellow;
        colors.button_primary_foreground = black;
        colors.ring = yellow;
        colors.link = yellow;
        colors.list_active = hsla(0., 0., 0.2, 1.);
        colors.list_active_border = yellow;
        colors.danger = hsla(0., 1., 0.75, 1.);
        colors.warning = yellow;
        colors.info = hsla(0.5, 1., 0.75, 1.);
    }
    theme.tokens = ThemeTokens::from(&theme.colors);
    window.refresh();
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
fn editor_height(text: &str) -> Pixels {
    px(editor_lines(text) as f32 * 20. + 12.)
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
        None => "Offline inspection".into(),
        Some("absent") => "No worker".into(),
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
fn badge(label: impl Into<SharedString>, color: Hsla) -> Div {
    h_flex()
        .flex_none()
        .h(px(18.))
        .px(px(6.))
        .items_center()
        .rounded(px(4.))
        .bg(color.opacity(0.12))
        .text_color(color)
        .text_size(px(11.))
        .font_weight(FontWeight::MEDIUM)
        .child(label.into())
}
fn dot(color: Hsla) -> Div {
    div().flex_none().size(px(7.)).rounded_full().bg(color)
}
fn vsep(color: Hsla) -> Div {
    div().flex_none().w_px().h(px(18.)).mx_1().bg(color)
}
fn icon_tile(glyph: Glyph, size: f32, cx: &App) -> Div {
    let primary = cx.theme().primary;
    div()
        .flex_none()
        .size(px(size))
        .rounded(px(size / 4.))
        .flex()
        .items_center()
        .justify_center()
        .bg(primary.opacity(0.1))
        .text_color(primary)
        .child(Icon::new(glyph).with_size(px(size / 2.)))
}
fn banner(color: Hsla, glyph: Glyph) -> Div {
    h_flex()
        .w_full()
        .gap_3()
        .px_4()
        .py_2()
        .items_center()
        .bg(color.opacity(0.08))
        .border_b_1()
        .border_color(color.opacity(0.25))
        .child(
            div()
                .flex_none()
                .text_color(color)
                .child(Icon::new(glyph).small()),
        )
}
fn section(title: &str, cx: &App) -> Div {
    v_flex().gap_2().child(
        div()
            .text_xs()
            .font_weight(FontWeight::SEMIBOLD)
            .text_color(cx.theme().muted_foreground)
            .child(title.to_uppercase()),
    )
}
fn card(cx: &App) -> Div {
    let theme = cx.theme();
    v_flex()
        .rounded(theme.radius_lg)
        .border_1()
        .border_color(theme.border)
        .bg(theme.background)
        .overflow_hidden()
}
fn row(cx: &App) -> Div {
    h_flex()
        .flex_wrap()
        .min_w_0()
        .gap_3()
        .px_4()
        .py_2p5()
        .items_center()
        .border_b_1()
        .border_color(cx.theme().border)
}
fn empty_row(text: &str, cx: &App) -> Div {
    div()
        .px_4()
        .py_4()
        .text_sm()
        .text_color(cx.theme().muted_foreground)
        .child(text.to_string())
}
fn record_row(glyph: Glyph, id: &str, status: &str, cx: &App) -> Div {
    let theme = cx.theme();
    row(cx)
        .child(Icon::new(glyph).small().text_color(theme.muted_foreground))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .truncate()
                .whitespace_normal()
                .text_sm()
                .child(id.to_string()),
        )
        .child(badge(status.to_string(), tone(status, cx)))
}
fn setting_label(title: &str, description: &str, cx: &App) -> Div {
    v_flex()
        .flex_1()
        .min_w_0()
        .whitespace_normal()
        .gap_0p5()
        .child(
            div()
                .font_weight(FontWeight::MEDIUM)
                .child(title.to_string()),
        )
        .child(
            div()
                .text_xs()
                .text_color(cx.theme().muted_foreground)
                .child(description.to_string()),
        )
}
fn safe_link(url: &SharedString, _: &ClickEvent, _: &mut Window, cx: &mut App) {
    if url.starts_with("https://") || url.starts_with("http://") {
        cx.open_url(url);
    }
}
fn output(event: &Value, id: String, cx: &App) -> AnyElement {
    let label = event["text"]
        .as_str()
        .or(event["evalue"].as_str())
        .map(|text| bounded(text, 65536))
        .unwrap_or_else(|| {
            format!(
                "Recorded {} output",
                event["kind"].as_str().unwrap_or("rich")
            )
        });
    div()
        .id(SharedString::from(format!("output-{id}")))
        .role(Role::Log)
        .aria_label(label)
        .min_w_0()
        .whitespace_normal()
        .child(output_inner(event, id, cx))
        .into_any_element()
}
fn output_inner(event: &Value, id: String, cx: &App) -> AnyElement {
    let theme = cx.theme();
    let mono = theme.mono_font_family.clone();
    if let Some(error) = event["viewer_error"].as_str() {
        return div()
            .text_xs()
            .text_color(theme.muted_foreground)
            .child(format!("Output unavailable: {error}"))
            .into_any_element();
    }
    let kind = event["kind"].as_str().unwrap_or("unknown");
    if matches!(kind, "stdout" | "stderr") {
        return TextView::markdown(
            SharedString::from(id),
            format!(
                "```text\n{}\n```",
                bounded(event["text"].as_str().unwrap_or("Blob output"), 65536)
                    .replace("```", "~~~")
            ),
        )
        .selectable(true)
        .into_any_element();
    }
    if kind == "error" {
        let danger = theme.danger;
        return v_flex()
            .gap_1()
            .p_3()
            .rounded(px(6.))
            .border_1()
            .border_color(danger.opacity(0.3))
            .bg(danger.opacity(0.06))
            .text_color(danger)
            .child(
                h_flex()
                    .gap_1p5()
                    .items_center()
                    .text_xs()
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(Icon::new(Glyph::CircleAlert).xsmall())
                    .child(event["ename"].as_str().unwrap_or("Error").to_string()),
            )
            .child(
                div().font_family(mono).text_xs().child(
                    event["traceback"]
                        .as_array()
                        .map(|a| a.iter().filter_map(Value::as_str).collect::<String>())
                        .unwrap_or_else(|| event.to_string()),
                ),
            )
            .into_any_element();
    }
    let bundle = &event["bundle"];
    for mime in ["image/png", "image/jpeg"] {
        if let Some(path) = bundle[mime]["local_path"].as_str() {
            return img(PathBuf::from(path))
                .max_w_full()
                .max_h(px(500.))
                .rounded(px(6.))
                .into_any_element();
        }
    }
    if let Some(rows) = bundle["text/html"]["rows"].as_array() {
        let (border, muted) = (theme.border, theme.muted);
        let mut table = v_flex()
            .id(SharedString::from(id))
            .max_h(px(400.))
            .overflow_y_scroll()
            .rounded(px(6.))
            .border_1()
            .border_color(border)
            .bg(theme.background)
            .text_xs();
        for (index, row) in rows.iter().enumerate() {
            let mut line = h_flex()
                .border_b_1()
                .border_color(border)
                .when(index == 0, |d| {
                    d.bg(muted).font_weight(FontWeight::SEMIBOLD)
                });
            if let Some(values) = row.as_array() {
                for value in values {
                    line = line.child(
                        div()
                            .min_w(px(100.))
                            .px_3()
                            .py_1p5()
                            .child(value.as_str().unwrap_or_default().to_string()),
                    );
                }
            }
            table = table.child(line);
        }
        return table.into_any_element();
    }
    if let Some(text) = bundle["text/markdown"]["value"].as_str() {
        return TextView::markdown(SharedString::from(id), safe_markdown(text))
            .on_link_click(safe_link)
            .into_any_element();
    }
    if !bundle["application/json"]["value"].is_null() {
        return div()
            .font_family(mono)
            .text_xs()
            .child(bounded(
                &serde_json::to_string_pretty(&bundle["application/json"]["value"])
                    .unwrap_or_default(),
                65536,
            ))
            .into_any_element();
    }
    for mime in ["text/plain", "text/latex"] {
        if let Some(text) = bundle[mime]["value"].as_str() {
            return div()
                .font_family(mono)
                .text_xs()
                .child(bounded(text, 65536))
                .into_any_element();
        }
    }
    div()
        .text_xs()
        .text_color(theme.muted_foreground)
        .child(format!(
            "{kind}: native preview unavailable; recorded data retained"
        ))
        .into_any_element()
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

fn runtime_action_available(method: &str, runtime: &Value, surface: &str) -> bool {
    let state = runtime["worker_state"].as_str();
    surface == "Notebook"
        && match method {
            "training-checkpoint" => {
                state == Some("idle")
                    && runtime["capabilities"]
                        .as_array()
                        .is_some_and(|values| values.iter().any(|v| v == "portable_training"))
            }
            "interrupt" | "interrupt_kill" => {
                matches!(state, Some("running" | "waiting_input" | "interrupting"))
            }
            "stop" | "restart" | "checkpoint" => {
                matches!(state, Some("idle" | "running" | "waiting_input"))
            }
            _ => true,
        }
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
                "from" => item["start"]
                    .as_str()
                    .or(item["timestamp"].as_str())
                    .is_some_and(|s| s >= value),
                "to" => item["start"]
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
