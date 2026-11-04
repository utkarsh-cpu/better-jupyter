//! Bounded committed feeds. Socket writes never hold ledger or runtime locks.
use super::{
    CAPABILITIES, Shared, coded, helper, invalid, read_frame, reader, validate,
    write_frame as encode_frame,
};
use anyhow::{Context, Result};
use serde_json::{Value, json};
use std::{
    io::{self, Write},
    net::TcpStream,
    path::Path,
    sync::atomic::Ordering,
    time::{Duration, Instant},
};

const PAGE: u64 = 64;
const ROW_LIMIT: u64 = 10_000;
const BYTE_LIMIT: u64 = 8 * 1024 * 1024;
const STREAM_LIMIT: usize = 8;
struct Feed {
    execution: Option<String>,
    cursor: i64,
}

struct DeadlineWriter<'a> {
    socket: &'a mut TcpStream,
    deadline: Instant,
}
impl Write for DeadlineWriter<'_> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let remaining = self
            .deadline
            .checked_duration_since(Instant::now())
            .filter(|duration| !duration.is_zero())
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::TimedOut,
                    "subscriber write deadline exceeded",
                )
            })?;
        self.socket.set_write_timeout(Some(remaining))?;
        self.socket.write(bytes)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.socket.flush()
    }
}
fn write_frame(socket: &mut TcpStream, value: &Value) -> Result<()> {
    encode_frame(
        DeadlineWriter {
            socket,
            deadline: Instant::now() + Duration::from_secs(5),
        },
        value,
    )
}

fn cursor(request: &Value, key: &str) -> Result<i64> {
    i64::try_from(request.get(key).and_then(Value::as_u64).unwrap_or(0))
        .map_err(|_| invalid(format!("{key} exceeds ledger sequence range")))
}

// Lifecycle changes cannot replace the selected worker during this short read.
// All mutable runtime fields below come from the same SQLite snapshot as the
// watermark; no pending/operation/queue state is sampled after that boundary.
fn snapshot(root: &Path, shared: &Shared) -> Result<Value> {
    let _lifecycle = shared.admission.lock().unwrap();
    let state = shared.status.lock().unwrap().clone();
    let live = shared.socket.lock().unwrap().is_some();
    let clients = shared
        .desktop_clients
        .lock()
        .unwrap()
        .values()
        .filter_map(|hello| hello["client_id"].as_str())
        .map(str::to_owned)
        .collect::<std::collections::HashSet<_>>()
        .len();
    let ledger = reader(root)?;
    ledger.snapshot(|ledger| {
        let watermark = ledger.one("SELECT COALESCE(MAX(change_seq),0) AS seq FROM changes",&[])?["seq"].clone();
        if ledger.count("SELECT exec_id FROM executions WHERE status='queued' LIMIT 1001",&[])?>1000 {return Err(coded("too_large","queue snapshot requires paged resynchronization",json!({})));}
        let session = state["session_id"].as_str().unwrap_or("");
        let active = ledger.rows("SELECT exec_id FROM executions WHERE session_id=? AND status='running' ORDER BY ordinal DESC LIMIT 1",&[session.to_owned().into()])?.first().map(|row|row["exec_id"].clone()).unwrap_or(Value::Null);
        let bounds = ledger.one("SELECT COUNT(*) AS count,COALESCE(SUM(length(CAST(e.payload_json AS BLOB))),0) AS bytes FROM (SELECT * FROM input_requests WHERE state='pending' LIMIT 10001) i JOIN events e ON e.exec_id=i.exec_id AND e.seq=i.seq",&[])?;
        if bounds["count"].as_u64().unwrap_or(0)>ROW_LIMIT || bounds["bytes"].as_u64().unwrap_or(0)>BYTE_LIMIT {return Err(coded("too_large","input snapshot requires paged resynchronization",json!({})));}
        let pending = ledger.rows("SELECT i.*,e.payload_json FROM input_requests i JOIN events e ON e.exec_id=i.exec_id AND e.seq=i.seq WHERE i.state='pending' ORDER BY i.input_id",&[])?.into_iter().map(|row|->Result<Value>{
            let mut view:Value=serde_json::from_str(row["payload_json"].as_str().context("input event")?)?;
            for key in ["input_id","exec_id","seq","client_name"] { view[key]=row[key].clone(); }
            Ok(view)
        }).collect::<Result<Vec<_>>>()?;
        let mut runtime=shared.identity.clone();
        runtime["worker_state"]=if !pending.is_empty() {"waiting_input"} else if !active.is_null() {"running"} else if live {"idle"} else {"absent"}.into();
        runtime["session_id"]=state["session_id"].clone();
        runtime["active_execution"]=active;
        runtime["pending_inputs"]=pending.into();
        runtime["capabilities"]=json!(CAPABILITIES);
        runtime["interpreter"]=state.get("environment").cloned().unwrap_or_else(||json!({}));
        runtime["record_secret_input"]=shared.record_secret_input.into();
        runtime["attached_client_count"]=clients.into();
        runtime["queue"]=ledger.rows("SELECT * FROM executions WHERE status='queued' ORDER BY session_id,ordinal",&[])?.into();
        runtime["active_operations"]=ledger.rows("SELECT * FROM operations WHERE state IN ('accepted','running') ORDER BY accepted",&[])?.into();
        Ok(json!({"kind":"snapshot","change_seq":watermark,"runtime":runtime}))
    })
}

fn resync(feed: &Feed) -> Value {
    if let Some(execution) = &feed.execution {
        json!({"kind":"resync_required","exec_id":execution,"after_seq":feed.cursor})
    } else {
        json!({"kind":"resync_required","after_change_seq":feed.cursor})
    }
}

// Returns serialized frames so all read transactions end before socket I/O.
// Limits are checked in SQL before materializing any payload page.
fn page(root: &Path, feed: &Feed) -> Result<Option<Vec<Value>>> {
    let ledger = reader(root)?;
    ledger.snapshot(|ledger| {
        let (sql,args)=if let Some(execution)=&feed.execution {
            ("SELECT payload_json AS payload FROM events WHERE exec_id=? AND seq>? ORDER BY seq LIMIT 10001",vec![execution.clone().into(),feed.cursor.into()])
        } else { ("SELECT ref_json AS payload FROM changes WHERE change_seq>? ORDER BY change_seq LIMIT 10001",vec![feed.cursor.into()]) };
        let bounds=ledger.one(&format!("SELECT COUNT(*) AS count,COALESCE(SUM(length(CAST(payload AS BLOB))),0) AS bytes,COALESCE(MAX(length(CAST(payload AS BLOB))),0) AS largest FROM ({sql})"),&args)?;
        if bounds["count"].as_u64().unwrap_or(0)>ROW_LIMIT || bounds["bytes"].as_u64().unwrap_or(0)>BYTE_LIMIT || bounds["largest"].as_u64().unwrap_or(0)>kernelledger_protocol::FRAME_LIMIT as u64 {
            return Ok(None);
        }
        let frames=if let Some(execution)=&feed.execution {
            ledger.events(execution,feed.cursor,Some((PAGE,0)))?.into_iter().map(|event|json!({"kind":"event","exec_id":execution,"event":event})).collect()
        } else {
            ledger.rows("SELECT * FROM changes WHERE change_seq>? ORDER BY change_seq LIMIT 64",&[feed.cursor.into()])?.into_iter().map(|row|->Result<Value>{
                let mut frame:Value=row.into();
                frame["change_kind"]=frame["kind"].clone();
                frame["kind"]="change".into();
                frame["ref"]=serde_json::from_str(frame["ref_json"].as_str().context("change reference")?)?;
                Ok(frame)
            }).collect::<Result<Vec<_>>>()?
        };
        Ok(Some(frames))
    })
}

pub(super) fn serve(socket: &mut TcpStream, root: &Path, shared: &Shared) -> Result<()> {
    socket.set_read_timeout(Some(Duration::from_millis(50)))?;
    let mut feeds: Vec<Feed> = Vec::new();
    let mut last = Instant::now();
    let mut ping = Instant::now();
    while !shared.stopping.load(Ordering::Acquire) {
        match socket.peek(&mut [0; 1]) {
            Ok(0) => return Ok(()),
            Ok(_) => {
                socket.set_read_timeout(Some(Duration::from_secs(5)))?;
                let request = read_frame(socket)?;
                socket.set_read_timeout(Some(Duration::from_millis(50)))?;
                last = Instant::now();
                let method = request
                    .get("method")
                    .or_else(|| request.get("kind"))
                    .and_then(Value::as_str)
                    .unwrap_or("");
                if method == "ping" {
                    write_frame(&mut *socket, &json!({"kind":"pong"}))?;
                } else if method != "pong" {
                    let result = (|| -> Result<()> {
                        let method = validate(&request)?;
                        if method != "subscribe" && method != "execution.follow" {
                            return Err(invalid("request requires a control or blob connection"));
                        }
                        if feeds.len() >= STREAM_LIMIT {
                            return Err(coded(
                                "busy",
                                "subscription stream limit reached",
                                json!({"limit":STREAM_LIMIT}),
                            ));
                        }
                        let mut feed = Feed {
                            execution: if method == "execution.follow" {
                                Some(request["exec_id"].as_str().unwrap().into())
                            } else {
                                None
                            },
                            cursor: cursor(
                                &request,
                                if method == "subscribe" {
                                    "after_change_seq"
                                } else {
                                    "after_seq"
                                },
                            )?,
                        };
                        if method == "subscribe" && request.get("after_change_seq").is_none() {
                            let frame = match snapshot(root, shared) {
                                Ok(frame) => frame,
                                Err(error) if helper::error_body(&error)["code"] == "too_large" => {
                                    write_frame(&mut *socket, &resync(&feed))?;
                                    return Ok(());
                                }
                                Err(error) => return Err(error),
                            };
                            feed.cursor =
                                frame["change_seq"].as_i64().context("snapshot watermark")?;
                            if serde_json::to_vec(&frame)?.len()
                                > kernelledger_protocol::FRAME_LIMIT
                            {
                                write_frame(&mut *socket, &resync(&feed))?;
                                return Ok(());
                            }
                            write_frame(&mut *socket, &frame)?;
                        }
                        feeds.push(feed);
                        Ok(())
                    })();
                    if let Err(error) = result {
                        write_frame(
                            &mut *socket,
                            &json!({"kind":"error","request_id":request.get("request_id"),"error":helper::error_body(&error)}),
                        )?;
                    }
                }
            }
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock
                ) => {}
            Err(error) => return Err(error.into()),
        }
        if last.elapsed() >= Duration::from_secs(30) {
            return Ok(());
        }
        if ping.elapsed() >= Duration::from_secs(10) {
            write_frame(&mut *socket, &json!({"kind":"ping"}))?;
            ping = Instant::now();
        }
        let mut index = 0;
        while index < feeds.len() {
            if shared.stopping.load(Ordering::Acquire) {
                return Ok(());
            }
            let feed = &mut feeds[index];
            let Some(frames) = page(root, feed)? else {
                write_frame(&mut *socket, &resync(feed))?;
                feeds.remove(index);
                continue;
            };
            let mut oversized = false;
            let mut sent_bytes = 0usize;
            for frame in frames {
                if shared.stopping.load(Ordering::Acquire) {
                    return Ok(());
                }
                let size = serde_json::to_vec(&frame)?.len();
                if size > kernelledger_protocol::FRAME_LIMIT {
                    oversized = true;
                    break;
                }
                if sent_bytes > 0 && sent_bytes + size > 256 * 1024 {
                    break;
                }
                write_frame(&mut *socket, &frame)?;
                sent_bytes += size;
                feed.cursor = if feed.execution.is_some() {
                    frame["event"]["seq"].as_i64()
                } else {
                    frame["change_seq"].as_i64()
                }
                .context("feed cursor")?;
            }
            if oversized {
                write_frame(&mut *socket, &resync(feed))?;
                feeds.remove(index);
            } else {
                index += 1;
            }
        }
    }
    Ok(())
}
