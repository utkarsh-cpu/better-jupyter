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
    pub outcome: String,
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
            outcome: String::new(),
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
    pub fn run_hash(&self) -> Option<&str> {
        (self.save == SaveState::Saved && self.disk.kind == "code")
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
}
impl NotebookModel {
    pub fn apply_snapshot(&mut self, snapshot: SourceSnapshot) {
        self.source_state = snapshot.state;
        self.manifest_revision = snapshot.manifest_revision;
        if self.source_state != "ok" && self.source_state != "conflict_markers" {
            return;
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
    fn replayed_feed_deduplicates() {
        let mut feed = FeedCursor::default();
        assert!(feed.change(1));
        assert!(!feed.change(1));
        assert!(feed.event("e", 1));
        assert!(!feed.event("e", 1));
    }
}
