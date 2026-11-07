use super::*;
use gpui_kit::component::ElementExt;

/// Native caption buttons on Windows stay about 102 logical pixels wide.
/// Interface scale grows app controls; it does not grow minimize/maximize/close.
/// Non-Windows title bars do not reserve that right-edge strip in GPUI tests.
#[cfg(windows)]
pub(crate) const CAPTION_GUTTER: f32 = 102.;
#[cfg(not(windows))]
pub(crate) const CAPTION_GUTTER: f32 = 8.;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct HeaderCollapse {
    pub export_label: bool,
    pub run_label: bool,
    pub navigation: bool,
    pub switcher: bool,
    pub switcher_width: f32,
    pub runtime: bool,
    pub export: bool,
    pub settings: bool,
    /// When true, the product wordmark shortens to "KL" so Run all stays labeled.
    pub wordmark_short: bool,
}

// These are also applied to the rendered controls. The only flexible control
// is the notebook identity, which receives all spare room up to 240 scaled px.
pub(crate) fn header_collapse(width: f32, scale: f32, comfortable: bool) -> HeaderCollapse {
    let scale = if scale.is_finite() && scale > 0. {
        scale
    } else {
        1.
    };
    let mut c = HeaderCollapse {
        export_label: false,
        run_label: false,
        navigation: false,
        switcher: false,
        switcher_width: 96. * scale,
        runtime: false,
        export: false,
        settings: false,
        // Title-bar mark is always the short "KL"; the full name lives in the
        // status bar so Run all / Menu keep their caption clearance.
        wordmark_short: true,
    };
    for step in 0..8 {
        if header_fits(width, scale, comfortable, c) {
            break;
        }
        match step {
            0 => c.export_label = true,
            1 => c.navigation = true,
            2 => c.export = true,
            3 => c.runtime = true,
            4 => c.settings = true,
            5 => c.switcher = true,
            _ => c.run_label = true,
        }
    }
    if !c.switcher {
        c.switcher_width += (width - CAPTION_GUTTER - header_used(width, scale, comfortable, c))
            .max(0.)
            .min(144. * scale);
    }
    c
}

fn header_used(width: f32, scale: f32, comfortable: bool, c: HeaderCollapse) -> f32 {
    let compact = width < 1280. * scale;
    let tight = compact && (scale > 1. || comfortable);
    let button = if comfortable { 44. } else { 28. } * scale;
    let gap = if compact { 4. } else { 8. } * scale;
    let mut items = vec![button]; // Outline, including on other destinations as a conservative reserve.
    // Compact product mark ("KL") — full "KernelLedger" is in the status bar.
    items.push(24. * scale);
    if !c.switcher {
        items.push(c.switcher_width);
    }
    items.push(if c.navigation {
        128. * scale
    } else {
        300. * scale
    });
    items.push(8. * scale); // Native window drag region.
    if !tight {
        items.push(button);
    }
    if !c.runtime {
        items.push(116. * scale);
    }
    items.push(if c.run_label { button } else { 92. * scale });
    if !c.export {
        items.push(if c.export_label { button } else { 86. * scale });
    }
    if !c.settings {
        items.push(button);
    }
    items.push(button);
    items.iter().sum::<f32>() + gap * (items.len() - 1) as f32 + 32. * scale
}

pub(crate) fn header_fits(width: f32, scale: f32, comfortable: bool, c: HeaderCollapse) -> bool {
    header_used(width, scale, comfortable, c) <= width - CAPTION_GUTTER + 0.01
}

/// Keep a distinguishing suffix; ask the actual text shaper, not a character budget.
pub(crate) fn middle_ellipsis(name: &str, budget: f32, measure: impl Fn(&str) -> f32) -> String {
    if measure(name) <= budget {
        return name.to_owned();
    }
    let chars: Vec<_> = name.chars().collect();
    // Allocate by shaped width on both sides: a wide prefix must not crowd out
    // the suffix that distinguishes otherwise similar notebook names.
    let ellipsis = measure("\u{2026}");
    let mut prefix = String::new();
    for c in &chars {
        let next = format!("{prefix}{c}");
        if measure(&next) > (budget - ellipsis).max(0.) * 0.4 {
            break;
        }
        prefix = next;
    }
    let prefix_count = prefix.chars().count();
    let mut suffix = String::new();
    for c in chars[prefix_count..].iter().rev() {
        let next = format!("{c}{suffix}");
        if measure(&format!("{prefix}\u{2026}{next}")) > budget {
            break;
        }
        suffix = next;
    }
    if !prefix.is_empty() || !suffix.is_empty() {
        return format!("{prefix}\u{2026}{suffix}");
    }

    "\u{2026}".into()
}

fn runtime_compact_word(connection_lost: bool, state: Option<&str>) -> &'static str {
    if connection_lost {
        return "Lost";
    }
    match state {
        Some("running" | "busy" | "starting" | "queued" | "accepted" | "pending") => "Busy",
        _ => "Idle",
    }
}

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

    #[inline(never)]
    pub(super) fn header(
        &self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> gpui_kit::component::TitleBar {
        let width = window.viewport_size().width / px(1.);
        let scale = self.preferences.ui_scale;
        let compact = width < 1280. * scale;
        let tight = compact && (scale > 1. || self.preferences.comfortable);
        let collapse = header_collapse(width, scale, self.preferences.comfortable);
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
        let path = self
            .tabs
            .get(self.active)
            .map(|tab| tab.model.project.display().to_string())
            .unwrap_or_default();
        let measure = |text: &str| {
            window
                .text_system()
                .shape_line(
                    text.to_owned().into(),
                    px(14. * scale),
                    &[TextRun {
                        len: text.len(),
                        font: Font {
                            family: cx.theme().font_family.clone(),
                            weight: FontWeight::MEDIUM,
                            ..Default::default()
                        },
                        color: cx.theme().foreground,
                        background_color: None,
                        underline: None,
                        strikethrough: None,
                    }],
                    None,
                )
                .width
                / px(1.)
        };
        let label = middle_ellipsis(
            &name,
            (collapse.switcher_width - 36. * scale).max(0.),
            measure,
        );
        let notebooks = action_button("notebook-switcher")
            .debug_selector("notebook-switcher")
            .ghost()
            .small()
            .w(px(collapse.switcher_width))
            .hint(format!("{path} \u{2014} switch or open notebook"))
            .accessibility_label(format!("Notebook switcher: {path}"))
            .dropdown_caret(true)
            .label(label)
            .on_click(cx.listener(|v, _, window, cx| {
                v.close_menu(window, cx);
                v.return_focus = window.focused(cx);
                v.menu_anchor = "switcher".into();
                v.notebook_menu = true;
                v.dialog_focus.focus(window, cx);
                cx.notify();
            }));
        // Compact mark in the title bar; full name is in the status bar so
        // identity survives cropping the OS title (live UX A / LD-02).
        header = header.child(
            div()
                .id("product-wordmark")
                .role(Role::Label)
                .aria_label("KernelLedger")
                .flex_none()
                .px_1()
                .rounded(cx.theme().radius)
                .bg(cx.theme().primary.opacity(0.12))
                .text_color(cx.theme().primary)
                .font_weight(FontWeight::BOLD)
                .text_xs()
                .child("KL"),
        );
        if !self.tabs.is_empty() {
            if self.surface == "Notebook" {
                header = header.child(
                    action_button("outline-toggle")
                        .debug_selector("outline-toggle")
                        .ghost()
                        .small()
                        .icon(Glyph::PanelLeft)
                        .accessibility_label("Outline")
                        .hint("Toggle notebook outline (Ctrl+B)")
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
            if !collapse.switcher {
                header = header.child(self.anchored("switcher", notebooks, cx));
            }
            let mut destinations = segmented_track(cx).self_center().flex_none().flex_nowrap();
            for (surface, _icon) in [
                ("Notebook", Glyph::NotebookPen),
                ("History", Glyph::Clock),
                ("Checkpoints", Glyph::Flag),
            ] {
                if collapse.navigation
                    && self.surface != surface
                    && !(surface == "Notebook"
                        && !matches!(
                            self.surface.as_str(),
                            "Notebook" | "History" | "Checkpoints"
                        ))
                {
                    continue;
                }
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
                    .hint(if collapse.navigation {
                        "Destinations: Notebook, History, Checkpoints (Ctrl+1/2/3)".to_owned()
                    } else {
                        format!(
                            "{} ({})",
                            surface,
                            match surface {
                                "Notebook" => "Ctrl+1",
                                "History" => "Ctrl+2",
                                _ => "Ctrl+3",
                            }
                        )
                    })
                    .on_click(cx.listener(move |v, _, window, cx| {
                        if collapse.navigation {
                            v.close_menu(window, cx);
                            v.return_focus = window.focused(cx);
                            v.menu_anchor = "application".into();
                            v.app_menu = true;
                            v.dialog_focus.focus(window, cx);
                            cx.notify();
                        } else {
                            v.select_surface(surface, cx);
                            v.focus.focus(window, cx);
                        }
                    }));
                button = button
                    .label(surface)
                    .w(px(if collapse.navigation { 116. } else { 96. } * scale));
                if collapse.navigation {
                    button = button.dropdown_caret(true);
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
                    .hint("Search actions (Ctrl+Shift+P)")
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
                .accessibility_label(format!("Runtime: Python ({state})"))
                .hint(format!("Python ({state}) \u{2014} runtime controls"))
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
                runtime = runtime.icon(Glyph::Cpu).label(runtime_compact_word(
                    tab.runtime["connection_lost"] == true,
                    tab.runtime["worker_state"].as_str(),
                ));
            } else {
                runtime = runtime.label(state.clone()).dropdown_caret(true);
            }
            let color = if tab.runtime["connection_lost"] == true {
                cx.theme().danger
            } else {
                tab.runtime["worker_state"]
                    .as_str()
                    .map(|s| tone(s, cx))
                    .unwrap_or(cx.theme().muted_foreground)
            };
            if !collapse.runtime {
                header = header.child(
                    h_flex()
                        .w(px(116. * scale))
                        .flex_none()
                        .pl_1()
                        .gap_1()
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
                if !collapse.run_label {
                    run = run.label("Run all");
                }
                header = header.child(run.w(px(if collapse.run_label {
                    if self.preferences.comfortable {
                        44.
                    } else {
                        28.
                    }
                } else {
                    92.
                } * scale)));
            }
            if !collapse.export {
                let mut export = action_button("export-notebook")
                    .debug_selector("export-notebook")
                    .ghost()
                    .small()
                    .icon(Glyph::FileText)
                    .accessibility_label("Export notebook")
                    .hint("Export PDF, Word, Jupyter, or bundle (Ctrl+Shift+E)")
                    .unavailable(self.command_reason("export-menu").is_some())
                    .on_click(cx.listener(|v, _, window, cx| v.toggle_export(window, cx)));
                if !collapse.export_label {
                    export = export.label("Export");
                }
                header = header.child(self.anchored(
                    "export",
                    export.w(px(if collapse.export_label {
                        if self.preferences.comfortable {
                            44.
                        } else {
                            28.
                        }
                    } else {
                        86.
                    } * scale)),
                    cx,
                ));
            }
        }
        let settings = action_button("settings-button")
            .debug_selector("settings-button")
            .ghost()
            .small()
            .icon(Glyph::Settings)
            .selected(self.surface == "Settings")
            .accessibility_label("Settings")
            .unavailable(self.command_reason("settings").is_some())
            .hint(
                self.command_reason("settings")
                    .unwrap_or("Settings (Ctrl+,)"),
            )
            .on_click(cx.listener(|v, _, window, cx| {
                v.close_menu(window, cx);
                v.run_command("settings", window, cx);
                v.focus.focus(window, cx);
            }));
        if !collapse.settings {
            header = header.child(settings);
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
                3.25
            } else {
                2.875
            }))
            .bg(cx.theme().title_bar)
            .border_b_1()
            .border_color(divider(cx))
            .child(header)
    }
    #[inline(never)]
    pub(super) fn menu_overlay(&self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let menu_sep = |cx: &App| div().h(px(1.)).w_full().my_1().bg(divider(cx));
        let mut menu = v_flex()
            .id("workspace-menu-content")
            .debug_selector(|| "workspace-menu-content".into())
            .overflow_y_scroll()
            .w(px(272.))
            .max_h(px(440.))
            .p_1()
            .gap_0p5()
            .items_stretch()
            .bg(cx.theme().popover)
            .border_1()
            .border_color(surface_edge(cx))
            .rounded(px(8.))
            .when(!self.high_contrast, |d| d.shadow_lg());
        if self.export_menu {
            menu = menu
                .child(
                    div()
                        .px_2()
                        .py_1()
                        .text_sm()
                        .font_weight(FontWeight::SEMIBOLD)
                        .child("Export notebook"),
                )
                .child(
                    div()
                        .px_2()
                        .pb_2()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child("PDF and Word include saved source and recorded outputs."),
                );
            for (format, label, description) in [
                (
                    "pdf",
                    "PDF document (.pdf)",
                    "A paginated document for reading and sharing",
                ),
                (
                    "docx",
                    "Word document (.docx)",
                    "An editable document for reports",
                ),
                (
                    "ipynb",
                    "Jupyter notebook (.ipynb)",
                    "Current source in Jupyter format",
                ),
                (
                    "zip",
                    "Portable bundle (.zip)",
                    "Source and recorded history",
                ),
            ] {
                menu = menu.child(
                    action_button(SharedString::from(format!("export-{format}")))
                        .ghost()
                        .w_full()
                        .menu_item(label, None)
                        .hint(description)
                        .on_click(cx.listener(move |v, _, window, cx| {
                            v.close_menu(window, cx);
                            v.export_dialog(format, window, cx);
                        })),
                );
            }
        } else if let Some(index) = self.insert_menu {
            for (kind, label, icon) in [
                ("code", "Code cell", Glyph::Code),
                ("markdown", "Markdown prose", Glyph::Type),
                ("raw", "Raw text", Glyph::FileText),
            ] {
                menu = menu.child(
                    action_button(SharedString::from(format!("insert-type-{kind}")))
                        .ghost()
                        .w_full()
                        .icon(icon)
                        .menu_item(label, None)
                        .on_click(cx.listener(move |v, _, window, cx| {
                            v.close_menu(window, cx);
                            v.mutate(json!({"action":"add","type":kind,"index":index}));
                        })),
                );
            }
        } else if let Some((id, _)) = self.cell_menu.as_ref().filter(|_| self.cell_menu_assistant) {
            menu = menu
                .child(
                    action_button("cell-menu-back")
                        .ghost()
                        .w_full()
                        .menu_item("\u{2039} Back", Some("left"))
                        .accessibility_label("Back to cell actions")
                        .on_click(cx.listener(|v, _, window, cx| {
                            v.cell_menu_assistant = false;
                            v.dialog_focus.focus(window, cx);
                            cx.notify();
                        })),
                )
                .child(menu_sep(cx))
                .child(
                    div()
                        .px_2()
                        .py_0p5()
                        .text_xs()
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(cx.theme().muted_foreground)
                        .child("Assistant"),
                );
            for (action, label) in CELL_ASSISTANT_ACTIONS {
                let id = id.clone();
                menu = menu.child(
                    action_button(action)
                        .ghost()
                        .w_full()
                        .menu_item(label, None)
                        .on_click(cx.listener(move |v, _, window, cx| {
                            v.tabs[v.active].model.selected = Some(id.clone());
                            v.tabs[v.active].assistant.selected_cells = vec![id.clone()];
                            v.close_menu(window, cx);
                            v.run_command(action, window, cx);
                        })),
                );
            }
        } else if let Some((id, index)) = &self.cell_menu {
            for (action, label) in [
                ("duplicate", "Duplicate cell"),
                ("move-up", "Move up"),
                ("move-down", "Move down"),
                ("assistant-group", "Assistant"),
                ("delete", "Delete cell\u{2026}"),
            ] {
                let cell_id = id.clone();
                let index = *index;
                if action == "assistant-group" {
                    menu = menu.child(menu_sep(cx)).child(
                        action_button("cell-menu-assistant")
                            .ghost()
                            .w_full()
                            .menu_item(format!("{label} \u{203a}"), Some("right"))
                            .accessibility_label("Assistant actions")
                            .hint("Ask, suggest an edit, refactor, document, or generate tests")
                            .on_click(cx.listener(|v, _, window, cx| {
                                v.cell_menu_assistant = true;
                                v.dialog_focus.focus(window, cx);
                                cx.notify();
                            })),
                    );
                    continue;
                }
                if action == "delete" {
                    menu = menu.child(menu_sep(cx));
                }
                menu = menu.child(
                    action_button(action)
                        .ghost()
                        .w_full()
                        .menu_item(
                            label,
                            match action {
                                "duplicate" => Some("ctrl-shift-d"),
                                _ => None,
                            },
                        )
                        .unavailable(
                            (action == "move-up" && index == 0)
                                || (action == "move-down"
                                    && index + 1 == self.tabs[self.active].model.order.len()),
                        )
                        .on_click(cx.listener(move |v, _, window, cx| {
                            v.close_menu(window, cx);
                            match action {
                                "delete" => {
                                    v.confirmation =
                                        Some(("delete".into(), json!({"cell_id":cell_id})));
                                    cx.notify();
                                }
                                "move-up" | "move-down" => v.mutate(json!({
                                    "action":"move",
                                    "cell_id":cell_id,
                                    "index": if action == "move-up" {
                                        index.saturating_sub(1)
                                    } else {
                                        index + 1
                                    }
                                })),
                                _ => v.mutate(json!({"action":action,"cell_id":cell_id})),
                            }
                        })),
                );
            }
        } else if self.notebook_menu {
            for (index, tab) in self.tabs.iter().enumerate() {
                let basename = tab
                    .model
                    .project
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy();
                let duplicate = self
                    .tabs
                    .iter()
                    .filter(|other| {
                        other.model.project.file_name() == tab.model.project.file_name()
                    })
                    .count()
                    > 1;
                let label = if duplicate {
                    format!(
                        "{basename} \u{2014} {}",
                        tab.model
                            .project
                            .parent()
                            .unwrap_or(&tab.model.project)
                            .display()
                    )
                } else {
                    basename.to_string()
                };
                menu = menu.child(
                    action_button(("switch-notebook", index))
                        .ghost()
                        .w_full()
                        .menu_item(label, None)
                        .accessibility_label(format!(
                            "Notebook: {}{}",
                            tab.model.project.display(),
                            if index == self.active {
                                ", current notebook"
                            } else {
                                ""
                            }
                        ))
                        .hint(tab.model.project.display().to_string())
                        .segment(index == self.active)
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
            menu = menu.child(menu_sep(cx)).child(
                action_button("switch-open")
                    .ghost()
                    .w_full()
                    .icon(Glyph::FolderOpen)
                    .menu_item("Open notebook\u{2026}", Some("ctrl-o"))
                    .on_click(cx.listener(|v, _, window, cx| {
                        v.close_menu(window, cx);
                        v.open_dialog(&OpenNotebook, window, cx);
                    })),
            );
        } else if self.runtime_menu {
            let runtime = &self.tabs[self.active].runtime;
            let status_color = if runtime["connection_lost"] == true {
                cx.theme().danger
            } else {
                runtime["worker_state"]
                    .as_str()
                    .map(|s| tone(s, cx))
                    .unwrap_or(cx.theme().muted_foreground)
            };
            menu = menu
                .child(
                    v_flex()
                        .gap_1()
                        .px_2p5()
                        .py_2()
                        .rounded(px(7.))
                        .bg(cx.theme().muted)
                        .child(
                            h_flex()
                                .gap_2()
                                .items_center()
                                .child(dot(status_color))
                                .child(div().text_sm().font_weight(FontWeight::SEMIBOLD).child(
                                    if runtime["connection_lost"] == true {
                                        "Connection lost \u{2014} execution state unknown".into()
                                    } else {
                                        worker_label(runtime["worker_state"].as_str())
                                    },
                                )),
                        )
                        .child(
                            div()
                                .text_xs()
                                .text_color(cx.theme().muted_foreground)
                                .font_family(cx.theme().mono_font_family.clone())
                                .whitespace_normal()
                                .child(
                                    runtime["interpreter"]["executable"]
                                        .as_str()
                                        .or(runtime["interpreter"]["python"].as_str())
                                        .unwrap_or(
                                            "Interpreter details are available after running a cell",
                                        )
                                        .to_string(),
                                ),
                        ),
                )
                .child(menu_sep(cx));
            for (method, label) in [
                ("interrupt", "Interrupt"),
                ("restart", "Restart empty\u{2026}"),
                ("stop", "Stop runtime\u{2026}"),
            ] {
                menu = menu.child(
                    action_button(method)
                        .ghost()
                        .w_full()
                        .menu_item(
                            label,
                            match method {
                                "interrupt" => Some("ctrl-shift-i"),
                                _ => None,
                            },
                        )
                        .unavailable(action_reason(method, runtime).is_some())
                        .hint(action_reason(method, runtime).unwrap_or(label))
                        .on_click(cx.listener(move |v, _, window, cx| {
                            v.runtime_menu = false;
                            v.run_command(method, window, cx);
                        })),
                );
            }
            menu = menu.child(menu_sep(cx)).child(
                action_button("all-runtimes")
                    .ghost()
                    .w_full()
                    .menu_item("All runtimes", None)
                    .on_click(cx.listener(|v, _, window, cx| {
                        v.runtime_menu = false;
                        v.select_surface("Runtimes", cx);
                        v.focus.focus(window, cx);
                    })),
            );
        } else {
            menu = menu
                .child(
                    action_button("open")
                        .ghost()
                        .w_full()
                        .debug_selector("open")
                        .menu_item("Open notebook\u{2026}", Some("ctrl-o"))
                        .on_click(cx.listener(|v, _, window, cx| {
                            v.app_menu = false;
                            v.open_dialog(&OpenNotebook, window, cx);
                        })),
                )
                .child(
                    action_button("inspect-bundle")
                        .ghost()
                        .w_full()
                        .menu_item("Inspect bundle\u{2026}", None)
                        .unavailable(self.tabs.is_empty())
                        .on_click(cx.listener(|v, _, window, cx| {
                            v.app_menu = false;
                            v.run_command("inspect-bundle", window, cx);
                        })),
                )
                .child(
                    action_button("menu-notebooks")
                        .ghost()
                        .w_full()
                        .menu_item("Switch notebook\u{2026}", None)
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
                        .w_full()
                        .menu_item("Commands", Some("ctrl-shift-p"))
                        .on_click(cx.listener(|v, _, window, cx| {
                            v.close_menu(window, cx);
                            v.palette(&Palette, window, cx);
                        })),
                )
                .child(
                    action_button("menu-assistant")
                        .ghost()
                        .w_full()
                        .icon(Glyph::Sparkles)
                        .menu_item("Assistant", None)
                        .unavailable(self.tabs.is_empty())
                        .on_click(cx.listener(|v, _, window, cx| {
                            v.close_menu(window, cx);
                            v.open_assistant(cx);
                        })),
                )
                .child(
                    action_button("menu-runtime")
                        .ghost()
                        .w_full()
                        .menu_item("Runtime", None)
                        .unavailable(self.tabs.is_empty())
                        .on_click(cx.listener(|v, _, window, cx| {
                            v.close_menu(window, cx);
                            v.return_focus = window.focused(cx);
                            v.menu_anchor = "application".into();
                            v.runtime_menu = true;
                            v.dialog_focus.focus(window, cx);
                            cx.notify();
                        })),
                )
                .child(menu_sep(cx));
            if !self.tabs.is_empty() {
                menu = menu.child(
                    action_button("menu-export")
                        .ghost()
                        .w_full()
                        .menu_item("Export notebook\u{2026}", Some("ctrl-shift-e"))
                        .unavailable(self.command_reason("export-menu").is_some())
                        .hint("Export notebook (Ctrl+Shift+E)")
                        .on_click(cx.listener(|v, _, window, cx| v.toggle_export(window, cx))),
                );
            }
            for (surface, shortcut) in [
                ("Notebook", "ctrl-1"),
                ("History", "ctrl-2"),
                ("Checkpoints", "ctrl-3"),
            ] {
                menu = menu.child(
                    action_button(SharedString::from(format!("destination-{surface}")))
                        .debug_selector(surface)
                        .ghost()
                        .w_full()
                        .menu_item(surface, Some(shortcut))
                        .segment(self.surface == surface)
                        .accessibility_label(format!(
                            "{surface}{}",
                            if self.surface == surface {
                                ", current destination"
                            } else {
                                ""
                            }
                        ))
                        .on_click(cx.listener(move |v, _, window, cx| {
                            v.close_menu(window, cx);
                            v.select_surface(surface, cx);
                            v.focus.focus(window, cx);
                        })),
                );
            }
            for surface in ["Settings", "Help", "Diagnostics", "Runtimes"] {
                menu = menu.child(
                    action_button(surface)
                        .ghost()
                        .w_full()
                        .debug_selector(surface)
                        .menu_item(
                            surface,
                            match surface {
                                "Settings" => Some("ctrl-,"),
                                "Help" => Some("f1"),
                                _ => None,
                            },
                        )
                        .unavailable(self.tabs.is_empty() && surface != "Help")
                        .on_click(cx.listener(move |v, _, window, cx| {
                            v.app_menu = false;
                            v.select_surface(surface, cx);
                            v.focus.focus(window, cx);
                        })),
                );
            }
        }
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
                let width = px(280. * self.preferences.ui_scale)
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
                        Animation::new(Duration::from_millis(110)),
                        |surface, progress| surface.opacity(0.45 + 0.55 * progress),
                    )
                    .into_any_element()
                }
            })
            .into_any_element()
    }
    pub(super) fn close_menu(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.app_menu = false;
        self.export_menu = false;
        self.runtime_menu = false;
        self.notebook_menu = false;
        self.cell_menu = None;
        self.cell_menu_assistant = false;
        self.insert_menu = None;
        if let Some(focus) = self.return_focus.take() {
            focus.focus(window, cx);
        }
        cx.notify();
    }
}
