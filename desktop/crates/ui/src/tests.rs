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

#[test]
fn assistant_provider_save_preserves_unedited_options() {
    let mut config = json!({"providers":{"openai-compatible":{"base_url":"old","structured_output":true,"allow_http_host":"lan.test","enable_thinking":false}}});
    super::assistant::merge_provider(
        &mut config,
        "openai-compatible",
        "http://127.0.0.1:13305/api/v1".into(),
    );
    assert_eq!(
        config["providers"]["openai-compatible"]["structured_output"],
        true
    );
    assert_eq!(
        config["providers"]["openai-compatible"]["enable_thinking"],
        false
    );
    assert_eq!(
        config["providers"]["openai-compatible"]["allow_http_host"],
        "lan.test"
    );
    assert_eq!(
        config["providers"]["openai-compatible"]["base_url"],
        "http://127.0.0.1:13305/api/v1"
    );
}

#[gpui_kit::test]
fn assistant_conversation_load_replaces_answer_proposals_and_task(cx: &mut TestAppContext) {
    use kernelledger_client::service::Event;
    let (handle, view, commands) = mount(cx);
    let (send, receive) = mpsc::channel();
    cx.update_window(handle, |_, window, cx| {
        view.update(cx, |v,_| {
            v.tick=gpui_kit::Task::ready(());
            v.tabs[0].service.events=receive;
            v.tabs[0].assistant.answer=json!({"answer":"Old conversation"});
            v.tabs[0].assistant.proposals=vec![json!({"id":"old"})];
        });
        send.send(Event::AssistantDone{method:"conversation.get".into(),value:json!({"conversation_id":"new","requests":[{"id":"new-request","task":"refactor","warnings":[{"code":"invalid_target","message":"Optional edit rejected"}]}],"messages":[{"role":"assistant","text":"New answer","request_id":"new-request","citations":[]}],"proposals":[{"id":"new-proposal","state":"applied"}]})}).unwrap();
        view.update(cx, |v,cx|v.drain(window,cx));
        let ai=&view.read(cx).tabs[0].assistant;
        assert_eq!(ai.answer["answer"],"New answer");
        assert_eq!(ai.answer["warnings"][0]["code"],"invalid_target");
        assert!(super::assistant::answer_report(&ai.answer).contains("Optional edit rejected"));
        assert_eq!(ai.proposals[0]["id"],"new-proposal");
        assert_eq!(ai.task,"refactor");
        assert_eq!(ai.conversation.as_deref(),Some("new"));
        assert!(ai.receipt.is_null());
    }).unwrap();
    assert!(
        !commands
            .try_iter()
            .any(|c| matches!(c, Command::AssistantApply(_)))
    );
}

#[test]
fn assistant_receipt_and_export_present_real_text_and_honest_citations() {
    let text = super::assistant::receipt_text(
        &json!({"kind":"output","text":json!({"outputs":[{"kind":"error","ename":"ZeroDivisionError","evalue":"division by zero","traceback":["line 1\n","1 / 0\n"]}]}).to_string()}),
    );
    assert!(text.contains("ZeroDivisionError: division by zero"));
    assert!(text.contains("line 1\n1 / 0\n"));
    assert!(!text.contains("\\n"));
    let report = super::assistant::answer_report(
        &json!({"answer":"Reviewed answer", "citations":[{"kind":"cell","id":"first","verified":true,"in_context":false}]}),
    );
    assert!(report.starts_with("Reviewed answer"));
    assert!(report.contains("cell: first"));
    assert!(report.contains("in approved context: false"));
    assert!(report.contains("not fact checking"));
}

#[gpui_kit::test]
fn assistant_typed_failure_consumes_receipt_and_rebuilds_without_sending(cx: &mut TestAppContext) {
    use super::assistant::{Connection, ReceiptState};
    use kernelledger_client::service::Event;
    let (handle, view, commands) = mount(cx);
    let (send, receive) = mpsc::channel();
    cx.update_window(handle, |_, window, cx| {
        view.update(cx, |v, _| {
            v.tick = gpui_kit::Task::ready(());
            v.tabs[0].service.events = receive;
            let ai = &mut v.tabs[0].assistant;
            ai.open = true;
            ai.busy = true;
            ai.connection = Connection::Ready;
            ai.settings =
                json!({"policy":"allowed","settings":{"provider":"fake","model":"fixture"}});
            ai.task = "explain".into();
            ai.last_question = "Explain recorded failure".into();
            ai.selected_cells = vec!["first".into()];
            ai.exec_ids = vec!["execution-1".into()];
            ai.receipt = json!({"hash":"spent"});
            ai.receipt_state = ReceiptState::Sending;
        });
        send.send(Event::AssistantDone {
            method: "respond".into(),
            value: json!({"assistant_error":{"code":"stale","message":"Proposal base mismatch"}}),
        })
        .unwrap();
        view.update(cx, |v, cx| v.drain(window, cx));
        assert!(view.read(cx).tabs[0].assistant.error);
        assert!(view.read(cx).tabs[0].assistant.receipt.is_null());
        assert_eq!(
            view.read(cx).tabs[0].assistant.receipt_state,
            ReceiptState::Consumed
        );
        send.send(Event::AssistantDone {
            method: "status".into(),
            value: json!({"usage":{"used_today":12345}}),
        })
        .unwrap();
        view.update(cx, |v, cx| v.drain(window, cx));
        assert!(
            view.read(cx).tabs[0].assistant.error,
            "A background refresh must retain failure severity"
        );
        assert_eq!(
            view.read(cx).tabs[0].assistant.status,
            "Proposal base mismatch"
        );
        window.render_frame(cx);
        window.click("assistant-rebuild", cx);
    })
    .unwrap();
    let sent = commands.try_iter().collect::<Vec<_>>();
    assert!(sent.iter().any(|c|matches!(c,Command::Assistant{method,fields} if method=="context.preview" && fields["question"]=="Explain recorded failure" && fields["task"]=="explain")));
    assert!(!sent.iter().any(|c|matches!(c,Command::Assistant{method,..} if method=="respond" || method=="proposal.resolve")));
}

#[gpui_kit::test]
fn console_editor_newline_and_shortcuts_preserve_notebook_source(cx: &mut TestAppContext) {
    let (handle, view, commands) = mount(cx);
    cx.update_window(handle, |_, window, cx| {
        view.update(cx, |v, cx| {
            v.surface = "Console".into();
            v.tabs[0].model.selected = Some("first".into());
            v.console_editor(window, cx).update(cx, |editor, cx| {
                editor.set_value("value = 42", window, cx);
                editor.focus(window, cx);
            });
        });
        window.render_frame(cx);
        window.press("enter", cx);
        window.press("ctrl-enter", cx);
        window.press("shift-enter", cx);
        window.press("escape", cx);
        assert!(
            view.read(cx)
                .console_editors
                .values()
                .next()
                .unwrap()
                .read(cx)
                .value()
                .contains('\n')
        );
        assert_eq!(view.read(cx).tabs[0].model.cells["first"].text, "print(42)");
    })
    .unwrap();
    assert!(
        !commands
            .try_iter()
            .any(|c| matches!(c, Command::Run { .. } | Command::Console { .. }))
    );
}

#[gpui_kit::test]
fn escape_dismisses_completion_before_command_mode_and_preserves_editing(cx: &mut TestAppContext) {
    let (handle, view, commands) = mount(cx);
    cx.update_window(handle, |_, window, cx| {
        view.update(cx, |v, cx| {
            v.tick = gpui_kit::Task::ready(());
            v.tabs[0].model.selected = Some("first".into());
            v.editor(0, "first", window, cx).update(cx, |editor, cx| {
                editor.set_value("answ", window, cx);
                editor.present_completion_items(
                    0,
                    "answ",
                    vec![lsp_types::CompletionItem {
                        label: "answer".into(),
                        ..Default::default()
                    }],
                    cx,
                );
                editor.focus(window, cx);
            });
        });
        window.render_frame(cx);
        window.render_frame(cx);
        let editor = view.read(cx).tabs[0].editors["first"].clone();
        assert!(editor.read(cx).completion_menu_state().open);
        window.press("escape", cx);
        assert!(!editor.read(cx).completion_menu_state().open);
        assert!(editor.read(cx).focus_handle(cx).is_focused(window));
        window.press("enter", cx);
        assert!(editor.read(cx).value().contains('\n'));
        editor.update(cx, |editor, cx| {
            editor.present_hover(
                0..4,
                lsp_types::Hover {
                    contents: lsp_types::HoverContents::Scalar(lsp_types::MarkedString::String(
                        "Live help".into(),
                    )),
                    range: None,
                },
                cx,
            );
        });
        window.render_frame(cx);
        window.press("escape", cx);
        assert!(editor.read(cx).hover_popover().is_none());
        assert!(editor.read(cx).focus_handle(cx).is_focused(window));
        window.press("escape", cx);
        assert!(view.read(cx).focus.is_focused(window));
    })
    .unwrap();
    assert!(
        !commands
            .try_iter()
            .any(|c| matches!(c, Command::Run { .. } | Command::Console { .. }))
    );
}

fn output_action(id: &str, action: &'static str) -> (gpui_kit::ElementId, &'static str) {
    (
        gpui_kit::ElementId::from(gpui_kit::SharedString::from(id.to_owned())),
        action,
    )
}

fn html_display(html: &str) -> serde_json::Value {
    json!({"kind":"display","bundle":{"text/html":{"value": html}}})
}

fn html_js_kind_outputs() -> Vec<(&'static str, serde_json::Value, bool, bool)> {
    let pandas = "<style>#T th{color:red}</style><table id='T'><tr><th>A</th></tr><tr><td>1</td></tr></table>";
    let plotly = "<div><script src='https://cdn.plot.ly/plotly.min.js'></script><div class='plotly-graph-div'></div><script>Plotly.newPlot('p',[])</script></div>";
    let bokeh = "<script src='https://cdn.bokeh.org/bokeh.min.js'></script><div class='bk-root'></div><script>Bokeh.embed.embed_item({})</script>";
    let altair = "<div id='vis'></div><script src='https://cdn.jsdelivr.net/npm/vega@5'></script><script>vegaEmbed('#vis',{})</script>";
    let onclick = "<table><tr><td onclick='evil'>Safe</td></tr></table>";
    let ipython = "<h1>Hi</h1><script>console.log(1)</script>";
    let js_href = "<a href='javascript:alert(1)'>go</a>";
    let iframe = "<iframe src='about:blank'></iframe>";
    let object = "<object data='about:blank'></object>";
    let embed = "<embed src='about:blank'>";
    vec![
        (
            "pandas-table",
            json!({"kind":"display","bundle":{"text/html":{"value":pandas,"rows":[["A"],["1"]]}}}),
            false,
            true,
        ),
        (
            "table-onclick",
            json!({"kind":"display","bundle":{"text/html":{"value":onclick,"rows":[["Safe"]]}}}),
            true,
            true,
        ),
        ("plotly-cdn", html_display(plotly), true, true),
        (
            "plotly-png",
            json!({"kind":"display","bundle":{"image/png":{"local_path":"plot.png"},"text/html":{"value":plotly}}}),
            true,
            true,
        ),
        ("bokeh", html_display(bokeh), true, true),
        ("altair", html_display(altair), true, true),
        ("ipython-html", html_display(ipython), true, true),
        ("custom-page", html_display("<div>hello</div>"), true, true),
        ("iframe", html_display(iframe), true, true),
        ("object", html_display(object), true, true),
        ("embed", html_display(embed), true, true),
        ("javascript-href", html_display(js_href), true, true),
        (
            "oversize-plotly",
            json!({"kind":"display","bundle":{"text/html":{"value":plotly,"page":{"offset":0,"end":32,"total":65537}}}}),
            true,
            false,
        ),
    ]
}

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
                view.tabs.push(Tab { assistant: Default::default(), model, service: NotebookService { commands: send, events }, list: ListState::new(2, ListAlignment::Top, px(200.)), editors: BTreeMap::new(), subscriptions: vec![], runtime: json!({"worker_state":"idle"}), operations: BTreeMap::new(), inspections: BTreeMap::new(), inspection_method: BTreeMap::new(), scrolls: BTreeMap::new(), parents: vec![], raw_details: BTreeMap::new(), reveal: None,
                    debug_decorations: BTreeMap::new(), uninitialized: false, selections: BTreeMap::new(), drafts: vec![], history_filter: String::new(), history_applied: String::new(), checkpoint_notice: None, error: None, attachment_pages: BTreeMap::new(), variables_scroll: Default::default(), html_views: BTreeMap::new(), html_states: BTreeMap::new(), html_notes: BTreeMap::new() });
                view
            })
        }).unwrap()
    });
    (window, view, commands)
}

#[gpui_kit::test]
fn debug_gutter_and_navigation_bind_to_the_executed_revision(cx: &mut TestAppContext) {
    let (handle, view, commands) = mount(cx);
    let source = "first = 1\nsecond = 2\nthird = 3";
    let revision = kernelledger_client::hash(source.as_bytes());
    let file = format!("cells/first.py@{}", uuid::Uuid::new_v4());
    let location = kernelledger_client::recorded::Location::parse(&file, &revision, 3).unwrap();
    cx.update_window(handle, |_, window, cx| {
        view.update(cx, |v, _| {
            v.tick = gpui_kit::Task::ready(());
            v.tabs[0].model.apply_snapshot(serde_json::from_value(json!({"state":"ok","manifest_revision":"test","cells":[{"id":"first","type":"code","source":source,"revision":revision}]})).unwrap());
            v.tabs[0].editors.clear();
            v.tabs[0].debug_decorations.clear();
            v.tabs[0].list.splice(0..2, 1);
            v.tabs[0].runtime = json!({"worker_state":"idle", "interactive":{"features":["debugger"],"worker_generation":"g"},"debug":{"stack":[{"file":file,"source_revision":revision,"line":2}]}});
        });
        window.render_frame(cx);
        window.render_frame(cx);
        window.click("gutter-first-3", cx);
        view.update(cx, |v, cx| {
            assert_eq!(v.tabs[0].inspections["Breakpoints"]["breakpoints"][0]["source_revision"], revision);
            v.navigate_recorded_source(location.clone(), window, cx);
            assert_eq!(v.tabs[0].editors["first"].read(cx).cursor_position().line, 2);
            v.tabs[0].editors["first"].update(cx, |editor, cx| editor.set_value("changed = True", window, cx));
            v.navigate_recorded_source(location.clone(), window, cx);
            v.toggle_breakpoint_line("first", 2, &revision, cx);
        });
    }).unwrap();
    let sent = commands.try_iter().collect::<Vec<_>>();
    assert_eq!(sent.iter().filter(|c| matches!(c, Command::Interactive { method, .. } if method == "debug.breakpoints")).count(), 1);
    assert!(sent.iter().any(|c| matches!(c, Command::RecordedSource(value) if value.revision == revision && value.line == 3)));
    assert!(!sent.iter().any(|c| matches!(c, Command::Run { .. })));
}

#[gpui_kit::test]
fn media_save_requires_a_selected_path_and_cancellation_sends_nothing(cx: &mut TestAppContext) {
    let (handle, view, commands) = mount(cx);
    let digest = "a".repeat(64);
    cx.update_window(handle, |_, window, cx| {
        view.update(cx, |v, _| {
            v.tick = gpui_kit::Task::ready(());
            let cell = v.tabs[0].model.cells.get_mut("first").unwrap();
            cell.output = vec![json!({"kind":"display","bundle":{"audio/wav":{"blob":digest,"encoding":"binary"},"text/plain":{"value":"Recorded audio"}}})];
            v.tabs[0].list.splice(0..1, 1);
        });
        window.render_frame(cx);
        window.click("first-0-save-audio/wav", cx);
    }).unwrap();
    assert!(cx.did_prompt_for_new_path());
    cx.simulate_new_path_selection(|_| None);
    cx.executor().run_until_parked();
    assert!(commands.try_recv().is_err());
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.click("first-0-save-audio/wav", cx);
    })
    .unwrap();
    cx.simulate_new_path_selection(|_| Some(PathBuf::from("sound.wav")));
    cx.executor().run_until_parked();
    assert!(
        matches!(commands.try_recv().unwrap(), Command::SaveOutput { path, digest: value, mime } if path == std::path::Path::new("sound.wav") && value == digest && mime == "audio/wav")
    );
}

#[gpui_kit::test]
fn late_output_pages_and_images_cannot_replace_an_updated_display(cx: &mut TestAppContext) {
    use kernelledger_client::service::Event;
    let (handle, view, _) = mount(cx);
    let (events, receiver) = mpsc::channel();
    cx.update_window(handle, |_, window, cx| {
        view.update(cx, |v, _| {
            v.tick = gpui_kit::Task::ready(());
            v.tabs[0].service.events = receiver;
            let cell = v.tabs[0].model.cells.get_mut("first").unwrap();
            cell.exec_id = Some("exec".into());
            cell.output = vec![json!({"kind":"display","bundle":{"text/plain":{"blob":"new","value":"new value"},"image/png":{"blob":"new","local_path":"new.png"}}})];
            v.tabs[0].inspections.insert("History".into(),json!({"exec_id":"exec","outputs":[{"bundle":{"text/plain":{"blob":"old","value":"old value"},"image/png":{"blob":"old"}}}]}));
        });
        events.send(Event::OutputPage { cell_id:"first".into(),exec_id:"exec".into(),index:0,slot:"text/plain".into(),digest:"old".into(),text:"old page".into(),offset:0,end:8,total:8 }).unwrap();
        events.send(Event::ImageCached { cell_id:"first".into(),exec_id:"exec".into(),index:0,mime:"image/png".into(),digest:"old".into(),path:"old.png".into() }).unwrap();
        view.update(cx, |v,cx| v.drain(window,cx));
        let current = &view.read(cx).tabs[0];
        assert_eq!(current.model.cells["first"].output[0]["bundle"]["text/plain"]["value"],"new value");
        assert_eq!(current.model.cells["first"].output[0]["bundle"]["image/png"]["local_path"],"new.png");
        assert_eq!(current.inspections["History"]["outputs"][0]["bundle"]["text/plain"]["value"],"old page");
        assert_eq!(current.inspections["History"]["outputs"][0]["bundle"]["image/png"]["local_path"],"old.png");
    }).unwrap();
}

#[gpui_kit::test]
fn display_replay_limits_offer_an_explicit_reload_without_running(cx: &mut TestAppContext) {
    let (handle, view, commands) = mount(cx);
    cx.update_window(handle, |_, window, cx| {
        view.update(cx, |v, _| {
            v.tabs[0].inspections.insert(
                "Display updates".into(),
                json!({"note":"Catching up with recorded display updates"}),
            );
        });
        window.render_frame(cx);
        window.click("refresh-display-updates", cx);
    })
    .unwrap();
    assert!(matches!(commands.try_recv().unwrap(), Command::Refresh));
    assert!(commands.try_recv().is_err());
}

#[gpui_kit::test]
fn reveal_after_a_structural_reset_renders_the_unmeasured_target(cx: &mut TestAppContext) {
    let (handle, view, commands) = mount(cx);
    cx.update_window(handle, |_, window, cx| {
        view.update(cx, |v, cx| {
            v.tick = gpui_kit::Task::ready(());
            let tab = &mut v.tabs[0];
            tab.model.cells.clear();
            tab.model.order.clear();
            for index in 0..20 {
                let id = format!("deep-{index}");
                let source = kernelledger_models::SourceCell {
                    id: id.clone(),
                    kind: "code".into(),
                    source: vec!["# tall source"; 30].join("\n"),
                    revision: "a".repeat(64),
                    metadata: Default::default(),
                };
                tab.model.cells.insert(
                    id.clone(),
                    kernelledger_models::CellModel::from_source(source),
                );
                tab.model.order.push(id);
            }
            tab.editors.clear();
            tab.list.reset(20);
            tab.reveal = Some("deep-19".into());
            v.drain(window, cx);
        });
        window.render_frame(cx);
        window.render_frame(cx);
        assert!(view.read(cx).tabs[0].list.bounds_for_item(19).is_some());
        assert_eq!(
            view.read(cx).tabs[0].model.selected.as_deref(),
            Some("deep-19")
        );
        window.click(("run", 19usize), cx);
    })
    .unwrap();
    assert!(
        commands.try_iter().any(
            |command| matches!(command,Command::Run {cell_ids,..} if cell_ids==vec!["deep-19"])
        )
    );
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
                gpui_kit::PlatformInput::Scroll(gpui_kit::ScrollEvent {
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
            v.palette_active = v
                .palette_matches("checkpoint")
                .iter()
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
    let (section, id) = ("editor", "editor-size-24");
    let key = format!("Settings/{section}");
    let observe = |cx: &mut TestAppContext, height: f32| {
        cx.simulate_window_resize(handle, size(px(800.), px(height)));
        cx.update_window(handle, |_, window, cx| {
            view.update(cx, |v, _| {
                v.preferences.ui_scale = 2.;
                v.surface = "Settings".into();
                v.settings_section = section;
            });
            window.refresh();
            window.render_frame(cx);
            let viewport = view.read(cx).tabs[0]
                .scrolls
                .get(&format!("Settings/{section}"))
                .expect("each Settings section keeps a scroll handle")
                .bounds();
            let node = window.find(id);
            (viewport, node.bounds(), node.visible())
        })
        .unwrap()
    };
    // Size the window so the viewport's bottom edge cuts through the control.
    let (viewport, bounds, _) = observe(cx, 900.);
    let height = 900. - (viewport.bottom() - bounds.top() - bounds.size.height * 0.6) / px(1.);
    let (viewport, before, visible) = observe(cx, height);
    assert!(
        visible
            && before.center().y <= viewport.bottom()
            && before.bottom() > viewport.bottom() + px(1.),
        "{id} {before:?} must straddle the viewport bottom {viewport:?} at height {height}"
    );
    let offset_before = cx
        .update_window(handle, |_, _, cx| {
            view.read(cx).tabs[0].scrolls[&key].offset()
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
            view.read(cx).tabs[0].scrolls[&key].offset()
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
            view.read(cx).tabs[0].scrolls[&key].bounds()
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
        assert!(window.try_find("editor-size-16").is_none());
        window.click("settings-nav-editor", cx);
        assert_eq!(view.read(cx).settings_section, "editor");
        window.render_frame(cx);
        assert!(window.try_find("appearance-dark").is_none());
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
fn changing_destination_clears_menus_and_palette(cx: &mut TestAppContext) {
    let (handle, view, _) = mount(cx);
    cx.update_window(handle, |_, window, cx| {
        view.update(cx, |v, cx| {
            v.app_menu = true;
            v.export_menu = true;
            v.runtime_menu = true;
            v.palette = true;
            v.cell_menu = Some(("first".into(), 0));
            v.insert_menu = Some(0);
            v.select_surface("History", cx);
            assert!(!v.app_menu);
            assert!(!v.export_menu);
            assert!(!v.runtime_menu);
            assert!(!v.palette);
            assert!(v.cell_menu.is_none());
            assert!(v.insert_menu.is_none());
            assert_eq!(v.surface, "History");
        });
        window.render_frame(cx);
    })
    .unwrap();
}

#[gpui_kit::test]
fn application_menu_rows_span_the_panel_and_start_on_the_leading_edge(cx: &mut TestAppContext) {
    let (handle, view, _) = mount(cx);
    cx.simulate_window_resize(handle, size(px(1280.), px(850.)));
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.click("application-menu", cx);
        window.render_frame(cx);
        assert!(view.read(cx).app_menu);
    })
    .unwrap();
    let mut visual = gpui_kit::VisualTestContext::from_window(handle, cx);
    let menu = visual
        .debug_bounds("workspace-menu-content")
        .expect("application menu panel");
    let open = visual.debug_bounds("open").expect("Open notebook menu row");
    let help = visual.debug_bounds("Help").expect("Help menu row");
    assert!(
        open.origin.x <= menu.origin.x + px(16.),
        "menu row should start on the leading edge, open={open:?}, menu={menu:?}"
    );
    assert!(
        (open.origin.x - help.origin.x).abs() <= px(1.),
        "menu labels should share a leading edge, open={open:?}, help={help:?}"
    );
    assert!(
        open.size.width >= menu.size.width - px(24.),
        "menu row should span the panel, open={open:?}, menu={menu:?}"
    );
    assert!(
        visual.debug_bounds("close-menu").is_none(),
        "menus dismiss with Escape and click-away; they should not ship a Close menu row"
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

#[test]
fn history_filter_equivalence_and_date_validation() {
    use super::history::{history_filter_valid, same_filter};
    assert!(same_filter("outcome:error  total", "TOTAL outcome:error"));
    assert!(same_filter("from:2026-01-01", "after:2026-01-01"));
    assert!(!same_filter("outcome:error", ""));
    assert!(!same_filter("cell:UpperCell", "cell:uppercell"));
    assert!(same_filter("CELL:UpperCell", "cell:UpperCell"));
    assert!(history_filter_valid(""));
    assert!(history_filter_valid("after:2026-01-01 before:2026-01-01"));
    assert!(!history_filter_valid("after:2026-02-30"));
    assert!(!history_filter_valid("after:2026-1-1"));
    assert!(!history_filter_valid("after:2026-03-01 before:2026-02-01"));
}

#[gpui_kit::test]
fn history_pending_edits_never_describe_the_list_and_clear_resets_both(cx: &mut TestAppContext) {
    let (handle, view, commands) = mount(cx);
    let rows = json!({"items":[
        {"exec_id":"a","cell_id":"first","source":"print(42)","status":"ok","start":"2026-01-01T00:00:00Z","duration":0.5},
        {"exec_id":"b","cell_id":"second","source":"x = 1","status":"error","start":"2026-01-02T00:00:00Z"}
    ],"cursor":null});
    cx.update_window(handle, |_, window, cx| {
        view.update(cx, |v, _| {
            v.surface = "History".into();
            v.history_owner = Some(0);
            v.tabs[0].inspections.insert("History".into(), rows);
        });
        window.render_frame(cx);
        assert_eq!(view.read(cx).tabs[0].history_applied, "");
        assert!(
            window.try_find("apply-history-filters").is_none()
                && window.try_find("clear-history-filters").is_none(),
            "Apply and Clear stay hidden when draft matches the applied filter"
        );
        // Clicking an outcome segment applies immediately without waiting for Apply.
        window.click("outcome-error", cx);
        window.render_frame(cx);
        assert_eq!(view.read(cx).tabs[0].history_applied, "outcome:error");
        assert!(
            window.try_find("apply-history-filters").is_none()
                && window.try_find("clear-history-filters").is_none(),
            "Immediate outcome filter leaves no pending draft"
        );
        // Editing the text query waits for Apply and reveals Apply/Clear.
        view.update(cx, |v, cx| {
            v.history_query
                .update(cx, |s, cx| s.set_value("outcome:error print", window, cx));
        });
        window.render_frame(cx);
        assert!(
            window.try_find("apply-history-filters").is_some()
                && window.try_find("clear-history-filters").is_some(),
            "Draft text query must reveal Apply and Clear"
        );
        let v = view.read(cx);
        assert_eq!(v.tabs[0].history_applied, "outcome:error");
        let model = &v.tabs[0].model;
        let shown = v.tabs[0].inspections["History"]["items"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|item| history_matches(item, &v.tabs[0].history_applied, model))
            .map(|item| item["exec_id"].as_str().unwrap())
            .collect::<Vec<_>>();
        assert_eq!(shown, ["b"]);
        window.click("clear-history-filters", cx);
        window.render_frame(cx);
        assert_eq!(view.read(cx).tabs[0].history_applied, "");
        assert_eq!(view.read(cx).history_query.read(cx).value().to_string(), "");
        assert!(window.try_find("apply-history-filters").is_none());
    })
    .unwrap();
    let mut visual = gpui_kit::VisualTestContext::from_window(handle, cx);
    assert!(
        visual.debug_bounds("history-duration-b").is_some(),
        "unknown duration cell remains in its aligned column"
    );
    let sent = commands.try_iter().collect::<Vec<_>>();
    assert_eq!(
        sent.iter()
            .filter(|c| matches!(c, Command::Inspect { method, .. } if method == "executions.list"))
            .count(),
        2,
        "Immediate outcome click and Clear each request a bounded server page"
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
fn caption_gutter_preserves_run_label_before_secondary_chrome() {
    for comfortable in [false, true] {
        for scale in [1., 1.25, 1.5, 2.] {
            let c = super::shell::header_collapse(800., scale, comfortable);
            assert!(
                super::shell::header_fits(800., scale, comfortable, c),
                "{c:?}"
            );
            if scale == 1. {
                assert!(!c.run_label, "Run all must have text at 800/100%: {c:?}");
            }
            if c.run_label {
                assert!(c.navigation && c.export && c.runtime && c.settings && c.switcher);
            }
        }
    }
    let roomy = super::shell::header_collapse(1440., 1., false);
    assert!(!roomy.export && !roomy.navigation);
    assert!(roomy.wordmark_short);
    assert_eq!(roomy.switcher_width, 360.);
    let width = |text: &str| text.chars().count() as f32 * 8.0;
    let name_28 = "kl-marimo-verify-vf_okbtf.py";
    assert_eq!(name_28.chars().count(), 28);
    assert_eq!(
        super::shell::middle_ellipsis(name_28, roomy.switcher_width - 36., width),
        name_28
    );
}

#[test]
fn product_mark_keeps_run_label_at_standard_compact_width() {
    let tight = super::shell::header_collapse(800., 1., false);
    assert!(
        !tight.run_label && tight.wordmark_short,
        "Run all stays labeled at 800/100% with the compact KL mark: {tight:?}"
    );
    let roomy = super::shell::header_collapse(1440., 1., false);
    assert!(!roomy.run_label);
}

#[test]
fn switcher_reclaims_only_room_reserved_for_undrawn_controls() {
    use super::shell::{header_collapse, switcher_slack};
    let c = header_collapse(800., 1., false);
    // Notebook draws Outline and Run all; "Disconnected" fills its reserve.
    assert_eq!(switcher_slack(800., 1., false, c, true, 0.), 0.);
    // Other destinations hand back Outline (28 + 4) and Run all (92 + 4).
    assert_eq!(switcher_slack(800., 1., false, c, false, 0.), 128.);
    // A shorter runtime label returns its measured saving; never a negative one.
    assert_eq!(switcher_slack(800., 1., false, c, true, 30.), 30.);
    assert_eq!(switcher_slack(800., 1., false, c, true, -5.), 0.);
    // Tight chrome shows an icon and short word, so no label saving applies.
    let tight = header_collapse(800., 1., true);
    assert_eq!(switcher_slack(800., 1., true, tight, true, 30.), 0.);
    // A collapsed switcher has no width to grow.
    let mut hidden = c;
    hidden.switcher = true;
    assert_eq!(switcher_slack(800., 1., false, hidden, false, 30.), 0.);
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
                let mut actions = vec!["Notebook", "application-menu", "run-all", "outline-toggle"];
                if !collapse.navigation {
                    actions.extend(["History", "Checkpoints"]);
                }
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
                let mut horizontal = actions
                    .iter()
                    .map(|name| (*name, visual.debug_bounds(name).unwrap()))
                    .collect::<Vec<_>>();
                horizontal.sort_by(|a, b| a.1.origin.x.partial_cmp(&b.1.origin.x).unwrap());
                for pair in horizontal.windows(2) {
                    assert!(
                        pair[0].1.origin.x + pair[0].1.size.width <= pair[1].1.origin.x,
                        "Overlapping header controls at {width}/{scale}: {pair:?}"
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

#[test]
fn assistant_setup_follows_settings_and_test_results_without_leaking_keys() {
    use super::assistant::{Connection, Setup, assistant_setup, redact_secrets};
    let configured = json!({"policy":"allowed","settings":{"provider":"openai","model":"gpt"}});
    assert_eq!(
        assistant_setup(&serde_json::Value::Null, &Connection::Ready),
        Setup::Loading
    );
    assert_eq!(
        assistant_setup(
            &json!({"policy":"off","settings":{}}),
            &Connection::Untested
        ),
        Setup::Off
    );
    assert_eq!(
        assistant_setup(
            &json!({"settings":{"provider":"openai","model":" "}}),
            &Connection::Ready
        ),
        Setup::Unconfigured
    );
    assert_eq!(
        assistant_setup(&configured, &Connection::Untested),
        Setup::Untested
    );
    assert_eq!(
        assistant_setup(&configured, &Connection::Testing),
        Setup::Testing
    );
    assert_eq!(
        assistant_setup(&configured, &Connection::Ready),
        Setup::Ready
    );
    let message =
        redact_secrets("401 for Authorization: Bearer abc.def with key sk-proj-1234567890abcd");
    assert!(
        !message.contains("abc.def") && !message.contains("sk-proj-1234567890abcd"),
        "{message}"
    );
}

#[gpui_kit::test]
fn provider_setup_round_trip_keeps_the_question_and_context(cx: &mut TestAppContext) {
    let (handle, view, commands) = mount(cx);
    cx.simulate_window_resize(handle, size(px(1280.), px(850.)));
    cx.update_window(handle, |_, window, cx| {
        view.update(cx, |v, cx| {
            v.open_assistant(cx);
            v.tabs[0].assistant.settings =
                json!({"policy":"allowed","settings":{"provider":"","model":""}});
            v.tabs[0].assistant.selected_cells = vec!["second".into()];
        });
        window.render_frame(cx);
        let question = view
            .read(cx)
            .questions
            .iter()
            .find(|(key, _)| key.ends_with("-Question"))
            .map(|(_, input)| input.clone())
            .expect("assistant composer");
        question.update(cx, |s, cx| s.set_value("Why is x zero?", window, cx));
        window.click("assistant-open-settings", cx);
        window.render_frame(cx);
        let v = view.read(cx);
        assert_eq!(v.surface, "Settings");
        assert_eq!(v.settings_section, "assistant");
        assert_eq!(v.tabs[0].assistant.selected_cells, ["second"]);
        assert_eq!(question.read(cx).value().to_string(), "Why is x zero?");
    })
    .unwrap();
    assert!(!commands.try_iter().any(|c| matches!(
        c,
        Command::Assistant { method, .. } if method == "context.preview" || method == "respond" || method == "providers.test"
    )));
}

#[gpui_kit::test]
fn settings_owns_its_width_and_return_restores_assistant(cx: &mut TestAppContext) {
    let (handle, view, _) = mount(cx);
    let mut visual = gpui_kit::VisualTestContext::from_window(handle, cx);
    for scale in [1., 1.5, 2.] {
        for width in [800., 1024., 1280., 1440., 1920.] {
            cx.simulate_window_resize(handle, size(px(width), px(600.)));
            cx.update_window(handle, |_, window, cx| {
                view.update(cx, |v, cx| {
                    v.preferences.ui_scale = scale;
                    v.tabs[0].assistant.open = true;
                    v.settings_section = "assistant";
                    v.select_surface("Settings", cx);
                });
                window.render_frame(cx);
            })
            .unwrap();
            assert!(visual.debug_bounds("assistant-inspector").is_none());
            let form = visual
                .debug_bounds("settings-content")
                .expect("settings form");
            assert!(
                form.origin.x >= px(0.) && form.origin.x + form.size.width <= px(width),
                "{width}/{scale}: {form:?}"
            );
            cx.update_window(handle, |_, window, cx| {
                assert!(view.read(cx).tabs[0].assistant.open);
                view.update(cx, |v, cx| v.open_assistant(cx));
                assert_eq!(view.read(cx).surface, "Notebook");
                window.render_frame(cx);
            })
            .unwrap();
            assert!(visual.debug_bounds("assistant-inspector").is_some());
        }
    }
}

#[gpui_kit::test]
fn unread_source_never_looks_like_an_empty_saved_notebook(cx: &mut TestAppContext) {
    let (handle, view, commands) = mount(cx);
    let mut visual = gpui_kit::VisualTestContext::from_window(handle, cx);
    for state in ["", "invalid", "ok"] {
        cx.update_window(handle, |_, window, cx| {
            view.update(cx, |v, _| {
                v.tabs[0].model.order.clear();
                v.tabs[0].model.cells.clear();
                v.tabs[0].model.source_state = state.into();
                v.tabs[0].error = (state == "invalid").then(|| "Cannot load source".into());
            });
            window.render_frame(cx);
        })
        .unwrap();
        assert_eq!(
            visual.debug_bounds("empty-notebook").is_some(),
            state == "ok"
        );
        assert_eq!(
            visual.debug_bounds("unloaded-notebook").is_some(),
            state != "ok"
        );
    }
    assert!(
        !commands
            .try_iter()
            .any(|c| matches!(c, Command::Mutate(_) | Command::Run { .. }))
    );
}

#[gpui_kit::test]
fn long_source_scrolls_horizontally_without_clipping_wrapped_rows(cx: &mut TestAppContext) {
    let (handle, view, commands) = mount(cx);
    let source = format!("value = {:?}", "x".repeat(400));
    for width in [800., 1440.] {
        cx.simulate_window_resize(handle, size(px(width), px(600.)));
        cx.update_window(handle, |_, window, cx| {
            let editor = view.update(cx, |v, cx| {
                v.tabs[0].assistant.open = true;
                v.tabs[0].model.cells.get_mut("first").unwrap().text = source.clone();
                v.tabs[0].editors.remove("first");
                v.editor(0, "first", window, cx)
            });
            assert_eq!(
                editor.read(cx).cursor(),
                0,
                "new source opens at the leading edge"
            );
            editor.update(cx, |e, cx| {
                e.set_value(source.clone(), window, cx);
                e.focus(window, cx);
                e.set_selected_range(source.len()..source.len(), cx);
            });
            window.render_frame(cx);
            editor.update(cx, |e, cx| {
                e.set_scroll_offset(Point::new(px(-200.), px(0.)), cx)
            });
            window.render_frame(cx);
            let state = editor.read(cx);
            assert!(
                state.scroll_offset().x < px(0.),
                "long source needs horizontal scroll"
            );
            assert_eq!(
                state.scroll_offset().y,
                px(0.),
                "one source line must remain one visual row"
            );
            let (caret, _) = state.cursor_layout().expect("laid-out caret");
            let bounds = state.input_bounds();
            assert!(caret.top() >= bounds.top() && caret.bottom() <= bounds.bottom());
        })
        .unwrap();
    }
    assert!(
        !commands
            .try_iter()
            .any(|c| matches!(c, Command::Run { .. }))
    );
}

#[gpui_kit::test]
fn returning_to_history_refreshes_lists_without_replacing_details(cx: &mut TestAppContext) {
    let (handle, view, commands) = mount(cx);
    cx.update_window(handle, |_, window, cx| {
        for method in ["executions.list", "sessions.list", "execution.get"] {
            let page = json!({"cursor":"page-2","session_id":"session-a"});
            view.update(cx, |v, cx| {
                v.tabs[0]
                    .inspections
                    .insert("History".into(), json!({"items":[],"_page_request":page}));
                v.tabs[0]
                    .inspection_method
                    .insert("History".into(), method.into());
                v.tabs[0].history_applied = "outcome:error".into();
                v.select_surface("Notebook", cx);
                v.select_surface("History", cx);
            });
            let refreshes = commands
                .try_iter()
                .filter_map(|c| match c {
                    Command::Inspect { method, fields } => Some((method, fields)),
                    _ => None,
                })
                .collect::<Vec<_>>();
            if method == "execution.get" {
                assert!(refreshes.is_empty());
            } else {
                let mut expected = page;
                if method == "executions.list" {
                    expected["query"] = json!("outcome:error");
                    expected["page_size"] = json!(100);
                    expected["scan_pages"] = json!(10);
                }
                assert_eq!(refreshes, [(method.to_owned(), expected)]);
            }
            assert_eq!(view.read(cx).tabs[0].history_applied, "outcome:error");
        }
        let (send, receive) = mpsc::channel();
        view.update(cx, |v, _| {
            v.tick = gpui_kit::Task::ready(());
            v.tabs[0].service.events = receive;
            v.tabs[0]
                .inspection_method
                .insert("History".into(), "executions.list".into());
            v.tabs[0]
                .inspections
                .insert("History".into(), json!({"items":[],"_page_request":{}}));
        });
        for _ in 0..3 {
            send.send(kernelledger_client::service::Event::Inspection {
                method: "executions.list".into(),
                value: json!({"items":[{"exec_id":"new"}],"_page_request":{}}),
            })
            .unwrap();
            view.update(cx, |v, cx| v.drain(window, cx));
        }
        assert!(
            view.read(cx).tabs[0].parents.is_empty(),
            "refreshes must not accumulate Back history"
        );
        assert_eq!(
            view.read(cx).tabs[0].inspections["History"]["items"][0]["exec_id"],
            "new"
        );
    })
    .unwrap();
}

#[gpui_kit::test]
fn platform_commands_and_execution_bindings_stay_distinct(cx: &mut TestAppContext) {
    let (handle, view, commands) = mount(cx);
    let modifier = if cfg!(target_os = "macos") {
        "cmd"
    } else {
        "ctrl"
    };
    assert_eq!(
        super::shortcuts::platform_binding("ctrl-enter"),
        "ctrl-enter"
    );
    assert_eq!(
        super::shortcuts::platform_binding("ctrl-shift-enter"),
        "ctrl-shift-enter"
    );
    assert_eq!(
        super::shortcuts::platform_binding("ctrl-s"),
        format!("{modifier}-s")
    );
    let label = super::shortcuts::shortcut_text("Save Ctrl+S; run Ctrl+Enter / Ctrl+Shift+Enter");
    assert!(label.contains("run Ctrl+Enter / Ctrl+Shift+Enter"));
    cx.update_window(handle, |_, window, cx| {
        window.press(&format!("{modifier}-,"), cx);
        assert_eq!(view.read(cx).surface, "Settings");
        window.press(&format!("{modifier}-1"), cx);
        assert_eq!(view.read(cx).surface, "Notebook");
        window.press(&format!("{modifier}-s"), cx);
        window.press(&format!("{modifier}-shift-s"), cx);
        window.press(&format!("{modifier}-shift-p"), cx);
        assert!(view.read(cx).palette);
        window.press("escape", cx);
        window.press("ctrl-enter", cx);
    })
    .unwrap();
    let sent = commands.try_iter().collect::<Vec<_>>();
    assert!(sent.iter().any(|c| matches!(c, Command::Save)));
    assert!(
        sent.iter()
            .any(|c| matches!(c, Command::Runtime { method, .. } if method == "checkpoint"))
    );
    assert_eq!(
        sent.iter()
            .filter(|c| matches!(c, Command::Run { .. }))
            .count(),
        1
    );
}

#[gpui_kit::test]
fn cell_menu_puts_editing_first_and_assistant_actions_target_the_invoking_cell(
    cx: &mut TestAppContext,
) {
    let (handle, view, commands) = mount(cx);
    let open = |v: &mut Workspace,
                window: &mut gpui_kit::Window,
                cx: &mut gpui_kit::Context<Workspace>| {
        v.focus.focus(window, cx);
        v.return_focus = Some(v.focus.clone());
        v.cell_menu = Some(("second".into(), 1));
        v.dialog_focus.focus(window, cx);
    };
    cx.update_window(handle, |_, window, cx| {
        view.update(cx, |v, cx| {
            v.tabs[0].model.selected = Some("first".into());
            open(v, window, cx);
        });
        window.render_frame(cx);
        window.press("right", cx);
        assert!(view.read(cx).cell_menu_assistant);
        window.render_frame(cx);
        window.press("left", cx);
        assert!(!view.read(cx).cell_menu_assistant);
        window.render_frame(cx);
        window.click("cell-menu-assistant", cx);
        window.render_frame(cx);
        window.click("ai-edit", cx);
        let v = view.read(cx);
        assert!(v.cell_menu.is_none());
        assert_eq!(v.tabs[0].model.selected.as_deref(), Some("second"));
        assert_eq!(v.tabs[0].assistant.selected_cells, ["second"]);
    })
    .unwrap();
    cx.update_window(handle, |_, window, cx| {
        view.update(cx, |v, cx| open(v, window, cx));
        window.render_frame(cx);
        window.click("duplicate", cx);
        view.update(cx, |v, cx| open(v, window, cx));
        window.render_frame(cx);
        window.press("escape", cx);
        assert!(view.read(cx).cell_menu.is_none());
        assert!(view.read(cx).focus.is_focused(window));
    })
    .unwrap();
    assert!(commands.try_iter().any(|c| matches!(
        &c,
        Command::Mutate(v) if v["action"] == "duplicate" && v["cell_id"] == "second"
    )));
}

#[gpui_kit::test]
fn assistant_previews_context_without_sending_or_applying(cx: &mut TestAppContext) {
    let (handle, view, commands) = mount(cx);
    cx.update_window(handle, |_, window, cx| {
        view.update(cx, |v, cx| {
            v.open_assistant(cx);
            assert!(v.tabs[0].assistant.open);
            v.tabs[0].assistant.selected_cells = vec!["first".into()];
            v.ai_preview("ask", "Explain this cell".into(), cx);
        });
        window.render_frame(cx);
        assert!(view.read(cx).tabs[0].assistant.busy);
    })
    .unwrap();
    let sent = commands.try_iter().collect::<Vec<_>>();
    assert!(
        sent.iter()
            .any(|c| matches!(c, Command::Assistant { method, .. } if method=="settings.get"))
    );
    assert!(sent.iter().any(|c| matches!(c, Command::Assistant { method, fields } if method=="context.preview" && fields["cell_ids"]==json!(["first"]))));
    assert!(!sent.iter().any(|c| matches!(c, Command::Assistant { method, .. } if method=="respond" || method=="proposal.resolve")));
    assert!(!sent.iter().any(|c| matches!(
        c,
        Command::Mutate(_) | Command::Run { .. } | Command::AssistantApply(_)
    )));
}

#[gpui_kit::test]
fn assistant_error_action_prepares_only_selected_cell_and_execution(cx: &mut TestAppContext) {
    let (handle, view, commands) = mount(cx);
    cx.update_window(handle, |_, window, cx| {
        view.update(cx, |v, cx| {
            v.tabs[0].model.selected = Some("first".into());
            v.tabs[0].model.cells.get_mut("first").unwrap().exec_id = Some("recorded-error".into());
            v.run_command("ai-explain", window, cx);
        });
        window.render_frame(cx);
    })
    .unwrap();
    let request = commands
        .try_iter()
        .find_map(|command| {
            if let Command::Assistant { method, fields } = command {
                if method == "context.preview" {
                    Some(fields)
                } else {
                    None
                }
            } else {
                None
            }
        })
        .unwrap();
    assert_eq!(request["cell_ids"], json!(["first"]));
    assert_eq!(request["exec_ids"], json!(["recorded-error"]));
    assert_eq!(request["task"], "explain");
}

#[gpui_kit::test]
fn assistant_extended_commands_and_unavailable_reasons_work(cx: &mut TestAppContext) {
    let (handle, view, commands) = mount(cx);
    cx.update_window(handle, |_, window, cx| {
        view.update(cx, |v, cx| {
            v.tabs[0].model.selected = None;
            assert!(v.command_reason("ai-refactor").is_some());
            assert!(v.command_reason("ai-tests").is_some());
            assert!(v.command_reason("ai-repro").is_none());
            assert!(v.command_reason("ai-report").is_none());
            v.tabs[0].model.selected = Some("first".into());
            assert!(v.command_reason("ai-refactor").is_none());
            assert!(v.command_reason("ai-tests").is_none());
            v.run_command("ai-refactor", window, cx);
        });
        window.render_frame(cx);
    })
    .unwrap();
    let request = commands
        .try_iter()
        .find_map(|command| {
            if let Command::Assistant { method, fields } = command {
                if method == "context.preview" {
                    Some(fields)
                } else {
                    None
                }
            } else {
                None
            }
        })
        .unwrap();
    assert_eq!(request["cell_ids"], json!(["first"]));
    assert_eq!(request["task"], "refactor");
}

#[gpui_kit::test]
fn assistant_idle_suggestions_preview_from_the_composer(cx: &mut TestAppContext) {
    let (handle, view, commands) = mount(cx);
    cx.simulate_window_resize(handle, size(px(1280.), px(850.)));
    cx.update_window(handle, |_, window, cx| {
        view.update(cx, |v, cx| {
            v.open_assistant(cx);
            v.tabs[0].model.cells.get_mut("first").unwrap().outcome = "error".into();
        });
        window.render_frame(cx);
        window.click("assistant-summary", cx);
        window.render_frame(cx);
        assert!(view.read(cx).tabs[0].assistant.busy);
        assert_eq!(view.read(cx).tabs[0].assistant.task, "summary");
    })
    .unwrap();
    assert!(commands.try_iter().any(|command| matches!(
        command,
        Command::Assistant { method, fields }
            if method == "context.preview" && fields["task"] == "summary"
    )));
}

#[test]
fn middle_ellipsis_respects_measured_width_and_keeps_the_suffix() {
    use super::shell::middle_ellipsis;
    let width = |text: &str| {
        text.chars()
            .map(|c| if c == 'W' { 12. } else { 5. })
            .sum::<f32>()
    };
    assert_eq!(
        middle_ellipsis("comparison-notebook", 240., width),
        "comparison-notebook"
    );
    let name = "WWWW-long-notebook-final.py";
    let label = middle_ellipsis(name, 80., width);
    assert!(
        label.contains('\u{2026}') && label.ends_with("final.py"),
        "{label}"
    );
    assert!(width(&label) <= 80.);
    assert_ne!(
        middle_ellipsis("??????-A", 30., width),
        middle_ellipsis("??????-B", 30., width)
    );
}

#[test]
fn markdown_reading_blocks_keep_heading_body_and_code_content() {
    let blocks = super::notebook::markdown_reading_blocks(
        "# Results\n\nA **readable** paragraph.\n\n```python\nprint(42)\n```",
    );
    assert_eq!(blocks[0], (1, "Results".into()));
    assert_eq!(
        super::notebook::markdown_reading_blocks("# Heading\nBody without a blank line"),
        vec![
            (1, "Heading".into()),
            (0, "Body without a blank line".into())
        ]
    );
    assert_eq!(
        super::notebook::markdown_reading_blocks("```\n# code comment\n```")[0].0,
        0
    );
    assert_eq!(blocks[1], (0, "A **readable** paragraph.".into()));
    assert_eq!(blocks[2].1, "```python\nprint(42)\n```");
    assert!(!super::notebook::compact_source_layout(
        false, 2., 14., false
    ));
    assert!(!super::notebook::compact_source_layout(
        false, 1., 24., false
    ));
    assert!(!super::notebook::compact_source_layout(
        false, 1., 14., true
    ));
    assert!(!super::notebook::compact_source_layout(
        true, 1., 14., false
    ));
}

#[gpui_kit::test]
fn compact_cells_remove_a_header_row_and_preserve_notable_states(cx: &mut TestAppContext) {
    let (handle, view, commands) = mount(cx);
    cx.simulate_window_resize(handle, size(px(1280.), px(850.)));
    let mut visual = gpui_kit::VisualTestContext::from_window(handle, cx);
    let mut heights = Vec::new();
    // Same editor/source/output and rhythm; enabling debugger only selects the
    // retained header layout. Measure the whole cell slot, not just the source.
    for debugger in [false, true] {
        cx.update_window(handle, |_, window, cx| {
            view.update(cx, |v, _| {
                let cell = v.tabs[0].model.cells.get_mut("first").unwrap();
                cell.ordinal = Some(1);
                cell.outcome = "ok".into();
                cell.output = vec![json!({"kind":"stdout", "text":"42"})];
                v.tabs[0].runtime["interactive"] =
                    json!({"features":if debugger { vec!["debugger"] } else { vec![] }});
                v.tabs[0].list.splice(0..2, 2);
            });
            window.render_frame(cx);
        })
        .unwrap();
        heights.push(visual.debug_bounds("cell-frame-first").unwrap().size.height / px(1.));
    }
    assert!(
        heights[1] - heights[0] >= 28.,
        "compact / retained header slots: {heights:?}"
    );
    eprintln!(
        "Matched single-line source/output slot heights: compact={} retained-header={}",
        heights[0], heights[1]
    );
    cx.update_window(handle, |_, window, cx| {
        view.update(cx, |v, _| {
            v.tabs[0].runtime["interactive"] = json!({"features":[]});
            let cell = v.tabs[0].model.cells.get_mut("first").unwrap();
            cell.save = kernelledger_models::SaveState::Conflict;
            cell.executed_revision = Some("old-source".into());
            cell.outcome = "error".into();
            v.tabs[0].list.splice(0..2, 2);
        });
        window.render_frame(cx);
        assert_eq!(
            view.read(cx).tabs[0].model.selected.as_deref(),
            Some("first")
        );
        window.click(("run", 0usize), cx);
    })
    .unwrap();
    assert!(visual.debug_bounds("cell-frame-first").unwrap().size.height > px(heights[0] + 28.));
    assert!(
        !commands
            .try_iter()
            .any(|c| matches!(c, Command::Run { .. }))
    );
}

#[gpui_kit::test]
fn markdown_edit_targets_the_invoking_cell_and_semantics_expose_rendered_content(
    cx: &mut TestAppContext,
) {
    let (handle, view, _) = mount(cx);
    cx.update_window(handle, |_, window, cx| {
        view.update(cx, |v, _| {
            for id in ["first", "second"] {
                let cell = v.tabs[0].model.cells.get_mut(id).unwrap();
                cell.disk.kind = "markdown".into();
                cell.text = "# Readable heading\n\nRendered body.".into();
            }
            v.tabs[0].model.mode = kernelledger_models::Mode::Edit;
            v.tabs[0].model.selected = Some("first".into());
            v.tabs[0].list.splice(0..2, 2);
        });
        window.render_frame(cx);
        let nodes = super::notebook::markdown_accessibility_nodes(
            &view.read(cx).tabs[0].model.cells["second"].text,
        );
        assert_eq!(nodes[0].role(), gpui_kit::Role::Heading);
        assert_eq!(nodes[0].label(), Some("Readable heading"));
        assert_eq!(nodes[0].level(), Some(1));
        assert_eq!(nodes[1].label(), Some("Rendered body."));
        // Move only the headless pointer into the reserved action area.
        window.dispatch_event(
            gpui_kit::PlatformInput::MouseMove(gpui_kit::MouseMoveEvent {
                position: window.find(("edit-markdown", 1usize)).bounds().center(),
                pressed_button: None,
                modifiers: Default::default(),
            }),
            cx,
        );
        window.render_frame(cx);
        window.click(("edit-markdown", 1usize), cx);
        assert_eq!(
            view.read(cx).tabs[0].model.selected.as_deref(),
            Some("second")
        );
        assert_eq!(
            view.read(cx).tabs[0].model.mode,
            kernelledger_models::Mode::Edit
        );
        assert!(
            view.read(cx).tabs[0].editors["second"]
                .read(cx)
                .focus_handle(cx)
                .is_focused(window)
        );
        window.render_frame(cx);
        window.click(("edit-markdown", 1usize), cx);
        assert_eq!(
            view.read(cx).tabs[0].model.mode,
            kernelledger_models::Mode::Command
        );
    })
    .unwrap();
}

#[gpui_kit::test]
fn collapsed_destinations_and_export_remain_reachable(cx: &mut TestAppContext) {
    let (handle, view, _) = mount(cx);
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        assert!(!super::shell::header_collapse(800., 1., false).run_label);
        window.click("Notebook", cx);
        assert!(view.read(cx).app_menu);
        window.render_frame(cx);
        window.click("destination-History", cx);
        assert_eq!(view.read(cx).surface, "History");
        window.render_frame(cx);
        window.click("History", cx);
        window.render_frame(cx);
        window.click("destination-Checkpoints", cx);
        assert_eq!(view.read(cx).surface, "Checkpoints");
        window.render_frame(cx);
        window.click("application-menu", cx);
        window.render_frame(cx);
        window.click("menu-export", cx);
        assert!(view.read(cx).export_menu);
        window.render_frame(cx);
        window.press("escape", cx);
        assert!(!view.read(cx).export_menu);
    })
    .unwrap();
}

#[gpui_kit::test]
fn duration_preference_is_optional_and_last_execution_identity_controls_the_badge(
    cx: &mut TestAppContext,
) {
    use kernelledger_client::service::Event;
    let (handle, view, _) = mount(cx);
    let (send, events) = mpsc::channel();
    cx.update_window(handle, |_, window, cx| {
        view.update(cx, |v, _| { v.tick = gpui_kit::Task::ready(()); v.tabs[0].service.events = events; });
        for (exec_id, duration) in [("recorded-1", Some(1.25)), ("recorded-2", None)] {
            send.send(Event::Output(json!({"cell_id":"first", "exec_id":exec_id, "status":"ok", "duration":duration, "outputs":[]}))).unwrap();
            view.update(cx, |v, cx| v.drain(window, cx));
            let cell = &view.read(cx).tabs[0].model.cells["first"];
            assert_eq!(cell.exec_id.as_deref(), Some(exec_id));
            assert_eq!(cell.execution_seconds, duration);
        }
        view.update(cx, |v, _| {
            assert!(!v.preferences.show_cell_duration);
            v.surface = "Settings".into(); v.settings_section = "layout";
        });
        window.render_frame(cx);
        window.click("show-cell-duration", cx);
        // The section heading leaves this group partly clipped in the test
        // window, so it scrolls into view and the setting applies next frame.
        window.simulate_next_frame(cx);
        assert!(view.read(cx).preferences.show_cell_duration);
        view.update(cx, |v, _| v.surface = "Notebook".into());
        window.render_frame(cx);
        // Unknown is not rendered as zero. Native provider activation is a
        // manual qualification; the headless platform does not activate UIA.
        send.send(Event::Output(json!({"cell_id":"first", "exec_id":"recorded-3", "status":"ok", "duration":1.25, "outputs":[]}))).unwrap();
        view.update(cx, |v, cx| v.drain(window, cx));
        window.render_frame(cx);
        assert_eq!(view.read(cx).tabs[0].model.cells["first"].execution_seconds, Some(1.25));
        assert_eq!(super::record_duration(&json!({"duration":-1})), None);
        assert_eq!(super::record_duration(&json!({"start":"2026-11-05T00:00:00Z","end":"2026-11-05T00:00:02.250Z"})), Some(2.25));
    }).unwrap();
    let mut visual = gpui_kit::VisualTestContext::from_window(handle, cx);
    assert!(visual.debug_bounds("cell-duration-first").is_some());
    cx.update_window(handle, |_, window, cx| {
        view.update(cx, |v, _| {
            v.preferences.show_cell_duration = false;
            v.tabs[0].list.splice(0..2, 2);
        });
        window.render_frame(cx);
    })
    .unwrap();
    assert!(visual.debug_bounds("cell-duration-first").is_none());
    send.send(Event::Output(
        json!({"cell_id":"first", "exec_id":"recorded-4", "status":"ok", "outputs":[]}),
    ))
    .unwrap();
    cx.update_window(handle, |_, window, cx| {
        view.update(cx, |v, cx| {
            v.preferences.show_cell_duration = true;
            v.drain(window, cx);
        });
        window.render_frame(cx);
    })
    .unwrap();
    assert!(
        visual.debug_bounds("cell-duration-first").is_none(),
        "A rerun with unknown duration must clear the old badge"
    );
}

#[test]
fn table_html_is_not_interactive_and_html_preference_defaults_off() {
    assert!(!kernelledger_client::output::html_is_interactive(&json!({
        "bundle":{"text/html":{"value":"<table><tr><th>Name</th><td>Safe</td></tr></table>","rows":[["Name","Safe"]]}}
    })));
    assert!(kernelledger_client::output::html_is_interactive(&json!({
        "bundle":{"text/html":{"value":"<div><script>window.Plotly={}</script></div>"}}
    })));
    assert!(!kernelledger_client::preferences::Preferences::default().allow_html_output);
}

#[gpui_kit::test]
fn html_network_settings_validate_domains_and_stop_existing_outputs(cx: &mut TestAppContext) {
    use kernelledger_client::preferences::HtmlNetworkPolicy;
    let (handle, view, _) = mount(cx);
    cx.simulate_window_resize(handle, size(px(1280.), px(1000.)));
    cx.update_window(handle, |_, window, cx| {
        view.update(cx, |v, _| {
            v.surface = "Settings".into();
            v.settings_section = "layout";
            v.tabs[0].html_notes.insert(
                "first-0".into(),
                "Interactive HTML is not created in automated tests.".into(),
            );
        });
        window.render_frame(cx);
        window.click("html-network-allowed", cx);
        assert_eq!(
            view.read(cx).preferences.html_network_policy,
            HtmlNetworkPolicy::AllowedDomains
        );
        assert!(view.read(cx).tabs[0].html_notes.is_empty());
        let input = view.read(cx).html_network_origins.clone();
        input.update(cx, |input, cx| {
            input.set_value("cdn.plot.ly, http://127.0.0.1:18763", window, cx)
        });
        window.render_frame(cx);
        window.click("html-network-save-domains", cx);
        assert_eq!(
            view.read(cx).preferences.html_allowed_origins,
            vec!["https://cdn.plot.ly", "http://127.0.0.1:18763"]
        );
        input.update(cx, |input, cx| {
            input.set_value("https://good.example;default-src *", window, cx)
        });
        window.render_frame(cx);
        window.click("html-network-save-domains", cx);
        assert!(view.read(cx).html_network_error.is_some());
        assert_eq!(view.read(cx).preferences.html_allowed_origins.len(), 2);
        window.render_frame(cx);
        window.click("html-network-full", cx);
        assert_eq!(
            view.read(cx).preferences.html_network_policy,
            HtmlNetworkPolicy::Full
        );
        window.render_frame(cx);
        window.click("html-network-offline", cx);
        assert_eq!(
            view.read(cx).preferences.html_network_policy,
            HtmlNetworkPolicy::Offline
        );
    })
    .unwrap();
}

#[gpui_kit::test]
fn interactive_html_stays_off_until_render_and_does_not_construct_a_webview(
    cx: &mut TestAppContext,
) {
    let (handle, view, _) = mount(cx);
    cx.update_window(handle, |_, window, cx| {
        assert!(!view.read(cx).preferences.allow_html_output);
        view.update(cx, |v, _| {
            v.tabs[0].model.selected = Some("first".into());
            v.tabs[0].model.cells.get_mut("first").unwrap().output = vec![json!({
                "kind":"display",
                "bundle":{"text/html":{"value":"<div><script>window.Plotly={}</script></div>"}}
            })];
            v.tabs[0].list.splice(0..1, 1);
            v.surface = "Settings".into();
            v.settings_section = "layout";
        });
        window.render_frame(cx);
        window.click("allow-html-output", cx);
        // The section heading leaves this group partly clipped in the test
        // window, so it scrolls into view and the setting applies next frame.
        window.simulate_next_frame(cx);
        assert!(view.read(cx).preferences.allow_html_output);
        assert!(view.read(cx).tabs[0].html_views.is_empty());
        view.update(cx, |v, _| v.surface = "Notebook".into());
        window.render_frame(cx);
        assert!(view.read(cx).tabs[0].html_views.is_empty());
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
                "render-html",
            ),
            cx,
        );
        window.render_frame(cx);
        assert!(
            view.read(cx).tabs[0].html_views.is_empty(),
            "headless tests must not construct a WebView"
        );
        assert!(
            view.read(cx).tabs[0]
                .html_notes
                .get("first-0")
                .is_some_and(|note| note.contains("automated tests"))
        );
    })
    .unwrap();
}

#[gpui_kit::test]
fn table_html_keeps_native_rows_and_offers_original_html_in_tools(cx: &mut TestAppContext) {
    let (handle, view, _) = mount(cx);
    cx.update_window(handle, |_, window, cx| {
        view.update(cx, |v, _| {
            v.tabs[0].model.selected = Some("first".into());
            v.tabs[0].model.cells.get_mut("first").unwrap().output = vec![json!({
                "kind":"display",
                "bundle":{"text/html":{
                    "value":"<table><tr><th>Column</th></tr><tr><td>Value</td></tr></table>",
                    "rows":[["Column"],["Value"]]
                }}
            })];
            v.tabs[0].list.splice(0..1, 1);
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
        assert!(
            window
                .try_find((
                    gpui_kit::ElementId::from(gpui_kit::SharedString::from("first-0")),
                    "render-html",
                ))
                .is_some()
        );
    })
    .unwrap();
    let mut visual = gpui_kit::VisualTestContext::from_window(handle, cx);
    assert!(visual.debug_bounds("output-block-first-0").is_some());
    assert!(visual.debug_bounds("cell-frame-first").is_some());
}

#[gpui_kit::test]
fn html_js_output_kinds_keep_native_defaults_and_offer_opt_in_render(cx: &mut TestAppContext) {
    let kinds = html_js_kind_outputs();
    let (handle, view, _) = mount(cx);
    cx.update_window(handle, |_, window, cx| {
        view.update(cx, |v, _| {
            v.preferences.allow_html_output = true;
            v.tabs[0].model.selected = Some("first".into());
            v.tabs[0].model.cells.get_mut("first").unwrap().output =
                kinds.iter().map(|(_, event, _, _)| event.clone()).collect();
            v.tabs[0].list.splice(0..1, 1);
        });
        window.render_frame(cx);
        for (index, (name, _, interactive, available)) in kinds.iter().enumerate() {
            let id = format!("first-{index}");
            let render = window.try_find(output_action(&id, "render-html"));
            assert_eq!(
                render.is_some(),
                *interactive && *available,
                "{name}: Render HTML visibility"
            );
        }
        window.click(
            (
                gpui_kit::ElementId::from(gpui_kit::SharedString::from("notebook-0-first")),
                "output-tools-toggle",
            ),
            cx,
        );
        window.render_frame(cx);
        assert!(
            window
                .try_find(output_action("first-3", "image-size"))
                .is_some()
        );
        assert!(view.read(cx).tabs[0].html_views.is_empty());
        assert!(view.read(cx).tabs[0].html_notes.is_empty());

        window.click(output_action("first-2", "render-html"), cx);
        window.render_frame(cx);
        assert_eq!(
            view.read(cx).tabs[0].html_notes.get("first-2").unwrap(),
            "Interactive HTML is not created in automated tests."
        );
        assert!(view.read(cx).tabs[0].html_views.is_empty());
    })
    .unwrap();
    let mut visual = gpui_kit::VisualTestContext::from_window(handle, cx);
    assert!(visual.debug_bounds("output-block-first-0").is_some());
    assert!(visual.debug_bounds("cell-frame-first").is_some());
}

#[gpui_kit::test]
fn html_js_history_does_not_auto_render_and_live_views_are_capped(cx: &mut TestAppContext) {
    let plotly = html_display("<div><script>Plotly.newPlot('p',[])</script></div>");
    let (handle, view, _) = mount(cx);
    cx.update_window(handle, |_, window, cx| {
        view.update(cx, |v, _| {
            v.preferences.allow_html_output = true;
            v.surface = "History".into();
            v.tabs[0]
                .inspection_method
                .insert("History".into(), "execution.get".into());
            v.tabs[0].inspections.insert(
                "History".into(),
                json!({
                    "exec_id":"recorded",
                    "cell_id":"first",
                    "ordinal":1,
                    "status":"ok",
                    "imported":true,
                    "outputs":[plotly]
                }),
            );
        });
        window.render_frame(cx);
        assert!(view.read(cx).tabs[0].html_views.is_empty());
        assert!(view.read(cx).tabs[0].html_notes.is_empty());
        window.click(output_action("history-result-0", "tools"), cx);
        window.render_frame(cx);
        window.click(output_action("history-result-0", "render-html"), cx);
        window.render_frame(cx);
        assert!(
            view.read(cx).tabs[0]
                .html_notes
                .get("history-result-0")
                .is_some_and(|note| note.contains("automated tests"))
        );

        view.update(cx, |v, _| {
            v.surface = "Notebook".into();
            v.tabs[0].html_notes.clear();
            v.tabs[0].model.selected = Some("first".into());
            v.tabs[0].model.cells.get_mut("first").unwrap().output =
                (0..5).map(|_| plotly.clone()).collect();
            v.tabs[0].list.splice(0..1, 1);
        });
        window.render_frame(cx);
        for index in 0..5 {
            window.click(output_action(&format!("first-{index}"), "render-html"), cx);
            window.render_frame(cx);
        }
        let notes = &view.read(cx).tabs[0].html_notes;
        for index in 0..4 {
            assert!(
                notes
                    .get(&format!("first-{index}"))
                    .is_some_and(|note| note.contains("automated tests")),
                "slot {index}"
            );
        }
        assert!(
            notes
                .get("first-4")
                .is_some_and(|note| note.contains("Too many interactive outputs")),
        );
        assert!(view.read(cx).tabs[0].html_views.is_empty());
    })
    .unwrap();
}

#[gpui_kit::test]
fn contextual_cell_actions_reveal_on_keyboard_focus(cx: &mut TestAppContext) {
    let (handle, view, _) = mount(cx);
    cx.update_window(handle, |_, window, cx| {
        view.update(cx, |v, _| v.tabs[0].model.selected = Some("first".into()));
        window.render_frame(cx);
        let mut reached = false;
        for _ in 0..80 {
            window.focus_next(cx);
            window.render_frame(cx);
            let more = window.find(("cell-more", 1usize));
            if more.focused() == Some(true) {
                assert!(more.visible(), "Focused cell action must be visible");
                reached = true;
                break;
            }
        }
        assert!(reached, "Second cell actions must be keyboard reachable");
    })
    .unwrap();
}

#[gpui_kit::test]
fn source_undo_shortcuts_preserve_editor_undo_and_use_revision_checked_mutations(
    cx: &mut TestAppContext,
) {
    let (handle, view, commands) = mount(cx);
    cx.update_window(handle, |_, window, cx| {
        let editor = view.update(cx, |v, cx| {
            v.tabs[0]
                .inspections
                .insert("Source history".into(), json!({"undo":true,"redo":true}));
            v.editor(0, "first", window, cx)
        });
        editor.update(cx, |e, cx| e.focus(window, cx));
        window.press(
            if cfg!(target_os = "macos") {
                "cmd-z"
            } else {
                "ctrl-z"
            },
            cx,
        );
        assert!(
            !commands
                .try_iter()
                .any(|command| matches!(command, Command::Mutate(_)))
        );
        view.update(cx, |v, cx| {
            v.tabs[0].model.mode = kernelledger_models::Mode::Command;
            v.focus.focus(window, cx)
        });
        window.press(
            if cfg!(target_os = "macos") {
                "cmd-z"
            } else {
                "ctrl-z"
            },
            cx,
        );
        let request = commands
            .try_iter()
            .find_map(|c| {
                if let Command::Mutate(r) = c {
                    Some(r)
                } else {
                    None
                }
            })
            .expect("structural undo");
        assert_eq!(request["action"], "undo");
        assert_eq!(request["manifest_revision"], "test");
        assert_eq!(request["cell_revisions"]["first"], "hash");
    })
    .unwrap();
}

#[gpui_kit::test]
fn shifted_native_commands_and_modal_blocking_preserve_source(cx: &mut TestAppContext) {
    let (handle, view, commands) = mount(cx);
    cx.update_window(handle, |_, window, cx| {
        view.update(cx, |v, cx| {
            let mut key = gpui_kit::Keystroke::parse(if cfg!(target_os = "macos") {
                "cmd-shift-s"
            } else {
                "ctrl-shift-s"
            })
            .unwrap();
            key.key = "S".into();
            v.key_down(
                &gpui_kit::KeyDownEvent {
                    keystroke: key,
                    is_held: false,
                    prefer_character_input: false,
                },
                window,
                cx,
            );
        });
        assert_eq!(
            commands
                .try_iter()
                .filter(|c| matches!(c,Command::Runtime{method,..} if method=="checkpoint"))
                .count(),
            1
        );
        window.press(
            if cfg!(target_os = "macos") {
                "cmd-shift-p"
            } else {
                "ctrl-shift-p"
            },
            cx,
        );
        window.press(
            if cfg!(target_os = "macos") {
                "cmd-shift-s"
            } else {
                "ctrl-shift-s"
            },
            cx,
        );
        assert!(view.read(cx).palette);
        assert!(!commands.try_iter().any(|c| matches!(
            c,
            Command::Runtime { .. } | Command::Run { .. } | Command::Mutate(_)
        )));
        window.press("escape", cx);
        assert!(!view.read(cx).palette);
        view.update(cx, |v, _| {
            v.confirmation = Some(("delete".into(), json!({"id": "first"})));
        });
        window.render_frame(cx);
        window.press(
            if cfg!(target_os = "macos") {
                "cmd-shift-s"
            } else {
                "ctrl-shift-s"
            },
            cx,
        );
        assert!(view.read(cx).confirmation.is_some());
        assert!(!commands.try_iter().any(|c| matches!(
            c,
            Command::Runtime { .. } | Command::Run { .. } | Command::Mutate(_)
        )));
        assert_eq!(view.read(cx).tabs[0].model.cells["first"].text, "print(42)");
        assert!(
            super::COMMANDS
                .iter()
                .all(|(_, label, _, _)| !label.contains("...")),
            "All command labels must use typographical ellipsis instead of three ASCII dots"
        );
    })
    .unwrap();
}

#[gpui_kit::test]
fn replacing_application_python_preserves_edits_and_worker_on_validation_failure(
    cx: &mut TestAppContext,
) {
    use kernelledger_client::service::Event;
    let (handle, view, commands) = mount(cx);
    let (events, receive) = mpsc::channel();
    cx.update_window(handle, |_, window, cx| {
        view.update(cx, |v, _| {
            v.tick = gpui_kit::Task::ready(());
            v.tabs[0].service.events = receive;
            v.tabs[0].runtime = json!({"worker_state":"idle","session_id":"retained"});
            v.tabs[0].inspections.insert(
                "Application Python".into(),
                json!({"python":"old-python","ready":true}),
            );
            v.tabs[0].model.cells.get_mut("first").unwrap().text = "unsaved = True".into();
        });
        view.update(cx, |v, cx| v.choose_application_python(window, cx));
    })
    .unwrap();
    cx.simulate_path_prompt_response(|_| Some(vec![PathBuf::from("replacement-python")]));
    cx.executor().run_until_parked();
    assert!(
        matches!(commands.try_recv().unwrap(),Command::SelectApplicationPython(path) if path==std::path::Path::new("replacement-python"))
    );
    cx.update_window(handle, |_, window, cx| {
        events
            .send(Event::Error {
                code: "helper_validation".into(),
                message: "No module named kernelledger".into(),
            })
            .unwrap();
        view.update(cx, |v, cx| v.drain(window, cx));
        let tab = &view.read(cx).tabs[0];
        assert_eq!(tab.model.cells["first"].text, "unsaved = True");
        assert_eq!(tab.runtime["session_id"], "retained");
        assert_ne!(tab.runtime["connection_lost"], true);
        assert!(
            tab.error
                .as_deref()
                .unwrap()
                .contains("No module named kernelledger")
        );
        assert!(
            tab.error
                .as_deref()
                .unwrap()
                .contains("current connection is unchanged")
        );
        events
            .send(Event::Inspection {
                method: "helper.status".into(),
                value: json!({"python":"replacement-python","ready":true}),
            })
            .unwrap();
        view.update(cx, |v, cx| v.drain(window, cx));
        assert!(view.read(cx).tabs[0].error.is_none());
        assert!(
            !view.read(cx).tabs[0]
                .inspections
                .contains_key("Technical error")
        );
    })
    .unwrap();
}

#[gpui_kit::test]
fn application_python_chooser_accepts_environment_folder_without_resolving_python(
    cx: &mut TestAppContext,
) {
    let root = std::env::temp_dir().join(format!("kl-python-chooser-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&root).unwrap();
    let (handle, view, commands) = mount(cx);
    cx.update_window(handle, |_, window, cx| {
        view.update(cx, |v, cx| {
            v.tabs[0].inspections.insert(
                "Application Python".into(),
                json!({"python":"old-python","ready":true}),
            );
            v.choose_application_python(window, cx);
        });
    })
    .unwrap();
    cx.simulate_path_prompt_response(|options| {
        assert!(options.files && options.directories);
        Some(vec![root.clone()])
    });
    cx.executor().run_until_parked();
    let expected = root.join(if cfg!(windows) {
        "Scripts/python.exe"
    } else {
        "bin/python"
    });
    assert!(
        matches!(commands.try_recv().unwrap(),Command::SelectApplicationPython(path) if path==expected)
    );
    std::fs::remove_dir_all(root).unwrap();
}

#[gpui_kit::test]
fn short_inline_cells_clamp_vertical_scroll_after_content_shrinks(cx: &mut TestAppContext) {
    let (handle, view, _) = mount(cx);
    cx.simulate_window_resize(handle, size(px(800.), px(600.)));
    cx.update_window(handle, |_, window, cx| {
        let long = (0..45)
            .map(|i| format!("value_{i} = {i}\n"))
            .collect::<String>();
        let short = "first = 1\nsecond = 2\n";
        let editor = view.update(cx, |v, cx| {
            v.tabs[0].model.cells.get_mut("first").unwrap().text = long.clone();
            v.editor(0, "first", window, cx)
        });
        editor.update(cx, |e, cx| {
            e.focus(window, cx);
            e.set_selected_range(long.len()..long.len(), cx);
        });
        window.render_frame(cx);
        view.update(cx, |v, _| {
            v.tabs[0].model.cells.get_mut("first").unwrap().text = short.into()
        });
        editor.update(cx, |e, cx| {
            e.set_value(short, window, cx);
            e.set_selected_range(short.len()..short.len(), cx);
            e.set_scroll_offset(Point::new(px(0.), px(-42.)), cx);
        });
        window.render_frame(cx);
        window.render_frame(cx);
        let state = editor.read(cx);
        assert_eq!(
            state.scroll_offset().y,
            px(0.),
            "all short-cell rows fit the inline viewport"
        );
        assert_eq!(state.visible_row_range().unwrap().start, 0);
        assert_eq!(
            state.cursor(),
            short.len(),
            "clamping scroll must preserve the caret"
        );
    })
    .unwrap();
}

#[gpui_kit::test]
fn header_runtime_label_and_checkpoint_notice_stay_concise_and_scoped(cx: &mut TestAppContext) {
    assert_eq!(
        super::shell::header_worker_label(false, None),
        "Not running"
    );
    assert_eq!(
        super::shell::header_worker_label(false, Some("idle")),
        "Ready"
    );
    assert_eq!(
        super::shell::header_worker_label(false, Some("running")),
        "Running"
    );
    assert_eq!(
        super::shell::header_worker_label(false, Some("stopped")),
        "Stopped"
    );
    assert_eq!(
        super::shell::header_worker_label(true, Some("idle")),
        "Disconnected"
    );

    let (handle, view, _) = mount(cx);
    cx.update_window(handle, |_, window, cx| {
        for (lost, state) in [
            (false, "absent"),
            (false, "idle"),
            (false, "running"),
            (false, "stopped"),
            (true, "idle"),
        ] {
            view.update(cx, |v, cx| {
                v.tabs[0].runtime = json!({
                    "connection_lost": lost,
                    "worker_state": state,
                });
                cx.notify();
            });
            window.render_frame(cx);
            window.render_frame(cx);
            assert!(
                view.read(cx).menu_bounds.contains_key("runtime"),
                "runtime anchor bounds must be recorded for state={state} lost={lost}"
            );
        }

        view.update(cx, |v, cx| {
            v.tabs[0].checkpoint_notice = Some("cp-123".into());
            cx.notify();
        });
        window.render_frame(cx);
        assert_eq!(
            view.read(cx).tabs[0].checkpoint_notice.as_deref(),
            Some("cp-123")
        );

        view.update(cx, |v, cx| {
            v.select_surface("Checkpoints", cx);
        });
        window.render_frame(cx);
        assert!(
            view.read(cx).tabs[0].checkpoint_notice.is_none(),
            "navigating to Checkpoints must clear the transient checkpoint_notice banner"
        );
    })
    .unwrap();
}

#[gpui_kit::test]
fn large_500_cell_notebook_and_variables_virtualize_cleanly(cx: &mut TestAppContext) {
    let (handle, view, _) = mount(cx);
    cx.simulate_window_resize(handle, size(px(1280.), px(800.)));
    cx.update_window(handle, |_, window, cx| {
        let cells: Vec<super::Value> = (0..500)
            .map(|i| {
                json!({
                    "id": format!("cell_{i:03}"),
                    "type": if i % 5 == 0 { "markdown" } else { "code" },
                    "path": format!("cells/{i:03}.py"),
                    "source": format!("metric_{i} = {i} * 2\nprint(metric_{i})"),
                    "revision": format!("rev_{i}"),
                    "editable": true,
                    "state": "ok",
                })
            })
            .collect();
        view.update(cx, |v, cx| {
            let tab = &mut v.tabs[0];
            tab.model.apply_snapshot(
                serde_json::from_value(json!({
                    "state": "ok",
                    "manifest_revision": "stress-500",
                    "cells": cells,
                }))
                .unwrap(),
            );
            tab.list.reset(tab.model.order.len());
            tab.reveal = Some("cell_499".into());
            cx.notify();
        });
        view.update(cx, |v, cx| v.drain(window, cx));
        window.render_frame(cx);
        assert_eq!(view.read(cx).tabs[0].model.order.len(), 500);
        assert_eq!(
            view.read(cx).tabs[0].model.selected.as_deref(),
            Some("cell_499")
        );

        let vars: Vec<super::Value> = (0..100)
            .map(|i| {
                json!({
                    "name": format!("var_{i:03}"),
                    "type": "ndarray",
                    "preview": format!("shape=(1024, {i})"),
                    "handle": format!("h_{i}")
                })
            })
            .collect();
        view.update(cx, |v, cx| {
            v.tabs[0]
                .inspections
                .insert("Variables".into(), json!({"items": vars}));
            v.select_surface("Variables", cx);
        });
        window.render_frame(cx);
        assert_eq!(view.read(cx).surface, "Variables");
    })
    .unwrap();
}

#[gpui_kit::test]
fn cell_boundary_spacing_and_inline_button_alignment_match_marimo_rhythm(cx: &mut TestAppContext) {
    let (handle, view, _) = mount(cx);
    cx.simulate_window_resize(handle, size(px(1280.), px(850.)));
    cx.update_window(handle, |_, window, cx| {
        view.update(cx, |v, cx| {
            v.tabs[0].model.selected = Some("first".into());
            v.tabs[0].model.cells.get_mut("first").unwrap().ordinal = Some(1);
            cx.notify();
        });
        assert_eq!(
            super::notebook::cell_border_color(false, false, false, false, cx),
            super::surface_edge(cx),
            "unselected cells at rest must use the surface_edge hairline"
        );
        assert_eq!(super::notebook::boundary_button_label(false), "Insert");
        assert_eq!(super::notebook::boundary_button_label(true), "Add cell");
        window.render_frame(cx);
        window.render_frame(cx);
    })
    .unwrap();

    let mut visual = gpui_kit::VisualTestContext::from_window(handle, cx);
    let card_first = visual
        .debug_bounds("cell-card-first")
        .expect("first cell card");
    let card_second = visual
        .debug_bounds("cell-card-second")
        .expect("second unselected cell card");
    assert_eq!(
        card_first.size.width, card_second.size.width,
        "selected and unselected cell cards must share identical card frame width"
    );
    let boundary_first = visual
        .debug_bounds("cell-boundary-first")
        .expect("first cell boundary");
    let (insert_first, insert_last) = cx
        .update_window(handle, |_, window, _| {
            (
                window.find(("insert-cell", 0usize)).bounds(),
                window.find(("insert-cell", 1usize)).bounds(),
            )
        })
        .unwrap();
    assert!(
        insert_first.size.width >= px(64.),
        "visible middle boundary must render the '+ Insert' pill rather than an icon-only circle, got {:?}",
        insert_first.size
    );
    assert_eq!(
        insert_first.size.height, insert_last.size.height,
        "middle '+ Insert' and trailing '+ Add cell' pills must share the same height"
    );

    // Verify generous Marimo-style breathing room between the cell card and the + button.
    assert!(
        boundary_first.size.height >= px(36.),
        "compact inter-cell boundary must be at least 36px tall, got {:?}",
        boundary_first.size.height
    );
    assert!(
        insert_first.origin.y >= card_first.origin.y + card_first.size.height + px(3.),
        "insert button {:?} must have clear vertical gap below card {:?}",
        insert_first,
        card_first
    );
    assert!(
        (insert_first.center().y - boundary_first.center().y).abs() <= px(1.),
        "insert button must be vertically centered in its boundary slot"
    );

    // Cell tools share one centerline and float above the source, never over it.
    let (run_btn, assistant_btn, more_btn) = cx
        .update_window(handle, |_, window, _| {
            (
                window.find(("run", 0usize)).bounds(),
                window.find(("cell-assistant", 0usize)).bounds(),
                window.find(("cell-more", 0usize)).bounds(),
            )
        })
        .unwrap();
    assert!(
        (assistant_btn.center().y - more_btn.center().y).abs() <= px(1.),
        "assistant button {:?} and more button {:?} must align on the same row centerline",
        assistant_btn,
        more_btn
    );
    let source_first = visual
        .debug_bounds("cell-source-first")
        .expect("first cell source");
    assert!(
        more_btn.origin.y + more_btn.size.height <= source_first.origin.y,
        "cell tools {more_btn:?} must not cover the source {source_first:?}"
    );
    assert!(
        run_btn.origin.x + run_btn.size.width <= source_first.origin.x,
        "run control {run_btn:?} must sit in the margin left of the source {source_first:?}"
    );

    // Verify comfortable mode expands the inter-cell boundary to >= 48px.
    cx.update_window(handle, |_, window, cx| {
        view.update(cx, |v, cx| {
            v.preferences.comfortable = true;
            v.tabs[0].list.splice(0..2, 2);
            cx.notify();
        });
        window.render_frame(cx);
        window.render_frame(cx);
    })
    .unwrap();
    let boundary_comfortable = visual
        .debug_bounds("cell-boundary-first")
        .expect("comfortable boundary");
    assert!(
        boundary_comfortable.size.height >= px(48.),
        "comfortable inter-cell boundary must be at least 48px tall, got {:?}",
        boundary_comfortable.size.height
    );
}

#[gpui_kit::test]
fn provider_choice_is_a_draft_and_one_save_uses_the_selected_endpoint(cx: &mut TestAppContext) {
    let (handle, view, commands) = mount(cx);
    cx.update_window(handle, |_, window, cx| {
        view.update(cx, |v, _| {
            v.surface = "Settings".into();
            v.settings_section = "assistant";
            v.tabs[0].assistant.settings = json!({"settings":{"provider":"openai-compatible","model":"test-model","providers":{"openai-compatible":{"base_url":"http://localhost:11434/v1","structured_output":true}}}});
        });
        window.render_frame(cx);
        window.render_frame(cx);
        assert!(window.try_find("ai-load-settings").is_none());
        assert!(window.try_find("ai-models").is_none());
        assert!(window.try_find("ai-save-key").is_none());
        window.click("ai-provider-openai", cx);
        window.render_frame(cx);
        assert_eq!(view.read(cx).tabs[0].assistant.provider_draft.as_deref(), Some("openai"));
        assert_eq!(view.read(cx).tabs[0].assistant.settings["settings"]["provider"], "openai-compatible");
        assert!(commands.try_iter().all(|c| !matches!(c, Command::Assistant { .. })), "choosing a provider must not save settings or contact its endpoint");
        window.click("ai-save-provider", cx);
    }).unwrap();
    let sent = commands.try_iter().collect::<Vec<_>>();
    assert!(sent.iter().any(|c| matches!(c, Command::Assistant {method,fields} if method == "settings.update" && fields["settings"]["provider"] == "openai" && fields["settings"]["providers"]["openai"]["base_url"] == "https://api.openai.com/v1" && fields["settings"]["providers"]["openai-compatible"]["structured_output"] == true)));
    assert!(!sent.iter().any(|c|matches!(c, Command::Assistant {method,..} if method == "providers.test" || method == "credential.set")));
    // Saving another settings section must not silently switch the provider draft.
    let (send, receive) = mpsc::channel();
    cx.update_window(handle, |_, window, cx| {
        view.update(cx, |v, _| {
            v.tick = gpui_kit::Task::ready(());
            v.tabs[0].service.events = receive;
        });
        for (provider, expected) in [("openai-compatible", Some("openai")), ("openai", None)] {
            send.send(kernelledger_client::service::Event::AssistantDone {
                method: "settings.update".into(),
                value: json!({"settings":{"provider":provider}}),
            })
            .unwrap();
            view.update(cx, |v, cx| v.drain(window, cx));
            assert_eq!(
                view.read(cx).tabs[0].assistant.provider_draft.as_deref(),
                expected
            );
        }
    })
    .unwrap();
}

#[gpui_kit::test]
fn compact_assistant_insets_notebook_and_empty_checkpoints_card_includes_action(
    cx: &mut TestAppContext,
) {
    let (handle, view, _) = mount(cx);
    cx.simulate_window_resize(handle, size(px(800.), px(600.)));
    cx.update_window(handle, |_, window, cx| {
        view.update(cx, |v, cx| {
            v.tabs[0].assistant.open = true;
            cx.notify();
        });
        window.render_frame(cx);
        window.render_frame(cx);
    })
    .unwrap();
    let mut visual = gpui_kit::VisualTestContext::from_window(handle, cx);
    let card_first = visual.debug_bounds("cell-card-first").unwrap();
    let assistant = visual.debug_bounds("assistant-inspector").unwrap();
    let editor = visual.debug_bounds("cell-editor-first").unwrap();
    assert!(
        editor.size.width >= px(330.),
        "compact split must reserve the cell width for source, got {:?}",
        editor
    );
    assert!(
        card_first.origin.x + card_first.size.width <= assistant.origin.x + px(1.),
        "at 800px compact width, notebook cell {:?} must sit beside assistant drawer {:?} without being covered",
        card_first,
        assistant
    );

    cx.update_window(handle, |_, window, cx| {
        view.update(cx, |v, cx| {
            v.surface = "Checkpoints".into();
            v.tabs[0].runtime = json!({"worker_state": "stopped"});
            v.tabs[0]
                .inspections
                .insert("Checkpoints".into(), json!({"items": [], "cursor": null}));
            cx.notify();
        });
        window.render_frame(cx);
        window.render_frame(cx);
    })
    .unwrap();
    let empty_card = visual
        .debug_bounds("checkpoints-empty-card")
        .expect("empty checkpoints card");
    let hint = visual
        .debug_bounds("checkpoint-empty-hint")
        .expect("stopped runtime hint inside empty checkpoints card");
    let create_btn = cx
        .update_window(handle, |_, window, _| {
            window.find("create-checkpoint").bounds()
        })
        .unwrap();
    assert!(
        create_btn.origin.x >= empty_card.origin.x
            && create_btn.origin.y >= empty_card.origin.y
            && create_btn.origin.y + create_btn.size.height
                <= empty_card.origin.y + empty_card.size.height
            && hint.origin.y >= empty_card.origin.y
            && hint.origin.y + hint.size.height <= empty_card.origin.y + empty_card.size.height,
        "Create checkpoint button {:?} and hint {:?} must live inside the empty checkpoints card {:?}",
        create_btn,
        hint,
        empty_card
    );
}

#[gpui_kit::test]
fn html_load_cancellation_and_signal_tokens_prevent_stale_updates(cx: &mut TestAppContext) {
    use kernelledger_client::service::Event;
    let (handle, view, commands) = mount(cx);
    let (send, receive) = mpsc::channel();
    cx.update_window(handle, |_, window, cx| {
        view.update(cx, |v, cx| {
            v.tick = gpui_kit::Task::ready(());
            v.tabs[0].service.events = receive;
            v.preferences.allow_html_output = true;
            v.request_html_output(
                "first-0".into(),
                "preview".into(),
                Some("recorded-html".into()),
                "exec-1".into(),
                window,
                cx,
            );
        });
        let (key, token) = match commands.recv().unwrap() {
            Command::LoadHtml { key, token, digest } => {
                assert_eq!(digest, "recorded-html");
                (key, token)
            }
            command => panic!("Expected explicit HTML load, got {command:?}"),
        };
        assert!(view.read(cx).html_pending(&key));
        view.update(cx, |v, cx| v.stop_html_output(&key, cx));
        send.send(Event::HtmlLoaded {
            key: key.clone(),
            token,
            result: Ok("<script>stale()</script>".into()),
        })
        .unwrap();
        view.update(cx, |v, cx| v.drain(window, cx));
        assert!(view.read(cx).tabs[0].html_states.is_empty());
        assert!(view.read(cx).tabs[0].html_notes.is_empty());
        view.update(cx, |v, cx| {
            v.request_html_output(
                key.clone(),
                "<p>current</p>".into(),
                None,
                String::new(),
                window,
                cx,
            )
        });
        let state = &view.read(cx).tabs[0].html_states[&key];
        let current_token = state.token.clone();
        let signals = view.read(cx).html_signals.clone();
        signals
            .try_send(super::html_output::HtmlSignal {
                key: key.clone(),
                token: "stale".into(),
                value: json!({"kind":"height","height":700}),
            })
            .unwrap();
        view.update(cx, |v, cx| v.drain(window, cx));
        assert_eq!(
            view.read(cx).html_height(&key),
            super::html_output::HOST_HEIGHT
        );
        signals
            .try_send(super::html_output::HtmlSignal {
                key: key.clone(),
                token: current_token,
                value: json!({"kind":"height","height":1_000_000}),
            })
            .unwrap();
        view.update(cx, |v, cx| v.drain(window, cx));
        assert_eq!(view.read(cx).html_height(&key), 800.);
        view.update(cx, |v, cx| v.resize_html_output(&key, Some(-120.), cx));
        assert_eq!(view.read(cx).html_height(&key), 680.);
        view.update(cx, |v, cx| v.resize_html_output(&key, None, cx));
        assert_eq!(view.read(cx).html_height(&key), 800.);
    })
    .unwrap();
}

#[gpui_kit::test]
fn invalid_budget_is_reported_inline_and_blocks_save(cx: &mut TestAppContext) {
    let (handle, view, commands) = mount(cx);
    cx.update_window(handle, |_, window, cx| {
        view.update(cx, |v, _| {
            v.surface = "Settings".into();
            v.settings_section = "assistant";
            v.tabs[0].assistant.settings =
                json!({"settings":{"provider":"openai-compatible","model":"m"}});
        });
        window.render_frame(cx);
        let cap = view
            .read(cx)
            .prompts
            .iter()
            .find(|(key, _)| key.ends_with("-Daily token cap"))
            .map(|(_, input)| input.clone())
            .expect("daily token field");
        cap.update(cx, |s, cx| s.set_value("abc", window, cx));
        window.render_frame(cx);
        window.click("ai-save-provider", cx);
        window.render_frame(cx);
    })
    .unwrap();
    assert!(
        !commands
            .try_iter()
            .any(|c| matches!(c, Command::Assistant { method, .. } if method == "settings.update")),
        "an invalid budget must not be saved"
    );
}
