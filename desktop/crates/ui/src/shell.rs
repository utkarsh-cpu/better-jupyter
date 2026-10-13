use super::*;
use gpui_kit::component::ElementExt;

impl Workspace {
    pub(super) fn anchored(
        &self,
        key: impl Into<String>,
        button: WorkbenchButton,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let key = key.into();
        let view = cx.entity();
        div()
            .id(SharedString::from(format!("anchor-{key}")))
            .flex_none()
            .on_prepaint(move |bounds, _, cx| {
                view.update(cx, |v, cx| {
                    if v.menu_bounds.get(&key) != Some(&bounds) {
                        v.menu_bounds.insert(key, bounds);
                        cx.notify();
                    }
                });
            })
            .child(button)
    }

    pub(super) fn header(
        &self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> gpui_kit::component::TitleBar {
        let compact = window.viewport_size().width < px(1280. * self.preferences.ui_scale);
        let tight = compact && (self.preferences.ui_scale > 1. || self.preferences.comfortable);
        let overflow = self.preferences.comfortable
            && window.viewport_size().width < px(560. * self.preferences.ui_scale);
        let mut header = h_flex()
            .debug_selector(|| "workspace-header".into())
            .flex_1()
            .min_w_0()
            .h_full()
            .pr_2()
            .gap(rems(if compact { 0.25 } else { 0.5 }))
            .items_center();
        let name = self
            .tabs
            .get(self.active)
            .map(|tab| {
                tab.model
                    .project
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_string()
            })
            .unwrap_or_default();
        let name_limit = if compact { 8 } else { 20 };
        let mut notebooks = action_button("notebook-switcher")
            .debug_selector("notebook-switcher")
            .ghost()
            .small()
            .hint(format!("{name} — switch or open notebook"))
            .accessibility_label(format!("Notebook switcher: {name}"))
            .on_click(cx.listener(|v, _, window, cx| {
                v.close_menu(window, cx);
                v.return_focus = window.focused(cx);
                v.menu_anchor = "switcher".into();
                v.notebook_menu = true;
                v.dialog_focus.focus(window, cx);
                cx.notify();
            }));
        if tight {
            notebooks = notebooks.icon(Glyph::NotebookPen);
        } else {
            notebooks =
                notebooks
                    .dropdown_caret(true)
                    .label(if name.chars().count() > name_limit {
                        format!("{}…", name.chars().take(name_limit).collect::<String>())
                    } else {
                        name.clone()
                    });
        }
        if self.tabs.is_empty() {
            header = header.child(
                div()
                    .font_weight(FontWeight::SEMIBOLD)
                    .child("KernelLedger"),
            );
        } else {
            if self.surface == "Notebook" {
                header = header.child(
                    action_button("outline-toggle")
                        .debug_selector("outline-toggle")
                        .ghost()
                        .small()
                        .icon(Glyph::PanelLeft)
                        .accessibility_label("Outline")
                        .hint("Toggle notebook outline")
                        .selected(if self.compact {
                            self.outline_drawer
                        } else {
                            self.sidebar_visible
                        })
                        .on_click(cx.listener(|v, _, _, cx| {
                            if v.compact {
                                v.outline_drawer = !v.outline_drawer;
                            } else {
                                v.sidebar_visible = !v.sidebar_visible;
                                v.preferences.outline_visible = v.sidebar_visible;
                                v.save_preferences(cx);
                            }
                            cx.notify();
                        })),
                );
            }
            if !overflow {
                header = header.child(self.anchored("switcher", notebooks, cx));
            }
            let mut destinations = segmented_track(cx).self_center().flex_none().flex_nowrap();
            for (surface, icon) in [
                ("Notebook", Glyph::NotebookPen),
                ("History", Glyph::Clock),
                ("Checkpoints", Glyph::Flag),
            ] {
                let mut button = action_button(surface)
                    .debug_selector(surface)
                    .small()
                    .segment(self.surface == surface)
                    .accessibility_label(format!(
                        "{surface}{}",
                        if self.surface == surface {
                            ", current destination"
                        } else {
                            ""
                        }
                    ))
                    .hint(surface)
                    .on_click(cx.listener(move |v, _, window, cx| {
                        v.select_surface(surface, cx);
                        v.focus.focus(window, cx);
                    }));
                if tight {
                    button = button.icon(icon);
                } else {
                    button = button.label(surface);
                }
                destinations = destinations.child(button);
            }
            header = header.child(destinations);
        }
        // This flexible blank area remains available for native window dragging.
        header = header.child(div().flex_1().min_w(rems(0.5)).h_full());
        if !tight {
            header = header.child(
                action_button("palette-trigger")
                    .debug_selector("palette-trigger")
                    .ghost()
                    .small()
                    .icon(Glyph::Search)
                    .accessibility_label("Commands")
                    .hint("Search actions · Ctrl+Shift+P")
                    .on_click(cx.listener(|v, _, window, cx| v.palette(&Palette, window, cx))),
            );
        }
        if let Some(tab) = self.tabs.get(self.active) {
            let state = if tab.runtime["connection_lost"] == true {
                "Connection lost".to_string()
            } else {
                worker_label(tab.runtime["worker_state"].as_str())
            };
            let mut runtime = action_button("runtime-menu")
                .debug_selector("runtime-menu")
                .ghost()
                .small()
                .rounded_full()
                .bg(cx.theme().popover)
                .accessibility_label(format!("Runtime: Python · {state}"))
                .hint(format!("Python · {state} — runtime controls"))
                .on_click(cx.listener(|v, _, window, cx| {
                    let open = !v.runtime_menu;
                    v.close_menu(window, cx);
                    v.return_focus = window.focused(cx);
                    v.menu_anchor = "runtime".into();
                    v.runtime_menu = open;
                    v.dialog_focus.focus(window, cx);
                    cx.notify();
                }));
            if tight {
                runtime = runtime.icon(Glyph::Cpu);
            } else {
                runtime = runtime
                    .label(if compact {
                        state.clone()
                    } else {
                        format!("Python · {state}")
                    })
                    .dropdown_caret(true);
            }
            let color = if tab.runtime["connection_lost"] == true {
                cx.theme().danger
            } else {
                tab.runtime["worker_state"]
                    .as_str()
                    .map(|s| tone(s, cx))
                    .unwrap_or(cx.theme().muted_foreground)
            };
            if !overflow {
                header = header.child(
                    h_flex()
                        .rounded_full()
                        .bg(cx.theme().popover)
                        .pl_2()
                        .gap_1p5()
                        .items_center()
                        .child(dot(color))
                        .child(self.anchored("runtime", runtime, cx)),
                );
            }
            if self.surface == "Notebook" {
                let mut run = action_button("run-all")
                    .debug_selector("run-all")
                    .primary()
                    .small()
                    .icon(Glyph::Play)
                    .accessibility_label("Run all")
                    .unavailable(self.command_reason("run-all").is_some())
                    .hint(
                        self.command_reason("run-all")
                            .unwrap_or("Run all code cells"),
                    )
                    .on_click(cx.listener(|v, _, window, cx| v.run_command("run-all", window, cx)));
                if !compact {
                    run = run.label("Run all");
                }
                header = header.child(run);
            }
        }
        header = header.child(
            self.anchored(
                "application",
                action_button("application-menu")
                    .debug_selector("application-menu")
                    .ghost()
                    .small()
                    .icon(Glyph::Ellipsis)
                    .accessibility_label("Menu")
                    .hint("Workspace menu")
                    .on_click(cx.listener(|v, _, window, cx| {
                        let open = !v.app_menu;
                        v.close_menu(window, cx);
                        v.return_focus = window.focused(cx);
                        v.menu_anchor = "application".into();
                        v.app_menu = open;
                        v.dialog_focus.focus(window, cx);
                        cx.notify();
                    })),
                cx,
            ),
        );
        gpui_kit::component::TitleBar::new()
            .h(rems(if self.preferences.comfortable {
                3.5
            } else {
                3.25
            }))
            .bg(cx.theme().title_bar)
            .border_b_0()
            .when(self.high_contrast, |bar| {
                bar.border_b_1().border_color(divider(cx))
            })
            .when(!self.high_contrast, |bar| bar.shadow_sm())
            .child(header)
    }
    pub(super) fn menu_overlay(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let mut menu = v_flex()
            .id("workspace-menu-content")
            .overflow_y_scroll()
            .w(px(300.))
            .max_h(px(440.))
            .p_3()
            .gap_2()
            .bg(cx.theme().popover)
            .border_1()
            .border_color(divider(cx))
            .rounded(px(12.))
            .when(!self.high_contrast, |d| d.shadow_md());
        if let Some(index) = self.insert_menu {
            for (kind, label) in [("code", "Code"), ("markdown", "Markdown"), ("raw", "Raw")] {
                menu = menu.child(
                    action_button(SharedString::from(format!("insert-type-{kind}")))
                        .ghost()
                        .label(label)
                        .on_click(cx.listener(move |v, _, window, cx| {
                            v.close_menu(window, cx);
                            v.mutate(json!({"action":"add","type":kind,"index":index}));
                        })),
                );
            }
        } else if let Some((id, index)) = &self.cell_menu {
            for (action, label) in [
                ("duplicate", "Duplicate cell"),
                ("move-up", "Move up"),
                ("move-down", "Move down"),
                ("delete", "Delete cell..."),
            ] {
                let cell_id = id.clone();
                let index = *index;
                menu = menu.child(action_button(action).ghost().label(label)
                    .unavailable((action == "move-up" && index == 0) || (action == "move-down" && index + 1 == self.tabs[self.active].model.order.len()))
                    .on_click(cx.listener(move |v, _, window, cx| {
                        v.close_menu(window, cx);
                        match action {
                            "delete" => { v.confirmation = Some(("delete".into(), json!({"cell_id":cell_id}))); cx.notify(); }
                            "move-up" | "move-down" => v.mutate(json!({"action":"move","cell_id":cell_id,"index": if action == "move-up" { index.saturating_sub(1) } else { index + 1 }})),
                            _ => v.mutate(json!({"action":action,"cell_id":cell_id})),
                        }
                    })));
            }
        } else if self.notebook_menu {
            for (index, tab) in self.tabs.iter().enumerate() {
                menu = menu.child(
                    action_button(("switch-notebook", index))
                        .ghost()
                        .label(
                            tab.model
                                .project
                                .file_name()
                                .unwrap_or_default()
                                .to_string_lossy()
                                .to_string(),
                        )
                        .hint(tab.model.project.display().to_string())
                        .selected(index == self.active)
                        .on_click(cx.listener(move |v, _, window, cx| {
                            if let Some(tab) = v.tabs.get_mut(v.active) {
                                tab.selections.insert(v.surface.clone(), v.record_active);
                            }
                            v.active = index;
                            v.record_active = v.tabs[index]
                                .selections
                                .get(&v.surface)
                                .copied()
                                .unwrap_or(0);
                            v.close_menu(window, cx);
                        })),
                );
            }
            menu = menu.child(
                action_button("switch-open")
                    .ghost()
                    .label("Open notebook…")
                    .on_click(cx.listener(|v, _, window, cx| {
                        v.close_menu(window, cx);
                        v.open_dialog(&OpenNotebook, window, cx);
                    })),
            );
        } else if self.runtime_menu {
            let runtime = &self.tabs[self.active].runtime;
            menu = menu
                .child(div().font_weight(FontWeight::SEMIBOLD).child(
                    if runtime["connection_lost"] == true {
                        "Connection lost — execution state unknown".into()
                    } else {
                        worker_label(runtime["worker_state"].as_str())
                    },
                ))
                .child(
                    div().text_xs().whitespace_normal().child(
                        runtime["interpreter"]["executable"]
                            .as_str()
                            .or(runtime["interpreter"]["python"].as_str())
                            .unwrap_or("Interpreter details are available after running a cell")
                            .to_string(),
                    ),
                );
            for (method, label) in [
                ("interrupt", "Interrupt"),
                ("restart", "Restart empty…"),
                ("stop", "Stop runtime…"),
            ] {
                menu = menu.child(
                    action_button(method)
                        .ghost()
                        .label(label)
                        .unavailable(action_reason(method, runtime).is_some())
                        .hint(action_reason(method, runtime).unwrap_or(label))
                        .on_click(cx.listener(move |v, _, window, cx| {
                            v.runtime_menu = false;
                            v.run_command(method, window, cx);
                        })),
                );
            }
            menu = menu.child(
                action_button("all-runtimes")
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
                action_button("open")
                    .ghost()
                    .label("Open notebook…")
                    .on_click(cx.listener(|v, _, window, cx| {
                        v.app_menu = false;
                        v.open_dialog(&OpenNotebook, window, cx);
                    })),
            );
            menu = menu
                .child(
                    action_button("menu-notebooks")
                        .ghost()
                        .label("Switch notebook…")
                        .on_click(cx.listener(|v, _, window, cx| {
                            v.close_menu(window, cx);
                            v.return_focus = window.focused(cx);
                            v.menu_anchor = "application".into();
                            v.notebook_menu = true;
                            v.dialog_focus.focus(window, cx);
                            cx.notify();
                        })),
                )
                .child(
                    action_button("menu-commands")
                        .ghost()
                        .label("Commands")
                        .on_click(cx.listener(|v, _, window, cx| {
                            v.close_menu(window, cx);
                            v.palette(&Palette, window, cx);
                        })),
                )
                .child(
                    action_button("menu-runtime")
                        .ghost()
                        .label("Runtime")
                        .unavailable(self.tabs.is_empty())
                        .on_click(cx.listener(|v, _, window, cx| {
                            v.close_menu(window, cx);
                            v.return_focus = window.focused(cx);
                            v.menu_anchor = "application".into();
                            v.runtime_menu = true;
                            v.dialog_focus.focus(window, cx);
                            cx.notify();
                        })),
                );
            for surface in ["Settings", "Help", "Diagnostics", "Runtimes"] {
                menu = menu.child(
                    action_button(surface)
                        .ghost()
                        .label(surface)
                        .unavailable(self.tabs.is_empty() && surface != "Help")
                        .on_click(cx.listener(move |v, _, window, cx| {
                            v.app_menu = false;
                            v.select_surface(surface, cx);
                            v.focus.focus(window, cx);
                        })),
                );
            }
        }
        menu = menu.child(
            action_button("close-menu")
                .ghost()
                .label("Close menu")
                .on_click(cx.listener(|v, _, window, cx| v.close_menu(window, cx))),
        );
        div()
            .id("workspace-menu")
            .occlude()
            .track_focus(&self.dialog_focus)
            .absolute()
            .inset_0()
            .flex()
            .on_click(cx.listener(|v, _, window, cx| v.close_menu(window, cx)))
            .child({
                let bounds = self
                    .menu_bounds
                    .get(&self.menu_anchor)
                    .copied()
                    .unwrap_or_default();
                let width = px(300. * self.preferences.ui_scale)
                    .min(window.viewport_size().width - px(24.));
                let left = bounds
                    .origin
                    .x
                    .min(window.viewport_size().width - width - px(12.))
                    .max(px(12.));
                let top = (bounds.origin.y + bounds.size.height + px(4.))
                    .min(window.viewport_size().height - px(180.))
                    .max(px(4.));
                let menu = menu
                    .id("workspace-menu-content")
                    .role(Role::Menu)
                    .aria_label("Workspace menu")
                    .absolute()
                    .left(left)
                    .top(top)
                    .w(width)
                    .max_h(window.viewport_size().height - top - px(12.))
                    .occlude()
                    .on_click(|_, _, cx| cx.stop_propagation());
                // Fade only: the anchored position and input are ready immediately.
                // Native GPUI also honors the operating system's reduced motion.
                if self.preferences.reduced_motion {
                    menu.into_any_element()
                } else {
                    menu.with_animation(
                        SharedString::from(format!("menu-enter-{}", self.menu_anchor)),
                        Animation::new(Duration::from_millis(100)),
                        |surface, progress| surface.opacity(0.5 + 0.5 * progress),
                    )
                    .into_any_element()
                }
            })
    }
    pub(super) fn close_menu(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.app_menu = false;
        self.runtime_menu = false;
        self.notebook_menu = false;
        self.cell_menu = None;
        self.insert_menu = None;
        if let Some(focus) = self.return_focus.take() {
            focus.focus(window, cx);
        }
        cx.notify();
    }
}
