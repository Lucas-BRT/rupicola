//! The canonical record of one capture, shared by the local store and the backend.
//!
//! A record is immutable: its [`digest`](CaptureRecord::digest) is the SHA-256 of a canonical
//! JSON form, so the client that wrote it and the server that received it get the same value
//! without agreeing on anything but this module. The record holds no floating-point number:
//! the text of a float is not unique, so two sides could hash the same value differently.
//! Times are whole milliseconds and scales are whole thousandths.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use thiserror::Error;

/// The record format this module reads and writes.
pub const RECORD_FORMAT: u32 = 1;
/// Longest id, in characters.
pub const MAX_ID_LEN: usize = 64;
/// Longest file path inside a capture, and deepest nesting (slashes).
pub const MAX_PATH_LEN: usize = 180;
pub const MAX_PATH_DEPTH: usize = 4;
/// Longest checkpoint, clip or surface name.
pub const MAX_NAME_LEN: usize = 128;
/// Longest scenario name, in characters.
pub const MAX_SCENARIO_CHARS: usize = 512;
/// Longest series key.
pub const MAX_SERIES_LEN: usize = 640;
/// Longest key of a free-form map (`extra`, `locator`) and its value.
pub const MAX_KEY_LEN: usize = 64;
pub const MAX_VALUE_LEN: usize = 256;
/// Longest media type.
pub const MAX_MIME_LEN: usize = 100;
/// Length of a SHA-256 written as hex.
const SHA256_HEX_LEN: usize = 64;
/// File extensions a capture may hold.
const EXTENSIONS: [&str; 5] = ["png", "gif", "mp4", "webm", "json"];

/// Why a record was refused.
#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum RecordError {
    /// The record breaks a rule of the format.
    #[error("invalid record: {0}")]
    Invalid(String),
    /// A number is not a whole number, so it has no canonical text.
    #[error("{0} is not a whole number")]
    NotWhole(String),
}

impl RecordError {
    fn invalid(message: impl Into<String>) -> Self {
        Self::Invalid(message.into())
    }
}

text_enum! {
    /// What produced a capture.
    RecordKind {
        /// A scenario run on a capture backend.
        Scenario => "scenario",
        /// Computed from other captures, such as the diff of two of them.
        Derived => "derived",
    }
}

text_enum! {
    /// How a capture relates to its parent.
    ParentKind {
        /// The one before it in the same series.
        Previous => "previous",
        /// What this capture is compared against.
        Baseline => "baseline",
        /// An input of a derived capture.
        Input => "input",
    }
}

text_enum! {
    /// What a file is inside its capture.
    MemberRole {
        Checkpoint => "checkpoint",
        ClipVideo => "clip-video",
        ClipFrame => "clip-frame",
        ClipSheet => "clip-sheet",
        ClipAnalysis => "clip-analysis",
        /// The user-interface tree of a checkpoint, when it was asked for.
        UiSnapshot => "ui-snapshot",
    }
}

text_enum! {
    /// The kind of surface a capture was taken on.
    PlatformFamily {
        Web => "web",
        Android => "android",
        Ios => "ios",
        Desktop => "desktop",
    }
}

/// A parent capture and how this one relates to it.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ParentRef {
    pub id: String,
    pub kind: ParentKind,
}

/// The scenario a capture ran.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ScenarioRef {
    pub name: String,
    /// SHA-256 of the scenario file, so a changed scenario can be told from the same one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sha256: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Platform {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub family: Option<PlatformFamily>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub os_version: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Device {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub form_factor: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Display {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub width: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub height: Option<u32>,
    /// Pixel density in thousandths: 2.625 is `2625`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scale_milli: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub orientation: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Appearance {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color_scheme: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub contrast: Option<String>,
    /// Font scale in thousandths: 1.3 is `1300`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub font_scale_milli: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reduced_motion: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub locale: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Software {
    /// The capture backend that took it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub backend: Option<String>,
    /// What drew the pixels, with its version.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub renderer: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub app_version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub app_build: Option<String>,
}

/// The machine that ran the capture, which is not the surface that was captured.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Host {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub os: Option<String>,
}

/// Where and how a capture was taken, in namespaces so no platform is built into the core.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Environment {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub platform: Option<Platform>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub device: Option<Device>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display: Option<Display>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub appearance: Option<Appearance>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub software: Option<Software>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub host: Option<Host>,
    /// What a backend reports that the namespaces above do not cover, keyed by a prefix of
    /// its platform, such as `android.api_level`.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub extra: BTreeMap<String, String>,
}

impl Environment {
    /// The environment flattened to dotted keys (`appearance.color_scheme`), each with its
    /// value as text, for searching captures by any of them.
    ///
    /// # Panics
    ///
    /// Never: an environment always serialises to JSON.
    #[must_use]
    pub fn facets(&self) -> BTreeMap<String, String> {
        let value = serde_json::to_value(self).expect("an environment serialises to JSON");
        let mut facets = BTreeMap::new();
        Self::flatten("", &value, &mut facets);
        facets
    }

    fn flatten(prefix: &str, value: &Value, facets: &mut BTreeMap<String, String>) {
        match value {
            Value::Object(map) => {
                for (key, inner) in map {
                    let name = if prefix.is_empty() {
                        key.clone()
                    } else {
                        format!("{prefix}.{key}")
                    };
                    Self::flatten(&name, inner, facets);
                }
            }
            Value::String(text) => {
                facets.insert(prefix.to_owned(), text.clone());
            }
            Value::Null => {}
            other => {
                facets.insert(prefix.to_owned(), other.to_string());
            }
        }
    }
}

/// One file of a capture and what it is there.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Member {
    pub path: String,
    pub sha256: String,
    pub bytes: u64,
    pub mime: String,
    pub role: MemberRole,
    /// The checkpoint or clip the file belongs to.
    pub owner: String,
    /// The logical screen the file shows.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub surface: Option<String>,
    /// Where that screen is on its platform: a route, an activity, a window.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub locator: BTreeMap<String, String>,
    /// For a clip frame: when it was asked for, and when the video shows it, in milliseconds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub video_ms: Option<u64>,
}

/// The canonical, immutable description of one capture.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CaptureRecord {
    pub format: u32,
    /// A `UUIDv7` made by the client when the capture was taken.
    pub id: String,
    pub project: String,
    pub kind: RecordKind,
    pub scenario: ScenarioRef,
    /// What the captured software was built from, as the user names it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub revision: Option<String>,
    /// The line of captures this one continues: scenario and environment.
    pub series: String,
    /// The parents it was generated with. The effective lineage can be edited later.
    #[serde(default)]
    pub parents: Vec<ParentRef>,
    /// When it was taken, by the clock of the machine that took it, in milliseconds.
    pub captured_at: i64,
    pub status: crate::CaptureStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub blocker: Option<String>,
    #[serde(default)]
    pub environment: Environment,
    /// Sorted by path, which is unique.
    pub members: Vec<Member>,
}

impl CaptureRecord {
    /// The SHA-256 of the canonical form, as lowercase hex.
    ///
    /// # Errors
    ///
    /// Fails when the record breaks a rule of the format.
    pub fn digest(&self) -> Result<String, RecordError> {
        self.validate()?;
        Ok(format!(
            "{:x}",
            Sha256::digest(self.canonical()?.as_bytes())
        ))
    }

    /// The canonical JSON: keys sorted, no spaces, whole numbers only.
    ///
    /// # Errors
    ///
    /// Fails when the record holds a number that is not whole.
    pub fn canonical(&self) -> Result<String, RecordError> {
        let value = serde_json::to_value(self).map_err(|e| RecordError::invalid(e.to_string()))?;
        canonical_json(&value)
    }

    /// Checks the rules the type system cannot.
    ///
    /// # Errors
    ///
    /// [`RecordError::Invalid`] naming the first rule the record breaks.
    pub fn validate(&self) -> Result<(), RecordError> {
        if self.format != RECORD_FORMAT {
            return Err(RecordError::invalid(format!(
                "format {} is not {RECORD_FORMAT}",
                self.format
            )));
        }
        if !valid_id(&self.id) {
            return Err(RecordError::invalid(
                "the id is empty, too long or not plain",
            ));
        }
        if !valid_id(&self.project) {
            return Err(RecordError::invalid(
                "the project is empty, too long or not plain",
            ));
        }
        if !(1..=MAX_SCENARIO_CHARS).contains(&self.scenario.name.chars().count()) {
            return Err(RecordError::invalid(
                "the scenario name has the wrong length",
            ));
        }
        if let Some(sha) = &self.scenario.sha256
            && !valid_sha256(sha)
        {
            return Err(RecordError::invalid("the scenario digest is not a SHA-256"));
        }
        if !(1..=MAX_SERIES_LEN).contains(&self.series.len()) {
            return Err(RecordError::invalid("the series has the wrong length"));
        }
        if self.captured_at < 0 {
            return Err(RecordError::invalid("captured_at is negative"));
        }
        self.validate_parents()?;
        self.validate_status()?;
        self.validate_environment()?;
        self.validate_members()
    }

    fn validate_parents(&self) -> Result<(), RecordError> {
        let mut seen = std::collections::HashSet::new();
        for parent in &self.parents {
            if parent.id == self.id {
                return Err(RecordError::invalid("a capture cannot be its own parent"));
            }
            if !valid_id(&parent.id) {
                return Err(RecordError::invalid(
                    "a parent id is empty, too long or not plain",
                ));
            }
            if !seen.insert(parent.id.as_str()) {
                return Err(RecordError::invalid("a parent is listed twice"));
            }
        }
        Ok(())
    }

    fn validate_status(&self) -> Result<(), RecordError> {
        let blocker = self.blocker.as_deref().map(str::trim).unwrap_or_default();
        match self.status {
            crate::CaptureStatus::Blocked if blocker.is_empty() => {
                Err(RecordError::invalid("a blocked capture needs a blocker"))
            }
            crate::CaptureStatus::Complete if self.blocker.is_some() => {
                Err(RecordError::invalid("a complete capture has no blocker"))
            }
            crate::CaptureStatus::Complete if self.members.is_empty() => Err(RecordError::invalid(
                "a complete capture needs at least one file",
            )),
            _ => Ok(()),
        }
    }

    fn validate_environment(&self) -> Result<(), RecordError> {
        let env = &self.environment;
        if let Some(display) = &env.display
            && (display.width == Some(0) || display.height == Some(0))
        {
            return Err(RecordError::invalid("the display has a zero side"));
        }
        for (key, value) in &env.extra {
            check_pair("extra", key, value)?;
        }
        Ok(())
    }

    fn validate_members(&self) -> Result<(), RecordError> {
        let mut previous: Option<&str> = None;
        for member in &self.members {
            if previous.is_some_and(|before| before >= member.path.as_str()) {
                return Err(RecordError::invalid(
                    "members must be sorted by path, and each path listed once",
                ));
            }
            previous = Some(&member.path);
            member.validate()?;
        }
        Ok(())
    }
}

impl Member {
    fn validate(&self) -> Result<(), RecordError> {
        if !valid_path(&self.path) {
            return Err(RecordError::invalid(format!(
                "`{}` is not a valid path",
                self.path
            )));
        }
        if !valid_sha256(&self.sha256) {
            return Err(RecordError::invalid(format!(
                "`{}` has a digest that is not a SHA-256",
                self.path
            )));
        }
        if !(1..=MAX_MIME_LEN).contains(&self.mime.len()) {
            return Err(RecordError::invalid(format!(
                "`{}` has no media type",
                self.path
            )));
        }
        if !valid_name(&self.owner) {
            return Err(RecordError::invalid(format!(
                "`{}` has a bad owner",
                self.path
            )));
        }
        if self
            .surface
            .as_deref()
            .is_some_and(|surface| !valid_name(surface))
        {
            return Err(RecordError::invalid(format!(
                "`{}` has a bad surface",
                self.path
            )));
        }
        for (key, value) in &self.locator {
            check_pair("locator", key, value)?;
        }
        let frame = self.role == MemberRole::ClipFrame;
        if frame != self.target_ms.is_some() {
            return Err(RecordError::invalid(format!(
                "`{}`: only clip frames, and all of them, carry target_ms",
                self.path
            )));
        }
        if !frame && self.video_ms.is_some() {
            return Err(RecordError::invalid(format!(
                "`{}`: only clip frames carry video_ms",
                self.path
            )));
        }
        Ok(())
    }
}

/// An id: short, and made of letters, digits and hyphens.
fn valid_id(id: &str) -> bool {
    (1..=MAX_ID_LEN).contains(&id.len())
        && id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
}

fn valid_name(name: &str) -> bool {
    (1..=MAX_NAME_LEN).contains(&name.len())
}

fn valid_sha256(text: &str) -> bool {
    text.len() == SHA256_HEX_LEN && text.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

/// Paths inside a capture: short, relative, plain names with a media extension.
fn valid_path(path: &str) -> bool {
    path.len() <= MAX_PATH_LEN
        && path.matches('/').count() <= MAX_PATH_DEPTH
        && std::path::Path::new(path)
            .extension()
            .and_then(|ext| ext.to_str())
            .is_some_and(|ext| EXTENSIONS.contains(&ext))
        && path.split('/').all(|part| {
            !part.is_empty()
                && !part.starts_with('.')
                && part
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'))
        })
}

/// A key of a free-form map is plain lowercase text, so its sort order is the same in every
/// language; its value is short.
fn check_pair(map: &str, key: &str, value: &str) -> Result<(), RecordError> {
    let plain = (1..=MAX_KEY_LEN).contains(&key.len())
        && key
            .bytes()
            .all(|b| matches!(b, b'a'..=b'z' | b'0'..=b'9' | b'.' | b'_' | b'-'));
    if !plain {
        return Err(RecordError::invalid(format!(
            "`{key}` is not a valid {map} key: use lowercase letters, digits, . _ -"
        )));
    }
    if value.len() > MAX_VALUE_LEN {
        return Err(RecordError::invalid(format!(
            "the {map} value of `{key}` is too long"
        )));
    }
    Ok(())
}

mod canonical;
#[cfg(test)]
mod tests;

pub use canonical::canonical_json;
