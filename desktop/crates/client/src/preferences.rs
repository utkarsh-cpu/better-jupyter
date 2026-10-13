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
    pub ui_scale: f32,
}

impl Default for Preferences {
    fn default() -> Self {
        Self {
            schema: 2,
            outline_visible: true,
            outline_width: 220.,
            editor_size: 14.,
            comfortable: false,
            reduced_motion: false,
            ui_scale: 1.,
        }
    }
}

impl Preferences {
    pub fn normalize(&mut self) {
        self.ui_scale = if self.ui_scale.is_finite() {
            self.ui_scale.clamp(1., 2.)
        } else {
            1.
        };
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
            matches!(value.schema, 1 | 2),
            "This version cannot read the newer workspace preferences"
        );
        value.schema = 2;
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
        value.schema = 2;
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
    #[test]
    fn schema_one_migrates_without_losing_layout_and_scale_is_normalized() {
        let directory =
            std::env::temp_dir().join(format!("kl-prefs-migration-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&directory).unwrap();
        std::fs::write(
            directory.join("workbench.json"),
            r#"{"schema":1,"editor_size":18,"outline_visible":false,"outline_width":260}"#,
        )
        .unwrap();
        let mut prefs = Preferences::load(&directory).unwrap();
        assert_eq!(prefs.schema, 2);
        assert_eq!(prefs.editor_size, 18.);
        assert_eq!(prefs.outline_width, 260.);
        assert!(!prefs.outline_visible);
        assert_eq!(prefs.ui_scale, 1.);
        for (value, expected) in [(4., 2.), (0.5, 1.), (f32::NAN, 1.)] {
            prefs.ui_scale = value;
            prefs.normalize();
            assert_eq!(prefs.ui_scale, expected);
        }
        prefs.ui_scale = 1.5;
        prefs.save(&directory).unwrap();
        assert_eq!(Preferences::load(&directory).unwrap().ui_scale, 1.5);
        std::fs::remove_file(directory.join("workbench.json")).unwrap();
        std::fs::remove_dir(directory).unwrap();
    }
    #[test]
    fn save_replaces_own_file_and_preserves_other_preferences_and_future_schema() {
        let directory =
            std::env::temp_dir().join(format!("kl-workbench-prefs-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&directory).unwrap();
        let appearance = directory.join("appearance.txt");
        std::fs::write(&appearance, "contrast").unwrap();
        let mut prefs = Preferences::default();
        prefs.save(&directory).unwrap();
        prefs.outline_visible = false;
        prefs.editor_size = 18.;
        prefs.save(&directory).unwrap();
        assert_eq!(Preferences::load(&directory).unwrap(), prefs);
        assert_eq!(std::fs::read_to_string(&appearance).unwrap(), "contrast");
        let path = directory.join("workbench.json");
        let future = r#"{"schema":99,"future_setting":"preserve"}"#;
        std::fs::write(&path, future).unwrap();
        assert!(prefs.save(&directory).is_err());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), future);
        std::fs::remove_file(path).unwrap();
        std::fs::remove_file(appearance).unwrap();
        std::fs::remove_dir(directory).unwrap();
    }
}
