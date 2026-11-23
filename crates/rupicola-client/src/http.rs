use crate::{Api, ArtifactBody, ClientError, McpReply, Result};
use async_trait::async_trait;
use reqwest::{Method, RequestBuilder, header};
use rupicola_api::{
    AuditEvent, Capture, CaptureSummary, Credentials, Finding, IssuedInvite, Me, Member,
    NewCapture, NewFinding, NewInvite, NewProject, PREFIX, PasswordChange, Project, Redeem,
    Register, SessionToken, Setup, SetupStatus,
};
use serde::de::DeserializeOwned;
use sha2::{Digest, Sha256};
use std::time::Duration;

/// Path of the MCP endpoint, which sits outside the versioned prefix.
const MCP_PATH: &str = "/mcp";
const OCTET_STREAM: &str = "application/octet-stream";
const JSON: &str = "application/json";

/// Longest a call may take, uploads included.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
/// Longest the backend gets to accept a connection.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);

/// The backend over HTTP.
#[derive(Clone, Debug)]
pub struct HttpApi {
    base: String,
    client: reqwest::Client,
}

impl HttpApi {
    /// A client for the backend at `server_url`, such as `https://rupicola.example`.
    ///
    /// # Errors
    ///
    /// Fails when the HTTP client cannot be built.
    pub fn new(server_url: &str) -> Result<Self> {
        let client = reqwest::Client::builder()
            .timeout(REQUEST_TIMEOUT)
            .connect_timeout(CONNECT_TIMEOUT)
            .build()
            .map_err(|e| ClientError::Unavailable(e.to_string()))?;
        Ok(Self {
            base: server_url.trim_end_matches('/').to_owned(),
            client,
        })
    }

    fn request(&self, method: Method, path: &str, token: Option<&str>) -> RequestBuilder {
        let builder = self
            .client
            .request(method, format!("{}{PREFIX}{path}", self.base));
        match token {
            Some(token) => builder.bearer_auth(token),
            None => builder,
        }
    }

    async fn send(request: RequestBuilder) -> Result<reqwest::Response> {
        let response = request
            .send()
            .await
            .map_err(|e| ClientError::Unavailable(e.to_string()))?;
        let status = response.status();
        if status.is_success() {
            return Ok(response);
        }
        let retry_after = response
            .headers()
            .get(header::RETRY_AFTER)
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.parse().ok());
        let body = response.text().await.unwrap_or_default();
        Err(ClientError::from_response(status, retry_after, &body))
    }

    async fn read<T: DeserializeOwned>(request: RequestBuilder) -> Result<T> {
        Self::send(request)
            .await?
            .json()
            .await
            .map_err(|e| ClientError::Malformed(e.to_string()))
    }
}

#[async_trait]
impl Api for HttpApi {
    async fn setup_required(&self) -> Result<bool> {
        Self::read::<SetupStatus>(self.request(Method::GET, "/setup", None))
            .await
            .map(|status| status.required)
    }

    async fn setup(&self, form: &Setup) -> Result<SessionToken> {
        Self::read(self.request(Method::POST, "/setup", None).json(form)).await
    }

    async fn sign_in(&self, form: &Credentials) -> Result<SessionToken> {
        Self::read(self.request(Method::POST, "/sessions", None).json(form)).await
    }

    async fn register(&self, form: &Register) -> Result<SessionToken> {
        Self::read(self.request(Method::POST, "/register", None).json(form)).await
    }

    async fn sign_out(&self, token: &str) -> Result<()> {
        Self::send(self.request(Method::DELETE, "/sessions/current", Some(token)))
            .await
            .map(|_| ())
    }

    async fn sign_out_others(&self, token: &str) -> Result<()> {
        Self::send(self.request(Method::DELETE, "/sessions/others", Some(token)))
            .await
            .map(|_| ())
    }

    async fn change_password(&self, token: &str, form: &PasswordChange) -> Result<()> {
        Self::send(
            self.request(Method::PUT, "/me/password", Some(token))
                .json(form),
        )
        .await
        .map(|_| ())
    }

    async fn me(&self, token: &str) -> Result<Me> {
        Self::read(self.request(Method::GET, "/me", Some(token))).await
    }

    async fn projects(&self, token: &str) -> Result<Vec<Project>> {
        Self::read(self.request(Method::GET, "/projects", Some(token))).await
    }

    async fn create_project(&self, token: &str, form: &NewProject) -> Result<Project> {
        Self::read(
            self.request(Method::POST, "/projects", Some(token))
                .json(form),
        )
        .await
    }

    async fn members(&self, token: &str, project: &str) -> Result<Vec<Member>> {
        Self::read(self.request(
            Method::GET,
            &format!("/projects/{project}/members"),
            Some(token),
        ))
        .await
    }

    async fn remove_member(&self, token: &str, project: &str, handle: &str) -> Result<()> {
        Self::send(self.request(
            Method::DELETE,
            &format!("/projects/{project}/members/{handle}"),
            Some(token),
        ))
        .await
        .map(|_| ())
    }

    async fn audit(&self, token: &str, project: &str) -> Result<Vec<AuditEvent>> {
        Self::read(self.request(
            Method::GET,
            &format!("/projects/{project}/audit"),
            Some(token),
        ))
        .await
    }

    async fn create_invite(
        &self,
        token: &str,
        project: &str,
        form: &NewInvite,
    ) -> Result<IssuedInvite> {
        Self::read(
            self.request(
                Method::POST,
                &format!("/projects/{project}/invites"),
                Some(token),
            )
            .json(form),
        )
        .await
    }

    async fn redeem_invite(&self, token: &str, form: &Redeem) -> Result<Project> {
        Self::read(
            self.request(Method::POST, "/invites/redeem", Some(token))
                .json(form),
        )
        .await
    }

    async fn captures(&self, token: &str, project: &str) -> Result<Vec<CaptureSummary>> {
        Self::read(self.request(
            Method::GET,
            &format!("/projects/{project}/captures"),
            Some(token),
        ))
        .await
    }

    async fn capture(&self, token: &str, id: &str) -> Result<Capture> {
        Self::read(self.request(Method::GET, &format!("/captures/{id}"), Some(token))).await
    }

    async fn add_finding(&self, token: &str, capture: &str, form: &NewFinding) -> Result<Finding> {
        Self::read(
            self.request(
                Method::POST,
                &format!("/captures/{capture}/findings"),
                Some(token),
            )
            .json(form),
        )
        .await
    }

    async fn artifact(&self, token: &str, id: &str, range: Option<String>) -> Result<ArtifactBody> {
        let mut request = self.request(Method::GET, &format!("/artifacts/{id}"), Some(token));
        if let Some(range) = range {
            request = request.header(header::RANGE, range);
        }
        let response = Self::send(request).await?;
        let text = |name| {
            response
                .headers()
                .get(name)
                .and_then(|value| value.to_str().ok())
                .map(str::to_owned)
        };
        let status = response.status().as_u16();
        let content_type = text(header::CONTENT_TYPE);
        let content_range = text(header::CONTENT_RANGE);
        let bytes = response
            .bytes()
            .await
            .map_err(|e| ClientError::Unavailable(e.to_string()))?
            .to_vec();
        Ok(ArtifactBody {
            status,
            content_type,
            content_range,
            bytes,
        })
    }

    async fn put_blob(&self, token: &str, data: &[u8]) -> Result<String> {
        let sha256 = format!("{:x}", Sha256::digest(data));
        let path = format!("/blobs/{sha256}");
        let known = self
            .request(Method::HEAD, &path, Some(token))
            .send()
            .await
            .map_err(|e| ClientError::Unavailable(e.to_string()))?;
        if known.status().is_success() {
            return Ok(sha256);
        }
        Self::send(
            self.request(Method::PUT, &path, Some(token))
                .header(header::CONTENT_TYPE, OCTET_STREAM)
                .body(data.to_vec()),
        )
        .await?;
        Ok(sha256)
    }

    async fn publish_capture(
        &self,
        token: &str,
        project: &str,
        capture: &NewCapture,
    ) -> Result<Capture> {
        Self::read(
            self.request(
                Method::POST,
                &format!("/projects/{project}/captures"),
                Some(token),
            )
            .json(capture),
        )
        .await
    }

    async fn mcp(&self, token: &str, payload: &str) -> Result<McpReply> {
        let response = self
            .client
            .post(format!("{}{MCP_PATH}", self.base))
            .bearer_auth(token)
            .header(header::CONTENT_TYPE, JSON)
            .body(payload.to_owned())
            .send()
            .await
            .map_err(|e| ClientError::Unavailable(e.to_string()))?;
        let status = response.status().as_u16();
        let body = response.text().await.unwrap_or_default();
        Ok(McpReply { status, body })
    }
}
