use super::*;

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
            let mut s = EditorState::new(window, cx).language(if kind == "code" {
                "python"
            } else if kind == "markdown" {
                "markdown"
            } else {
                "text"
            });
            s.set_value(text, window, cx);
            s
        });
        let id = id.to_string();
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
        self.tabs[tab_index].editors.insert(id, editor.clone());
        editor
    }
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
        let editor_focused = editor.read(cx).focus_handle(cx).is_focused(window);
        let tab = &self.tabs[self.active];
        let cell = tab.model.cells[&id].clone();
        let selected = tab.model.selected.as_deref() == Some(id.as_str());
        let last = index + 1 == tab.model.order.len();
        let theme = cx.theme();
        let (muted, muted_fg, primary, background, transparent) = (
            theme.muted,
            theme.muted_foreground,
            theme.primary,
            theme.background,
            theme.transparent,
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
        let mut actions = h_flex().flex_none().gap_1().items_center();
        let run_action = if cell.disk.kind == "code" {
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
                    .label(if tab.model.mode == Mode::Edit {
                        "Preview"
                    } else {
                        "Edit"
                    })
                    .on_click(cx.listener({
                        let id = id.clone();
                        move |v, _, window, cx| {
                            let t = &mut v.tabs[v.active];
                            if t.model.mode == Mode::Edit {
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
                            v.close_menu(window, cx);
                            v.return_focus = window.focused(cx);
                            v.cell_menu = Some((id.clone(), index));
                            v.menu_anchor = key.clone();
                            v.dialog_focus.focus(window, cx);
                            cx.notify();
                        }
                    })),
                cx,
            ),
        );
        let header = h_flex()
            .min_h(rems(if self.preferences.comfortable {
                2.75
            } else {
                2.0
            }))
            .px_3()
            .gap_2()
            .items_center()
            .bg(if self.high_contrast {
                cx.theme().popover
            } else {
                muted.opacity(0.42)
            })
            .flex_wrap()
            .py_1()
            .when_some(run_action, |d, button| d.child(button))
            .border_b_1()
            .border_color(divider(cx))
            .when(cell.disk.kind == "code", |d| {
                d.child(
                    div()
                        .flex_none()
                        .font_family(mono.clone())
                        .text_xs()
                        .text_color(muted_fg)
                        .child(
                            cell.ordinal
                                .map(|n| format!("[{n}]"))
                                .unwrap_or_else(|| "[ ]".into()),
                        ),
                )
            })
            .child(badge(kind_label(&cell.disk.kind), muted_fg))
            .when_some(save, |d, (label, color)| d.child(badge(label, color)))
            .when(!cell.outcome.is_empty(), |d| {
                d.child(badge(
                    outcome_label(&cell.outcome).to_string(),
                    tone(&cell.outcome, cx),
                ))
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
            .child(actions);
        let mut card = v_flex()
            .id(SharedString::from(id.clone()))
            .w_full()
            .group(group)
            .border_1()
            .rounded(px(10.))
            .border_color(if selected { primary } else { divider(cx) })
            .bg(cx.theme().popover)
            .when(!self.high_contrast && selected, |d| d.shadow_sm())
            .overflow_hidden()
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
            .child(header)
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
                            .px_2()
                            .py_1p5()
                            .border_l_2()
                            .border_color(if editor_focused { primary } else { transparent })
                            .child(
                                Editor::new(&editor)
                                    .text_size(px(self.preferences.editor_size))
                                    .line_height(px(self.preferences.editor_size * 1.5))
                                    .aria_label(format!("{} cell {id}", cell.disk.kind))
                                    .h(px(editor_lines(&cell.text) as f32
                                        * self.preferences.editor_size
                                        * 1.5
                                        + 12.))
                                    .bordered(false),
                            ),
                    )
                },
            );
        if cell.save == SaveState::Conflict {
            card = card.child(
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
                            .font_family(mono.clone())
                            .text_xs()
                            .child(bounded(&cell.disk.source, 4096)),
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
                                            let _ = t
                                                .service
                                                .commands
                                                .send(Command::DiscardDraft(id.clone()));
                                            if let Some(e) = t.editors.get(&id) {
                                                e.update(cx, |s, cx| {
                                                    s.set_value(
                                                        t.model.cells[&id].text.clone(),
                                                        window,
                                                        cx,
                                                    )
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
                                            if let Some(r) =
                                                v.tabs[v.active].model.overwrite_request(&id)
                                            {
                                                v.send(Command::Mutate(r));
                                            }
                                        }
                                    })),
                            ),
                    ),
            );
        }
        if cell.disk.kind == "markdown" && !(selected && tab.model.mode == Mode::Edit) {
            card = card.child(
                div().px_4().py_3().max_w(rems(52.)).child(
                    TextView::markdown(("preview", index), safe_markdown(&cell.text))
                        .on_link_click(safe_link),
                ),
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
                    .py_2()
                    .gap_2()
                    .items_center()
                    .border_t_1()
                    .border_color(divider(cx))
                    .bg(muted.opacity(0.45))
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
                    .py_2p5()
                    .gap_2()
                    .border_t_1()
                    .border_color(divider(cx))
                    .bg(muted.opacity(0.55))
                    .children(cell.output.iter().enumerate().map(|(i, e)| {
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
                            exec_id: cell.exec_id.clone().unwrap_or_default().into(),
                            index: i,
                        }
                        .into_any_element()
                    }))
            });
        }
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
            .pt(px(if index == 0 { 14. } else { 0. }))
            .pb(px(if last { 24. } else { 0. }))
            .child(card)
            .child(
                h_flex()
                    .group(SharedString::from(format!("boundary-{id}")))
                    .h(rems(if self.preferences.comfortable {
                        2.75
                    } else {
                        1.875
                    }))
                    .items_center()
                    .justify_center()
                    .child(
                        self.anchored(
                            format!("insert-{id}"),
                            action_button(("insert-cell", index))
                                .ghost()
                                .small()
                                .icon(Glyph::Plus)
                                .label(if last { "Add cell" } else { "Insert cell" })
                                .text_color(muted_fg)
                                .contextual(
                                    SharedString::from(format!("boundary-{id}")),
                                    last || self.preferences.comfortable || selected,
                                )
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
                    ),
            )
            .into_any_element()
    }
}
