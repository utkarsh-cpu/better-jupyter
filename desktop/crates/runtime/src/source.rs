//! Revision-checked source transactions. Only this module publishes tracked source.
//!
//! The journal format matches `kernelledger.source`: one file is replaced in
//! place, and every multi-file change is committed under `.kl/source-journal`
//! before it is rolled forward. Python and Rust take the same `source.lock`,
//! so an interrupted transaction recovers from either implementation.
use crate::{
    notebook::{self, canonical, extension, filename, source_hash},
    platform::FileLock,
    store::coded,
};
use anyhow::{Context, Result};
use base64::{
    Engine as _, alphabet,
    engine::{GeneralPurpose, GeneralPurposeConfig},
};

/// Python `base64.b64decode(validate=True)` tolerates non-zero trailing bits.
const PASTE_BASE64: GeneralPurpose = GeneralPurpose::new(
    &alphabet::STANDARD,
    GeneralPurposeConfig::new().with_decode_allow_trailing_bits(true),
);
use serde_json::{Map, Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    fs,
    io::Write,
    path::{Path, PathBuf},
    thread,
    time::{Duration, Instant},
};

type Files = Vec<(String, Vec<u8>)>;

pub struct SourceService {
    root: PathBuf,
}

impl SourceService {
    pub fn new(root: &Path) -> Self {
        let root = root
            .canonicalize()
            .unwrap_or_else(|_| std::path::absolute(root).unwrap_or_else(|_| root.to_path_buf()));
        Self { root }
    }

    fn state(&self) -> PathBuf {
        self.root.join(".kl")
    }

    /// `source.read` / `source.watch`. A missing lock does not create `.kl`.
    pub fn read(&self) -> Result<Value> {
        let lock = self.state().join("source.lock");
        if lock.exists() && writable(&self.state()) {
            let _guard = self.lock()?;
            return self.read_unlocked();
        }
        self.read_unlocked()
    }

    pub fn recover(&self) -> Result<Value> {
        let _guard = self.lock()?;
        self.read_unlocked()
    }
    /// Freeze requested executable cells under the same lock used by mutations.
    pub fn execution_sources(&self, ids: &Value, expected: &Value) -> Result<Vec<Value>> {
        let _guard = self.lock()?;
        let current = self.read_unlocked()?;
        if current.get("manifest").is_none() {
            return Err(fail("source_invalid", "invalid manifest", current));
        }
        let cells = current["cells"].as_array().context("source cells")?;
        let selected = if ids == "all" {
            cells
                .iter()
                .filter(|cell| cell["type"] == "code")
                .map(|cell| cell["id"].clone())
                .collect::<Vec<_>>()
        } else {
            ids.as_array().context("cell ids must be an array")?.clone()
        };
        let mut result = Vec::new();
        for id in selected {
            let cell = cells
                .iter()
                .find(|cell| cell["id"] == id && cell["type"] == "code")
                .ok_or_else(|| {
                    fail(
                        "source_invalid",
                        "missing or non-code cell",
                        json!({"cell_id":id}),
                    )
                })?;
            if markers(cell["source"].as_str().context("source text")?) {
                return Err(fail(
                    "source_invalid",
                    "cell has conflict markers",
                    json!({"cell_id":id}),
                ));
            }
            if let Some(revision) = id.as_str().and_then(|id| expected.get(id))
                && revision != &cell["revision"]
            {
                return Err(fail("source_conflict", "cell changed", current.clone()));
            }
            result.push(cell.clone());
        }
        Ok(result)
    }

    pub fn mutate(&self, request: &Value) -> Result<Value> {
        let _guard = self.lock()?;
        let current = self.check(request)?;
        let before = self.files()?;
        let mut after = before.clone();
        let mut data = current["manifest"].clone();
        let mut cells = data["cells"].as_array().cloned().unwrap_or_default();
        let action = request["action"].as_str().unwrap_or("");
        let cid = request
            .get("cell_id")
            .and_then(Value::as_str)
            .filter(|id| !id.is_empty());
        let revisions = request
            .get("cell_revisions")
            .cloned()
            .unwrap_or_else(|| json!({}));

        let cell_index = |cells: &[Value], id: &str| {
            cells
                .iter()
                .position(|cell| cell["id"].as_str() == Some(id))
        };
        let require = |cells: &[Value]| -> Result<usize> {
            let Some(id) = cid else {
                return Err(fail("not_found", "cell not found", json!({})));
            };
            let Some(index) = cell_index(cells, id) else {
                return Err(fail("not_found", "cell not found", json!({})));
            };
            let id = cells[index]["id"].as_str().unwrap_or("");
            if !revisions
                .as_object()
                .is_some_and(|map| map.contains_key(id))
            {
                return Err(fail(
                    "invalid_request",
                    "expected cell revision required",
                    json!({}),
                ));
            }
            Ok(index)
        };
        let name_of = |cell: &Value| -> Result<String> {
            let id = cell["id"].as_str().unwrap_or("");
            let kind = cell["type"].as_str().unwrap_or("");
            let ext = extension(kind).context("invalid cell type")?;
            Ok(format!("cells/{id}{ext}"))
        };

        match action {
            "edit" => {
                let index = require(&cells)?;
                let source = request["source"].as_str().context("source")?;
                let name = name_of(&cells[index])?;
                files_insert(&mut after, name, canonical(source).into_bytes());
            }
            "insert_cells" => {
                let Some(proposed) = request.get("cells").and_then(Value::as_array) else {
                    return Err(fail(
                        "invalid_request",
                        "invalid inserted cell set",
                        json!({}),
                    ));
                };
                let Some(index) = request.get("index").and_then(Value::as_i64) else {
                    return Err(fail(
                        "invalid_request",
                        "invalid inserted cell set",
                        json!({}),
                    ));
                };
                if !(1..=100).contains(&proposed.len())
                    || !(0..=cells.len() as i64).contains(&index)
                {
                    return Err(fail(
                        "invalid_request",
                        "invalid inserted cell set",
                        json!({}),
                    ));
                }
                for (offset, entry) in proposed.iter().enumerate() {
                    let Some(entry) = entry.as_object() else {
                        return Err(fail("invalid_request", "invalid inserted cell", json!({})));
                    };
                    if extension(entry.get("type").and_then(Value::as_str).unwrap_or("")).is_none()
                        || !entry.get("source").is_some_and(Value::is_string)
                    {
                        return Err(fail("invalid_request", "invalid inserted cell", json!({})));
                    }
                    let mut new = Map::new();
                    new.insert("id".into(), short_id().into());
                    new.insert("type".into(), entry["type"].clone());
                    let cell = Value::Object(new);
                    let name = name_of(&cell)?;
                    let source = entry["source"].as_str().unwrap_or("");
                    files_insert(&mut after, name, canonical(source).into_bytes());
                    cells.insert(index as usize + offset, cell);
                }
            }
            "add" | "duplicate" | "split" => {
                if action != "add" {
                    require(&cells)?;
                }
                let existing = cid.and_then(|id| cell_index(&cells, id));
                let mut new = match existing {
                    Some(index) => cells[index].clone(),
                    None => {
                        json!({"type": request.get("type").and_then(Value::as_str).unwrap_or("code")})
                    }
                };
                let new_id = request
                    .get("new_id")
                    .and_then(Value::as_str)
                    .map(str::to_owned)
                    .unwrap_or_else(short_id);
                new["id"] = new_id.into();
                let mut index = request
                    .get("index")
                    .and_then(Value::as_i64)
                    .unwrap_or(cells.len() as i64);
                let mut source = request
                    .get("source")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_owned();
                if matches!(action, "duplicate" | "split") {
                    let index_of = existing.context("cell")?;
                    let old_name = name_of(&cells[index_of])?;
                    source = String::from_utf8(
                        files_get(&before, &old_name).context("cell file")?.to_vec(),
                    )?;
                    index = index_of as i64 + 1;
                    if action == "split" {
                        let offset = request["offset"].as_i64().context("offset")?;
                        let count = source.chars().count() as i64;
                        if !(0..=count).contains(&offset) {
                            return Err(fail("invalid_request", "invalid split offset", json!({})));
                        }
                        let byte = byte_offset(&source, offset as usize);
                        files_insert(&mut after, old_name, source.as_bytes()[..byte].to_vec());
                        source = source[byte..].to_owned();
                    }
                    let old_id = cid.context("cell id")?;
                    for attachment in attachment_names(&cells[existing.context("cell")?])? {
                        let old = format!("attachments/{old_id}/{attachment}");
                        let copied = files_get(&before, &old).context("attachment")?.to_vec();
                        files_insert(
                            &mut after,
                            format!(
                                "attachments/{}/{attachment}",
                                new["id"].as_str().unwrap_or("")
                            ),
                            copied,
                        );
                    }
                }
                let name = name_of(&new)?;
                files_insert(&mut after, name, canonical(&source).into_bytes());
                insert_at(&mut cells, index, new);
            }
            "delete" => {
                let index = require(&cells)?;
                let id = cells[index]["id"].as_str().unwrap_or("").to_owned();
                let name = name_of(&cells[index])?;
                cells.remove(index);
                files_pop(&mut after, &name);
                let prefix = format!("attachments/{id}/");
                after.retain(|(path, _)| !path.starts_with(&prefix));
            }
            "move" => {
                let index = require(&cells)?;
                let cell = cells.remove(index);
                let target = request["index"].as_i64().context("index")?;
                insert_at(&mut cells, target, cell);
            }
            "convert" => {
                let index = require(&cells)?;
                let old = name_of(&cells[index])?;
                let bytes = files_pop(&mut after, &old).context("cell file")?;
                cells[index]["type"] = request["type"].clone();
                let new = name_of(&cells[index])?;
                files_insert(&mut after, new, bytes);
            }
            "merge" => {
                let index = require(&cells)?;
                let other_id = request["other_id"].as_str().context("other_id")?;
                require_id(&cells, other_id, &revisions)?;
                let other_index = cell_index(&cells, other_id).context("other")?;
                if cells[index]["type"] != cells[other_index]["type"] {
                    return Err(fail(
                        "invalid_request",
                        "merge requires matching cell types",
                        json!({}),
                    ));
                }
                if cells[other_index]
                    .get("attachment_alternatives")
                    .is_some_and(|v| !v.as_object().is_some_and(|m| m.is_empty()))
                {
                    return Err(fail(
                        "invalid_request",
                        "merge MIME alternatives separately before merging",
                        json!({}),
                    ));
                }
                let other_name = name_of(&cells[other_index])?;
                let mut other_source =
                    String::from_utf8(files_pop(&mut after, &other_name).context("cell file")?)?;
                let target_id = cells[index]["id"].as_str().unwrap_or("").to_owned();
                let existing: Vec<String> = attachment_names(&cells[index])?;
                for attachment in attachment_names(&cells[other_index])? {
                    let mut replacement = attachment.clone();
                    if existing
                        .iter()
                        .any(|name| name.to_lowercase() == replacement.to_lowercase())
                    {
                        replacement = format!("merged-{}{}", short_id(), suffix(&attachment));
                        other_source = other_source.replace(
                            &format!("attachment:{attachment}"),
                            &format!("attachment:{replacement}"),
                        );
                    }
                    let bytes =
                        files_pop(&mut after, &format!("attachments/{other_id}/{attachment}"))
                            .context("attachment")?;
                    files_insert(
                        &mut after,
                        format!("attachments/{target_id}/{replacement}"),
                        bytes,
                    );
                    let list = cells[index]
                        .as_object_mut()
                        .context("cell")?
                        .entry("attachments")
                        .or_insert_with(|| json!([]));
                    list.as_array_mut()
                        .context("attachments")?
                        .push(replacement.clone().into());
                    if let Some(mime) = cells[other_index]
                        .get("attachment_mimes")
                        .and_then(|m| m.get(&attachment))
                        .cloned()
                    {
                        let mimes = cells[index]
                            .as_object_mut()
                            .context("cell")?
                            .entry("attachment_mimes")
                            .or_insert_with(|| json!({}));
                        mimes
                            .as_object_mut()
                            .context("mimes")?
                            .insert(replacement, mime);
                    }
                }
                let target_name = name_of(&cells[index])?;
                let mut combined = files_get(&after, &target_name)
                    .context("cell file")?
                    .to_vec();
                combined.push(b'\n');
                combined.extend(other_source.into_bytes());
                files_insert(&mut after, target_name, combined);
                cells.remove(other_index);
            }
            "attach" => {
                let index = require(&cells)?;
                let file_path = request["file_path"].as_str().context("file_path")?;
                let fallback = Path::new(file_path)
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("");
                let attachment = request
                    .get("filename")
                    .and_then(Value::as_str)
                    .unwrap_or(fallback);
                filename(&Value::String(attachment.to_owned()))?;
                let names = attachment_names(&cells[index])?;
                if names
                    .iter()
                    .any(|name| name.to_lowercase() == attachment.to_lowercase())
                {
                    return Err(fail(
                        "invalid_request",
                        "attachment already exists",
                        json!({}),
                    ));
                }
                let id = cells[index]["id"].as_str().unwrap_or("");
                files_insert(
                    &mut after,
                    format!("attachments/{id}/{attachment}"),
                    fs::read(file_path)?,
                );
                let cell = cells[index].as_object_mut().context("cell")?;
                cell.entry("attachments")
                    .or_insert_with(|| json!([]))
                    .as_array_mut()
                    .context("attachments")?
                    .push(attachment.into());
                let media = request
                    .get("media_type")
                    .and_then(Value::as_str)
                    .unwrap_or("application/octet-stream");
                cell.entry("attachment_mimes")
                    .or_insert_with(|| json!({}))
                    .as_object_mut()
                    .context("mimes")?
                    .insert(attachment.to_owned(), media.into());
            }
            "paste" => {
                let index = request
                    .get("index")
                    .and_then(Value::as_i64)
                    .unwrap_or(cells.len() as i64);
                let items = request["cells"].as_array().context("cells")?;
                for (index, item) in (index..).zip(items.iter()) {
                    let mut new = item["cell"].clone();
                    new["id"] = short_id().into();
                    let name = name_of(&new)?;
                    let source = item["source"].as_str().context("source")?;
                    files_insert(&mut after, name, canonical(source).into_bytes());
                    let id = new["id"].as_str().unwrap_or("").to_owned();
                    for attachment in attachment_names(&new)? {
                        let encoded = item["attachments"][&attachment]
                            .as_str()
                            .context("attachment")?;
                        let bytes = PASTE_BASE64.decode(encoded).map_err(|_| {
                            fail("invalid_request", "invalid attachment", json!({}))
                        })?;
                        files_insert(&mut after, format!("attachments/{id}/{attachment}"), bytes);
                    }
                    insert_at(&mut cells, index, new);
                }
            }
            "metadata" => {
                let metadata = request.get("metadata").cloned().context("metadata")?;
                if let Some(id) = cid {
                    let index = require_id(&cells, id, &revisions)?;
                    cells[index]["metadata"] = metadata;
                } else {
                    data["metadata"] = metadata;
                }
            }
            "retarget" => {
                data["id"] = uuid::Uuid::new_v4().to_string().into();
            }
            _ => return Err(fail("invalid_request", "unknown mutation", json!({}))),
        }

        data["cells"] = Value::Array(cells);
        files_insert(&mut after, "notebook.json".into(), manifest_bytes(&data)?);
        self.stage(&data)?;
        self.publish(&before, &after)?;
        self.read_unlocked()
    }

    fn lock(&self) -> Result<FileLock> {
        fs::create_dir_all(self.state())?;
        let path = self.state().join("source.lock");
        let deadline = Instant::now() + Duration::from_secs(30);
        let guard = loop {
            match FileLock::acquire(&path) {
                Ok(guard) => break guard,
                Err(error) if Instant::now() < deadline && lock_contended(&error) => {
                    thread::sleep(Duration::from_millis(10));
                }
                Err(error) => return Err(error),
            }
        };
        self.recover_journals()?;
        Ok(guard)
    }

    fn check(&self, request: &Value) -> Result<Value> {
        let current = self.read_unlocked()?;
        if current["state"] != "ok" {
            return Err(fail("source_invalid", "source is not valid", current));
        }
        let revisions = current["cells"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|cell| {
                Some((
                    cell["id"].as_str()?.to_owned(),
                    cell["revision"].as_str()?.to_owned(),
                ))
            })
            .collect::<HashMap<String, String>>();
        let expected_manifest = request.get("manifest_revision");
        let manifest_ok = expected_manifest == Some(&current["manifest_revision"]);
        let cells_ok = match request.get("cell_revisions") {
            None => true,
            Some(Value::Object(map)) => map
                .iter()
                .all(|(id, revision)| revisions.get(id).map(String::as_str) == revision.as_str()),
            Some(_) => false,
        };
        if !manifest_ok || !cells_ok {
            return Err(fail("source_conflict", "source changed on disk", current));
        }
        Ok(current)
    }

    fn publish(&self, before: &Files, after: &Files) -> Result<()> {
        let writes: Vec<(String, Vec<u8>)> = after
            .iter()
            .filter(|(name, data)| files_get(before, name) != Some(data.as_slice()))
            .cloned()
            .collect();
        let mut removes: Vec<String> = before
            .iter()
            .map(|(name, _)| name.clone())
            .filter(|name| files_get(after, name).is_none())
            .collect();
        removes.sort();
        if writes.is_empty() && removes.is_empty() {
            return Ok(());
        }
        if writes.len() == 1 && removes.is_empty() {
            let (name, data) = &writes[0];
            return atomic(&self.checked(name)?, data);
        }
        let tx = self
            .state()
            .join("source-journal")
            .join(uuid::Uuid::new_v4().simple().to_string());
        fs::create_dir_all(&tx)?;
        for (name, data) in &writes {
            atomic(&tx.join("new").join(name), data)?;
        }
        for name in &removes {
            atomic(
                &tx.join("old").join(name),
                files_get(before, name).context("removed file")?,
            )?;
        }
        let mut prior = Map::new();
        for (name, data) in before {
            prior.insert(name.clone(), format!("{:x}", Sha256::digest(data)).into());
        }
        let intent = json!({
            "writes": writes.iter().map(|(name, _)| name.clone()).collect::<Vec<_>>(),
            "removes": removes,
            "prior_revisions": prior,
        });
        atomic(
            &tx.join("intent.json"),
            python_dump(&intent, false, false).as_bytes(),
        )?;
        crash_at("prepared")?;
        atomic(&tx.join("commit"), b"commit\n")?;
        crash_at("committed")?;
        self.roll_forward(&tx)
    }

    fn roll_forward(&self, tx: &Path) -> Result<()> {
        let intent: Value = serde_json::from_slice(&fs::read(tx.join("intent.json"))?)?;
        let mut writes = intent["writes"].as_array().cloned().unwrap_or_default();
        writes.sort_by(|a, b| {
            let a = a.as_str().unwrap_or("");
            let b = b.as_str().unwrap_or("");
            (a == "notebook.json")
                .cmp(&(b == "notebook.json"))
                .then(a.cmp(b))
        });
        for name in writes {
            let name = name.as_str().context("journal write")?;
            atomic(&self.checked(name)?, &fs::read(tx.join("new").join(name))?)?;
            crash_at(&format!("replaced:{name}"))?;
        }
        for name in intent["removes"].as_array().into_iter().flatten() {
            let name = name.as_str().context("journal remove")?;
            let path = self.checked(name)?;
            if path.exists() {
                fs::remove_file(path)?;
            }
        }
        fs::remove_dir_all(tx)?;
        Ok(())
    }

    fn recover_journals(&self) -> Result<()> {
        let directory = self.state().join("source-journal");
        if !directory.exists() {
            return Ok(());
        }
        let mut entries = fs::read_dir(&directory)?.collect::<std::io::Result<Vec<_>>>()?;
        entries.sort_by_key(|entry| entry.file_name());
        for entry in entries {
            let tx = entry.path();
            if tx.symlink_metadata()?.file_type().is_symlink() {
                return Err(fail("source_invalid", "invalid journal symlink", json!({})));
            }
            if tx.join("commit").is_file() {
                self.roll_forward(&tx)?;
            } else if tx.exists() {
                fs::remove_dir_all(tx)?;
            }
        }
        Ok(())
    }

    fn stage(&self, data: &Value) -> Result<()> {
        let directory = tempfile::tempdir()?;
        fs::write(
            directory.path().join("notebook.json"),
            manifest_bytes(data)?,
        )?;
        notebook::read(directory.path())
            .map_err(|error| fail("invalid_request", format!("{error:#}"), json!({})))?;
        Ok(())
    }

    fn read_unlocked(&self) -> Result<Value> {
        let path = self.root.join("notebook.json");
        let raw = match fs::read_to_string(&path) {
            Ok(raw) => raw,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(json!({"state": "missing", "message": error.to_string()}));
            }
            Err(error) => return Err(error.into()),
        };
        if markers(&raw) {
            return Ok(json!({"state": "conflict_markers", "files": ["notebook.json"]}));
        }
        let parsed: Value = match serde_json::from_str(&raw) {
            Ok(value) => value,
            Err(error) => return Ok(json!({"state": "malformed", "message": error.to_string()})),
        };
        if let Some(schema) = parsed.get("schema").and_then(Value::as_i64)
            && schema > 1
        {
            return Ok(json!({"state": "unsupported_schema", "supported": 1, "actual": schema}));
        }
        let data = match notebook::read(&self.root) {
            Ok(data) => data,
            Err(error) => {
                return Ok(json!({"state": "malformed", "message": format!("{error:#}")}));
            }
        };
        let mut cells = Vec::new();
        let mut marked = Vec::new();
        let mut missing = Vec::new();
        for cell in data["cells"].as_array().into_iter().flatten() {
            let id = cell["id"].as_str().unwrap_or("");
            let kind = cell["type"].as_str().unwrap_or("");
            let ext = extension(kind).unwrap_or("");
            let file = self.root.join("cells").join(format!("{id}{ext}"));
            let bytes = match fs::read(&file) {
                Ok(bytes) => bytes,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                    missing.push(rel_native(&self.root, &file));
                    continue;
                }
                Err(error) => return Err(error.into()),
            };
            let text = match String::from_utf8(bytes) {
                Ok(text) => canonical(&text),
                Err(error) => {
                    return Ok(json!({"state": "malformed", "message": error.to_string()}));
                }
            };
            if markers(&text) {
                marked.push(rel_native(&self.root, &file));
            }
            let mut item = cell.as_object().cloned().unwrap_or_default();
            item.insert("source".into(), text.clone().into());
            item.insert("revision".into(), source_hash(&text).into());
            cells.push(Value::Object(item));
        }
        let state = if !missing.is_empty() {
            "missing"
        } else if !marked.is_empty() {
            "conflict_markers"
        } else {
            "ok"
        };
        let files = if state == "missing" {
            missing
        } else if state == "conflict_markers" {
            marked
        } else {
            Vec::new()
        };
        Ok(json!({
            "state": state,
            "manifest": data,
            "manifest_revision": format!("{:x}", Sha256::digest(manifest_bytes(&data)?)),
            "cells": cells,
            "files": files,
        }))
    }

    fn files(&self) -> Result<Files> {
        let mut result = Files::new();
        let notebook = self.root.join("notebook.json");
        if notebook.is_file() {
            result.push(("notebook.json".into(), fs::read(notebook)?));
        }
        for directory in ["cells", "attachments"] {
            let root = self.root.join(directory);
            if root.exists() {
                collect_files(&self.root, &root, &mut result)?;
            }
        }
        Ok(result)
    }

    fn checked(&self, name: &str) -> Result<PathBuf> {
        if name != "notebook.json"
            && !name.starts_with("cells/")
            && !name.starts_with("attachments/")
        {
            return Err(fail("invalid_request", "invalid source path", json!({})));
        }
        if name
            .split(['/', '\\'])
            .any(|part| part == ".." || part.is_empty())
            && name != "notebook.json"
        {
            // Empty parts come from a trailing slash. `..` is an escape.
            if name.split('/').any(|part| part == "..") {
                return Err(fail(
                    "source_invalid",
                    "source path escapes notebook",
                    json!({"path": name}),
                ));
            }
        }
        let path = self.root.join(name);
        if path
            .symlink_metadata()
            .is_ok_and(|meta| meta.file_type().is_symlink())
        {
            return Err(fail(
                "source_invalid",
                "source path escapes notebook",
                json!({"path": name}),
            ));
        }
        let resolved = resolve_lexical(&path)?;
        let root = self
            .root
            .canonicalize()
            .unwrap_or_else(|_| self.root.clone());
        if !inside(&root, &resolved) {
            return Err(fail(
                "source_invalid",
                "source path escapes notebook",
                json!({"path": name}),
            ));
        }
        Ok(path)
    }
}

/// `manifest_bytes`: header keeps object order; each cell is compact and sorted.
pub fn manifest_bytes(data: &Value) -> Result<Vec<u8>> {
    let mut header = data.as_object().context("manifest")?.clone();
    let cells = header.remove("cells").unwrap_or(json!([]));
    let pretty = python_dump(&Value::Object(header), true, false);
    let stem = pretty
        .strip_suffix("\n}")
        .or_else(|| pretty.strip_suffix('}'))
        .unwrap_or(&pretty);
    let lines = cells
        .as_array()
        .context("cells")?
        .iter()
        .map(|cell| format!("    {}", python_dump(cell, false, true)))
        .collect::<Vec<_>>()
        .join(",\n");
    Ok(format!("{stem},\n  \"cells\": [\n{lines}\n  ]\n}}\n").into_bytes())
}

fn python_dump(value: &Value, pretty: bool, sort: bool) -> String {
    fn dump(value: &Value, indent: Option<usize>, level: usize, sort: bool) -> String {
        match value {
            Value::Null => "null".into(),
            Value::Bool(true) => "true".into(),
            Value::Bool(false) => "false".into(),
            Value::Number(number) => python_number(number),
            Value::String(text) => quote(text),
            Value::Array(items) if items.is_empty() => "[]".into(),
            Value::Array(items) => match indent {
                None => format!(
                    "[{}]",
                    items
                        .iter()
                        .map(|item| dump(item, None, 0, sort))
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
                Some(width) => {
                    let pad = " ".repeat(width * (level + 1));
                    let end = " ".repeat(width * level);
                    let body = items
                        .iter()
                        .map(|item| format!("{pad}{}", dump(item, indent, level + 1, sort)))
                        .collect::<Vec<_>>()
                        .join(",\n");
                    format!("[\n{body}\n{end}]")
                }
            },
            Value::Object(map) if map.is_empty() => "{}".into(),
            Value::Object(map) => {
                let mut keys: Vec<&String> = map.keys().collect();
                if sort {
                    keys.sort();
                }
                match indent {
                    None => format!(
                        "{{{}}}",
                        keys.iter()
                            .map(|key| format!(
                                "{}: {}",
                                quote(key),
                                dump(&map[*key], None, 0, sort)
                            ))
                            .collect::<Vec<_>>()
                            .join(", ")
                    ),
                    Some(width) => {
                        let pad = " ".repeat(width * (level + 1));
                        let end = " ".repeat(width * level);
                        let body = keys
                            .iter()
                            .map(|key| {
                                format!(
                                    "{pad}{}: {}",
                                    quote(key),
                                    dump(&map[*key], indent, level + 1, sort)
                                )
                            })
                            .collect::<Vec<_>>()
                            .join(",\n");
                        format!("{{\n{body}\n{end}}}")
                    }
                }
            }
        }
    }
    dump(value, pretty.then_some(2), 0, sort)
}

fn quote(text: &str) -> String {
    let mut out = String::from("\"");
    for ch in text.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\u{0008}' => out.push_str("\\b"),
            '\u{000c}' => out.push_str("\\f"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                out.push_str(&format!("\\u{:04x}", c as u32));
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn atomic(path: &Path, data: &[u8]) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let parent = path.parent().context("atomic parent")?;
    let mut file = tempfile::Builder::new()
        .prefix(".kl-")
        .tempfile_in(parent)?;
    file.write_all(data)?;
    file.as_file().sync_all()?;
    file.persist(path).map_err(|error| error.error)?;
    #[cfg(unix)]
    {
        let directory = fs::File::open(parent)?;
        directory.sync_all()?;
    }
    Ok(())
}

/// Python `repr(float)` / `str(int)` as written by `json.dumps`.
fn python_number(number: &serde_json::Number) -> String {
    if number.is_i64() || number.is_u64() {
        return number.to_string();
    }
    let Some(value) = number.as_f64() else {
        return number.to_string();
    };
    if value == 0.0 {
        return if value.is_sign_negative() {
            "-0.0".into()
        } else {
            "0.0".into()
        };
    }
    // Rust `{:e}` gives the shortest round-trip digits, as Python's repr does.
    let scientific = format!("{value:e}");
    let (mantissa, exponent) = scientific.split_once('e').unwrap_or((&scientific, "0"));
    let exponent: i32 = exponent.parse().unwrap_or(0);
    let negative = mantissa.starts_with('-');
    let digits: String = mantissa.chars().filter(char::is_ascii_digit).collect();
    let sign = if negative { "-" } else { "" };
    if (-4..16).contains(&exponent) {
        let point = exponent + 1;
        let text = if point <= 0 {
            format!("0.{}{digits}", "0".repeat((-point) as usize))
        } else if point as usize >= digits.len() {
            format!("{digits}{}.0", "0".repeat(point as usize - digits.len()))
        } else {
            format!(
                "{}.{}",
                &digits[..point as usize],
                &digits[point as usize..]
            )
        };
        return format!("{sign}{text}");
    }
    let (head, tail) = digits.split_at(1);
    let body = if tail.is_empty() {
        head.to_owned()
    } else {
        format!("{head}.{tail}")
    };
    let exp_sign = if exponent < 0 { '-' } else { '+' };
    format!("{sign}{body}e{exp_sign}{:02}", exponent.abs())
}

/// Python's helper reports `KeyError`/`TypeError`/`ValueError` from a malformed
/// mutation as `invalid_request`. Keep coded and I/O errors as they are.
pub fn python_error(error: anyhow::Error) -> anyhow::Error {
    let coded = error.chain().any(|cause| {
        cause.is::<kernelledger_protocol::ProtocolError>() || cause.is::<std::io::Error>()
    });
    if coded {
        error
    } else {
        fail("invalid_request", format!("{error:#}"), json!({}))
    }
}

fn crash_at(point: &str) -> Result<()> {
    if std::env::var("KERNELLEDGER_SOURCE_CRASH").ok().as_deref() == Some(point) {
        return Err(fail(
            "internal",
            format!("source crash hook: {point}"),
            json!({}),
        ));
    }
    Ok(())
}

fn fail(code: &str, message: impl Into<String>, details: Value) -> anyhow::Error {
    coded(code, message, details)
}

fn short_id() -> String {
    uuid::Uuid::new_v4().simple().to_string()[..12].to_owned()
}

fn suffix(name: &str) -> String {
    match name.rfind('.') {
        Some(index) if index > 0 && !(name.starts_with('.') && !name[1..].contains('.')) => {
            name[index..].to_owned()
        }
        _ => String::new(),
    }
}

fn byte_offset(text: &str, chars: usize) -> usize {
    text.chars().take(chars).map(|ch| ch.len_utf8()).sum()
}

fn insert_at(cells: &mut Vec<Value>, index: i64, cell: Value) {
    let len = cells.len() as i64;
    let index = if index < 0 {
        (len + index).max(0)
    } else {
        index.min(len)
    } as usize;
    cells.insert(index, cell);
}

fn require_id(cells: &[Value], id: &str, revisions: &Value) -> Result<usize> {
    let Some(index) = cells
        .iter()
        .position(|cell| cell["id"].as_str() == Some(id))
    else {
        return Err(fail("not_found", "cell not found", json!({})));
    };
    if !revisions
        .as_object()
        .is_some_and(|map| map.contains_key(id))
    {
        return Err(fail(
            "invalid_request",
            "expected cell revision required",
            json!({}),
        ));
    }
    Ok(index)
}

fn attachment_names(cell: &Value) -> Result<Vec<String>> {
    let mut names = Vec::new();
    if let Some(list) = cell.get("attachments").and_then(Value::as_array) {
        for name in list {
            filename(name)?;
            names.push(name.as_str().unwrap_or("").to_owned());
        }
    }
    if let Some(alternatives) = cell
        .get("attachment_alternatives")
        .and_then(Value::as_object)
    {
        for group in alternatives.values() {
            let group = group.as_object().context("attachment alternatives")?;
            for name in group.values() {
                filename(name)?;
                names.push(name.as_str().unwrap_or("").to_owned());
            }
        }
    }
    Ok(names)
}

fn files_get<'a>(files: &'a Files, name: &str) -> Option<&'a [u8]> {
    files
        .iter()
        .find(|(item, _)| item == name)
        .map(|(_, data)| data.as_slice())
}

fn files_insert(files: &mut Files, name: String, data: Vec<u8>) {
    if let Some((_, slot)) = files.iter_mut().find(|(item, _)| item == &name) {
        *slot = data;
    } else {
        files.push((name, data));
    }
}

fn files_pop(files: &mut Files, name: &str) -> Option<Vec<u8>> {
    files
        .iter()
        .position(|(item, _)| item == name)
        .map(|index| files.remove(index).1)
}

fn markers(text: &str) -> bool {
    // Python `str.splitlines()` boundaries, not only `\n` and `\r\n`.
    text.split([
        '\n', '\r', '\u{0b}', '\u{0c}', '\u{1c}', '\u{1d}', '\u{1e}', '\u{85}', '\u{2028}',
        '\u{2029}',
    ])
    .any(|line| {
        line.starts_with("<<<<<<< ") || line.starts_with("=======") || line.starts_with(">>>>>>> ")
    })
}

fn rel_native(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .components()
        .map(|component| component.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join(std::path::MAIN_SEPARATOR_STR)
}

fn collect_files(root: &Path, directory: &Path, out: &mut Files) -> Result<()> {
    let mut entries = fs::read_dir(directory)?.collect::<std::io::Result<Vec<_>>>()?;
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        let path = entry.path();
        if path.symlink_metadata()?.file_type().is_symlink() {
            let name = path
                .strip_prefix(root)
                .unwrap_or(&path)
                .to_string_lossy()
                .replace('\\', "/");
            return Err(fail(
                "source_invalid",
                "source path escapes notebook",
                json!({"path": name}),
            ));
        }
        if path.is_dir() {
            collect_files(root, &path, out)?;
        } else if path.is_file() {
            let name = path
                .strip_prefix(root)
                .unwrap_or(&path)
                .components()
                .map(|c| c.as_os_str().to_string_lossy())
                .collect::<Vec<_>>()
                .join("/");
            out.push((name, fs::read(path)?));
        }
    }
    Ok(())
}

fn writable(dir: &Path) -> bool {
    fs::metadata(dir).is_ok_and(|meta| !meta.permissions().readonly())
}

fn lock_contended(error: &anyhow::Error) -> bool {
    error.chain().any(|cause| {
        cause.downcast_ref::<std::io::Error>().is_some_and(|io| {
            io.kind() == std::io::ErrorKind::WouldBlock || io.raw_os_error() == Some(33)
        })
    })
}

fn resolve_lexical(path: &Path) -> Result<PathBuf> {
    if path.exists() {
        return Ok(path.canonicalize()?);
    }
    let mut parent = path.parent().context("path parent")?;
    let mut rest = vec![path.file_name().context("path name")?.to_os_string()];
    while !parent.exists() {
        let Some(name) = parent.file_name() else {
            break;
        };
        rest.push(name.to_os_string());
        let Some(next) = parent.parent() else { break };
        parent = next;
    }
    let mut resolved = if parent.exists() {
        parent.canonicalize()?
    } else {
        parent.to_path_buf()
    };
    for name in rest.into_iter().rev() {
        resolved.push(name);
    }
    Ok(resolved)
}

fn inside(root: &Path, path: &Path) -> bool {
    let root = strip_verbatim(root);
    let path = strip_verbatim(path);
    // Windows paths compare case-insensitively; POSIX paths do not.
    let fold = |text: std::borrow::Cow<'_, str>| {
        if cfg!(windows) {
            text.to_lowercase()
        } else {
            text.into_owned()
        }
    };
    let root = fold(root.to_string_lossy());
    let path = fold(path.to_string_lossy());
    let root = root.trim_end_matches(['\\', '/']);
    path == root || path.starts_with(&format!("{root}\\")) || path.starts_with(&format!("{root}/"))
}

fn strip_verbatim(path: &Path) -> PathBuf {
    let text = path.to_string_lossy();
    if let Some(rest) = text.strip_prefix(r"\\?\UNC\") {
        return PathBuf::from(format!(r"\\{rest}"));
    }
    PathBuf::from(text.strip_prefix(r"\\?\").unwrap_or(&text))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_text_matches_python_dumps() {
        for (value, expected) in [
            (0.1, "0.1"),
            (100.0, "100.0"),
            (1e16, "1e+16"),
            (1.5e-7, "1.5e-07"),
            (0.0001, "0.0001"),
            (-2.5, "-2.5"),
            (1e15, "1000000000000000.0"),
            (-0.0, "-0.0"),
            (123456789012345680000.0, "1.2345678901234568e+20"),
        ] {
            assert_eq!(
                python_number(&serde_json::Number::from_f64(value).unwrap()),
                expected
            );
        }
        assert_eq!(
            quote("a\u{2028}\u{7f}\u{1}\\\""),
            "\"a\u{2028}\u{7f}\\u0001\\\\\\\"\""
        );
        assert!(markers("x\u{2028}<<<<<<< ours"));
        assert!(!markers("x <<<<<<< ours"));
    }

    #[test]
    fn manifest_bytes_match_python_indentation_and_order() {
        let data = json!({
            "schema": 1,
            "id": "0f896e5a-15b0-474a-8d85-9422aeaa4f73",
            "metadata": {},
            "cells": [{"id": "a", "type": "code"}]
        });
        let bytes = manifest_bytes(&data).unwrap();
        let text = String::from_utf8(bytes.clone()).unwrap();
        assert_eq!(
            text,
            "{\n  \"schema\": 1,\n  \"id\": \"0f896e5a-15b0-474a-8d85-9422aeaa4f73\",\n  \"metadata\": {},\n  \"cells\": [\n    {\"id\": \"a\", \"type\": \"code\"}\n  ]\n}\n"
        );
        assert_eq!(
            format!("{:x}", Sha256::digest(&bytes)),
            "8fbb7df3bedab239165656de73ddb67ebda34d1f0ed17e4ddcf65ffee4920ab2"
        );
    }
}
