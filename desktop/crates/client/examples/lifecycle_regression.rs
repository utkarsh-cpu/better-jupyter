//! Exercises new lifecycle and first-use behavior on a disposable directory.
use kernelledger_client::{
    service::{Command, Event, NotebookService},
    *,
};
use serde_json::{Value, json};
use std::{
    path::PathBuf,
    time::{Duration, Instant},
};
use uuid::Uuid;

fn wait(service: &NotebookService, predicate: impl Fn(&Event) -> bool) -> anyhow::Result<Event> {
    let deadline = Instant::now() + Duration::from_secs(20);
    while Instant::now() < deadline {
        if let Ok(event) = service.events.recv_timeout(Duration::from_millis(100)) {
            if let Event::Error { code, message } = &event {
                anyhow::bail!("{code}: {message}");
            }
            if predicate(&event) {
                return Ok(event);
            }
        }
    }
    anyhow::bail!("Timed out waiting for lifecycle event")
}
fn stopped(service: &NotebookService, project: &std::path::Path) -> anyhow::Result<()> {
    wait(
        service,
        |e| matches!(e, Event::Runtime(v) if v["worker_state"] == "stopped"),
    )?;
    anyhow::ensure!(!project.join(".kl/supervisor.json").exists());
    let deadline = Instant::now() + Duration::from_millis(600);
    while Instant::now() < deadline {
        if let Ok(Event::Error { code, message }) =
            service.events.recv_timeout(Duration::from_millis(50))
        {
            anyhow::bail!("After stop: {code}: {message}");
        }
    }
    service.commands.send(Command::Inspect {
        method: "sessions.list".into(),
        fields: json!({}),
    })?;
    wait(
        service,
        |e| matches!(e, Event::Inspection { method, value } if method == "sessions.list" && value["items"].as_array().is_some_and(|v| !v.is_empty())),
    )?;
    Ok(())
}
fn main() -> anyhow::Result<()> {
    let mut args = std::env::args_os().skip(1);
    let project = PathBuf::from(args.next().expect("disposable empty directory"));
    let python = PathBuf::from(args.next().expect("Python"));
    let service = NotebookService::start(project.clone(), python);
    service
        .commands
        .send(Command::Initialize { sample: true })?;
    let Event::Source(snapshot) = wait(
        &service,
        |e| matches!(e, Event::Source(v) if v["state"] == "ok" && v["cells"].as_array().is_some_and(|v| v.len() == 3)),
    )?
    else {
        unreachable!()
    };
    let id = snapshot["cells"][1]["id"].as_str().unwrap().to_string();
    // Export must save the latest editor content before the helper reads source,
    // and it must remain usable without starting a Python worker.
    let edited_source = "print('exported current source')\n";
    service.commands.send(Command::Edit {
        cell_id: id.clone(),
        request: json!({"action":"edit","cell_id":id,"source":edited_source,"manifest_revision":snapshot["manifest_revision"],"cell_revisions":{id.clone():snapshot["cells"][1]["revision"]}}),
    })?;
    let export_path = project.join("latest.ipynb");
    service.commands.send(Command::Inspect {
        method: "export_ipynb".into(),
        fields: json!({"path":export_path}),
    })?;
    wait(
        &service,
        |e| matches!(e, Event::Inspection { method, .. } if method == "export_ipynb"),
    )?;
    let exported: Value = serde_json::from_slice(&std::fs::read(&export_path)?)?;
    anyhow::ensure!(
        exported["cells"]
            .as_array()
            .unwrap()
            .iter()
            .any(|cell| cell["source"].as_array().is_some_and(|lines| lines
                .iter()
                .filter_map(Value::as_str)
                .collect::<String>()
                == edited_source)
                || cell["source"] == edited_source),
        "Export did not include the acknowledged edit"
    );
    anyhow::ensure!(
        !project.join(".kl/supervisor.json").exists(),
        "Offline source export started a runtime"
    );
    service.commands.send(Command::Inspect {
        method: "source.read".into(),
        fields: json!({}),
    })?;
    let Event::Inspection {
        value: snapshot, ..
    } = wait(
        &service,
        |e| matches!(e, Event::Inspection { method, .. } if method == "source.read"),
    )?
    else {
        unreachable!()
    };
    let run = || Command::Run {
        cell_ids: vec![id.clone()],
        on_error: "stop".into(),
    };
    service.commands.send(run())?;
    wait(
        &service,
        |e| matches!(e, Event::Operation(v) if v["state"] == "completed"),
    )?;
    service.commands.send(Command::Runtime {
        method: "stop".into(),
        fields: json!({}),
    })?;
    stopped(&service, &project)?;
    let bundle_path = project.join("portable.zip");
    service.commands.send(Command::Inspect {
        method: "bundle_export".into(),
        fields: json!({"path":bundle_path}),
    })?;
    wait(
        &service,
        |e| matches!(e, Event::Inspection { method, .. } if method == "bundle_export"),
    )?;
    anyhow::ensure!(
        std::fs::read(&bundle_path)?.starts_with(b"PK"),
        "Bundle must be a ZIP archive"
    );
    anyhow::ensure!(
        !project.join(".kl/supervisor.json").exists(),
        "Offline export started a runtime"
    );

    service.commands.send(Command::Edit { cell_id: id.clone(), request: json!({"action":"edit","cell_id":id,"source":"import time\ntime.sleep(30)","manifest_revision":snapshot["manifest_revision"],"cell_revisions":{id.clone():snapshot["cells"][1]["revision"]}}) })?;
    service.commands.send(run())?;
    wait(
        &service,
        |e| matches!(e, Event::Runtime(v) if v["worker_state"] == "running"),
    )?;
    service.commands.send(Command::Runtime {
        method: "stop".into(),
        fields: json!({}),
    })?;
    stopped(&service, &project)?;

    service.commands.send(run())?;
    wait(
        &service,
        |e| matches!(e, Event::Runtime(v) if v["worker_state"] == "running"),
    )?;
    let identity = ClientIdentity::load(&profile_path())?;
    let mut other = RuntimeClient::connect(&project, &identity, "control")?;
    other.request("stop", json!({"client_request_id":Uuid::new_v4()}))?;
    stopped(&service, &project)?;
    let mut cursor = Value::Null;
    let mut seen = std::collections::BTreeSet::new();
    loop {
        let fields = json!({"page_size":1,"cursor":cursor});
        service.commands.send(Command::Inspect {
            method: "executions.list".into(),
            fields: fields.clone(),
        })?;
        let Event::Inspection { value, .. } = wait(
            &service,
            |e| matches!(e, Event::Inspection { method, .. } if method == "executions.list"),
        )?
        else {
            unreachable!()
        };
        anyhow::ensure!(
            value["_page_request"] == fields,
            "Page navigation lost its request scope"
        );
        let items = value["items"].as_array().unwrap();
        anyhow::ensure!(items.len() == 1, "Requested a single execution per page");
        anyhow::ensure!(
            seen.insert(items[0]["exec_id"].as_str().unwrap().to_owned()),
            "Paging repeated an execution"
        );
        cursor = value["cursor"].clone();
        if cursor.is_null() {
            break;
        }
        anyhow::ensure!(seen.len() < 10, "Pagination did not terminate");
    }
    anyhow::ensure!(seen.len() == 3, "Paging omitted recorded executions");
    println!(
        "{}",
        json!({"sample_creation":"passed","stop_idle":"passed","stop_busy":"passed","stop_other_client":"passed","offline_history":"passed","run_restarts":"passed","export_saves_current_source":"passed","offline_bundle_export":"passed","offline_history_paging":"passed"})
    );
    service.commands.send(Command::Detach)?;
    let _: Value = snapshot;
    Ok(())
}
