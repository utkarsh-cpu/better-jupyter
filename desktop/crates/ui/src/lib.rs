mod accessibility;
mod checkpoints;
mod components;
mod diagnostics;
mod help;
mod history;
mod images;
mod interchange;
mod notebook;
mod outputs;
mod runtime;
mod settings;
mod shell;
mod shortcuts;
use shortcuts::command_shortcut;
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
const COMMANDS: [(&str, &str, &str, Glyph); 38] = [
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
        "Delete cell...",
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
    raw_details: BTreeMap<String, bool>,
    reveal: Option<String>,
    uninitialized: bool,
    selections: BTreeMap<String, usize>,
    drafts: Vec<Value>,
    history_filter: String,
    checkpoint_notice: Option<String>,
    error: Option<String>,
    attachment_pages: BTreeMap<String, Value>,
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
    palette_scroll: UniformListScrollHandle,
    return_focus: Option<FocusHandle>,
    sidebar_visible: bool,
    app_menu: bool,
    notebook_menu: bool,
    export_menu: bool,
    history_owner: Option<usize>,
    runtime_menu: bool,
    cell_menu: Option<(String, usize)>,
    insert_menu: Option<usize>,
    menu_anchor: String,
    menu_bounds: BTreeMap<String, Bounds<Pixels>>,
    preferences: kernelledger_client::preferences::Preferences,
    preference_status: Option<String>,
    preference_saving: bool,
    compact: bool,
    outline_drawer: bool,
    follow_system: bool,
    history_query: Entity<InputState>,
    history_controls: BTreeMap<String, Entity<InputState>>,
    history_field_subscriptions: Vec<Subscription>,
    history_fields_key: String,
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
        let loaded_preferences = if cfg!(test) {
            Ok(kernelledger_client::preferences::Preferences::default())
        } else {
            kernelledger_client::preferences::Preferences::load(&kernelledger_client::profile_path())
        };
        let preference_status = loaded_preferences
            .as_ref()
            .err()
            .map(|e| format!("Preferences could not be loaded: {e}"));
        let mut preferences = loaded_preferences.unwrap_or_default();
        if !cfg!(test) {
            let profile = kernelledger_client::profile_path();
            if saved_reduced_motion(&profile).is_none() {
                preferences.reduced_motion = system_reduced_motion();
            }
        }
        let mut view = Self {
            tabs: vec![],
            active: 0,
            focus: cx.focus_handle(),
            surface: "Notebook".into(),
            palette: false,
            query: cx.new(|cx| InputState::new(window, cx).placeholder("Filter cells")),
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
            palette_scroll: UniformListScrollHandle::default(),
            return_focus: None,
            sidebar_visible: preferences.outline_visible,
            app_menu: false,
            notebook_menu: false,
            export_menu: false,
            history_owner: None,
            runtime_menu: false,
            cell_menu: None,
            insert_menu: None,
            menu_anchor: String::new(),
            menu_bounds: BTreeMap::new(),
            preferences,
            preference_status,
            preference_saving: false,
            compact: false,
            outline_drawer: false,
            follow_system,
            history_query: cx.new(|cx| {
                InputState::new(window, cx).placeholder(
                    "Search recorded source; advanced filters: cell:, session:, after:, before:",
                )
            }),
            history_controls: BTreeMap::new(),
            history_field_subscriptions: Vec::new(),
            history_fields_key: String::new(),
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
            raw_details: BTreeMap::new(),
            reveal: None,
            uninitialized: false,
            selections: BTreeMap::new(),
            drafts: vec![],
            history_filter: String::new(),
            checkpoint_notice: None,
            error: None,
            attachment_pages: BTreeMap::new(),
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
                        if value["state"] == "completed"
                            && matches!(
                                value["kind"].as_str().or(value["method"].as_str()),
                                Some("restore" | "continue" | "replay" | "training-restore")
                            )
                            && value["result"]["session_id"].is_string()
                        {
                            tab.inspections
                                .insert("Restoration result".into(), value.clone());
                        }
                        if value["state"] == "failed" {
                            let error = &value["result"]["error"];
                            let code = error["code"].as_str().unwrap_or("operation_failed");
                            tab.error = Some(error_message(code).into());
                            tab.inspections
                                .insert("Technical error".into(), error.clone());
                        }
                        if value["state"] == "completed"
                            && let Some(id) = value["result"]["checkpoint_id"].as_str()
                        {
                            tab.checkpoint_notice = Some(id.to_owned());
                            let _ = tab.service.commands.send(Command::Inspect {
                                method: "checkpoints.list".into(),
                                fields: json!({}),
                            });
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
                        if method == "bundle.inspect" {
                            let path = tab
                                .inspections
                                .get("Bundle review")
                                .and_then(|review| review.get("path"))
                                .filter(|path| path.is_string())
                                .cloned();
                            let mut review = value;
                            if review.get("path").is_none()
                                && let Some(path) = path
                            {
                                review["path"] = path;
                            }
                            review["pending"] = json!(false);
                            tab.inspections.insert("Bundle review".into(), review);
                            self.surface = "Diagnostics".into();
                            continue;
                        }
                        if matches!(
                            method.as_str(),
                            "export_ipynb" | "bundle_export" | "export_document"
                        ) {
                            tab.inspections
                                .insert("Export result".into(), value.clone());
                            continue;
                        }
                        let surface = inspection_surface(&method).to_string();
                        if (matches!(
                            method.as_str(),
                            "executions.list"
                                | "execution.get"
                                | "checkpoint.get"
                                | "training.inspect"
                        ) || (matches!(surface.as_str(), "History" | "Checkpoints")
                            && !value["_page_request"]["cursor"].is_null()))
                            && let Some(previous) = tab.inspections.get(&surface)
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
                        tab.inspections.insert(surface.clone(), value);
                        tab.raw_details.insert(surface, false);
                    }
                    Event::OutputPage {
                        cell_id,
                        exec_id,
                        index,
                        slot,
                        text,
                        offset,
                        end,
                        total,
                    } => {
                        if let Some(cell) = tab.model.cells.get_mut(&cell_id)
                            && let Some(event) = cell.output.get_mut(index)
                        {
                            kernelledger_client::output::write_page(
                                event, &slot, &text, offset, end, total,
                            );
                            if let Some(position) =
                                tab.model.order.iter().position(|id| id == &cell_id)
                            {
                                tab.list.splice(position..position + 1, 1);
                            }
                        }
                        if let Some(history) = tab.inspections.get_mut("History")
                            && history["exec_id"] == exec_id
                            && let Some(event) = history["outputs"].get_mut(index)
                        {
                            kernelledger_client::output::write_page(
                                event, &slot, &text, offset, end, total,
                            );
                        }
                    }
                    Event::ImageCached {
                        cell_id,
                        exec_id,
                        index,
                        mime,
                        path,
                    } => {
                        let assign = |event: &mut Value| {
                            if event["bundle"][&mime].is_object() {
                                event["bundle"][&mime]["local_path"] = json!(path.clone());
                            }
                        };
                        if let Some(cell) = tab.model.cells.get_mut(&cell_id)
                            && let Some(event) = cell.output.get_mut(index)
                        {
                            assign(event);
                            if let Some(position) =
                                tab.model.order.iter().position(|id| id == &cell_id)
                            {
                                tab.list.splice(position..position + 1, 1);
                            }
                        }
                        if let Some(history) = tab.inspections.get_mut("History")
                            && history["exec_id"] == exec_id
                            && let Some(event) = history["outputs"].get_mut(index)
                        {
                            assign(event);
                        }
                    }
                    Event::AttachmentPreview {
                        cell_id,
                        name,
                        mime,
                        text,
                        path,
                        offset,
                        end,
                        total,
                        error,
                    } => {
                        tab.attachment_pages.insert(
                            format!("{cell_id}/{name}"),
                            json!({
                                "mime": mime,
                                "text": text,
                                "path": path,
                                "offset": offset,
                                "end": end,
                                "total": total,
                                "error": error,
                            }),
                        );
                        if let Some(position) = tab.model.order.iter().position(|id| id == &cell_id)
                        {
                            tab.list.splice(position..position + 1, 1);
                        }
                    }
                    Event::Draft(value) => tab.drafts.push(value),
                    Event::Error { code, message } => {
                        if let Some(review) = tab.inspections.get_mut("Bundle review")
                            && review["pending"] == true
                        {
                            review["pending"] = json!(false);
                            review["error"] = json!({"code": code});
                        }
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
            tab.raw_details.insert(self.surface.clone(), false);
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
        if let Some(path) = fields["path"].as_str() {
            return PathBuf::from(path)
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_else(|| "the selected bundle".into());
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
        if self.app_menu
            || self.export_menu
            || self.runtime_menu
            || self.notebook_menu
            || self.cell_menu.is_some()
            || self.insert_menu.is_some()
        {
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
            if matches!(key, "down" | "up" | "home" | "end") {
                // Reveal keyboard selections without undoing manual scrolling on redraw.
                self.palette_scroll
                    .scroll_to_item(self.palette_active, ScrollStrategy::Nearest);
            }
            cx.stop_propagation();
            cx.notify();
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
            "History" => self.inspect("executions.list", json!({})),
            "Checkpoints" => self.inspect("checkpoints.list", json!({})),
            "Diagnostics" => self.inspect("doctor", json!({})),
            "Runtimes" => self.inspect("runtimes.list", json!({})),
            _ => {}
        }
        cx.notify();
    }
    fn run_command(&mut self, method: &str, window: &mut Window, cx: &mut Context<Self>) {
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
    fn command_reason(&self, method: &str) -> Option<&'static str> {
        if matches!(method, "open" | "help") {
            return None;
        }
        let Some(tab) = self.tabs.get(self.active) else {
            return Some("Open a notebook to use this action");
        };
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
        let (
            bar,
            muted_fg,
            primary,
            active_bg,
            hover_bg,
            radius,
            warning,
            danger,
            info,
            transparent,
        ) = (
            theme.sidebar,
            theme.muted_foreground,
            theme.primary,
            theme.list_active,
            theme.list_hover,
            theme.radius,
            theme.warning,
            theme.danger,
            theme.info,
            theme.transparent,
        );
        let mono = theme.mono_font_family.clone();
        let tab = &self.tabs[self.active];
        let query = self.query.read(cx).value().to_lowercase();
        let mut list = v_flex()
            .id("cell-list")
            .role(Role::ListBox)
            .aria_label("Notebook cells")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .px_2()
            .pb_2()
            .gap_0p5();
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
                    .aria_label({
                        let status = if matches!(cell.save, SaveState::Conflict | SaveState::Failed)
                            || matches!(cell.outcome.as_str(), "error" | "failed")
                        {
                            ", failed"
                        } else if cell.stale() {
                            ", stale"
                        } else {
                            ""
                        };
                        format!(
                            "Cell {}: {}{status}",
                            i + 1,
                            cell_title(&cell.text, &cell.disk.kind)
                        )
                    })
                    .aria_selected(selected)
                    .gap_2()
                    .px_2()
                    .py_1p5()
                    .items_center()
                    .rounded(radius)
                    .border_l_2()
                    .border_color(if selected { primary } else { transparent })
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
                            .gap_0p5()
                            .child(
                                div()
                                    .truncate()
                                    .text_sm()
                                    .when(selected, |d| d.font_weight(FontWeight::MEDIUM))
                                    .when(preview.is_none(), |d| d.italic().text_color(muted_fg))
                                    .child(preview.unwrap_or_else(|| {
                                        format!("Empty {} cell", cell.disk.kind)
                                    })),
                            )
                            .child(
                                h_flex()
                                    .justify_between()
                                    .items_center()
                                    .gap_1()
                                    .text_size(rems(0.6875))
                                    .text_color(muted_fg)
                                    .child(div().truncate().child(kind_label(&cell.disk.kind)))
                                    .when(cell.disk.kind == "code", |d| {
                                        d.child(
                                            div().flex_none().font_family(mono.clone()).child(
                                                cell.ordinal
                                                    .map(|n| format!("[{n}]"))
                                                    .unwrap_or_else(|| "Unrun".into()),
                                            ),
                                        )
                                    }),
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
        v_flex()
            .id("sidebar")
            .debug_selector(|| "notebook-outline".into())
            .track_focus(&self.sidebar_focus)
            .size_full()
            .bg(bar)
            .border_r_1()
            .border_color(divider(cx))
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
                            .child("Cells"),
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
    }
    fn empty_notebook(&self, cx: &mut Context<Self>) -> AnyElement {
        let muted_fg = cx.theme().muted_foreground;
        v_flex()
            .size_full()
            .items_center()
            .justify_center()
            .p_6()
            .child(
                card(cx)
                    .max_w(px(440.))
                    .p_6()
                    .gap_4()
                    .items_center()
                    .child(icon_tile(Glyph::NotebookPen, 44., cx))
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
                                div().text_sm().text_center().text_color(muted_fg).child(
                                    "Add a cell to start writing. Nothing runs until you ask.",
                                ),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .text_center()
                                    .text_color(muted_fg)
                                    .child("Notebook Ctrl+1, History Ctrl+2, Checkpoints Ctrl+3."),
                            ),
                    )
                    .child(
                        h_flex()
                            .gap_2()
                            .flex_wrap()
                            .justify_center()
                            .child(
                                action_button("empty-code")
                                    .primary()
                                    .icon(Glyph::Code)
                                    .label("Add code cell")
                                    .on_click(cx.listener(|v, _, _, _| {
                                        v.mutate(json!({"action":"add","type":"code"}))
                                    })),
                            )
                            .child(
                                action_button("empty-markdown")
                                    .outline()
                                    .icon(Glyph::Type)
                                    .label("Add markdown cell")
                                    .on_click(cx.listener(|v, _, _, _| {
                                        v.mutate(json!({"action":"add","type":"markdown"}))
                                    })),
                            ),
                    ),
            )
            .into_any_element()
    }
    fn welcome(&self, cx: &mut Context<Self>) -> Div {
        let theme = cx.theme();
        let (muted_fg, radius) = (theme.muted_foreground, theme.radius_lg);
        let sep = divider(cx);
        let mut shortcuts = v_flex()
            .w(px(360.))
            .max_w_full()
            .rounded(radius)
            .border_1()
            .border_color(sep)
            .bg(theme.popover)
            .overflow_hidden();
        let shortcut_rows = [
            ("Open notebook", "ctrl-o"),
            ("Command palette", "ctrl-shift-p"),
            ("Run selected cell", "shift-enter"),
            ("Run all cells", "ctrl-shift-enter"),
            ("Save", "ctrl-s"),
            ("Export PDF or Word", "ctrl-shift-e"),
            ("Command mode", "escape"),
        ];
        for (idx, (label, keys)) in shortcut_rows.into_iter().enumerate() {
            shortcuts = shortcuts.child(
                h_flex()
                    .px_3p5()
                    .py_2()
                    .justify_between()
                    .items_center()
                    .when(idx + 1 < shortcut_rows.len(), |d| {
                        d.border_b_1().border_color(sep)
                    })
                    .text_sm()
                    .child(div().text_color(muted_fg).child(label))
                    .children(kbd(keys)),
            );
        }
        let recent = self.recent.iter().take(5).enumerate().map(|(i, path)| {
            let path = path.clone();
            action_button(("recent", i))
                .outline()
                .small()
                .icon(Glyph::NotebookPen)
                .label(
                    path.file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .to_string(),
                )
                .hint(path.display().to_string())
                .on_click(cx.listener(move |v, _, _, cx| v.open(path.clone(), cx)))
        });
        v_flex()
            .flex_1()
            .items_center()
            .justify_center()
            .px_6()
            .py_6()
            .gap_5()
            .child(
                v_flex()
                    .items_center()
                    .gap_1p5()
                    .child(icon_tile(Glyph::NotebookPen, 44., cx))
                    .child(
                        div()
                            .text_2xl()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child("KernelLedger"),
                    )
                    .child(
                        div()
                            .text_sm()
                            .text_center()
                            .text_color(muted_fg)
                            .child("Local-first reproducible notebooks. Source and recorded execution history stay in plain local files."),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_center()
                            .text_color(muted_fg)
                            .child("Open or create a notebook, then use Notebook (Ctrl+1), History (Ctrl+2), or Checkpoints (Ctrl+3)."),
                    ),
            )
            .child(
                h_flex()
                    .gap_2p5()
                    .flex_wrap()
                    .justify_center()
                    .items_center()
                    .child(
                        action_button("welcome-open")
                            .primary()
                            .icon(Glyph::FolderOpen)
                            .label("Open notebook")
                            .on_click(cx.listener(|v, _, window, cx| {
                                v.open_dialog(&OpenNotebook, window, cx)
                            })),
                    )
                    .child(
                        action_button("create")
                            .outline()
                            .icon(Glyph::Plus)
                            .label("Create notebook")
                            .on_click(cx.listener(|v, _, w, cx| v.create_dialog(false, w, cx))),
                    )
                    .child(
                        action_button("sample")
                            .outline()
                            .label("Create sample notebook")
                            .on_click(cx.listener(|v, _, w, cx| v.create_dialog(true, w, cx))),
                    )
                    .child(
                        action_button("welcome-help")
                            .ghost()
                            .icon(Glyph::Info)
                            .label("Help and shortcuts")
                            .hint("Open Help (F1)")
                            .on_click(cx.listener(|v, _, _, cx| {
                                v.surface = "Help".into();
                                cx.notify();
                            })),
                    ),
            )
            .when(!self.recent.is_empty(), |d| {
                d.child(
                    v_flex()
                        .items_center()
                        .gap_2()
                        .child(
                            div()
                                .text_xs()
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(muted_fg)
                                .child("Recent notebooks"),
                        )
                        .child(h_flex().gap_2().flex_wrap().justify_center().children(recent)),
                )
            })
            .child(shortcuts)
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
        if self.history_owner != Some(self.active) && !self.tabs.is_empty() {
            if let Some(old) = self
                .history_owner
                .and_then(|index| self.tabs.get_mut(index))
            {
                old.history_filter = self.history_query.read(cx).value().to_string();
            }
            let query = self.tabs[self.active].history_filter.clone();
            self.history_query
                .update(cx, |state, cx| state.set_value(query, window, cx));
            self.history_owner = Some(self.active);
            self.history_controls.clear();
            self.history_field_subscriptions.clear();
            self.history_fields_key.clear();
        }
        cx.set_global(WorkbenchPresentation {
            comfortable: self.preferences.comfortable,
        });
        window.set_rem_size(px(16. * self.preferences.ui_scale));
        let font_size = px(14. * self.preferences.ui_scale);
        if cx.theme().font_size != font_size {
            let theme = Theme::global_mut(cx);
            theme.font_size = font_size;
            theme.mono_font_size = font_size;
            Theme::sync_base(cx);
        }
        self.compact = window.viewport_size().width < px(1024. * self.preferences.ui_scale);
        let title = self
            .tabs
            .get(self.active)
            .map(|t| {
                format!(
                    "KernelLedger \u{2014} {}",
                    t.model
                        .project
                        .file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                )
            })
            .unwrap_or_else(|| "KernelLedger".into());
        window.set_window_title(&title);
        let filter = self.palette_query.read(cx).value().to_lowercase();
        if filter != self.palette_filter {
            self.palette_filter = filter;
            self.palette_active = 0;
            self.palette_scroll.scroll_to_item(0, ScrollStrategy::Top);
        }
        let mut root = v_flex()
            .id("workspace")
            .key_context("KernelLedger")
            .track_focus(&self.focus)
            .relative()
            .size_full()
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .text_size(rems(if self.preferences.comfortable {
                0.9375
            } else {
                0.875
            }))
            .on_action(cx.listener(Self::save))
            .on_action(cx.listener(Self::run))
            .on_action(cx.listener(Self::all))
            .on_action(cx.listener(Self::palette))
            .on_action(cx.listener(Self::open_dialog))
            .on_action(cx.listener(Self::command_mode))
            .child(self.header(window, cx));
        if self.tabs.is_empty() {
            if self.surface == "Help" {
                root = root.child(
                    div()
                        .id("welcome-help-content")
                        .flex_1()
                        .overflow_y_scroll()
                        .p_6()
                        .child(
                            v_flex()
                                .max_w(px(760.))
                                .mx_auto()
                                .gap_4()
                                .child(
                                    action_button("back-welcome")
                                        .self_start()
                                        .ghost()
                                        .small()
                                        .label("Back to welcome")
                                        .on_click(cx.listener(|v, _, _, cx| {
                                            v.surface = "Notebook".into();
                                            cx.notify();
                                        })),
                                )
                                .child(self.help_content(cx)),
                        ),
                );
            } else {
                root = root.child(self.welcome(cx));
            }
        } else {
            let center = if self.surface != "Notebook" {
                self.panel(window, cx)
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
                .child(self.banners(window, cx))
                .child(
                    h_flex()
                        .flex_1()
                        .min_h_0()
                        .relative()
                        .child(
                            h_resizable(SharedString::from(format!(
                                "notebook-layout-v5-{}-{}-{}",
                                self.preferences.outline_width,
                                self.surface,
                                self.sidebar_visible && !self.compact
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
                                    if v.surface == "Notebook"
                                        && !v.compact
                                        && v.sidebar_visible
                                        && let Some(width) = state.read(cx).sizes().first()
                                    {
                                        v.preferences.outline_width =
                                            f32::from(*width).clamp(180., 300.);
                                        v.save_preferences(cx);
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
                                        .w(px(220. * self.preferences.ui_scale))
                                        .max_w_full()
                                        .border_r_1()
                                        .border_color(divider(cx))
                                        .when(!self.high_contrast, |d| d.shadow_lg())
                                        .child(self.sidebar(cx)),
                                )
                            },
                        ),
                )
                .child(self.status_bar(cx));
        }
        if self.app_menu
            || self.export_menu
            || self.runtime_menu
            || self.notebook_menu
            || self.cell_menu.is_some()
            || self.insert_menu.is_some()
        {
            root = root.child(self.menu_overlay(window, cx));
        }
        if self.palette {
            root = root.child(self.palette_overlay(cx));
        }
        if let Some((method, fields)) = self.confirmation.clone() {
            root = root.child(self.confirm_overlay(method, fields, window, cx));
        }
        if cx.try_global::<CopyNotice>().is_some_and(|n| n.visible) {
            root = root.child(
                div()
                    .id("copy-notice")
                    .role(Role::Status)
                    .aria_label("Copied to clipboard")
                    .absolute()
                    .bottom(rems(2.25))
                    .right(rems(0.75))
                    .px_3()
                    .py_2()
                    .rounded(px(7.))
                    .bg(cx.theme().popover)
                    .border_1()
                    .border_color(divider(cx))
                    .when(!self.high_contrast, |d| d.shadow_md())
                    .text_sm()
                    .child("Copied to clipboard"),
            );
        }
        if let Some(notice) = self.export_notice(cx) {
            root = root.child(notice);
        }
        gpui::image_cache(images::workspace_cache("workspace-images"))
            .size_full()
            .child(root)
            .into_any_element()
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
    text.split('\n').count().clamp(1, 30)
}
fn tone(status: &str, cx: &App) -> Hsla {
    let theme = cx.theme();
    match status {
        "ok" | "success" | "succeeded" | "completed" | "complete" | "idle" | "ready"
        | "verified" | "Active" => theme.success,
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
        Some("stopped") => "Stopped (run to start)".into(),
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
fn output(
    commands: &std::sync::mpsc::Sender<Command>,
    event: &Value,
    id: String,
    cell_id: &str,
    exec_id: &str,
    index: usize,
) -> AnyElement {
    outputs::OutputBlock {
        tools_state: None,
        context: id.clone().into(),
        id: id.into(),
        event: event.clone(),
        commands: Some(commands.clone()),
        cell_id: cell_id.to_owned().into(),
        exec_id: exec_id.to_owned().into(),
        index,
    }
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
        "interrupt_kill" => (
            "Terminate worker for",
            "Forcibly stops the worker process. Live variables and in-progress computation are lost; recorded history and saved source remain available.",
            "Terminate worker",
        ),
        "training-restore" => (
            "Restore training for",
            "Executes the captured reconstruction factory in a fresh worker. After success it replaces your live namespace. Registered training state is restored; notebook variables and native process state are excluded. Failed reconstruction preserves the current worker.",
            "Trust and restore training",
        ),
        "merge" => (
            "Merge cells",
            "The next cell is appended to this cell and removed from the source document. Recorded history is retained. Structural undo is not available.",
            "Merge",
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
        "bundle_import" => (
            "Import recorded history from",
            "Copies sessions, executions, and checkpoints into this notebook. Local records with the same identifiers are kept. Notebook source is not replaced, and no code runs. Saved code in the bundle can run later only if you restore or replay it.",
            "Import recorded history",
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
        "transport" => {
            "Connection lost \u{2014} execution state unknown. Your edits are retained. Reconnect in Diagnostics to reconcile the operation."
        }
        "python_mismatch" => {
            "The checkpoint uses a different Python version. Choose a compatible interpreter, or review the mismatch override when retrying this checkpoint."
        }
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
        "KernelLedger desktop {}\nKernelLedger runtime: {version} (development Python environment; private bundle not packaged)\nRuntime plan: 5\nFrontend plan: 6\nDesktop protocol: kernelledger.desktop v1",
        env!("CARGO_PKG_VERSION")
    )
}
fn persist_appearance(value: &str, cx: &mut App) {
    if cfg!(test) {
        return;
    }
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
            "{} \u{2014} {}",
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
        .map(|d| format!(", {d:.2}s"))
        .unwrap_or_default();
    format!("{title} \u{2014} {time_label}{duration}")
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

fn saved_reduced_motion(profile: &std::path::Path) -> Option<bool> {
    let bytes = std::fs::read(profile.join("workbench.json")).ok()?;
    serde_json::from_slice::<Value>(&bytes)
        .ok()?
        .get("reduced_motion")
        .and_then(Value::as_bool)
}

#[cfg(target_os = "windows")]
fn system_reduced_motion() -> bool {
    #[link(name = "user32")]
    unsafe extern "system" {
        fn SystemParametersInfoW(
            action: u32,
            param: u32,
            data: *mut std::ffi::c_void,
            flags: u32,
        ) -> i32;
    }
    let mut enabled = 1i32;
    // SPI_GETCLIENTAREAANIMATION. A zero result means the OS turned animations off.
    let read =
        unsafe { SystemParametersInfoW(0x1042, 0, (&mut enabled as *mut i32).cast(), 0) != 0 };
    read && enabled == 0
}

#[cfg(not(target_os = "windows"))]
fn system_reduced_motion() -> bool {
    false
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
        "{kind} \u{2014} {target} ({})",
        op["accepted"]
            .as_str()
            .map(relative_time)
            .unwrap_or_else(|| "this session".into())
    )
}
