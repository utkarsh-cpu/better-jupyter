use super::*;

impl Workspace {
    pub(super) fn checkpoint_page(&mut self, value: &Value, cx: &mut Context<Self>) -> AnyElement {
        let mut body = v_flex().gap_4().child(
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
                                .child("Checkpoints"),
                        )
                        .child(
                            div()
                                .text_sm()
                                .text_color(cx.theme().muted_foreground)
                                .child("Save supported Python values, inspect coverage, then restore or continue in a new session."),
                        ),
                )
                .child(div().flex_1())
                .child(
                    action_button("create-checkpoint")
                        .primary()
                        .small()
                        .icon(Glyph::Plus)
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
                ),
        );
        if value.get("checkpoint_id").is_some() {
            body = body
                .child(self.record_back(cx))
                .child(self.checkpoint_report(value, cx));
        } else {
            let items = value["items"].as_array().cloned().unwrap_or_default();
            let mut rows = card(cx);
            if items.is_empty() {
                rows = rows.child(empty_row(
                    if value.is_null() {
                        "Loading checkpoints\u{2026}"
                    } else {
                        "No checkpoints yet. Run your notebook, then create a checkpoint."
                    },
                    cx,
                ));
            }
            for (i, item) in items.iter().enumerate() {
                let id = item["checkpoint_id"]
                    .as_str()
                    .unwrap_or_default()
                    .to_string();
                rows = rows.child(
                    record_row(
                        Glyph::Flag,
                        &record_label(item, &self.tabs[self.active].model),
                        item["status"].as_str().unwrap_or("Unknown"),
                        cx,
                    )
                    .when(i == self.record_active, |d| d.bg(cx.theme().list_active))
                    .child(
                        action_button(("report", i))
                            .outline()
                            .small()
                            .label("Inspect checkpoint")
                            .on_click(cx.listener(move |v, _, _, _| {
                                v.inspect("checkpoint.get", json!({"checkpoint_id":id}))
                            })),
                    ),
                );
            }
            body = body.child(rows);
            if !value["cursor"].is_null() {
                let mut fields = value["_page_request"]
                    .as_object()
                    .cloned()
                    .unwrap_or_default();
                fields.insert("cursor".into(), value["cursor"].clone());
                body = body.child(
                    action_button("checkpoint-next-page")
                        .self_start()
                        .ghost()
                        .small()
                        .label("Next page")
                        .on_click(cx.listener(move |v, _, _, _| {
                            v.inspect("checkpoints.list", Value::Object(fields.clone()))
                        })),
                );
            }
        }
        let advanced = self.tabs[self.active]
            .raw_details
            .get("advanced-checkpoints")
            .copied()
            .unwrap_or(false);
        body = body.child(
            action_button("advanced-checkpoints")
                .self_start()
                .ghost()
                .small()
                .label(if advanced {
                    "Hide advanced checkpoint types"
                } else {
                    "Advanced checkpoint types"
                })
                .dropdown_caret(true)
                .on_click(cx.listener(|v, _, _, cx| {
                    let flag = v.tabs[v.active]
                        .raw_details
                        .entry("advanced-checkpoints".into())
                        .or_default();
                    *flag = !*flag;
                    cx.notify();
                })),
        );
        if advanced {
            let available = self.command_reason("training-checkpoint").is_none();
            body = body.child(
                section("Portable training (Experimental)", cx).child(
                    card(cx)
                        .p_4()
                        .gap_3()
                        .child(
                            div()
                                .text_sm()
                                .text_color(cx.theme().muted_foreground)
                                .child("Saves explicitly registered model, optimizer and cursor state. It does not capture a complete process."),
                        )
                        .child(
                            h_flex()
                                .gap_2()
                                .flex_wrap()
                                .child(
                                    action_button("training-list")
                                        .outline()
                                        .small()
                                        .label("View training snapshots")
                                        .on_click(cx.listener(|v, _, _, _| {
                                            v.inspect("training.list", json!({}))
                                        })),
                                )
                                .child(
                                    action_button("create-training-checkpoint")
                                        .ghost()
                                        .small()
                                        .label("Create training checkpoint")
                                        .unavailable(!available)
                                        .hint(
                                            self.command_reason("training-checkpoint")
                                                .unwrap_or("Save registered training state"),
                                        )
                                        .on_click(cx.listener(|v, _, window, cx| {
                                            v.run_command("training-checkpoint", window, cx)
                                        })),
                                ),
                        ),
                ),
            );
        }
        body = body.child(self.record_details(value, cx));
        self.record_page(body, cx)
    }

    pub(super) fn checkpoint_report(&self, value: &Value, cx: &mut Context<Self>) -> Div {
        let id = value["checkpoint_id"]
            .as_str()
            .unwrap_or_default()
            .to_string();
        let mut report = card(cx)
            .p_4()
            .gap_4()
            .child(
                v_flex()
                    .gap_1()
                    .child(
                        h_flex()
                            .gap_2()
                            .items_center()
                            .child(
                                div()
                                    .text_base()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child("Namespace checkpoint"),
                            )
                            .child(badge(
                                outcome_label(value["status"].as_str().unwrap_or("Unknown"))
                                    .to_string(),
                                tone(value["status"].as_str().unwrap_or(""), cx),
                            )),
                    )
                    .child(
                        div()
                            .text_sm()
                            .text_color(cx.theme().muted_foreground)
                            .child("Supported Python values are saved. Processes, open files and external services are not captured."),
                    )
                    .when_some(value["timestamp"].as_str(), |d, time| {
                        d.child(
                            div()
                                .text_xs()
                                .text_color(cx.theme().muted_foreground)
                                .child(format!("Created {}", local_time(time))),
                        )
                    }),
            )
            .child(
                section("Saved coverage", cx)
                    .child(div().text_sm().child("Saved value count: Unknown"))
                    .child(
                        div()
                            .text_sm()
                            .text_color(cx.theme().muted_foreground)
                            .child("Creation context is not reported by this checkpoint."),
                    ),
            );
        for (field, title, empty) in [
            ("excluded", "Excluded values", "No excluded values."),
            ("notes", "Coverage notes", "No coverage notes."),
        ] {
            let mut content = section(title, cx);
            match value.get(field).and_then(Value::as_array) {
                Some(items) if items.is_empty() => {
                    content = content.child(
                        div()
                            .text_sm()
                            .text_color(cx.theme().muted_foreground)
                            .child(empty),
                    );
                }
                Some(items) => {
                    for (i, item) in items.iter().enumerate() {
                        let path = item["path"]
                            .as_str()
                            .or(item["name"].as_str())
                            .unwrap_or("Value");
                        let reason = item["reason"]
                            .as_str()
                            .or(item["message"].as_str())
                            .or(item.as_str())
                            .map(str::to_owned)
                            .unwrap_or_else(|| bounded(&item.to_string(), 1024));
                        content = content.child(
                            h_flex()
                                .gap_3()
                                .items_start()
                                .child(
                                    div()
                                        .text_sm()
                                        .font_weight(FontWeight::MEDIUM)
                                        .child(format!("{}. {path}", i + 1)),
                                )
                                .child(div().text_sm().min_w_0().whitespace_normal().child(reason)),
                        );
                    }
                }
                None => {
                    content = content.child(
                        div()
                            .text_sm()
                            .text_color(cx.theme().muted_foreground)
                            .child("Not reported by this checkpoint."),
                    );
                }
            }
            report = report.child(content);
        }
        report = report.child(
            section("Before restoring", cx)
                .child(
                    div()
                        .text_sm()
                        .whitespace_normal()
                        .child("Restore replaces the live namespace with saved supported values in a new session. Excluded values remain absent. Continue also executes later recorded code."),
                )
                .child(
                    div()
                        .text_sm()
                        .text_color(cx.theme().muted_foreground)
                        .whitespace_normal()
                        .child("Artifact provenance, source compatibility and environment compatibility are not established by this report. The trust confirmation remains required."),
                ),
        );
        let mut actions = h_flex().gap_2().flex_wrap().items_center();
        for (method, label) in [
            ("restore", "Restore checkpoint\u{2026}"),
            ("continue", "Continue in new session\u{2026}"),
        ] {
            let id = id.clone();
            actions = actions.child(
                action_button(SharedString::from(format!("report-{method}")))
                    .outline()
                    .small()
                    .label(label)
                    .on_click(cx.listener(move |v, _, _, cx| {
                        v.confirmation = Some((method.into(), json!({"checkpoint_id":id})));
                        cx.notify();
                    })),
            );
        }
        let ai_id = id.clone();
        actions = actions
            .child(
                action_button("checkpoint-ai-context")
                    .outline()
                    .small()
                    .icon(Glyph::MessageCircle)
                    .label("Explain this checkpoint in Assistant")
                    .on_click(cx.listener(move |v, _, _, cx| {
                        let ai = &mut v.tabs[v.active].assistant;
                        ai.checkpoint_ids = vec![ai_id.clone()];
                        ai.open = true;
                        ai.receipt = Value::Null;
                        v.ai_preview(
                            "checkpoint",
                            "Explain this checkpoint's coverage, exclusions and recovery options. Cite the saved evidence.".into(),
                            cx,
                        );
                    })),
            )
            .child(
                action_button("copy-checkpoint-id")
                    .ghost()
                    .small()
                    .label("Copy checkpoint identifier")
                    .on_click(cx.listener(move |_, _, _, cx| copy_acknowledged(id.clone(), cx))),
            );
        report.child(actions)
    }
}
