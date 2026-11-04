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
        let show_header = self.high_contrast
            || cell.disk.kind == "code"
            || selected
            || self.preferences.comfortable
            || save.is_some()
            || has_error
            || cell.stale();
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
            .when_some(cell.ordinal.filter(|_| cell.disk.kind == "code"), |d, n| {
                d.child(
                    div()
                        .flex_none()
                        .min_w(px(28.))
                        .font_family(mono.clone())
                        .text_xs()
                        .text_color(muted_fg)
                        .child(format!("[{n}]")),
                )
            })
            .when_some(save, |d, (label, color)| d.child(badge(label, color)))
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
            .child(actions);
        let mut card = v_flex()
            .id(SharedString::from(id.clone()))
            .w_full()
            .group(group)
            .relative()
            .border_1()
            .rounded(cx.theme().radius_lg)
            .border_color(if self.high_contrast {
                if selected { primary } else { edge(cx) }
            } else {
                transparent
            })
            .bg(if selected || self.high_contrast {
                popover
            } else {
                transparent
            })
            .when(!selected && !self.high_contrast, |d| {
                d.hover(move |s| s.bg(popover))
            })
            .overflow_hidden()
            .child(
                div()
                    .absolute()
                    .left_0()
                    .top_0()
                    .bottom_0()
                    .w(px(2.))
                    .bg(if selected { primary } else { transparent }),
            )
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
                                h_flex().relative().w_full().child(gutter).child(
                                    Editor::new(&editor)
                                        .flex_1()
                                        .min_w_0()
                                        .text_size(px(self.preferences.editor_size))
                                        .line_height(px(self.preferences.editor_size * 1.5))
                                        .aria_label(format!("{} cell {id}", cell.disk.kind))
                                        .h(editor_height(
                                            &cell.text,
                                            self.preferences.editor_size,
                                            self.preferences.ui_scale,
                                        ))
                                        .bordered(false),
                                ),
                            ),
                    )
                },
            );
        if cell.save == SaveState::Conflict {
            card = card.child(self.cell_conflict_banner(index, &id, &cell.disk.source, cx));
        }
        if cell.disk.kind == "markdown" && !(selected && tab.model.mode == Mode::Edit) {
            card = card.child(
                div().px_5().py_3().max_w(rems(52.)).child(
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
            .pt(px(if index == 0 { 12. } else { 0. }))
            .pb(px(if last { 20. } else { 0. }))
            .child(card)
            .child(
                h_flex()
                    .group(SharedString::from(format!("boundary-{id}")))
                    .h(px(if last {
                        spacing.boundary_last
                    } else {
                        spacing.boundary
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
                                .when(last, |b| b.label("Add cell"))
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
