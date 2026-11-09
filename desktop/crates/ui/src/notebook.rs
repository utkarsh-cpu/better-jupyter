use super::*;

/// Retain the existing header when text, scale or debugger targets need extra room.
pub(crate) fn compact_source_layout(
    comfortable: bool,
    scale: f32,
    editor_size: f32,
    debugger: bool,
) -> bool {
    !comfortable && scale <= 1.25 && editor_size <= 18. && !debugger
}

pub(crate) fn cell_border_color(
    selected: bool,
    editor_focused: bool,
    has_error: bool,
    high_contrast: bool,
    cx: &App,
) -> Hsla {
    let theme = cx.theme();
    if selected || editor_focused {
        theme.primary.opacity(if high_contrast { 1. } else { 0.55 })
    } else if has_error {
        // Failure cue independent of color-only badges (live UX D).
        theme.danger.opacity(if high_contrast { 1. } else { 0.45 })
    } else {
        surface_edge(cx)
    }
}

pub(crate) fn boundary_button_label(last: bool) -> &'static str {
    if last { "Add cell" } else { "Insert" }
}

impl Workspace {
    pub(super) fn editor(
        &mut self,
        tab_index: usize,
        id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Entity<EditorState> {
        if let Some(editor) = self.tabs[tab_index].editors.get(id) {
            return editor.clone();
        }
        let cell = &self.tabs[tab_index].model.cells[id];
        let text = cell.text.clone();
        let kind = cell.disk.kind.clone();
        let editor = cx.new(|cx| {
            // Height follows source lines. Soft wrapping would add visual rows
            // behind that fixed viewport and clip a continuation after resizing.
            // Keep source geometry stable and use the editor's horizontal scroll.
            let mut s = EditorState::new(window, cx)
                .soft_wrap(false)
                // Inline cells already size to their content. Full-file editor
                // trailing space can leave short cells scrolled past line one.
                .scroll_beyond_last_line(Some(0))
                .language(if kind == "code" {
                    "python"
                } else if kind == "markdown" {
                    "markdown"
                } else {
                    "text"
                });
            s.set_value(text, window, cx);
            // Initial source inspection starts at the leading edge even when
            // the end of a long line lies outside a narrow editor viewport.
            s.set_selected_range(0..0, cx);
            s
        });
        let id = id.to_string();
        if kind == "code" {
            let provider = std::rc::Rc::new(interactive::NotebookCompletion {
                view: cx.entity().downgrade(),
                tab: tab_index,
                editor: editor.downgrade(),
            });
            editor.update(cx, |editor, _| {
                editor.lsp_mut().completion_provider = Some(provider.clone());
                editor.lsp_mut().hover_provider = Some(provider);
            });
        }
        let subscription = cx.subscribe(&editor, {
            let id = id.clone();
            move |view, editor, event: &InputEvent, cx| {
                let tab = &mut view.tabs[tab_index];
                if matches!(event, InputEvent::Focus) {
                    view.active = tab_index;
                    tab.model.selected = Some(id.clone());
                    tab.model.mode = Mode::Edit;
                }
                if matches!(event, InputEvent::Change) {
                    let text = editor.read(cx).value().to_string();
                    if let Some(cell) = tab.model.cells.get_mut(&id)
                        && cell.text != text
                    {
                        let resized = editor_lines(&cell.text) != editor_lines(&text);
                        cell.edit(text);
                        if resized && let Some(i) = tab.model.order.iter().position(|c| *c == id) {
                            tab.list.splice(i..i + 1, 1);
                        }
                        if let Some(request) = tab.model.save_request(&id) {
                            let _ = tab.service.commands.send(Command::Edit {
                                cell_id: id.clone(),
                                request,
                            });
                        }
                    }
                }
                cx.notify();
            }
        });
        self.tabs[tab_index].subscriptions.push(subscription);
        let observed = cx.observe(&editor, |_, _, cx| cx.notify());
        self.tabs[tab_index].subscriptions.push(observed);
        self.tabs[tab_index].editors.insert(id, editor.clone());
        editor
    }
    #[inline(never)]
    pub(super) fn cell(
        &mut self,
        index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(id) = self.tabs[self.active].model.order.get(index).cloned() else {
            return div().into_any_element();
        };
        let editor = self.editor(self.active, &id, window, cx);
        self.debug_editor_markers(&id, &editor, cx);
        let gutter = self.debug_gutter(&id, &editor, cx);
        let editor_focused = editor.read(cx).focus_handle(cx).is_focused(window);
        let tab = &self.tabs[self.active];
        let cell = tab.model.cells[&id].clone();
        let selected = tab.model.selected.as_deref() == Some(id.as_str());
        let last = index + 1 == tab.model.order.len();
        let theme = cx.theme();
        let (muted_fg, primary, transparent, popover) = (
            theme.muted_foreground,
            theme.primary,
            theme.transparent,
            theme.popover,
        );
        let mono = theme.mono_font_family.clone();
        let (warning, danger, info) = (theme.warning, theme.danger, theme.info);
        let save = match cell.save {
            SaveState::Saved => None,
            SaveState::Dirty => Some(("Unsaved", warning)),
            SaveState::Saving => Some(("Saving\u{2026}", info)),
            SaveState::Conflict => Some(("Conflict", danger)),
            SaveState::Failed => Some(("Save failed", danger)),
        };
        let group: SharedString = format!("cell-{id}").into();
        let tools_owner: SharedString = format!("notebook-{}-{id}", self.active).into();
        let tools_state = outputs::cell_tools(tools_owner.clone(), window, cx);
        let has_error = matches!(cell.outcome.as_str(), "error" | "failed")
            || cell.output.iter().any(|e| e["kind"] == "error");
        let mut actions = h_flex().flex_none().gap_1().items_center();
        if selected
            && cell.disk.kind == "code"
            && tab.runtime["interactive"]["features"]
                .as_array()
                .is_some_and(|features| features.iter().any(|feature| feature == "debugger"))
        {
            let id = id.clone();
            actions = actions.child(
                action_button(("breakpoint", index))
                    .ghost()
                    .small()
                    .icon(Glyph::Flag)
                    .accessibility_label("Toggle breakpoint at caret")
                    .hint("Bind a breakpoint to the saved source at the caret")
                    .unavailable(cell.save != SaveState::Saved)
                    .on_click(cx.listener(move |view, _, window, cx| {
                        view.tabs[view.active].model.selected = Some(id.clone());
                        view.toggle_breakpoint(window, cx);
                    })),
            );
        }
        let compact_source = compact_source_layout(
            self.preferences.comfortable,
            self.preferences.ui_scale,
            self.preferences.editor_size,
            tab.runtime["interactive"]["features"]
                .as_array()
                .is_some_and(|f| f.iter().any(|f| f == "debugger")),
        );
        // A narrow split needs a full-width editor. Keep its run button,
        // execution count, and output tools in the toolbar above the source.
        let narrow_split = self.compact && tab.assistant.open;
        let inline_code = compact_source && !narrow_split && cell.disk.kind == "code";
        let inline_markdown = compact_source && cell.disk.kind == "markdown";
        let mut run_action = if cell.disk.kind == "code" {
            Some(
                action_button(("run", index))
                    .ghost()
                    .small()
                    .rounded(px(15.))
                    .bg(primary.opacity(0.11))
                    .text_color(primary)
                    .icon(Glyph::Play)
                    .hint("Run cell (Ctrl+Enter)")
                    .accessibility_label(format!("Run {}", cell_title(&cell.text, &cell.disk.kind)))
                    .unavailable(cell.save == SaveState::Conflict)
                    .on_click(cx.listener({
                        let id = id.clone();
                        move |v, _, _, _| {
                            v.send(Command::Run {
                                cell_ids: vec![id.clone()],
                                on_error: "stop".into(),
                            })
                        }
                    })),
            )
        } else {
            None
        };
        if cell.disk.kind == "markdown" {
            actions = actions.child(
                action_button(("edit-markdown", index))
                    .ghost()
                    .small()
                    .label(if selected && tab.model.mode == Mode::Edit {
                        "Preview"
                    } else {
                        "Edit"
                    })
                    .on_click(cx.listener({
                        let id = id.clone();
                        move |v, _, window, cx| {
                            let t = &mut v.tabs[v.active];
                            let editing_this = t.model.selected.as_deref() == Some(id.as_str())
                                && t.model.mode == Mode::Edit;
                            t.model.selected = Some(id.clone());
                            if editing_this {
                                t.model.mode = Mode::Command;
                                v.focus.focus(window, cx);
                            } else {
                                t.model.mode = Mode::Edit;
                                let e = v.editor(v.active, &id, window, cx);
                                e.update(cx, |e, cx| e.focus(window, cx));
                            }
                            cx.notify();
                        }
                    })),
            );
        }
        if !cell.output.is_empty() {
            actions = actions.child(outputs::OutputTools {
                owner: tools_owner.clone(),
                state: tools_state.clone(),
                context: cell_title(&cell.text, &cell.disk.kind).into(),
                group: group.clone(),
                always: selected || self.preferences.comfortable,
            });
            actions = actions.child(
                action_button(("collapse", index))
                    .ghost()
                    .small()
                    .icon(if cell.collapsed {
                        Glyph::Eye
                    } else {
                        Glyph::EyeOff
                    })
                    .accessibility_label(if cell.collapsed {
                        "Show output"
                    } else {
                        "Hide output"
                    })
                    .hint(if cell.collapsed {
                        "Show output"
                    } else {
                        "Hide output"
                    })
                    .on_click(cx.listener({
                        let id = id.clone();
                        move |v, _, _, cx| {
                            let t = &mut v.tabs[v.active];
                            if let Some(c) = t.model.cells.get_mut(&id) {
                                c.collapsed = !c.collapsed;
                            }
                            t.list.splice(index..index + 1, 1);
                            cx.notify();
                        }
                    })),
            );
        }
        if !has_error {
            actions = actions.child(
                action_button(("cell-assistant", index))
                    .ghost()
                    .small()
                    .icon(Glyph::Sparkles)
                    .contextual(group.clone(), selected || self.preferences.comfortable)
                    .hint("Ask Assistant about this cell")
                    .accessibility_label(format!(
                        "Ask Assistant about {}",
                        cell_title(&cell.text, &cell.disk.kind)
                    ))
                    .on_click(cx.listener({
                        let id = id.clone();
                        move |v, _, _, cx| {
                            v.tabs[v.active].model.selected = Some(id.clone());
                            v.tabs[v.active].assistant.selected_cells = vec![id.clone()];
                            v.open_assistant(cx);
                        }
                    })),
            );
        }
        let key = format!("cell-more-{id}");
        actions = actions.child(
            self.anchored(
                key.clone(),
                action_button(("cell-more", index))
                    .ghost()
                    .small()
                    .icon(Glyph::Ellipsis)
                    .contextual(group.clone(), selected || self.preferences.comfortable)
                    .hint("Cell actions")
                    .accessibility_label(format!(
                        "More actions for {}",
                        cell_title(&cell.text, &cell.disk.kind)
                    ))
                    .on_click(cx.listener({
                        let id = id.clone();
                        move |v, _, window, cx| {
                            // Snapshot before close_menu restores the previous menu's focus.
                            let opener = window.focused(cx);
                            v.close_menu(window, cx);
                            v.return_focus = opener;
                            v.cell_menu = Some((id.clone(), index));
                            v.menu_anchor = key.clone();
                            v.dialog_focus.focus(window, cx);
                            cx.notify();
                        }
                    })),
                cx,
            ),
        );
        let show_header = if inline_code || inline_markdown {
            save.is_some() || has_error || cell.stale() || notable_status(&cell.outcome)
        } else {
            self.high_contrast
                || cell.disk.kind == "code"
                || selected
                || self.preferences.comfortable
                || save.is_some()
                || has_error
                || cell.stale()
        };
        let mut inline_run = if inline_code { run_action.take() } else { None };
        let mut header_actions = Some(actions);
        let mut inline_actions = if inline_code || inline_markdown {
            header_actions.take()
        } else {
            None
        };
        let notable_outcome = notable_status(&cell.outcome);
        let spacing = cell_spacing(self.preferences.comfortable);
        let header = h_flex()
            .min_h(rems(spacing.header_min_h))
            .px_3()
            .gap_1p5()
            .items_center()
            .flex_wrap()
            .py(px(spacing.header_py))
            .when_some(run_action, |d, button| d.child(button))
            .when(selected || self.high_contrast, |d| {
                d.border_b_1().border_color(divider(cx))
            })
            .when_some(
                cell.ordinal
                    .filter(|_| cell.disk.kind == "code" && !inline_code),
                |d, n| {
                    d.child(
                        div()
                            .flex_none()
                            .min_w(px(28.))
                            .font_family(mono.clone())
                            .text_xs()
                            .text_color(muted_fg)
                            .child(format!("[{n}]")),
                    )
                },
            )
            .when_some(
                cell.execution_seconds
                    .filter(|_| self.preferences.show_cell_duration && !inline_code),
                |d, seconds| {
                    d.child(
                        div()
                            .font_family(mono.clone())
                            .text_xs()
                            .text_color(muted_fg)
                            .child(format!(
                                "{}{}",
                                if cell.stale() { "Last run " } else { "" },
                                history::format_duration(seconds)
                            )),
                    )
                },
            )
            .when_some(save, |d, (label, color)| d.child(badge(label, color)))
            .when(has_error && !notable_outcome, |d| {
                d.child(badge("Failed", danger))
            })
            .when(notable_outcome, |d| {
                d.child(badge(
                    outcome_label(&cell.outcome).to_string(),
                    tone(&cell.outcome, cx),
                ))
            })
            .when(has_error, |d| {
                let err_cell_id = id.clone();
                d.child(
                    action_button(("explain-error", index))
                        .ghost()
                        .small()
                        .icon(Glyph::Sparkles)
                        .label("Explain error")
                        .on_click(cx.listener(move |v, _, window, cx| {
                            v.tabs[v.active].model.selected = Some(err_cell_id.clone());
                            v.tabs[v.active].assistant.selected_cells = vec![err_cell_id.clone()];
                            v.run_command("ai-explain", window, cx);
                        })),
                )
            })
            .when(cell.stale(), |d| {
                d.child(badge("Edited since run", warning))
            })
            .when(cell.stale() && cell.exec_id.is_some(), |d| {
                let exec_id = cell.exec_id.clone().unwrap_or_default();
                d.child(
                    action_button(("compare", index))
                        .ghost()
                        .small()
                        .label("Compare")
                        .on_click(cx.listener(move |v, _, _, cx| {
                            v.select_surface("History", cx);
                            v.inspect("execution.get", json!({"exec_id":exec_id}));
                        })),
                )
            })
            .child(div().flex_1())
            .children(header_actions);
        let mut card = v_flex()
            .id(SharedString::from(id.clone()))
            .w_full()
            .debug_selector({
                let id = id.clone();
                move || format!("cell-card-{id}")
            })
            .role(Role::Group)
            .aria_label(format!(
                "Cell {}, {}{}{}{}",
                index + 1,
                cell.disk.kind,
                if selected { ", selected" } else { "" },
                if editor_focused { ", editing" } else { "" },
                if has_error { ", failed" } else { "" }
            ))
            .group(group)
            .relative()
            .border_1()
            .rounded(cx.theme().radius_lg)
            .border_color(cell_border_color(
                selected,
                editor_focused,
                has_error,
                self.high_contrast,
                cx,
            ))
            .bg(if selected || editor_focused || self.high_contrast {
                popover
            } else {
                transparent
            })
            .when(!selected && !self.high_contrast, |d| {
                d.hover(move |s| s.bg(popover))
            })
            .overflow_hidden()
            .child(div().absolute().left_0().top_0().bottom_0().w(px(2.)).bg(
                if selected || editor_focused {
                    primary
                } else if has_error {
                    danger
                } else {
                    transparent
                },
            ))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener({
                    let id = id.clone();
                    move |v, _, _, cx| {
                        let t = &mut v.tabs[v.active];
                        if t.model.selected.as_deref() != Some(id.as_str()) {
                            t.model.selected = Some(id.clone());
                            cx.notify();
                        }
                    }
                }),
            )
            .when(show_header, |d| d.child(header))
            .when(
                cell.disk.kind != "markdown" || (selected && tab.model.mode == Mode::Edit),
                |d| {
                    d.child(
                        div()
                            .debug_selector({
                                let id = id.clone();
                                move || format!("cell-source-{id}")
                            })
                            .w_full()
                            .max_w(rems(64.))
                            .mx_auto()
                            .px_3()
                            .pt(px(spacing.source_top))
                            .pb(px(spacing.source_bottom))
                            .border_l_2()
                            .border_color(if editor_focused {
                                primary.opacity(0.7)
                            } else {
                                transparent
                            })
                            .child(
                                h_flex()
                                    .relative()
                                    .w_full()
                                    .items_start()
                                    .gap_2()
                                    .when(inline_code, |d| {
                                        let row_h = px((self.preferences.editor_size * 1.5
                                            + 18. * self.preferences.ui_scale)
                                            .max(30. * self.preferences.ui_scale));
                                        d.child(
                                            h_flex()
                                                .flex_none()
                                                .min_h(row_h)
                                                .w(rems(if self.preferences.show_cell_duration {
                                                    6.0
                                                } else {
                                                    4.75
                                                }))
                                                .gap_1p5()
                                                .items_center()
                                                .children(inline_run.take())
                                                .when_some(cell.ordinal, |d, n| {
                                                    d.child(
                                                        div()
                                                            .font_family(mono.clone())
                                                            .text_xs()
                                                            .text_color(muted_fg)
                                                            .child(format!("[{n}]")),
                                                    )
                                                })
                                                .when_some(
                                                    cell.execution_seconds.filter(|_| {
                                                        self.preferences.show_cell_duration
                                                    }),
                                                    |d, seconds| {
                                                        d.child(
                                                            div()
                                                                .id(("cell-duration", index))
                                                                .debug_selector({
                                                                    let id = id.clone();
                                                                    move || {
                                                                        format!(
                                                                            "cell-duration-{id}"
                                                                        )
                                                                    }
                                                                })
                                                                .font_family(mono.clone())
                                                                .text_xs()
                                                                .text_color(muted_fg)
                                                                .role(Role::Label)
                                                                .aria_label(format!(
                                                                    "Last run duration: {}",
                                                                    history::format_duration(
                                                                        seconds
                                                                    )
                                                                ))
                                                                .child(history::format_duration(
                                                                    seconds,
                                                                )),
                                                        )
                                                    },
                                                ),
                                        )
                                    })
                                    .child(gutter)
                                    .child(
                                        div()
                                            .flex_1()
                                            .min_w_0()
                                            .debug_selector({
                                                let id = id.clone();
                                                move || format!("cell-editor-{id}")
                                            })
                                            .child(
                                                Editor::new(&editor)
                                                    .w_full()
                                                    .text_size(px(self.preferences.editor_size))
                                                    .line_height(px(
                                                        self.preferences.editor_size * 1.5
                                                    ))
                                                    .aria_label(format!(
                                                        "{} cell {id}",
                                                        cell.disk.kind
                                                    ))
                                                    .h(editor_height(
                                                        &cell.text,
                                                        self.preferences.editor_size,
                                                        self.preferences.ui_scale,
                                                    ))
                                                    .bordered(false),
                                            ),
                                    )
                                    .when(inline_code || inline_markdown, |d| {
                                        let row_h = px((self.preferences.editor_size * 1.5
                                            + 18. * self.preferences.ui_scale)
                                            .max(30. * self.preferences.ui_scale));
                                        d.child(
                                            h_flex()
                                                .flex_none()
                                                .min_h(row_h)
                                                .items_center()
                                                .pl_2()
                                                .children(inline_actions.take()),
                                        )
                                    }),
                            ),
                    )
                },
            );
        if cell.save == SaveState::Conflict {
            card = card.child(self.cell_conflict_banner(index, &id, &cell.disk.source, cx));
        }
        if cell.disk.kind == "markdown" && !(selected && tab.model.mode == Mode::Edit) {
            let row_h = px(
                (self.preferences.editor_size * 1.5 + 18. * self.preferences.ui_scale)
                    .max(30. * self.preferences.ui_scale),
            );
            card = card.child(
                h_flex()
                    .w_full()
                    .items_start()
                    .px_3()
                    .pt(px(spacing.source_top))
                    .pb(px(spacing.source_bottom))
                    .gap_2()
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .max_w(rems(52.))
                            .min_h(row_h)
                            .px_2()
                            .py_1()
                            // TextView's pinned Markdown renderer paints spans without
                            // content semantics. Expose one readable document per cell.
                            .id(("markdown-content", index))
                            .role(Role::Document)
                            .aria_label(format!("Rendered Markdown cell {}", index + 1))
                            .a11y_synthetic_children({
                                let nodes = markdown_accessibility_nodes(&cell.text);
                                move |tree| {
                                    for (i, node) in nodes.into_iter().enumerate() {
                                        let id = tree.synthetic_node_id(i);
                                        tree.push_child(id, node);
                                    }
                                }
                            })
                            .child(
                                TextView::markdown(("preview", index), safe_markdown(&cell.text))
                                    .on_link_click(safe_link),
                            ),
                    )
                    .when(inline_markdown, |d| {
                        d.child(
                            h_flex()
                                .flex_none()
                                .min_h(row_h)
                                .items_center()
                                .pl_2()
                                .children(inline_actions.take()),
                        )
                    }),
            );
        }
        let attachment_names = cell
            .disk
            .metadata
            .get("attachments")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        if !attachment_names.is_empty() {
            let commands = self.tabs[self.active].service.commands.clone();
            let previews = attachment_names
                .iter()
                .filter_map(Value::as_str)
                .map(|name| {
                    let key = format!("{id}/{name}");
                    (
                        name.to_owned(),
                        self.tabs[self.active].attachment_pages.get(&key).cloned(),
                    )
                })
                .collect::<Vec<_>>();
            card = card.child(
                v_flex()
                    .px_4()
                    .py_2()
                    .gap_2()
                    .children(previews.into_iter().map(|(name, preview)| {
                        let mime = cell
                            .disk
                            .metadata
                            .get("attachment_mimes")
                            .and_then(|mimes| mimes.get(&name))
                            .and_then(Value::as_str)
                            .unwrap_or("application/octet-stream");
                        outputs::attachment_preview(
                            commands.clone(),
                            id.clone().into(),
                            name.clone().into(),
                            mime.to_owned().into(),
                            preview,
                            window,
                            cx,
                        )
                    })),
            );
        }
        if !cell.output.is_empty() {
            card = card.child(if cell.collapsed {
                h_flex()
                    .px_4()
                    .py_1p5()
                    .gap_2()
                    .items_center()
                    .border_t_1()
                    .border_color(divider(cx))
                    .text_xs()
                    .text_color(muted_fg)
                    .child(Icon::new(Glyph::EyeOff).xsmall())
                    .child(format!(
                        "{} output{} hidden",
                        cell.output.len(),
                        if cell.output.len() == 1 { "" } else { "s" }
                    ))
            } else {
                v_flex()
                    .px_4()
                    .py(px(spacing.output_py))
                    .gap(px(spacing.output_gap))
                    .border_t_1()
                    .border_color(divider(cx))
                    .children(cell.output.iter().enumerate().map(|(i, e)| {
                        let mut e = e.clone();
                        if let Some(handle) = e["bundle"]["application/vnd.kernelledger.widget+json"]["value"]["handle"].as_str().map(str::to_owned)
                            && let Some(model) = self.tabs[self.active].inspections.get("Widget state").and_then(|states| states.get(&handle)) {
                                e["bundle"]["application/vnd.kernelledger.widget+json"]["value"] = model.clone();
                        }
                        outputs::OutputBlock {
                            tools_state: Some(tools_state.clone()),
                            id: format!("{id}-{i}").into(),
                            event: e.clone(),
                            context: format!(
                                "Output {} for cell {}: {}",
                                i + 1,
                                index + 1,
                                cell_title(&cell.text, &cell.disk.kind)
                            )
                            .into(),
                            commands: Some(self.tabs[self.active].service.commands.clone()),
                            cell_id: id.clone().into(),
                            project: Some(self.tabs[self.active].model.project.clone()),
                            exec_id: cell.exec_id.clone().unwrap_or_default().into(),
                            index: i,
                            worker_generation:
                                self.tabs[self.active].runtime["interactive"]["worker_generation"]
                                    .as_str()
                                    .map(str::to_owned),
                            workspace: Some(cx.entity()),
                        }
                        .into_any_element()
                    }))
            });
        }
        let boundary_group: SharedString = format!("boundary-{id}").into();
        let boundary_visible = last || self.preferences.comfortable || selected;
        let rule_color = divider(cx);
        let card_edge = surface_edge(cx);
        let line_rule = |group: SharedString| {
            div()
                .flex_1()
                .max_w(rems(8.))
                .h(px(1.))
                .bg(rule_color)
                .when(!boundary_visible, |d| {
                    d.opacity(0.).group_hover(group, |s| s.opacity(1.))
                })
        };
        div()
            .debug_selector({
                let id = id.clone();
                move || format!("cell-frame-{id}")
            })
            .w_full()
            .flex()
            .flex_col()
            // Tables and plots may use the wider canvas; source stays readable.
            .max_w(rems(if cell.output.iter().any(outputs::is_wide_output) {
                80.
            } else {
                64.
            }))
            .mx_auto()
            .px_4()
            .pt(px(if index == 0 { 16. } else { 0. }))
            .pb(px(if last { 24. } else { 0. }))
            .child(card)
            .child(
                h_flex()
                    .debug_selector({
                        let id = id.clone();
                        move || format!("cell-boundary-{id}")
                    })
                    .group(boundary_group.clone())
                    .w_full()
                    .h(px(if last {
                        spacing.boundary_last
                    } else {
                        spacing.boundary
                    }))
                    .py_1p5()
                    .gap_3()
                    .items_center()
                    .justify_center()
                    .child(line_rule(boundary_group.clone()))
                    .child(
                        self.anchored(
                            format!("insert-{id}"),
                            action_button(("insert-cell", index))
                                .ghost()
                                .small()
                                .rounded_full()
                                .px_2p5()
                                .border_1()
                                .border_color(card_edge)
                                .bg(popover)
                                .icon(Glyph::Plus)
                                .label(boundary_button_label(last))
                                .text_color(muted_fg)
                                .contextual(boundary_group.clone(), boundary_visible)
                                .accessibility_label(if last {
                                    "Add cell at end of notebook".to_string()
                                } else {
                                    format!("Insert cell after cell {}", index + 1)
                                })
                                .on_click(cx.listener({
                                    let key = format!("insert-{id}");
                                    move |v, _, window, cx| {
                                        v.close_menu(window, cx);
                                        v.return_focus = window.focused(cx);
                                        v.insert_menu = Some(index + 1);
                                        v.menu_anchor = key.clone();
                                        v.dialog_focus.focus(window, cx);
                                        cx.notify();
                                    }
                                })),
                            cx,
                        ),
                    )
                    .child(line_rule(boundary_group)),
            )
            .into_any_element()
    }

    #[inline(never)]
    fn cell_conflict_banner(
        &self,
        index: usize,
        id: &str,
        disk_source: &str,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = cx.theme();
        let (danger, background) = (theme.danger, theme.background);
        let mono = theme.mono_font_family.clone();
        let id = id.to_owned();
        v_flex()
            .gap_2()
            .px_4()
            .py_3()
            .border_t_1()
            .border_color(danger.opacity(0.3))
            .bg(danger.opacity(0.06))
            .child(
                h_flex()
                    .gap_2()
                    .items_center()
                    .text_sm()
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(danger)
                    .child(Icon::new(Glyph::TriangleAlert).small())
                    .child("Conflict \u{2014} both versions are preserved"),
            )
            .child(
                div()
                    .p_2()
                    .rounded(px(6.))
                    .bg(background)
                    .border_1()
                    .border_color(divider(cx))
                    .font_family(mono)
                    .text_xs()
                    .child(bounded(disk_source, 4096)),
            )
            .child(
                h_flex()
                    .gap_2()
                    .child(
                        action_button(("disk", index))
                            .small()
                            .outline()
                            .label("Take disk")
                            .on_click(cx.listener({
                                let id = id.clone();
                                move |v, _, window, cx| {
                                    let t = &mut v.tabs[v.active];
                                    t.model.take_disk(&id);
                                    let _ =
                                        t.service.commands.send(Command::DiscardDraft(id.clone()));
                                    if let Some(e) = t.editors.get(&id) {
                                        e.update(cx, |s, cx| {
                                            s.set_value(t.model.cells[&id].text.clone(), window, cx)
                                        });
                                    }
                                    cx.notify();
                                }
                            })),
                    )
                    .child(
                        action_button(("mine", index))
                            .small()
                            .primary()
                            .label("Take mine")
                            .on_click(cx.listener({
                                let id = id.clone();
                                move |v, _, _, _| {
                                    if let Some(r) = v.tabs[v.active].model.overwrite_request(&id) {
                                        v.send(Command::Mutate(r));
                                    }
                                }
                            })),
                    ),
            )
            .into_any_element()
    }
}

/// Coherent reading blocks rather than a separate accessibility node per span.
/// Preserve inline syntax and code verbatim so no source content is silently lost.
pub(crate) fn markdown_reading_blocks(source: &str) -> Vec<(usize, String)> {
    let mut blocks = Vec::new();
    let mut paragraph = String::new();
    let mut fence: Option<&str> = None;
    let flush = |blocks: &mut Vec<(usize, String)>, paragraph: &mut String| {
        let text = paragraph.trim();
        if !text.is_empty() {
            blocks.push((0, text.to_owned()));
        }
        paragraph.clear();
    };
    for line in source.lines() {
        let text = line.trim();
        let delimiter = if text.starts_with("```") {
            Some("```")
        } else if text.starts_with("~~~") {
            Some("~~~")
        } else {
            None
        };
        if fence.is_none() && delimiter.is_some() {
            flush(&mut blocks, &mut paragraph);
            fence = delimiter;
        } else if fence.is_some() && delimiter == fence {
            if !paragraph.is_empty() {
                paragraph.push('\n');
            }
            paragraph.push_str(line);
            flush(&mut blocks, &mut paragraph);
            fence = None;
            continue;
        }
        let level = text.chars().take_while(|c| *c == '#').count();
        if fence.is_none() && level > 0 && level <= 6 && text.as_bytes().get(level) == Some(&b' ') {
            flush(&mut blocks, &mut paragraph);
            blocks.push((level, text[level..].trim().to_owned()));
        } else if fence.is_none() && text.is_empty() {
            flush(&mut blocks, &mut paragraph);
        } else {
            if !paragraph.is_empty() {
                paragraph.push('\n');
            }
            paragraph.push_str(line);
        }
    }
    flush(&mut blocks, &mut paragraph);
    blocks
}

pub(crate) fn markdown_accessibility_nodes(source: &str) -> Vec<accesskit::Node> {
    markdown_reading_blocks(source)
        .into_iter()
        .map(|(level, text)| {
            let mut node = accesskit::Node::new(if level > 0 {
                Role::Heading
            } else {
                Role::Paragraph
            });
            node.set_label(text);
            if level > 0 {
                node.set_level(level);
            }
            node
        })
        .collect()
}
