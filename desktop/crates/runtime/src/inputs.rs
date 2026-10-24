//! Connection-owned input claims. All worker delivery is serialized with control.
use super::Shared;
use crate::{
    store::coded,
    writer::{Mutation, Writer},
};
use anyhow::{Context, Result, ensure};
use kernelledger_protocol::write_worker;
use serde_json::{Value, json};
use std::{collections::BTreeMap, io::Write, net::TcpStream};
use uuid::Uuid;

pub(super) struct Pending {
    view: Value,
    claimant: Option<Uuid>,
    worker_id: Value,
}
pub(super) type Requests = BTreeMap<String, Pending>;

pub(super) fn owned(id: &str, connection: Uuid, shared: &Shared) -> bool {
    shared
        .inputs
        .lock()
        .unwrap()
        .get(id)
        .is_some_and(|pending| pending.claimant == Some(connection))
}

pub(super) fn views(shared: &Shared) -> Vec<Value> {
    shared
        .inputs
        .lock()
        .unwrap()
        .values()
        .map(|p| p.view.clone())
        .collect()
}

// Caller holds worker_send before acquiring inputs. No socket or secret is stored
// in a public view, change reference, or operation result.
fn answer(
    pending: &Pending,
    reply: Value,
    shared: &Shared,
    ledger: &Writer,
    socket: Option<&mut TcpStream>,
) -> Result<()> {
    let committed = ledger.call(Mutation::InputAnswer {
        view: pending.view.clone(),
        reply: reply.clone(),
        record_secret: shared.record_secret_input,
    })?;
    if committed["updated"] != true {
        return Ok(());
    }
    if let Some(socket) = socket {
        let mut message = reply;
        message["schema"] = 1.into();
        message["kind"] = "stdin_reply".into();
        message["input_id"] = pending.worker_id.clone();
        send(socket, &message)?;
    }
    Ok(())
}

pub(super) fn send(socket: &mut TcpStream, message: &Value) -> Result<()> {
    let bytes = serde_json::to_vec(message)?;
    ensure!(
        bytes.len() as u64 <= super::ENVELOPE_LIMIT,
        "input message exceeds native structured-message limit"
    );
    if bytes.len() <= kernelledger_protocol::WORKER_INLINE {
        write_worker(socket, message)?;
    } else {
        // Secret input envelopes stay in memory; never publish to BlobStore.
        let digest = kernelledger_protocol::hash(&bytes);
        write_worker(
            &mut *socket,
            &json!({"schema":1,"kind":"blob","sha256":digest,"byte_size":bytes.len()}),
        )?;
        socket.write_all(&bytes)?;
        write_worker(socket, &json!({"schema":1,"kind":"envelope","blob":digest}))?;
    }
    Ok(())
}

pub(super) fn requested(
    execution: &str,
    payload: Value,
    operation: Option<&str>,
    shared: &Shared,
    ledger: &Writer,
    socket: &mut TcpStream,
    input: Option<&(Uuid, std::sync::mpsc::SyncSender<Value>)>,
) -> Result<()> {
    let _send = shared.worker_send.lock().unwrap();
    let mut inputs = shared.inputs.lock().unwrap();
    let worker_id = payload["input_id"].clone();
    let view = ledger.call(Mutation::InputRequested {
        execution: execution.into(),
        payload,
    })?;
    let id = view["input_id"].as_str().context("input id")?.to_owned();
    let pending = Pending {
        view,
        claimant: None,
        worker_id,
    };
    let canceled = operation
        .map(|oid| {
            ledger.call(Mutation::Operation {
                operation: oid.into(),
            })
        })
        .transpose()?
        .is_some_and(|row| row["result"]["cancel_requested"] == true);
    inputs.insert(id.clone(), pending);
    if (operation.is_none() && input.is_none())
        || canceled
        || shared.stopping.load(std::sync::atomic::Ordering::Acquire)
    {
        answer(
            &inputs[&id],
            json!({"error":"input was required but stdin is closed"}),
            shared,
            ledger,
            Some(socket),
        )?;
        inputs.remove(&id);
    } else if let Some((connection, route)) = input {
        let pending = inputs.get_mut(&id).unwrap();
        pending.view["client_name"] = "CLI".into();
        ledger.call(Mutation::InputState {
            view: pending.view.clone(),
        })?;
        pending.claimant = Some(*connection);
        let mut prompt = pending.view.clone();
        prompt["schema"] = 1.into();
        prompt["kind"] = "stdin_request".into();
        prompt["recorded"] = (prompt["password"] != true || shared.record_secret_input).into();
        if route.try_send(prompt).is_err() {
            pending.view["client_name"] = Value::Null;
            ledger.call(Mutation::InputState {
                view: pending.view.clone(),
            })?;
            pending.claimant = None;
        }
    }
    Ok(())
}

pub(super) fn legacy_reply(
    id: &str,
    reply: &Value,
    connection: Uuid,
    shared: &Shared,
    ledger: &Writer,
) -> Result<()> {
    ensure!(reply["kind"] == "stdin_reply", "expected stdin reply");
    ensure!(
        reply.get("input_id").is_none_or(|value| value == id),
        "stdin reply does not match the pending input"
    );
    let reply = if let Some(text) = reply.get("text").and_then(Value::as_str) {
        json!({"text":text})
    } else if let Some(error) = reply.get("error").and_then(Value::as_str) {
        json!({"error":error})
    } else {
        anyhow::bail!("stdin reply must contain text or error");
    };
    let _send = shared.worker_send.lock().unwrap();
    let mut inputs = shared.inputs.lock().unwrap();
    let Some(pending) = inputs.get(id) else {
        return Ok(());
    };
    if pending.claimant != Some(connection) {
        return Ok(());
    }
    let mut socket = shared.socket.lock().unwrap();
    let result = answer(
        pending,
        reply,
        shared,
        ledger,
        Some(socket.as_mut().context("worker unavailable")?),
    );
    if result.is_err() {
        shared
            .killed
            .store(true, std::sync::atomic::Ordering::Release);
    } else {
        inputs.remove(id);
    }
    result
}

pub(super) fn control(
    method: &str,
    request: &Value,
    greeting: &Value,
    connection: Uuid,
    shared: &Shared,
    ledger: &Writer,
) -> Result<Value> {
    let _send = shared.worker_send.lock().unwrap();
    let mut inputs = shared.inputs.lock().unwrap();
    let id = request["input_id"].as_str().context("input id")?;
    let pending = inputs
        .get_mut(id)
        .ok_or_else(|| coded("not_found", "input request no longer pending", json!({})))?;
    if method == "input.claim" {
        if pending.claimant.is_some_and(|owner| owner != connection) {
            return Err(coded(
                "input_claimed",
                "input claimed by another client",
                pending.view.clone(),
            ));
        }
        let mut view = pending.view.clone();
        view["client_name"] = greeting["client_name"].clone();
        ledger.call(Mutation::InputState { view: view.clone() })?;
        pending.view = view;
        pending.claimant = Some(connection);
    } else {
        if pending.claimant != Some(connection) {
            return Err(coded(
                "input_claimed",
                "only the claimant may answer or release input",
                json!({}),
            ));
        }
        if method == "input.release" {
            let mut view = pending.view.clone();
            view["client_name"] = Value::Null;
            ledger.call(Mutation::InputState { view: view.clone() })?;
            pending.view = view;
            pending.claimant = None;
        } else {
            let view = pending.view.clone();
            let reply = if method == "input.cancel" {
                json!({"error":"input canceled"})
            } else {
                json!({"text":request["text"]})
            };
            let mut socket = shared.socket.lock().unwrap();
            let socket = socket.as_mut().context("worker unavailable")?;
            // Failed delivery ends this worker. Retain the entry for cleanup;
            // the writer's final-state guard prevents a second durable answer.
            let result = answer(pending, reply, shared, ledger, Some(socket));
            if result.is_err() {
                shared
                    .killed
                    .store(true, std::sync::atomic::Ordering::Release);
            } else {
                inputs.remove(id);
            }
            result?;
            return Ok(view);
        }
    }
    Ok(pending.view.clone())
}

pub(super) fn disconnected(connection: Uuid, shared: &Shared, ledger: &Writer) -> Result<()> {
    let mut inputs = shared.inputs.lock().unwrap();
    for pending in inputs
        .values_mut()
        .filter(|p| p.claimant == Some(connection))
    {
        let mut view = pending.view.clone();
        view["client_name"] = Value::Null;
        ledger.call(Mutation::InputState { view: view.clone() })?;
        pending.view = view;
        pending.claimant = None;
    }
    Ok(())
}

// Caller holds worker_send. Cancellation may send an interrupt first, then
// deliver an error to wake Queue.get on platforms where SIGINT alone cannot.
pub(super) fn cancel(
    execution: &str,
    shared: &Shared,
    ledger: &Writer,
    mut socket: Option<&mut TcpStream>,
) -> Result<()> {
    let mut inputs = shared.inputs.lock().unwrap();
    let ids: Vec<_> = inputs
        .iter()
        .filter(|(_, p)| p.view["exec_id"] == execution)
        .map(|(id, _)| id.clone())
        .collect();
    for id in ids {
        answer(
            &inputs[&id],
            json!({"error":"input canceled"}),
            shared,
            ledger,
            socket.as_deref_mut(),
        )?;
        inputs.remove(&id);
    }
    Ok(())
}
