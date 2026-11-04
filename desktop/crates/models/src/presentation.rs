//! Shared action decisions for menus, commands, and page controls.
use serde_json::Value;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RuntimeAction {
    Checkpoint,
    TrainingCheckpoint,
    Interrupt,
    Terminate,
    Stop,
    Restart,
}

impl RuntimeAction {
    pub fn parse(value: &str) -> Option<Self> {
        Some(match value {
            "checkpoint" => Self::Checkpoint,
            "training-checkpoint" => Self::TrainingCheckpoint,
            "interrupt" => Self::Interrupt,
            "interrupt_kill" => Self::Terminate,
            "stop" => Self::Stop,
            "restart" => Self::Restart,
            _ => return None,
        })
    }
    pub fn unavailable_reason(self, runtime: &Value) -> Option<&'static str> {
        if runtime["connection_lost"] == true {
            return Some("Connection lost — execution state unknown. Reconnect first.");
        }
        let state = runtime["worker_state"].as_str();
        match self {
            Self::Checkpoint | Self::TrainingCheckpoint if runtime["background_tasks"] == true => {
                Some("Background tasks refused cancellation; restart or terminate this worker")
            }
            Self::Checkpoint | Self::TrainingCheckpoint if state != Some("idle") => {
                Some("Run a cell and wait until the runtime is ready")
            }
            Self::TrainingCheckpoint
                if !runtime["capabilities"]
                    .as_array()
                    .is_some_and(|v| v.iter().any(|c| c == "portable_training")) =>
            {
                Some("This interpreter does not support portable training checkpoints")
            }
            Self::Interrupt | Self::Terminate
                if !matches!(
                    state,
                    Some("running" | "waiting_input" | "interrupting" | "paused")
                ) =>
            {
                Some("No execution is running")
            }
            Self::Stop | Self::Restart
                if !matches!(
                    state,
                    Some("idle" | "running" | "waiting_input" | "interrupting" | "paused")
                ) =>
            {
                Some("The runtime is not running")
            }
            _ => None,
        }
    }
}

/// What a checkpoint status means for restore. `None` for statuses this build does not know.
pub fn checkpoint_status(status: &str) -> Option<&'static str> {
    match status {
        "partial" => Some(
            "Some values were not saved. Restore brings back only the saved values; recreate the excluded values listed below.",
        ),
        "complete" => Some("No values were excluded from this checkpoint."),
        _ => None,
    }
}

/// Plain-language meaning of a checkpoint exclusion reason code.
/// `None` for codes this build does not know; callers keep the original code visible.
pub fn exclusion_reason(code: &str) -> Option<&'static str> {
    Some(match code {
        "source_unavailable" => {
            "Its definition could not be matched to the source of a notebook cell, so it cannot be rebuilt."
        }
        "unsupported_type" => "Checkpoints do not support this kind of value.",
        "unsupported_cycle" => {
            "It refers back to itself through a tuple, frozenset, or dictionary key, which checkpoints cannot rebuild."
        }
        "resource" => {
            "It is a live resource, such as an open file, socket, thread, generator, or lock. Live resources are never saved."
        }
        "missing_codec" => {
            "Saving it needs a library that is not installed in this interpreter, such as pyarrow for pandas data."
        }
        "limit" => "It is larger or more deeply nested than the checkpoint limits allow.",
        "denylisted" => "Its name is on the list of values checkpoints never save.",
        _ => return None,
    })
}

/// Plain-language meaning of a structured coverage note kind.
pub fn coverage_note(kind: &str) -> Option<&'static str> {
    Some(match kind {
        "class_from_source" => {
            "Saved as its notebook source; restore defines the class again from that source."
        }
        "closure_cells_split" => {
            "Its closure variables are saved as separate copies. Functions that shared them no longer share them after restore."
        }
        "view_copied" => {
            "This NumPy array view is saved as an independent copy and no longer shares memory with its base array after restore."
        }
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn checkpoint_reasons_are_readable_and_unknown_codes_fall_back() {
        for code in [
            "source_unavailable",
            "unsupported_type",
            "unsupported_cycle",
            "resource",
            "missing_codec",
            "limit",
            "denylisted",
        ] {
            let text = exclusion_reason(code).unwrap();
            assert!(!text.contains('_'), "{code}: {text}");
        }
        assert_eq!(exclusion_reason("future_reason"), None);
        assert_eq!(coverage_note("future_note"), None);
        assert!(coverage_note("class_from_source").is_some());
        assert_eq!(checkpoint_status("restoring"), None);
        assert!(checkpoint_status("partial").unwrap().contains("not saved"));
    }
    #[test]
    fn snapshots_require_idle_and_training_requires_capability() {
        for state in [
            "absent",
            "starting",
            "running",
            "waiting_input",
            "stopped",
            "crashed",
        ] {
            assert!(
                RuntimeAction::Checkpoint
                    .unavailable_reason(&json!({"worker_state":state}))
                    .is_some()
            );
        }
        assert!(
            RuntimeAction::Checkpoint
                .unavailable_reason(&json!({"worker_state":"idle"}))
                .is_none()
        );
        assert!(
            RuntimeAction::TrainingCheckpoint
                .unavailable_reason(&json!({"worker_state":"idle"}))
                .is_some()
        );
        assert!(
            RuntimeAction::TrainingCheckpoint
                .unavailable_reason(
                    &json!({"worker_state":"idle","capabilities":["portable_training"]})
                )
                .is_none()
        );
    }
}
