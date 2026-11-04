//! Revision-aware editor markers and immutable source navigation.
use super::*;
use gpui_kit::component::input::{RopeExt, TextDecoration};
use kernelledger_client::{hash, recorded::Location};
use lsp_types::Position;

fn frame_location(frame: &Value) -> Option<Location> {
    Location::parse(
        frame["file"].as_str()?,
        frame["source_revision"].as_str()?,
        frame["line"].as_u64()?,
    )
    .ok()
}

impl Workspace {
    pub(super) fn navigate_recorded_source(
        &mut self,
        location: Location,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(id) = location.cell_id.as_ref()
            && self.tabs[self.active]
                .model
                .cells
                .get(id)
                .is_some_and(|cell| {
                    hash(cell.text.as_bytes()) == location.revision
                        && self.tabs[self.active]
                            .editors
                            .get(id)
                            .is_none_or(|editor| editor.read(cx).value() == cell.text)
                })
        {
            self.select_surface("Notebook", cx);
            self.tabs[self.active].model.selected = Some(id.clone());
            self.tabs[self.active].model.mode = Mode::Edit;
            self.tabs[self.active].reveal = Some(id.clone());
            let editor = self.editor(self.active, id, window, cx);
            editor.update(cx, |editor, cx| {
                let position = Position::new(location.line.saturating_sub(1) as u32, 0);
                editor.unfold_at(position, cx);
                editor.set_cursor_position(position, window, cx);
            });
        } else {
            self.send(Command::RecordedSource(location));
        }
        cx.notify();
    }

    pub(super) fn debug_frame_links(&self, cx: &mut Context<Self>) -> Div {
        let muted = cx.theme().muted_foreground;
        let mut frames = h_flex().gap_1p5().flex_wrap().items_center();
        for (index, frame) in self.tabs[self.active].runtime["debug"]["stack"]
            .as_array()
            .into_iter()
            .flatten()
            .take(32)
            .enumerate()
        {
            let label = format!(
                "{} \u{00b7} line {}",
                frame["function"].as_str().unwrap_or("frame"),
                frame["line"]
            );
            if let Some(location) = frame_location(frame) {
                frames = frames.child(
                    action_button(SharedString::from(format!("debug-source-{index}")))
                        .outline()
                        .small()
                        .label(label)
                        .hint("Open executed source location (opens read-only window if source changed)")
                        .on_click(cx.listener(move |view, _, window, cx| {
                            view.navigate_recorded_source(location.clone(), window, cx)
                        })),
                );
            } else {
                frames = frames.child(badge(format!("{label} (context only)"), muted));
            }
        }
        frames
    }

    pub(super) fn recorded_source_page(&self, cx: &App) -> AnyElement {
        let record = self.tabs[self.active]
            .inspections
            .get("Recorded source")
            .unwrap_or(&Value::Null);
        let selected = record["requested_line"].as_u64().unwrap_or(1);
        let text = record["lines"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|row| {
                format!(
                    "{} {:>6}  {}{}",
                    if row["line"] == selected {
                        "\u{2192}"
                    } else {
                        " "
                    },
                    row["line"].as_u64().unwrap_or(0),
                    row["text"].as_str().unwrap_or(""),
                    if row["truncated"] == true {
                        " \u{2026} [line truncated]"
                    } else {
                        ""
                    }
                )
            })
            .collect::<Vec<_>>()
            .join("\n");
        let theme = cx.theme();
        let (muted, info, mono) = (
            theme.muted_foreground,
            theme.info,
            theme.mono_font_family.clone(),
        );
        let mut paging = h_flex().gap_2().flex_wrap().items_center();
        let mut has_paging = false;
        for (field, label) in [
            ("previous_line", "Previous lines"),
            ("next_line", "Next lines"),
        ] {
            if let Some(line) = record[field].as_u64()
                && let Ok(location) = Location::parse(
                    record["file"].as_str().unwrap_or(""),
                    record["source_revision"].as_str().unwrap_or(""),
                    line,
                )
            {
                has_paging = true;
                let controls = self.tabs[self.active].service.commands.clone();
                paging = paging.child(
                    action_button(SharedString::from(field))
                        .outline()
                        .small()
                        .label(label)
                        .on_click(move |_, _, _| {
                            let _ = controls.send(Command::RecordedSource(location.clone()));
                        }),
                );
            }
        }
        let body = v_flex()
            .w_full()
            .max_w(rems(76.))
            .mx_auto()
            .gap_4()
            .p_5()
            .child(
                h_flex()
                    .justify_between()
                    .items_center()
                    .gap_2()
                    .flex_wrap()
                    .child(
                        h_flex()
                            .gap_2()
                            .items_center()
                            .child(
                                div()
                                    .text_lg()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child("Recorded source"),
                            )
                            .child(badge("Read-only", info))
                            .child(info_tip(
                                "recorded-source-tip",
                                "Read-only executed revision; current notebook edits are preserved. Each page shows up to 15 lines (512 B/line). Copy or export from History for the full source.",
                                cx,
                            )),
                    )
                    .child(
                        div()
                            .text_xs()
                            .font_family(mono)
                            .text_color(muted)
                            .child(
                                record["file"]
                                    .as_str()
                                    .unwrap_or("Recorded execution")
                                    .to_owned(),
                            ),
                    ),
            )
            .child(
                card(cx)
                    .p_4()
                    .gap_3()
                    .child(
                        TextView::markdown(
                            "immutable-source-window",
                            format!("```text\n{}\n```", text.replace("```", "~~~")),
                        )
                        .selectable(true),
                    )
                    .when(has_paging, |d| d.child(paging)),
            );
        div()
            .id("recorded-source-panel")
            .role(Role::Group)
            .aria_label("Recorded source preview")
            .size_full()
            .overflow_y_scroll()
            .child(body)
            .into_any_element()
    }

    pub(super) fn debug_editor_markers(
        &mut self,
        id: &str,
        editor: &Entity<EditorState>,
        cx: &mut Context<Self>,
    ) {
        let tab = &mut self.tabs[self.active];
        let current = frame_location(&tab.runtime["debug"]["stack"][0]).filter(|location| {
            location.cell_id.as_deref() == Some(id)
                && hash(tab.model.cells[id].text.as_bytes()) == location.revision
        });
        let mut decorations = Vec::new();
        if let Some(current) = current {
            let state = editor.read(cx);
            let text = state.text();
            let line = current.line.saturating_sub(1) as usize;
            if line < text.lines_len() {
                let start = text.line_start_offset(line);
                let end = if line + 1 < text.lines_len() {
                    text.line_start_offset(line + 1)
                } else {
                    text.len()
                };
                decorations.push(TextDecoration::new(
                    start..end,
                    HighlightStyle {
                        background_color: Some(cx.theme().warning.opacity(0.2)),
                        ..Default::default()
                    },
                ));
            }
        }
        let collection = tab
            .debug_decorations
            .entry(id.to_owned())
            .or_insert_with(|| {
                editor.update(cx, |editor, cx| {
                    editor.create_decorations_collection(vec![], cx)
                })
            });
        collection.set(decorations, cx);
    }

    pub(super) fn debug_gutter(
        &self,
        id: &str,
        editor: &Entity<EditorState>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let tab = &self.tabs[self.active];
        if !tab.runtime["interactive"]["features"]
            .as_array()
            .is_some_and(|features| features.iter().any(|f| f == "debugger"))
        {
            return div().into_any_element();
        }
        let state = editor.read(cx);
        let revision = hash(tab.model.cells[id].text.as_bytes());
        let saved =
            tab.model.cells[id].save == SaveState::Saved && tab.runtime["worker_state"] == "idle";
        let points = tab
            .inspections
            .get("Breakpoints")
            .and_then(|v| v["breakpoints"].as_array())
            .cloned()
            .unwrap_or_default();
        let current = frame_location(&tab.runtime["debug"]["stack"][0])
            .filter(|location| {
                location.cell_id.as_deref() == Some(id) && location.revision == revision
            })
            .map(|location| location.line);
        let mut lines = std::collections::BTreeSet::new();
        if let Some(range) = state.visible_row_range() {
            lines.extend(range.clone().take(512));
            lines.extend(range.rev().take(512));
        }
        lines.extend(
            points
                .iter()
                .filter(|p| p["cell_id"] == id && p["source_revision"] == revision)
                .filter_map(|p| p["line"].as_u64())
                .map(|line| line.saturating_sub(1) as usize),
        );
        if let Some(line) = current {
            lines.insert(line.saturating_sub(1) as usize);
        }
        let bounds = state.input_bounds();
        let height = editor_height(
            &tab.model.cells[id].text,
            self.preferences.editor_size,
            self.preferences.ui_scale,
        );
        let inset = ((height - bounds.size.height) / 2.).max(px(0.));
        let mut gutter = div()
            .id(SharedString::from(format!("debug-gutter-{id}")))
            .relative()
            .w(px(24.))
            .h(height)
            .flex_none()
            .role(Role::Group)
            .aria_label(format!("Breakpoints for cell {id}"));
        for line in lines {
            if line >= state.text().lines_len() {
                continue;
            }
            let start = state.text().line_start_offset(line);
            let Some(row) = state.range_to_bounds(&(start..start)) else {
                continue;
            };
            let y = row.top() - bounds.top();
            if y < px(0.) || y + row.size.height > bounds.size.height {
                continue;
            }
            let line = line as u64 + 1;
            let active = points.iter().any(|p| {
                p["cell_id"] == id && p["source_revision"] == revision && p["line"] == line
            });
            let marker = if current == Some(line) {
                "\u{2192}"
            } else if active {
                "\u{25cf}"
            } else {
                "\u{00b7}"
            };
            let id = id.to_owned();
            let revision = revision.clone();
            gutter = gutter.child(
                action_button(SharedString::from(format!("gutter-{id}-{line}")))
                    .ghost()
                    .small()
                    .absolute()
                    .top(y + inset)
                    .left_0()
                    .w(px(24.))
                    .h(row.size.height)
                    .p_0()
                    .label(marker)
                    .accessibility_label(format!(
                        "{} breakpoint at line {line} in cell {id}{}",
                        if active { "Remove" } else { "Add" },
                        if current == Some(line) {
                            ", current execution line"
                        } else {
                            ""
                        }
                    ))
                    .unavailable(!saved)
                    .hint("Set breakpoints in a saved cell while the worker is idle")
                    .on_click(cx.listener(move |view, _, _, cx| {
                        view.toggle_breakpoint_line(&id, line, &revision, cx)
                    })),
            );
        }
        gutter.into_any_element()
    }

    pub(super) fn toggle_breakpoint_line(
        &mut self,
        id: &str,
        line: u64,
        revision: &str,
        cx: &mut Context<Self>,
    ) {
        let tab = &mut self.tabs[self.active];
        if tab.runtime["worker_state"] != "idle" {
            return;
        }
        if !tab.model.cells.get(id).is_some_and(|cell| {
            cell.save == SaveState::Saved
                && cell.disk.revision == revision
                && hash(cell.text.as_bytes()) == revision
                && tab
                    .editors
                    .get(id)
                    .is_none_or(|editor| editor.read(cx).value() == cell.text)
        }) {
            return;
        }
        let mut points = tab
            .inspections
            .get("Breakpoints")
            .and_then(|v| v["breakpoints"].as_array())
            .cloned()
            .unwrap_or_default();
        if let Some(index) = points.iter().position(|point| {
            point["cell_id"] == id && point["source_revision"] == revision && point["line"] == line
        }) {
            points.remove(index);
        } else if points.len() < 100 {
            points.push(json!({"cell_id":id,"source_revision":revision,"line":line}));
        }
        tab.inspections
            .insert("Breakpoints".into(), json!({"breakpoints":points}));
        self.interactive_request("debug.breakpoints", json!({"breakpoints":points}));
        cx.notify();
    }
}
