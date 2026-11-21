use super::*;

pub(super) struct ParsedHelp {
    pub intro: String,
    pub shortcuts: Vec<(String, String)>,
    pub post_table_intro: String,
    pub sections: Vec<String>,
}

pub(super) fn parse_help_document(source: &str) -> ParsedHelp {
    let mut sections_iter = source.split("\n## ");
    let first_block = sections_iter.next().unwrap_or("");
    let sections = sections_iter
        .map(|section| format!("## {section}"))
        .collect::<Vec<_>>();

    let mut intro_lines = Vec::new();
    let mut shortcuts = Vec::new();
    let mut post_lines = Vec::new();
    let mut in_table = false;
    let mut seen_table = false;

    for line in first_block.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('|') && trimmed.ends_with('|') {
            in_table = true;
            seen_table = true;
            let cells: Vec<&str> = trimmed
                .trim_matches('|')
                .split('|')
                .map(str::trim)
                .collect();
            if cells.len() >= 2 {
                let is_header = cells[0].eq_ignore_ascii_case("action")
                    && cells[1].eq_ignore_ascii_case("shortcut");
                let is_divider = cells
                    .iter()
                    .all(|cell| !cell.is_empty() && cell.chars().all(|c| c == '-' || c == ':'));
                if !is_header && !is_divider {
                    shortcuts.push((cells[0].to_owned(), cells[1].to_owned()));
                }
            }
            continue;
        }
        if in_table {
            in_table = false;
        }
        if seen_table {
            post_lines.push(line);
        } else {
            intro_lines.push(line);
        }
    }

    ParsedHelp {
        intro: intro_lines.join("\n").trim().to_owned(),
        shortcuts,
        post_table_intro: post_lines.join("\n").trim().to_owned(),
        sections,
    }
}

fn parse_help_keystroke(display: &str) -> Option<Keystroke> {
    let mut modifiers = Modifiers::default();
    let mut key: Option<String> = None;
    for raw in display.trim().split('+') {
        let token = raw.trim().to_ascii_lowercase();
        match token.as_str() {
            "ctrl" | "control" => modifiers.control = true,
            "cmd" | "command" | "super" => modifiers.platform = true,
            "shift" => modifiers.shift = true,
            "alt" | "option" => modifiers.alt = true,
            "" => return None,
            other => {
                if key.is_some() {
                    return None;
                }
                key = Some(other.to_owned());
            }
        }
    }
    Some(Keystroke {
        modifiers,
        key: key?,
        key_char: None,
    })
}

fn render_shortcut_combo(combo: &str, cx: &App) -> Div {
    let muted = cx.theme().muted_foreground;
    let mut row = h_flex().gap_1().items_center().flex_wrap();
    for (index, part) in combo.split(" / ").enumerate() {
        if index > 0 {
            row = row.child(div().text_xs().text_color(muted).child("/"));
        }
        if let Some(keystroke) = parse_help_keystroke(part) {
            row = row.child(Kbd::new(keystroke));
        } else {
            row = row.child(
                div()
                    .px_1p5()
                    .py_0p5()
                    .rounded(px(4.))
                    .border_1()
                    .border_color(surface_edge(cx))
                    .bg(cx.theme().muted)
                    .font_family(cx.theme().mono_font_family.clone())
                    .text_xs()
                    .child(part.trim().to_owned()),
            );
        }
    }
    row
}

impl Workspace {
    pub(super) fn help_content(&self, cx: &mut Context<Self>) -> Div {
        let query = self.help_query.read(cx).value().trim().to_lowercase();
        let raw = include_str!("../../../../docs/desktop/help.md")
            .trim_start()
            .strip_prefix("# Help and shortcuts")
            .unwrap_or(include_str!("../../../../docs/desktop/help.md"))
            .trim_start();
        let source = shortcuts::shortcut_text(raw);
        let parsed = parse_help_document(&source);

        let matching_shortcuts: Vec<&(String, String)> = parsed
            .shortcuts
            .iter()
            .filter(|(action, keys)| {
                query.is_empty()
                    || action.to_lowercase().contains(&query)
                    || keys.to_lowercase().contains(&query)
            })
            .collect();

        let matching_sections: Vec<&String> = parsed
            .sections
            .iter()
            .filter(|section| query.is_empty() || section.to_lowercase().contains(&query))
            .collect();

        let intro_matches = query.is_empty()
            || parsed.intro.to_lowercase().contains(&query)
            || parsed.post_table_intro.to_lowercase().contains(&query);

        let mut container = v_flex()
            .max_w(px(760.))
            .gap_3()
            .child(Input::new(&self.help_query).cleanable(true));

        if !query.is_empty()
            && matching_shortcuts.is_empty()
            && matching_sections.is_empty()
            && !intro_matches
        {
            return container.child(empty_row(
                "No matching help topics. Try \u{2018}run\u{2019}, \u{2018}checkpoint\u{2019}, or \u{2018}recovery\u{2019}.",
                cx,
            ));
        }

        if intro_matches && !parsed.intro.is_empty() {
            container = container.child(
                div()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .whitespace_normal()
                    .child(parsed.intro),
            );
        }

        if !matching_shortcuts.is_empty() {
            let muted = cx.theme().muted_foreground;
            let mut table = card(cx).child(
                h_flex()
                    .w_full()
                    .justify_between()
                    .items_center()
                    .px_3()
                    .py_2()
                    .border_b_1()
                    .border_color(divider(cx))
                    .text_xs()
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(muted)
                    .child("Action")
                    .child("Shortcut"),
            );
            for (index, (action, keys)) in matching_shortcuts.iter().enumerate() {
                let last = index + 1 == matching_shortcuts.len();
                table = table.child(
                    h_flex()
                        .w_full()
                        .justify_between()
                        .items_center()
                        .gap_4()
                        .px_3()
                        .py_1p5()
                        .when(!last, |d| d.border_b_1().border_color(divider(cx)))
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .text_sm()
                                .whitespace_normal()
                                .child(action.clone()),
                        )
                        .child(render_shortcut_combo(keys, cx)),
                );
            }
            container = container.child(section("Keyboard shortcuts", cx).child(table));
        }

        let mut markdown_parts = Vec::new();
        if intro_matches && !parsed.post_table_intro.is_empty() {
            markdown_parts.push(parsed.post_table_intro);
        }
        for sec in matching_sections {
            markdown_parts.push(sec.clone());
        }
        if !markdown_parts.is_empty() {
            container = container.child(
                TextView::markdown("task-help", markdown_parts.join("\n\n")).selectable(true),
            );
        }

        container
    }
}
