use super::*;

/// Field filters edited through the advanced controls rather than as free text.
fn is_field_token(part: &str) -> bool {
    part.split_once(':').is_some_and(|(field, _)| {
        matches!(
            field.to_ascii_lowercase().as_str(),
            "cell" | "session" | "after" | "before" | "from" | "to"
        )
    })
}

/// Canonical tokens of a filter: `from:`/`to:` read as `after:`/`before:`, and
/// order, text case, and repetition do not change what the filter selects.
/// Cell and session IDs retain their exact case.
fn filter_tokens(filter: &str) -> Vec<String> {
    let mut tokens = filter
        .split_whitespace()
        .map(|part| {
            if let Some((field, value)) = part.split_once(':') {
                let field = field.to_ascii_lowercase();
                if matches!(field.as_str(), "cell" | "session") {
                    return format!("{field}:{value}");
                }
            }
            let part = part.to_lowercase();
            if let Some(value) = part.strip_prefix("from:") {
                format!("after:{value}")
            } else if let Some(value) = part.strip_prefix("to:") {
                format!("before:{value}")
            } else {
                part
            }
        })
        .collect::<Vec<_>>();
    tokens.sort();
    tokens.dedup();
    tokens
}

pub(super) fn same_filter(a: &str, b: &str) -> bool {
    filter_tokens(a) == filter_tokens(b)
}

/// Dates must be real `YYYY-MM-DD` days, with every start on or before every end.
pub(super) fn history_filter_valid(filter: &str) -> bool {
    let tokens = filter_tokens(filter);
    let dates = |key: &str| {
        tokens
            .iter()
            .filter_map(|token| token.strip_prefix(key))
            .map(|value| {
                (value.len() == 10)
                    .then(|| chrono::NaiveDate::parse_from_str(value, "%Y-%m-%d").ok())
                    .flatten()
            })
            .collect::<Option<Vec<_>>>()
    };
    let (Some(after), Some(before)) = (dates("after:"), dates("before:")) else {
        return false;
    };
    after
        .iter()
        .all(|start| before.iter().all(|end| start <= end))
}

pub(super) fn format_duration(seconds: f64) -> String {
    if seconds < 60. {
        format!("{seconds:.2} s")
    } else {
        let minutes = (seconds / 60.).floor();
        format!("{minutes:.0} min {:.0} s", seconds - minutes * 60.)
    }
}

/// Run time and duration for an execution's detail page, in the list's terms.
fn execution_facts(item: &Value) -> String {
    let time = record_time(item);
    let ran = match (time.map(relative_time), time.map(local_time)) {
        (Some(relative), Some(exact)) if relative != exact => format!("Ran {relative} ({exact})"),
        (_, Some(exact)) => format!("Ran {exact}"),
        _ => "Run time unknown".into(),
    };
    let duration = record_duration(item)
        .map(format_duration)
        .unwrap_or_else(|| "Unknown".into());
    format!("{ran} \u{00b7} Duration {duration}")
}

/// Column widths shared by the History header and execution rows.
const OUTCOME_W: f32 = 7.;
const TIME_W: f32 = 12.5;
const DURATION_W: f32 = 6.;

fn toggle_raw_details(v: &mut Workspace, _: &mut Window, cx: &mut Context<Workspace>) {
    let flag = v.tabs[v.active]
        .raw_details
        .entry(v.surface.clone())
        .or_default();
    *flag = !*flag;
    cx.notify();
}

impl Workspace {
    /// Refresh only a cached list. Recorded details stay stable until the user
    /// returns to the list; its session scope and cursor travel with the request.
    pub(super) fn history_refresh_request(&self) -> Option<(String, Value)> {
        let tab = self.tabs.get(self.active)?;
        let value = tab.inspections.get("History")?;
        let method = tab.inspection_method.get("History")?;
        matches!(method.as_str(), "executions.list" | "sessions.list").then(|| {
            (
                method.clone(),
                value
                    .get("_page_request")
                    .filter(|v| v.is_object())
                    .cloned()
                    .unwrap_or_else(|| json!({})),
            )
        })
    }

    /// Settings remembers one scroll position per section; other pages use one per surface.
    pub(super) fn scroll_key(&self) -> String {
        if self.surface == "Settings" {
            format!("Settings/{}", self.settings_section)
        } else {
            self.surface.clone()
        }
    }

    pub(super) fn record_page(&mut self, body: Div, _cx: &mut Context<Self>) -> AnyElement {
        let key = self.scroll_key();
        div()
            .id(SharedString::from(format!("inspection-{}", self.surface)))
            .role(Role::ScrollView)
            .a11y_synthetic_children(|tree| tree.parent_node().set_clips_children())
            .track_scroll(self.tabs[self.active].scrolls.entry(key).or_default())
            .size_full()
            .overflow_y_scroll()
            .child(
                div().w_full().flex().justify_center().px_5().py_5().child(
                    body.w_full()
                        .max_w(rems(match self.surface.as_str() {
                            "Settings" => 64.,
                            "Help" => 52.,
                            _ => 76.,
                        }))
                        .min_w_0(),
                ),
            )
            .into_any_element()
    }

    pub(super) fn record_details(&self, value: &Value, cx: &mut Context<Self>) -> Div {
        self.record_details_in(value, None, cx)
    }

    pub(super) fn record_details_in(
        &self,
        value: &Value,
        reveal: Option<GroupReveal>,
        cx: &mut Context<Self>,
    ) -> Div {
        let open = self.tabs[self.active]
            .raw_details
            .get(&self.surface)
            .copied()
            .unwrap_or(false);
        let view = cx.entity();
        let a11y_reveal = reveal.clone();
        let details = action_button("raw-details")
            .self_start()
            .ghost()
            .small()
            .label("Technical details")
            .dropdown_caret(true)
            .selected(open)
            .accessibility_label(if open {
                "Hide technical details"
            } else {
                "Technical details"
            })
            .hint("The raw ledger record as JSON")
            .on_a11y_click(move |window, cx| {
                let reveal = a11y_reveal.clone();
                view.update(cx, |v, cx| match &reveal {
                    Some(reveal) => reveal.activate(v, window, cx, toggle_raw_details),
                    None => toggle_raw_details(v, window, cx),
                });
            })
            .on_click(cx.listener({
                let reveal = reveal.clone();
                move |v, _, window, cx| match &reveal {
                    Some(reveal) => reveal.activate(v, window, cx, toggle_raw_details),
                    None => toggle_raw_details(v, window, cx),
                }
            }));
        let raw = serde_json::to_string_pretty(value).unwrap_or_default();
        let body = v_flex().gap_2().child(details).when(open, |d| {
            d.child(
                card(cx)
                    .p_3()
                    .gap_2()
                    .child(
                        action_button("copy-raw-details")
                            .self_start()
                            .ghost()
                            .small()
                            .label("Copy JSON")
                            .accessibility_label("Copy technical details as JSON")
                            .on_click(cx.listener({
                                let raw = raw.clone();
                                move |_, _, _, cx| copy_acknowledged(raw.clone(), cx)
                            })),
                    )
                    .child(
                        TextView::markdown(
                            "record-raw",
                            format!("```json\n{}\n```", bounded(&raw, 65536)),
                        )
                        .selectable(true),
                    ),
            )
        });
        match reveal {
            Some(reveal) => reveal.track(body),
            None => body,
        }
    }

    pub(super) fn record_back(&self, cx: &mut Context<Self>) -> WorkbenchButton {
        action_button("back")
            .self_start()
            .ghost()
            .small()
            .label("Back")
            .accessibility_label("Back to list")
            .hint("Back to list (Alt+Left)")
            .on_click(cx.listener(|v, _, _, cx| {
                v.back();
                cx.notify();
            }))
    }

    /// The filter the controls currently describe, which may not be applied yet.
    /// Field tokens typed into the search box win until the field controls sync.
    pub(super) fn history_draft(&self, cx: &App) -> String {
        let query = self.history_query.read(cx).value().to_string();
        let (fields, text): (Vec<&str>, Vec<&str>) =
            query.split_whitespace().partition(|p| is_field_token(p));
        let mut parts = text.into_iter().map(str::to_owned).collect::<Vec<_>>();
        if self.history_controls.is_empty() || fields.join(" ") != self.history_fields_key {
            parts.extend(fields.into_iter().map(str::to_owned));
        } else {
            for (key, input) in &self.history_controls {
                let text = input.read(cx).value().trim().to_string();
                if !text.is_empty() {
                    parts.push(format!("{key}:{text}"));
                }
            }
        }
        parts.join(" ")
    }

    pub(super) fn apply_history_filters(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.tabs.is_empty() {
            return;
        }
        let draft = self.history_draft(cx);
        if !history_filter_valid(&draft) {
            cx.notify();
            return;
        }
        self.history_query
            .update(cx, |s, cx| s.set_value(draft.clone(), window, cx));
        self.tabs[self.active].history_applied = draft;
        self.record_active = 0;
        self.inspect("executions.list", json!({}));
        cx.notify();
    }

    /// Clear resets both the edited and the applied filter.
    pub(super) fn clear_history_filters(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.history_query
            .update(cx, |s, cx| s.set_value("", window, cx));
        for input in self.history_controls.values() {
            input.update(cx, |s, cx| s.set_value("", window, cx));
        }
        if let Some(tab) = self.tabs.get_mut(self.active) {
            tab.history_applied.clear();
        }
        self.record_active = 0;
        self.inspect("executions.list", json!({}));
        cx.notify();
    }

    fn history_header(&self, cx: &App) -> Div {
        let label = |text: &'static str| {
            div()
                .text_xs()
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(cx.theme().muted_foreground)
                .child(text)
        };
        h_flex()
            .px_3()
            .py_1p5()
            .gap_3()
            .items_center()
            .border_b_1()
            .border_color(divider(cx))
            .bg(cx.theme().muted.opacity(0.5))
            .child(div().w(px(16.)).flex_none())
            .child(label("Cell").flex_1().min_w_0())
            .child(label("Outcome").w(rems(OUTCOME_W)).flex_none())
            .child(label("Run time").w(rems(TIME_W)).flex_none())
            .child(
                h_flex()
                    .w(rems(DURATION_W))
                    .flex_none()
                    .justify_end()
                    .child(label("Duration")),
            )
            // Reserves the action column so headings sit over their values.
            .child(div().w(rems(5.75)).flex_none())
    }

    /// Cell, outcome, run time, and duration as separate fields: aligned columns
    /// on wide pages, labeled lines when the window is narrow.
    fn execution_row(&self, item: &Value, selected: bool, cx: &App) -> Div {
        let theme = cx.theme();
        let muted = theme.muted_foreground;
        let status = item["status"].as_str().unwrap_or("");
        let title = cell_title(
            item["source"]
                .as_str()
                .unwrap_or("Recorded execution (source unavailable)"),
            "code",
        );
        let outcome = if status.is_empty() {
            div().text_sm().text_color(muted).child("Unknown")
        } else if notable_status(status) {
            div().child(badge(outcome_label(status).to_string(), tone(status, cx)))
        } else {
            div()
                .text_sm()
                .text_color(muted)
                .child(outcome_label(status).to_string())
        };
        let time = record_time(item);
        let relative = time.map(relative_time);
        let exact = time.map(local_time);
        let known_duration = record_duration(item).map(format_duration);
        let duration_text = known_duration.clone().unwrap_or_else(|| "\u{2014}".into());
        let duration_a11y = known_duration
            .as_ref()
            .map(|d| format!("Duration {d}"))
            .unwrap_or_else(|| "Duration unknown".into());
        let exec_id = item["exec_id"].as_str().unwrap_or("row");
        let base = h_flex()
            .min_w_0()
            .gap_3()
            .px_3()
            .py_1p5()
            .items_center()
            .border_b_1()
            .border_color(divider(cx))
            .when(selected, |d| d.bg(theme.list_active))
            .hover(move |s| s.bg(theme.list_hover));
        let glyph = Icon::new(Glyph::Terminal).small().text_color(muted);
        let title = div()
            .min_w_0()
            .truncate()
            .text_sm()
            .font_weight(FontWeight::MEDIUM)
            .font_family(theme.mono_font_family.clone())
            .child(title);
        if self.compact {
            let field = |label: &'static str, value: AnyElement| {
                h_flex()
                    .gap_1p5()
                    .items_center()
                    .child(div().text_xs().text_color(muted).child(label))
                    .child(value)
            };
            return base.child(glyph).child(
                v_flex().flex_1().min_w_0().gap_0p5().child(title).child(
                    h_flex()
                        .flex_wrap()
                        .gap_x_4()
                        .gap_y_0p5()
                        .child(field("Outcome", outcome.into_any_element()))
                        .child(field(
                            "Run time",
                            div()
                                .text_xs()
                                .whitespace_normal()
                                .child(match (&relative, &exact) {
                                    (Some(r), Some(e)) if r != e => format!("{r} \u{00b7} {e}"),
                                    (_, Some(e)) => e.clone(),
                                    _ => "Unknown".into(),
                                })
                                .into_any_element(),
                        ))
                        .child(field(
                            "Duration",
                            div()
                                .id(SharedString::from(format!("history-duration-{exec_id}")))
                                .debug_selector({
                                    let exec_id = exec_id.to_owned();
                                    move || format!("history-duration-{exec_id}")
                                })
                                .role(Role::Label)
                                .aria_label(duration_a11y)
                                .text_xs()
                                .when(known_duration.is_none(), |d| d.text_color(muted))
                                .child(duration_text)
                                .into_any_element(),
                        )),
                ),
            );
        }
        base.child(glyph)
            .child(title.flex_1())
            .child(
                h_flex()
                    .w(rems(OUTCOME_W))
                    .flex_none()
                    .items_center()
                    .child(outcome),
            )
            .child(
                v_flex()
                    .w(rems(TIME_W))
                    .flex_none()
                    .min_w_0()
                    .child(
                        div()
                            .text_sm()
                            .truncate()
                            .child(relative.clone().unwrap_or_else(|| "Unknown".into())),
                    )
                    .when(relative != exact, |d| {
                        d.child(
                            div()
                                .text_xs()
                                .truncate()
                                .text_color(muted)
                                .children(exact.clone()),
                        )
                    }),
            )
            .child(
                h_flex()
                    .w(rems(DURATION_W))
                    .flex_none()
                    .justify_end()
                    .child(
                        div()
                            .id(SharedString::from(format!("history-duration-{exec_id}")))
                            .debug_selector({
                                let exec_id = exec_id.to_owned();
                                move || format!("history-duration-{exec_id}")
                            })
                            .role(Role::Label)
                            .aria_label(duration_a11y)
                            .text_sm()
                            .font_family(theme.mono_font_family.clone())
                            .when(known_duration.is_none(), |d| d.text_color(muted))
                            .child(duration_text),
                    ),
            )
    }

    #[inline(never)]
    pub(super) fn history_page(
        &mut self,
        value: &Value,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let mut body = v_flex().gap_3();
        if value.get("exec_id").is_some() {
            let origin = value["origin"]
                .as_str()
                .filter(|origin| *origin != "unknown")
                .map(str::to_owned);
            let eid_ai = value["exec_id"].as_str().unwrap_or_default().to_string();
            let cid_ai = value["cell_id"].as_str().unwrap_or_default().to_string();
            let is_err = value["status"].as_str() == Some("error");
            body = body.child(
                h_flex()
                    .gap_2()
                    .flex_wrap()
                    .items_center()
                    .child(self.record_back(cx))
                    .child(
                        div()
                            .text_lg()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child("Recorded execution"),
                    )
                    .when(
                        notable_status(value["status"].as_str().unwrap_or("")),
                        |d| {
                            d.child(badge(
                                outcome_label(value["status"].as_str().unwrap_or("Unknown"))
                                    .to_string(),
                                tone(value["status"].as_str().unwrap_or(""), cx),
                            ))
                        },
                    )
                    .when_some(origin, |d, origin| {
                        d.child(badge(
                            format!("Source: {origin}"),
                            cx.theme().muted_foreground,
                        ))
                    })
                    .child(div().flex_1())
                    .child(
                        action_button("history-ai-explain")
                            .outline()
                            .small()
                            .icon(Glyph::Sparkles)
                            .label(if is_err {
                                "Explain failure"
                            } else {
                                "Explain in Assistant"
                            })
                            .accessibility_label(if is_err {
                                "Explain failure in Assistant"
                            } else {
                                "Explain execution in Assistant"
                            })
                            .hint("Open in Assistant with this execution attached")
                            .on_click(cx.listener(move |v, _, _, cx| {
                                let tab = &mut v.tabs[v.active];
                                if !cid_ai.is_empty() {
                                    tab.model.selected = Some(cid_ai.clone());
                                }
                                tab.assistant.exec_ids = vec![eid_ai.clone()];
                                tab.assistant.open = true;
                                tab.assistant.receipt = Value::Null;
                                let task = if is_err { "error" } else { "explain" };
                                let prompt = if is_err {
                                    format!(
                                        "Explain why execution {eid_ai} failed and propose a fix."
                                    )
                                } else {
                                    format!("Explain recorded execution {eid_ai} and its output.")
                                };
                                v.ai_preview(task, prompt, cx);
                            })),
                    ),
            );
            body = body.child(
                div()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child(execution_facts(value)),
            );
            if let Some(source) = value["source"].as_str() {
                let reused = source.to_owned();
                body = body.child(
                    v_flex()
                        .gap_2()
                        .child(
                            h_flex()
                                .justify_between()
                                .items_center()
                                .gap_2()
                                .flex_wrap()
                                .child(
                                    div()
                                        .text_sm()
                                        .font_weight(FontWeight::SEMIBOLD)
                                        .text_color(cx.theme().foreground)
                                        .child("Recorded source"),
                                )
                                .child(
                                    action_button("reuse-recorded-input")
                                        .outline()
                                        .small()
                                        .icon(Glyph::Plus)
                                        .label("Insert as cell")
                                        .accessibility_label("Insert recorded source as a new cell")
                                        .hint("Saves a new code cell with this recorded source without running it")
                                        .on_click(cx.listener(move |view, _, _, cx| {
                                            let index = view.tabs[view.active].model.order.len();
                                            view.mutate(json!({"action":"add","type":"code","source":reused,"index":index}));
                                            view.select_surface("Notebook", cx);
                                        })),
                                ),
                        )
                        .child(
                            card(cx).p_3().child(
                                TextView::markdown(
                                    "history-source",
                                    format!("```python\n{}\n```", bounded(source, 65536)),
                                )
                                .selectable(true),
                            ),
                        ),
                );
                if let Some(current) = value["cell_id"]
                    .as_str()
                    .and_then(|id| self.tabs[self.active].model.cells.get(id))
                {
                    let changed = current.text != source;
                    let comparing = self.tabs[self.active]
                        .raw_details
                        .get("source-comparison")
                        .copied()
                        .unwrap_or(false);
                    let id = current.disk.id.clone();
                    body = body.child(
                        h_flex()
                            .gap_2()
                            .flex_wrap()
                            .items_center()
                            .when(changed, |d| {
                                d.child(badge("Edited since run", cx.theme().warning))
                            })
                            .child(
                                action_button("compare-current-source")
                                    .ghost()
                                    .small()
                                    .label(if comparing {
                                        "Hide comparison"
                                    } else {
                                        "Compare source"
                                    })
                                    .on_click(cx.listener(|v, _, _, cx| {
                                        let flag = v.tabs[v.active]
                                            .raw_details
                                            .entry("source-comparison".into())
                                            .or_default();
                                        *flag = !*flag;
                                        cx.notify();
                                    })),
                            )
                            .child(
                                action_button("return-to-cell")
                                    .outline()
                                    .small()
                                    .label("Return to cell")
                                    .on_click(cx.listener(move |v, _, window, cx| {
                                        v.select_surface("Notebook", cx);
                                        let tab = &mut v.tabs[v.active];
                                        tab.model.selected = Some(id.clone());
                                        tab.model.mode = Mode::Edit;
                                        tab.reveal = Some(id.clone());
                                        if let Some(editor) = tab.editors.get(&id) {
                                            editor.update(cx, |e, cx| e.focus(window, cx));
                                        } else {
                                            v.focus.focus(window, cx);
                                        }
                                        cx.notify();
                                    })),
                            ),
                    );
                    if changed || comparing {
                        body = body.child(
                            section("Current source", cx).child(
                                card(cx).p_3().child(
                                    TextView::markdown(
                                        "history-current-source",
                                        format!(
                                            "```python\n{}\n```",
                                            bounded(&current.text, 65536)
                                        ),
                                    )
                                    .selectable(true),
                                ),
                            ),
                        );
                    }
                } else {
                    body = body.child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child("Cell removed from notebook."),
                    );
                }
            }
            if let Some(outputs) = value["outputs"].as_array() {
                for (i, event) in outputs.iter().enumerate() {
                    body = body.child(
                        outputs::OutputBlock {
                            tools_state: None,
                            id: format!("history-result-{i}").into(),
                            event: event.clone(),
                            context: format!(
                                "Recorded execution {} output {}",
                                value["ordinal"].as_u64().unwrap_or_default(),
                                i + 1
                            )
                            .into(),
                            commands: Some(self.tabs[self.active].service.commands.clone()),
                            cell_id: value["cell_id"].as_str().unwrap_or_default().into(),
                            exec_id: value["exec_id"].as_str().unwrap_or_default().into(),
                            index: i,
                            worker_generation: None,
                            project: Some(self.tabs[self.active].model.project.clone()),
                        }
                        .into_any_element(),
                    );
                }
            }
        } else {
            let current_method = self.tabs[self.active]
                .inspection_method
                .get("History")
                .map(String::as_str)
                .unwrap_or("executions.list");
            body = body.child(
                h_flex()
                    .gap_3()
                    .items_center()
                    .child(page_heading("History", "", cx))
                    .child(
                        segmented_track(cx)
                            .child(
                                action_button("all-executions")
                                    .small()
                                    .segment(current_method != "sessions.list")
                                    .label("Executions")
                                    .on_click(cx.listener(|v, _, _, _| {
                                        v.inspect("executions.list", json!({}))
                                    })),
                            )
                            .child(
                                action_button("all-sessions")
                                    .small()
                                    .segment(current_method == "sessions.list")
                                    .label("Sessions")
                                    .on_click(cx.listener(|v, _, _, _| {
                                        v.inspect("sessions.list", json!({}))
                                    })),
                            ),
                    ),
            );
            let query = self.history_query.read(cx).value().to_string();
            let fields_key = query
                .split_whitespace()
                .filter(|p| {
                    matches!(
                        p.split(':').next(),
                        Some("cell" | "session" | "after" | "before" | "from" | "to")
                    )
                })
                .collect::<Vec<_>>()
                .join(" ");
            if fields_key != self.history_fields_key {
                for (key, input) in &self.history_controls {
                    let value = query
                        .split_whitespace()
                        .find_map(|p| {
                            p.strip_prefix(&format!("{key}:")).or_else(|| {
                                if key == "after" {
                                    p.strip_prefix("from:")
                                } else if key == "before" {
                                    p.strip_prefix("to:")
                                } else {
                                    None
                                }
                            })
                        })
                        .unwrap_or("")
                        .to_string();
                    input.update(cx, |s, cx| s.set_value(value, window, cx));
                }
                self.history_fields_key = fields_key;
            }
            let mut fields = h_flex().gap_2().flex_wrap();
            for (key, label) in [
                ("cell", "Cell"),
                ("session", "Session"),
                ("after", "From (YYYY-MM-DD)"),
                ("before", "Through (YYYY-MM-DD)"),
            ] {
                let initial = query
                    .split_whitespace()
                    .find_map(|p| {
                        p.strip_prefix(&format!("{key}:")).or_else(|| {
                            if key == "after" {
                                p.strip_prefix("from:")
                            } else if key == "before" {
                                p.strip_prefix("to:")
                            } else {
                                None
                            }
                        })
                    })
                    .unwrap_or("")
                    .to_string();
                if !self.history_controls.contains_key(key) {
                    let input = cx.new(|cx| {
                        let mut s = InputState::new(window, cx).placeholder(label);
                        s.set_value(initial, window, cx);
                        s
                    });
                    self.history_field_subscriptions.push(cx.subscribe_in(
                        &input,
                        window,
                        |v, _, event: &InputEvent, window, cx| match event {
                            InputEvent::Change => cx.notify(),
                            InputEvent::PressEnter { .. } => v.apply_history_filters(window, cx),
                            _ => {}
                        },
                    ));
                    self.history_controls.insert(key.into(), input);
                }
                let input = self.history_controls[key].clone();
                fields = fields.child(
                    div()
                        .w(rems(11.))
                        .child(Input::new(&input).small().cleanable(true)),
                );
            }
            let draft = self.history_draft(cx);
            let valid = history_filter_valid(&draft);
            let applied = self.tabs[self.active].history_applied.clone();
            let pending = !same_filter(&draft, &applied);
            let advanced_query = query.split_whitespace().any(|p| {
                matches!(
                    p.split(':').next(),
                    Some("cell" | "session" | "after" | "before" | "from" | "to")
                )
            });
            let show_fields = self.history_filters_open || advanced_query || !valid;
            body = body.child(
                v_flex()
                    .gap_2()
                    .child(
                        h_flex()
                            .gap_2()
                            .flex_wrap()
                            .items_center()
                            .child(
                                div()
                                    .flex_1()
                                    .min_w(rems(16.))
                                    .child(Input::new(&self.history_query).cleanable(true).small()),
                            )
                            .child(self.history_filters(cx))
                            .child(
                                action_button("history-more-filters")
                                    .ghost()
                                    .small()
                                    .label("Filters")
                                    .selected(show_fields)
                                    .accessibility_label(if show_fields {
                                        "Hide advanced history filters"
                                    } else {
                                        "Show advanced history filters"
                                    })
                                    .hint(if show_fields {
                                        "Hide Cell, Session, and date fields"
                                    } else {
                                        "Cell and Session match exact IDs. Dates use after:/before:. Search reads up to 1 MiB per execution."
                                    })
                                    .on_click(cx.listener(|v, _, _, cx| {
                                        v.history_filters_open = !v.history_filters_open;
                                        cx.notify();
                                    })),
                            )
                            .when(pending, |d| {
                                let apply = action_button("apply-history-filters")
                                    .small()
                                    .label("Apply")
                                    .accessibility_label("Apply filters")
                                    .hint("Apply filters (Enter)")
                                    .on_click(cx.listener(|v, _, window, cx| {
                                        v.apply_history_filters(window, cx)
                                    }))
                                    .unavailable(!valid);
                                let apply = if valid {
                                    apply.primary()
                                } else {
                                    apply.outline()
                                };
                                d.child(apply).child(
                                    action_button("clear-history-filters")
                                        .ghost()
                                        .small()
                                        .label("Clear")
                                        .accessibility_label("Clear filters")
                                        .hint("Clear edited and applied filters")
                                        .on_click(cx.listener(|v, _, window, cx| {
                                            v.clear_history_filters(window, cx)
                                        })),
                                )
                            }),
                    )
                    .when(show_fields, |d| d.child(fields))
                    .when(!valid, |d| {
                        d.child(
                            div().text_color(cx.theme().danger).text_sm().child(
                                "Enter dates as YYYY-MM-DD, with From on or before Through.",
                            ),
                        )
                    })
                    .child({
                        let (text, color) = match (pending, applied.trim().is_empty()) {
                            (true, true) => (
                                "Filters edited, not applied. Press Apply or Enter to search recorded runs.".to_string(),
                                cx.theme().warning,
                            ),
                            (true, false) => (
                                format!("Filters edited, not applied. The current page still matches: {applied}"),
                                cx.theme().warning,
                            ),
                            (false, true) => (
                                "Showing one page of recorded runs.".to_string(),
                                cx.theme().muted_foreground,
                            ),
                            (false, false) => (
                                format!("Searched {} executions for: {applied}", value["scanned"].as_u64().unwrap_or(0)),
                                cx.theme().muted_foreground,
                            ),
                        };
                        h_flex()
                            .id("history-filter-state")
                            .role(Role::Status)
                            .aria_label(text.clone())
                            .gap_1p5()
                            .items_center()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .when(pending, |d| d.child(dot(color)))
                            .child(div().whitespace_normal().child(text))
                    }),
            );
            if value["source_search_limited"].as_u64().unwrap_or(0) > 0 {
                body = body.child(div().text_sm().text_color(cx.theme().warning).whitespace_normal().child("Some source could not be searched or exceeds 1 MiB. Narrow by cell/session and inspect those records directly."));
            }
            let items = value["items"].as_array().cloned().unwrap_or_default();
            let filter = applied.to_lowercase();
            let executions = items.iter().any(|item| item.get("exec_id").is_some());
            let mut rows = card(cx).when(executions && !self.compact, |d| {
                d.child(self.history_header(cx))
            });
            let mut count = 0;
            for (i, item) in items.iter().enumerate().filter(|(_, item)| {
                value["search_page"] == true
                    || history_matches(item, &filter, &self.tabs[self.active].model)
            }) {
                count += 1;
                let execution = item["exec_id"].as_str();
                let id = execution
                    .or(item["session_id"].as_str())
                    .unwrap_or_default()
                    .to_string();
                let method = if execution.is_some() {
                    "execution.get"
                } else {
                    "executions.list"
                };
                let fields = if execution.is_some() {
                    json!({"exec_id":id})
                } else {
                    json!({"session_id":id})
                };
                let action_name = if execution.is_some() {
                    format!(
                        "View execution {}: {}",
                        item["ordinal"].as_u64().unwrap_or(i as u64 + 1),
                        record_label(item, &self.tabs[self.active].model)
                    )
                } else {
                    format!(
                        "View session {}: {}",
                        i + 1,
                        record_label(item, &self.tabs[self.active].model)
                    )
                };
                let replay_id = id.clone();
                let ai_exec_id = id.clone();
                let ai_cell_id = item["cell_id"].as_str().unwrap_or_default().to_string();
                let ai_is_err = item["status"].as_str() == Some("error");
                let record = if execution.is_some() {
                    self.execution_row(item, i == self.record_active, cx)
                } else {
                    record_row(
                        Glyph::Clock,
                        &record_label(item, &self.tabs[self.active].model),
                        item["status"].as_str().unwrap_or(""),
                        i == self.record_active,
                        cx,
                    )
                };
                rows = rows.child(record.child(
                    h_flex()
                        .flex_none()
                        .gap_1()
                        .items_center()
                        .justify_end()
                        .when(execution.is_some() && !self.compact, |d| d.w(rems(5.75)))
                    .when(execution.is_some(), |d| {
                        d.child(
                            action_button(("record-ai", i))
                                .ghost()
                                .small()
                                .icon(Glyph::Sparkles)
                                .accessibility_label("Explain execution in Assistant")
                                .hint("Open in Assistant with this execution attached")
                                .on_click(cx.listener(move |v, _, _, cx| {
                                    let tab = &mut v.tabs[v.active];
                                    if !ai_cell_id.is_empty() {
                                        tab.model.selected = Some(ai_cell_id.clone());
                                    }
                                    tab.assistant.exec_ids = vec![ai_exec_id.clone()];
                                    tab.assistant.open = true;
                                    tab.assistant.receipt = Value::Null;
                                    let task = if ai_is_err { "error" } else { "explain" };
                                    let prompt = if ai_is_err {
                                        format!(
                                            "Explain why execution {ai_exec_id} failed and propose a fix."
                                        )
                                    } else {
                                        format!("Explain recorded execution {ai_exec_id} and its output.")
                                    };
                                    v.ai_preview(task, prompt, cx);
                                })),
                        )
                    })
                    .child(
                        action_button(("record-view", i))
                            .ghost()
                            .small()
                            .label(if execution.is_some() {
                                "View"
                            } else {
                                "Executions"
                            })
                            .accessibility_label(action_name)
                            .on_click(
                                cx.listener(move |v, _, _, _| v.inspect(method, fields.clone())),
                            ),
                    )
                    .when(execution.is_none(), |d| {
                        d.child(
                            action_button(("replay-session", i))
                                .outline()
                                .small()
                                .label("Replay\u{2026}")
                                .accessibility_label(format!(
                                    "Replay {}",
                                    record_label(item, &self.tabs[self.active].model)
                                ))
                                .on_click(cx.listener(move |v, _, _, cx| {
                                    v.confirmation =
                                        Some(("replay".into(), json!({"session_id":replay_id})));
                                    cx.notify();
                                })),
                        )
                    }),
                ));
            }
            if count == 0 {
                body = body.child(empty_row(
                    if value.is_null() {
                        "Loading history\u{2026}"
                    } else if value["search_page"] == true && (!applied.is_empty() || !value["cursor"].is_null()) {
                        if value["cursor"].is_null() {
                            "No matching executions found. Change or clear the filters to search again."
                        } else {
                            "No matches in the executions searched so far. Continue search to search the remaining records."
                        }
                    } else if items.is_empty() {
                        "No recorded history yet. Run a code cell to record executions."
                    } else if value["cursor"].is_null() {
                        "No loaded records match the applied filters. Clear filters to see every record."
                    } else {
                        "No records on this page match the applied filters. Later pages may contain matches."
                    },
                    cx,
                ));
            } else {
                body = body.child(rows);
            }
            if !value["cursor"].is_null() {
                let method = self.tabs[self.active]
                    .inspection_method
                    .get("History")
                    .cloned()
                    .unwrap_or_else(|| "executions.list".into());
                let mut fields = value["_page_request"]
                    .as_object()
                    .cloned()
                    .unwrap_or_default();
                fields.insert("cursor".into(), value["cursor"].clone());
                body = body.child(
                    action_button("history-next-page")
                        .self_start()
                        .ghost()
                        .small()
                        .label(if count == 0 && value["search_page"] == true {
                            "Continue search"
                        } else {
                            "Next page"
                        })
                        .on_click(cx.listener(move |v, _, _, _| {
                            v.inspect(&method, Value::Object(fields.clone()))
                        })),
                );
            }
        }
        body = body.child(self.record_details(value, cx));
        self.record_page(body, cx)
    }
    pub(super) fn history_filters(&self, cx: &mut Context<Self>) -> Div {
        let query = self.history_query.read(cx).value().to_string();
        let mut outcomes = segmented_track(cx);
        for (value, label) in [
            ("", "All"),
            ("ok", "Succeeded"),
            ("error", "Failed"),
            ("interrupted", "Interrupted"),
        ] {
            let token = format!("outcome:{value}");
            let selected = if value.is_empty() {
                !query.split_whitespace().any(|s| s.starts_with("outcome:"))
            } else {
                query.split_whitespace().any(|s| s == token)
            };
            outcomes = outcomes.child(
                action_button(SharedString::from(format!("outcome-{value}")))
                    .small()
                    .label(label)
                    .segment(selected)
                    .on_click(cx.listener(move |v, _, window, cx| {
                        let query = v.history_query.read(cx).value().to_string();
                        let mut parts = query
                            .split_whitespace()
                            .filter(|s| !s.starts_with("outcome:"))
                            .map(str::to_owned)
                            .collect::<Vec<_>>();
                        if !value.is_empty() {
                            parts.push(format!("outcome:{value}"));
                        }
                        v.history_query
                            .update(cx, |s, cx| s.set_value(parts.join(" "), window, cx));
                        v.record_active = 0;
                        v.apply_history_filters(window, cx);
                    })),
            );
        }
        h_flex()
            .flex_wrap()
            .gap_2()
            .items_center()
            .child(outcomes)
            .child(info_tip(
                "history-filter-syntax-tip",
                "Search source text, or combine cell:, session:, after:YYYY-MM-DD, and before:YYYY-MM-DD filters.",
                cx,
            ))
    }
}
