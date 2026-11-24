//! Fixes that answer problems: filing with before/after proof, steps and review.

use crate::{
    domain::{
        ApiError, Principal,
        fix::{AFTER_LABEL, BEFORE_LABEL, check_step, validate, validate_review},
        secret::now_ms,
    },
    repositories::{audit, fixes, selections},
    services::{
        access::{require, require_publisher, require_reader},
        captures::require_project_artifact,
        selections::keep,
    },
    state::AppState,
};
use rupicola_api::{
    Fix, FixSelectionLink, FixSelectionRole, FixSummary, NewFix, NewReview, NewSelection, NewStep,
    ProblemStatus, Role, SelectionItem, SelectionMark, Step,
};
use sqlx::SqliteConnection;
use std::collections::BTreeSet;

/// Project of a fix, checked against the caller's access.
async fn fix_access(
    state: &AppState,
    principal: &Principal,
    fix: &str,
    role: Role,
) -> Result<String, ApiError> {
    let project = fixes::project_of(&state.db, fix)
        .await?
        .ok_or(ApiError::NotFound)?;
    if role == Role::Reader {
        require_reader(state, principal, &project).await?;
    } else {
        require(state, principal, &project, role).await?;
    }
    Ok(project)
}

/// Fails unless the fix's problems and artifacts all belong to `project`; returns the
/// distinct problem ids.
async fn check_references<'a>(
    state: &AppState,
    project: &str,
    form: &'a NewFix,
) -> Result<BTreeSet<&'a str>, ApiError> {
    let problems: BTreeSet<&str> = form.problems.iter().map(String::as_str).collect();
    for problem in &problems {
        if !fixes::problem_belongs(&state.db, problem, project).await? {
            return Err(ApiError::invalid(
                "the problems must belong to this project",
            ));
        }
    }
    for comparison in &form.comparisons {
        for artifact in [
            Some(&comparison.before),
            Some(&comparison.after),
            comparison.diff.as_ref(),
        ]
        .into_iter()
        .flatten()
        {
            require_project_artifact(state, project, artifact).await?;
        }
    }
    for artifact in form.steps.iter().filter_map(|step| step.evidence.as_ref()) {
        require_project_artifact(state, project, artifact).await?;
    }
    Ok(problems)
}

/// An anchor of proof at an artifact the caller already checked belongs to the project.
async fn anchor(
    tx: &mut SqliteConnection,
    project: &str,
    artifact: &str,
) -> Result<SelectionItem, ApiError> {
    let capture = fixes::capture_of_artifact(tx, project, artifact)
        .await?
        .ok_or(ApiError::invalid(
            "the comparisons must use files of this project",
        ))?;
    Ok(SelectionItem {
        capture,
        artifact: Some(artifact.to_owned()),
        path: None,
        mark: SelectionMark::Proof,
        fragment: None,
    })
}

/// The selections a new fix cites: its comparisons' before files as the baseline and their
/// after and diff files as proof, then any selections it brings.
async fn proofs(
    tx: &mut SqliteConnection,
    project: &str,
    form: &NewFix,
) -> Result<Vec<(FixSelectionRole, NewSelection)>, ApiError> {
    let mut before = Vec::new();
    let mut after = Vec::new();
    for comparison in &form.comparisons {
        before.push(anchor(tx, project, &comparison.before).await?);
        after.push(anchor(tx, project, &comparison.after).await?);
        if let Some(diff) = &comparison.diff {
            after.push(anchor(tx, project, diff).await?);
        }
    }
    let mut all = Vec::new();
    for (role, label, items) in [
        (FixSelectionRole::Baseline, BEFORE_LABEL, before),
        (FixSelectionRole::SolutionProof, AFTER_LABEL, after),
    ] {
        if !items.is_empty() {
            all.push((
                role,
                NewSelection {
                    label: label.to_owned(),
                    items,
                },
            ));
        }
    }
    all.extend(
        form.baseline
            .iter()
            .cloned()
            .map(|s| (FixSelectionRole::Baseline, s)),
    );
    all.extend(
        form.proof
            .iter()
            .cloned()
            .map(|s| (FixSelectionRole::SolutionProof, s)),
    );
    Ok(all)
}

/// Files a new fix in `project`, moving its problems to `in-progress` and citing its proofs.
pub async fn create(
    state: &AppState,
    principal: &Principal,
    project: &str,
    form: &NewFix,
) -> Result<Fix, ApiError> {
    require_publisher(state, principal, project, Role::Reviewer).await?;
    validate(form)?;
    let problems = check_references(state, project, form).await?;
    let id = uuid::Uuid::now_v7().to_string();
    let now = now_ms();
    let mut tx = state.db.begin().await?;
    fixes::insert(
        &mut tx,
        &id,
        project,
        form,
        principal.handle.as_deref(),
        now,
    )
    .await?;
    for problem in &problems {
        fixes::link_problem(&mut tx, &id, problem).await?;
        fixes::set_problem_status(&mut tx, problem, ProblemStatus::InProgress, now).await?;
    }
    fixes::insert_details(&mut tx, &id, form, now).await?;
    for (position, (role, proof)) in proofs(&mut tx, project, form)
        .await?
        .into_iter()
        .enumerate()
    {
        let selection = keep(&mut tx, project, principal.handle.as_deref(), &proof).await?;
        let position = i64::try_from(position).map_err(anyhow::Error::from)?;
        fixes::cite(&mut tx, project, &id, &selection, role, position).await?;
    }
    audit::record(
        &mut *tx,
        Some(project),
        principal.actor(),
        &format!("Filed fix {}", form.title.trim()),
    )
    .await?;
    tx.commit().await?;
    fixes::load(&state.db, &id).await
}

/// Lists every fix summary of `project`.
pub async fn list(
    state: &AppState,
    principal: &Principal,
    project: &str,
) -> Result<Vec<FixSummary>, ApiError> {
    require_reader(state, principal, project).await?;
    fixes::list(&state.db, project).await
}

/// Lists every fix of `project` in full.
pub async fn list_details(
    state: &AppState,
    principal: &Principal,
    project: &str,
) -> Result<Vec<Fix>, ApiError> {
    require_reader(state, principal, project).await?;
    fixes::list_details(&state.db, project).await
}

/// Loads one fix after checking the caller may read its project.
pub async fn get(state: &AppState, principal: &Principal, id: &str) -> Result<Fix, ApiError> {
    fix_access(state, principal, id, Role::Reader).await?;
    fixes::load(&state.db, id).await
}

/// Records one more verified step on `id`.
pub async fn add_step(
    state: &AppState,
    principal: &Principal,
    id: &str,
    step: NewStep,
) -> Result<Step, ApiError> {
    let project = fixes::project_of(&state.db, id)
        .await?
        .ok_or(ApiError::NotFound)?;
    require_publisher(state, principal, &project, Role::Reviewer).await?;
    check_step(&step)?;
    if let Some(artifact) = &step.evidence {
        require_project_artifact(state, &project, artifact).await?;
    }
    let now = now_ms();
    let mut tx = state.db.begin().await?;
    fixes::insert_step(&mut tx, id, &step, now).await?;
    fixes::touch(&mut tx, id, now).await?;
    tx.commit().await?;
    Ok(Step {
        step,
        created_at: now,
    })
}

/// Records a review of `id` and updates its linked problems accordingly.
pub async fn review(
    state: &AppState,
    principal: &Principal,
    id: &str,
    form: &NewReview,
) -> Result<Fix, ApiError> {
    let project = fix_access(state, principal, id, Role::Reviewer).await?;
    let problem_status = validate_review(form)?;
    let now = now_ms();
    let mut tx = state.db.begin().await?;
    let problems = fixes::apply_review(
        &mut tx,
        id,
        principal.handle.as_deref(),
        form.outcome,
        &form.comment,
        now,
    )
    .await?;
    if let Some(status) = problem_status {
        for problem in problems {
            fixes::set_problem_status(&mut tx, &problem, status, now).await?;
        }
    }
    audit::record(
        &mut *tx,
        Some(&project),
        principal.actor(),
        &format!("Reviewed fix {id}: {}", form.outcome),
    )
    .await?;
    tx.commit().await?;
    fixes::load(&state.db, id).await
}

/// Makes `id` cite `selection` of the same project as `link.role`.
pub async fn cite_selection(
    state: &AppState,
    principal: &Principal,
    id: &str,
    selection: &str,
    link: &FixSelectionLink,
) -> Result<Fix, ApiError> {
    let project = fix_access(state, principal, id, Role::Reviewer).await?;
    if selections::project_of(&state.db, selection)
        .await?
        .as_deref()
        != Some(project.as_str())
    {
        return Err(ApiError::NotFound);
    }
    let mut tx = state.db.begin().await?;
    let next = fixes::next_selection_position(&mut tx, id).await?;
    fixes::cite(&mut tx, &project, id, selection, link.role, next).await?;
    tx.commit().await?;
    fixes::load(&state.db, id).await
}

/// Removes `selection` from `id`'s cited selections; the selection itself stays.
pub async fn uncite_selection(
    state: &AppState,
    principal: &Principal,
    id: &str,
    selection: &str,
) -> Result<Fix, ApiError> {
    fix_access(state, principal, id, Role::Reviewer).await?;
    fixes::uncite(&state.db, id, selection).await?;
    fixes::load(&state.db, id).await
}
