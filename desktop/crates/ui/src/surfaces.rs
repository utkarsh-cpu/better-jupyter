use super::*;

impl Workspace {
    pub(super) fn panel(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let value = self.tabs[self.active]
            .inspections
            .get(&self.surface)
            .cloned()
            .unwrap_or(Value::Null);
        if self.surface == "Settings" {
            return self.settings_page(&value, cx);
        }
        if self.surface == "History" {
            return self.history_page(&value, window, cx);
        }
        if self.surface == "Checkpoints"
            && value.get("snapshot").is_none()
            && !self.tabs[self.active]
                .inspection_method
                .get("Checkpoints")
                .is_some_and(|m| m.starts_with("training.") || m.starts_with("native."))
        {
            return self.checkpoint_page(&value, cx);
        }
        self.supporting_panel(window, cx)
    }

    fn supporting_panel(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let value = self.tabs[self.active]
            .inspections
            .get(&self.surface)
            .cloned()
            .unwrap_or(Value::Null);
        let theme = cx.theme();
        let muted_fg = theme.muted_foreground;
        let mono = theme.mono_font_family.clone();
        let (_glyph, description) = SURFACES
            .iter()
            .find(|(name, ..)| *name == self.surface)
            .map(|(_, glyph, description)| (*glyph, *description))
            .unwrap_or((Glyph::Info, ""));
        let mut body = v_flex()
            .w_full()
            .max_w(rems(
                if self.surface == "Settings" || self.surface == "Help" {
                    52.
                } else {
                    76.
                },
            ))
            .min_w_0()
            .gap_4()
            .child(
                h_flex().gap_3().items_center().child(
                    v_flex()
                        .gap_0p5()
                        .child(
                            div()
                                .text_lg()
                                .font_weight(FontWeight::SEMIBOLD)
                                .child(self.surface.clone()),
                        )
                        .child(
                            div()
                                .text_sm()
                                .text_color(muted_fg)
                                .child(description.to_string()),
                        ),
                ),
            );
        if self.surface == "Help" {
            body = body.child(
                action_button("about")
                    .outline()
                    .label("About KernelLedger")
                    .on_click(cx.listener(|v, _, _, cx| {
                        v.confirmation = Some(("about".into(), json!({})));
                        cx.notify();
                    })),
            );
            body = body.child(self.help_content(cx));
            body = body.child(
                action_button("copy-about")
                    .outline()
                    .label("Copy build information")
                    .on_click(cx.listener(|v, _, _, cx| {
                        copy_acknowledged(v.support_info(), cx);
                    })),
            );
        }
        if matches!(self.surface.as_str(), "History" | "Checkpoints")
            && !self.tabs[self.active].parents.is_empty()
        {
            body = body.child(
                action_button("back")
                    .ghost()
                    .small()
                    .w(px(100.))
                    .label("Back")
                    .hint("Back to the list · Alt+Left")
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
                action_button("technical-details")
                    .ghost()
                    .label("Copy technical details")
                    .on_click(cx.listener(move |_, _, _, cx| {
                        copy_acknowledged(detail.to_string(), cx);
                    })),
            );
        }
        if self.surface == "Diagnostics" {
            if let Some(path) = self.tabs[self.active]
                .inspections
                .get("Export result")
                .and_then(|value| value["path"].as_str())
                .map(str::to_owned)
            {
                body = body.child(
                    section("Export completed", cx)
                        .child("The exported file is saved.")
                        .child(
                            action_button("copy-export-location")
                                .ghost()
                                .label("Copy export location")
                                .on_click(cx.listener(move |_, _, _, cx| {
                                    copy_acknowledged(path.clone(), cx)
                                })),
                        ),
                );
            }
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
                                action_button(("cancel-operation", index))
                                    .small()
                                    .outline()
                                    .label("Cancel operation")
                                    .unavailable(!cancelable)
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
                    action_button(method)
                        .small()
                        .outline()
                        .icon(glyph)
                        .label(label)
                        .on_click(cx.listener(move |v, _, _, _| v.inspect(method, json!({})))),
                );
            }
            tools = tools
                .child(
                    action_button("refresh")
                        .small()
                        .outline()
                        .icon(Glyph::RefreshCw)
                        .label("Reconnect / reload")
                        .on_click(cx.listener(|v, _, _, _| v.send(Command::Refresh))),
                )
                .child(
                    action_button("migrate")
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
        if self.surface == "Checkpoints" {
            body = body.child(
                action_button("create-checkpoint")
                    .outline()
                    .label("Create checkpoint")
                    .unavailable(
                        action_reason("checkpoint", &self.tabs[self.active].runtime).is_some(),
                    )
                    .hint(
                        action_reason("checkpoint", &self.tabs[self.active].runtime)
                            .unwrap_or("Save supported values from the live namespace"),
                    )
                    .on_click(
                        cx.listener(|v, _, window, cx| v.run_command("checkpoint", window, cx)),
                    ),
            );
            body = body.child(h_flex().gap_2()
                .child(action_button("namespace-list").ghost().label("Namespace checkpoints").on_click(cx.listener(|v, _, _, _| v.inspect("checkpoints.list", json!({})))))
                .child(action_button("training-list").ghost().label("Training snapshots").on_click(cx.listener(|v, _, _, _| v.inspect("training.list", json!({})))))
                .child(div().whitespace_normal().child("Training snapshots save registered model/optimizer state. Namespace and native process state are excluded.")));
            if self.tabs[self.active].runtime["capabilities"]
                .as_array()
                .is_some_and(|caps| caps.iter().any(|cap| cap == "portable_training"))
            {
                body = body.child(
                    action_button("create-training-checkpoint")
                        .ghost()
                        .label("Create training checkpoint (Experimental)")
                        .unavailable(self.command_reason("training-checkpoint").is_some())
                        .hint(self.command_reason("training-checkpoint").unwrap_or(
                            "Save explicitly registered model, optimizer and cursor state",
                        ))
                        .on_click(cx.listener(|v, _, window, cx| {
                            v.run_command("training-checkpoint", window, cx)
                        })),
                );
            }
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
                        action_button("restore-training")
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
            body = body.child(self.history_filters(cx));
            body = body.child(Input::new(&self.history_query).cleanable(true));
            body = body.child(
                h_flex()
                    .gap_2()
                    .flex_wrap()
                    .child(
                        action_button("search-executions")
                            .outline()
                            .label("Search executions")
                            .on_click(cx.listener(|v, _, _, _| {
                                v.inspect("executions.list", json!({"search_all":true}))
                            })),
                    )
                    .child(
                        action_button("clear-history-filters")
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
            if items.is_empty() && !history_filter.is_empty() {
                rows = rows.child(empty_row("No records on this page match these filters. Clear filters or inspect another page.", cx));
            }
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
                            action_button(("training-inspect", i))
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
                            action_button(("execution", i))
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
                            action_button(("report", i))
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
                            action_button(("continue", i))
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
                            action_button(("restore", i))
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
                            action_button(("session", i))
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
                            action_button(("replay", i))
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
                                action_button(("python", i))
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
                            action_button(("runtime-stop", i))
                                .outline()
                                .label("Stop runtime")
                                .unavailable(status != "verified")
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
                            action_button(("runtime-open", i))
                                .outline()
                                .label("Open notebook")
                                .unavailable(item["project_path"].as_str().is_none())
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
            body = body.child(rows);
            if !value["cursor"].is_null()
                && matches!(self.surface.as_str(), "History" | "Checkpoints")
            {
                let method = self.tabs[self.active]
                    .inspection_method
                    .get(&self.surface)
                    .cloned()
                    .unwrap_or_default();
                let mut fields = value["_page_request"]
                    .as_object()
                    .cloned()
                    .unwrap_or_default();
                fields.insert("cursor".into(), value["cursor"].clone());
                body = body.child(
                    action_button("next-record-page")
                        .ghost()
                        .label("Next page")
                        .on_click(cx.listener(move |v, _, _, _| {
                            v.inspect(&method, Value::Object(fields.clone()))
                        })),
                );
            }
        }
        if self.surface == "Checkpoints" && value.get("checkpoint_id").is_some() {
            body = body.child(self.checkpoint_report(&value, cx));
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
                body = body.child(section("Recorded source", cx));
                body = body.child(
                    TextView::markdown(
                        "recorded-source",
                        format!("```python\n{}\n```", bounded(source, 65536)),
                    )
                    .selectable(true),
                );
                if let Some(current) = value["cell_id"]
                    .as_str()
                    .and_then(|id| self.tabs[self.active].model.cells.get(id))
                {
                    body = body.child(
                        section(
                            if current.text == source {
                                "Current source — unchanged"
                            } else {
                                "Current source — edited since this execution"
                            },
                            cx,
                        )
                        .child(
                            TextView::markdown(
                                "current-source",
                                format!("```python\n{}\n```", bounded(&current.text, 65536)),
                            )
                            .selectable(true),
                        ),
                    );
                } else {
                    body = body
                        .child("The recorded cell is no longer present in the current notebook.");
                }
            }
            if let Some(outputs) = value["outputs"].as_array() {
                for (i, event) in outputs.iter().enumerate() {
                    body = body.child(output(event, format!("recorded-output-{i}"), cx));
                }
            }
        }
        if self.surface == "Diagnostics" {
            body = body.child(self.diagnostic_findings(&value, cx));
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
                action_button("raw-details")
                    .ghost()
                    .small()
                    .w(px(170.))
                    .label(
                        if self.tabs[self.active]
                            .raw_details
                            .get(&self.surface)
                            .copied()
                            .unwrap_or(false)
                        {
                            "Hide details"
                        } else {
                            "Technical details"
                        },
                    )
                    .on_click(cx.listener(|v, _, _, cx| {
                        let expanded = v.tabs[v.active]
                            .raw_details
                            .entry(v.surface.clone())
                            .or_default();
                        *expanded = !*expanded;
                        cx.notify();
                    })),
            );
        }
        if !value.is_null()
            && self.tabs[self.active]
                .raw_details
                .get(&self.surface)
                .copied()
                .unwrap_or(false)
        {
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
                    .px_4()
                    .py_4()
                    .child(body),
            )
            .into_any_element()
    }
}
