use super::*;

#[derive(IntoElement)]
pub(super) struct OutputBlock {
    pub id: SharedString,
    pub event: Value,
}

struct OutputState {
    collapsed: bool,
    details: bool,
    search: bool,
    limit: usize,
    actual_size: bool,
}

impl RenderOnce for OutputBlock {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let state = window.use_keyed_state((ElementId::from(self.id.clone()), "controls"), cx, |_, _| OutputState {
            collapsed: false, details: false, search: false, limit: 65536, actual_size: false,
        });
        let query = window.use_keyed_state((ElementId::from(self.id.clone()), "query"), cx, |w, cx| InputState::new(w, cx).placeholder("Find in displayed output"));
        let current = state.read(cx);
        let (collapsed, details, search, limit, actual) = (current.collapsed, current.details, current.search, current.limit, current.actual_size);
        let event = &self.event;
        let error = event["kind"] == "error";
        let text = event["text"].as_str().or(event["bundle"]["text/plain"]["value"].as_str()).map(str::to_owned)
            .or_else(|| if error { Some(event["traceback"].as_array().map(|a| a.iter().filter_map(Value::as_str).collect::<String>()).unwrap_or_else(|| event.to_string())) } else { None });
        let mut actions = h_flex().gap_1().flex_wrap();
        if let Some(text) = text.clone() {
            actions = actions.child(Button::new((ElementId::from(self.id.clone()), "copy")).ghost().small().label("Copy")
                .accessibility_label("Copy recorded output")
                .on_click(move |_, _, cx| cx.write_to_clipboard(ClipboardItem::new_string(text.clone()))));
            let state = state.clone();
            actions = actions.child(Button::new((ElementId::from(self.id.clone()), "find")).ghost().small().label("Find").selected(search)
                .on_click(move |_, _, cx| state.update(cx, |s, cx| { s.search = !s.search; cx.notify(); })));
        }
        let state_for_collapse = state.clone();
        actions = actions.child(Button::new((ElementId::from(self.id.clone()), "collapse")).ghost().small().label(if collapsed { "Show output" } else { "Hide output" })
            .on_click(move |_, _, cx| state_for_collapse.update(cx, |s, cx| { s.collapsed = !s.collapsed; cx.notify(); })));
        if text.as_ref().is_some_and(|s| s.len() > 16384) && !collapsed {
            for cap in [16384, 65536, 262144] {
                let state = state.clone();
                actions = actions.child(Button::new((self.id.clone(), cap)).ghost().small().label(format!("{} KiB", cap / 1024)).selected(limit == cap)
                    .on_click(move |_, _, cx| state.update(cx, |s, cx| { s.limit = cap; cx.notify(); })));
            }
        }
        if event["bundle"]["text/html"]["rows"].is_array() && text.is_some() {
            let state = state.clone();
            actions = actions.child(Button::new((ElementId::from(self.id.clone()), "plain-table")).ghost().small().label(if search { "Table view" } else { "Plain text / find" })
                .on_click(move |_, _, cx| state.update(cx, |s, cx| { s.search = !s.search; cx.notify(); })));
        }
        let image_path = ["image/png", "image/jpeg"].into_iter().find_map(|mime| event["bundle"][mime]["local_path"].as_str());
        if image_path.is_some() {
            let state = state.clone();
            actions = actions.child(Button::new((ElementId::from(self.id.clone()), "image-size")).ghost().small().label(if actual { "Fit image" } else { "Actual size" })
                .on_click(move |_, _, cx| state.update(cx, |s, cx| { s.actual_size = !s.actual_size; cx.notify(); })));
        }
        let mut body = v_flex().id((ElementId::from(self.id.clone()), "output")).min_w_0().gap_1()
            .role(Role::Group).aria_label(if error { "Recorded execution error" } else { "Recorded output" }).child(actions);
        if collapsed { return body.child("Output hidden").into_any_element(); }
        if search { body = body.child(Input::new(&query).small().cleanable(true)); }
        if error {
            let state = state.clone();
            body = body.child(div().text_color(cx.theme().danger).child(format!("{}: {}", event["ename"].as_str().unwrap_or("Error"), event["evalue"].as_str().unwrap_or("Execution failed"))))
                .child(Button::new((ElementId::from(self.id.clone()), "traceback")).ghost().small().label(if details { "Hide traceback" } else { "Show full traceback" })
                    .on_click(move |_, _, cx| state.update(cx, |s, cx| { s.details = !s.details; cx.notify(); })));
        }
        let prefer_rich = image_path.is_some() || event["bundle"]["text/html"]["rows"].is_array() || event["bundle"]["text/markdown"]["value"].is_string() || !event["bundle"]["application/json"]["value"].is_null();
        if let Some(text) = text.filter(|_| !prefer_rich || search) {
            if !error || details || search {
                let shown = bounded(&text, limit);
                let needle = query.read(cx).value().to_lowercase();
                let shown = if search && !needle.is_empty() {
                    let lines = shown.lines().filter(|line| line.to_lowercase().contains(&needle)).collect::<Vec<_>>();
                    if lines.is_empty() { "No matches in displayed output".into() } else { lines.join("\n") }
                } else { shown };
                body = body.child(TextView::markdown((ElementId::from(self.id.clone()), "text"), format!("```text\n{}\n```", shown.replace("```", "~~~"))).selectable(true));
                if text.len() > limit { body = body.child("Display limited; Copy retains the complete loaded text. Recorded output is unchanged."); }
            }
        } else if let Some(path) = image_path {
            body = body.child(div().id((ElementId::from(self.id.clone()), "image-scroll")).overflow_x_scroll().child(
                img(PathBuf::from(path)).when(!actual, |image| image.max_w_full().max_h(px(500.)))
            ));
        } else { body = body.child(output_inner(event, self.id.to_string(), cx)); }
        body.into_any_element()
    }
}

fn output_inner(event: &Value, id: String, cx: &App) -> AnyElement {
    let theme = cx.theme();
    let mono = theme.mono_font_family.clone();
    if let Some(error) = event["viewer_error"].as_str() {
        return div()
            .text_xs()
            .text_color(theme.muted_foreground)
            .child(format!("Output unavailable: {error}"))
            .into_any_element();
    }
    let kind = event["kind"].as_str().unwrap_or("unknown");
    if matches!(kind, "stdout" | "stderr") {
        return TextView::markdown(
            SharedString::from(id),
            format!(
                "```text\n{}\n```",
                bounded(event["text"].as_str().unwrap_or("Blob output"), 65536)
                    .replace("```", "~~~")
            ),
        )
        .selectable(true)
        .into_any_element();
    }
    if kind == "error" {
        let danger = theme.danger;
        return v_flex()
            .gap_1()
            .p_3()
            .rounded(px(6.))
            .border_1()
            .border_color(danger.opacity(0.3))
            .bg(danger.opacity(0.06))
            .text_color(danger)
            .child(
                h_flex()
                    .gap_1p5()
                    .items_center()
                    .text_xs()
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(Icon::new(Glyph::CircleAlert).xsmall())
                    .child(event["ename"].as_str().unwrap_or("Error").to_string()),
            )
            .child(
                div().font_family(mono).text_xs().child(
                    event["traceback"]
                        .as_array()
                        .map(|a| a.iter().filter_map(Value::as_str).collect::<String>())
                        .unwrap_or_else(|| event.to_string()),
                ),
            )
            .into_any_element();
    }
    let bundle = &event["bundle"];
    for mime in ["image/png", "image/jpeg"] {
        if let Some(path) = bundle[mime]["local_path"].as_str() {
            return img(PathBuf::from(path))
                .max_w_full()
                .max_h(px(500.))
                .rounded(px(6.))
                .into_any_element();
        }
    }
    if let Some(rows) = bundle["text/html"]["rows"].as_array() {
        let columns = rows.iter().filter_map(Value::as_array).map(Vec::len).max().unwrap_or(0);
        let render_row = |index: usize, row: &Value| {
            let mut line = h_flex().id(SharedString::from(format!("{id}-row-{index}"))).role(Role::Row).border_b_1().border_color(theme.border)
                .when(index == 0, |d| d.bg(theme.muted).font_weight(FontWeight::SEMIBOLD));
            for column in 0..columns {
                let text = row[column].as_str().unwrap_or_default().to_string();
                line = line.child(div().id(SharedString::from(format!("{id}-r{index}-c{column}")))
                    .role(if index == 0 { Role::ColumnHeader } else { Role::Cell })
                    .aria_label(text.clone()).aria_row_index(index).aria_column_index(column)
                    .focusable().w(px(160.)).flex_none().px_3().py_2().whitespace_normal()
                    .focus(|style| style.border_2().border_color(theme.ring)).child(text));
            }
            line
        };
        let mut table = v_flex().id(SharedString::from(format!("{id}-table-body"))).min_w(px(columns as f32 * 160.)).role(Role::Table)
            .aria_label("Recorded table").aria_row_count(rows.len()).aria_column_count(columns).text_xs();
        if let Some(header) = rows.first() { table = table.child(render_row(0, header)); }
        table = table.child(v_flex().id(SharedString::from(format!("{id}-rows"))).max_h(px(320.)).overflow_y_scroll()
            .children(rows.iter().enumerate().skip(1).take(200).map(|(index, row)| render_row(index, row))));
        if rows.len() > 201 { table = table.child("Showing the first 200 data rows. Recorded data is unchanged; use the text fallback for additional rows."); }
        return div().id(SharedString::from(format!("{id}-table"))).overflow_x_scroll().child(table).into_any_element();
    }
    if let Some(text) = bundle["text/markdown"]["value"].as_str() {
        return TextView::markdown(SharedString::from(id), safe_markdown(text))
            .on_link_click(safe_link)
            .into_any_element();
    }
    if !bundle["application/json"]["value"].is_null() {
        return div()
            .font_family(mono)
            .text_xs()
            .child(bounded(
                &serde_json::to_string_pretty(&bundle["application/json"]["value"])
                    .unwrap_or_default(),
                65536,
            ))
            .into_any_element();
    }
    for mime in ["text/plain", "text/latex"] {
        if let Some(text) = bundle[mime]["value"].as_str() {
            return div()
                .font_family(mono)
                .text_xs()
                .child(bounded(text, 65536))
                .into_any_element();
        }
    }
    div()
        .text_xs()
        .text_color(theme.muted_foreground)
        .child(format!(
            "{kind}: native preview unavailable; recorded data retained"
        ))
        .into_any_element()
}

