//! Client for the Rupicola backend.
//!
//! Callers depend on the [`Api`] trait: [`HttpApi`] talks to a real backend and, with the
//! `mock` feature, `MockApi` lets a test script the answers without one.

mod error;
mod http;

use async_trait::async_trait;
use rupicola_api::{
    AuditEvent, Capture, CaptureSummary, Comment, Credential, Credentials, DesignEntry,
    DocumentEdit, Finding, Fix, FixSummary, IssuedCredential, IssuedInvite, Me, Member, NewCapture,
    NewComment, NewCredential, NewDesignEntry, NewFinding, NewFix, NewInvite, NewProblem,
    NewProject, NewReview, NewStep, PasswordChange, Problem, ProblemPatch, ProblemSummary, Project,
    Redeem, Register, SessionToken, Setup, Step, SystemActionForm, SystemList, SystemVersion,
};

pub use error::{ClientError, Result};
pub use http::HttpApi;

/// The raw answer of the MCP endpoint. Any HTTP status is data here, because an MCP
/// client relays it instead of failing.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct McpReply {
    pub status: u16,
    pub body: String,
}

/// A stored file as the backend serves it, which may be only the part a `Range` asked for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ArtifactBody {
    /// 200 for the whole file, 206 for the requested part.
    pub status: u16,
    pub content_type: Option<String>,
    /// Which bytes of the file `bytes` holds, for a 206.
    pub content_range: Option<String>,
    pub bytes: Vec<u8>,
}

/// What a client may ask of the backend. Every call that acts for a person takes the
/// bearer `token` returned by [`Api::sign_in`], [`Api::register`] or [`Api::setup`].
#[cfg_attr(feature = "mock", mockall::automock)]
#[async_trait]
pub trait Api: Send + Sync {
    /// Whether the first administrator still has to be created with [`Api::setup`].
    async fn setup_required(&self) -> Result<bool>;

    /// Creates the first administrator account and signs it in.
    async fn setup(&self, form: &Setup) -> Result<SessionToken>;

    async fn sign_in(&self, form: &Credentials) -> Result<SessionToken>;

    /// Creates an account with an invitation code and signs it in.
    async fn register(&self, form: &Register) -> Result<SessionToken>;

    async fn sign_out(&self, token: &str) -> Result<()>;

    /// Ends every other session of the person `token` belongs to.
    async fn sign_out_others(&self, token: &str) -> Result<()>;

    /// Changes the password of the person `token` belongs to; their other sessions end.
    ///
    /// # Errors
    ///
    /// [`ClientError::Invalid`] when the current password is wrong or the new one is not
    /// acceptable.
    async fn change_password(&self, token: &str, form: &PasswordChange) -> Result<()>;

    /// The sessions and machine tokens of the person `token` belongs to.
    async fn tokens(&self, token: &str) -> Result<Vec<Credential>>;

    /// Issues a machine token that acts as the person `token` belongs to. The secret is in
    /// the answer and is never shown again.
    ///
    /// # Errors
    ///
    /// [`ClientError::Forbidden`] when `token` is not a person's session.
    async fn create_token(&self, token: &str, form: &NewCredential) -> Result<IssuedCredential>;

    /// Revokes one of the caller's sessions or machine tokens by its id.
    async fn delete_token(&self, token: &str, id: &str) -> Result<()>;

    /// Who `token` belongs to.
    async fn me(&self, token: &str) -> Result<Me>;

    /// The projects the caller can see.
    async fn projects(&self, token: &str) -> Result<Vec<Project>>;

    async fn create_project(&self, token: &str, form: &NewProject) -> Result<Project>;

    /// Who belongs to `project`, and in which role.
    async fn members(&self, token: &str, project: &str) -> Result<Vec<Member>>;

    /// Removes `handle` from `project`.
    async fn remove_member(&self, token: &str, project: &str, handle: &str) -> Result<()>;

    /// The most recent access events of `project`.
    async fn audit(&self, token: &str, project: &str) -> Result<Vec<AuditEvent>>;

    /// Issues an invitation to `project`; the code is shown only in the answer.
    async fn create_invite(
        &self,
        token: &str,
        project: &str,
        form: &NewInvite,
    ) -> Result<IssuedInvite>;

    /// Joins the project an invitation code belongs to.
    async fn redeem_invite(&self, token: &str, form: &Redeem) -> Result<Project>;

    /// The captures of `project`, newest first.
    async fn captures(&self, token: &str, project: &str) -> Result<Vec<CaptureSummary>>;

    /// One capture, with its checkpoints, clips and findings.
    async fn capture(&self, token: &str, id: &str) -> Result<Capture>;

    /// Records an observation on a capture, optionally pointing at one of its artifacts.
    async fn add_finding(&self, token: &str, capture: &str, form: &NewFinding) -> Result<Finding>;

    /// The problems of `project`.
    async fn problems(&self, token: &str, project: &str) -> Result<Vec<ProblemSummary>>;

    /// One problem, with its evidence and comments.
    async fn problem(&self, token: &str, id: &str) -> Result<Problem>;

    /// Opens a problem on `project`.
    async fn create_problem(
        &self,
        token: &str,
        project: &str,
        form: &NewProblem,
    ) -> Result<Problem>;

    /// Changes the status, severity or proposed solution of a problem.
    async fn update_problem(&self, token: &str, id: &str, patch: &ProblemPatch) -> Result<Problem>;

    /// Adds a comment to a problem.
    async fn comment_problem(&self, token: &str, id: &str, form: &NewComment) -> Result<Comment>;

    /// The design records of `project`, newest first.
    async fn design(&self, token: &str, project: &str) -> Result<Vec<DesignEntry>>;

    /// Records a design brief, direction, tactic or decision on `project`.
    async fn add_design(
        &self,
        token: &str,
        project: &str,
        form: &NewDesignEntry,
    ) -> Result<DesignEntry>;

    /// The design-system versions of `project`, newest first, and which one is adopted.
    async fn design_systems(&self, token: &str, project: &str) -> Result<SystemList>;

    /// One version of the project's design system.
    async fn design_system(&self, token: &str, project: &str, id: &str) -> Result<SystemVersion>;

    /// A new draft, starting from the adopted version.
    async fn create_design_system(&self, token: &str, project: &str) -> Result<SystemVersion>;

    /// Replaces the document of a draft; `edit` names the generation it read.
    async fn edit_design_system(
        &self,
        token: &str,
        project: &str,
        id: &str,
        edit: &DocumentEdit,
    ) -> Result<SystemVersion>;

    /// Validates, reviews, adopts or varies a version.
    async fn act_on_design_system(
        &self,
        token: &str,
        project: &str,
        id: &str,
        form: &SystemActionForm,
    ) -> Result<SystemVersion>;

    /// The fixes of `project`.
    async fn fixes(&self, token: &str, project: &str) -> Result<Vec<FixSummary>>;

    /// One fix, with its steps and reviews.
    async fn fix(&self, token: &str, id: &str) -> Result<Fix>;

    /// Records a fix for one or more problems of `project`.
    async fn create_fix(&self, token: &str, project: &str, form: &NewFix) -> Result<Fix>;

    /// Appends a step to a fix.
    async fn add_step(&self, token: &str, id: &str, form: &NewStep) -> Result<Step>;

    /// Reviews a fix; the backend moves its problems accordingly.
    async fn review_fix(&self, token: &str, id: &str, form: &NewReview) -> Result<Fix>;

    /// The bytes of an artifact; `range` is an HTTP `Range` header value. It is owned because
    /// the mock generated for this trait cannot take an optional borrowed argument.
    async fn artifact(&self, token: &str, id: &str, range: Option<String>) -> Result<ArtifactBody>;

    /// Uploads `data` unless the backend already has it; returns its SHA-256.
    async fn put_blob(&self, token: &str, data: &[u8]) -> Result<String>;

    /// Publishes a capture whose files were uploaded with [`Api::put_blob`].
    async fn publish_capture(
        &self,
        token: &str,
        project: &str,
        capture: &NewCapture,
    ) -> Result<Capture>;

    /// Sends one MCP JSON-RPC message.
    ///
    /// # Errors
    ///
    /// Only when the backend cannot be reached.
    async fn mcp(&self, token: &str, payload: &str) -> Result<McpReply>;
}
