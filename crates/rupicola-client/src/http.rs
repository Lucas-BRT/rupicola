use crate::{Api, ArtifactBody, ClientError, McpReply, Result};
use async_trait::async_trait;
use reqwest::{Method, RequestBuilder, header};
use rupicola_api::{
    AuditEvent, Capture, CapturePage, CaptureSummary, Comment, Credential, Credentials,
    DesignEntry, DocumentEdit, Finding, Fix, FixSummary, IssuedCredential, IssuedInvite, Job,
    JobFailure, JobProgress, JobResult, Me, Member, Missing, MissingQuery, NewCapture, NewComment,
    NewCredential, NewDesignEntry, NewFinding, NewFix, NewInvite, NewJob, NewProblem, NewProject,
    NewReview, NewStep, PREFIX, PasswordChange, Problem, ProblemPatch, ProblemSummary, Project,
    Redeem, Register, RunCapture, Scenario, ScenarioSummary, ScenarioVersion, SessionToken, Setup,
    SetupStatus, Step, SystemActionForm, SystemList, SystemVersion,
};
use serde::de::DeserializeOwned;
use std::time::Duration;

/// Path of the MCP endpoint, which sits outside the versioned prefix.
const MCP_PATH: &str = "/mcp";
const OCTET_STREAM: &str = "application/octet-stream";
const JSON: &str = "application/json";
const HEX_UPPER: &[u8; 16] = b"0123456789ABCDEF";
const HIGH_NIBBLE_SHIFT: u8 = 4;
const LOW_NIBBLE_MASK: u8 = 0x0F;

/// Longest standard API calls may take.
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);
/// Longest blob uploads, artifact downloads, `run_capture`, and `mcp` calls may take.
const LONG_TIMEOUT: Duration = Duration::from_secs(600);
/// Longest the backend gets to accept a connection.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);

/// Percent-encodes a single URL path segment per RFC 3986, also escaping `.` and `..`
/// dot-segments so they cannot traverse paths.
fn encode_segment(segment: &str) -> String {
    if segment == "." {
        return "%2E".to_owned();
    }
    if segment == ".." {
        return "%2E%2E".to_owned();
    }
    let mut encoded = String::with_capacity(segment.len());
    for &byte in segment.as_bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
            encoded.push(char::from(byte));
        } else {
            encoded.push('%');
            encoded.push(char::from(
                HEX_UPPER[usize::from(byte >> HIGH_NIBBLE_SHIFT)],
            ));
            encoded.push(char::from(HEX_UPPER[usize::from(byte & LOW_NIBBLE_MASK)]));
        }
    }
    encoded
}

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
            .timeout(DEFAULT_TIMEOUT)
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

    async fn tokens(&self, token: &str) -> Result<Vec<Credential>> {
        Self::read(self.request(Method::GET, "/tokens", Some(token))).await
    }

    async fn create_token(&self, token: &str, form: &NewCredential) -> Result<IssuedCredential> {
        Self::read(
            self.request(Method::POST, "/tokens", Some(token))
                .json(form),
        )
        .await
    }

    async fn delete_token(&self, token: &str, id: &str) -> Result<()> {
        Self::send(self.request(
            Method::DELETE,
            &format!("/tokens/{}", encode_segment(id)),
            Some(token),
        ))
        .await
        .map(|_| ())
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
            &format!("/projects/{}/members", encode_segment(project)),
            Some(token),
        ))
        .await
    }

    async fn remove_member(&self, token: &str, project: &str, handle: &str) -> Result<()> {
        Self::send(self.request(
            Method::DELETE,
            &format!(
                "/projects/{}/members/{}",
                encode_segment(project),
                encode_segment(handle)
            ),
            Some(token),
        ))
        .await
        .map(|_| ())
    }

    async fn audit(&self, token: &str, project: &str) -> Result<Vec<AuditEvent>> {
        Self::read(self.request(
            Method::GET,
            &format!("/projects/{}/audit", encode_segment(project)),
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
                &format!("/projects/{}/invites", encode_segment(project)),
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
            &format!("/projects/{}/captures", encode_segment(project)),
            Some(token),
        ))
        .await
    }

    async fn search_captures(
        &self,
        token: &str,
        project: &str,
        query: Option<String>,
    ) -> Result<CapturePage> {
        let project = encode_segment(project);
        let path = match query.as_deref().filter(|q| !q.is_empty()) {
            Some(q) => format!("/projects/{project}/captures/search?{q}"),
            None => format!("/projects/{project}/captures/search"),
        };
        Self::read(self.request(Method::GET, &path, Some(token))).await
    }

    async fn capture(&self, token: &str, id: &str) -> Result<Capture> {
        Self::read(self.request(
            Method::GET,
            &format!("/captures/{}", encode_segment(id)),
            Some(token),
        ))
        .await
    }

    async fn add_finding(&self, token: &str, capture: &str, form: &NewFinding) -> Result<Finding> {
        Self::read(
            self.request(
                Method::POST,
                &format!("/captures/{}/findings", encode_segment(capture)),
                Some(token),
            )
            .json(form),
        )
        .await
    }

    async fn analyze_capture(
        &self,
        token: &str,
        project: &str,
        capture: &str,
    ) -> Result<Vec<Problem>> {
        Self::read(self.request(
            Method::POST,
            &format!(
                "/projects/{}/captures/{}/analyze",
                encode_segment(project),
                encode_segment(capture)
            ),
            Some(token),
        ))
        .await
    }

    async fn problems(&self, token: &str, project: &str) -> Result<Vec<ProblemSummary>> {
        Self::read(self.request(
            Method::GET,
            &format!("/projects/{}/problems", encode_segment(project)),
            Some(token),
        ))
        .await
    }

    async fn problems_detailed(&self, token: &str, project: &str) -> Result<Vec<Problem>> {
        Self::read(self.request(
            Method::GET,
            &format!("/projects/{}/problems/details", encode_segment(project)),
            Some(token),
        ))
        .await
    }

    async fn problem(&self, token: &str, id: &str) -> Result<Problem> {
        Self::read(self.request(
            Method::GET,
            &format!("/problems/{}", encode_segment(id)),
            Some(token),
        ))
        .await
    }

    async fn create_problem(
        &self,
        token: &str,
        project: &str,
        form: &NewProblem,
    ) -> Result<Problem> {
        Self::read(
            self.request(
                Method::POST,
                &format!("/projects/{}/problems", encode_segment(project)),
                Some(token),
            )
            .json(form),
        )
        .await
    }

    async fn update_problem(&self, token: &str, id: &str, patch: &ProblemPatch) -> Result<Problem> {
        Self::read(
            self.request(
                Method::PATCH,
                &format!("/problems/{}", encode_segment(id)),
                Some(token),
            )
            .json(patch),
        )
        .await
    }

    async fn comment_problem(&self, token: &str, id: &str, form: &NewComment) -> Result<Comment> {
        Self::read(
            self.request(
                Method::POST,
                &format!("/problems/{}/comments", encode_segment(id)),
                Some(token),
            )
            .json(form),
        )
        .await
    }

    async fn design(&self, token: &str, project: &str) -> Result<Vec<DesignEntry>> {
        Self::read(self.request(
            Method::GET,
            &format!("/projects/{}/design", encode_segment(project)),
            Some(token),
        ))
        .await
    }

    async fn add_design(
        &self,
        token: &str,
        project: &str,
        form: &NewDesignEntry,
    ) -> Result<DesignEntry> {
        Self::read(
            self.request(
                Method::POST,
                &format!("/projects/{}/design", encode_segment(project)),
                Some(token),
            )
            .json(form),
        )
        .await
    }

    async fn design_systems(&self, token: &str, project: &str) -> Result<SystemList> {
        Self::read(self.request(
            Method::GET,
            &format!("/projects/{}/design-systems", encode_segment(project)),
            Some(token),
        ))
        .await
    }

    async fn design_system(&self, token: &str, project: &str, id: &str) -> Result<SystemVersion> {
        Self::read(self.request(
            Method::GET,
            &format!(
                "/projects/{}/design-systems/{}",
                encode_segment(project),
                encode_segment(id)
            ),
            Some(token),
        ))
        .await
    }

    async fn create_design_system(&self, token: &str, project: &str) -> Result<SystemVersion> {
        Self::read(self.request(
            Method::POST,
            &format!("/projects/{}/design-systems", encode_segment(project)),
            Some(token),
        ))
        .await
    }

    async fn edit_design_system(
        &self,
        token: &str,
        project: &str,
        id: &str,
        edit: &DocumentEdit,
    ) -> Result<SystemVersion> {
        Self::read(
            self.request(
                Method::PUT,
                &format!(
                    "/projects/{}/design-systems/{}/document",
                    encode_segment(project),
                    encode_segment(id)
                ),
                Some(token),
            )
            .json(edit),
        )
        .await
    }

    async fn act_on_design_system(
        &self,
        token: &str,
        project: &str,
        id: &str,
        form: &SystemActionForm,
    ) -> Result<SystemVersion> {
        Self::read(
            self.request(
                Method::POST,
                &format!(
                    "/projects/{}/design-systems/{}/actions",
                    encode_segment(project),
                    encode_segment(id)
                ),
                Some(token),
            )
            .json(form),
        )
        .await
    }

    async fn fixes(&self, token: &str, project: &str) -> Result<Vec<FixSummary>> {
        Self::read(self.request(
            Method::GET,
            &format!("/projects/{}/fixes", encode_segment(project)),
            Some(token),
        ))
        .await
    }

    async fn fixes_detailed(&self, token: &str, project: &str) -> Result<Vec<Fix>> {
        Self::read(self.request(
            Method::GET,
            &format!("/projects/{}/fixes/details", encode_segment(project)),
            Some(token),
        ))
        .await
    }

    async fn fix(&self, token: &str, id: &str) -> Result<Fix> {
        Self::read(self.request(
            Method::GET,
            &format!("/fixes/{}", encode_segment(id)),
            Some(token),
        ))
        .await
    }

    async fn create_fix(&self, token: &str, project: &str, form: &NewFix) -> Result<Fix> {
        Self::read(
            self.request(
                Method::POST,
                &format!("/projects/{}/fixes", encode_segment(project)),
                Some(token),
            )
            .json(form),
        )
        .await
    }

    async fn jobs(&self, token: &str, project: &str, open_only: bool) -> Result<Vec<Job>> {
        Self::read(self.request(
            Method::GET,
            &format!(
                "/projects/{}/jobs?open={open_only}",
                encode_segment(project)
            ),
            Some(token),
        ))
        .await
    }

    async fn job(&self, token: &str, id: &str) -> Result<Job> {
        Self::read(self.request(
            Method::GET,
            &format!("/jobs/{}", encode_segment(id)),
            Some(token),
        ))
        .await
    }

    async fn create_job(&self, token: &str, project: &str, form: &NewJob) -> Result<Job> {
        Self::read(
            self.request(
                Method::POST,
                &format!("/projects/{}/jobs", encode_segment(project)),
                Some(token),
            )
            .json(form),
        )
        .await
    }

    async fn claim_job(&self, token: &str, project: &str) -> Result<Option<Job>> {
        let response = Self::send(self.request(
            Method::POST,
            &format!("/projects/{}/jobs/claim", encode_segment(project)),
            Some(token),
        ))
        .await?;
        if response.status() == reqwest::StatusCode::NO_CONTENT {
            return Ok(None);
        }
        response
            .json()
            .await
            .map(Some)
            .map_err(|e| ClientError::Malformed(e.to_string()))
    }

    async fn job_progress(&self, token: &str, id: &str, progress: &JobProgress) -> Result<Job> {
        Self::read(
            self.request(
                Method::POST,
                &format!("/jobs/{}/progress", encode_segment(id)),
                Some(token),
            )
            .json(progress),
        )
        .await
    }

    async fn complete_job(&self, token: &str, id: &str, result: &JobResult) -> Result<Job> {
        Self::read(
            self.request(
                Method::POST,
                &format!("/jobs/{}/complete", encode_segment(id)),
                Some(token),
            )
            .json(result),
        )
        .await
    }

    async fn fail_job(&self, token: &str, id: &str, failure: &JobFailure) -> Result<Job> {
        Self::read(
            self.request(
                Method::POST,
                &format!("/jobs/{}/fail", encode_segment(id)),
                Some(token),
            )
            .json(failure),
        )
        .await
    }

    async fn cancel_job(&self, token: &str, id: &str) -> Result<Job> {
        Self::read(self.request(
            Method::POST,
            &format!("/jobs/{}/cancel", encode_segment(id)),
            Some(token),
        ))
        .await
    }

    async fn scenarios(&self, token: &str, project: &str) -> Result<Vec<ScenarioSummary>> {
        Self::read(self.request(
            Method::GET,
            &format!("/projects/{}/scenarios", encode_segment(project)),
            Some(token),
        ))
        .await
    }

    async fn scenario(&self, token: &str, project: &str, name: &str) -> Result<ScenarioVersion> {
        Self::read(self.request(
            Method::GET,
            &format!(
                "/projects/{}/scenarios/{}",
                encode_segment(project),
                encode_segment(name)
            ),
            Some(token),
        ))
        .await
    }

    async fn push_scenario(
        &self,
        token: &str,
        project: &str,
        scenario: &Scenario,
    ) -> Result<ScenarioVersion> {
        Self::read(
            self.request(
                Method::POST,
                &format!("/projects/{}/scenarios", encode_segment(project)),
                Some(token),
            )
            .json(scenario),
        )
        .await
    }

    async fn add_step(&self, token: &str, id: &str, form: &NewStep) -> Result<Step> {
        Self::read(
            self.request(
                Method::POST,
                &format!("/fixes/{}/steps", encode_segment(id)),
                Some(token),
            )
            .json(form),
        )
        .await
    }

    async fn review_fix(&self, token: &str, id: &str, form: &NewReview) -> Result<Fix> {
        Self::read(
            self.request(
                Method::POST,
                &format!("/fixes/{}/reviews", encode_segment(id)),
                Some(token),
            )
            .json(form),
        )
        .await
    }

    async fn artifact(&self, token: &str, id: &str, range: Option<String>) -> Result<ArtifactBody> {
        let mut request = self
            .request(
                Method::GET,
                &format!("/artifacts/{}", encode_segment(id)),
                Some(token),
            )
            .timeout(LONG_TIMEOUT);
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

    async fn missing_blobs(
        &self,
        token: &str,
        project: &str,
        query: &MissingQuery,
    ) -> Result<Missing> {
        Self::read(
            self.request(
                Method::POST,
                &format!("/projects/{}/blobs/missing", encode_segment(project)),
                Some(token),
            )
            .json(query),
        )
        .await
    }

    async fn upload_project_blob(
        &self,
        token: &str,
        project: &str,
        sha256: &str,
        data: Vec<u8>,
    ) -> Result<()> {
        let path = format!(
            "/projects/{}/blobs/{}",
            encode_segment(project),
            encode_segment(sha256)
        );
        Self::send(
            self.request(Method::PUT, &path, Some(token))
                .timeout(LONG_TIMEOUT)
                .header(header::CONTENT_TYPE, OCTET_STREAM)
                .body(data),
        )
        .await?;
        Ok(())
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
                &format!("/projects/{}/captures", encode_segment(project)),
                Some(token),
            )
            .json(capture),
        )
        .await
    }

    async fn run_capture(
        &self,
        token: &str,
        project: &str,
        capture: &RunCapture,
    ) -> Result<Capture> {
        Self::read(
            self.request(
                Method::POST,
                &format!("/projects/{}/captures/run", encode_segment(project)),
                Some(token),
            )
            .timeout(LONG_TIMEOUT)
            .json(capture),
        )
        .await
    }

    async fn mcp(&self, token: &str, payload: &str) -> Result<McpReply> {
        let response = self
            .client
            .post(format!("{}{MCP_PATH}", self.base))
            .timeout(LONG_TIMEOUT)
            .bearer_auth(token)
            .header(header::CONTENT_TYPE, JSON)
            .body(payload.to_owned())
            .send()
            .await
            .map_err(|e| ClientError::Unavailable(e.to_string()))?;
        let status = response.status().as_u16();
        let body = response
            .text()
            .await
            .map_err(|e| ClientError::Unavailable(e.to_string()))?;
        Ok(McpReply { status, body })
    }
}

#[cfg(test)]
mod tests {
    use super::encode_segment;

    #[test]
    fn encode_segment_escapes_delimiters_and_dot_segments() {
        assert_eq!(encode_segment("demo-project_1.0~a"), "demo-project_1.0~a");
        assert_eq!(encode_segment(".."), "%2E%2E");
        assert_eq!(encode_segment("."), "%2E");
        assert_eq!(encode_segment("../etc/passwd"), "..%2Fetc%2Fpasswd");
        assert_eq!(encode_segment("a/b?c#d%e"), "a%2Fb%3Fc%23d%25e");
    }
}
