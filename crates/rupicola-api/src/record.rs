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

/// The canonical text of a JSON value: object keys sorted by their bytes, no spaces, and no
/// number that is not a whole one.
///
/// # Errors
///
/// [`RecordError::NotWhole`] naming the first number that has a fraction or an exponent.
pub fn canonical_json(value: &Value) -> Result<String, RecordError> {
    let mut out = String::new();
    write_canonical(value, "$", &mut out)?;
    Ok(out)
}

fn write_canonical(value: &Value, at: &str, out: &mut String) -> Result<(), RecordError> {
    match value {
        Value::Null => out.push_str("null"),
        Value::Bool(flag) => out.push_str(if *flag { "true" } else { "false" }),
        Value::Number(number) => {
            if let Some(whole) = number.as_i64() {
                out.push_str(&whole.to_string());
            } else if let Some(whole) = number.as_u64() {
                out.push_str(&whole.to_string());
            } else {
                return Err(RecordError::NotWhole(at.to_owned()));
            }
        }
        Value::String(text) => out.push_str(
            &serde_json::to_string(text).map_err(|e| RecordError::invalid(e.to_string()))?,
        ),
        Value::Array(items) => {
            out.push('[');
            for (index, item) in items.iter().enumerate() {
                if index > 0 {
                    out.push(',');
                }
                write_canonical(item, &format!("{at}[{index}]"), out)?;
            }
            out.push(']');
        }
        Value::Object(map) => {
            let mut entries: Vec<(&String, &Value)> = map.iter().collect();
            entries.sort_by(|a, b| a.0.as_bytes().cmp(b.0.as_bytes()));
            out.push('{');
            for (index, (key, item)) in entries.into_iter().enumerate() {
                if index > 0 {
                    out.push(',');
                }
                out.push_str(
                    &serde_json::to_string(key).map_err(|e| RecordError::invalid(e.to_string()))?,
                );
                out.push(':');
                write_canonical(item, &format!("{at}.{key}"), out)?;
            }
            out.push('}');
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::CaptureStatus;
    use serde_json::json;

    const ID: &str = "01936f2c-aaaa-7bbb-8ccc-000000000001";
    const PARENT: &str = "01936f2c-aaaa-7bbb-8ccc-000000000000";
    const SHA_A: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    const SHA_B: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
    /// The canonical form of [`record`], written out by hand.
    const CANONICAL: &str = concat!(
        r#"{"captured_at":1795000000000,"environment":{"appearance":{"color_scheme":"dark"},"#,
        r#""display":{"height":915,"orientation":"portrait","scale_milli":2625,"width":412},"#,
        r#""extra":{"android.api_level":"34"},"platform":{"family":"android"}},"format":1,"#,
        r#""id":"01936f2c-aaaa-7bbb-8ccc-000000000001","kind":"scenario","#,
        r#""members":[{"bytes":10,"mime":"image/png","owner":"home","path":"checkpoints/home.png","#,
        r#""role":"checkpoint","sha256":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","#,
        r#""surface":"home"},"#,
        r#"{"bytes":20,"mime":"image/png","owner":"open","path":"clips/open/003.png","#,
        r#""role":"clip-frame","sha256":"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb","#,
        r#""target_ms":300,"video_ms":300}],"#,
        r#""parents":[{"id":"01936f2c-aaaa-7bbb-8ccc-000000000000","kind":"previous"}],"#,
        r#""project":"demo","scenario":{"name":"Settings"},"series":"Settings@a1b2c3","status":"complete"}"#,
    );
    /// SHA-256 of [`CANONICAL`], computed outside this crate.
    const DIGEST: &str = "d0c3fec2457872a0ece7588a177d8522b40e6ae324a12400e9d22785335b34a0";

    fn record() -> CaptureRecord {
        CaptureRecord {
            format: RECORD_FORMAT,
            id: ID.to_owned(),
            project: "demo".to_owned(),
            kind: RecordKind::Scenario,
            scenario: ScenarioRef {
                name: "Settings".to_owned(),
                sha256: None,
            },
            revision: None,
            series: "Settings@a1b2c3".to_owned(),
            parents: vec![ParentRef {
                id: PARENT.to_owned(),
                kind: ParentKind::Previous,
            }],
            captured_at: 1_795_000_000_000,
            status: CaptureStatus::Complete,
            blocker: None,
            environment: Environment {
                platform: Some(Platform {
                    family: Some(PlatformFamily::Android),
                    os_version: None,
                }),
                display: Some(Display {
                    width: Some(412),
                    height: Some(915),
                    scale_milli: Some(2625),
                    orientation: Some("portrait".to_owned()),
                }),
                appearance: Some(Appearance {
                    color_scheme: Some("dark".to_owned()),
                    ..Appearance::default()
                }),
                extra: BTreeMap::from([("android.api_level".to_owned(), "34".to_owned())]),
                ..Environment::default()
            },
            members: vec![
                Member {
                    path: "checkpoints/home.png".to_owned(),
                    sha256: SHA_A.to_owned(),
                    bytes: 10,
                    mime: "image/png".to_owned(),
                    role: MemberRole::Checkpoint,
                    owner: "home".to_owned(),
                    surface: Some("home".to_owned()),
                    locator: BTreeMap::new(),
                    target_ms: None,
                    video_ms: None,
                },
                Member {
                    path: "clips/open/003.png".to_owned(),
                    sha256: SHA_B.to_owned(),
                    bytes: 20,
                    mime: "image/png".to_owned(),
                    role: MemberRole::ClipFrame,
                    owner: "open".to_owned(),
                    surface: None,
                    locator: BTreeMap::new(),
                    target_ms: Some(300),
                    video_ms: Some(300),
                },
            ],
        }
    }

    fn broken(change: impl FnOnce(&mut CaptureRecord)) -> String {
        let mut record = record();
        change(&mut record);
        record.validate().unwrap_err().to_string()
    }

    #[test]
    fn the_canonical_form_is_the_one_written_by_hand() {
        assert_eq!(record().canonical().unwrap(), CANONICAL);
    }

    #[test]
    fn the_digest_is_the_sha256_of_that_form_and_never_changes() {
        assert_eq!(record().digest().unwrap(), DIGEST);
    }

    #[test]
    fn key_order_and_spacing_do_not_change_the_canonical_text() {
        let one = json!({"b": 1, "a": {"y": [1, 2], "x": "é"}});
        let two: Value =
            serde_json::from_str(r#"{ "a" : { "x" : "é", "y" : [ 1 , 2 ] }, "b" : 1 }"#).unwrap();
        assert_eq!(canonical_json(&one).unwrap(), canonical_json(&two).unwrap());
        assert_eq!(
            canonical_json(&one).unwrap(),
            r#"{"a":{"x":"é","y":[1,2]},"b":1}"#
        );
    }

    #[test]
    fn a_number_with_a_fraction_or_an_exponent_has_no_canonical_text() {
        for text in ["2.625", "1e3", "-0.0"] {
            let value: Value = serde_json::from_str(&format!(r#"{{"scale":{text}}}"#)).unwrap();
            assert_eq!(
                canonical_json(&value).unwrap_err(),
                RecordError::NotWhole("$.scale".to_owned()),
                "{text}"
            );
        }
    }

    #[test]
    fn a_record_survives_a_round_trip_with_the_same_digest() {
        let text = serde_json::to_string(&record()).unwrap();
        let back: CaptureRecord = serde_json::from_str(&text).unwrap();
        assert_eq!(back, record());
        assert_eq!(back.digest().unwrap(), record().digest().unwrap());
    }

    #[test]
    fn any_change_to_the_record_changes_the_digest() {
        let mut other = record();
        other.members[1].target_ms = Some(301);
        assert_ne!(other.digest().unwrap(), record().digest().unwrap());
    }

    #[test]
    fn an_unknown_field_is_refused_rather_than_ignored() {
        let mut value = serde_json::to_value(record()).unwrap();
        value["surprise"] = json!(1);
        assert!(serde_json::from_value::<CaptureRecord>(value).is_err());
    }

    #[test]
    fn the_environment_flattens_to_searchable_facets() {
        let facets = record().environment.facets();
        assert_eq!(facets["platform.family"], "android");
        assert_eq!(facets["display.scale_milli"], "2625");
        assert_eq!(facets["appearance.color_scheme"], "dark");
        assert_eq!(facets["extra.android.api_level"], "34");
        assert!(!facets.contains_key("device.model"));
    }

    #[test]
    fn the_rules_of_the_format_are_checked() {
        assert!(broken(|r| r.format = 2).contains("format"));
        assert!(broken(|r| r.id = String::new()).contains("id"));
        assert!(broken(|r| r.parents[0].id = ID.to_owned()).contains("own parent"));
        assert!(broken(|r| r.parents.push(r.parents[0].clone())).contains("twice"));
        assert!(broken(|r| r.captured_at = -1).contains("negative"));
        assert!(broken(|r| r.status = CaptureStatus::Blocked).contains("blocker"));
        assert!(broken(|r| r.blocker = Some("x".to_owned())).contains("no blocker"));
        assert!(broken(|r| r.members.clear()).contains("at least one file"));
        assert!(broken(|r| r.members.reverse()).contains("sorted"));
        assert!(broken(|r| r.members[0].path = "../x.png".to_owned()).contains("path"));
        assert!(broken(|r| r.members[0].path = "a/b.exe".to_owned()).contains("path"));
        assert!(broken(|r| r.members[0].sha256 = "ABC".to_owned()).contains("SHA-256"));
        assert!(broken(|r| r.members[0].target_ms = Some(1)).contains("target_ms"));
        assert!(broken(|r| r.members[1].target_ms = None).contains("target_ms"));
        assert!(broken(|r| r.members[0].video_ms = Some(1)).contains("video_ms"));
        assert!(
            broken(|r| {
                r.environment
                    .extra
                    .insert("Bad Key".to_owned(), "v".to_owned());
            })
            .contains("key")
        );
        assert!(
            broken(|r| {
                r.environment.display.as_mut().unwrap().width = Some(0);
            })
            .contains("zero")
        );
    }

    #[test]
    fn a_blocked_capture_with_a_blocker_and_no_files_is_valid() {
        let mut blocked = record();
        blocked.status = CaptureStatus::Blocked;
        blocked.blocker = Some("ffmpeg is missing".to_owned());
        blocked.members.clear();
        blocked.validate().unwrap();
    }
}
