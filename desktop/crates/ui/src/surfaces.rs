use super::*;

impl Workspace {
    #[inline(never)]
    pub(super) fn panel(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        if self.surface == "Recorded source" {
            return self.recorded_source_page(cx);
        }
        if matches!(
            self.surface.as_str(),
            "Variables" | "Console" | "Debugger" | "Source preview"
        ) {
            return self.interactive_page(window, cx);
        }
        let value = self.tabs[self.active]
            .inspections
            .get(&self.surface)
            .cloned()
            .unwrap_or(Value::Null);
        if self.surface == "Settings" {
            return self.settings_page(&value, window, cx);
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

    #[inline(never)]
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
        let mut header_row = h_flex()
            .gap_3()
            .flex_wrap()
            .items_center()
            .child(page_heading(
                self.surface.clone(),
                if self.surface == "Help" {
                    ""
                } else {
                    description
                },
                cx,
            ));
        if self.surface == "Help" {
            header_row = header_row.child(
                h_flex()
                    .gap_2()
                    .items_center()
                    .child(
                        action_button("about")
                            .outline()
                            .small()
                            .label("About")
                            .accessibility_label("About KernelLedger")
                            .on_click(cx.listener(|v, _, _, cx| {
                                v.confirmation = Some(("about".into(), json!({})));
                                cx.notify();
                            })),
                    )
                    .child(
                        action_button("copy-about")
                            .outline()
                            .small()
                            .label("Copy build info")
                            .accessibility_label("Copy build information")
                            .on_click(cx.listener(|v, _, _, cx| {
                                copy_acknowledged(v.support_info(), cx);
                            })),
                    ),
            );
        }
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
            .child(header_row);
        if self.surface == "Help" {
            body = body.child(self.help_content(cx));
        }
        if matches!(self.surface.as_str(), "History" | "Checkpoints")
            && !self.tabs[self.active].parents.is_empty()
        {
            body = body.child(
                action_button("back")
                    .self_start()
                    .ghost()
                    .small()
                    .label("Back")
                    .accessibility_label("Back to list")
                    .hint("Back to the list (Alt+Left)")
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
                    .self_start()
                    .outline()
                    .small()
                    .label("Copy technical details")
                    .on_click(cx.listener(move |_, _, _, cx| {
                        copy_acknowledged(detail.to_string(), cx);
                    })),
            );
        }
        if self.surface == "Diagnostics" {
            body = body.child(self.supporting_diagnostics_header(cx));
        }
        if self.surface == "Checkpoints" {
            body = body.child(
                h_flex()
                    .gap_2()
                    .flex_wrap()
                    .items_center()
                    .child(
                        action_button("create-checkpoint")
                            .primary()
                            .small()
                            .icon(Glyph::Plus)
                            .label("Create checkpoint")
                            .unavailable(
                                action_reason("checkpoint", &self.tabs[self.active].runtime)
                                    .is_some(),
                            )
                            .hint(
                                action_reason("checkpoint", &self.tabs[self.active].runtime)
                                    .unwrap_or("Save supported values from the live namespace"),
                            )
                            .on_click(cx.listener(|v, _, window, cx| {
                                v.run_command("checkpoint", window, cx)
                            })),
                    )
                    .child(
                        action_button("namespace-list")
                            .ghost()
                            .small()
                            .label("Namespace checkpoints")
                            .on_click(
                                cx.listener(|v, _, _, _| v.inspect("checkpoints.list", json!({}))),
                            ),
                    )
                    .child(
                        action_button("training-list")
                            .ghost()
                            .small()
                            .label("Training snapshots")
                            .on_click(
                                cx.listener(|v, _, _, _| v.inspect("training.list", json!({}))),
                            ),
                    )
                    .child(info_tip(
                        "training-snapshots-tip",
                        "Training snapshots save registered model and optimizer state. Namespace and native process state are excluded.",
                        cx,
                    )),
            );
            if self.tabs[self.active].runtime["capabilities"]
                .as_array()
                .is_some_and(|caps| caps.iter().any(|cap| cap == "portable_training"))
            {
                body = body.child(
                    action_button("create-training-checkpoint")
                        .self_start()
                        .outline()
                        .small()
                        .label("Create training checkpoint")
                        .accessibility_label("Create training checkpoint (Experimental)")
                        .unavailable(self.command_reason("training-checkpoint").is_some())
                        .hint(self.command_reason("training-checkpoint").unwrap_or(
                            "Save explicitly registered model, optimizer and cursor state (Experimental)",
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
                section("Portable training coverage", cx).child(
                    card(cx)
                        .p_4()
                        .gap_2()
                        .child(
                            div()
                                .text_sm()
                                .whitespace_normal()
                                .child(bounded(&value["coverage"].to_string(), 4096)),
                        )
                        .child(
                            div()
                                .text_sm()
                                .text_color(muted_fg)
                                .whitespace_normal()
                                .child(format!(
                                    "Source coherence: {}",
                                    value["source_coherence"].as_str().unwrap_or("unverified")
                                )),
                        )
                        .child(
                            action_button("restore-training")
                                .self_start()
                                .outline()
                                .small()
                                .label("Restore training")
                                .accessibility_label("Restore training in fresh worker")
                                .hint("Restore training snapshot in a fresh worker process")
                                .on_click(cx.listener(move |v, _, _, cx| {
                                    v.confirmation = Some((
                                        "training-restore".into(),
                                        json!({"snapshot":snapshot,"device":device,"trust":false}),
                                    ));
                                    cx.notify();
                                })),
                        ),
                ),
            );
        }
        if value.get("native_qualified").is_some() && value.get("capabilities").is_some() {
            body = body.child(
                section("Native process snapshot availability", cx).child(card(cx).p_4().child(
                    div().text_sm().whitespace_normal().child(bounded(
                        &value["capabilities"]["features"].to_string(),
                        8192,
                    )),
                )),
            );
        }
        if let Some(items) = value["items"].as_array() {
            body = body.child(self.supporting_items(&value, items, cx));
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
                    .text_xs()
                    .text_color(muted_fg)
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
                                "Current source \u{2014} unchanged"
                            } else {
                                "Current source \u{2014} edited since this execution"
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
                    body = body.child("Cell removed from current notebook.");
                }
            }
            if let Some(outputs) = value["outputs"].as_array() {
                for (i, event) in outputs.iter().enumerate() {
                    body = body.child(output(
                        &self.tabs[self.active].service.commands,
                        event,
                        format!("recorded-output-{i}"),
                        value["cell_id"].as_str().unwrap_or_default(),
                        value["exec_id"].as_str().unwrap_or_default(),
                        i,
                    ));
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
                    .self_start()
                    .ghost()
                    .small()
                    .label(
                        if self.tabs[self.active]
                            .raw_details
                            .get(&self.surface)
                            .copied()
                            .unwrap_or(false)
                        {
                            "Hide JSON"
                        } else {
                            "Show JSON"
                        },
                    )
                    .accessibility_label(
                        if self.tabs[self.active]
                            .raw_details
                            .get(&self.surface)
                            .copied()
                            .unwrap_or(false)
                        {
                            "Hide technical details"
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
                    .py_5()
                    .child(body),
            )
            .into_any_element()
    }

    #[inline(never)]
    fn supporting_diagnostics_header(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = cx.theme();
        let muted_fg = theme.muted_foreground;
        let mono = theme.mono_font_family.clone();
        let mut body = v_flex().gap_4();
        if let Some(path) = self.tabs[self.active]
            .inspections
            .get("Export result")
            .and_then(|value| value["path"].as_str())
            .map(str::to_owned)
        {
            body = body.child(
                section("Export completed", cx).child(
                    card(cx).p_3().gap_2().child("Export saved.").child(
                        action_button("copy-export-location")
                            .self_start()
                            .outline()
                            .small()
                            .label("Copy path")
                            .accessibility_label("Copy export location")
                            .on_click(
                                cx.listener(move |_, _, _, cx| copy_acknowledged(path.clone(), cx)),
                            ),
                    ),
                ),
            );
        }
        if let Some(review) = self.bundle_review(cx) {
            body = body.child(review);
        }
        let operations = self.tabs[self.active].runtime["active_operations"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        if operations.is_empty() {
            body = body.child(
                section("Active operations", cx).child(
                    div()
                        .text_sm()
                        .text_color(muted_fg)
                        .child("No operations in flight"),
                ),
            );
        } else {
            let mut rows = card(cx);
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
                                    .label("Cancel")
                                    .accessibility_label("Cancel operation")
                                    .unavailable(!cancelable)
                                    .on_click(cx.listener(move |v, _, _, _| {
                                        v.runtime("operation.cancel", json!({"operation_id":oid}));
                                    })),
                            ),
                    );
            }
            body = body.child(section("Active operations", cx).child(rows));
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
                    .label("Migrate ledger")
                    .accessibility_label("Migrate ledger with backup")
                    .hint("Migrate the ledger schema after creating a backup")
                    .on_click(cx.listener(|v, _, _, cx| {
                        v.confirmation = Some(("migrate".into(), json!({})));
                        cx.notify();
                    })),
            );
        body = body.child(section("Tools", cx).child(tools));
        body.into_any_element()
    }

    #[inline(never)]
    fn supporting_items(
        &mut self,
        value: &Value,
        items: &[Value],
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = cx.theme();
        let muted_fg = theme.muted_foreground;
        let mono = theme.mono_font_family.clone();
        let mut body = v_flex().gap_4();
        let history_filter = self.tabs[self.active].history_applied.to_lowercase();
        let source_empty = items.is_empty();
        let items = items
            .iter()
            .filter(|item| {
                self.surface != "History"
                    || history_matches(item, &history_filter, &self.tabs[self.active].model)
            })
            .collect::<Vec<_>>();
        let empty = items.is_empty();
        if empty {
            body = body.child(empty_row(
                if source_empty {
                    match self.surface.as_str() {
                        "Runtimes" => "No runtimes attached. Run a cell to start one.",
                        "History" => {
                            "No recorded history yet. Run a code cell to record executions."
                        }
                        "Checkpoints" => {
                            "No checkpoints yet. Run your notebook, then create a checkpoint."
                        }
                        _ => "Nothing recorded yet",
                    }
                } else {
                    "No matching records on this page."
                },
                cx,
            ));
        }
        let mut rows = card(cx);
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
                        false,
                        cx,
                    )
                    .child(
                        action_button(("training-inspect", i))
                            .small()
                            .outline()
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
                        i == self.record_active,
                        cx,
                    )
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
                        i == self.record_active,
                        cx,
                    )
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
                        i == self.record_active,
                        cx,
                    )
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
                            .label("Replay")
                            .accessibility_label("Replay session")
                            .on_click(cx.listener(move |v, _, _, cx| {
                                v.confirmation = Some(("replay".into(), json!({"session_id":sid})));
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
                let _ = label;
                let path_for_open = item["project_path"].as_str().map(PathBuf::from);
                let path_for_stop = path_for_open.clone();
                rows = rows.child(
                    record_row(
                        Glyph::Cpu,
                        &record_label(item, &self.tabs[self.active].model),
                        &status,
                        i == self.record_active,
                        cx,
                    )
                    .child(
                        action_button(("runtime-open", i))
                            .small()
                            .outline()
                            .label("Open")
                            .accessibility_label("Open notebook")
                            .unavailable(path_for_open.is_none())
                            .on_click(cx.listener(move |v, _, _, cx| {
                                if let Some(path) = path_for_open.clone() {
                                    v.open(path, cx);
                                    v.surface = "Notebook".into();
                                }
                            })),
                    )
                    .child(
                        action_button(("runtime-stop", i))
                            .small()
                            .outline()
                            .label("Stop")
                            .accessibility_label("Stop runtime")
                            .unavailable(status != "verified")
                            .on_click(cx.listener(move |v, _, _, cx| {
                                if let Some(path) = path_for_stop.clone() {
                                    v.open(path, cx);
                                    v.confirmation = Some(("stop".into(), json!({})));
                                    cx.notify();
                                }
                            })),
                    ),
                );
            }
        }
        if !empty {
            body = body.child(rows);
        }
        if !value["cursor"].is_null() && matches!(self.surface.as_str(), "History" | "Checkpoints")
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
                    .self_start()
                    .ghost()
                    .small()
                    .label("Next page")
                    .on_click(cx.listener(move |v, _, _, _| {
                        v.inspect(&method, Value::Object(fields.clone()))
                    })),
            );
        }
        body.into_any_element()
    }
}
