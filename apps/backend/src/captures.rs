//! Capture upload, browsing and review, and artifact downloads.
//!
//! Uploading is two steps: each file goes to `PUT /v1/blobs/{sha256}` (skipped when
//! `HEAD` says it is already stored), then `POST /v1/projects/{id}/captures`
//! describes the capture by referencing those digests.

use crate::{
    auth::{Principal, now_ms},
    blobs,
    error::ApiError,
    projects::{Access, access},
    state::Shared,
};
use axum::{
    Json,
    body::Bytes,
    extract::{Path, Request, State},
    http::StatusCode,
    response::Response,
};
use rupicola_api::{
    Artifact, BlobRef, Capture, CaptureStatus, CaptureSummary, Checkpoint, Clip, Finding, Frame,
    Inspection, Missing, MissingQuery, NewCapture, NewFinding, Role,
};
use std::collections::HashSet;

/// Longest file path inside a capture, and deepest nesting (slashes).
const MAX_PATH_LEN: usize = 180;
const MAX_PATH_DEPTH: usize = 4;
/// Longest checkpoint or clip id.
const MAX_NAME_LEN: usize = 128;
/// Longest scenario name.
const MAX_SCENARIO_CHARS: usize = 512;
/// Longest review note.
const MAX_NOTE_CHARS: usize = 8000;

/// `PUT /v1/blobs/{sha256}`: stores a file under its digest. Any signed-in caller
/// may upload; blobs nobody references are pruned later.
pub async fn put_blob(
    State(state): State<Shared>,
    _principal: Principal,
    Path(sha256): Path<String>,
    data: Bytes,
) -> Result<StatusCode, ApiError> {
    let store = state.clone();
    let created = tokio::task::spawn_blocking(move || store.blobs.put(&sha256, &data))
        .await
        .map_err(anyhow::Error::from)??;
    Ok(if created {
        StatusCode::CREATED
    } else {
        StatusCode::OK
    })
}

/// `POST /v1/projects/{id}/blobs/missing`: which of these files the project does not have
/// yet, so a client uploads only those, with the limits to upload them under.
pub async fn missing(
    State(state): State<Shared>,
    principal: Principal,
    Path(project): Path<String>,
    Json(query): Json<MissingQuery>,
) -> Result<Json<Missing>, ApiError> {
    match access(&state, &principal, &project).await? {
        Access::Device => {}
        person @ Access::Person(_) => person.require(Role::Reviewer)?,
    }
    query.validate().map_err(ApiError::invalid)?;
    Ok(Json(Missing {
        missing: blobs::BlobIndex::missing(&state.db, &project, &query.sha256).await?,
        limits: state.limits.effective(&state, &project).await?,
    }))
}

/// `HEAD /v1/blobs/{sha256}`: 200 when stored, so clients skip the upload.
pub async fn head_blob(
    State(state): State<Shared>,
    _principal: Principal,
    Path(sha256): Path<String>,
) -> Result<StatusCode, ApiError> {
    match state.blobs.size(&sha256).map_err(anyhow::Error::from)? {
        Some(_) => Ok(StatusCode::OK),
        None => Err(ApiError::NotFound),
    }
}

/// Paths inside a capture: short, relative, plain names with a media extension.
fn valid_path(path: &str) -> bool {
    path.len() <= MAX_PATH_LEN
        && path.matches('/').count() <= MAX_PATH_DEPTH
        && matches!(
            std::path::Path::new(path)
                .extension()
                .and_then(|ext| ext.to_str()),
            Some("png" | "gif" | "mp4" | "webm" | "json")
        )
        && path.split('/').all(|part| {
            !part.is_empty()
                && !part.starts_with('.')
                && part
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
        })
}

fn valid_name(name: &str) -> bool {
    (1..=MAX_NAME_LEN).contains(&name.len())
}

struct Planned<'a> {
    role: &'static str,
    owner: &'a str,
    file: &'a BlobRef,
    target_ms: Option<f64>,
    video_ms: Option<f64>,
}

fn plan(form: &NewCapture, max_artifacts: usize) -> Result<Vec<Planned<'_>>, ApiError> {
    let mut planned = Vec::new();
    for checkpoint in &form.checkpoints {
        planned.push(Planned {
            role: "checkpoint",
            owner: &checkpoint.id,
            file: &checkpoint.file,
            target_ms: None,
            video_ms: None,
        });
    }
    for clip in &form.clips {
        for (role, file) in [
            ("clip-video", &clip.video),
            ("clip-sheet", &clip.sheet),
            ("clip-analysis", &clip.analysis),
        ] {
            if let Some(file) = file {
                planned.push(Planned {
                    role,
                    owner: &clip.id,
                    file,
                    target_ms: None,
                    video_ms: None,
                });
            }
        }
        for frame in &clip.frames {
            planned.push(Planned {
                role: "clip-frame",
                owner: &clip.id,
                file: &frame.file,
                target_ms: Some(frame.target_ms),
                video_ms: frame.video_ms,
            });
        }
    }
    let mut paths = HashSet::new();
    if planned.len() > max_artifacts
        || planned.iter().any(|item| {
            !valid_name(item.owner)
                || !valid_path(&item.file.path)
                || !blobs::valid_digest(&item.file.sha256)
                || !paths.insert(item.file.path.as_str())
        })
    {
        return Err(ApiError::invalid(format!(
            "a capture needs at most {max_artifacts} files with unique, plain relative paths \
                 and valid SHA-256 digests"
        )));
    }
    Ok(planned)
}

/// `POST /v1/projects/{id}/captures`: devices and reviewers publish captures.
pub async fn create(
    State(state): State<Shared>,
    principal: Principal,
    Path(project): Path<String>,
    Json(form): Json<NewCapture>,
) -> Result<(StatusCode, Json<Capture>), ApiError> {
    match access(&state, &principal, &project).await? {
        Access::Device => {}
        person @ Access::Person(_) => person.require(Role::Reviewer)?,
    }
    if !(1..=MAX_SCENARIO_CHARS).contains(&form.scenario.chars().count()) {
        return Err(ApiError::invalid(format!(
            "the scenario must have 1–{MAX_SCENARIO_CHARS} characters"
        )));
    }
    let limits = state.limits.effective(&state, &project).await?;
    let max_artifacts = usize::try_from(limits.max_artifacts_per_capture).unwrap_or(usize::MAX);
    let planned = plan(&form, max_artifacts)?;
    let mut sizes = Vec::with_capacity(planned.len());
    for item in &planned {
        let size = state
            .blobs
            .size(&item.file.sha256)
            .map_err(anyhow::Error::from)?
            .ok_or(ApiError::invalid(
                "upload every file with PUT /v1/blobs/{sha256} first",
            ))?;
        sizes.push(size);
    }
    let id = uuid::Uuid::now_v7().to_string();
    let mut tx = state.db.begin().await?;
    sqlx::query(
        "INSERT INTO captures (id, project_id, scenario, revision, status, blocker, inspection,
           environment, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'pending', ?7, ?8)",
    )
    .bind(&id)
    .bind(&project)
    .bind(&form.scenario)
    .bind(&form.revision)
    .bind(status_text(form.status))
    .bind(&form.blocker)
    .bind(serde_json::to_string(&form.environment).map_err(anyhow::Error::from)?)
    .bind(now_ms())
    .execute(&mut *tx)
    .await?;
    for (position, (item, size)) in planned.iter().zip(&sizes).enumerate() {
        sqlx::query(
            "INSERT INTO artifacts (id, capture_id, role, owner_id, position, path, sha256, bytes,
               mime, target_ms, video_ms)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
        )
        .bind(uuid::Uuid::now_v7().to_string())
        .bind(&id)
        .bind(item.role)
        .bind(item.owner)
        .bind(i64::try_from(position).map_err(anyhow::Error::from)?)
        .bind(&item.file.path)
        .bind(&item.file.sha256)
        .bind(i64::try_from(*size).map_err(anyhow::Error::from)?)
        .bind(blobs::mime(&item.file.path))
        .bind(item.target_ms)
        .bind(item.video_ms)
        .execute(&mut *tx)
        .await?;
        // The project now cites the file, so it is no longer missing for it.
        blobs::BlobIndex::record(&mut tx, &project, &item.file.sha256, *size).await?;
    }
    let actor = principal.handle.as_deref().unwrap_or("device");
    crate::projects::audit(
        &mut *tx,
        Some(&project),
        actor,
        &format!("Published capture {}", form.scenario),
    )
    .await?;
    tx.commit().await?;
    Ok((StatusCode::CREATED, Json(load(&state, &id).await?)))
}

const fn status_text(status: CaptureStatus) -> &'static str {
    match status {
        CaptureStatus::Complete => "complete",
        CaptureStatus::Blocked => "blocked",
    }
}

/// `GET /v1/projects/{id}/captures`
pub async fn list(
    State(state): State<Shared>,
    principal: Principal,
    Path(project): Path<String>,
) -> Result<Json<Vec<CaptureSummary>>, ApiError> {
    access(&state, &principal, &project)
        .await?
        .require(Role::Reader)?;
    let rows: Vec<SummaryRow> = sqlx::query_as(
        "SELECT c.id, c.scenario, c.status, c.inspection, c.created_at,
           (SELECT a.id FROM artifacts a WHERE a.capture_id = c.id AND a.role = 'checkpoint'
            ORDER BY a.position LIMIT 1) AS preview
         FROM captures c WHERE c.project_id = ?1 ORDER BY c.created_at DESC, c.id DESC",
    )
    .bind(&project)
    .fetch_all(&state.db)
    .await?;
    Ok(Json(rows.into_iter().map(CaptureSummary::from).collect()))
}

/// Project of a capture, checked against the caller's access.
async fn capture_access(
    state: &Shared,
    principal: &Principal,
    capture: &str,
    role: Role,
) -> Result<(), ApiError> {
    let project: Option<String> =
        sqlx::query_scalar("SELECT project_id FROM captures WHERE id = ?1")
            .bind(capture)
            .fetch_optional(&state.db)
            .await?;
    let project = project.ok_or(ApiError::NotFound)?;
    access(state, principal, &project).await?.require(role)
}

/// Fails unless `artifact` is a file of one of `project`'s captures.
pub async fn require_project_artifact(
    state: &Shared,
    project: &str,
    artifact: &str,
) -> Result<(), ApiError> {
    let belongs: Option<i64> = sqlx::query_scalar(
        "SELECT 1 FROM artifacts a JOIN captures c ON c.id = a.capture_id
         WHERE a.id = ?1 AND c.project_id = ?2",
    )
    .bind(artifact)
    .bind(project)
    .fetch_optional(&state.db)
    .await?;
    belongs.map(|_| ()).ok_or(ApiError::invalid(
        "the evidence must be files of this project's captures",
    ))
}

/// `GET /v1/captures/{id}`
pub async fn get(
    State(state): State<Shared>,
    principal: Principal,
    Path(id): Path<String>,
) -> Result<Json<Capture>, ApiError> {
    capture_access(&state, &principal, &id, Role::Reader).await?;
    Ok(Json(load(&state, &id).await?))
}

/// A `captures` row as listed, with its first checkpoint as preview.
#[derive(sqlx::FromRow)]
struct SummaryRow {
    id: String,
    scenario: String,
    status: String,
    inspection: String,
    created_at: i64,
    preview: Option<String>,
}

impl From<SummaryRow> for CaptureSummary {
    fn from(row: SummaryRow) -> Self {
        Self {
            id: row.id,
            scenario: row.scenario,
            status: row.status.parse().unwrap_or(CaptureStatus::Complete),
            inspection: row.inspection.parse().unwrap_or(Inspection::Pending),
            created_at: row.created_at,
            preview: row.preview,
        }
    }
}

/// A `captures` row.
#[derive(sqlx::FromRow)]
struct CaptureRow {
    project_id: String,
    scenario: String,
    revision: Option<String>,
    status: String,
    blocker: Option<String>,
    inspection: String,
    environment: String,
    created_at: i64,
}

/// A `capture_findings` row.
#[derive(sqlx::FromRow)]
struct FindingRow {
    id: String,
    note: String,
    evidence_artifact_id: Option<String>,
    author: Option<String>,
    created_at: i64,
}

impl From<FindingRow> for Finding {
    fn from(row: FindingRow) -> Self {
        Self {
            id: row.id,
            note: row.note,
            evidence_artifact_id: row.evidence_artifact_id,
            author: row.author,
            created_at: row.created_at,
        }
    }
}

/// An `artifacts` row.
#[derive(sqlx::FromRow)]
struct ArtifactRow {
    id: String,
    role: String,
    owner_id: String,
    path: String,
    sha256: String,
    bytes: i64,
    mime: String,
    target_ms: Option<f64>,
    video_ms: Option<f64>,
}

/// Project, digest and media type of an artifact.
#[derive(sqlx::FromRow)]
struct BlobRow {
    project_id: String,
    sha256: String,
    mime: String,
    bytes: i64,
}

async fn load(state: &Shared, id: &str) -> Result<Capture, ApiError> {
    let capture: CaptureRow = sqlx::query_as(
        "SELECT project_id, scenario, revision, status, blocker, inspection, environment,
               created_at FROM captures WHERE id = ?1",
    )
    .bind(id)
    .fetch_optional(&state.db)
    .await?
    .ok_or(ApiError::NotFound)?;
    let artifacts: Vec<ArtifactRow> = sqlx::query_as(
        "SELECT id, role, owner_id, path, sha256, bytes, mime, target_ms, video_ms
         FROM artifacts WHERE capture_id = ?1 ORDER BY position",
    )
    .bind(id)
    .fetch_all(&state.db)
    .await?;
    let mut checkpoints = Vec::new();
    let mut clips: Vec<Clip> = Vec::new();
    for row in artifacts {
        let (role, owner, target_ms, video_ms) =
            (row.role, row.owner_id, row.target_ms, row.video_ms);
        let artifact = Artifact {
            id: row.id,
            path: row.path,
            mime: row.mime,
            bytes: u64::try_from(row.bytes).unwrap_or_default(),
            sha256: row.sha256,
        };
        if role == "checkpoint" {
            checkpoints.push(Checkpoint {
                id: owner,
                artifact,
            });
            continue;
        }
        let index = if let Some(index) = clips.iter().position(|clip| clip.id == owner) {
            index
        } else {
            clips.push(Clip {
                id: owner,
                video: None,
                sheet: None,
                analysis: None,
                frames: Vec::new(),
            });
            clips.len() - 1
        };
        let clip = &mut clips[index];
        match role.as_str() {
            "clip-video" => clip.video = Some(artifact),
            "clip-sheet" => clip.sheet = Some(artifact),
            "clip-analysis" => clip.analysis = Some(artifact),
            _ => clip.frames.push(Frame {
                artifact,
                target_ms: target_ms.unwrap_or_default(),
                video_ms,
            }),
        }
    }
    let findings: Vec<FindingRow> = sqlx::query_as(
        "SELECT id, note, evidence_artifact_id, author, created_at FROM capture_findings
         WHERE capture_id = ?1 ORDER BY created_at, id",
    )
    .bind(id)
    .fetch_all(&state.db)
    .await?;
    Ok(Capture {
        id: id.to_owned(),
        project_id: capture.project_id,
        scenario: capture.scenario,
        revision: capture.revision,
        status: capture.status.parse().unwrap_or(CaptureStatus::Complete),
        blocker: capture.blocker,
        inspection: capture.inspection.parse().unwrap_or(Inspection::Pending),
        environment: serde_json::from_str(&capture.environment).map_err(anyhow::Error::from)?,
        created_at: capture.created_at,
        checkpoints,
        clips,
        findings: findings.into_iter().map(Finding::from).collect(),
    })
}

/// `POST /v1/captures/{id}/findings`: reviewers note what they saw; the capture
/// becomes reviewed.
pub async fn add_finding(
    State(state): State<Shared>,
    principal: Principal,
    Path(id): Path<String>,
    Json(form): Json<NewFinding>,
) -> Result<(StatusCode, Json<Finding>), ApiError> {
    capture_access(&state, &principal, &id, Role::Reviewer).await?;
    let note = form.note.trim();
    if !(1..=MAX_NOTE_CHARS).contains(&note.chars().count()) {
        return Err(ApiError::invalid(format!(
            "the note must have 1–{MAX_NOTE_CHARS} characters"
        )));
    }
    if let Some(evidence) = &form.evidence_artifact_id {
        let belongs: Option<i64> =
            sqlx::query_scalar("SELECT 1 FROM artifacts WHERE id = ?1 AND capture_id = ?2")
                .bind(evidence)
                .bind(&id)
                .fetch_optional(&state.db)
                .await?;
        if belongs.is_none() {
            return Err(ApiError::invalid(
                "the evidence must be a file of this capture",
            ));
        }
    }
    let finding = Finding {
        id: uuid::Uuid::now_v7().to_string(),
        note: note.to_owned(),
        evidence_artifact_id: form.evidence_artifact_id,
        author: principal.handle,
        created_at: now_ms(),
    };
    let mut tx = state.db.begin().await?;
    sqlx::query(
        "INSERT INTO capture_findings (id, capture_id, note, evidence_artifact_id, author, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
    )
    .bind(&finding.id)
    .bind(&id)
    .bind(&finding.note)
    .bind(&finding.evidence_artifact_id)
    .bind(&finding.author)
    .bind(finding.created_at)
    .execute(&mut *tx)
    .await?;
    sqlx::query("UPDATE captures SET inspection = 'reviewed' WHERE id = ?1")
        .bind(&id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok((StatusCode::CREATED, Json(finding)))
}

/// Digest, media type and size of an artifact, for readers of its project.
pub async fn artifact_blob(
    state: &Shared,
    principal: &Principal,
    id: &str,
) -> Result<(String, String, i64), ApiError> {
    let found: Option<BlobRow> = sqlx::query_as(
        "SELECT c.project_id, a.sha256, a.mime, a.bytes FROM artifacts a
         JOIN captures c ON c.id = a.capture_id WHERE a.id = ?1",
    )
    .bind(id)
    .fetch_optional(&state.db)
    .await?;
    let blob = found.ok_or(ApiError::NotFound)?;
    access(state, principal, &blob.project_id)
        .await?
        .require(Role::Reader)?;
    Ok((blob.sha256, blob.mime, blob.bytes))
}

/// `GET /v1/artifacts/{id}`: the file's bytes, for readers of its project.
pub async fn artifact(
    State(state): State<Shared>,
    principal: Principal,
    Path(id): Path<String>,
    request: Request,
) -> Result<Response, ApiError> {
    let found: Option<BlobRow> = sqlx::query_as(
        "SELECT c.project_id, a.sha256, a.mime, a.bytes FROM artifacts a
         JOIN captures c ON c.id = a.capture_id WHERE a.id = ?1",
    )
    .bind(&id)
    .fetch_optional(&state.db)
    .await?;
    let blob = found.ok_or(ApiError::NotFound)?;
    access(&state, &principal, &blob.project_id)
        .await?
        .require(Role::Reader)?;
    state.blobs.respond(&blob.sha256, &blob.mime, request).await
}

#[cfg(test)]
mod tests {
    use crate::testkit::*;
    use axum::http::StatusCode;
    use rupicola_api::{Capture, CaptureSummary, Finding, Inspection};
    use sha2::{Digest, Sha256};

    fn digest(data: &[u8]) -> String {
        format!("{:x}", Sha256::digest(data))
    }

    async fn upload(port: u16, token: &str, data: &[u8]) -> String {
        let sha = digest(data);
        let response = request(
            port,
            format!(
                "PUT /v1/blobs/{sha} HTTP/1.1\r\nHost: test\r\nAuthorization: Bearer {token}\r\n\
                 Content-Type: application/octet-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                data.len(),
                String::from_utf8_lossy(data)
            ),
        )
        .await;
        assert!(
            matches!(status(&response), StatusCode::OK | StatusCode::CREATED),
            "{response}"
        );
        sha
    }

    /// Ana owns `demo`; Bruno reads it; Carla is an outsider. Returns their tokens.
    async fn demo() -> (u16, [String; USERS]) {
        let (port, state) = start_custom(settings(), None, true).await;
        for handle in ["ana", "bruno", "carla"] {
            seed_account(&state, handle, false).await;
        }
        let [ana, bruno, carla] = [
            sign_in(port, "ana").await,
            sign_in(port, "bruno").await,
            sign_in(port, "carla").await,
        ];
        call(
            port,
            "POST",
            "/v1/projects",
            Some(&ana),
            Some(r#"{"id":"demo","label":"Demo"}"#),
        )
        .await;
        call(
            port,
            "PUT",
            "/v1/projects/demo/members/bruno",
            Some(&ana),
            Some(r#"{"role":"reader"}"#),
        )
        .await;
        (port, [ana, bruno, carla])
    }

    /// Ana (owner), Bruno (reader) and Carla (outsider).
    const USERS: usize = 3;
    const VIEWPORT_WIDTH: u32 = 1280;
    const VIEWPORT_HEIGHT: u32 = 720;
    const FRAME_TARGET_MS: f64 = 100.0;

    fn capture_json(home: &str, frame: &str) -> String {
        format!(
            r#"{{"scenario":"home","status":"complete",
                "environment":{{"width":{VIEWPORT_WIDTH},"height":{VIEWPORT_HEIGHT},"color_scheme":"dark"}},
                "checkpoints":[{{"id":"home","path":"checkpoints/home.png","sha256":"{home}"}}],
                "clips":[{{"id":"open","frames":[{{"path":"clips/open/1.png","sha256":"{frame}","target_ms":{FRAME_TARGET_MS}}}]}}]}}"#
        )
    }

    #[tokio::test]
    async fn blobs_are_verified_and_deduplicated() {
        let (port, [ana, ..]) = demo().await;
        let sha = upload(port, &ana, b"png-bytes").await;
        let again = request(
            port,
            format!("PUT /v1/blobs/{sha} HTTP/1.1\r\nHost: t\r\nAuthorization: Bearer {ana}\r\nContent-Length: 9\r\nConnection: close\r\n\r\npng-bytes"),
        )
        .await;
        assert_eq!(status(&again), StatusCode::OK, "{again}");
        let head = request(port, format!("HEAD /v1/blobs/{sha} HTTP/1.1\r\nHost: t\r\nAuthorization: Bearer {ana}\r\nConnection: close\r\n\r\n")).await;
        assert_eq!(status(&head), StatusCode::OK, "{head}");
        let forged = request(
            port,
            format!("PUT /v1/blobs/{sha} HTTP/1.1\r\nHost: t\r\nAuthorization: Bearer {ana}\r\nContent-Length: 5\r\nConnection: close\r\n\r\nother"),
        )
        .await;
        assert_eq!(
            status(&forged),
            StatusCode::UNPROCESSABLE_ENTITY,
            "{forged}"
        );
        let anonymous = request(
            port,
            format!("HEAD /v1/blobs/{sha} HTTP/1.1\r\nHost: t\r\nConnection: close\r\n\r\n"),
        )
        .await;
        assert_eq!(status(&anonymous), StatusCode::UNAUTHORIZED, "{anonymous}");
    }

    #[tokio::test]
    async fn a_published_capture_can_be_browsed_reviewed_and_downloaded() {
        let (port, [ana, bruno, _]) = demo().await;
        let home = upload(port, &ana, b"home-png").await;
        let frame = upload(port, &ana, b"frame-png").await;
        let created = call(
            port,
            "POST",
            "/v1/projects/demo/captures",
            Some(&ana),
            Some(&capture_json(&home, &frame)),
        )
        .await;
        assert_eq!(status(&created), StatusCode::CREATED, "{created}");
        let capture: Capture = json(&created);
        assert_eq!(capture.checkpoints[0].artifact.mime, "image/png");
        assert_eq!(capture.clips[0].frames[0].target_ms, FRAME_TARGET_MS);
        assert_eq!(capture.environment.width, Some(VIEWPORT_WIDTH));
        assert_eq!(capture.inspection, Inspection::Pending);

        let listed: Vec<CaptureSummary> = json(
            &call(
                port,
                "GET",
                "/v1/projects/demo/captures",
                Some(&bruno),
                None,
            )
            .await,
        );
        assert_eq!(listed.len(), 1);
        assert_eq!(
            listed[0].preview.as_deref(),
            Some(capture.checkpoints[0].artifact.id.as_str())
        );

        let artifact = &capture.checkpoints[0].artifact.id;
        let file = call(
            port,
            "GET",
            &format!("/v1/artifacts/{artifact}"),
            Some(&bruno),
            None,
        )
        .await;
        assert_eq!(status(&file), StatusCode::OK, "{file}");
        assert_eq!(header(&file, "content-type"), Some("image/png"));
        assert_eq!(body(&file), "home-png");

        let note = format!(r#"{{"note":"Title overflows","evidence_artifact_id":"{artifact}"}}"#);
        let readers_cannot = call(
            port,
            "POST",
            &format!("/v1/captures/{}/findings", capture.id),
            Some(&bruno),
            Some(&note),
        )
        .await;
        assert_eq!(
            status(&readers_cannot),
            StatusCode::FORBIDDEN,
            "{readers_cannot}"
        );
        let added = call(
            port,
            "POST",
            &format!("/v1/captures/{}/findings", capture.id),
            Some(&ana),
            Some(&note),
        )
        .await;
        assert_eq!(status(&added), StatusCode::CREATED, "{added}");
        let finding: Finding = json(&added);
        assert_eq!(finding.author.as_deref(), Some("ana"));

        let reviewed: Capture = json(
            &call(
                port,
                "GET",
                &format!("/v1/captures/{}", capture.id),
                Some(&bruno),
                None,
            )
            .await,
        );
        assert_eq!(reviewed.inspection, Inspection::Reviewed);
        assert_eq!(reviewed.findings, [finding]);
    }

    #[tokio::test]
    async fn publishing_needs_uploaded_files_and_a_reviewer() {
        let (port, [ana, bruno, carla]) = demo().await;
        let home = upload(port, &ana, b"home-png").await;
        let missing = digest(b"never uploaded");
        let incomplete = call(
            port,
            "POST",
            "/v1/projects/demo/captures",
            Some(&ana),
            Some(&capture_json(&home, &missing)),
        )
        .await;
        assert_eq!(
            status(&incomplete),
            StatusCode::UNPROCESSABLE_ENTITY,
            "{incomplete}"
        );

        let frame = upload(port, &ana, b"frame-png").await;
        let body = capture_json(&home, &frame);
        let reader = call(
            port,
            "POST",
            "/v1/projects/demo/captures",
            Some(&bruno),
            Some(&body),
        )
        .await;
        assert_eq!(status(&reader), StatusCode::FORBIDDEN, "{reader}");
        let outsider = call(
            port,
            "POST",
            "/v1/projects/demo/captures",
            Some(&carla),
            Some(&body),
        )
        .await;
        assert_eq!(status(&outsider), StatusCode::NOT_FOUND, "{outsider}");

        let escaping = body.replace("checkpoints/home.png", "../home.png");
        let bad_path = call(
            port,
            "POST",
            "/v1/projects/demo/captures",
            Some(&ana),
            Some(&escaping),
        )
        .await;
        assert_eq!(
            status(&bad_path),
            StatusCode::UNPROCESSABLE_ENTITY,
            "{bad_path}"
        );
    }

    #[tokio::test]
    async fn outsiders_cannot_read_captures_or_files() {
        let (port, [ana, _, carla]) = demo().await;
        let home = upload(port, &ana, b"home-png").await;
        let frame = upload(port, &ana, b"frame-png").await;
        let capture: Capture = json(
            &call(
                port,
                "POST",
                "/v1/projects/demo/captures",
                Some(&ana),
                Some(&capture_json(&home, &frame)),
            )
            .await,
        );
        for path in [
            format!("/v1/captures/{}", capture.id),
            format!("/v1/artifacts/{}", capture.checkpoints[0].artifact.id),
            "/v1/projects/demo/captures".to_owned(),
        ] {
            let response = call(port, "GET", &path, Some(&carla), None).await;
            assert_eq!(
                status(&response),
                StatusCode::NOT_FOUND,
                "{path}: {response}"
            );
        }
    }

    fn missing_body(hashes: &[&str]) -> String {
        serde_json::to_string(&serde_json::json!({ "sha256": hashes })).unwrap()
    }

    #[tokio::test]
    async fn only_the_files_a_project_does_not_hold_are_missing_and_once_each() {
        let (port, [ana, _, carla]) = demo().await;
        call(
            port,
            "POST",
            "/v1/projects",
            Some(&carla),
            Some(r#"{"id":"other","label":"Other"}"#),
        )
        .await;
        let home = upload(port, &ana, b"home pixels").await;
        let frame = upload(port, &ana, b"frame pixels").await;
        let fresh = digest(b"never uploaded");
        let ask = |token: &str, project: &str, hashes: &[&str]| {
            let (token, path, body) = (
                token.to_owned(),
                format!("/v1/projects/{project}/blobs/missing"),
                missing_body(hashes),
            );
            async move { call(port, "POST", &path, Some(&token), Some(&body)).await }
        };
        // Stored, but no capture of the project cites them yet.
        let before = ask(&ana, "demo", &[&home, &fresh, &home]).await;
        assert_eq!(status(&before), StatusCode::OK, "{before}");
        let before: rupicola_api::Missing = json(&before);
        assert_eq!(
            before.missing,
            [home.clone(), fresh.clone()],
            "in order and once each"
        );
        assert!(before.limits.max_blob_bytes > 0, "the limits come along");
        let created = call(
            port,
            "POST",
            "/v1/projects/demo/captures",
            Some(&ana),
            Some(&capture_json(&home, &frame)),
        )
        .await;
        assert_eq!(status(&created), StatusCode::CREATED, "{created}");
        let after: rupicola_api::Missing = json(&ask(&ana, "demo", &[&home, &frame, &fresh]).await);
        assert_eq!(after.missing, std::slice::from_ref(&fresh));
        let elsewhere: rupicola_api::Missing = json(&ask(&carla, "other", &[&home]).await);
        assert_eq!(
            elsewhere.missing,
            [home],
            "another project does not learn of the blob"
        );
    }

    #[tokio::test]
    async fn asking_what_is_missing_needs_upload_rights_and_plain_hashes() {
        let (port, [ana, bruno, carla]) = demo().await;
        let path = "/v1/projects/demo/blobs/missing";
        let good = missing_body(&[&digest(b"x")]);
        let reader = call(port, "POST", path, Some(&bruno), Some(&good)).await;
        assert_eq!(
            status(&reader),
            StatusCode::FORBIDDEN,
            "a reader does not upload"
        );
        let outsider = call(port, "POST", path, Some(&carla), Some(&good)).await;
        assert_eq!(status(&outsider), StatusCode::NOT_FOUND);
        let shouting = missing_body(&[&digest(b"x").to_uppercase()]);
        let bad = call(port, "POST", path, Some(&ana), Some(&shouting)).await;
        assert_eq!(status(&bad), StatusCode::UNPROCESSABLE_ENTITY, "{bad}");
        let many: Vec<String> = (0..=rupicola_api::MAX_MISSING_BATCH)
            .map(|i| digest(i.to_string().as_bytes()))
            .collect();
        let refs: Vec<&str> = many.iter().map(String::as_str).collect();
        let too_many = call(port, "POST", path, Some(&ana), Some(&missing_body(&refs))).await;
        assert_eq!(
            status(&too_many),
            StatusCode::UNPROCESSABLE_ENTITY,
            "{too_many}"
        );
    }
}
