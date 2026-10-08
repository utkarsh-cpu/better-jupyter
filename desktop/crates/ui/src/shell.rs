use super::*;

impl Workspace {
    pub(super) fn header(&self, cx: &mut Context<Self>) -> Div {
        let mut header = h_flex()
            .debug_selector(|| "workspace-header".into())
            .h(px(44.))
            .flex_none()
            .px_3()
            .gap_2()
            .items_center()
            .bg(cx.theme().title_bar)
            .border_b_1()
            .border_color(cx.theme().border);
        let mut notebooks = h_flex()
            .max_w(px(if self.compact { 160. } else { 280. }))
            .min_w_0()
            .overflow_hidden()
            .gap_1();
        for (i, tab) in self.tabs.iter().enumerate() {
            notebooks = notebooks.child(
                Button::new(("tab", i))
                    .ghost()
                    .small()
                    .label(
                        tab.model
                            .project
                            .file_name()
                            .unwrap_or_default()
                            .to_string_lossy()
                            .to_string(),
                    )
                    .tooltip(tab.model.project.display().to_string())
                    .selected(i == self.active)
                    .on_click(cx.listener(move |v, _, window, cx| {
                        let query = v.history_query.read(cx).value().to_string();
                        if let Some(tab) = v.tabs.get_mut(v.active) {
                            tab.selections.insert(v.surface.clone(), v.record_active);
                            tab.history_filter = query;
                        }
                        v.active = i;
                        let query = v.tabs[i].history_filter.clone();
                        v.history_query
                            .update(cx, |state, cx| state.set_value(query, window, cx));
                        v.record_active =
                            v.tabs[i].selections.get(&v.surface).copied().unwrap_or(0);
                        cx.notify();
                    })),
            );
        }
        if self.tabs.is_empty() {
            header = header.child("KernelLedger");
        } else {
            header = header.child(notebooks).child(vsep(cx.theme().border));
            for surface in ["Notebook", "History", "Checkpoints"] {
                header = header.child(
                    Button::new(surface)
                        .ghost()
                        .small()
                        .label(surface)
                        .selected(self.surface == surface)
                        .accessibility_label(format!(
                            "{surface}{}",
                            if self.surface == surface {
                                ", current destination"
                            } else {
                                ""
                            }
                        ))
                        .on_click(cx.listener(move |v, _, window, cx| {
                            v.select_surface(surface, cx);
                            v.focus.focus(window, cx);
                        })),
                );
            }
        }
        header
            .child(div().flex_1())
            .child(
                Button::new("palette-trigger")
                    .ghost()
                    .small()
                    .label("Commands")
                    .tooltip("Search actions · Ctrl+Shift+P")
                    .on_click(cx.listener(|v, _, window, cx| v.palette(&Palette, window, cx))),
            )
            .child(
                Button::new("runtime-menu")
                    .ghost()
                    .small()
                    .label("Runtime")
                    .disabled(self.tabs.is_empty())
                    .on_click(cx.listener(|v, _, window, cx| {
                        v.return_focus = window.focused(cx);
                        v.runtime_menu = !v.runtime_menu;
                        v.app_menu = false;
                        v.dialog_focus.focus(window, cx);
                        cx.notify();
                    })),
            )
            .child(
                Button::new("application-menu")
                    .ghost()
                    .small()
                    .label("Menu")
                    .on_click(cx.listener(|v, _, window, cx| {
                        v.return_focus = window.focused(cx);
                        v.app_menu = !v.app_menu;
                        v.runtime_menu = false;
                        v.dialog_focus.focus(window, cx);
                        cx.notify();
                    })),
            )
    }
    pub(super) fn toolbar(&self, cx: &mut Context<Self>) -> Div {
        h_flex()
            .flex_none()
            .h(px(40.))
            .px_3()
            .gap_2()
            .items_center()
            .border_b_1()
            .border_color(cx.theme().border)
            .child(
                Button::new("outline-toggle")
                    .ghost()
                    .small()
                    .label("Outline")
                    .selected(if self.compact {
                        self.outline_drawer
                    } else {
                        self.sidebar_visible
                    })
                    .on_click(cx.listener(|v, _, _, cx| {
                        if v.compact {
                            v.outline_drawer = !v.outline_drawer;
                            cx.notify();
                            return;
                        }
                        v.sidebar_visible = !v.sidebar_visible;
                        v.preferences.outline_visible = v.sidebar_visible;
                        v.save_preferences(cx);
                        cx.notify();
                    })),
            )
            .child(
                Button::new("run-selected")
                    .ghost()
                    .small()
                    .icon(Glyph::Play)
                    .label("Run")
                    .on_click(cx.listener(|v, _, window, cx| v.run(&RunCell, window, cx))),
            )
            .child(
                Button::new("all")
                    .ghost()
                    .small()
                    .label("Run all")
                    .on_click(cx.listener(|v, _, _, _| v.batch("all"))),
            )
            .child(
                Button::new("interrupt")
                    .ghost()
                    .small()
                    .label("Interrupt")
                    .disabled(!runtime_action_available(
                        "interrupt",
                        &self.tabs[self.active].runtime,
                        &self.surface,
                    ))
                    .tooltip(
                        action_reason("interrupt", &self.tabs[self.active].runtime)
                            .unwrap_or("Interrupt current execution"),
                    )
                    .on_click(
                        cx.listener(|v, _, window, cx| v.run_command("interrupt", window, cx)),
                    ),
            )
            .child(div().flex_1())
            .child(
                Button::new("add-code")
                    .ghost()
                    .small()
                    .label("+ Code")
                    .on_click(
                        cx.listener(|v, _, _, _| v.mutate(json!({"action":"add","type":"code"}))),
                    ),
            )
            .child(
                Button::new("add-markdown")
                    .ghost()
                    .small()
                    .label("+ Markdown")
                    .on_click(cx.listener(|v, _, _, _| {
                        v.mutate(json!({"action":"add","type":"markdown"}))
                    })),
            )
            .child(
                Button::new("add-raw")
                    .ghost()
                    .small()
                    .label("+ Raw")
                    .on_click(
                        cx.listener(|v, _, _, _| v.mutate(json!({"action":"add","type":"raw"}))),
                    ),
            )
    }
    pub(super) fn menu_overlay(&self, cx: &mut Context<Self>) -> Stateful<Div> {
        let mut menu = v_flex()
            .w(px(300.))
            .max_h(px(440.))
            .p_3()
            .gap_2()
            .bg(cx.theme().popover)
            .border_1()
            .border_color(cx.theme().border)
            .rounded(px(6.));
        if self.runtime_menu {
            let runtime = &self.tabs[self.active].runtime;
            menu = menu
                .child(
                    div()
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(if runtime["connection_lost"] == true { "Connection lost — execution state unknown".into() } else { worker_label(runtime["worker_state"].as_str()) }),
                )
                .child(
                    div()
                        .text_xs()
                        .whitespace_normal()
                        .child(runtime["interpreter"]["executable"].as_str().or(runtime["interpreter"]["python"].as_str()).unwrap_or("Interpreter details are available after running a cell").to_string()),
                );
            for (method, label) in [
                ("interrupt", "Interrupt"),
                ("restart", "Restart empty…"),
                ("stop", "Stop runtime…"),
            ] {
                menu = menu.child(
                    Button::new(method)
                        .ghost()
                        .label(label)
                        .disabled(action_reason(method, runtime).is_some())
                        .tooltip(action_reason(method, runtime).unwrap_or(label))
                        .on_click(cx.listener(move |v, _, window, cx| {
                            v.runtime_menu = false;
                            v.run_command(method, window, cx);
                        })),
                );
            }
            menu = menu.child(
                Button::new("all-runtimes")
                    .ghost()
                    .label("All runtimes")
                    .on_click(cx.listener(|v, _, window, cx| {
                        v.runtime_menu = false;
                        v.select_surface("Runtimes", cx);
                        v.focus.focus(window, cx);
                    })),
            );
        } else {
            menu = menu.child(
                Button::new("open")
                    .ghost()
                    .label("Open notebook…")
                    .on_click(cx.listener(|v, _, window, cx| {
                        v.app_menu = false;
                        v.open_dialog(&OpenNotebook, window, cx);
                    })),
            );
            for surface in ["Settings", "Help", "Diagnostics", "Runtimes"] {
                menu = menu.child(
                    Button::new(surface)
                        .ghost()
                        .label(surface)
                        .disabled(self.tabs.is_empty() && surface != "Help")
                        .on_click(cx.listener(move |v, _, window, cx| {
                            v.app_menu = false;
                            v.select_surface(surface, cx);
                            v.focus.focus(window, cx);
                        })),
                );
            }
        }
        menu = menu.child(
            Button::new("close-menu")
                .ghost()
                .label("Close menu")
                .on_click(cx.listener(|v, _, window, cx| v.close_menu(window, cx))),
        );
        div()
            .id("workspace-menu")
            .role(Role::Dialog)
            .aria_label("Workspace menu")
            .occlude()
            .track_focus(&self.dialog_focus)
            .absolute()
            .inset_0()
            .flex()
            .justify_end()
            .items_start()
            .pt(px(48.))
            .pr_3()
            .child(menu)
    }
    pub(super) fn close_menu(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.app_menu = false;
        self.runtime_menu = false;
        if let Some(focus) = self.return_focus.take() {
            focus.focus(window, cx);
        }
        cx.notify();
    }
}
