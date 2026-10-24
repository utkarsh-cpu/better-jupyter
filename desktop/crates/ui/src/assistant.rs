//! Notebook-scoped assistant inspector and explicit context/proposal review.
use super::*;

#[derive(Default)]
pub(super) struct AssistantView {
    pub open: bool,
    pub busy: bool,
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
    pub task_group: String,
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
        self.tabs[self.active].assistant.open = true;
        self.ai("settings.get", json!({}));
        cx.notify();
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
            self.prompts.insert(key.clone(), input);
        }
        self.prompts[&key].clone()
    }

    pub(super) fn ai_preview(&mut self, task: &str, question: String, cx: &mut Context<Self>) {
        let tab = &mut self.tabs[self.active];
        tab.assistant.busy = true;
        tab.assistant.task = task.to_owned();
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
            fields["exec_ids"] = json!([id]);
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
        let settings_root = state.settings.clone();
        let theme = cx.theme();
        let (primary, _muted, success, warning, danger, info) = (
            theme.primary,
            theme.muted_foreground,
            theme.success,
            theme.warning,
            theme.danger,
            theme.info,
        );
        let policy = settings_root["policy"].as_str().unwrap_or("allowed");
        let provider_name = settings_root["settings"]["provider"]
            .as_str()
            .unwrap_or("");
        let model_name = settings_root["settings"]["model"]
            .as_str()
            .unwrap_or("");
        let dest_label = settings_root["destination"]["label"]
            .as_str()
            .unwrap_or("Local");
        let dest_local = settings_root["destination"]["local"]
            .as_bool()
            .unwrap_or(true);

        let mut status_badges = h_flex().gap_1().items_center().flex_wrap();
        if policy == "off" {
            status_badges = status_badges.child(badge("Policy: Off", danger));
        } else if provider_name.is_empty() || model_name.is_empty() {
            status_badges = status_badges.child(badge("Unconfigured", warning));
        } else {
            status_badges = status_badges.child(badge(
                dest_label.to_string(),
                if dest_local { success } else { info },
            ));
            if policy == "local_only" {
                status_badges = status_badges.child(badge("Local only", warning));
            }
        }

        h_flex()
            .px_3()
            .py_2()
            .justify_between()
            .items_center()
            .gap_2()
            .flex_wrap()
            .bg(theme.title_bar)
            .border_b_1()
            .border_color(divider(cx))
            .child(
                h_flex()
                    .gap_2()
                    .items_center()
                    .min_w_0()
                    .child(Icon::new(Glyph::Sparkles).small().text_color(primary))
                    .child(
                        div()
                            .text_sm()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child("Assistant"),
                    )
                    .child(status_badges),
            )
            .child(
                h_flex()
                    .gap_1()
                    .items_center()
                    .flex_wrap()
                    .child(
                        action_button("assistant-new")
                            .ghost()
                            .small()
                            .icon(Glyph::Plus)
                            .label("New")
                            .unavailable(busy)
                            .on_click(cx.listener(|v, _, _, cx| {
                                let ai = &mut v.tabs[v.active].assistant;
                                ai.conversation = None;
                                ai.transcript.clear();
                                ai.answer = Value::Null;
                                ai.receipt = Value::Null;
                                ai.proposals.clear();
                                ai.status.clear();
                                cx.notify();
                            })),
                    )
                    .child(
                        action_button("assistant-history")
                            .ghost()
                            .small()
                            .icon(Glyph::Clock)
                            .label("Conversations")
                            .unavailable(busy)
                            .on_click(cx.listener(|v, _, _, cx| {
                                v.ai("conversations.list", json!({}));
                                cx.notify();
                            })),
                    )
                    .child(
                        action_button("assistant-proposals-btn")
                            .ghost()
                            .small()
                            .icon(Glyph::FileText)
                            .label("Proposals")
                            .unavailable(busy)
                            .on_click(cx.listener(|v, _, _, cx| {
                                v.ai("proposals.list", json!({}));
                                cx.notify();
                            })),
                    )
                    .child(
                        action_button("assistant-config")
                            .ghost()
                            .small()
                            .icon(Glyph::Settings)
                            .label("Settings \u{2192} AI")
                            .on_click(cx.listener(|v, _, _, cx| {
                                v.ai("settings.get", json!({}));
                                v.select_surface("Settings", cx);
                            })),
                    )
                    .child(
                        action_button("assistant-close")
                            .ghost()
                            .small()
                            .icon(Glyph::X)
                            .label("Close")
                            .on_click(cx.listener(|v, _, _, cx| {
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
            card(cx)
                .p_2p5()
                .gap_2()
                .border_color(warning.opacity(0.4))
                .bg(warning.opacity(0.06))
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
                                .child(dot(warning))
                                .child(
                                    div()
                                        .text_xs()
                                        .font_weight(FontWeight::SEMIBOLD)
                                        .child("Notebook ledger signals"),
                                ),
                        )
                        .child(
                            h_flex()
                                .gap_1p5()
                                .flex_wrap()
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
                        ),
                )
                .child(
                    h_flex()
                        .gap_2()
                        .flex_wrap()
                        .when(stale_count > 0, |d| {
                            d.child(
                                action_button("ai-banner-stale")
                                    .outline()
                                    .small()
                                    .label("Review stale cells")
                                    .unavailable(busy)
                                    .on_click(cx.listener(|v, _, _, cx| {
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
                                    .outline()
                                    .small()
                                    .label("Check reproducibility")
                                    .unavailable(busy)
                                    .on_click(cx.listener(|v, _, _, cx| {
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
    fn assistant_context_section(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let busy = self.tabs[self.active].assistant.busy;
        let selected_cells = self.tabs[self.active].assistant.selected_cells.clone();
        let exec_ids = self.tabs[self.active].assistant.exec_ids.clone();
        let checkpoint_ids = self.tabs[self.active].assistant.checkpoint_ids.clone();
        let active_cell = self.tabs[self.active].model.selected.clone();
        let theme = cx.theme();
        let (muted, primary, info, mono) = (
            theme.muted_foreground,
            theme.primary,
            theme.info,
            theme.mono_font_family.clone(),
        );

        let mut cell_chips = h_flex().gap_1().flex_wrap().items_center();
        for id in self.tabs[self.active].model.order.clone() {
            let selected = selected_cells.contains(&id);
            let cell_id = id.clone();
            let mut btn = action_button(SharedString::from(format!("ai-cell-{id}")))
                .small()
                .label(format!("@{id}"))
                .selected(selected)
                .unavailable(busy);
            if selected {
                btn = btn.outline();
            } else {
                btn = btn.ghost();
            }
            cell_chips = cell_chips.child(btn.on_click(cx.listener(move |v, _, _, cx| {
                let cells = &mut v.tabs[v.active].assistant.selected_cells;
                if cells.contains(&cell_id) {
                    cells.retain(|c| c != &cell_id);
                } else {
                    cells.push(cell_id.clone());
                }
                v.tabs[v.active].assistant.receipt = Value::Null;
                cx.notify();
            })));
        }

        let mut context_strip = v_flex()
            .gap_1p5()
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
                            .child(Icon::new(Glyph::Code).small().text_color(muted))
                            .child(
                                div()
                                    .text_xs()
                                    .font_family(mono)
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(muted)
                                    .child("@Context"),
                            )
                            .child(if selected_cells.is_empty() {
                                badge("All cells (budgeted)", info)
                            } else {
                                badge(format!("{} selected", selected_cells.len()), primary)
                            }),
                    )
                    .child(
                        h_flex()
                            .gap_1()
                            .items_center()
                            .when_some(
                                active_cell.filter(|id| !selected_cells.contains(id)),
                                |d, id| {
                                    d.child(
                                        action_button("ai-focus-cell")
                                            .ghost()
                                            .small()
                                            .label("Use active cell")
                                            .unavailable(busy)
                                            .on_click(cx.listener(move |v, _, _, cx| {
                                                v.tabs[v.active].assistant.selected_cells =
                                                    vec![id.clone()];
                                                v.tabs[v.active].assistant.receipt = Value::Null;
                                                cx.notify();
                                            })),
                                    )
                                },
                            )
                            .when(
                                !selected_cells.is_empty()
                                    || !exec_ids.is_empty()
                                    || !checkpoint_ids.is_empty(),
                                |d| {
                                    d.child(
                                        action_button("ai-clear-context")
                                            .ghost()
                                            .small()
                                            .label("Clear")
                                            .unavailable(busy)
                                            .on_click(cx.listener(|v, _, _, cx| {
                                                let ai = &mut v.tabs[v.active].assistant;
                                                ai.selected_cells.clear();
                                                ai.exec_ids.clear();
                                                ai.checkpoint_ids.clear();
                                                ai.receipt = Value::Null;
                                                cx.notify();
                                            })),
                                    )
                                },
                            ),
                    ),
            )
            .child(cell_chips);

        if !exec_ids.is_empty() || !checkpoint_ids.is_empty() {
            let mut record_chips = h_flex().gap_1().flex_wrap();
            for (kind, ids) in [
                ("execution", exec_ids.clone()),
                ("checkpoint", checkpoint_ids.clone()),
            ] {
                for (index, id) in ids.iter().enumerate() {
                    let identity = id.clone();
                    record_chips = record_chips.child(
                        action_button(SharedString::from(format!("ai-remove-{kind}-{index}")))
                            .outline()
                            .small()
                            .icon(if kind == "execution" {
                                Glyph::Clock
                            } else {
                                Glyph::Flag
                            })
                            .label(format!("@{kind}:{id} \u{00d7}"))
                            .on_click(cx.listener(move |v, _, _, cx| {
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
            context_strip = context_strip.child(record_chips);
        }
        if exec_ids.len() == 2 {
            context_strip = context_strip.child(
                action_button("ai-compare")
                    .primary()
                    .small()
                    .icon(Glyph::Clock)
                    .label("Compare selected executions")
                    .unavailable(busy)
                    .on_click(cx.listener(|v, _, _, cx| {
                        v.ai_preview(
                            "compare",
                            "Compare these recorded runs. Separate observed changes from suggested causes.".into(),
                            cx,
                        )
                    })),
            );
        }
        context_strip.into_any_element()
    }

    #[inline(never)]
    fn assistant_workflows_section(
        &mut self,
        input: Entity<InputState>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let state = &self.tabs[self.active].assistant;
        let busy = state.busy;
        let active_group = if state.task_group.is_empty() {
            "core".to_string()
        } else {
            state.task_group.clone()
        };
        let theme = cx.theme();
        let (muted, mono) = (theme.muted_foreground, theme.mono_font_family.clone());
        let view_for_explain = cx.entity();
        let view_for_summary = cx.entity();
        let view_for_repro = cx.entity();

        let mut group_tabs = segmented_track(cx);
        for (group_id, group_label) in [
            ("core", "Cell & Code"),
            ("ledger", "Ledger & Runtime"),
            ("analysis", "Analysis & Reports"),
        ] {
            let selected = active_group == group_id;
            let gid = group_id.to_owned();
            group_tabs = group_tabs.child(
                action_button(SharedString::from(format!("ai-group-{group_id}")))
                    .small()
                    .label(group_label)
                    .segment(selected)
                    .on_click(cx.listener(move |v, _, _, cx| {
                        v.tabs[v.active].assistant.task_group = gid.clone();
                        cx.notify();
                    })),
            );
        }

        let task_presets: &[(&str, &str, &str)] = match active_group.as_str() {
            "ledger" => &[
                (
                    "compare",
                    "/compare",
                    "Compare recorded executions and highlight changed source, inputs, durations, and outputs.",
                ),
                (
                    "checkpoint",
                    "/checkpoint",
                    "Explain saved checkpoint coverage, excluded values, and whether to Restore or Continue.",
                ),
                (
                    "stale",
                    "/stale",
                    "Identify cells edited since their last run and recommend the minimal safe rerun order.",
                ),
                (
                    "environment",
                    "/environment",
                    "Review interpreter, package, and GPU diagnostics. Format shell commands as copy-only proposals.",
                ),
                (
                    "training",
                    "/training",
                    "Inspect portable training checkpoints, step cadence, and safe resume options.",
                ),
            ],
            "analysis" => &[
                (
                    "explore",
                    "/explore",
                    "Propose a read-only data profiling cell from recorded schemas and table previews.",
                ),
                (
                    "anomaly",
                    "/anomaly",
                    "Analyze recorded execution durations and outputs for regressions or anomalies.",
                ),
                (
                    "report",
                    "/report",
                    "Draft an evidence-backed experiment report citing cells, executions, and checkpoints.",
                ),
            ],
            _ => &[
                (
                    "edit",
                    "/edit",
                    "Propose a revision-checked edit to improve this cell.",
                ),
                (
                    "refactor",
                    "/refactor",
                    "Refactor the selected cell for clarity and reproducibility without changing behavior.",
                ),
                (
                    "document",
                    "/document",
                    "Add concise docstrings or explanatory Markdown for the selected cell.",
                ),
                (
                    "tests",
                    "/tests",
                    "Propose assertion or unit-test cells verifying the selected code cell.",
                ),
                (
                    "starter",
                    "/starter",
                    "Propose a structured sequence of starter cells for this notebook.",
                ),
            ],
        };

        let mut preset_buttons = h_flex().gap_1().flex_wrap();
        for &(task_key, label, default_prompt) in task_presets {
            let question_input = input.clone();
            preset_buttons = preset_buttons.child(
                action_button(SharedString::from(format!("ai-task-{task_key}")))
                    .outline()
                    .small()
                    .label(label)
                    .unavailable(busy)
                    .on_click(cx.listener(move |v, _, _, cx| {
                        let typed = question_input.read(cx).value().trim().to_string();
                        let prompt = if typed.is_empty() {
                            default_prompt.to_owned()
                        } else {
                            typed
                        };
                        v.ai_preview(task_key, prompt, cx);
                    })),
            );
        }

        card(cx)
            .p_2p5()
            .gap_2()
            .child(
                h_flex()
                    .justify_between()
                    .items_center()
                    .gap_2()
                    .flex_wrap()
                    .child(
                        div()
                            .text_xs()
                            .font_family(mono)
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(muted)
                            .child("AGENT COMMANDS"),
                    )
                    .child(
                        h_flex()
                            .gap_1()
                            .flex_wrap()
                            .child(
                                action_button("assistant-summary")
                                    .outline()
                                    .small()
                                    .icon(Glyph::Clock)
                                    .label("Resume summary")
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
                                    .on_click(cx.listener(|v, _, _, cx| {
                                        v.ai_preview(
                                            "summary",
                                            "Summarize this notebook's recorded state and what I can continue from. Cite every recorded fact.".into(),
                                            cx,
                                        );
                                    })),
                            )
                            .child(
                                action_button("assistant-explain")
                                    .outline()
                                    .small()
                                    .icon(Glyph::CircleAlert)
                                    .label("Explain error")
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
                                    .on_click(cx.listener(|v, _, _, cx| {
                                        v.ai_preview(
                                            "explain",
                                            "Explain this cell's recorded error and propose a fix.".into(),
                                            cx,
                                        );
                                    })),
                            )
                            .child(
                                action_button("assistant-repro")
                                    .outline()
                                    .small()
                                    .icon(Glyph::Flag)
                                    .label("Reproducibility")
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
                                    .on_click(cx.listener(|v, _, _, cx| {
                                        v.ai_preview(
                                            "reproducibility",
                                            "Review reproducibility from recorded evidence.".into(),
                                            cx,
                                        );
                                    })),
                            ),
                    ),
            )
            .child(
                v_flex()
                    .gap_1p5()
                    .pt_1p5()
                    .border_t_1()
                    .border_color(divider(cx))
                    .child(group_tabs)
                    .child(preset_buttons),
            )
            .into_any_element()
    }

    #[inline(never)]
    fn assistant_composer_dock(
        &mut self,
        input: Entity<InputState>,
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
        let settings_root = state.settings.clone();
        let provider_name = settings_root["settings"]["provider"]
            .as_str()
            .unwrap_or("");
        let model_name = settings_root["settings"]["model"]
            .as_str()
            .unwrap_or("unconfigured");
        let dest_local = settings_root["destination"]["local"]
            .as_bool()
            .unwrap_or(true);
        let used_today = settings_root["usage"]["used_today"].as_u64().unwrap_or(0);
        let daily_cap = settings_root["usage"]["daily_token_cap"]
            .as_u64()
            .or_else(|| settings_root["settings"]["daily_token_cap"].as_u64())
            .unwrap_or(100_000);

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
            .p_3()
            .gap_2()
            .bg(theme.title_bar)
            .border_t_1()
            .border_color(divider(cx))
            .child(context_sec)
            .child(
                v_flex()
                    .p_2()
                    .gap_2()
                    .rounded(px(8.))
                    .border_1()
                    .border_color(divider(cx))
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
                                    .flex_wrap()
                                    .child(badge(format!("Agent \u{00b7} {active_task}"), primary))
                                    .child(dot(if dest_local { success } else { info }))
                                    .child(
                                        div()
                                            .text_xs()
                                            .font_family(mono.clone())
                                            .text_color(muted)
                                            .child(model_name.to_string()),
                                    )
                                    .when(!provider_name.is_empty(), |d| {
                                        d.child(
                                            div()
                                                .text_xs()
                                                .font_family(mono)
                                                .text_color(muted)
                                                .child(format!("{used_today}/{daily_cap} tok")),
                                        )
                                    }),
                            )
                            .child(
                                action_button("assistant-preview")
                                    .primary()
                                    .small()
                                    .icon(Glyph::Search)
                                    .label("Review context")
                                    .unavailable(busy)
                                    .on_a11y_click(move |_, cx| {
                                        let question = input_for_a11y.read(cx).value().to_string();
                                        view_for_preview.update(cx, |v, cx| {
                                            v.ai_preview("ask", question, cx);
                                        });
                                    })
                                    .on_click(cx.listener(move |v, _, _, cx| {
                                        let question = input_for_click.read(cx).value().to_string();
                                        v.ai_preview("ask", question, cx);
                                    })),
                            ),
                    ),
            )
            .into_any_element()
    }

    #[inline(never)]
    fn assistant_status_section(&mut self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let state = &self.tabs[self.active].assistant;
        let busy = state.busy;
        let status_text = state.status.clone();
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
        let is_err = status_text.contains("error")
            || status_text.contains("Error")
            || status_text.contains("disabled")
            || status_text.contains("failed");
        let status_color = if is_err {
            danger
        } else if busy {
            info
        } else {
            success
        };
        let mut col = v_flex().gap_2p5().child(
            card(cx)
                .id("assistant-status-card")
                .role(Role::Status)
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
                                        .child(status_text),
                                ),
                        )
                        .when(busy, |d| {
                            d.child(
                                action_button("assistant-cancel")
                                    .outline()
                                    .small()
                                    .icon(Glyph::Square)
                                    .label("Cancel request")
                                    .on_click(cx.listener(|v, _, _, cx| {
                                        v.send(Command::AssistantCancel);
                                        let ai = &mut v.tabs[v.active].assistant;
                                        ai.busy = false;
                                        ai.receipt = Value::Null;
                                        ai.status =
                                            "Cancelled; prepare fresh context to continue.".into();
                                        cx.notify();
                                    })),
                            )
                        }),
                ),
        );
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
                                    .child("Evidence verification pending"),
                            ),
                    )
                    .child(TextView::markdown(
                        "ai-stream-draft",
                        safe_markdown(&draft),
                    )),
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

        let mut receipt_card = card(cx)
            .p_3()
            .gap_2p5()
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
                                    .child("CONTEXT RECEIPT"),
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
                        "{} bytes \u{00b7} ~{} input tokens \u{00b7} #{}",
                        receipt["bytes"],
                        receipt["token_estimate"],
                        &r_hash[..r_hash.len().min(12)]
                    )),
            );

        if let Some(items) = receipt["items"].as_array() {
            let mut items_col = v_flex().gap_1p5();
            for (index, item) in items.iter().enumerate() {
                let text = item["text"].as_str().unwrap_or("").to_owned();
                let kind = item["kind"].as_str().unwrap_or("item");
                let id = item["id"].as_str().unwrap_or("");
                let redacted = item["redacted"] == true;
                let truncated = item["truncated"] == true;
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
                                        .child(
                                            div()
                                                .text_xs()
                                                .font_family(mono.clone())
                                                .font_weight(FontWeight::MEDIUM)
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
        let a11y_hash = r_hash.clone();
        let click_hash = r_hash;
        let view = cx.entity();
        let approval_url = if r_base_url.is_empty() {
            r_host.to_owned()
        } else {
            r_base_url.to_owned()
        };

        receipt_card = receipt_card.child(
            v_flex()
                .gap_2()
                .pt_2()
                .border_t_1()
                .border_color(divider(cx))
                .child(
                    action_button("assistant-send")
                        .primary()
                        .label("Approve context and send")
                        .unavailable(busy)
                        .on_a11y_click(move |_, cx| {
                            view.update(cx, |v, cx| v.ai_send(a11y_hash.clone(), cx));
                        })
                        .on_click(cx.listener(move |v, _, _, cx| {
                            v.ai_send(click_hash.clone(), cx);
                        })),
                )
                .child(
                    h_flex()
                        .gap_1p5()
                        .flex_wrap()
                        .child(
                            action_button("ai-receipt-copy")
                                .ghost()
                                .small()
                                .label("Copy full receipt and system prompt")
                                .on_click(cx.listener(move |_, _, _, cx| {
                                    copy_acknowledged(full.clone(), cx);
                                })),
                        )
                        .when(!approval_url.is_empty(), |d| {
                            d.child(
                                action_button("ai-receipt-remember")
                                    .ghost()
                                    .small()
                                    .label(if auto_approved {
                                        "Revoke destination auto-approval"
                                    } else {
                                        "Always allow for this notebook & destination"
                                    })
                                    .on_click(cx.listener(move |v, _, _, cx| {
                                        let root = v.tabs[v.active].assistant.settings
                                            ["notebook_root"]
                                            .as_str()
                                            .map(str::to_owned)
                                            .unwrap_or_else(|| {
                                                v.tabs[v.active]
                                                    .model
                                                    .project
                                                    .display()
                                                    .to_string()
                                            });
                                        let key = format!("{root}::{approval_url}");
                                        let mut config = v.tabs[v.active].assistant.settings
                                            ["settings"]
                                            .clone();
                                        if !config.is_object() {
                                            config = json!({});
                                        }
                                        if !config["approvals"].is_object() {
                                            config["approvals"] = json!({});
                                        }
                                        if auto_approved {
                                            if let Some(map) = config["approvals"].as_object_mut()
                                            {
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
                ),
        );
        Some(receipt_card.into_any_element())
    }

    #[inline(never)]
    fn assistant_answer_section(
        &mut self,
        export_path: Entity<InputState>,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let answer = self.tabs[self.active].assistant.answer.clone();
        let text = answer["answer"].as_str()?;
        let theme = cx.theme();
        let (primary, muted, success, mono) = (
            theme.primary,
            theme.muted_foreground,
            theme.success,
            theme.mono_font_family.clone(),
        );
        let mut answer_card = card(cx)
            .p_3()
            .gap_2p5()
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
                            .child(badge("Evidence-checked", success))
                            .when_some(
                                answer["usage"]["total_tokens"].as_u64(),
                                |d, tokens| {
                                    d.child(
                                        div()
                                            .text_xs()
                                            .font_family(mono)
                                            .text_color(muted)
                                            .child(format!("{tokens} tokens")),
                                    )
                                },
                            ),
                    )
                    .child({
                        let copy_text = text.to_owned();
                        action_button("ai-copy-answer")
                            .ghost()
                            .small()
                            .label("Copy answer")
                            .on_click(cx.listener(move |_, _, _, cx| {
                                copy_acknowledged(copy_text.clone(), cx);
                            }))
                    }),
            )
            .child(
                div()
                    .id("assistant-answer-region")
                    .role(Role::Group)
                    .aria_label(format!("Assistant answer: {text}"))
                    .child(
                        TextView::markdown("assistant-answer", safe_markdown(text))
                            .selectable(true),
                    ),
            )
            .child(
                div()
                    .text_xs()
                    .text_color(muted)
                    .child("Citation targets verified where marked. Uncited claims and suggested causes are unverified."),
            );

        if let Some(citations) = answer["citations"].as_array()
            && !citations.is_empty()
        {
            let mut cite_row = h_flex().gap_1p5().flex_wrap();
            for (i, citation) in citations.iter().enumerate() {
                let verified = citation["verified"] == true;
                let kind = citation["kind"].as_str().unwrap_or("record").to_owned();
                let id = citation["id"].as_str().unwrap_or("").to_owned();
                let label = format!(
                    "@{kind}:{id}{}",
                    if verified { "" } else { " (unverified)" }
                );
                let click_kind = kind.clone();
                let click_id = id.clone();
                let a11y_kind = kind;
                let a11y_id = id;
                let view = cx.entity();
                cite_row = cite_row.child(
                    action_button(("ai-citation", i))
                        .outline()
                        .small()
                        .label(label)
                        .unavailable(!verified)
                        .on_a11y_click(move |_, cx| {
                            view.update(cx, |v, cx| {
                                v.navigate_citation(&a11y_kind, &a11y_id, cx);
                            });
                        })
                        .on_click(cx.listener(move |v, _, _, cx| {
                            v.navigate_citation(&click_kind, &click_id, cx);
                        })),
                );
            }
            answer_card = answer_card.child(
                v_flex()
                    .gap_1p5()
                    .pt_2()
                    .border_t_1()
                    .border_color(divider(cx))
                    .child(
                        div()
                            .text_xs()
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(muted)
                            .child("Referenced ledger evidence"),
                    )
                    .child(cite_row),
            );
        }

        let pdf_path = export_path.clone();
        let docx_path = export_path.clone();
        answer_card = answer_card.child(
            v_flex()
                .gap_2()
                .pt_2()
                .border_t_1()
                .border_color(divider(cx))
                .child(
                    div()
                        .text_xs()
                        .text_color(muted)
                        .child("Export notebook report (PDF / Word)"),
                )
                .child(Input::new(&export_path).small().cleanable(true))
                .child(
                    h_flex()
                        .gap_2()
                        .flex_wrap()
                        .child(
                            action_button("ai-export-pdf")
                                .outline()
                                .small()
                                .icon(Glyph::FileText)
                                .label("Export PDF")
                                .on_click(cx.listener(move |v, _, window, cx| {
                                    let p = pdf_path.read(cx).value().trim().to_string();
                                    if p.is_empty() {
                                        v.export_dialog("pdf", window, cx);
                                    } else {
                                        v.runtime(
                                            "export_document",
                                            json!({"path": p, "format": "pdf"}),
                                        );
                                    }
                                })),
                        )
                        .child(
                            action_button("ai-export-docx")
                                .outline()
                                .small()
                                .icon(Glyph::FileText)
                                .label("Export Word")
                                .on_click(cx.listener(move |v, _, window, cx| {
                                    let p = docx_path.read(cx).value().trim().to_string();
                                    if p.is_empty() {
                                        v.export_dialog("docx", window, cx);
                                    } else {
                                        v.runtime(
                                            "export_document",
                                            json!({"path": p, "format": "docx"}),
                                        );
                                    }
                                })),
                        ),
                ),
        );
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
                .unwrap_or_else(|| serde_json::to_string_pretty(&payload["cells"]).unwrap_or_default());

            let state_tone = match p_state {
                "accepted" => success,
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
                                .child(Icon::new(Glyph::Code).small().text_color(primary))
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
                        .child(badge(p_state.to_string(), state_tone)),
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
                            .child("Source or cell order changed since this proposal was generated. Click Regenerate to prepare a fresh proposal against current source."),
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
                        .on_click(cx.listener(move |_, _, _, cx| {
                            copy_acknowledged(command.clone(), cx)
                        })),
                );
            } else {
                action_row = action_row.child(
                    action_button(("ai-apply", index))
                        .primary()
                        .small()
                        .label("Apply reviewed proposal")
                        .unavailable(busy || p_state != "proposed")
                        .on_click(cx.listener(move |v, _, _, cx| {
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
                        .on_click(cx.listener(move |v, _, _, cx| {
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
                        .on_click(cx.listener(move |v, _, _, cx| {
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
    ) -> AnyElement {
        let busy = self.tabs[self.active].assistant.busy;
        let theme = cx.theme();
        let (primary, info, mono) = (
            theme.primary,
            theme.info,
            theme.mono_font_family.clone(),
        );
        let search_results = self.tabs[self.active]
            .inspections
            .get("AI search")
            .and_then(|v| v["items"].as_array())
            .cloned()
            .unwrap_or_default();
        let search_for_click = search_input.clone();
        let mut search_card = card(cx)
            .p_2p5()
            .gap_2()
            .child(
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
                            .label("Search history")
                            .unavailable(busy)
                            .on_click(cx.listener(move |v, _, _, cx| {
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
            for (i, hit) in search_results.iter().take(8).enumerate() {
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
                                        .on_click(cx.listener(move |v, _, _, cx| {
                                            v.navigate_citation("execution", &inspect_exec, cx);
                                        })),
                                )
                                .child(
                                    action_button(("ai-search-add", i))
                                        .outline()
                                        .small()
                                        .label("Add to context")
                                        .on_click(cx.listener(move |v, _, _, cx| {
                                            let ai = &mut v.tabs[v.active].assistant;
                                            if !ai.exec_ids.contains(&add_exec) {
                                                if ai.exec_ids.len() == 2 {
                                                    ai.exec_ids.remove(0);
                                                }
                                                ai.exec_ids.push(add_exec.clone());
                                            }
                                            ai.receipt = Value::Null;
                                            cx.notify();
                                        })),
                                ),
                        ),
                );
            }
            search_card = search_card.child(results_col);
        }

        let mut col = v_flex().gap_2p5().child(search_card);

        if let Some(rows) = self.tabs[self.active]
            .inspections
            .get("AI conversations")
            .and_then(|v| v["items"].as_array())
            .cloned()
            && !rows.is_empty()
        {
            let active_conv = self.tabs[self.active].assistant.conversation.clone();
            let mut conv_chips = h_flex().gap_1().flex_wrap();
            for (i, row) in rows.iter().enumerate() {
                let id = row["id"].as_str().unwrap_or("").to_owned();
                let is_selected = active_conv.as_deref() == Some(id.as_str());
                conv_chips = conv_chips.child(
                    action_button(("ai-conversation", i))
                        .ghost()
                        .small()
                        .selected(is_selected)
                        .label(row["title"].as_str().unwrap_or("Conversation").to_owned())
                        .on_click(cx.listener(move |v, _, _, cx| {
                            v.tabs[v.active].assistant.conversation = Some(id.clone());
                            v.ai("conversation.get", json!({"conversation_id": id}));
                            cx.notify();
                        })),
                );
            }
            col = col.child(card(cx).p_2p5().gap_1p5().child(conv_chips));
        }

        let transcript = self.tabs[self.active].assistant.transcript.clone();
        if !transcript.is_empty() {
            let mut transcript_col = v_flex().gap_2();
            for (i, message) in transcript.iter().enumerate() {
                let role = message["role"].as_str().unwrap_or("assistant");
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
        }
        col.into_any_element()
    }

    pub(super) fn assistant_panel(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let input = self.ai_input("Question", "", window, cx);
        let search_input = self.ai_input("Search history query", "", window, cx);
        let export_path = self.ai_input("Report export path", "", window, cx);

        let header = self.assistant_header(cx);
        let health = self.assistant_health_banner(cx);
        let search_conv_sec = self.assistant_search_and_conversations(search_input, cx);
        let workflows_sec = self.assistant_workflows_section(input.clone(), cx);
        let receipt_sec = self.assistant_receipt_section(cx);
        let status_sec = self.assistant_status_section(cx);
        let answer_sec = self.assistant_answer_section(export_path, cx);
        let proposals_sec = self.assistant_proposals_section(cx);
        let composer_dock = self.assistant_composer_dock(input, cx);

        let bar_bg = cx.theme().sidebar;
        let mut feed = v_flex().gap_3().p_3().w_full().min_w_0();
        if let Some(el) = health {
            feed = feed.child(el);
        }
        feed = feed.child(search_conv_sec).child(workflows_sec);
        if let Some(el) = receipt_sec {
            feed = feed.child(el);
        }
        if let Some(el) = status_sec {
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
            .size_full()
            .min_w_0()
            .bg(bar_bg)
            .border_l_1()
            .border_color(divider(cx))
            .child(header)
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
        let provider = settings["provider"]
            .as_str()
            .unwrap_or("openai-compatible")
            .to_owned();

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
            &settings["retention_days"].as_u64().unwrap_or(90).to_string(),
            window,
            cx,
        );

        let theme = cx.theme();
        let (muted, success, info) = (theme.muted_foreground, theme.success, theme.info);

        let mut providers = h_flex().gap_2().flex_wrap();
        for (prov_key, label) in [
            ("openai-compatible", "Save local / compatible provider"),
            ("openai", "Save OpenAI provider"),
        ] {
            let model = model.clone();
            let endpoint = endpoint.clone();
            let coding_model = coding_model.clone();
            let cap = cap.clone();
            let output = output.clone();
            let context_bytes = context_bytes.clone();
            let retention = retention.clone();
            let selected = settings["provider"].as_str() == Some(prov_key);
            let mut btn = action_button(SharedString::from(format!("ai-provider-{prov_key}")))
                .small()
                .label(label)
                .selected(selected);
            if selected {
                btn = btn.primary();
            } else {
                btn = btn.outline();
            }
            providers = providers.child(btn.on_click(cx.listener(move |v, _, _, cx| {
                let mut config = v.tabs[v.active].assistant.settings["settings"].clone();
                if !config.is_object() {
                    config = json!({});
                }
                config["provider"] = json!(prov_key);
                config["model"] = json!(model.read(cx).value().to_string());
                if !config["providers"].is_object() {
                    config["providers"] = json!({});
                }
                config["providers"][prov_key] =
                    json!({"base_url": endpoint.read(cx).value().to_string()});
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
            })));
        }

        let mut endpoint_presets = h_flex().gap_1p5().flex_wrap();
        for (preset_label, preset_url) in [
            ("Ollama (:11434)", "http://127.0.0.1:11434/v1"),
            ("LM Studio (:1234)", "http://127.0.0.1:1234/v1"),
            ("vLLM (:8000)", "http://127.0.0.1:8000/v1"),
            ("OpenAI API", "https://api.openai.com/v1"),
        ] {
            let endpoint_input = endpoint.clone();
            endpoint_presets = endpoint_presets.child(
                action_button(SharedString::from(format!("ai-preset-{preset_label}")))
                    .ghost()
                    .small()
                    .label(preset_label)
                    .on_click(cx.listener(move |_, _, window, cx| {
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
            if let Some(m_name) = m.as_str().map(str::to_owned) {
                let model_input = model.clone();
                model_chips = model_chips.child(
                    action_button(("ai-picked-model", i))
                        .outline()
                        .small()
                        .label(m_name.clone())
                        .on_click(cx.listener(move |_, _, window, cx| {
                            model_input.update(cx, |s, cx| s.set_value(m_name.clone(), window, cx));
                        })),
                );
            }
        }

        settings_group("AI assistant \u{2014} Provider & connection", cx)
            .id("settings-ai-provider")
            .child(
                div()
                    .text_sm()
                    .text_color(muted)
                    .child("Off until configured. Credentials use the OS store. Cloud requests require an explicit context receipt."),
            )
            .child(
                h_flex()
                    .gap_2()
                    .flex_wrap()
                    .items_center()
                    .child(
                        action_button("ai-load-settings")
                            .outline()
                            .small()
                            .label("Load AI settings")
                            .on_click(cx.listener(|v, _, _, cx| {
                                v.ai("settings.get", json!({}));
                                cx.notify();
                            })),
                    )
                    .child(
                        action_button("ai-test")
                            .primary()
                            .small()
                            .label("Test connection")
                            .on_click(cx.listener(|v, _, _, cx| {
                                v.ai("providers.test", json!({}));
                                v.tabs[v.active].assistant.status =
                                    "Testing provider (no notebook context)\u{2026}".into();
                                cx.notify();
                            })),
                    )
                    .child(
                        action_button("ai-models")
                            .outline()
                            .small()
                            .label("List available models")
                            .on_click(cx.listener(|v, _, _, cx| {
                                v.ai("models.list", json!({}));
                                cx.notify();
                            })),
                    )
                    .when_some(
                        root["destination"]["label"].as_str(),
                        |d, dest_label| {
                            let is_local = root["destination"]["local"]
                                .as_bool()
                                .unwrap_or(true);
                            d.child(badge(
                                format!(
                                    "{dest_label}: {}",
                                    root["destination"]["host"]
                                        .as_str()
                                        .unwrap_or("unconfigured")
                                ),
                                if is_local { success } else { info },
                            ))
                        },
                    ),
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
            .child(setting_row(
                "Base URL",
                "Local server (Ollama, LM Studio, vLLM, llama.cpp) or OpenAI API endpoint.",
                v_flex()
                    .gap_2()
                    .child(Input::new(&endpoint))
                    .child(endpoint_presets),
                cx,
            ))
            .child(setting_row(
                "Coding task model override (optional)",
                "Optional specialized model for edit, refactor, and test generation tasks.",
                Input::new(&coding_model),
                cx,
            ))
            .child(setting_row(
                "Save provider & model configuration",
                "Select whether this endpoint is an OpenAI-compatible server or OpenAI API.",
                providers,
                cx,
            ))
            .child(setting_row(
                "API key (stored securely; never saved with notebook data)",
                "Stored in the OS credential store and redacted from all logs and receipts.",
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
                                    .on_click(cx.listener(move |v, _, window, cx| {
                                        let provider = v.tabs[v.active].assistant.settings
                                            ["settings"]["provider"]
                                            .as_str()
                                            .unwrap_or("openai")
                                            .to_owned();
                                        let secret = key.read(cx).value().to_string();
                                        key.update(cx, |s, cx| s.set_value("", window, cx));
                                        v.ai(
                                            "credential.set",
                                            json!({"provider": provider, "credential": secret}),
                                        );
                                        cx.notify();
                                    })),
                            )
                            .child(
                                action_button("ai-delete-key")
                                    .ghost()
                                    .small()
                                    .label("Remove API key")
                                    .on_click(cx.listener(|v, _, _, cx| {
                                        let provider = v.tabs[v.active].assistant.settings
                                            ["settings"]["provider"]
                                            .as_str()
                                            .unwrap_or("openai")
                                            .to_owned();
                                        v.ai("credential.delete", json!({"provider": provider}));
                                        cx.notify();
                                    })),
                            ),
                    ),
                cx,
            ))
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
            &settings["retention_days"].as_u64().unwrap_or(90).to_string(),
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
                    .on_click(cx.listener(move |v, _, _, cx| {
                        v.ai("policy.update", json!({"policy": pol_key}));
                        cx.notify();
                    })),
            );
        }

        let approvals = settings["approvals"].as_object().cloned().unwrap_or_default();
        let mut approvals_box = v_flex().gap_1p5();
        if approvals.is_empty() {
            approvals_box = approvals_box.child(
                div()
                    .text_xs()
                    .text_color(muted)
                    .child("No auto-approved destinations saved. Every remote request requires explicit context receipt approval."),
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
                                .truncate()
                                .child(key_name.clone()),
                        )
                        .child(
                            action_button(("ai-revoke-approval", idx))
                                .ghost()
                                .small()
                                .label("Revoke")
                                .on_click(cx.listener(move |v, _, _, cx| {
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

        let used_today = root["usage"]["used_today"].as_u64().unwrap_or(0);
        let cap_val = settings["daily_token_cap"].as_u64().unwrap_or(100_000);
        let status_line = self.tabs[self.active].assistant.status.clone();

        v_flex()
            .gap_4()
            .child(
                settings_group("AI assistant \u{2014} Notebook policy & budgets", cx)
                    .id("settings-ai-policy")
                    .child(setting_row(
                        "Notebook AI policy (.kl/config.json)",
                        "Controls whether this notebook permits AI requests or restricts them to localhost.",
                        policy_track,
                        cx,
                    ))
                    .child(setting_row(
                        "Daily token cap",
                        &format!(
                            "Maximum total tokens per provider per day. Used today: {used_today} / {cap_val}."
                        ),
                        Input::new(&cap),
                        cx,
                    ))
                    .child(setting_row(
                        "Maximum output tokens",
                        "Upper bound on response tokens per assistant turn.",
                        Input::new(&output),
                        cx,
                    ))
                    .child(setting_row(
                        "Context byte budget",
                        "Maximum UTF-8 bytes of notebook evidence assembled into a single receipt (1,024\u{2013}262,144).",
                        Input::new(&context_bytes),
                        cx,
                    ))
                    .child(setting_row(
                        "Retention days",
                        "Automatic retention window for local .kl/assistant/ conversation and proposal history.",
                        Input::new(&retention),
                        cx,
                    )),
            )
            .child(
                settings_group("AI assistant \u{2014} Approvals, privacy & capability gates", cx)
                    .id("settings-ai-privacy")
                    .child(setting_row(
                        "Receipt auto-approvals",
                        "Per-notebook destination approvals for known endpoints.",
                        approvals_box,
                        cx,
                    ))
                    .child(setting_row(
                        "Assistant records (.kl/assistant/assistant.sqlite)",
                        "Deletes local assistant conversations, receipts, proposals, and FTS index without touching notebook source or execution ledger.",
                        action_button("ai-purge")
                            .outline()
                            .small()
                            .icon(Glyph::Trash)
                            .label("Purge assistant records")
                            .on_click(cx.listener(|v, _, _, cx| {
                                v.ai("purge", json!({}));
                                cx.notify();
                            })),
                        cx,
                    ))
                    .child(setting_row(
                        "Capability qualification gates",
                        "Capabilities remain conservative until probed; gated surfaces stay off until qualified.",
                        h_flex()
                            .gap_1p5()
                            .flex_wrap()
                            .child(badge("Streaming: Yes", success))
                            .child(badge("Structured JSON: Yes", success))
                            .child(badge("Vision: Off (gated)", warning))
                            .child(badge("Inline completion: Off (gated)", warning))
                            .child(badge("Autonomous agent: Off (gated)", danger)),
                        cx,
                    ))
                    .when(!status_line.is_empty(), |d| {
                        d.child(
                            div()
                                .id("settings-ai-status")
                                .role(Role::Status)
                                .p_2p5()
                                .rounded(px(6.))
                                .bg(info.opacity(0.08))
                                .text_xs()
                                .child(status_line),
                        )
                    }),
            )
            .into_any_element()
    }

    pub(super) fn ai_settings(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let provider_group = self.ai_settings_provider_group(window, cx);
        let policy_privacy = self.ai_settings_policy_and_privacy(window, cx);
        v_flex()
            .gap_4()
            .w_full()
            .min_w_0()
            .child(provider_group)
            .child(policy_privacy)
            .into_any_element()
    }
}
