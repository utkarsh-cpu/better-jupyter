use super::*;

impl Workspace {
    pub(super) fn checkpoint_report(&self, value: &Value, cx: &mut Context<Self>) -> Div {
        let id = value["checkpoint_id"]
            .as_str()
            .unwrap_or_default()
            .to_string();
        let mut report = section("Namespace checkpoint coverage", cx)
            .child("Saves supported namespace values. This is not a complete process snapshot.")
            .child(format!(
                "Recorded outcome: {}",
                value["status"].as_str().unwrap_or("Unknown")
            ));
        for field in ["excluded", "notes"] {
            if let Some(data) = value.get(field) {
                report = report.child(
                    section(
                        if field == "excluded" {
                            "Excluded values"
                        } else {
                            "Coverage notes"
                        },
                        cx,
                    )
                    .child(
                        TextView::markdown(
                            SharedString::from(format!("coverage-{field}")),
                            format!(
                                "```json\n{}\n```",
                                bounded(
                                    &serde_json::to_string_pretty(data).unwrap_or_default(),
                                    16384
                                )
                            ),
                        )
                        .selectable(true),
                    ),
                );
            }
        }
        report = report.child(div().text_xs().child("Provenance and environment compatibility are not established by this report. Review the trust confirmation before restoring."));
        let mut actions = h_flex().gap_2();
        for (method, label) in [
            ("restore", "Restore…"),
            ("continue", "Continue in new session…"),
        ] {
            let id = id.clone();
            actions = actions.child(
                Button::new(SharedString::from(format!("report-{method}")))
                    .outline()
                    .label(label)
                    .on_click(cx.listener(move |v, _, _, cx| {
                        v.confirmation = Some((method.into(), json!({"checkpoint_id":id})));
                        cx.notify();
                    })),
            );
        }
        report.child(actions).child(
            Button::new("copy-checkpoint-id")
                .ghost()
                .label("Copy checkpoint identifier")
                .on_click(cx.listener(move |_, _, _, cx| {
                    cx.write_to_clipboard(ClipboardItem::new_string(id.clone()))
                })),
        )
    }
}
