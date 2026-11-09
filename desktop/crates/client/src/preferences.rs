//! Versioned local presentation preferences. Never touches notebook data or drafts.
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HtmlNetworkPolicy {
    #[default]
    Offline,
    AllowedDomains,
    Full,
}

impl HtmlNetworkPolicy {
    pub fn description(self) -> &'static str {
        match self {
            Self::Offline => {
                "Network access is off. Use self-contained HTML or change HTML network access in Settings."
            }
            Self::AllowedDomains => {
                "Network access is limited to the allowed domains in Settings. API requests still require CORS permission."
            }
            Self::Full => {
                "HTTP(S) and WebSocket network access is enabled. API requests still require CORS permission."
            }
        }
    }
}

/// Exact network origins, never arbitrary CSP source expressions. Bare domains use HTTPS.
pub fn parse_html_origins(input: &str) -> anyhow::Result<Vec<String>> {
    let mut origins = Vec::new();
    for entry in input
        .split(|c: char| c.is_whitespace() || c == ',')
        .filter(|s| !s.is_empty())
    {
        anyhow::ensure!(
            !entry.contains([';', '\'', '"', '*', '\\']),
            "Use exact domains or HTTP(S)/WS(S) origins, without wildcards or credentials."
        );
        let value = if entry.contains("://") {
            entry.to_owned()
        } else {
            format!("https://{entry}")
        };
        let url = url::Url::parse(&value)?;
        anyhow::ensure!(
            matches!(url.scheme(), "http" | "https" | "ws" | "wss")
                && url.username().is_empty()
                && url.password().is_none()
                && url.path() == "/"
                && url.query().is_none()
                && url.fragment().is_none()
                && url.host_str().is_some_and(|host| host
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '.')),
            "Use exact domains or HTTP(S)/WS(S) origins, without paths, credentials or IPv6 literals."
        );
        let origin = url.origin().ascii_serialization();
        if !origins.contains(&origin) {
            origins.push(origin);
        }
    }
    Ok(origins)
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Preferences {
    pub schema: u32,
    pub outline_visible: bool,
    pub outline_width: f32,
    pub editor_size: f32,
    pub comfortable: bool,
    pub reduced_motion: bool,
    pub show_cell_duration: bool,
    pub allow_html_output: bool,
    pub html_network_policy: HtmlNetworkPolicy,
    pub html_allowed_origins: Vec<String>,
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
            show_cell_duration: false,
            allow_html_output: false,
            html_network_policy: HtmlNetworkPolicy::Offline,
            html_allowed_origins: Vec::new(),
            ui_scale: 1.,
        }
    }
}

impl Preferences {
    pub fn normalize(&mut self) {
        // An invalid saved list must fail closed rather than weaken the CSP.
        self.html_allowed_origins =
            parse_html_origins(&self.html_allowed_origins.join(" ")).unwrap_or_default();
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
        assert!(!p.show_cell_duration);
        assert!(!p.allow_html_output);
        assert_eq!(p.html_network_policy, HtmlNetworkPolicy::Offline);
        assert!(p.html_allowed_origins.is_empty());
    }

    #[test]
    fn html_origins_canonicalize_and_reject_policy_injection() {
        assert_eq!(parse_html_origins("CDN.PLOT.LY, https://cdn.plot.ly/\nhttp://127.0.0.1:18763 wss://api.example.com:444").unwrap(), vec!["https://cdn.plot.ly", "http://127.0.0.1:18763", "wss://api.example.com:444"]);
        for bad in [
            "https://good.example;script-src",
            "https://*.example.com",
            "https://user:pass@example.com",
            "https://example.com/path",
            "https://example.com?x=1",
            "https://example.com#x",
            "file:///tmp/page",
            "data:text/html,hi",
            "https://example.com\\evil",
            "https://example.com:'unsafe-inline'",
            "http://[::1]:8000",
        ] {
            assert!(parse_html_origins(bad).is_err(), "{bad}");
        }
        let mut prefs = Preferences {
            html_allowed_origins: vec!["https://good.example;script-src *".into()],
            ..Default::default()
        };
        prefs.normalize();
        assert!(prefs.html_allowed_origins.is_empty());
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
        prefs.show_cell_duration = true;
        prefs.html_network_policy = HtmlNetworkPolicy::AllowedDomains;
        prefs.html_allowed_origins =
            vec!["https://cdn.plot.ly".into(), "http://localhost:8000".into()];
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
