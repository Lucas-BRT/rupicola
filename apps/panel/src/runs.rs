//! Captures as the panel shows them. The backend owns them: this reads a project's
//! captures through the [`Api`] and wraps each with its project label.

use super::{Account, PageResult, Panel, Run, StatusCode, auth, backend};
use rupicola_api::{Capture, Project};
use std::sync::Arc;
use topcoat::context::{Cx, context};

/// Newest captures read in full per project when listing; older ones are not shown.
const RECENT_PER_PROJECT: usize = 50;

fn run_of(project: &Project, capture: &Capture) -> Run {
    Run {
        key: capture.id.clone(),
        project_id: project.id.clone(),
        project: project.label.clone(),
        scenario: capture.scenario.clone(),
        status: capture.status.to_string(),
        inspection: capture.inspection.to_string(),
        created_at_ms: capture.created_at,
        capture: capture.clone(),
    }
}

/// The recent captures of every project the account can see, newest first.
///
/// # Errors
///
/// The status of a failed call to the backend.
pub async fn list(cx: &Cx) -> PageResult<Vec<Run>> {
    let projects = backend::projects(cx).await.as_ref().map_err(|e| *e)?;
    let mut runs = Vec::new();
    for project in projects {
        runs.extend(of_project(cx, project).await?);
    }
    runs.sort_by_key(|run| std::cmp::Reverse(run.created_at_ms));
    Ok(runs)
}

/// The recent captures of one project, newest first.
///
/// # Errors
///
/// The status of a failed call to the backend.
pub async fn of_project(cx: &Cx, project: &Project) -> PageResult<Vec<Run>> {
    let (panel, account) = (context::<Arc<Panel>>(cx), context::<Account>(cx));
    let query = format!("limit={RECENT_PER_PROJECT}&include=details");
    let page = panel
        .api
        .search_captures(&account.session, &project.id, Some(query))
        .await
        .map_err(|e| auth::api_error(&e))?;
    Ok(page
        .details
        .iter()
        .map(|capture| run_of(project, capture))
        .collect())
}

/// One capture with the project it belongs to.
///
/// # Errors
///
/// 404 when the account cannot see it.
pub async fn find(cx: &Cx, id: &str) -> PageResult<Run> {
    let capture = backend::capture(cx, id).await.as_ref().map_err(|e| *e)?;
    let project = backend::projects(cx)
        .await
        .as_ref()
        .map_err(|e| *e)?
        .iter()
        .find(|project| project.id == capture.project_id)
        .ok_or(StatusCode::NOT_FOUND)?;
    Ok(run_of(project, capture))
}

/// The captures of one project, newest first, without their files.
///
/// # Errors
///
/// The status of a failed call to the backend.
pub async fn summaries<'a>(
    cx: &'a Cx,
    project: &str,
) -> PageResult<&'a [rupicola_api::CaptureSummary]> {
    backend::captures(cx, project)
        .await
        .as_ref()
        .map(Vec::as_slice)
        .map_err(|e| *e)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rupicola_api::{
        Artifact, CaptureStatus, Checkpoint as ApiCheckpoint, Environment as ApiEnvironment,
        Finding as ApiFinding, Inspection,
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
            deleted: None,
            plan_spec: None,
            plan_digest: None,
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
        assert_eq!(run.capture.checkpoints[0].artifact.id, ARTIFACT);
        assert_eq!(run.capture.environment.width, Some(640));
        assert_eq!(
            run.capture.environment.color_scheme.as_deref(),
            Some("dark")
        );
        assert_eq!(run.capture.blocker.as_deref(), Some("no browser"));
        assert_eq!(
            run.capture.findings[0].evidence_artifact_id.as_deref(),
            Some(ARTIFACT)
        );
        assert_eq!(run.capture.findings[0].note, NOTE);
    }
}
