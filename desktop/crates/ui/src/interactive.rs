//! Local editor assistance and optional live surfaces. Never starts a worker for a read.
use super::*;
use gpui_kit::component::input::{CompletionProvider, HoverProvider, Rope};
use kernelledger_client::{ClientIdentity, RuntimeClient, hash, profile_path};
use lsp_types::{
    CompletionContext, CompletionItem, CompletionResponse, CompletionTextEdit, Position, Range,
    TextEdit,
};

pub(super) struct NotebookCompletion {
    pub view: WeakEntity<Workspace>,
    pub tab: usize,
    pub editor: WeakEntity<EditorState>,
}

/// Text for one previewed element: its rendered preview, or its type when the
/// worker could not produce a passive preview for that kind of value.
fn preview_text(preview: &Value, kind: &Value) -> String {
    match preview.as_str() {
        Some(text) if !text.is_empty() => text.to_owned(),
        _ => format!("<{}>", kind.as_str().unwrap_or("value")),
    }
}

fn position(source: &str, offset: usize) -> Position {
    let prefix: String = source.chars().take(offset).collect();
    Position::new(
        prefix.bytes().filter(|byte| *byte == b'\n').count() as u32,
        prefix
            .rsplit('\n')
            .next()
            .unwrap_or("")
            .encode_utf16()
            .count() as u32,
    )
}

fn static_completion(source: &str, cursor: usize) -> Value {
    let prefix: String = source.chars().take(cursor).collect();
    let stem = prefix
        .rsplit(|character: char| !unicode_ident::is_xid_continue(character))
        .next()
        .unwrap_or("");
    let words = "False None True and as assert async await break class continue def del elif else except finally for from global if import in is lambda nonlocal not or pass raise return try while with yield print range len sum list dict set str int float enumerate zip sorted open display";
    let mut names = words
        .split_whitespace()
        .chain(source.split(|character: char| !unicode_ident::is_xid_continue(character)))
        .filter(|word| word.starts_with(stem) && !word.is_empty() && word.len() <= 1024)
        .map(str::to_owned)
        .collect::<std::collections::BTreeSet<_>>();
    names.remove(stem);
    json!({"start":cursor-stem.chars().count(),"end":cursor,"candidates":names.into_iter().take(100).collect::<Vec<_>>(),"origin":"static"})
}

impl CompletionProvider for NotebookCompletion {
    fn completions(
        &self,
        text: &Rope,
        offset: usize,
        _: CompletionContext,
        _: &mut Window,
        cx: &mut App,
    ) -> Task<anyhow::Result<CompletionResponse>> {
        let source = text.to_string();
        if source.len() > 65536 || !source.is_char_boundary(offset) {
            return Task::ready(Ok(CompletionResponse::Array(vec![])));
        }
        let cursor = source[..offset].chars().count();
        let current = self.view.upgrade().and_then(|view| {
            view.read(cx)
                .tabs
                .get(self.tab)
                .map(|tab| (tab.model.project.clone(), tab.runtime.clone()))
        });
        let editor = self.editor.clone();
        let view = self.view.clone();
        let tab_index = self.tab;
        cx.spawn(async move |cx| {
            let expected = source.clone();
            let value = cx
                .background_executor()
                .spawn(async move {
                    let fallback = static_completion(&source, cursor);
                    let Some((project, runtime)) = current else {
                        return fallback;
                    };
                    let Some(generation) = runtime["interactive"]["worker_generation"].as_str()
                    else {
                        return fallback;
                    };
                    if runtime["worker_state"] != "idle" {
                        return fallback;
                    }
                    let result = (|| -> anyhow::Result<Value> {
                        let identity = ClientIdentity::load(&profile_path())?;
                        let mut client = RuntimeClient::connect(&project, &identity, "control")?;
                        let reply = client.request(
                            "code.complete",
                            json!({"worker_generation":generation,"source":source,
                        "source_revision":hash(source.as_bytes()),"cursor":cursor}),
                        )?;
                        anyhow::ensure!(reply["error"].is_null(), "Live completion unavailable");
                        Ok(reply)
                    })();
                    result.unwrap_or(fallback)
                })
                .await;
            let generation_current = value["origin"] == "static"
                || view.upgrade().is_some_and(|view| {
                    view.read_with(cx, |view, _| {
                        view.tabs.get(tab_index).is_some_and(|tab| {
                            tab.runtime["interactive"]["worker_generation"]
                                == value["worker_generation"]
                                && tab.runtime["interactive"]["namespace_epoch"]
                                    == value["namespace_epoch"]
                        })
                    })
                });
            let current = generation_current
                && editor.upgrade().is_some_and(|editor| {
                    editor.read_with(cx, |editor, _| {
                        editor.value() == expected && editor.cursor() == offset
                    })
                });
            if !current {
                return Ok(CompletionResponse::Array(vec![]));
            }
            let start = value["start"].as_u64().unwrap_or(cursor as u64) as usize;
            let end = value["end"].as_u64().unwrap_or(cursor as u64) as usize;
            let range = Range::new(position(&expected, start), position(&expected, end));
            let items = value["candidates"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .map(|name| CompletionItem {
                    label: name.to_owned(),
                    detail: Some(
                        if value["origin"] == "static" {
                            "Local source"
                        } else {
                            "Idle notebook namespace"
                        }
                        .into(),
                    ),
                    text_edit: Some(CompletionTextEdit::Edit(TextEdit::new(
                        range,
                        name.to_owned(),
                    ))),
                    ..Default::default()
                })
                .collect();
            Ok(CompletionResponse::Array(items))
        })
    }

    fn is_completion_trigger(&self, _: usize, text: &str, _: &mut App) -> bool {
        text.chars().last().is_some_and(|character| {
            character.is_alphabetic() || character == '_' || character == '.'
        })
    }
}

impl HoverProvider for NotebookCompletion {
    fn hover(
        &self,
        text: &Rope,
        offset: usize,
        _: &mut Window,
        cx: &mut App,
    ) -> Task<anyhow::Result<Option<lsp_types::Hover>>> {
        let source = text.to_string();
        if source.len() > 60000 || !source.is_char_boundary(offset) {
            return Task::ready(Ok(None));
        }
        let start = source[..offset]
            .rfind(|ch: char| !unicode_ident::is_xid_continue(ch) && ch != '.')
            .map_or(0, |index| {
                index + source[index..].chars().next().unwrap().len_utf8()
            });
        let end = source[offset..]
            .find(|ch: char| !unicode_ident::is_xid_continue(ch) && ch != '.')
            .map_or(source.len(), |index| offset + index);
        let name = source[start..end].to_owned();
        let current = self.view.upgrade().and_then(|view| {
            view.read(cx)
                .tabs
                .get(self.tab)
                .map(|tab| (tab.model.project.clone(), tab.runtime.clone()))
        });
        let editor = self.editor.clone();
        let view = self.view.clone();
        let tab_index = self.tab;
        cx.spawn(async move |cx| {
            let expected = source.clone();
            let value = cx.background_executor().spawn(async move {
                let (project, runtime) = current?;
                if runtime["worker_state"] != "idle" || name.is_empty() { return None; }
                let result = (|| -> anyhow::Result<Value> {
                    let identity = ClientIdentity::load(&profile_path())?;
                    let mut client = RuntimeClient::connect(&project, &identity, "control")?;
                    let reply = client.request("code.inspect", json!({"worker_generation":runtime["interactive"]["worker_generation"],"name":name}))?;
                    anyhow::ensure!(reply["error"].is_null(), "Help unavailable");
                    Ok(reply)
                })();
                result.ok()
            }).await;
            if value.as_ref().is_some_and(|value| !view.upgrade().is_some_and(|view| view.read_with(cx, |view, _|
                view.tabs.get(tab_index).is_some_and(|tab| tab.runtime["interactive"]["worker_generation"] == value["worker_generation"] && tab.runtime["interactive"]["namespace_epoch"] == value["namespace_epoch"])) )) {
                return Ok(None);
            }
            if !editor.upgrade().is_some_and(|editor| editor.read_with(cx, |editor, _| editor.value() == expected)) {
                return Ok(None);
            }
            Ok(value.map(|value| lsp_types::Hover {
                contents: lsp_types::HoverContents::Markup(lsp_types::MarkupContent {
                    kind: lsp_types::MarkupKind::PlainText,
                    value: format!("{}\n{}\n{}", value["signature"].as_str().unwrap_or(value["type"].as_str().unwrap_or("")), value["documentation"].as_str().unwrap_or(""), value["preview"].as_str().unwrap_or("")),
                }), range: None,
            }))
        })
    }
}

impl Workspace {
    pub(super) fn load_source_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let paths = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("Preview Python source".into()),
        });
        cx.spawn_in(window, async move |view, window| {
            if let Ok(Ok(Some(paths))) = paths.await
                && let Some(path) = paths.into_iter().next()
            {
                let _ = view.update_in(window, |view, _, _| {
                    view.send(Command::LoadSourcePreview { path })
                });
            }
        })
        .detach();
    }
    pub(super) fn toggle_breakpoint(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let tab = &mut self.tabs[self.active];
        let Some(id) = tab.model.selected.clone() else {
            return;
        };
        let Some(editor) = tab.editors.get(&id).cloned() else {
            return;
        };
        let cell = &tab.model.cells[&id];
        if cell.save != SaveState::Saved {
            return;
        }
        let cursor = editor.read(cx).cursor();
        let line = cell
            .text
            .get(..cursor)
            .unwrap_or("")
            .bytes()
            .filter(|byte| *byte == b'\n')
            .count()
            + 1;
        let revision = cell.disk.revision.clone();
        self.toggle_breakpoint_line(&id, line as u64, &revision, cx);
        editor.update(cx, |editor, cx| editor.focus(window, cx));
    }
    pub(super) fn console_editor(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Entity<EditorState> {
        let key = self.tabs[self.active].model.project.display().to_string();
        if let Some(editor) = self.console_editors.get(&key) {
            return editor.clone();
        }
        let editor = cx.new(|cx| EditorState::new(window, cx).language("python"));
        let provider = std::rc::Rc::new(NotebookCompletion {
            view: cx.entity().downgrade(),
            tab: self.active,
            editor: editor.downgrade(),
        });
        editor.update(cx, |editor, _| {
            editor.lsp_mut().completion_provider = Some(provider.clone());
            editor.lsp_mut().hover_provider = Some(provider);
        });
        self.console_editors.insert(key, editor.clone());
        editor
    }
    pub(super) fn interactivity_settings(&self, cx: &mut Context<Self>) -> Div {
        let attached = self
            .tabs
            .get(self.active)
            .is_some_and(|tab| tab.runtime["worker_state"] == "idle");
        let mut actions = h_flex().gap_2().flex_wrap().items_center();
        for (command, label, tip) in [
            (
                "on",
                "Enable shortcuts",
                "Cache up to 100 recent results (max 8 MiB) in the active session",
            ),
            (
                "clear",
                "Clear result cache",
                "Evict cached result bindings from the active session",
            ),
            (
                "off",
                "Disable shortcuts",
                "Stop caching result bindings for subsequent executions",
            ),
        ] {
            actions = actions.child(
                action_button(SharedString::from(format!("history-cache-{command}")))
                    .outline()
                    .small()
                    .label(label)
                    .hint(tip)
                    .unavailable(!attached)
                    .on_click(cx.listener(move |view, _, _, _| {
                        view.send(Command::Console {
                            source: format!("%history {command}"),
                        })
                    })),
            );
        }
        settings_group_with(
            "Result shortcuts",
            "Optional session cache: up to 100 results (8 MiB) of supported built-in values. Existing user names are preserved and cache actions are recorded executions.",
            cx,
        )
        .child(actions)
    }
    pub(super) fn interactive_request(&self, method: &str, mut fields: Value) {
        if let Some(tab) = self.tabs.get(self.active) {
            fields["worker_generation"] = tab.runtime["interactive"]["worker_generation"].clone();
            self.send(Command::Interactive {
                method: method.into(),
                fields,
            });
        }
    }

    /// Render a live value page as rows.
    ///
    /// `namespace.preview` returns bounded pages of element previews (or
    /// key/value pairs for mappings). Printing the reply as JSON put quotes,
    /// braces and escaped newlines in front of the value the reader asked for,
    /// so each element becomes a row and a non-pageable value falls back to its
    /// short type summary.
    pub(super) fn variable_page_rows(&self, preview: &Value, cx: &mut Context<Self>) -> Div {
        let muted = cx.theme().muted_foreground;
        let page = &preview["page"];
        let mut rows = v_flex().gap_1();
        if page["unsupported"] == true {
            return rows.child(
                div()
                    .text_sm()
                    .text_color(muted)
                    .child("Preview page unavailable for this type."),
            );
        }
        let items = page["items"].as_array().cloned().unwrap_or_default();
        if items.is_empty() {
            return rows.child(div().text_sm().text_color(muted).child("Empty value."));
        }
        let offset = page["offset"].as_u64().unwrap_or(0);
        for (index, item) in items.iter().enumerate() {
            let position = offset as usize + index;
            let (key, value) = if let Some(key) = item.get("key") {
                (
                    Some(preview_text(&key["preview"], &key["type"])),
                    preview_text(&item["value"]["preview"], &item["value"]["type"]),
                )
            } else {
                (None, preview_text(&item["preview"], &item["type"]))
            };
            rows = rows.child(
                h_flex()
                    .gap_3()
                    .items_start()
                    .child(
                        div()
                            .flex_none()
                            .text_xs()
                            .text_color(muted)
                            .child(format!("{position}")),
                    )
                    .when_some(key, |d, key| {
                        d.child(
                            div()
                                .flex_none()
                                .text_sm()
                                .font_weight(FontWeight::MEDIUM)
                                .child(key),
                        )
                    })
                    .child(div().min_w_0().text_sm().whitespace_normal().child(value)),
            );
        }
        if let Some(total) = page["total"].as_u64() {
            rows = rows.child(div().text_xs().text_color(muted).child(format!(
                "Showing {} of {total}",
                offset as usize + items.len()
            )));
        }
        rows
    }

    pub(super) fn interactive_page(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let tab = &self.tabs[self.active];
        let runtime = tab.runtime.clone();
        let value = tab
            .inspections
            .get("Variables")
            .cloned()
            .unwrap_or(Value::Null);
        let stale = value["worker_generation"]
            .as_str()
            .is_some_and(|generation| {
                Some(generation) != runtime["interactive"]["worker_generation"].as_str()
            })
            || (value["namespace_epoch"].is_number()
                && value["namespace_epoch"] != runtime["interactive"]["namespace_epoch"]);
        let theme = cx.theme();
        let (muted, warning, mono) = (
            theme.muted_foreground,
            theme.warning,
            theme.mono_font_family.clone(),
        );
        let worker_state = runtime["worker_state"].as_str().unwrap_or("offline");
        let mut body = v_flex()
            .w_full()
            .max_w(rems(76.))
            .mx_auto()
            .gap_4()
            .p_5()
            .child(
                h_flex()
                    .justify_between()
                    .items_center()
                    .gap_3()
                    .flex_wrap()
                    .child(page_heading(self.surface.clone(), "", cx))
                    .child(
                        h_flex()
                            .gap_2()
                            .items_center()
                            .child(
                                div()
                                    .text_xs()
                                    .font_family(mono.clone())
                                    .text_color(muted)
                                    .child(format!(
                                        "Session {}",
                                        short_session(runtime["session_id"].as_str().unwrap_or(""))
                                    )),
                            )
                            .when(notable_status(worker_state), |d| {
                                d.child(badge(
                                    outcome_label(worker_state).to_string(),
                                    tone(worker_state, cx),
                                ))
                            }),
                    ),
            );
        if self.surface == "Source preview" {
            let preview = tab
                .inspections
                .get("Source preview")
                .cloned()
                .unwrap_or(Value::Null);
            let source = preview["source"].as_str().unwrap_or("").to_owned();
            let path_label = preview["path"].as_str().unwrap_or("").to_owned();
            body = body.child(
                card(cx)
                    .p_4()
                    .gap_3()
                    .when(!path_label.is_empty(), |d| {
                        d.child(
                            div()
                                .text_xs()
                                .font_family(mono)
                                .text_color(muted)
                                .child(path_label),
                        )
                    })
                    .child(
                        TextView::markdown(
                            "source-file-preview",
                            format!("```python\n{}\n```", source.replace("```", "~~~")),
                        )
                        .selectable(true),
                    )
                    .child(
                        h_flex()
                            .gap_2()
                            .flex_wrap()
                            .items_center()
                            .child(
                                action_button("insert-preview-source")
                                    .primary()
                                    .small()
                                    .icon(Glyph::Plus)
                                    .label("Insert as cell")
                                    .accessibility_label("Insert as a new cell")
                                    .hint("Append this file's contents as a new code cell")
                                    .on_click(cx.listener(move |view, _, _, cx| {
                                        let index = view.tabs[view.active].model.order.len();
                                        view.mutate(
                                            json!({"action":"add","type":"code","source":source,"index":index}),
                                        );
                                        view.select_surface("Notebook", cx);
                                    })),
                            )
                            .child(
                                action_button("cancel-source-preview")
                                    .outline()
                                    .small()
                                    .label("Cancel")
                                    .on_click(cx.listener(|view, _, window, cx| {
                                        view.select_surface("Notebook", cx);
                                        view.focus.focus(window, cx);
                                    })),
                            ),
                    ),
            );
        } else if self.surface == "Debugger" {
            let controller = self.tabs[self.active]
                .inspections
                .entry("Debug controller".into())
                .or_insert_with(|| json!(uuid::Uuid::new_v4().to_string()))
                .clone();
            let mut toolbar = h_flex().gap_2().flex_wrap().items_center();
            for (command, label, icon) in [
                ("continue", "Continue", Some(Glyph::Play)),
                ("next", "Step over", None),
                ("step", "Step into", None),
                ("out", "Step out", None),
                ("stop", "Stop debugging", Some(Glyph::Square)),
            ] {
                let controller = controller.clone();
                let mut btn = action_button(SharedString::from(format!("debug-{command}")))
                    .outline()
                    .small()
                    .label(label)
                    .unavailable(
                        runtime["worker_state"] != "paused"
                            || (runtime["debug"]["post_mortem"] == true
                                && !["continue", "stop"].contains(&command)),
                    );
                if let Some(ic) = icon {
                    btn = btn.icon(ic);
                }
                toolbar = toolbar.child(btn.on_click(cx.listener(move |view, _, _, _| {
                    view.interactive_request(
                        "debug.control",
                        json!({"command":command,"controller_id":controller}),
                    )
                })));
            }
            toolbar = toolbar.child(info_tip(
                "debugger-controls-tip",
                "Checkpoint and restore are unavailable while paused. Interrupt and terminate remain available in runtime controls.",
                cx,
            ));
            if runtime["debug"]["post_mortem"] == true {
                toolbar = toolbar.child(
                    div()
                        .id("post-mortem-badge")
                        .child(badge("Post-mortem frame", warning))
                        .tooltip(|_, cx| {
                            cx.new(|_| {
                                WorkbenchHint(
                                    "Paused at the failed frame. Inspect locals, then Continue or Stop to finish the failed execution.".into(),
                                )
                            })
                            .into()
                        }),
                );
            }
            body = body.child(card(cx).p_3().child(toolbar));
            if runtime["debug"].is_object() {
                body = body.child(self.debug_frame_links(cx));
                let context = runtime["debug"]["source_context"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .map(|line| {
                        format!(
                            "{} {:>5}  {}",
                            if line["current"] == true {
                                "\u{2192}"
                            } else {
                                " "
                            },
                            line["line"].as_u64().unwrap_or(0),
                            line["text"].as_str().unwrap_or("").trim_end()
                        )
                    })
                    .collect::<Vec<_>>()
                    .join("\n");
                let mut source_card = card(cx).p_3().gap_2().child(
                    TextView::markdown(
                        "debug-source-context",
                        format!("```text\n{}\n```", context.replace("```", "~~~")),
                    )
                    .selectable(true),
                );
                if let Some(file) = runtime["debug"]["stack"][0]["file"].as_str()
                    && self.tabs[self.active].model.cells.iter().any(|(id, cell)| {
                        file.starts_with(&format!("cells/{id}.py@"))
                            && hash(cell.text.as_bytes())
                                != runtime["debug"]["source_revision"].as_str().unwrap_or("")
                    })
                {
                    source_card = source_card.child(
                        h_flex()
                            .gap_1p5()
                            .items_center()
                            .child(badge("Source modified since run", warning))
                            .child(info_tip(
                                "debug-source-diff-tip",
                                "The current editor source differs from this executed revision. The arrow and breakpoints refer to the recorded source above.",
                                cx,
                            )),
                    );
                }
                body = body.child(source_card);
                let locals = runtime["debug"]["locals"]
                    .as_array()
                    .cloned()
                    .unwrap_or_default();
                let mut frame_locals = card(cx).p_4().gap_2().child(
                    div()
                        .text_sm()
                        .font_weight(FontWeight::SEMIBOLD)
                        .child("Frame locals"),
                );
                if locals.is_empty() {
                    frame_locals = frame_locals.child(
                        div()
                            .text_sm()
                            .text_color(muted)
                            .child("No local bindings in this frame."),
                    );
                }
                for item in locals.iter().filter(|item| item["name"].is_string()) {
                    frame_locals =
                        frame_locals.child(
                            h_flex()
                                .gap_3()
                                .items_start()
                                .child(
                                    div()
                                        .flex_none()
                                        .text_sm()
                                        .font_weight(FontWeight::MEDIUM)
                                        .child(item["name"].as_str().unwrap_or("").to_owned()),
                                )
                                .child(
                                    div().flex_none().text_xs().text_color(muted).child(
                                        item["type"].as_str().unwrap_or("unknown").to_owned(),
                                    ),
                                )
                                .child(
                                    div().min_w_0().text_sm().whitespace_normal().child(bounded(
                                        item["preview"].as_str().unwrap_or(""),
                                        4096,
                                    )),
                                ),
                        );
                }
                body = body.child(frame_locals);
                let show_frames = self.tabs[self.active]
                    .raw_details
                    .get("debug-frames")
                    .copied()
                    .unwrap_or(false);
                body = body.child(
                    action_button("debug-frame-data")
                        .self_start()
                        .ghost()
                        .small()
                        .label(if show_frames {
                            "Hide frame data"
                        } else {
                            "Frame data"
                        })
                        .on_click(cx.listener(|view, _, _, cx| {
                            let flag = view.tabs[view.active]
                                .raw_details
                                .entry("debug-frames".into())
                                .or_default();
                            *flag = !*flag;
                            cx.notify();
                        })),
                );
                if show_frames {
                    body = body.child(
                        TextView::markdown(
                            "debug-frames",
                            format!(
                                "```json\n{}\n```",
                                bounded(
                                    &serde_json::to_string_pretty(&runtime["debug"])
                                        .unwrap_or_default(),
                                    65536
                                )
                                .replace("```", "~~~")
                            ),
                        )
                        .selectable(true),
                    );
                }
            } else {
                body =
                    body.child(
                        v_flex()
                            .gap_1()
                            .child(empty_row("No active debug session", cx))
                            .child(div().text_xs().text_color(muted).child(
                                "Run Debug selected cell from the command palette to begin.",
                            )),
                    );
            }
        } else if self.surface == "Console" {
            let input = self.console_editor(window, cx);
            let text = input.clone();
            body = body.child(
                card(cx)
                    .p_4()
                    .gap_3()
                    .child(
                        Editor::new(&input)
                            .aria_label("Python code (same notebook worker)")
                            .h(px(120.)),
                    )
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
                                    .flex_wrap()
                                    .child(
                                        action_button("console-run")
                                            .primary()
                                            .small()
                                            .icon(Glyph::Play)
                                            .label("Run in notebook session")
                                            .unavailable(runtime["session_id"].is_null())
                                            .hint("Run in the attached notebook worker (recorded in History)")
                                            .on_click(cx.listener(move |view, _, _, cx| {
                                                let source = text.read(cx).value().to_string();
                                                if !source.trim().is_empty() {
                                                    view.send(Command::Console { source });
                                                }
                                            })),
                                    )
                                    .child(
                                        action_button("console-history")
                                            .outline()
                                            .small()
                                            .icon(Glyph::Clock)
                                            .label("History")
                                            .accessibility_label("View console history")
                                            .hint("Open execution history")
                                            .on_click(cx.listener(|view, _, _, cx| {
                                                view.select_surface("History", cx)
                                            })),
                                    ),
                            )
                            .child(info_tip(
                                "console-help-tip",
                                "Console executions are recorded in History. Enter inserts a newline; click Run to execute. Closing this view preserves live variables.",
                                cx,
                            )),
                    ),
            );
            if let Some(record) = self.tabs[self.active].inspections.get("Console output") {
                let status = record["status"].as_str().unwrap_or("unknown");
                let mut out_card = card(cx)
                    .p_4()
                    .gap_3()
                    .child(
                        h_flex()
                            .justify_between()
                            .items_center()
                            .gap_2()
                            .child(
                                div()
                                    .text_sm()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child("Latest execution"),
                            )
                            .when(notable_status(status), |d| {
                                d.child(badge(outcome_label(status).to_string(), tone(status, cx)))
                            }),
                    )
                    .child(
                        TextView::markdown(
                            "console-recorded-source",
                            format!(
                                "```python\n{}\n```",
                                bounded(record["source"].as_str().unwrap_or(""), 65536)
                                    .replace("```", "~~~")
                            ),
                        )
                        .selectable(true),
                    );
                for (index, event) in record["outputs"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .take(100)
                    .enumerate()
                {
                    out_card = out_card.child(outputs::OutputBlock {
                        id: format!(
                            "console-{}-{index}",
                            record["exec_id"].as_str().unwrap_or("")
                        )
                        .into(),
                        event: event.clone(),
                        context: format!("Console output {}", index + 1).into(),
                        tools_state: None,
                        commands: Some(self.tabs[self.active].service.commands.clone()),
                        cell_id: "".into(),
                        exec_id: record["exec_id"].as_str().unwrap_or("").into(),
                        index,
                        worker_generation: None,
                        project: Some(self.tabs[self.active].model.project.clone()),
                    });
                }
                body = body.child(out_card);
            }
        } else {
            let mut header_bar = h_flex()
                .gap_2()
                .items_center()
                .flex_wrap()
                .child(
                    action_button("variables-refresh")
                        .outline()
                        .small()
                        .icon(Glyph::RotateCcw)
                        .label("Refresh live variables")
                        .unavailable(runtime["worker_state"] != "idle")
                        .hint("Inspect live namespace values when the worker is idle (starts no runtime)")
                        .on_click(cx.listener(|view, _, _, _| {
                            view.interactive_request("namespace.list", json!({}))
                        })),
                )
                .child(info_tip(
                    "variables-panel-tip",
                    "Reads live namespace bindings while the worker is idle. Opening this panel never starts a runtime.",
                    cx,
                ));
            if stale {
                header_bar = header_bar.child(
                    div()
                        .id("variables-stale-badge")
                        .child(badge("Cached snapshot", warning))
                        .tooltip(|_, cx| {
                            cx.new(|_| {
                                WorkbenchHint(
                                    "The namespace changed since this snapshot. Refresh to inspect current values.".into(),
                                )
                            })
                            .into()
                        }),
                );
            }
            body = body.child(header_bar);
            if let Some(error) = value["error"]["message"].as_str() {
                body = body.child(
                    div()
                        .text_sm()
                        .text_color(theme.danger)
                        .child(error.to_owned()),
                );
            }
            let items = value["items"].as_array().cloned().unwrap_or_default();
            if !items.is_empty() {
                let mut list_card = card(cx).p_2().gap_1();
                let count = items.len();
                let scroll = self.tabs[self.active].variables_scroll.clone();
                let view = cx.entity();
                list_card = list_card.child(
                    uniform_list("variables-list", count, move |range, _, _cx| {
                        range
                            .map(|index| {
                                let item = &items[index];
                                let handle = item["handle"].clone();
                                let label = format!(
                                    "{} \u{00b7} {} \u{00b7} {}",
                                    item["name"].as_str().unwrap_or(""),
                                    item["type"].as_str().unwrap_or(""),
                                    item["preview"].as_str().unwrap_or("")
                                );
                                let view = view.clone();
                                div().h(rems(2.0)).flex().items_center().child(
                                    action_button(("variable", index))
                                        .self_start()
                                        .ghost()
                                        .small()
                                        .label(label)
                                        .on_click(move |_, _, cx| {
                                            view.update(cx, |view, _| {
                                                view.interactive_request(
                                                    "namespace.preview",
                                                    json!({"handle":handle}),
                                                );
                                            });
                                        }),
                                )
                            })
                            .collect()
                    })
                    .track_scroll(&scroll)
                    .h(rems((count as f32 * 2.0).min(18.0))),
                );
                if let Some(cursor) = value["cursor"].as_str() {
                    let cursor = cursor.to_owned();
                    list_card = list_card.child(
                        action_button("variables-next")
                            .self_start()
                            .outline()
                            .small()
                            .label("Next variables")
                            .on_click(cx.listener(move |view, _, _, _| {
                                view.interactive_request("namespace.list", json!({"cursor":cursor}))
                            })),
                    );
                }
                body = body.child(list_card);
            } else if value.is_null() {
                body = body.child(
                    card(cx).p_4().child(
                        div()
                            .text_sm()
                            .text_color(muted)
                            .child("Click Refresh when the runtime is idle to inspect variables."),
                    ),
                );
            }
            if let Some(preview) = tab.inspections.get("Variable preview") {
                let name = preview["name"].as_str().unwrap_or("Value");
                let mut pages = card(cx).p_4().gap_2().child(section(
                    &format!(
                        "{name} \u{00b7} {}",
                        preview["type"].as_str().unwrap_or("unknown type")
                    ),
                    cx,
                ));
                if let Some(documentation) = preview["documentation"]
                    .as_str()
                    .filter(|text| !text.trim().is_empty())
                {
                    pages = pages.child(
                        div()
                            .text_sm()
                            .whitespace_normal()
                            .child(bounded(documentation, 16384)),
                    );
                }
                if let Some(signature) = preview["signature"].as_str() {
                    pages = pages.child(
                        TextView::markdown(
                            "variable-signature",
                            format!("```python\n{}\n```", signature.replace("```", "~~~")),
                        )
                        .selectable(true),
                    );
                }
                pages = pages.child(
                    div()
                        .text_sm()
                        .whitespace_normal()
                        .child(bounded(preview["preview"].as_str().unwrap_or(""), 65536)),
                );
                pages = pages.child(self.variable_page_rows(preview, cx));
                if let Some(offset) = preview["page"]["next_offset"].as_u64() {
                    let handle = preview["_context"]["handle"].clone();
                    pages = pages.child(
                        action_button("variable-next-page")
                            .self_start()
                            .outline()
                            .small()
                            .label("Next value page")
                            .on_click(cx.listener(move |view, _, _, _| {
                                view.interactive_request(
                                    "namespace.preview",
                                    json!({"handle":handle,"offset":offset}),
                                )
                            })),
                    );
                }
                body = body.child(pages);
            }
        }
        div()
            .id("interactive-panel")
            .size_full()
            .overflow_y_scroll()
            .child(body)
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::{position, static_completion};
    use lsp_types::Position;
    #[test]
    fn unicode_positions_and_static_fallback() {
        assert_eq!(position("\u{1f600}\u{03b1}\nname", 2), Position::new(0, 3));
        assert_eq!(position("\u{1f600}\u{03b1}\nname", 5), Position::new(1, 2));
        let value = static_completion(
            "\u{03b1}\u{03c1}\u{03b9}\u{03b8}\u{03bc}\u{03cc}\u{03c2} = 42\n\u{03b1}\u{03c1}\u{03b9}",
            16,
        );
        assert!(
            value["candidates"]
                .as_array()
                .unwrap()
                .iter()
                .any(|value| value == "\u{03b1}\u{03c1}\u{03b9}\u{03b8}\u{03bc}\u{03cc}\u{03c2}")
        );
    }
}
