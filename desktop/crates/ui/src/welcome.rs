use super::*;

impl Workspace {
    #[inline(never)]
    pub(super) fn empty_notebook(&self, cx: &mut Context<Self>) -> AnyElement {
        let muted_fg = cx.theme().muted_foreground;
        let tab = &self.tabs[self.active];
        if tab.model.source_state != "ok" {
            let (title, description) = if tab.uninitialized {
                (
                    "Initialize this folder",
                    "Use Initialize above to create a notebook here, or choose another folder.",
                )
            } else if tab.error.is_some() || !tab.model.source_state.is_empty() {
                (
                    "Notebook source is unavailable",
                    "Choose a Python environment with KernelLedger installed, or retry after fixing its setup. Diagnostics contains the detailed error.",
                )
            } else {
                (
                    "Loading notebook source\u{2026}",
                    "Reading saved cells. Opening a notebook does not start code execution.",
                )
            };
            return v_flex()
                .debug_selector(|| "unloaded-notebook".into())
                .size_full()
                .items_center()
                .justify_center()
                .p_6()
                .gap_3()
                .child(
                    div()
                        .text_lg()
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(title),
                )
                .child(
                    div()
                        .max_w(rems(28.))
                        .text_sm()
                        .text_center()
                        .text_color(muted_fg)
                        .child(description),
                )
                .when(tab.error.is_some(), |d| {
                    d.child(
                        h_flex()
                            .gap_2()
                            .flex_wrap()
                            .justify_center()
                            .child(
                                action_button("retry-helper")
                                    .primary()
                                    .label("Retry loading")
                                    .on_click(
                                        cx.listener(|v, _, _, cx| v.retry_application_python(cx)),
                                    ),
                            )
                            .child(
                                action_button("choose-helper-python")
                                    .outline()
                                    .label("Choose application Python\u{2026}")
                                    .on_click(cx.listener(|v, _, window, cx| {
                                        v.choose_application_python(window, cx)
                                    })),
                            ),
                    )
                })
                .into_any_element();
        }
        v_flex()
            .debug_selector(|| "empty-notebook".into())
            .size_full()
            .items_center()
            .justify_center()
            .p_6()
            .child(
                v_flex()
                    .max_w(px(400.))
                    .gap_3()
                    .items_center()
                    .child(icon_tile(Glyph::NotebookPen, 40., cx))
                    .child(
                        v_flex()
                            .items_center()
                            .gap_1()
                            .child(
                                div()
                                    .text_lg()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child("This notebook is empty"),
                            )
                            .child(
                                div().text_sm().text_center().text_color(muted_fg).child(
                                    "Add a cell to start writing. Nothing runs until you ask.",
                                ),
                            ),
                    )
                    .child(
                        h_flex()
                            .gap_2()
                            .flex_wrap()
                            .justify_center()
                            .child(
                                action_button("empty-code")
                                    .primary()
                                    .icon(Glyph::Code)
                                    .label("Add code cell")
                                    .on_click(cx.listener(|v, _, _, _| {
                                        v.mutate(json!({"action":"add","type":"code"}))
                                    })),
                            )
                            .child(
                                action_button("empty-markdown")
                                    .outline()
                                    .icon(Glyph::Type)
                                    .label("Add markdown cell")
                                    .on_click(cx.listener(|v, _, _, _| {
                                        v.mutate(json!({"action":"add","type":"markdown"}))
                                    })),
                            ),
                    ),
            )
            .into_any_element()
    }

    #[inline(never)]
    pub(super) fn welcome(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = cx.theme();
        let muted_fg = theme.muted_foreground;
        let shortcut_hints = [
            ("Open", "ctrl-o"),
            ("Commands", "ctrl-shift-p"),
            ("Run", "shift-enter"),
            ("Save", "ctrl-s"),
        ];
        let warning = theme.warning;
        let recent = self.recent.iter().take(5).enumerate().map(|(i, item)| {
            let path = item.path.clone();
            let name = notebook_name(&path);
            let folder = path
                .parent()
                .map(|p| p.display().to_string())
                .unwrap_or_default();
            let missing = !path.is_dir();
            let opened = item.opened.as_deref().map(relative_time);
            let detail = match (&opened, missing) {
                (_, true) => format!("{} \u{00b7} Not found", tail_path(&folder, 44)),
                (Some(opened), false) => {
                    format!("{} \u{00b7} Opened {opened}", tail_path(&folder, 44))
                }
                (None, false) => tail_path(&folder, 44),
            };
            let spoken = format!(
                "Open {name} in {folder}{}",
                match (&opened, missing) {
                    (_, true) => ", folder not found".to_string(),
                    (Some(opened), false) => format!(", opened {opened}"),
                    (None, false) => String::new(),
                }
            );
            let open_path = path.clone();
            let removed = path.clone();
            h_flex()
                .w_full()
                .min_w_0()
                .gap_1()
                .items_center()
                .child(
                    action_button(("recent", i))
                        .ghost()
                        .flex_1()
                        .min_w_0()
                        .icon(if missing {
                            Glyph::TriangleAlert
                        } else {
                            Glyph::NotebookPen
                        })
                        .when(missing, |b| b.text_color(warning))
                        .detail_row(name.clone(), detail)
                        .accessibility_label(spoken)
                        .hint(path.display().to_string())
                        .on_click(cx.listener(move |v, _, _, cx| {
                            if open_path.is_dir() {
                                v.welcome_notice = None;
                                v.open(open_path.clone(), cx);
                            } else {
                                v.welcome_notice = Some(missing_recent_message(&open_path));
                                cx.notify();
                            }
                        })),
                )
                .when(missing, |row| {
                    row.child(
                        action_button(("recent-remove", i))
                            .ghost()
                            .small()
                            .label("Remove")
                            .accessibility_label(format!("Remove {name} from recent notebooks"))
                            .on_click(cx.listener(move |v, _, _, cx| {
                                kernelledger_client::recent::remove(&mut v.recent, &removed);
                                v.welcome_notice = None;
                                if !cfg!(test) {
                                    let recent = v.recent.clone();
                                    cx.background_executor()
                                        .spawn(async move {
                                            let _ = kernelledger_client::recent::save(
                                                &kernelledger_client::profile_path(),
                                                &recent,
                                            );
                                        })
                                        .detach();
                                }
                                cx.notify();
                            })),
                    )
                })
        });
        v_flex()
            .flex_1()
            .items_center()
            .justify_center()
            .px_6()
            .py_6()
            .gap_6()
            .child(
                v_flex()
                    .items_center()
                    .gap_3()
                    // The same mark as the title bar, at a size that can carry the page.
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_center()
                            .size(px(52.))
                            .rounded(px(13.))
                            .bg(theme.primary)
                            .text_color(theme.primary_foreground)
                            .font_weight(FontWeight::BOLD)
                            .text_lg()
                            .child("KL"),
                    )
                    .child(
                        v_flex()
                            .items_center()
                            .gap_1()
                            .child(
                                div()
                                    .text_2xl()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child("KernelLedger"),
                            )
                            .child(
                                div()
                                    .text_sm()
                                    .text_center()
                                    .max_w(px(420.))
                                    .text_color(muted_fg)
                                    .child("Python notebooks with a durable record of every run. Source stays in ordinary files on this computer."),
                            ),
                    ),
            )
            .child(
                v_flex()
                    .items_center()
                    .gap_2()
                    .child(
                        h_flex()
                            .gap_2()
                            .flex_wrap()
                            .justify_center()
                            .child(
                                action_button("welcome-open")
                                    .primary()
                                    .icon(Glyph::FolderOpen)
                                    .label("Open notebook")
                                    .on_click(cx.listener(|v, _, window, cx| {
                                        v.open_dialog(&OpenNotebook, window, cx)
                                    })),
                            )
                            .child(
                                action_button("create")
                                    .outline()
                                    .icon(Glyph::Plus)
                                    .label("New notebook")
                                    .on_click(
                                        cx.listener(|v, _, w, cx| v.create_dialog(false, w, cx)),
                                    ),
                            ),
                    )
                    .child(
                        h_flex()
                            .gap_1()
                            .flex_wrap()
                            .justify_center()
                            .items_center()
                            .child(
                                action_button("sample")
                                    .ghost()
                                    .small()
                                    .text_color(muted_fg)
                                    .label("Try a sample notebook")
                                    .on_click(
                                        cx.listener(|v, _, w, cx| v.create_dialog(true, w, cx)),
                                    ),
                            )
                            .child(status_rule(cx))
                            .child(
                                action_button("welcome-help")
                                    .ghost()
                                    .small()
                                    .text_color(muted_fg)
                                    .label("Help and shortcuts")
                                    .hint("Open Help (F1)")
                                    .on_click(cx.listener(|v, _, _, cx| {
                                        v.surface = "Help".into();
                                        cx.notify();
                                    })),
                            ),
                    ),
            )
            .when(!self.recent.is_empty(), |d| {
                d.child(
                    v_flex()
                        .w_full()
                        .max_w(rems(36.))
                        .gap_1()
                        .child(
                            div()
                                .px_2()
                                .text_xs()
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(muted_fg)
                                .child("Recent notebooks"),
                        )
                        .child(card(cx).p_1().gap_0p5().children(recent))
                        .when_some(self.welcome_notice.clone(), |d, notice| {
                            d.child(
                                div()
                                    .id("recent-notice")
                                    .role(Role::Alert)
                                    .aria_label(notice.clone())
                                    .px_2()
                                    .py_1p5()
                                    .text_sm()
                                    .whitespace_normal()
                                    .text_color(warning)
                                    .child(notice),
                            )
                        }),
                )
            })
            .child(
                h_flex()
                    .gap_4()
                    .flex_wrap()
                    .justify_center()
                    .items_center()
                    .children(shortcut_hints.into_iter().map(|(label, keys)| {
                        h_flex()
                            .gap_1p5()
                            .items_center()
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(muted_fg)
                                    .child(label),
                            )
                            .children(kbd(keys))
                    })),
            )
            .into_any_element()
    }
}
