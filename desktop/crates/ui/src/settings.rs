use std::rc::Rc;

use super::*;

type SettingsAction = Rc<dyn Fn(&mut Workspace, &mut Window, &mut Context<Workspace>)>;

fn revealed(
    button: WorkbenchButton,
    reveal: GroupReveal,
    cx: &mut Context<Workspace>,
    apply: SettingsAction,
) -> WorkbenchButton {
    let view = cx.entity();
    let a11y_reveal = reveal.clone();
    let a11y_apply = apply.clone();
    button
        .on_a11y_click(move |window, cx| {
            let reveal = a11y_reveal.clone();
            let apply = a11y_apply.clone();
            view.update(cx, |v, cx| {
                reveal.activate(v, window, cx, move |v, window, cx| apply(v, window, cx));
            });
        })
        .on_click(cx.listener(move |v, _, window, cx| {
            let apply = apply.clone();
            reveal.activate(v, window, cx, move |v, window, cx| apply(v, window, cx));
        }))
}

impl Workspace {
    pub(super) fn save_preferences(&mut self, cx: &mut Context<Self>) {
        self.preferences.normalize();
        // Headless interaction tests must never overwrite the user's profile.
        // Persistence itself is exercised against temporary paths in the client tests.
        if cfg!(test) {
            return;
        }
        if self.preference_saving {
            return;
        }
        self.preference_saving = true;
        let preferences = self.preferences.clone();
        let submitted = preferences.clone();
        self.preference_status = Some("Saving preferences\u{2026}".into());
        cx.spawn(async move |view, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { preferences.save(&kernelledger_client::profile_path()) })
                .await;
            let _ = view.update(cx, |v, cx| {
                v.preference_saving = false;
                v.preference_status = Some(match result {
                    Ok(()) => "Preferences saved".into(),
                    Err(error) => format!("Preferences were not saved: {error}"),
                });
                if v.preferences != submitted {
                    v.save_preferences(cx);
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub(super) fn settings_page(
        &mut self,
        value: &Value,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let section = self.settings_section;
        let key = self.scroll_key();
        let settings_scroll = self.tabs[self.active]
            .scrolls
            .entry(key)
            .or_default()
            .clone();
        let appearance_reveal = GroupReveal::new(settings_scroll.clone());
        let type_reveal = GroupReveal::new(settings_scroll.clone());
        let layout_reveal = GroupReveal::new(settings_scroll.clone());
        let access_reveal = GroupReveal::new(settings_scroll.clone());
        let runtime_reveal = GroupReveal::new(settings_scroll.clone());
        let details_reveal = GroupReveal::new(settings_scroll);
        let mut appearance = segmented_track(cx);
        for (key, label) in [
            ("system", "System"),
            ("light", "Light"),
            ("dark", "Dark"),
            ("contrast", "High contrast"),
        ] {
            let selected = if key == "system" {
                self.follow_system
            } else {
                !self.follow_system
                    && match key {
                        "contrast" => self.high_contrast,
                        "dark" => self.dark && !self.high_contrast,
                        _ => !self.dark && !self.high_contrast,
                    }
            };
            appearance = appearance.child(revealed(
                action_button(SharedString::from(format!("appearance-{key}")))
                    .small()
                    .label(label)
                    .segment(selected),
                appearance_reveal.clone(),
                cx,
                Rc::new(move |v, window, cx| {
                    v.follow_system = key == "system";
                    v.dark = key == "dark"
                        || (v.follow_system
                            && matches!(
                                window.appearance(),
                                WindowAppearance::Dark | WindowAppearance::VibrantDark
                            ));
                    v.high_contrast =
                        key == "contrast" || (v.follow_system && system_high_contrast());
                    persist_appearance(key, cx);
                    apply_theme(v.dark, v.high_contrast, window, cx);
                    cx.notify();
                }),
            ));
        }
        let mut scales = segmented_track(cx);
        for scale in [1., 1.25, 1.5, 2.] {
            scales = scales.child(revealed(
                action_button(SharedString::from(format!("ui-scale-{scale}")))
                    .small()
                    .label(format!("{}%", (scale * 100.) as u32))
                    .accessibility_label(format!(
                        "Interface size {} percent",
                        (scale * 100.) as u32
                    ))
                    .segment(self.preferences.ui_scale == scale),
                type_reveal.clone(),
                cx,
                Rc::new(move |v, window, cx| {
                    v.preferences.ui_scale = scale;
                    v.save_preferences(cx);
                    for tab in &mut v.tabs {
                        tab.list
                            .splice(0..tab.model.order.len(), tab.model.order.len());
                    }
                    window.refresh();
                    cx.notify();
                }),
            ));
        }
        let mut sizes = segmented_track(cx);
        for size in [12., 14., 16., 18., 20., 24.] {
            sizes = sizes.child(revealed(
                action_button(SharedString::from(format!("editor-size-{size}")))
                    .ghost()
                    .small()
                    .label(format!("{size} px"))
                    .segment(self.preferences.editor_size == size),
                type_reveal.clone(),
                cx,
                Rc::new(move |v, window, cx| {
                    v.preferences.editor_size = size;
                    v.save_preferences(cx);
                    for tab in &mut v.tabs {
                        tab.list
                            .splice(0..tab.model.order.len(), tab.model.order.len());
                    }
                    window.refresh();
                    cx.notify();
                }),
            ));
        }
        let mut widths = segmented_track(cx);
        for width in [180., 220., 260., 300.] {
            widths = widths.child(revealed(
                action_button(SharedString::from(format!("outline-width-{width}")))
                    .ghost()
                    .small()
                    .label(format!("{width} px"))
                    .segment(self.preferences.outline_width == width),
                layout_reveal.clone(),
                cx,
                Rc::new(move |v, _, cx| {
                    v.preferences.outline_width = width;
                    v.save_preferences(cx);
                    cx.notify();
                }),
            ));
        }
        let mut density = segmented_track(cx);
        for (comfortable, label) in [(false, "Compact"), (true, "Comfortable")] {
            density = density.child(revealed(
                action_button(SharedString::from(format!("density-{comfortable}")))
                    .small()
                    .label(label)
                    .segment(self.preferences.comfortable == comfortable),
                layout_reveal.clone(),
                cx,
                Rc::new(move |v, _, cx| {
                    v.preferences.comfortable = comfortable;
                    v.save_preferences(cx);
                    for tab in &mut v.tabs {
                        tab.list
                            .splice(0..tab.model.order.len(), tab.model.order.len());
                    }
                    cx.notify();
                }),
            ));
        }
        let interpreter = &self.tabs[self.active].runtime["interpreter"];
        let path = interpreter["executable"]
            .as_str()
            .or(interpreter["python"].as_str())
            .or(value["path"].as_str())
            .map(str::to_owned);
        let version = interpreter["version"]
            .as_str()
            .unwrap_or("Version available after runtime start")
            .to_string();
        let details = self.tabs[self.active]
            .raw_details
            .get("interpreter-path")
            .copied()
            .unwrap_or(false);
        let mut runtime_actions = h_flex().gap_2().flex_wrap().items_center().child(revealed(
            action_button("interpreter")
                .outline()
                .small()
                .label("Choose interpreter\u{2026}")
                .accessibility_label("Choose project interpreter\u{2026}"),
            runtime_reveal.clone(),
            cx,
            Rc::new(|v, window, cx| v.choose_interpreter(window, cx)),
        ));
        if let Some(ref p) = path {
            let p_copy = p.clone();
            runtime_actions = runtime_actions.child(revealed(
                action_button("interpreter-details")
                    .ghost()
                    .small()
                    .label(if details { "Hide path" } else { "Show path" }),
                runtime_reveal.clone(),
                cx,
                Rc::new(|v, _, cx| {
                    let flag = v.tabs[v.active]
                        .raw_details
                        .entry("interpreter-path".into())
                        .or_default();
                    *flag = !*flag;
                    cx.notify();
                }),
            ));
            if details {
                runtime_actions = runtime_actions.child(revealed(
                    action_button("copy-interpreter-path")
                        .ghost()
                        .small()
                        .label("Copy path")
                        .accessibility_label("Copy interpreter path"),
                    runtime_reveal.clone(),
                    cx,
                    Rc::new(move |_, _, cx| copy_acknowledged(p_copy.clone(), cx)),
                ));
            }
        }
        let mut runtime = settings_group("Project interpreter", cx)
            .id("settings-interpreter")
            .child(
                div()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child(version),
            )
            .child(runtime_actions);
        if let Some(path) = path {
            if details {
                runtime = runtime.child(
                    div()
                        .p_2p5()
                        .rounded(px(6.))
                        .bg(cx.theme().muted)
                        .font_family(cx.theme().mono_font_family.clone())
                        .text_xs()
                        .whitespace_normal()
                        .child(path),
                );
            }
        } else {
            runtime = runtime.child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child("No project interpreter selected."),
            );
        }
        if !value.is_null() {
            runtime = runtime.child(
                div().text_sm().child(
                    value["message"]
                        .as_str()
                        .unwrap_or("Interpreter validation is available in technical details.")
                        .to_string(),
                ),
            );
        }
        let mut groups = sheet(cx);
        match section {
            "appearance" => {
                groups = groups.child(
                    appearance_reveal
                        .track(
                            settings_group_with(
                                "Appearance",
                                "Match your operating system or choose a dedicated contrast mode.",
                                cx,
                            )
                            .id("settings-appearance"),
                        )
                        .child(appearance),
                );
            }
            "editor" => {
                groups = groups.child(
                    type_reveal
                        .track(settings_group("Typography", cx).id("settings-typography"))
                        .child(setting_row(
                            "UI scale",
                            "Scale navigation, controls, and reading surfaces.",
                            scales,
                            cx,
                        ))
                        .child(setting_row(
                            "Editor font",
                            "Monospace font size inside notebook cells.",
                            sizes,
                            cx,
                        )),
                );
            }
            "layout" => {
                groups = groups
                    .child(
                        layout_reveal
                            .track(settings_group("Layout", cx).id("settings-layout"))
                            .child(setting_row(
                                "Spacing",
                                "Compact keeps cells close together. Comfortable adds room inside cells and enlarges pointer targets to at least 44 by 44 px.",
                                density,
                                cx,
                            ))
                            .child({
                                let view = cx.entity();
                                let reveal = layout_reveal.clone();
                                let a11y_reveal = reveal.clone();
                                Checkbox::new("show-cell-duration")
                                    .label("Show last recorded cell duration")
                                    .checked(self.preferences.show_cell_duration)
                                    .on_a11y_action(accesskit::Action::Click, move |_, window, cx| {
                                        view.update(cx, |v, cx| {
                                            a11y_reveal.activate(v, window, cx, |v, _, cx| {
                                                v.preferences.show_cell_duration = !v.preferences.show_cell_duration;
                                                v.save_preferences(cx);
                                                for tab in &mut v.tabs { tab.list.splice(0..tab.model.order.len(), tab.model.order.len()); }
                                                cx.notify();
                                            });
                                        });
                                    })
                                    .on_click(cx.listener(move |v, checked: &bool, window, cx| {
                                        let checked = *checked;
                                        reveal.activate(v, window, cx, move |v, _, cx| {
                                            v.preferences.show_cell_duration = checked;
                                            v.save_preferences(cx);
                                            for tab in &mut v.tabs { tab.list.splice(0..tab.model.order.len(), tab.model.order.len()); }
                                            cx.notify();
                                        });
                                    }))
                            })
                            .child(setting_row(
                                "Outline width",
                                "Default width of the notebook cell outline sidebar.",
                                widths,
                                cx,
                            )),
                    )
                    .child(
                        access_reveal.track(
                            settings_group("Accessibility", cx)
                                .id("settings-accessibility")
                                .child({
                            let checked = self.preferences.reduced_motion;
                            let view = cx.entity();
                            let reveal = access_reveal.clone();
                            Checkbox::new("reduced-motion")
                                .label("Reduce motion")
                                .checked(checked)
                                .on_a11y_action(accesskit::Action::Click, {
                                    let reveal = reveal.clone();
                                    move |_, window, cx| {
                                        view.update(cx, |v, cx| {
                                            let next = !v.preferences.reduced_motion;
                                            reveal.activate(v, window, cx, move |v, _, cx| {
                                                v.preferences.reduced_motion = next;
                                                v.save_preferences(cx);
                                                cx.notify();
                                            });
                                        });
                                    }
                                })
                                .on_click(cx.listener(move |v, value: &bool, window, cx| {
                                    let value = *value;
                                    reveal.activate(v, window, cx, move |v, _, cx| {
                                        v.preferences.reduced_motion = value;
                                        v.save_preferences(cx);
                                        cx.notify();
                                    });
                                }))
                                }),
                        ),
                    );
            }
            "runtime" => {
                groups = groups
                    .child(runtime_reveal.track(runtime))
                    .child(self.interactivity_settings(cx));
            }
            _ => {
                groups = groups.child(self.ai_settings(window, cx));
            }
        }
        let content = v_flex()
            .flex_1()
            .min_w_0()
            .gap_4()
            .child(groups)
            .when(matches!(section, "appearance" | "editor" | "layout"), |d| {
                d.child(self.settings_preview(cx))
            })
            .when(section == "runtime" && !value.is_null(), |d| {
                d.child(self.record_details_in(value, Some(details_reveal), cx))
            });
        let heading = h_flex()
            .gap_3()
            .flex_wrap()
            .items_center()
            .child(page_heading("Settings", "", cx).flex_none())
            .when_some(self.preference_status.clone(), |d, message| {
                d.child(
                    div()
                        .id("preference-status")
                        .role(Role::Status)
                        .aria_label(message.clone())
                        .text_sm()
                        .text_color(cx.theme().muted_foreground)
                        .child(message),
                )
            });
        let body = if self.compact {
            v_flex()
                .gap_4()
                .child(heading)
                .child(self.settings_selector(cx))
                .child(content)
        } else {
            v_flex().gap_4().child(heading).child(
                h_flex()
                    .items_start()
                    .gap_6()
                    .child(self.settings_rail(cx))
                    .child(content),
            )
        };
        self.record_page(body, cx)
    }

    fn select_settings_section(&mut self, key: &'static str, cx: &mut Context<Self>) {
        if self.settings_section != key {
            self.settings_section = key;
            if key == "assistant" && self.tabs[self.active].assistant.settings.is_null() {
                self.ai("settings.get", json!({}));
            }
            cx.notify();
        }
    }

    fn settings_rail(&self, cx: &mut Context<Self>) -> Div {
        let mut rail = v_flex()
            .id("settings-sections")
            .role(Role::List)
            .aria_label("Settings sections")
            .w(rems(12.5))
            .flex_none()
            .gap(px(2.))
            .p(px(3.))
            .rounded(px(8.))
            .bg(cx.theme().muted);
        for (key, title, description) in SECTIONS {
            rail = rail.child(
                action_button(SharedString::from(format!("settings-nav-{key}")))
                    .menu_item(title, None)
                    .segment(self.settings_section == key)
                    .accessibility_label(format!("{title} settings"))
                    .hint(description)
                    .on_click(cx.listener(move |v, _, _, cx| v.select_settings_section(key, cx))),
            );
        }
        div().child(rail)
    }

    fn settings_selector(&self, cx: &mut Context<Self>) -> Div {
        let mut track = segmented_track(cx);
        for (key, title, description) in SECTIONS {
            track = track.child(
                action_button(SharedString::from(format!("settings-nav-{key}")))
                    .small()
                    .label(title.split(" & ").next().unwrap_or(title))
                    .segment(self.settings_section == key)
                    .accessibility_label(format!("{title} settings"))
                    .hint(description)
                    .on_click(cx.listener(move |v, _, _, cx| v.select_settings_section(key, cx))),
            );
        }
        track
    }

    /// A static sample cell drawn with the notebook's spacing and font size. Nothing in it runs.
    fn settings_preview(&self, cx: &App) -> Stateful<Div> {
        let theme = cx.theme();
        let spacing = cell_spacing(self.preferences.comfortable);
        let size = self.preferences.editor_size;
        let mono = theme.mono_font_family.clone();
        let muted = theme.muted_foreground;
        let rule = divider(cx);
        let inline = notebook::compact_source_layout(
            self.preferences.comfortable,
            self.preferences.ui_scale,
            size,
            self.tabs[self.active].runtime["interactive"]["features"]
                .as_array()
                .is_some_and(|features| features.iter().any(|f| f == "debugger")),
        );
        let code = |text: &'static str| {
            div()
                .font_family(mono.clone())
                .text_size(px(size))
                .line_height(px(size * 1.5))
                .whitespace_nowrap()
                .child(text)
        };
        let cell = |index: &'static str,
                    lines: Vec<&'static str>,
                    result: Option<&'static str>,
                    last: bool| {
            v_flex()
                .w_full()
                .when(!inline, |d| {
                    d.child(
                        h_flex()
                            .min_h(rems(spacing.header_min_h))
                            .py(px(spacing.header_py))
                            .px_3()
                            .gap_2()
                            .items_center()
                            .text_xs()
                            .text_color(muted)
                            .child(div().font_family(mono.clone()).child(index))
                            .child("Code")
                            .child(div().flex_1())
                            .child(Icon::new(Glyph::Play).with_size(px(12.)))
                            .child("Run"),
                    )
                })
                .child(
                    h_flex()
                        .items_start()
                        .gap_2()
                        .px_3()
                        .pt(px(spacing.source_top + 8.))
                        .pb(px(spacing.source_bottom + 8.))
                        .overflow_hidden()
                        .when(inline, |d| {
                            d.child(
                                h_flex()
                                    .w(rems(4.5))
                                    .flex_none()
                                    .items_center()
                                    .gap_1()
                                    .child(
                                        div()
                                            .min_h(rems(1.875))
                                            .min_w(rems(1.75))
                                            .flex()
                                            .items_center()
                                            .justify_center()
                                            .rounded_full()
                                            .bg(theme.primary.opacity(0.11))
                                            .text_color(theme.primary)
                                            .child(Icon::new(Glyph::Play).small()),
                                    )
                                    .child(
                                        div()
                                            .font_family(mono.clone())
                                            .text_xs()
                                            .text_color(muted)
                                            .child(index),
                                    ),
                            )
                        })
                        .child(
                            v_flex()
                                .flex_1()
                                .min_w_0()
                                .children(lines.into_iter().map(&code)),
                        )
                        .when(inline, |d| {
                            d.child(
                                div()
                                    .flex_none()
                                    .min_h(rems(1.875))
                                    .child(Icon::new(Glyph::Ellipsis).small()),
                            )
                        }),
                )
                .when_some(result, |d, result| {
                    d.child(
                        div()
                            .mx_3()
                            .py(px(spacing.output_py))
                            .border_t_1()
                            .border_color(rule)
                            .font_family(mono.clone())
                            .text_sm()
                            .child(result),
                    )
                })
                .when(!last, |d| d.child(div().h(px(spacing.boundary))))
        };
        v_flex()
            .id("settings-preview")
            .role(Role::Figure)
            .aria_label("Preview of notebook cells with the current appearance and spacing")
            .gap_2()
            .child(
                h_flex()
                    .gap_2()
                    .items_baseline()
                    .child(
                        div()
                            .text_sm()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child("Preview"),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(muted)
                            .child("Sample cells. Nothing here runs."),
                    ),
            )
            .child(
                h_flex()
                    .px_3()
                    .py_1()
                    .gap_2()
                    .items_center()
                    .rounded(theme.radius)
                    .bg(theme.background)
                    .border_1()
                    .border_color(rule)
                    .child(
                        div()
                            .text_sm()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child("KernelLedger"),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(muted)
                            .child("Quiet ledger · cool paper · precision cobalt"),
                    ),
            )
            .child(
                card(cx)
                    .py_2()
                    .bg(theme.background)
                    .child(cell(
                        "[1]",
                        vec!["quarters = [412_000, 389_500, 483_000]"],
                        None,
                        false,
                    ))
                    .child(cell(
                        "[2]",
                        vec!["total = sum(quarters)", "f\"{total:,}\""],
                        Some("'1,284,500'"),
                        true,
                    )),
            )
    }
}

/// Section key, title and one-line description, in navigation order.
pub(super) const SECTIONS: [(&str, &str, &str); 5] = [
    (
        "appearance",
        "Appearance",
        "Color theme and contrast for every surface.",
    ),
    (
        "editor",
        "Editor",
        "Interface size and the font size used inside notebook cells.",
    ),
    (
        "layout",
        "Layout & accessibility",
        "Cell spacing, outline width, and motion.",
    ),
    (
        "runtime",
        "Runtime",
        "The project interpreter and optional result shortcuts.",
    ),
    (
        "assistant",
        "Assistant",
        "Provider, model, budgets, and what may be sent.",
    ),
];
