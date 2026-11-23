//! Client for the Rupicola backend.
//!
//! Callers depend on the [`Api`] trait: [`HttpApi`] talks to a real backend and, with the
//! `mock` feature, `MockApi` lets a test script the answers without one.

mod error;
mod http;

use async_trait::async_trait;
use rupicola_api::{
    Capture, Credentials, Me, NewCapture, NewProject, Project, Register, SessionToken, Setup,
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

    /// Who `token` belongs to.
    async fn me(&self, token: &str) -> Result<Me>;

    /// The projects the caller can see.
    async fn projects(&self, token: &str) -> Result<Vec<Project>>;

    async fn create_project(&self, token: &str, form: &NewProject) -> Result<Project>;

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
