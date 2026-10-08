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
    println!(
        "{}",
        json!({"sample_creation":"passed","stop_idle":"passed","stop_busy":"passed","stop_other_client":"passed","offline_history":"passed","run_restarts":"passed"})
    );
    service.commands.send(Command::Detach)?;
    let _: Value = snapshot;
    Ok(())
}
