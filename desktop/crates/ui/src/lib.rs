mod accessibility;
mod assistant;
mod checkpoints;
mod codegen;
mod components;
mod dag;
mod debug_source;
mod diagnostics;
mod help;
mod history;
mod html_output;
mod images;
mod interactive;
mod interchange;
mod math_output;
mod notebook;
mod outputs;
mod runtime;
mod settings;
mod shell;
mod shortcuts;
use shortcuts::command_shortcut;
mod surfaces;
mod welcome;
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
    input::{Editor, EditorState, Input, InputEvent, InputState, Textarea, TextareaState},
    kbd::Kbd,
    resizable::{h_resizable, resizable_panel},
    text::TextView,
    v_flex,
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use kernelledger_client::{
    output::{bounded, safe_markdown},
    recent::RecentNotebook,
    service::{Command, Event, NotebookService},
};
use kernelledger_models::{Mode, NotebookModel, SaveState, SourceSnapshot};
use serde_json::{Value, json};
use std::{collections::BTreeMap, path::PathBuf, time::Duration};

actions!(
    kernelledger,
    [
        Save,
        RunCell,
        RunAll,
        Palette,
        OpenNotebook,
        CommandMode,
        ToggleAppMode
    ]
);
const SURFACES: [(&str, Glyph, &str); 10] = [
    (
        "Debugger",
        Glyph::Code,
        "Step through an immutable executed cell and inspect passive locals",
    ),
    (
        "Variables",
        Glyph::Code,
        "Passive live metadata; refresh while the notebook is idle",
    ),
    (
        "Console",
        Glyph::Code,
        "Execute Python in the attached notebook session",
    ),
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
mod commands;
use commands::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct TabId(pub u64);

/// At most one header or contextual popover menu can be open at a time.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ActiveMenu {
    Application,
    Notebook,
    Export,
    Runtime,
    /// Keyed by cell id: cells inserted or removed while the menu is open
    /// must not redirect its actions, so indexes are resolved on activation.
    Cell {
        cell_id: String,
        assistant: bool,
    },
    /// Insert after the cell with this id.
    Insert {
        after: String,
    },
}

#[derive(Clone, Debug)]
pub(crate) struct VariablesState {
    pub category: String,
    pub detail_tab: String,
    pub selected: Option<String>,
    pub sort_col: Option<String>,
    /// Position of the sorted column; the worker prefers it over the label, which
    /// is ambiguous for duplicate, non-string, or truncated column names.
    pub sort_col_index: Option<u64>,
    pub sort_desc: bool,
    pub filter_query: String,
    pub chart_x: Option<String>,
    pub chart_y: Option<String>,
    pub chart_kind: String,
}

impl Default for VariablesState {
    fn default() -> Self {
        Self {
            category: "all".into(),
            detail_tab: "data".into(),
            selected: None,
            sort_col: None,
            sort_col_index: None,
            sort_desc: false,
            filter_query: String::new(),
            chart_x: None,
            chart_y: None,
            chart_kind: "bar".into(),
        }
    }
}

struct Tab {
    id: TabId,
    assistant: assistant::AssistantView,
    model: NotebookModel,
    service: NotebookService,
    list: ListState,
    editors: BTreeMap<String, Entity<EditorState>>,
    debug_decorations: BTreeMap<String, gpui_kit::component::input::TextDecorationCollection>,
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
    /// The History filter the rows reflect; the search input holds the editable draft.
    history_applied: String,
    checkpoint_notice: Option<String>,
    error: Option<String>,
    attachment_pages: BTreeMap<String, Value>,
    variables_scroll: UniformListScrollHandle,
    variables_state: VariablesState,
    console_history: Vec<Value>,
    html_views: BTreeMap<String, Entity<gpui_webview::WebView>>,
    html_notes: BTreeMap<String, String>,
    html_states: BTreeMap<String, html_output::HtmlState>,
    sidebar_tab: String,
    app_mode: bool,
    files_cache: Option<Value>,
    packages_cache: Option<Value>,
    package_install_status: Option<String>,
    formatting_cell: Option<String>,
}
pub struct Workspace {
    tabs: Vec<Tab>,
    active: usize,
    next_tab_id: u64,
    focus: FocusHandle,
    surface: String,
    palette: bool,
    query: Entity<InputState>,
    palette_query: Entity<InputState>,
    prompts: BTreeMap<String, Entity<InputState>>,
    /// The Assistant composer for each notebook, kept across panel toggles.
    questions: BTreeMap<String, Entity<TextareaState>>,
    console_editors: BTreeMap<String, Entity<EditorState>>,
    tick: Task<()>,
    /// Re-reads the system high-contrast setting, which Windows can toggle
    /// without a window-appearance change.
    contrast_poll: Task<()>,
    wake: smol::channel::Sender<()>,
    appearance_subscription: Option<Subscription>,
    python: PathBuf,
    dark: bool,
    high_contrast: bool,
    confirmation: Option<(String, Value)>,
    /// Why the last requested notebook action did not run.
    notice: Option<String>,
    /// Keyboard focus for each cell's insert boundary, keyed by cell id.
    insert_focus: BTreeMap<String, FocusHandle>,
    /// Default output variables for new SQL cells, by cell id, until stored.
    pending_output_vars: BTreeMap<String, String>,
    /// The SQL output variable being renamed in a cell header.
    output_var_edit: Option<notebook::OutputVarEdit>,
    palette_active: usize,
    palette_filter: String,
    palette_scroll: UniformListScrollHandle,
    assistant_scroll: ScrollHandle,
    assistant_scroll_mark: (usize, bool, bool, usize, bool, usize),
    return_focus: Option<FocusHandle>,
    sidebar_visible: bool,
    menu: Option<ActiveMenu>,
    history_owner: Option<TabId>,
    menu_anchor: String,
    menu_bounds: BTreeMap<String, Bounds<Pixels>>,
    preferences: kernelledger_client::preferences::Preferences,
    preference_status: Option<String>,
    preference_saving: bool,
    html_network_origins: Entity<InputState>,
    html_network_error: Option<String>,
    html_signals: std::sync::mpsc::SyncSender<html_output::HtmlSignal>,
    html_signal_receiver: std::sync::mpsc::Receiver<html_output::HtmlSignal>,
    compact: bool,
    outline_drawer: bool,
    follow_system: bool,
    history_query: Entity<InputState>,
    history_controls: BTreeMap<String, Entity<InputState>>,
    history_field_subscriptions: Vec<Subscription>,
    history_fields_key: String,
    history_filters_open: bool,
    history_enter: Option<Subscription>,
    settings_section: settings::SettingsSection,
    help_query: Entity<InputState>,
    variables_query: Entity<InputState>,
    df_filter_query: Entity<InputState>,
    pkg_install_input: Entity<InputState>,
    query_subscriptions: Vec<Subscription>,
    record_active: usize,
    sidebar_focus: FocusHandle,
    assistant_focus: FocusHandle,
    confirm_focus: FocusHandle,
    dialog_focus: FocusHandle,
    dialog_open: bool,
    keys: Option<Subscription>,
    recent: Vec<RecentNotebook>,
    recorded_opens: std::collections::BTreeSet<PathBuf>,
    welcome_notice: Option<String>,
    applied_title: String,
    applied_presentation: Option<WorkbenchPresentation>,
}
impl Workspace {
    pub fn new(
        project: Option<PathBuf>,
        appearance: &str,
        recent: Vec<RecentNotebook>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        shortcuts::bind_commands(cx);
        cx.bind_keys([
            KeyBinding::new("ctrl-s", Save, Some("KernelLedger")),
            KeyBinding::new("ctrl-enter", RunCell, Some("KernelLedger")),
            KeyBinding::new("shift-enter", RunCell, Some("KernelLedger")),
            KeyBinding::new("ctrl-shift-enter", RunAll, Some("KernelLedger")),
            KeyBinding::new("ctrl-shift-p", Palette, Some("KernelLedger")),
            KeyBinding::new("ctrl-o", OpenNotebook, Some("KernelLedger")),
            KeyBinding::new("ctrl-.", ToggleAppMode, Some("KernelLedger")),
            KeyBinding::new("escape", CommandMode, Some("KernelLedger")),
        ]);
        #[cfg(target_os = "macos")]
        cx.bind_keys([
            KeyBinding::new("cmd-s", Save, Some("KernelLedger")),
            KeyBinding::new("cmd-shift-p", Palette, Some("KernelLedger")),
            KeyBinding::new("cmd-o", OpenNotebook, Some("KernelLedger")),
            KeyBinding::new("cmd-.", ToggleAppMode, Some("KernelLedger")),
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
        let html_network_origins = cx.new(|cx| {
            let mut input =
                InputState::new(window, cx).placeholder("cdn.plot.ly, https://api.example.com");
            input.set_value(preferences.html_allowed_origins.join(", "), window, cx);
            input
        });
        let (html_signals, html_signal_receiver) = std::sync::mpsc::sync_channel(64);
        let (wake, wake_rx) = smol::channel::bounded(1);
        let mut view = Self {
            tabs: vec![],
            active: 0,
            next_tab_id: 1,
            focus: cx.focus_handle(),
            surface: "Notebook".into(),
            palette: false,
            query: cx.new(|cx| InputState::new(window, cx).placeholder("Filter cells")),
            palette_query: cx.new(|cx| InputState::new(window, cx).placeholder("Type a command")),
            prompts: BTreeMap::new(),
            questions: BTreeMap::new(),
            console_editors: BTreeMap::new(),
            tick: Task::ready(()),
            contrast_poll: Task::ready(()),
            wake,
            appearance_subscription: None,
            python: std::env::var_os("KL_RUNTIME_PYTHON")
                .map(PathBuf::from)
                .or_else(|| {
                    (!cfg!(test))
                        .then(|| {
                            std::fs::read_to_string(
                                kernelledger_client::profile_path().join("helper-python.txt"),
                            )
                            .ok()
                        })
                        .flatten()
                        .map(|p| PathBuf::from(p.trim()))
                })
                .unwrap_or_default(),
            dark,
            high_contrast,
            confirmation: None,
            notice: None,
            insert_focus: BTreeMap::new(),
            pending_output_vars: BTreeMap::new(),
            output_var_edit: None,
            palette_active: 0,
            palette_filter: String::new(),
            palette_scroll: UniformListScrollHandle::default(),
            assistant_scroll: ScrollHandle::new(),
            assistant_scroll_mark: Default::default(),
            return_focus: None,
            sidebar_visible: preferences.outline_visible,
            menu: None,
            history_owner: None,
            menu_anchor: String::new(),
            menu_bounds: BTreeMap::new(),
            preferences,
            preference_status,
            preference_saving: false,
            html_network_origins,
            html_network_error: None,
            html_signals,
            html_signal_receiver,
            compact: false,
            outline_drawer: false,
            follow_system,
            history_query: cx
                .new(|cx| InputState::new(window, cx).placeholder("Search recorded source")),
            history_controls: BTreeMap::new(),
            history_field_subscriptions: Vec::new(),
            history_fields_key: String::new(),
            history_filters_open: false,
            history_enter: None,
            settings_section: settings::SettingsSection::Appearance,
            help_query: cx.new(|cx| InputState::new(window, cx).placeholder("Search Help")),
            variables_query: cx.new(|cx| {
                InputState::new(window, cx).placeholder("Filter name, type, value, or column")
            }),
            df_filter_query: cx.new(|cx| {
                InputState::new(window, cx).placeholder("Filter: col>10, col==val, or text\u{2026}")
            }),
            pkg_install_input: cx
                .new(|cx| InputState::new(window, cx).placeholder("Install package (e.g. duckdb)")),
            query_subscriptions: Vec::new(),
            record_active: 0,
            sidebar_focus: cx.focus_handle(),
            assistant_focus: cx.focus_handle(),
            confirm_focus: cx.focus_handle(),
            dialog_focus: cx.focus_handle(),
            dialog_open: false,
            keys: None,
            recent,
            recorded_opens: Default::default(),
            welcome_notice: None,
            applied_title: String::new(),
            applied_presentation: None,
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
        view.appearance_subscription =
            Some(cx.observe_window_appearance(window, |view, window, cx| {
                view.sync_system_appearance(window, cx)
            }));
        view.history_enter = Some(cx.subscribe_in(
            &view.history_query,
            window,
            |v, _, event: &InputEvent, window, cx| match event {
                InputEvent::PressEnter { .. } => v.apply_history_filters(window, cx),
                InputEvent::Change => cx.notify(),
                _ => {}
            },
        ));
        for input in [
            view.query.clone(),
            view.palette_query.clone(),
            view.help_query.clone(),
            view.variables_query.clone(),
        ] {
            view.query_subscriptions.push(cx.subscribe_in(
                &input,
                window,
                |_, _, event: &InputEvent, _, cx| {
                    if matches!(event, InputEvent::Change) {
                        cx.notify();
                    }
                },
            ));
        }
        if let Some(project) = project {
            view.open(project, cx);
        }
        view.tick = cx.spawn_in(window, async move |view, window| {
            loop {
                let busy = view
                    .read_with(window, |view, _| {
                        view.tabs.iter().any(|tab| tab.assistant.busy)
                    })
                    .unwrap_or(false);
                if busy {
                    let wake_fut = async { wake_rx.recv().await.is_ok() };
                    let timer_fut = async {
                        window
                            .background_executor()
                            .timer(Duration::from_secs(1))
                            .await;
                        true
                    };
                    if !smol::future::or(wake_fut, timer_fut).await {
                        break;
                    }
                } else if wake_rx.recv().await.is_err() {
                    break;
                }
                if view
                    .update_in(window, |view, window, cx| {
                        view.drain(window, cx);
                        if view.tabs.iter().any(|tab| tab.assistant.busy) {
                            cx.notify();
                        }
                    })
                    .is_err()
                {
                    break;
                }
            }
        });
        if SYSTEM_CONTRAST_POLL && !cfg!(test) {
            view.contrast_poll = cx.spawn_in(window, async move |view, window| {
                loop {
                    window
                        .background_executor()
                        .timer(Duration::from_secs(3))
                        .await;
                    let alive = view.update_in(window, |view, window, cx| {
                        // A cheap read; the theme is rebuilt only when it changed.
                        if view.follow_system && view.high_contrast != system_high_contrast() {
                            view.sync_system_appearance(window, cx);
                            cx.notify();
                        }
                    });
                    if alive.is_err() {
                        break;
                    }
                }
            });
        }
        // Application keybindings need a focused descendant from the first
        // frame, before a pointer click has focused a button or cell editor.
        view.focus.focus(window, cx);
        view
    }

    pub(crate) fn wake_notifier(&self) -> std::sync::Arc<dyn Fn() + Send + Sync> {
        let wake = self.wake.clone();
        std::sync::Arc::new(move || {
            let _ = wake.try_send(());
        })
    }

    pub(crate) fn has_menu(&self) -> bool {
        self.menu.is_some()
    }

    pub(crate) fn tab_index_by_id(&self, id: TabId) -> Option<usize> {
        self.tabs.iter().position(|tab| tab.id == id)
    }

    pub(crate) fn tab_by_id(&self, id: TabId) -> Option<&Tab> {
        self.tabs.iter().find(|tab| tab.id == id)
    }

    pub(crate) fn tab_by_id_mut(&mut self, id: TabId) -> Option<&mut Tab> {
        self.tabs.iter_mut().find(|tab| tab.id == id)
    }

    fn sync_system_appearance(&mut self, window: &mut Window, cx: &mut Context<Self>) {
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
    }

    fn sync_confirmation_focus(&mut self, window: &mut Window, cx: &mut Context<Self>) {
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
    }

    fn open(&mut self, project: PathBuf, cx: &mut Context<Self>) {
        if let Some(i) = self.tabs.iter().position(|t| t.model.project == project) {
            self.active = i;
            return;
        }
        let id = TabId(self.next_tab_id);
        self.next_tab_id += 1;
        let wake = self.wake_notifier();
        self.tabs.push(Tab {
            id,
            assistant: assistant::AssistantView::default(),
            service: NotebookService::start_with_wake(project.clone(), self.python.clone(), wake),
            model: NotebookModel {
                project,
                ..Default::default()
            },
            list: ListState::new(0, ListAlignment::Top, px(200.)),
            editors: BTreeMap::new(),
            debug_decorations: BTreeMap::new(),
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
            history_applied: String::new(),
            checkpoint_notice: None,
            error: None,
            attachment_pages: BTreeMap::new(),
            variables_scroll: UniformListScrollHandle::default(),
            variables_state: VariablesState::default(),
            console_history: vec![],
            html_views: BTreeMap::new(),
            html_states: BTreeMap::new(),
            html_notes: BTreeMap::new(),
            sidebar_tab: "outline".into(),
            app_mode: false,
            files_cache: None,
            packages_cache: None,
            package_install_status: None,
            formatting_cell: None,
        });
        self.active = self.tabs.len() - 1;
        cx.notify();
    }

    pub(crate) fn close_tab(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        if index >= self.tabs.len() {
            return;
        }
        for view in self.tabs[index].html_views.values() {
            view.update(cx, |wv, _| {
                let _ = wv.set_visible(false);
            });
        }
        let closed = self.tabs.remove(index);
        let key = closed.model.project.display().to_string();
        self.console_editors.remove(&key);
        self.questions.remove(&key);
        if self.history_owner == Some(closed.id) {
            self.history_owner = None;
            self.history_controls.clear();
            self.history_field_subscriptions.clear();
            self.history_fields_key.clear();
        }
        self.confirmation = None;
        if self.tabs.is_empty() {
            self.active = 0;
            self.record_active = 0;
            if self.surface != "Help" {
                self.surface = "Notebook".into();
            }
        } else {
            if index < self.active {
                self.active -= 1;
            } else {
                self.active = self.active.min(self.tabs.len() - 1);
            }
            self.record_active = self.tabs[self.active]
                .selections
                .get(&self.surface)
                .copied()
                .unwrap_or(0);
        }
        self.close_menu(window, cx);
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
    fn inspect(&self, method: &str, mut fields: Value) {
        if method == "executions.list" && fields.get("query").is_none() {
            fields["query"] = json!(
                self.tabs
                    .get(self.active)
                    .map(|tab| tab.history_applied.as_str())
                    .unwrap_or("")
            );
            fields["page_size"] = json!(100);
            fields["scan_pages"] = json!(10);
        }
        self.send(Command::Inspect {
            method: method.into(),
            fields,
        });
    }

    pub(crate) fn toggle_app_mode(
        &mut self,
        _: &ToggleAppMode,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.tabs.is_empty() {
            return;
        }
        // Menus capture cell actions that App mode no longer offers.
        self.close_menu(window, cx);
        self.palette = false;
        self.notice = None;
        self.surface = "Notebook".into();
        let tab = &mut self.tabs[self.active];
        tab.app_mode = !tab.app_mode;
        if tab.app_mode {
            // No editor is drawn in App mode; keep keys off a hidden editor.
            tab.model.mode = Mode::Command;
            tab.reveal = None;
            self.focus.focus(window, cx);
        }
        let count = tab.model.order.len();
        tab.list.splice(0..count, count);
        if tab.model.saved_app_mode() != tab.app_mode {
            let metadata = tab.model.metadata_with_app_mode(tab.app_mode);
            self.mutate(json!({"action": "metadata", "metadata": metadata}));
        }
        cx.notify();
    }

    /// Selects, scrolls to and edits `id`. The change is applied by `drain`,
    /// so wake the loop: nothing else may arrive to run it.
    pub(crate) fn request_reveal(&mut self, id: Option<String>) {
        if let Some(tab) = self.tabs.get_mut(self.active) {
            tab.reveal = id;
            let _ = self.wake.try_send(());
        }
    }

    /// Like `request_reveal`, for the cell a source mutation creates or moves:
    /// it waits for the snapshot that mutation produces, so the cell is shown
    /// where it lands. That snapshot drains it, so no wake is needed.
    fn reveal_after_mutation(&mut self, id: Option<String>) {
        if let Some(tab) = self.tabs.get_mut(self.active) {
            tab.reveal = id;
        }
    }

    /// Shows an informational notice above the notebook.
    pub(crate) fn show_notice(&mut self, notice: impl Into<String>, cx: &mut Context<Self>) {
        self.notice = Some(notice.into());
        cx.notify();
    }

    pub(crate) fn mutate_cell_metadata(
        &mut self,
        cell_id: &str,
        key: &str,
        value: Value,
        cx: &mut Context<Self>,
    ) {
        let Some(tab) = self.tabs.get(self.active) else {
            return;
        };
        let Some(cell) = tab.model.cells.get(cell_id) else {
            return;
        };
        let metadata = cell.metadata_with(key, value);
        self.mutate(json!({
            "action": "metadata",
            "cell_id": cell_id,
            "metadata": metadata,
        }));
        cx.notify();
    }

    pub(crate) fn insert_cell_with_source(
        &mut self,
        kind: &str,
        source: String,
        cx: &mut Context<Self>,
    ) {
        let index = self.index_after_selection();
        self.add_cell(kind, index, Some(source));
        self.surface = "Notebook".into();
        cx.notify();
    }

    /// The insertion index just below the selected cell, or the end.
    pub(crate) fn index_after_selection(&self) -> usize {
        self.tabs.get(self.active).map_or(0, |tab| {
            tab.model
                .selected
                .as_ref()
                .and_then(|id| tab.model.order.iter().position(|c| c == id))
                .map_or(tab.model.order.len(), |i| i + 1)
        })
    }

    /// Inserts below the cell `after`, resolved now rather than when a menu
    /// opened, or at the end when that cell has since been removed.
    pub(crate) fn insert_cell_after(&mut self, after: &str, kind: &str) {
        let Some(tab) = self.tabs.get(self.active) else {
            return;
        };
        let index = tab
            .model
            .order
            .iter()
            .position(|id| id == after)
            .map_or(tab.model.order.len(), |i| i + 1);
        self.add_cell(kind, index, None);
    }

    /// Adds a `kind` cell at `index`. A new SQL cell gets an output variable
    /// that no other cell defines (`sql_result`, `sql_result_2`, …).
    pub(crate) fn add_cell(&mut self, kind: &str, index: usize, source: Option<String>) {
        let id = uuid::Uuid::new_v4().simple().to_string();
        let mut request = json!({"action": "add", "type": kind, "index": index, "new_id": id});
        if let Some(source) = source {
            request["source"] = json!(source);
        }
        if kind == "sql" {
            self.assign_output_var(id);
        }
        self.mutate(request);
    }

    /// Records a unique default output variable for SQL cell `id`, applied
    /// once the cell exists as SQL (see `apply_output_vars`).
    pub(crate) fn assign_output_var(&mut self, id: String) {
        let Some(tab) = self.tabs.get(self.active).filter(|tab| !tab.app_mode) else {
            return;
        };
        let mut taken = notebook_names(&tab.model);
        taken.extend(self.pending_output_vars.values().cloned());
        let name = codegen::unique_name("sql_result", &taken);
        self.pending_output_vars.insert(id, name);
    }

    /// Stores pending default output variables on SQL cells that now exist.
    /// The `add` mutation cannot carry metadata, so this follows it.
    fn apply_output_vars(&mut self, cx: &mut Context<Self>) {
        let Some(tab) = self.tabs.get(self.active) else {
            return;
        };
        let ready: Vec<String> = self
            .pending_output_vars
            .keys()
            .filter(|id| {
                tab.model
                    .cells
                    .get(*id)
                    .is_some_and(|c| c.disk.kind == "sql")
            })
            .cloned()
            .collect();
        for id in ready {
            let name = self.pending_output_vars.remove(&id).unwrap_or_default();
            let unset = !self.tabs[self.active].model.cells[&id]
                .cell_metadata()
                .contains_key("output_var");
            if unset {
                self.mutate_cell_metadata(&id, "output_var", json!(name), cx);
            }
        }
    }

    pub(crate) fn format_cell(&mut self, cell_id: &str, cx: &mut Context<Self>) {
        let Some(tab) = self.tabs.get_mut(self.active) else {
            return;
        };
        let Some(cell) = tab.model.cells.get(cell_id) else {
            return;
        };
        if cell.disk.kind != "code" {
            return;
        }
        let source = cell.text.clone();
        tab.formatting_cell = Some(cell_id.to_string());
        self.inspect("code.format", json!({"source": source}));
        cx.notify();
    }

    pub(crate) fn install_package(&mut self, package: String, cx: &mut Context<Self>) {
        let spec = package.trim().to_string();
        if spec.is_empty() {
            return;
        }
        if let Some(tab) = self.tabs.get_mut(self.active) {
            tab.package_install_status = Some(format!("Installing {spec}\u{2026}"));
        }
        self.inspect("packages.install", json!({"package": spec}));
        cx.notify();
    }

    pub(crate) fn run_stale_cells(&mut self, cx: &mut Context<Self>) {
        let Some(tab) = self.tabs.get(self.active) else {
            return;
        };
        let graph = dag::analyze_notebook(&tab.model);
        let blocked = blocked_cells(&tab.model, &graph);
        let ids: Vec<_> = graph
            .run_stale_ids
            .into_iter()
            .filter(|id| !blocked.contains(id))
            .collect();
        if ids.is_empty() {
            return;
        }
        self.send(Command::Run {
            cell_ids: ids,
            on_error: "stop".into(),
        });
        cx.notify();
    }

    pub(crate) fn run_cell_and_downstream(&mut self, cell_id: &str, cx: &mut Context<Self>) {
        let Some(tab) = self.tabs.get(self.active) else {
            return;
        };
        match downstream_plan(&tab.model, &dag::analyze_notebook(&tab.model), cell_id) {
            Ok(ids) => self.send(Command::Run {
                cell_ids: ids,
                on_error: "stop".into(),
            }),
            Err(reason) => self.notice = Some(reason.into()),
        }
        cx.notify();
    }

    pub(crate) fn jump_to_cell(&mut self, cell_id: String, cx: &mut Context<Self>) {
        if let Some(tab) = self.tabs.get_mut(self.active)
            && tab.model.cells.contains_key(&cell_id)
        {
            tab.model.selected = Some(cell_id.clone());
            self.surface = "Notebook".into();
            self.request_reveal(Some(cell_id));
            cx.notify();
        }
    }
    fn open_dialog(&mut self, _: &OpenNotebook, window: &mut Window, cx: &mut Context<Self>) {
        let focus = window.focused(cx).unwrap_or_else(|| self.focus.clone());
        let paths = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some("Open notebook directory".into()),
        });
        cx.spawn_in(window, async move |view, window| {
            let selected = paths.await;
            let _ = view.update_in(window, |_, window, cx| focus.focus(window, cx));
            if let Ok(Ok(Some(paths))) = selected
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
        self.sync_system_appearance(window, cx);
        self.sync_confirmation_focus(window, cx);
        let mut changed = false;
        let mut html_ready = Vec::new();
        for (event_tab, tab) in self.tabs.iter_mut().enumerate() {
            while let Ok(event) = tab.service.events.try_recv() {
                changed = true;
                match event {
                    Event::HtmlLoaded { key, token, result } => {
                        if let Some(state) = tab.html_states.get_mut(&key)
                            && state.token == token
                            && state.pending
                        {
                            state.pending = false;
                            tab.html_notes.remove(&key);
                            html_ready.push((event_tab, key, token, result));
                        }
                    }
                    Event::AssistantDelta(frame) => {
                        tab.assistant
                            .draft
                            .push_str(frame["text"].as_str().unwrap_or(""));
                        tab.assistant.status =
                            "Receiving draft; citations and proposals are being checked\u{2026}"
                                .into();
                    }
                    Event::AssistantProposal(proposal) => tab.assistant.proposals.push(proposal),
                    Event::AssistantError { method, message } => {
                        tab.assistant.busy = false;
                        tab.assistant.started = None;
                        tab.assistant.error = true;
                        if method == "respond" || method == "context.preview" {
                            tab.assistant.receipt = Value::Null;
                            tab.assistant.receipt_state = assistant::ReceiptState::Consumed;
                            tab.assistant.draft.clear();
                            tab.assistant.rebuild = true;
                        }
                        if method == "providers.test" {
                            let message = assistant::redact_secrets(&message);
                            tab.assistant.connection =
                                assistant::Connection::Failed(message.clone());
                            tab.assistant.status = format!("Provider test failed: {message}");
                            continue;
                        }
                        if method == "proposal"
                            && (message.contains("changed") || message.contains("revision"))
                        {
                            for proposal in &mut tab.assistant.proposals {
                                if proposal["state"] == "accepted" {
                                    proposal["state"] = json!("stale");
                                }
                            }
                        }
                        tab.assistant.status = format!("{method}: {message}");
                    }
                    Event::AssistantDone { method, value } => {
                        if !matches!(
                            method.as_str(),
                            "settings.get"
                                | "status"
                                | "conversations.list"
                                | "models.list"
                                | "search"
                        ) {
                            tab.assistant.busy = false;
                            tab.assistant.started = None;
                        }
                        if let Some(error) = value.get("assistant_error") {
                            tab.assistant.error = true;
                            tab.assistant.status = assistant::redact_secrets(
                                error["message"]
                                    .as_str()
                                    .unwrap_or("Assistant request failed"),
                            );
                            if method == "respond" || method == "context.preview" {
                                tab.assistant.receipt = Value::Null;
                                tab.assistant.receipt_state = assistant::ReceiptState::Consumed;
                                tab.assistant.draft.clear();
                                tab.assistant.rebuild = true;
                            }
                            if method == "providers.test" {
                                tab.assistant.connection =
                                    assistant::Connection::Failed(tab.assistant.status.clone());
                            }
                            continue;
                        }
                        if !matches!(
                            method.as_str(),
                            "status" | "settings.get" | "conversations.list"
                        ) {
                            tab.assistant.error = false;
                        }
                        match method.as_str() {
                            "status" => {
                                for key in ["usage", "capabilities", "profile", "task_profiles"] {
                                    tab.assistant.settings[key] = value[key].clone();
                                }
                            }
                            "context.preview" => {
                                tab.assistant.receipt = value;
                                tab.assistant.receipt_state = assistant::ReceiptState::Prepared;
                                tab.assistant.status =
                                    "Context ready. Inspect the receipt before sending.".into();
                            }
                            "respond" => {
                                tab.assistant.status =
                                    "Answer complete. Review evidence and proposed changes.".into();
                                tab.assistant.proposals =
                                    value["proposals"].as_array().cloned().unwrap_or_default();
                                tab.assistant.conversation =
                                    value["conversation_id"].as_str().map(str::to_owned);
                                let request_id = value["request_id"].clone();
                                let question = tab.assistant.last_question.clone();
                                tab.assistant.transcript.push(
                                    json!({"role":"user","text":question,"request_id":request_id}),
                                );
                                tab.assistant.transcript.push(json!({"role":"assistant","text":value["answer"],"citations":value["citations"],"request_id":request_id}));
                                tab.assistant.answer = value;
                                tab.assistant.receipt = Value::Null;
                                tab.assistant.receipt_state = assistant::ReceiptState::Consumed;
                                tab.assistant.rebuild = false;
                                let _ = tab.service.commands.send(Command::Assistant {
                                    method: "status".into(),
                                    fields: json!({}),
                                });
                            }
                            "settings.get" => {
                                tab.assistant.provider_draft = None;
                                tab.assistant.settings = value;
                                let prefix = format!("ai-{}-", tab.model.project.display());
                                self.prompts.retain(|key, _| {
                                    !key.starts_with(&prefix) || key.ends_with("-Question")
                                });
                            }
                            "settings.update" => {
                                if tab.assistant.provider_draft.as_deref()
                                    == value["settings"]["provider"].as_str()
                                {
                                    tab.assistant.provider_draft = None;
                                }
                                if value.get("destination").is_some() {
                                    tab.assistant.settings = value;
                                } else {
                                    tab.assistant.settings["settings"] = value["settings"].clone();
                                }
                                tab.assistant.status = "AI settings saved.".into();
                                tab.assistant.receipt = Value::Null;
                                tab.assistant.connection = assistant::Connection::Untested;
                            }
                            "approval.revoke" => {
                                tab.assistant.settings = value;
                                tab.assistant.receipt = Value::Null;
                                tab.assistant.rebuild = true;
                                tab.assistant.status =
                                    "Destination grant revoked; rebuild context to continue."
                                        .into();
                            }
                            "policy.update" => {
                                tab.assistant.settings["policy"] = value["policy"].clone();
                                tab.assistant.status = "Notebook AI policy saved.".into();
                                tab.assistant.receipt = Value::Null;
                            }
                            "proposal.resolve" => {
                                for proposal in &mut tab.assistant.proposals {
                                    if proposal["id"] == value["proposal_id"] {
                                        proposal["state"] = value["state"].clone();
                                    }
                                }
                                if value["state"] == "accepted" {
                                    let _ =
                                        tab.service.commands.send(Command::AssistantApply(value));
                                    tab.assistant.status =
                                        "Applying reviewed proposal through source/runtime contracts\u{2026}".into();
                                }
                            }
                            "application.record" => {
                                let applied = value["state"] == "applied";
                                tab.assistant.error = !applied;
                                tab.assistant.status = if applied {
                                    "Proposal applied and recorded."
                                } else if value["state"] == "unknown" {
                                    "Application outcome uncertain; inspect current source/runtime before continuing."
                                } else {
                                    "Application failed; inspect current source before retrying."
                                }
                                .into();
                                for p in &mut tab.assistant.proposals {
                                    if p["id"] == value["proposal_id"] {
                                        p["state"] = value["state"].clone();
                                    }
                                }
                            }
                            "conversations.list" => {
                                tab.inspections.insert("AI conversations".into(), value);
                            }
                            "conversation.get" => {
                                tab.assistant.transcript =
                                    value["messages"].as_array().cloned().unwrap_or_default();
                                tab.assistant.conversation =
                                    value["conversation_id"].as_str().map(str::to_owned);
                                tab.assistant.answer = Value::Null;
                                if let Some(last) = tab
                                    .assistant
                                    .transcript
                                    .iter()
                                    .rev()
                                    .find(|m| m["role"] == "assistant")
                                    .cloned()
                                {
                                    tab.assistant.answer = json!({"answer":last["text"],"citations":last["citations"],"request_id":last["request_id"]});
                                    if let Some(request) =
                                        value["requests"].as_array().and_then(|items| {
                                            items
                                                .iter()
                                                .find(|request| request["id"] == last["request_id"])
                                        })
                                    {
                                        tab.assistant.answer["warnings"] =
                                            request["warnings"].clone();
                                    }
                                }
                                if let Some(last_request) =
                                    value["requests"].as_array().and_then(|r| r.last())
                                {
                                    tab.assistant.task =
                                        last_request["task"].as_str().unwrap_or("ask").to_owned();
                                }
                                tab.assistant.receipt = Value::Null;
                                tab.assistant.proposals =
                                    value["proposals"].as_array().cloned().unwrap_or_default();
                                tab.assistant.status = if value["has_more"] == true { "Showing the first 200 requests; older/newer history may be outside this bounded view." } else { "Conversation loaded." }.into();
                            }
                            "proposals.list" => {
                                tab.assistant.proposals =
                                    value["items"].as_array().cloned().unwrap_or_default();
                                tab.assistant.status = format!(
                                    "Loaded {} recorded proposals.",
                                    tab.assistant.proposals.len()
                                );
                            }
                            "models.list" => {
                                let count = value["items"]
                                    .as_array()
                                    .or_else(|| value["models"].as_array())
                                    .map(|m| m.len())
                                    .unwrap_or(0);
                                tab.inspections.insert("AI models".into(), value);
                                tab.assistant.status = if count == 0 {
                                    "Endpoint returned no models.".into()
                                } else {
                                    format!("Discovered {count} models.")
                                };
                            }
                            "providers.test" => {
                                let ok = value["streaming"].as_bool() == Some(true)
                                    || value["ok"].as_bool() == Some(true);
                                if ok {
                                    tab.assistant.connection = assistant::Connection::Ready;
                                    tab.assistant.tested_profile = value["profile"].clone();
                                    let _ = tab.service.commands.send(Command::Assistant {
                                        method: "status".into(),
                                        fields: json!({}),
                                    });
                                    tab.assistant.status =
                                        "SSE and one JSON response observed; task/model quality is not qualified."
                                            .into();
                                } else {
                                    let message = assistant::redact_secrets(
                                        value["error"].as_str().unwrap_or("Provider unreachable"),
                                    );
                                    tab.assistant.status =
                                        format!("Provider test failed: {message}");
                                    tab.assistant.connection =
                                        assistant::Connection::Failed(message);
                                }
                            }
                            "search" => {
                                let count = value["items"]
                                    .as_array()
                                    .map(|items| items.len())
                                    .unwrap_or(0);
                                tab.inspections.insert("AI search".into(), value);
                                tab.assistant.status =
                                    format!("Found {count} matching ledger executions.");
                            }
                            "purge" => {
                                tab.assistant.conversation = None;
                                tab.assistant.transcript.clear();
                                tab.assistant.proposals.clear();
                                tab.assistant.answer = Value::Null;
                                tab.assistant.receipt = Value::Null;
                                tab.inspections.remove("AI conversations");
                                tab.inspections.remove("AI search");
                                tab.assistant.status = "Assistant records purged.".into();
                            }
                            "credential.set" | "credential.delete" => {
                                tab.assistant.status = "OS credential store updated.".into();
                                tab.assistant.connection = assistant::Connection::Untested;
                            }
                            _ => {
                                tab.assistant.status =
                                    serde_json::to_string_pretty(&value).unwrap_or_default()
                            }
                        }
                    }
                    Event::Source(snapshot) => {
                        if let Some(history) = snapshot.get("history") {
                            tab.inspections
                                .insert("Source history".into(), history.clone());
                        }
                        if let Ok(snapshot) = serde_json::from_value::<SourceSnapshot>(snapshot) {
                            tab.uninitialized = snapshot.state == "missing"
                                && !snapshot.details.contains_key("manifest");
                            let order = tab.model.order.clone();
                            let saved_app_mode = (!tab.model.manifest_revision.is_empty())
                                .then(|| tab.model.saved_app_mode());
                            let (app_mode, cell_metadata) =
                                (tab.app_mode, cell_metadata(&tab.model));
                            tab.model.apply_snapshot(snapshot);
                            // Adopt the saved App mode on open and on changes made
                            // elsewhere; a local toggle in flight is not reverted.
                            if tab.model.source_state == "ok"
                                && saved_app_mode != Some(tab.model.saved_app_mode())
                            {
                                tab.app_mode = tab.model.saved_app_mode();
                                if tab.app_mode {
                                    tab.model.mode = Mode::Command;
                                }
                            }
                            // Hidden code, disabled cells and App mode change cell heights.
                            if order == tab.model.order
                                && (app_mode != tab.app_mode
                                    || cell_metadata != self::cell_metadata(&tab.model))
                            {
                                tab.list.splice(0..order.len(), order.len());
                            }
                            if tab.model.source_state == "ok"
                                && self.recorded_opens.insert(tab.model.project.clone())
                            {
                                kernelledger_client::recent::record(
                                    &mut self.recent,
                                    tab.model.project.clone(),
                                    chrono::Utc::now().to_rfc3339(),
                                );
                                if !cfg!(test) {
                                    let recent = self.recent.clone();
                                    cx.background_executor()
                                        .spawn(async move {
                                            let _ = kernelledger_client::recent::save(
                                                &kernelledger_client::profile_path(),
                                                &recent,
                                            );
                                        })
                                        .detach();
                                }
                            }
                            if order != tab.model.order {
                                tab.list.reset(tab.model.order.len());
                            }
                            html_output::prune_tab(tab);
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
                        if let Some(history) = snapshot.get("history") {
                            tab.inspections
                                .insert("Source history".into(), history.clone());
                        }
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
                        let prev_state = tab.runtime["worker_state"].as_str().map(str::to_owned);
                        let prev_epoch = tab.runtime["interactive"]["namespace_epoch"].clone();
                        let prev_gen = tab.runtime["interactive"]["worker_generation"].clone();
                        if value["interactive"]["worker_generation"] != prev_gen {
                            tab.inspections.remove("Widget state");
                        }
                        if tab
                            .inspections
                            .get("Technical error")
                            .is_some_and(|v| v["code"] == "transport")
                        {
                            tab.error = None;
                            tab.inspections.remove("Technical error");
                        }
                        tab.runtime = value;
                        tab.model.live_session = tab.runtime["session_id"]
                            .as_str()
                            .filter(|s| !s.is_empty())
                            .map(str::to_owned);
                        let became_idle = prev_state.as_deref() != Some("idle");
                        let epoch_changed = tab.runtime["interactive"]["namespace_epoch"]
                            != prev_epoch
                            || tab.runtime["interactive"]["worker_generation"] != prev_gen;
                        if event_tab == self.active
                            && self.surface == "Variables"
                            && tab.runtime["worker_state"] == "idle"
                            && (became_idle || epoch_changed)
                            && tab.runtime["interactive"]["worker_generation"].is_string()
                        {
                            let _ = tab.service.commands.send(Command::Interactive {
                                method: "namespace.list".into(),
                                fields: json!({
                                    "worker_generation": tab.runtime["interactive"]["worker_generation"]
                                }),
                            });
                        }
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
                            cell.execution_seconds =
                                cell.exec_id.as_ref().and_then(|_| record_duration(&value));
                            cell.executed_revision =
                                value["source_sha256"].as_str().map(str::to_owned);
                            cell.ordinal = value["ordinal"].as_u64();
                            cell.outcome = value["status"].as_str().unwrap_or("unknown").into();
                            cell.session_id = value["session_id"].as_str().map(str::to_owned);
                            if let Some(i) = tab.model.order.iter().position(|cid| cid == id) {
                                tab.list.splice(i..i + 1, 1);
                            }
                        }
                        html_output::prune_tab(tab);
                    }
                    Event::Inspection { method, value } => {
                        if method == "helper.status" {
                            tab.error = None;
                            tab.inspections.remove("Technical error");
                            if !cfg!(test)
                                && !self.python.as_os_str().is_empty()
                                && value["python"]
                                    .as_str()
                                    .is_some_and(|p| std::path::Path::new(p) == self.python)
                                && std::env::var_os("KL_RUNTIME_PYTHON").is_none()
                            {
                                let python = self.python.clone();
                                cx.background_executor()
                                    .spawn(async move {
                                        let profile = kernelledger_client::profile_path();
                                        std::fs::create_dir_all(&profile)?;
                                        std::fs::write(
                                            profile.join("helper-python.txt"),
                                            python.to_string_lossy().as_bytes(),
                                        )
                                    })
                                    .detach();
                            }
                            tab.inspections.insert("Application Python".into(), value);
                            continue;
                        }
                        if method == "source.execution" {
                            tab.inspections.insert("Recorded source".into(), value);
                            if event_tab == self.active {
                                self.surface = "Recorded source".into();
                            }
                            continue;
                        }
                        if method == "output.saved" {
                            tab.inspections.insert("Export result".into(), value);
                            continue;
                        }
                        if method == "console.execution" {
                            tab.console_history.push(value.clone());
                            if tab.console_history.len() > 50 {
                                let excess = tab.console_history.len() - 50;
                                tab.console_history.drain(..excess);
                            }
                            tab.inspections.insert("Console output".into(), value);
                            continue;
                        }
                        if method == "display.status" {
                            tab.inspections.insert("Display updates".into(), value);
                            continue;
                        }
                        if method == "widget.state" {
                            if value["model"]["worker_generation"]
                                == tab.runtime["interactive"]["worker_generation"]
                                && let Some(handle) = value["model"]["handle"].as_str()
                            {
                                let states = tab
                                    .inspections
                                    .entry("Widget state".into())
                                    .or_insert_with(|| json!({}));
                                states[handle] = value["model"].clone();
                            }
                            continue;
                        }
                        if method == "source.preview" {
                            tab.inspections.insert("Source preview".into(), value);
                            self.surface = "Source preview".into();
                            continue;
                        }
                        if method == "debug.breakpoints" {
                            tab.inspections.insert("Breakpoints".into(), value);
                            continue;
                        }
                        if method == "files.list" {
                            tab.files_cache = Some(value);
                            continue;
                        }
                        if method == "packages.list" {
                            tab.packages_cache = Some(value);
                            continue;
                        }
                        if method == "packages.install" {
                            let ok = value["ok"].as_bool().unwrap_or(false);
                            let pkg = value["package"].as_str().unwrap_or("package");
                            if ok {
                                tab.package_install_status =
                                    Some(format!("Installed {pkg} successfully."));
                                let _ = tab.service.commands.send(Command::Inspect {
                                    method: "packages.list".into(),
                                    fields: json!({}),
                                });
                            } else {
                                let err = value["error"]
                                    .as_str()
                                    .or_else(|| value["stderr"].as_str())
                                    .unwrap_or("Package installation failed");
                                tab.package_install_status =
                                    Some(format!("Failed to install {pkg}: {err}"));
                            }
                            continue;
                        }
                        if method == "code.format" {
                            if let Some(cid) = tab.formatting_cell.take()
                                && let Some(formatted) = value["source"].as_str()
                                && let Some(cell) = tab.model.cells.get_mut(&cid)
                                && cell.text != formatted
                            {
                                let resized = editor_lines(&cell.text) != editor_lines(formatted);
                                cell.edit(formatted.to_owned());
                                if let Some(editor) = tab.editors.get(&cid) {
                                    editor.update(cx, |s, cx| {
                                        s.set_value(formatted.to_owned(), window, cx)
                                    });
                                }
                                if resized
                                    && let Some(i) = tab.model.order.iter().position(|c| *c == cid)
                                {
                                    tab.list.splice(i..i + 1, 1);
                                }
                                if let Some(request) = tab.model.save_request(&cid) {
                                    let _ = tab.service.commands.send(Command::Edit {
                                        cell_id: cid,
                                        request,
                                    });
                                }
                            }
                            continue;
                        }
                        if method == "code.inspect" {
                            tab.inspections.insert("Live docs".into(), value.clone());
                            if self.surface == "Console" {
                                continue;
                            }
                        }
                        if method.starts_with("namespace.") || method == "code.inspect" {
                            let key = if method == "namespace.list" {
                                "Variables"
                            } else {
                                "Variable preview"
                            };
                            if value["worker_generation"].is_null()
                                || value["worker_generation"]
                                    == tab.runtime["interactive"]["worker_generation"]
                            {
                                if method == "namespace.preview"
                                    && value["error"]["code"].as_str() == Some("name_not_found")
                                    && value["_context"]["segments"]
                                        .as_array()
                                        .is_some_and(|s| !s.is_empty())
                                    && let Some(handle) = value["_context"]["handle"].as_str()
                                {
                                    let _ = tab.service.commands.send(Command::Interactive {
                                        method: "namespace.preview".into(),
                                        fields: json!({
                                            "worker_generation": tab.runtime["interactive"]["worker_generation"],
                                            "handle": handle,
                                            "segments": [],
                                        }),
                                    });
                                    continue;
                                }
                                tab.inspections.insert(key.into(), value.clone());
                                if method == "namespace.list"
                                    && tab.runtime["worker_state"] == "idle"
                                    && let Some(selected_name) =
                                        tab.variables_state.selected.clone()
                                {
                                    if let Some(found) = value["items"].as_array().and_then(|arr| {
                                        arr.iter().find(|it| {
                                            it["name"].as_str() == Some(selected_name.as_str())
                                        })
                                    }) {
                                        let needs_refresh = tab
                                            .inspections
                                            .get("Variable preview")
                                            .is_none_or(|prev| {
                                                prev["name"].as_str()
                                                    != Some(selected_name.as_str())
                                                    || prev["namespace_epoch"]
                                                        != value["namespace_epoch"]
                                                    || prev["worker_generation"]
                                                        != value["worker_generation"]
                                            });
                                        if needs_refresh
                                            && let Some(handle) = found["handle"].as_str()
                                        {
                                            let segments = tab
                                                .inspections
                                                .get("Variable preview")
                                                .filter(|prev| {
                                                    prev["name"].as_str()
                                                        == Some(selected_name.as_str())
                                                })
                                                .and_then(|prev| prev["segments"].as_array())
                                                .cloned()
                                                .unwrap_or_default();
                                            let mut fields = json!({
                                                "worker_generation": tab.runtime["interactive"]["worker_generation"],
                                                "handle": handle,
                                            });
                                            if !segments.is_empty() {
                                                fields["segments"] = Value::Array(segments);
                                            }
                                            let _ =
                                                tab.service.commands.send(Command::Interactive {
                                                    method: "namespace.preview".into(),
                                                    fields,
                                                });
                                        }
                                    } else {
                                        tab.variables_state.selected = None;
                                        tab.inspections.remove("Variable preview");
                                    }
                                }
                            }
                            continue;
                        }
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
                            "execution.get" | "checkpoint.get" | "training.inspect"
                        ) || (method == "executions.list"
                            && tab
                                .inspection_method
                                .get(&surface)
                                .is_some_and(|previous| previous != &method))
                            || (matches!(surface.as_str(), "History" | "Checkpoints")
                                && !value["_page_request"]["cursor"].is_null()
                                && tab.inspections.get(&surface).is_some_and(|previous| {
                                    previous["_page_request"] != value["_page_request"]
                                })))
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
                        digest,
                        offset,
                        end,
                        total,
                    } => {
                        if let Some(cell) = tab.model.cells.get_mut(&cell_id)
                            && cell.exec_id.as_deref() == Some(exec_id.as_str())
                            && let Some(event) = cell.output.get_mut(index)
                            && kernelledger_client::output::matches_blob(event, &slot, &digest)
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
                        for surface in ["History", "Console output"] {
                            if let Some(history) = tab.inspections.get_mut(surface)
                                && history["exec_id"] == exec_id
                                && let Some(event) = history["outputs"].get_mut(index)
                                && kernelledger_client::output::matches_blob(event, &slot, &digest)
                            {
                                kernelledger_client::output::write_page(
                                    event, &slot, &text, offset, end, total,
                                );
                            }
                        }
                        for record in &mut tab.console_history {
                            if record["exec_id"] == exec_id
                                && let Some(event) = record["outputs"].get_mut(index)
                                && kernelledger_client::output::matches_blob(event, &slot, &digest)
                            {
                                kernelledger_client::output::write_page(
                                    event, &slot, &text, offset, end, total,
                                );
                            }
                        }
                    }
                    Event::ImageCached {
                        cell_id,
                        exec_id,
                        index,
                        mime,
                        path,
                        digest,
                    } => {
                        let assign = |event: &mut Value| {
                            if event["bundle"][&mime]["blob"] == digest {
                                event["bundle"][&mime]["local_path"] = json!(path.clone());
                            }
                        };
                        if let Some(cell) = tab.model.cells.get_mut(&cell_id)
                            && cell.exec_id.as_deref() == Some(exec_id.as_str())
                            && let Some(event) = cell.output.get_mut(index)
                        {
                            assign(event);
                            if let Some(position) =
                                tab.model.order.iter().position(|id| id == &cell_id)
                            {
                                tab.list.splice(position..position + 1, 1);
                            }
                        }
                        for surface in ["History", "Console output"] {
                            if let Some(history) = tab.inspections.get_mut(surface)
                                && history["exec_id"] == exec_id
                                && let Some(event) = history["outputs"].get_mut(index)
                            {
                                assign(event);
                            }
                        }
                        for record in &mut tab.console_history {
                            if record["exec_id"] == exec_id
                                && let Some(event) = record["outputs"].get_mut(index)
                            {
                                assign(event);
                            }
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
                        tab.error = Some(
                            if code == "helper_startup" || code == "helper_validation" {
                                let detail = message
                                    .lines()
                                    .find(|line| line.contains("No module named"))
                                    .unwrap_or("Choose a Python environment with KernelLedger installed, then retry.");
                                let status = if code == "helper_validation" {
                                    "Selected Python could not load notebook services; the current connection is unchanged."
                                } else {
                                    "Notebook services could not start."
                                };
                                format!("{status} {}", bounded(detail, 300))
                            } else {
                                error_message(&code).into()
                            },
                        );
                    }
                }
            }
        }
        for (index, key, token, result) in html_ready {
            let tab = &mut self.tabs[index];
            if !tab
                .html_states
                .get(&key)
                .is_some_and(|state| state.token == token)
            {
                continue;
            }
            match result {
                Ok(html) if index == self.active => self.start_html_output(key, html, window, cx),
                Ok(_) => {
                    tab.html_states.remove(&key);
                    tab.html_notes
                        .insert(key, "Press Render HTML again on this tab.".into());
                }
                Err(error) => {
                    tab.html_states.remove(&key);
                    tab.html_notes
                        .insert(key, format!("HTML could not load: {error}"));
                }
            }
        }
        while let Ok(signal) = self.html_signal_receiver.try_recv() {
            for tab in &mut self.tabs {
                if let Some(state) = tab.html_states.get_mut(&signal.key)
                    && state.token == signal.token
                {
                    match signal.value["kind"].as_str() {
                        Some("height") => {
                            if let Some(height) = signal.value["height"].as_f64()
                                && height.is_finite()
                            {
                                let height = (height as f32).clamp(160., 800.);
                                if (state.height - height).abs() > 2. {
                                    state.height = height;
                                    tab.list
                                        .splice(0..tab.model.order.len(), tab.model.order.len());
                                }
                            }
                        }
                        Some("ready") if !state.failed => state.status.clear(),
                        Some("error" | "blocked") => {
                            state.failed = true;
                            state.status = if signal.value["kind"] == "blocked" { "Some page resources were blocked by the HTML sandbox or network access settings." } else { "Some page scripts or resources failed to load. Check HTML network access and the page's dependencies." }.into();
                        }
                        _ => {}
                    }
                    changed = true;
                }
            }
        }
        if changed {
            self.apply_output_vars(cx);
        }
        changed |= self.apply_reveal(window, cx);
        if changed {
            cx.notify();
        }
    }
    /// Selects, scrolls to and edits the pending reveal target once it exists.
    /// App mode only selects and scrolls: it draws no editor.
    fn apply_reveal(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let Some(tab) = self.tabs.get_mut(self.active) else {
            return false;
        };
        let Some(id) = tab.reveal.clone() else {
            return false;
        };
        let Some(index) = tab.model.order.iter().position(|c| c == &id) else {
            return false;
        };
        tab.reveal = None;
        tab.model.selected = Some(id.clone());
        if tab.list.bounds_for_item(index).is_some() {
            tab.list.scroll_to_reveal_item(index);
        } else {
            // Height estimates are absent after a structural reset. Anchor
            // directly to an unmeasured target instead of revealing its estimate.
            tab.list.scroll_to(ListOffset {
                item_ix: index,
                offset_in_item: px(0.),
            });
        }
        if !tab.app_mode {
            tab.model.mode = Mode::Edit;
            let editor = self.editor(self.active, &id, window, cx);
            editor.update(cx, |s, cx| s.focus(window, cx));
        }
        true
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
    fn retry_application_python(&mut self, cx: &mut Context<Self>) {
        let Some(tab) = self.tabs.get_mut(self.active) else {
            return;
        };
        if let Some(path) = tab
            .inspections
            .get("Application Python")
            .and_then(|v| v["python"].as_str())
        {
            let path = if self.python.as_os_str().is_empty() {
                PathBuf::from(path)
            } else {
                self.python.clone()
            };
            let _ = tab
                .service
                .commands
                .send(Command::SelectApplicationPython(path));
            cx.notify();
            return;
        }
        // Preserve editors, drafts and durable operation keys; never resubmit work.
        let wake = self.wake_notifier();
        let tab = &mut self.tabs[self.active];
        tab.service =
            NotebookService::start_with_wake(tab.model.project.clone(), self.python.clone(), wake);
        tab.error = None;
        tab.inspections.remove("Technical error");
        tab.inspections.remove("Application Python");
        tab.runtime["connection_lost"] = json!(false);
        cx.notify();
    }

    fn choose_application_python(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let focus = window.focused(cx).unwrap_or_else(|| self.focus.clone());
        let paths = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: true,
            multiple: false,
            prompt: Some("Choose a Python environment folder or executable".into()),
        });
        cx.spawn_in(window, async move |view, window| {
            let selected = paths.await;
            let _ = view.update_in(window, |v, window, cx| {
                focus.focus(window, cx);
                if let Ok(Ok(Some(paths))) = selected
                    && let Some(path) = paths.into_iter().next()
                {
                    v.python = python_environment_executable(path);
                    v.retry_application_python(cx);
                }
            });
        })
        .detach();
    }

    fn choose_interpreter(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let paths = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: true,
            multiple: false,
            prompt: Some("Choose a CPython environment folder or executable".into()),
        });
        cx.spawn_in(window, async move |view, window| {
            if let Ok(Ok(Some(paths))) = paths.await
                && let Some(path) = paths.into_iter().next()
            {
                let _ = view.update_in(window, |v, _, _| {
                    v.send(Command::SelectInterpreter {
                        path: python_environment_executable(path),
                        remember: false,
                    })
                });
            }
        })
        .detach();
    }
    #[inline(never)]
    fn sidebar(&self, cx: &mut Context<Self>) -> AnyElement {
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
            success,
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
            theme.success,
            theme.transparent,
        );
        let mono = theme.mono_font_family.clone();
        let tab = &self.tabs[self.active];
        let active_tab = tab.sidebar_tab.as_str();
        let graph = dag::analyze_notebook(&tab.model);

        let tab_bar = h_flex()
            .flex_none()
            .px_2()
            .pt_2()
            .pb_1()
            .gap_1()
            .items_center()
            .border_b_1()
            .border_color(divider(cx))
            .child(
                action_button("sidebar-tab-outline")
                    .small()
                    .segment(active_tab == "outline")
                    .label("Outline")
                    .on_click(cx.listener(|v, _, _, cx| {
                        v.tabs[v.active].sidebar_tab = "outline".into();
                        cx.notify();
                    })),
            )
            .child(
                action_button("sidebar-tab-deps")
                    .small()
                    .segment(active_tab == "dependencies")
                    .label(if graph.run_stale_ids.is_empty() {
                        "Deps".into()
                    } else {
                        format!("Deps ({})", graph.run_stale_ids.len())
                    })
                    .on_click(cx.listener(|v, _, _, cx| {
                        v.tabs[v.active].sidebar_tab = "dependencies".into();
                        cx.notify();
                    })),
            )
            .child(
                action_button("sidebar-tab-files")
                    .small()
                    .segment(active_tab == "files")
                    .label("Files")
                    .on_click(cx.listener(|v, _, _, cx| {
                        v.tabs[v.active].sidebar_tab = "files".into();
                        if v.tabs[v.active].files_cache.is_none() {
                            v.inspect("files.list", json!({}));
                        }
                        cx.notify();
                    })),
            );

        if active_tab == "dependencies" {
            let mut dep_list = v_flex()
                .id("deps-list")
                .flex_1()
                .min_h_0()
                .overflow_y_scroll()
                .px_2()
                .py_2()
                .gap_2();
            dep_list = dep_list.child(
                h_flex()
                    .gap_1p5()
                    .flex_wrap()
                    .items_center()
                    .child(badge(format!("{} edges", graph.edges.len()), muted_fg))
                    .when(!graph.run_stale_ids.is_empty(), |d| {
                        d.child(badge(
                            format!("{} stale", graph.run_stale_ids.len()),
                            warning,
                        ))
                    })
                    .when(!graph.cycles.is_empty(), |d| {
                        d.child(badge(format!("{} cycle", graph.cycles.len()), danger))
                    })
                    .when(!graph.multiple_definitions.is_empty(), |d| {
                        d.child(badge(
                            format!("{} redef", graph.multiple_definitions.len()),
                            danger,
                        ))
                    }),
            );
            if !graph.run_stale_ids.is_empty() {
                dep_list = dep_list.child(
                    action_button("sidebar-run-stale")
                        .primary()
                        .small()
                        .w_full()
                        .icon(Glyph::Play)
                        .label(format!("Run stale ({})", graph.run_stale_ids.len()))
                        .on_click(cx.listener(|v, _, _, cx| v.run_stale_cells(cx))),
                );
            }
            if !graph.cycles.is_empty() {
                let summary = graph
                    .cycles
                    .iter()
                    .map(|c| c.join(" \u{2192} "))
                    .collect::<Vec<_>>()
                    .join("; ");
                dep_list = dep_list.child(
                    div()
                        .p_2()
                        .rounded(radius)
                        .border_1()
                        .border_color(danger.opacity(0.35))
                        .bg(danger.opacity(0.08))
                        .text_xs()
                        .text_color(danger)
                        .whitespace_normal()
                        .child(format!("Dependency cycle: {summary}")),
                );
            }
            for (i, id) in tab.model.order.iter().enumerate() {
                let Some(cell) = tab.model.cells.get(id) else {
                    continue;
                };
                if !cell.is_executable() {
                    continue;
                }
                let defs = graph.defs.get(id).map(Vec::as_slice).unwrap_or(&[]);
                let refs = graph.refs.get(id).map(Vec::as_slice).unwrap_or(&[]);
                let upstream_stale = graph.is_upstream_stale(id);
                let direct_stale = cell.stale();
                let selected = tab.model.selected.as_deref() == Some(id.as_str());
                let jump_id = id.clone();
                dep_list = dep_list.child(
                    v_flex()
                        .id(("dep-cell", i))
                        .p_2()
                        .gap_1()
                        .rounded(radius)
                        .border_1()
                        .border_color(if selected { primary } else { surface_edge(cx) })
                        .when(selected, |d| d.bg(active_bg))
                        .when(!selected, |d| d.hover(move |s| s.bg(hover_bg)))
                        .cursor_pointer()
                        .on_click(cx.listener(move |v, _, _, cx| {
                            v.jump_to_cell(jump_id.clone(), cx);
                        }))
                        .child(
                            h_flex()
                                .gap_1p5()
                                .items_center()
                                .child(Icon::new(kind_icon(&cell.disk.kind)).xsmall())
                                .child(
                                    div()
                                        .flex_1()
                                        .min_w_0()
                                        .truncate()
                                        .font_family(mono.clone())
                                        .text_xs()
                                        .font_weight(FontWeight::MEDIUM)
                                        .child(format!("Cell {} ({id})", i + 1)),
                                )
                                .when(direct_stale || upstream_stale, |d| d.child(dot(warning))),
                        )
                        .when(!defs.is_empty(), |d| {
                            d.child(
                                h_flex()
                                    .gap_1()
                                    .flex_wrap()
                                    .items_center()
                                    .child(
                                        div()
                                            .text_size(rems(0.6875))
                                            .text_color(muted_fg)
                                            .child("defs:"),
                                    )
                                    .children(defs.iter().take(6).map(|name| {
                                        div()
                                            .px_1()
                                            .rounded(px(4.))
                                            .bg(success.opacity(0.14))
                                            .font_family(mono.clone())
                                            .text_size(rems(0.6875))
                                            .text_color(success)
                                            .child(name.clone())
                                    })),
                            )
                        })
                        .when(!refs.is_empty(), |d| {
                            d.child(
                                h_flex()
                                    .gap_1()
                                    .flex_wrap()
                                    .items_center()
                                    .child(
                                        div()
                                            .text_size(rems(0.6875))
                                            .text_color(muted_fg)
                                            .child("refs:"),
                                    )
                                    .children(refs.iter().take(6).map(|name| {
                                        div()
                                            .px_1()
                                            .rounded(px(4.))
                                            .bg(info.opacity(0.12))
                                            .font_family(mono.clone())
                                            .text_size(rems(0.6875))
                                            .text_color(info)
                                            .child(name.clone())
                                    })),
                            )
                        }),
                );
            }
            return v_flex()
                .id("sidebar")
                .debug_selector(|| "notebook-outline".into())
                .track_focus(&self.sidebar_focus)
                .size_full()
                .bg(bar)
                .child(tab_bar)
                .child(dep_list)
                .into_any_element();
        }

        if active_tab == "files" {
            let files = tab
                .files_cache
                .as_ref()
                .and_then(|v| v["items"].as_array())
                .cloned()
                .unwrap_or_default();
            let mut file_list = v_flex()
                .id("files-list")
                .flex_1()
                .min_h_0()
                .overflow_y_scroll()
                .px_2()
                .py_2()
                .gap_1();
            file_list = file_list.child(
                h_flex()
                    .justify_between()
                    .items_center()
                    .pb_1()
                    .child(
                        div()
                            .text_xs()
                            .text_color(muted_fg)
                            .child(format!("{} file(s)", files.len())),
                    )
                    .child(
                        action_button("refresh-workspace-files")
                            .ghost()
                            .small()
                            .icon(Glyph::RefreshCw)
                            .hint("Refresh workspace files")
                            .on_click(cx.listener(|v, _, _, _| {
                                v.inspect("files.list", json!({}));
                            })),
                    ),
            );
            if files.is_empty() {
                file_list = file_list.child(
                    div()
                        .py_4()
                        .text_center()
                        .text_xs()
                        .text_color(muted_fg)
                        .child("No workspace files found."),
                );
            } else {
                for (i, item) in files.iter().enumerate() {
                    let rel = item["path"].as_str().unwrap_or_default().to_string();
                    let name = item["name"].as_str().unwrap_or(&rel).to_string();
                    let size = item["size"].as_u64().unwrap_or(0);
                    // Generated here, not by the helper, so each loader binds its own
                    // variable and the path is always a safely escaped literal.
                    let loadable = codegen::loader_source(&rel, &Default::default()).is_some();
                    file_list = file_list.child(
                        h_flex()
                            .id(("ws-file", i))
                            .px_2()
                            .py_1()
                            .gap_1p5()
                            .items_center()
                            .rounded(radius)
                            .hover(move |s| s.bg(hover_bg))
                            .child(Icon::new(Glyph::FileText).xsmall().text_color(muted_fg))
                            .child(
                                v_flex()
                                    .flex_1()
                                    .min_w_0()
                                    .child(
                                        div()
                                            .truncate()
                                            .text_xs()
                                            .font_family(mono.clone())
                                            .child(name),
                                    )
                                    .child(
                                        div()
                                            .truncate()
                                            .text_size(rems(0.6875))
                                            .text_color(muted_fg)
                                            .child(format!(
                                                "{rel} \u{00b7} {}",
                                                runtime::format_rss_bytes(size)
                                            )),
                                    ),
                            )
                            .when(loadable, |d| {
                                d.child(
                                    action_button(("load-file", i))
                                        .ghost()
                                        .small()
                                        .label("Load")
                                        .hint("Insert loader cell for this file")
                                        .on_click(cx.listener(move |v, _, _, cx| {
                                            let taken = notebook_names(&v.tabs[v.active].model);
                                            if let Some(code) = codegen::loader_source(&rel, &taken)
                                            {
                                                v.insert_cell_with_source("code", code, cx);
                                            }
                                        })),
                                )
                            }),
                    );
                }
            }
            return v_flex()
                .id("sidebar")
                .debug_selector(|| "notebook-outline".into())
                .track_focus(&self.sidebar_focus)
                .size_full()
                .bg(bar)
                .child(tab_bar)
                .child(file_list)
                .into_any_element();
        }

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
        let blocked = blocked_cells(&tab.model, &graph);
        for (i, id) in tab.model.order.iter().enumerate() {
            let cell = &tab.model.cells[id];
            let state = OutlineState::of(cell, blocked.contains(id));
            if !query.is_empty()
                && !cell.text.to_lowercase().contains(&query)
                && !id.to_lowercase().contains(&query)
            {
                continue;
            }
            shown += 1;
            let selected = tab.model.selected.as_deref() == Some(id.as_str());
            let first_line = cell.text.lines().map(str::trim).find(|l| !l.is_empty());
            // Prose reads by its heading, and sub-headings nest; code stays literal.
            let heading = (cell.disk.kind == "markdown")
                .then(|| first_line.map_or(0, |l| l.chars().take_while(|c| *c == '#').count()))
                .filter(|level| (1..=6).contains(level));
            let preview = first_line.map(|l| {
                let text = if cell.disk.kind == "markdown" {
                    l.trim_start_matches('#').trim_start()
                } else {
                    l
                };
                text.chars().take(80).collect::<String>()
            });
            let is_exec = cell.is_executable();
            let upstream_stale = graph.is_upstream_stale(id);
            let marker = if matches!(cell.save, SaveState::Conflict | SaveState::Failed)
                || matches!(cell.outcome.as_str(), "error" | "failed")
            {
                Some(danger)
            } else if cell.stale() || upstream_stale {
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
                        } else if cell.stale() || upstream_stale {
                            ", stale"
                        } else {
                            ""
                        };
                        format!(
                            "Cell {}: {} cell {id}: {}{status}{}",
                            i + 1,
                            cell.disk.kind,
                            cell.text
                                .lines()
                                .find(|l| !l.trim().is_empty())
                                .unwrap_or("Empty cell"),
                            state
                                .map(|s| format!(", {}", s.label()))
                                .unwrap_or_default()
                        )
                    })
                    .aria_selected(selected)
                    .gap_2()
                    .px_2()
                    .py_1()
                    .items_center()
                    .rounded(radius)
                    .when(heading.is_some_and(|level| level > 1), |d| d.ml_3())
                    // Fill and icon carry selection; contrast mode adds the bar.
                    .when(self.high_contrast, |d| {
                        d.border_l_2()
                            .border_color(if selected { primary } else { transparent })
                    })
                    .cursor_pointer()
                    .when(selected, |d| d.bg(active_bg))
                    .when(!selected, |d| d.hover(move |s| s.bg(hover_bg)))
                    .when(state.is_some_and(OutlineState::muted), |d| d.opacity(0.55))
                    .child(
                        div()
                            .flex_none()
                            .text_color(if selected { primary } else { muted_fg })
                            .child(Icon::new(kind_icon(&cell.disk.kind)).small()),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .when(is_exec, |d| {
                                d.font_family(mono.clone())
                                    .text_size(rems(0.75))
                                    .text_color(muted_fg)
                            })
                            .when(!is_exec, |d| d.text_size(rems(0.8125)))
                            .when(heading.is_some(), |d| d.font_weight(FontWeight::SEMIBOLD))
                            .when(selected, |d| {
                                d.font_weight(FontWeight::MEDIUM)
                                    .text_color(cx.theme().foreground)
                            })
                            .when(preview.is_none(), |d| d.italic().text_color(muted_fg))
                            .child(preview.unwrap_or_else(|| {
                                format!("Empty {} cell", kind_label(&cell.disk.kind))
                            })),
                    )
                    .when_some(state, |d, state| {
                        d.child(
                            div()
                                .id(("outline-state", i))
                                .debug_selector({
                                    let id = id.clone();
                                    move || format!("outline-state-{id}")
                                })
                                .flex_none()
                                .text_color(muted_fg)
                                .child(Icon::new(state.glyph()).xsmall())
                                .tooltip(move |_, cx| {
                                    cx.new(|_| WorkbenchHint(state.hint().into())).into()
                                }),
                        )
                    })
                    .child(
                        h_flex().flex_none().w(px(28.)).justify_end().child(
                            div()
                                .text_size(rems(0.6875))
                                .font_family(mono.clone())
                                .text_color(muted_fg)
                                .child(if is_exec {
                                    cell.ordinal
                                        .map(|n| format!("[{n}]"))
                                        .unwrap_or_else(|| "\u{2014}".into())
                                } else {
                                    String::new()
                                }),
                        ),
                    )
                    .child(dot(marker.unwrap_or(transparent)))
                    .on_click(cx.listener(move |v, _, window, cx| {
                        v.surface = "Notebook".into();
                        let t = &mut v.tabs[v.active];
                        t.model.selected = Some(id.clone());
                        t.list.scroll_to_reveal_item(i);
                        v.focus.focus(window, cx);
                        v.request_reveal(Some(id.clone()));
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
                    .child(
                        if tab.model.source_state != "ok" && tab.model.order.is_empty() {
                            "Source not loaded"
                        } else if tab.model.order.is_empty() {
                            "No cells yet"
                        } else {
                            "No cells match"
                        },
                    ),
            );
        }
        v_flex()
            .id("sidebar")
            .debug_selector(|| "notebook-outline".into())
            .track_focus(&self.sidebar_focus)
            .size_full()
            .bg(bar)
            .child(tab_bar)
            .child(
                h_flex()
                    .flex_none()
                    .px_3()
                    .pt_2()
                    .pb_1()
                    .justify_between()
                    .items_center()
                    .child(
                        div()
                            .text_xs()
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(muted_fg)
                            .child("Outline"),
                    )
                    .child(
                        div()
                            .text_xs()
                            .font_family(mono)
                            .text_color(muted_fg)
                            .child(tab.model.order.len().to_string()),
                    ),
            )
            .when(!tab.model.order.is_empty(), |d| {
                d.child(
                    div().flex_none().px_2().pb_1().child(
                        Input::new(&self.query)
                            .small()
                            .cleanable(true)
                            .prefix(Icon::new(Glyph::Search).small().text_color(muted_fg)),
                    ),
                )
            })
            .child(list)
            .into_any_element()
    }
}
impl Render for Workspace {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.conceal_html_views(cx);
        self.sync_confirmation_focus(window, cx);
        let active_id = self.tabs.get(self.active).map(|tab| tab.id);
        if self.history_owner != active_id && !self.tabs.is_empty() {
            let previous_query = self.history_query.read(cx).value().to_string();
            if let Some(old) = self.history_owner.and_then(|id| self.tab_by_id_mut(id)) {
                old.history_filter = previous_query;
            }
            let query = self.tabs[self.active].history_filter.clone();
            self.history_query
                .update(cx, |state, cx| state.set_value(query, window, cx));
            self.history_owner = active_id;
            self.history_controls.clear();
            self.history_field_subscriptions.clear();
            self.history_fields_key.clear();
        }
        let presentation = WorkbenchPresentation {
            comfortable: self.preferences.comfortable,
            high_contrast: self.high_contrast,
        };
        if self.applied_presentation != Some(presentation) {
            cx.set_global(presentation);
            self.applied_presentation = Some(presentation);
        }
        let rem_size = px(16. * self.preferences.ui_scale);
        if window.rem_size() != rem_size {
            window.set_rem_size(rem_size);
        }
        let font_size = px(14. * self.preferences.ui_scale);
        let mono_font_size =
            output_font_size(self.preferences.editor_size, self.preferences.ui_scale);
        if cx.theme().font_size != font_size || cx.theme().mono_font_size != mono_font_size {
            let theme = Theme::global_mut(cx);
            theme.font_size = font_size;
            theme.mono_font_size = mono_font_size;
            Theme::sync_base(cx);
        }
        self.compact = window.viewport_size().width < px(1024. * self.preferences.ui_scale);
        // Settings owns its full form width. Keep the conversation mounted in
        // state so returning to the notebook restores its draft and context.
        let show_assistant = self.surface != "Settings"
            && self
                .tabs
                .get(self.active)
                .is_some_and(|tab| tab.assistant.open);
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
        if self.applied_title != title {
            window.set_window_title(&title);
            self.applied_title = title;
        }
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
            .on_action(cx.listener(Self::toggle_app_mode))
            .on_action(
                cx.listener(|v, action: &shortcuts::WorkbenchCommand, window, cx| {
                    if !v.palette
                        && v.confirmation.is_none()
                        && !v.has_menu()
                        && (!matches!(action.method.as_str(), "undo-source" | "redo-source")
                            || v.focus.is_focused(window))
                    {
                        v.run_command(&action.method, window, cx);
                    }
                }),
            )
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
            let app_mode = self.tabs[self.active].app_mode;
            let center = if self.surface != "Notebook" {
                self.panel(window, cx)
            } else if self.tabs[self.active].model.order.is_empty() {
                self.empty_notebook(cx)
            } else {
                let state = self.tabs[self.active].list.clone();
                let has_executed = self.tabs[self.active]
                    .model
                    .cells
                    .values()
                    .any(|c| c.executed_revision.is_some());
                let stale_count = dag::analyze_notebook(&self.tabs[self.active].model)
                    .run_stale_ids
                    .len();
                let view = cx.entity();
                let cell_list = list(state, move |i, window, cx| {
                    view.update(cx, |view, cx| view.cell(i, window, cx))
                })
                .size_full();
                if app_mode || (has_executed && stale_count > 0) {
                    let strip = if app_mode {
                        h_flex()
                            .flex_none()
                            .w_full()
                            .px_4()
                            .py_1p5()
                            .gap_2()
                            .items_center()
                            .bg(cx.theme().info.opacity(0.08))
                            .border_b_1()
                            .border_color(cx.theme().info.opacity(0.25))
                            .child(Icon::new(Glyph::Eye).small().text_color(cx.theme().info))
                            .child(
                                div()
                                    .flex_1()
                                    .text_xs()
                                    .font_weight(FontWeight::MEDIUM)
                                    .child(
                                        "App / Report mode \u{2014} code and cell controls hidden",
                                    ),
                            )
                            .child(
                                action_button("exit-app-mode-bar")
                                    .outline()
                                    .small()
                                    .label("Exit App mode")
                                    .on_click(cx.listener(|v, _, window, cx| {
                                        v.toggle_app_mode(&ToggleAppMode, window, cx)
                                    })),
                            )
                    } else {
                        h_flex()
                            .flex_none()
                            .w_full()
                            .px_4()
                            .py_1()
                            .gap_2()
                            .items_center()
                            .bg(cx.theme().warning.opacity(0.08))
                            .border_b_1()
                            .border_color(cx.theme().warning.opacity(0.25))
                            .child(dot(cx.theme().warning))
                            .child(div().flex_1().text_xs().child(format!(
                                "{stale_count} stale cell{} need re-running to stay in sync",
                                if stale_count == 1 { "" } else { "s" }
                            )))
                            .child(
                                action_button("notebook-open-deps")
                                    .ghost()
                                    .small()
                                    .label("Dependencies")
                                    .on_click(cx.listener(|v, _, window, cx| {
                                        v.run_command("sidebar-deps", window, cx)
                                    })),
                            )
                            .child(
                                action_button("notebook-run-stale")
                                    .primary()
                                    .small()
                                    .icon(Glyph::Play)
                                    .label(format!("Run stale ({stale_count})"))
                                    .on_click(cx.listener(|v, _, _, cx| v.run_stale_cells(cx))),
                            )
                    };
                    v_flex()
                        .size_full()
                        .min_h_0()
                        .child(strip)
                        .child(div().flex_1().min_h_0().child(cell_list))
                        .into_any_element()
                } else {
                    cell_list.into_any_element()
                }
            };
            let viewport_width = window.viewport_size().width / px(1.);
            let scale = self.preferences.ui_scale;
            let min_notebook_w = 420.;
            let min_drawer_w = 280.;
            let compact_assistant_sibling =
                show_assistant && self.compact && viewport_width >= min_notebook_w + min_drawer_w;
            let compact_assistant_overlay =
                show_assistant && self.compact && !compact_assistant_sibling;
            let compact_drawer_width = if compact_assistant_sibling {
                (viewport_width - min_notebook_w)
                    .min((360. * scale).min(440.))
                    .max(min_drawer_w)
            } else {
                (360. * scale)
                    .min(440.)
                    .min(viewport_width.max(min_drawer_w))
            };
            let center = if compact_assistant_overlay {
                div()
                    .size_full()
                    .min_w_0()
                    .pr(px(compact_drawer_width))
                    .child(center)
                    .into_any_element()
            } else {
                center
            };
            let show_sidebar =
                self.surface == "Notebook" && !self.compact && self.sidebar_visible && !app_mode;
            root = root
                .child(self.banners(window, cx))
                .child(
                    h_flex()
                        .flex_1()
                        .min_h_0()
                        .relative()
                        .child(
                            h_resizable(SharedString::from(format!(
                                "notebook-layout-v5-{}-{}-{}-{}-{}",
                                self.preferences.outline_width,
                                self.surface,
                                show_sidebar,
                                show_assistant && !self.compact,
                                compact_assistant_sibling
                            )))
                            .when(show_sidebar, |d| {
                                d.child(
                                    resizable_panel()
                                        .size(px(self.preferences.outline_width))
                                        .size_range(px(180.)..px(300.))
                                        .child(self.sidebar(cx)),
                                )
                            })
                            .child(
                                resizable_panel()
                                    .size_range(if compact_assistant_sibling {
                                        px(min_notebook_w)..px(4096.)
                                    } else {
                                        px(0.)..px(4096.)
                                    })
                                    .child(center),
                            )
                            .when(show_assistant && !self.compact, |d| {
                                // Keep the notebook column readable when the
                                // window is only just above the compact break
                                // (live UX B.5 / H.3).
                                let narrow = window.viewport_size().width
                                    < px(1280. * self.preferences.ui_scale);
                                d.child(
                                    resizable_panel()
                                        .size(px(if narrow { 320. } else { 400. }))
                                        .size_range(px(280.)..px(560.))
                                        .child(self.assistant_panel(window, cx)),
                                )
                            })
                            .when(compact_assistant_sibling, |d| {
                                d.child(
                                    resizable_panel()
                                        .size(px(compact_drawer_width))
                                        .size_range(
                                            px(min_drawer_w)
                                                ..px((360. * scale).min(440.).max(min_drawer_w)),
                                        )
                                        .child(self.assistant_panel(window, cx)),
                                )
                            })
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
                        )
                        .when(compact_assistant_overlay, |d| {
                            d.child(
                                div()
                                    .absolute()
                                    .right_0()
                                    .top_0()
                                    .bottom_0()
                                    .w(px(compact_drawer_width))
                                    .max_w_full()
                                    .bg(cx.theme().background)
                                    .border_l_1()
                                    .border_color(divider(cx))
                                    .when(!self.high_contrast, |d| d.shadow_lg())
                                    .child(self.assistant_panel(window, cx)),
                            )
                        }),
                )
                .child(self.status_bar(cx));
        }
        if self.has_menu() {
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
                    .rounded(px(6.))
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
        gpui_kit::image_cache(images::workspace_cache("workspace-images"))
            .size_full()
            .child(root)
            .into_any_element()
    }
}
fn kind_label(kind: &str) -> &'static str {
    match kind {
        "code" => "Python",
        "sql" => "SQL",
        "markdown" => "Markdown",
        _ => "Raw",
    }
}
fn kind_icon(kind: &str) -> Glyph {
    match kind {
        "code" => Glyph::Code,
        "sql" => Glyph::Database,
        "markdown" => Glyph::Type,
        _ => Glyph::FileText,
    }
}
fn editor_lines(text: &str) -> usize {
    text.split('\n').count().clamp(1, 30)
}
fn editor_height(text: &str, font_size: f32, ui_scale: f32) -> Pixels {
    // The editor facade reserves 8 logical pixels on each vertical edge.
    px(editor_lines(text) as f32 * font_size * 1.5 + 18. * ui_scale)
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
fn kbd(keys: &'static str) -> Option<Kbd> {
    Keystroke::parse(shortcuts::platform_binding(keys))
        .ok()
        .map(Kbd::new)
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
        worker_generation: None,
        project: None,
        workspace: None,
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
            "Force-stops the worker. Live variables and running work are lost; saved source and history are kept.",
            "Terminate worker",
        ),
        "training-restore" => (
            "Restore training for",
            "Runs the saved reconstruction code in a fresh worker and replaces your live namespace with the restored training state. If reconstruction fails, it preserves the current worker.",
            "Trust and restore training",
        ),
        "merge" => (
            "Merge cells",
            "The next cell is appended to this one and removed. Recorded history is kept, and you can undo this.",
            "Merge",
        ),
        "delete" => (
            "Delete",
            "The cell and its source are removed. Its recorded runs stay in History, and you can undo this for changes up to 4 MiB.",
            "Delete cell",
        ),
        "stop" => (
            "Stop runtime for",
            "Stops running code and shuts down the runtime for every open window. Live variables are lost; saved source and history are kept.",
            "Stop runtime",
        ),
        "restart" => (
            "Restart runtime for",
            "Starts an empty worker. Live variables are lost; saved source and history are kept.",
            "Restart empty",
        ),
        "restore" => (
            "Restore",
            "Loads the checkpoint's saved values into a new session. Values the checkpoint excluded stay absent.",
            "Restore checkpoint",
        ),
        "continue" => (
            "Continue from",
            "Restores the checkpoint, then runs the code recorded after it. File and network side effects may repeat.",
            "Continue",
        ),
        "replay" => (
            "Replay",
            "Runs the recorded code in a new session. File and network side effects may repeat, and secret input must be entered again.",
            "Replay session",
        ),
        "bundle_import" => (
            "Import recorded history from",
            "Copies sessions, runs, and checkpoints into this notebook; no code runs and your source is not replaced. Existing records with the same IDs are kept.",
            "Import recorded history",
        ),
        "migrate" => (
            "Migrate ledger for",
            "Backs up the ledger, then upgrades it. Older versions of KernelLedger cannot open the upgraded ledger.",
            "Migrate ledger",
        ),
        _ => (
            "Apply action to",
            "Review the target before applying this action.",
            "Apply action",
        ),
    }
}
/// Why a cell is marked in the outline: it will not run, or shows no source.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum OutlineState {
    Disabled,
    Blocked,
    CodeHidden,
}
impl OutlineState {
    /// Disabled first, then blocked by a disabled ancestor, then hidden code.
    pub(crate) fn of(cell: &kernelledger_models::CellModel, blocked: bool) -> Option<Self> {
        if cell.is_disabled() {
            Some(Self::Disabled)
        } else if blocked {
            Some(Self::Blocked)
        } else if cell.is_code_hidden() {
            Some(Self::CodeHidden)
        } else {
            None
        }
    }
    fn glyph(self) -> Glyph {
        match self {
            Self::Disabled => Glyph::PowerOff,
            Self::Blocked => Glyph::Link2Off,
            Self::CodeHidden => Glyph::EyeOff,
        }
    }
    fn label(self) -> &'static str {
        match self {
            Self::Disabled => "disabled",
            Self::Blocked => "blocked by disabled cell",
            Self::CodeHidden => "code hidden",
        }
    }
    fn hint(self) -> &'static str {
        match self {
            Self::Disabled => "Disabled: skipped by Run all",
            Self::Blocked => "Depends on a disabled cell; Run all skips it",
            Self::CodeHidden => "Code hidden",
        }
    }
    /// Cells that will not run read as muted.
    fn muted(self) -> bool {
        self != Self::CodeHidden
    }
}

/// Every cell's manifest fields in notebook order, to detect layout changes.
fn cell_metadata(model: &NotebookModel) -> Vec<Option<BTreeMap<String, Value>>> {
    model
        .order
        .iter()
        .map(|id| model.cells.get(id).map(|cell| cell.disk.metadata.clone()))
        .collect()
}

fn error_message(code: &str) -> &'static str {
    match code {
        "undo_conflict" => {
            "Undo stopped because affected source changed. Your newer edits are retained. Review the source before changing cell structure."
        }
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
        "output_export" => {
            "The output could not be saved. Choose a new filename with the matching extension in a writable folder. Details are available in Diagnostics."
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
    let time_label = relative_time(record_time(record).unwrap_or("time unavailable"));
    let duration = record_duration(record)
        .map(|d| format!(", {d:.2}s"))
        .unwrap_or_default();
    format!("{title} \u{2014} {time_label}{duration}")
}

fn record_time(record: &Value) -> Option<&str> {
    ["timestamp", "start", "started", "accepted", "start_time"]
        .iter()
        .find_map(|key| record[*key].as_str())
}

/// Seconds from the recorded duration, or from start and end; `None` when neither was recorded.
fn record_duration(record: &Value) -> Option<f64> {
    record["duration"]
        .as_f64()
        .filter(|seconds| seconds.is_finite() && *seconds >= 0.)
        .or_else(|| {
            let start = chrono::DateTime::parse_from_rfc3339(record["start"].as_str()?).ok()?;
            let end = chrono::DateTime::parse_from_rfc3339(record["end"].as_str()?).ok()?;
            let elapsed = (end - start).num_milliseconds();
            (elapsed >= 0).then_some(elapsed as f64 / 1000.)
        })
}

fn runtime_action_available(method: &str, runtime: &Value, _surface: &str) -> bool {
    action_reason(method, runtime).is_none()
}
/// Picking a venv executable on macOS resolves its symlink in NSOpenPanel.
/// Picking the environment directory preserves the path needed by Python's venv.
fn python_environment_executable(path: PathBuf) -> PathBuf {
    if !path.is_dir() {
        return path;
    }
    if cfg!(windows) {
        let executable = path.join("python.exe");
        if executable.is_file() {
            executable
        } else {
            path.join("Scripts/python.exe")
        }
    } else {
        path.join("bin/python")
    }
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

/// First eight characters of a session identifier, for inline labels.
///
/// A full 36-character UUID in a heading pushes the worker state off to a
/// second line and reads as an opaque wall in every screenshot of the live
/// surfaces. The prefix identifies the session within a notebook, and Copy
/// session/identifier actions keep the complete value available.
fn short_session(value: &str) -> String {
    if value.is_empty() {
        return "unattached".into();
    }
    value.chars().take(8).collect()
}

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

fn notebook_name(path: &std::path::Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string())
}

/// Shorten a folder path to `max` characters, keeping its root and the
/// trailing components that distinguish same-named notebooks.
fn tail_path(path: &str, max: usize) -> String {
    let chars: Vec<char> = path.chars().collect();
    if chars.len() <= max {
        return path.to_owned();
    }
    let is_separator = |c: &char| matches!(c, '\\' | '/');
    let root_len = chars
        .iter()
        .position(is_separator)
        .map_or(0, |i| i + 1)
        .min(max / 3);
    let budget = max.saturating_sub(root_len + 1);
    let mut tail = &chars[chars.len() - budget..];
    if let Some(boundary) = tail.iter().position(is_separator)
        && boundary + 1 < tail.len()
    {
        tail = &tail[boundary..];
    }
    let root: String = chars[..root_len].iter().collect();
    let tail: String = tail.iter().collect();
    format!("{root}\u{2026}{tail}")
}

fn missing_recent_message(path: &std::path::Path) -> String {
    format!(
        "\u{201c}{}\u{201d} is no longer at {}. It may have been moved, renamed, or deleted. \
         Use Open notebook to choose its new location, or remove it from this list.",
        notebook_name(path),
        path.parent()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| path.display().to_string())
    )
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

/// Whether `system_high_contrast` can change while the window appearance stays
/// the same, and so needs a periodic re-check.
const SYSTEM_CONTRAST_POLL: bool = cfg!(target_os = "windows");

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
