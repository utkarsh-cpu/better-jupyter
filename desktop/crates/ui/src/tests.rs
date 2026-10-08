use super::{
    Palette, Tab, Workspace, confirmation_copy, history_matches, inspection_surface,
    runtime_action_available,
};
use gpui_kit::{
    AnyWindowHandle, AppContext, Bounds, Entity, ListAlignment, ListState, Point, WindowBounds,
    WindowOptions, px, size,
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
                view.tabs.push(Tab { model, service: NotebookService { commands: send, events }, list: ListState::new(2, ListAlignment::Top, px(200.)), editors: BTreeMap::new(), subscriptions: vec![], runtime: json!({"worker_state":"idle"}), operations: BTreeMap::new(), inspections: BTreeMap::new(), inspection_method: BTreeMap::new(), scrolls: BTreeMap::new(), parents: vec![], raw_details: false, reveal: None,
                    uninitialized: false, selections: BTreeMap::new(), drafts: vec![], history_filter: String::new(), checkpoint_notice: None, error: None });
                view
            })
        }).unwrap()
    });
    (window, view, commands)
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
        window.click(("delete", 0usize), cx);
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
                (t.primary_foreground, t.primary),
            ] {
                assert!(
                    contrast(a, b) >= 4.5,
                    "dark={dark}, high={high}: {}",
                    contrast(a, b)
                );
            }
            assert!(contrast(t.ring, t.background) >= 3.0);
        }
    })
    .unwrap();
}

#[gpui_kit::test]
fn outline_is_contextual_and_collapse_survives_ten_resize_cycles(cx: &mut TestAppContext) {
    let (handle, view, _) = mount(cx);
    let mut visual = gpui_kit::VisualTestContext::from_window(handle, cx);
    for collapsed in [false, true] {
        cx.update_window(handle, |_, _, cx| view.update(cx, |v, _| {
            v.sidebar_visible = !collapsed;
            v.preferences.outline_width = 260.;
        })).unwrap();
        for _ in 0..10 {
            for width in [800., 1024., 1280., 1440.] {
                cx.simulate_window_resize(handle, size(px(width), px(850.)));
                cx.update_window(handle, |_, window, cx| { window.render_frame(cx); }).unwrap();
                let outline = visual.debug_bounds("notebook-outline");
                if width < 1024. || collapsed { assert!(outline.is_none(), "width={width}, collapsed={collapsed}"); }
                else {
                    let bounds = outline.expect("Wide Notebook must show the outline");
                    assert!(bounds.size.width >= px(180.) && bounds.size.width <= px(300.), "{:?}", bounds.size);
                    assert!(px(width) - bounds.size.width >= px(560.));
                }
            }
        }
    }
    for surface in ["History", "Checkpoints", "Settings", "Diagnostics", "Runtimes", "Help"] {
        cx.update_window(handle, |_, window, cx| {
            view.update(cx, |v, _| { v.surface = surface.into(); v.sidebar_visible = true; });
            window.render_frame(cx);
        }).unwrap();
        assert!(visual.debug_bounds("notebook-outline").is_none(), "{surface}");
    }
}

#[gpui_kit::test]
fn checkpoint_commands_work_from_checkpoint_page_and_reject_unknown_runtime(cx: &mut TestAppContext) {
    let (handle, view, commands) = mount(cx);
    cx.update_window(handle, |_, window, cx| {
        view.update(cx, |v, cx| {
            v.surface = "Checkpoints".into();
            v.run_command("checkpoint", window, cx);
            v.tabs[0].runtime["connection_lost"] = json!(true);
            v.run_command("checkpoint", window, cx);
        });
    }).unwrap();
    assert_eq!(commands.try_iter().filter(|c| matches!(c, Command::Runtime { method, .. } if method == "checkpoint")).count(), 1);
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
    }).unwrap();
    assert!(!commands.try_iter().any(|c| matches!(c, Command::Run { .. })));
}
