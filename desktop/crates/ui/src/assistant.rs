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
        let ai = &self.tabs[self.active].assistant;
        let idle = ai.transcript.is_empty()
            && ai.receipt.is_null()
            && ai.answer.is_null()
            && ai.proposals.is_empty()
            && !ai.busy;
        let setup = assistant_setup(&ai.settings, &ai.connection);
        let model = ai.settings["settings"]["model"]
            .as_str()
            .unwrap_or("")
            .to_owned();
        let failed = matches!(setup, Setup::Failed(_));
        if !(idle || failed) || matches!(setup, Setup::Loading | Setup::Ready) {
            return None;
        }
        let theme = cx.theme();
        let (muted, warning, info) = (theme.muted_foreground, theme.warning, theme.info);
        let (title, body, accent): (&str, String, Hsla) = match &setup {
            Setup::Off => (
                "Assistant is off for this notebook",
                "The notebook's AI policy blocks requests. You can change the policy in provider settings.".into(),
                muted,
            ),
            Setup::Unconfigured => (
                "Connect a provider",
                "The Assistant uses a model you choose: a local server such as Ollama or LM Studio, or the OpenAI API. Nothing from this notebook is sent until you review exactly what will be shared.".into(),
                info,
            ),
            Setup::Untested => (
                "Test the connection",
                format!("Provider settings are saved for {model}. Test the connection once before your first request; the test sends no notebook content."),
                info,
            ),
            Setup::Testing => (
                "Testing the connection\u{2026}",
                "Checking that the provider responds. No notebook content is sent.".into(),
                info,
            ),
            Setup::Failed(message) => (
                "Connection failed",
                format!(
                    "{}. Check the base URL, model, and API key in provider settings, then try again.",
                    message.trim_end_matches('.')
                ),
                warning,
            ),
            Setup::Loading | Setup::Ready => unreachable!(),
        };
        let testing = setup == Setup::Testing;
        let can_test = matches!(setup, Setup::Untested | Setup::Failed(_));
        Some(
            card(cx)
                .id("assistant-setup")
                .role(Role::Group)
                .aria_label(title)
                .p_3()
                .gap_2()
                .border_l_2()
                .border_color(accent)
                .child(
                    div()
                        .text_sm()
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(title),
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(muted)
                        .whitespace_normal()
                        .child(body),
                )
                .child(
                    h_flex()
                        .gap_1p5()
                        .flex_wrap()
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
                        .child(
                            action_button("assistant-open-settings")
                                .outline()
                                .small()
                                .icon(Glyph::Settings)
                                .label("Open provider settings")
                                .unavailable(testing)
                                .on_click_accessible(
                                    cx.listener(|v, _, _, cx| v.open_provider_settings(cx)),
                                ),
                        ),
                )
                .into_any_element(),
        )
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
                let placeholder = if name == "Question" {
                    "Ask about this notebook\u{2026}"
                } else {
                    name
                };
                let mut state = InputState::new(window, cx).placeholder(placeholder);
                state.set_value(initial.to_owned(), window, cx);
                if name == "API key" {
                    state.set_masked(true, window, cx);
                }
                state
            });
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
        let settings_root = state.settings.clone();
        let theme = cx.theme();
        let (primary, success, warning, danger, info) = (
            theme.primary,
            theme.success,
            theme.warning,
            theme.danger,
            theme.info,
        );
        let policy = settings_root["policy"].as_str().unwrap_or("allowed");
        let provider_name = settings_root["settings"]["provider"].as_str().unwrap_or("");
        let active_model = effective_model(&settings_root, &state.task);
        let model_name = active_model.as_str();
        let dest_label = settings_root["destination"]["label"]
            .as_str()
            .unwrap_or("Local");
        let dest_local = settings_root["destination"]["local"]
            .as_bool()
            .unwrap_or(true);

        let status_badge = if policy == "off" {
            badge("Off", danger)
        } else if provider_name.is_empty() || model_name.is_empty() {
            badge("Setup", warning)
        } else if policy == "local_only" {
            badge("Local only", warning)
        } else {
            badge(
                dest_label.to_string(),
                if dest_local { success } else { info },
            )
        };

        h_flex()
            .px_3()
            .py_1p5()
            .justify_between()
            .items_center()
            .gap_1p5()
            .bg(theme.title_bar)
            .border_b_1()
            .border_color(divider(cx))
            .child(
                h_flex()
                    .gap_1p5()
                    .items_center()
                    .min_w_0()
                    .child(Icon::new(Glyph::Sparkles).small().text_color(primary))
                    .child(
                        div()
                            .text_sm()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child("Assistant"),
                    )
                    .child(status_badge),
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
                            .selected(show_history)
                            .accessibility_label("Conversations")
                            .hint("Search history & saved conversations")
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
                        action_button("assistant-proposals-btn")
                            .ghost()
                            .small()
                            .icon(Glyph::FileText)
                            .accessibility_label("Proposals")
                            .hint("List cell and code proposals")
                            .unavailable(busy)
                            .on_click_accessible(cx.listener(|v, _, _, cx| {
                                v.ai("proposals.list", json!({}));
                                cx.notify();
                            })),
                    )
                    .child(
                        action_button("assistant-config")
                            .ghost()
                            .small()
                            .icon(Glyph::Settings)
                            .accessibility_label("Assistant provider settings")
                            .hint("AI provider, models, and privacy settings")
                            .on_click_accessible(
                                cx.listener(|v, _, _, cx| v.open_provider_settings(cx)),
                            ),
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
    fn assistant_health_banner(&mut self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let state = &self.tabs[self.active].assistant;
        let busy = state.busy;
        let settings_root = &state.settings;
        let stale_count = settings_root["analysis"]["stale_cells"]
            .as_array()
            .map_or(0, Vec::len);
        let repro_status = settings_root["analysis"]["reproducibility"]["status"]
            .as_str()
            .unwrap_or("ready")
            .to_owned();
        if stale_count == 0 && repro_status == "ready" {
            return None;
        }
        let warning = cx.theme().warning;
        Some(
            h_flex()
                .px_2p5()
                .py_1p5()
                .rounded(px(8.))
                .border_1()
                .border_color(warning.opacity(0.35))
                .bg(warning.opacity(0.06))
                .justify_between()
                .items_center()
                .gap_2()
                .flex_wrap()
                .child(
                    h_flex()
                        .gap_1p5()
                        .items_center()
                        .child(dot(warning))
                        .when(stale_count > 0, |d| {
                            d.child(badge(
                                format!(
                                    "{stale_count} stale cell{}",
                                    if stale_count == 1 { "" } else { "s" }
                                ),
                                warning,
                            ))
                        })
                        .when(repro_status != "ready", |d| {
                            d.child(badge(format!("Repro: {repro_status}"), warning))
                        }),
                )
                .child(
                    h_flex()
                        .gap_1()
                        .flex_wrap()
                        .when(stale_count > 0, |d| {
                            d.child(
                                action_button("ai-banner-stale")
                                    .ghost()
                                    .small()
                                    .icon(Glyph::Sparkles)
                                    .label("Review stale")
                                    .unavailable(busy)
                                    .on_click_accessible(cx.listener(|v, _, _, cx| {
                                        v.ai_preview(
                                            "stale",
                                            "Identify stale or out-of-order cells and recommend the minimal safe rerun or checkpoint recovery.".into(),
                                            cx,
                                        );
                                    })),
                            )
                        })
                        .when(repro_status != "ready", |d| {
                            d.child(
                                action_button("ai-banner-repro")
                                    .ghost()
                                    .small()
                                    .icon(Glyph::Sparkles)
                                    .label("Audit repro")
                                    .unavailable(busy)
                                    .on_click_accessible(cx.listener(|v, _, _, cx| {
                                        v.ai_preview(
                                            "reproducibility",
                                            "Review reproducibility from recorded evidence.".into(),
                                            cx,
                                        );
                                    })),
                            )
                        }),
                )
                .into_any_element(),
        )
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

        for id in &selected_cells {
            let cell_id = id.clone();
            let title = self.context_cell_title(id);
            pills = pills.child(
                action_button(SharedString::from(format!("ai-cell-{id}")))
                    .outline()
                    .small()
                    .icon(Glyph::Code)
                    .label(format!("{} \u{00d7}", short_title(&title, 28)))
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
                    .label(format!("+ {}", short_title(&title, 28)))
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
                .gap_1()
                .child(
                    div()
                        .text_xs()
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(cx.theme().muted_foreground)
                        .child(summary),
                )
                .child(pills)
                .into_any_element(),
        )
    }

    #[inline(never)]
    fn assistant_workflows_section(
        &mut self,
        idle: bool,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if !idle {
            return None;
        }
        let busy = self.tabs[self.active].assistant.busy;
        let has_error = self.tabs[self.active].model.cells.values().any(|cell| {
            matches!(cell.outcome.as_str(), "error" | "failed")
                || cell.output.iter().any(|event| event["kind"] == "error")
        });
        let view_for_explain = cx.entity();
        let view_for_summary = cx.entity();
        let view_for_repro = cx.entity();

        Some(
            h_flex()
                .gap_1()
                .flex_wrap()
                .items_center()
                .child(
                    action_button("assistant-summary")
                        .ghost()
                        .small()
                        .debug_selector("assistant-summary")
                        .label("Resume summary")
                        .hint("Summarize recorded state and what you can continue from")
                        .unavailable(busy)
                        .on_a11y_click(move |_, cx| {
                            view_for_summary.update(cx, |v, cx| {
                                v.ai_preview(
                                    "summary",
                                    "Summarize this notebook's recorded state and what I can continue from. Cite every recorded fact.".into(),
                                    cx,
                                );
                            });
                        })
                        .on_click_accessible(cx.listener(|v, _, _, cx| {
                            v.ai_preview(
                                "summary",
                                "Summarize this notebook's recorded state and what I can continue from. Cite every recorded fact.".into(),
                                cx,
                            );
                        })),
                )
                .when(has_error, |d| {
                    d.child(
                        action_button("assistant-explain")
                            .ghost()
                            .small()
                            .label("Explain error")
                            .hint("Explain the selected cell's recorded error and propose a fix")
                            .unavailable(busy)
                            .on_a11y_click(move |_, cx| {
                                view_for_explain.update(cx, |v, cx| {
                                    v.ai_preview(
                                        "explain",
                                        "Explain this cell's recorded error and propose a fix."
                                            .into(),
                                        cx,
                                    );
                                });
                            })
                            .on_click_accessible(cx.listener(|v, _, _, cx| {
                                v.ai_preview(
                                    "explain",
                                    "Explain this cell's recorded error and propose a fix.".into(),
                                    cx,
                                );
                            })),
                    )
                })
                .child(
                    action_button("assistant-repro")
                        .ghost()
                        .small()
                        .label("Reproducibility")
                        .hint("Review reproducibility from recorded ledger evidence")
                        .unavailable(busy)
                        .on_a11y_click(move |_, cx| {
                            view_for_repro.update(cx, |v, cx| {
                                v.ai_preview(
                                    "reproducibility",
                                    "Review reproducibility from recorded evidence.".into(),
                                    cx,
                                );
                            });
                        })
                        .on_click_accessible(cx.listener(|v, _, _, cx| {
                            v.ai_preview(
                                "reproducibility",
                                "Review reproducibility from recorded evidence.".into(),
                                cx,
                            );
                        })),
                )
                .into_any_element(),
        )
    }

    #[inline(never)]
    fn assistant_composer_dock(
        &mut self,
        input: Entity<InputState>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let context_sec = self.assistant_context_section(cx);
        let idle = {
            let ai = &self.tabs[self.active].assistant;
            ai.transcript.is_empty()
                && ai.receipt.is_null()
                && ai.answer.is_null()
                && ai.proposals.is_empty()
                && !ai.busy
        };
        let workflows = self.assistant_workflows_section(idle, cx);
        let state = &self.tabs[self.active].assistant;
        let busy = state.busy;
        let active_task = if state.task.is_empty() {
            "ask".to_owned()
        } else {
            state.task.clone()
        };
        let settings_root = state.settings.clone();
        let model_name =
            Some(effective_model(&settings_root, &active_task)).filter(|s| !s.is_empty());
        let dest_local = settings_root["destination"]["local"]
            .as_bool()
            .unwrap_or(true);

        let theme = cx.theme();
        let (primary, muted, success, info, mono) = (
            theme.primary,
            theme.muted_foreground,
            theme.success,
            theme.info,
            theme.mono_font_family.clone(),
        );
        let view_for_preview = cx.entity();
        let input_for_a11y = input.clone();
        let input_for_click = input.clone();

        v_flex()
            .p_2p5()
            .gap_2()
            .bg(theme.title_bar)
            .border_t_1()
            .border_color(divider(cx))
            .when_some(workflows, |d, el| d.child(el))
            .when_some(context_sec, |d, el| d.child(el))
            .child(
                v_flex()
                    .p_2()
                    .gap_2()
                    .rounded(px(8.))
                    .border_1()
                    .border_color(surface_edge(cx))
                    .bg(theme.background)
                    .child(Input::new(&input).cleanable(true))
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
                                    .min_w_0()
                                    .when_some(model_name, |d, name| {
                                        d.child(dot(if dest_local { success } else { info }))
                                            .child(
                                                div()
                                                    .text_xs()
                                                    .font_family(mono)
                                                    .text_color(muted)
                                                    .truncate()
                                                    .child(name),
                                            )
                                    })
                                    .when(active_task != "ask", |d| {
                                        d.child(badge(format!("/{active_task}"), primary))
                                    }),
                            )
                            .child(
                                action_button("assistant-preview")
                                    .primary()
                                    .small()
                                    .icon(Glyph::Sparkles)
                                    .label("Review context")
                                    .hint("Preview exactly what will be shared; nothing is sent until you approve it")
                                    .unavailable(busy)
                                    .on_a11y_click(move |_, cx| {
                                        let question = input_for_a11y.read(cx).value().to_string();
                                        view_for_preview.update(cx, |v, cx| {
                                            let task = v.tabs[v.active].assistant.task.clone();
                                            v.ai_preview(if task.is_empty() { "ask" } else { &task }, question, cx);
                                        });
                                    })
                                    .on_click_accessible(cx.listener(move |v, _, _, cx| {
                                        let question = input_for_click.read(cx).value().to_string();
                                        let task = v.tabs[v.active].assistant.task.clone();
                                        v.ai_preview(if task.is_empty() { "ask" } else { &task }, question, cx);
                                    })),
                            ),
                    ),
            )
            .when(idle, |d| {
                d.child(
                    div()
                        .px_1()
                        .text_xs()
                        .text_color(muted)
                        .whitespace_normal()
                        .child("Review context lists exactly what would be shared. Nothing is sent until you approve it."),
                )
            })
            .into_any_element()
    }

    #[inline(never)]
    fn assistant_status_section(&mut self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let state = &self.tabs[self.active].assistant;
        let busy = state.busy;
        let status_text = if busy {
            format!(
                "{}; {} s elapsed; final answer may follow model reasoning",
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
        let theme = cx.theme();
        let (muted, success, warning, danger, info) = (
            theme.muted_foreground,
            theme.success,
            theme.warning,
            theme.danger,
            theme.info,
        );
        let is_err = state.error;
        let status_color = if is_err {
            danger
        } else if busy {
            info
        } else {
            success
        };
        let mut col = v_flex().gap_2().child(
            card(cx)
                .id("assistant-status-card")
                .role(if is_err { Role::Alert } else { Role::Status })
                .aria_label(status_text.clone())
                .p_2p5()
                .gap_2()
                .border_l_2()
                .border_color(status_color)
                .child(
                    h_flex()
                        .justify_between()
                        .items_center()
                        .gap_2()
                        .flex_wrap()
                        .child(
                            h_flex()
                                .gap_2()
                                .items_center()
                                .min_w_0()
                                .flex_1()
                                .child(dot(status_color))
                                .child(
                                    div()
                                        .text_xs()
                                        .font_weight(FontWeight::MEDIUM)
                                        .whitespace_normal()
                                        .min_w_0()
                                        .flex_1()
                                        .child(status_text),
                                ),
                        )
                        .when(busy, |d| {
                            d.child(
                                action_button("assistant-cancel")
                                    .outline()
                                    .small()
                                    .icon(Glyph::Square)
                                    .label("Cancel")
                                    .accessibility_label("Cancel request")
                                    .on_click_accessible(cx.listener(|v, _, _, cx| {
                                        v.send(Command::AssistantCancel);
                                        let ai = &mut v.tabs[v.active].assistant;
                                        ai.busy = false;
                                        ai.receipt = Value::Null;
                                        ai.receipt_state = ReceiptState::Consumed;
                                        ai.started = None;
                                        ai.error = false;
                                        ai.rebuild = true;
                                        ai.status =
                                            "Cancelled; prepare fresh context to continue.".into();
                                        cx.notify();
                                    })),
                            )
                        }),
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
                card(cx)
                    .p_3()
                    .gap_2()
                    .child(
                        h_flex()
                            .gap_2()
                            .items_center()
                            .child(badge("Streaming draft", warning))
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(muted)
                                    .child("Verifying citations\u{2026}"),
                            ),
                    )
                    .child(TextView::markdown("ai-stream-draft", safe_markdown(&draft))),
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

        let mut receipt_card = card(cx)
            .p_3()
            .gap_2()
            .border_l_2()
            .border_color(if r_local {
                success.opacity(0.6)
            } else {
                warning.opacity(0.75)
            })
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
                            .child(Icon::new(Glyph::Terminal).small().text_color(muted))
                            .child(
                                div()
                                    .text_xs()
                                    .font_family(mono.clone())
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child("Context receipt"),
                            )
                            .child(badge(
                                r_label.to_string(),
                                if r_local { success } else { warning },
                            ))
                            .child(
                                div()
                                    .text_xs()
                                    .font_family(mono.clone())
                                    .child(format!("{r_host} \u{00b7} {r_model}")),
                            ),
                    )
                    .when(auto_approved, |d| d.child(badge("Auto-approved", info))),
            )
            .child(
                div()
                    .text_xs()
                    .font_family(mono.clone())
                    .text_color(muted)
                    .child(format!(
                        "{} B \u{00b7} ~{} tokens \u{00b7} #{}",
                        receipt["bytes"],
                        receipt["token_estimate"],
                        &r_hash[..r_hash.len().min(12)]
                    )),
            );

        receipt_card = receipt_card.child(div().text_xs().whitespace_normal().child(format!("Question: {}\nIncluded records: {}; omitted: {}; truncated: {}\nOutput allowance: {} tokens; context window: {} (configured; unknown if null)", receipt["question"].as_str().unwrap_or(""), receipt["selection"]["included"], receipt["selection"]["omitted"], receipt["selection"]["truncated"], receipt["profile"]["max_output_tokens"], receipt["profile"]["context_window"])));
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
            let mut items_col = v_flex().gap_1p5();
            for (index, item) in items.iter().enumerate() {
                let text = receipt_text(item);
                let kind = item["kind"].as_str().unwrap_or("item");
                let id = item["id"].as_str().unwrap_or("");
                let redacted = item["redacted"] == true;
                let truncated = item["truncated"] == true;
                let title = (kind == "cell" && !id.is_empty()).then(|| self.context_cell_title(id));
                items_col = items_col.child(
                    v_flex()
                        .p_2()
                        .gap_1()
                        .rounded(px(6.))
                        .border_1()
                        .border_color(divider(cx))
                        .bg(theme.background)
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
                                        .child(badge(kind.to_string(), muted))
                                        .when_some(title, |d, title| {
                                            d.child(
                                                div()
                                                    .text_xs()
                                                    .font_weight(FontWeight::MEDIUM)
                                                    .child(short_title(&title, 40)),
                                            )
                                        })
                                        .child(
                                            div()
                                                .text_xs()
                                                .font_family(mono.clone())
                                                .text_color(muted)
                                                .child(id.to_string()),
                                        ),
                                )
                                .child(
                                    h_flex()
                                        .gap_1p5()
                                        .items_center()
                                        .child(
                                            div()
                                                .text_xs()
                                                .font_family(mono.clone())
                                                .text_color(muted)
                                                .child(format!("{} B", item["bytes"])),
                                        )
                                        .when(redacted, |d| d.child(badge("redacted", warning)))
                                        .when(truncated, |d| d.child(badge("truncated", info))),
                                ),
                        )
                        .child(
                            TextView::markdown(
                                SharedString::from(format!("ai-receipt-{index}")),
                                safe_markdown(&text),
                            )
                            .selectable(true),
                        ),
                );
            }
            receipt_card = receipt_card.child(items_col);
        }

        let full = serde_json::to_string_pretty(&receipt).unwrap_or_default();
        let expanded = state.payload_expanded;
        receipt_card = receipt_card.child(
            action_button("assistant-exact-payload")
                .outline()
                .small()
                .label(if expanded {
                    "Hide exact payload and system prompt"
                } else {
                    "Show exact payload and system prompt"
                })
                .on_click_accessible(cx.listener(|v, _, _, cx| {
                    v.tabs[v.active].assistant.payload_expanded =
                        !v.tabs[v.active].assistant.payload_expanded;
                    cx.notify();
                })),
        );
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
                .pt_2()
                .border_t_1()
                .border_color(divider(cx))
                .child(
                    action_button("assistant-send")
                        .primary()
                        .small()
                        .icon(Glyph::Sparkles)
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
                    action_button("ai-receipt-copy")
                        .ghost()
                        .small()
                        .label("Copy receipt")
                        .accessibility_label("Copy full receipt and system prompt")
                        .hint("Copy full JSON receipt and system prompt to clipboard")
                        .on_click_accessible(cx.listener(move |_, _, _, cx| {
                            copy_acknowledged(full.clone(), cx);
                        })),
                )
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
        let mut answer_card = card(cx)
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
                            .child(Icon::new(Glyph::Sparkles).small().text_color(primary))
                            .child(
                                div()
                                    .text_xs()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child("Assistant"),
                            )
                            .child(badge(format!("{resolved}/{} targets found; {supplied} in context", citations.len()), if resolved > 0 { success } else { muted }))
                            .child(info_tip(
                                "ai-evidence-tip",
                                "Citation targets are verified against the ledger where marked. Uncited claims and suggested causes are unverified.",
                                cx,
                            ))
                            .when_some(
                                answer["usage"]["total_tokens"].as_u64(),
                                |d, tokens| {
                                    d.child(
                                        div()
                                            .text_xs()
                                            .font_family(mono)
                                            .text_color(muted)
                                            .child(format!("{tokens} tok")),
                                    )
                                },
                            ),
                    )
                    .child(
                        h_flex()
                            .gap_1()
                            .items_center()
                            .child(
                                action_button("ai-copy-answer")
                                    .ghost()
                                    .small()
                                    .label("Copy")
                                    .accessibility_label("Copy answer")
                                    .on_click_accessible(cx.listener(move |_, _, _, cx| {
                                        copy_acknowledged(copy_text.clone(), cx);
                                    })),
                            )
                            .child(
                                action_button("ai-export-pdf")
                                    .ghost()
                                    .small()
                                    .icon(Glyph::FileText)
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
                                    .icon(Glyph::FileText)
                                    .label("Word")
                                    .accessibility_label("Export Word")
                                    .hint("Export this reviewed answer as Word (.docx)")
                                    .on_click_accessible(cx.listener(move |v, _, window, cx| {
                                        v.export_answer_dialog("docx", word_text.clone(), window, cx);
                                    })),
                            ),
                    ),
            )
            .child(
                div()
                    .id("assistant-answer-region")
                    .role(Role::Document)
                    .aria_label("Assistant answer")
                    .a11y_synthetic_children({
                        let nodes = super::notebook::markdown_accessibility_nodes(text);
                        move |tree| { for (i, node) in nodes.into_iter().enumerate() { let id = tree.synthetic_node_id(i); tree.push_child(id, node); } }
                    })
                    .child(
                        TextView::markdown("assistant-answer", safe_markdown(text))
                            .selectable(true),
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
            answer_card = answer_card.child(
                v_flex()
                    .gap_1()
                    .pt_1p5()
                    .border_t_1()
                    .border_color(divider(cx))
                    .child(cite_row),
            );
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
        Some(answer_card.into_any_element())
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
                .border_l_2()
                .border_color(state_tone)
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
    fn assistant_search_and_conversations(
        &mut self,
        search_input: Entity<InputState>,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let state = &self.tabs[self.active].assistant;
        let busy = state.busy;
        let show_history = state.show_history;
        let transcript = state.transcript.clone();
        let active_conv = state.conversation.clone();
        let theme = cx.theme();
        let (primary, info, mono) = (theme.primary, theme.info, theme.mono_font_family.clone());
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

        if !show_history && search_results.is_empty() && transcript.is_empty() {
            return None;
        }

        let mut col = v_flex().gap_2();

        if show_history || !search_results.is_empty() {
            let search_for_click = search_input.clone();
            let mut search_card = card(cx).p_2().gap_2().child(
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
            if !conversations.is_empty() {
                let mut conv_chips = h_flex().gap_1().flex_wrap().pt_1();
                for (i, row) in conversations.iter().enumerate() {
                    let id = row["id"].as_str().unwrap_or("").to_owned();
                    let is_selected = active_conv.as_deref() == Some(id.as_str());
                    conv_chips = conv_chips.child(
                        action_button(("ai-conversation", i))
                            .ghost()
                            .small()
                            .selected(is_selected)
                            .label(row["title"].as_str().unwrap_or("Conversation").to_owned())
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
                                v.ai("conversation.get", json!({"conversation_id": id}));
                                cx.notify();
                            })),
                    );
                }
                search_card = search_card.child(conv_chips);
            }
            col = col.child(search_card);
        }

        if !transcript.is_empty() {
            let mut transcript_col = v_flex().gap_2();
            for (i, message) in transcript.iter().enumerate() {
                let role = message["role"].as_str().unwrap_or("assistant");
                if role == "assistant"
                    && message["request_id"]
                        == self.tabs[self.active].assistant.answer["request_id"]
                    && !self.tabs[self.active].assistant.answer.is_null()
                {
                    continue;
                }
                let text = message["text"].as_str().unwrap_or("");
                let is_user = role == "user";
                transcript_col = transcript_col.child(
                    v_flex()
                        .p_2p5()
                        .gap_1p5()
                        .rounded(px(8.))
                        .border_1()
                        .border_color(divider(cx))
                        .bg(if is_user {
                            theme.popover
                        } else {
                            theme.background
                        })
                        .child(
                            h_flex()
                                .gap_1p5()
                                .items_center()
                                .when(!is_user, |d| {
                                    d.child(Icon::new(Glyph::Sparkles).small().text_color(primary))
                                })
                                .child(badge(
                                    if is_user { "You" } else { "Assistant" },
                                    if is_user { info } else { primary },
                                )),
                        )
                        .child(
                            TextView::markdown(
                                SharedString::from(format!("ai-message-{i}")),
                                safe_markdown(text),
                            )
                            .selectable(true),
                        ),
                );
            }
            col = col.child(transcript_col);
            for (turn, message) in transcript.iter().enumerate() {
                if message["request_id"] == self.tabs[self.active].assistant.answer["request_id"]
                    && !self.tabs[self.active].assistant.answer.is_null()
                {
                    continue;
                }
                if let Some(citations) = message["citations"].as_array() {
                    let mut links = h_flex().gap_1().flex_wrap();
                    for (index, citation) in citations.iter().enumerate() {
                        let kind = citation["kind"].as_str().unwrap_or("").to_owned();
                        let id = citation["id"].as_str().unwrap_or("").to_owned();
                        links = links.child(
                            action_button(("ai-history-target", turn * 100 + index))
                                .outline()
                                .small()
                                .label(format!(
                                    "Turn {}: @{}:{}",
                                    turn + 1,
                                    kind,
                                    short_title(&id, 24)
                                ))
                                .unavailable(citation["verified"] != true)
                                .on_click_accessible(cx.listener(move |v, _, _, cx| {
                                    v.navigate_citation(&kind, &id, cx)
                                })),
                        );
                    }
                    col = col.child(links);
                }
            }
        }
        Some(col.into_any_element())
    }

    #[inline(never)]
    pub(super) fn assistant_panel(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let input = self.ai_input("Question", "", window, cx);
        let search_input = self.ai_input("Search history query", "", window, cx);

        let header = self.assistant_header(cx);
        let setup = self.assistant_setup_card(cx);
        let health = self.assistant_health_banner(cx);
        let search_conv_sec = self.assistant_search_and_conversations(search_input, cx);
        let receipt_sec = self.assistant_receipt_section(cx);
        let status_sec = self.assistant_status_section(cx);
        let answer_sec = self.assistant_answer_section(cx);
        let proposals_sec = self.assistant_proposals_section(cx);
        let composer_dock = self.assistant_composer_dock(input, cx);

        let bar_bg = cx.theme().sidebar;
        let mut feed = v_flex().gap_2().p_3().w_full().min_w_0();
        if let Some(el) = setup {
            feed = feed.child(el);
        }
        if let Some(el) = health {
            feed = feed.child(el);
        }
        if let Some(el) = search_conv_sec {
            feed = feed.child(el);
        }
        if let Some(el) = receipt_sec {
            feed = feed.child(el);
        }

        if let Some(el) = answer_sec {
            feed = feed.child(el);
        }
        if let Some(el) = proposals_sec {
            feed = feed.child(el);
        }

        v_flex()
            .id("assistant-inspector")
            .debug_selector(|| "assistant-inspector".into())
            .size_full()
            .min_w_0()
            .bg(bar_bg)
            .border_l_1()
            .border_color(divider(cx))
            .child(header)
            .when_some(status_sec, |d, el| d.child(div().px_2().py_1().child(el)))
            .child(
                div()
                    .id("assistant-inspector-scroll")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .child(feed),
            )
            .child(composer_dock)
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
            .child(action_button("ai-open-sidebar").ghost().small().self_start().label("Open Assistant")
                .on_click_accessible(cx.listener(|v, _, _, cx| {v.select_surface("Notebook", cx);v.open_assistant(cx);})))
            .child(action_button("ai-advanced-settings").ghost().small().self_start()
                .label(if advanced_settings { "Hide advanced settings" } else { "Advanced settings" })
                .selected(advanced_settings)
                .on_click_accessible(cx.listener(|v, _, _, cx| {
                    v.tabs[v.active].assistant.advanced_settings = !v.tabs[v.active].assistant.advanced_settings;
                    cx.notify();
                })))
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
        let (muted, success, warning, danger, info, mono) = (
            theme.muted_foreground,
            theme.success,
            theme.warning,
            theme.danger,
            theme.info,
            theme.mono_font_family.clone(),
        );

        let mut policy_track = segmented_track(cx);
        for (pol_key, pol_label) in [
            ("off", "off"),
            ("local_only", "local_only"),
            ("allowed", "allowed"),
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

        settings_group("AI assistant \u{2014} Policy, budgets & privacy", cx)
            .id("settings-ai-policy")
            .child(setting_row(
                "Enterprise policy",
                "Off blocks all AI features. local_only restricts requests to loopback addresses (127.0.0.1, ::1, localhost).",
                h_flex()
                    .gap_2()
                    .items_center()
                    .flex_wrap()
                    .child(badge(
                        current_policy.clone(),
                        match current_policy.as_str() {
                            "off" => danger,
                            "local_only" => warning,
                            _ => success,
                        },
                    ))
                    .child(policy_track),
                cx,
            ))
            .child(setting_row(
                "Token budgets & retention",
                "Daily token cap, max output tokens, context byte budget (1 KB \u{2013} 256 KB), and conversation retention days. Save via the provider button above.",
                v_flex()
                    .gap_2()
                    .child(
                        h_flex()
                            .gap_2()
                            .flex_wrap()
                            .child(div().min_w(px(130.)).flex_1().child(Input::new(&cap)))
                            .child(div().min_w(px(130.)).flex_1().child(Input::new(&output)))
                            .child(
                                div()
                                    .min_w(px(130.))
                                    .flex_1()
                                    .child(Input::new(&context_bytes)),
                            )
                            .child(div().min_w(px(130.)).flex_1().child(Input::new(&retention))),
                    )
                    .child(
                        div().text_xs().font_family(mono).text_color(info).child(format!(
                            "Reserved today: {} tokens (this notebook/provider, UTC; includes failures and tests)",
                            root["usage"]["used_today"].as_u64().map(|v| v.to_string()).unwrap_or_else(|| "Unknown".into())
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
