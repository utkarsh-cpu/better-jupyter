use super::*;

impl Workspace {
    pub(super) fn history_filters(&self, cx: &mut Context<Self>) -> Div {
        let query = self.history_query.read(cx).value().to_string();
        let mut filters = h_flex().flex_wrap().gap_2().items_center().child("Outcome");
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
                Button::new(SharedString::from(format!("outcome-{value}")))
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
        v_flex().gap_2().child(filters)
            .child(div().text_xs().text_color(cx.theme().muted_foreground).child("Search source, or combine cell:, session:, after:YYYY-MM-DD and before:YYYY-MM-DD filters."))
    }
}
