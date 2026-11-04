//! Bounded current-view cache. History and raw events never pass through this cache.
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

const VIEW_LIMIT: usize = 128;
const VIEW_BYTES: usize = 2 * 1024 * 1024;
const CACHE_BYTES: usize = 8 * 1024 * 1024;
const UPDATE_BYTES: usize = 4 * 1024 * 1024;
const UPDATE_IDS: usize = 1024;

fn size(value: &Value) -> usize {
    serde_json::to_vec(value).map_or(usize::MAX, |v| v.len())
}
fn valid_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 256 && id.chars().all(|c| c as u32 >= 32)
}
fn key(view: &Value) -> Option<String> {
    view["cell_id"]
        .as_str()
        .map(str::to_owned)
        .or_else(|| (view["origin"] == "console").then(|| "@console".into()))
}
fn stamp(update: &Value) -> (u64, u64) {
    (
        update["ordinal"].as_u64().unwrap_or(0),
        update["seq"].as_u64().unwrap_or(0),
    )
}

fn view_ids(view: &Value, ids: &mut BTreeSet<(String, String)>) {
    if let Some(session) = view["session_id"].as_str()
        && let Some(outputs) = view["outputs"].as_array()
    {
        for event in outputs {
            if matches!(event["kind"].as_str(), Some("display" | "result"))
                && let Some(id) = event["display_id"].as_str().filter(|id| valid_id(id))
            {
                ids.insert((session.to_owned(), id.to_owned()));
            }
        }
    }
}

pub fn apply(view: &mut Value, update: &Value) -> bool {
    if view["session_id"] != update["session_id"] {
        return false;
    }
    let base_ordinal = view["ordinal"].as_u64().unwrap_or(0);
    let Some(outputs) = view["outputs"].as_array_mut() else {
        return false;
    };
    let mut changed = false;
    for previous in outputs {
        if !matches!(previous["kind"].as_str(), Some("display" | "result"))
            || previous["display_id"] != update["event"]["display_id"]
        {
            continue;
        }
        let baseline = if previous["display_update_origin"].is_object() {
            stamp(&previous["display_update_origin"])
        } else {
            (base_ordinal, previous["seq"].as_u64().unwrap_or(0))
        };
        if stamp(update) <= baseline {
            continue;
        }
        let mut event = update["event"].clone();
        event["kind"] = json!("display");
        event["seq"] = previous["seq"].clone();
        event["timestamp"] = previous["timestamp"].clone();
        event["display_update_origin"] = json!({"exec_id":update["exec_id"],"ordinal":update["ordinal"],"seq":update["seq"],"timestamp":update["timestamp"]});
        *previous = event;
        changed = true;
    }
    changed
}

#[derive(Default)]
pub struct Views {
    views: BTreeMap<String, Value>,
    sizes: BTreeMap<String, usize>,
    bytes: usize,
    latest: BTreeMap<(String, String), Value>,
    update_bytes: usize,
    ids: BTreeSet<(String, String)>,
    pub limited: bool,
}
impl Views {
    pub fn console(&self) -> Option<Value> {
        self.views.get("@console").cloned()
    }
    pub fn register(&mut self, mut view: Value) -> Value {
        let Some(key) = key(&view) else {
            return view;
        };
        self.views.remove(&key);
        self.bytes -= self.sizes.remove(&key).unwrap_or(0);
        let outputs = view["outputs"].as_array();
        let has_id = outputs.is_some_and(|outputs| {
            outputs
                .iter()
                .any(|event| event["display_id"].as_str().is_some_and(valid_id))
        });
        let too_many = outputs.is_some_and(|outputs| outputs.len() > 256);
        if !has_id {
            self.reindex();
            self.prune();
            return view;
        }
        let mut ids = BTreeSet::new();
        for current in self.views.values() {
            view_ids(current, &mut ids);
        }
        view_ids(&view, &mut ids);
        if too_many || ids.len() > UPDATE_IDS {
            self.limited = true;
            self.reindex();
            self.prune();
            return view;
        }
        let fallback = (!self.latest.is_empty()).then(|| view.clone());
        for update in self.latest.values() {
            apply(&mut view, update);
        }
        let bytes = size(&view);
        if self.views.len() >= VIEW_LIMIT || bytes > VIEW_BYTES || self.bytes + bytes > CACHE_BYTES
        {
            self.limited = true;
            if let Some(original) = fallback {
                view = original;
            }
        } else {
            self.bytes += bytes;
            self.sizes.insert(key.clone(), bytes);
            self.views.insert(key, view.clone());
        }
        self.reindex();
        self.prune();
        view
    }
    fn prune(&mut self) {
        self.latest.retain(|key, update| {
            let keep = self.ids.contains(key);
            if !keep {
                self.update_bytes -= size(update);
            }
            keep
        });
    }
    fn reindex(&mut self) {
        self.ids.clear();
        for view in self.views.values() {
            view_ids(view, &mut self.ids);
        }
    }
    pub fn retain(&mut self, executions: &BTreeSet<String>) {
        self.views.retain(|key, view| {
            let keep = key == "@console"
                || view["exec_id"]
                    .as_str()
                    .is_some_and(|id| executions.contains(id));
            if !keep {
                self.bytes -= self.sizes.remove(key).unwrap_or(0);
            }
            keep
        });
        self.reindex();
        self.prune();
    }
    pub fn update(&mut self, update: &Value) -> Vec<Value> {
        if update["omitted"].is_string() {
            self.limited = true;
            return vec![];
        }
        let (Some(session), Some(id)) = (
            update["session_id"].as_str(),
            update["event"]["display_id"].as_str(),
        ) else {
            self.limited = true;
            return vec![];
        };
        if !valid_id(id) {
            self.limited = true;
            return vec![];
        }
        if !self.ids.contains(&(session.to_owned(), id.to_owned())) {
            return vec![];
        }
        let key = (session.to_owned(), id.to_owned());
        if self
            .latest
            .get(&key)
            .is_some_and(|previous| stamp(previous) >= stamp(update))
        {
            return vec![];
        }
        let previous_size = self.latest.get(&key).map_or(0, size);
        let update_size = size(update);
        if (!self.latest.contains_key(&key) && self.latest.len() >= UPDATE_IDS)
            || self.update_bytes - previous_size + update_size > UPDATE_BYTES
        {
            self.limited = true;
            return vec![];
        }
        self.update_bytes = self.update_bytes - previous_size + update_size;
        self.latest.insert(key, update.clone());
        let mut changed = Vec::new();
        let mut dropped = false;
        self.views.retain(|key, view| {
            let applied = apply(view, update);
            if !applied {
                return true;
            }
            let count = size(view);
            let old = self.sizes.get(key).copied().unwrap_or(0);
            if count > VIEW_BYTES || self.bytes - old + count > CACHE_BYTES {
                self.limited = true;
                dropped = true;
                self.bytes -= self.sizes.remove(key).unwrap_or(0);
                return false;
            }
            self.bytes = self.bytes - old + count;
            self.sizes.insert(key.clone(), count);
            changed.push(view.clone());
            true
        });
        if dropped {
            self.reindex();
        }
        self.prune();
        changed
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn view(cell: &str, ordinal: u64) -> Value {
        json!({"cell_id":cell,"exec_id":format!("exec-{cell}-{ordinal}"),"session_id":"session","ordinal":ordinal,
            "outputs":[{"kind":"display","display_id":"shared","bundle":{"text/plain":{"value":"original"}},"seq":1,"timestamp":"original-time"}]})
    }
    fn update(ordinal: u64, value: &str) -> Value {
        json!({"session_id":"session","exec_id":format!("update-{ordinal}"),"ordinal":ordinal,"seq":2,"timestamp":"update-time",
            "event":{"kind":"display_update","display_id":"shared","bundle":{"text/plain":{"value":value}}}})
    }
    #[test]
    fn broadcast_replay_keeps_history_identity_and_reapplies_after_refold() {
        let mut state = Views::default();
        let mut original = view("a", 1);
        let duplicate = original["outputs"][0].clone();
        original["outputs"].as_array_mut().unwrap().push(duplicate);
        state.register(original.clone());
        state.register(view("b", 2));
        let changes = state.update(&update(3, "new"));
        assert_eq!(changes.len(), 2);
        assert_eq!(changes[0]["outputs"].as_array().unwrap().len(), 2);
        for changed in changes {
            for event in changed["outputs"].as_array().unwrap() {
                assert_eq!(event["bundle"]["text/plain"]["value"], "new");
                assert_eq!(event["timestamp"], "original-time");
                assert_eq!(event["seq"], 1);
                assert_eq!(event["display_update_origin"]["exec_id"], "update-3");
            }
        }
        assert_eq!(
            original["outputs"][0]["bundle"]["text/plain"]["value"],
            "original"
        );
        let refolded = state.register(original);
        assert_eq!(
            refolded["outputs"][0]["bundle"]["text/plain"]["value"],
            "new"
        );
        assert!(state.update(&update(2, "stale")).is_empty());
        let mut foreign = update(100, "other worker");
        foreign["session_id"] = json!("other");
        assert!(state.update(&foreign).is_empty());
    }
    #[test]
    fn clear_and_rerun_retire_targets_and_do_not_apply_earlier_updates() {
        let mut state = Views::default();
        state.register(view("a", 1));
        state.update(&update(2, "old"));
        let fresh = state.register(view("a", 3));
        assert_eq!(
            fresh["outputs"][0]["bundle"]["text/plain"]["value"],
            "original"
        );
        let mut empty = view("a", 3);
        empty["outputs"] = json!([]);
        state.register(empty);
        assert!(state.update(&update(4, "no target")).is_empty());
        assert!(state.latest.is_empty());
        state.register(view("a", 1));
        state.retain(&BTreeSet::new());
        assert!(state.views.is_empty());
    }
    #[test]
    fn content_and_target_bounds_report_unfollowed_outputs() {
        let mut ids = Views::default();
        for row in 0..5 {
            let mut candidate = view(&format!("ids-{row}"), row);
            candidate["outputs"] = json!((0..256).map(|index|json!({"kind":"display","display_id":format!("id-{row}-{index}"),"seq":index+1})).collect::<Vec<_>>());
            ids.register(candidate);
        }
        assert!(ids.limited && ids.ids.len() == UPDATE_IDS && ids.views.len() == 4);
        let mut state = Views::default();
        for ordinal in 0..129 {
            state.register(view(&format!("cell-{ordinal}"), ordinal));
        }
        assert!(state.limited && state.views.len() == VIEW_LIMIT);
        let mut large = view("too-large", 200);
        large["source"] = json!("x".repeat(VIEW_BYTES));
        state.register(large);
        assert!(!state.views.contains_key("too-large"));
        state.update(&update(1000, &"x".repeat(60000)));
        assert!(state.views.values().map(size).sum::<usize>() <= CACHE_BYTES);
        assert!(state.latest.values().map(size).sum::<usize>() <= UPDATE_BYTES);
        assert_eq!(state.bytes, state.views.values().map(size).sum::<usize>());
        assert_eq!(
            state.update_bytes,
            state.latest.values().map(size).sum::<usize>()
        );
        let retained = state.views.len();
        state.update(&json!({"omitted":"oversized_payload"}));
        assert_eq!(state.views.len(), retained);
        assert!(state.limited);
    }

    #[test]
    fn a_refold_never_expands_beyond_the_view_budget() {
        let mut state = Views::default();
        state.register(view("existing", 1));
        state.update(&update(3, &"x".repeat(60000)));
        let mut dense = view("dense", 1);
        let first = dense["outputs"][0].clone();
        dense["outputs"] = json!(vec![first; 256]);
        let result = state.register(dense);
        assert!(state.limited && size(&result) < VIEW_BYTES);
        assert_eq!(
            result["outputs"][0]["bundle"]["text/plain"]["value"],
            "original"
        );
        assert!(!state.views.contains_key("dense"));
    }
}
