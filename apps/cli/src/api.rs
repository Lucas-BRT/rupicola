//! Blocking client for the Rupicola backend, authenticated with the stored bearer token.

use crate::mcp_bridge;
use anyhow::{Context, Result, bail};
use rupicola_api::{Capture, NewCapture};
use sha2::{Digest, Sha256};
use std::time::Duration;
use ureq::Agent;

/// Path of the MCP endpoint, which clients may be configured with instead of the base URL.
const MCP_PATH: &str = "/mcp";
/// Versioned API prefix.
const API_PREFIX: &str = rupicola_api::PREFIX;
/// Longest a request may take, uploads included.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(120);

pub struct Client {
    base: String,
    token: String,
    agent: Agent,
}

impl Client {
    /// A client for `server_url` (the base URL, or its `/mcp` endpoint), using the token
    /// from [`mcp_bridge::resolve_token`].
    ///
    /// # Errors
    ///
    /// Fails when no token is stored.
    pub fn connect(server_url: &str) -> Result<Self> {
        let base = server_url.trim_end_matches('/');
        let base = base.strip_suffix(MCP_PATH).unwrap_or(base);
        Ok(Self {
            base: base.to_owned(),
            token: mcp_bridge::resolve_token()?,
            agent: Agent::config_builder()
                .timeout_global(Some(REQUEST_TIMEOUT))
                .http_status_as_error(false)
                .build()
                .into(),
        })
    }

    fn authorization(&self) -> String {
        format!("Bearer {}", self.token)
    }

    /// Sends one MCP JSON-RPC message; returns the HTTP status and body.
    ///
    /// # Errors
    ///
    /// Fails when the server cannot be reached.
    pub fn mcp(&self, payload: &str) -> Result<(u16, String)> {
        let mut response = self
            .agent
            .post(format!("{}{MCP_PATH}", self.base))
            .header("Authorization", self.authorization())
            .header("Content-Type", "application/json")
            .send(payload)
            .context("reach the Rupicola backend")?;
        let status = response.status().as_u16();
        let body = response.body_mut().read_to_string().unwrap_or_default();
        Ok((status, body))
    }

    /// Uploads `data` unless the server already has it; returns its SHA-256.
    ///
    /// # Errors
    ///
    /// Fails when the server rejects the upload.
    pub fn put_blob(&self, data: &[u8]) -> Result<String> {
        let sha256 = format!("{:x}", Sha256::digest(data));
        let url = format!("{}{API_PREFIX}/blobs/{sha256}", self.base);
        let known = self
            .agent
            .head(&url)
            .header("Authorization", self.authorization())
            .call()
            .context("reach the Rupicola backend")?;
        if known.status().is_success() {
            return Ok(sha256);
        }
        let response = self
            .agent
            .put(&url)
            .header("Authorization", self.authorization())
            .header("Content-Type", "application/octet-stream")
            .send(data)
            .context("upload a file")?;
        if !response.status().is_success() {
            bail!("the backend rejected an upload: HTTP {}", response.status());
        }
        Ok(sha256)
    }

    /// Publishes a capture whose files were uploaded with [`Self::put_blob`].
    ///
    /// # Errors
    ///
    /// Fails when the server refuses the capture.
    pub fn publish_capture(&self, project: &str, capture: &NewCapture) -> Result<Capture> {
        let mut response = self
            .agent
            .post(format!(
                "{}{API_PREFIX}/projects/{project}/captures",
                self.base
            ))
            .header("Authorization", self.authorization())
            .header("Content-Type", "application/json")
            .send(serde_json::to_string(capture)?)
            .context("reach the Rupicola backend")?;
        let status = response.status();
        let body = response.body_mut().read_to_string().unwrap_or_default();
        if !status.is_success() {
            bail!("the backend refused the capture: HTTP {status}: {body}");
        }
        serde_json::from_str(&body).context("read the backend's answer")
    }
}
