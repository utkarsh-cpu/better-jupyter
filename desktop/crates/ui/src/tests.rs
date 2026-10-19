use super::{
    Palette, Tab, Workspace, confirmation_copy, history_matches, inspection_surface,
    runtime_action_available,
};
use gpui_kit::{
    AnyWindowHandle, AppContext, Bounds, Entity, Focusable, ListAlignment, ListState, Point,
    ScrollDelta, WindowBounds, WindowOptions, px, size,
};
use gpui_kit::{TestAppContext, test::TestWindowExt};
use kernelledger_client::service::{Command, NotebookService};
use kernelledger_models::NotebookModel;
use serde_json::json;
use std::sync::mpsc::{self, Receiver};
use std::{collections::BTreeMap, path::PathBuf};

fn mount(cx: &mut TestAppContext) -> (AnyWindowHandle, Entity<Workspace>, Receiver<Command>) {
    cx.update(gpui_kit::init);
    let (send, commands) = mpsc::channel();
    let (_, events) = mpsc::channel();
    let (window, view) = cx.update(|cx| {
        gpui_kit::open_window(WindowOptions { window_bounds: Some(WindowBounds::Windowed(Bounds { origin: Point::default(), size: size(px(800.), px(600.)) })), ..Default::default() }, cx, |window, cx| {
            cx.new(|cx| {
                let mut view = Workspace::new(None, "system", vec![], window, cx);
                let mut model = NotebookModel { project: PathBuf::from("test-notebook"), ..Default::default() };
                model.apply_snapshot(serde_json::from_value(json!({"state":"ok","manifest_revision":"test","cells":[{"id":"first","type":"code","source":"print(42)","revision":"hash"},{"id":"second","type":"code","source":"x = 1","revision":"hash2"}]})).unwrap());
                view.tabs.push(Tab { model, service: NotebookService { commands: send, events }, list: ListState::new(2, ListAlignment::Top, px(200.)), editors: BTreeMap::new(), subscriptions: vec![], runtime: json!({"worker_state":"idle"}), operations: BTreeMap::new(), inspections: BTreeMap::new(), inspection_method: BTreeMap::new(), scrolls: BTreeMap::new(), parents: vec![], raw_details: BTreeMap::new(), reveal: None,
                    uninitialized: false, selections: BTreeMap::new(), drafts: vec![], history_filter: String::new(), checkpoint_notice: None, error: None, attachment_pages: BTreeMap::new() });
                view
            })
        }).unwrap()
    });
    (window, view, commands)
}

#[gpui_kit::test]
fn project_shortcuts_navigate_find_and_preserve_editor_text(cx: &mut TestAppContext) {
    let (handle, view, commands) = mount(cx);
    cx.update_window(handle, |_, window, cx| {
        view.update(cx, |v, cx| {
            v.tabs[0].model.selected = Some("first".into());
            v.editor(0, "first", window, cx)
                .update(cx, |s, cx| s.focus(window, cx));
        });
        window.render_frame(cx);
        window.press("ctrl-2", cx);
        assert_eq!(view.read(cx).surface, "History");
        window.render_frame(cx);
        window.press("ctrl-3", cx);
        assert_eq!(view.read(cx).surface, "Checkpoints");
        window.render_frame(cx);
        window.press("ctrl-,", cx);
        assert_eq!(view.read(cx).surface, "Settings");
        window.render_frame(cx);
        window.press("ctrl-1", cx);
        assert_eq!(view.read(cx).surface, "Notebook");
        window.render_frame(cx);
        window.press("ctrl-shift-f", cx);
        assert!(
            view.read(cx)
                .query
                .read(cx)
                .focus_handle(cx)
                .is_focused(window)
        );
        assert!(view.read(cx).outline_drawer);
        assert_eq!(
            view.read(cx).tabs[0].editors["first"]
                .read(cx)
                .value()
                .as_ref(),
            "print(42)"
        );
    })
    .unwrap();
    assert!(
        !commands
            .try_iter()
            .any(|c| matches!(c, Command::Mutate(_) | Command::Run { .. }))
    );
}

#[gpui_kit::test]
fn cell_shortcuts_reuse_source_guards_and_delete_confirmation(cx: &mut TestAppContext) {
    let (handle, view, commands) = mount(cx);
    cx.update_window(handle, |_, window, cx| {
        view.update(cx, |v, cx| {
            v.tabs[0].model.selected = Some("first".into());
            v.focus.focus(window, cx);
        });
        window.render_frame(cx);
        window.press("ctrl-shift-n", cx);
        window.press("ctrl-shift-m", cx);
        window.press("ctrl-shift-d", cx);
        window.press("ctrl-alt-down", cx);
        window.press("ctrl-shift-delete", cx);
        assert_eq!(view.read(cx).confirmation.as_ref().unwrap().0, "delete");
        window.render_frame(cx);
        window.press("ctrl-shift-n", cx); // Dialogs block background cell changes.
        window.press("escape", cx);
        view.update(cx, |v, _| {
            v.tabs[0].model.cells.get_mut("first").unwrap().save =
                kernelledger_models::SaveState::Dirty
        });
        window.press("ctrl-shift-d", cx);
    })
    .unwrap();
    let mutations: Vec<_> = commands
        .try_iter()
        .filter_map(|c| {
            if let Command::Mutate(value) = c {
                Some(value)
            } else {
                None
            }
        })
        .collect();
    assert_eq!(mutations.len(), 4);
    assert_eq!(mutations[0]["type"], "code");
    assert_eq!(mutations[1]["type"], "markdown");
    assert_eq!(mutations[2]["action"], "duplicate");
    assert_eq!(mutations[3]["index"], 1);
}

#[gpui_kit::test]
fn export_menu_blocks_background_actions_and_escape_restores_focus(cx: &mut TestAppContext) {
    let (handle, view, commands) = mount(cx);
    cx.update_window(handle, |_, window, cx| {
        view.update(cx, |v, cx| v.focus.focus(window, cx));
        window.render_frame(cx);
        window.press("ctrl-shift-e", cx);
        assert!(view.read(cx).export_menu);
        assert!(view.read(cx).dialog_focus.is_focused(window));
        window.render_frame(cx);
        window.press("ctrl-shift-n", cx);
        window.press("ctrl-shift-enter", cx);
        assert!(commands.try_recv().is_err());
        window.press("escape", cx);
        assert!(!view.read(cx).export_menu);
        assert!(view.read(cx).focus.is_focused(window));
        view.update(cx, |v, _| {
            v.tabs[0].model.cells.get_mut("first").unwrap().save =
                kernelledger_models::SaveState::Conflict
        });
        window.press("ctrl-shift-e", cx);
        assert!(!view.read(cx).export_menu);
    })
    .unwrap();
}

#[gpui_kit::test]
fn export_selection_sends_format_and_cancellation_sends_nothing(cx: &mut TestAppContext) {
    let (handle, view, commands) = mount(cx);
    cx.update(|cx| view.update(cx, |v, _| v.tick = gpui_kit::Task::ready(())));
    for format in ["pdf", "docx"] {
        cx.update_window(handle, |_, window, cx| {
            view.update(cx, |v, cx| v.focus.focus(window, cx));
            window.render_frame(cx);
            // At 800px the header folds Export into Menu; the shortcut stays.
            window.press("ctrl-shift-e", cx);
            window.render_frame(cx);
            window.click(format!("export-{format}"), cx);
            assert!(!view.read(cx).export_menu);
        })
        .unwrap();
        assert!(cx.did_prompt_for_new_path());
        cx.simulate_new_path_selection(|_| Some(PathBuf::from(format!("report.{format}"))));
        cx.executor().run_until_parked();
        assert!(
            matches!(commands.try_recv().unwrap(), Command::Inspect { method, fields } if method == "export_document" && fields["format"] == format && fields["path"] == format!("report.{format}"))
        );
    }
    cx.update_window(handle, |_, window, cx| {
        view.update(cx, |v, cx| v.export_dialog("pdf", window, cx));
    })
    .unwrap();
    cx.simulate_new_path_selection(|_| None);
    cx.executor().run_until_parked();
    assert!(commands.try_recv().is_err());
}

#[gpui_kit::test]
fn structural_commands_respect_saved_source_and_require_merge_confirmation(
    cx: &mut TestAppContext,
) {
    let (window, view, commands) = mount(cx);
    cx.update_window(window, |_, window, cx| {
        view.update(cx, |v, cx| {
            v.tabs[0].model.selected = Some("first".into());
            let editor = v.editor(0, "first", window, cx);
            editor.update(cx, |s, cx| s.set_value("a\u{1f600}b", window, cx));
            v.tabs[0].model.cells.get_mut("first").unwrap().text = "a\u{1f600}b".into();
            v.tabs[0].model.cells.get_mut("first").unwrap().save =
                kernelledger_models::SaveState::Saved;
            // Kit offsets are UTF-8 bytes; source service offsets count Unicode characters.
            editor.update(cx, |s, cx| {
                s.set_cursor_position(gpui_kit::component::input::Position::new(0, 2), window, cx)
            });
            v.run_command("split-cell", window, cx);
            v.run_command("merge-cell", window, cx);
            assert_eq!(v.confirmation.as_ref().unwrap().0, "merge");
            v.confirmation = None;
            v.tabs[0].model.cells.get_mut("first").unwrap().save =
                kernelledger_models::SaveState::Conflict;
            assert!(v.command_reason("split-cell").is_some());
            assert!(v.command_reason("merge-cell").is_some());
            v.tabs[0].model.selected = Some("second".into());
            assert!(v.command_reason("merge-cell").is_some());
        });
    })
    .unwrap();
    let mutations = commands
        .try_iter()
        .filter_map(|c| {
            if let Command::Mutate(v) = c {
                Some(v)
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    assert_eq!(mutations.len(), 1);
    assert_eq!(mutations[0]["action"], "split");
    assert_eq!(mutations[0]["offset"], 2);
}

#[gpui_kit::test]
fn run_keys_precede_editor_newline_and_submit_once(cx: &mut TestAppContext) {
    let (window, view, commands) = mount(cx);
    for key in ["ctrl-enter", "shift-enter", "alt-enter"] {
        cx.update_window(window, |_, window, cx| {
            view.update(cx, |v, cx| {
                v.tabs[0].model.selected = Some("first".into());
                let editor = v.editor(0, "first", window, cx);
                editor.update(cx, |s, cx| s.focus(window, cx));
            });
            window.render_frame(cx);
            window.press(key, cx);
            assert_eq!(
                view.read(cx).tabs[0].editors["first"]
                    .read(cx)
                    .value()
                    .as_ref(),
                "print(42)"
            );
        })
        .unwrap();
        let sent = commands.try_iter().collect::<Vec<_>>();
        assert_eq!(
            sent.iter()
                .filter(|c| matches!(c, Command::Run { .. }))
                .count(),
            1,
            "{key}"
        );
        if key == "alt-enter" {
            assert!(
                sent.iter()
                    .any(|c| matches!(c, Command::Mutate(v) if v["action"] == "add"))
            );
        }
    }
    cx.update_window(window, |_, window, cx| {
        window.press("enter", cx);
        assert_ne!(
            view.read(cx).tabs[0].editors["first"]
                .read(cx)
                .value()
                .as_ref(),
            "print(42)"
        );
    })
    .unwrap();
}

#[gpui_kit::test]
fn palette_manual_scroll_survives_redraw_and_keyboard_reveals_selection(cx: &mut TestAppContext) {
    let (window, view, _) = mount(cx);
    cx.update_window(window, |_, window, cx| {
        view.update(cx, |v, cx| v.palette(&Palette, window, cx));
        window.render_frame(cx);
        let list_scroll = view.read(cx).palette_scroll.clone();
        let scroll = list_scroll.0.borrow().base_handle.clone();
        assert!(scroll.bounds().size.height > px(100.));
        assert!(scroll.max_offset().y > px(0.));

        let started = std::time::Instant::now();
        for _ in 0..30 {
            window.dispatch_event(
                gpui_kit::PlatformInput::ScrollWheel(gpui_kit::ScrollWheelEvent {
                    position: scroll.bounds().center(),
                    delta: ScrollDelta::Pixels(Point::new(px(0.), px(-240.))),
                    ..Default::default()
                }),
                cx,
            );
            view.update(cx, |_, cx| cx.notify());
            window.render_frame(cx);
        }
        eprintln!("30 palette scroll frames: {:?}", started.elapsed());
        assert_eq!(list_scroll.is_scrolled_to_end(), Some(true));
        let offset = scroll.offset();
        view.update(cx, |_, cx| cx.notify());
        window.render_frame(cx);
        assert_eq!(scroll.offset(), offset);

        window.press("down", cx);
        window.render_frame(cx);
        let item_size = list_scroll.0.borrow().last_item_size.unwrap();
        let row_height = item_size.contents.height / super::COMMANDS.len() as f32;
        assert!(row_height + scroll.offset().y >= px(0.));
        assert!(row_height * 2. + scroll.offset().y <= item_size.item.height);
        window.press("end", cx);
        window.render_frame(cx);
        assert_eq!(view.read(cx).palette_active, super::COMMANDS.len() - 1);
        assert_eq!(list_scroll.is_scrolled_to_end(), Some(true));

        view.update(cx, |v, cx| {
            v.palette_query
                .update(cx, |s, cx| s.set_value("checkpoint", window, cx));
        });
        window.render_frame(cx);
        assert_eq!(view.read(cx).palette_active, 0);
        assert!(scroll.offset().y >= px(-10.));

        view.update(cx, |v, cx| {
            v.palette_query
                .update(cx, |s, cx| s.set_value("", window, cx));
        });
        window.render_frame(cx);
        window.press("end", cx);
        window.render_frame(cx);
        assert!(scroll.offset().y < px(-10.));
        window.press("escape", cx);
        view.update(cx, |v, cx| v.palette(&Palette, window, cx));
        window.render_frame(cx);
        assert_eq!(view.read(cx).palette_active, 0);
        assert!(scroll.offset().y >= px(-10.));
    })
    .unwrap();
}

#[gpui_kit::test]
fn palette_filters_navigates_and_restores_focus(cx: &mut TestAppContext) {
    let (window, view, commands) = mount(cx);
    cx.update_window(window, |_, window, cx| {
        view.update(cx, |v, cx| {
            v.focus.focus(window, cx);
            v.palette(&Palette, window, cx);
            v.palette_query
                .update(cx, |s, cx| s.set_value("checkpoint", window, cx));
        });
        window.render_frame(cx);
        view.update(cx, |v, _| {
            v.palette_active = super::COMMANDS
                .iter()
                .filter(|(_, label, description, _)| {
                    label.to_lowercase().contains("checkpoint")
                        || description.to_lowercase().contains("checkpoint")
                })
                .position(|(method, ..)| *method == "checkpoint")
                .unwrap();
        });
        window.press("enter", cx);
        assert!(!view.read(cx).palette);
        assert!(view.read(cx).focus.is_focused(window));
    })
    .unwrap();
    assert!(
        matches!(commands.try_recv().unwrap(), Command::Runtime { method, .. } if method == "checkpoint")
    );
}

#[gpui_kit::test]
fn delete_requires_confirmation_and_escape_preserves_source(cx: &mut TestAppContext) {
    let (window, view, commands) = mount(cx);
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        window.click(("cell-more", 0usize), cx);
        window.render_frame(cx);
        window.click("delete", cx);
        assert_eq!(view.read(cx).confirmation.as_ref().unwrap().0, "delete");
        assert!(commands.try_recv().is_err());
        window.render_frame(cx);
        window.press("escape", cx);
        assert!(view.read(cx).confirmation.is_none());
        assert_eq!(view.read(cx).tabs[0].model.cells["first"].text, "print(42)");
    })
    .unwrap();
}

#[test]
fn filters_combine_cell_outcome_session_and_time() {
    let model = NotebookModel::default();
    let row = json!({"exec_id":"abc","cell_id":"sales","status":"error","session_id":"s1","start":"2026-10-05T10:00:00Z","source":"total = sum(values)"});
    assert!(history_matches(
        &row,
        "cell:sales outcome:error session:s1 from:2026-10-01 to:2026-10-05 total",
        &model
    ));
    assert!(!history_matches(&row, "outcome:ok", &model));
}

#[test]
fn surface_results_and_runtime_actions_are_scoped() {
    assert_eq!(inspection_surface("training.inspect"), "Checkpoints");
    assert!(!runtime_action_available(
        "training-checkpoint",
        &json!({"worker_state":"idle"}),
        "Notebook"
    ));
    assert!(!runtime_action_available(
        "training-checkpoint",
        &json!({"worker_state":"running","capabilities":["portable_training"]}),
        "Notebook"
    ));
    assert!(runtime_action_available(
        "training-checkpoint",
        &json!({"worker_state":"idle","capabilities":["portable_training"]}),
        "Notebook"
    ));
    assert!(
        confirmation_copy("training-restore")
            .1
            .contains("preserves the current worker")
    );
    assert_eq!(inspection_surface("doctor"), "Diagnostics");
    assert_eq!(inspection_surface("interpreter.validate"), "Settings");
    assert!(!runtime_action_available(
        "interrupt",
        &json!({"worker_state":"stopped"}),
        "Notebook"
    ));
    assert!(runtime_action_available(
        "checkpoint",
        &json!({"worker_state":"idle"}),
        "History"
    ));
    assert!(runtime_action_available(
        "interrupt",
        &json!({"worker_state":"running"}),
        "Notebook"
    ));
    assert!(confirmation_copy("stop").1.contains("shuts down"));
    assert_eq!(confirmation_copy("stop").2, "Stop runtime");
    assert!(
        confirmation_copy("bundle_import")
            .1
            .contains("no code runs")
    );
    assert_eq!(
        confirmation_copy("bundle_import").2,
        "Import recorded history"
    );
}

#[gpui_kit::test]
fn command_mode_run_bindings_preserve_source(cx: &mut TestAppContext) {
    let (handle, view, commands) = mount(cx);
    for key in ["ctrl-enter", "shift-enter", "alt-enter"] {
        cx.update_window(handle, |_, window, cx| {
            view.update(cx, |v, cx| {
                v.tabs[0].model.selected = Some("first".into());
                v.tabs[0].model.mode = kernelledger_models::Mode::Command;
                v.focus.focus(window, cx);
            });
            window.render_frame(cx);
            window.press(key, cx);
            assert_eq!(view.read(cx).tabs[0].model.cells["first"].text, "print(42)");
        })
        .unwrap();
        assert_eq!(
            commands
                .try_iter()
                .filter(|c| matches!(c, Command::Run { .. }))
                .count(),
            1
        );
    }
}

#[gpui_kit::test]
fn ime_composition_suppresses_run_shortcuts(cx: &mut TestAppContext) {
    use gpui_kit::EntityInputHandler;
    let (handle, view, commands) = mount(cx);
    cx.update_window(handle, |_, window, cx| {
        view.update(cx, |v, cx| {
            let editor = v.editor(0, "first", window, cx);
            editor.update(cx, |s, cx| {
                s.focus(window, cx);
                s.replace_and_mark_text_in_range(Some(0..0), "x", Some(0..1), window, cx);
            });
        });
        window.render_frame(cx);
        for key in ["ctrl-enter", "shift-enter", "alt-enter"] {
            window.press(key, cx);
        }
    })
    .unwrap();
    assert!(
        !commands
            .try_iter()
            .any(|c| matches!(c, Command::Run { .. }))
    );
}

#[gpui_kit::test]
fn dialog_starts_on_cancel_and_traps_keyboard_focus(cx: &mut TestAppContext) {
    let (handle, view, _) = mount(cx);
    cx.update_window(handle, |_, window, cx| {
        view.update(cx, |v, cx| {
            v.focus.focus(window, cx);
            v.confirmation = Some(("stop".into(), json!({})));
        });
        window.render_frame(cx);
        assert!(view.read(cx).confirm_focus.is_focused(window));
        for _ in 0..10 {
            window.press("tab", cx);
            assert!(view.read(cx).dialog_focus.contains_focused(window, cx));
        }
        window.press("escape", cx);
        assert!(view.read(cx).focus.is_focused(window));
    })
    .unwrap();
}

#[gpui_kit::test]
fn theme_text_and_focus_tokens_meet_contrast_thresholds(cx: &mut TestAppContext) {
    use gpui_kit::component::ActiveTheme;
    fn luminance(color: gpui_kit::Hsla) -> f32 {
        let color = color.to_rgb();
        let linear = |v: f32| {
            if v <= 0.04045 {
                v / 12.92
            } else {
                ((v + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * linear(color.r) + 0.7152 * linear(color.g) + 0.0722 * linear(color.b)
    }
    let contrast = |a, b| {
        let (a, b) = (luminance(a), luminance(b));
        (a.max(b) + 0.05) / (a.min(b) + 0.05)
    };
    let (handle, _, _) = mount(cx);
    cx.update_window(handle, |_, window, cx| {
        for (dark, high) in [(false, false), (true, false), (false, true)] {
            super::apply_theme(dark, high, window, cx);
            let t = cx.theme();
            for (a, b) in [
                (t.foreground, t.background),
                (t.muted_foreground, t.background),
                (t.foreground, t.popover),
                (t.muted_foreground, t.popover),
                (t.danger, t.background),
                (t.success, t.background),
                (t.warning, t.background),
                (t.primary_foreground, t.primary),
                (t.button_primary_foreground, t.button_primary_hover),
                (t.button_primary_foreground, t.button_primary_active),
                (t.button_danger_foreground, t.button_danger),
                (t.button_danger_foreground, t.button_danger_hover),
            ] {
                assert!(
                    contrast(a, b) >= 4.5,
                    "dark={dark}, high={high}: {}",
                    contrast(a, b)
                );
            }
            assert!(contrast(t.ring, t.background) >= 3.0);
            let syntax = serde_json::to_value(&t.highlight_theme.style.syntax).unwrap();
            for style in syntax.as_object().unwrap().values() {
                if let Some(color) = style
                    .get("color")
                    .and_then(|c| serde_json::from_value::<gpui_kit::Hsla>(c.clone()).ok())
                {
                    assert!(
                        contrast(color, t.background) >= 4.5 && contrast(color, t.muted) >= 4.5
                    );
                }
            }
        }
    })
    .unwrap();
}

#[gpui_kit::test]
fn clipped_settings_control_scrolls_into_view_before_it_runs(cx: &mut TestAppContext) {
    let (handle, view, _) = mount(cx);
    cx.simulate_window_resize(handle, size(px(800.), px(640.)));
    cx.update_window(handle, |_, window, cx| {
        view.update(cx, |v, _| {
            v.preferences.ui_scale = 2.;
            v.surface = "Settings".into();
        });
        window.refresh();
        window.render_frame(cx);
    })
    .unwrap();
    let (viewport, observed) = cx
        .update_window(handle, |_, window, cx| {
            let viewport = view.read(cx).tabs[0]
                .scrolls
                .get("Settings")
                .expect("Settings keeps a scroll handle")
                .bounds();
            let mut observed = Vec::new();
            for id in [
                "settings-appearance",
                "settings-typography",
                "settings-layout",
                "settings-accessibility",
                "settings-interpreter",
                "appearance-contrast",
                "ui-scale-2",
                "editor-size-24",
                "density-true",
                "outline-width-300",
                "reduced-motion",
                "interpreter",
            ] {
                match window.try_find(id) {
                    Some(node) => observed.push((id, node.bounds(), node.visible())),
                    None => observed.push((id, Bounds::default(), false)),
                }
            }
            (viewport, observed)
        })
        .unwrap();
    let mut chosen = None;
    for (id, bounds, visible) in &observed {
        if !id.starts_with("settings-") && *visible {
            let center = bounds.center();
            if center.y <= viewport.bottom()
                && center.y >= viewport.top()
                && bounds.bottom() > viewport.bottom() + px(1.)
            {
                chosen = Some((*id, *bounds));
                break;
            }
        }
    }
    let (id, before) = chosen.unwrap_or_else(|| {
        panic!(
            "a settings control must extend below the viewport {viewport:?}\n{}",
            observed
                .iter()
                .map(|(id, bounds, visible)| format!("{id}: visible={visible} {bounds:?}"))
                .collect::<Vec<_>>()
                .join("\n")
        )
    });
    let offset_before = cx
        .update_window(handle, |_, _, cx| {
            view.read(cx).tabs[0].scrolls["Settings"].offset()
        })
        .unwrap();
    cx.update_window(handle, |_, window, cx| {
        window.click(id, cx);
        assert_eq!(
            window.simulate_next_frame(cx),
            2,
            "scroll is scheduled before the setting runs"
        );
        window.render_frame(cx);
    })
    .unwrap();
    let offset_after = cx
        .update_window(handle, |_, _, cx| {
            view.read(cx).tabs[0].scrolls["Settings"].offset()
        })
        .unwrap();
    assert!(
        offset_after.y < offset_before.y,
        "clipped control {id} should scroll before running: {offset_before:?} -> {offset_after:?}"
    );
    let after = cx
        .update_window(handle, |_, window, _| {
            window
                .try_find(id)
                .expect("control remains after scrolling")
                .bounds()
        })
        .unwrap();
    let viewport = cx
        .update_window(handle, |_, _, cx| {
            view.read(cx).tabs[0].scrolls["Settings"].bounds()
        })
        .unwrap();
    assert!(
        after.bottom() <= viewport.bottom() + px(1.),
        "{id} bottom {:?} still outside viewport {:?}; before {:?}",
        after,
        viewport,
        before
    );
    cx.update_window(handle, |_, _, cx| {
        let view = view.read(cx);
        match id {
            "appearance-contrast" => assert!(view.high_contrast),
            "ui-scale-2" => assert_eq!(view.preferences.ui_scale, 2.),
            "editor-size-24" => assert_eq!(view.preferences.editor_size, 24.),
            "density-true" => assert!(view.preferences.comfortable),
            "outline-width-300" => assert_eq!(view.preferences.outline_width, 300.),
            "reduced-motion" => assert!(view.preferences.reduced_motion),
            _ => {}
        }
    })
    .unwrap();
}

#[gpui_kit::test]
fn settings_segments_change_preferences_through_native_buttons(cx: &mut TestAppContext) {
    let (handle, view, _) = mount(cx);
    cx.simulate_window_resize(handle, size(px(1280.), px(900.)));
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.click("settings-button", cx);
        assert_eq!(view.read(cx).surface, "Settings");
        window.render_frame(cx);
        window.click("editor-size-16", cx);
        assert_eq!(view.read(cx).preferences.editor_size, 16.);
        window.click("ui-scale-1.25", cx);
        assert_eq!(view.read(cx).preferences.ui_scale, 1.25);
        window.render_frame(cx);
        window.click("ui-scale-1", cx);
        assert_eq!(view.read(cx).preferences.ui_scale, 1.);
    })
    .unwrap();
}

#[gpui_kit::test]
fn header_run_action_executes_an_unselected_cell_once(cx: &mut TestAppContext) {
    let (handle, view, commands) = mount(cx);
    cx.update_window(handle, |_, window, cx| {
        view.update(cx, |v, _| v.tabs[0].model.selected = Some("first".into()));
        window.render_frame(cx);
        window.click(("run", 1usize), cx);
    })
    .unwrap();
    let runs: Vec<_> = commands
        .try_iter()
        .filter_map(|command| match command {
            Command::Run { cell_ids, .. } => Some(cell_ids),
            _ => None,
        })
        .collect();
    assert_eq!(runs, vec![vec!["second".to_string()]]);
}

#[gpui_kit::test]
fn outline_is_contextual_and_collapse_survives_ten_resize_cycles(cx: &mut TestAppContext) {
    let (handle, view, _) = mount(cx);
    let mut visual = gpui_kit::VisualTestContext::from_window(handle, cx);
    for collapsed in [false, true] {
        cx.update_window(handle, |_, _, cx| {
            view.update(cx, |v, _| {
                v.sidebar_visible = !collapsed;
                v.preferences.outline_width = 260.;
            })
        })
        .unwrap();
        for _ in 0..10 {
            for width in [800., 1024., 1280., 1440.] {
                cx.simulate_window_resize(handle, size(px(width), px(850.)));
                cx.update_window(handle, |_, window, cx| {
                    window.render_frame(cx);
                })
                .unwrap();
                let outline = visual.debug_bounds("notebook-outline");
                if width < 1024. || collapsed {
                    assert!(outline.is_none(), "width={width}, collapsed={collapsed}");
                } else {
                    let bounds = outline.expect("Wide Notebook must show the outline");
                    assert!(
                        bounds.size.width >= px(180.) && bounds.size.width <= px(300.),
                        "{:?}",
                        bounds.size
                    );
                    assert!(px(width) - bounds.size.width >= px(560.));
                }
            }
        }
    }
    for surface in [
        "History",
        "Checkpoints",
        "Settings",
        "Diagnostics",
        "Runtimes",
        "Help",
    ] {
        cx.update_window(handle, |_, window, cx| {
            view.update(cx, |v, _| {
                v.surface = surface.into();
                v.sidebar_visible = true;
            });
            window.render_frame(cx);
        })
        .unwrap();
        assert!(
            visual.debug_bounds("notebook-outline").is_none(),
            "{surface}"
        );
    }
}

#[gpui_kit::test]
fn checkpoint_commands_work_from_checkpoint_page_and_reject_unknown_runtime(
    cx: &mut TestAppContext,
) {
    let (handle, view, commands) = mount(cx);
    cx.update_window(handle, |_, window, cx| {
        view.update(cx, |v, cx| {
            v.surface = "Checkpoints".into();
            v.run_command("checkpoint", window, cx);
            v.tabs[0].runtime["connection_lost"] = json!(true);
            v.run_command("checkpoint", window, cx);
        });
    })
    .unwrap();
    assert_eq!(
        commands
            .try_iter()
            .filter(|c| matches!(c, Command::Runtime { method, .. } if method == "checkpoint"))
            .count(),
        1
    );
}

#[gpui_kit::test]
fn menu_blocks_background_execution_and_escape_returns_focus(cx: &mut TestAppContext) {
    let (handle, view, commands) = mount(cx);
    cx.update_window(handle, |_, window, cx| {
        view.update(cx, |v, cx| {
            v.tabs[0].model.selected = Some("first".into());
            v.focus.focus(window, cx);
            v.return_focus = Some(v.focus.clone());
            v.app_menu = true;
            v.dialog_focus.focus(window, cx);
        });
        window.render_frame(cx);
        window.press("ctrl-enter", cx);
        window.press("escape", cx);
        assert!(!view.read(cx).app_menu);
        assert!(view.read(cx).focus.is_focused(window));
    })
    .unwrap();
    assert!(
        !commands
            .try_iter()
            .any(|c| matches!(c, Command::Run { .. }))
    );
}

#[gpui_kit::test]
fn contextual_commands_preserve_ids_and_reject_irrelevant_or_unsaved_actions(
    cx: &mut TestAppContext,
) {
    let (handle, view, commands) = mount(cx);
    cx.update_window(handle, |_, window, cx| {
        view.update(cx, |v, cx| {
            v.tabs[0].model.selected = Some("first".into());
            v.run_command("add-markdown", window, cx);
            v.run_command("duplicate-cell", window, cx);
            assert!(v.command_reason("move-up").is_some());
            v.run_command("move-up", window, cx);
            v.tabs[0].model.cells.get_mut("first").unwrap().disk.kind = "markdown".into();
            assert!(v.command_reason("run-selected").is_some());
            v.run_command("run-selected", window, cx);
            v.tabs[0].model.cells.get_mut("first").unwrap().save =
                kernelledger_models::SaveState::Dirty;
            assert!(v.command_reason("delete-cell").is_some());
            v.run_command("delete-cell", window, cx);
            assert!(v.confirmation.is_none());
        })
    })
    .unwrap();
    let sent = commands.try_iter().collect::<Vec<_>>();
    assert_eq!(sent.len(), 2);
    assert!(
        matches!(&sent[0],Command::Mutate(v) if v["action"]=="add" && v["type"]=="markdown" && v["index"]==1 && v["new_id"].is_string())
    );
    assert!(
        matches!(&sent[1],Command::Mutate(v) if v["action"]=="duplicate" && v["cell_id"]=="first" && v["manifest_revision"]=="test")
    );
}

#[gpui_kit::test]
fn history_filters_validate_dates_and_keep_existing_text_and_outcome(cx: &mut TestAppContext) {
    let (handle, view, commands) = mount(cx);
    cx.update_window(handle, |_, window, cx| {
        view.update(cx, |v, cx| {
            v.surface = "History".into();
            v.history_owner = Some(0);
            v.history_query
                .update(cx, |s, cx| s.set_value("outcome:error total", window, cx));
            v.tabs[0]
                .inspections
                .insert("History".into(), json!({"items":[],"cursor":null}));
        });
        window.render_frame(cx);
        view.update(cx, |v, cx| {
            v.history_controls["cell"].update(cx, |s, cx| s.set_value("second", window, cx));
            v.history_controls["after"].update(cx, |s, cx| s.set_value("2026-02-30", window, cx));
        });
        window.render_frame(cx);
        window.click("apply-history-filters", cx);
        assert!(commands.try_recv().is_err());
        view.update(cx, |v, cx| {
            v.history_controls["after"].update(cx, |s, cx| s.set_value("2026-02-01", window, cx))
        });
        window.render_frame(cx);
        window.click("apply-history-filters", cx);
        let query = view.read(cx).history_query.read(cx).value().to_string();
        for token in ["outcome:error", "total", "cell:second", "after:2026-02-01"] {
            assert!(query.contains(token));
        }
    })
    .unwrap();
    assert!(
        matches!(commands.try_recv().unwrap(),Command::Inspect{method,fields} if method=="executions.list" && fields.get("search_all").is_none())
    );
}

#[gpui_kit::test]
fn history_cursor_keeps_session_scope_and_return_to_cell_restores_editor(cx: &mut TestAppContext) {
    let (handle, view, commands) = mount(cx);
    cx.update_window(handle,|_,window,cx|{
        view.update(cx,|v,cx|{
            v.surface="History".into();v.history_owner=Some(0);
            v.tabs[0].inspection_method.insert("History".into(),"executions.list".into());
            v.tabs[0].inspections.insert("History".into(),json!({"items":[],"cursor":"next","_page_request":{"session_id":"saved-session"}}));
            v.editor(0,"first",window,cx);
        });window.render_frame(cx);window.click("history-next-page",cx);
        view.update(cx,|v,_|{v.tabs[0].inspections.insert("History".into(),json!({"exec_id":"recorded","cell_id":"first","source":"print(42)","status":"ok","outputs":[]}));});
        window.render_frame(cx);window.click("return-to-cell",cx);
        assert_eq!(view.read(cx).surface,"Notebook");assert_eq!(view.read(cx).tabs[0].model.selected.as_deref(),Some("first"));
        assert!(view.read(cx).tabs[0].editors["first"].read(cx).focus_handle(cx).is_focused(window));
    }).unwrap();
    assert!(
        matches!(commands.try_recv().unwrap(),Command::Inspect{method,fields} if method=="executions.list" && fields["session_id"]=="saved-session" && fields["cursor"]=="next" && fields.get("search_all").is_none())
    );
}

#[test]
fn caption_gutter_collapses_export_before_settings_at_compact_200_percent() {
    let collapse = super::shell::header_collapse(800., 2., false);
    assert!(
        collapse.export && collapse.runtime && !collapse.switcher && !collapse.settings,
        "{collapse:?}"
    );
    assert!(super::shell::header_fits(800., 2., false, collapse));
    let comfortable = super::shell::header_collapse(800., 2., true);
    assert!(comfortable.switcher && comfortable.runtime && comfortable.export);
    assert!(super::shell::header_fits(800., 2., true, comfortable));
    assert!(super::shell::header_collapse(800., 1., false).export);
    assert!(!super::shell::header_collapse(1440., 1., false).export);
}

#[gpui_kit::test]
fn scaled_chrome_keeps_core_actions_inside_the_client_at_every_supported_width(
    cx: &mut TestAppContext,
) {
    let (handle, view, _) = mount(cx);
    let mut visual = gpui_kit::VisualTestContext::from_window(handle, cx);
    for scale in [1., 1.25, 1.5, 2.] {
        for comfortable in [false, true] {
            for width in [800., 1024., 1280., 1440.] {
                cx.simulate_window_resize(handle, size(px(width), px(600.)));
                cx.update_window(handle, |_, window, cx| {
                    view.update(cx, |v, _| {
                        v.preferences.ui_scale = scale;
                        v.preferences.comfortable = comfortable;
                    });
                    window.render_frame(cx);
                })
                .unwrap();
                let collapse = super::shell::header_collapse(width, scale, comfortable);
                let mut actions = vec![
                    "Notebook",
                    "History",
                    "Checkpoints",
                    "application-menu",
                    "run-all",
                    "outline-toggle",
                ];
                if !collapse.settings {
                    actions.push("settings-button");
                }
                if !collapse.runtime {
                    actions.push("runtime-menu");
                }
                if !collapse.switcher {
                    actions.push("notebook-switcher");
                }
                if !collapse.export {
                    actions.push("export-notebook");
                }
                let tight = width < 1280. * scale && (scale > 1. || comfortable);
                if !tight {
                    actions.push("palette-trigger");
                }
                for name in &actions {
                    let bounds = visual.debug_bounds(name).unwrap_or_else(|| {
                        panic!("Missing {name} at scale={scale}, width={width}, comfortable={comfortable}")
                    });
                    let right = bounds.origin.x + bounds.size.width;
                    assert!(
                        bounds.origin.x >= px(0.)
                            && bounds.origin.y >= px(0.)
                            && right <= px(width - super::shell::CAPTION_GUTTER)
                            && bounds.origin.y + bounds.size.height <= px(600.),
                        "{name}: scale={scale}, width={width}, comfortable={comfortable}, bounds={bounds:?}"
                    );
                }
                let header = visual.debug_bounds("workspace-header").unwrap();
                let center = header.origin.y + header.size.height / 2.;
                for name in &actions {
                    let bounds = visual.debug_bounds(name).unwrap();
                    let delta = (bounds.origin.y + bounds.size.height / 2. - center).abs();
                    assert!(
                        delta <= px(1.),
                        "{name}: vertical center differs by {delta:?}, scale={scale}, width={width}, comfortable={comfortable}"
                    );
                }
            }
        }
    }
}

#[gpui_kit::test]
fn disabled_titlebar_action_stays_centered_and_runtime_actions_live_in_menu(
    cx: &mut TestAppContext,
) {
    let (handle, view, commands) = mount(cx);
    cx.simulate_window_resize(handle, size(px(1280.), px(850.)));
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.click("run-all", cx);
        window.click("runtime-menu", cx);
        assert!(view.read(cx).runtime_menu);
        window.render_frame(cx);
        window.click("interrupt", cx);
        assert!(view.read(cx).confirmation.is_none());
        window.click("restart", cx);
        assert_eq!(view.read(cx).confirmation.as_ref().unwrap().0, "restart");
        view.update(cx, |v, _| {
            v.confirmation = None;
            v.tabs[0].runtime["connection_lost"] = json!(true);
        });
        window.render_frame(cx);
        window.click("run-all", cx);
    })
    .unwrap();
    let runs: Vec<_> = commands
        .try_iter()
        .filter_map(|c| match c {
            Command::Run { cell_ids, .. } => Some(cell_ids),
            _ => None,
        })
        .collect();
    assert_eq!(runs, vec![vec!["first".to_string(), "second".to_string()]]);
    let mut visual = gpui_kit::VisualTestContext::from_window(handle, cx);
    let button = visual.debug_bounds("run-all").unwrap();
    let header = visual.debug_bounds("workspace-header").unwrap();
    assert!(
        (button.origin.y + button.size.height / 2. - header.origin.y - header.size.height / 2.)
            .abs()
            <= px(1.)
    );
}

#[gpui_kit::test]
fn output_tools_are_in_cell_header_and_outputs_use_the_full_reading_width(cx: &mut TestAppContext) {
    let (handle, view, _) = mount(cx);
    cx.simulate_window_resize(handle, size(px(1440.), px(850.)));
    cx.update_window(handle, |_, window, cx| {
        view.update(cx, |v, _| {
            v.sidebar_visible = false;
            v.tabs[0].model.selected = Some("first".into());
            let cell = v.tabs[0].model.cells.get_mut("first").unwrap();
            cell.ordinal = Some(1);
            cell.output = vec![json!({"kind":"stdout", "text":"hello"})];
        });
        window.render_frame(cx);
        window.click(
            (
                gpui_kit::ElementId::from(gpui_kit::SharedString::from("notebook-0-first")),
                "output-tools-toggle",
            ),
            cx,
        );
        window.render_frame(cx);
        window.click(
            (
                gpui_kit::ElementId::from(gpui_kit::SharedString::from("first-0")),
                "find",
            ),
            cx,
        );
        window.render_frame(cx);
    })
    .unwrap();
    let mut visual = gpui_kit::VisualTestContext::from_window(handle, cx);
    let frame = visual.debug_bounds("cell-frame-first").unwrap();
    let source = visual.debug_bounds("cell-source-first").unwrap();
    let output = visual.debug_bounds("output-block-first-0").unwrap();
    assert!(frame.size.width <= px(1024.));
    assert!(
        output.size.width >= source.size.width - px(40.),
        "Output lost width: source={source:?}, output={output:?}"
    );
    cx.update_window(handle, |_, window, cx| {
        view.update(cx, |v, _| {
            v.tabs[0].model.cells.get_mut("first").unwrap().output = vec![json!({
                "kind":"display", "bundle":{"text/html":{"rows":[["Column"],["Value"]]}}
            })];
            v.tabs[0].list.splice(0..1, 1);
        });
        window.render_frame(cx);
    })
    .unwrap();
    let wide_frame = visual.debug_bounds("cell-frame-first").unwrap();
    let source = visual.debug_bounds("cell-source-first").unwrap();
    assert!(wide_frame.size.width > frame.size.width);
    assert!(source.size.width <= px(1024.));
}

#[gpui_kit::test]
fn comfortable_actions_have_a_full_touch_target(cx: &mut TestAppContext) {
    let (handle, view, _) = mount(cx);
    cx.update_window(handle, |_, window, cx| {
        view.update(cx, |v, _| v.preferences.comfortable = true);
        window.render_frame(cx);
    })
    .unwrap();
    let mut visual = gpui_kit::VisualTestContext::from_window(handle, cx);
    for name in [
        "run-all",
        "runtime-menu",
        "outline-toggle",
        "application-menu",
    ] {
        let bounds = visual.debug_bounds(name).unwrap();
        assert!(bounds.size.height >= px(44.) && bounds.size.width >= px(44.));
    }
}

#[gpui_kit::test]
fn bundle_review_inspects_without_running_and_keeps_source_conflicts(cx: &mut TestAppContext) {
    let (handle, view, commands) = mount(cx);
    cx.update(|cx| view.update(cx, |v, _| v.tick = gpui_kit::Task::ready(())));
    cx.update_window(handle, |_, window, cx| {
        view.update(cx, |v, cx| {
            v.tabs[0].model.source_state = "conflict_markers".into();
            v.tabs[0].model.cells.get_mut("first").unwrap().save =
                kernelledger_models::SaveState::Conflict;
            v.run_command("inspect-bundle", window, cx);
        });
    })
    .unwrap();
    assert!(cx.did_prompt_for_paths());
    cx.simulate_path_prompt_response(|_| Some(vec![PathBuf::from("foreign.zip")]));
    cx.executor().run_until_parked();
    assert!(matches!(
        commands.try_recv().unwrap(),
        Command::Inspect { method, fields }
            if method == "bundle.inspect" && fields["path"] == "foreign.zip"
    ));
    assert!(commands.try_recv().is_err());
    cx.update_window(handle, |_, window, cx| {
        assert_eq!(view.read(cx).surface, "Diagnostics");
        view.update(cx, |v, _| {
            v.tabs[0].inspections.insert(
                "Bundle review".into(),
                json!({
                    "path": "foreign.zip",
                    "pending": false,
                    "schema": 1,
                    "session_ids": ["s1"],
                    "checkpoint_ids": [],
                    "hash_mismatches": ["ledger.jsonl"]
                }),
            );
        });
        window.render_frame(cx);
        window.click("import-bundle-history", cx);
        assert!(view.read(cx).confirmation.is_none());
        view.update(cx, |v, _| {
            v.tabs[0].inspections.insert(
                "Bundle review".into(),
                json!({
                    "path": "foreign.zip",
                    "pending": false,
                    "schema": 1,
                    "session_ids": ["s1"],
                    "checkpoint_ids": ["c1"],
                    "hash_mismatches": []
                }),
            );
        });
        window.render_frame(cx);
        window.click("import-bundle-history", cx);
        assert_eq!(
            view.read(cx).confirmation.as_ref().unwrap().0,
            "bundle_import"
        );
        assert_eq!(
            view.read(cx).confirmation.as_ref().unwrap().1["trust"],
            false
        );
        window.render_frame(cx);
        window.click("proceed", cx);
        assert!(view.read(cx).confirmation.is_some());
        window.click("bundle-trust", cx);
        window.click("proceed", cx);
        assert!(view.read(cx).confirmation.is_none());
        assert_eq!(view.read(cx).tabs[0].model.cells["first"].text, "print(42)");
        assert!(matches!(
            view.read(cx).tabs[0].model.cells["first"].save,
            kernelledger_models::SaveState::Conflict
        ));
        assert_eq!(view.read(cx).tabs[0].model.source_state, "conflict_markers");
    })
    .unwrap();
    assert!(matches!(
        commands.try_recv().unwrap(),
        Command::Runtime { method, fields }
            if method == "bundle_import"
                && fields["path"] == "foreign.zip"
                && fields.get("trust").is_none()
    ));
}

#[gpui_kit::test]
fn large_output_paging_requests_the_next_recorded_window(cx: &mut TestAppContext) {
    let (handle, view, commands) = mount(cx);
    cx.update_window(handle, |_, window, cx| {
        view.update(cx, |v, _| {
            let cell = v.tabs[0].model.cells.get_mut("first").unwrap();
            cell.exec_id = Some("exec-1".into());
            cell.output = vec![json!({
                "kind": "stdout",
                "blob": "abc",
                "text": "hello",
                "page": {"offset": 0, "end": 5, "total": 100}
            })];
            v.tabs[0].list.splice(0..1, 1);
        });
        window.render_frame(cx);
        window.click(
            (
                gpui_kit::ElementId::from(gpui_kit::SharedString::from("first-0")),
                "output-next",
            ),
            cx,
        );
    })
    .unwrap();
    let page = commands
        .try_iter()
        .find(|command| matches!(command, Command::OutputPage { .. }));
    match page {
        Some(Command::OutputPage {
            cell_id,
            exec_id,
            index,
            slot,
            digest,
            offset,
            length,
        }) => {
            assert_eq!(cell_id, "first");
            assert_eq!(exec_id, "exec-1");
            assert_eq!(index, 0);
            assert!(slot.is_empty());
            assert_eq!(digest, "abc");
            assert_eq!(offset, 5);
            assert_eq!(length, 5);
        }
        other => panic!("expected an output page request, got {other:?}"),
    }
}

#[gpui_kit::test]
fn attachment_preview_requests_the_first_text_window(cx: &mut TestAppContext) {
    let (handle, view, commands) = mount(cx);
    cx.update_window(handle, |_, window, cx| {
        view.update(cx, |v, _| {
            let cell = v.tabs[0].model.cells.get_mut("first").unwrap();
            cell.disk
                .metadata
                .insert("attachments".into(), json!(["notes.txt"]));
            cell.disk.metadata.insert(
                "attachment_mimes".into(),
                json!({"notes.txt": "text/plain"}),
            );
            v.tabs[0].list.splice(0..1, 1);
        });
        window.render_frame(cx);
    })
    .unwrap();
    let previews = commands
        .try_iter()
        .filter(|command| matches!(command, Command::PreviewAttachment { .. }))
        .collect::<Vec<_>>();
    assert_eq!(previews.len(), 1);
    match &previews[0] {
        Command::PreviewAttachment {
            cell_id,
            name,
            offset,
            length,
        } => {
            assert_eq!(cell_id, "first");
            assert_eq!(name, "notes.txt");
            assert_eq!(*offset, 0);
            assert_eq!(*length, kernelledger_client::output::VIEW_TEXT_LIMIT);
        }
        other => panic!("expected an attachment preview, got {other:?}"),
    }
}
