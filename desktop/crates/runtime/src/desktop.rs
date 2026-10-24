//! Native desktop protocol 1 admission and committed operation queries.
//! Input claims are connection-owned; feeds read bounded committed snapshots.
#[path = "feeds.rs"]
mod feeds;
use super::{Execution, Job, Shared, Worker, checkpoint, execute, inputs, reserve, restore};
use crate::{
    helper,
    ledger::Ledger,
    notebook,
    source::SourceService,
    store::{BlobStore, coded, invalid},
    writer::{FrozenSource, Mutation, Writer},
};
use anyhow::{Context, Result};
use base64::{
    Engine as _, alphabet,
    engine::{GeneralPurpose, GeneralPurposeConfig},
};
use kernelledger_protocol::{read_frame as protocol_read, write_frame as protocol_write};
use serde_json::{Value, json};
use std::{
    collections::HashSet,
    io::{Read, Seek, SeekFrom, Write},
    net::TcpStream,
    path::Path,
    sync::{atomic::Ordering, mpsc},
    time::{Duration, Instant},
};
use uuid::Uuid;

pub(super) const CAPABILITIES: &[&str] = &[
    "operations",
    "batches",
    "blob_ranges",
    "input_claims",
    "change_feed",
];
const EFFECTS: &[&str] = &[
    "run",
    "batch",
    "restart",
    "stop",
    "checkpoint",
    "restore",
    "continue",
];
const METHODS: &[&str] = &[
    "runtime.status",
    "run",
    "batch",
    "interrupt",
    "interrupt_kill",
    "restart",
    "stop",
    "checkpoint",
    "restore",
    "continue",
    "operation.get",
    "operation.list",
    "operation.cancel",
    "changes.list",
    "blob.read",
    "input.claim",
    "input.release",
    "input.reply",
    "input.cancel",
    "subscribe",
    "execution.follow",
];
fn transport(error: anyhow::Error) -> anyhow::Error {
    let text = error.to_string();
    if text == "JSON frame exceeds 1 MiB" {
        coded("too_large", text, json!({}))
    } else if error.downcast_ref::<serde_json::Error>().is_some()
        || text == "frame must be an object"
    {
        invalid(text)
    } else {
        error
    }
}
fn read_frame(socket: &mut TcpStream) -> Result<Value> {
    protocol_read(socket).map_err(transport)
}
fn write_frame(socket: impl Write, value: &Value) -> Result<()> {
    protocol_write(socket, value).map_err(transport)
}

fn reader(root: &Path) -> Result<Ledger> {
    Ok(Ledger::open(&root.join(".kl"))?
        .context("history is empty")?
        .0)
}
fn op(root: &Path, id: &str) -> Result<Value> {
    let rows = reader(root)?.rows(
        "SELECT * FROM operations WHERE operation_id=?",
        &[id.to_owned().into()],
    )?;
    operation_view(
        rows.into_iter()
            .next()
            .ok_or_else(|| coded("not_found", "operation not found", json!({})))?
            .into(),
    )
}
fn operation_view(mut row: Value) -> Result<Value> {
    let result = row
        .as_object_mut()
        .context("operation row")?
        .remove("result_json")
        .unwrap_or(Value::Null);
    row["result"] = result
        .as_str()
        .map(serde_json::from_str)
        .transpose()?
        .unwrap_or(Value::Null);
    Ok(row)
}
fn existing(root: &Path, client: &str, key: &str, request: &Value) -> Result<Option<Value>> {
    let rows = reader(root)?.rows(
        "SELECT * FROM operations WHERE client_id=? AND client_request_id=?",
        &[client.to_owned().into(), key.to_owned().into()],
    )?;
    if let Some(row) = rows.into_iter().next() {
        if row["request_sha256"] != kernelledger_protocol::request_hash(request)? {
            return Err(coded(
                "key_reused",
                "request key already used for different content",
                json!({}),
            ));
        }
        return Ok(Some(Value::Object(row)));
    }
    Ok(None)
}
fn validate(request: &Value) -> Result<&str> {
    let method = request
        .get("method")
        .and_then(Value::as_str)
        .ok_or_else(|| invalid("unknown request method"))?;
    if !METHODS.contains(&method) {
        return Err(coded(
            "unsupported_capability",
            "method is not implemented by the native desktop supervisor",
            json!({"method":method}),
        ));
    }
    let required: &[&str] = match method {
        "run" => &["expected_sha256"],
        "restore" | "continue" => &["checkpoint_id"],
        "operation.cancel" => &["operation_id"],
        "blob.read" => &["sha256"],
        "input.reply" => &["input_id", "text"],
        "input.claim" | "input.release" | "input.cancel" => &["input_id"],
        "execution.follow" => &["exec_id"],
        _ => &[],
    };
    for key in required {
        if request.get(*key).is_none() {
            return Err(invalid(format!("{key} is required")));
        }
    }
    // Reuse the common field validator, with a helper method requiring no fields.
    let mut fields = request.clone();
    fields["method"] = "ping".into();
    helper::validate(&fields)?;
    if EFFECTS.contains(&method)
        && !request["client_request_id"]
            .as_str()
            .is_some_and(|key| Uuid::parse_str(key).is_ok())
    {
        return Err(invalid("client_request_id must be a UUID"));
    }
    if method == "run" && request.get("source").is_some() == request.get("cell_id").is_some() {
        return Err(invalid("run requires one cell_id or source"));
    }
    if EFFECTS.contains(&method) {
        kernelledger_protocol::request_hash(request)
            .map_err(|error| invalid(format!("native request hashing: {error}")))?;
    }
    Ok(method)
}
fn freeze(root: &Path, request: &Value) -> Result<Vec<FrozenSource>> {
    let cells = if request["method"] == "run" {
        if let Some(source) = request.get("source").and_then(Value::as_str) {
            let source = notebook::canonical(source);
            let digest = notebook::source_hash(&source);
            if request["expected_sha256"] != digest {
                return Err(coded(
                    "source_conflict",
                    "unattributed source hash differs",
                    json!({"revision":digest}),
                ));
            }
            vec![json!({"id":null,"source":source,"revision":digest})]
        } else {
            let id = request["cell_id"].as_str().context("cell id")?;
            let expected = request["expected_sha256"]
                .as_str()
                .filter(|s| !s.is_empty())
                .ok_or_else(|| invalid("expected_sha256 is required"))?;
            SourceService::new(root).execution_sources(&json!([id]), &json!({(id):expected}))?
        }
    } else {
        let policy = request
            .get("on_error")
            .and_then(Value::as_str)
            .unwrap_or("stop");
        if !["stop", "continue"].contains(&policy) {
            return Err(invalid("invalid on_error policy"));
        }
        let ids = request
            .get("cell_ids")
            .cloned()
            .unwrap_or_else(|| json!("all"));
        if ids != "all" {
            let ids = ids
                .as_array()
                .ok_or_else(|| invalid("batch requires unique ordered cell ids"))?;
            let mut seen = HashSet::new();
            if ids
                .iter()
                .any(|id| id.as_str().is_none_or(|id| !seen.insert(id)))
            {
                return Err(invalid("batch requires unique ordered cell ids"));
            }
        }
        let expected = request
            .get("expected_sha256")
            .cloned()
            .unwrap_or_else(|| json!({}));
        if !expected.is_object() {
            return Err(invalid("batch expected_sha256 must be an object"));
        }
        SourceService::new(root).execution_sources(&ids, &expected)?
    };
    let store = BlobStore::new(&root.join(".kl"));
    cells
        .into_iter()
        .map(|cell| {
            let source = cell["source"].as_str().context("frozen source")?;
            // Leave room for the worker execute envelope around the source text.
            if source.len() > super::ENVELOPE_LIMIT as usize / 2 {
                return Err(coded(
                    "too_large",
                    "native source exceeds the structured-message budget",
                    json!({}),
                ));
            }
            Ok(FrozenSource {
                cell: cell["id"].as_str().map(String::from),
                source: store.publish(source.as_bytes(), None)?,
            })
        })
        .collect()
}
fn accept(
    root: &Path,
    request: &Value,
    greeting: &Value,
    shared: &Shared,
    jobs: &mpsc::SyncSender<Job>,
    ledger: &Writer,
) -> Result<Value> {
    let method = request["method"].as_str().unwrap();
    let key = request["client_request_id"].as_str().unwrap();
    let client = greeting["client_id"].as_str().unwrap();
    // Stop must bypass both the work queue and a lifecycle/source admission
    // gate held by a stalled restore or worker startup.
    if method == "stop" {
        let _stop = shared.stop_admission.lock().unwrap();
        if let Some(row) = existing(root, client, key, request)? {
            return Ok(json!({"kind":"accepted","operation_id":row["operation_id"]}));
        }
        if shared.stopping.load(Ordering::Acquire) {
            return Err(coded("busy", "runtime is stopping", json!({})));
        }
        let admitted = ledger.call(Mutation::Admit {
            client: client.into(),
            key: key.into(),
            kind: "stop".into(),
            request: request.clone(),
            session: None,
            cells: Vec::new(),
            on_error: "stop".into(),
            refuse_busy: false,
        })?;
        let oid = admitted["operation"]["operation_id"]
            .as_str()
            .context("stop operation id")?
            .to_owned();
        *shared.stop_operation.lock().unwrap() = Some(oid.clone());
        shared.stopping.store(true, Ordering::Release);
        return Ok(json!({"kind":"accepted","operation_id":oid,"executions":[]}));
    }
    let gate = shared.admission.lock().unwrap();
    if let Some(row) = existing(root, client, key, request)? {
        return Ok(json!({"kind":"accepted","operation_id":row["operation_id"]}));
    }
    if shared.stopping.load(Ordering::Acquire) {
        return Err(coded("busy", "runtime is stopping", json!({})));
    }
    let slot = if method != "stop" {
        Some(reserve(shared)?)
    } else {
        None
    };
    let cells = if method == "run" || method == "batch" {
        freeze(root, request).map_err(crate::source::python_error)?
    } else {
        Vec::new()
    };
    let needs_worker = method == "run" || method == "batch";
    let absent = shared.socket.lock().unwrap().is_none();
    drop(gate);
    if needs_worker && absent {
        let (reply, receive) = mpsc::sync_channel(1);
        jobs.try_send(Job {
            input: None,
            request: json!({"command":"_ensure_worker"}),
            frozen: None,
            reply,
            admitted: None,
            _slot: reserve(shared)?,
        })
        .map_err(|_| coded("busy", "runtime queue is full or stopped", json!({})))?;
        receive
            .recv_timeout(Duration::from_secs(30))
            .context("worker startup did not complete")??;
    }
    let _gate = shared.admission.lock().unwrap();
    if let Some(row) = existing(root, client, key, request)? {
        return Ok(json!({"kind":"accepted","operation_id":row["operation_id"]}));
    }
    if shared.stopping.load(Ordering::Acquire) {
        return Err(coded("busy", "runtime is stopping", json!({})));
    }
    let status = shared.status.lock().unwrap().clone();
    let session = status["session_id"].as_str().map(String::from);
    if needs_worker && shared.socket.lock().unwrap().is_none() {
        return Err(coded("busy", "worker session is unavailable", json!({})));
    }
    let refuse_busy = method == "checkpoint"
        && (!status["in_flight"].is_null() || shared.active_operation.lock().unwrap().is_some());
    let admitted = ledger.call(Mutation::Admit {
        client: client.into(),
        key: key.into(),
        kind: method.into(),
        request: request.clone(),
        session,
        cells,
        on_error: request
            .get("on_error")
            .and_then(Value::as_str)
            .unwrap_or("stop")
            .into(),
        refuse_busy,
    })?;
    let oid = admitted["operation"]["operation_id"]
        .as_str()
        .context("operation id")?
        .to_owned();
    let ids = admitted["executions"]
        .as_array()
        .context("admitted executions")?
        .iter()
        .map(|cell| cell["exec_id"].clone())
        .collect::<Vec<_>>();
    if admitted["fresh"] == true && admitted["operation"]["state"] == "accepted" {
        if method == "stop" {
            *shared.stop_operation.lock().unwrap() = Some(oid.clone());
            shared.stopping.store(true, Ordering::Release);
        } else {
            let (reply, receive) = mpsc::sync_channel(1);
            drop(receive);
            let job = Job {
                input: None,
                request: request.clone(),
                frozen: None,
                reply,
                admitted: Some(admitted),
                _slot: slot.context("missing queue reservation")?,
            };
            if jobs.try_send(job).is_err() {
                let _ = ledger.call(Mutation::Cancel {
                    operation: oid.clone(),
                });
                return Err(coded(
                    "busy",
                    "runtime stopped after admission; reconcile the original request key",
                    json!({"operation_id":oid}),
                ));
            }
        }
    }
    Ok(json!({"kind":"accepted","operation_id":oid,"executions":ids}))
}

pub(super) fn perform(
    job: &Job,
    worker: &mut Option<Worker>,
    root: &Path,
    python: &Path,
    resource: &Path,
    ledger: &Writer,
    shared: &Shared,
) {
    let admitted = job.admitted.as_ref().unwrap();
    let oid = admitted["operation"]["operation_id"].as_str().unwrap();
    let method = job.request["method"].as_str().unwrap();
    let started = (|| -> Result<bool> {
        let _gate = shared.admission.lock().unwrap();
        if op(root, oid)?["state"] != "accepted" {
            return Ok(false);
        }
        ledger.call(Mutation::OperationState {
            operation: oid.into(),
            state: "running".into(),
            result: None,
        })?;
        *shared.active_operation.lock().unwrap() = Some(oid.into());
        Ok(true)
    })();
    if matches!(started, Ok(false)) {
        return;
    }
    let result=started.and_then(|_|->Result<Value>{
        if method=="run" || method=="batch" {
            let mut results=Vec::new();
            for cell in admitted["executions"].as_array().context("admitted execution list")? {
                if worker.as_ref().is_none_or(|worker|cell["session_id"]!=worker.session) {return Err(coded("busy","accepted worker session is no longer available",json!({})));}
                let begin=ledger.call(Mutation::BeginExecution {operation:oid.into(),execution:cell["exec_id"].as_str().context("execution id")?.into()})?;
                if begin["started"]!=true {break;}
                let current=worker.as_mut().unwrap();
                let value=execute(current,root,ledger,shared,Execution {input:None,request:&job.request,frozen:None,operation:Some(oid),accepted:Some(cell)})?;
                let stop=value["status"]=="aborted" || (value["status"]!="ok" && job.request.get("on_error").and_then(Value::as_str).unwrap_or("stop")=="stop");
                results.push(value);if stop {break;}
            }
            ledger.call(Mutation::CancelQueued {operation:oid.into()})?;
            if method=="run" {return Ok(results.into_iter().next().unwrap_or_else(||json!({"status":"canceled"})));}
            return Ok(json!({"executions":results,"session_id":worker.as_ref().map(|worker|worker.session.clone())}));
        }
        let _gate=shared.admission.lock().unwrap();
        match method {
            "checkpoint"=>checkpoint(worker.as_mut().context("no live worker; run or restore first")?,root,python,resource,ledger,shared),
            "restore"|"continue"=>{let mut request=job.request.clone();request["command"]=method.into();restore(worker,root,python,resource,ledger,shared,&request)},
            "restart"=>{
                if let Some(old)=worker.take() {ledger.call(Mutation::SessionState {session:old.session.clone(),state:"closed".into()})?;drop(old);}
                *worker=Some(Worker::start(root,python,resource,ledger,shared,None,None)?);
                Ok(json!({"session_id":worker.as_ref().unwrap().session}))
            }
            _=>Err(invalid("unsupported admitted method")),
        }
    });
    *shared.active_operation.lock().unwrap() = None;
    let finished = (|| -> Result<()> {
        let current = op(root, oid)?;
        if ["completed", "canceled", "failed", "unknown"]
            .iter()
            .any(|state| current["state"] == *state)
        {
            return Ok(());
        }
        let (state, saved) = match result {
            Ok(value) => ("completed", value),
            Err(error) => {
                ledger.call(Mutation::ReconcileFailure {
                    operation: oid.into(),
                })?;
                ("failed", json!({"error":helper::error_body(&error)}))
            }
        };
        ledger.call(Mutation::OperationState {
            operation: oid.into(),
            state: state.into(),
            result: Some(saved),
        })?;
        Ok(())
    })();
    if finished.is_err() {
        shared.stopping.store(true, Ordering::Release);
    }
    if worker.as_mut().is_some_and(|worker| {
        worker
            .process
            .tree
            .child
            .try_wait()
            .ok()
            .flatten()
            .is_some()
            || worker.process.pipes.check().is_err()
    }) {
        let _gate = shared.admission.lock().unwrap();
        if let Some(old) = worker.take() {
            let _ = ledger.call(Mutation::SessionState {
                session: old.session.clone(),
                state: "crashed".into(),
            });
            drop(old);
        }
        *shared.socket.lock().unwrap() = None;
        *shared.status.lock().unwrap() = json!({"pid":std::process::id(),"session_id":null,"in_flight":null,"worker_start_stage":"absent","implementation":"rust","experimental":true});
    }
}

fn page(
    root: &Path,
    sql: &str,
    mut args: Vec<rusqlite::types::Value>,
    request: &Value,
) -> Result<Value> {
    let size = request
        .get("page_size")
        .and_then(Value::as_u64)
        .unwrap_or(100);
    let engine = GeneralPurpose::new(
        &alphabet::STANDARD,
        GeneralPurposeConfig::new().with_decode_allow_trailing_bits(true),
    );
    let mut offset = 0u64;
    if crate::ledger::truthy(request.get("cursor")) {
        let raw = request["cursor"]
            .as_str()
            .ok_or_else(|| invalid("invalid cursor"))?;
        let bytes = engine.decode(raw).map_err(|_| invalid("invalid cursor"))?;
        let digits = std::str::from_utf8(&bytes)
            .map_err(|_| invalid("invalid cursor"))?
            .trim();
        offset = digits
            .strip_prefix('+')
            .unwrap_or(digits)
            .parse()
            .map_err(|_| invalid("invalid cursor"))?;
    }
    if offset > i64::MAX as u64 {
        return Err(invalid("invalid cursor"));
    }
    let ledger = reader(root)?;
    ledger.snapshot(|ledger| {
        let count = ledger.count(sql, &args)?;
        args.push((size as i64).into());
        args.push((offset as i64).into());
        let rows = ledger.rows(&format!("{sql} LIMIT ? OFFSET ?"), &args)?;
        let cursor = if offset.saturating_add(size) < count {
            json!(engine.encode((offset + size).to_string()))
        } else {
            Value::Null
        };
        Ok(json!({"items":rows,"cursor":cursor}))
    })
}
fn status(root: &Path, shared: &Shared) -> Result<Value> {
    let mut value = shared.identity.clone();
    let state = shared.status.lock().unwrap().clone();
    let ledger = reader(root)?;
    let pending = inputs::views(shared);
    value["worker_state"] = if !pending.is_empty() {
        "waiting_input"
    } else if !state["in_flight"].is_null() {
        "running"
    } else if shared.socket.lock().unwrap().is_some() {
        "idle"
    } else {
        "absent"
    }
    .into();
    value["session_id"] = state["session_id"].clone();
    value["active_execution"] = state["in_flight"].clone();
    value["capabilities"] = json!(CAPABILITIES);
    value["interpreter"] = state
        .get("environment")
        .cloned()
        .unwrap_or_else(|| json!({}));
    value["pending_inputs"] = json!(pending);
    value["record_secret_input"] = shared.record_secret_input.into();
    value["attached_client_count"] = shared
        .desktop_clients
        .lock()
        .unwrap()
        .values()
        .filter_map(|hello| hello["client_id"].as_str())
        .collect::<HashSet<_>>()
        .len()
        .into();
    value["queue"] = json!(ledger.rows(
        "SELECT * FROM executions WHERE status='queued' ORDER BY session_id,ordinal",
        &[]
    )?);
    value["active_operations"] = json!(ledger.rows(
        "SELECT * FROM operations WHERE state IN ('accepted','running') ORDER BY accepted",
        &[]
    )?);
    Ok(value)
}
fn control(
    root: &Path,
    request: &Value,
    greeting: &Value,
    shared: &Shared,
    jobs: &mpsc::SyncSender<Job>,
    ledger: &Writer,
    connection: Uuid,
) -> Result<Value> {
    let method = validate(request)?;
    match method {
        "runtime.status" => status(root, shared),
        "input.claim" | "input.release" | "input.reply" | "input.cancel" => {
            inputs::control(method, request, greeting, connection, shared, ledger)
        }
        method if EFFECTS.contains(&method) => {
            accept(root, request, greeting, shared, jobs, ledger)
        }
        "operation.get" => {
            if let Some(id) = request.get("operation_id").and_then(Value::as_str) {
                op(root, id)
            } else {
                let key = request["client_request_id"]
                    .as_str()
                    .ok_or_else(|| invalid("operation_id or client_request_id is required"))?;
                let rows = reader(root)?.rows(
                    "SELECT * FROM operations WHERE client_id=? AND client_request_id=?",
                    &[
                        greeting["client_id"].as_str().unwrap().to_owned().into(),
                        key.to_owned().into(),
                    ],
                )?;
                operation_view(
                    rows.into_iter()
                        .next()
                        .ok_or_else(|| coded("not_found", "operation not found", json!({})))?
                        .into(),
                )
            }
        }
        "operation.list" => page(
            root,
            "SELECT * FROM operations ORDER BY accepted,operation_id",
            Vec::new(),
            request,
        ),
        "changes.list" => {
            let after = i64::try_from(
                request
                    .get("after_change_seq")
                    .and_then(Value::as_u64)
                    .unwrap_or(0),
            )
            .map_err(|_| invalid("after_change_seq exceeds the native integer range"))?;
            let mut result = page(
                root,
                "SELECT * FROM changes WHERE change_seq>? ORDER BY change_seq",
                vec![after.into()],
                request,
            )?;
            for row in result["items"].as_array_mut().unwrap() {
                row["ref"] =
                    serde_json::from_str(row["ref_json"].as_str().context("change reference")?)?;
            }
            Ok(result)
        }
        "operation.cancel" => {
            let oid = request["operation_id"].as_str().unwrap();
            let mut result = ledger.call(Mutation::Cancel {
                operation: oid.into(),
            })?;
            if result["kind"] == "cancel_requested"
                && (result["operation_kind"] == "run" || result["operation_kind"] == "batch")
                && shared.active_operation.lock().unwrap().as_deref() == Some(oid)
            {
                let _send = shared.worker_send.lock().unwrap();
                let mut delivery = shared.cancel_delivery.lock().unwrap();
                if delivery.0.as_deref() == Some(oid)
                    && !delivery.1
                    && let Some(socket) = shared.socket.lock().unwrap().as_mut()
                {
                    kernelledger_protocol::write_worker(
                        socket,
                        &json!({"schema":1,"kind":"interrupt"}),
                    )?;
                    delivery.1 = true;
                }
                if let Some(eid) = shared.status.lock().unwrap()["in_flight"].as_str() {
                    let mut socket = shared.socket.lock().unwrap();
                    inputs::cancel(eid, shared, ledger, socket.as_mut())?;
                }
            }
            result.as_object_mut().unwrap().remove("operation_kind");
            Ok(result)
        }
        "interrupt" | "interrupt_kill" => {
            if method == "interrupt_kill" {
                shared.killed.store(true, Ordering::Release);
            } else {
                let _send = shared.worker_send.lock().unwrap();
                let mut guard = shared.socket.lock().unwrap();
                let socket = guard
                    .as_mut()
                    .ok_or_else(|| coded("busy", "no in-flight execution", json!({})))?;
                if shared.status.lock().unwrap()["in_flight"].is_null() {
                    return Err(coded("busy", "no in-flight execution", json!({})));
                }
                kernelledger_protocol::write_worker(
                    &mut *socket,
                    &json!({"schema":1,"kind":"interrupt"}),
                )?;
                if let Some(eid) = shared.status.lock().unwrap()["in_flight"].as_str() {
                    inputs::cancel(eid, shared, ledger, Some(socket))?;
                }
            }
            Ok(json!({"interrupted":true}))
        }
        _ => Err(invalid("request requires its dedicated connection role")),
    }
}
fn blob(root: &Path, request: &Value, socket: &mut TcpStream) -> Result<()> {
    let store = BlobStore::new(&root.join(".kl"));
    let digest = request["sha256"]
        .as_str()
        .ok_or_else(|| invalid("invalid blob hash"))?;
    crate::store::check_hash(digest)?;
    let mut file = store.open(digest).map_err(|error| {
        if error
            .downcast_ref::<std::io::Error>()
            .is_some_and(|error| error.kind() == std::io::ErrorKind::NotFound)
        {
            coded("not_found", "blob missing", json!({"sha256":digest}))
        } else {
            error
        }
    })?;
    let evidence = store.verify_file(&mut file, digest)?;
    let total = evidence.size;
    let offset = request.get("offset").and_then(Value::as_u64).unwrap_or(0);
    let length = request
        .get("length")
        .and_then(Value::as_u64)
        .unwrap_or(total.max(1));
    if offset > total || length == 0 {
        return Err(invalid("invalid blob range"));
    }
    let end = total.min(offset.saturating_add(length));
    let mut position = offset;
    file.seek(SeekFrom::Start(offset))?;
    let mut buffer = vec![0u8; kernelledger_protocol::FRAME_LIMIT];
    loop {
        let count = (end - position).min(buffer.len() as u64) as usize;
        file.read_exact(&mut buffer[..count])?;
        evidence.check_current(digest)?;
        let final_chunk = position + count as u64 == end;
        write_frame(
            &mut *socket,
            &json!({"kind":"blob","request_id":request.get("request_id"),"sha256":digest,"offset":position,"byte_size":count,"total_size":total,"final":final_chunk,"verified":offset==0 && end==total && final_chunk}),
        )?;
        socket.write_all(&buffer[..count])?;
        position += count as u64;
        if final_chunk {
            break;
        }
    }
    Ok(())
}
pub(super) fn serve(
    mut socket: TcpStream,
    root: &Path,
    secret: &str,
    shared: &Shared,
    jobs: &mpsc::SyncSender<Job>,
    ledger: &Writer,
    connection: Uuid,
) -> Result<()> {
    socket.set_read_timeout(Some(Duration::from_secs(5)))?;
    socket.set_write_timeout(Some(Duration::from_secs(5)))?;
    let outcome = (|| -> Result<()> {
        let greeting = read_frame(&mut socket)?;
        helper::hello(&greeting)?;
        if greeting["token"].as_str() != Some(secret) {
            return Err(coded("unauthenticated", "authentication failed", json!({})));
        }
        shared
            .desktop_clients
            .lock()
            .unwrap()
            .insert(connection, greeting.clone());
        write_frame(
            &mut socket,
            &json!({"kind":"welcome","api":kernelledger_protocol::API,"version":1,"capabilities":CAPABILITIES,"identity":shared.identity,"native_methods":METHODS}),
        )?;
        let role = greeting["role"].as_str().unwrap();
        if role == "subscription" {
            return feeds::serve(&mut socket, root, shared);
        }
        let mut last = Instant::now();
        socket.set_read_timeout(Some(Duration::from_secs(10)))?;
        while !shared.stopping.load(Ordering::Acquire) {
            match socket.peek(&mut [0; 1]) {
                Ok(0) => break,
                Ok(_) => {}
                Err(error)
                    if matches!(
                        error.kind(),
                        std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock
                    ) =>
                {
                    if last.elapsed() >= Duration::from_secs(30) {
                        break;
                    }
                    write_frame(&mut socket, &json!({"kind":"ping"}))?;
                    continue;
                }
                Err(error) => return Err(error.into()),
            }
            let request = read_frame(&mut socket)?;
            last = Instant::now();
            if request
                .get("method")
                .or_else(|| request.get("kind"))
                .and_then(Value::as_str)
                .is_some_and(|method| ["ping", "pong"].contains(&method))
            {
                if request
                    .get("method")
                    .or_else(|| request.get("kind"))
                    .and_then(Value::as_str)
                    == Some("ping")
                {
                    write_frame(&mut socket, &json!({"kind":"pong"}))?;
                }
                continue;
            }
            let response = (|| -> Result<Value> {
                let method = validate(&request)?;
                if method == "blob.read" {
                    if role != "blob" {
                        return Err(invalid("blob.read requires a blob connection"));
                    }
                    blob(root, &request, &mut socket)?;
                    return Ok(Value::Null);
                }
                if role != "control" {
                    return Err(invalid("request requires a control connection"));
                }
                control(root, &request, &greeting, shared, jobs, ledger, connection)
            })();
            match response {
                Ok(result) if request["method"] == "blob.read" && result.is_null() => {}
                Ok(result) => write_frame(
                    &mut socket,
                    &json!({"kind":"response","request_id":request.get("request_id"),"result":result}),
                )?,
                Err(error) => write_frame(
                    &mut socket,
                    &json!({"kind":"error","request_id":request.get("request_id"),"error":helper::error_body(&error)}),
                )?,
            }
        }
        Ok(())
    })();
    if let Err(error) = outcome {
        let _ = write_frame(
            &mut socket,
            &json!({"kind":"error","error":helper::error_body(&error)}),
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{sync::Arc, thread};
    #[test]
    fn stop_admission_bypasses_a_held_lifecycle_gate() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().to_path_buf();
        let ledger = Writer::start(&root.join(".kl")).unwrap();
        let shared = Arc::new(Shared::default());
        let held = shared.admission.lock().unwrap();
        let (jobs, _receiver) = mpsc::sync_channel(1);
        let (send, result) = mpsc::sync_channel(1);
        let child_shared = shared.clone();
        let child_ledger = ledger.clone();
        let request = json!({"method":"stop","client_request_id":Uuid::new_v4().to_string()});
        let greeting = json!({"client_id":Uuid::new_v4().to_string()});
        let child_request = request.clone();
        let child_greeting = greeting.clone();
        let child_jobs = jobs.clone();
        let child_root = root.clone();
        let child = thread::spawn(move || {
            let _ = send.send(accept(
                &child_root,
                &child_request,
                &child_greeting,
                &child_shared,
                &child_jobs,
                &child_ledger,
            ));
        });
        let accepted = result
            .recv_timeout(Duration::from_secs(2))
            .expect("stop blocked behind lifecycle work")
            .unwrap();
        let retry = accept(&root, &request, &greeting, &shared, &jobs, &ledger).unwrap();
        assert_eq!(accepted["operation_id"], retry["operation_id"]);
        assert!(shared.stopping.load(Ordering::Acquire));
        drop(held);
        child.join().unwrap();
    }
}
