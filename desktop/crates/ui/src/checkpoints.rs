use super::*;
use kernelledger_models::presentation::{checkpoint_status, coverage_note, exclusion_reason};

/// Reason codes are lowercase identifiers; anything else is a runtime message.
fn is_code(text: &str) -> bool {
    !text.is_empty() && text.chars().all(|c| c.is_ascii_lowercase() || c == '_')
}

/// One excluded value or coverage note in readable form. Known codes are explained;
/// unknown codes and messages stay visible as reported, and the raw entry remains in
/// Technical details.
fn checkpoint_entry(field: &str, index: usize, item: &Value, cx: &App) -> Div {
    let muted = cx.theme().muted_foreground;
    let name = item["path"].as_str().or(item["name"].as_str());
    let python_type = item["type"]
        .as_str()
        .map(|t| t.strip_prefix("builtins.").unwrap_or(t))
        .filter(|t| *t != "type");
    let (explanation, code) = if field == "excluded" {
        let reason = item["reason"]
            .as_str()
            .or(item["message"].as_str())
            .or(item.as_str())
            .unwrap_or("");
        match exclusion_reason(reason) {
            Some(text) => (text.to_string(), Some(format!("Reason code: {reason}"))),
            None if is_code(reason) => (
                "Not saved. This build has no description for this reason.".to_string(),
                Some(format!("Reason code: {reason}")),
            ),
            None if reason.is_empty() => (
                "Not saved. No reason was reported; see Technical details.".to_string(),
                None,
            ),
            None => (format!("Not saved: {}", bounded(reason, 1024)), None),
        }
    } else if let Some(text) = item.as_str() {
        (bounded(text, 1024), None)
    } else {
        let kind = item["kind"].as_str().unwrap_or("");
        match coverage_note(kind) {
            Some(text) => (text.to_string(), Some(format!("Note: {kind}"))),
            None if !kind.is_empty() => (
                "This build has no description for this note. The full note is in Technical details."
                    .to_string(),
                Some(format!("Note: {kind}")),
            ),
            None => ("The full note is in Technical details.".to_string(), None),
        }
    };
    let number = format!("{}.", index + 1);
    v_flex()
        .gap_0p5()
        .child(
            h_flex()
                .gap_2()
                .items_baseline()
                .flex_wrap()
                .child(
                    div()
                        .text_sm()
                        .font_weight(FontWeight::MEDIUM)
                        .child(match name {
                            Some(name) => format!("{number} {name}"),
                            None => number,
                        }),
                )
                .when_some(python_type, |d, t| {
                    d.child(
                        div()
                            .text_xs()
                            .text_color(muted)
                            .child(format!("Type: {t}")),
                    )
                }),
        )
        .child(div().text_sm().whitespace_normal().child(explanation))
        .when_some(code, |d, code| {
            d.child(
                div()
                    .text_xs()
                    .text_color(muted)
                    .font_family(cx.theme().mono_font_family.clone())
                    .child(code),
            )
        })
}

impl Workspace {
    #[inline(never)]
    pub(super) fn checkpoint_page(&mut self, value: &Value, cx: &mut Context<Self>) -> AnyElement {
        let mut body = v_flex().gap_4().child(
            h_flex()
                .gap_3()
                .items_center()
                .child(page_heading("Checkpoints", "", cx))
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
            if items.is_empty() {
                body = body.child(empty_row(
                    if value.is_null() {
                        "Loading checkpoints\u{2026}"
                    } else {
                        "No checkpoints yet. Run your notebook, then create a checkpoint."
                    },
                    cx,
                ));
            }
            let mut rows = card(cx);
            for (i, item) in items.iter().enumerate() {
                let id = item["checkpoint_id"]
                    .as_str()
                    .unwrap_or_default()
                    .to_string();
                let ai_id = id.clone();
                rows = rows.child(
                    record_row(
                        Glyph::Flag,
                        &record_label(item, &self.tabs[self.active].model),
                        item["status"].as_str().unwrap_or("Unknown"),
                        i == self.record_active,
                        cx,
                    )
                    .child(
                        action_button(("checkpoint-ai", i))
                            .ghost()
                            .small()
                            .icon(Glyph::Sparkles)
                            .accessibility_label("Explain checkpoint in Assistant")
                            .hint("Ask Assistant about this checkpoint")
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
                        action_button(("report", i))
                            .outline()
                            .small()
                            .label("Inspect")
                            .accessibility_label("Inspect checkpoint")
                            .on_click(cx.listener(move |v, _, _, _| {
                                v.inspect("checkpoint.get", json!({"checkpoint_id":id}))
                            })),
                    ),
                );
            }
            if !items.is_empty() {
                body = body.child(rows);
            }
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
                    "Hide advanced"
                } else {
                    "Advanced checkpoints"
                })
                .accessibility_label(if advanced {
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
                card(cx)
                    .p_4()
                    .gap_3()
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
                                    .child(
                                        div()
                                            .text_sm()
                                            .font_weight(FontWeight::SEMIBOLD)
                                            .child("Portable training"),
                                    )
                                    .child(badge("Experimental", cx.theme().warning))
                                    .child(info_tip(
                                        "portable-training-tip",
                                        "Saves explicitly registered model, optimizer, and cursor state. Does not capture a complete OS process.",
                                        cx,
                                    )),
                            )
                            .child(
                                h_flex()
                                    .gap_2()
                                    .flex_wrap()
                                    .child(
                                        action_button("training-list")
                                            .outline()
                                            .small()
                                            .label("Training snapshots")
                                            .accessibility_label("View training snapshots")
                                            .on_click(cx.listener(|v, _, _, _| {
                                                v.inspect("training.list", json!({}))
                                            })),
                                    )
                                    .child(
                                        action_button("create-training-checkpoint")
                                            .ghost()
                                            .small()
                                            .label("Save snapshot")
                                            .accessibility_label("Create training checkpoint")
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

    #[inline(never)]
    pub(super) fn checkpoint_report(&self, value: &Value, cx: &mut Context<Self>) -> Div {
        let id = value["checkpoint_id"]
            .as_str()
            .unwrap_or_default()
            .to_string();
        let muted = cx.theme().muted_foreground;
        let status = value["status"].as_str().unwrap_or("");
        let status_text = match checkpoint_status(status) {
            Some(text) => text.to_string(),
            None if status.is_empty() => "This checkpoint does not report a status.".into(),
            None => format!(
                "This build has no description for the status \u{201c}{status}\u{201d}. Review the excluded values before restoring."
            ),
        };
        let mut report = card(cx)
            .p_4()
            .gap_3p5()
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
                            .child(
                                div()
                                    .text_base()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child("Namespace checkpoint"),
                            )
                            .child(badge(
                                outcome_label(if status.is_empty() { "Unknown" } else { status })
                                    .to_string(),
                                tone(status, cx),
                            )),
                    )
                    .when_some(value["timestamp"].as_str(), |d, time| {
                        d.child(
                            div()
                                .text_xs()
                                .text_color(muted)
                                .child(format!("Created {}", local_time(time))),
                        )
                    }),
            )
            .child(
                v_flex()
                    .id("checkpoint-status")
                    .gap_1()
                    .child(div().text_sm().whitespace_normal().child(status_text))
                    .child(
                        div()
                            .text_xs()
                            .text_color(muted)
                            .whitespace_normal()
                            .child("Checkpoints save supported Python values only. Processes, open files, and external services are never captured."),
                    ),
            )
            .child(
                h_flex()
                    .gap_2()
                    .items_center()
                    .flex_wrap()
                    .child(
                        div()
                            .text_xs()
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(muted)
                            .child("Saved values:"),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(muted)
                            .whitespace_normal()
                            .child("Count not reported by this checkpoint (unknown, not zero)"),
                    ),
            );
        for (field, title, empty) in [
            ("excluded", "Excluded values", "No values were excluded."),
            ("notes", "Coverage notes", "No coverage notes."),
        ] {
            let items = value.get(field).and_then(Value::as_array);
            let mut content = section(
                &match items {
                    Some(items) if !items.is_empty() => format!("{title} ({})", items.len()),
                    _ => title.to_string(),
                },
                cx,
            );
            match items {
                Some(items) if items.is_empty() => {
                    content = content.child(div().text_xs().text_color(muted).child(empty));
                }
                Some(items) => {
                    for (i, item) in items.iter().enumerate() {
                        content = content.child(checkpoint_entry(field, i, item, cx));
                    }
                }
                None => {
                    content = content.child(
                        div()
                            .text_xs()
                            .text_color(muted)
                            .child("Not reported by this checkpoint."),
                    );
                }
            }
            report = report.child(content);
        }
        let mut choices = v_flex()
            .gap_2()
            .pt_3()
            .border_t_1()
            .border_color(divider(cx));
        for (method, label, description) in [
            (
                "restore",
                "Restore checkpoint\u{2026}",
                "Start a new session with the saved supported values. This replaces the live namespace; excluded values stay absent.",
            ),
            (
                "continue",
                "Continue in new session\u{2026}",
                "Restore the saved values, then replay the executions recorded after this checkpoint. Replayed code may repeat file and network side effects.",
            ),
        ] {
            let id = id.clone();
            choices = choices.child(
                h_flex()
                    .gap_3()
                    .items_center()
                    .flex_wrap()
                    .child(
                        action_button(SharedString::from(format!("report-{method}")))
                            .outline()
                            .small()
                            .label(label)
                            .hint(description)
                            .on_click(cx.listener(move |v, _, _, cx| {
                                v.confirmation = Some((method.into(), json!({"checkpoint_id":id})));
                                cx.notify();
                            })),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w(rems(14.))
                            .text_xs()
                            .text_color(muted)
                            .whitespace_normal()
                            .child(description),
                    ),
            );
        }
        report = report.child(choices);
        let mut actions = h_flex().gap_2().flex_wrap().items_center();
        let ai_id = id.clone();
        actions = actions
            .child(
                action_button("checkpoint-ai-context")
                    .outline()
                    .small()
                    .icon(Glyph::Sparkles)
                    .label("Ask Assistant")
                    .accessibility_label("Explain this checkpoint in Assistant")
                    .hint("Explain coverage, exclusions, and recovery options in Assistant")
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
                    .label("Copy ID")
                    .accessibility_label("Copy checkpoint identifier")
                    .hint("Copy checkpoint identifier")
                    .on_click(cx.listener(move |_, _, _, cx| copy_acknowledged(id.clone(), cx))),
            );
        report.child(actions)
    }
}
