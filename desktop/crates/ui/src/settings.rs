use super::*;

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
        self.preference_status = Some("Saving preferences…".into());
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

    pub(super) fn settings_page(&mut self, value: &Value, cx: &mut Context<Self>) -> AnyElement {
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
            appearance = appearance.child(
                action_button(SharedString::from(format!("appearance-{key}")))
                    .small()
                    .label(label)
                    .segment(selected)
                    .on_click(cx.listener(move |v, _, window, cx| {
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
                    })),
            );
        }
        let mut scales = segmented_track(cx);
        for scale in [1., 1.25, 1.5, 2.] {
            scales = scales.child(
                action_button(SharedString::from(format!("ui-scale-{scale}")))
                    .small()
                    .label(format!("{}%", (scale * 100.) as u32))
                    .accessibility_label(format!(
                        "Interface size {} percent",
                        (scale * 100.) as u32
                    ))
                    .segment(self.preferences.ui_scale == scale)
                    .on_click(cx.listener(move |v, _, window, cx| {
                        v.preferences.ui_scale = scale;
                        v.save_preferences(cx);
                        window.refresh();
                        cx.notify();
                    })),
            );
        }
        let mut sizes = segmented_track(cx);
        for size in [12., 14., 16., 18., 20., 24.] {
            sizes = sizes.child(
                action_button(SharedString::from(format!("editor-size-{size}")))
                    .ghost()
                    .small()
                    .label(format!("{size} px"))
                    .segment(self.preferences.editor_size == size)
                    .on_click(cx.listener(move |v, _, window, cx| {
                        v.preferences.editor_size = size;
                        v.save_preferences(cx);
                        for tab in &mut v.tabs {
                            tab.list
                                .splice(0..tab.model.order.len(), tab.model.order.len());
                        }
                        window.refresh();
                        cx.notify();
                    })),
            );
        }
        let mut widths = segmented_track(cx);
        for width in [180., 220., 260., 300.] {
            widths = widths.child(
                action_button(SharedString::from(format!("outline-width-{width}")))
                    .ghost()
                    .small()
                    .label(format!("{width} px"))
                    .segment(self.preferences.outline_width == width)
                    .on_click(cx.listener(move |v, _, _, cx| {
                        v.preferences.outline_width = width;
                        v.save_preferences(cx);
                        cx.notify();
                    })),
            );
        }
        let mut density = segmented_track(cx);
        for (comfortable, label) in [(false, "Compact"), (true, "Comfortable")] {
            density = density.child(
                action_button(SharedString::from(format!("density-{comfortable}")))
                    .small()
                    .label(label)
                    .segment(self.preferences.comfortable == comfortable)
                    .on_click(cx.listener(move |v, _, _, cx| {
                        v.preferences.comfortable = comfortable;
                        v.save_preferences(cx);
                        cx.notify();
                    })),
            );
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
        let mut runtime = settings_group("Project interpreter", cx)
            .child(div().text_sm().child(version))
            .child(
                action_button("interpreter")
                    .small()
                    .label("Choose project interpreter...")
                    .on_click(cx.listener(|v, _, window, cx| v.choose_interpreter(window, cx))),
            );
        if let Some(path) = path {
            runtime = runtime
                .child(
                    action_button("interpreter-details")
                        .ghost()
                        .small()
                        .label(if details { "Hide path" } else { "Show path" })
                        .on_click(cx.listener(|v, _, _, cx| {
                            let flag = v.tabs[v.active]
                                .raw_details
                                .entry("interpreter-path".into())
                                .or_default();
                            *flag = !*flag;
                            cx.notify();
                        })),
                )
                .when(details, |d| {
                    d.child(div().text_sm().whitespace_normal().child(path.clone()))
                        .child(
                            action_button("copy-interpreter-path")
                                .ghost()
                                .small()
                                .label("Copy interpreter path")
                                .on_click(move |_, _, cx| copy_acknowledged(path.clone(), cx)),
                        )
                });
        } else {
            runtime = runtime.child("No project interpreter selected.");
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
            .gap_5()
            .child(
                div()
                    .text_lg()
                    .font_weight(FontWeight::SEMIBOLD)
                    .child("Settings"),
            )
            .child(settings_group("Appearance", cx).child(appearance))
            .child(
                settings_group("Typography", cx)
                    .child("Interface size")
                    .child(scales)
                    .child("Editor text size")
                    .child(sizes),
            )
            .child(
                settings_group("Layout", cx)
                    .child("Spacing")
                    .child(density)
                    .child("Outline width")
                    .child(widths),
            )
            .child(
                settings_group("Accessibility", cx).child(
                    Checkbox::new("reduced-motion")
                        .label("Reduce motion")
                        .checked(self.preferences.reduced_motion)
                        .on_click(cx.listener(|v, value: &bool, _, cx| {
                            v.preferences.reduced_motion = *value;
                            v.save_preferences(cx);
                            cx.notify();
                        })),
                ),
            )
            .child(runtime)
            .when_some(self.preference_status.clone(), |d, message| {
                d.child(
                    div()
                        .text_sm()
                        .text_color(cx.theme().muted_foreground)
                        .child(message),
                )
            })
            .when(!value.is_null(), |d| {
                d.child(self.record_details(value, cx))
            });
        self.record_page(body, cx)
    }
}
