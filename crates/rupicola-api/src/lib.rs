//! Contract between the Rupicola backend and its clients (web panel, CLI, desktop GUI).
//!
//! Every route the backend serves takes and returns types from this crate, so a
//! client and the server cannot drift apart.

use serde::{Deserialize, Serialize};

#[macro_use]
mod text_enum;
mod design;
mod design_system;
mod fixes;
mod limits;
mod lineage;
mod problems;
pub mod record;
mod removal;
mod scenarios;
mod selections;
mod topics;
pub use design::*;
pub use design_system::*;
pub use fixes::*;
pub use limits::*;
pub use lineage::*;
pub use problems::*;
pub use removal::*;
pub use scenarios::*;
pub use selections::*;
pub use text_enum::UnknownValue;
pub use topics::*;

/// Placeholder used when formatting secret fields in `Debug` output.
const REDACTED: &str = "[redacted]";

/// Path prefix of every versioned API route.
pub const PREFIX: &str = "/v1";

/// Response of `GET /healthz`.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct Health {
    pub status: HealthStatus,
    /// Backend package version.
    pub version: String,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum HealthStatus {
    Ok,
}

/// `POST /v1/sessions`: sign in with a username and password.
#[derive(Clone, Deserialize, Serialize)]
pub struct Credentials {
    pub handle: String,
    pub password: String,
}

/// `POST /v1/setup`: create the first administrator account. `code` is the server's setup
/// code, required unless the backend only listens on loopback.
#[derive(Clone, Deserialize, Serialize)]
pub struct Setup {
    pub handle: String,
    pub password: String,
    #[serde(default)]
    pub code: Option<String>,
}

/// `PUT /v1/me/password`: change the signed-in person's password. Their other sessions end.
#[derive(Clone, Deserialize, Serialize)]
pub struct PasswordChange {
    pub current: String,
    pub new: String,
}

/// `GET /v1/setup`: whether the first administrator still has to be created, so a client
/// knows whether to offer sign-in or setup.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct SetupStatus {
    pub required: bool,
}

/// A bearer token and when it stops working (Unix milliseconds).
#[derive(Clone, Deserialize, Serialize)]
pub struct SessionToken {
    pub token: String,
    pub expires_at: i64,
}

text_enum! {
    /// What a credential is for.
    TokenKind {
        /// A signed-in person, from the panel, CLI or desktop app.
        Session => "session",
        /// An agent or script acting for one person across their projects.
        Machine => "machine",
        /// A capture device bound to one project.
        Device => "device",
    }
}

/// `GET /v1/me`: who the presented credential belongs to.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct Me {
    pub kind: TokenKind,
    pub handle: Option<String>,
    /// Administers the whole server.
    pub admin: bool,
    pub project: Option<String>,
}

text_enum! {
    /// What a caller may do in one project.
    Role {
        /// Reads everything in the project.
        Reader => "reader",
        /// Also triages problems and reviews captures and fixes.
        Reviewer => "reviewer",
        /// Also manages the project, its members and its credentials.
        Owner => "owner",
    }
}

/// A project as seen by the caller.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct Project {
    pub id: String,
    pub label: String,
    pub created_at: i64,
    /// The caller's role; `None` for a capture device, which may only publish.
    pub role: Option<Role>,
}

/// `POST /v1/projects`. The id is a stable slug chosen by the client, such as one
/// derived from the repository.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct NewProject {
    pub id: String,
    pub label: String,
}

/// `PATCH /v1/projects/{id}`.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ProjectUpdate {
    pub label: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct Member {
    pub handle: String,
    pub role: Role,
}

/// `PUT /v1/projects/{id}/members/{handle}`.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct MemberRole {
    pub role: Role,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct AuditEvent {
    pub id: String,
    pub actor: String,
    pub event: String,
    pub occurred_at: i64,
}

/// A file already uploaded with `PUT /v1/blobs/{sha256}`, placed at `path` inside
/// a capture.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct BlobRef {
    pub path: String,
    pub sha256: String,
}

/// Where and how a capture was taken. Every field is optional because backends
/// report different things.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct Environment {
    #[serde(default)]
    pub width: Option<u32>,
    #[serde(default)]
    pub height: Option<u32>,
    #[serde(default)]
    pub scale: Option<f64>,
    #[serde(default)]
    pub color_scheme: Option<String>,
    #[serde(default)]
    pub reduced_motion: bool,
    #[serde(default)]
    pub locale: Option<String>,
    #[serde(default)]
    pub platform: Option<String>,
    #[serde(default)]
    pub renderer: Option<String>,
    #[serde(default)]
    pub browser: Option<String>,
}

/// Thousandths in one unit, for the scales the capture record keeps as whole numbers.
const MILLI: f64 = 1000.0;

impl From<&record::Environment> for Environment {
    /// The environment of a capture record, in the older shape the capture views still read.
    fn from(env: &record::Environment) -> Self {
        let display = env.display.clone().unwrap_or_default();
        let appearance = env.appearance.clone().unwrap_or_default();
        let software = env.software.clone().unwrap_or_default();
        Self {
            width: display.width,
            height: display.height,
            scale: display.scale_milli.map(|milli| f64::from(milli) / MILLI),
            color_scheme: appearance.color_scheme,
            reduced_motion: appearance.reduced_motion.unwrap_or_default(),
            locale: appearance.locale,
            platform: env
                .platform
                .as_ref()
                .and_then(|platform| platform.family)
                .map(|family| family.to_string()),
            renderer: software.renderer,
            browser: None,
        }
    }
}

text_enum! {
    CaptureStatus {
        Complete => "complete",
        /// The capture could not finish; `blocker` says why.
        Blocked => "blocked",
    }
}

text_enum! {
    Inspection {
        Pending => "pending",
        Reviewed => "reviewed",
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct NewCheckpoint {
    pub id: String,
    #[serde(flatten)]
    pub file: BlobRef,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct NewFrame {
    #[serde(flatten)]
    pub file: BlobRef,
    pub target_ms: f64,
    #[serde(default)]
    pub video_ms: Option<f64>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct NewClip {
    pub id: String,
    #[serde(default)]
    pub video: Option<BlobRef>,
    #[serde(default)]
    pub sheet: Option<BlobRef>,
    #[serde(default)]
    pub analysis: Option<BlobRef>,
    #[serde(default)]
    pub frames: Vec<NewFrame>,
}

/// `POST /v1/projects/{id}/captures`: describes a capture whose files were
/// uploaded first.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct NewCapture {
    pub scenario: String,
    #[serde(default)]
    pub revision: Option<String>,
    pub status: CaptureStatus,
    #[serde(default)]
    pub blocker: Option<String>,
    #[serde(default)]
    pub environment: Environment,
    #[serde(default)]
    pub checkpoints: Vec<NewCheckpoint>,
    #[serde(default)]
    pub clips: Vec<NewClip>,
}

/// Interactive step inside a server-run [`CaptureStep::Clip`] trigger.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CaptureAction {
    Click {
        selector: String,
    },
    Hover {
        selector: String,
    },
    Fill {
        selector: String,
        value: String,
    },
    Press {
        selector: String,
        key: String,
    },
    Ready {
        selector: String,
    },
    /// Scrolls by `dx` and `dy` pixels: an element (or its closest scrollable ancestor), or the
    /// page with the mouse wheel when `selector` is absent.
    Scroll {
        #[serde(default)]
        selector: Option<String>,
        dx: i32,
        dy: i32,
    },
    /// Brings an element on screen without moving the pointer.
    ScrollIntoView {
        selector: String,
    },
    /// Scrolls an element, or the page when `selector` is absent, to an absolute offset.
    ScrollTo {
        #[serde(default)]
        selector: Option<String>,
        x: i32,
        y: i32,
    },
    /// Scrolls an element, or the page when `selector` is absent, to its end.
    ScrollToEnd {
        #[serde(default)]
        selector: Option<String>,
    },
    /// Pauses for `ms` milliseconds.
    Wait {
        ms: u32,
    },
}

/// One step executed by the server's external capture runner.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CaptureStep {
    Click {
        selector: String,
    },
    Hover {
        selector: String,
    },
    Fill {
        selector: String,
        value: String,
    },
    Press {
        selector: String,
        key: String,
    },
    Ready {
        selector: String,
    },
    Scroll {
        #[serde(default)]
        selector: Option<String>,
        dx: i32,
        dy: i32,
    },
    ScrollIntoView {
        selector: String,
    },
    ScrollTo {
        #[serde(default)]
        selector: Option<String>,
        x: i32,
        y: i32,
    },
    ScrollToEnd {
        #[serde(default)]
        selector: Option<String>,
    },
    Wait {
        ms: u32,
    },
    Checkpoint {
        id: String,
        /// Shoots the whole scrollable page instead of the viewport.
        #[serde(default)]
        full_page: bool,
    },
    Clip {
        id: String,
        trigger: CaptureAction,
        samples: usize,
        interval_ms: u32,
    },
}

/// `POST /v1/projects/{id}/captures/run`: asks the server to capture `url` using its
/// external capture backend and store the resulting artifacts.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct RunCapture {
    pub scenario: String,
    pub url: String,
    #[serde(default)]
    pub revision: Option<String>,
    #[serde(default)]
    pub environment: Environment,
    #[serde(default)]
    pub timeout_ms: Option<u32>,
    #[serde(default)]
    pub steps: Vec<CaptureStep>,
    #[serde(default)]
    pub allow_degraded: bool,
}

/// A stored media file; fetch its bytes from `GET /v1/artifacts/{id}`.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct Artifact {
    pub id: String,
    pub path: String,
    pub mime: String,
    pub bytes: u64,
    pub sha256: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct Checkpoint {
    pub id: String,
    pub artifact: Artifact,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct Frame {
    pub artifact: Artifact,
    pub target_ms: f64,
    pub video_ms: Option<f64>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct Clip {
    pub id: String,
    pub video: Option<Artifact>,
    pub sheet: Option<Artifact>,
    pub analysis: Option<Artifact>,
    pub frames: Vec<Frame>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct Finding {
    pub id: String,
    pub note: String,
    pub evidence_artifact_id: Option<String>,
    pub author: Option<String>,
    pub created_at: i64,
}

/// `POST /v1/captures/{id}/findings`: a review note, optionally pointing at one of
/// the capture's artifacts.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct NewFinding {
    pub note: String,
    #[serde(default)]
    pub evidence_artifact_id: Option<String>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct Capture {
    pub id: String,
    pub project_id: String,
    pub scenario: String,
    pub revision: Option<String>,
    pub status: CaptureStatus,
    pub blocker: Option<String>,
    pub inspection: Inspection,
    pub environment: Environment,
    pub created_at: i64,
    pub checkpoints: Vec<Checkpoint>,
    pub clips: Vec<Clip>,
    pub findings: Vec<Finding>,
    /// Set when the capture was removed; what cited it still resolves.
    #[serde(default)]
    pub deleted: Option<Tombstone>,
}

/// `GET /v1/projects/{id}/captures/search`: one page of captures, newest first.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct CapturePage {
    pub items: Vec<CaptureSummary>,
    /// The same captures in full, when the search asked for `include=details`.
    #[serde(default)]
    pub details: Vec<Capture>,
    /// What to send as `cursor` for the next page; none on the last one.
    pub next: Option<String>,
}

/// One row of `GET /v1/projects/{id}/captures`, newest first.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct CaptureSummary {
    pub id: String,
    pub scenario: String,
    pub status: CaptureStatus,
    pub inspection: Inspection,
    pub created_at: i64,
    /// Artifact id of the first checkpoint, for a thumbnail.
    pub preview: Option<String>,
}

/// `POST /v1/projects/{id}/invites`. The code is valid for `hours` (default 24,
/// at most 168).
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct NewInvite {
    pub role: Role,
    #[serde(default)]
    pub hours: Option<u32>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct Invite {
    /// SHA-256 of the code; the code itself is shown only once.
    pub id: String,
    pub role: Role,
    pub created_by: Option<String>,
    pub expires_at: i64,
}

/// A new invitation: give `code` to the invitee.
#[derive(Clone, Deserialize, Serialize)]
pub struct IssuedInvite {
    pub code: String,
    pub invite: Invite,
}

impl std::fmt::Debug for IssuedInvite {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("IssuedInvite")
            .field("code", &REDACTED)
            .field("invite", &self.invite)
            .finish()
    }
}

/// `POST /v1/register`: create an account with an invitation code.
#[derive(Clone, Deserialize, Serialize)]
pub struct Register {
    pub code: String,
    pub handle: String,
    pub password: String,
}

/// `POST /v1/invites/redeem`: a signed-in person joins the inviting project.
#[derive(Clone, Deserialize, Serialize)]
pub struct Redeem {
    pub code: String,
}

/// `POST /v1/tokens` and `POST /v1/projects/{id}/devices`.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct NewCredential {
    pub label: String,
}

/// A credential as listed to its owner. The secret is never shown again.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct Credential {
    /// SHA-256 of the secret; use it to revoke the credential.
    pub id: String,
    pub kind: TokenKind,
    pub label: String,
    pub project_id: Option<String>,
    pub created_at: i64,
    pub expires_at: Option<i64>,
    pub last_used_at: Option<i64>,
}

/// A new credential: `token` is the bearer secret, shown only this once.
#[derive(Clone, Deserialize, Serialize)]
pub struct IssuedCredential {
    pub token: String,
    pub credential: Credential,
}

impl std::fmt::Debug for IssuedCredential {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("IssuedCredential")
            .field("token", &REDACTED)
            .field("credential", &self.credential)
            .finish()
    }
}

/// Availability and OS install guidance for one capture backend on the host.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq, Serialize)]
pub struct CaptureBackendStatus {
    /// Backend identifier (for example `web` or `desktop`).
    pub id: String,
    /// Human-readable backend name.
    pub label: String,
    /// Whether a compatible driver or package is detected on the host OS.
    pub available: bool,
    /// Detected binary, browser, or package name when available.
    pub detected: Option<String>,
    /// OS-specific installation guidance shown when absent.
    pub install_hint: Option<String>,
}

/// Detected capture capabilities on the host OS (zero-install default).
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq, Serialize)]
pub struct CaptureCapabilities {
    /// Individual capture backends probed on the host.
    pub backends: Vec<CaptureBackendStatus>,
}

/// Body of every non-2xx API response.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct ErrorBody {
    /// Stable, machine-readable error code, such as `not-found`.
    pub code: String,
    /// Human-readable explanation.
    pub message: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn health_uses_a_stable_wire_format() {
        let health = Health {
            status: HealthStatus::Ok,
            version: "0.1.1".into(),
        };
        let json = serde_json::to_string(&health).unwrap();
        assert_eq!(json, r#"{"status":"ok","version":"0.1.1"}"#);
        assert_eq!(serde_json::from_str::<Health>(&json).unwrap(), health);
    }

    #[test]
    fn issued_invite_debug_redacts_code() {
        const SECRET_CODE: &str = "invite-secret-code-12345";
        const SAMPLE_EXPIRY_MS: i64 = 1_700_000_000_000;

        let issued = IssuedInvite {
            code: SECRET_CODE.into(),
            invite: Invite {
                id: "inv-id".into(),
                role: Role::Reviewer,
                created_by: Some("owner".into()),
                expires_at: SAMPLE_EXPIRY_MS,
            },
        };
        let debug = format!("{issued:?}");
        assert!(!debug.contains(SECRET_CODE));
        assert!(debug.contains(REDACTED));
    }

    #[test]
    fn issued_credential_debug_redacts_token() {
        const SECRET_TOKEN: &str = "rupicola-secret-token-67890";
        const SAMPLE_CREATED_MS: i64 = 1_700_000_000_000;

        let issued = IssuedCredential {
            token: SECRET_TOKEN.into(),
            credential: Credential {
                id: "cred-id".into(),
                kind: TokenKind::Machine,
                label: "ci".into(),
                project_id: None,
                created_at: SAMPLE_CREATED_MS,
                expires_at: None,
                last_used_at: None,
            },
        };
        let debug = format!("{issued:?}");
        assert!(!debug.contains(SECRET_TOKEN));
        assert!(debug.contains(REDACTED));
    }
}
