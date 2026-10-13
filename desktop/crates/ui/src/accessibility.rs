use super::*;

impl Workspace {
    pub(super) fn palette_overlay(&self, cx: &mut Context<Self>) -> Stateful<Div> {
        let theme = cx.theme();
        let (border, popover, popover_fg, muted_fg, hover_bg, overlay, radius) = (
            theme.border,
            theme.popover,
            theme.popover_foreground,
            theme.muted_foreground,
            theme.list_hover,
            theme.overlay,
            theme.radius_lg,
        );
        let query = self.palette_query.read(cx).value().to_lowercase();
        let mut items = v_flex()
            .id("command-results")
            .track_scroll(&self.palette_scroll)
            .max_h(rems(12.))
            .overflow_y_scroll()
            .p_1()
            .gap_px();
        let mut shown = 0;
        for (method, label, description, glyph) in COMMANDS {
            if !query.is_empty()
                && !label.to_lowercase().contains(&query)
                && !description.to_lowercase().contains(&query)
            {
                continue;
            }
            shown += 1;
            let reason = self.command_reason(method);
            let available = reason.is_none();
            let description = reason.unwrap_or(description);
            items = items.child(
                h_flex()
                    .id(SharedString::from(format!("cmd-{method}")))
                    .role(Role::Button)
                    .aria_selected(shown - 1 == self.palette_active)
                    .a11y_synthetic_children(move |tree| {
                        if !available {
                            tree.parent_node().set_disabled();
                        }
                    })
                    .aria_label(format!(
                        "{label}{}{}",
                        if shown - 1 == self.palette_active {
                            ", selected"
                        } else {
                            ""
                        },
                        if available { "" } else { ", unavailable" }
                    ))
                    .gap_3()
                    .px_3()
                    .py_2()
                    .items_center()
                    .rounded(px(6.))
                    .cursor_pointer()
                    .when(shown - 1 == self.palette_active, |d| d.bg(hover_bg))
                    .hover(move |s| s.bg(hover_bg))
                    .child(div().text_color(muted_fg).child(Icon::new(glyph)))
                    .child(
                        v_flex()
                            .flex_1()
                            .child(div().text_sm().font_weight(FontWeight::MEDIUM).child(label))
                            .child(div().text_xs().text_color(muted_fg).child(description)),
                    )
                    .on_click(
                        cx.listener(move |v, _, window, cx| v.run_command(method, window, cx)),
                    ),
            );
        }
        if shown == 0 {
            items = items.child(
                div()
                    .px_3()
                    .py_6()
                    .text_center()
                    .text_sm()
                    .text_color(muted_fg)
                    .child("No matching commands"),
            );
        }
        div()
            .id("palette-backdrop")
            .occlude()
            .absolute()
            .top_0()
            .left_0()
            .size_full()
            .flex()
            .justify_center()
            .pt(rems(2.))
            .px_3()
            .bg(overlay)
            .on_click(cx.listener(|v, _, window, cx| {
                v.palette = false;
                v.focus.focus(window, cx);
                cx.notify();
            }))
            .child(
                v_flex()
                    .id("palette")
                    .role(Role::Dialog)
                    .aria_label("Commands")
                    .occlude()
                    .w(rems(35.))
                    .max_w_full()
                    .h_auto()
                    .flex_none()
                    .self_start()
                    .max_h(rems(24.))
                    .rounded(radius)
                    .border_1()
                    .border_color(border)
                    .bg(popover)
                    .text_color(popover_fg)
                    .shadow_xl()
                    .overflow_hidden()
                    .child(
                        div().p_2().border_b_1().border_color(border).child(
                            Input::new(&self.palette_query)
                                .prefix(Icon::new(Glyph::Search).small().text_color(muted_fg))
                                .appearance(false),
                        ),
                    )
                    .child(items)
                    .child(
                        h_flex()
                            .px_3()
                            .py_1p5()
                            .gap_1p5()
                            .items_center()
                            .border_t_1()
                            .border_color(border)
                            .text_xs()
                            .text_color(muted_fg)
                            .children(kbd("escape"))
                            .child("to close"),
                    ),
            )
    }
    pub(super) fn confirm_overlay(
        &self,
        method: String,
        fields: Value,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let theme = cx.theme();
        let (border, popover, popover_fg, muted, muted_fg, overlay, warning, radius) = (
            theme.border,
            theme.popover,
            theme.popover_foreground,
            theme.muted,
            theme.muted_foreground,
            theme.overlay,
            theme.warning,
            theme.radius_lg,
        );
        let mono = theme.mono_font_family.clone();
        let clients = self
            .tabs
            .get(self.active)
            .map(|t| t.runtime["attached_client_count"].clone())
            .unwrap_or(Value::Null);
        if method == "about" {
            return div()
                .id("about-backdrop")
                .occlude()
                .absolute()
                .top_0()
                .left_0()
                .size_full()
                .flex()
                .items_center()
                .justify_center()
                .bg(overlay)
                .child(
                    v_flex()
                        .id("about-dialog")
                        .role(Role::Dialog)
                        .aria_label("About KernelLedger")
                        .track_focus(&self.dialog_focus)
                        .w(px(480.))
                        .max_w_full()
                        .min_w_0()
                        .whitespace_normal()
                        .p_5()
                        .gap_3()
                        .bg(popover)
                        .text_color(popover_fg)
                        .rounded(radius)
                        .child(div().text_lg().child("About KernelLedger"))
                        .child(self.support_info())
                        .child(
                            action_button("about-copy")
                                .outline()
                                .label("Copy support information")
                                .on_click(cx.listener(|v, _, _, cx| {
                                    copy_acknowledged(v.support_info(), cx)
                                })),
                        )
                        .child(
                            div()
                                .id("about-close-focus")
                                .role(Role::Button)
                                .aria_label("Close About")
                                .track_focus(&self.confirm_focus)
                                .child(
                                    action_button("about-close")
                                        .semantics_in_parent()
                                        .primary()
                                        .label("Close")
                                        .on_click(cx.listener(|v, _, _, cx| {
                                            v.confirmation = None;
                                            cx.notify();
                                        })),
                                ),
                        ),
                );
        }
        let (title, consequence, verb) = confirmation_copy(&method);
        let target = self.target_summary(&fields);

        let mut content = v_flex()
            .gap_3()
            .child(
                h_flex()
                    .gap_3()
                    .items_start()
                    .child(
                        div()
                            .flex_none()
                            .size(px(36.))
                            .rounded_full()
                            .flex()
                            .items_center()
                            .justify_center()
                            .bg(warning.opacity(0.12))
                            .text_color(warning)
                            .child(Icon::new(Glyph::TriangleAlert)),
                    )
                    .child(
                        v_flex()
                            .flex_1()
                            .gap_1()
                            .min_w_0()
                            .whitespace_normal()
                            .child(
                                div()
                                    .text_base()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child(format!("{title} {target}?")),
                            )
                            .child(div().text_sm().text_color(muted_fg).child(consequence)),
                    ),
            )
            .child(
                v_flex()
                    .gap_1()
                    .p_3()
                    .rounded(px(6.))
                    .bg(muted.opacity(0.5))
                    .text_xs()
                    .child(
                        h_flex()
                            .gap_2()
                            .child(
                                div()
                                    .w(px(110.))
                                    .text_color(muted_fg)
                                    .child("Attached clients"),
                            )
                            .child(div().font_family(mono.clone()).child(clients.to_string())),
                    )
                    .child(
                        h_flex()
                            .gap_2()
                            .child(div().w(px(110.)).text_color(muted_fg).child("Target"))
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .font_family(mono.clone())
                                    .whitespace_normal()
                                    .child(target),
                            ),
                    ),
            );
        if matches!(
            method.as_str(),
            "restore" | "continue" | "replay" | "training-restore"
        ) {
            content = content.child(
                Checkbox::new("artifact-trust")
                    .label("Allow saved Python code to run")
                    .checked(fields["trust"] == true)
                    .on_click(cx.listener(|v, checked: &bool, _, cx| {
                        if let Some((_, fields)) = &mut v.confirmation {
                            fields["trust"] = json!(*checked);
                        }
                        cx.notify();
                    })),
            );
        }
        if matches!(method.as_str(), "restore" | "continue")
            && let Some(tab) = self.tabs.get(self.active)
        {
            let notebook = tab
                .model
                .project
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();
            content=content.child(section("Affected notebook",cx).child(div().text_sm().whitespace_normal().child(notebook))
                .child(div().text_sm().whitespace_normal().child(if method=="restore"{"The live namespace is replaced in a new session. Excluded values remain absent."}else{"A new session starts from saved values and runs later recorded code; side effects can occur."}))
                .child(div().text_xs().text_color(muted_fg).whitespace_normal().child("Source and environment compatibility have not been established. Review the checkpoint coverage before proceeding.")));
        }
        let known_mismatch = self.tabs.get(self.active).is_some_and(|tab| {
            tab.operations.values().any(|operation| {
                operation["state"] == "failed"
                    && operation["result"]["error"]["code"] == "python_mismatch"
                    && operation["request"]["checkpoint_id"].is_string()
                    && operation["request"]["checkpoint_id"] == fields["checkpoint_id"]
            })
        });
        if matches!(method.as_str(), "restore" | "continue") && known_mismatch {
            content = content
                .child(
                    Checkbox::new("python-mismatch")
                        .label("Override a Python version mismatch")
                        .checked(fields["allow_python_mismatch"] == true)
                        .on_click(cx.listener(|v, checked: &bool, _, cx| {
                            if let Some((_, fields)) = &mut v.confirmation {
                                fields["allow_python_mismatch"] = json!(*checked);
                            }
                            cx.notify();
                        })),
                )
                .child(div().text_xs().text_color(muted_fg).child(
                    "Review the checkpoint report before overriding a Python version mismatch.",
                ));
        }
        let destructive = matches!(
            method.as_str(),
            "restart" | "stop" | "interrupt_kill" | "migrate" | "restore"
        );
        let proceed = action_button("proceed").label(verb).unavailable(
            matches!(
                method.as_str(),
                "restore" | "continue" | "replay" | "training-restore"
            ) && fields["trust"] != true,
        );
        let proceed = if destructive {
            proceed.danger()
        } else {
            proceed.primary()
        };
        let actions =
            h_flex()
                .pt_2()
                .gap_2()
                .justify_end()
                .child(
                    div()
                        .id("safe-cancel")
                        .role(Role::Button)
                        .aria_label("Cancel")
                        .track_focus(&self.confirm_focus)
                        .child(action_button("cancel").semantics_in_parent().ghost().label("Cancel").on_click(
                            cx.listener(|v, _, _, cx| {
                                v.confirmation = None;
                                cx.notify();
                            }),
                        )),
                )
                .child(proceed.on_click(cx.listener(move |v, _, _, cx| {
                    if matches!(method.as_str(), "delete" | "merge") {
                        v.mutate(json!({"action":method,"cell_id":fields["cell_id"],"other_id":fields["other_id"]}));
                    } else {
                        v.runtime(&method, fields.clone());
                    }
                    v.confirmation = None;
                    cx.notify();
                })));
        content = v_flex()
            .gap_3()
            .child(
                div()
                    .id("confirmation-body")
                    .max_h((window.viewport_size().height - px(180.)).max(px(120.)))
                    .overflow_y_scroll()
                    .child(content),
            )
            .child(actions);
        div()
            .id("confirm-backdrop")
            .occlude()
            .absolute()
            .top_0()
            .left_0()
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .bg(overlay)
            .on_click(cx.listener(|v, _, _, cx| {
                v.confirmation = None;
                cx.notify();
            }))
            .child(
                div()
                    .id("confirm")
                    .role(Role::Dialog)
                    .aria_label(title)
                    .track_focus(&self.dialog_focus)
                    .occlude()
                    .w(px(480.))
                    .max_w_full()
                    .min_w_0()
                    .whitespace_normal()
                    .p_5()
                    .rounded(radius)
                    .border_1()
                    .border_color(border)
                    .bg(popover)
                    .text_color(popover_fg)
                    .shadow_xl()
                    .child(content),
            )
    }
}
