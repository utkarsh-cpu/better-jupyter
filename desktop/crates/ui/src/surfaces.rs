use super::*;

impl Workspace {
    pub(super) fn panel(&mut self, cx: &mut Context<Self>) -> AnyElement {
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
        let mut body = v_flex().w_full().min_w_0().gap_4().child(
            h_flex().gap_3().items_center().child(
                v_flex()
                    .gap_0p5()
                    .child(
                        div()
                            .text_lg()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(self.surface.clone()),
                    )
                    .child(div().text_sm().text_color(muted_fg).child(
                        description.to_string(),
                    )),
            ),
        );
        if self.surface == "Help" {
            body = body.child(
                Button::new("about")
                    .outline()
                    .label("About KernelLedger")
                    .on_click(cx.listener(|v, _, _, cx| {
                        v.confirmation = Some(("about".into(), json!({})));
                        cx.notify();
                    })),
            );
            body = body.child(self.help_content(cx));
            body = body.child(
                Button::new("copy-about")
                    .outline()
                    .label("Copy build information")
                    .on_click(cx.listener(|v, _, _, cx| {
                        cx.write_to_clipboard(ClipboardItem::new_string(v.support_info()));
                    })),
            );
        }
        if matches!(self.surface.as_str(), "History" | "Checkpoints")
            && !self.tabs[self.active].parents.is_empty()
        {
            body = body.child(
                Button::new("back")
                    .outline()
                    .label("Back | Alt+Left")
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
                Button::new("technical-details")
                    .ghost()
                    .label("Copy technical details")
                    .on_click(cx.listener(move |_, _, _, cx| {
                        cx.write_to_clipboard(ClipboardItem::new_string(detail.to_string()));
                    })),
            );
        }
        if self.surface == "Diagnostics" {
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
                                Button::new(("cancel-operation", index))
                                    .small()
                                    .outline()
                                    .label("Cancel operation")
                                    .disabled(!cancelable)
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
                    Button::new(method)
                        .small()
                        .outline()
                        .icon(glyph)
                        .label(label)
                        .on_click(cx.listener(move |v, _, _, _| v.inspect(method, json!({})))),
                );
            }
            tools = tools
                .child(
                    Button::new("refresh")
                        .small()
                        .outline()
                        .icon(Glyph::RefreshCw)
                        .label("Reconnect / reload")
                        .on_click(cx.listener(|v, _, _, _| v.send(Command::Refresh))),
                )
                .child(
                    Button::new("migrate")
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
        if self.surface == "Settings" {
            body = body.child(self.settings_controls(cx));
            let dark = self.dark;
            body = body.child(
                section("Preferences", cx).child(
                    card(cx)
                        .child(
                            row(cx)
                                .child(setting_label(
                                    "Appearance",
                                    if self.high_contrast {
                                        "High contrast theme"
                                    } else if dark {
                                        "Dark theme"
                                    } else {
                                        "Light theme"
                                    },
                                    cx,
                                ))
                                .child(
                                    Button::new("theme")
                                        .small()
                                        .outline()
                                        .icon(if dark { Glyph::Sun } else { Glyph::Moon })
                                        .label("Toggle light / dark")
                                        .on_click(cx.listener(|v, _, window, cx| {
                                            v.toggle_theme(window, cx)
                                        })),
                                ),
                        )
                        .child(
                            row(cx)
                                .child(setting_label(
                                    "Project interpreter",
                                    "CPython 3.11 or newer runs this notebook's worker",
                                    cx,
                                ))
                                .child(
                                    Button::new("interpreter")
                                        .small()
                                        .outline()
                                        .icon(Glyph::FolderOpen)
                                        .label("Choose project interpreter")
                                        .on_click(cx.listener(|v, _, window, cx| {
                                            v.choose_interpreter(window, cx)
                                        })),
                                ),
                        ),
                ),
            );
        }
        if self.surface == "Settings" {
            body = body.child(
                Button::new("high-contrast")
                    .outline()
                    .label("High contrast")
                    .on_click(cx.listener(|v, _, window, cx| {
                        v.follow_system = false;
                        v.high_contrast = true;
                        persist_appearance("contrast", cx);
                        apply_theme(v.dark, true, window, cx);
                        cx.notify();
                    })),
            );
        }
        if self.surface == "Settings" {
            body = body.child(
                Checkbox::new("system-theme")
                    .label("Follow system appearance")
                    .checked(self.follow_system)
                    .on_click(cx.listener(|v, checked: &bool, window, cx| {
                        v.follow_system = *checked;
                        v.high_contrast = *checked && system_high_contrast();
                        if *checked {
                            v.dark = matches!(
                                window.appearance(),
                                WindowAppearance::Dark | WindowAppearance::VibrantDark
                            );
                        }
                        persist_appearance(
                            if *checked {
                                "system"
                            } else if v.dark {
                                "dark"
                            } else {
                                "light"
                            },
                            cx,
                        );
                        apply_theme(v.dark, v.high_contrast, window, cx);
                        cx.notify();
                    })),
            );
            let interpreter = &self.tabs[self.active].runtime["interpreter"];
            let summary = format!(
                "Interpreter: {} | {}",
                interpreter["executable"]
                    .as_str()
                    .or(interpreter["python"].as_str())
                    .or(value["path"].as_str())
                    .unwrap_or("Not selected; choose a project interpreter"),
                interpreter["version"]
                    .as_str()
                    .unwrap_or("version available after start")
            );
            body = body.child(div().whitespace_normal().child(summary));
        }
        if self.surface == "Checkpoints" {
            body = body.child(
                Button::new("create-checkpoint")
                    .outline()
                    .label("Create checkpoint")
                    .disabled(
                        action_reason("checkpoint", &self.tabs[self.active].runtime).is_some(),
                    )
                    .tooltip(
                        action_reason("checkpoint", &self.tabs[self.active].runtime)
                            .unwrap_or("Save supported values from the live namespace"),
                    )
                    .on_click(
                        cx.listener(|v, _, window, cx| v.run_command("checkpoint", window, cx)),
                    ),
            );
            body = body.child(h_flex().gap_2()
                .child(Button::new("namespace-list").ghost().label("Namespace checkpoints").on_click(cx.listener(|v, _, _, _| v.inspect("checkpoints.list", json!({})))))
                .child(Button::new("training-list").ghost().label("Training snapshots").on_click(cx.listener(|v, _, _, _| v.inspect("training.list", json!({})))))
                .child(div().whitespace_normal().child("Training snapshots save registered model/optimizer state. Namespace and native process state are excluded.")));
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
                        Button::new("restore-training")
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
                        Button::new("search-executions")
                            .outline()
                            .label("Search executions")
                            .on_click(cx.listener(|v, _, _, _| {
                                v.inspect("executions.list", json!({"search_all":true}))
                            })),
                    )
                    .child(
                        Button::new("clear-history-filters")
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
                rows = rows.child(empty_row("No executions match these filters. Clear filters to see all recorded executions.", cx));
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
                            Button::new(("training-inspect", i))
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
                            Button::new(("execution", i))
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
                            Button::new(("report", i))
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
                            Button::new(("continue", i))
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
                            Button::new(("restore", i))
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
                            Button::new(("session", i))
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
                            Button::new(("replay", i))
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
                                Button::new(("python", i))
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
                            Button::new(("runtime-stop", i))
                                .outline()
                                .label("Stop runtime")
                                .disabled(status != "verified")
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
                            Button::new(("runtime-open", i))
                                .outline()
                                .label("Open notebook")
                                .disabled(item["project_path"].as_str().is_none())
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
        if self.surface == "Diagnostics" && !value.is_null() {
            body = body.child(section("Doctor findings", cx).child(
                div().whitespace_normal().child(format!(
                    "Source: {} | {} findings",
                    value["source"]["state"].as_str().unwrap_or("unknown"),
                    value["issues"].as_array().map(Vec::len).unwrap_or(0)
                )),
            ));
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
                Button::new("raw-details")
                    .ghost()
                    .label(if self.tabs[self.active].raw_details {
                        "Hide raw details"
                    } else {
                        "Raw details"
                    })
                    .on_click(cx.listener(|v, _, _, cx| {
                        v.tabs[v.active].raw_details = !v.tabs[v.active].raw_details;
                        cx.notify();
                    })),
            );
        }
        if !value.is_null() && self.tabs[self.active].raw_details {
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
