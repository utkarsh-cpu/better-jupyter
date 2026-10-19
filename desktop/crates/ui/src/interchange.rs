use super::*;

impl Workspace {
    pub(super) fn export_notice(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let path = PathBuf::from(
            self.tabs
                .get(self.active)?
                .inspections
                .get("Export result")?["path"]
                .as_str()?,
        );
        let open_path = path.clone();
        let folder_path = path.clone();
        let file_name = path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        Some(
            v_flex()
                .id("export-notice")
                .absolute()
                .bottom(rems(2.5))
                .right(rems(0.75))
                .max_w(rems(28.))
                .p_3()
                .gap_2()
                .rounded(px(8.))
                .bg(cx.theme().popover)
                .border_1()
                .border_color(divider(cx))
                .when(!self.high_contrast, |d| d.shadow_md())
                .role(Role::Status)
                .aria_label(format!("Export complete: {file_name}"))
                .child(
                    div()
                        .text_sm()
                        .font_weight(FontWeight::MEDIUM)
                        .child("Export complete"),
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .truncate()
                        .child(file_name),
                )
                .child(
                    h_flex()
                        .gap_2()
                        .flex_wrap()
                        .child(
                            action_button("open-export")
                                .small()
                                .label("Open file")
                                .on_click(move |_, _, cx| cx.open_with_system(&open_path)),
                        )
                        .child(
                            action_button("reveal-export")
                                .small()
                                .ghost()
                                .label("Show in folder")
                                .on_click(move |_, _, cx| cx.reveal_path(&folder_path)),
                        )
                        .child(
                            action_button("dismiss-export")
                                .small()
                                .ghost()
                                .label("Dismiss")
                                .on_click(cx.listener(|v, _, _, cx| {
                                    if let Some(tab) = v.tabs.get_mut(v.active) {
                                        tab.inspections.remove("Export result");
                                    }
                                    cx.notify();
                                })),
                        ),
                )
                .into_any_element(),
        )
    }

    pub(super) fn export_dialog(
        &mut self,
        format: &'static str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(tab) = self.tabs.get(self.active) else {
            return;
        };
        let service = tab.service.commands.clone();
        let directory = tab.model.project.clone();
        let origin = directory.clone();
        let name = format!(
            "{}.{}",
            directory.file_name().unwrap_or_default().to_string_lossy(),
            format
        );
        let path = cx.prompt_for_new_path(&directory, Some(&name));
        cx.spawn_in(window, async move |view, window| match path.await {
            Ok(Ok(Some(mut path))) => {
                if path.extension().is_none() {
                    path.set_extension(format);
                }
                let method = match format {
                    "zip" => "bundle_export",
                    "ipynb" => "export_ipynb",
                    _ => "export_document",
                };
                let mut fields = json!({"path":path});
                if method == "export_document" {
                    fields["format"] = json!(format);
                }
                let _ = service.send(Command::Inspect {
                    method: method.into(),
                    fields,
                });
                let _ = view.update_in(window, |v, _, cx| {
                    if let Some(tab) = v.tabs.iter_mut().find(|tab| tab.model.project == origin) {
                        tab.inspections.remove("Export result");
                    }
                    cx.notify();
                });
            }
            Ok(Err(error)) => {
                let _ = view.update_in(window, |v, _, cx| {
                    if let Some(tab) = v.tabs.iter_mut().find(|tab| tab.model.project == origin) {
                        tab.error = Some(format!(
                            "The export location could not be selected: {error}"
                        ));
                    }
                    cx.notify();
                });
            }
            _ => {}
        })
        .detach();
    }

    pub(super) fn inspect_bundle_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let paths = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("Inspect a portable bundle".into()),
        });
        cx.spawn_in(window, async move |view, window| {
            if let Ok(Ok(Some(paths))) = paths.await
                && let Some(path) = paths.into_iter().next()
            {
                let _ = view.update_in(window, |v, _, cx| {
                    if let Some(tab) = v.tabs.get_mut(v.active) {
                        tab.inspections.insert(
                            "Bundle review".into(),
                            json!({"path": path, "pending": true}),
                        );
                    }
                    v.surface = "Diagnostics".into();
                    v.inspect("bundle.inspect", json!({"path": path}));
                    cx.notify();
                });
            }
        })
        .detach();
    }

    pub(super) fn bundle_review(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let tab = self.tabs.get(self.active)?;
        let value = tab.inspections.get("Bundle review")?;
        let path = value["path"].as_str().unwrap_or("Selected bundle");
        let file_name = PathBuf::from(path)
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.to_owned());
        let pending = value["pending"] == true;
        let mismatches = value["hash_mismatches"].as_array();
        let clean = !pending
            && value.get("error").is_none()
            && value["schema"].as_u64() == Some(1)
            && mismatches.is_some_and(|items| items.is_empty())
            && value["path"].is_string();
        let import_path = value["path"].as_str().unwrap_or_default().to_owned();
        let mut card_body = card(cx)
            .id("bundle-review")
            .role(Role::Group)
            .aria_label(format!("Foreign bundle review: {file_name}"))
            .p_4()
            .gap_2()
            .child(
                div()
                    .text_sm()
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(file_name),
            )
            .child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .whitespace_normal()
                    .child(path.to_owned()),
            );
        if pending {
            card_body = card_body.child(
                div()
                    .text_sm()
                    .child("Reading the bundle. Nothing has been imported."),
            );
        } else if let Some(code) = value["error"]["code"].as_str() {
            card_body = card_body.child(
                div()
                    .text_sm()
                    .text_color(cx.theme().danger)
                    .whitespace_normal()
                    .child(format!(
                        "{}. Notebook source was not changed.",
                        error_message(code)
                    )),
            );
        } else if value["schema"].as_u64().is_some_and(|schema| schema != 1) {
            card_body = card_body.child(
                div()
                    .text_sm()
                    .text_color(cx.theme().danger)
                    .whitespace_normal()
                    .child("This bundle schema is not supported. Nothing was imported."),
            );
        } else {
            let sessions = value["session_ids"].as_array().map(Vec::len).unwrap_or(0);
            let checkpoints = value["checkpoint_ids"]
                .as_array()
                .map(Vec::len)
                .unwrap_or(0);
            card_body = card_body.child(
                div().text_sm().whitespace_normal().child(format!(
                    "{sessions} recorded sessions and {checkpoints} checkpoints. Import copies them into this notebook and keeps local records with the same identifiers."
                )),
            );
            if let Some(external) = value["external"].as_array()
                && !external.is_empty()
            {
                let names = external
                    .iter()
                    .filter_map(|item| {
                        item.as_str()
                            .or_else(|| item["path"].as_str().or(item["name"].as_str()))
                    })
                    .take(6)
                    .collect::<Vec<_>>()
                    .join(", ");
                card_body = card_body.child(
                    div()
                        .text_sm()
                        .whitespace_normal()
                        .child(format!("External files named in the bundle: {names}")),
                );
            }
            if let Some(items) = mismatches
                && !items.is_empty()
            {
                let names = items
                    .iter()
                    .filter_map(|item| item.as_str())
                    .take(8)
                    .collect::<Vec<_>>()
                    .join(", ");
                card_body = card_body.child(
                    div()
                        .text_sm()
                        .text_color(cx.theme().danger)
                        .whitespace_normal()
                        .child(format!(
                            "Import is unavailable. {} file hash mismatches: {names}",
                            items.len()
                        )),
                );
            }
            card_body = card_body.child(
                div()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .whitespace_normal()
                    .child(
                        "Provenance is the selected bundle. Saved code does not run during import. Checkpoint values and recorded input may contain secrets.",
                    ),
            );
        }
        if let Some(note) = source_conflict_note(&tab.model) {
            card_body = card_body.child(
                div()
                    .text_sm()
                    .whitespace_normal()
                    .text_color(cx.theme().warning)
                    .child(note),
            );
        } else if !pending {
            card_body = card_body.child(
                div()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .whitespace_normal()
                    .child("Notebook source is not replaced."),
            );
        }
        card_body = card_body.child(
            h_flex()
                .gap_2()
                .flex_wrap()
                .child(
                    action_button("import-bundle-history")
                        .outline()
                        .small()
                        .label("Import recorded history\u{2026}")
                        .unavailable(!clean)
                        .on_click(cx.listener(move |v, _, _, cx| {
                            if v.tabs.get(v.active).is_some_and(|tab| {
                                tab.inspections.get("Bundle review").is_some_and(|review| {
                                    review["pending"] != true
                                        && review.get("error").is_none()
                                        && review["schema"].as_u64() == Some(1)
                                        && review["hash_mismatches"]
                                            .as_array()
                                            .is_some_and(|items| items.is_empty())
                                })
                            }) {
                                v.confirmation = Some((
                                    "bundle_import".into(),
                                    json!({"path": import_path.clone(), "trust": false}),
                                ));
                            }
                            cx.notify();
                        })),
                )
                .child(
                    action_button("dismiss-bundle-review")
                        .ghost()
                        .small()
                        .label("Dismiss")
                        .on_click(cx.listener(|v, _, _, cx| {
                            if let Some(tab) = v.tabs.get_mut(v.active) {
                                tab.inspections.remove("Bundle review");
                            }
                            cx.notify();
                        })),
                ),
        );
        Some(
            section("Foreign bundle", cx)
                .child(card_body)
                .into_any_element(),
        )
    }
}

fn source_conflict_note(model: &NotebookModel) -> Option<String> {
    let mut notes = Vec::new();
    if model.source_state == "conflict_markers" {
        notes.push("Git conflict markers are in the notebook source.".to_string());
    }
    let cells = model
        .order
        .iter()
        .filter_map(|id| {
            let cell = model.cells.get(id)?;
            matches!(cell.save, SaveState::Conflict | SaveState::Failed)
                .then(|| format!("{} ({id})", cell_title(&cell.text, &cell.disk.kind)))
        })
        .collect::<Vec<_>>();
    if !cells.is_empty() {
        notes.push(format!(
            "External source conflicts remain: {}.",
            cells.join(", ")
        ));
    }
    if model.external_changes > 0 {
        notes.push(format!(
            "{} external file change(s) are still pending.",
            model.external_changes
        ));
    }
    if notes.is_empty() {
        None
    } else {
        Some(format!(
            "{} Import does not replace notebook source.",
            notes.join(" ")
        ))
    }
}
