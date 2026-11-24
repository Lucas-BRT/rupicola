//! Problems found in a project's interface: filing, triage and discussion.

use crate::{
    domain::{
        Access, ApiError, Principal,
        problem::{
            EVIDENCE_LABEL, infer_from_finding, is_suppressed, validate, validate_comment,
            validate_patch,
        },
        secret::now_ms,
    },
    repositories::{audit, captures, problems, selections},
    services::{access::access, selections::keep},
    state::AppState,
};
use rupicola_api::{
    Comment, NewComment, NewProblem, NewSelection, Problem, ProblemPatch, ProblemStatus,
    ProblemSummary, Role, SelectionItem, SelectionMark,
};
use std::collections::HashSet;

/// Project of a problem, checked against the caller's access.
async fn problem_access(
    state: &AppState,
    principal: &Principal,
    problem: &str,
    role: Role,
) -> Result<String, ApiError> {
    let project = problems::project_of(&state.db, problem)
        .await?
        .ok_or(ApiError::NotFound)?;
    access(state, principal, &project).await?.require(role)?;
    Ok(project)
}

/// Files a new problem in `project`, turning any evidence artifacts into a proof selection.
pub async fn create(
    state: &AppState,
    principal: &Principal,
    project: &str,
    form: &NewProblem,
) -> Result<Problem, ApiError> {
    create_with_status(state, principal, project, form, ProblemStatus::Open).await
}

async fn create_with_status(
    state: &AppState,
    principal: &Principal,
    project: &str,
    form: &NewProblem,
    status: ProblemStatus,
) -> Result<Problem, ApiError> {
    match access(state, principal, project).await? {
        Access::Device => {}
        person @ Access::Person(_) => person.require(Role::Reviewer)?,
    }
    validate(form)?;
    let mut seen = HashSet::new();
    let mut anchors = Vec::new();
    for artifact in form.evidence.iter().filter(|a| seen.insert(a.as_str())) {
        let capture = problems::resolve_evidence_capture(&state.db, project, artifact)
            .await?
            .ok_or(ApiError::invalid(
                "the evidence must be files of this project's captures",
            ))?;
        anchors.push(SelectionItem {
            capture,
            artifact: Some(artifact.clone()),
            path: None,
            mark: SelectionMark::Proof,
            fragment: None,
        });
    }
    let mut proofs = Vec::new();
    if !anchors.is_empty() {
        proofs.push(NewSelection {
            label: EVIDENCE_LABEL.to_owned(),
            items: anchors,
        });
    }
    proofs.extend(form.selections.iter().cloned());
    let id = uuid::Uuid::now_v7().to_string();
    let now = now_ms();
    let mut tx = state.db.begin().await?;
    problems::insert_with_status(
        &mut tx,
        &id,
        project,
        form,
        status,
        principal.handle.as_deref(),
        now,
    )
    .await?;
    for (position, proof) in proofs.iter().enumerate() {
        let selection = keep(&mut tx, project, principal.handle.as_deref(), proof).await?;
        problems::cite(&mut tx, project, &id, &selection, position).await?;
    }
    let actor = principal.handle.as_deref().unwrap_or("device");
    audit::record(
        &mut *tx,
        Some(project),
        actor,
        &format!("Filed problem {}", form.title.trim()),
    )
    .await?;
    tx.commit().await?;
    problems::load(&state.db, &id).await
}

/// Lists every problem summary of `project`.
pub async fn list(
    state: &AppState,
    principal: &Principal,
    project: &str,
) -> Result<Vec<ProblemSummary>, ApiError> {
    access(state, principal, project)
        .await?
        .require(Role::Reader)?;
    problems::list(&state.db, project).await
}

/// Lists every problem of `project` in full.
pub async fn list_details(
    state: &AppState,
    principal: &Principal,
    project: &str,
) -> Result<Vec<Problem>, ApiError> {
    access(state, principal, project)
        .await?
        .require(Role::Reader)?;
    problems::list_details(&state.db, project).await
}

/// Analyzes a capture's review findings (or requests visual inspection of its first checkpoint
/// when no finding exists) and opens deduplicated, unsuppressed problems in `project`.
pub async fn analyze_capture(
    state: &AppState,
    principal: &Principal,
    project: &str,
    capture_id: &str,
) -> Result<Vec<Problem>, ApiError> {
    match access(state, principal, project).await? {
        Access::Device => {}
        person @ Access::Person(_) => person.require(Role::Reviewer)?,
    }
    let capture = captures::load(&state.db, capture_id).await?;
    if capture.project_id != project || capture.deleted.is_some() {
        return Err(ApiError::NotFound);
    }
    let mut existing = problems::list_details(&state.db, project).await?;
    let mut created = Vec::new();
    if capture.findings.is_empty() {
        if let Some(first) = capture.checkpoints.first() {
            let candidate = infer_from_finding(
                &format!(
                    "[{} @ {}×{} {}] Visual inspection requested on capture '{}'",
                    first.id,
                    capture.environment.width.unwrap_or(1440),
                    capture.environment.height.unwrap_or(900),
                    capture.environment.color_scheme.as_deref().unwrap_or(""),
                    capture.scenario
                ),
                Some(&first.artifact.id),
            );
            if !existing
                .iter()
                .any(|problem| problem.diagnosis == candidate.diagnosis)
            {
                let problem = create_with_status(
                    state,
                    principal,
                    project,
                    &candidate,
                    ProblemStatus::NeedsReprocess,
                )
                .await?;
                created.push(problem);
            }
        }
    } else {
        for finding in &capture.findings {
            let candidate =
                infer_from_finding(&finding.note, finding.evidence_artifact_id.as_deref());
            let rule_code = candidate.rules.first().map_or("", |r| r.code.as_str());
            if is_suppressed(&existing, &candidate.route, rule_code, &candidate.title)
                || existing
                    .iter()
                    .any(|problem| problem.diagnosis == candidate.diagnosis)
            {
                continue;
            }
            let problem =
                create_with_status(state, principal, project, &candidate, ProblemStatus::Open)
                    .await?;
            existing.push(problem.clone());
            created.push(problem);
        }
    }
    Ok(created)
}

/// Loads one problem after checking the caller may read its project.
pub async fn get(state: &AppState, principal: &Principal, id: &str) -> Result<Problem, ApiError> {
    problem_access(state, principal, id, Role::Reader).await?;
    problems::load(&state.db, id).await
}

/// Triages or edits a problem.
pub async fn update(
    state: &AppState,
    principal: &Principal,
    id: &str,
    patch: &ProblemPatch,
) -> Result<Problem, ApiError> {
    let project = problem_access(state, principal, id, Role::Reviewer).await?;
    validate_patch(patch)?;
    let mut tx = state.db.begin().await?;
    problems::update(&mut tx, id, patch, now_ms()).await?;
    let actor = principal.handle.as_deref().unwrap_or("device");
    audit::record(
        &mut *tx,
        Some(&project),
        actor,
        &format!("Triaged problem {id}"),
    )
    .await?;
    tx.commit().await?;
    problems::load(&state.db, id).await
}

/// Adds a comment to a problem.
pub async fn comment(
    state: &AppState,
    principal: &Principal,
    id: &str,
    form: &NewComment,
) -> Result<Comment, ApiError> {
    problem_access(state, principal, id, Role::Reviewer).await?;
    let text = validate_comment(&form.text)?;
    let comment = Comment {
        id: uuid::Uuid::now_v7().to_string(),
        author: principal.handle.clone(),
        text: text.to_owned(),
        created_at: now_ms(),
    };
    let mut tx = state.db.begin().await?;
    problems::insert_comment(&mut tx, id, &comment).await?;
    problems::touch(&mut tx, id, comment.created_at).await?;
    tx.commit().await?;
    Ok(comment)
}

/// Makes `id` cite `selection` of the same project.
pub async fn cite_selection(
    state: &AppState,
    principal: &Principal,
    id: &str,
    selection: &str,
) -> Result<Problem, ApiError> {
    let project = problem_access(state, principal, id, Role::Reviewer).await?;
    if selections::project_of(&state.db, selection)
        .await?
        .as_deref()
        != Some(project.as_str())
    {
        return Err(ApiError::NotFound);
    }
    let mut tx = state.db.begin().await?;
    let next = problems::next_selection_position(&mut tx, id).await?;
    problems::cite(&mut tx, &project, id, selection, next).await?;
    problems::touch(&mut tx, id, now_ms()).await?;
    tx.commit().await?;
    problems::load(&state.db, id).await
}

/// Removes `selection` from `id`'s cited selections; the selection itself stays.
pub async fn uncite_selection(
    state: &AppState,
    principal: &Principal,
    id: &str,
    selection: &str,
) -> Result<Problem, ApiError> {
    problem_access(state, principal, id, Role::Reviewer).await?;
    let mut tx = state.db.begin().await?;
    problems::uncite(&mut tx, id, selection).await?;
    problems::touch(&mut tx, id, now_ms()).await?;
    tx.commit().await?;
    problems::load(&state.db, id).await
}
