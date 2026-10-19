use super::*;

impl Workspace {
    pub(super) fn banners(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Div {
        let theme = cx.theme();
        let (danger, warning, info, success, muted_fg) = (
            theme.danger,
            theme.warning,
            theme.info,
            theme.success,
            theme.muted_foreground,
        );
        let mut banners = v_flex().flex_none();
        if let Some(id) = self.tabs[self.active].checkpoint_notice.clone() {
            banners = banners.child(
                banner(success, Glyph::Flag)
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_sm()
                            .font_weight(FontWeight::MEDIUM)
                            .child("Checkpoint saved"),
                    )
                    .child(
                        action_button("view-saved-checkpoint")
                            .outline()
                            .small()
                            .label("View checkpoint")
                            .on_click(cx.listener(move |v, _, _, cx| {
                                v.select_surface("Checkpoints", cx);
                                v.inspect("checkpoint.get", json!({"checkpoint_id":id}));
                                v.tabs[v.active].checkpoint_notice = None;
                            })),
                    )
                    .child(
                        action_button("dismiss-checkpoint-notice")
                            .ghost()
                            .small()
                            .label("Dismiss")
                            .on_click(cx.listener(|v, _, _, cx| {
                                v.tabs[v.active].checkpoint_notice = None;
                                cx.notify();
                            })),
                    ),
            );
        }
        if let Some(result) = self.tabs[self.active]
            .inspections
            .get("Restoration result")
            .cloned()
        {
            let session = result["result"]["session_id"]
                .as_str()
                .unwrap_or_default()
                .to_string();
            let message = match result["kind"].as_str().or(result["method"].as_str()) {
                Some("replay") => "Replay completed in a new session",
                Some("continue") => "Continue completed in a new session",
                _ => "Restored in a new session",
            };
            banners = banners.child(
                banner(info, Glyph::RotateCcw)
                    .id("restoration-notice")
                    .role(Role::Group)
                    .aria_label(format!("{message}: {session}"))
                    .flex_wrap()
                    .child(
                        div()
                            .text_sm()
                            .font_weight(FontWeight::MEDIUM)
                            .child(message),
                    )
                    .child(badge(
                        format!("Session {}", session.chars().take(8).collect::<String>()),
                        muted_fg,
                    ))
                    .child(div().flex_1())
                    .child(
                        action_button("restoration-return")
                            .outline()
                            .small()
                            .label("Return to notebook")
                            .on_click(cx.listener(|v, _, window, cx| {
                                v.select_surface("Notebook", cx);
                                v.focus.focus(window, cx);
                            })),
                    )
                    .child(
                        action_button("restoration-copy-session")
                            .ghost()
                            .small()
                            .label("Copy session identifier")
                            .on_click(move |_, _, cx| copy_acknowledged(session.clone(), cx)),
                    )
                    .child(
                        action_button("dismiss-restoration")
                            .ghost()
                            .small()
                            .label("Dismiss restored session notice")
                            .on_click(cx.listener(|v, _, _, cx| {
                                v.tabs[v.active].inspections.remove("Restoration result");
                                cx.notify();
                            })),
                    ),
            );
        }
        if let Some(error) = self.tabs[self.active].error.clone() {
            banners = banners.child(
                banner(danger, Glyph::CircleAlert)
                    .child(div().flex_1().min_w_0().text_sm().child(error))
                    .child(
                        action_button("diagnostics")
                            .small()
                            .outline()
                            .label("Diagnostics")
                            .on_click(
                                cx.listener(|v, _, _, cx| v.select_surface("Diagnostics", cx)),
                            ),
                    )
                    .child(
                        action_button("dismiss-error")
                            .small()
                            .ghost()
                            .icon(Glyph::X)
                            .hint("Dismiss")
                            .accessibility_label("Dismiss")
                            .on_click(cx.listener(|v, _, _, cx| {
                                v.tabs[v.active].error = None;
                                cx.notify();
                            })),
                    ),
            );
        }
        let source_state = self.tabs[self.active].model.source_state.clone();
        if self.tabs[self.active].uninitialized {
            banners = banners.child(
                banner(warning, Glyph::Info)
                    .flex_wrap()
                    .child(
                        div()
                            .min_w_0()
                            .flex_1()
                            .text_sm()
                            .whitespace_normal()
                            .child("This directory is not an initialized notebook. Initialize it to add cells, or choose another directory."),
                    )
                    .child(
                        action_button("initialize")
                            .small()
                            .outline()
                            .label("Initialize as notebook")
                            .on_click(cx.listener(|v, _, _, _| {
                                v.send(Command::Initialize { sample: false })
                            })),
                    )
                    .child(
                        action_button("choose-another")
                            .small()
                            .ghost()
                            .label("Choose another")
                            .on_click(cx.listener(|v, _, w, cx| {
                                v.open_dialog(&OpenNotebook, w, cx)
                            })),
                    ),
            );
        } else if source_state != "ok" {
            banners = banners.child(banner(warning, Glyph::TriangleAlert).child(
                div().text_sm().child(format!(
                    "Source state: {source_state} \u{2014} history remains available"
                )),
            ));
        }
        let drafts = self.tabs[self.active].drafts.clone();
        for (index, draft) in drafts.iter().enumerate() {
            let id = draft["cell_id"].as_str().unwrap_or("unknown").to_owned();
            banners = banners.child(
                banner(warning, Glyph::FileText)
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_sm()
                            .child(format!("Recovered an unsaved draft for {id}")),
                    )
                    .child(
                        action_button(("restore-draft", index))
                            .small()
                            .primary()
                            .label("Restore")
                            .on_click(cx.listener({
                                let draft = draft.clone();
                                move |v, _, _, cx| {
                                    v.send(Command::RestoreDraft(draft.clone()));
                                    cx.notify();
                                }
                            })),
                    )
                    .child(
                        action_button(("compare-draft", index))
                            .small()
                            .outline()
                            .label("Compare")
                            .on_click(cx.listener({
                                let draft = draft.clone();
                                move |v, _, _, cx| {
                                    v.tabs[v.active]
                                        .inspections
                                        .insert("Recovered draft".into(), draft.clone());
                                    v.select_surface("Diagnostics", cx);
                                    cx.notify();
                                }
                            })),
                    )
                    .child(
                        action_button(("discard-draft", index))
                            .small()
                            .ghost()
                            .label("Discard")
                            .on_click(cx.listener(move |v, _, _, cx| {
                                v.send(Command::DiscardDraft(id.clone()));
                                v.tabs[v.active].drafts.retain(|d| d["cell_id"] != id);
                                cx.notify();
                            })),
                    ),
            );
        }
        let inputs = self.tabs[self.active].runtime["pending_inputs"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        for input in inputs {
            let iid = input["input_id"].as_str().unwrap_or_default().to_string();
            let password = input["password"] == true;
            let state = self
                .prompts
                .entry(iid.clone())
                .or_insert_with(|| cx.new(|cx| InputState::new(window, cx).masked(password)))
                .clone();
            let notice =
                if password && self.tabs[self.active].runtime["record_secret_input"] != true {
                    "Not recorded"
                } else {
                    "Your reply is recorded in history and included in bundles"
                };
            banners = banners.child(
                banner(info, Glyph::Terminal)
                    .child(
                        v_flex()
                            .flex_none()
                            .max_w(px(320.))
                            .child(
                                div()
                                    .text_sm()
                                    .font_weight(FontWeight::MEDIUM)
                                    .truncate()
                                    .child(input["prompt"].as_str().unwrap_or("Input").to_string()),
                            )
                            .child(div().text_xs().text_color(muted_fg).child(notice)),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(160.))
                            .child(Input::new(&state).small()),
                    )
                    .child(
                        action_button(SharedString::from(format!("claim-{iid}")))
                            .small()
                            .ghost()
                            .label("Claim")
                            .on_click(cx.listener({
                                let iid = iid.clone();
                                move |v, _, _, _| v.runtime("input.claim", json!({"input_id":iid}))
                            })),
                    )
                    .child(
                        action_button(SharedString::from(format!("reply-{iid}")))
                            .small()
                            .primary()
                            .label("Reply")
                            .on_click(cx.listener(move |v, _, window, cx| {
                                let text = state.read(cx).value().to_string();
                                v.runtime("input.reply", json!({"input_id":iid,"text":text}));
                                state.update(cx, |s, cx| s.set_value("", window, cx));
                            })),
                    ),
            );
        }
        banners
    }
    pub(super) fn status_bar(&self, cx: &mut Context<Self>) -> Div {
        let theme = cx.theme();
        let (bar, muted_fg, primary) = (theme.status_bar, theme.muted_foreground, theme.primary);
        let tab = &self.tabs[self.active];
        let state = tab.runtime["worker_state"].as_str();
        let state_color = state.map(|s| tone(s, cx)).unwrap_or(muted_fg);
        let edit = tab.model.mode == Mode::Edit;
        h_flex()
            .flex_none()
            .min_h(rems(1.625))
            .flex_wrap()
            .px_3()
            .gap_4()
            .items_center()
            .bg(bar)
            .border_t_1()
            .border_color(divider(cx))
            .text_xs()
            .text_color(muted_fg)
            .when(self.surface == "Notebook", |d| {
                d.child(
                    div()
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(if edit { primary } else { muted_fg })
                        .child(if edit {
                            "Editing"
                        } else {
                            "Cell navigation"
                        }),
                )
            })
            .child(format!(
                "{} cell{}",
                tab.model.order.len(),
                if tab.model.order.len() == 1 { "" } else { "s" }
            ))
            .child(
                h_flex()
                    .id("runtime-status")
                    .tooltip(|_, cx| {
                        cx.new(|_| {
                            WorkbenchHint(
                                "Closing detaches; computations continue. Use Stop runtime to end them."
                                    .into(),
                            )
                        })
                        .into()
                    })
                    .gap_1p5()
                    .items_center()
                    .child(dot(if tab.runtime["connection_lost"] == true {
                        theme.danger
                    } else {
                        state_color
                    }))
                    .child(if tab.runtime["connection_lost"] == true {
                        "Connection lost \u{2014} execution state unknown".into()
                    } else {
                        worker_label(state)
                    }),
            )
            .child(
                if tab
                    .model
                    .cells
                    .values()
                    .any(|c| matches!(c.save, SaveState::Failed | SaveState::Conflict))
                {
                    "Edits retained \u{2014} save needs attention"
                } else if tab.model.cells.values().any(|c| c.save != SaveState::Saved) {
                    "Saving\u{2026}"
                } else {
                    "Saved"
                },
            )
            .child(div().flex_1())
    }
}
