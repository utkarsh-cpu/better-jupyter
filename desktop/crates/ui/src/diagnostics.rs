use super::*;

impl Workspace {
    #[inline(never)]
    pub(super) fn diagnostic_findings(&self, value: &Value, cx: &mut Context<Self>) -> AnyElement {
        let issues = value["issues"].as_array();
        let mut findings = section("Notebook health check", cx);
        if let Some(draft) = self.tabs[self.active].inspections.get("Recovered draft") {
            let mut comparison = card(cx)
                .p_4()
                .gap_3()
                .child(
                    h_flex()
                        .gap_1p5()
                        .items_center()
                        .child(
                            div()
                                .text_sm()
                                .font_weight(FontWeight::SEMIBOLD)
                                .child("Recovered draft"),
                        )
                        .child(info_tip(
                            "recovered-draft-tip",
                            "Compare this retained draft with source on disk before restoring it.",
                            cx,
                        )),
                )
                .child(
                    TextView::markdown(
                        "recovered-draft-source",
                        format!(
                            "```text\n{}\n```",
                            bounded(
                                draft["source"].as_str().unwrap_or("Source unavailable"),
                                65536
                            )
                            .replace("```", "~~~")
                        ),
                    )
                    .selectable(true),
                );
            if let Some(cell) = draft["cell_id"]
                .as_str()
                .and_then(|id| self.tabs[self.active].model.cells.get(id))
            {
                comparison = comparison.child(
                    section("Source on disk", cx).child(
                        TextView::markdown(
                            "recovered-draft-disk",
                            format!(
                                "```text\n{}\n```",
                                bounded(&cell.disk.source, 65536).replace("```", "~~~")
                            ),
                        )
                        .selectable(true),
                    ),
                );
            }
            findings = findings.child(comparison);
        }
        if value.get("source").is_some() {
            let state = value["source"]["state"].as_str().unwrap_or("Unknown");
            let issue_list = issues.filter(|items| !items.is_empty());
            let summary = h_flex()
                .gap_2()
                .items_center()
                .flex_wrap()
                .when(notable_status(state) || issue_list.is_some(), |d| {
                    d.child(
                        div()
                            .text_sm()
                            .font_weight(FontWeight::MEDIUM)
                            .child("Source state"),
                    )
                })
                .when(notable_status(state), |d| {
                    d.child(badge(state.to_string(), tone(state, cx)))
                })
                .when(issue_list.is_none(), |d| {
                    d.child(
                        div()
                            .text_sm()
                            .text_color(cx.theme().muted_foreground)
                            .child("No issues reported"),
                    )
                });
            if let Some(issues) = issue_list {
                let mut card_body = card(cx).p_3().gap_2().child(summary).child(
                    div()
                        .text_sm()
                        .text_color(cx.theme().muted_foreground)
                        .child(format!("{} issues need attention", issues.len())),
                );
                for (index, issue) in issues.iter().enumerate() {
                    let text = issue
                        .as_str()
                        .or(issue["message"].as_str())
                        .or(issue["code"].as_str())
                        .unwrap_or("Inspect technical details for this finding");
                    card_body = card_body.child(
                        div()
                            .id(("doctor-finding", index))
                            .text_sm()
                            .whitespace_normal()
                            .child(text.to_owned()),
                    );
                }
                findings = findings.child(card_body);
            } else {
                findings = findings.child(summary);
            }
        }
        findings.child(
            h_flex()
                .gap_2()
                .flex_wrap()
                .child(
                    action_button("refresh-health")
                        .outline()
                        .small()
                        .label("Run health check")
                        .on_click(cx.listener(|v, _, _, _| v.inspect("doctor", json!({})))),
                )
                .child(
                    action_button("ai-explain-diagnostics")
                        .outline()
                        .small()
                        .icon(Glyph::Sparkles)
                        .label("Ask Assistant")
                        .accessibility_label("Explain environment in Assistant")
                        .hint("Review interpreter and diagnostics in Assistant")
                        .on_click(cx.listener(|v, _, window, cx| {
                            v.run_command("ai-environment", window, cx);
                        })),
                )
                .child(
                    action_button("copy-redacted-diagnostics")
                        .ghost()
                        .small()
                        .label("Copy diagnostics")
                        .accessibility_label("Copy redacted diagnostics")
                        .hint("Copy allowlisted status metadata without source, paths, or outputs")
                        .on_click(cx.listener(|v, _, _, cx| {
                            let tab = &v.tabs[v.active];
                            // Deliberate allowlist: no source, paths, interpreter environment,
                            // outputs, input contents, identity tokens, or raw error message.
                            let diagnostic = json!({
                                "desktop_version":env!("CARGO_PKG_VERSION"),
                                "source_state":tab.model.source_state,
                                "worker_state":tab.runtime["worker_state"],
                                "connection_lost":tab.runtime["connection_lost"] == true,
                                "error_code":tab.inspections.get("Technical error").map(|e| e["code"].clone()),
                                "cell_count":tab.model.order.len(),
                                "pending_operations":tab.operations.values().filter(|op| !matches!(op["state"].as_str(), Some("completed" | "failed" | "canceled" | "unknown"))).count(),
                            });
                            copy_acknowledged(
                                serde_json::to_string_pretty(&diagnostic).unwrap_or_default(),
                                cx,
                            );
                        })),
                ),
        )
         .into_any_element()
    }
}
