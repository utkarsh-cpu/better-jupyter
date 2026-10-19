use super::*;

#[derive(IntoElement)]
pub(super) struct OutputBlock {
    pub id: SharedString,
    pub event: Value,
    pub context: SharedString,
    pub tools_state: Option<Entity<bool>>,
    pub commands: Option<std::sync::mpsc::Sender<Command>>,
    pub cell_id: SharedString,
    pub exec_id: SharedString,
    pub index: usize,
}

#[derive(Clone)]
struct PageSource {
    slot: String,
    digest: String,
    offset: u64,
    end: u64,
    total: u64,
}

fn page_fields(slot: &str, node: &Value) -> Option<PageSource> {
    let digest = node["blob"].as_str()?.to_owned();
    let page = &node["page"];
    Some(PageSource {
        slot: slot.to_owned(),
        digest,
        offset: page["offset"].as_u64()?,
        end: page["end"].as_u64()?,
        total: page["total"].as_u64()?,
    })
}

fn page_source(event: &Value) -> Option<PageSource> {
    if event["page"].is_object()
        && let Some(digest) = event["blob"].as_str()
    {
        return Some(PageSource {
            slot: String::new(),
            digest: digest.to_owned(),
            offset: event["page"]["offset"].as_u64()?,
            end: event["page"]["end"].as_u64()?,
            total: event["page"]["total"].as_u64()?,
        });
    }
    [
        "text/plain",
        "text/markdown",
        "text/latex",
        "application/json",
    ]
    .into_iter()
    .find_map(|mime| page_fields(mime, &event["bundle"][mime]))
}

pub(super) fn cell_tools(id: SharedString, window: &mut Window, cx: &mut App) -> Entity<bool> {
    window.use_keyed_state((ElementId::from(id), "output-tools"), cx, |_, _| false)
}

pub(super) fn is_wide_output(event: &Value) -> bool {
    event["bundle"]["text/html"]["rows"].is_array()
        || ["image/png", "image/jpeg"]
            .iter()
            .any(|mime| event["bundle"][*mime]["local_path"].is_string())
}

/// The cell owns the disclosure; individual outputs retain copy/find/view state.
#[derive(IntoElement)]
pub(super) struct OutputTools {
    pub owner: SharedString,
    pub state: Entity<bool>,
    pub context: SharedString,
    pub group: SharedString,
    pub always: bool,
}
impl RenderOnce for OutputTools {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let state = self.state;
        let open = *state.read(cx);
        action_button((ElementId::from(self.owner), "output-tools-toggle"))
            .ghost()
            .small()
            .icon(Glyph::Ellipsis)
            .selected(open)
            .contextual(self.group, self.always || open)
            .accessibility_label(format!("Output tools: {}", self.context))
            .hint("Output tools \u{2014} copy, find, and display options")
            .on_click(move |_, _, cx| {
                state.update(cx, |open, cx| {
                    *open = !*open;
                    cx.notify();
                });
            })
    }
}

struct OutputState {
    collapsed: bool,
    details: bool,
    search: bool,
    limit: usize,
    actual_size: bool,
    tools: bool,
    copied: bool,
    copy_revision: u64,
}

impl RenderOnce for OutputBlock {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let state = window.use_keyed_state(
            (ElementId::from(self.id.clone()), "controls"),
            cx,
            |_, _| OutputState {
                collapsed: false,
                details: false,
                search: false,
                limit: 65536,
                actual_size: false,
                tools: false,
                copied: false,
                copy_revision: 0,
            },
        );
        let query =
            window.use_keyed_state((ElementId::from(self.id.clone()), "query"), cx, |w, cx| {
                InputState::new(w, cx).placeholder("Find in displayed output")
            });
        let owner_tools = self.tools_state.as_ref().map(|state| *state.read(cx));
        let current = state.read(cx);
        let (collapsed, details, search, limit, actual) = (
            current.collapsed,
            current.details,
            current.search,
            current.limit,
            current.actual_size,
        );
        let event = &self.event;
        let paging = page_source(event);
        let commands = self.commands.clone();
        let cell_id = self.cell_id.to_string();
        let exec_id = self.exec_id.to_string();
        let output_index = self.index;
        let error = event["kind"] == "error";
        let text = event["text"]
            .as_str()
            .or(event["bundle"]["text/plain"]["value"].as_str())
            .map(str::to_owned)
            .or_else(|| {
                if error {
                    Some(
                        event["traceback"]
                            .as_array()
                            .map(|a| a.iter().filter_map(Value::as_str).collect::<String>())
                            .unwrap_or_else(|| event.to_string()),
                    )
                } else {
                    None
                }
            });
        let tools = owner_tools.unwrap_or(current.tools);
        let copied = current.copied;
        let mut actions = h_flex()
            .gap_1()
            .flex_wrap()
            .pb_1()
            .border_b_1()
            .border_color(divider(cx));
        if let Some(text) = text.clone() {
            let copy_state = state.clone();
            actions = actions.child(
                action_button((ElementId::from(self.id.clone()), "copy"))
                    .ghost()
                    .small()
                    .label(if copied { "Copied" } else { "Copy" })
                    .accessibility_label(format!("Copy recorded output: {}", self.context))
                    .on_click(move |_, _, cx| {
                        cx.write_to_clipboard(ClipboardItem::new_string(text.clone()));
                        let revision = copy_state.update(cx, |s, cx| {
                            s.copied = true;
                            s.copy_revision += 1;
                            cx.notify();
                            s.copy_revision
                        });
                        let state = copy_state.clone();
                        cx.spawn(async move |cx| {
                            cx.background_executor().timer(Duration::from_secs(2)).await;
                            state.update(cx, |s, cx| {
                                if s.copy_revision == revision {
                                    s.copied = false;
                                    cx.notify();
                                }
                            });
                        })
                        .detach();
                    }),
            );
            let state = state.clone();
            actions = actions.child(
                action_button((ElementId::from(self.id.clone()), "find"))
                    .ghost()
                    .small()
                    .label("Find")
                    .accessibility_label(format!("Find in output: {}", self.context))
                    .selected(search)
                    .on_click(move |_, _, cx| {
                        state.update(cx, |s, cx| {
                            s.search = !s.search;
                            cx.notify();
                        })
                    }),
            );
        }
        let state_for_collapse = state.clone();
        actions = actions.child(
            action_button((ElementId::from(self.id.clone()), "collapse"))
                .ghost()
                .small()
                .label(if collapsed {
                    "Show output"
                } else {
                    "Hide output"
                })
                .accessibility_label(format!("Toggle output visibility: {}", self.context))
                .on_click(move |_, _, cx| {
                    state_for_collapse.update(cx, |s, cx| {
                        s.collapsed = !s.collapsed;
                        cx.notify();
                    })
                }),
        );
        if (text.as_ref().is_some_and(|text| text.len() > 16384)
            || paging.as_ref().is_some_and(|page| page.total > 16384))
            && !collapsed
        {
            for cap in [16384, 65536, 262144] {
                let state = state.clone();
                let paging = paging.clone();
                let commands = commands.clone();
                let cell_id = cell_id.clone();
                let exec_id = exec_id.clone();
                actions = actions.child(
                    action_button((self.id.clone(), cap))
                        .ghost()
                        .small()
                        .label(format!("{} KiB", cap / 1024))
                        .selected(paging.is_none() && limit == cap)
                        .on_click(move |_, _, cx| {
                            state.update(cx, |s, cx| {
                                s.limit = cap;
                                cx.notify();
                            });
                            if let (Some(commands), Some(page)) = (&commands, &paging) {
                                let _ = commands.send(Command::OutputPage {
                                    cell_id: cell_id.clone(),
                                    exec_id: exec_id.clone(),
                                    index: output_index,
                                    slot: page.slot.clone(),
                                    digest: page.digest.clone(),
                                    offset: page.offset,
                                    length: cap,
                                });
                            }
                        }),
                );
            }
        }
        if event["bundle"]["text/html"]["rows"].is_array() && text.is_some() {
            let state = state.clone();
            actions = actions.child(
                action_button((ElementId::from(self.id.clone()), "plain-table"))
                    .ghost()
                    .small()
                    .label(if search {
                        "Table view"
                    } else {
                        "Plain text / find"
                    })
                    .on_click(move |_, _, cx| {
                        state.update(cx, |s, cx| {
                            s.search = !s.search;
                            cx.notify();
                        })
                    }),
            );
        }
        let image_path = ["image/png", "image/jpeg"]
            .into_iter()
            .find_map(|mime| event["bundle"][mime]["local_path"].as_str());
        if image_path.is_some() {
            let state = state.clone();
            actions = actions.child(
                action_button((ElementId::from(self.id.clone()), "image-size"))
                    .ghost()
                    .small()
                    .label(if actual { "Fit image" } else { "Actual size" })
                    .on_click(move |_, _, cx| {
                        state.update(cx, |s, cx| {
                            s.actual_size = !s.actual_size;
                            cx.notify();
                        })
                    }),
            );
        }
        let mut body = v_flex()
            .debug_selector({
                let id = self.id.clone();
                move || format!("output-block-{id}")
            })
            .id((ElementId::from(self.id.clone()), "output"))
            .min_w_0()
            .w_full()
            .relative()
            .min_h(rems(1.875))
            .gap_1p5()
            .role(Role::Group)
            .aria_label(if error {
                format!(
                    "Recorded execution error: {}: {}",
                    event["ename"].as_str().unwrap_or("Error"),
                    event["evalue"].as_str().unwrap_or_default()
                )
            } else if let Some(text) = &text {
                format!("Recorded output: {}", bounded(text, 4096))
            } else if image_path.is_some() {
                recorded_figure_name(event, &self.context)
            } else {
                format!("Recorded output: {}", self.context)
            })
            .when(error, |d| {
                d.p_3()
                    .rounded(px(6.))
                    .border_l_2()
                    .border_color(cx.theme().danger)
                    .bg(cx.theme().danger.opacity(0.08))
            })
            .when(self.tools_state.is_none(), |d| {
                d.child(
                    h_flex().justify_end().child(
                        action_button((ElementId::from(self.id.clone()), "tools"))
                            .ghost()
                            .small()
                            .label("Output tools")
                            .dropdown_caret(true)
                            .selected(tools)
                            .accessibility_label(format!("Output tools: {}", self.context))
                            .on_click({
                                let state = state.clone();
                                move |_, _, cx| {
                                    state.update(cx, |s, cx| {
                                        s.tools = !s.tools;
                                        cx.notify();
                                    });
                                }
                            }),
                    ),
                )
            })
            .when(tools, |d| d.child(actions));
        if collapsed {
            return body
                .child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child("Output hidden"),
                )
                .into_any_element();
        }
        if let Some(page) = paging.clone()
            && (page.offset > 0 || page.end < page.total)
        {
            let page_len = page.end.saturating_sub(page.offset).max(1) as usize;
            let previous = commands.clone();
            let next = commands.clone();
            let cell_for_previous = cell_id.clone();
            let cell_for_next = cell_id.clone();
            let exec_for_previous = exec_id.clone();
            let exec_for_next = exec_id.clone();
            let previous_page = page.clone();
            let next_page = page.clone();
            body = body.child(
                h_flex()
                    .gap_1()
                    .flex_wrap()
                    .items_center()
                    .child(
                        action_button((ElementId::from(self.id.clone()), "output-previous"))
                            .ghost()
                            .small()
                            .label("Previous page")
                            .unavailable(page.offset == 0)
                            .accessibility_label(format!("Previous output page: {}", self.context))
                            .on_click(move |_, _, _| {
                                if let Some(commands) = &previous {
                                    let _ = commands.send(Command::OutputPage {
                                        cell_id: cell_for_previous.clone(),
                                        exec_id: exec_for_previous.clone(),
                                        index: output_index,
                                        slot: previous_page.slot.clone(),
                                        digest: previous_page.digest.clone(),
                                        offset: previous_page
                                            .offset
                                            .saturating_sub(page_len as u64),
                                        length: page_len,
                                    });
                                }
                            }),
                    )
                    .child(
                        action_button((ElementId::from(self.id.clone()), "output-next"))
                            .ghost()
                            .small()
                            .label("Next page")
                            .unavailable(page.end >= page.total)
                            .accessibility_label(format!("Next output page: {}", self.context))
                            .on_click(move |_, _, _| {
                                if let Some(commands) = &next {
                                    let _ = commands.send(Command::OutputPage {
                                        cell_id: cell_for_next.clone(),
                                        exec_id: exec_for_next.clone(),
                                        index: output_index,
                                        slot: next_page.slot.clone(),
                                        digest: next_page.digest.clone(),
                                        offset: next_page.end,
                                        length: page_len,
                                    });
                                }
                            }),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(format!(
                                "Bytes {}–{} of {}",
                                if page.end > page.offset {
                                    page.offset + 1
                                } else {
                                    page.offset
                                },
                                page.end,
                                page.total
                            )),
                    ),
            );
        }
        if search {
            body = body.child(Input::new(&query).small().cleanable(true));
        }
        if error {
            let state = state.clone();
            body = body.child(
                h_flex()
                    .gap_2()
                    .flex_wrap()
                    .items_center()
                    .child(
                        div()
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(cx.theme().danger)
                            .child(format!(
                                "{}: {}",
                                event["ename"].as_str().unwrap_or("Error"),
                                event["evalue"].as_str().unwrap_or("Execution failed")
                            )),
                    )
                    .child(
                        action_button((ElementId::from(self.id.clone()), "traceback"))
                            .ghost()
                            .small()
                            .label(if details {
                                "Hide traceback"
                            } else {
                                "Show full traceback"
                            })
                            .accessibility_label(format!(
                                "{} for {}",
                                if details {
                                    "Hide traceback"
                                } else {
                                    "Show full traceback"
                                },
                                self.context
                            ))
                            .on_click(move |_, _, cx| {
                                state.update(cx, |s, cx| {
                                    s.details = !s.details;
                                    cx.notify();
                                })
                            }),
                    ),
            );
        }
        let prefer_rich = image_path.is_some()
            || event["bundle"]["text/html"]["rows"].is_array()
            || event["bundle"]["text/markdown"]["value"].is_string()
            || !event["bundle"]["application/json"]["value"].is_null();
        if let Some(text) = text.filter(|_| !prefer_rich || search) {
            if !error || details || search {
                let shown = if paging.is_some() {
                    text.clone()
                } else {
                    bounded(&text, limit)
                };
                let needle = query.read(cx).value().to_lowercase();
                let shown = if search && !needle.is_empty() {
                    let lines = shown
                        .lines()
                        .filter(|line| line.to_lowercase().contains(&needle))
                        .collect::<Vec<_>>();
                    if lines.is_empty() {
                        "No matches in displayed output".into()
                    } else {
                        lines.join("\n")
                    }
                } else {
                    shown
                };
                body = body.child(
                    TextView::markdown(
                        (ElementId::from(self.id.clone()), "text"),
                        format!("```text\n{}\n```", shown.replace("```", "~~~")),
                    )
                    .style(gpui_kit::component::text::TextViewStyle {
                        paragraph_gap: rems(0.),
                        code_block: div()
                            .p_0()
                            .bg(cx.theme().transparent)
                            .rounded(px(0.))
                            .style()
                            .clone(),
                        ..Default::default()
                    })
                    .selectable(true),
                );
                if let Some(page) = &paging {
                    if page.end < page.total || page.offset > 0 {
                        body = body.child(
                            div()
                                .text_xs()
                                .text_color(cx.theme().muted_foreground)
                                .child(
                                    "Copy and Find use this page. Recorded output is unchanged.",
                                ),
                        );
                    }
                } else if text.len() > limit {
                    body = body.child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child("Display limited; Copy retains the complete loaded text. Recorded output is unchanged."),
                    );
                }
            }
        } else if let Some(path) = image_path {
            let image_file = PathBuf::from(path);
            body = body.child(if image_file.is_file() {
                remember_cached_image(&image_file, &self.id, window, cx);
                div()
                    .id((ElementId::from(self.id.clone()), "image-scroll"))
                    .overflow_x_scroll()
                    .child(
                        img(image_file)
                            .rounded(px(6.))
                            .when(!actual, |image| image.max_w_full().max_h(px(500.))),
                    )
                    .into_any_element()
            } else {
                let commands = commands.clone();
                let cell_id = cell_id.clone();
                let exec_id = exec_id.clone();
                let mime = ["image/png", "image/jpeg"]
                    .into_iter()
                    .find(|mime| event["bundle"][*mime]["local_path"].as_str() == Some(path))
                    .unwrap_or("image/png");
                let digest = event["bundle"][mime]["blob"]
                    .as_str()
                    .unwrap_or_default()
                    .to_owned();
                let mime = mime.to_owned();
                h_flex()
                    .gap_2()
                    .items_center()
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child("This figure was evicted from the image cache."),
                    )
                    .child(
                        action_button((ElementId::from(self.id.clone()), "show-image"))
                            .ghost()
                            .small()
                            .label("Show again")
                            .unavailable(digest.is_empty())
                            .on_click(move |_, _, _| {
                                if let Some(commands) = &commands {
                                    let _ = commands.send(Command::ShowImage {
                                        cell_id: cell_id.clone(),
                                        exec_id: exec_id.clone(),
                                        index: output_index,
                                        mime: mime.clone(),
                                        digest: digest.clone(),
                                    });
                                }
                            }),
                    )
                    .into_any_element()
            });
        } else {
            body = body.child(output_inner(event, self.id.to_string(), cx));
        }
        body.into_any_element()
    }
}

fn recorded_figure_name(event: &Value, context: &str) -> String {
    let alt = ["image/png", "image/jpeg"].into_iter().find_map(|mime| {
        event["bundle"][mime]["alt"]
            .as_str()
            .or(event["bundle"][mime]["name"].as_str())
            .filter(|label| !label.is_empty())
    });
    match alt {
        Some(label) => format!("Recorded figure: {label}"),
        None => format!("Recorded figure: {context}"),
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
        .style(gpui_kit::component::text::TextViewStyle {
            paragraph_gap: rems(0.),
            code_block: div()
                .p_0()
                .bg(theme.transparent)
                .rounded(px(0.))
                .style()
                .clone(),
            ..Default::default()
        })
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
        let columns = rows
            .iter()
            .filter_map(Value::as_array)
            .map(Vec::len)
            .max()
            .unwrap_or(0);
        let row_border = divider(cx);
        let render_row = |index: usize, row: &Value| {
            let mut line = h_flex()
                .id(SharedString::from(format!("{id}-row-{index}")))
                .role(Role::Row)
                .border_b_1()
                .border_color(row_border)
                .when(index == 0, |d| {
                    d.bg(theme.muted).font_weight(FontWeight::SEMIBOLD)
                });
            for column in 0..columns {
                let text = row[column].as_str().unwrap_or_default().to_string();
                line = line.child(
                    div()
                        .id(SharedString::from(format!("{id}-r{index}-c{column}")))
                        .role(if index == 0 {
                            Role::ColumnHeader
                        } else {
                            Role::Cell
                        })
                        .aria_label(text.clone())
                        .aria_row_index(index)
                        .aria_column_index(column)
                        .focusable()
                        .w(px(160.))
                        .flex_none()
                        .px_3()
                        .py_2()
                        .whitespace_normal()
                        .focus(|style| style.border_2().border_color(theme.ring))
                        .child(text),
                );
            }
            line
        };
        let mut table = v_flex()
            .id(SharedString::from(format!("{id}-table-body")))
            .min_w(px(columns as f32 * 160.))
            .role(Role::Table)
            .aria_label("Recorded table")
            .aria_row_count(rows.len())
            .aria_column_count(columns)
            .rounded(px(6.))
            .border_1()
            .border_color(row_border)
            .bg(theme.popover)
            .overflow_hidden()
            .text_xs();
        if let Some(header) = rows.first() {
            table = table.child(render_row(0, header));
        }
        table = table.child(
            v_flex()
                .id(SharedString::from(format!("{id}-rows")))
                .max_h(px(320.))
                .overflow_y_scroll()
                .children(
                    rows.iter()
                        .enumerate()
                        .skip(1)
                        .take(200)
                        .map(|(index, row)| render_row(index, row)),
                ),
        );
        if rows.len() > 201 {
            table = table.child(
                div()
                    .px_3()
                    .py_2()
                    .text_xs()
                    .text_color(theme.muted_foreground)
                    .child("Showing the first 200 data rows. Recorded data is unchanged; use the text fallback for additional rows."),
            );
        }
        return div()
            .id(SharedString::from(format!("{id}-table")))
            .overflow_x_scroll()
            .child(table)
            .into_any_element();
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

fn remember_cached_image(
    path: &std::path::Path,
    id: &SharedString,
    window: &mut Window,
    cx: &mut App,
) {
    let Some(hash) = path.file_name().and_then(|name| name.to_str()) else {
        return;
    };
    let Some(dir) = path.parent() else {
        return;
    };
    if hash.len() != 64 || !hash.chars().all(|character| character.is_ascii_hexdigit()) {
        return;
    }
    let seen = window.use_keyed_state(
        (ElementId::from(id.clone()), format!("image-touch-{hash}")),
        cx,
        |_, _| false,
    );
    if *seen.read(cx) {
        return;
    }
    seen.update(cx, |flag, cx| {
        *flag = true;
        cx.notify();
    });
    kernelledger_client::output::touch_cached_image(dir, hash);
}

pub(super) fn attachment_preview(
    commands: std::sync::mpsc::Sender<Command>,
    cell_id: SharedString,
    name: SharedString,
    mime: SharedString,
    preview: Option<Value>,
    window: &mut Window,
    cx: &mut App,
) -> AnyElement {
    let requested = window.use_keyed_state(
        (
            ElementId::from(cell_id.clone()),
            format!("attachment-{name}"),
        ),
        cx,
        |_, _| false,
    );
    if preview.is_none() && !*requested.read(cx) {
        requested.update(cx, |flag, cx| {
            *flag = true;
            cx.notify();
        });
        let _ = commands.send(Command::PreviewAttachment {
            cell_id: cell_id.to_string(),
            name: name.to_string(),
            offset: 0,
            length: kernelledger_client::output::VIEW_TEXT_LIMIT,
        });
    }
    let mut block = v_flex()
        .id((
            ElementId::from(cell_id.clone()),
            format!("attachment-{name}"),
        ))
        .gap_1()
        .role(Role::Group)
        .aria_label(format!("Attachment {name}"))
        .child(
            div()
                .text_xs()
                .text_color(cx.theme().muted_foreground)
                .child(format!("{name} · {mime}")),
        );
    let Some(preview) = preview else {
        return block
            .child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child("Loading attachment preview"),
            )
            .into_any_element();
    };
    if let Some(error) = preview["error"].as_str().filter(|error| !error.is_empty()) {
        let retry = commands.clone();
        let retry_cell = cell_id.to_string();
        let retry_name = name.to_string();
        return block
            .child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(error.to_string()),
            )
            .child(
                action_button((
                    ElementId::from(cell_id.clone()),
                    format!("attachment-retry-{name}"),
                ))
                .ghost()
                .small()
                .label("Try preview again")
                .on_click(move |_, _, _| {
                    let _ = retry.send(Command::PreviewAttachment {
                        cell_id: retry_cell.clone(),
                        name: retry_name.clone(),
                        offset: 0,
                        length: kernelledger_client::output::VIEW_TEXT_LIMIT,
                    });
                }),
            )
            .into_any_element();
    }
    if let Some(path) = preview["path"].as_str().filter(|path| !path.is_empty()) {
        let file = PathBuf::from(path);
        if file.is_file() {
            remember_cached_image(&file, &cell_id, window, cx);
            block = block.child(img(file).max_w_full().max_h(px(320.)).rounded(px(6.)));
        }
    } else if let Some(text) = preview["text"].as_str() {
        let offset = preview["offset"].as_u64().unwrap_or(0);
        let end = preview["end"].as_u64().unwrap_or(0);
        let total = preview["total"].as_u64().unwrap_or(0);
        block = block.child(
            div()
                .font_family(cx.theme().mono_font_family.clone())
                .text_xs()
                .child(text.to_owned()),
        );
        if offset > 0 || end < total {
            let page_len = end.saturating_sub(offset).max(1);
            let previous = commands.clone();
            let next = commands.clone();
            let previous_cell = cell_id.to_string();
            let next_cell = cell_id.to_string();
            let previous_name = name.to_string();
            let next_name = name.to_string();
            block = block.child(
                h_flex()
                    .gap_1()
                    .items_center()
                    .child(
                        action_button((
                            ElementId::from(cell_id.clone()),
                            format!("attachment-previous-{name}"),
                        ))
                        .ghost()
                        .small()
                        .label("Previous page")
                        .unavailable(offset == 0)
                        .on_click(move |_, _, _| {
                            let _ = previous.send(Command::PreviewAttachment {
                                cell_id: previous_cell.clone(),
                                name: previous_name.clone(),
                                offset: offset.saturating_sub(page_len),
                                length: page_len as usize,
                            });
                        }),
                    )
                    .child(
                        action_button((
                            ElementId::from(cell_id.clone()),
                            format!("attachment-next-{name}"),
                        ))
                        .ghost()
                        .small()
                        .label("Next page")
                        .unavailable(end >= total)
                        .on_click(move |_, _, _| {
                            let _ = next.send(Command::PreviewAttachment {
                                cell_id: next_cell.clone(),
                                name: next_name.clone(),
                                offset: end,
                                length: page_len as usize,
                            });
                        }),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(format!("Bytes {}–{} of {total}", offset + 1, end)),
                    ),
            );
        }
    }
    block.into_any_element()
}
