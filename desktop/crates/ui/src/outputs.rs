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
    pub worker_generation: Option<String>,
    pub project: Option<PathBuf>,
    pub workspace: Option<Entity<Workspace>>,
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
        || kernelledger_client::output::html_is_interactive(event)
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
            .icon(Glyph::Settings)
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
    html: bool,
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
                html: false,
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
        if let Some(source) = event["bundle"]["text/latex"]["value"].as_str() {
            let source = source.to_owned();
            actions = actions.child(
                action_button((ElementId::from(self.id.clone()), "copy-equation"))
                    .ghost()
                    .small()
                    .label("Copy TeX")
                    .accessibility_label("Copy TeX source")
                    .on_click(move |_, _, cx| {
                        cx.write_to_clipboard(ClipboardItem::new_string(source.clone()))
                    }),
            );
        }
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
        let html_interactive = kernelledger_client::output::html_is_interactive(event);
        let html_available = kernelledger_client::output::html_output_available(event);
        let allow_html = self
            .workspace
            .as_ref()
            .map(|workspace| workspace.read(cx).preferences.allow_html_output)
            .unwrap_or(false);
        let html_network = self
            .workspace
            .as_ref()
            .map(|workspace| workspace.read(cx).preferences.html_network_policy)
            .unwrap_or_default()
            .description();
        let html_view = self
            .workspace
            .as_ref()
            .and_then(|workspace| workspace.read(cx).html_view(self.id.as_ref()));
        let html_note = self
            .workspace
            .as_ref()
            .and_then(|workspace| workspace.read(cx).html_note(self.id.as_ref()));
        let html_blocked = self
            .workspace
            .as_ref()
            .map(|workspace| workspace.read(cx).html_views_blocked())
            .unwrap_or(false);
        let html_live = html_view.is_some();
        let html_pending = self
            .workspace
            .as_ref()
            .is_some_and(|workspace| workspace.read(cx).html_pending(self.id.as_ref()));
        let html_height = self
            .workspace
            .as_ref()
            .map_or(html_output::HOST_HEIGHT, |workspace| {
                workspace.read(cx).html_height(self.id.as_ref())
            });
        let html_has_rows = event["bundle"]["text/html"]["rows"]
            .as_array()
            .is_some_and(|rows| !rows.is_empty());
        let html_toggle = if html_available {
            let workspace = self.workspace.clone();
            let output_id = self.id.to_string();
            let html_source = event["bundle"]["text/html"]["value"]
                .as_str()
                .unwrap_or_default()
                .to_owned();
            let html_state = state.clone();
            let html_digest = event["bundle"]["text/html"]["blob"]
                .as_str()
                .map(str::to_owned);
            let html_exec = self.exec_id.to_string();
            Some(
                action_button((ElementId::from(self.id.clone()), "render-html"))
                    .outline()
                    .small()
                    .label(if html_live {
                        "Stop rendering"
                    } else if html_pending {
                        "Cancel loading"
                    } else if html_has_rows {
                        "Render original HTML"
                    } else {
                        "Render HTML"
                    })
                    .unavailable(!allow_html)
                    .hint(if allow_html {
                        format!("Render this recorded HTML. {html_network}")
                    } else {
                        "Enable interactive HTML in Settings, then press Render HTML. Network access follows your HTML network setting.".to_owned()
                    })
                    .accessibility_label(if html_live {
                        format!("Stop rendering HTML: {}", self.context)
                    } else {
                        format!("Render HTML: {}", self.context)
                    })
                    .on_click(move |_, window, cx| {
                        if !allow_html {
                            return;
                        }
                        let start = html_state.update(cx, |s, cx| {
                            s.html = !(html_live || html_pending);
                            cx.notify();
                            s.html
                        });
                        if let Some(workspace) = &workspace {
                            let key = output_id.clone();
                            let html = html_source.clone();
                            workspace.update(cx, |ws, cx| {
                                if start {
                                    ws.request_html_output(key, html, html_digest.clone(), html_exec.clone(), window, cx);
                                } else {
                                    ws.stop_html_output(&key, cx);
                                }
                                cx.notify();
                            });
                        }
                    }),
            )
        } else {
            None
        };
        let mut body = v_flex()
            .debug_selector({
                let id = self.id.clone();
                move || format!("output-block-{id}")
            })
            .id((ElementId::from(self.id.clone()), "output"))
            .min_w_0()
            .w_full()
            .relative()
            // Only a block that hosts its own Output tools button needs a control-height row.
            .when(self.tools_state.is_none(), |d| d.min_h(rems(1.875)))
            .gap_1p5()
            .role(Role::Group)
            .aria_label(if error {
                format!(
                    "Recorded execution error: {}: {}",
                    event["ename"].as_str().unwrap_or("Error"),
                    event["evalue"].as_str().unwrap_or_default()
                )
            } else if html_live {
                format!("Interactive HTML output: {}", self.context)
            } else if html_interactive {
                format!("Interactive HTML recorded: {}", self.context)
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
            .when(tools, |d| d.child(actions))
            .children(html_toggle.filter(|_| {
                !html_has_rows || html_interactive || html_live || html_pending || tools
            }));
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
        if let Some(ordinal) = event["display_update_origin"]["ordinal"].as_u64() {
            body = body.child(
                div()
                    .id((ElementId::from(self.id.clone()), "display-origin"))
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .role(Role::Group)
                    .aria_label(format!("Display updated by execution {ordinal}"))
                    .child(format!("Updated by execution {ordinal}")),
            );
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
                                "Bytes {}\u{2013}{} of {}",
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
        let mut save_row = h_flex().gap_2().flex_wrap().items_center();
        let mut has_save = false;
        for mime in [
            "audio/wav",
            "audio/mpeg",
            "audio/ogg",
            "video/mp4",
            "video/webm",
            "image/png",
            "image/jpeg",
        ] {
            if let Some(digest) = event["bundle"][mime]["blob"]
                .as_str()
                .filter(|digest| kernelledger_client::media::valid_digest(digest))
            {
                has_save = true;
                let digest = digest.to_owned();
                let controls = commands.clone();
                let directory = self.project.clone().unwrap_or_else(|| PathBuf::from("."));
                let filename = format!(
                    "output-{}.{}",
                    &digest[..digest.len().min(12)],
                    kernelledger_client::media::extension(mime).unwrap()
                );
                save_row = save_row.child(
                    action_button(SharedString::from(format!("{}-save-{mime}", self.id)))
                        .outline()
                        .small()
                        .label(format!("Save {mime}\u{2026}"))
                        .unavailable(controls.is_none())
                        .hint("Save a verified copy to disk; open it in your system media player after saving.")
                        .on_click(move |_, _, cx| {
                            let chosen = cx.prompt_for_new_path(&directory, Some(&filename));
                            let controls = controls.clone();
                            let digest = digest.clone();
                            cx.spawn(async move |_| {
                                if let Some(commands) = controls {
                                    match chosen.await {
                                        Ok(Ok(Some(path))) => {
                                            let _ = commands.send(Command::SaveOutput {
                                                path,
                                                digest,
                                                mime: mime.into(),
                                            });
                                        }
                                        Ok(Ok(None)) => {}
                                        Ok(Err(error)) => {
                                            let _ = commands.send(Command::OutputError(format!(
                                                "The save location could not be selected: {error}"
                                            )));
                                        }
                                        Err(error) => {
                                            let _ = commands.send(Command::OutputError(format!(
                                                "The save dialog did not finish: {error}"
                                            )));
                                        }
                                    }
                                }
                            })
                            .detach();
                        }),
                );
            }
        }
        if has_save {
            body = body.child(save_row);
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
        let widget = &event["bundle"]["application/vnd.kernelledger.widget+json"]["value"];
        if widget["schema"] == "kernelledger.widget/1" {
            let active = event["imported"] != true
                && widget["worker_generation"]
                    .as_str()
                    .is_some_and(|generation| {
                        self.worker_generation.as_deref() == Some(generation)
                    });
            let kind = widget["kind"].as_str().unwrap_or("");
            let mut widget_row = h_flex().gap_2().flex_wrap().items_center().child(
                div()
                    .text_sm()
                    .font_weight(FontWeight::MEDIUM)
                    .child(widget["label"].as_str().unwrap_or("Widget").to_owned()),
            );
            if active {
                if kind == "checkbox" {
                    let model = widget.clone();
                    let controls = commands.clone();
                    widget_row = widget_row.child(
                        Checkbox::new(SharedString::from(format!("{}-widget-checkbox", self.id)))
                            .label(widget["label"].as_str().unwrap_or("Checkbox").to_owned())
                            .checked(widget["value"] == true)
                            .on_click(move |value: &bool, _, _| {
                                if let Some(commands) = &controls {
                                    let _ = commands.send(Command::WidgetEvent {
                                        model: model.clone(),
                                        value: json!(value),
                                    });
                                }
                            }),
                    );
                }
                let mut choices: Vec<(String, Value)> = Vec::new();
                match kind {
                    "button" => choices.push(("Click".into(), Value::Null)),
                    "checkbox" => {}
                    "dropdown" => {
                        for option in widget["options"].as_array().into_iter().flatten().take(100) {
                            if let Some(label) = option.as_str() {
                                choices.push((label.to_owned(), option.clone()));
                            }
                        }
                    }
                    "integer_slider" => {
                        if let Some(value) = widget["value"].as_i64() {
                            if value > widget["lower"].as_i64().unwrap_or(0) {
                                choices.push((format!("Decrease ({value})"), json!(value - 1)));
                            }
                            if value < widget["upper"].as_i64().unwrap_or(100) {
                                choices.push((format!("Increase ({value})"), json!(value + 1)));
                            }
                        }
                    }
                    "progress" => {
                        widget_row = widget_row.child(badge(
                            format!("{} / {}", widget["value"], widget["upper"]),
                            cx.theme().info,
                        ));
                    }
                    _ => {}
                }
                for (index, (label, value)) in choices.into_iter().enumerate() {
                    let model = widget.clone();
                    let commands = commands.clone();
                    widget_row = widget_row.child(
                        action_button(SharedString::from(format!("{}-widget-{index}", self.id)))
                            .outline()
                            .small()
                            .accessibility_label(format!(
                                "{}: {label}",
                                widget["label"].as_str().unwrap_or("Widget")
                            ))
                            .label(label)
                            .on_click(move |_, _, _| {
                                if let Some(commands) = &commands {
                                    let _ = commands.send(Command::WidgetEvent {
                                        model: model.clone(),
                                        value: value.clone(),
                                    });
                                }
                            }),
                    );
                }
            } else {
                widget_row = widget_row
                    .child(badge("Recorded snapshot", cx.theme().muted_foreground))
                    .child(info_tip(
                        SharedString::from(format!("{}-widget-tip", self.id)),
                        "Recorded widget snapshot. Recreate the widget in the current runtime to interact.",
                        cx,
                    ));
            }
            body = body.child(widget_row);
        }
        if html_interactive && !html_available {
            body = body.child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child("Interactive HTML preview unavailable; recorded data retained."),
            );
        }
        if let Some(note) = html_note {
            body = body.child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(note),
            );
        }
        if let Some(view) = html_view {
            let mut sizing = h_flex().gap_2().flex_wrap();
            for (id, label, delta) in [
                ("html-shorter", "Shorter", Some(-120.)),
                ("html-taller", "Taller", Some(120.)),
                ("html-fit", "Fit content", None),
            ] {
                let workspace = self.workspace.clone();
                let key = self.id.to_string();
                sizing = sizing.child(
                    action_button((ElementId::from(self.id.clone()), id))
                        .small()
                        .ghost()
                        .label(label)
                        .on_click(move |_, _, cx| {
                            if let Some(workspace) = &workspace {
                                workspace
                                    .update(cx, |ws, cx| ws.resize_html_output(&key, delta, cx));
                            }
                        }),
                );
            }
            body = body
                .child(sizing)
                .child(html_output::host(view, html_blocked, html_height));
            return body.into_any_element();
        }
        if html_pending {
            return body.into_any_element();
        }
        let prefer_rich = image_path.is_some()
            || event["bundle"]["text/latex"]["value"].is_string()
            || event["bundle"]["application/vnd.kernelledger.table+json"]["value"]["rows"]
                .is_array()
            || event["bundle"]["application/vnd.kernelledger.profile+json"]["value"]["rows"]
                .is_array()
            || event["bundle"]["text/html"]["rows"].is_array()
            || html_interactive
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
                            h_flex()
                                .gap_1()
                                .items_center()
                                .child(
                                    div()
                                        .text_xs()
                                        .text_color(cx.theme().muted_foreground)
                                        .child("Page view"),
                                )
                                .child(info_tip(
                                    (ElementId::from(self.id.clone()), "page-tip"),
                                    "Copy and Find use this page. Recorded output is unchanged.",
                                    cx,
                                )),
                        );
                    }
                } else if text.len() > limit {
                    body = body.child(
                        h_flex()
                            .gap_1()
                            .items_center()
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(cx.theme().muted_foreground)
                                    .child("Display truncated"),
                            )
                            .child(info_tip(
                                (ElementId::from(self.id.clone()), "limit-tip"),
                                "Display limited; Copy retains the complete loaded text. Recorded output is unchanged.",
                                cx,
                            )),
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
                            .child("Figure evicted from cache."),
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
        } else if html_interactive && !html_has_rows && html_available {
            body = body.child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(if allow_html {
                        format!("Press Render HTML to show this page. {html_network}")
                    } else {
                        "Enable interactive HTML in Settings, then press Render HTML.".to_owned()
                    }),
            );
        } else {
            body = body.child(output_inner(
                event,
                self.id.to_string(),
                commands.clone(),
                cx,
            ));
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

fn output_inner(
    event: &Value,
    id: String,
    commands: Option<std::sync::mpsc::Sender<Command>>,
    cx: &App,
) -> AnyElement {
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
    if let Some(source) = bundle["text/latex"]["value"].as_str() {
        return math_output::equation(source, id, cx);
    }
    let native_table = &bundle["application/vnd.kernelledger.table+json"]["value"];
    let native_rows = native_table["rows"]
        .as_array()
        .map(|rows| {
            let mut result = vec![native_table["columns"].clone()];
            result.extend(rows.iter().take(100).cloned());
            result
        })
        .or_else(|| {
            bundle["application/vnd.kernelledger.profile+json"]["value"]["rows"]
                .as_array()
                .map(|rows| {
                    let mut result = vec![json!([
                        "Function",
                        "Calls",
                        "Self seconds",
                        "Total seconds",
                        "Source"
                    ])];
                    result.extend(rows.iter().take(100).map(|row| {
                        json!([
                            row["function"].as_str().unwrap_or(""),
                            row["calls"].to_string(),
                            row["self_seconds"].to_string(),
                            row["total_seconds"].to_string(),
                            format!("{}:{}", row["file"].as_str().unwrap_or(""), row["line"])
                        ])
                    }));
                    result
                })
        });
    if let Some(rows) = native_rows
        .as_ref()
        .or_else(|| bundle["text/html"]["rows"].as_array())
    {
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
                let mut content = div()
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
                    .focus(|style| style.border_2().border_color(theme.ring));
                let location = index.checked_sub(1).filter(|_| column == 4).and_then(|row| {
                    let value = &bundle["application/vnd.kernelledger.profile+json"]["value"]["rows"][row];
                    kernelledger_client::recorded::Location::parse(value["file"].as_str()?, value["source_revision"].as_str()?, value["line"].as_u64()?).ok()
                });
                if let (Some(location), Some(controls)) = (location, commands.clone()) {
                    content = content.child(
                        action_button(SharedString::from(format!("{id}-source-{index}")))
                            .ghost()
                            .small()
                            .label(format!("Line {}", location.line))
                            .accessibility_label(format!("Open line {}", location.line))
                            .hint(text)
                            .on_click(move |_, _, _| {
                                let _ = controls.send(Command::RecordedSource(location.clone()));
                            }),
                    );
                } else {
                    content = content.child(text);
                }
                line = line.child(content);
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
                h_flex()
                    .px_3()
                    .py_2()
                    .gap_1()
                    .items_center()
                    .text_xs()
                    .text_color(theme.muted_foreground)
                    .child("Showing first 200 rows")
                    .child(info_tip(
                        SharedString::from(format!("{id}-table-limit-tip")),
                        "Showing the first 200 data rows. Recorded data is unchanged; use Plain text / find for additional rows.",
                        cx,
                    )),
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
                .child(format!("{name} \u{00b7} {mime}")),
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
                .self_start()
                .ghost()
                .small()
                .label("Retry preview")
                .accessibility_label("Try preview again")
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
                            .child(format!("Bytes {}\u{2013}{} of {total}", offset + 1, end)),
                    ),
            );
        }
    }
    block.into_any_element()
}
