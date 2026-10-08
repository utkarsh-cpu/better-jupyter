//! One offline helper per canonical notebook, persistent operation keys and background I/O.
use super::*;
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::mpsc::{self, Receiver, Sender},
    time::Instant,
};

#[derive(Debug, Clone)]
pub enum Command {
    Edit {
        cell_id: String,
        request: Value,
    },
    Save,
    Initialize {
        sample: bool,
    },
    Mutate(Value),
    Run {
        cell_ids: Vec<String>,
        on_error: String,
    },
    Runtime {
        method: String,
        fields: Value,
    },
    Inspect {
        method: String,
        fields: Value,
    },
    SelectInterpreter {
        path: PathBuf,
        remember: bool,
    },
    Refresh,
    DiscardDraft(String),
    RestoreDraft(Value),
    Detach,
}
#[derive(Debug, Clone)]
pub enum Event {
    Source(Value),
    Saved {
        cell_id: String,
        submitted: String,
        snapshot: Value,
    },
    Runtime(Value),
    Operation(Value),
    Inspection {
        method: String,
        value: Value,
    },
    Output(Value),
    Draft(Value),
    Error {
        code: String,
        message: String,
    },
}
#[derive(Clone)]
struct Edit {
    request: Value,
    deadline: Instant,
}

pub struct NotebookService {
    pub commands: Sender<Command>,
    pub events: Receiver<Event>,
}
impl NotebookService {
    pub fn start(project: PathBuf, python: PathBuf) -> Self {
        let (commands, receive) = mpsc::channel();
        let (send, events) = mpsc::channel();
        std::thread::spawn(move || {
            if let Err(error) = Session::new(project, python, send.clone())
                .and_then(|mut session| session.run(receive))
            {
                report(&send, error);
            }
        });
        Self { commands, events }
    }
}
impl Drop for NotebookService {
    fn drop(&mut self) {
        let _ = self.commands.send(Command::Detach);
    }
}

struct Session {
    project: PathBuf,
    python: PathBuf,
    profile: PathBuf,
    identity: ClientIdentity,
    helper: HelperClient,
    runtime: Option<RuntimeClient>,
    worker_python: Option<PathBuf>,
    snapshot: Value,
    edits: BTreeMap<String, Edit>,
    conflicts: BTreeSet<String>,
    operations: BTreeMap<String, Value>,
    events: Sender<Event>,
    last_poll: Instant,
    last_source: Instant,
    folds: BTreeMap<String, Value>,
    last_status: Value,
    stopping_since: Option<Instant>,
    stopped: bool,
    disconnect_since: Option<Instant>,
}
impl Session {
    fn new(project: PathBuf, python: PathBuf, events: Sender<Event>) -> Result<Self> {
        let project = project.canonicalize()?;
        let profile = profile_path();
        let identity = ClientIdentity::load(&profile)?;
        let helper = HelperClient::start(&python, &project, &identity)?;
        let mut session = Self {
            project,
            python,
            profile,
            identity,
            helper,
            runtime: None,
            worker_python: None,
            snapshot: Value::Null,
            edits: BTreeMap::new(),
            conflicts: BTreeSet::new(),
            operations: BTreeMap::new(),
            events,
            last_poll: Instant::now(),
            last_source: Instant::now(),
            folds: BTreeMap::new(),
            last_status: Value::Null,
            stopping_since: None,
            stopped: false,
            disconnect_since: None,
        };
        session.refresh()?;
        let _ = session.events.send(Event::Inspection {
            method: "build.identity".into(),
            value: session.helper.welcome.clone(),
        });
        let operations = session.operation_path();
        if operations.exists() {
            session.operations = serde_json::from_slice(&fs::read(operations)?)?;
        }
        let directory = session.draft_directory();
        if directory.exists() {
            for entry in fs::read_dir(directory)? {
                let entry = entry?;
                if entry.path().extension().is_some_and(|ext| ext == "json") {
                    let draft: Value = serde_json::from_slice(&fs::read(entry.path())?)?;
                    let _ = session.events.send(Event::Draft(draft));
                }
            }
        }
        // Attach to an existing supervisor, but opening a notebook never starts one.
        session.runtime =
            RuntimeClient::connect(&session.project, &session.identity, "control").ok();
        if let Some(runtime) = &mut session.runtime
            && let Ok(status) = runtime.request("runtime.status", json!({}))
        {
            session.last_status = status.clone();
            let _ = session.events.send(Event::Runtime(status));
        }
        Ok(session)
    }
    fn operation_path(&self) -> PathBuf {
        self.profile.join("operations").join(format!(
            "{}.json",
            hash(self.project.to_string_lossy().as_bytes())
        ))
    }
    fn draft_directory(&self) -> PathBuf {
        self.profile
            .join("drafts")
            .join(hash(self.project.to_string_lossy().as_bytes()))
    }
    fn draft_path(&self, id: &str) -> PathBuf {
        self.draft_directory()
            .join(format!("{}.json", hash(id.as_bytes())))
    }
    fn persist_operations(&self) -> Result<()> {
        write_profile(
            &self.operation_path(),
            &serde_json::to_vec(&self.operations)?,
        )
    }
    fn draft(&self, id: &str, request: &Value) -> Result<()> {
        let source = request["source"].as_str().unwrap_or_default();
        write_profile(
            &self.draft_path(id),
            &serde_json::to_vec(
                &json!({"project":self.project,"cell_id":id,"base_revision":request["cell_revisions"][id],"draft_revision":hash(source.as_bytes()),"source":source}),
            )?,
        )
    }
    fn refresh(&mut self) -> Result<()> {
        let snapshot = self.helper.request("source.read", json!({}))?;
        if snapshot != self.snapshot {
            self.snapshot = snapshot.clone();
            let _ = self.events.send(Event::Source(snapshot));
        }
        self.last_source = Instant::now();
        Ok(())
    }
    fn save(&mut self, id: &str) -> Result<()> {
        let Some(edit) = self.edits.get(id).cloned() else {
            return Ok(());
        };
        let source = edit.request["source"]
            .as_str()
            .unwrap_or_default()
            .to_string();
        let result = self.helper.request("source.mutate", edit.request.clone());
        let snapshot = match result {
            Ok(snapshot) => snapshot,
            Err(error) => {
                // A lost source reply is reconciled by content, never by blind overwrite.
                let disk = self.helper.request("source.read", json!({}))?;
                let applied = disk["cells"].as_array().is_some_and(|cells| {
                    cells
                        .iter()
                        .any(|cell| cell["id"] == id && cell["source"] == source)
                });
                if applied {
                    disk
                } else {
                    self.conflicts.insert(id.into());
                    self.edits.remove(id); // Draft stays on disk for an explicit conflict decision.
                    let _ = self.events.send(Event::Source(disk));
                    return Err(error);
                }
            }
        };
        self.snapshot = snapshot.clone();
        self.edits.remove(id);
        self.conflicts.remove(id);
        match fs::remove_file(self.draft_path(id)) {
            Err(error) if error.kind() != std::io::ErrorKind::NotFound => return Err(error.into()),
            _ => {}
        }
        let _ = self.events.send(Event::Saved {
            cell_id: id.into(),
            submitted: source,
            snapshot,
        });
        Ok(())
    }
    fn save_all(&mut self) -> Result<()> {
        for id in self.edits.keys().cloned().collect::<Vec<_>>() {
            self.save(&id)?;
        }
        Ok(())
    }
    fn ensure_runtime(&mut self) -> Result<()> {
        self.stopped = false;
        if self.runtime.is_some() {
            return Ok(());
        }
        self.runtime = RuntimeClient::connect(&self.project, &self.identity, "control").ok();
        if self.runtime.is_none() {
            let _ = self
                .events
                .send(Event::Runtime(json!({"worker_state":"starting"})));
            start_runtime(&self.python, &self.project, self.worker_python.as_deref())?;
            self.runtime = Some(RuntimeClient::connect(
                &self.project,
                &self.identity,
                "control",
            )?);
        }
        Ok(())
    }
    fn submit(&mut self, method: &str, mut fields: Value) -> Result<()> {
        self.ensure_runtime()?;
        if matches!(method, "training-checkpoint" | "training-restore") {
            ensure!(
                self.runtime.as_ref().unwrap().welcome["capabilities"]
                    .as_array()
                    .is_some_and(|values| values.iter().any(|v| v == "portable_training")),
                "This runtime does not advertise portable training checkpoints"
            );
        }
        let key = Uuid::new_v4().to_string();
        fields["client_request_id"] = json!(key);
        self.operations.insert(
            key.clone(),
            json!({"method":method,"request":fields,"state":"submitting","operation_id":null}),
        );
        self.persist_operations()?; // The key survives an application crash before any socket write.
        if method == "stop" {
            self.stopping_since = Some(Instant::now());
        }
        let result = self.runtime.as_mut().unwrap().request(method, fields);
        match result {
            Ok(mut accepted) => {
                self.operations.get_mut(&key).unwrap()["operation_id"] =
                    accepted["operation_id"].clone();
                self.operations.get_mut(&key).unwrap()["state"] = json!("accepted");
                self.persist_operations()?;
                if method == "stop" {
                    self.stopping_since = Some(Instant::now());
                }
                accepted["kind"] = json!(method);
                accepted["state"] = json!("accepted");
                accepted["request"] = self.operations[&key]["request"].clone();
                let _ = self.events.send(Event::Operation(accepted));
            }
            Err(error) => {
                if error.downcast_ref::<ProtocolError>().is_some() {
                    self.operations.get_mut(&key).unwrap()["state"] = json!("failed");
                } else {
                    self.operations.get_mut(&key).unwrap()["state"] = json!("reconciling");
                    self.runtime = None;
                }
                self.persist_operations()?;
                return Err(error);
            }
        }
        Ok(())
    }
    fn handle(&mut self, command: Command) -> Result<bool> {
        match command {
            Command::Detach => return Ok(false),
            Command::DiscardDraft(id) => {
                self.edits.remove(&id);
                self.conflicts.remove(&id);
                if self.draft_path(&id).exists() {
                    fs::remove_file(self.draft_path(&id))?;
                }
            }
            Command::RestoreDraft(draft) => {
                let id = draft["cell_id"]
                    .as_str()
                    .ok_or_else(|| anyhow::anyhow!("Invalid draft cell id"))?
                    .to_string();
                let request = json!({"action":"edit","cell_id":id,"source":draft["source"],"manifest_revision":self.snapshot["manifest_revision"],"cell_revisions":{id.clone():draft["base_revision"]}});
                self.draft(&id, &request)?;
                self.edits.insert(
                    id.clone(),
                    Edit {
                        request,
                        deadline: Instant::now(),
                    },
                );
                self.save(&id)?;
            }
            Command::Edit { cell_id, request } => {
                self.draft(&cell_id, &request)?;
                self.edits.insert(
                    cell_id,
                    Edit {
                        request,
                        deadline: Instant::now() + Duration::from_millis(500),
                    },
                );
            }
            Command::Save => self.save_all()?,
            Command::Initialize { sample } => {
                let mut create = std::process::Command::new(&self.python);
                create
                    .args(["-m", "kernelledger", "--project"])
                    .arg(&self.project)
                    .arg("init");
                hide_console(&mut create);
                let result = create.output()?;
                ensure!(
                    result.status.success(),
                    "{}",
                    String::from_utf8_lossy(&result.stderr)
                );
                self.refresh()?;
                if sample {
                    for (kind, source) in [
                        (
                            "markdown",
                            "# Welcome to KernelLedger\nEdit the next cell, then press Ctrl+Enter to run it.",
                        ),
                        ("code", "values = [10, 20, 30]\nprint(sum(values))"),
                        (
                            "code",
                            "# A deliberate error to explore recorded tracebacks\n1 / 0",
                        ),
                    ] {
                        let snapshot = self.helper.request("source.mutate", json!({"action":"add","type":kind,"source":source,"manifest_revision":self.snapshot["manifest_revision"]}))?;
                        self.snapshot = snapshot;
                    }
                    let _ = self.events.send(Event::Source(self.snapshot.clone()));
                }
            }
            Command::Mutate(request) => {
                self.save_all()?;
                let edited_id = request["cell_id"].as_str().map(str::to_owned);
                let edit = request["action"] == "edit";
                let snapshot = self.helper.request("source.mutate", request)?;
                if edit && let Some(id) = edited_id {
                    self.conflicts.remove(&id);
                    if self.draft_path(&id).exists() {
                        fs::remove_file(self.draft_path(&id))?;
                    }
                }
                self.snapshot = snapshot.clone();
                let _ = self.events.send(Event::Source(snapshot));
            }
            Command::Run { cell_ids, on_error } => {
                ensure!(
                    !cell_ids.iter().any(|id| self.conflicts.contains(id)),
                    "Resolve the source conflict before running"
                );
                self.save_all()?;
                self.refresh()?;
                let mut hashes = serde_json::Map::new();
                for id in &cell_ids {
                    let cell = self.snapshot["cells"]
                        .as_array()
                        .and_then(|cells| cells.iter().find(|cell| cell["id"] == *id))
                        .ok_or_else(|| anyhow::anyhow!("Cell removed from disk"))?;
                    hashes.insert(id.clone(), cell["revision"].clone());
                }
                if cell_ids.len() == 1 {
                    self.submit(
                        "run",
                        json!({"cell_id":cell_ids[0],"expected_sha256":hashes[&cell_ids[0]]}),
                    )?;
                } else {
                    self.submit(
                        "batch",
                        json!({"cell_ids":cell_ids,"expected_sha256":hashes,"on_error":on_error}),
                    )?;
                }
            }
            Command::Runtime { method, fields } => {
                if matches!(
                    method.as_str(),
                    "stop" | "interrupt" | "interrupt_kill" | "restart" | "checkpoint"
                ) && self.runtime.is_none()
                {
                    bail!("No attached runtime. Run a code cell to start one.");
                }
                if matches!(
                    method.as_str(),
                    "interrupt"
                        | "interrupt_kill"
                        | "input.claim"
                        | "input.release"
                        | "input.reply"
                        | "input.cancel"
                        | "operation.cancel"
                ) {
                    self.ensure_runtime()?;
                    let result = self.runtime.as_mut().unwrap().request(&method, fields)?;
                    let _ = self.events.send(Event::Inspection {
                        method,
                        value: result,
                    });
                } else {
                    self.submit(&method, fields)?;
                }
            }
            Command::Inspect { method, mut fields } => {
                if matches!(method.as_str(), "export_ipynb" | "bundle_export") {
                    self.save_all()?;
                }
                let required_capability = match method.as_str() {
                    "training.list" | "training.inspect" => Some("portable_training"),
                    "native.status" => Some("native_snapshot_status"),
                    _ => None,
                };
                if let Some(capability) = required_capability {
                    ensure!(
                        self.helper.welcome["capabilities"]
                            .as_array()
                            .is_some_and(|values| values.iter().any(|v| v == capability)),
                        "This helper does not advertise {capability}"
                    );
                }
                let value = if method == "runtimes.list" {
                    super::registry::list(&self.identity)?
                } else {
                    let search_all = fields["search_all"] == true;
                    if let Some(fields) = fields.as_object_mut() {
                        fields.remove("search_all");
                    }
                    let mut value = if search_all && method == "executions.list" {
                        let items = all_pages(|page| self.helper.request(&method, page))?;
                        json!({"items":items,"cursor":null})
                    } else {
                        self.helper.request(&method, fields)?
                    };
                    if method == "executions.list"
                        && let Some(items) = value["items"].as_array_mut()
                    {
                        for item in items {
                            if let Some(id) = item["exec_id"].as_str()
                                && let Ok(fold) =
                                    self.helper.request("fold.get", json!({"exec_id":id}))
                            {
                                item["source"] = fold["source"].clone();
                            }
                        }
                    }
                    value
                };
                let _ = self.events.send(Event::Inspection { method, value });
            }
            Command::SelectInterpreter { path, remember } => {
                let validation = self
                    .helper
                    .request("interpreter.validate", json!({"path":path}))?;
                ensure!(
                    validation["supported"] == true,
                    "Requires CPython 3.11 or newer"
                );
                self.worker_python = Some(path.clone());
                if remember {
                    let config_path = self.project.join(".kl/config.json");
                    let mut config: Value = if config_path.exists() {
                        serde_json::from_slice(&fs::read(&config_path)?)?
                    } else {
                        json!({})
                    };
                    config["python"] = json!(path);
                    write_profile(&config_path, &serde_json::to_vec_pretty(&config)?)?;
                }
                let _ = self.events.send(Event::Inspection {
                    method: "interpreter.validate".into(),
                    value: validation,
                });
            }
            Command::Refresh => {
                self.refresh()?;
                self.runtime =
                    RuntimeClient::connect(&self.project, &self.identity, "control").ok();
            }
        }
        Ok(true)
    }
    fn poll(&mut self) -> Result<()> {
        self.last_poll = Instant::now();
        if let Some(runtime) = &mut self.runtime {
            let status = match runtime.request("runtime.status", json!({})) {
                Ok(status) => status,
                Err(error) => {
                    if !self.project.join(".kl/supervisor.json").exists() {
                        self.mark_stopped()?;
                        return Ok(());
                    }
                    if self
                        .stopping_since
                        .is_some_and(|started| started.elapsed() < Duration::from_secs(5))
                    {
                        return Ok(());
                    }
                    return Err(error);
                }
            };
            self.disconnect_since = None;
            if status != self.last_status {
                self.last_status = status.clone();
                let _ = self.events.send(Event::Runtime(status));
            }
            let keys = self
                .operations
                .iter()
                .filter(|(_, op)| {
                    !matches!(
                        op["state"].as_str(),
                        Some("completed" | "failed" | "canceled" | "unknown")
                    )
                })
                .map(|(key, _)| key.clone())
                .collect::<Vec<_>>();
            for key in keys {
                let lookup = json!({"client_request_id":key});
                match runtime.request("operation.get", lookup) {
                    Ok(mut operation) => {
                        self.operations.get_mut(&key).unwrap()["state"] =
                            operation["state"].clone();
                        self.operations.get_mut(&key).unwrap()["operation_id"] =
                            operation["operation_id"].clone();
                        operation["request"] = self.operations[&key]["request"].clone();
                        let _ = self.events.send(Event::Operation(operation));
                    }
                    Err(error)
                        if error
                            .downcast_ref::<ProtocolError>()
                            .is_some_and(|e| e.code == "not_found") =>
                    {
                        if self.operations[&key]["stopped_unconfirmed"] == true {
                            self.operations.get_mut(&key).unwrap()["state"] = json!("unknown");
                            let _ = self
                                .events
                                .send(Event::Operation(self.operations[&key].clone()));
                            continue;
                        }
                        // No durable acceptance exists. Retry exactly the persisted request and key.
                        let op = &self.operations[&key];
                        let accepted = runtime
                            .request(op["method"].as_str().unwrap(), op["request"].clone())?;
                        self.operations.get_mut(&key).unwrap()["operation_id"] =
                            accepted["operation_id"].clone();
                    }
                    Err(error) => return Err(error),
                }
            }
            self.persist_operations()?;
        }
        // Read committed events through the offline helper and reconcile with runtime fold.get.
        let executions = all_pages(|fields| self.helper.request("executions.list", fields))?;
        let mut latest = BTreeMap::new();
        for row in executions {
            if let Some(id) = row["cell_id"].as_str() {
                latest.insert(id.to_string(), row);
            }
        }
        for row in latest.values() {
            let eid = row["exec_id"].as_str().unwrap();
            let mut fold = self.helper.request("fold.get", json!({"exec_id":eid}))?;
            if let Some(outputs) = fold["outputs"].as_array_mut() {
                for event in outputs {
                    if let Err(error) =
                        super::output::resolve_event(event, |digest| self.helper.blob(digest))
                    {
                        event["viewer_error"] = json!(error.to_string());
                    }
                }
            }
            if self.folds.get(eid) != Some(&fold) {
                self.folds.insert(eid.into(), fold.clone());
                let _ = self.events.send(Event::Output(fold));
            }
        }
        Ok(())
    }
    fn mark_stopped(&mut self) -> Result<()> {
        self.runtime = None;
        self.stopped = true;
        self.disconnect_since = None;
        self.stopping_since = None;
        // Retain unresolved outcomes for lookup after reconnect, without replaying them.
        for op in self.operations.values_mut() {
            if !matches!(
                op["state"].as_str(),
                Some("completed" | "failed" | "canceled" | "unknown")
            ) {
                op["state"] = json!("reconciling");
                op["stopped_unconfirmed"] = json!(true);
                let _ = self.events.send(Event::Operation(op.clone()));
            }
        }
        self.persist_operations()?;
        self.last_status = json!({"worker_state":"stopped","active_operations":[],"queue":[]});
        let _ = self.events.send(Event::Runtime(self.last_status.clone()));
        Ok(())
    }
    fn run(&mut self, commands: Receiver<Command>) -> Result<()> {
        loop {
            match commands.recv_timeout(Duration::from_millis(50)) {
                Ok(command) => match self.handle(command) {
                    Ok(false) => {
                        let _ = self.save_all();
                        return Ok(());
                    }
                    Err(error) if self.stopping_since.is_none() => {
                        report(&self.events, error);
                    }
                    _ => {}
                },
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    let _ = self.save_all();
                    return Ok(());
                }
                _ => {}
            }
            for id in self
                .edits
                .iter()
                .filter(|(_, e)| e.deadline <= Instant::now())
                .map(|(id, _)| id.clone())
                .collect::<Vec<_>>()
            {
                if let Err(error) = self.save(&id) {
                    report(&self.events, error);
                }
            }
            if self.last_source.elapsed() >= Duration::from_millis(500)
                && let Err(error) = self.refresh()
            {
                report(&self.events, error);
            }
            if self.stopping_since.is_some() && !self.project.join(".kl/supervisor.json").exists() {
                self.mark_stopped()?;
            }
            if self.last_poll.elapsed() >= Duration::from_millis(150)
                && let Err(error) = self.poll()
            {
                if !self.project.join(".kl/supervisor.json").exists()
                    && (self.runtime.is_some() || self.stopping_since.is_some())
                {
                    self.mark_stopped()?;
                    continue;
                }
                if self
                    .stopping_since
                    .is_some_and(|started| started.elapsed() < Duration::from_secs(5))
                {
                    continue;
                }
                let since = *self.disconnect_since.get_or_insert_with(Instant::now);
                if since.elapsed() < Duration::from_secs(5) {
                    continue;
                }
                self.runtime = None;
                self.stopping_since = None;
                let _ = self
                    .events
                    .send(Event::Runtime(json!({"worker_state":"disconnected"})));
                report(&self.events, error);
            }
        }
    }
}
fn write_profile(path: &Path, bytes: &[u8]) -> Result<()> {
    fs::create_dir_all(path.parent().unwrap())?;
    let temporary = path.with_extension(format!("{}.tmp", Uuid::new_v4()));
    let mut file = fs::File::create(&temporary)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    fs::rename(temporary, path)?;
    Ok(())
}
fn report(events: &Sender<Event>, error: anyhow::Error) {
    let code = error
        .downcast_ref::<ProtocolError>()
        .map(|error| error.code.clone())
        .unwrap_or_else(|| {
            if error.downcast_ref::<std::io::Error>().is_some() {
                "transport".into()
            } else {
                "internal".into()
            }
        });
    let _ = events.send(Event::Error {
        code,
        message: error.to_string(),
    });
}
