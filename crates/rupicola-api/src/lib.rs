//! Contract between the Rupicola backend and its clients (web panel, CLI, desktop GUI).
//!
//! Every route the backend serves takes and returns types from this crate, so a
//! client and the server cannot drift apart.

use serde::{Deserialize, Serialize};

#[macro_use]
mod text_enum;
mod fixes;
mod problems;
pub use fixes::*;
pub use problems::*;
pub use text_enum::UnknownValue;

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

/// What a credential is for.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum TokenKind {
    /// A signed-in person, from the panel, CLI or desktop app.
    Session,
    /// An agent or script acting for one person across their projects.
    Machine,
    /// A capture device bound to one project.
    Device,
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

/// What a caller may do in one project.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Role {
    /// Reads everything in the project.
    Reader,
    /// Also triages problems and reviews captures and fixes.
    Reviewer,
    /// Also manages the project, its members and its credentials.
    Owner,
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

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum CaptureStatus {
    Complete,
    /// The capture could not finish; `blocker` says why.
    Blocked,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Inspection {
    Pending,
    Reviewed,
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
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct IssuedInvite {
    pub code: String,
    pub invite: Invite,
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
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct IssuedCredential {
    pub token: String,
    pub credential: Credential,
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
}
