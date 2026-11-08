use super::*;

impl Workspace {
    #[inline(never)]
    pub(super) fn banners(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let theme = cx.theme();
        let (danger, warning, info, success, muted_fg) = (
            theme.danger,
            theme.warning,
            theme.info,
            theme.success,
            theme.muted_foreground,
        );
        let mut banners = v_flex().flex_none();
        if let Some(note) = self.tabs[self.active]
            .inspections
            .get("Display updates")
            .and_then(|value| value["note"].as_str())
            .filter(|note| !note.is_empty())
        {
            banners = banners.child(
                banner(warning, Glyph::Info)
                    .id("display-update-status")
                    .role(Role::Group)
                    .aria_label(note.to_owned())
                    .child(div().flex_1().text_sm().child(note.to_owned()))
                    .child(
                        action_button("refresh-display-updates")
                            .outline()
                            .small()
                            .label("Reload outputs")
                            .on_click(cx.listener(|view, _, _, _| view.send(Command::Refresh))),
                    ),
            );
        }
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
                        format!("Session {}", short_session(&session)),
                        muted_fg,
                    ))
                    .child(div().flex_1())
                    .child(
                        action_button("restoration-return")
                            .outline()
                            .small()
                            .label("Notebook")
                            .accessibility_label("Return to notebook")
                            .on_click(cx.listener(|v, _, window, cx| {
                                v.select_surface("Notebook", cx);
                                v.focus.focus(window, cx);
                            })),
                    )
                    .child(
                        action_button("restoration-copy-session")
                            .ghost()
                            .small()
                            .label("Copy session ID")
                            .accessibility_label("Copy session identifier")
                            .on_click(move |_, _, cx| copy_acknowledged(session.clone(), cx)),
                    )
                    .child(
                        action_button("dismiss-restoration")
                            .ghost()
                            .small()
                            .label("Dismiss")
                            .accessibility_label("Dismiss restored session notice")
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
                            .child("Directory is not an initialized notebook."),
                    )
                    .child(
                        action_button("initialize")
                            .small()
                            .outline()
                            .label("Initialize")
                            .accessibility_label("Initialize as notebook")
                            .on_click(cx.listener(|v, _, _, _| {
                                v.send(Command::Initialize { sample: false })
                            })),
                    )
                    .child(
                        action_button("choose-another")
                            .small()
                            .ghost()
                            .label("Choose another")
                            .on_click(
                                cx.listener(|v, _, w, cx| v.open_dialog(&OpenNotebook, w, cx)),
                            ),
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
                            .child(format!("Recovered unsaved draft for {id}")),
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
                    "Not recorded in history"
                } else {
                    "Your reply is recorded in history and included in bundles"
                };
            banners = banners.child(
                banner(info, Glyph::Terminal)
                    .child(
                        h_flex()
                            .flex_none()
                            .max_w(px(320.))
                            .gap_1p5()
                            .items_center()
                            .child(
                                div()
                                    .text_sm()
                                    .font_weight(FontWeight::MEDIUM)
                                    .truncate()
                                    .child(input["prompt"].as_str().unwrap_or("Input").to_string()),
                            )
                            .child(info_tip(
                                SharedString::from(format!("input-tip-{iid}")),
                                notice,
                                cx,
                            )),
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
        banners.into_any_element()
    }
    #[inline(never)]
    pub(super) fn status_bar(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = cx.theme();
        let (bar, muted_fg, success, info) = (
            theme.status_bar,
            theme.muted_foreground,
            theme.success,
            theme.info,
        );
        let tab = &self.tabs[self.active];
        let state = tab.runtime["worker_state"].as_str();
        let state_color = state.map(|s| tone(s, cx)).unwrap_or(muted_fg);
        let edit = tab.model.mode == Mode::Edit;
        let ai_provider = tab.assistant.receipt["provider"]
            .as_str()
            .or_else(|| tab.assistant.settings["settings"]["provider"].as_str())
            .unwrap_or("");
        let ai_dest_label = tab.assistant.receipt["destination"]["label"]
            .as_str()
            .or_else(|| tab.assistant.settings["destination"]["label"].as_str())
            .unwrap_or("Local");
        let ai_dest_local = tab.assistant.receipt["destination"]["local"]
            .as_bool()
            .or_else(|| tab.assistant.settings["destination"]["local"].as_bool())
            .unwrap_or(true);
        let view = cx.entity();

        h_flex()
            .flex_none()
            .min_h(rems(1.75))
            .flex_wrap()
            .px_3()
            .gap_2()
            .items_center()
            .bg(bar)
            .border_t_1()
            .border_color(divider(cx))
            .text_xs()
            .text_color(muted_fg)
            .child(
                div()
                    .id("status-wordmark")
                    .role(Role::Label)
                    .aria_label("KernelLedger")
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(theme.foreground)
                    .child("KernelLedger"),
            )
            .child(status_rule(cx))
            .when(self.surface == "Notebook", |d| {
                d.child(if edit {
                    "Editing"
                } else {
                    "Cell navigation"
                })
                .child(status_rule(cx))
            })
            .child(div().id("cell-count").role(Role::Label).aria_label(format!("{} cells", tab.model.order.len())).child(format!(
                "{} cell{}",
                tab.model.order.len(),
                if tab.model.order.len() == 1 { "" } else { "s" }
            )))
            .child(status_rule(cx))
            .child(
                h_flex()
                    .id("runtime-status")
                    .role(Role::Status)
                    .aria_label(if tab.runtime["connection_lost"] == true { "Runtime connection lost; execution state unknown".to_owned() } else { format!("Runtime: {}", worker_label(state)) })
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
            .child(status_rule(cx))
            .child({
                let save = if tab.model.cells.values().any(|c| matches!(c.save, SaveState::Failed | SaveState::Conflict)) {
                    "Edits retained \u{2014} save needs attention"
                } else if tab.model.cells.values().any(|c| c.save != SaveState::Saved) { "Saving\u{2026}" } else { "Saved" };
                div().id("save-status").role(Role::Label).aria_label(save).child(save)
            })
            .child(div().flex_1())
            .when(tab.assistant.busy, |d| {
                let provider = if ai_provider.is_empty() {
                    "AI"
                } else {
                    ai_provider
                };
                d.child(status_rule(cx)).child(
                    h_flex()
                        .gap_1p5()
                        .items_center()
                        .child(dot(if ai_dest_local { success } else { info }))
                        .child(format!("{provider} \u{00b7} {ai_dest_label} \u{00b7} Working\u{2026}")),
                )
            })
            .child(
                action_button("status-assistant")
                    .ghost()
                    .small()
                    .icon(Glyph::Sparkles)
                    .label("Assistant")
                    .selected(tab.assistant.open)
                    .on_a11y_click(move |_, cx| {
                        view.update(cx, |v, cx| v.open_assistant(cx));
                    })
                    .on_click(cx.listener(|v, _, _, cx| v.open_assistant(cx))),
            )
            .when(tab.assistant.busy, |d| {
                d.child(
                    action_button("status-ai-cancel")
                        .ghost()
                        .small()
                        .label("Cancel AI")
                        .on_click(cx.listener(|v, _, _, _| v.send(Command::AssistantCancel))),
                )
            })
            .into_any_element()
    }
}
