mod accessibility;
mod assistant;
mod checkpoints;
mod components;
mod debug_source;
mod diagnostics;
mod help;
mod history;
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
    recent::RecentNotebook,
    service::{Command, Event, NotebookService},
};
use kernelledger_models::{Mode, NotebookModel, SaveState, SourceSnapshot};
use serde_json::{Value, json};
use std::{collections::BTreeMap, path::PathBuf, time::Duration};

actions!(
    kernelledger,
    [Save, RunCell, RunAll, Palette, OpenNotebook, CommandMode]
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

struct Tab {
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
    console_editors: BTreeMap<String, Entity<EditorState>>,
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
    cell_menu_assistant: bool,
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
    history_filters_open: bool,
    history_enter: Option<Subscription>,
    settings_section: &'static str,
    help_query: Entity<InputState>,
    record_active: usize,
    sidebar_focus: FocusHandle,
    confirm_focus: FocusHandle,
    dialog_focus: FocusHandle,
    dialog_open: bool,
    keys: Option<Subscription>,
    recent: Vec<RecentNotebook>,
    recorded_opens: std::collections::BTreeSet<PathBuf>,
    welcome_notice: Option<String>,
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
            KeyBinding::new("escape", CommandMode, Some("KernelLedger")),
        ]);
        #[cfg(target_os = "macos")]
        cx.bind_keys([
            KeyBinding::new("cmd-s", Save, Some("KernelLedger")),
            KeyBinding::new("cmd-shift-p", Palette, Some("KernelLedger")),
            KeyBinding::new("cmd-o", OpenNotebook, Some("KernelLedger")),
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
            console_editors: BTreeMap::new(),
            tick: Task::ready(()),
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
            cell_menu_assistant: false,
            insert_menu: None,
            menu_anchor: String::new(),
            menu_bounds: BTreeMap::new(),
            preferences,
            preference_status,
            preference_saving: false,
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
            settings_section: settings::SECTIONS[0].0,
            help_query: cx.new(|cx| InputState::new(window, cx).placeholder("Search Help")),
            record_active: 0,
            sidebar_focus: cx.focus_handle(),
            confirm_focus: cx.focus_handle(),
            dialog_focus: cx.focus_handle(),
            dialog_open: false,
            keys: None,
            recent,
            recorded_opens: Default::default(),
            welcome_notice: None,
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
        view.history_enter = Some(cx.subscribe_in(
            &view.history_query,
            window,
            |v, _, event: &InputEvent, window, cx| match event {
                InputEvent::PressEnter { .. } => v.apply_history_filters(window, cx),
                InputEvent::Change => cx.notify(),
                _ => {}
            },
        ));
        if let Some(project) = project {
            view.open(project, cx);
        }
        view.tick = cx.spawn_in(window, async move |view, window| {
            loop {
                window
                    .background_executor()
                    .timer(Duration::from_millis(50))
                    .await;
                if view
                    .update_in(window, |view, window, cx| view.drain(window, cx))
                    .is_err()
                {
                    break;
                }
            }
        });
        // Application keybindings need a focused descendant from the first
        // frame, before a pointer click has focused a button or cell editor.
        view.focus.focus(window, cx);
        view
    }
    fn open(&mut self, project: PathBuf, cx: &mut Context<Self>) {
        if let Some(i) = self.tabs.iter().position(|t| t.model.project == project) {
            self.active = i;
            return;
        }
        self.tabs.push(Tab {
            assistant: assistant::AssistantView::default(),
            service: NotebookService::start(project.clone(), self.python.clone()),
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
        for (event_tab, tab) in self.tabs.iter_mut().enumerate() {
            while let Ok(event) = tab.service.events.try_recv() {
                changed = true;
                match event {
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
                            tab.model.apply_snapshot(snapshot);
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
                        if value["interactive"]["worker_generation"]
                            != tab.runtime["interactive"]["worker_generation"]
                        {
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
                            if let Some(i) = tab.model.order.iter().position(|cid| cid == id) {
                                tab.list.splice(i..i + 1, 1);
                            }
                        }
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
                                tab.inspections.insert(key.into(), value);
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
        if let Some(tab) = self.tabs.get_mut(self.active)
            && let Some(id) = tab.reveal.clone()
            && let Some(index) = tab.model.order.iter().position(|c| c == &id)
        {
            tab.model.selected = Some(id.clone());
            tab.model.mode = Mode::Edit;
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
        tab.service = NotebookService::start(tab.model.project.clone(), self.python.clone());
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
                            "Cell {}: {} cell {id}: {}{status}",
                            i + 1,
                            cell.disk.kind,
                            cell.text
                                .lines()
                                .find(|l| !l.trim().is_empty())
                                .unwrap_or("Empty cell")
                        )
                    })
                    .aria_selected(selected)
                    .gap_2()
                    .px_2()
                    .py_1()
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
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .text_sm()
                            .when(selected, |d| d.font_weight(FontWeight::MEDIUM))
                            .when(preview.is_none(), |d| d.italic().text_color(muted_fg))
                            .child(preview.unwrap_or_else(|| {
                                format!("Empty {} cell", kind_label(&cell.disk.kind))
                            })),
                    )
                    .child(
                        h_flex().flex_none().w(px(28.)).justify_end().child(
                            div()
                                .text_size(rems(0.6875))
                                .font_family(mono.clone())
                                .text_color(muted_fg)
                                .child(if cell.disk.kind == "code" {
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
            .border_r_1()
            .border_color(divider(cx))
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
                            .child("Cells"),
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
            .on_action(
                cx.listener(|v, action: &shortcuts::WorkbenchCommand, window, cx| {
                    if !v.palette
                        && v.confirmation.is_none()
                        && !v.app_menu
                        && !v.runtime_menu
                        && !v.export_menu
                        && !v.notebook_menu
                        && v.cell_menu.is_none()
                        && v.insert_menu.is_none()
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
                                self.sidebar_visible && !self.compact,
                                show_assistant && !self.compact,
                                compact_assistant_sibling
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
fn kbd(keys: &str) -> Option<Kbd> {
    Keystroke::parse(&shortcuts::platform_binding(keys))
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
            "The next cell is appended to this cell and removed from the source document. Recorded history is retained. Undo cell structure can restore this change while its source remains unchanged and within undo limits.",
            "Merge",
        ),
        "delete" => (
            "Delete",
            "Deleting removes this cell and its source. Recorded executions remain in History. Undo cell structure can restore it while affected files remain unchanged; changes larger than 4 MiB exceed undo limits.",
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
