//! Versioned local presentation preferences. Never touches notebook data or drafts.
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Preferences {
    pub schema: u32,
    pub outline_visible: bool,
    pub outline_width: f32,
    pub editor_size: f32,
    pub comfortable: bool,
    pub reduced_motion: bool,
}

impl Default for Preferences {
    fn default() -> Self {
        Self {
            schema: 1,
            outline_visible: true,
            outline_width: 220.,
            editor_size: 14.,
            comfortable: false,
            reduced_motion: false,
        }
    }
}

impl Preferences {
    pub fn normalize(&mut self) {
        self.outline_width = if self.outline_width.is_finite() {
            self.outline_width.clamp(180., 300.)
        } else {
            220.
        };
        self.editor_size = if self.editor_size.is_finite() {
            self.editor_size.clamp(12., 24.)
        } else {
            14.
        };
    }
    pub fn load(profile: &Path) -> anyhow::Result<Self> {
        let path = profile.join("workbench.json");
        if !path.exists() {
            return Ok(Self::default());
        }
        let mut value: Self = serde_json::from_slice(&std::fs::read(path)?)?;
        anyhow::ensure!(
            value.schema == 1,
            "This version cannot read the newer workspace preferences"
        );
        value.normalize();
        Ok(value)
    }
    pub fn save(&self, profile: &Path) -> anyhow::Result<()> {
        std::fs::create_dir_all(profile)?;
        let path = profile.join("workbench.json");
        // Refuse to overwrite an unknown future schema.
        if path.exists() {
            Self::load(profile)?;
        }
        let mut value = self.clone();
        value.normalize();
        let temp = profile.join("workbench.json.tmp");
        std::fs::write(&temp, serde_json::to_vec_pretty(&value)?)?;
        std::fs::rename(temp, path)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn old_partial_preferences_receive_defaults_and_stale_dimensions_are_clamped() {
        let mut p: Preferences = serde_json::from_str(
            r#"{"outline_width":900,"editor_size":3,"outline_visible":false}"#,
        )
        .unwrap();
        p.normalize();
        assert_eq!(p.outline_width, 300.);
        assert_eq!(p.editor_size, 12.);
        assert!(!p.outline_visible);
        assert!(!p.comfortable);
    }
}
