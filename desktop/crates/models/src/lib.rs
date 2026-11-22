//! Presentation state is independent of GPUI and never writes tracked notebook files.
pub mod presentation;
use kernelledger_client::{ProtocolError, hash};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum ConnectionState {
    #[default]
    Offline,
    Connecting,
    Ready,
    Disconnected,
    Stopped,
    MigrationRequired,
    Unsupported,
}
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum WorkerState {
    #[default]
    Absent,
    Starting,
    Idle,
    Running,
    WaitingInput,
    Interrupting,
    Crashed,
    Stopped,
}
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum Mode {
    #[default]
    Command,
    Edit,
}
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum SaveState {
    #[default]
    Saved,
    Dirty,
    Saving,
    Conflict,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceCell {
    pub id: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub source: String,
    pub revision: String,
    #[serde(flatten)]
    pub metadata: BTreeMap<String, Value>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceSnapshot {
    pub state: String,
    #[serde(default)]
    pub manifest_revision: String,
    #[serde(default)]
    pub cells: Vec<SourceCell>,
    #[serde(default)]
    pub files: Vec<String>,
    #[serde(flatten)]
    pub details: BTreeMap<String, Value>,
}
#[derive(Debug, Clone)]
pub struct CellModel {
    pub disk: SourceCell,
    pub text: String,
    pub save: SaveState,
    pub base_revision: String,
    pub output: Vec<Value>,
    pub executed_revision: Option<String>,
    pub exec_id: Option<String>,
    pub ordinal: Option<u64>,
    /// Recorded duration of `exec_id`, never a UI clock or an estimate.
    pub execution_seconds: Option<f64>,
    pub outcome: String,
    /// Kernel session that produced `outcome`/`ordinal`. Runs from an earlier
    /// session say nothing about the live namespace.
    pub session_id: Option<String>,
    pub collapsed: bool,
}
impl CellModel {
    pub fn from_source(disk: SourceCell) -> Self {
        Self {
            text: disk.source.clone(),
            base_revision: disk.revision.clone(),
            disk,
            save: SaveState::Saved,
            output: vec![],
            executed_revision: None,
            exec_id: None,
            ordinal: None,
            execution_seconds: None,
            outcome: String::new(),
            session_id: None,
            collapsed: false,
        }
    }
    pub fn edit(&mut self, text: String) {
        self.text = text;
        if self.text == self.disk.source {
            self.save = SaveState::Saved;
        } else if self.save != SaveState::Conflict {
            self.save = SaveState::Dirty;
        }
    }
    pub fn stale(&self) -> bool {
        self.executed_revision
            .as_ref()
            .is_some_and(|revision| *revision != hash(self.text.as_bytes()))
    }
    pub fn is_executable(&self) -> bool {
        matches!(self.disk.kind.as_str(), "code" | "sql")
    }
    /// The cell's own `metadata` object. `SourceCell::metadata` flattens every
    /// other manifest field (attachments, attachment MIME types, …) beside it,
    /// so per-cell settings such as `disabled` live one level down.
    pub fn cell_metadata(&self) -> serde_json::Map<String, Value> {
        self.disk
            .metadata
            .get("metadata")
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default()
    }
    /// The complete cell `metadata` object with `key` set, or removed when
    /// `value` is null. This is the payload of a `metadata` source mutation,
    /// which replaces the object wholesale.
    pub fn metadata_with(&self, key: &str, value: Value) -> Value {
        let mut metadata = self.cell_metadata();
        if value.is_null() {
            metadata.remove(key);
        } else {
            metadata.insert(key.to_owned(), value);
        }
        Value::Object(metadata)
    }
    fn cell_meta(&self, key: &str) -> Option<&Value> {
        self.disk.metadata.get("metadata").and_then(|m| m.get(key))
    }
    pub fn is_disabled(&self) -> bool {
        self.cell_meta("disabled").and_then(Value::as_bool) == Some(true)
    }
    pub fn is_code_hidden(&self) -> bool {
        self.cell_meta("hide_code").and_then(Value::as_bool) == Some(true)
    }
    pub fn sql_output_var(&self) -> String {
        self.cell_meta("output_var")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
            .unwrap_or("_sql_df")
            .to_owned()
    }
    pub fn run_hash(&self) -> Option<&str> {
        (self.save == SaveState::Saved && self.is_executable())
            .then_some(self.disk.revision.as_str())
    }
    pub fn acknowledge(&mut self, source: SourceCell, submitted: &str) {
        self.base_revision = source.revision.clone();
        self.disk = source;
        self.save = if self.text == submitted {
            SaveState::Saved
        } else {
            SaveState::Dirty
        };
    }
}

#[derive(Default)]
pub struct NotebookModel {
    pub project: PathBuf,
    pub manifest_revision: String,
    pub source_state: String,
    pub cells: BTreeMap<String, CellModel>,
    pub order: Vec<String>,
    pub selected: Option<String>,
    pub mode: Mode,
    pub connection: ConnectionState,
    pub worker: WorkerState,
    pub external_changes: usize,
    pub deleted_drafts: Vec<Draft>,
    pub banner: Option<String>,
    /// Notebook-level manifest `metadata`, as last read from disk.
    pub metadata: serde_json::Map<String, Value>,
    /// The running kernel session, from runtime status; `None` while unknown.
    pub live_session: Option<String>,
}
impl NotebookModel {
    /// Whether the notebook was last saved to open in App mode.
    pub fn saved_app_mode(&self) -> bool {
        self.metadata
            .get("kernelledger")
            .and_then(|k| k.get("app_mode"))
            .and_then(Value::as_bool)
            == Some(true)
    }
    /// The notebook `metadata` object with App mode recorded and every other
    /// field preserved. Off removes the key so ordinary notebooks stay unchanged.
    pub fn metadata_with_app_mode(&self, on: bool) -> Value {
        let mut metadata = self.metadata.clone();
        let mut ledger = metadata
            .get("kernelledger")
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default();
        if on {
            ledger.insert("app_mode".into(), Value::Bool(true));
        } else {
            ledger.remove("app_mode");
        }
        if ledger.is_empty() {
            metadata.remove("kernelledger");
        } else {
            metadata.insert("kernelledger".into(), Value::Object(ledger));
        }
        Value::Object(metadata)
    }
    pub fn apply_snapshot(&mut self, snapshot: SourceSnapshot) {
        self.source_state = snapshot.state;
        self.manifest_revision = snapshot.manifest_revision;
        if self.source_state != "ok" && self.source_state != "conflict_markers" {
            return;
        }
        if let Some(metadata) = snapshot
            .details
            .get("manifest")
            .and_then(|manifest| manifest.get("metadata"))
            .and_then(Value::as_object)
        {
            self.metadata = metadata.clone();
        }
        let mut changed = 0;
        let next_ids: BTreeSet<_> = snapshot.cells.iter().map(|c| c.id.clone()).collect();
        for (id, cell) in &self.cells {
            if !next_ids.contains(id) && cell.save != SaveState::Saved {
                self.deleted_drafts
                    .push(Draft::from_cell(&self.project, cell));
            }
        }
        self.order = snapshot.cells.iter().map(|cell| cell.id.clone()).collect();
        self.cells.retain(|id, _| next_ids.contains(id));
        for source in snapshot.cells {
            if let Some(cell) = self.cells.get_mut(&source.id) {
                if cell.disk.revision != source.revision || cell.disk.kind != source.kind {
                    changed += 1;
                    if cell.save == SaveState::Saved {
                        cell.text = source.source.clone();
                        cell.base_revision = source.revision.clone();
                    } else if cell.text == source.source {
                        cell.save = SaveState::Saved;
                        cell.base_revision = source.revision.clone();
                    } else {
                        cell.save = SaveState::Conflict;
                    }
                    cell.disk = source;
                } else if cell.disk.metadata != source.metadata {
                    cell.disk.metadata = source.metadata;
                }
            } else {
                self.cells
                    .insert(source.id.clone(), CellModel::from_source(source));
                changed += 1;
            }
        }
        self.external_changes = changed;
        if changed > 0 {
            self.banner = Some(format!("{changed} cells changed on disk"));
        }
        if self
            .selected
            .as_ref()
            .is_none_or(|id| !self.cells.contains_key(id))
        {
            self.selected = self.order.first().cloned();
        }
    }
    pub fn save_request(&self, id: &str) -> Option<Value> {
        let cell = self.cells.get(id)?;
        if cell.save == SaveState::Conflict {
            return None;
        }
        Some(
            json!({"action":"edit","cell_id":id,"source":cell.text,"manifest_revision":self.manifest_revision,"cell_revisions":{id:cell.base_revision}}),
        )
    }
    pub fn batch_request(&self, ids: &[String], continue_on_error: bool) -> Option<Value> {
        let mut hashes = BTreeMap::new();
        for id in ids {
            hashes.insert(id, self.cells.get(id)?.run_hash()?);
        }
        Some(
            json!({"cell_ids":ids,"expected_sha256":hashes,"on_error":if continue_on_error {"continue"} else {"stop"}}),
        )
    }
    pub fn take_disk(&mut self, id: &str) {
        if let Some(cell) = self.cells.get_mut(id) {
            cell.text = cell.disk.source.clone();
            cell.base_revision = cell.disk.revision.clone();
            cell.save = SaveState::Saved;
        }
    }
    pub fn overwrite_request(&self, id: &str) -> Option<Value> {
        let cell = self.cells.get(id)?;
        Some(
            json!({"action":"edit","cell_id":id,"source":cell.text,"manifest_revision":self.manifest_revision,"cell_revisions":{id:cell.disk.revision}}),
        )
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Draft {
    pub project: PathBuf,
    pub cell_id: String,
    pub base_revision: String,
    pub draft_revision: String,
    pub source: String,
}
impl Draft {
    pub fn from_cell(project: &Path, cell: &CellModel) -> Self {
        Self {
            project: project.to_owned(),
            cell_id: cell.disk.id.clone(),
            base_revision: cell.base_revision.clone(),
            draft_revision: hash(cell.text.as_bytes()),
            source: cell.text.clone(),
        }
    }
    pub fn path(&self, profile: &Path) -> PathBuf {
        profile
            .join("drafts")
            .join(hash(self.project.to_string_lossy().as_bytes()))
            .join(format!("{}.json", hash(self.cell_id.as_bytes())))
    }
    pub fn persist(&self, profile: &Path) -> std::io::Result<()> {
        let path = self.path(profile);
        fs::create_dir_all(path.parent().unwrap())?;
        let temporary = path.with_extension(format!("{}.tmp", Uuid::new_v4()));
        let mut file = fs::File::create(&temporary)?;
        use std::io::Write;
        file.write_all(&serde_json::to_vec(self)?)?;
        file.sync_all()?;
        fs::rename(temporary, path)
    }
    pub fn remove(&self, profile: &Path) -> std::io::Result<()> {
        match fs::remove_file(self.path(profile)) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            result => result,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingOperation {
    pub request_id: Uuid,
    pub method: String,
    pub request: Value,
    pub operation_id: Option<String>,
    pub state: String,
    pub result: Value,
}
impl PendingOperation {
    pub fn new(method: &str, mut request: Value) -> Self {
        let request_id = Uuid::new_v4();
        request["client_request_id"] = json!(request_id);
        Self {
            request_id,
            method: method.into(),
            request,
            operation_id: None,
            state: "submitting".into(),
            result: Value::Null,
        }
    }
    pub fn disconnected(&mut self) {
        if !self.final_state() {
            self.state = "reconciling".into();
        }
    }
    pub fn final_state(&self) -> bool {
        matches!(
            self.state.as_str(),
            "completed" | "failed" | "canceled" | "unknown"
        )
    }
    pub fn lookup(&self) -> Value {
        self.operation_id.as_ref().map_or_else(
            || json!({"client_request_id":self.request_id}),
            |id| json!({"operation_id":id}),
        )
    }
}

#[derive(Default)]
pub struct FeedCursor {
    pub change_seq: u64,
    pub executions: BTreeMap<String, u64>,
}
impl FeedCursor {
    pub fn change(&mut self, seq: u64) -> bool {
        if seq <= self.change_seq {
            return false;
        }
        self.change_seq = seq;
        true
    }
    pub fn event(&mut self, id: &str, seq: u64) -> bool {
        let current = self.executions.entry(id.into()).or_default();
        if seq <= *current {
            return false;
        }
        *current = seq;
        true
    }
}

pub fn recovery(error: &ProtocolError) -> &'static str {
    match error.code.as_str() {
        "source_conflict" => "Compare your edit with disk",
        "source_invalid" => "Open Diagnostics and resolve the source error",
        "migration_required" => "Migrate ledger with a verified backup",
        "interpreter_required" => "Select a project interpreter",
        "unauthenticated" => "Reconnect to the runtime",
        "unsupported_version" => "Upgrade KernelLedger",
        "trust_required" => "Review provenance before trusting this artifact",
        "python_mismatch" => "Select a matching Python interpreter",
        "input_claimed" => "Wait for the input claim to be released",
        "not_found" => "Inspect the missing artifact in Diagnostics",
        "key_reused" => "Look up the original operation",
        "busy" => "Wait for the active operation",
        "too_large" => "Use a blob reference for this payload",
        _ => "Open Diagnostics for details",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn cell(source: &str) -> SourceCell {
        SourceCell {
            id: "a".into(),
            kind: "code".into(),
            source: source.into(),
            revision: hash(source.as_bytes()),
            metadata: BTreeMap::new(),
        }
    }
    fn snapshot(source: &str) -> SourceSnapshot {
        SourceSnapshot {
            state: "ok".into(),
            manifest_revision: "manifest".into(),
            cells: vec![cell(source)],
            files: vec![],
            details: BTreeMap::new(),
        }
    }
    #[test]
    fn run_waits_for_acknowledged_content() {
        let mut cell = CellModel::from_source(cell("old"));
        cell.edit("new".into());
        assert!(cell.run_hash().is_none());
        cell.acknowledge(super::tests::cell("new"), "new");
        assert_eq!(cell.run_hash(), Some(hash(b"new").as_str()));
    }
    #[test]
    fn conflict_preserves_both_versions() {
        let mut notebook = NotebookModel::default();
        notebook.apply_snapshot(snapshot("old"));
        notebook.cells.get_mut("a").unwrap().edit("mine".into());
        notebook.apply_snapshot(snapshot("theirs"));
        let cell = &notebook.cells["a"];
        assert_eq!(cell.text, "mine");
        assert_eq!(cell.disk.source, "theirs");
        assert_eq!(cell.save, SaveState::Conflict);
        assert!(notebook.save_request("a").is_none());
    }
    #[test]
    fn typing_during_save_remains_dirty() {
        let mut model = CellModel::from_source(cell("old"));
        model.edit("sent".into());
        model.edit("newer".into());
        model.acknowledge(cell("sent"), "sent");
        assert_eq!(model.text, "newer");
        assert_eq!(model.save, SaveState::Dirty);
    }
    #[test]
    fn reconnect_retains_operation_key_and_unknown_is_final() {
        let mut op = PendingOperation::new("run", json!({}));
        let key = op.request_id;
        op.disconnected();
        assert_eq!(op.request_id, key);
        assert!(!op.final_state());
        op.state = "unknown".into();
        assert!(op.final_state());
    }
    #[test]
    fn cell_settings_live_in_the_inner_metadata_object() {
        let disk: SourceCell = serde_json::from_value(json!({
            "id": "a", "type": "sql", "source": "SELECT 1", "revision": "r",
            "metadata": {"tags": ["keep"], "output_var": "totals", "kernelledger": {"ipynb_fields": {"x": 1}}},
            "attachments": ["plot.png"],
            "attachment_mimes": {"plot.png": "image/png"},
            // A top-level field with a setting's name is not the setting.
            "disabled": true,
        }))
        .unwrap();
        let cell = CellModel::from_source(disk);
        assert!(!cell.is_disabled());
        assert_eq!(cell.sql_output_var(), "totals");
        assert_eq!(
            cell.metadata_with("hide_code", json!(true)),
            json!({"tags": ["keep"], "output_var": "totals", "kernelledger": {"ipynb_fields": {"x": 1}}, "hide_code": true})
        );
        assert_eq!(
            cell.metadata_with("output_var", Value::Null),
            json!({"tags": ["keep"], "kernelledger": {"ipynb_fields": {"x": 1}}})
        );
    }
    #[test]
    fn app_mode_round_trips_through_notebook_metadata_without_clobbering() {
        let mut notebook = NotebookModel::default();
        let mut snap = snapshot("x");
        snap.details.insert(
            "manifest".into(),
            json!({"metadata": {"title": "Report", "kernelledger": {"imported_ids": ["a"]}}}),
        );
        notebook.apply_snapshot(snap);
        assert!(!notebook.saved_app_mode());
        let on = notebook.metadata_with_app_mode(true);
        assert_eq!(
            on,
            json!({"title": "Report", "kernelledger": {"imported_ids": ["a"], "app_mode": true}})
        );
        notebook.metadata = on.as_object().unwrap().clone();
        assert!(notebook.saved_app_mode());
        assert_eq!(
            notebook.metadata_with_app_mode(false),
            json!({"title": "Report", "kernelledger": {"imported_ids": ["a"]}})
        );
        notebook.metadata = serde_json::Map::new();
        assert_eq!(notebook.metadata_with_app_mode(false), json!({}));
    }
    #[test]
    fn replayed_feed_deduplicates() {
        let mut feed = FeedCursor::default();
        assert!(feed.change(1));
        assert!(!feed.change(1));
        assert!(feed.event("e", 1));
        assert!(!feed.event("e", 1));
    }
}
