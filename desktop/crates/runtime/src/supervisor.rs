//! Experimental native legacy-protocol supervisor. Explicit invocation only.
//! The desktop surface advertises only its implemented capability subset.
#[path = "accept.rs"]
mod accept;
#[path = "desktop.rs"]
mod desktop;
#[path = "inputs.rs"]
mod inputs;
use crate::{
    capture::Capture,
    notebook,
    platform::{WorkerTree, python_command},
    source::SourceService,
    store::BlobStore,
    writer::{Mutation, Writer},
};
use anyhow::{Context, Result, bail, ensure};
use kernelledger_protocol::{read_worker, write_worker};
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    fs,
    io::Read,
    net::{TcpListener, TcpStream},
    path::{Path, PathBuf},
    process::Stdio,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
        mpsc,
    },
    thread,
    time::{Duration, Instant},
};
use uuid::Uuid;

const COMMAND_LIMIT: usize = 16;
const CLIENT_LIMIT: usize = 32;
const ENVELOPE_LIMIT: u64 = 16 * 1024 * 1024;
struct Job {
    input: Option<(Uuid, mpsc::SyncSender<Value>)>,
    request: Value,
    frozen: Option<(String, String)>,
    reply: mpsc::SyncSender<Result<Value>>,
    admitted: Option<Value>,
    _slot: QueueSlot,
}
struct QueueSlot {
    count: Arc<AtomicUsize>,
}
impl Drop for QueueSlot {
    fn drop(&mut self) {
        self.count.fetch_sub(1, Ordering::AcqRel);
    }
}
fn reserve(shared: &Shared) -> Result<QueueSlot> {
    shared
        .queued
        .try_update(Ordering::AcqRel, Ordering::Acquire, |count| {
            (count < COMMAND_LIMIT).then_some(count + 1)
        })
        .map_err(|_| crate::store::coded("busy", "runtime queue is full", json!({})))?;
    Ok(QueueSlot {
        count: shared.queued.clone(),
    })
}
#[derive(Default)]
struct Shared {
    identity: Value,
    status: Mutex<Value>,
    socket: Mutex<Option<TcpStream>>,
    worker_send: Mutex<()>,
    validation: Mutex<crate::resources::ValidationCache>,
    cache_validation: bool,
    cancel_delivery: Mutex<(Option<String>, bool)>,
    inputs: Mutex<inputs::Requests>,
    record_secret_input: bool,
    stopping: AtomicBool,
    killed: AtomicBool,
    finished: AtomicBool,
    clients: AtomicUsize,
    queued: Arc<AtomicUsize>,
    admission: Mutex<()>,
    active_operation: Mutex<Option<String>>,
    stop_operation: Mutex<Option<String>>,
    stop_admission: Mutex<()>,
    stop_error: Mutex<Option<String>>,
    connections: Mutex<HashMap<Uuid, TcpStream>>,
    desktop_clients: Mutex<HashMap<Uuid, Value>>,
}
struct Worker {
    process: Process,
    socket: TcpStream,
    events: mpsc::Receiver<Result<Value>>,
    session: String,
    environment: Value,
    _temporary: tempfile::TempDir,
}
struct Process {
    tree: WorkerTree,
    pipes: Capture,
}
impl Process {
    fn stop(&mut self) -> Result<()> {
        let terminated = self.tree.terminate();
        let captured = self.pipes.close();
        terminated?;
        captured
    }
}
impl Drop for Process {
    fn drop(&mut self) {
        let _ = self.stop();
    }
}

fn token() -> String {
    format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple())
}
fn receive(socket: &mut TcpStream, store: &BlobStore) -> Result<Value> {
    loop {
        let value = read_worker(&mut *socket)?;
        match value["kind"].as_str() {
            Some("blob") => {
                store.receive(&mut *socket, &value)?;
            }
            Some("envelope") => {
                let digest = value["blob"].as_str().context("missing envelope digest")?;
                let mut file = store.open(digest)?;
                ensure!(
                    store.verify_file(&mut file, digest)?.size <= ENVELOPE_LIMIT,
                    "envelope exceeds native structured-message limit"
                );
                let decoded: Value = serde_json::from_slice(&store.get(digest)?)?;
                ensure!(
                    decoded.is_object() && decoded["schema"] == 1,
                    "invalid envelope"
                );
                return Ok(decoded);
            }
            _ => return Ok(value),
        }
    }
}

impl Worker {
    fn start(
        root: &Path,
        python: &Path,
        resource: &Path,
        ledger: &Writer,
        shared: &Shared,
        parent: Option<String>,
        checkpoint: Option<String>,
    ) -> Result<Self> {
        let listener = TcpListener::bind(("127.0.0.1", 0))?;
        listener.set_nonblocking(true)?;
        let secret = token();
        let temporary = tempfile::tempdir()?;
        let mut command = python_command(python)?;
        command
            .args(["-I", "-X", "utf8"])
            .arg(resource)
            .arg(root)
            .env("KL_WORKER_PORT", listener.local_addr()?.port().to_string())
            .env("KL_WORKER_TOKEN", &secret)
            .env("KL_TRAINING_TEMP_ROOT", temporary.path())
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let mut tree = WorkerTree::spawn(&mut command)?;
        let stdout = tree.child.stdout.take().context("missing worker stdout")?;
        let stderr = tree.child.stderr.take().context("missing worker stderr")?;
        let limit = fs::read(root.join(".kl/config.json"))
            .ok()
            .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
            .and_then(|config| config["output_limit_bytes"].as_u64())
            .unwrap_or(268435456);
        let pipes = Capture::start(stdout, stderr, ledger.clone(), usize::try_from(limit)?)?;
        let mut process = Process { tree, pipes };
        let deadline = Instant::now() + Duration::from_secs(20);
        let (mut socket, mut hello) = loop {
            ensure!(!shared.stopping.load(Ordering::Acquire), "runtime stopping");
            ensure!(Instant::now() < deadline, "worker handshake timed out");
            ensure!(
                process.tree.child.try_wait()?.is_none(),
                "worker exited before readiness"
            );
            match listener.accept() {
                Ok((mut socket, _)) => {
                    socket.set_nonblocking(false)?;
                    socket.set_read_timeout(Some(Duration::from_secs(2)))?;
                    socket.set_write_timeout(Some(Duration::from_secs(10)))?;
                    if let Ok(hello) = read_worker(&mut socket)
                        && hello["kind"] == "hello"
                        && hello["token"].as_str() == Some(secret.as_str())
                    {
                        break (socket, hello);
                    }
                }
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(5))
                }
                Err(e) => return Err(e.into()),
            }
        };
        socket.set_read_timeout(Some(Duration::from_secs(20)))?;
        ensure!(
            read_worker(&mut socket)?["kind"] == "ready",
            "worker failed readiness"
        );
        hello
            .as_object_mut()
            .context("worker hello")?
            .remove("token");
        hello.as_object_mut().unwrap().remove("schema");
        hello.as_object_mut().unwrap().remove("kind");
        let session = ledger.call(Mutation::Session {
            notebook: notebook::read(root)?["id"]
                .as_str()
                .context("notebook id")?
                .into(),
            environment: hello.clone(),
            parent,
            checkpoint,
        })?["session_id"]
            .as_str()
            .context("session id")?
            .to_owned();
        process.pipes.session(&session)?;
        socket.set_read_timeout(None)?;
        let mut input = socket.try_clone()?;
        let store = BlobStore::new(&root.join(".kl"));
        let (send, events) = mpsc::sync_channel(64);
        thread::Builder::new()
            .name("kl-worker-reader".into())
            .spawn(move || {
                loop {
                    let event = receive(&mut input, &store);
                    let failed = event.is_err();
                    if send.send(event).is_err() || failed {
                        break;
                    }
                }
            })?;
        *shared.socket.lock().unwrap() = Some(socket.try_clone()?);
        *shared.status.lock().unwrap() = json!({"pid":std::process::id(),"session_id":session,"worker_python":hello["executable"],"environment":hello,"in_flight":null,"worker_start_stage":"ready","implementation":"rust","experimental":true});
        Ok(Self {
            process,
            socket,
            events,
            session,
            environment: hello,
            _temporary: temporary,
        })
    }
    fn wait(&mut self, shared: &Shared) -> Result<Value> {
        loop {
            self.process.pipes.check()?;
            if shared.stopping.load(Ordering::Acquire)
                || shared.killed.swap(false, Ordering::AcqRel)
            {
                self.process.stop()?;
                bail!("worker was stopped");
            }
            match self.events.recv_timeout(Duration::from_millis(20)) {
                Ok(Ok(value)) => {
                    if value["exec_id"].is_null()
                        && matches!(value["kind"].as_str(), Some("stdout" | "stderr"))
                    {
                        self.process
                            .pipes
                            .unattributed(value["kind"].as_str().unwrap(), &value)?;
                        continue;
                    }
                    return Ok(value);
                }
                Ok(Err(error)) => {
                    let _ = self.process.stop();
                    return Err(error);
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    ensure!(
                        self.process.tree.child.try_wait()?.is_none(),
                        "worker exited"
                    );
                }
                Err(_) => {
                    let _ = self.process.stop();
                    bail!("worker disconnected");
                }
            }
        }
    }
}

impl Worker {
    fn drain_background(&mut self, ledger: &Writer, shared: &Shared) -> Result<()> {
        // Bound each drain pass so background writers cannot starve queued work.
        for _ in 0..16 {
            let mut event = match self.events.try_recv() {
                Ok(event) => event?,
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => bail!("worker disconnected"),
            };
            let kind = event["kind"]
                .as_str()
                .context("missing background event kind")?
                .to_owned();
            if kind == "stdout" || kind == "stderr" {
                self.process.pipes.unattributed(&kind, &event)?;
                continue;
            }
            if kind == "stdin_request" {
                let _send = shared.worker_send.lock().unwrap();
                write_worker(
                    &mut self.socket,
                    &json!({"schema":1,"kind":"stdin_reply","error":"no active execution owns this input"}),
                )?;
            }
            let payload = event.as_object_mut().context("background event")?;
            payload.remove("schema");
            payload.remove("kind");
            if let Some(eid) = payload.remove("exec_id").filter(|value| !value.is_null()) {
                payload.insert("original_exec_id".into(), eid);
            }
            payload.insert("cell_attribution".into(), "unattributed".into());
            ledger.call(Mutation::SessionEvent {
                session: self.session.clone(),
                kind,
                payload: event,
            })?;
        }
        Ok(())
    }
}

/// Retain the exact Python graph validator and canonical manifest encoder.
/// It reads only the supplied document and trusted worker resource, never notebook source.
fn validate_manifest(
    python: &Path,
    resource: &Path,
    manifest: &Value,
    shared: &Shared,
) -> Result<Vec<u8>> {
    use std::io::Write;
    let data = serde_json::to_vec(manifest)?;
    ensure!(
        data.len() as u64 <= ENVELOPE_LIMIT,
        "manifest exceeds structured-message limit"
    );
    if shared.cache_validation
        && let Some(encoded) = shared.validation.lock().unwrap().get(&data)
    {
        return Ok(encoded);
    }
    let mut command = python_command(python)?;
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    let mut child = command
        .args(["-I", "-c", crate::resources::VALIDATOR_COMMAND])
        .arg(resource)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()?;
    let input = child
        .stdin
        .take()
        .context("validator input")?
        .write_all(&data);
    if let Err(error) = input {
        let _ = child.kill();
        let _ = child.wait();
        return Err(error.into());
    }
    let output = child.wait_with_output()?;
    ensure!(
        output.status.success(),
        "checkpoint graph validation failed"
    );
    ensure!(
        output.stdout.len() as u64 <= ENVELOPE_LIMIT,
        "encoded manifest too large"
    );
    if shared.cache_validation {
        shared
            .validation
            .lock()
            .unwrap()
            .insert(&data, &output.stdout);
    }
    Ok(output.stdout)
}

fn checkpoint(
    worker: &mut Worker,
    root: &Path,
    python: &Path,
    resource: &Path,
    ledger: &Writer,
    shared: &Shared,
) -> Result<Value> {
    let total_started = Instant::now();
    let mut stage = Instant::now();
    let mut timings = json!({});
    let config: Value = fs::read(root.join(".kl/config.json"))
        .ok()
        .and_then(|data| serde_json::from_slice(&data).ok())
        .unwrap_or_else(|| json!({}));
    {
        let _send = shared.worker_send.lock().unwrap();
        write_worker(
            &mut worker.socket,
            &json!({"schema":1,"kind":"checkpoint","denylist":config.get("checkpoint_denylist").cloned().unwrap_or_else(||json!([])),"thread_allowlist":config.get("thread_allowlist").cloned().unwrap_or_else(||json!(["tqdm._monitor.TMonitor"]))}),
        )?;
    }
    let offer = loop {
        let event = worker.wait(shared)?;
        if event["kind"] == "checkpoint_offer" {
            break event;
        }
    };
    let Some(manifest) = offer.get("manifest") else {
        return Ok(offer);
    };
    timings["worker_offer_seconds"] = stage.elapsed().as_secs_f64().into();
    stage = Instant::now();
    let encoded = validate_manifest(python, resource, manifest, shared)?;
    timings["validation_seconds"] = stage.elapsed().as_secs_f64().into();
    stage = Instant::now();
    let digest = BlobStore::new(&root.join(".kl")).publish(&encoded, None)?;
    timings["manifest_publish_seconds"] = stage.elapsed().as_secs_f64().into();
    stage = Instant::now();
    let status = if manifest["excluded"]
        .as_array()
        .context("excluded")?
        .is_empty()
    {
        "complete"
    } else {
        "partial"
    };
    let mut result = ledger.call(Mutation::Checkpoint {
        session: worker.session.clone(),
        manifest: digest,
        status: status.into(),
    })?;
    result["excluded"] = manifest["excluded"].clone();
    result["notes"] = manifest["notes"].clone();
    timings["ledger_publication_seconds"] = stage.elapsed().as_secs_f64().into();
    timings["total_seconds"] = total_started.elapsed().as_secs_f64().into();
    if std::env::var("KERNELLEDGER_PROFILE").as_deref() == Ok("1") {
        result["timings"] = timings;
    }
    Ok(result)
}

fn send_stored(socket: &mut TcpStream, store: &BlobStore, digest: &str) -> Result<()> {
    let mut file = store.open(digest)?;
    let evidence = store.verify_file(&mut file, digest)?;
    send_verified(socket, file, digest, &evidence)
}

fn send_verified(
    socket: &mut TcpStream,
    mut file: fs::File,
    digest: &str,
    evidence: &crate::store::Verified,
) -> Result<()> {
    use sha2::{Digest, Sha256};
    use std::io::{Seek, SeekFrom, Write};
    evidence.check_current(digest)?;
    file.seek(SeekFrom::Start(0))?;
    write_worker(
        &mut *socket,
        &json!({"schema":1,"kind":"blob","sha256":digest,"byte_size":evidence.size}),
    )?;
    let mut remaining = evidence.size;
    let mut buffer = vec![0; kernelledger_protocol::BUFFER_SIZE];
    let mut hasher = Sha256::new();
    while remaining > 0 {
        let count = remaining.min(buffer.len() as u64) as usize;
        file.read_exact(&mut buffer[..count])?;
        hasher.update(&buffer[..count]);
        socket.write_all(&buffer[..count])?;
        remaining -= count as u64;
    }
    evidence.check_current(digest)?;
    ensure!(
        format!("{:x}", hasher.finalize()) == digest,
        "blob changed during transfer"
    );
    Ok(())
}

fn restore(
    worker: &mut Option<Worker>,
    root: &Path,
    python: &Path,
    resource: &Path,
    ledger: &Writer,
    shared: &Shared,
    request: &Value,
) -> Result<Value> {
    let total_started = Instant::now();
    let mut stage = Instant::now();
    let mut timings = json!({});
    let checkpoint = request["checkpoint_id"]
        .as_str()
        .context("checkpoint_id is required")?;
    let reader = crate::ledger::Ledger::open(&root.join(".kl"))?
        .context("history is empty")?
        .0;
    let row = reader.one(
        "SELECT * FROM checkpoints WHERE checkpoint_id=?",
        &[checkpoint.to_owned().into()],
    )?;
    ensure!(
        row["origin"] == "local" || request["trust"] == true,
        "imported checkpoint executes saved source; pass --trust"
    );
    let store = BlobStore::new(&root.join(".kl"));
    let digest = row["manifest_sha256"].as_str().context("manifest digest")?;
    let closure = store.verified_closure(&[digest.to_owned()])?;
    let manifest: Value = serde_json::from_slice(&store.get(digest)?)?;
    timings["read_verify_seconds"] = stage.elapsed().as_secs_f64().into();
    stage = Instant::now();
    validate_manifest(python, resource, &manifest, shared)?;
    timings["validation_seconds"] = stage.elapsed().as_secs_f64().into();
    stage = Instant::now();
    let saved_status = shared.status.lock().unwrap().clone();
    let saved_socket = shared
        .socket
        .lock()
        .unwrap()
        .as_ref()
        .map(TcpStream::try_clone)
        .transpose()?;
    let parent = row["session_id"]
        .as_str()
        .context("checkpoint session")?
        .to_owned();
    let started = Worker::start(
        root,
        python,
        resource,
        ledger,
        shared,
        Some(parent),
        Some(checkpoint.into()),
    );
    let mut candidate = match started {
        Ok(candidate) => candidate,
        Err(error) => {
            *shared.status.lock().unwrap() = saved_status;
            *shared.socket.lock().unwrap() = saved_socket;
            return Err(error);
        }
    };
    timings["candidate_start_seconds"] = stage.elapsed().as_secs_f64().into();
    stage = Instant::now();
    shared.status.lock().unwrap()["worker_start_stage"] = "restore".into();
    let reconstructed = (|| -> Result<Value> {
        let minor = |text: &str| text.split('.').take(2).collect::<Vec<_>>().join(".");
        if minor(
            candidate.environment["python"]
                .as_str()
                .context("worker Python version")?,
        ) != minor(
            manifest["python"]
                .as_str()
                .context("checkpoint Python version")?,
        ) && request["allow_python_mismatch"] != true
        {
            bail!("Python major.minor mismatch; pass --allow-python-mismatch");
        }
        {
            let _send = shared.worker_send.lock().unwrap();
            for (digest, evidence) in closure {
                send_verified(
                    &mut candidate.socket,
                    store.open(&digest)?,
                    &digest,
                    &evidence,
                )?;
            }
            // Large restore commands use the existing blob-backed worker envelope.
            let command = json!({"schema":1,"kind":"restore","manifest":manifest});
            let data = serde_json::to_vec(&command)?;
            if data.len() > kernelledger_protocol::WORKER_INLINE {
                let digest = store.publish(&data, None)?;
                send_stored(&mut candidate.socket, &store, &digest)?;
                write_worker(
                    &mut candidate.socket,
                    &json!({"schema":1,"kind":"envelope","blob":digest}),
                )?;
            } else {
                write_worker(&mut candidate.socket, &command)?;
            }
        }
        timings["upload_seconds"] = stage.elapsed().as_secs_f64().into();
        stage = Instant::now();
        let mut report = Value::Null;
        loop {
            let event = candidate.wait(shared)?;
            if event["kind"] == "ready" {
                break;
            }
            if event["kind"] == "restore_result" {
                report = event;
            }
        }
        ensure!(report.is_object(), "worker did not return restore report");
        timings["reconstruct_seconds"] = stage.elapsed().as_secs_f64().into();
        report.as_object_mut().unwrap().remove("schema");
        report.as_object_mut().unwrap().remove("kind");
        report["session_id"] = candidate.session.clone().into();
        report["platform_drift"] =
            (manifest["platform"] != candidate.environment["platform"]).into();
        ledger.call(Mutation::SessionEvent {
            session: candidate.session.clone(),
            kind: "restore_result".into(),
            payload: report.clone(),
        })?;
        if let Some(old) = worker.as_ref() {
            ledger.call(Mutation::SessionState {
                session: old.session.clone(),
                state: "closed".into(),
            })?;
        }
        Ok(report)
    })();
    match reconstructed {
        Ok(mut report) => {
            stage = Instant::now();
            worker.take();
            timings["retire_seconds"] = stage.elapsed().as_secs_f64().into();
            *worker = Some(candidate);
            shared.status.lock().unwrap()["worker_start_stage"] = "ready".into();
            timings["total_seconds"] = total_started.elapsed().as_secs_f64().into();
            if std::env::var("KERNELLEDGER_PROFILE").as_deref() == Ok("1") {
                report["timings"] = timings;
            }
            Ok(report)
        }
        Err(error) => {
            let _ = ledger.call(Mutation::SessionEvent {
                session: candidate.session.clone(),
                kind: "restore_failed".into(),
                payload: json!({"error":format!("{error:#}")}),
            });
            let _ = ledger.call(Mutation::SessionState {
                session: candidate.session.clone(),
                state: "closed".into(),
            });
            drop(candidate);
            *shared.status.lock().unwrap() = saved_status;
            *shared.socket.lock().unwrap() = saved_socket;
            Err(error)
        }
    }
}
impl Drop for Worker {
    fn drop(&mut self) {
        let _ = self.socket.shutdown(std::net::Shutdown::Both);
        let _ = self.process.stop();
    }
}

struct Execution<'a> {
    input: Option<&'a (Uuid, mpsc::SyncSender<Value>)>,
    request: &'a Value,
    frozen: Option<&'a (String, String)>,
    operation: Option<&'a str>,
    accepted: Option<&'a Value>,
}
fn execute(
    worker: &mut Worker,
    root: &Path,
    ledger: &Writer,
    shared: &Shared,
    execution: Execution<'_>,
) -> Result<Value> {
    let Execution {
        input,
        request,
        frozen,
        operation,
        accepted,
    } = execution;
    let (source, cell) = if let Some(accepted) = accepted {
        let hash = accepted["source_sha256"]
            .as_str()
            .context("admitted source digest")?;
        (
            String::from_utf8(BlobStore::new(&root.join(".kl")).get(hash)?)?,
            accepted["cell_id"].as_str().map(String::from),
        )
    } else if let Some((source, id)) = frozen {
        (source.clone(), Some(id.clone()))
    } else if let Some(text) = request.get("source").and_then(Value::as_str) {
        (notebook::canonical(text), None)
    } else {
        let id = request["cell_id"].as_str().context("cell_id is required")?;
        let snapshot = SourceService::new(root).read()?;
        ensure!(snapshot["state"] == "ok", "source is not valid");
        let cell = snapshot["cells"]
            .as_array()
            .context("cells")?
            .iter()
            .find(|cell| cell["id"] == id)
            .context("cell not found")?;
        ensure!(cell["type"] == "code", "cell is not executable");
        (
            cell["source"].as_str().context("source")?.to_owned(),
            Some(id.to_owned()),
        )
    };
    let store = BlobStore::new(&root.join(".kl"));
    let digest = store.publish(source.as_bytes(), None)?;
    let eid = if let Some(accepted) = accepted {
        accepted["exec_id"]
            .as_str()
            .context("admitted execution id")?
            .to_owned()
    } else {
        ledger.call(Mutation::Execution {
            session: worker.session.clone(),
            cell: cell.clone(),
            source: digest.clone(),
        })?["exec_id"]
            .as_str()
            .context("execution id")?
            .to_owned()
    };
    if let Some(oid) = operation.filter(|_| accepted.is_none()) {
        ledger.call(Mutation::OperationProgress {
            operation: oid.into(),
            result: json!({"exec_ids":[eid],"session_id":worker.session}),
        })?;
    }
    let result = (|| -> Result<Value> {
        {
            let _send = shared.worker_send.lock().unwrap();
            if let Some(oid) = operation.filter(|_| accepted.is_some())
                && ledger.call(Mutation::Operation {
                    operation: oid.into(),
                })?["result"]["cancel_requested"]
                    == true
            {
                ledger.call(Mutation::ExecutionState {
                    execution: eid.clone(),
                    state: "canceled".into(),
                })?;
                return Ok(
                    json!({"exec_id":eid,"status":"canceled","session_id":worker.session,"new_empty_session":false}),
                );
            }
            worker.process.pipes.begin(&eid)?;
            *shared.cancel_delivery.lock().unwrap() = (None, false);
            shared.status.lock().unwrap()["in_flight"] = eid.clone().into();
            send_command(
                &mut worker.socket,
                &store,
                &json!({"schema":1,"kind":"execute","exec_id":eid,"cell_id":cell,"source":source,"source_sha256":digest,"correlated_inputs":true}),
            )?;
        }
        loop {
            let mut event = worker.wait(shared)?;
            let kind = event["kind"]
                .as_str()
                .context("missing event kind")?
                .to_owned();
            if event["exec_id"].as_str() != Some(&eid) {
                if kind == "stdout" || kind == "stderr" {
                    worker.process.pipes.unattributed(&kind, &event)?;
                }
                continue;
            }
            event.as_object_mut().unwrap().remove("schema");
            event.as_object_mut().unwrap().remove("kind");
            event.as_object_mut().unwrap().remove("exec_id");
            if kind == "status" && event["state"] == "running" && accepted.is_none() {
                ledger.call(Mutation::ExecutionState {
                    execution: eid.clone(),
                    state: "running".into(),
                })?;
            }
            if kind == "status"
                && event["state"] == "running"
                && let Some(oid) = operation.filter(|_| accepted.is_some())
            {
                // An interrupt received before the worker's execution thread
                // starts may be ignored by worker protocol 1. Reconcile the
                // durable cancellation when its running acknowledgment arrives.
                let _send = shared.worker_send.lock().unwrap();
                let mut delivery = shared.cancel_delivery.lock().unwrap();
                delivery.0 = Some(oid.into());
                if !delivery.1
                    && ledger.call(Mutation::Operation {
                        operation: oid.into(),
                    })?["result"]["cancel_requested"]
                        == true
                {
                    write_worker(&mut worker.socket, &json!({"schema":1,"kind":"interrupt"}))?;
                    delivery.1 = true;
                }
            }
            if kind == "stdin_request" {
                inputs::requested(
                    &eid,
                    event,
                    operation.filter(|_| accepted.is_some()),
                    shared,
                    ledger,
                    &mut worker.socket,
                    input,
                )?;
                continue;
            }
            if kind == "stdout" || kind == "stderr" {
                worker.process.pipes.protocol(&kind, &event)?;
                continue;
            }
            if kind == "status" && event["state"] == "idle" {
                // Socket and pipe completion have no cross-channel total order.
                // Give already-readable pipe bytes a bounded drain window, then
                // retain later bytes as explicitly unattributed session output.
                thread::sleep(Duration::from_millis(20));
                worker.process.pipes.finish()?;
                ledger.call(Mutation::Event {
                    execution: eid.clone(),
                    kind: kind.clone(),
                    payload: event.clone(),
                })?;
                let status = event["outcome"]
                    .as_str()
                    .context("missing execution outcome")?;
                ledger.call(Mutation::ExecutionState {
                    execution: eid.clone(),
                    state: status.into(),
                })?;
                return Ok(
                    json!({"exec_id":eid,"status":status,"session_id":worker.session,"new_empty_session":false}),
                );
            }
            ledger.call(Mutation::Event {
                execution: eid.clone(),
                kind,
                payload: event,
            })?;
        }
    })();
    let cleanup = (|| -> Result<()> {
        let _send = shared.worker_send.lock().unwrap();
        inputs::cancel(&eid, shared, ledger, None)?;
        Ok(())
    })();
    if cleanup.is_err() {
        shared.stopping.store(true, Ordering::Release);
    }
    let result = result.and_then(|value| cleanup.map(|_| value));
    shared.status.lock().unwrap()["in_flight"] = Value::Null;
    if result.is_err() {
        let _ = worker.process.stop();
        let _ = worker.process.pipes.finish();
        ledger.call(Mutation::ExecutionState {
            execution: eid,
            state: "aborted".into(),
        })?;
    }
    result
}

fn coordinator(
    root: PathBuf,
    python: PathBuf,
    resource: PathBuf,
    ledger: Writer,
    shared: Arc<Shared>,
    jobs: mpsc::Receiver<Job>,
) {
    let mut worker: Option<Worker> = None;
    while !shared.stopping.load(Ordering::Acquire) {
        let job = match jobs.recv_timeout(Duration::from_millis(20)) {
            Ok(job) => job,
            Err(mpsc::RecvTimeoutError::Timeout) => {
                if let Some(current) = worker.as_mut()
                    && (shared.killed.swap(false, Ordering::AcqRel)
                        || current
                            .process
                            .tree
                            .child
                            .try_wait()
                            .ok()
                            .flatten()
                            .is_some()
                        || current.process.pipes.check().is_err()
                        || current.drain_background(&ledger, &shared).is_err())
                {
                    let _gate = shared.admission.lock().unwrap();
                    let sid = current.session.clone();
                    worker.take();
                    let _ = ledger.call(Mutation::SessionState {
                        session: sid,
                        state: "crashed".into(),
                    });
                    *shared.socket.lock().unwrap() = None;
                    shared.status.lock().unwrap()["worker_start_stage"] = "absent".into();
                }
                continue;
            }
            Err(_) => break,
        };
        if job.admitted.is_some() {
            desktop::perform(
                &job,
                &mut worker,
                &root,
                &python,
                &resource,
                &ledger,
                &shared,
            );
            continue;
        }
        let mut operation = None;
        let result = (|| -> Result<Value> {
            let command = job.request["command"]
                .as_str()
                .context("command is required")?;
            ensure!(
                [
                    "run",
                    "restart",
                    "checkpoint",
                    "restore",
                    "continue",
                    "_ensure_worker"
                ]
                .contains(&command),
                "unsupported native command: {command}"
            );
            let mut lifecycle = if command != "run" || worker.is_none() {
                Some(shared.admission.lock().unwrap())
            } else {
                None
            };
            if let Some(key) = job.request.get("client_request_id").and_then(Value::as_str) {
                let client = job.request["client_id"]
                    .as_str()
                    .context("client_id is required with client_request_id")?;
                let accepted = ledger.call(Mutation::Accept {
                    client: client.into(),
                    key: key.into(),
                    kind: command.into(),
                    request: job.request.clone(),
                })?;
                let row = &accepted["operation"];
                if accepted["fresh"] != true {
                    ensure!(
                        row["state"] == "completed",
                        "operation already exists with state {}; inspect its original operation_id",
                        row["state"]
                    );
                    return Ok(row["result"].clone());
                }
                let oid = row["operation_id"]
                    .as_str()
                    .context("operation id")?
                    .to_owned();
                ledger.call(Mutation::OperationState {
                    operation: oid.clone(),
                    state: "running".into(),
                    result: None,
                })?;
                operation = Some(oid);
                *shared.active_operation.lock().unwrap() = operation.clone();
            }
            if command == "restore" || command == "continue" {
                return restore(
                    &mut worker,
                    &root,
                    &python,
                    &resource,
                    &ledger,
                    &shared,
                    &job.request,
                );
            }
            if command == "checkpoint" {
                return checkpoint(
                    worker
                        .as_mut()
                        .context("no live worker; run or restore first")?,
                    &root,
                    &python,
                    &resource,
                    &ledger,
                    &shared,
                );
            }
            if command == "restart"
                && let Some(old) = worker.take()
            {
                ledger.call(Mutation::SessionState {
                    session: old.session.clone(),
                    state: "closed".into(),
                })?;
                drop(old);
            }
            let fresh = worker.is_none();
            if fresh {
                worker = Some(Worker::start(
                    &root, &python, &resource, &ledger, &shared, None, None,
                )?);
            }
            let current = worker.as_mut().unwrap();
            if command == "restart" || command == "_ensure_worker" {
                return Ok(json!({"session_id":current.session}));
            }
            // Worker startup is a lifecycle transition; execution is not. New
            // desktop work must be admissible while this cell is running.
            drop(lifecycle.take());
            let mut result = execute(
                current,
                &root,
                &ledger,
                &shared,
                Execution {
                    input: job.input.as_ref(),
                    request: &job.request,
                    frozen: job.frozen.as_ref(),
                    operation: operation.as_deref(),
                    accepted: None,
                },
            )?;
            result["new_empty_session"] = fresh.into();
            Ok(result)
        })();
        *shared.active_operation.lock().unwrap() = None;
        if let Some(oid) = operation {
            let (state, saved) = match &result {
                Ok(value) => ("completed", value.clone()),
                Err(error) => {
                    let previous = ledger
                        .call(Mutation::Operation {
                            operation: oid.clone(),
                        })
                        .ok();
                    let mut known = previous
                        .and_then(|row| row.get("result").cloned())
                        .filter(Value::is_object)
                        .unwrap_or_else(|| json!({}));
                    known["error"] = format!("{error:#}").into();
                    known["reason"]="native operation did not complete; reconcile recorded executions before retrying".into();
                    ("unknown", known)
                }
            };
            if let Err(error) = ledger.call(Mutation::OperationState {
                operation: oid,
                state: state.into(),
                result: Some(saved),
            }) {
                shared.stopping.store(true, Ordering::Release);
                let _ = job.reply.send(Err(error));
                continue;
            }
        }
        if result.is_err()
            && worker.as_mut().is_some_and(|w| {
                w.process.tree.child.try_wait().ok().flatten().is_some()
                    || w.process.pipes.check().is_err()
            })
            && let Some(old) = worker.take()
        {
            let _gate = shared.admission.lock().unwrap();
            let _ = ledger.call(Mutation::SessionState {
                session: old.session.clone(),
                state: "crashed".into(),
            });
            drop(old);
            *shared.socket.lock().unwrap() = None;
            *shared.status.lock().unwrap() = json!({"pid":std::process::id(),"session_id":null,"in_flight":null,"worker_start_stage":"absent","implementation":"rust","experimental":true});
        }
        let _ = job.reply.send(result);
    }
    let _gate = shared.admission.lock().unwrap();
    let stop = shared.stop_operation.lock().unwrap().clone();
    if let Err(error) = ledger.call(Mutation::CancelPending {
        except: stop.clone(),
    }) {
        *shared.stop_error.lock().unwrap() = Some(format!("{error:#}"));
    }
    if let Some(old) = worker.take() {
        let _ = ledger.call(Mutation::SessionState {
            session: old.session.clone(),
            state: "closed".into(),
        });
        drop(old);
    }
    if let Some(oid) = stop {
        let finished = (|| -> Result<()> {
            ledger.call(Mutation::OperationState {
                operation: oid.clone(),
                state: "running".into(),
                result: None,
            })?;
            ledger.call(Mutation::OperationState {
                operation: oid,
                state: "completed".into(),
                result: Some(json!({"stopped":true})),
            })?;
            Ok(())
        })();
        if let Err(error) = finished {
            *shared.stop_error.lock().unwrap() = Some(format!("{error:#}"));
        }
    }
    *shared.socket.lock().unwrap() = None;
    // Release the coordinator handle after worker/capture shutdown. The main
    // thread retains ownership until runtime and registry hints are removed.
    drop(ledger);
    shared.finished.store(true, Ordering::Release);
}

fn send_command(socket: &mut TcpStream, store: &BlobStore, command: &Value) -> Result<()> {
    let data = serde_json::to_vec(command)?;
    ensure!(
        data.len() as u64 <= ENVELOPE_LIMIT,
        "command exceeds native structured-message limit"
    );
    if data.len() <= kernelledger_protocol::WORKER_INLINE {
        return write_worker(socket, command);
    }
    let digest = store.publish(&data, None)?;
    send_stored(socket, store, &digest)?;
    write_worker(socket, &json!({"schema":1,"kind":"envelope","blob":digest}))
}

// CLI replies may contain secrets. Decode bounded binary envelopes in memory,
// independently of the durable worker/source blob receiver.
fn receive_input(socket: &mut TcpStream) -> Result<Value> {
    let header = read_worker(&mut *socket)?;
    if header["kind"] != "blob" {
        return Ok(header);
    }
    let size = header["byte_size"]
        .as_u64()
        .context("invalid input envelope size")?;
    ensure!(
        size <= kernelledger_protocol::FRAME_LIMIT as u64,
        "input envelope exceeds 1 MiB"
    );
    let mut bytes = Vec::with_capacity(size as usize);
    kernelledger_protocol::copy_blob(&mut *socket, &mut bytes, &header)?;
    let envelope = read_worker(socket)?;
    ensure!(
        envelope["kind"] == "envelope" && envelope["blob"] == header["sha256"],
        "invalid input envelope"
    );
    let message: Value = serde_json::from_slice(&bytes)?;
    ensure!(
        message.is_object() && message["schema"] == 1,
        "invalid input message"
    );
    Ok(message)
}

fn relay(
    socket: &mut TcpStream,
    result: mpsc::Receiver<Result<Value>>,
    prompts: mpsc::Receiver<Value>,
    connection: Uuid,
    shared: &Shared,
    ledger: &Writer,
) -> Result<Value> {
    socket.set_read_timeout(Some(Duration::from_millis(20)))?;
    let mut pending: Option<String> = None;
    loop {
        match result.try_recv() {
            Ok(value) => return value,
            Err(mpsc::TryRecvError::Disconnected) => bail!("runtime stopped before reply"),
            Err(mpsc::TryRecvError::Empty) => {}
        }
        if let Ok(prompt) = prompts.try_recv() {
            let id = prompt["input_id"].as_str().context("input id")?.to_owned();
            if inputs::owned(&id, connection, shared) {
                inputs::send(socket, &prompt)?;
                pending = Some(id);
            }
        }
        match socket.peek(&mut [0; 1]) {
            Ok(0) => bail!("interactive client disconnected"),
            Ok(_) => {
                let id = pending
                    .take()
                    .context("unexpected interactive client message")?;
                socket.set_read_timeout(Some(Duration::from_secs(10)))?;
                let reply = receive_input(socket)?;
                socket.set_read_timeout(Some(Duration::from_millis(20)))?;
                inputs::legacy_reply(&id, &reply, connection, shared, ledger)?;
            }
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock
                ) => {}
            Err(error) => return Err(error.into()),
        }
    }
}

fn client(
    mut socket: TcpStream,
    root: &Path,
    secret: &str,
    shared: &Shared,
    jobs: &mpsc::SyncSender<Job>,
    ledger: &Writer,
    connection: Uuid,
) -> Result<()> {
    socket.set_read_timeout(Some(Duration::from_secs(10)))?;
    socket.set_write_timeout(Some(Duration::from_secs(10)))?;
    let result = (|| -> Result<Value> {
        let auth = read_worker(&mut socket)?;
        ensure!(
            auth["kind"] == "auth" && auth["token"].as_str() == Some(secret),
            "authentication failed"
        );
        let request = receive(&mut socket, &BlobStore::new(&root.join(".kl")))?;
        ensure!(request["kind"] == "request", "expected request");
        match request["command"].as_str() {
            Some("status") => {
                let mut status = shared.identity.clone();
                status
                    .as_object_mut()
                    .unwrap()
                    .extend(shared.status.lock().unwrap().as_object().unwrap().clone());
                Ok(status)
            }
            Some("operation") => {
                let reader = crate::ledger::Ledger::open(&root.join(".kl"))?
                    .context("history is empty")?
                    .0;
                let mut row = if let Some(oid) = request.get("operation_id").and_then(Value::as_str)
                {
                    reader.one(
                        "SELECT * FROM operations WHERE operation_id=?",
                        &[oid.to_owned().into()],
                    )?
                } else {
                    reader.one(
                        "SELECT * FROM operations WHERE client_id=? AND client_request_id=?",
                        &[
                            request["client_id"]
                                .as_str()
                                .context("client_id")?
                                .to_owned()
                                .into(),
                            request["client_request_id"]
                                .as_str()
                                .context("client_request_id")?
                                .to_owned()
                                .into(),
                        ],
                    )?
                };
                let result = row.remove("result_json").unwrap_or(Value::Null);
                row.insert(
                    "result".into(),
                    result
                        .as_str()
                        .map(serde_json::from_str)
                        .transpose()?
                        .unwrap_or(Value::Null),
                );
                Ok(Value::Object(row))
            }
            Some("interrupt") => {
                if request["kill"] == true {
                    shared.killed.store(true, Ordering::Release);
                } else {
                    let _send = shared.worker_send.lock().unwrap();
                    let mut guard = shared.socket.lock().unwrap();
                    let worker = guard.as_mut().context("no live worker")?;
                    ensure!(
                        !shared.status.lock().unwrap()["in_flight"].is_null(),
                        "no in-flight execution"
                    );
                    write_worker(&mut *worker, &json!({"schema":1,"kind":"interrupt"}))?;
                    if let Some(eid) = shared.status.lock().unwrap()["in_flight"].as_str() {
                        inputs::cancel(eid, shared, ledger, Some(worker))?;
                    }
                }
                Ok(json!({"interrupted":true}))
            }
            Some("stop") => {
                shared.stopping.store(true, Ordering::Release);
                let deadline = Instant::now() + Duration::from_secs(30);
                while !shared.finished.load(Ordering::Acquire) {
                    ensure!(Instant::now() < deadline, "stop timed out");
                    thread::sleep(Duration::from_millis(10));
                }
                ensure!(
                    shared.stop_error.lock().unwrap().is_none(),
                    "runtime stop could not commit its result"
                );
                Ok(json!({"stopped":true}))
            }
            Some("run" | "restart" | "checkpoint" | "restore" | "continue") => {
                ensure!(!shared.stopping.load(Ordering::Acquire), "runtime stopping");
                // A retry belongs to its original admitted request even after
                // tracked source is edited, removed, or contains conflict markers.
                // The coordinator still validates the request hash before reuse.
                let known = if let Some(key) = request.get("client_request_id") {
                    let key = key.as_str().context("client_request_id must be a string")?;
                    let client = request["client_id"]
                        .as_str()
                        .context("client_id is required with client_request_id")?;
                    let reader = crate::ledger::Ledger::open(&root.join(".kl"))?
                        .context("history is empty")?
                        .0;
                    !reader.rows("SELECT operation_id FROM operations WHERE client_id=? AND client_request_id=?",&[client.to_owned().into(),key.to_owned().into()])?.is_empty()
                } else {
                    false
                };
                let frozen = if request["command"] == "run"
                    && !request.get("source").is_some_and(Value::is_string)
                    && !known
                {
                    let id = request["cell_id"].as_str().context("cell_id is required")?;
                    let snapshot = SourceService::new(root).read()?;
                    ensure!(snapshot["state"] == "ok", "source is not valid");
                    let cell = snapshot["cells"]
                        .as_array()
                        .context("cells")?
                        .iter()
                        .find(|cell| cell["id"] == id)
                        .context("cell not found")?;
                    ensure!(cell["type"] == "code", "cell is not executable");
                    Some((
                        cell["source"].as_str().context("source")?.to_owned(),
                        id.to_owned(),
                    ))
                } else {
                    None
                };
                let (reply, receive) = mpsc::sync_channel(1);
                let (input_send, input_receive) = mpsc::sync_channel(1);
                let interactive = request["command"] == "run" && request["interactive"] == true;
                jobs.try_send(Job {
                    input: interactive.then_some((connection, input_send)),
                    request,
                    frozen,
                    reply,
                    admitted: None,
                    _slot: reserve(shared)?,
                })
                .map_err(|_| anyhow::anyhow!("runtime queue is full or stopped"))?;
                if interactive {
                    relay(
                        &mut socket,
                        receive,
                        input_receive,
                        connection,
                        shared,
                        ledger,
                    )
                } else {
                    receive.recv().context("runtime stopped before reply")?
                }
            }
            _ => bail!("unsupported native command"),
        }
    })();
    let response = match result {
        Ok(result) => json!({"schema":1,"kind":"response","result":result}),
        Err(error) => json!({"schema":1,"kind":"response","error":format!("{error:#}")}),
    };
    write_worker(&mut socket, &response)
}

pub fn serve(args: &[String]) -> Result<Value> {
    let flag = |name: &str| -> Result<PathBuf> {
        let index = args
            .iter()
            .position(|a| a == name)
            .with_context(|| format!("missing {name}"))?;
        Ok(PathBuf::from(
            args.get(index + 1).context("missing flag value")?,
        ))
    };
    let root = flag("--project")?.canonicalize()?;
    // Preserve venv symlinks: resolving bin/python selects the base interpreter
    // and loses the environment's site-packages on Unix.
    let python = std::path::absolute(flag("--python")?)?;
    ensure!(python.is_file(), "worker interpreter is missing");
    let resources = crate::resources::extract()?;
    let resource = if args.iter().any(|a| a == "--worker") {
        flag("--worker")?.canonicalize()?
    } else {
        resources.path().join("worker.py")
    };
    notebook::read(&root)?;
    let ledger = Writer::start(&root.join(".kl"))?;
    SourceService::new(&root).recover()?;
    ledger.call(Mutation::Takeover)?;
    let secret = token();
    let token_path = root.join(".kl/token");
    // Retained Python platform utility applies/verifies the existing Windows ACL
    // contract. No worker or ledger connection is created by this utility.
    #[cfg(unix)]
    {
        use std::{io::Write, os::unix::fs::OpenOptionsExt};
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(&token_path)?;
        file.write_all(secret.as_bytes())?;
        use std::os::unix::fs::PermissionsExt;
        file.set_permissions(fs::Permissions::from_mode(0o600))?;
    }
    #[cfg(windows)]
    {
        fs::write(&token_path, [])?;
        let status=python_command(&python)?.args(["-I","-c","import sys; from kernelledger.supervisor.platform import secure_token; secure_token(sys.argv[1])"]).arg(&token_path).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).status()?;
        ensure!(
            status.success(),
            "cannot secure runtime token using installed platform utility"
        );
        fs::write(&token_path, &secret)?;
    }
    let listener = TcpListener::bind(("127.0.0.1", 0))?;
    listener.set_nonblocking(true)?;
    let desktop_listener = TcpListener::bind(("127.0.0.1", 0))?;
    desktop_listener.set_nonblocking(true)?;
    let started = crate::ledger::Ledger::open(&root.join(".kl"))?
        .context("history is empty")?
        .0
        .one(
            "SELECT strftime('%Y-%m-%dT%H:%M:%fZ','now') AS started",
            &[],
        )?["started"]
        .clone();
    let info = json!({"pid":std::process::id(),"port":listener.local_addr()?.port(),"desktop_port":desktop_listener.local_addr()?.port(),"instance_id":Uuid::new_v4().to_string(),"project_path":crate::display_path(&root),"notebook_id":notebook::read(&root)?["id"],"implementation":"rust","experimental":true,"kernelledger_version":env!("CARGO_PKG_VERSION"),"start_time":started,"ledger_schema":2,"migration_required":false,"distribution":"native-experimental","protocols":{"legacy":[1],"desktop":[1]}});
    let publication_owner = ledger.clone();
    let mut publication = crate::publication::Publication::publish(&root, &info)?;
    let shared = Arc::new(Shared {
        identity: info,
        status: Mutex::new(
            json!({"pid":std::process::id(),"session_id":null,"in_flight":null,"worker_start_stage":"absent","implementation":"rust","experimental":true}),
        ),
        socket: Mutex::new(None),
        worker_send: Mutex::new(()),
        validation: Mutex::new(crate::resources::ValidationCache::default()),
        cache_validation: !args.iter().any(|arg| arg == "--worker"),
        cancel_delivery: Mutex::new((None, false)),
        inputs: Mutex::new(inputs::Requests::new()),
        record_secret_input: fs::read(root.join(".kl/config.json"))
            .ok()
            .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
            .is_some_and(|config| config["record_secret_input"] == true),
        stopping: AtomicBool::new(false),
        killed: AtomicBool::new(false),
        finished: AtomicBool::new(false),
        clients: AtomicUsize::new(0),
        queued: Arc::new(AtomicUsize::new(0)),
        admission: Mutex::new(()),
        active_operation: Mutex::new(None),
        stop_operation: Mutex::new(None),
        stop_admission: Mutex::new(()),
        stop_error: Mutex::new(None),
        connections: Mutex::new(HashMap::new()),
        desktop_clients: Mutex::new(HashMap::new()),
    });
    let (send, jobs) = mpsc::sync_channel(COMMAND_LIMIT);
    let acceptors = accept::Acceptors::start(
        &[(&listener, false), (&desktop_listener, true)],
        shared.clone(),
    )?;
    let engine_shared = shared.clone();
    let engine_root = root.clone();
    let engine = thread::Builder::new()
        .name("kl-coordinator".into())
        .spawn(move || coordinator(engine_root, python, resource, ledger, engine_shared, jobs))?;
    let mut clients = Vec::new();
    let outcome = (|| -> Result<()> {
        while !shared.stopping.load(Ordering::Acquire) {
            if engine.is_finished() {
                shared.finished.store(true, Ordering::Release);
                break;
            }
            clients.retain(|handle: &thread::JoinHandle<()>| !handle.is_finished());
            let (socket, is_desktop) =
                match acceptors.receive.recv_timeout(Duration::from_millis(20)) {
                    Ok(connection) => connection,
                    Err(mpsc::RecvTimeoutError::Timeout) => continue,
                    Err(mpsc::RecvTimeoutError::Disconnected) => bail!("native listeners stopped"),
                };
            socket.set_nonblocking(false)?;
            if shared.clients.load(Ordering::Acquire) >= CLIENT_LIMIT {
                drop(socket);
                continue;
            }
            shared.clients.fetch_add(1, Ordering::AcqRel);
            let local = shared.clone();
            let queue = send.clone();
            let auth = secret.clone();
            let project = root.clone();
            let connection = Uuid::new_v4();
            shared
                .connections
                .lock()
                .unwrap()
                .insert(connection, socket.try_clone()?);
            let writer = publication_owner.clone();
            clients.push(thread::spawn(move || {
                if is_desktop {
                    let _ = desktop::serve(
                        socket, &project, &auth, &local, &queue, &writer, connection,
                    );
                } else {
                    let _ = client(socket, &project, &auth, &local, &queue, &writer, connection);
                }
                if inputs::disconnected(connection, &local, &writer).is_err() {
                    local.stopping.store(true, Ordering::Release);
                }
                local.desktop_clients.lock().unwrap().remove(&connection);
                local.connections.lock().unwrap().remove(&connection);
                local.clients.fetch_sub(1, Ordering::AcqRel);
            }));
        }
        Ok(())
    })();
    shared.stopping.store(true, Ordering::Release);
    drop(acceptors);
    drop(send);
    let joined = engine.join();
    for socket in shared.connections.lock().unwrap().values() {
        let _ = socket.shutdown(std::net::Shutdown::Read);
    }
    for client in clients {
        let _ = client.join();
    }
    publication.cleanup()?;
    drop(publication);
    drop(publication_owner);
    ensure!(joined.is_ok(), "native coordinator panicked");
    ensure!(
        shared.stop_error.lock().unwrap().is_none(),
        "native shutdown did not commit all results"
    );
    outcome?;
    Ok(json!({"stopped":true}))
}
