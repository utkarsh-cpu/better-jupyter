//! Notebook-scoped assistant inspector and explicit context/proposal review.
use super::*;

#[derive(Default)]
pub(super) struct AssistantView {
    pub open: bool,
    pub busy: bool,
    pub error: bool,
    pub rebuild: bool,
    pub started: Option<std::time::Instant>,
    pub purge_armed: bool,
    pub receipt_state: ReceiptState,
    pub payload_expanded: bool,
    pub show_history: bool,
    pub advanced_settings: bool,
    pub provider_draft: Option<String>,
    pub status: String,
    pub task: String,
    pub last_question: String,
    pub receipt: Value,
    pub answer: Value,
    pub proposals: Vec<Value>,
    pub settings: Value,
    pub selected_cells: Vec<String>,
    pub conversation: Option<String>,
    pub transcript: Vec<Value>,
    pub draft: String,
    pub exec_ids: Vec<String>,
    pub checkpoint_ids: Vec<String>,
    #[allow(dead_code)]
    pub task_group: String,
    pub connection: Connection,
    pub tested_profile: Value,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub(super) enum ReceiptState {
    #[default]
    Empty,
    Prepared,
    Sending,
    Consumed,
}

pub(super) fn effective_model(settings: &Value, task: &str) -> String {
    settings["settings"]["task_models"][task]
        .as_str()
        .filter(|s| !s.is_empty())
        .or_else(|| settings["settings"]["model"].as_str())
        .unwrap_or("")
        .to_owned()
}

pub(super) fn merge_provider(config: &mut Value, provider: &str, endpoint: String) {
    if !config["providers"].is_object() {
        config["providers"] = json!({});
    }
    if !config["providers"][provider].is_object() {
        config["providers"][provider] = json!({});
    }
    config["providers"][provider]["base_url"] = json!(endpoint);
}

pub(super) fn answer_report(answer: &Value) -> String {
    let mut text = answer["answer"].as_str().unwrap_or("").to_owned();
    text.push_str("\n\n## Citation targets\n\nTarget existence is not fact checking.\n");
    if let Some(items) = answer["citations"].as_array() {
        for item in items {
            text.push_str(&format!(
                "\n- {}: {} (target found: {}; in approved context: {})",
                item["kind"].as_str().unwrap_or("record"),
                item["id"].as_str().unwrap_or(""),
                item["verified"] == true,
                item["in_context"] == true
            ));
        }
    }
    if let Some(warnings) = answer["warnings"]
        .as_array()
        .filter(|items| !items.is_empty())
    {
        text.push_str("\n\n## Proposal warnings\n");
        for warning in warnings {
            text.push_str(&format!(
                "\n- {}: {}",
                warning["code"].as_str().unwrap_or("invalid"),
                warning["message"].as_str().unwrap_or("Proposal rejected")
            ));
        }
    }
    text
}

pub(super) fn receipt_text(item: &Value) -> String {
    let raw = item["text"].as_str().unwrap_or("");
    let Ok(data) = serde_json::from_str::<Value>(raw) else {
        return raw.to_owned();
    };
    if let Some(source) = data["source"].as_str()
        && item["kind"] == "cell"
    {
        return format!("Source:\n\n```\n{source}\n```");
    }
    if let Some(outputs) = data["outputs"].as_array() {
        let mut result = String::new();
        for output in outputs {
            if output["kind"] == "error" {
                result.push_str(&format!(
                    "{}: {}\n\n",
                    output["ename"].as_str().unwrap_or("Exception"),
                    output["evalue"].as_str().unwrap_or("")
                ));
                if let Some(trace) = output["traceback"].as_array() {
                    for line in trace {
                        result.push_str(line.as_str().unwrap_or(""));
                    }
                }
            } else {
                result.push_str(&serde_json::to_string_pretty(output).unwrap_or_default());
            }
        }
        return result;
    }
    serde_json::to_string_pretty(&data).unwrap_or_else(|_| raw.to_owned())
}

/// Result of the last explicit provider test; any settings change makes it stale.
#[derive(Clone, Debug, Default, PartialEq)]
pub(super) enum Connection {
    #[default]
    Untested,
    Testing,
    Ready,
    Failed(String),
}

/// Setup state the idle Assistant panel explains before any request is possible.
#[derive(Clone, Debug, PartialEq)]
pub(super) enum Setup {
    Loading,
    Off,
    Unconfigured,
    Untested,
    Testing,
    Failed(String),
    Ready,
}

pub(super) fn assistant_setup(settings: &Value, connection: &Connection) -> Setup {
    if settings.is_null() {
        return Setup::Loading;
    }
    if settings["policy"].as_str() == Some("off") {
        return Setup::Off;
    }
    let provider = settings["settings"]["provider"].as_str().unwrap_or("");
    let model = settings["settings"]["model"].as_str().unwrap_or("");
    if provider.is_empty() || model.trim().is_empty() {
        return Setup::Unconfigured;
    }
    match connection {
        Connection::Untested => Setup::Untested,
        Connection::Testing => Setup::Testing,
        Connection::Ready => Setup::Ready,
        Connection::Failed(message) => Setup::Failed(message.clone()),
    }
}

/// Provider errors can echo request headers or keys; mask anything credential-shaped.
pub(super) fn redact_secrets(message: &str) -> String {
    let mut bearer = false;
    let words = message.split(' ').map(|word| {
        let core = word.trim_matches(|c: char| !c.is_ascii_alphanumeric() && c != '-' && c != '_');
        let secret = bearer
            || (core.len() >= 12
                && ["sk-", "sk_", "key-", "api_key="]
                    .iter()
                    .any(|prefix| core.to_ascii_lowercase().starts_with(prefix)));
        bearer = core.eq_ignore_ascii_case("bearer");
        if secret && !core.is_empty() {
            word.replace(core, "[redacted]")
        } else {
            word.to_owned()
        }
    });
    let text = words.collect::<Vec<_>>().join(" ");
    let text = text.trim();
    if text.is_empty() {
        "The provider did not respond".into()
    } else {
        short_title(text, 240)
    }
}

fn short_title(title: &str, max: usize) -> String {
    if title.chars().count() <= max {
        title.to_owned()
    } else {
        let mut short: String = title.chars().take(max.saturating_sub(1)).collect();
        short.push('\u{2026}');
        short
    }
}

impl Workspace {
    pub(super) fn ai(&self, method: &str, fields: Value) {
        self.send(Command::Assistant {
            method: method.into(),
            fields,
        });
    }

    pub(super) fn open_assistant(&mut self, cx: &mut Context<Self>) {
        if self.tabs.is_empty() {
            return;
        }
        if self.surface == "Settings" {
            self.select_surface("Notebook", cx);
        }
        self.tabs[self.active].assistant.open = true;
        self.ai("settings.get", json!({}));
        cx.notify();
    }

    /// Opens Settings at the Assistant section. The question draft and the chosen
    /// context stay in the panel, so returning to the notebook resumes the same request.
    pub(super) fn open_provider_settings(&mut self, cx: &mut Context<Self>) {
        self.settings_section = "assistant";
        self.ai("settings.get", json!({}));
        self.select_surface("Settings", cx);
        cx.notify();
    }

    pub(super) fn test_provider(&mut self, cx: &mut Context<Self>) {
        let ai = &mut self.tabs[self.active].assistant;
        ai.connection = Connection::Testing;
        ai.busy = true;
        ai.error = false;
        ai.started = Some(std::time::Instant::now());
        let task = if ai.task.is_empty() {
            "ask".to_owned()
        } else {
            ai.task.clone()
        };
        ai.status = "Testing provider (no notebook context)\u{2026}".into();
        self.ai("providers.test", json!({"task":task}));
        cx.notify();
    }

    /// True for a new chat: nothing asked yet and nothing in progress.
    fn assistant_idle(&self) -> bool {
        let ai = &self.tabs[self.active].assistant;
        ai.transcript.is_empty()
            && ai.receipt.is_null()
            && ai.answer.is_null()
            && ai.proposals.is_empty()
            && !ai.busy
    }

    /// Prepares the composer's question for review and clears the composer.
    fn submit_question(
        &mut self,
        input: Entity<TextareaState>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let question = input.read(cx).value().trim().to_string();
        if question.is_empty() || self.tabs[self.active].assistant.busy {
            return;
        }
        let task = self.tabs[self.active].assistant.task.clone();
        self.ai_preview(if task.is_empty() { "ask" } else { &task }, question, cx);
        input.update(cx, |state, cx| state.set_value("", window, cx));
    }

    /// The readable name a cell has in the notebook, falling back to its id once removed.
    pub(super) fn context_cell_title(&self, id: &str) -> String {
        self.tabs[self.active]
            .model
            .cells
            .get(id)
            .map(|cell| cell_title(&cell.text, &cell.disk.kind))
            .unwrap_or_else(|| format!("Cell {id}"))
    }

    #[inline(never)]
    fn assistant_setup_card(&mut self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let idle = self.assistant_idle();
        let ai = &self.tabs[self.active].assistant;
        let setup = assistant_setup(&ai.settings, &ai.connection);
        let model = ai.settings["settings"]["model"]
            .as_str()
            .unwrap_or("")
            .to_owned();
        let failed = matches!(setup, Setup::Failed(_));
        if !(idle || failed) || matches!(setup, Setup::Loading | Setup::Ready) {
            return None;
        }
        let muted = cx.theme().muted_foreground;
        let (title, body): (&str, String) = match &setup {
            Setup::Off => (
                "Assistant is off for this notebook",
                "The notebook's AI policy blocks requests. You can change it in provider settings."
                    .into(),
            ),
            Setup::Unconfigured => (
                "Connect a model",
                "Use a local server such as Ollama or LM Studio, or the OpenAI API. Nothing from this notebook is shared until you approve it.".into(),
            ),
            Setup::Untested => (
                "Test the connection",
                format!("Settings are saved for {model}. The test sends no notebook content."),
            ),
            Setup::Testing => (
                "Testing the connection\u{2026}",
                "Checking that the provider responds. No notebook content is sent.".into(),
            ),
            Setup::Failed(message) => (
                "Connection failed",
                format!(
                    "{}. Check the base URL, model, and API key in provider settings.",
                    message.trim_end_matches('.')
                ),
            ),
            Setup::Loading | Setup::Ready => unreachable!(),
        };
        let testing = setup == Setup::Testing;
        let can_test = matches!(setup, Setup::Untested | Setup::Failed(_));
        Some(
            v_flex()
                .id("assistant-setup")
                .role(Role::Group)
                .aria_label(title)
                .w_full()
                .items_center()
                .gap_3()
                .child(icon_tile(Glyph::Sparkles, 36., cx))
                .child(
                    v_flex()
                        .items_center()
                        .gap_1()
                        .child(
                            div()
                                .text_base()
                                .font_weight(FontWeight::SEMIBOLD)
                                .child(title),
                        )
                        .child(
                            div()
                                .max_w(rems(20.))
                                .text_sm()
                                .text_center()
                                .text_color(muted)
                                .whitespace_normal()
                                .child(body),
                        ),
                )
                .child(
                    h_flex()
                        .gap_1p5()
                        .flex_wrap()
                        .justify_center()
                        .when(can_test, |d| {
                            d.child(
                                action_button("assistant-test")
                                    .primary()
                                    .small()
                                    .label(if failed {
                                        "Try again"
                                    } else {
                                        "Test connection"
                                    })
                                    .accessibility_label(if failed {
                                        "Try the connection again"
                                    } else {
                                        "Test connection"
                                    })
                                    .on_click_accessible(
                                        cx.listener(|v, _, _, cx| v.test_provider(cx)),
                                    ),
                            )
                        })
                        .child({
                            let settings = action_button("assistant-open-settings")
                                .small()
                                .icon(Glyph::Settings)
                                .label("Open provider settings")
                                .unavailable(testing)
                                .on_click_accessible(
                                    cx.listener(|v, _, _, cx| v.open_provider_settings(cx)),
                                );
                            if can_test || testing {
                                settings.outline()
                            } else {
                                settings.primary()
                            }
                        }),
                )
                .into_any_element(),
        )
    }

    /// The composer: grows from one to six lines; Enter sends, Shift+Enter adds a line.
    fn ai_question(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Entity<TextareaState> {
        let key = format!(
            "ai-{}-Question",
            self.tabs[self.active].model.project.display()
        );
        if let Some(question) = self.questions.get(&key) {
            return question.clone();
        }
        let question = cx.new(|cx| {
            TextareaState::new(window, cx)
                .placeholder("Ask about this notebook\u{2026}")
                .auto_grow(1, 6)
                .submit_on_enter(true)
        });
        let subscription = cx.subscribe_in(
            &question,
            window,
            |v, question, event: &InputEvent, window, cx| match event {
                InputEvent::PressEnter { shift: false, .. } => {
                    v.submit_question(question.clone(), window, cx)
                }
                InputEvent::Change => cx.notify(),
                _ => {}
            },
        );
        self.tabs[self.active].subscriptions.push(subscription);
        self.questions.insert(key, question.clone());
        question
    }

    fn ai_input(
        &mut self,
        name: &str,
        initial: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Entity<InputState> {
        let key = format!(
            "ai-{}-{name}",
            self.tabs[self.active].model.project.display()
        );
        if !self.prompts.contains_key(&key) {
            let input = cx.new(|cx| {
                let mut state = InputState::new(window, cx).placeholder(name);
                state.set_value(initial.to_owned(), window, cx);
                if name == "API key" {
                    state.set_masked(true, window, cx);
                }
                state
            });
            // Redraw on edit so inline validation follows typing.
            let subscription = cx.subscribe(&input, |_, _, event: &InputEvent, cx| {
                if matches!(event, InputEvent::Change) {
                    cx.notify();
                }
            });
            self.tabs[self.active].subscriptions.push(subscription);
            self.prompts.insert(key.clone(), input);
        }
        self.prompts[&key].clone()
    }

    pub(super) fn ai_preview(&mut self, task: &str, question: String, cx: &mut Context<Self>) {
        let tab = &mut self.tabs[self.active];
        tab.assistant.busy = true;
        tab.assistant.error = false;
        tab.assistant.rebuild = false;
        tab.assistant.receipt_state = ReceiptState::Empty;
        tab.assistant.started = Some(std::time::Instant::now());
        tab.assistant.task = task.to_owned();
        if tab.assistant.tested_profile["model"].as_str()
            != Some(effective_model(&tab.assistant.settings, task).as_str())
        {
            tab.assistant.connection = Connection::Untested;
        }
        tab.assistant.last_question = question.clone();
        tab.assistant.status = "Preparing context for review\u{2026}".into();
        tab.assistant.receipt = Value::Null;
        tab.assistant.answer = Value::Null;
        tab.assistant.proposals.clear();
        let mut fields = json!({
            "question": question,
            "task": task,
            "cell_ids": tab.assistant.selected_cells,
            "exec_ids": tab.assistant.exec_ids,
            "checkpoint_ids": tab.assistant.checkpoint_ids
        });
        tab.assistant.draft.clear();
        if let Some(cid) = tab.assistant.conversation.as_ref() {
            fields["conversation_id"] = json!(cid);
        }
        if task == "explain"
            && let Some(id) = tab
                .model
                .selected
                .as_ref()
                .and_then(|id| tab.model.cells.get(id))
                .and_then(|c| c.exec_id.as_ref())
        {
            let id = id.clone();
            fields["exec_ids"] = json!([id]);
            tab.assistant.exec_ids = vec![id];
        }
        self.ai("context.preview", fields);
        cx.notify();
    }

    pub(super) fn ai_send(&mut self, hash: String, cx: &mut Context<Self>) {
        let ai = &mut self.tabs[self.active].assistant;
        if ai.busy || ai.receipt["hash"].as_str() != Some(hash.as_str()) {
            return;
        }
        ai.busy = true;
        ai.error = false;
        ai.rebuild = false;
        ai.receipt_state = ReceiptState::Sending;
        ai.started = Some(std::time::Instant::now());
        ai.status = "Request started; generating a draft\u{2026}".into();
        ai.answer = Value::Null;
        self.ai(
            "respond",
            json!({"receipt_hash": hash, "approved_receipt": hash}),
        );
        cx.notify();
    }

    fn navigate_citation(&mut self, kind: &str, id: &str, cx: &mut Context<Self>) {
        if id.is_empty() {
            return;
        }
        match kind {
            "cell" => {
                self.surface = "Notebook".into();
                self.tabs[self.active].model.selected = Some(id.into());
                self.tabs[self.active].reveal = Some(id.into());
            }
            "execution" => {
                self.select_surface("History", cx);
                self.inspect("execution.get", json!({"exec_id": id}));
            }
            "checkpoint" => {
                self.select_surface("Checkpoints", cx);
                self.inspect("checkpoint.get", json!({"checkpoint_id": id}));
            }
            "source" => {
                if let Some(cid) = self.tabs[self.active]
                    .model
                    .cells
                    .iter()
                    .find(|(_, c)| c.disk.revision == id)
                    .map(|(cid, _)| cid.clone())
                {
                    self.surface = "Notebook".into();
                    self.tabs[self.active].model.selected = Some(cid.clone());
                    self.tabs[self.active].reveal = Some(cid);
                }
            }
            _ => {}
        }
        cx.notify();
    }

    #[inline(never)]
    fn assistant_header(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let state = &self.tabs[self.active].assistant;
        let busy = state.busy;
        let show_history = state.show_history;
        h_flex()
            .flex_none()
            .h(rems(2.5))
            .pl_3p5()
            .pr_1p5()
            .justify_between()
            .items_center()
            .child(
                div()
                    .text_sm()
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(if show_history { "History" } else { "Assistant" }),
            )
            .child(
                h_flex()
                    .gap_0p5()
                    .items_center()
                    .child(
                        action_button("assistant-new")
                            .ghost()
                            .small()
                            .icon(Glyph::Plus)
                            .accessibility_label("New")
                            .hint("New conversation")
                            .unavailable(busy)
                            .on_click_accessible(cx.listener(|v, _, _, cx| {
                                let ai = &mut v.tabs[v.active].assistant;
                                ai.conversation = None;
                                ai.transcript.clear();
                                ai.answer = Value::Null;
                                ai.receipt = Value::Null;
                                ai.proposals.clear();
                                ai.status.clear();
                                ai.show_history = false;
                                cx.notify();
                            })),
                    )
                    .child(
                        action_button("assistant-history")
                            .ghost()
                            .small()
                            .icon(Glyph::Clock)
                            .quiet_toggle(show_history)
                            .accessibility_label("Conversations")
                            .hint("Saved conversations and history search")
                            .unavailable(busy)
                            .on_click_accessible(cx.listener(|v, _, _, cx| {
                                let next = !v.tabs[v.active].assistant.show_history;
                                v.tabs[v.active].assistant.show_history = next;
                                if next {
                                    v.ai("conversations.list", json!({}));
                                }
                                cx.notify();
                            })),
                    )
                    .child(
                        action_button("assistant-close")
                            .ghost()
                            .small()
                            .icon(Glyph::X)
                            .accessibility_label("Close")
                            .hint("Close assistant panel")
                            .on_click_accessible(cx.listener(|v, _, _, cx| {
                                v.tabs[v.active].assistant.open = false;
                                cx.notify();
                            })),
                    ),
            )
            .into_any_element()
    }

    #[inline(never)]
    fn assistant_context_section(&mut self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let busy = self.tabs[self.active].assistant.busy;
        let selected_cells = self.tabs[self.active].assistant.selected_cells.clone();
        let exec_ids = self.tabs[self.active].assistant.exec_ids.clone();
        let checkpoint_ids = self.tabs[self.active].assistant.checkpoint_ids.clone();
        let active_cell = self.tabs[self.active].model.selected.clone();

        let has_explicit =
            !selected_cells.is_empty() || !exec_ids.is_empty() || !checkpoint_ids.is_empty();
        if !has_explicit && active_cell.is_none() {
            return None;
        }

        let mut pills = h_flex().gap_1().flex_wrap().items_center();
        let chip = |title: &str| short_title(title.trim_start_matches('#').trim_start(), 24);

        for id in &selected_cells {
            let cell_id = id.clone();
            let title = self.context_cell_title(id);
            pills = pills.child(
                action_button(SharedString::from(format!("ai-cell-{id}")))
                    .outline()
                    .small()
                    .icon(Glyph::Code)
                    .label(format!("{} \u{00d7}", chip(&title)))
                    .accessibility_label(format!("Remove {title} from context"))
                    .hint(format!("{title} \u{2014} remove from context"))
                    .unavailable(busy)
                    .on_click_accessible(cx.listener(move |v, _, _, cx| {
                        v.tabs[v.active]
                            .assistant
                            .selected_cells
                            .retain(|c| c != &cell_id);
                        v.tabs[v.active].assistant.receipt = Value::Null;
                        cx.notify();
                    })),
            );
        }
        for (kind, ids) in [
            ("execution", exec_ids.clone()),
            ("checkpoint", checkpoint_ids.clone()),
        ] {
            for (index, id) in ids.iter().enumerate() {
                let identity = id.clone();
                pills = pills.child(
                    action_button(SharedString::from(format!("ai-remove-{kind}-{index}")))
                        .outline()
                        .small()
                        .icon(if kind == "execution" {
                            Glyph::Clock
                        } else {
                            Glyph::Flag
                        })
                        .label(format!(
                            "{} {} \u{00d7}",
                            if kind == "execution" {
                                "Run"
                            } else {
                                "Checkpoint"
                            },
                            id.chars().take(8).collect::<String>()
                        ))
                        .accessibility_label(format!("Remove {kind} {id} from context"))
                        .unavailable(busy)
                        .on_click_accessible(cx.listener(move |v, _, _, cx| {
                            let ai = &mut v.tabs[v.active].assistant;
                            if kind == "execution" {
                                ai.exec_ids.retain(|item| item != &identity);
                            } else {
                                ai.checkpoint_ids.retain(|item| item != &identity);
                            }
                            ai.receipt = Value::Null;
                            cx.notify();
                        })),
                );
            }
        }

        if let Some(id) = active_cell.filter(|id| !selected_cells.contains(id)) {
            let title = self.context_cell_title(&id);
            pills = pills.child(
                action_button("ai-focus-cell")
                    .ghost()
                    .small()
                    .text_color(cx.theme().muted_foreground)
                    .label(format!("+ {}", chip(&title)))
                    .accessibility_label(format!("Add {title} to context"))
                    .hint("Add the selected cell to the context for review")
                    .unavailable(busy)
                    .on_click_accessible(cx.listener(move |v, _, _, cx| {
                        let cells = &mut v.tabs[v.active].assistant.selected_cells;
                        if !cells.contains(&id) {
                            cells.push(id.clone());
                        }
                        v.tabs[v.active].assistant.receipt = Value::Null;
                        cx.notify();
                    })),
            );
        }

        if has_explicit {
            pills = pills.child(
                action_button("ai-clear-context")
                    .ghost()
                    .small()
                    .label("Clear")
                    .unavailable(busy)
                    .on_click_accessible(cx.listener(|v, _, _, cx| {
                        let ai = &mut v.tabs[v.active].assistant;
                        ai.selected_cells.clear();
                        ai.exec_ids.clear();
                        ai.checkpoint_ids.clear();
                        ai.receipt = Value::Null;
                        cx.notify();
                    })),
            );
        }

        if exec_ids.len() == 2 {
            pills = pills.child(
                action_button("ai-compare")
                    .primary()
                    .small()
                    .icon(Glyph::Sparkles)
                    .label("Compare runs")
                    .accessibility_label("Compare selected executions")
                    .hint("Compare these two recorded runs and separate observed changes from suggested causes")
                    .unavailable(busy)
                    .on_click_accessible(cx.listener(|v, _, _, cx| {
                        v.ai_preview(
                            "compare",
                            "Compare these recorded runs. Separate observed changes from suggested causes.".into(),
                            cx,
                        )
                    })),
            );
        }

        let count = selected_cells.len() + exec_ids.len() + checkpoint_ids.len();
        let summary = match count {
            0 => "Selected context: no items".to_string(),
            1 => "Selected context: 1 item".to_string(),
            n => format!("Selected context: {n} items"),
        };
        Some(
            v_flex()
                .id("assistant-context")
                .role(Role::Group)
                .aria_label(summary.clone())
                .child(pills)
                .into_any_element(),
        )
    }

    #[inline(never)]
    fn assistant_workflows_section(&mut self, cx: &mut Context<Self>) -> AnyElement {
        const SUMMARY: &str = "Summarize this notebook's recorded state and what I can continue from. Cite every recorded fact.";
        const EXPLAIN: &str = "Explain this cell's recorded error and propose a fix.";
        const REPRO: &str = "Review reproducibility from recorded evidence.";
        const STALE: &str = "Identify stale or out-of-order cells and recommend the minimal safe rerun or checkpoint recovery.";
        let analysis = &self.tabs[self.active].assistant.settings["analysis"];
        let stale_count = analysis["stale_cells"].as_array().map_or(0, Vec::len);
        let repro_status = analysis["reproducibility"]["status"]
            .as_str()
            .unwrap_or("ready")
            .to_owned();
        let has_error = self.tabs[self.active].model.cells.values().any(|cell| {
            matches!(cell.outcome.as_str(), "error" | "failed")
                || cell.output.iter().any(|event| event["kind"] == "error")
        });
        let mut rows = vec![(
            "assistant-summary",
            "Resume summary".to_owned(),
            "What is recorded and where to continue".to_owned(),
            "summary",
            SUMMARY,
        )];
        if has_error {
            rows.push((
                "assistant-explain",
                "Explain error".to_owned(),
                "Why the selected cell failed and a fix".to_owned(),
                "explain",
                EXPLAIN,
            ));
        }
        if stale_count > 0 {
            rows.push((
                "ai-banner-stale",
                format!(
                    "Review {stale_count} stale cell{}",
                    if stale_count == 1 { "" } else { "s" }
                ),
                "The smallest safe rerun or recovery".to_owned(),
                "stale",
                STALE,
            ));
        }
        rows.push((
            "assistant-repro",
            "Check reproducibility".to_owned(),
            if repro_status == "ready" {
                "Audit from the recorded runs".to_owned()
            } else {
                format!("Current status: {repro_status}")
            },
            "reproducibility",
            REPRO,
        ));
        let mut list = v_flex().w_full().gap_1p5();
        for (id, title, detail, task, prompt) in rows {
            let view = cx.entity();
            list = list.child(
                action_button(id)
                    .debug_selector(id)
                    .suggestion(title, detail)
                    .on_a11y_click(move |_, cx| {
                        view.update(cx, |v, cx| v.ai_preview(task, prompt.into(), cx));
                    })
                    .on_click_accessible(cx.listener(move |v, _, _, cx| {
                        v.ai_preview(task, prompt.into(), cx);
                    })),
            );
        }
        list.into_any_element()
    }

    /// The new-chat screen: setup when a provider is still needed, otherwise
    /// a short greeting and suggestions.
    #[inline(never)]
    fn assistant_welcome(&mut self, cx: &mut Context<Self>) -> AnyElement {
        if let Some(setup) = self.assistant_setup_card(cx) {
            return setup;
        }
        let suggestions = self.assistant_workflows_section(cx);
        v_flex()
            .w_full()
            .gap_5()
            .child(
                v_flex()
                    .items_center()
                    .gap_3()
                    .child(icon_tile(Glyph::Sparkles, 36., cx))
                    .child(
                        v_flex()
                            .items_center()
                            .gap_1()
                            .child(
                                div()
                                    .text_base()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child("Ask about this notebook"),
                            )
                            .child(
                                div()
                                    .max_w(rems(20.))
                                    .text_sm()
                                    .text_center()
                                    .text_color(cx.theme().muted_foreground)
                                    .whitespace_normal()
                                    .child(
                                        "Answers cite your cells and runs. You approve what is shared before anything is sent.",
                                    ),
                            ),
                    ),
            )
            .child(suggestions)
            .into_any_element()
    }

    #[inline(never)]
    fn assistant_composer_dock(
        &mut self,
        input: Entity<TextareaState>,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let context_sec = self.assistant_context_section(cx);
        let state = &self.tabs[self.active].assistant;
        let busy = state.busy;
        let active_task = if state.task.is_empty() {
            "ask".to_owned()
        } else {
            state.task.clone()
        };
        let model_name = effective_model(&state.settings, &active_task);
        let dest_local = state.settings["destination"]["local"]
            .as_bool()
            .unwrap_or(true);
        let dest_label = state.settings["destination"]["label"]
            .as_str()
            .unwrap_or("Local")
            .to_owned();
        let empty = input.read(cx).value().trim().is_empty();
        let focused = input.read(cx).focus_handle(cx).is_focused(window);
        let theme = cx.theme();
        let (primary, muted, success, warning) = (
            theme.primary,
            theme.muted_foreground,
            theme.success,
            theme.warning,
        );
        let popover = theme.popover;
        let model_label = if model_name.is_empty() {
            "Choose a model".to_owned()
        } else {
            short_title(&model_name, 26)
        };
        let action = if busy {
            action_button("assistant-cancel")
                .small()
                .rounded_full()
                .icon(Glyph::Square)
                .accessibility_label("Cancel request")
                .hint("Cancel request")
                .on_click_accessible(cx.listener(|v, _, _, cx| {
                    v.send(Command::AssistantCancel);
                    let ai = &mut v.tabs[v.active].assistant;
                    ai.busy = false;
                    ai.receipt = Value::Null;
                    ai.receipt_state = ReceiptState::Consumed;
                    ai.started = None;
                    ai.error = false;
                    ai.rebuild = true;
                    ai.status = "Cancelled; prepare fresh context to continue.".into();
                    cx.notify();
                }))
        } else {
            let view = cx.entity();
            let for_a11y = input.clone();
            let for_click = input.clone();
            action_button("assistant-preview")
                .primary()
                .small()
                .rounded_full()
                .icon(Glyph::ArrowUp)
                .accessibility_label("Review context")
                .hint("Review what will be shared (Enter). Nothing is sent until you approve it.")
                .unavailable(empty)
                .on_a11y_click(move |window, cx| {
                    let input = for_a11y.clone();
                    view.update(cx, |v, cx| v.submit_question(input, window, cx));
                })
                .on_click_accessible(cx.listener(move |v, _, window, cx| {
                    v.submit_question(for_click.clone(), window, cx);
                }))
        };
        v_flex()
            .flex_none()
            .px_3()
            .pt_1()
            .pb_3()
            .child(
                v_flex()
                    .p_2()
                    .gap_1p5()
                    .rounded(px(12.))
                    .border_1()
                    .border_color(if focused {
                        primary.opacity(0.6)
                    } else {
                        surface_edge(cx)
                    })
                    .bg(popover)
                    .when(!self.high_contrast, |d| d.shadow_sm())
                    .when_some(context_sec, |d, el| d.child(el))
                    .child(
                        Textarea::new(&input)
                            .appearance(false)
                            .aria_label("Question for the Assistant"),
                    )
                    .child(
                        h_flex()
                            .justify_between()
                            .items_center()
                            .gap_2()
                            .child(
                                h_flex()
                                    .min_w_0()
                                    .gap_1()
                                    .items_center()
                                    .child(
                                        action_button("assistant-config")
                                            .ghost()
                                            .small()
                                            .text_color(muted)
                                            .label(model_label)
                                            .accessibility_label("Assistant provider settings")
                                            .hint(format!(
                                                "{dest_label} \u{00b7} change provider, model, and privacy settings"
                                            ))
                                            .on_click_accessible(cx.listener(|v, _, _, cx| {
                                                v.open_provider_settings(cx)
                                            })),
                                    )
                                    .when(!model_name.is_empty(), |d| {
                                        d.child(dot(if dest_local { success } else { warning }))
                                    })
                                    .when(active_task != "ask", |d| {
                                        d.child(badge(format!("/{active_task}"), primary))
                                    }),
                            )
                            .child(action),
                    ),
            )
            .into_any_element()
    }

    #[inline(never)]
    fn assistant_status_section(&mut self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let state = &self.tabs[self.active].assistant;
        let busy = state.busy;
        let status_text = if busy {
            format!(
                "{} {} s",
                state.status,
                state.started.map(|s| s.elapsed().as_secs()).unwrap_or(0)
            )
        } else {
            state.status.clone()
        };
        let rebuild = state.rebuild && !busy;
        let draft = state.draft.clone();
        if status_text.is_empty() && !busy {
            return None;
        }
        // The review card already says what to do next.
        if !busy
            && !state.error
            && (state.receipt_state == ReceiptState::Prepared || !state.answer.is_null())
        {
            return None;
        }
        let theme = cx.theme();
        let (muted, danger, info) = (theme.muted_foreground, theme.danger, theme.info);
        let is_err = state.error;
        let reduced_motion = self.preferences.reduced_motion;
        let status_color = if is_err {
            danger
        } else if busy {
            info
        } else {
            muted
        };
        let indicator = dot(status_color);
        let mut col = v_flex().gap_2().child(
            h_flex()
                .id("assistant-status-card")
                .role(if is_err { Role::Alert } else { Role::Status })
                .aria_label(status_text.clone())
                .gap_2()
                .items_center()
                .text_xs()
                .text_color(if is_err { danger } else { muted })
                .child(if busy && !reduced_motion {
                    indicator
                        .with_animation(
                            "assistant-working",
                            Animation::new(Duration::from_millis(1100))
                                .repeat()
                                .with_easing(pulsating_between(0.35, 1.)),
                            |dot, delta| dot.opacity(delta),
                        )
                        .into_any_element()
                } else {
                    indicator.into_any_element()
                })
                .child(
                    div()
                        .min_w_0()
                        .flex_1()
                        .whitespace_normal()
                        .child(status_text),
                ),
        );
        if rebuild {
            col = col.child(
                action_button("assistant-rebuild")
                    .primary()
                    .label("Rebuild context")
                    .accessibility_label("Rebuild context after failed send")
                    .on_click_accessible(cx.listener(|v, _, _, cx| {
                        let ai = &v.tabs[v.active].assistant;
                        let task = ai.task.clone();
                        let question = ai.last_question.clone();
                        v.ai_preview(&task, question, cx);
                    })),
            );
        }
        if !draft.is_empty() && busy {
            col = col.child(
                v_flex()
                    .gap_1()
                    .child(TextView::markdown("ai-stream-draft", safe_markdown(&draft)))
                    .child(
                        div()
                            .text_xs()
                            .text_color(muted)
                            .child("Draft \u{00b7} verifying citations\u{2026}"),
                    ),
            );
        }
        Some(col.into_any_element())
    }

    #[inline(never)]
    fn assistant_receipt_section(&mut self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let state = &self.tabs[self.active].assistant;
        if state.receipt.is_null() {
            return None;
        }
        let busy = state.busy;
        let receipt = state.receipt.clone();
        let theme = cx.theme();
        let (muted, success, warning, info, mono) = (
            theme.muted_foreground,
            theme.success,
            theme.warning,
            theme.info,
            theme.mono_font_family.clone(),
        );
        let destination = &receipt["destination"];
        let r_label = destination["label"].as_str().unwrap_or("Local");
        let r_local = destination["local"].as_bool().unwrap_or(true);
        let r_host = destination["host"].as_str().unwrap_or("");
        let r_base_url = destination["base_url"].as_str().unwrap_or("");
        let r_model = receipt["model"].as_str().unwrap_or("");
        let r_hash = receipt["hash"].as_str().unwrap_or("").to_owned();
        let auto_approved = receipt["auto_approved"] == true;
        let sendable = state.receipt_state == ReceiptState::Prepared;

        let expanded = state.payload_expanded;
        let included = receipt["items"].as_array().map_or(0, Vec::len);
        let summary = format!(
            "{included} record{} \u{00b7} ~{} tokens \u{00b7} {r_label} \u{00b7} {r_model}",
            if included == 1 { "" } else { "s" },
            receipt["token_estimate"],
        );
        // Once sent, the receipt folds to one line and stays available for audit.
        if !sendable && !busy && !expanded {
            return Some(
                h_flex()
                    .id("assistant-receipt-sent")
                    .gap_1()
                    .items_center()
                    .text_xs()
                    .text_color(muted)
                    .child(
                        div()
                            .min_w_0()
                            .flex_1()
                            .truncate()
                            .child(format!("Shared {summary}")),
                    )
                    .child(
                        action_button("assistant-exact-payload")
                            .ghost()
                            .small()
                            .label("Details")
                            .accessibility_label("Show exact payload and system prompt")
                            .on_click_accessible(cx.listener(|v, _, _, cx| {
                                v.tabs[v.active].assistant.payload_expanded = true;
                                cx.notify();
                            })),
                    )
                    .into_any_element(),
            );
        }

        let mut receipt_card = card(cx)
            .p_3()
            .gap_2()
            .child(
                h_flex()
                    .justify_between()
                    .items_center()
                    .gap_2()
                    .child(
                        div()
                            .text_sm()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child("Review before sending"),
                    )
                    .when(auto_approved, |d| d.child(badge("Auto-approved", info)))
                    .when(!r_local, |d| d.child(badge("Leaves this device", warning))),
            )
            .child(
                h_flex()
                    .gap_1p5()
                    .items_center()
                    .text_xs()
                    .text_color(muted)
                    .child(dot(if r_local { success } else { warning }))
                    .child(div().min_w_0().flex_1().whitespace_normal().child(summary)),
            );

        if let Some(omitted) = receipt["omitted"].as_array() {
            for item in omitted {
                receipt_card = receipt_card.child(
                    div()
                        .text_xs()
                        .whitespace_normal()
                        .text_color(warning)
                        .child(format!(
                            "Omitted {} {}: {}",
                            item["kind"], item["id"], item["reason"]
                        )),
                );
            }
        }
        if let Some(items) = receipt["items"].as_array() {
            let mut items_col = v_flex().gap_1();
            for (index, item) in items.iter().enumerate() {
                let kind = item["kind"].as_str().unwrap_or("item");
                let id = item["id"].as_str().unwrap_or("");
                let name = if kind == "cell" && !id.is_empty() {
                    self.context_cell_title(id)
                        .trim_start_matches('#')
                        .trim_start()
                        .to_owned()
                } else {
                    format!("{kind} {}", short_title(id, 16))
                };
                items_col = items_col.child(
                    h_flex()
                        .gap_1p5()
                        .items_center()
                        .text_xs()
                        .child(
                            div()
                                .min_w_0()
                                .flex_1()
                                .truncate()
                                .font_family(mono.clone())
                                .child(name),
                        )
                        .when(item["redacted"] == true, |d| {
                            d.child(badge("redacted", warning))
                        })
                        .when(item["truncated"] == true, |d| {
                            d.child(badge("truncated", info))
                        })
                        .child(
                            div()
                                .flex_none()
                                .text_color(muted)
                                .child(format!("{} B", item["bytes"])),
                        ),
                );
                if expanded {
                    items_col = items_col.child(
                        div().p_2().rounded(px(6.)).bg(theme.muted).child(
                            TextView::markdown(
                                SharedString::from(format!("ai-receipt-{index}")),
                                safe_markdown(&receipt_text(item)),
                            )
                            .selectable(true),
                        ),
                    );
                }
            }
            receipt_card = receipt_card.child(items_col);
        }

        let full = serde_json::to_string_pretty(&receipt).unwrap_or_default();
        if expanded {
            receipt_card = receipt_card.child(
                TextView::markdown(
                    "assistant-exact-json",
                    safe_markdown(&format!("```json\n{full}\n```")),
                )
                .selectable(true),
            );
        }
        let a11y_hash = r_hash.clone();
        let click_hash = r_hash;
        let view = cx.entity();
        let approval_url = if r_base_url.is_empty() {
            r_host.to_owned()
        } else {
            r_base_url.to_owned()
        };

        receipt_card = receipt_card.child(
            h_flex()
                .gap_1p5()
                .flex_wrap()
                .items_center()
                .pt_1()
                .child(
                    action_button("assistant-send")
                        .primary()
                        .small()
                        .label("Approve & send")
                        .accessibility_label("Approve context and send")
                        .unavailable(busy || !sendable)
                        .on_a11y_click(move |_, cx| {
                            view.update(cx, |v, cx| v.ai_send(a11y_hash.clone(), cx));
                        })
                        .on_click_accessible(cx.listener(move |v, _, _, cx| {
                            v.ai_send(click_hash.clone(), cx);
                        })),
                )
                .child(
                    action_button("assistant-exact-payload")
                        .ghost()
                        .small()
                        .label(if expanded { "Hide details" } else { "Details" })
                        .accessibility_label(if expanded {
                            "Hide exact payload and system prompt"
                        } else {
                            "Show exact payload and system prompt"
                        })
                        .on_click_accessible(cx.listener(|v, _, _, cx| {
                            v.tabs[v.active].assistant.payload_expanded =
                                !v.tabs[v.active].assistant.payload_expanded;
                            cx.notify();
                        })),
                )
                .when(expanded, |d| {
                    d.child(
                        action_button("ai-receipt-copy")
                            .ghost()
                            .small()
                            .icon(Glyph::Copy)
                            .accessibility_label("Copy full receipt and system prompt")
                            .hint("Copy full JSON receipt and system prompt to clipboard")
                            .on_click_accessible(cx.listener(move |_, _, _, cx| {
                                copy_acknowledged(full.clone(), cx);
                            })),
                    )
                })
                .when(!approval_url.is_empty(), |d| {
                    d.child(
                        action_button("ai-receipt-remember")
                            .ghost()
                            .small()
                            .label(if auto_approved {
                                "Revoke auto-approval"
                            } else {
                                "Always allow"
                            })
                            .accessibility_label(if auto_approved {
                                "Revoke destination auto-approval"
                            } else {
                                "Always allow for this notebook & destination"
                            })
                            .hint(if auto_approved {
                                "Require manual receipt approval for this destination again"
                            } else {
                                "Auto-approve future receipts for this notebook and destination"
                            })
                            .on_click_accessible(cx.listener(move |v, _, _, cx| {
                                if auto_approved {
                                    v.ai("approval.revoke", json!({}));
                                    cx.notify();
                                    return;
                                }
                                let root = v.tabs[v.active].assistant.settings["notebook_root"]
                                    .as_str()
                                    .map(str::to_owned)
                                    .unwrap_or_else(|| {
                                        v.tabs[v.active].model.project.display().to_string()
                                    });
                                let key = format!("{root}::{approval_url}");
                                let mut config =
                                    v.tabs[v.active].assistant.settings["settings"].clone();
                                if !config.is_object() {
                                    config = json!({});
                                }
                                if !config["approvals"].is_object() {
                                    config["approvals"] = json!({});
                                }
                                if auto_approved {
                                    if let Some(map) = config["approvals"].as_object_mut() {
                                        map.remove(&key);
                                        map.remove(&approval_url);
                                    }
                                } else {
                                    config["approvals"][key] = json!(true);
                                }
                                v.ai("settings.update", json!({"settings": config}));
                                cx.notify();
                            })),
                    )
                }),
        );
        Some(receipt_card.into_any_element())
    }

    #[inline(never)]
    fn assistant_answer_section(&mut self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let answer = self.tabs[self.active].assistant.answer.clone();
        let text = answer["answer"].as_str()?;
        let theme = cx.theme();
        let (primary, muted, success, mono) = (
            theme.primary,
            theme.muted_foreground,
            theme.success,
            theme.mono_font_family.clone(),
        );
        let copy_text = text.to_owned();
        let pdf_text = answer_report(&answer);
        let word_text = pdf_text.clone();
        let citations = answer["citations"].as_array().cloned().unwrap_or_default();
        let resolved = citations.iter().filter(|c| c["verified"] == true).count();
        let supplied = citations.iter().filter(|c| c["in_context"] == true).count();
        let meta = format!(
            "{resolved}/{} sources found{}",
            citations.len(),
            answer["usage"]["total_tokens"]
                .as_u64()
                .map(|tokens| format!(" \u{00b7} {tokens} tokens"))
                .unwrap_or_default()
        );
        let _ = (primary, success, supplied, &mono);
        let action_bar = h_flex()
            .gap_0p5()
            .items_center()
            .child(
                action_button("ai-copy-answer")
                    .ghost()
                    .small()
                    .icon(Glyph::Copy)
                    .accessibility_label("Copy answer")
                    .hint("Copy answer")
                    .on_click_accessible(cx.listener(move |_, _, _, cx| {
                        copy_acknowledged(copy_text.clone(), cx);
                    })),
            )
            .child(
                action_button("ai-export-pdf")
                    .ghost()
                    .small()
                    .text_color(muted)
                    .label("PDF")
                    .accessibility_label("Export PDF")
                    .hint("Export this reviewed answer as PDF")
                    .on_click_accessible(cx.listener(move |v, _, window, cx| {
                        v.export_answer_dialog("pdf", pdf_text.clone(), window, cx);
                    })),
            )
            .child(
                action_button("ai-export-docx")
                    .ghost()
                    .small()
                    .text_color(muted)
                    .label("Word")
                    .accessibility_label("Export Word")
                    .hint("Export this reviewed answer as Word (.docx)")
                    .on_click_accessible(cx.listener(move |v, _, window, cx| {
                        v.export_answer_dialog("docx", word_text.clone(), window, cx);
                    })),
            )
            .child(div().flex_1())
            .child(
                div()
                    .flex_none()
                    .text_xs()
                    .text_color(muted)
                    .child(meta),
            )
            .child(info_tip(
                "ai-evidence-tip",
                "Citation targets are verified against the ledger where marked. Uncited claims and suggested causes are unverified.",
                cx,
            ));
        let mut answer_card = v_flex().w_full().gap_2().child(
            div()
                .id("assistant-answer-region")
                .role(Role::Document)
                .aria_label("Assistant answer")
                .a11y_synthetic_children({
                    let nodes = super::notebook::markdown_accessibility_nodes(text);
                    move |tree| {
                        for (i, node) in nodes.into_iter().enumerate() {
                            let id = tree.synthetic_node_id(i);
                            tree.push_child(id, node);
                        }
                    }
                })
                .child(
                    TextView::markdown("assistant-answer", safe_markdown(text)).selectable(true),
                ),
        );

        if let Some(citations) = answer["citations"].as_array()
            && !citations.is_empty()
        {
            let mut cite_row = h_flex().gap_1().flex_wrap();
            for (i, citation) in citations.iter().enumerate() {
                let verified = citation["verified"] == true;
                let kind = citation["kind"].as_str().unwrap_or("record").to_owned();
                let id = citation["id"].as_str().unwrap_or("").to_owned();
                let label = format!(
                    "@{kind}:{}{}",
                    short_title(&id, 24),
                    if verified { "" } else { " (unverified)" }
                );
                let click_kind = kind.clone();
                let click_id = id.clone();
                let a11y_kind = kind.clone();
                let a11y_id = id.clone();
                let view = cx.entity();
                cite_row = cite_row.child(
                    action_button(("ai-citation", i))
                        .outline()
                        .small()
                        .label(label)
                        .accessibility_label(format!(
                            "@{kind}:{id}{}",
                            if verified { "" } else { " (unverified)" }
                        ))
                        .hint(format!(
                            "{kind}: {id}; target existence is not fact checking"
                        ))
                        .unavailable(!verified)
                        .on_a11y_click(move |_, cx| {
                            view.update(cx, |v, cx| {
                                v.navigate_citation(&a11y_kind, &a11y_id, cx);
                            });
                        })
                        .on_click_accessible(cx.listener(move |v, _, _, cx| {
                            v.navigate_citation(&click_kind, &click_id, cx);
                        })),
                );
            }
            answer_card = answer_card.child(cite_row);
        }

        if let Some(warnings) = answer["warnings"].as_array() {
            for (index, warning) in warnings.iter().enumerate() {
                answer_card = answer_card.child(
                    div()
                        .id(("ai-proposal-warning", index))
                        .role(Role::Alert)
                        .text_xs()
                        .whitespace_normal()
                        .text_color(cx.theme().warning)
                        .child(format!(
                            "Proposal not applied ({}): {}",
                            warning["code"].as_str().unwrap_or("invalid"),
                            warning["message"]
                                .as_str()
                                .unwrap_or("Review current source and request another edit")
                        )),
                );
            }
        }
        Some(answer_card.child(action_bar).into_any_element())
    }

    #[inline(never)]
    fn assistant_proposals_section(&mut self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let state = &self.tabs[self.active].assistant;
        if state.proposals.is_empty() {
            return None;
        }
        let busy = state.busy;
        let proposals = state.proposals.clone();
        let theme = cx.theme();
        let (muted, primary, success, warning, mono) = (
            theme.muted_foreground,
            theme.primary,
            theme.success,
            theme.warning,
            theme.mono_font_family.clone(),
        );
        let mut proposals_col = v_flex().gap_2p5();
        for (index, proposal) in proposals.iter().enumerate() {
            let pid = proposal["id"].as_str().unwrap_or("").to_owned();
            let reject = pid.clone();
            let payload = if proposal["payload"].is_object() {
                &proposal["payload"]
            } else {
                proposal
            };
            let p_state = proposal["state"].as_str().unwrap_or("proposed");
            let kind = payload["kind"].as_str().unwrap_or("proposal");
            let target_cell = payload["cell_id"].as_str().map(str::to_owned);
            let current = target_cell
                .as_deref()
                .and_then(|id| self.tabs[self.active].model.cells.get(id))
                .map(|c| c.text.clone())
                .unwrap_or_default();
            let proposed = payload["source"]
                .as_str()
                .or(payload["text"].as_str())
                .map(str::to_owned)
                .unwrap_or_else(|| {
                    serde_json::to_string_pretty(&payload["cells"]).unwrap_or_default()
                });

            let state_tone = match p_state {
                "applied" => success,
                "accepted" | "unknown" => warning,
                "rejected" => muted,
                "stale" => warning,
                _ => primary,
            };

            let mut prop_card = card(cx)
                .p_3()
                .gap_2()
                .child(
                    h_flex()
                        .justify_between()
                        .items_center()
                        .gap_2()
                        .flex_wrap()
                        .child(
                            h_flex()
                                .gap_1p5()
                                .items_center()
                                .flex_wrap()
                                .child(Icon::new(Glyph::Sparkles).small().text_color(primary))
                                .child(badge(kind.to_string(), primary))
                                .when_some(target_cell.clone(), |d, cid| {
                                    d.child(
                                        div()
                                            .text_xs()
                                            .font_family(mono.clone())
                                            .font_weight(FontWeight::SEMIBOLD)
                                            .child(format!("@{cid}")),
                                    )
                                }),
                        )
                        .child(badge(if p_state == "unknown" { "Needs review".to_owned() } else if p_state == "accepted" { "Applying".to_owned() } else { p_state.to_string() }, state_tone)),
                )
                .child(
                    div()
                        .text_sm()
                        .whitespace_normal()
                        .child(payload["rationale"].as_str().unwrap_or("").to_owned()),
                )
                .when(p_state == "stale", |d| {
                    d.child(
                        div()
                            .p_2()
                            .rounded(px(6.))
                            .bg(warning.opacity(0.1))
                            .text_xs()
                            .text_color(warning)
                            .child("Source changed since proposal generation \u{2014} click Regenerate."),
                    )
                })
                .child(
                    h_flex()
                        .gap_2()
                        .items_start()
                        .flex_wrap()
                        .when(!current.is_empty(), |d| {
                            d.child(
                                v_flex()
                                    .flex_1()
                                    .min_w(px(140.))
                                    .gap_1()
                                    .child(
                                        div()
                                            .text_xs()
                                            .font_family(mono.clone())
                                            .font_weight(FontWeight::MEDIUM)
                                            .text_color(muted)
                                            .child("- Current"),
                                    )
                                    .child(
                                        div()
                                            .p_2()
                                            .rounded(px(6.))
                                            .border_1()
                                            .border_color(divider(cx))
                                            .bg(theme.background)
                                            .child(
                                                TextView::markdown(
                                                    SharedString::from(format!(
                                                        "ai-before-{index}"
                                                    )),
                                                    safe_markdown(&format!(
                                                        "```python\n{current}\n```"
                                                    )),
                                                )
                                                .selectable(true),
                                            ),
                                    ),
                            )
                        })
                        .child(
                            v_flex()
                                .flex_1()
                                .min_w(px(140.))
                                .gap_1()
                                .child(
                                    div()
                                        .text_xs()
                                        .font_family(mono.clone())
                                        .font_weight(FontWeight::MEDIUM)
                                        .text_color(success)
                                        .child("+ Proposed"),
                                )
                                .child(
                                    div()
                                        .p_2()
                                        .rounded(px(6.))
                                        .border_1()
                                        .border_color(divider(cx))
                                        .bg(theme.background)
                                        .child(
                                            TextView::markdown(
                                                SharedString::from(format!("ai-after-{index}")),
                                                safe_markdown(&format!(
                                                    "```python\n{proposed}\n```"
                                                )),
                                            )
                                            .selectable(true),
                                        ),
                                ),
                        ),
                );

            let mut action_row = h_flex()
                .gap_1p5()
                .flex_wrap()
                .pt_1p5()
                .border_t_1()
                .border_color(divider(cx));
            if payload["kind"] == "command" {
                let command = payload["text"].as_str().unwrap_or("").to_owned();
                action_row = action_row.child(
                    action_button(("ai-copy-command", index))
                        .outline()
                        .small()
                        .label("Copy command")
                        .on_click_accessible(
                            cx.listener(move |_, _, _, cx| copy_acknowledged(command.clone(), cx)),
                        ),
                );
            } else {
                action_row = action_row.child(
                    action_button(("ai-apply", index))
                        .primary()
                        .small()
                        .label(if payload["kind"] == "run_cells" {
                            "Run reviewed cells"
                        } else {
                            "Apply"
                        })
                        .accessibility_label("Apply reviewed proposal")
                        .hint("Apply reviewed proposal to notebook")
                        .unavailable(busy || p_state != "proposed")
                        .on_click_accessible(cx.listener(move |v, _, _, cx| {
                            v.ai(
                                "proposal.resolve",
                                json!({"proposal_id": pid, "state": "accepted"}),
                            );
                            v.tabs[v.active].assistant.busy = true;
                            cx.notify();
                        })),
                );
            }
            action_row = action_row
                .child(
                    action_button(("ai-reject", index))
                        .ghost()
                        .small()
                        .label("Reject")
                        .unavailable(busy || p_state != "proposed")
                        .on_click_accessible(cx.listener(move |v, _, _, cx| {
                            v.ai(
                                "proposal.resolve",
                                json!({"proposal_id": reject, "state": "rejected"}),
                            );
                            cx.notify();
                        })),
                )
                .child({
                    let regen_cell = target_cell.clone();
                    let regen_rationale =
                        payload["rationale"].as_str().unwrap_or("").to_owned();
                    action_button(("ai-regenerate", index))
                        .outline()
                        .small()
                        .icon(Glyph::RotateCcw)
                        .label("Regenerate")
                        .unavailable(busy)
                        .on_click_accessible(cx.listener(move |v, _, _, cx| {
                            if let Some(cid) = &regen_cell {
                                v.tabs[v.active].assistant.selected_cells = vec![cid.clone()];
                            }
                            let task = if v.tabs[v.active].assistant.task.is_empty() {
                                "edit".to_owned()
                            } else {
                                v.tabs[v.active].assistant.task.clone()
                            };
                            let prompt = if v.tabs[v.active].assistant.last_question.is_empty() {
                                format!(
                                    "Regenerate proposal against current notebook source: {regen_rationale}"
                                )
                            } else {
                                v.tabs[v.active].assistant.last_question.clone()
                            };
                            v.ai_preview(&task, prompt, cx);
                        }))
                });
            prop_card = prop_card.child(action_row);
            proposals_col = proposals_col.child(prop_card);
        }
        Some(proposals_col.into_any_element())
    }

    #[inline(never)]
    fn assistant_history_view(
        &mut self,
        search_input: Entity<InputState>,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let state = &self.tabs[self.active].assistant;
        let busy = state.busy;
        let show_history = state.show_history;
        let active_conv = state.conversation.clone();
        let theme = cx.theme();
        let (muted, mono) = (theme.muted_foreground, theme.mono_font_family.clone());
        let search_results = self.tabs[self.active]
            .inspections
            .get("AI search")
            .and_then(|v| v["items"].as_array())
            .cloned()
            .unwrap_or_default();
        let conversations = self.tabs[self.active]
            .inspections
            .get("AI conversations")
            .and_then(|v| v["items"].as_array())
            .cloned()
            .unwrap_or_default();

        if !show_history {
            return None;
        }

        let mut col = v_flex().gap_3();

        {
            let search_for_click = search_input.clone();
            let mut search_card = v_flex().gap_2().child(
                h_flex()
                    .gap_1p5()
                    .items_center()
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .child(Input::new(&search_input).small().cleanable(true)),
                    )
                    .child(
                        action_button("ai-search-submit")
                            .outline()
                            .small()
                            .icon(Glyph::Search)
                            .label("Search")
                            .accessibility_label("Search history")
                            .hint("Search execution ledger and assistant history")
                            .unavailable(busy)
                            .on_click_accessible(cx.listener(move |v, _, _, cx| {
                                let q = search_for_click.read(cx).value().trim().to_string();
                                if !q.is_empty() {
                                    v.ai("search", json!({"query": q}));
                                    cx.notify();
                                }
                            })),
                    ),
            );
            if !search_results.is_empty() {
                let mut results_col = v_flex().gap_1().pt_1();
                for (i, hit) in search_results.iter().take(6).enumerate() {
                    let exec_id = hit["exec_id"]
                        .as_str()
                        .or(hit["identity"].as_str())
                        .unwrap_or("")
                        .to_owned();
                    let cell_id = hit["cell_id"]
                        .as_str()
                        .or(hit["excerpt"].as_str())
                        .unwrap_or("execution")
                        .to_owned();
                    let status = hit["status"].as_str().unwrap_or("recorded").to_owned();
                    let inspect_exec = exec_id.clone();
                    let add_exec = exec_id.clone();
                    results_col = results_col.child(
                        h_flex()
                            .justify_between()
                            .items_center()
                            .gap_2()
                            .flex_wrap()
                            .p_1p5()
                            .rounded(px(6.))
                            .bg(theme.background)
                            .border_1()
                            .border_color(divider(cx))
                            .child(
                                h_flex()
                                    .gap_1p5()
                                    .items_center()
                                    .min_w_0()
                                    .child(badge(status.clone(), tone(&status, cx)))
                                    .child(
                                        div()
                                            .text_xs()
                                            .font_family(mono.clone())
                                            .truncate()
                                            .child(format!("{cell_id} \u{00b7} {exec_id}")),
                                    ),
                            )
                            .child(
                                h_flex()
                                    .gap_1()
                                    .child(
                                        action_button(("ai-search-open", i))
                                            .ghost()
                                            .small()
                                            .label("Inspect")
                                            .on_click_accessible(cx.listener(
                                                move |v, _, _, cx| {
                                                    v.navigate_citation(
                                                        "execution",
                                                        &inspect_exec,
                                                        cx,
                                                    );
                                                },
                                            )),
                                    )
                                    .child(
                                        action_button(("ai-search-add", i))
                                            .outline()
                                            .small()
                                            .label("+ Context")
                                            .accessibility_label("Add to context")
                                            .hint("Include this execution in assistant context")
                                            .on_click_accessible(cx.listener(
                                                move |v, _, _, cx| {
                                                    let ai = &mut v.tabs[v.active].assistant;
                                                    if !ai.exec_ids.contains(&add_exec) {
                                                        if ai.exec_ids.len() == 2 {
                                                            ai.exec_ids.remove(0);
                                                        }
                                                        ai.exec_ids.push(add_exec.clone());
                                                    }
                                                    ai.receipt = Value::Null;
                                                    ai.receipt_state = ReceiptState::Consumed;
                                                    ai.started = None;
                                                    ai.error = false;
                                                    ai.rebuild = true;
                                                    cx.notify();
                                                },
                                            )),
                                    ),
                            ),
                    );
                }
                search_card = search_card.child(results_col);
            }
            let mut conv_rows = v_flex().gap_0p5();
            for (i, row) in conversations.iter().enumerate() {
                let id = row["id"].as_str().unwrap_or("").to_owned();
                let is_selected = active_conv.as_deref() == Some(id.as_str());
                conv_rows = conv_rows.child(
                    action_button(("ai-conversation", i))
                        .ghost()
                        .w_full()
                        .menu_item(
                            row["title"].as_str().unwrap_or("Conversation").to_owned(),
                            None,
                        )
                        .selected(is_selected)
                        .on_click_accessible(cx.listener(move |v, _, _, cx| {
                            let ai = &mut v.tabs[v.active].assistant;
                            ai.conversation = Some(id.clone());
                            ai.answer = Value::Null;
                            ai.receipt = Value::Null;
                            ai.proposals.clear();
                            ai.transcript.clear();
                            ai.exec_ids.clear();
                            ai.selected_cells.clear();
                            ai.task = "ask".into();
                            ai.show_history = false;
                            v.ai("conversation.get", json!({"conversation_id": id}));
                            cx.notify();
                        })),
                );
            }
            col = col.child(search_card).child(
                v_flex()
                    .gap_1()
                    .child(
                        div()
                            .px_1()
                            .text_xs()
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(muted)
                            .child("Conversations"),
                    )
                    .when(conversations.is_empty(), |d| {
                        d.child(
                            div()
                                .px_1()
                                .text_sm()
                                .text_color(muted)
                                .child("No saved conversations yet."),
                        )
                    })
                    .child(conv_rows),
            );
            col = col.child(
                action_button("assistant-proposals-btn")
                    .ghost()
                    .small()
                    .self_start()
                    .icon(Glyph::FileText)
                    .label("Show proposals")
                    .accessibility_label("Proposals")
                    .hint("List cell and code proposals")
                    .unavailable(busy)
                    .on_click_accessible(cx.listener(|v, _, _, cx| {
                        v.tabs[v.active].assistant.show_history = false;
                        v.ai("proposals.list", json!({}));
                        cx.notify();
                    })),
            );
        }
        Some(col.into_any_element())
    }

    /// A question from the person, as a right-aligned bubble.
    fn assistant_user_bubble(text: &str, cx: &App) -> Div {
        h_flex().w_full().justify_end().child(
            div()
                .max_w(relative(0.85))
                .px_3()
                .py_2()
                .rounded(px(14.))
                .bg(cx.theme().muted)
                .text_sm()
                .whitespace_normal()
                .child(text.to_owned()),
        )
    }

    /// Earlier turns. The current answer renders after this with its action bar.
    #[inline(never)]
    fn assistant_transcript(&mut self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let state = &self.tabs[self.active].assistant;
        if state.transcript.is_empty() {
            return None;
        }
        let transcript = state.transcript.clone();
        let current = state.answer["request_id"].clone();
        let has_answer = !state.answer.is_null();
        let mut col = v_flex().w_full().gap_4();
        for (i, message) in transcript.iter().enumerate() {
            let is_current = has_answer && message["request_id"] == current;
            let text = message["text"].as_str().unwrap_or("");
            if message["role"].as_str() == Some("user") {
                col = col.child(Self::assistant_user_bubble(text, cx));
                continue;
            }
            if is_current {
                continue;
            }
            let mut turn = v_flex().w_full().gap_2().child(
                TextView::markdown(
                    SharedString::from(format!("ai-message-{i}")),
                    safe_markdown(text),
                )
                .selectable(true),
            );
            if let Some(citations) = message["citations"].as_array()
                && !citations.is_empty()
            {
                let mut links = h_flex().gap_1().flex_wrap();
                for (index, citation) in citations.iter().enumerate() {
                    let kind = citation["kind"].as_str().unwrap_or("").to_owned();
                    let id = citation["id"].as_str().unwrap_or("").to_owned();
                    links = links.child(
                        action_button(("ai-history-target", i * 100 + index))
                            .outline()
                            .small()
                            .label(format!("@{}:{}", kind, short_title(&id, 24)))
                            .unavailable(citation["verified"] != true)
                            .on_click_accessible(
                                cx.listener(move |v, _, _, cx| v.navigate_citation(&kind, &id, cx)),
                            ),
                    );
                }
                turn = turn.child(links);
            }
            col = col.child(turn);
        }
        Some(col.into_any_element())
    }

    #[inline(never)]
    pub(super) fn assistant_panel(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let input = self.ai_question(window, cx);
        let search_input = self.ai_input("Search history query", "", window, cx);

        let header = self.assistant_header(cx);
        let composer = self.assistant_composer_dock(input, window, cx);
        let idle = self.assistant_idle();
        let history = self.assistant_history_view(search_input, cx);
        let thread = match history {
            Some(history) => v_flex().w_full().p_3().child(history),
            None if idle => {
                let welcome = self.assistant_welcome(cx);
                // A new chat shows only work in progress or a failure, not
                // diagnostic notes left over from a connection test.
                let ai = &self.tabs[self.active].assistant;
                let status = if ai.busy || ai.error {
                    self.assistant_status_section(cx)
                } else {
                    None
                };
                v_flex()
                    .w_full()
                    .min_h_full()
                    .justify_center()
                    .gap_4()
                    .p_4()
                    .child(welcome)
                    .children(status)
            }
            None => {
                let setup = self.assistant_setup_card(cx);
                let transcript = self.assistant_transcript(cx);
                let receipt = self.assistant_receipt_section(cx);
                let status = self.assistant_status_section(cx);
                let answer = self.assistant_answer_section(cx);
                let proposals = self.assistant_proposals_section(cx);
                let ai = &self.tabs[self.active].assistant;
                let pending = (ai.answer.is_null()
                    && !ai.last_question.is_empty()
                    && (ai.busy || !ai.receipt.is_null()))
                .then(|| Self::assistant_user_bubble(&ai.last_question, cx));
                v_flex()
                    .w_full()
                    .min_w_0()
                    .gap_4()
                    .p_4()
                    .children(setup)
                    .children(transcript)
                    .children(pending)
                    .children(receipt)
                    .children(status)
                    .children(answer)
                    .children(proposals)
            }
        };

        // Follow the conversation: always on a new step, and while a draft
        // streams only if the reader has not scrolled up.
        let ai = &self.tabs[self.active].assistant;
        let mark = (
            ai.transcript.len(),
            ai.receipt.is_null(),
            ai.answer.is_null(),
            ai.proposals.len(),
            ai.busy,
            ai.draft.len(),
        );
        let previous = self.assistant_scroll_mark;
        if mark != previous {
            let structural = (mark.0, mark.1, mark.2, mark.3, mark.4)
                != (previous.0, previous.1, previous.2, previous.3, previous.4);
            let near_bottom =
                self.assistant_scroll.offset().y <= -self.assistant_scroll.max_offset().y + px(48.);
            if structural || near_bottom {
                self.assistant_scroll.scroll_to_bottom();
            }
            self.assistant_scroll_mark = mark;
        }

        v_flex()
            .id("assistant-inspector")
            .debug_selector(|| "assistant-inspector".into())
            .track_focus(&self.assistant_focus)
            .size_full()
            .min_w_0()
            .bg(cx.theme().background)
            .border_l_1()
            .border_color(divider(cx))
            .child(header)
            .child(
                div()
                    .id("assistant-inspector-scroll")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .track_scroll(&self.assistant_scroll)
                    .child(thread),
            )
            .child(composer)
            .into_any_element()
    }

    #[inline(never)]
    fn ai_settings_provider_group(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let root = self.tabs[self.active].assistant.settings.clone();
        let settings = root["settings"].clone();
        let provider = self.tabs[self.active]
            .assistant
            .provider_draft
            .clone()
            .unwrap_or_else(|| {
                settings["provider"]
                    .as_str()
                    .unwrap_or("openai-compatible")
                    .to_owned()
            });

        let model = self.ai_input(
            "Model",
            settings["model"].as_str().unwrap_or(""),
            window,
            cx,
        );
        let endpoint = self.ai_input(
            "Base URL",
            settings["providers"][&provider]["base_url"]
                .as_str()
                .unwrap_or("http://127.0.0.1:11434/v1"),
            window,
            cx,
        );
        let coding_model = self.ai_input(
            "Coding task model override",
            settings["task_models"]["edit"].as_str().unwrap_or(""),
            window,
            cx,
        );
        let key = self.ai_input("API key", "", window, cx);
        let cap = self.ai_input(
            "Daily token cap",
            &settings["daily_token_cap"]
                .as_u64()
                .unwrap_or(100000)
                .to_string(),
            window,
            cx,
        );
        let output = self.ai_input(
            "Maximum output tokens",
            &settings["max_output_tokens"]
                .as_u64()
                .unwrap_or(2048)
                .to_string(),
            window,
            cx,
        );
        let context_bytes = self.ai_input(
            "Context byte budget",
            &settings["context_bytes"]
                .as_u64()
                .unwrap_or(32768)
                .to_string(),
            window,
            cx,
        );
        let retention = self.ai_input(
            "Retention days",
            &settings["retention_days"]
                .as_u64()
                .unwrap_or(90)
                .to_string(),
            window,
            cx,
        );

        let timeout = self.ai_input(
            "Request deadline seconds",
            &settings["timeout"].as_u64().unwrap_or(60).to_string(),
            window,
            cx,
        );
        let context_window = self.ai_input(
            "Configured context window tokens",
            &settings["providers"][&provider]["context_window"]
                .as_u64()
                .map(|v| v.to_string())
                .unwrap_or_default(),
            window,
            cx,
        );
        let schema_provider = provider.clone();
        let schema_a11y_provider = provider.clone();
        let direct_provider = provider.clone();
        let schema_view = cx.entity();
        let schema_option = Checkbox::new("ai-schema-mode")
            .label("Enforce JSON schema (server must support it)")
            .checked(settings["providers"][&provider]["structured_output"] == true)
            .on_a11y_action(accesskit::Action::Click, move |_, _, cx| {
                schema_view.update(cx, |v, cx| {
                    let options = &mut v.tabs[v.active].assistant.settings["settings"]["providers"]
                        [&schema_a11y_provider];
                    if !options.is_object() {
                        *options = json!({});
                    }
                    options["structured_output"] = json!(options["structured_output"] != true);
                    cx.notify();
                });
            })
            .on_click(cx.listener(move |v, checked: &bool, _, cx| {
                let options = &mut v.tabs[v.active].assistant.settings["settings"]["providers"]
                    [&schema_provider];
                if !options.is_object() {
                    *options = json!({});
                }
                options["structured_output"] = json!(checked);
                cx.notify();
            }));
        let direct_option = Checkbox::new("ai-direct-answer")
            .label("Lemonade extension: request direct answers (thinking off)")
            .checked(settings["providers"][&provider]["enable_thinking"] == false)
            .on_click(cx.listener(move |v, checked: &bool, _, cx| {
                let options = &mut v.tabs[v.active].assistant.settings["settings"]["providers"]
                    [&direct_provider];
                if !options.is_object() {
                    *options = json!({});
                }
                if *checked {
                    options["enable_thinking"] = json!(false);
                } else {
                    options.as_object_mut().unwrap().remove("enable_thinking");
                }
                cx.notify();
            }));
        // Inline validation: each numeric field reports its own problem, and
        // Save stays unavailable until every field holds a whole number.
        let invalid_budget = [&cap, &output, &context_bytes, &retention]
            .iter()
            .any(|input| input.read(cx).value().trim().parse::<u64>().is_err());
        let invalid_deadline = timeout.read(cx).value().trim().parse::<u64>().is_err();
        let window_text = context_window.read(cx).value().trim().to_owned();
        let invalid_window = !window_text.is_empty() && window_text.parse::<u64>().is_err();
        let form_problem = if invalid_budget {
            Some(
                "Budgets must be whole numbers. Fix the highlighted field under Privacy and limits.",
            )
        } else if invalid_deadline {
            Some("Request deadline must be a whole number of seconds (Advanced settings).")
        } else if invalid_window {
            Some("Context window must be empty or a whole number of tokens (Advanced settings).")
        } else {
            None
        };
        let connection = self.tabs[self.active].assistant.connection.clone();
        let advanced_settings = self.tabs[self.active].assistant.advanced_settings;
        let theme = cx.theme();
        let (success, info, danger, muted) = (
            theme.success,
            theme.info,
            theme.danger,
            theme.muted_foreground,
        );

        let mut provider_choices = segmented_track(cx);
        for (choice, label) in [
            ("openai-compatible", "Local / compatible"),
            ("openai", "OpenAI API"),
        ] {
            let endpoint = endpoint.clone();
            let key = key.clone();
            let context_window = context_window.clone();
            provider_choices = provider_choices.child(
                action_button(SharedString::from(format!("ai-provider-{choice}")))
                    .small()
                    .label(label)
                    .segment(provider == choice)
                    .on_click_accessible(cx.listener(move |v, _, window, cx| {
                        let ai = &mut v.tabs[v.active].assistant;
                        if ai.provider_draft.as_deref().unwrap_or_else(|| {
                            ai.settings["settings"]["provider"]
                                .as_str()
                                .unwrap_or("openai-compatible")
                        }) == choice
                        {
                            return;
                        }
                        ai.provider_draft = Some(choice.into());
                        key.update(cx, |s, cx| s.set_value("", window, cx));
                        let options = v.tabs[v.active].assistant.settings["settings"]["providers"]
                            [choice]
                            .clone();
                        endpoint.update(cx, |s, cx| {
                            s.set_value(
                                options["base_url"]
                                    .as_str()
                                    .unwrap_or(if choice == "openai" {
                                        "https://api.openai.com/v1"
                                    } else {
                                        "http://127.0.0.1:11434/v1"
                                    })
                                    .to_owned(),
                                window,
                                cx,
                            )
                        });
                        context_window.update(cx, |s, cx| {
                            s.set_value(
                                options["context_window"]
                                    .as_u64()
                                    .map(|n| n.to_string())
                                    .unwrap_or_default(),
                                window,
                                cx,
                            )
                        });
                        cx.notify();
                    })),
            );
        }
        let save_provider = {
            let prov_key = provider.clone();
            let model = model.clone();
            let endpoint = endpoint.clone();
            let timeout = timeout.clone();
            let context_window = context_window.clone();
            let coding_model = coding_model.clone();
            let cap = cap.clone();
            let output = output.clone();
            let context_bytes = context_bytes.clone();
            let retention = retention.clone();
            action_button("ai-save-provider")
                .primary()
                .small()
                .label("Save configuration")
                .unavailable(form_problem.is_some())
                .hint(form_problem.unwrap_or("Save provider, model, and budgets"))
                .on_click_accessible(cx.listener(move |v, _, _, cx| {
                    let mut config = v.tabs[v.active].assistant.settings["settings"].clone();
                    if !config.is_object() {
                        config = json!({});
                    }
                    config["provider"] = json!(prov_key.clone());
                    config["model"] = json!(model.read(cx).value().to_string());
                    if !config["providers"].is_object() {
                        config["providers"] = json!({});
                    }
                    merge_provider(
                        &mut config,
                        &prov_key,
                        endpoint.read(cx).value().to_string(),
                    );
                    let deadline = timeout.read(cx).value().parse::<u64>();
                    let window_text = context_window.read(cx).value().trim().to_owned();
                    if let Ok(deadline) = deadline {
                        config["timeout"] = json!(deadline);
                    } else {
                        v.tabs[v.active].assistant.error = true;
                        v.tabs[v.active].assistant.status =
                            "Request deadline must be a whole number of seconds.".into();
                        cx.notify();
                        return;
                    }
                    if window_text.is_empty() {
                        config["providers"][&prov_key]
                            .as_object_mut()
                            .unwrap()
                            .remove("context_window");
                    } else if let Ok(window) = window_text.parse::<u64>() {
                        config["providers"][&prov_key]["context_window"] = json!(window);
                    } else {
                        v.tabs[v.active].assistant.error = true;
                        v.tabs[v.active].assistant.status =
                            "Context window must be empty (unknown) or a whole number of tokens."
                                .into();
                        cx.notify();
                        return;
                    }
                    if prov_key != "openai-compatible" {
                        config["providers"][&prov_key]
                            .as_object_mut()
                            .unwrap()
                            .remove("enable_thinking");
                    }
                    let coding_override = coding_model.read(cx).value().trim().to_string();
                    if !config["task_models"].is_object() {
                        config["task_models"] = json!({});
                    }
                    if coding_override.is_empty() {
                        if let Some(map) = config["task_models"].as_object_mut() {
                            map.remove("edit");
                            map.remove("refactor");
                            map.remove("tests");
                        }
                    } else {
                        config["task_models"]["edit"] = json!(coding_override);
                        config["task_models"]["refactor"] = json!(coding_override);
                        config["task_models"]["tests"] = json!(coding_override);
                    }
                    let parsed = (
                        cap.read(cx).value().parse::<u64>(),
                        output.read(cx).value().parse::<u64>(),
                        context_bytes.read(cx).value().parse::<u64>(),
                        retention.read(cx).value().parse::<u64>(),
                    );
                    if let (Ok(cap), Ok(output), Ok(cbytes), Ok(retention)) = parsed {
                        config["daily_token_cap"] = json!(cap);
                        config["max_output_tokens"] = json!(output);
                        config["context_bytes"] = json!(cbytes.clamp(1024, 262_144));
                        config["retention_days"] = json!(retention);
                    } else {
                        v.tabs[v.active].assistant.status =
                            "Budgets and retention must be positive integers.".into();
                        cx.notify();
                        return;
                    }
                    v.ai("settings.update", json!({"settings": config}));
                    cx.notify();
                }))
        };

        let mut endpoint_presets = h_flex().gap_1p5().flex_wrap();
        for (preset_label, preset_url) in [
            ("Lemonade", "http://127.0.0.1:13305/api/v1"),
            ("Ollama", "http://127.0.0.1:11434/v1"),
            ("LM Studio", "http://127.0.0.1:1234/v1"),
            ("vLLM", "http://127.0.0.1:8000/v1"),
        ] {
            let endpoint_input = endpoint.clone();
            endpoint_presets = endpoint_presets.child(
                action_button(SharedString::from(format!("ai-preset-{preset_label}")))
                    .ghost()
                    .small()
                    .label(preset_label)
                    .on_click_accessible(cx.listener(move |_, _, window, cx| {
                        endpoint_input.update(cx, |s, cx| {
                            s.set_value(preset_url.to_owned(), window, cx);
                        });
                    })),
            );
        }

        let discovered_models = self.tabs[self.active]
            .inspections
            .get("AI models")
            .and_then(|v| v["items"].as_array().or_else(|| v["models"].as_array()))
            .cloned()
            .unwrap_or_default();
        let mut model_chips = h_flex().gap_1p5().flex_wrap();
        for (i, m) in discovered_models.iter().take(12).enumerate() {
            if let Some(m_name) = m.as_str().or_else(|| m["id"].as_str()).map(str::to_owned) {
                let model_input = model.clone();
                model_chips = model_chips.child(
                    action_button(("ai-picked-model", i))
                        .outline()
                        .small()
                        .label(m_name.clone())
                        .on_click_accessible(cx.listener(move |_, _, window, cx| {
                            model_input.update(cx, |s, cx| s.set_value(m_name.clone(), window, cx));
                        })),
                );
            }
        }

        let saved_configured = settings["model"]
            .as_str()
            .is_some_and(|m| !m.trim().is_empty());
        settings_group("Assistant connection", cx)
            .id("settings-ai-provider")
            .child(div().text_sm().text_color(muted).child("Choose a provider, enter its model, then save and test the connection."))
            .child(setting_row("Provider", "Select a local or compatible server, or the OpenAI API.", provider_choices, cx))
            .child(setting_row("Base URL", "The server endpoint used by this provider.",
                v_flex().gap_2().child(Input::new(&endpoint))
                    .when(provider == "openai-compatible", |d| d.child(endpoint_presets)), cx))
            .when_some(
                match &connection {
                    Connection::Failed(message) => Some(message.clone()),
                    _ => None,
                },
                |d, message| {
                    d.child(
                        div()
                            .id("ai-test-failure")
                            .role(Role::Alert)
                            .aria_label(format!("Connection failed: {message}"))
                            .text_sm()
                            .text_color(danger)
                            .whitespace_normal()
                            .child(format!("Connection failed: {message}")),
                    )
                },
            )
            .child(setting_row(
                "Model",
                "Default model identifier used for notebook requests.",
                v_flex()
                    .gap_2()
                    .child(Input::new(&model))
                    .when(!discovered_models.is_empty(), |d| d.child(model_chips)),
                cx,
            ))
            .when(provider == "openai" || advanced_settings, |d| d.child(setting_row(
                "API key",
                "Stored securely in the OS credential store and redacted from all logs, receipts, and notebook data.",
                v_flex()
                    .gap_2()
                    .child(Input::new(&key).mask_toggle())
                    .child(
                        h_flex()
                            .gap_2()
                            .flex_wrap()
                            .child(
                                action_button("ai-save-key")
                                    .outline()
                                    .small()
                                    .label("Store API key")
                                    .on_click_accessible(cx.listener(move |v, _, window, cx| {
                                        let provider = v.tabs[v.active].assistant.provider_draft.clone()
                                            .unwrap_or_else(|| v.tabs[v.active].assistant.settings["settings"]["provider"].as_str().unwrap_or("openai-compatible").to_owned());
                                        let secret = key.read(cx).value().to_string();
                                        key.update(cx, |s, cx| s.set_value("", window, cx));
                                        v.ai(
                                            "credential.set",
                                            json!({"provider": provider, "credential": secret}),
                                        );
                                        cx.notify();
                                    })),
                            )
                            .when(advanced_settings, |d| d.child(
                                action_button("ai-delete-key")
                                    .ghost()
                                    .small()
                                    .label("Remove API key")
                                    .on_click_accessible(cx.listener(|v, _, _, cx| {
                                        let provider = v.tabs[v.active].assistant.provider_draft.clone()
                                            .unwrap_or_else(|| v.tabs[v.active].assistant.settings["settings"]["provider"].as_str().unwrap_or("openai-compatible").to_owned());
                                        v.ai("credential.delete", json!({"provider": provider}));
                                        cx.notify();
                                    })),
                            )),
                    ),
                cx,
            )))
            .child(h_flex().gap_2().items_center().flex_wrap()
                .child(save_provider)
                .child(action_button("ai-test").outline().small().label("Test saved connection")
                    .hint("Tests the saved configuration; save changes first")
                    .unavailable(!saved_configured || connection == Connection::Testing)
                    .on_click_accessible(cx.listener(|v, _, _, cx| v.test_provider(cx))))
                .child(match &connection {
                    Connection::Untested => badge("Not tested", muted),
                    Connection::Testing => badge("Testing\u{2026}", info),
                    Connection::Ready => badge("Connected", success),
                    Connection::Failed(_) => badge("Connection failed", danger),
                }))
            .when_some(form_problem, |d, problem| {
                d.child(
                    div()
                        .id("ai-form-error")
                        .role(Role::Alert)
                        .aria_label(problem)
                        .text_sm()
                        .text_color(danger)
                        .whitespace_normal()
                        .child(problem),
                )
            })
            .child(h_flex().gap_1().flex_wrap()
                .child(action_button("ai-advanced-settings").ghost().small()
                    .label(if advanced_settings { "Hide advanced settings" } else { "Advanced settings" })
                    .dropdown_caret(true)
                    .on_click_accessible(cx.listener(|v, _, _, cx| {
                        v.tabs[v.active].assistant.advanced_settings = !v.tabs[v.active].assistant.advanced_settings;
                        cx.notify();
                    })))
                .child(action_button("ai-open-sidebar").ghost().small().label("Open Assistant")
                    .on_click_accessible(cx.listener(|v, _, _, cx| {v.select_surface("Notebook", cx);v.open_assistant(cx);}))))
            .when(advanced_settings, |d| d
            .child(h_flex().gap_2().flex_wrap()
                .child(action_button("ai-load-settings").outline().small().label("Reload saved settings")
                    .on_click_accessible(cx.listener(|v, _, _, _| v.ai("settings.get", json!({})))))
                .child(action_button("ai-models").outline().small().label("List models from saved connection")
                    .on_click_accessible(cx.listener(|v, _, _, _| v.ai("models.list", json!({}))))))
            .child(setting_row(
                "Coding model override",
                "Optional specialized model for edit, refactor, and test generation tasks.",
                div().min_w(px(240.)).flex_1().child(Input::new(&coding_model)),
                cx,
            ))
            .child(setting_row("Inference profile", "Unknown context window: leave empty. Context size is a configured estimate, not server qualification. The direct-answer extension is only for a compatible Lemonade server; Save applies changes. No model is downloaded or loaded.", v_flex().gap_2().child(schema_option).child(direct_option).child(Input::new(&timeout)).child(Input::new(&context_window)), cx))
            )
            .into_any_element()
    }

    #[inline(never)]
    fn ai_settings_policy_and_privacy(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let root = self.tabs[self.active].assistant.settings.clone();
        let settings = root["settings"].clone();
        let current_policy = root["policy"].as_str().unwrap_or("allowed").to_owned();
        let cap = self.ai_input(
            "Daily token cap",
            &settings["daily_token_cap"]
                .as_u64()
                .unwrap_or(100000)
                .to_string(),
            window,
            cx,
        );
        let output = self.ai_input(
            "Maximum output tokens",
            &settings["max_output_tokens"]
                .as_u64()
                .unwrap_or(2048)
                .to_string(),
            window,
            cx,
        );
        let context_bytes = self.ai_input(
            "Context byte budget",
            &settings["context_bytes"]
                .as_u64()
                .unwrap_or(32768)
                .to_string(),
            window,
            cx,
        );
        let retention = self.ai_input(
            "Retention days",
            &settings["retention_days"]
                .as_u64()
                .unwrap_or(90)
                .to_string(),
            window,
            cx,
        );
        let theme = cx.theme();
        let (muted, mono) = (theme.muted_foreground, theme.mono_font_family.clone());

        let mut policy_track = segmented_track(cx);
        for (pol_key, pol_label) in [
            ("off", "Off"),
            ("local_only", "Local only"),
            ("allowed", "Allowed"),
        ] {
            let selected = current_policy == pol_key;
            policy_track = policy_track.child(
                action_button(SharedString::from(format!("ai-policy-{pol_key}")))
                    .small()
                    .label(pol_label)
                    .segment(selected)
                    .on_click_accessible(cx.listener(move |v, _, _, cx| {
                        v.ai("policy.update", json!({"policy": pol_key}));
                        cx.notify();
                    })),
            );
        }

        let approvals = settings["approvals"]
            .as_object()
            .cloned()
            .unwrap_or_default();
        let mut approvals_box = v_flex().gap_1p5();
        if approvals.is_empty() {
            approvals_box = approvals_box.child(
                div()
                    .text_xs()
                    .text_color(muted)
                    .child("None saved \u{2014} remote requests require manual receipt approval."),
            );
        } else {
            for (idx, (k, _)) in approvals.iter().enumerate() {
                let key_name = k.clone();
                approvals_box = approvals_box.child(
                    h_flex()
                        .justify_between()
                        .items_center()
                        .gap_2()
                        .p_2()
                        .rounded(px(6.))
                        .bg(theme.muted)
                        .child(
                            div()
                                .text_xs()
                                .font_family(mono.clone())
                                .whitespace_normal()
                                .child(k.clone()),
                        )
                        .child(
                            action_button(("ai-revoke-approval", idx))
                                .ghost()
                                .small()
                                .label("Revoke")
                                .on_click_accessible(cx.listener(move |v, _, _, cx| {
                                    let mut config =
                                        v.tabs[v.active].assistant.settings["settings"].clone();
                                    if let Some(map) = config["approvals"].as_object_mut() {
                                        map.remove(&key_name);
                                    }
                                    v.ai("settings.update", json!({"settings": config}));
                                    cx.notify();
                                })),
                        ),
                );
            }
        }

        let danger = cx.theme().danger;
        let labeled = |label: &'static str, input: &Entity<InputState>| {
            let invalid = input.read(cx).value().trim().parse::<u64>().is_err();
            v_flex()
                .min_w(px(130.))
                .flex_1()
                .gap_1()
                .child(div().text_xs().text_color(muted).child(label))
                .child(
                    div()
                        .rounded(px(6.))
                        .when(invalid, |d| d.border_1().border_color(danger))
                        .child(Input::new(input)),
                )
                .when(invalid, |d| {
                    d.child(
                        div()
                            .id(SharedString::from(format!("ai-invalid-{label}")))
                            .role(Role::Alert)
                            .aria_label(format!("{label}: enter a whole number"))
                            .text_xs()
                            .text_color(danger)
                            .child("Enter a whole number"),
                    )
                })
        };
        settings_group("Privacy and limits", cx)
            .id("settings-ai-policy")
            .child(setting_row(
                "Policy",
                "Off blocks all AI features. Local only restricts requests to this computer (127.0.0.1, ::1, localhost).",
                policy_track,
                cx,
            ))
            .child(setting_row(
                "Budgets",
                "Context bytes range from 1 KB to 256 KB. Save with Save configuration above.",
                v_flex()
                    .gap_2()
                    .child(
                        h_flex()
                            .gap_2()
                            .flex_wrap()
                            .items_start()
                            .child(labeled("Daily tokens", &cap))
                            .child(labeled("Max output tokens", &output))
                            .child(labeled("Context bytes", &context_bytes))
                            .child(labeled("Keep history (days)", &retention)),
                    )
                    .child(
                        div().text_xs().text_color(muted).whitespace_normal().child(format!(
                            "Used today: {} tokens, counting failed requests and tests (UTC).",
                            root["usage"]["used_today"].as_u64().map(|v| v.to_string()).unwrap_or_else(|| "unknown".into())
                        )),
                    ),
                cx,
            ))
            .child(setting_row(
                "Trusted destinations",
                "Saved notebook + endpoint pairs that auto-approve context receipts.",
                approvals_box,
                cx,
            ))
            .child(setting_row(
                "Local assistant data",
                "Clear saved conversations, receipts, proposals, and usage counters for this notebook.",
                h_flex()
                    .gap_2()
                    .flex_wrap()
                    .child(
                        action_button("ai-prune")
                            .outline()
                            .small()
                            .label("Prune expired")
                            .accessibility_label("Prune expired AI data")
                            .on_click_accessible(cx.listener(|v, _, _, cx| {
                                v.ai("prune", json!({}));
                                cx.notify();
                            })),
                    )
                    .child(
                        action_button("ai-purge")
                            .danger()
                            .small()
                            .label(if self.tabs[self.active].assistant.purge_armed { "Confirm delete all AI data" } else { "Delete all AI data" })
                            .on_click_accessible(cx.listener(|v, _, _, cx| {
                                if !v.tabs[v.active].assistant.purge_armed {
                                    v.tabs[v.active].assistant.purge_armed = true;
                                    v.tabs[v.active].assistant.status = "Delete conversations and assistant records? Click Confirm delete all AI data to continue; notebook source/history are preserved.".into(); cx.notify(); return;
                                }
                                v.tabs[v.active].assistant.purge_armed = false;
                                v.ai("purge", json!({}));
                                let ai = &mut v.tabs[v.active].assistant;
                                ai.transcript.clear();
                                ai.receipt = Value::Null;
                                ai.answer = Value::Null;
                                ai.proposals.clear();
                                cx.notify();
                            })),
                    ),
                cx,
            ))
            .into_any_element()
    }

    pub(super) fn ai_settings(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        if self.tabs.is_empty() {
            return div().into_any_element();
        }
        let provider_group = self.ai_settings_provider_group(window, cx);
        let policy_group = self.ai_settings_policy_and_privacy(window, cx);
        v_flex()
            .child(provider_group)
            .child(policy_group)
            .into_any_element()
    }
}
