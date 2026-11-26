//! The `captures`, `artifacts` and `capture_findings` tables.

use crate::domain::ApiError;
use rupicola_api::{
    Artifact, Capture, CaptureSummary, Checkpoint, Clip, Finding, Frame, Tombstone,
};
use sqlx::SqliteExecutor;

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

impl TryFrom<SummaryRow> for CaptureSummary {
    type Error = ApiError;

    fn try_from(row: SummaryRow) -> Result<Self, Self::Error> {
        Ok(Self {
            id: row.id,
            scenario: row.scenario,
            status: row.status.parse().map_err(super::corrupt)?,
            inspection: row.inspection.parse().map_err(super::corrupt)?,
            created_at: row.created_at,
            preview: row.preview,
        })
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
    /// Set for a capture published as a record, whose environment has the record's shape.
    digest: Option<String>,
    deleted_at: Option<i64>,
    deleted_by: Option<String>,
    delete_reason: Option<String>,
    plan_json: Option<String>,
    plan_digest: Option<String>,
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

/// Project, digest, media type and size of an artifact.
#[derive(sqlx::FromRow)]
pub struct ArtifactBlob {
    pub capture_id: String,
    pub project_id: String,
    pub sha256: String,
    pub mime: String,
    pub bytes: i64,
}

/// What a summary selects, with its first checkpoint as preview.
const SUMMARY_COLUMNS: &str = "c.id, c.scenario, c.status, c.inspection, c.created_at,
   (SELECT a.id FROM artifacts a WHERE a.capture_id = c.id AND a.role = 'checkpoint'
    ORDER BY a.position LIMIT 1) AS preview";

/// A project's live captures, newest first.
pub async fn of_project(
    db: impl SqliteExecutor<'_>,
    project: &str,
) -> Result<Vec<CaptureSummary>, ApiError> {
    let rows: Vec<SummaryRow> = sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "SELECT {SUMMARY_COLUMNS}
         FROM captures c WHERE c.project_id = ?1 AND c.deleted_at IS NULL
         ORDER BY c.created_at DESC, c.id DESC LIMIT ?2"
    )))
    .bind(project)
    .bind(super::MAX_LIST_ITEMS)
    .fetch_all(db)
    .await?;
    rows.into_iter().map(CaptureSummary::try_from).collect()
}

/// The summaries of the live captures named in `ids`, newest first.
pub async fn summaries(
    db: impl SqliteExecutor<'_>,
    ids: &[String],
) -> Result<Vec<CaptureSummary>, ApiError> {
    let rows: Vec<SummaryRow> = sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "SELECT {SUMMARY_COLUMNS}
         FROM captures c
         WHERE c.deleted_at IS NULL AND c.id IN (SELECT value FROM json_each(?1))
         ORDER BY c.created_at DESC, c.id DESC"
    )))
    .bind(serde_json::to_string(ids).map_err(anyhow::Error::from)?)
    .fetch_all(db)
    .await?;
    rows.into_iter().map(CaptureSummary::try_from).collect()
}

/// The project a capture belongs to.
pub async fn project_of(
    db: impl SqliteExecutor<'_>,
    capture: &str,
) -> sqlx::Result<Option<String>> {
    sqlx::query_scalar("SELECT project_id FROM captures WHERE id = ?1")
        .bind(capture)
        .fetch_optional(db)
        .await
}

/// Whether `artifact` is a file of one of `project`'s captures.
pub async fn artifact_in_project(
    db: impl SqliteExecutor<'_>,
    artifact: &str,
    project: &str,
) -> sqlx::Result<bool> {
    let belongs: Option<i64> = sqlx::query_scalar(
        "SELECT 1 FROM artifacts a JOIN captures c ON c.id = a.capture_id
         WHERE a.id = ?1 AND c.project_id = ?2",
    )
    .bind(artifact)
    .bind(project)
    .fetch_optional(db)
    .await?;
    Ok(belongs.is_some())
}

/// Whether `artifact` is a file of `capture`.
pub async fn artifact_in_capture(
    db: impl SqliteExecutor<'_>,
    artifact: &str,
    capture: &str,
) -> sqlx::Result<bool> {
    let belongs: Option<i64> =
        sqlx::query_scalar("SELECT 1 FROM artifacts WHERE id = ?1 AND capture_id = ?2")
            .bind(artifact)
            .bind(capture)
            .fetch_optional(db)
            .await?;
    Ok(belongs.is_some())
}

pub async fn artifact_blob(
    db: impl SqliteExecutor<'_>,
    artifact: &str,
) -> sqlx::Result<Option<ArtifactBlob>> {
    sqlx::query_as(
        "SELECT a.capture_id, c.project_id, a.sha256, a.mime, a.bytes FROM artifacts a
         JOIN captures c ON c.id = a.capture_id WHERE a.id = ?1",
    )
    .bind(artifact)
    .fetch_optional(db)
    .await
}

/// Inserts a derived capture holding a comparison diff PNG artifact, returning the artifact ID.
pub async fn insert_diff_artifact(
    tx: &mut sqlx::SqliteConnection,
    project: &str,
    scenario: &str,
    owner: &str,
    sha256: &str,
    bytes: u64,
    now: i64,
) -> Result<String, ApiError> {
    let path = format!("diffs/{owner}.png");
    let capture_id = uuid::Uuid::now_v7().to_string();
    sqlx::query(
        "INSERT INTO captures (id, project_id, kind, scenario, status, inspection, environment,
           created_at)
         VALUES (?1, ?2, 'derived', ?3, 'complete', 'reviewed', '{}', ?4)",
    )
    .bind(&capture_id)
    .bind(project)
    .bind(scenario)
    .bind(now)
    .execute(&mut *tx)
    .await?;

    let artifact_id = uuid::Uuid::now_v7().to_string();
    sqlx::query(
        "INSERT INTO artifacts (id, capture_id, role, owner_id, position, path, sha256, bytes, mime)
         VALUES (?1, ?2, 'checkpoint', ?3, 0, ?4, ?5, ?6, 'image/png')",
    )
    .bind(&artifact_id)
    .bind(&capture_id)
    .bind(owner)
    .bind(&path)
    .bind(sha256)
    .bind(i64::try_from(bytes).map_err(anyhow::Error::from)?)
    .execute(&mut *tx)
    .await?;
    Ok(artifact_id)
}

pub async fn insert_finding(
    db: impl SqliteExecutor<'_>,
    capture: &str,
    finding: &Finding,
) -> sqlx::Result<()> {
    sqlx::query(
        "INSERT INTO capture_findings (id, capture_id, note, evidence_artifact_id, author, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
    )
    .bind(&finding.id)
    .bind(capture)
    .bind(&finding.note)
    .bind(&finding.evidence_artifact_id)
    .bind(&finding.author)
    .bind(finding.created_at)
    .execute(db)
    .await
    .map(drop)
}

pub async fn mark_reviewed(db: impl SqliteExecutor<'_>, capture: &str) -> sqlx::Result<()> {
    sqlx::query("UPDATE captures SET inspection = 'reviewed' WHERE id = ?1")
        .bind(capture)
        .execute(db)
        .await
        .map(drop)
}

/// Groups a capture's files, in order, into its checkpoints and clips.
fn arrange(artifacts: Vec<ArtifactRow>) -> (Vec<Checkpoint>, Vec<Clip>) {
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
    (checkpoints, clips)
}

fn build_capture(
    id: String,
    capture: CaptureRow,
    artifacts: Vec<ArtifactRow>,
    findings: Vec<FindingRow>,
) -> Result<Capture, ApiError> {
    let (checkpoints, clips) = arrange(artifacts);
    let deleted = capture.deleted_at.map(|at| Tombstone {
        at,
        by: capture.deleted_by.clone().unwrap_or_default(),
        reason: capture.delete_reason.clone().unwrap_or_default(),
    });
    let plan_spec = capture
        .plan_json
        .as_deref()
        .map(serde_json::from_str)
        .transpose()
        .map_err(anyhow::Error::from)?;
    Ok(Capture {
        id,
        project_id: capture.project_id,
        scenario: capture.scenario,
        revision: capture.revision,
        status: capture.status.parse().map_err(super::corrupt)?,
        blocker: capture.blocker,
        inspection: capture.inspection.parse().map_err(super::corrupt)?,
        environment: if capture.digest.is_some() {
            let recorded: rupicola_api::record::Environment =
                serde_json::from_str(&capture.environment).map_err(anyhow::Error::from)?;
            rupicola_api::Environment::from(&recorded)
        } else {
            serde_json::from_str(&capture.environment).map_err(anyhow::Error::from)?
        },
        created_at: capture.created_at,
        checkpoints,
        clips,
        findings: findings.into_iter().map(Finding::from).collect(),
        deleted,
        plan_spec,
        plan_digest: capture.plan_digest,
    })
}

/// A capture with its files and findings, removed or not.
pub async fn load(db: &sqlx::SqlitePool, id: &str) -> Result<Capture, ApiError> {
    let capture: CaptureRow = sqlx::query_as(
        "SELECT project_id, scenario, revision, status, blocker, inspection, environment,
               created_at, digest, deleted_at, deleted_by, delete_reason, plan_json, plan_digest
         FROM captures WHERE id = ?1",
    )
    .bind(id)
    .fetch_optional(db)
    .await?
    .ok_or(ApiError::NotFound)?;
    let artifacts: Vec<ArtifactRow> = sqlx::query_as(
        "SELECT id, role, owner_id, path, sha256, bytes, mime, target_ms, video_ms
         FROM artifacts WHERE capture_id = ?1 ORDER BY position",
    )
    .bind(id)
    .fetch_all(db)
    .await?;
    let findings: Vec<FindingRow> = sqlx::query_as(
        "SELECT id, note, evidence_artifact_id, author, created_at FROM capture_findings
         WHERE capture_id = ?1 ORDER BY created_at, id",
    )
    .bind(id)
    .fetch_all(db)
    .await?;
    build_capture(id.to_owned(), capture, artifacts, findings)
}

#[derive(sqlx::FromRow)]
struct KeyedCaptureRow {
    id: String,
    #[sqlx(flatten)]
    row: CaptureRow,
}

#[derive(sqlx::FromRow)]
struct KeyedArtifactRow {
    capture_id: String,
    #[sqlx(flatten)]
    row: ArtifactRow,
}

#[derive(sqlx::FromRow)]
struct KeyedFindingRow {
    capture_id: String,
    #[sqlx(flatten)]
    row: FindingRow,
}

/// Batch-loads captures with their files and findings in O(1) queries, preserving `ids` order.
pub async fn load_many(db: &sqlx::SqlitePool, ids: &[String]) -> Result<Vec<Capture>, ApiError> {
    use std::collections::HashMap;
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    let mut qb = sqlx::QueryBuilder::new(
        "SELECT id, project_id, scenario, revision, status, blocker, inspection, environment,
                created_at, digest, deleted_at, deleted_by, delete_reason, plan_json, plan_digest
         FROM captures WHERE id",
    );
    super::push_in(&mut qb, ids);
    let capture_rows: Vec<KeyedCaptureRow> = qb.build_query_as().fetch_all(db).await?;
    let mut by_id: HashMap<String, CaptureRow> =
        capture_rows.into_iter().map(|r| (r.id, r.row)).collect();

    let mut qb = sqlx::QueryBuilder::new(
        "SELECT capture_id, id, role, owner_id, path, sha256, bytes, mime, target_ms, video_ms
         FROM artifacts WHERE capture_id",
    );
    super::push_in(&mut qb, ids);
    qb.push(" ORDER BY capture_id, position");
    let artifact_rows: Vec<KeyedArtifactRow> = qb.build_query_as().fetch_all(db).await?;
    let mut artifacts_by_id: HashMap<String, Vec<ArtifactRow>> = HashMap::new();
    for item in artifact_rows {
        artifacts_by_id
            .entry(item.capture_id)
            .or_default()
            .push(item.row);
    }

    let mut qb = sqlx::QueryBuilder::new(
        "SELECT capture_id, id, note, evidence_artifact_id, author, created_at
         FROM capture_findings WHERE capture_id",
    );
    super::push_in(&mut qb, ids);
    qb.push(" ORDER BY capture_id, created_at, id");
    let finding_rows: Vec<KeyedFindingRow> = qb.build_query_as().fetch_all(db).await?;
    let mut findings_by_id: HashMap<String, Vec<FindingRow>> = HashMap::new();
    for item in finding_rows {
        findings_by_id
            .entry(item.capture_id)
            .or_default()
            .push(item.row);
    }

    let mut out = Vec::with_capacity(ids.len());
    for id in ids {
        if let Some(capture) = by_id.remove(id) {
            let artifacts = artifacts_by_id.remove(id).unwrap_or_default();
            let findings = findings_by_id.remove(id).unwrap_or_default();
            out.push(build_capture(id.clone(), capture, artifacts, findings)?);
        }
    }
    Ok(out)
}
