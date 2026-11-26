mod transport;

pub use transport::HttpApi;

use self::transport::{JSON, LONG_TIMEOUT, MCP_PATH, OCTET_STREAM, encode_segment};
use crate::{Api, ArtifactBody, ClientError, McpReply, Result};
use async_trait::async_trait;
use reqwest::{Method, header};
use rupicola_api::{
    AuditEvent, Capture, CaptureCapabilities, CaptureDiff, CapturePage, CaptureRemoval,
    CaptureSummary, Citations, Comment, Credential, Credentials, DesignEntry, DocumentEdit,
    Finding, Fix, FixSelectionLink, FixSummary, IssuedCredential, IssuedInvite, Job, JobFailure,
    JobProgress, JobResult, LimitSettings, LimitValue, Limits, Lineage, LineageGraph, Me, Member,
    Missing, MissingQuery, NewComment, NewCredential, NewDesignEntry, NewFinding, NewFix,
    NewInvite, NewJob, NewProblem, NewProject, NewReview, NewSelection, NewStep, NewTopic,
    ParentsEdit, PasswordChange, PolicyView, Problem, ProblemPatch, ProblemSummary, Project,
    ProjectUpdate, RecaptureSettings, RecaptureSettingsPatch, Redeem, Register, Scenario,
    ScenarioSummary, ScenarioVersion, Selection, SessionToken, Setup, SetupStatus, Step,
    SystemActionForm, SystemList, SystemVersion, Topic, TopicMove, TopicRename, TopicTree,
};

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
        self.get_json(token, "/me").await
    }

    async fn projects(&self, token: &str) -> Result<Vec<Project>> {
        self.get_json(token, "/projects").await
    }

    async fn tokens(&self, token: &str) -> Result<Vec<Credential>> {
        self.get_json(token, "/tokens").await
    }

    async fn create_token(&self, token: &str, form: &NewCredential) -> Result<IssuedCredential> {
        self.post_json(token, "/tokens", form).await
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
        self.post_json(token, "/projects", form).await
    }

    async fn members(&self, token: &str, project: &str) -> Result<Vec<Member>> {
        self.get_json(
            token,
            &format!("/projects/{}/members", encode_segment(project)),
        )
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
        self.get_json(
            token,
            &format!("/projects/{}/audit", encode_segment(project)),
        )
        .await
    }

    async fn create_invite(
        &self,
        token: &str,
        project: &str,
        form: &NewInvite,
    ) -> Result<IssuedInvite> {
        self.post_json(
            token,
            &format!("/projects/{}/invites", encode_segment(project)),
            form,
        )
        .await
    }

    async fn redeem_invite(&self, token: &str, form: &Redeem) -> Result<Project> {
        self.post_json(token, "/invites/redeem", form).await
    }

    async fn captures(&self, token: &str, project: &str) -> Result<Vec<CaptureSummary>> {
        self.get_json(
            token,
            &format!("/projects/{}/captures", encode_segment(project)),
        )
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
        self.get_json(token, &path).await
    }

    async fn capture(&self, token: &str, id: &str) -> Result<Capture> {
        self.get_json(token, &format!("/captures/{}", encode_segment(id)))
            .await
    }

    async fn add_finding(&self, token: &str, capture: &str, form: &NewFinding) -> Result<Finding> {
        self.post_json(
            token,
            &format!("/captures/{}/findings", encode_segment(capture)),
            form,
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
        self.get_json(
            token,
            &format!("/projects/{}/problems", encode_segment(project)),
        )
        .await
    }

    async fn problems_detailed(&self, token: &str, project: &str) -> Result<Vec<Problem>> {
        self.get_json(
            token,
            &format!("/projects/{}/problems/details", encode_segment(project)),
        )
        .await
    }

    async fn problem(&self, token: &str, id: &str) -> Result<Problem> {
        self.get_json(token, &format!("/problems/{}", encode_segment(id)))
            .await
    }

    async fn create_problem(
        &self,
        token: &str,
        project: &str,
        form: &NewProblem,
    ) -> Result<Problem> {
        self.post_json(
            token,
            &format!("/projects/{}/problems", encode_segment(project)),
            form,
        )
        .await
    }

    async fn update_problem(&self, token: &str, id: &str, patch: &ProblemPatch) -> Result<Problem> {
        self.patch_json(token, &format!("/problems/{}", encode_segment(id)), patch)
            .await
    }

    async fn comment_problem(&self, token: &str, id: &str, form: &NewComment) -> Result<Comment> {
        self.post_json(
            token,
            &format!("/problems/{}/comments", encode_segment(id)),
            form,
        )
        .await
    }

    async fn design(&self, token: &str, project: &str) -> Result<Vec<DesignEntry>> {
        self.get_json(
            token,
            &format!("/projects/{}/design", encode_segment(project)),
        )
        .await
    }

    async fn add_design(
        &self,
        token: &str,
        project: &str,
        form: &NewDesignEntry,
    ) -> Result<DesignEntry> {
        self.post_json(
            token,
            &format!("/projects/{}/design", encode_segment(project)),
            form,
        )
        .await
    }

    async fn design_systems(&self, token: &str, project: &str) -> Result<SystemList> {
        self.get_json(
            token,
            &format!("/projects/{}/design-systems", encode_segment(project)),
        )
        .await
    }

    async fn design_system(&self, token: &str, project: &str, id: &str) -> Result<SystemVersion> {
        self.get_json(
            token,
            &format!(
                "/projects/{}/design-systems/{}",
                encode_segment(project),
                encode_segment(id)
            ),
        )
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
        self.put_json(
            token,
            &format!(
                "/projects/{}/design-systems/{}/document",
                encode_segment(project),
                encode_segment(id)
            ),
            edit,
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
        self.post_json(
            token,
            &format!(
                "/projects/{}/design-systems/{}/actions",
                encode_segment(project),
                encode_segment(id)
            ),
            form,
        )
        .await
    }

    async fn fixes(&self, token: &str, project: &str) -> Result<Vec<FixSummary>> {
        self.get_json(
            token,
            &format!("/projects/{}/fixes", encode_segment(project)),
        )
        .await
    }

    async fn fixes_detailed(&self, token: &str, project: &str) -> Result<Vec<Fix>> {
        self.get_json(
            token,
            &format!("/projects/{}/fixes/details", encode_segment(project)),
        )
        .await
    }

    async fn fix(&self, token: &str, id: &str) -> Result<Fix> {
        self.get_json(token, &format!("/fixes/{}", encode_segment(id)))
            .await
    }

    async fn create_fix(&self, token: &str, project: &str, form: &NewFix) -> Result<Fix> {
        self.post_json(
            token,
            &format!("/projects/{}/fixes", encode_segment(project)),
            form,
        )
        .await
    }

    async fn jobs(&self, token: &str, project: &str, open_only: bool) -> Result<Vec<Job>> {
        self.get_json(
            token,
            &format!(
                "/projects/{}/jobs?open={open_only}",
                encode_segment(project)
            ),
        )
        .await
    }

    async fn job(&self, token: &str, id: &str) -> Result<Job> {
        self.get_json(token, &format!("/jobs/{}", encode_segment(id)))
            .await
    }

    async fn create_job(&self, token: &str, project: &str, form: &NewJob) -> Result<Job> {
        self.post_json(
            token,
            &format!("/projects/{}/jobs", encode_segment(project)),
            form,
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
        self.post_json(
            token,
            &format!("/jobs/{}/progress", encode_segment(id)),
            progress,
        )
        .await
    }

    async fn complete_job(&self, token: &str, id: &str, result: &JobResult) -> Result<Job> {
        self.post_json(
            token,
            &format!("/jobs/{}/complete", encode_segment(id)),
            result,
        )
        .await
    }

    async fn fail_job(&self, token: &str, id: &str, failure: &JobFailure) -> Result<Job> {
        self.post_json(
            token,
            &format!("/jobs/{}/fail", encode_segment(id)),
            failure,
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

    async fn capture_settings(&self, token: &str, project: &str) -> Result<RecaptureSettings> {
        self.get_json(
            token,
            &format!("/projects/{}/capture-settings", encode_segment(project)),
        )
        .await
    }

    async fn update_capture_settings(
        &self,
        token: &str,
        project: &str,
        patch: &RecaptureSettingsPatch,
    ) -> Result<RecaptureSettings> {
        self.patch_json(
            token,
            &format!("/projects/{}/capture-settings", encode_segment(project)),
            patch,
        )
        .await
    }

    async fn scenarios(&self, token: &str, project: &str) -> Result<Vec<ScenarioSummary>> {
        self.get_json(
            token,
            &format!("/projects/{}/scenarios", encode_segment(project)),
        )
        .await
    }

    async fn scenario(&self, token: &str, project: &str, name: &str) -> Result<ScenarioVersion> {
        self.get_json(
            token,
            &format!(
                "/projects/{}/scenarios/{}",
                encode_segment(project),
                encode_segment(name)
            ),
        )
        .await
    }

    async fn scenario_version(
        &self,
        token: &str,
        project: &str,
        name: &str,
        hash: &str,
    ) -> Result<ScenarioVersion> {
        self.get_json(
            token,
            &format!(
                "/projects/{}/scenarios/{}/{}",
                encode_segment(project),
                encode_segment(name),
                encode_segment(hash)
            ),
        )
        .await
    }

    async fn push_scenario(
        &self,
        token: &str,
        project: &str,
        scenario: &Scenario,
    ) -> Result<ScenarioVersion> {
        self.post_json(
            token,
            &format!("/projects/{}/scenarios", encode_segment(project)),
            scenario,
        )
        .await
    }

    async fn publish_record(
        &self,
        token: &str,
        project: &str,
        id: &str,
        record: &rupicola_api::record::CaptureRecord,
    ) -> Result<Capture> {
        self.put_json(
            token,
            &format!(
                "/projects/{}/captures/{}",
                encode_segment(project),
                encode_segment(id)
            ),
            record,
        )
        .await
    }

    async fn add_step(&self, token: &str, id: &str, form: &NewStep) -> Result<Step> {
        self.post_json(token, &format!("/fixes/{}/steps", encode_segment(id)), form)
            .await
    }

    async fn review_fix(&self, token: &str, id: &str, form: &NewReview) -> Result<Fix> {
        self.post_json(
            token,
            &format!("/fixes/{}/reviews", encode_segment(id)),
            form,
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
        self.post_json(
            token,
            &format!("/projects/{}/blobs/missing", encode_segment(project)),
            query,
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

    async fn delete_problem(&self, token: &str, id: &str) -> Result<()> {
        self.delete_empty(token, &format!("/problems/{}", encode_segment(id)))
            .await
    }

    async fn project(&self, token: &str, id: &str) -> Result<Project> {
        self.get_json(token, &format!("/projects/{}", encode_segment(id)))
            .await
    }

    async fn update_project(&self, token: &str, id: &str, form: &ProjectUpdate) -> Result<Project> {
        self.patch_json(token, &format!("/projects/{}", encode_segment(id)), form)
            .await
    }

    async fn delete_project(&self, token: &str, id: &str) -> Result<()> {
        self.delete_empty(token, &format!("/projects/{}", encode_segment(id)))
            .await
    }

    async fn project_limits(&self, token: &str, project: &str) -> Result<Limits> {
        self.get_json(
            token,
            &format!("/projects/{}/limits", encode_segment(project)),
        )
        .await
    }

    async fn capture_capabilities(&self, token: &str) -> Result<CaptureCapabilities> {
        self.get_json(token, "/capture-capabilities").await
    }

    async fn capture_citations(&self, token: &str, id: &str) -> Result<Citations> {
        self.get_json(
            token,
            &format!("/captures/{}/citations", encode_segment(id)),
        )
        .await
    }

    async fn delete_capture(&self, token: &str, id: &str, form: &CaptureRemoval) -> Result<()> {
        Self::send(
            self.request(
                Method::DELETE,
                &format!("/captures/{}", encode_segment(id)),
                Some(token),
            )
            .json(form),
        )
        .await?;
        Ok(())
    }

    async fn recapture_capture(
        &self,
        token: &str,
        id: &str,
        form: &rupicola_api::RecaptureRequest,
    ) -> Result<Job> {
        self.post_json(
            token,
            &format!("/captures/{}/recapture", encode_segment(id)),
            form,
        )
        .await
    }

    async fn capture_lineage(
        &self,
        token: &str,
        id: &str,
        depth: Option<u32>,
    ) -> Result<LineageGraph> {
        let id = encode_segment(id);
        let path = match depth {
            Some(d) => format!("/captures/{id}/lineage?depth={d}"),
            None => format!("/captures/{id}/lineage"),
        };
        self.get_json(token, &path).await
    }

    async fn capture_parents(&self, token: &str, id: &str) -> Result<Lineage> {
        self.get_json(token, &format!("/captures/{}/parents", encode_segment(id)))
            .await
    }

    async fn edit_capture_parents(
        &self,
        token: &str,
        id: &str,
        form: &ParentsEdit,
    ) -> Result<Lineage> {
        self.put_json(
            token,
            &format!("/captures/{}/parents", encode_segment(id)),
            form,
        )
        .await
    }

    async fn compare_captures(
        &self,
        token: &str,
        before: &str,
        after: &str,
    ) -> Result<CaptureDiff> {
        self.get_json(
            token,
            &format!(
                "/captures/{}/compare/{}",
                encode_segment(before),
                encode_segment(after)
            ),
        )
        .await
    }

    async fn cite_problem_selection(
        &self,
        token: &str,
        id: &str,
        selection: &str,
    ) -> Result<Problem> {
        Self::read(self.request(
            Method::PUT,
            &format!(
                "/problems/{}/selections/{}",
                encode_segment(id),
                encode_segment(selection)
            ),
            Some(token),
        ))
        .await
    }

    async fn uncite_problem_selection(
        &self,
        token: &str,
        id: &str,
        selection: &str,
    ) -> Result<Problem> {
        self.delete_json(
            token,
            &format!(
                "/problems/{}/selections/{}",
                encode_segment(id),
                encode_segment(selection)
            ),
        )
        .await
    }

    async fn cite_fix_selection(
        &self,
        token: &str,
        id: &str,
        selection: &str,
        link: &FixSelectionLink,
    ) -> Result<Fix> {
        self.put_json(
            token,
            &format!(
                "/fixes/{}/selections/{}",
                encode_segment(id),
                encode_segment(selection)
            ),
            link,
        )
        .await
    }

    async fn uncite_fix_selection(&self, token: &str, id: &str, selection: &str) -> Result<Fix> {
        self.delete_json(
            token,
            &format!(
                "/fixes/{}/selections/{}",
                encode_segment(id),
                encode_segment(selection)
            ),
        )
        .await
    }

    async fn create_selection(
        &self,
        token: &str,
        project: &str,
        form: &NewSelection,
    ) -> Result<Selection> {
        self.post_json(
            token,
            &format!("/projects/{}/selections", encode_segment(project)),
            form,
        )
        .await
    }

    async fn selection(&self, token: &str, id: &str) -> Result<Selection> {
        self.get_json(token, &format!("/selections/{}", encode_segment(id)))
            .await
    }

    async fn topic_tree(&self, token: &str, project: &str) -> Result<TopicTree> {
        self.get_json(
            token,
            &format!("/projects/{}/topics", encode_segment(project)),
        )
        .await
    }

    async fn create_topic(&self, token: &str, project: &str, form: &NewTopic) -> Result<Topic> {
        self.post_json(
            token,
            &format!("/projects/{}/topics", encode_segment(project)),
            form,
        )
        .await
    }

    async fn rename_topic(&self, token: &str, id: &str, form: &TopicRename) -> Result<Topic> {
        self.put_json(token, &format!("/topics/{}/name", encode_segment(id)), form)
            .await
    }

    async fn move_topic(&self, token: &str, id: &str, form: &TopicMove) -> Result<Topic> {
        self.put_json(
            token,
            &format!("/topics/{}/parent", encode_segment(id)),
            form,
        )
        .await
    }

    async fn delete_topic(&self, token: &str, id: &str, recursive: bool) -> Result<()> {
        self.delete_empty(
            token,
            &format!("/topics/{}?recursive={recursive}", encode_segment(id)),
        )
        .await
    }

    async fn topic_captures(
        &self,
        token: &str,
        id: &str,
        direct: bool,
    ) -> Result<Vec<CaptureSummary>> {
        self.get_json(
            token,
            &format!("/topics/{}/captures?direct={direct}", encode_segment(id)),
        )
        .await
    }

    async fn file_topic_capture(&self, token: &str, id: &str, capture: &str) -> Result<()> {
        self.put_empty(
            token,
            &format!(
                "/topics/{}/captures/{}",
                encode_segment(id),
                encode_segment(capture)
            ),
        )
        .await
    }

    async fn unfile_topic_capture(&self, token: &str, id: &str, capture: &str) -> Result<()> {
        self.delete_empty(
            token,
            &format!(
                "/topics/{}/captures/{}",
                encode_segment(id),
                encode_segment(capture)
            ),
        )
        .await
    }

    async fn list_devices(&self, token: &str, project: &str) -> Result<Vec<Credential>> {
        self.get_json(
            token,
            &format!("/projects/{}/devices", encode_segment(project)),
        )
        .await
    }

    async fn create_device(
        &self,
        token: &str,
        project: &str,
        form: &NewCredential,
    ) -> Result<IssuedCredential> {
        self.post_json(
            token,
            &format!("/projects/{}/devices", encode_segment(project)),
            form,
        )
        .await
    }

    async fn delete_device(&self, token: &str, project: &str, device: &str) -> Result<()> {
        self.delete_empty(
            token,
            &format!(
                "/projects/{}/devices/{}",
                encode_segment(project),
                encode_segment(device)
            ),
        )
        .await
    }

    async fn deletion_policy(&self, token: &str, project: &str) -> Result<PolicyView> {
        self.get_json(
            token,
            &format!("/projects/{}/deletion-policy", encode_segment(project)),
        )
        .await
    }

    async fn set_deletion_policy(
        &self,
        token: &str,
        project: &str,
        form: &PolicyView,
    ) -> Result<PolicyView> {
        self.put_json(
            token,
            &format!("/projects/{}/deletion-policy", encode_segment(project)),
            form,
        )
        .await
    }

    async fn project_settings(&self, token: &str, project: &str) -> Result<LimitSettings> {
        self.get_json(
            token,
            &format!("/projects/{}/settings", encode_segment(project)),
        )
        .await
    }

    async fn set_project_setting(
        &self,
        token: &str,
        project: &str,
        key: &str,
        value: &LimitValue,
    ) -> Result<LimitSettings> {
        self.put_json(
            token,
            &format!(
                "/projects/{}/settings/{}",
                encode_segment(project),
                encode_segment(key)
            ),
            value,
        )
        .await
    }

    async fn reset_project_setting(
        &self,
        token: &str,
        project: &str,
        key: &str,
    ) -> Result<LimitSettings> {
        self.delete_json(
            token,
            &format!(
                "/projects/{}/settings/{}",
                encode_segment(project),
                encode_segment(key)
            ),
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
