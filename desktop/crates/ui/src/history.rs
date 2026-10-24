use super::*;

fn toggle_raw_details(v: &mut Workspace, _: &mut Window, cx: &mut Context<Workspace>) {
    let flag = v.tabs[v.active]
        .raw_details
        .entry(v.surface.clone())
        .or_default();
    *flag = !*flag;
    cx.notify();
}

impl Workspace {
    pub(super) fn record_page(&mut self, body: Div, _cx: &mut Context<Self>) -> AnyElement {
        div()
            .id(SharedString::from(format!("inspection-{}", self.surface)))
            .role(Role::ScrollView)
            .a11y_synthetic_children(|tree| tree.parent_node().set_clips_children())
            .track_scroll(
                self.tabs[self.active]
                    .scrolls
                    .entry(self.surface.clone())
                    .or_default(),
            )
            .size_full()
            .overflow_y_scroll()
            .child(
                div().w_full().flex().justify_center().px_4().py_5().child(
                    body.w_full()
                        .max_w(rems(
                            if self.surface == "Settings" || self.surface == "Help" {
                                52.
                            } else {
                                76.
                            },
                        ))
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
            .label(if open {
                "Hide technical details"
            } else {
                "Technical details"
            })
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
        let body = v_flex().gap_2().child(details).when(open, |d| {
            d.child(
                card(cx).p_3().child(
                    TextView::markdown(
                        "record-raw",
                        format!(
                            "```json\n{}\n```",
                            bounded(
                                &serde_json::to_string_pretty(value).unwrap_or_default(),
                                65536
                            )
                        ),
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
            .label("Back to list")
            .hint("Alt+Left")
            .on_click(cx.listener(|v, _, _, cx| {
                v.back();
                cx.notify();
            }))
    }

    pub(super) fn history_page(
        &mut self,
        value: &Value,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let mut body = v_flex().gap_3();
        if value.get("exec_id").is_some() {
            let eid_ai = value["exec_id"].as_str().unwrap_or_default().to_string();
            let cid_ai = value["cell_id"].as_str().unwrap_or_default().to_string();
            let is_err = value["status"].as_str() == Some("error");
            body = body.child(
                h_flex()
                    .gap_3()
                    .flex_wrap()
                    .items_center()
                    .child(self.record_back(cx))
                    .child(
                        div()
                            .text_lg()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child("Recorded execution"),
                    )
                    .child(badge(
                        outcome_label(value["status"].as_str().unwrap_or("Unknown")).to_string(),
                        tone(value["status"].as_str().unwrap_or(""), cx),
                    ))
                    .child(div().flex_1())
                    .child(
                        action_button("history-ai-explain")
                            .outline()
                            .small()
                            .icon(Glyph::Sparkles)
                            .label(if is_err {
                                "Explain failure in Assistant"
                            } else {
                                "Explain execution in Assistant"
                            })
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
                                    format!("Explain why execution {eid_ai} failed and propose a fix.")
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
                    .child(record_label(value, &self.tabs[self.active].model)),
            );
            if let Some(source) = value["source"].as_str() {
                body = body.child(
                    section("Recorded source", cx).child(
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
                            .child(
                                div()
                                    .text_sm()
                                    .text_color(cx.theme().muted_foreground)
                                    .child(if changed {
                                        "Source edited since this execution"
                                    } else {
                                        "Current source is unchanged"
                                    }),
                            )
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
                            .text_sm()
                            .text_color(cx.theme().muted_foreground)
                            .child("This cell is no longer present in the notebook."),
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
                    .child(
                        v_flex()
                            .gap_0p5()
                            .child(
                                div()
                                    .text_lg()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child("History"),
                            )
                            .child(
                                div()
                                    .text_sm()
                                    .text_color(cx.theme().muted_foreground)
                                    .child("Sessions and executions recorded in the ledger"),
                            ),
                    )
                    .child(div().flex_1())
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
            let mut valid = true;
            for (key, label) in [
                ("cell", "Cell"),
                ("session", "Session"),
                ("after", "From (UTC date)"),
                ("before", "Through (UTC date)"),
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
                    self.history_field_subscriptions.push(cx.subscribe(
                        &input,
                        |_, _, event: &InputEvent, cx| {
                            if matches!(event, InputEvent::Change) {
                                cx.notify();
                            }
                        },
                    ));
                    self.history_controls.insert(key.into(), input);
                }
                let input = self.history_controls[key].clone();
                let text = input.read(cx).value().to_string();
                if matches!(key, "after" | "before")
                    && !text.is_empty()
                    && chrono::NaiveDate::parse_from_str(&text, "%Y-%m-%d").is_err()
                {
                    valid = false;
                }
                fields = fields.child(
                    v_flex()
                        .gap_1()
                        .child(
                            div()
                                .text_xs()
                                .text_color(cx.theme().muted_foreground)
                                .child(label),
                        )
                        .child(
                            div()
                                .w(rems(11.))
                                .child(Input::new(&input).small().cleanable(true)),
                        ),
                );
            }
            let from = self.history_controls["after"]
                .read(cx)
                .value()
                .trim()
                .to_string();
            let through = self.history_controls["before"]
                .read(cx)
                .value()
                .trim()
                .to_string();
            if !from.is_empty() && !through.is_empty() && from > through {
                valid = false;
            }
            body = body.child(
                card(cx)
                    .p_3()
                    .gap_2p5()
                    .child(Input::new(&self.history_query).cleanable(true).small())
                    .child(self.history_filters(cx))
                    .child(fields)
                    .when(!valid, |d| {
                        d.child(
                            div().text_color(cx.theme().danger).text_sm().child(
                                "Enter dates as YYYY-MM-DD, with From on or before Through.",
                            ),
                        )
                    })
                    .child(
                        h_flex()
                            .gap_2()
                            .child(
                                action_button("apply-history-filters")
                                    .outline()
                                    .small()
                                    .label("Apply filters")
                                    .unavailable(!valid)
                                    .on_click(cx.listener(|v, _, window, cx| {
                                        let query = v.history_query.read(cx).value().to_string();
                                        let mut parts = query
                                            .split_whitespace()
                                            .filter(|p| {
                                                !matches!(
                                                    p.split(':').next(),
                                                    Some(
                                                        "cell"
                                                            | "session"
                                                            | "after"
                                                            | "before"
                                                            | "from"
                                                            | "to"
                                                    )
                                                )
                                            })
                                            .map(str::to_owned)
                                            .collect::<Vec<_>>();
                                        for (key, input) in &v.history_controls {
                                            let text = input.read(cx).value().trim().to_string();
                                            if !text.is_empty() {
                                                parts.push(format!("{key}:{text}"));
                                            }
                                        }
                                        v.history_query.update(cx, |s, cx| {
                                            s.set_value(parts.join(" "), window, cx)
                                        });
                                        v.record_active = 0;
                                        v.inspect("executions.list", json!({}));
                                        cx.notify();
                                    })),
                            )
                            .child(
                                action_button("clear-history-filters")
                                    .ghost()
                                    .small()
                                    .label("Clear filters")
                                    .on_click(cx.listener(|v, _, window, cx| {
                                        v.history_query
                                            .update(cx, |s, cx| s.set_value("", window, cx));
                                        for input in v.history_controls.values() {
                                            input.update(cx, |s, cx| s.set_value("", window, cx));
                                        }
                                        cx.notify();
                                    })),
                            ),
                    ),
            );
            let items = value["items"].as_array().cloned().unwrap_or_default();
            let filter = self.history_query.read(cx).value().to_lowercase();
            let mut rows = card(cx);
            let mut count = 0;
            for (i, item) in items
                .iter()
                .enumerate()
                .filter(|(_, item)| history_matches(item, &filter, &self.tabs[self.active].model))
            {
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
                rows = rows.child(
                    record_row(
                        if execution.is_some() {
                            Glyph::Terminal
                        } else {
                            Glyph::Clock
                        },
                        &record_label(item, &self.tabs[self.active].model),
                        item["status"].as_str().unwrap_or(""),
                        cx,
                    )
                    .when(i == self.record_active, |d| d.bg(cx.theme().list_active))
                    .child(
                        action_button(("record-view", i))
                            .ghost()
                            .small()
                            .label(if execution.is_some() {
                                "View execution"
                            } else {
                                "View session"
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
                                .label("Replay session\u{2026}")
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
                );
            }
            if count == 0 {
                rows = rows.child(empty_row(
                    if value.is_null() {
                        "Loading recorded history\u{2026}"
                    } else if items.is_empty() {
                        "No recorded history in this list. Run a code cell to create history."
                    } else {
                        "No records match these filters."
                    },
                    cx,
                ));
            }
            body = body.child(rows);
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
                        .label("Next page")
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
        let mut filters = h_flex().flex_wrap().gap_1p5().items_center().child(
            div()
                .text_xs()
                .font_weight(FontWeight::MEDIUM)
                .text_color(cx.theme().muted_foreground)
                .mr_1()
                .child("Outcome"),
        );
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
            filters = filters.child(
                action_button(SharedString::from(format!("outcome-{value}")))
                    .ghost()
                    .small()
                    .label(label)
                    .selected(selected)
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
                        cx.notify();
                    })),
            );
        }
        v_flex().gap_1p5().child(filters).child(
            div()
                .text_xs()
                .text_color(cx.theme().muted_foreground)
                .child(
                    "Search source, or combine cell:, session:, after:YYYY-MM-DD and before:YYYY-MM-DD filters.",
                ),
        )
    }
}
