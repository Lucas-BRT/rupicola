//! Captures as the panel shows them. The backend owns them: this reads a project's
//! captures through the [`Api`] and shapes each into the [`Run`] the screens expect, with the
//! id of every artifact standing where a file path used to.

use super::{Account, PageResult, Panel, Run, StatusCode, auth};
use rupicola_api::{Artifact, Capture, CaptureStatus, Inspection, Project};
use rupicola_client::Api;
use rupicola_core::capture_manifest::{
    Checkpoint, Clip, Environment, Finding, Frame, Manifest, Project as ManifestProject,
};
use std::sync::Arc;
use tokio::task::JoinSet;

/// Newest captures read in full per project when listing; older ones are not shown.
const RECENT_PER_PROJECT: usize = 50;
/// Version of the manifest shape the screens read.
const MANIFEST_VERSION: u32 = 1;

fn status_name(status: CaptureStatus) -> &'static str {
    match status {
        CaptureStatus::Complete => "complete",
        CaptureStatus::Blocked => "blocked",
    }
}

fn inspection_name(inspection: Inspection) -> &'static str {
    match inspection {
        Inspection::Pending => "pending",
        Inspection::Reviewed => "reviewed",
    }
}

/// What a screen links an artifact by: its id.
fn reference(artifact: &Artifact) -> String {
    artifact.id.clone()
}

fn manifest_of(project: &Project, capture: &Capture) -> Manifest {
    let environment = &capture.environment;
    Manifest {
        version: MANIFEST_VERSION,
        project: ManifestProject {
            id: project.id.clone(),
            label: project.label.clone(),
        },
        scenario: capture.scenario.clone(),
        revision: capture.revision.clone(),
        created_at_ms: capture.created_at,
        status: status_name(capture.status).to_owned(),
        blocker: capture.blocker.clone(),
        inspection: inspection_name(capture.inspection).to_owned(),
        environment: Environment {
            color_scheme: environment.color_scheme.clone().unwrap_or_default(),
            reduced_motion: environment.reduced_motion,
            width: environment.width,
            height: environment.height,
            renderer: environment.renderer.clone().unwrap_or_default(),
        },
        checkpoints: capture
            .checkpoints
            .iter()
            .map(|checkpoint| Checkpoint {
                id: checkpoint.id.clone(),
                path: reference(&checkpoint.artifact),
            })
            .collect(),
        clips: capture
            .clips
            .iter()
            .map(|clip| Clip {
                id: clip.id.clone(),
                frames: clip
                    .frames
                    .iter()
                    .map(|frame| Frame {
                        path: reference(&frame.artifact),
                        target_ms: frame.target_ms,
                        video_ms: frame.video_ms,
                    })
                    .collect(),
                analysis_path: clip.analysis.as_ref().map(reference),
                video_path: clip.video.as_ref().map(reference),
                sheet_path: clip.sheet.as_ref().map(reference),
            })
            .collect(),
        findings: capture
            .findings
            .iter()
            .map(|finding| Finding {
                note: finding.note.clone(),
                evidence: finding.evidence_artifact_id.clone(),
                author: finding.author.clone(),
            })
            .collect(),
    }
}

fn run_of(project: &Project, capture: &Capture) -> Run {
    Run {
        key: capture.id.clone(),
        project_id: project.id.clone(),
        project: project.label.clone(),
        scenario: capture.scenario.clone(),
        status: status_name(capture.status).to_owned(),
        inspection: inspection_name(capture.inspection).to_owned(),
        created_at_ms: capture.created_at,
        manifest: manifest_of(project, capture),
    }
}

/// The recent captures of every project `account` can see, newest first.
///
/// # Errors
///
/// The status of a failed call to the backend.
pub async fn list(panel: &Panel, account: &Account) -> PageResult<Vec<Run>> {
    let projects = panel
        .api
        .projects(&account.session)
        .await
        .map_err(|e| auth::api_error(&e))?;
    let mut reading = JoinSet::new();
    for project in projects {
        let api = Arc::clone(&panel.api);
        let session = account.session.clone();
        reading.spawn(async move { project_runs(api.as_ref(), &session, &project).await });
    }
    let mut runs = Vec::new();
    while let Some(joined) = reading.join_next().await {
        runs.extend(joined.map_err(super::db_error)??);
    }
    runs.sort_by_key(|run| std::cmp::Reverse(run.created_at_ms));
    Ok(runs)
}

async fn project_runs(api: &dyn Api, session: &str, project: &Project) -> PageResult<Vec<Run>> {
    let summaries = api
        .captures(session, &project.id)
        .await
        .map_err(|e| auth::api_error(&e))?;
    let mut runs = Vec::new();
    for summary in summaries.into_iter().take(RECENT_PER_PROJECT) {
        let capture = api
            .capture(session, &summary.id)
            .await
            .map_err(|e| auth::api_error(&e))?;
        runs.push(run_of(project, &capture));
    }
    Ok(runs)
}

/// One capture with the project it belongs to.
///
/// # Errors
///
/// 404 when the account cannot see it.
pub async fn find(panel: &Panel, account: &Account, id: &str) -> PageResult<Run> {
    let capture = panel
        .api
        .capture(&account.session, id)
        .await
        .map_err(|e| auth::api_error(&e))?;
    let project = panel
        .api
        .projects(&account.session)
        .await
        .map_err(|e| auth::api_error(&e))?
        .into_iter()
        .find(|project| project.id == capture.project_id)
        .ok_or(StatusCode::NOT_FOUND)?;
    Ok(run_of(&project, &capture))
}

/// The captures of one project, newest first, without their files.
///
/// # Errors
///
/// The status of a failed call to the backend.
pub async fn summaries(
    panel: &Panel,
    account: &Account,
    project: &str,
) -> PageResult<Vec<rupicola_api::CaptureSummary>> {
    panel
        .api
        .captures(&account.session, project)
        .await
        .map_err(|e| auth::api_error(&e))
}

#[cfg(test)]
mod tests {
    use super::*;
    use rupicola_api::{
        Checkpoint as ApiCheckpoint, Environment as ApiEnvironment, Finding as ApiFinding,
    };

    const ARTIFACT: &str = "artifact-1";
    const CAPTURE: &str = "capture-1";
    const PROJECT: &str = "demo";
    const NOTE: &str = "Focus is visible.";

    fn artifact(id: &str) -> Artifact {
        Artifact {
            id: id.to_owned(),
            path: "checkpoints/before.png".to_owned(),
            mime: "image/png".to_owned(),
            bytes: 1,
            sha256: String::new(),
        }
    }

    fn capture() -> Capture {
        Capture {
            id: CAPTURE.to_owned(),
            project_id: PROJECT.to_owned(),
            scenario: "Settings".to_owned(),
            revision: None,
            status: CaptureStatus::Blocked,
            blocker: Some("no browser".to_owned()),
            inspection: Inspection::Pending,
            environment: ApiEnvironment {
                width: Some(640),
                color_scheme: Some("dark".to_owned()),
                ..ApiEnvironment::default()
            },
            created_at: 7,
            checkpoints: vec![ApiCheckpoint {
                id: "before".to_owned(),
                artifact: artifact(ARTIFACT),
            }],
            clips: Vec::new(),
            findings: vec![ApiFinding {
                id: "f1".to_owned(),
                note: NOTE.to_owned(),
                evidence_artifact_id: Some(ARTIFACT.to_owned()),
                author: Some("ana".to_owned()),
                created_at: 8,
            }],
        }
    }

    fn project() -> Project {
        Project {
            id: PROJECT.to_owned(),
            label: "Demo".to_owned(),
            created_at: 0,
            role: None,
        }
    }

    #[test]
    fn a_capture_becomes_a_run_whose_files_are_named_by_artifact_id() {
        let run = run_of(&project(), &capture());
        assert_eq!(run.key, CAPTURE);
        assert_eq!(run.project, "Demo");
        assert_eq!(run.status, "blocked");
        assert_eq!(run.inspection, "pending");
        let manifest = &run.manifest;
        assert_eq!(manifest.checkpoints[0].path, ARTIFACT);
        assert_eq!(manifest.environment.width, Some(640));
        assert_eq!(manifest.environment.color_scheme, "dark");
        assert_eq!(manifest.blocker.as_deref(), Some("no browser"));
        assert_eq!(manifest.findings[0].evidence.as_deref(), Some(ARTIFACT));
        assert_eq!(manifest.findings[0].note, NOTE);
    }
}
