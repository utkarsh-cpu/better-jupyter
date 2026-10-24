//! Native offline helper for `kernelledger.desktop.helper`.
//!
//! Routing is fixed at process startup. Methods in [`NATIVE`] are served here.
//! Inspection methods never start a worker or write execution history.
//! `source.mutate` publishes tracked source through the native journal and is
//! never forwarded. Methods in [`DELEGATED`] go to one Python helper child,
//! and only when `--python` was given. A side effect keeps a single owner.
use crate::{
    display_path,
    ledger::{Ledger, Row, fold, truthy},
    notebook,
    store::{BlobStore, coded, decode_json, invalid},
};
use anyhow::{Context, Result};
use base64::{
    Engine, alphabet,
    engine::{GeneralPurpose, GeneralPurposeConfig},
};
use kernelledger_protocol::{API, FRAME_LIMIT, ProtocolError};
use serde_json::{Map, Value, json};
use std::{
    fs,
    io::{BufWriter, Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
    process::{Child, ChildStdin, ChildStdout, Command, Stdio},
};

/// Methods implemented in this process.
pub const NATIVE: &[&str] = &[
    "ping",
    "sessions.list",
    "executions.list",
    "executions.watermark",
    "checkpoints.list",
    "execution.get",
    "fold.get",
    "events.list",
    "checkpoint.get",
    "blob.read",
    "descriptor.inspect",
    "training.list",
    "source.read",
    "source.watch",
    "source.recover",
    "source.mutate",
];
/// Methods served only by the Python helper fallback.
pub const DELEGATED: &[&str] = &[
    "native.status",
    "training.inspect",
    "bundle.inspect",
    "export_ipynb",
    "export_document",
    "bundle_export",
    "doctor",
    "interpreter.candidates",
    "interpreter.validate",
];
/// Capability name -> every method it requires.
const CAPABILITIES: &[(&str, &[&str])] = &[
    ("blob_ranges", &["blob.read"]),
    ("portable_training", &["training.list", "training.inspect"]),
    ("native_snapshot_status", &["native.status"]),
    ("document_reports", &["export_document"]),
];

/// Required fields, from `kernelledger.desktop.contracts.HELPER`.
fn required(method: &str) -> Option<&'static [&'static str]> {
    Some(match method {
        "training.inspect" => &["snapshot"],
        "source.mutate" => &["action", "manifest_revision"],
        "execution.get" | "events.list" | "fold.get" => &["exec_id"],
        "checkpoint.get" => &["checkpoint_id"],
        "bundle.inspect"
        | "descriptor.inspect"
        | "export_ipynb"
        | "bundle_export"
        | "interpreter.validate" => &["path"],
        "export_document" => &["path", "format"],
        "blob.read" => &["sha256"],
        m if NATIVE.contains(&m) || DELEGATED.contains(&m) => &[],
        _ => return None,
    })
}

fn is_int(value: &Value) -> bool {
    matches!(value, Value::Number(n) if n.is_i64() || n.is_u64())
}
fn nonnegative(value: &Value) -> bool {
    is_int(value) && value.as_i64().is_none_or(|v| v >= 0)
}

/// Port of `contracts.validate(request, helper=True)`, in the same check order.
pub fn validate(request: &Value) -> Result<&str> {
    let Some(map) = request.as_object() else {
        return Err(invalid("request must be an object"));
    };
    let method = map.get("method").and_then(Value::as_str);
    let Some((method, keys)) = method.and_then(|m| required(m).map(|k| (m, k))) else {
        return Err(invalid("unknown request method"));
    };
    for key in keys {
        if !map.contains_key(*key) {
            return Err(invalid(format!("{key} is required")));
        }
    }
    for key in [
        "exec_id",
        "session_id",
        "checkpoint_id",
        "operation_id",
        "cell_id",
        "input_id",
        "path",
        "name",
        "text",
        "source",
        "manifest_revision",
        "snapshot",
        "device",
        "format",
        "markdown",
    ] {
        if map.get(key).is_some_and(|v| !v.is_string()) {
            return Err(invalid(format!("{key} must be a string")));
        }
    }
    for key in [
        "offset",
        "length",
        "after_seq",
        "after_change_seq",
        "page_size",
    ] {
        if map.get(key).is_some_and(|v| !nonnegative(v)) {
            return Err(invalid(format!("{key} must be a nonnegative integer")));
        }
    }
    if let Some(size) = map.get("page_size")
        && !size.as_u64().is_some_and(|s| (1..=500).contains(&s))
    {
        return Err(invalid("page_size must be between 1 and 500"));
    }
    for key in [
        "trust",
        "allow_python_mismatch",
        "reuse_input",
        "keep_outputs",
        "dry_run",
        "latest_per_cell",
    ] {
        if map.get(key).is_some_and(|v| !v.is_boolean()) {
            return Err(invalid(format!("{key} must be a boolean")));
        }
    }
    if let Some(snapshot) = map.get("snapshot")
        && !snapshot.as_str().is_some_and(crate::store::is_hex64)
    {
        return Err(invalid("snapshot must be a SHA256 digest"));
    }
    Ok(method)
}

/// Python `uuid.UUID(text)` acceptance.
fn python_uuid(text: &str) -> bool {
    let hex = text.replace("urn:", "").replace("uuid:", "");
    let hex = hex.trim_matches(['{', '}']).replace('-', "");
    hex.len() == 32 && hex.bytes().all(|b| b.is_ascii_hexdigit())
}

/// Port of `protocol.hello`.
pub fn hello(message: &Value) -> Result<()> {
    if message["kind"] != "hello" || message["api"] != API {
        return Err(invalid("expected desktop hello"));
    }
    let versions = message.get("versions").and_then(Value::as_array);
    if !versions.is_some_and(|v| v.iter().any(|x| x == &json!(1) || x == &json!(true))) {
        return Err(coded(
            "unsupported_version",
            "supported desktop versions: 1",
            json!({"versions":[1]}),
        ));
    }
    if !matches!(
        message["role"].as_str(),
        Some("control" | "subscription" | "blob")
    ) {
        return Err(invalid("invalid connection role"));
    }
    match message.get("client_id").map(Value::as_str) {
        None => return Err(invalid("'client_id'")),
        Some(Some(id)) if python_uuid(id) => {}
        _ => return Err(invalid("badly formed hexadecimal UUID string")),
    }
    if !message["client_name"]
        .as_str()
        .is_some_and(|n| n.chars().count() <= 256)
    {
        return Err(invalid("invalid client name"));
    }
    Ok(())
}

fn cursor_engine() -> GeneralPurpose {
    GeneralPurpose::new(
        &alphabet::STANDARD,
        GeneralPurposeConfig::new().with_decode_allow_trailing_bits(true),
    )
}

/// Port of `protocol.page_window`; the cursor stays a base64 decimal offset.
fn page_window(request: &Value) -> Result<(u64, u64)> {
    let size = request
        .get("page_size")
        .and_then(Value::as_u64)
        .unwrap_or(100);
    if !(1..=500).contains(&size) {
        return Err(invalid("page_size must be between 1 and 500"));
    }
    let mut offset = 0u64;
    let cursor = request.get("cursor");
    if truthy(cursor) {
        let bad = || invalid("invalid cursor");
        let text = cursor.and_then(Value::as_str).ok_or_else(bad)?;
        let decoded = cursor_engine().decode(text).map_err(|_| bad())?;
        let digits = std::str::from_utf8(&decoded).map_err(|_| bad())?.trim();
        let digits = digits.strip_prefix('+').unwrap_or(digits);
        if digits.starts_with('-') {
            return Err(bad());
        }
        offset = digits.parse::<u64>().map_err(|_| bad())?;
        if offset > i64::MAX as u64 {
            return Err(bad());
        }
    }
    Ok((size, offset))
}

fn cursor(offset: u64, size: u64, total: u64) -> Value {
    if offset + size < total {
        Value::from(cursor_engine().encode((offset + size).to_string()))
    } else {
        Value::Null
    }
}

fn page(items: Vec<Value>, request: &Value) -> Result<Value> {
    let (size, offset) = page_window(request)?;
    let total = items.len() as u64;
    let start = offset.min(total) as usize;
    let end = (offset + size).min(total) as usize;
    Ok(json!({"items": items[start..end], "cursor": cursor(offset, size, total)}))
}

fn sql_page(
    ledger: &Ledger,
    sql: &str,
    args: Vec<rusqlite::types::Value>,
    request: &Value,
) -> Result<(Vec<Row>, Value)> {
    let (size, offset) = page_window(request)?;
    ledger.snapshot(|ledger| {
        let total = ledger.count(sql, &args)?;
        let mut paged = args.clone();
        paged.push((size as i64).into());
        paged.push((offset as i64).into());
        let rows = ledger.rows(&format!("{sql} LIMIT ? OFFSET ?"), &paged)?;
        Ok((rows, cursor(offset, size, total)))
    })
}

fn execution_list_sql(request: &Value) -> (String, Vec<rusqlite::types::Value>) {
    let session = request
        .get("session_id")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .map(str::to_owned);
    if request.get("latest_per_cell") == Some(&Value::Bool(true)) {
        let mut filter = String::from("WHERE cell_id IS NOT NULL");
        let mut args = Vec::new();
        if let Some(session) = session {
            filter.push_str(" AND session_id=?");
            args.push(session.into());
        }
        return (
            format!(
                "SELECT e.*, \
                 (SELECT COUNT(*) FROM events ev WHERE ev.exec_id=e.exec_id) AS fold_events, \
                 (SELECT COALESCE(MAX(seq), 0) FROM events ev WHERE ev.exec_id=e.exec_id) AS fold_seq \
                 FROM executions e JOIN (\
                 SELECT cell_id, MAX(rowid) AS latest_rowid FROM executions {filter} GROUP BY cell_id\
                 ) latest ON e.rowid=latest.latest_rowid ORDER BY e.session_id, e.ordinal"
            ),
            args,
        );
    }
    match session {
        Some(session) => (
            "SELECT * FROM executions WHERE session_id=? ORDER BY session_id,ordinal".into(),
            vec![session.into()],
        ),
        None => (
            "SELECT * FROM executions ORDER BY session_id,ordinal".into(),
            Vec::new(),
        ),
    }
}

/// Python `bytes.decode()` strictness: invalid UTF-8 is a `ValueError`.
fn utf8(data: Vec<u8>) -> Result<String> {
    String::from_utf8(data).map_err(|e| invalid(format!("'utf-8' codec can't decode bytes: {e}")))
}

pub struct Helper {
    pub root: PathBuf,
    blobs: BlobStore,
    delegate: Option<Delegate>,
}

/// One response: a JSON frame plus optional raw bytes (blob ranges only).
pub struct Reply {
    pub frame: Value,
    pub data: Vec<u8>,
}

impl Helper {
    pub fn new(root: &Path, python: Option<PathBuf>) -> Self {
        let root = root
            .canonicalize()
            .unwrap_or_else(|_| std::path::absolute(root).unwrap_or_else(|_| root.to_path_buf()));
        let delegate = python.map(|python| Delegate {
            python,
            root: root.clone(),
            hello: Value::Null,
            child: None,
        });
        Self {
            blobs: BlobStore::new(&root.join(".kl")),
            root,
            delegate,
        }
    }

    pub fn delegated_methods(&self) -> Vec<&'static str> {
        if self.delegate.is_some() {
            DELEGATED.to_vec()
        } else {
            Vec::new()
        }
    }

    pub fn welcome(&mut self, hello: &Value) -> Value {
        if let Some(delegate) = &mut self.delegate {
            delegate.hello = hello.clone();
        }
        let delegated = self.delegated_methods();
        let capabilities: Vec<&str> = CAPABILITIES
            .iter()
            .filter(|(_, methods)| {
                methods
                    .iter()
                    .all(|m| NATIVE.contains(m) || delegated.contains(m))
            })
            .map(|(name, _)| *name)
            .collect();
        json!({
            "kind": "welcome",
            "api": API,
            "version": 1,
            "helper_version": 2,
            "capabilities": capabilities,
            "identity": {
                "kernelledger_version": env!("CARGO_PKG_VERSION"),
                "project_path": display_path(&self.root),
                "implementation": "rust",
            },
            "helper_implementation": "rust",
            "native_methods": NATIVE,
            "delegated_methods": delegated,
        })
    }

    /// Handle one authenticated request.
    pub fn handle(&mut self, request: &Value) -> Result<Reply> {
        let method = validate(request)?.to_owned();
        if !NATIVE.contains(&method.as_str()) {
            let Some(delegate) = &mut self.delegate else {
                return Err(coded(
                    "unsupported_method",
                    format!("{method} requires the Python helper fallback"),
                    json!({"method": method, "implementation": "rust"}),
                ));
            };
            return Ok(Reply {
                frame: delegate.call(request)?,
                data: Vec::new(),
            });
        }
        if method == "blob.read" {
            return self.blob_read(request);
        }
        let result = self.dispatch(&method, request)?;
        Ok(Reply {
            frame: json!({"kind":"response","request_id":request.get("request_id"),"result":result}),
            data: Vec::new(),
        })
    }

    fn blob_read(&self, request: &Value) -> Result<Reply> {
        let digest = request["sha256"]
            .as_str()
            .ok_or_else(|| invalid("invalid blob hash"))?;
        crate::store::check_hash(digest)?;
        let mut file = match self.blobs.open(digest) {
            Ok(file) => file,
            Err(e)
                if e.downcast_ref::<std::io::Error>()
                    .is_some_and(|e| e.kind() == std::io::ErrorKind::NotFound) =>
            {
                return Err(coded(
                    "not_found",
                    "blob missing",
                    json!({"sha256": digest}),
                ));
            }
            Err(e) => return Err(e),
        };
        // Verify the whole payload in bounded buffers before serving any range.
        let verified = self.blobs.verify_file(&mut file, digest)?;
        let offset = request.get("offset").map(Value::as_u64).unwrap_or(Some(0));
        let length = request
            .get("length")
            .map(Value::as_u64)
            .unwrap_or(Some(FRAME_LIMIT as u64));
        let (Some(offset), Some(length)) = (offset, length) else {
            return Err(invalid("invalid blob range"));
        };
        if !(1..=FRAME_LIMIT as u64).contains(&length) {
            return Err(invalid("invalid blob range"));
        }
        let total = verified.size;
        let start = offset.min(total);
        let count = length.min(total - start) as usize;
        let mut data = vec![0; count];
        file.seek(SeekFrom::Start(start))?;
        file.read_exact(&mut data)?;
        verified.check_current(digest)?;
        Ok(Reply {
            frame: json!({"kind":"blob","byte_size":count,"sha256":digest,"offset":offset,
                "total_size":total,"verified": offset == 0 && length >= total}),
            data,
        })
    }

    fn dispatch(&self, method: &str, request: &Value) -> Result<Value> {
        if matches!(
            method,
            "source.read" | "source.watch" | "source.recover" | "source.mutate"
        ) {
            let source = crate::source::SourceService::new(&self.root);
            return match method {
                "source.recover" => source.recover(),
                "source.mutate" => source.mutate(request).map_err(crate::source::python_error),
                _ => source.read(),
            };
        }
        match method {
            "ping" => return Ok(json!({"kind":"pong"})),
            "descriptor.inspect" => {
                let path = request["path"].as_str().unwrap_or_default();
                return decode_json(&fs::read(path)?);
            }
            "training.list" => {
                let directory = self.root.join(".kl/training-artifacts/roots");
                let mut names = Vec::new();
                if directory.exists() {
                    for entry in fs::read_dir(&directory)? {
                        let entry = entry?;
                        if entry.path().is_file() {
                            names.push(entry.file_name().to_string_lossy().into_owned());
                        }
                    }
                }
                names.sort();
                let items = names
                    .into_iter()
                    .map(|name| json!({"snapshot":name,"mode":"portable-training","native_qualified":false}))
                    .collect();
                return page(items, request);
            }
            _ => {}
        }
        let Some((ledger, immutable)) = Ledger::open(&self.root.join(".kl"))? else {
            if method == "executions.watermark" {
                return Ok(
                    json!({"executions":0,"execution_rowid":0,"events":0,"event_rowid":0,"max_end":""}),
                );
            }
            if method.ends_with(".list") {
                return Ok(json!({"items":[],"cursor":null}));
            }
            return Err(coded("not_found", "history is empty", json!({})));
        };
        let text = |key: &str| request[key].as_str().unwrap_or_default().to_owned();
        match method {
            "executions.watermark" => {
                // Executions alone miss event appends and status updates that keep the same rowid.
                let row = ledger.one(
                    "SELECT (SELECT COUNT(*) FROM executions) AS executions, \
                     (SELECT COALESCE(MAX(rowid), 0) FROM executions) AS execution_rowid, \
                     (SELECT COUNT(*) FROM events) AS events, \
                     (SELECT COALESCE(MAX(rowid), 0) FROM events) AS event_rowid, \
                     (SELECT COALESCE(MAX(end), '') FROM executions) AS max_end",
                    &[],
                )?;
                Ok(Value::Object(row))
            }
            "sessions.list" | "executions.list" | "checkpoints.list" => {
                let (sql, args) = match method {
                    "executions.list" => execution_list_sql(request),
                    "sessions.list" => (
                        "SELECT * FROM sessions ORDER BY start,session_id".into(),
                        Vec::new(),
                    ),
                    _ => (
                        "SELECT * FROM checkpoints ORDER BY timestamp,checkpoint_id".into(),
                        Vec::new(),
                    ),
                };
                let (mut rows, cursor) = sql_page(&ledger, &sql, args, request)?;
                if method == "executions.list"
                    && request.get("latest_per_cell") != Some(&Value::Bool(true))
                {
                    for item in &mut rows {
                        // Missing or corrupt sources are omitted, as in Python.
                        if let Some(source) = item
                            .get("source_sha256")
                            .and_then(Value::as_str)
                            .and_then(|d| self.blobs.get(d).ok())
                            .and_then(|data| String::from_utf8(data).ok())
                        {
                            item.insert("source".into(), source.into());
                        }
                    }
                }
                Ok(json!({"items": rows, "cursor": cursor, "read_only_snapshot": immutable}))
            }
            "execution.get" | "fold.get" => self.output_view(&ledger, &text("exec_id")),
            "events.list" => {
                let after = request
                    .get("after_seq")
                    .and_then(Value::as_i64)
                    .unwrap_or(0);
                let (size, offset) = page_window(request)?;
                let exec_id = text("exec_id");
                ledger.snapshot(|ledger| {
                    let total = ledger.count(
                        "SELECT 1 FROM events WHERE exec_id=? AND seq>?",
                        &[exec_id.clone().into(), after.into()],
                    )?;
                    let items = ledger.events(&exec_id, after, Some((size, offset)))?;
                    Ok(json!({"items": items, "cursor": cursor(offset, size, total)}))
                })
            }
            "checkpoint.get" => {
                let checkpoint_id = text("checkpoint_id");
                let row = ledger.one(
                    "SELECT * FROM checkpoints WHERE checkpoint_id=?",
                    &[checkpoint_id.clone().into()],
                )?;
                let digest = row["manifest_sha256"]
                    .as_str()
                    .unwrap_or_default()
                    .to_owned();
                let manifest = decode_json(&self.blobs.get(&digest)?)?;
                let hashes = self.blobs.closure(std::slice::from_ref(&digest))?;
                let field = |key: &str| {
                    manifest
                        .get(key)
                        .cloned()
                        .ok_or_else(|| invalid(format!("'{key}'")))
                };
                Ok(json!({
                    "schema": 1,
                    "checkpoint_id": checkpoint_id,
                    "manifest_sha256": digest,
                    "blob_hashes": hashes,
                    "status": row["status"],
                    "excluded": field("excluded")?,
                    "notes": field("notes")?,
                }))
            }
            _ => Err(invalid("unknown helper request")),
        }
    }

    /// Port of `sharing.output_view(raw=False)`.
    fn output_view(&self, ledger: &Ledger, exec_id: &str) -> Result<Value> {
        let mut row = ledger.one(
            "SELECT * FROM executions WHERE exec_id=?",
            &[exec_id.to_owned().into()],
        )?;
        let current = match row
            .get("cell_id")
            .and_then(Value::as_str)
            .filter(|c| !c.is_empty())
        {
            Some(cell) => notebook::current_source_hash(&self.root, cell).ok(),
            None => None,
        };
        let digest = row["source_sha256"].as_str().unwrap_or_default().to_owned();
        let source = utf8(self.blobs.get(&digest)?)?;
        let events = ledger.events(exec_id, i64::MIN, None)?;
        row.insert("source".into(), source.into());
        row.insert(
            "output_mismatch".into(),
            Value::Bool(current.as_deref() != Some(digest.as_str())),
        );
        row.insert(
            "current_source_sha256".into(),
            current.map_or(Value::Null, Value::from),
        );
        row.insert("outputs".into(), Value::Array(fold(events)));
        Ok(Value::Object(row))
    }
}

/// Lazily started Python helper child for methods not implemented natively.
struct Delegate {
    python: PathBuf,
    root: PathBuf,
    hello: Value,
    child: Option<(Child, ChildStdin, ChildStdout)>,
}
impl Delegate {
    fn start(&mut self) -> Result<()> {
        let mut command = Command::new(&self.python);
        command
            .args(["-I", "-m", "kernelledger.desktop.helper", "--project"])
            .arg(&self.root)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
        }
        let mut child = command.spawn().context("start Python helper fallback")?;
        let mut stdin = child.stdin.take().context("fallback stdin unavailable")?;
        let mut stdout = child.stdout.take().context("fallback stdout unavailable")?;
        kernelledger_protocol::write_frame(&mut stdin, &self.hello)?;
        let welcome = kernelledger_protocol::read_frame(&mut stdout)?;
        if welcome["kind"] != "welcome" || welcome["version"] != 1 {
            let _ = child.kill();
            let _ = child.wait();
            anyhow::bail!("Python helper fallback rejected the session");
        }
        self.child = Some((child, stdin, stdout));
        Ok(())
    }

    fn call(&mut self, request: &Value) -> Result<Value> {
        let attempt = (|| -> Result<Value> {
            if self.child.is_none() {
                self.start()?;
            }
            let (_, stdin, stdout) = self.child.as_mut().context("fallback unavailable")?;
            kernelledger_protocol::write_frame(&mut *stdin, request)?;
            kernelledger_protocol::read_frame(&mut *stdout)
        })();
        attempt.map_err(|error| {
            // Never replay: the outcome of a forwarded request is unknown.
            if let Some((mut child, _, _)) = self.child.take() {
                let _ = child.kill();
                let _ = child.wait();
            }
            coded(
                "fallback_unavailable",
                format!("Python helper fallback failed: {error:#}"),
                json!({"method": request["method"], "outcome": "unknown"}),
            )
        })
    }
}
impl Drop for Delegate {
    fn drop(&mut self) {
        if let Some((mut child, stdin, _)) = self.child.take() {
            drop(stdin);
            if !matches!(child.wait_timeout_ms(2000), Some(true)) {
                let _ = child.kill();
                let _ = child.wait();
            }
        }
    }
}

trait WaitTimeout {
    fn wait_timeout_ms(&mut self, ms: u64) -> Option<bool>;
}
impl WaitTimeout for Child {
    fn wait_timeout_ms(&mut self, ms: u64) -> Option<bool> {
        let deadline = std::time::Instant::now() + std::time::Duration::from_millis(ms);
        loop {
            match self.try_wait() {
                Ok(Some(_)) => return Some(true),
                Ok(None) if std::time::Instant::now() < deadline => {
                    std::thread::sleep(std::time::Duration::from_millis(10))
                }
                _ => return Some(false),
            }
        }
    }
}

/// Port of `protocol.error`.
pub fn error_body(error: &anyhow::Error) -> Value {
    if let Some(e) = error.downcast_ref::<ProtocolError>() {
        return json!({"code":e.code,"message":e.message,"retryable":e.retryable,
            "details": if e.details.is_object() { e.details.clone() } else { json!({}) }});
    }
    let code = match error.downcast_ref::<std::io::Error>() {
        Some(e) if e.kind() == std::io::ErrorKind::NotFound => "not_found",
        _ if error.downcast_ref::<serde_json::Error>().is_some() => "invalid_request",
        _ if error.downcast_ref::<std::string::FromUtf8Error>().is_some() => "invalid_request",
        _ => "internal",
    };
    json!({"code":code,"message":format!("{error:#}"),"retryable":false,"details":{}})
}

fn encode(message: &Value) -> Result<Vec<u8>> {
    let data = serde_json::to_vec(message)?;
    if data.len() > FRAME_LIMIT {
        return Err(coded("too_large", "JSON frame exceeds 1 MiB", json!({})));
    }
    let mut frame = (data.len() as u32).to_be_bytes().to_vec();
    frame.extend(data);
    Ok(frame)
}

/// Read exactly `buffer.len()` bytes; `false` on EOF or a short read.
fn read_full(input: &mut impl Read, buffer: &mut [u8]) -> Result<bool> {
    let mut filled = 0;
    while filled < buffer.len() {
        match input.read(&mut buffer[filled..]) {
            Ok(0) => return Ok(false),
            Ok(n) => filled += n,
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
            Err(e) => return Err(e.into()),
        }
    }
    Ok(true)
}

/// Serve one stdio session with the Python helper's framing and error rules.
pub fn serve(helper: &mut Helper, mut input: impl Read, output: impl Write) -> Result<()> {
    let mut output = BufWriter::new(output);
    let mut authenticated = false;
    loop {
        let mut header = [0u8; 4];
        if !read_full(&mut input, &mut header)? {
            break;
        }
        let size = u32::from_be_bytes(header) as usize;
        let mut request = Value::Object(Map::new());
        let mut fatal = false;
        // `Ok(None)` is a short body read: end the session silently, as Python does.
        let outcome = (|| -> Result<Option<Reply>> {
            if size > FRAME_LIMIT {
                fatal = true;
                return Err(coded("too_large", "JSON frame exceeds 1 MiB", json!({})));
            }
            let mut raw = vec![0; size];
            if !read_full(&mut input, &mut raw)? {
                return Ok(None);
            }
            request = decode_json(&raw)?;
            if !authenticated {
                hello(&request)?;
                authenticated = true;
                return Ok(Some(Reply {
                    frame: helper.welcome(&request),
                    data: Vec::new(),
                }));
            }
            helper.handle(&request).map(Some)
        })();
        let request_id = request.get("request_id").cloned().unwrap_or(Value::Null);
        let failure = |e: &anyhow::Error| {
            encode(&json!({"kind":"error","request_id":request_id,"error":error_body(e)}))
        };
        let bytes = match outcome {
            Ok(None) => break,
            Ok(Some(reply)) => match encode(&reply.frame) {
                Ok(mut frame) => {
                    frame.extend(reply.data);
                    frame
                }
                Err(e) => failure(&e)?,
            },
            Err(e) => failure(&e)?,
        };
        output.write_all(&bytes)?;
        output.flush()?;
        if fatal {
            break;
        }
    }
    Ok(())
}

/// `kernelledger-daemon helper --project PATH [--python PATH]`.
pub fn main(args: &[String]) -> Result<()> {
    let value = |flag: &str| {
        args.iter()
            .position(|a| a == flag)
            .and_then(|i| args.get(i + 1))
            .map(String::as_str)
    };
    let project = value("--project").context("missing --project")?;
    let python = value("--python").map(PathBuf::from);
    let mut helper = Helper::new(Path::new(project), python);
    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    serve(&mut helper, stdin.lock(), stdout.lock())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capabilities_match_the_configured_fallback() {
        let dir = tempfile::tempdir().unwrap();
        let mut native = Helper::new(dir.path(), None);
        let welcome = native.welcome(&Value::Null);
        assert_eq!(welcome["capabilities"], json!(["blob_ranges"]));
        assert!(welcome["delegated_methods"].as_array().unwrap().is_empty());
        assert_eq!(welcome["helper_implementation"], "rust");

        let mut delegated = Helper::new(dir.path(), Some(PathBuf::from("python")));
        let welcome = delegated.welcome(&Value::Null);
        assert_eq!(
            welcome["capabilities"],
            json!([
                "blob_ranges",
                "portable_training",
                "native_snapshot_status",
                "document_reports"
            ])
        );
        assert!(
            welcome["native_methods"]
                .as_array()
                .unwrap()
                .iter()
                .any(|method| method == "source.mutate")
        );
        assert!(
            welcome["delegated_methods"]
                .as_array()
                .unwrap()
                .iter()
                .all(|method| method != "source.mutate")
        );
    }
}
