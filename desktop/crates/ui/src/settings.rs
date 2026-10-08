use super::*;

impl Workspace {
    pub(super) fn save_preferences(&mut self, cx: &mut Context<Self>) {
        self.preferences.normalize();
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

    pub(super) fn settings_controls(&self, cx: &mut Context<Self>) -> Div {
        let mut controls = section("Reading and layout", cx)
            .child(
                Checkbox::new("comfortable-density")
                    .label("Comfortable spacing")
                    .checked(self.preferences.comfortable)
                    .on_click(cx.listener(|v, value: &bool, _, cx| {
                        v.preferences.comfortable = *value;
                        v.save_preferences(cx);
                        cx.notify();
                    })),
            )
            .child(
                Checkbox::new("reduced-motion")
                    .label("Reduce motion (workspace transitions are already instant)")
                    .checked(self.preferences.reduced_motion)
                    .on_click(cx.listener(|v, value: &bool, _, cx| {
                        v.preferences.reduced_motion = *value;
                        v.save_preferences(cx);
                        cx.notify();
                    })),
            );
        let mut sizes = h_flex().gap_2().items_center().child("Editor text size");
        for size in [12., 14., 16., 18., 20., 24.] {
            sizes = sizes.child(
                Button::new(SharedString::from(format!("editor-size-{size}")))
                    .ghost()
                    .small()
                    .label(format!("{size}"))
                    .selected(self.preferences.editor_size == size)
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
        controls = controls.child(sizes);
        let mut widths = h_flex().gap_2().items_center().child("Outline width");
        for width in [180., 220., 260., 300.] {
            widths = widths.child(
                Button::new(SharedString::from(format!("outline-width-{width}")))
                    .ghost()
                    .small()
                    .label(format!("{width}"))
                    .selected(self.preferences.outline_width == width)
                    .on_click(cx.listener(move |v, _, _, cx| {
                        v.preferences.outline_width = width;
                        v.save_preferences(cx);
                        cx.notify();
                    })),
            );
        }
        controls
            .child(widths)
            .when_some(self.preference_status.clone(), |d, message| {
                d.child(message)
            })
    }
}
