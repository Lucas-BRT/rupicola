//! Repository configuration stored in `.rupicola/project.toml`.

use serde::{Deserialize, Serialize};

/// Default seconds a preview command is given to start and pass its readiness check.
pub const DEFAULT_PREVIEW_TIMEOUT_SECS: u64 = 180;

const fn default_preview_timeout_secs() -> u64 {
    DEFAULT_PREVIEW_TIMEOUT_SECS
}

/// How the photographer builds and waits for a preview of a repository tree.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PreviewConfig {
    pub command: String,
    pub ready: String,
    #[serde(default = "default_preview_timeout_secs")]
    pub timeout_secs: u64,
}

/// Repository configuration in `.rupicola/project.toml`.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectConfig {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preview: Option<PreviewConfig>,
}

impl ProjectConfig {
    /// Parses `.rupicola/project.toml` text.
    ///
    /// # Errors
    ///
    /// Fails when the text is invalid TOML or contains unknown fields.
    pub fn from_toml(text: &str) -> Result<Self, toml::de::Error> {
        toml::from_str(text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn project_config_parses_minimal_link_and_full_preview() {
        let minimal = ProjectConfig::from_toml(r#"project = "demo""#).unwrap();
        assert_eq!(minimal.project.as_deref(), Some("demo"));
        assert_eq!(minimal.base, None);
        assert_eq!(minimal.preview, None);

        let full = ProjectConfig::from_toml(
            r#"
            project = "rupicola-site"
            base = "develop"
            [preview]
            command = "cargo run -q -p rupicola-site -- --port {port}"
            ready = "http://127.0.0.1:{port}/healthz"
            "#,
        )
        .unwrap();
        let preview = full.preview.unwrap();
        assert_eq!(preview.timeout_secs, DEFAULT_PREVIEW_TIMEOUT_SECS);
        assert_eq!(full.base.as_deref(), Some("develop"));
    }

    #[test]
    fn project_config_rejects_unknown_fields() {
        assert!(ProjectConfig::from_toml(r#"unknown = "value""#).is_err());
    }
}
