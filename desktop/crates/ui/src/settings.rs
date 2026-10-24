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
        let settings_scroll = self.tabs[self.active]
            .scrolls
            .entry("Settings".into())
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
                .label("Choose project interpreter\u{2026}"),
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
                        .label("Copy interpreter path"),
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
        let body = v_flex()
            .max_w(rems(52.))
            .gap_4()
            .child(
                v_flex()
                    .gap_0p5()
                    .child(
                        div()
                            .text_lg()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child("Settings"),
                    )
                    .child(
                        div()
                            .text_sm()
                            .text_color(cx.theme().muted_foreground)
                            .child("Appearance, typography, layout, and project interpreter"),
                    ),
            )
            .child(
                appearance_reveal
                    .track(settings_group("Appearance", cx).id("settings-appearance"))
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(
                                "Match your operating system or choose a dedicated contrast mode.",
                            ),
                    )
                    .child(appearance),
            )
            .child(
                type_reveal
                    .track(settings_group("Typography", cx).id("settings-typography"))
                    .child(setting_row(
                        "Interface size",
                        "Scale navigation, controls, and reading surfaces.",
                        scales,
                        cx,
                    ))
                    .child(setting_row(
                        "Editor text size",
                        "Monospace font size inside notebook cells.",
                        sizes,
                        cx,
                    )),
            )
            .child(
                layout_reveal
                    .track(settings_group("Layout", cx).id("settings-layout"))
                    .child(setting_row(
                        "Spacing",
                        "Comfortable mode enlarges pointer targets to at least 44 by 44 px.",
                        density,
                        cx,
                    ))
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
            )
            .child(runtime_reveal.track(runtime))
            .child(self.ai_settings(window, cx))
            .when_some(self.preference_status.clone(), |d, message| {
                d.child(
                    div()
                        .text_sm()
                        .text_color(cx.theme().muted_foreground)
                        .child(message),
                )
            })
            .when(!value.is_null(), |d| {
                d.child(self.record_details_in(value, Some(details_reveal), cx))
            });
        self.record_page(body, cx)
    }
}
