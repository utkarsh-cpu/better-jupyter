//! Single-threaded, bounded domain-command ledger owner. No network SQL surface.
//! The runtime lock lives on the owner thread until its connection is closed.
use crate::{
    platform::FileLock,
    store::{coded, invalid},
};
use anyhow::{Context, Result, ensure};
use rusqlite::{Connection, params};
use serde_json::{Value, json};
use std::{
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
        mpsc,
    },
    thread,
};
use uuid::Uuid;

pub const QUEUE_LIMIT: usize = 64;
pub const QUEUE_BYTES: usize = 8 * 1024 * 1024;
#[derive(Clone)]
pub struct FrozenSource {
    pub cell: Option<String>,
    pub source: String,
}
struct Reservation {
    bytes: usize,
    budget: Arc<AtomicUsize>,
}
impl Drop for Reservation {
    fn drop(&mut self) {
        self.budget.fetch_sub(self.bytes, Ordering::AcqRel);
    }
}
fn value_weight(value: &Value) -> usize {
    // Include container overhead as well as string payloads; no serialization copy.
    64 + match value {
        Value::String(text) => text.len(),
        Value::Array(items) => items.iter().map(value_weight).sum(),
        Value::Object(items) => items
            .iter()
            .map(|(key, value)| key.len() + value_weight(value))
            .sum(),
        _ => 0,
    }
}
impl Mutation {
    fn weight(&self) -> usize {
        512 + match self {
            Self::Session { environment, .. } => value_weight(environment),
            Self::Event { payload, .. } => value_weight(payload),
            Self::InputRequested { payload, .. } => value_weight(payload),
            Self::InputState { view, .. } => value_weight(view),
            Self::InputAnswer { view, reply, .. } => value_weight(view) + value_weight(reply),
            Self::SessionEvent { payload, .. } => value_weight(payload),
            Self::Accept { request, .. } => value_weight(request),
            Self::Admit { request, cells, .. } => {
                value_weight(request)
                    + cells
                        .iter()
                        .map(|cell| {
                            512 + cell.source.len() + cell.cell.as_ref().map_or(0, String::len)
                        })
                        .sum::<usize>()
            }
            Self::OperationState {
                result: Some(value),
                ..
            } => value_weight(value),
            Self::OperationProgress { result, .. } => value_weight(result),
            _ => 0,
        }
    }
}

pub enum Mutation {
    InputRequested {
        execution: String,
        payload: Value,
    },
    InputState {
        view: Value,
    },
    InputAnswer {
        view: Value,
        reply: Value,
        record_secret: bool,
    },
    Session {
        notebook: String,
        environment: Value,
        parent: Option<String>,
        checkpoint: Option<String>,
    },
    Execution {
        session: String,
        cell: Option<String>,
        source: String,
    },
    Event {
        execution: String,
        kind: String,
        payload: Value,
    },
    SessionEvent {
        session: String,
        kind: String,
        payload: Value,
    },
    ExecutionState {
        execution: String,
        state: String,
    },
    SessionState {
        session: String,
        state: String,
    },
    Accept {
        client: String,
        key: String,
        kind: String,
        request: Value,
    },
    Admit {
        client: String,
        key: String,
        kind: String,
        request: Value,
        session: Option<String>,
        cells: Vec<FrozenSource>,
        on_error: String,
        refuse_busy: bool,
    },
    BeginExecution {
        operation: String,
        execution: String,
    },
    Cancel {
        operation: String,
    },
    CancelQueued {
        operation: String,
    },
    ReconcileFailure {
        operation: String,
    },
    CancelPending {
        except: Option<String>,
    },
    OperationState {
        operation: String,
        state: String,
        result: Option<Value>,
    },
    Operation {
        operation: String,
    },
    OperationProgress {
        operation: String,
        result: Value,
    },
    Checkpoint {
        session: String,
        manifest: String,
        status: String,
    },
    Takeover,
}
struct Envelope {
    command: Mutation,
    reply: mpsc::SyncSender<Result<Value>>,
    _reservation: Reservation,
}

#[derive(Clone)]
pub struct Writer {
    owner: Arc<Owner>,
}
struct Owner {
    sender: Option<mpsc::SyncSender<Envelope>>,
    thread: Option<thread::JoinHandle<()>>,
    budget: Arc<AtomicUsize>,
}
impl Drop for Owner {
    fn drop(&mut self) {
        self.sender.take();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}
impl Writer {
    pub fn start(state: &Path) -> Result<Self> {
        let state = state.to_path_buf();
        let (sender, receiver) = mpsc::sync_channel::<Envelope>(QUEUE_LIMIT);
        let (ready_tx, ready_rx) = mpsc::sync_channel(1);
        let thread = thread::Builder::new()
            .name("kl-ledger".into())
            .spawn(move || {
                let opened = (|| -> Result<_> {
                    let lock = FileLock::acquire(&state.join("lock"))?;
                    let conn = Connection::open(state.join("ledger.sqlite"))?;
                    conn.busy_timeout(std::time::Duration::from_secs(5))?;
                    let version: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
                    ensure!(
                        version == 0 || version == 2,
                        "native writer requires schema 2; migrate schema 1 using Python first"
                    );
                    conn.execute_batch(
                        "PRAGMA journal_mode=WAL; PRAGMA synchronous=FULL; PRAGMA foreign_keys=ON;",
                    )?;
                    if version == 0 {
                        conn.execute_batch("BEGIN IMMEDIATE")?;
                        conn.execute_batch(include_str!("schema.sql"))?;
                        conn.execute_batch("PRAGMA user_version=2; COMMIT")?;
                    }
                    Ok((lock, conn))
                })();
                match opened {
                    Err(error) => {
                        let _ = ready_tx.send(Err(error));
                    }
                    Ok((_lock, mut conn)) => {
                        if ready_tx.send(Ok(())).is_err() {
                            return;
                        }
                        for envelope in receiver {
                            let result = apply(&mut conn, &state, envelope.command);
                            let _ = envelope.reply.send(result);
                        }
                    }
                }
            })?;
        ready_rx
            .recv()
            .context("ledger owner failed during startup")??;
        Ok(Self {
            owner: Arc::new(Owner {
                sender: Some(sender),
                thread: Some(thread),
                budget: Arc::new(AtomicUsize::new(0)),
            }),
        })
    }

    /// Reject overload before acceptance. Once queued, a lost caller cannot cancel
    /// a commit: its result remains discoverable by the original idempotency key.
    pub fn call(&self, command: Mutation) -> Result<Value> {
        let bytes = command.weight();
        self.owner
            .budget
            .try_update(Ordering::AcqRel, Ordering::Acquire, |used| {
                used.checked_add(bytes).filter(|next| *next <= QUEUE_BYTES)
            })
            .map_err(|_| coded("busy", "ledger byte budget is full", json!({})))?;
        let reservation = Reservation {
            bytes,
            budget: self.owner.budget.clone(),
        };
        let (reply, receive) = mpsc::sync_channel(1);
        self.owner
            .sender
            .as_ref()
            .context("ledger owner closed")?
            .try_send(Envelope {
                command,
                reply,
                _reservation: reservation,
            })
            .map_err(|error| match error {
                mpsc::TrySendError::Full(_) => coded("busy", "ledger queue is full", json!({})),
                mpsc::TrySendError::Disconnected(_) => {
                    coded("internal", "ledger owner stopped", json!({}))
                }
            })?;
        receive
            .recv()
            .context("ledger owner stopped before reply")?
    }
}

fn now(conn: &Connection) -> Result<String> {
    Ok(
        conn.query_row("SELECT strftime('%Y-%m-%dT%H:%M:%fZ','now')", [], |r| {
            r.get(0)
        })?,
    )
}
fn change(conn: &Connection, kind: &str, reference: Value, timestamp: &str) -> Result<()> {
    conn.execute(
        "INSERT INTO changes(kind,ref_json,timestamp) VALUES (?,?,?)",
        params![kind, reference.to_string(), timestamp],
    )?;
    Ok(())
}
fn operation(conn: &Connection, oid: &str) -> Result<Value> {
    let value = conn.query_row("SELECT operation_id,client_id,client_request_id,kind,request_sha256,state,result_json,accepted,started,finished FROM operations WHERE operation_id=?", [oid], |r| {
        let result: Option<String> = r.get(6)?;
        Ok(json!({"operation_id":r.get::<_,String>(0)?,"client_id":r.get::<_,String>(1)?,"client_request_id":r.get::<_,String>(2)?,"kind":r.get::<_,String>(3)?,"request_sha256":r.get::<_,String>(4)?,"state":r.get::<_,String>(5)?,"result":result.and_then(|s| serde_json::from_str::<Value>(&s).ok()),"accepted":r.get::<_,String>(7)?,"started":r.get::<_,Option<String>>(8)?,"finished":r.get::<_,Option<String>>(9)?}))
    });
    match value {
        Err(rusqlite::Error::QueryReturnedNoRows) => {
            Err(coded("not_found", "operation not found", json!({})))
        }
        other => Ok(other?),
    }
}
fn cancel_queued(conn: &Connection, row: &Value, timestamp: &str) -> Result<()> {
    for id in row["result"]["exec_ids"].as_array().into_iter().flatten() {
        conn.execute(
            "UPDATE executions SET status='canceled',end=? WHERE exec_id=? AND status='queued'",
            params![
                timestamp,
                id.as_str().context("invalid admitted execution id")?
            ],
        )?;
    }
    Ok(())
}
fn apply(conn: &mut Connection, state: &Path, command: Mutation) -> Result<Value> {
    if let Mutation::Admit {
        client,
        key,
        request,
        ..
    } = &command
    {
        let existing=conn.query_row("SELECT operation_id,request_sha256 FROM operations WHERE client_id=? AND client_request_id=?",params![client,key],|row|Ok((row.get::<_,String>(0)?,row.get::<_,String>(1)?)));
        match existing {
            Ok((oid, digest)) => {
                if digest != kernelledger_protocol::request_hash(request)? {
                    return Err(coded(
                        "key_reused",
                        "request key already used for different content",
                        json!({}),
                    ));
                }
                return Ok(
                    json!({"operation":operation(conn,&oid)?,"fresh":false,"executions":[]}),
                );
            }
            Err(rusqlite::Error::QueryReturnedNoRows) => {}
            Err(error) => return Err(error.into()),
        }
    }
    // Verify durable files before opening the publication transaction. No payload
    // copies or long file transfers occur while holding the SQLite transaction.
    let store = crate::store::BlobStore::new(state);
    let mut verified = Vec::new();
    let seeds = match &command {
        Mutation::Execution { source, .. } => vec![source.clone()],
        Mutation::Admit { cells, .. } => cells.iter().map(|cell| cell.source.clone()).collect(),
        Mutation::Checkpoint { manifest, .. } => {
            verified.extend(store.verified_closure(std::slice::from_ref(manifest))?);
            Vec::new()
        }
        Mutation::Event { payload, .. } | Mutation::SessionEvent { payload, .. } => {
            let mut hashes = Vec::new();
            crate::store::references(payload, &mut hashes);
            hashes
        }
        _ => Vec::new(),
    };
    for digest in seeds {
        let mut file = store.open(&digest)?;
        verified.push((digest.clone(), store.verify_file(&mut file, &digest)?));
    }
    let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
    let timestamp = now(&tx)?;
    for (digest, evidence) in &verified {
        evidence.check_current(digest)?;
        tx.execute(
            "INSERT OR IGNORE INTO blobs VALUES (?,?,NULL)",
            params![digest, i64::try_from(evidence.size)?],
        )?;
    }
    let result = match command {
        Mutation::Admit {
            client,
            key,
            kind,
            request,
            session,
            cells,
            on_error,
            refuse_busy,
        } => {
            Uuid::parse_str(&key).map_err(|_| invalid("invalid client request UUID"))?;
            let digest = kernelledger_protocol::request_hash(&request)?;
            if !cells.is_empty() {
                let sid = session
                    .as_deref()
                    .context("admitted execution requires a session")?;
                let state: String = tx.query_row(
                    "SELECT state FROM sessions WHERE session_id=?",
                    [sid],
                    |row| row.get(0),
                )?;
                if state != "running" {
                    return Err(coded(
                        "busy",
                        "accepted worker session is no longer available",
                        json!({}),
                    ));
                }
            }
            let oid = Uuid::new_v4().to_string();
            tx.execute(
                "INSERT INTO operations VALUES (?,?,?,?,?,'accepted',NULL,?,NULL,NULL)",
                params![oid, client, key, kind, digest, timestamp],
            )?;
            change(
                &tx,
                "operation_state",
                json!({"operation_id":oid,"state":"accepted"}),
                &timestamp,
            )?;
            let mut executions = Vec::new();
            let mut ids = Vec::new();
            for cell in cells {
                let sid = session.as_deref().context("missing admission session")?;
                let eid = Uuid::new_v4().to_string();
                tx.execute("INSERT INTO executions SELECT ?,?,COALESCE(MAX(ordinal),0)+1,?,?,'queued',?,NULL FROM executions WHERE session_id=?",params![eid,sid,cell.cell,cell.source,timestamp,sid])?;
                ids.push(eid.clone());
                executions.push(json!({"exec_id":eid,"session_id":sid,"cell_id":cell.cell,"source_sha256":cell.source}));
            }
            if kind == "batch" {
                tx.execute(
                    "INSERT INTO batches VALUES (?,?,?,?,?)",
                    params![
                        Uuid::new_v4().to_string(),
                        oid,
                        session.as_deref().context("batch session")?,
                        on_error,
                        serde_json::to_string(&ids)?
                    ],
                )?;
            }
            if kind == "run" || kind == "batch" {
                tx.execute(
                    "UPDATE operations SET result_json=? WHERE operation_id=?",
                    params![
                        json!({"exec_ids":ids,"session_id":session}).to_string(),
                        oid
                    ],
                )?;
            }
            if refuse_busy {
                tx.execute("UPDATE operations SET state='completed',result_json=?,finished=? WHERE operation_id=?",params![json!({"refused":"busy"}).to_string(),timestamp,oid])?;
                change(
                    &tx,
                    "operation_state",
                    json!({"operation_id":oid,"state":"completed"}),
                    &timestamp,
                )?;
                change(
                    &tx,
                    "checkpoint_refused",
                    json!({"operation_id":oid,"refused":"busy"}),
                    &timestamp,
                )?;
            }
            json!({"operation":operation(&tx,&oid)?,"fresh":true,"executions":executions})
        }
        Mutation::BeginExecution {
            operation: oid,
            execution,
        } => {
            let row = operation(&tx, &oid)?;
            let belongs = row["result"]["exec_ids"]
                .as_array()
                .is_some_and(|ids| ids.iter().any(|id| id == &execution));
            let allowed =
                belongs && row["state"] == "running" && row["result"]["cancel_requested"] != true;
            let count = if allowed {
                tx.execute(
                    "UPDATE executions SET status='running' WHERE exec_id=? AND status='queued'",
                    [&execution],
                )?
            } else {
                0
            };
            json!({"started":count==1})
        }
        Mutation::Cancel { operation: oid } => {
            let row = operation(&tx, &oid)?;
            let state = row["state"].as_str().context("operation state")?;
            if ["completed", "canceled", "failed", "unknown"].contains(&state) {
                json!({"kind":"already_final","state":state,"operation_kind":row["kind"]})
            } else {
                if state == "running" && row["kind"] != "run" && row["kind"] != "batch" {
                    return Err(coded(
                        "busy",
                        "operation is not cancelable once running",
                        json!({}),
                    ));
                }
                cancel_queued(&tx, &row, &timestamp)?;
                let mut saved = row["result"].as_object().cloned().unwrap_or_default();
                saved.insert("cancel_requested".into(), true.into());
                let next = if state == "accepted" {
                    "canceled"
                } else {
                    state
                };
                tx.execute("UPDATE operations SET state=?,result_json=?,finished=CASE WHEN ?='canceled' THEN ? ELSE finished END WHERE operation_id=?",params![next,Value::Object(saved).to_string(),next,timestamp,oid])?;
                change(
                    &tx,
                    "operation_state",
                    json!({"operation_id":oid,"state":next}),
                    &timestamp,
                )?;
                json!({"kind":"cancel_requested","state":next,"operation_kind":row["kind"]})
            }
        }
        Mutation::CancelQueued { operation: oid } => {
            cancel_queued(&tx, &operation(&tx, &oid)?, &timestamp)?;
            json!({"canceled":true})
        }
        Mutation::ReconcileFailure { operation: oid } => {
            let row = operation(&tx, &oid)?;
            cancel_queued(&tx, &row, &timestamp)?;
            for id in row["result"]["exec_ids"].as_array().into_iter().flatten() {
                tx.execute("UPDATE executions SET status='aborted',end=? WHERE exec_id=? AND status='running'",params![timestamp,id.as_str().context("execution id")?])?;
            }
            json!({"reconciled":true})
        }
        Mutation::CancelPending { except } => {
            let ids: Vec<String> = {
                let mut query =
                    tx.prepare("SELECT operation_id FROM operations WHERE state='accepted'")?;
                query
                    .query_map([], |row| row.get(0))?
                    .collect::<rusqlite::Result<_>>()?
            };
            for oid in ids {
                if except.as_deref() == Some(&oid) {
                    continue;
                }
                let row = operation(&tx, &oid)?;
                cancel_queued(&tx, &row, &timestamp)?;
                let mut saved = row["result"].as_object().cloned().unwrap_or_default();
                saved.insert("reason".into(), "runtime_stopped".into());
                tx.execute("UPDATE operations SET state='canceled',finished=?,result_json=? WHERE operation_id=?",params![timestamp,Value::Object(saved).to_string(),oid])?;
                change(
                    &tx,
                    "operation_state",
                    json!({"operation_id":oid,"state":"canceled"}),
                    &timestamp,
                )?;
            }
            json!({"canceled":true})
        }
        Mutation::Session {
            notebook,
            environment,
            parent,
            checkpoint,
        } => {
            let sid = Uuid::new_v4().to_string();
            tx.execute(
                "INSERT INTO sessions VALUES (?,?,?,?,'running',?,NULL,?,?,'local')",
                params![
                    sid,
                    notebook,
                    parent,
                    checkpoint,
                    timestamp,
                    environment.to_string(),
                    json!({"kernelledger":env!("CARGO_PKG_VERSION"),"implementation":"rust"})
                        .to_string()
                ],
            )?;
            json!({"session_id":sid})
        }
        Mutation::Execution {
            session,
            cell,
            source,
        } => {
            crate::store::check_hash(&source)?;
            let eid = Uuid::new_v4().to_string();
            tx.execute("INSERT INTO executions SELECT ?,?,COALESCE(MAX(ordinal),0)+1,?,?,'queued',?,NULL FROM executions WHERE session_id=?", params![eid, session, cell, source, timestamp, session])?;
            json!({"exec_id":eid})
        }
        Mutation::InputRequested { execution, payload } => {
            let seq: i64 = tx.query_row(
                "SELECT COALESCE(MAX(seq),0)+1 FROM events WHERE exec_id=?",
                [&execution],
                |r| r.get(0),
            )?;
            tx.execute(
                "INSERT INTO events VALUES (?,?,?,?,?,NULL)",
                params![
                    execution,
                    seq,
                    "stdin_request",
                    timestamp,
                    payload.to_string()
                ],
            )?;
            let iid = format!("{execution}:{seq}");
            let password = payload["password"] == true;
            tx.execute(
                "INSERT INTO input_requests VALUES (?,?,?,?,?,?,?,?)",
                params![
                    iid,
                    execution,
                    seq,
                    password,
                    "pending",
                    Option::<String>::None,
                    timestamp,
                    timestamp
                ],
            )?;
            let mut view = payload;
            view["input_id"] = iid.into();
            view["exec_id"] = execution.into();
            view["seq"] = seq.into();
            view["client_name"] = Value::Null;
            change(&tx, "input_state", view.clone(), &timestamp)?;
            view
        }
        Mutation::InputState { view } => {
            let changed = tx.execute("UPDATE input_requests SET client_name=?,updated=? WHERE input_id=? AND state='pending'",params![view["client_name"].as_str(),timestamp,view["input_id"].as_str().context("input id")?])?;
            ensure!(changed == 1, "input request no longer pending");
            change(&tx, "input_state", view, &timestamp)?;
            json!({"updated":true})
        }
        Mutation::InputAnswer {
            mut view,
            reply,
            record_secret,
        } => {
            let state = if reply.get("error").is_some() {
                "canceled"
            } else {
                "answered"
            };
            let changed = tx.execute("UPDATE input_requests SET state=?,client_name=?,updated=? WHERE input_id=? AND state='pending'",params![state,view["client_name"].as_str(),timestamp,view["input_id"].as_str().context("input id")?])?;
            if changed == 0 {
                tx.commit()?;
                return Ok(json!({"updated":false}));
            }
            let execution = view["exec_id"].as_str().context("input execution")?;
            let password = view["password"] == true;
            let recorded = if password && !record_secret {
                json!({"password":true,"redacted":true})
            } else {
                let mut recorded = reply;
                recorded["password"] = password.into();
                recorded
            };
            let seq: i64 = tx.query_row(
                "SELECT COALESCE(MAX(seq),0)+1 FROM events WHERE exec_id=?",
                [execution],
                |r| r.get(0),
            )?;
            tx.execute(
                "INSERT INTO events VALUES (?,?,?,?,?,NULL)",
                params![
                    execution,
                    seq,
                    "stdin_reply",
                    timestamp,
                    recorded.to_string()
                ],
            )?;
            view["state"] = state.into();
            change(&tx, "input_state", view, &timestamp)?;
            json!({"updated":true})
        }
        Mutation::Event {
            execution,
            kind,
            payload,
        } => {
            let seq: i64 = tx.query_row(
                "SELECT COALESCE(MAX(seq),0)+1 FROM events WHERE exec_id=?",
                [&execution],
                |r| r.get(0),
            )?;
            tx.execute(
                "INSERT INTO events VALUES (?,?,?,?,?,?)",
                params![
                    execution,
                    seq,
                    kind,
                    timestamp,
                    payload.to_string(),
                    payload.get("blob").and_then(Value::as_str)
                ],
            )?;
            json!({"seq":seq})
        }
        Mutation::SessionEvent {
            session,
            kind,
            payload,
        } => {
            let seq: i64 = tx.query_row(
                "SELECT COALESCE(MAX(seq),0)+1 FROM session_events WHERE session_id=?",
                [&session],
                |r| r.get(0),
            )?;
            tx.execute(
                "INSERT INTO session_events VALUES (?,?,?,?,?)",
                params![session, seq, kind, timestamp, payload.to_string()],
            )?;
            json!({"seq":seq})
        }
        Mutation::ExecutionState { execution, state } => {
            ensure!(
                [
                    "running",
                    "ok",
                    "error",
                    "interrupted",
                    "aborted",
                    "canceled",
                    "incomplete"
                ]
                .contains(&state.as_str()),
                "invalid execution state"
            );
            tx.execute("UPDATE executions SET status=?,end=CASE WHEN ?='running' THEN NULL ELSE ? END WHERE exec_id=?", params![state, state, timestamp, execution])?;
            json!({"updated":true})
        }
        Mutation::SessionState { session, state } => {
            ensure!(
                ["closed", "killed", "crashed"].contains(&state.as_str()),
                "invalid session state"
            );
            tx.execute("UPDATE executions SET status='aborted',end=? WHERE session_id=? AND status IN ('queued','running')", params![timestamp, session])?;
            tx.execute(
                "UPDATE sessions SET state=?,end=? WHERE session_id=?",
                params![state, timestamp, session],
            )?;
            json!({"updated":true})
        }
        Mutation::Accept {
            client,
            key,
            kind,
            request,
        } => {
            Uuid::parse_str(&key).map_err(|_| invalid("invalid client request UUID"))?;
            let digest = kernelledger_protocol::request_hash(&request)?;
            let existing = tx.query_row("SELECT operation_id,request_sha256 FROM operations WHERE client_id=? AND client_request_id=?", params![client,key], |r| Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?)));
            let (oid, fresh) = match existing {
                Ok((oid, old)) => {
                    if old != digest {
                        return Err(coded(
                            "key_reused",
                            "request key already used for different content",
                            json!({}),
                        ));
                    }
                    (oid, false)
                }
                Err(rusqlite::Error::QueryReturnedNoRows) => {
                    let oid = Uuid::new_v4().to_string();
                    tx.execute(
                        "INSERT INTO operations VALUES (?,?,?,?,?,'accepted',NULL,?,NULL,NULL)",
                        params![oid, client, key, kind, digest, timestamp],
                    )?;
                    change(
                        &tx,
                        "operation_state",
                        json!({"operation_id":oid,"state":"accepted"}),
                        &timestamp,
                    )?;
                    (oid, true)
                }
                Err(error) => return Err(error.into()),
            };
            json!({"operation":operation(&tx,&oid)?,"fresh":fresh})
        }
        Mutation::Operation { operation: oid } => operation(&tx, &oid)?,
        Mutation::OperationProgress {
            operation: oid,
            result,
        } => {
            ensure!(
                operation(&tx, &oid)?["state"] == "running",
                "only running operations can record progress"
            );
            tx.execute(
                "UPDATE operations SET result_json=? WHERE operation_id=?",
                params![result.to_string(), oid],
            )?;
            operation(&tx, &oid)?
        }
        Mutation::Checkpoint {
            session,
            manifest,
            status,
        } => {
            ensure!(
                ["complete", "partial"].contains(&status.as_str()),
                "invalid checkpoint status"
            );
            let id = Uuid::new_v4().to_string();
            tx.execute("INSERT INTO checkpoints VALUES (?, ?, (SELECT exec_id FROM executions WHERE session_id=? ORDER BY ordinal DESC LIMIT 1), ?, ?, ?, 'local')",params![id,session,session,status,manifest,timestamp])?;
            json!({"checkpoint_id":id,"status":status})
        }
        Mutation::OperationState {
            operation: oid,
            state,
            result,
        } => {
            let previous = operation(&tx, &oid)?;
            let old = previous["state"].as_str().unwrap_or("");
            let valid = matches!(
                (old, state.as_str()),
                ("accepted", "running" | "canceled" | "failed")
                    | ("running", "completed" | "failed" | "unknown" | "canceled")
            );
            ensure!(valid, "invalid operation transition {old} -> {state}");
            tx.execute("UPDATE operations SET state=?,result_json=COALESCE(?,result_json),started=CASE WHEN ?='running' THEN ? ELSE started END,finished=CASE WHEN ?='running' THEN finished ELSE ? END WHERE operation_id=?", params![state,result.map(|r|r.to_string()),state,timestamp,state,timestamp,oid])?;
            change(
                &tx,
                "operation_state",
                json!({"operation_id":oid,"state":state}),
                &timestamp,
            )?;
            operation(&tx, &oid)?
        }
        Mutation::Takeover => {
            tx.execute("UPDATE executions SET status='canceled',end=? WHERE status='queued' AND exec_id IN (SELECT value FROM batches,json_each(batches.exec_ids_json))",[&timestamp])?;
            let pending: Vec<(String, String, Option<String>)> = {
                let mut query = tx.prepare("SELECT operation_id,state,result_json FROM operations WHERE state IN ('accepted','running')")?;
                query
                    .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
                    .collect::<rusqlite::Result<_>>()?
            };
            for (oid, state, saved) in pending {
                let state = if state == "accepted" {
                    "canceled"
                } else {
                    "unknown"
                };
                let mut known: Value = saved
                    .and_then(|s| serde_json::from_str(&s).ok())
                    .unwrap_or_else(|| json!({}));
                ensure!(known.is_object(), "invalid saved operation result");
                known["reason"] = "supervisor takeover".into();
                let mut executions = Vec::new();
                for eid in known["exec_ids"].as_array().into_iter().flatten() {
                    let eid = eid.as_str().context("invalid saved execution ID")?;
                    if state == "canceled" {
                        tx.execute("UPDATE executions SET status='canceled',end=? WHERE exec_id=? AND status='queued'",params![timestamp,eid])?;
                    }
                    let mut q =
                        tx.prepare("SELECT exec_id,status FROM executions WHERE exec_id=?")?;
                    executions.extend(q.query_map([eid],|r|Ok(json!({"exec_id":r.get::<_,String>(0)?,"status":r.get::<_,String>(1)?})))?.collect::<rusqlite::Result<Vec<_>>>()?);
                }
                known["executions"] = executions.into();
                tx.execute(
                    "UPDATE operations SET state=?,finished=?,result_json=? WHERE operation_id=?",
                    params![state, timestamp, known.to_string(), oid],
                )?;
                change(
                    &tx,
                    "operation_state",
                    json!({"operation_id":oid,"state":state}),
                    &timestamp,
                )?;
            }
            let pending = {
                let mut query = tx.prepare("SELECT input_id,exec_id,seq,password FROM input_requests WHERE state='pending'")?;
                query.query_map([],|row|Ok(json!({"input_id":row.get::<_,String>(0)?,"exec_id":row.get::<_,String>(1)?,"seq":row.get::<_,i64>(2)?,"password":row.get::<_,bool>(3)?,"client_name":null,"state":"canceled"})))?.collect::<rusqlite::Result<Vec<_>>>()?
            };
            tx.execute("UPDATE input_requests SET state='canceled',client_name=NULL,updated=? WHERE state='pending'",[&timestamp])?;
            for view in pending {
                change(&tx, "input_state", view, &timestamp)?;
            }
            tx.execute("UPDATE executions SET status='incomplete',end=? WHERE status IN ('queued','running')",[&timestamp])?;
            tx.execute(
                "UPDATE sessions SET state='crashed',end=? WHERE state='running'",
                [&timestamp],
            )?;
            json!({"recovered":true})
        }
    };
    tx.commit()?;
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn input_answer_failure_rolls_back_and_retry_records_one_redacted_reply() {
        let directory = tempfile::tempdir().unwrap();
        let writer = Writer::start(directory.path()).unwrap();
        let session = writer
            .call(Mutation::Session {
                notebook: Uuid::new_v4().to_string(),
                environment: json!({}),
                parent: None,
                checkpoint: None,
            })
            .unwrap()["session_id"]
            .as_str()
            .unwrap()
            .to_owned();
        let source = crate::store::BlobStore::new(directory.path())
            .publish(b"input()", None)
            .unwrap();
        let execution = writer
            .call(Mutation::Execution {
                session,
                cell: None,
                source,
            })
            .unwrap()["exec_id"]
            .as_str()
            .unwrap()
            .to_owned();
        let view = writer
            .call(Mutation::InputRequested {
                execution,
                payload: json!({"prompt":"Secret: ","password":true}),
            })
            .unwrap();
        let db = Connection::open(directory.path().join("ledger.sqlite")).unwrap();
        db.execute_batch("CREATE TRIGGER reject_input_reply BEFORE INSERT ON events WHEN NEW.kind='stdin_reply' BEGIN SELECT RAISE(ABORT,'injected publication failure'); END;").unwrap();
        let answer = || Mutation::InputAnswer {
            view: view.clone(),
            reply: json!({"text":"never-store-this"}),
            record_secret: false,
        };
        assert!(writer.call(answer()).is_err());
        assert_eq!(
            db.query_row("SELECT state FROM input_requests", [], |row| row
                .get::<_, String>(0))
                .unwrap(),
            "pending"
        );
        db.execute_batch("DROP TRIGGER reject_input_reply").unwrap();
        assert_eq!(writer.call(answer()).unwrap()["updated"], true);
        assert_eq!(writer.call(answer()).unwrap()["updated"], false);
        assert_eq!(
            db.query_row(
                "SELECT COUNT(*) FROM events WHERE kind='stdin_reply'",
                [],
                |row| row.get::<_, i64>(0)
            )
            .unwrap(),
            1
        );
        let payload: String = db
            .query_row(
                "SELECT payload_json FROM events WHERE kind='stdin_reply'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(
            serde_json::from_str::<Value>(&payload).unwrap(),
            json!({"password":true,"redacted":true})
        );
        assert_eq!(
            db.query_row(
                "SELECT COUNT(*) FROM changes WHERE kind='input_state'",
                [],
                |row| row.get::<_, i64>(0)
            )
            .unwrap(),
            2
        );
        let mut pending = writer
            .call(Mutation::InputRequested {
                execution: view["exec_id"].as_str().unwrap().into(),
                payload: json!({"prompt":"Recover: ","password":false}),
            })
            .unwrap();
        pending["client_name"] = "Disconnected".into();
        writer
            .call(Mutation::InputState {
                view: pending.clone(),
            })
            .unwrap();
        writer.call(Mutation::Takeover).unwrap();
        let recovered: (String, Option<String>) = db
            .query_row(
                "SELECT state,client_name FROM input_requests WHERE input_id=?",
                [pending["input_id"].as_str().unwrap()],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(recovered, ("canceled".into(), None));
        let reference:String = db.query_row("SELECT ref_json FROM changes WHERE kind='input_state' ORDER BY change_seq DESC LIMIT 1",[],|row|row.get(0)).unwrap();
        assert_eq!(
            serde_json::from_str::<Value>(&reference).unwrap()["state"],
            "canceled"
        );
    }
    #[test]
    fn desktop_admission_is_atomic_and_cancellation_wins_before_dispatch() {
        let directory = tempfile::tempdir().unwrap();
        let writer = Writer::start(directory.path()).unwrap();
        let session = writer
            .call(Mutation::Session {
                notebook: Uuid::new_v4().to_string(),
                environment: json!({}),
                parent: None,
                checkpoint: None,
            })
            .unwrap()["session_id"]
            .as_str()
            .unwrap()
            .to_owned();
        let source = crate::store::BlobStore::new(directory.path())
            .publish(b"pass", None)
            .unwrap();
        let key = Uuid::new_v4().to_string();
        let make = |cells| Mutation::Admit {
            client: "desktop".into(),
            key: key.clone(),
            kind: "batch".into(),
            request: json!({"method":"batch"}),
            session: Some(session.clone()),
            cells,
            on_error: "stop".into(),
            refuse_busy: false,
        };
        assert!(
            writer
                .call(make(vec![
                    FrozenSource {
                        cell: Some("a".into()),
                        source: source.clone()
                    },
                    FrozenSource {
                        cell: Some("b".into()),
                        source: "ab".repeat(32)
                    }
                ]))
                .is_err()
        );
        let db = Connection::open(directory.path().join("ledger.sqlite")).unwrap();
        assert_eq!(
            db.query_row("SELECT COUNT(*) FROM operations", [], |row| row
                .get::<_, i64>(0))
                .unwrap(),
            0
        );
        assert_eq!(
            db.query_row("SELECT COUNT(*) FROM executions", [], |row| row
                .get::<_, i64>(0))
                .unwrap(),
            0
        );
        let admitted = writer
            .call(make(vec![FrozenSource {
                cell: Some("a".into()),
                source,
            }]))
            .unwrap();
        let oid = admitted["operation"]["operation_id"]
            .as_str()
            .unwrap()
            .to_owned();
        let eid = admitted["executions"][0]["exec_id"]
            .as_str()
            .unwrap()
            .to_owned();
        assert_eq!(writer.call(make(Vec::new())).unwrap()["fresh"], false);
        writer
            .call(Mutation::OperationState {
                operation: oid.clone(),
                state: "running".into(),
                result: None,
            })
            .unwrap();
        writer
            .call(Mutation::Cancel {
                operation: oid.clone(),
            })
            .unwrap();
        assert_eq!(
            writer
                .call(Mutation::BeginExecution {
                    operation: oid,
                    execution: eid.clone()
                })
                .unwrap()["started"],
            false
        );
        assert_eq!(
            db.query_row(
                "SELECT status FROM executions WHERE exec_id=?",
                [eid],
                |row| row.get::<_, String>(0)
            )
            .unwrap(),
            "canceled"
        );
    }
    #[test]
    fn rejected_publication_and_lost_reply_leave_recoverable_state() {
        let directory = tempfile::tempdir().unwrap();
        let writer = Writer::start(directory.path()).unwrap();
        let sid = writer
            .call(Mutation::Session {
                notebook: Uuid::new_v4().to_string(),
                environment: json!({}),
                parent: None,
                checkpoint: None,
            })
            .unwrap()["session_id"]
            .as_str()
            .unwrap()
            .to_owned();
        let store = crate::store::BlobStore::new(directory.path());
        let source = store.publish(b"x=1", None).unwrap();
        let eid = writer
            .call(Mutation::Execution {
                session: sid.clone(),
                cell: Some("a".into()),
                source,
            })
            .unwrap()["exec_id"]
            .as_str()
            .unwrap()
            .to_owned();
        assert!(
            writer
                .call(Mutation::Event {
                    execution: eid,
                    kind: "display".into(),
                    payload: json!({"blob":"ab".repeat(32)})
                })
                .is_err()
        );
        let payload = store.publish(b"payload", None).unwrap();
        let manifest = store
            .publish(
                serde_json::to_vec(
                    &json!({"schema":1,"objects":{"a":{"blob":payload}},"bindings":{}}),
                )
                .unwrap()
                .as_slice(),
                None,
            )
            .unwrap();
        std::fs::write(directory.path().join("blobs").join(payload), b"corrupt").unwrap();
        assert!(
            writer
                .call(Mutation::Checkpoint {
                    session: sid,
                    manifest,
                    status: "complete".into()
                })
                .is_err()
        );
        let key = Uuid::new_v4().to_string();
        let (reply, receive) = mpsc::sync_channel(1);
        drop(receive);
        writer
            .owner
            .sender
            .as_ref()
            .unwrap()
            .send(Envelope {
                command: Mutation::Accept {
                    client: "lost".into(),
                    key: key.clone(),
                    kind: "run".into(),
                    request: json!({"command":"run","source":"x"}),
                },
                reply,
                _reservation: Reservation {
                    bytes: 0,
                    budget: writer.owner.budget.clone(),
                },
            })
            .unwrap();
        let recovered = writer
            .call(Mutation::Accept {
                client: "lost".into(),
                key,
                kind: "run".into(),
                request: json!({"command":"run","source":"x"}),
            })
            .unwrap();
        assert_eq!(recovered["fresh"], false);
        drop(writer);
        let db = Connection::open(directory.path().join("ledger.sqlite")).unwrap();
        for table in ["events", "checkpoints"] {
            assert_eq!(
                db.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r
                    .get::<_, i64>(0))
                    .unwrap(),
                0
            );
        }
        assert_eq!(
            db.query_row("PRAGMA integrity_check", [], |r| r.get::<_, String>(0))
                .unwrap(),
            "ok"
        );
    }
    #[test]
    fn lock_acceptance_takeover_and_terminal_results() {
        let directory = tempfile::tempdir().unwrap();
        let writer = Writer::start(directory.path()).unwrap();
        assert!(Writer::start(directory.path()).is_err());
        let accept = |key: &str, request: Value| {
            writer.call(Mutation::Accept {
                client: "client".into(),
                key: key.into(),
                kind: "run".into(),
                request,
            })
        };
        let key = Uuid::new_v4().to_string();
        let first = accept(&key, json!({"method":"run","source":"x"})).unwrap();
        assert!(first["fresh"].as_bool().unwrap());
        let retry = accept(
            &key,
            json!({"method":"run","source":"x","request_id":"retry"}),
        )
        .unwrap();
        assert_eq!(first["operation"], retry["operation"]);
        assert_eq!(retry["fresh"], false);
        assert!(accept(&key, json!({"method":"run","source":"y"})).is_err());
        let oid = first["operation"]["operation_id"]
            .as_str()
            .unwrap()
            .to_owned();
        writer
            .call(Mutation::OperationState {
                operation: oid.clone(),
                state: "running".into(),
                result: None,
            })
            .unwrap();
        writer.call(Mutation::Takeover).unwrap();
        assert_eq!(
            writer
                .call(Mutation::Operation {
                    operation: oid.clone()
                })
                .unwrap()["state"],
            "unknown"
        );
        assert!(
            writer
                .call(Mutation::OperationState {
                    operation: oid,
                    state: "running".into(),
                    result: None
                })
                .is_err()
        );
        drop(writer);
        let reopened = Writer::start(directory.path()).unwrap();
        drop(reopened);
        let reader = Connection::open(directory.path().join("ledger.sqlite")).unwrap();
        assert_eq!(
            reader
                .query_row("PRAGMA integrity_check", [], |r| r.get::<_, String>(0))
                .unwrap(),
            "ok"
        );
    }
}
