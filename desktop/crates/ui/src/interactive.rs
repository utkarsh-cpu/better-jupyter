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
    pub tab_id: TabId,
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
                .tab_by_id(self.tab_id)
                .map(|tab| (tab.model.project.clone(), tab.runtime.clone()))
        });
        let editor = self.editor.clone();
        let view = self.view.clone();
        let tab_id = self.tab_id;
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
                        view.tab_by_id(tab_id).is_some_and(|tab| {
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
                .tab_by_id(self.tab_id)
                .map(|tab| (tab.model.project.clone(), tab.runtime.clone()))
        });
        let editor = self.editor.clone();
        let view = self.view.clone();
        let tab_id = self.tab_id;
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
                view.tab_by_id(tab_id).is_some_and(|tab| tab.runtime["interactive"]["worker_generation"] == value["worker_generation"] && tab.runtime["interactive"]["namespace_epoch"] == value["namespace_epoch"])) )) {
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
        let tab_id = self.tabs[self.active].id;
        let editor = cx.new(|cx| EditorState::new(window, cx).language("python"));
        let provider = std::rc::Rc::new(NotebookCompletion {
            view: cx.entity().downgrade(),
            tab_id,
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
        let mode_actions = h_flex()
            .gap_2()
            .flex_wrap()
            .items_center()
            .child(
                action_button("history-cache-on")
                    .outline()
                    .small()
                    .label("Enable shortcuts")
                    .hint("Cache up to 100 recent results (max 8 MiB) in the active session")
                    .unavailable(!attached)
                    .on_click(cx.listener(|view, _, _, _| {
                        view.send(Command::Console {
                            source: "%history on".into(),
                        })
                    })),
            )
            .child(
                action_button("history-cache-off")
                    .outline()
                    .small()
                    .label("Disable shortcuts")
                    .hint("Stop caching result bindings for subsequent executions")
                    .unavailable(!attached)
                    .on_click(cx.listener(|view, _, _, _| {
                        view.send(Command::Console {
                            source: "%history off".into(),
                        })
                    })),
            );
        let actions = h_flex()
            .gap_3()
            .flex_wrap()
            .items_center()
            .child(mode_actions)
            .child(status_rule(cx))
            .child(
                action_button("history-cache-clear")
                    .ghost()
                    .small()
                    .icon(Glyph::Trash)
                    .label("Clear result cache")
                    .hint("Evict cached result bindings from the active session")
                    .unavailable(!attached)
                    .on_click(cx.listener(|view, _, _, _| {
                        view.send(Command::Console {
                            source: "%history clear".into(),
                        })
                    })),
            );
        settings_group_with(
            "Result shortcuts",
            "Optional session cache: up to 100 results (8 MiB) of supported built-in values. Existing user names are preserved and cache actions are recorded executions.",
            cx,
        )
        .child(actions)
        .when(!attached, |d| {
            d.child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child("Available while the runtime is ready. Run a cell to start it."),
            )
        })
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

    /// Render a scrollable 2D grid for DataFrames (`mode == "table"`) and 2D/N-D arrays (`mode == "matrix"`).
    fn render_variable_grid(
        &self,
        preview: &Value,
        handle: Value,
        segments: Value,
        table: &Value,
        is_matrix: bool,
        cx: &mut Context<Self>,
    ) -> Div {
        let theme = cx.theme();
        let muted = theme.muted_foreground;
        let mono = theme.mono_font_family.clone();
        let border_col = divider(cx);
        let stripe_bg = theme.secondary.opacity(0.35);
        let header_bg = theme.secondary.opacity(0.65);

        let columns = table["columns"].as_array().cloned().unwrap_or_default();
        let rows = table["rows"].as_array().cloned().unwrap_or_default();
        let index_labels = table["index"].as_array().cloned().unwrap_or_default();
        let total_rows = table["total_rows"].as_u64().unwrap_or(rows.len() as u64);
        let total_cols = table["total_cols"].as_u64().unwrap_or(columns.len() as u64);
        let row_offset = table["row_offset"].as_u64().unwrap_or(0);
        let col_offset = table["col_offset"].as_u64().unwrap_or(0);
        let col_limit = table["col_limit"].as_u64().unwrap_or(16).max(1);
        let row_page_size = rows.len().max(1) as u64;
        let next_row = table["next_row_offset"].as_u64();
        let next_col = table["next_col_offset"].as_u64();
        let slice_prefix = preview["page"]["slice_prefix"].clone();
        let allow_device_copy = preview["_context"]["allow_device_copy"].as_bool() == Some(true);

        let mut container = v_flex().w_full().min_w_0().gap_2();

        // N-D slice steppers for >=3D arrays/tensors
        if is_matrix
            && let Some(lead_dims) = preview["page"]["lead_dims"].as_array()
            && !lead_dims.is_empty()
        {
            let prefix_vals: Vec<u64> = preview["page"]["slice_prefix"]
                .as_array()
                .map(|arr| arr.iter().filter_map(Value::as_u64).collect())
                .unwrap_or_else(|| vec![0; lead_dims.len()]);
            let mut stepper_bar = h_flex().gap_2().items_center().flex_wrap().child(
                div()
                    .text_xs()
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(muted)
                    .child("Slice:"),
            );
            for (axis, dim_val) in lead_dims.iter().enumerate() {
                let dim = dim_val.as_u64().unwrap_or(1).max(1);
                let cur = prefix_vals.get(axis).copied().unwrap_or(0);
                let mut prev_prefix = prefix_vals.clone();
                if axis < prev_prefix.len() {
                    prev_prefix[axis] = cur.saturating_sub(1);
                }
                let mut next_prefix = prefix_vals.clone();
                if axis < next_prefix.len() {
                    next_prefix[axis] = (cur + 1).min(dim.saturating_sub(1));
                }
                let h_prev = handle.clone();
                let s_prev = segments.clone();
                let h_next = handle.clone();
                let s_next = segments.clone();
                stepper_bar = stepper_bar.child(
                    h_flex()
                        .gap_1()
                        .items_center()
                        .px_1p5()
                        .py_0p5()
                        .rounded(px(6.))
                        .border_1()
                        .border_color(border_col)
                        .child(
                            div()
                                .text_xs()
                                .font_family(mono.clone())
                                .text_color(muted)
                                .child(format!("axis {axis}")),
                        )
                        .child(
                            action_button(("slice-dec", axis))
                                .ghost()
                                .xsmall()
                                .label("-")
                                .unavailable(cur == 0)
                                .on_click(cx.listener(move |view, _, _, _| {
                                    let mut req = json!({
                                        "handle": h_prev,
                                        "segments": s_prev,
                                        "slice_prefix": prev_prefix,
                                        "offset": 0,
                                        "col_offset": col_offset,
                                    });
                                    if allow_device_copy {
                                        req["allow_device_copy"] = json!(true);
                                    }
                                    view.interactive_request("namespace.preview", req);
                                })),
                        )
                        .child(
                            div()
                                .text_xs()
                                .font_family(mono.clone())
                                .child(format!("{cur}/{dim}")),
                        )
                        .child(
                            action_button(("slice-inc", axis))
                                .ghost()
                                .xsmall()
                                .label("+")
                                .unavailable(cur + 1 >= dim)
                                .on_click(cx.listener(move |view, _, _, _| {
                                    let mut req = json!({
                                        "handle": h_next,
                                        "segments": s_next,
                                        "slice_prefix": next_prefix,
                                        "offset": 0,
                                        "col_offset": col_offset,
                                    });
                                    if allow_device_copy {
                                        req["allow_device_copy"] = json!(true);
                                    }
                                    view.interactive_request("namespace.preview", req);
                                })),
                        ),
                );
            }
            container = container.child(stepper_bar);
        }

        if columns.is_empty() {
            return container.child(
                div()
                    .text_sm()
                    .text_color(muted)
                    .child("Empty table (0 columns)."),
            );
        }

        let col_width = if is_matrix { rems(5.5) } else { rems(6.8) };
        let idx_width = rems(3.2);

        let mut header_row = h_flex()
            .flex_none()
            .bg(header_bg)
            .border_b_1()
            .border_color(border_col)
            .child(
                div()
                    .w(idx_width)
                    .flex_none()
                    .px_2()
                    .py_1p5()
                    .border_r_1()
                    .border_color(border_col)
                    .text_xs()
                    .font_family(mono.clone())
                    .text_color(muted)
                    .child("#"),
            );

        for col in &columns {
            let cname = col["name"].as_str().unwrap_or("").to_owned();
            let cdtype = col["dtype"].as_str().unwrap_or("").to_owned();
            let numeric = matches!(col["kind"].as_str(), Some("integer" | "number"));
            header_row = header_row.child(
                v_flex()
                    .w(col_width)
                    .flex_none()
                    .px_2()
                    .py_1()
                    .border_r_1()
                    .border_color(border_col)
                    .child(
                        div()
                            .w_full()
                            .truncate()
                            .text_xs()
                            .font_weight(FontWeight::SEMIBOLD)
                            .font_family(mono.clone())
                            .when(numeric, |d| d.text_right())
                            .child(cname),
                    )
                    .when(!is_matrix && !cdtype.is_empty(), |d| {
                        d.child(
                            div()
                                .w_full()
                                .truncate()
                                .text_xs()
                                .font_family(mono.clone())
                                .text_color(muted)
                                .opacity(0.8)
                                .when(numeric, |d| d.text_right())
                                .child(cdtype),
                        )
                    }),
            );
        }

        let mut grid_rows = v_flex().min_w_full().child(header_row);
        if rows.is_empty() {
            grid_rows = grid_rows.child(
                div()
                    .px_3()
                    .py_3()
                    .text_xs()
                    .text_color(muted)
                    .child("0 rows"),
            );
        } else {
            for (r_idx, row_val) in rows.iter().enumerate() {
                let idx_text = index_labels
                    .get(r_idx)
                    .and_then(Value::as_str)
                    .map(str::to_owned)
                    .unwrap_or_else(|| format!("{}", row_offset as usize + r_idx));
                let mut row_el = h_flex()
                    .flex_none()
                    .when(r_idx % 2 == 1, |d| d.bg(stripe_bg))
                    .border_b_1()
                    .border_color(border_col)
                    .child(
                        div()
                            .w(idx_width)
                            .flex_none()
                            .truncate()
                            .px_2()
                            .py_1()
                            .border_r_1()
                            .border_color(border_col)
                            .text_xs()
                            .font_family(mono.clone())
                            .text_color(muted)
                            .child(idx_text),
                    );
                if let Some(cells) = row_val.as_array() {
                    for (c_idx, cell) in cells.iter().enumerate() {
                        let is_null = cell["null"].as_bool() == Some(true);
                        let col_numeric = columns.get(c_idx).is_some_and(|col| {
                            matches!(col["kind"].as_str(), Some("integer" | "number"))
                        });
                        let is_numeric = cell["numeric"].as_bool() == Some(true) || col_numeric;
                        let text = cell["text"].as_str().unwrap_or("").to_owned();
                        row_el = row_el.child(
                            div()
                                .w(col_width)
                                .flex_none()
                                .truncate()
                                .px_2()
                                .py_1()
                                .border_r_1()
                                .border_color(border_col)
                                .text_xs()
                                .font_family(mono.clone())
                                .when(is_numeric, |d| d.text_right())
                                .when(is_null, |d| d.text_color(muted).italic().opacity(0.75))
                                .child(text),
                        );
                    }
                }
                grid_rows = grid_rows.child(row_el);
            }
        }

        container = container.child(
            div()
                .id("variable-grid-scroll")
                .w_full()
                .max_h(rems(26.))
                .overflow_scroll()
                .rounded(px(6.))
                .border_1()
                .border_color(border_col)
                .child(grid_rows),
        );

        // Pagination footer
        let shown_row_start = if total_rows == 0 { 0 } else { row_offset + 1 };
        let shown_row_end = (row_offset + rows.len() as u64).min(total_rows);
        let shown_col_start = if total_cols == 0 { 0 } else { col_offset + 1 };
        let shown_col_end = (col_offset + columns.len() as u64).min(total_cols);

        let mut footer = h_flex()
            .w_full()
            .justify_between()
            .items_center()
            .gap_2()
            .flex_wrap()
            .child(
                div()
                    .text_xs()
                    .text_color(muted)
                    .font_family(mono)
                    .child(format!(
                        "Rows {shown_row_start}\u{2013}{shown_row_end} of {total_rows} \u{00b7} Cols {shown_col_start}\u{2013}{shown_col_end} of {total_cols}"
                    )),
            );

        let mut nav = h_flex().gap_1p5().items_center().flex_wrap();
        if row_offset > 0 || next_row.is_some() {
            let h1 = handle.clone();
            let s1 = segments.clone();
            let sp1 = slice_prefix.clone();
            let prev_off = row_offset.saturating_sub(row_page_size);
            nav = nav.child(
                action_button("var-grid-prev-rows")
                    .outline()
                    .xsmall()
                    .label("Prev rows")
                    .unavailable(row_offset == 0)
                    .on_click(cx.listener(move |view, _, _, _| {
                        let mut req = json!({
                            "handle": h1,
                            "segments": s1,
                            "offset": prev_off,
                            "col_offset": col_offset,
                            "col_limit": col_limit,
                        });
                        if sp1.is_array() {
                            req["slice_prefix"] = sp1.clone();
                        }
                        if allow_device_copy {
                            req["allow_device_copy"] = json!(true);
                        }
                        view.interactive_request("namespace.preview", req);
                    })),
            );
            let h2 = handle.clone();
            let s2 = segments.clone();
            let sp2 = slice_prefix.clone();
            let next_off = next_row.unwrap_or(row_offset);
            nav = nav.child(
                action_button("var-grid-next-rows")
                    .outline()
                    .xsmall()
                    .label("Next rows")
                    .unavailable(next_row.is_none())
                    .on_click(cx.listener(move |view, _, _, _| {
                        let mut req = json!({
                            "handle": h2,
                            "segments": s2,
                            "offset": next_off,
                            "col_offset": col_offset,
                            "col_limit": col_limit,
                        });
                        if sp2.is_array() {
                            req["slice_prefix"] = sp2.clone();
                        }
                        if allow_device_copy {
                            req["allow_device_copy"] = json!(true);
                        }
                        view.interactive_request("namespace.preview", req);
                    })),
            );
        }
        if col_offset > 0 || next_col.is_some() {
            let h3 = handle.clone();
            let s3 = segments.clone();
            let sp3 = slice_prefix.clone();
            let prev_c = col_offset.saturating_sub(col_limit);
            nav = nav.child(
                action_button("var-grid-prev-cols")
                    .outline()
                    .xsmall()
                    .label("Prev cols")
                    .unavailable(col_offset == 0)
                    .on_click(cx.listener(move |view, _, _, _| {
                        let mut req = json!({
                            "handle": h3,
                            "segments": s3,
                            "offset": row_offset,
                            "col_offset": prev_c,
                            "col_limit": col_limit,
                        });
                        if sp3.is_array() {
                            req["slice_prefix"] = sp3.clone();
                        }
                        if allow_device_copy {
                            req["allow_device_copy"] = json!(true);
                        }
                        view.interactive_request("namespace.preview", req);
                    })),
            );
            let h4 = handle;
            let s4 = segments;
            let sp4 = slice_prefix;
            let next_c = next_col.unwrap_or(col_offset);
            nav = nav.child(
                action_button("var-grid-next-cols")
                    .outline()
                    .xsmall()
                    .label("Next cols")
                    .unavailable(next_col.is_none())
                    .on_click(cx.listener(move |view, _, _, _| {
                        let mut req = json!({
                            "handle": h4,
                            "segments": s4,
                            "offset": row_offset,
                            "col_offset": next_c,
                            "col_limit": col_limit,
                        });
                        if sp4.is_array() {
                            req["slice_prefix"] = sp4.clone();
                        }
                        if allow_device_copy {
                            req["allow_device_copy"] = json!(true);
                        }
                        view.interactive_request("namespace.preview", req);
                    })),
            );
        }
        footer = footer.child(nav);
        container.child(footer)
    }

    /// Render marimo-style DataFrame column schema and optional summary stats.
    fn render_variable_schema(
        &self,
        preview: &Value,
        handle: Value,
        segments: Value,
        cx: &mut Context<Self>,
    ) -> Div {
        let theme = cx.theme();
        let muted = theme.muted_foreground;
        let mono = theme.mono_font_family.clone();
        let border_col = divider(cx);
        let stripe_bg = theme.secondary.opacity(0.35);
        let page = &preview["page"];
        let schema = page["schema"].as_array().cloned().unwrap_or_default();
        let is_lazy = page["lazy"].as_bool() == Some(true);
        let stats_included = page["stats_included"].as_bool() == Some(true);
        let has_stats = stats_included
            || schema
                .iter()
                .any(|c| c.get("unique").is_some() || c.get("min").is_some());

        let mut wrap = v_flex().w_full().min_w_0().gap_2();

        if !is_lazy {
            let mut bar = h_flex().justify_between().items_center().gap_2().child(
                div()
                    .text_xs()
                    .text_color(muted)
                    .child(format!("{} columns", schema.len())),
            );
            if !has_stats {
                let h = handle;
                let s = segments;
                let row_offset = page["table"]["row_offset"].as_u64().unwrap_or(0);
                let col_offset = page["table"]["col_offset"].as_u64().unwrap_or(0);
                bar = bar.child(
                    action_button("var-schema-compute-stats")
                        .outline()
                        .xsmall()
                        .label("Compute column stats")
                        .hint("Compute unique count, min, max, and mean on demand (bounded)")
                        .on_click(cx.listener(move |view, _, _, _| {
                            view.interactive_request(
                                "namespace.preview",
                                json!({
                                    "handle": h,
                                    "segments": s,
                                    "offset": row_offset,
                                    "col_offset": col_offset,
                                    "include_stats": true,
                                }),
                            );
                        })),
                );
            } else {
                bar = bar.child(badge("Summary stats loaded", theme.primary));
            }
            wrap = wrap.child(bar);
        }

        if schema.is_empty() {
            return wrap.child(
                div()
                    .text_sm()
                    .text_color(muted)
                    .child("No schema columns."),
            );
        }

        let mut list = v_flex()
            .w_full()
            .rounded(px(6.))
            .border_1()
            .border_color(border_col)
            .child(
                h_flex()
                    .w_full()
                    .px_2p5()
                    .py_1p5()
                    .gap_2()
                    .bg(theme.secondary.opacity(0.65))
                    .border_b_1()
                    .border_color(border_col)
                    .text_xs()
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(muted)
                    .child(div().w(rems(8.5)).flex_none().child("Column"))
                    .child(div().w(rems(6.5)).flex_none().child("Type"))
                    .when(!is_lazy, |r| {
                        r.child(div().w(rems(4.5)).flex_none().child("Nulls"))
                    })
                    .when(has_stats, |r| {
                        r.child(div().w(rems(11.)).flex_none().child("Stats"))
                    })
                    .child(div().flex_1().min_w_0().child("Samples")),
            );

        for (idx, col) in schema.iter().enumerate() {
            let cname = col["name"].as_str().unwrap_or("").to_owned();
            let cdtype = col["dtype"].as_str().unwrap_or("").to_owned();
            let nulls = col["null_count"]
                .as_u64()
                .map(|n| n.to_string())
                .unwrap_or_else(|| "\u{2014}".into());
            let samples = col["samples"]
                .as_array()
                .map(|arr| {
                    arr.iter()
                        .filter_map(Value::as_str)
                        .collect::<Vec<_>>()
                        .join(", ")
                })
                .unwrap_or_default();
            let mut stats_parts: Vec<String> = Vec::new();
            if let Some(u) = col["unique"].as_u64() {
                stats_parts.push(format!("{u} uniq"));
            }
            if let (Some(mn), Some(mx)) = (col["min"].as_str(), col["max"].as_str()) {
                stats_parts.push(format!("{mn}\u{2013}{mx}"));
            }
            if let Some(mean) = col["mean"].as_str() {
                stats_parts.push(format!("\u{03bc}={mean}"));
            }
            let stats_text = if stats_parts.is_empty() {
                "\u{2014}".to_owned()
            } else {
                stats_parts.join(" \u{00b7} ")
            };

            list = list.child(
                h_flex()
                    .w_full()
                    .px_2p5()
                    .py_1p5()
                    .gap_2()
                    .items_center()
                    .when(idx % 2 == 1, |d| d.bg(stripe_bg))
                    .border_b_1()
                    .border_color(border_col)
                    .child(
                        div()
                            .w(rems(8.5))
                            .flex_none()
                            .truncate()
                            .text_xs()
                            .font_family(mono.clone())
                            .font_weight(FontWeight::MEDIUM)
                            .child(cname),
                    )
                    .child(
                        div()
                            .w(rems(6.5))
                            .flex_none()
                            .truncate()
                            .text_xs()
                            .font_family(mono.clone())
                            .text_color(muted)
                            .child(cdtype),
                    )
                    .when(!is_lazy, |r| {
                        r.child(
                            div()
                                .w(rems(4.5))
                                .flex_none()
                                .text_xs()
                                .font_family(mono.clone())
                                .text_color(muted)
                                .child(nulls),
                        )
                    })
                    .when(has_stats, |r| {
                        r.child(
                            div()
                                .w(rems(11.))
                                .flex_none()
                                .truncate()
                                .text_xs()
                                .font_family(mono.clone())
                                .text_color(muted)
                                .child(stats_text),
                        )
                    })
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .text_xs()
                            .font_family(mono.clone())
                            .child(if samples.is_empty() {
                                "\u{2014}".into()
                            } else {
                                samples
                            }),
                    ),
            );
        }

        wrap.child(list)
    }

    /// Render a paged collection/mapping/set/attributes/series/1D-array value with nested drill-down.
    pub(super) fn variable_page_rows(&self, preview: &Value, cx: &mut Context<Self>) -> Div {
        let theme = cx.theme();
        let muted = theme.muted_foreground;
        let mono = theme.mono_font_family.clone();
        let border_col = divider(cx);
        let fg = theme.foreground;
        let page = &preview["page"];
        let mode = page["mode"].as_str().unwrap_or("");
        let handle = preview["_context"]["handle"].clone();
        let current_segments = preview["segments"].as_array().cloned().unwrap_or_default();

        let mut rows = v_flex().w_full().min_w_0().gap_2();
        if page["unsupported"] == true {
            return rows.child(
                div()
                    .text_sm()
                    .text_color(muted)
                    .child("Structured page preview is unavailable for this opaque type."),
            );
        }

        // Device tensor (GPU / MPS) passive guard + opt-in CPU slice button
        if mode == "device_tensor" {
            let device = page["device"].as_str().unwrap_or("device");
            let dtype = page["dtype"].as_str().unwrap_or("");
            let req_grad = page["requires_grad"].as_bool() == Some(true);
            let h = handle;
            let segs = Value::Array(current_segments);
            return rows.child(
                v_flex()
                    .p_3()
                    .gap_2()
                    .rounded(px(6.))
                    .border_1()
                    .border_color(border_col)
                    .child(
                        h_flex()
                            .gap_2()
                            .items_center()
                            .flex_wrap()
                            .child(badge(format!("Device: {device}"), theme.warning))
                            .when(!dtype.is_empty(), |d| {
                                d.child(badge(dtype.to_owned(), muted))
                            })
                            .when(req_grad, |d| d.child(badge("requires_grad", theme.primary))),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(muted)
                            .whitespace_normal()
                            .child("Tensor values live in accelerator memory and are never copied automatically."),
                    )
                    .child(
                        action_button("var-tensor-copy-cpu")
                            .outline()
                            .small()
                            .self_start()
                            .icon(Glyph::Cpu)
                            .label("Copy bounded slice to CPU")
                            .on_click(cx.listener(move |view, _, _, _| {
                                view.interactive_request(
                                    "namespace.preview",
                                    json!({
                                        "handle": h,
                                        "segments": segs,
                                        "allow_device_copy": true,
                                    }),
                                );
                            })),
                    ),
            );
        }

        // Long text chunk view
        if mode == "text" {
            let text = page["text"].as_str().unwrap_or("");
            let offset = page["offset"].as_u64().unwrap_or(0);
            let total = page["total"].as_u64().unwrap_or(text.len() as u64);
            let next_offset = page["next_offset"].as_u64();
            rows = rows.child(
                TextView::markdown(
                    SharedString::from(format!("var-text-chunk-{offset}")),
                    format!(
                        "```text\n{}\n```",
                        bounded(text, 8192).replace("```", "~~~")
                    ),
                )
                .selectable(true),
            );
            let shown_end = offset + text.chars().count() as u64;
            let h1 = handle.clone();
            let s1 = Value::Array(current_segments.clone());
            let h2 = handle;
            let s2 = Value::Array(current_segments);
            return rows.child(
                h_flex()
                    .justify_between()
                    .items_center()
                    .gap_2()
                    .child(
                        div()
                            .text_xs()
                            .font_family(mono)
                            .text_color(muted)
                            .child(format!(
                                "Chars {}\u{2013}{shown_end} of {total}",
                                offset + 1
                            )),
                    )
                    .child(
                        h_flex()
                            .gap_1p5()
                            .child(
                                action_button("var-text-prev")
                                    .outline()
                                    .xsmall()
                                    .label("Prev")
                                    .unavailable(offset == 0)
                                    .on_click(cx.listener(move |view, _, _, _| {
                                        view.interactive_request(
                                            "namespace.preview",
                                            json!({
                                                "handle": h1,
                                                "segments": s1,
                                                "offset": offset.saturating_sub(4000),
                                            }),
                                        );
                                    })),
                            )
                            .child(
                                action_button("var-text-next")
                                    .outline()
                                    .xsmall()
                                    .label("Next")
                                    .unavailable(next_offset.is_none())
                                    .on_click(cx.listener(move |view, _, _, _| {
                                        if let Some(off) = next_offset {
                                            view.interactive_request(
                                                "namespace.preview",
                                                json!({
                                                    "handle": h2,
                                                    "segments": s2,
                                                    "offset": off,
                                                }),
                                            );
                                        }
                                    })),
                            ),
                    ),
            );
        }

        // Bytes hex + ASCII view
        if mode == "bytes" {
            let offset = page["offset"].as_u64().unwrap_or(0);
            let total = page["total"].as_u64().unwrap_or(0);
            let next_offset = page["next_offset"].as_u64();
            let lines = page["hex_lines"]
                .as_array()
                .map(|arr| {
                    arr.iter()
                        .map(|line| {
                            format!(
                                "{:08x}  {:<47}  |{}|",
                                line["offset"].as_u64().unwrap_or(0),
                                line["hex"].as_str().unwrap_or(""),
                                line["ascii"].as_str().unwrap_or("")
                            )
                        })
                        .collect::<Vec<_>>()
                        .join("\n")
                })
                .unwrap_or_default();
            rows = rows.child(
                TextView::markdown(
                    SharedString::from(format!("var-bytes-chunk-{offset}")),
                    format!("```text\n{lines}\n```"),
                )
                .selectable(true),
            );
            let h1 = handle.clone();
            let s1 = Value::Array(current_segments.clone());
            let h2 = handle;
            let s2 = Value::Array(current_segments);
            return rows.child(
                h_flex()
                    .justify_between()
                    .items_center()
                    .gap_2()
                    .child(
                        div()
                            .text_xs()
                            .font_family(mono)
                            .text_color(muted)
                            .child(format!("Offset {offset} of {total} bytes")),
                    )
                    .child(
                        h_flex()
                            .gap_1p5()
                            .child(
                                action_button("var-bytes-prev")
                                    .outline()
                                    .xsmall()
                                    .label("Prev")
                                    .unavailable(offset == 0)
                                    .on_click(cx.listener(move |view, _, _, _| {
                                        view.interactive_request(
                                            "namespace.preview",
                                            json!({
                                                "handle": h1,
                                                "segments": s1,
                                                "offset": offset.saturating_sub(256),
                                            }),
                                        );
                                    })),
                            )
                            .child(
                                action_button("var-bytes-next")
                                    .outline()
                                    .xsmall()
                                    .label("Next")
                                    .unavailable(next_offset.is_none())
                                    .on_click(cx.listener(move |view, _, _, _| {
                                        if let Some(off) = next_offset {
                                            view.interactive_request(
                                                "namespace.preview",
                                                json!({
                                                    "handle": h2,
                                                    "segments": s2,
                                                    "offset": off,
                                                }),
                                            );
                                        }
                                    })),
                            ),
                    ),
            );
        }

        // Sequence / Mapping / Set / Attributes / Series / 1D Array
        let items = page["items"].as_array().cloned().unwrap_or_default();
        if items.is_empty() {
            return rows.child(div().text_sm().text_color(muted).child("Empty value."));
        }
        let offset = page["offset"].as_u64().unwrap_or(0);
        let next_offset = page["next_offset"].as_u64();
        let total = page["total"].as_u64();
        let is_keyed = items.first().is_some_and(|it| it.get("key").is_some());
        let key_heading = match mode {
            "mapping" => "Key",
            "attributes" => "Attribute",
            "series" => "Index",
            _ if is_keyed => "Field",
            _ => "Index",
        };

        let mut list = v_flex()
            .w_full()
            .rounded(px(6.))
            .border_1()
            .border_color(border_col)
            .child(
                h_flex()
                    .w_full()
                    .px_2p5()
                    .py_1()
                    .gap_2p5()
                    .bg(theme.secondary.opacity(0.65))
                    .border_b_1()
                    .border_color(border_col)
                    .text_xs()
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(muted)
                    .child(div().w(rems(7.5)).flex_none().child(key_heading))
                    .child(div().w(rems(5.5)).flex_none().child("Type"))
                    .child(div().flex_1().min_w_0().child("Value")),
            );

        for (index, item) in items.iter().enumerate() {
            let position = offset as usize + index;
            let (key_label, val_obj) = if let Some(key) = item.get("key") {
                (preview_text(&key["preview"], &key["type"]), &item["value"])
            } else {
                (format!("[{position}]"), item)
            };
            let val_type = variable_short_type(val_obj);
            let val_text = preview_text(&val_obj["preview"], &val_obj["type"]);
            let expandable =
                val_obj["expandable"].as_bool() == Some(true) && item.get("step").is_some();
            let step = item.get("step").cloned();

            if expandable && let Some(step_val) = step {
                let mut next_segs = current_segments.clone();
                next_segs.push(step_val);
                let h = handle.clone();
                list = list.child(
                    div().w_full().border_b_1().border_color(border_col).child(
                        action_button(("var-child-drill", index))
                            .ghost()
                            .small()
                            .variable_child_row(
                                key_label,
                                val_type,
                                val_text,
                                is_keyed,
                                mono.clone(),
                                fg,
                                muted,
                            )
                            .on_click(cx.listener(move |view, _, _, _| {
                                view.interactive_request(
                                    "namespace.preview",
                                    json!({
                                        "handle": h,
                                        "segments": next_segs,
                                        "offset": 0,
                                    }),
                                );
                            })),
                    ),
                );
            } else {
                list = list.child(
                    h_flex()
                        .w_full()
                        .min_w_0()
                        .px_2p5()
                        .py_1()
                        .gap_2p5()
                        .items_center()
                        .border_b_1()
                        .border_color(border_col)
                        .child(
                            div()
                                .w(rems(7.5))
                                .flex_none()
                                .truncate()
                                .text_xs()
                                .font_family(mono.clone())
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(if is_keyed { fg } else { muted })
                                .child(key_label),
                        )
                        .child(
                            div()
                                .w(rems(5.5))
                                .flex_none()
                                .truncate()
                                .text_xs()
                                .font_family(mono.clone())
                                .text_color(muted)
                                .child(val_type),
                        )
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .truncate()
                                .text_xs()
                                .font_family(mono.clone())
                                .child(val_text),
                        ),
                );
            }
        }

        rows = rows.child(list);

        if let Some(total_count) = total {
            let shown_start = offset + 1;
            let shown_end = offset + items.len() as u64;
            let h1 = handle.clone();
            let s1 = Value::Array(current_segments.clone());
            let h2 = handle;
            let s2 = Value::Array(current_segments);
            let mut footer = h_flex()
                .justify_between()
                .items_center()
                .gap_2()
                .flex_wrap()
                .child(
                    div()
                        .text_xs()
                        .font_family(mono)
                        .text_color(muted)
                        .child(format!(
                            "Showing {shown_start}\u{2013}{shown_end} of {total_count}"
                        )),
                );
            if offset > 0 || next_offset.is_some() {
                footer = footer.child(
                    h_flex()
                        .gap_1p5()
                        .child(
                            action_button("variable-prev-page")
                                .outline()
                                .xsmall()
                                .label("Previous page")
                                .unavailable(offset == 0)
                                .on_click(cx.listener(move |view, _, _, _| {
                                    view.interactive_request(
                                        "namespace.preview",
                                        json!({
                                            "handle": h1,
                                            "segments": s1,
                                            "offset": offset.saturating_sub(50),
                                        }),
                                    );
                                })),
                        )
                        .child(
                            action_button("variable-next-page")
                                .outline()
                                .xsmall()
                                .label("Next page")
                                .unavailable(next_offset.is_none())
                                .on_click(cx.listener(move |view, _, _, _| {
                                    if let Some(off) = next_offset {
                                        view.interactive_request(
                                            "namespace.preview",
                                            json!({
                                                "handle": h2,
                                                "segments": s2,
                                                "offset": off,
                                            }),
                                        );
                                    }
                                })),
                        ),
                );
            }
            rows = rows.child(footer);
        }
        rows
    }

    /// Render the complete detail inspector card for a selected variable (with breadcrumbs, tabs, and actions).
    fn render_variable_inspector(&mut self, preview: &Value, cx: &mut Context<Self>) -> Div {
        let theme = cx.theme();
        let muted = theme.muted_foreground;
        let warning = theme.warning;
        let mono = theme.mono_font_family.clone();

        let name = preview["name"].as_str().unwrap_or("Value").to_owned();
        let full_type = preview["type"].as_str().unwrap_or("unknown").to_owned();
        let short_type = variable_short_type(preview);
        let size_label = preview["size_label"].as_str().unwrap_or("").to_owned();
        let is_lazy = preview["lazy"].as_bool() == Some(true);
        let expr = breadcrumb_expression(preview);
        let raw_preview = preview["preview"].as_str().unwrap_or("").to_owned();
        let handle = preview["_context"]["handle"].clone();
        let segments = preview["segments"].clone();
        let breadcrumbs = preview["breadcrumbs"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        let page = &preview["page"];
        let mode = page["mode"].as_str().unwrap_or("");
        let detail_tab = self.tabs[self.active].variables_state.detail_tab.clone();

        let copy_expr = expr.clone();
        let copy_val = raw_preview.clone();
        let console_expr = expr;

        let mut panel = card(cx).p_4().gap_3().min_w_0().w_full();

        // Header bar: Name + Type + Size + Actions
        let header = h_flex()
            .w_full()
            .justify_between()
            .items_center()
            .gap_2()
            .flex_wrap()
            .child(
                h_flex()
                    .gap_2()
                    .items_center()
                    .flex_wrap()
                    .min_w_0()
                    .child(
                        div()
                            .text_sm()
                            .font_family(mono.clone())
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(name),
                    )
                    .child(
                        div()
                            .id("var-inspector-type-badge")
                            .child(badge(short_type, theme.primary))
                            .tooltip(move |_, cx| {
                                let t = full_type.clone();
                                cx.new(|_| WorkbenchHint(t.into())).into()
                            }),
                    )
                    .when(!size_label.is_empty(), |row| {
                        row.child(
                            div()
                                .text_xs()
                                .font_family(mono.clone())
                                .text_color(muted)
                                .child(size_label),
                        )
                    })
                    .when(is_lazy, |row| row.child(badge("Lazy", warning))),
            )
            .child(
                h_flex()
                    .gap_1()
                    .items_center()
                    .child(
                        action_button("var-copy-expr")
                            .ghost()
                            .xsmall()
                            .icon(Glyph::Copy)
                            .label("Copy expr")
                            .hint("Copy Python expression to clipboard")
                            .on_click(move |_, _, cx| copy_acknowledged(copy_expr.clone(), cx)),
                    )
                    .child(
                        action_button("var-copy-val")
                            .ghost()
                            .xsmall()
                            .label("Copy value")
                            .hint("Copy bounded preview text to clipboard")
                            .on_click(move |_, _, cx| copy_acknowledged(copy_val.clone(), cx)),
                    )
                    .child(
                        action_button("var-open-console")
                            .ghost()
                            .xsmall()
                            .icon(Glyph::Terminal)
                            .label("Console")
                            .hint("Open expression in Console")
                            .on_click(cx.listener(move |view, _, window, cx| {
                                let editor = view.console_editor(window, cx);
                                editor.update(cx, |ed, cx| {
                                    ed.set_value(console_expr.clone(), window, cx);
                                });
                                view.select_surface("Console", cx);
                            })),
                    )
                    .child(
                        action_button("var-close-inspector")
                            .ghost()
                            .xsmall()
                            .icon(Glyph::X)
                            .accessibility_label("Close variable inspector")
                            .hint("Close detail inspector")
                            .on_click(cx.listener(|view, _, _, cx| {
                                let tab = &mut view.tabs[view.active];
                                tab.variables_state.selected = None;
                                tab.inspections.remove("Variable preview");
                                cx.notify();
                            })),
                    ),
            );
        panel = panel.child(header);

        // Breadcrumbs trail for nested drill-down
        if breadcrumbs.len() > 1 {
            let mut trail = h_flex()
                .gap_1()
                .items_center()
                .flex_wrap()
                .px_2()
                .py_1()
                .rounded(px(6.))
                .bg(theme.secondary.opacity(0.45));
            for (idx, crumb) in breadcrumbs.iter().enumerate() {
                if idx > 0 {
                    trail = trail.child(div().text_xs().text_color(muted).child("\u{203a}"));
                }
                let label = crumb["label"].as_str().unwrap_or("").to_owned();
                let target_segs = crumb["segments"].clone();
                let is_last = idx + 1 == breadcrumbs.len();
                if is_last {
                    trail = trail.child(
                        div()
                            .px_1()
                            .text_xs()
                            .font_family(mono.clone())
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(label),
                    );
                } else {
                    let h = handle.clone();
                    trail = trail.child(
                        action_button(("var-crumb", idx))
                            .ghost()
                            .xsmall()
                            .label(label)
                            .on_click(cx.listener(move |view, _, _, _| {
                                view.interactive_request(
                                    "namespace.preview",
                                    json!({
                                        "handle": h,
                                        "segments": target_segs,
                                        "offset": 0,
                                    }),
                                );
                            })),
                    );
                }
            }
            panel = panel.child(trail);
        }

        // Error state
        if let Some(err_msg) = preview["error"]["message"].as_str() {
            let h = handle.clone();
            panel = panel.child(
                h_flex()
                    .justify_between()
                    .items_center()
                    .gap_2()
                    .child(
                        div()
                            .text_sm()
                            .text_color(theme.danger)
                            .child(err_msg.to_owned()),
                    )
                    .child(
                        action_button("var-error-reset")
                            .outline()
                            .xsmall()
                            .label("Back to root")
                            .on_click(cx.listener(move |view, _, _, _| {
                                view.interactive_request(
                                    "namespace.preview",
                                    json!({"handle": h, "segments": []}),
                                );
                            })),
                    ),
            );
            return panel;
        }

        if let Some(documentation) = preview["documentation"]
            .as_str()
            .filter(|text| !text.trim().is_empty())
        {
            panel = panel.child(
                div()
                    .text_sm()
                    .whitespace_normal()
                    .child(bounded(documentation, 16384)),
            );
        }
        if let Some(signature) = preview["signature"].as_str() {
            panel = panel.child(
                TextView::markdown(
                    "variable-signature",
                    format!("```python\n{}\n```", signature.replace("```", "~~~")),
                )
                .selectable(true),
            );
        }

        // Mode-specific rich inspector body
        if mode == "table" {
            if is_lazy {
                panel = panel
                    .child(
                        div()
                            .px_2p5()
                            .py_1p5()
                            .rounded(px(6.))
                            .bg(warning.opacity(0.12))
                            .text_xs()
                            .child(
                                "LazyFrame schema inspected passively without calling .collect().",
                            ),
                    )
                    .child(self.render_variable_schema(preview, handle, segments, cx));
            } else {
                let show_schema = detail_tab == "schema";
                panel = panel.child(
                    h_flex()
                        .gap_1()
                        .items_center()
                        .child(
                            action_button("var-tab-data")
                                .xsmall()
                                .segment(!show_schema)
                                .label("Data")
                                .on_click(cx.listener(|view, _, _, cx| {
                                    view.tabs[view.active].variables_state.detail_tab =
                                        "data".into();
                                    cx.notify();
                                })),
                        )
                        .child(
                            action_button("var-tab-schema")
                                .xsmall()
                                .segment(show_schema)
                                .label("Schema")
                                .on_click(cx.listener(|view, _, _, cx| {
                                    view.tabs[view.active].variables_state.detail_tab =
                                        "schema".into();
                                    cx.notify();
                                })),
                        ),
                );
                if show_schema {
                    panel = panel.child(self.render_variable_schema(preview, handle, segments, cx));
                } else {
                    panel = panel.child(self.render_variable_grid(
                        preview,
                        handle,
                        segments,
                        &page["table"],
                        false,
                        cx,
                    ));
                }
            }
        } else if mode == "matrix" {
            panel = panel.child(self.render_variable_grid(
                preview,
                handle,
                segments,
                &page["table"],
                true,
                cx,
            ));
        } else if matches!(
            mode,
            "device_tensor"
                | "text"
                | "bytes"
                | "sequence"
                | "mapping"
                | "set"
                | "attributes"
                | "series"
                | "array1d"
        ) {
            if !raw_preview.is_empty() && !matches!(mode, "text" | "bytes" | "device_tensor") {
                panel = panel.child(
                    div()
                        .text_xs()
                        .font_family(mono)
                        .text_color(muted)
                        .whitespace_normal()
                        .child(bounded(&raw_preview, 512)),
                );
            }
            panel = panel.child(self.variable_page_rows(preview, cx));
        } else {
            // Scalar or opaque object
            if !raw_preview.is_empty() {
                panel = panel.child(
                    div()
                        .p_2p5()
                        .rounded(px(6.))
                        .bg(theme.secondary.opacity(0.45))
                        .text_sm()
                        .font_family(mono)
                        .whitespace_normal()
                        .child(bounded(&raw_preview, 4096)),
                );
            }
            if page["unsupported"] == true && raw_preview.is_empty() {
                panel = panel.child(self.variable_page_rows(preview, cx));
            }
        }

        panel
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
        let page_max_w = if self.surface == "Variables" {
            rems(92.)
        } else {
            rems(76.)
        };
        let mut body = v_flex()
            .w_full()
            .max_w(page_max_w)
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
            let records: Vec<Value> = if !self.tabs[self.active].console_history.is_empty() {
                self.tabs[self.active]
                    .console_history
                    .iter()
                    .rev()
                    .cloned()
                    .collect()
            } else {
                self.tabs[self.active]
                    .inspections
                    .get("Console output")
                    .cloned()
                    .into_iter()
                    .collect()
            };
            if !records.is_empty() {
                let total = records.len();
                body = body.child(
                    h_flex()
                        .justify_between()
                        .items_center()
                        .gap_2()
                        .child(
                            div()
                                .text_xs()
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(muted)
                                .child(if total == 1 {
                                    "Session transcript (1 execution)".to_string()
                                } else {
                                    format!("Session transcript ({total} executions)")
                                }),
                        )
                        .child(
                            action_button("console-clear")
                                .ghost()
                                .small()
                                .label("Clear transcript")
                                .hint("Clear the visible console transcript (recorded History is preserved)")
                                .on_click(cx.listener(|view, _, _, cx| {
                                    let tab = &mut view.tabs[view.active];
                                    tab.console_history.clear();
                                    tab.inspections.remove("Console output");
                                    cx.notify();
                                })),
                        ),
                );
                let commands = self.tabs[self.active].service.commands.clone();
                let project = self.tabs[self.active].model.project.clone();
                for (entry_idx, record) in records.iter().enumerate() {
                    let status = record["status"].as_str().unwrap_or("unknown");
                    let exec_id = record["exec_id"].as_str().unwrap_or("");
                    let heading = if entry_idx == 0 {
                        "Latest execution".to_string()
                    } else {
                        format!("Execution #{}", total - entry_idx)
                    };
                    let mut out_card = card(cx)
                        .p_4()
                        .gap_3()
                        .child(
                            h_flex()
                                .justify_between()
                                .items_center()
                                .gap_2()
                                .child(
                                    h_flex()
                                        .gap_2()
                                        .items_center()
                                        .child(
                                            div()
                                                .text_sm()
                                                .font_weight(FontWeight::SEMIBOLD)
                                                .child(heading),
                                        )
                                        .when_some(record_time(record), |d, ts| {
                                            d.child(
                                                div()
                                                    .text_xs()
                                                    .text_color(muted)
                                                    .child(relative_time(ts)),
                                            )
                                        }),
                                )
                                .when(notable_status(status), |d| {
                                    d.child(badge(
                                        outcome_label(status).to_string(),
                                        tone(status, cx),
                                    ))
                                }),
                        )
                        .child(
                            TextView::markdown(
                                SharedString::from(format!(
                                    "console-recorded-source-{entry_idx}-{exec_id}"
                                )),
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
                            id: format!("console-{entry_idx}-{exec_id}-{index}").into(),
                            event: event.clone(),
                            context: format!("Console output {}", index + 1).into(),
                            tools_state: None,
                            commands: Some(commands.clone()),
                            cell_id: "".into(),
                            exec_id: exec_id.into(),
                            index,
                            worker_generation: None,
                            project: Some(project.clone()),
                            workspace: Some(cx.entity()),
                        });
                    }
                    body = body.child(out_card);
                }
            }
        } else {
            let danger = cx.theme().danger;
            let list_active = cx.theme().list_active;
            let query_raw = self.variables_query.read(cx).value().to_string();
            let query = query_raw.trim().to_lowercase();
            let active_category = self.tabs[self.active].variables_state.category.clone();
            let selected_var = self.tabs[self.active].variables_state.selected.clone();
            let preview_val = self.tabs[self.active]
                .inspections
                .get("Variable preview")
                .cloned();
            let scroll = self.tabs[self.active].variables_scroll.clone();

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
                .child(
                    div()
                        .w(rems(18.))
                        .min_w(rems(11.))
                        .child(Input::new(&self.variables_query).cleanable(true)),
                )
                .child(info_tip(
                    "variables-panel-tip",
                    "Reads live namespace bindings passively while the worker is idle. Never calls user __repr__ or starts a runtime.",
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

            let items = value["items"].as_array().cloned().unwrap_or_default();
            if !items.is_empty() {
                let mut chip_bar = h_flex().gap_1p5().items_center().flex_wrap();
                for (idx, (key, label)) in VARIABLE_CATEGORIES.iter().enumerate() {
                    let count = items
                        .iter()
                        .filter(|item| variable_matches_category(item, key))
                        .count();
                    let active = active_category == *key;
                    let chip_label = if count > 0 || *key == "all" {
                        format!("{label} ({count})")
                    } else {
                        (*label).to_owned()
                    };
                    let key_owned = (*key).to_owned();
                    chip_bar = chip_bar.child(
                        action_button(("variables-cat", idx))
                            .xsmall()
                            .segment(active)
                            .label(chip_label)
                            .on_click(cx.listener(move |view, _, _, cx| {
                                view.tabs[view.active].variables_state.category = key_owned.clone();
                                cx.notify();
                            })),
                    );
                }
                body = body.child(chip_bar);
            }

            if let Some(error) = value["error"]["message"].as_str() {
                body = body.child(div().text_sm().text_color(danger).child(error.to_owned()));
            }

            let filtered: Vec<Value> = items
                .iter()
                .filter(|item| {
                    variable_matches_category(item, &active_category)
                        && variable_matches(item, &query)
                })
                .cloned()
                .collect();

            let split_active = preview_val.is_some() && window.viewport_size().width >= px(1040.);
            let show_size = !split_active;

            let list_section: Div = if !items.is_empty() {
                if filtered.is_empty() {
                    let empty_msg = match (!query.is_empty(), active_category != "all") {
                        (true, true) => format!(
                            "No variables in this category match \u{201c}{}\u{201d}.",
                            query_raw.trim()
                        ),
                        (true, false) => {
                            format!("No variables match \u{201c}{}\u{201d}.", query_raw.trim())
                        }
                        (false, true) => "No variables in this category.".to_owned(),
                        (false, false) => "No variables match.".to_owned(),
                    };
                    card(cx).p_4().child(
                        h_flex()
                            .justify_between()
                            .items_center()
                            .gap_3()
                            .flex_wrap()
                            .child(div().text_sm().text_color(muted).child(empty_msg))
                            .child(
                                action_button("variables-clear-filter")
                                    .ghost()
                                    .small()
                                    .label("Clear filters")
                                    .on_click(cx.listener(|view, _, window, cx| {
                                        view.variables_query.update(cx, |input, cx| {
                                            input.set_value("", window, cx);
                                        });
                                        view.tabs[view.active].variables_state.category =
                                            "all".into();
                                        cx.notify();
                                    })),
                            ),
                    )
                } else {
                    let count = filtered.len();
                    let header_row = h_flex()
                        .w_full()
                        .px_2()
                        .py_1p5()
                        .gap_2p5()
                        .items_center()
                        .border_b_1()
                        .border_color(divider(cx))
                        .text_xs()
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(muted)
                        .child(div().w(rems(10.5)).flex_none().child("Name"))
                        .child(div().w(rems(7.2)).flex_none().child("Type"))
                        .when(show_size, |row| {
                            row.child(div().w(rems(9.5)).flex_none().child("Size"))
                        })
                        .child(div().flex_1().min_w_0().child("Value"));
                    let mut list_card = card(cx).p_2().gap_1().child(header_row);
                    let view = cx.entity();
                    let row_mono = mono.clone();
                    let selected_for_rows = selected_var.clone();
                    let max_list_h = if split_active { 28.0 } else { 22.0 };
                    list_card = list_card.child(
                        uniform_list("variables-list", count, move |range, _, _cx| {
                            range
                                .map(|index| {
                                    let item = &filtered[index];
                                    let handle = item["handle"].clone();
                                    let name = item["name"].as_str().unwrap_or("").to_owned();
                                    let short_type = variable_short_type(item);
                                    let size_label =
                                        item["size_label"].as_str().unwrap_or("").to_owned();
                                    let preview = item["preview"].as_str().unwrap_or("").to_owned();
                                    let expandable = item["expandable"].as_bool() == Some(true);
                                    let is_selected =
                                        selected_for_rows.as_deref() == Some(name.as_str());
                                    let click_name = name.clone();
                                    let view = view.clone();
                                    div().h(rems(2.1)).w_full().flex().items_center().child(
                                        action_button(("variable", index))
                                            .ghost()
                                            .small()
                                            .variable_row(
                                                name,
                                                short_type,
                                                size_label,
                                                preview,
                                                show_size,
                                                expandable,
                                                is_selected,
                                                row_mono.clone(),
                                                muted,
                                                list_active,
                                            )
                                            .on_click(move |_, _, cx| {
                                                view.update(cx, |view, cx| {
                                                    let state =
                                                        &mut view.tabs[view.active].variables_state;
                                                    if state.selected.as_deref()
                                                        != Some(click_name.as_str())
                                                    {
                                                        state.detail_tab = "data".into();
                                                    }
                                                    state.selected = Some(click_name.clone());
                                                    view.interactive_request(
                                                        "namespace.preview",
                                                        json!({
                                                            "handle": handle,
                                                            "segments": [],
                                                            "offset": 0,
                                                        }),
                                                    );
                                                    cx.notify();
                                                });
                                            }),
                                    )
                                })
                                .collect()
                        })
                        .track_scroll(&scroll)
                        .h(rems((count as f32 * 2.1).min(max_list_h))),
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
                                    view.interactive_request(
                                        "namespace.list",
                                        json!({"cursor":cursor}),
                                    )
                                })),
                        );
                    }
                    list_card
                }
            } else if value.is_null() {
                card(cx).p_4().child(
                    div()
                        .text_sm()
                        .text_color(muted)
                        .child("Click Refresh when the runtime is idle to inspect variables."),
                )
            } else {
                card(cx).p_4().child(
                    div()
                        .text_sm()
                        .text_color(muted)
                        .child("No variables are currently defined in this session."),
                )
            };

            if let Some(preview) = preview_val {
                let inspector = self.render_variable_inspector(&preview, cx);
                if split_active {
                    body = body.child(
                        h_flex()
                            .w_full()
                            .items_start()
                            .gap_4()
                            .child(div().w(rems(32.)).flex_none().min_w_0().child(list_section))
                            .child(div().flex_1().min_w_0().child(inspector)),
                    );
                } else {
                    body = body.child(list_section).child(inspector);
                }
            } else {
                body = body.child(list_section);
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

const VARIABLE_CATEGORIES: &[(&str, &str)] = &[
    ("all", "All"),
    ("tables_arrays", "Tables & Arrays"),
    ("collections", "Collections"),
    ("scalars", "Scalars"),
    ("other", "Other"),
];

pub(super) fn variable_short_type(item: &Value) -> String {
    if let Some(short) = item["short_type"].as_str().filter(|s| !s.is_empty()) {
        return short.to_owned();
    }
    let full = item["type"].as_str().unwrap_or("");
    let short = full.rsplit('.').next().unwrap_or(full);
    if short.is_empty() {
        "value".to_owned()
    } else {
        short.to_owned()
    }
}

pub(super) fn variable_category_bucket(item: &Value) -> &'static str {
    match item["category"].as_str() {
        Some("tabular" | "array") => "tables_arrays",
        Some("collection") => "collections",
        Some("scalar") => "scalars",
        Some(_) => "other",
        None => {
            let ty = variable_short_type(item);
            match ty.as_str() {
                "DataFrame" | "Series" | "LazyFrame" | "Table" | "RecordBatch" | "ndarray"
                | "Tensor" | "Array" => "tables_arrays",
                "list" | "tuple" | "dict" | "set" | "frozenset" | "deque" | "OrderedDict"
                | "Counter" | "defaultdict" => "collections",
                "int" | "float" | "bool" | "str" | "bytes" | "bytearray" | "complex"
                | "NoneType" | "Decimal" | "Fraction" | "datetime" | "date" | "time"
                | "timedelta" | "Path" | "PosixPath" | "WindowsPath" | "UUID" => "scalars",
                _ => "other",
            }
        }
    }
}

pub(super) fn variable_matches_category(item: &Value, category: &str) -> bool {
    category == "all" || variable_category_bucket(item) == category
}

pub(super) fn variable_matches(item: &Value, query: &str) -> bool {
    if query.is_empty() {
        return true;
    }
    if ["name", "short_type", "type", "preview", "size_label"]
        .iter()
        .any(|field| {
            item[*field]
                .as_str()
                .is_some_and(|text| text.to_lowercase().contains(query))
        })
    {
        return true;
    }
    item["columns_preview"].as_array().is_some_and(|cols| {
        cols.iter().any(|col| {
            col.as_str()
                .is_some_and(|text| text.to_lowercase().contains(query))
        })
    })
}

pub(super) fn breadcrumb_expression(preview: &Value) -> String {
    if let Some(crumbs) = preview["breadcrumbs"].as_array()
        && !crumbs.is_empty()
    {
        let mut expr = crumbs[0]["label"]
            .as_str()
            .unwrap_or_else(|| preview["name"].as_str().unwrap_or("value"))
            .to_owned();
        for crumb in &crumbs[1..] {
            let label = crumb["label"].as_str().unwrap_or("");
            if let Some(idx) = label.strip_prefix('#') {
                expr = format!("list({expr})[{idx}]");
            } else {
                expr.push_str(label);
            }
        }
        return expr;
    }
    preview["name"].as_str().unwrap_or("value").to_owned()
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
