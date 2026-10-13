use super::{
    Palette, Tab, Workspace, confirmation_copy, history_matches, inspection_surface,
    runtime_action_available,
};
use gpui_kit::{
    AnyWindowHandle, AppContext, Bounds, Entity, Focusable, ListAlignment, ListState, Point,
    WindowBounds, WindowOptions, px, size,
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
                    uninitialized: false, selections: BTreeMap::new(), drafts: vec![], history_filter: String::new(), checkpoint_notice: None, error: None });
                view
            })
        }).unwrap()
    });
    (window, view, commands)
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
fn settings_segments_change_preferences_through_native_buttons(cx: &mut TestAppContext) {
    let (handle, view, _) = mount(cx);
    cx.simulate_window_resize(handle, size(px(1280.), px(900.)));
    cx.update_window(handle, |_, window, cx| {
        view.update(cx, |v, _| v.surface = "Settings".into());
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
        matches!(commands.try_recv().unwrap(),Command::Inspect{method,fields} if method=="executions.list" && fields["search_all"]==true)
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
                let overflow = comfortable && width < 560. * scale;
                let mut actions = vec![
                    "Notebook",
                    "History",
                    "Checkpoints",
                    "application-menu",
                    "run-all",
                    "outline-toggle",
                ];
                if !overflow {
                    actions.extend(["runtime-menu", "notebook-switcher"]);
                }
                for name in &actions {
                    let bounds = visual.debug_bounds(name).unwrap_or_else(|| {
                        panic!("Missing {name} at scale={scale}, width={width}")
                    });
                    assert!(
                        bounds.origin.x >= px(0.)
                            && bounds.origin.y >= px(0.)
                            && bounds.origin.x + bounds.size.width <= px(width)
                            && bounds.origin.y + bounds.size.height <= px(600.),
                        "{name}: scale={scale}, width={width}, bounds={bounds:?}"
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
