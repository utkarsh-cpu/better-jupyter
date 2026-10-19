use super::*;

impl Workspace {
    pub(super) fn palette_overlay(&self, cx: &mut Context<Self>) -> Stateful<Div> {
        let theme = cx.theme();
        let (popover, popover_fg, muted_fg, overlay, radius) = (
            theme.popover,
            theme.popover_foreground,
            theme.muted_foreground,
            theme.overlay,
            theme.radius_lg,
        );
        let sep = divider(cx);
        let query = self.palette_query.read(cx).value().to_lowercase();
        let commands = COMMANDS
            .into_iter()
            .filter(|(_, label, description, _)| {
                label.to_lowercase().contains(&query) || description.to_lowercase().contains(&query)
            })
            .collect::<Vec<_>>();
        let items = if commands.is_empty() {
            div()
                .px_3()
                .py_6()
                .text_center()
                .text_sm()
                .text_color(muted_fg)
                .child("No matching commands")
                .into_any_element()
        } else {
            let view = cx.entity();
            let height = (commands.len() as f32 * 3.25 + 0.75).min(16.);
            // Lay out only the viewport's rows; hidden commands need no text or icon layout.
            uniform_list("command-results", commands.len(), move |range, _, cx| {
                view.update(cx, |view, cx| {
                    range
                        .map(|index| view.palette_item(index, commands[index], cx))
                        .collect::<Vec<_>>()
                })
            })
            .track_scroll(&self.palette_scroll)
            .h(rems(height))
            .flex_none()
            .p_1p5()
            .into_any_element()
        };
        let modal = v_flex()
            .id("palette")
            .role(Role::Dialog)
            .aria_label("Commands")
            .occlude()
            .w(rems(36.))
            .max_w_full()
            .h_auto()
            .flex_none()
            .self_start()
            .max_h(rems(26.))
            .rounded(radius)
            .border_1()
            .border_color(sep)
            .bg(popover)
            .text_color(popover_fg)
            .when(!self.high_contrast, |d| d.shadow_xl())
            .overflow_hidden()
            .on_click(|_, _, cx| cx.stop_propagation())
            .child(
                div().p_2p5().border_b_1().border_color(sep).child(
                    Input::new(&self.palette_query)
                        .prefix(Icon::new(Glyph::Search).small().text_color(muted_fg))
                        .appearance(false),
                ),
            )
            .child(items)
            .child(
                h_flex()
                    .px_3()
                    .py_2()
                    .gap_1p5()
                    .items_center()
                    .border_t_1()
                    .border_color(sep)
                    .bg(cx.theme().muted.opacity(0.45))
                    .text_xs()
                    .text_color(muted_fg)
                    .children(kbd("escape"))
                    .child("to close"),
            );
        div()
            .id("palette-backdrop")
            .occlude()
            .absolute()
            .top_0()
            .left_0()
            .size_full()
            .flex()
            .justify_center()
            .pt(rems(2.5))
            .px_3()
            .bg(overlay)
            .on_click(cx.listener(|v, _, window, cx| {
                v.palette = false;
                v.focus.focus(window, cx);
                cx.notify();
            }))
            .child(if self.preferences.reduced_motion {
                modal.into_any_element()
            } else {
                modal
                    .with_animation(
                        "palette-enter",
                        Animation::new(Duration::from_millis(110)),
                        |surface, progress| surface.opacity(0.45 + 0.55 * progress),
                    )
                    .into_any_element()
            })
    }
    fn palette_item(
        &self,
        index: usize,
        (method, label, description, glyph): (&'static str, &'static str, &'static str, Glyph),
        cx: &mut Context<Self>,
    ) -> Div {
        let theme = cx.theme();
        let (muted_fg, active_bg, hover_bg, primary) = (
            theme.muted_foreground,
            theme.list_active,
            theme.list_hover,
            theme.primary,
        );
        let is_active = index == self.palette_active;
        let reason = self.command_reason(method);
        let available = reason.is_none();
        let description = reason.unwrap_or(description);
        div().h(rems(3.25)).pb_0p5().child(
            h_flex()
                .id(SharedString::from(format!("cmd-{method}")))
                .role(Role::Button)
                .aria_selected(is_active)
                .a11y_synthetic_children(move |tree| {
                    if !available {
                        tree.parent_node().set_disabled();
                    }
                })
                .aria_label(format!(
                    "{label}{}{}",
                    if is_active { ", selected" } else { "" },
                    if available { "" } else { ", unavailable" }
                ))
                .h_full()
                .gap_3()
                .px_3()
                .py_2()
                .items_center()
                .rounded(px(7.))
                .cursor_pointer()
                .when(!available, |d| d.opacity(0.6))
                .when(is_active, |d| d.bg(active_bg))
                .when(!is_active, |d| d.hover(move |s| s.bg(hover_bg)))
                .child(
                    div()
                        .text_color(if is_active { primary } else { muted_fg })
                        .child(Icon::new(glyph).small()),
                )
                .child(
                    v_flex()
                        .flex_1()
                        .min_w_0()
                        .child(div().text_sm().font_weight(FontWeight::MEDIUM).child(label))
                        .child(div().text_xs().text_color(muted_fg).child(description)),
                )
                .children(command_shortcut(method).and_then(kbd))
                .on_click(cx.listener(move |v, _, window, cx| v.run_command(method, window, cx))),
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
        let (popover, popover_fg, muted, muted_fg, overlay, warning, radius) = (
            theme.popover,
            theme.popover_foreground,
            theme.muted,
            theme.muted_foreground,
            theme.overlay,
            theme.warning,
            theme.radius_lg,
        );
        let sep = divider(cx);
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
                .on_click(cx.listener(|v, _, _, cx| {
                    v.confirmation = None;
                    cx.notify();
                }))
                .child(
                    v_flex()
                        .id("about-dialog")
                        .role(Role::Dialog)
                        .aria_label("About KernelLedger")
                        .track_focus(&self.dialog_focus)
                        .occlude()
                        .on_click(|_, _, cx| cx.stop_propagation())
                        .w(px(480.))
                        .max_w_full()
                        .min_w_0()
                        .whitespace_normal()
                        .p_5()
                        .gap_4()
                        .bg(popover)
                        .text_color(popover_fg)
                        .border_1()
                        .border_color(sep)
                        .rounded(radius)
                        .when(!self.high_contrast, |d| d.shadow_xl())
                        .child(
                            v_flex()
                                .gap_1()
                                .child(
                                    div()
                                        .text_lg()
                                        .font_weight(FontWeight::SEMIBOLD)
                                        .child("About KernelLedger"),
                                )
                                .child(div().text_sm().text_color(muted_fg).child(
                                    "Local-first reproducible notebook and execution ledger",
                                )),
                        )
                        .child(
                            div()
                                .p_3()
                                .rounded(px(7.))
                                .bg(muted)
                                .border_1()
                                .border_color(sep)
                                .font_family(mono)
                                .text_xs()
                                .whitespace_normal()
                                .child(self.support_info()),
                        )
                        .child(
                            h_flex()
                                .gap_2()
                                .justify_end()
                                .items_center()
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
                    .gap_1p5()
                    .p_3()
                    .rounded(px(7.))
                    .bg(muted.opacity(0.65))
                    .border_1()
                    .border_color(sep)
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
        if method == "bundle_import" {
            content = content.child(
                Checkbox::new("bundle-trust")
                    .label("Trust this bundle's recorded history")
                    .checked(fields["trust"] == true)
                    .on_click(cx.listener(|v, checked: &bool, _, cx| {
                        if let Some((_, fields)) = &mut v.confirmation {
                            fields["trust"] = json!(*checked);
                        }
                        cx.notify();
                    })),
            );
        }
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
            content = content.child(
                section("Affected notebook", cx)
                    .child(div().text_sm().whitespace_normal().child(notebook))
                    .child(div().text_sm().whitespace_normal().child(if method == "restore" {
                        "The live namespace is replaced in a new session. Excluded values remain absent."
                    } else {
                        "A new session starts from saved values and runs later recorded code; side effects can occur."
                    }))
                    .child(
                        div()
                            .text_xs()
                            .text_color(muted_fg)
                            .whitespace_normal()
                            .child("Source and environment compatibility have not been established. Review the checkpoint coverage before proceeding."),
                    ),
            );
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
                "restore" | "continue" | "replay" | "training-restore" | "bundle_import"
            ) && fields["trust"] != true,
        );
        let proceed = if destructive {
            proceed.danger()
        } else {
            proceed.primary()
        };
        let actions = h_flex()
            .pt_2()
            .gap_2()
            .justify_end()
            .child(
                div()
                    .id("safe-cancel")
                    .role(Role::Button)
                    .aria_label("Cancel")
                    .track_focus(&self.confirm_focus)
                    .child(
                        action_button("cancel")
                            .semantics_in_parent()
                            .ghost()
                            .label("Cancel")
                            .on_click(cx.listener(|v, _, _, cx| {
                                v.confirmation = None;
                                cx.notify();
                            })),
                    ),
            )
            .child(proceed.on_click(cx.listener(move |v, _, _, cx| {
                if matches!(method.as_str(), "delete" | "merge") {
                    v.mutate(json!({
                        "action":method,
                        "cell_id":fields["cell_id"],
                        "other_id":fields["other_id"]
                    }));
                } else if method == "bundle_import" {
                    v.runtime("bundle_import", json!({"path": fields["path"]}));
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
        let dialog = div()
            .id("confirm")
            .role(Role::Dialog)
            .aria_label(title)
            .track_focus(&self.dialog_focus)
            .occlude()
            .on_click(|_, _, cx| cx.stop_propagation())
            .w(px(480.))
            .max_w_full()
            .min_w_0()
            .whitespace_normal()
            .p_5()
            .rounded(radius)
            .border_1()
            .border_color(sep)
            .bg(popover)
            .text_color(popover_fg)
            .when(!self.high_contrast, |d| d.shadow_xl())
            .child(content);
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
            .child(if self.preferences.reduced_motion {
                dialog.into_any_element()
            } else {
                dialog
                    .with_animation(
                        "confirm-enter",
                        Animation::new(Duration::from_millis(110)),
                        |surface, progress| surface.opacity(0.45 + 0.55 * progress),
                    )
                    .into_any_element()
            })
    }
}
