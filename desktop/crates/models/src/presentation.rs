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
                if !matches!(state, Some("running" | "waiting_input" | "interrupting")) =>
            {
                Some("No execution is running")
            }
            Self::Stop | Self::Restart
                if !matches!(
                    state,
                    Some("idle" | "running" | "waiting_input" | "interrupting")
                ) =>
            {
                Some("The runtime is not running")
            }
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
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
