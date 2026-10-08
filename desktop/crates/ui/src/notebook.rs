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
        let tab = &self.tabs[self.active];
        let cell = tab.model.cells[&id].clone();
        let selected = tab.model.selected.as_deref() == Some(id.as_str());
        let last = index + 1 == tab.model.order.len();
        let theme = cx.theme();
        let (border, muted, muted_fg, primary, background, _radius) = (
            theme.border,
            theme.muted,
            theme.muted_foreground,
            theme.primary,
            theme.background,
            theme.radius,
        );
        let mono = theme.mono_font_family.clone();
        let (warning, danger, info) = (theme.warning, theme.danger, theme.info);
        let save = match cell.save {
            SaveState::Saved => None,
            SaveState::Dirty => Some(("Unsaved", warning)),
            SaveState::Saving => Some(("Saving…", info)),
            SaveState::Conflict => Some(("Conflict", danger)),
            SaveState::Failed => Some(("Save failed", danger)),
        };
        let mut actions = h_flex()
            .flex_none()
            .gap_0p5()
            .child(
                Button::new(("run", index))
                    .ghost()
                    .small()
                    .icon(Glyph::Play)
                    .tooltip("Run cell · Shift+Enter")
                    .accessibility_label(format!("Run {}", cell_title(&cell.text, &cell.disk.kind)))
                    .disabled(cell.disk.kind != "code" || cell.save == SaveState::Conflict)
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
            .child(
                Button::new(("collapse", index))
                    .ghost()
                    .small()
                    .icon(if cell.collapsed {
                        Glyph::Eye
                    } else {
                        Glyph::EyeOff
                    })
                    .tooltip(if cell.collapsed {
                        "Show output"
                    } else {
                        "Hide output"
                    })
                    .accessibility_label(if cell.collapsed {
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
            )
            .child(
                Button::new(("duplicate", index))
                    .ghost()
                    .small()
                    .icon(Glyph::Copy)
                    .tooltip("Duplicate")
                    .accessibility_label(format!(
                        "Duplicate {}",
                        cell_title(&cell.text, &cell.disk.kind)
                    ))
                    .on_click(cx.listener({
                        let id = id.clone();
                        move |v, _, _, _| v.mutate(json!({"action":"duplicate","cell_id":id}))
                    })),
            )
            .child(
                Button::new(("delete", index))
                    .ghost()
                    .small()
                    .icon(Glyph::Trash)
                    .tooltip("Delete")
                    .accessibility_label(format!(
                        "Delete {}",
                        cell_title(&cell.text, &cell.disk.kind)
                    ))
                    .on_click(cx.listener({
                        let id = id.clone();
                        move |v, _, window, cx| {
                            v.confirmation = Some(("delete".into(), json!({"cell_id":id})));
                            v.return_focus = window.focused(cx);
                            v.focus.focus(window, cx);
                            cx.notify();
                        }
                    })),
            );
        if selected {
            for (delta, label, glyph) in [
                (-1isize, "Move cell up", Glyph::ChevronsUp),
                (1, "Move cell down", Glyph::ChevronsDown),
            ] {
                let id = id.clone();
                actions = actions.child(Button::new((SharedString::from(label), index)).ghost().small().icon(glyph)
                    .accessibility_label(format!("{label}: {}", cell_title(&cell.text, &cell.disk.kind)))
                    .tooltip(label).disabled((delta < 0 && index == 0) || (delta > 0 && last))
                    .on_click(cx.listener(move |v, _, _, _| {
                        v.mutate(json!({"action":"move","cell_id":id,"index":index.saturating_add_signed(delta)}));
                    })));
            }
        }
        let header = h_flex()
            .h(px(36.))
            .px_3()
            .gap_2()
            .items_center()
            .bg(background)
            .border_b_1()
            .border_color(border)
            .child(
                div()
                    .flex_none()
                    .font_family(mono.clone())
                    .text_xs()
                    .text_color(if selected { primary } else { muted_fg })
                    .child(
                        cell.ordinal
                            .map(|n| format!("[{n}]"))
                            .unwrap_or_else(|| "[ ]".into()),
                    ),
            )
            .child(badge(kind_label(&cell.disk.kind), muted_fg))
            .child(
                div()
                    .min_w_0()
                    .truncate()
                    .font_family(mono.clone())
                    .text_xs()
                    .text_color(muted_fg)
                    .child(cell_title(&cell.text, &cell.disk.kind)),
            )
            .when_some(save, |d, (label, color)| d.child(badge(label, color)))
            .when(!cell.outcome.is_empty(), |d| {
                d.child(badge(cell.outcome.clone(), tone(&cell.outcome, cx)))
            })
            .when(cell.stale(), |d| {
                d.child(badge("Edited since run", warning))
            })
            .when(cell.stale() && cell.exec_id.is_some(), |d| {
                let exec_id = cell.exec_id.clone().unwrap_or_default();
                d.child(
                    Button::new(("compare", index))
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
            .border_l_2()
            .border_color(if selected { primary } else { background })
            .bg(background)
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
                        div().px_1().py_1().child(
                            Editor::new(&editor)
                                .text_size(px(self.preferences.editor_size))
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
                            .child("Conflict — both versions are preserved"),
                    )
                    .child(
                        div()
                            .p_2()
                            .rounded(px(6.))
                            .bg(background)
                            .border_1()
                            .border_color(border)
                            .font_family(mono.clone())
                            .text_xs()
                            .child(bounded(&cell.disk.source, 4096)),
                    )
                    .child(
                        h_flex()
                            .gap_2()
                            .child(
                                Button::new(("disk", index))
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
                                Button::new(("mine", index))
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
                div().px_4().py_3().border_t_1().border_color(border).child(
                    TextView::markdown(("preview", index), safe_markdown(&cell.text))
                        .on_link_click(safe_link),
                ),
            );
        }
        if !cell.output.is_empty() {
            card = card.child(if cell.collapsed {
                h_flex()
                    .px_4()
                    .py_2()
                    .gap_2()
                    .border_t_1()
                    .border_color(border)
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
                    .py_3()
                    .gap_2()
                    .border_t_1()
                    .border_color(border)
                    .bg(muted.opacity(0.25))
                    .children(
                        cell.output
                            .iter()
                            .enumerate()
                            .map(|(i, e)| output(e, format!("{id}-{i}"), cx)),
                    )
            });
        }
        div()
            .w_full()
            .flex()
            .flex_col()
            .px_4()
            .pt(px(if index == 0 { 20. } else { 6. }))
            .pb(px(if last { 32. } else { 6. }))
            .child(card)
            .child(
                h_flex().gap_1().pt_2().children(
                    [
                        ("code", "+ Code"),
                        ("markdown", "+ Markdown"),
                        ("raw", "+ Raw"),
                    ]
                    .into_iter()
                    .map(|(kind, label)| {
                        Button::new((SharedString::from(format!("insert-{kind}")), index))
                            .ghost()
                            .small()
                            .label(label)
                            .accessibility_label(format!("Insert {kind} after cell {}", index + 1))
                            .on_click(cx.listener(move |v, _, _, _| {
                                v.mutate(json!({"action":"add","type":kind,"index":index + 1}))
                            }))
                    }),
                ),
            )
            .into_any_element()
    }
}
