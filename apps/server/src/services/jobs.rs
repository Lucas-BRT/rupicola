//! Capture jobs: asking for one, a runner taking it, reporting and finishing it.

use crate::{
    domain::{
        ApiError, Principal,
        fix::{AFTER_LABEL, BEFORE_LABEL},
        job::{
            LEASE_MS, TREE_APPROVED, TREE_APPROVED_FIX, TREE_BASE, TREE_FIX, check_error, validate,
        },
        pixel_diff::{DEFAULT_CHANNEL_TOLERANCE, diff_pngs},
        secret::now_ms,
    },
    repositories::{captures, fixes, jobs, scenarios},
    services::{
        access::{require, require_publisher, require_reader},
        captures::put_project_blob,
        selections::keep,
    },
    state::{AppState, Shared},
};
use rupicola_api::{
    Capture, Comparison, ComparisonView, FixSelectionRole, Job, JobFailure, JobProgress, JobResult,
    JobStatus, NewJob, NewSelection, Role, SelectionItem, SelectionMark,
};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashMap};

/// Who is calling, for holding a job: the credential, since two devices share a label.
fn key(principal: &Principal) -> &str {
    principal.key()
}

/// The project of a job, after checking the caller may work on it.
async fn project_of(
    state: &AppState,
    principal: &Principal,
    job: &str,
    reader: bool,
) -> Result<String, ApiError> {
    let project = jobs::project_of(&state.db, job)
        .await?
        .ok_or(ApiError::NotFound)?;
    if reader {
        require_reader(state, principal, &project).await?;
    } else {
        require_publisher(state, principal, &project, Role::Reviewer).await?;
    }
    Ok(project)
}

async fn one(state: &AppState, job: &str) -> Result<Job, ApiError> {
    jobs::get(&state.db, job).await?.ok_or(ApiError::NotFound)
}

/// Queues a job to run a scenario on trees. Only a person may ask.
pub async fn create(
    state: &AppState,
    principal: &Principal,
    project: &str,
    form: &NewJob,
) -> Result<Job, ApiError> {
    require(state, principal, project, Role::Reviewer).await?;
    validate(form)?;
    let version = match &form.hash {
        Some(hash) => scenarios::version(&state.db, project, &form.scenario, hash).await?,
        None => scenarios::latest(&state.db, project, &form.scenario).await?,
    }
    .ok_or_else(|| ApiError::invalid("the project has no such scenario version"))?;
    let cells = version.scenario.matrix.cells() * form.trees.len();
    let id = uuid::Uuid::now_v7().to_string();
    jobs::insert(
        &state.db,
        &jobs::NewRow {
            id: &id,
            project,
            scenario: &form.scenario,
            hash: &version.hash,
            fix: form.fix.as_deref(),
            trees: &form.trees,
            cells_total: u32::try_from(cells).map_err(anyhow::Error::from)?,
            created_by: principal.handle.as_deref(),
            now: now_ms(),
            plan_spec: None,
            parent_capture_id: None,
        },
    )
    .await?;
    one(state, &id).await
}

/// Queues a deterministic recapture job pinned to `capture_id`'s frozen [`rupicola_api::CapturePlanSpec`].
pub async fn recapture_from_capture(
    state: &AppState,
    principal: &Principal,
    capture_id: &str,
    form: &rupicola_api::RecaptureRequest,
) -> Result<Job, ApiError> {
    let source = captures::load(&state.db, capture_id).await?;
    require(state, principal, &source.project_id, Role::Reviewer).await?;
    let spec = source.plan_spec.ok_or_else(|| {
        ApiError::invalid(
            "source capture has no canonical plan_spec; cannot recapture deterministically",
        )
    })?;
    let trees = if form.trees.is_empty() {
        let base_ref =
            crate::services::settings::project_base_ref(state, &source.project_id).await?;
        vec![rupicola_api::TreeSpec {
            name: "recapture".to_owned(),
            base: base_ref,
            merge: Vec::new(),
            fixes: Vec::new(),
        }]
    } else {
        form.trees.clone()
    };
    let id = uuid::Uuid::now_v7().to_string();
    let norm = spec.normalized();
    let digest = norm.digest();
    jobs::insert(
        &state.db,
        &jobs::NewRow {
            id: &id,
            project: &source.project_id,
            scenario: &source.scenario,
            hash: &digest,
            fix: form.fix.as_deref(),
            trees: &trees,
            cells_total: u32::try_from(trees.len()).map_err(anyhow::Error::from)?,
            created_by: principal.handle.as_deref(),
            now: now_ms(),
            plan_spec: Some(&norm),
            parent_capture_id: Some(capture_id),
        },
    )
    .await?;
    one(state, &id).await
}

/// The project's jobs, newest first; `open_only` keeps the ones still waiting or running.
pub async fn list(
    state: &AppState,
    principal: &Principal,
    project: &str,
    open_only: bool,
) -> Result<Vec<Job>, ApiError> {
    require_reader(state, principal, project).await?;
    jobs::list(&state.db, project, open_only).await
}

/// One job.
pub async fn get(state: &AppState, principal: &Principal, id: &str) -> Result<Job, ApiError> {
    project_of(state, principal, id, true).await?;
    one(state, id).await
}

/// Gives the caller the oldest job it can take, if there is one.
pub async fn claim(
    state: &AppState,
    principal: &Principal,
    project: &str,
) -> Result<Option<Job>, ApiError> {
    require_publisher(state, principal, project, Role::Reviewer).await?;
    let id = jobs::claim(
        &state.db,
        project,
        key(principal),
        principal.actor(),
        now_ms(),
        LEASE_MS,
    )
    .await?;
    match id {
        Some(id) => one(state, &id).await.map(Some),
        None => Ok(None),
    }
}

/// What a runner that does not hold the job is told: cancelled jobs are reported as such so it
/// stops, anything else is a conflict.
async fn not_held(state: &AppState, id: &str) -> Result<Job, ApiError> {
    let job = one(state, id).await?;
    if job.status == JobStatus::Cancelled {
        Ok(job)
    } else {
        Err(ApiError::Conflict("this runner does not hold the job"))
    }
}

/// Records progress and extends the lease. A cancelled job comes back as such, so the runner
/// stops.
pub async fn progress(
    state: &AppState,
    principal: &Principal,
    id: &str,
    progress: JobProgress,
) -> Result<Job, ApiError> {
    project_of(state, principal, id, false).await?;
    let job = one(state, id).await?;
    if progress.cells_done > job.cells_total {
        return Err(ApiError::invalid("more captures done than the job makes"));
    }
    if jobs::progress(
        &state.db,
        id,
        key(principal),
        progress.cells_done,
        now_ms(),
        LEASE_MS,
    )
    .await?
    {
        one(state, id).await
    } else {
        not_held(state, id).await
    }
}

/// Finishes a job with the captures it made, which must be the project's own.
/// If the job is for a fix, pairs captures into isolated/integrated comparisons with pixel diffs.
pub async fn complete(
    state: &Shared,
    principal: &Principal,
    id: &str,
    result: &JobResult,
) -> Result<Job, ApiError> {
    let project = project_of(state, principal, id, false).await?;
    for capture in &result.captures {
        if captures::project_of(&state.db, &capture.capture)
            .await?
            .as_deref()
            != Some(project.as_str())
        {
            return Err(ApiError::invalid(
                "the captures must belong to this project",
            ));
        }
    }
    let mut tx = state.db.begin().await?;
    let held = jobs::complete(&mut tx, id, key(principal), &result.captures, now_ms()).await?;
    tx.commit().await?;
    if !held {
        return not_held(state, id).await;
    }
    let job = one(state, id).await?;
    if let Some(fix_id) = &job.fix {
        pair_fix_comparisons(state, principal, &project, &job, fix_id).await?;
    }
    Ok(job)
}

async fn read_blob(state: &Shared, sha256: String) -> Result<Option<Vec<u8>>, ApiError> {
    let shared = state.clone();
    tokio::task::spawn_blocking(move || {
        let Some(size) = shared.blobs.size(&sha256).map_err(anyhow::Error::from)? else {
            return Ok(None);
        };
        let len = usize::try_from(size).map_err(anyhow::Error::from)?;
        shared.blobs.read_range(&sha256, 0, len).map(Some)
    })
    .await
    .map_err(anyhow::Error::from)?
}

async fn compute_diff(
    state: &Shared,
    project: &str,
    scenario: &str,
    checkpoint_id: &str,
    before_sha: &str,
    after_sha: &str,
) -> Result<(Option<String>, Option<f64>), ApiError> {
    let Some(before_bytes) = read_blob(state, before_sha.to_owned()).await? else {
        return Ok((None, None));
    };
    let Some(after_bytes) = read_blob(state, after_sha.to_owned()).await? else {
        return Ok((None, None));
    };
    let diff = tokio::task::spawn_blocking(move || {
        diff_pngs(&before_bytes, &after_bytes, DEFAULT_CHANNEL_TOLERANCE, &[])
    })
    .await
    .map_err(anyhow::Error::from)?;
    let Ok(diff) = diff else {
        return Ok((None, None));
    };
    let diff_sha = format!("{:x}", Sha256::digest(&diff.diff_png));
    let diff_bytes = u64::try_from(diff.diff_png.len()).map_err(anyhow::Error::from)?;
    put_project_blob(state, project, diff_sha.clone(), diff.diff_png).await?;
    let mut tx = state.db.begin().await?;
    let diff_id = captures::insert_diff_artifact(
        &mut tx,
        project,
        scenario,
        checkpoint_id,
        &diff_sha,
        diff_bytes,
        now_ms(),
    )
    .await?;
    tx.commit().await?;
    Ok((Some(diff_id), Some(diff.changed_ratio)))
}

async fn pair_fix_comparisons(
    state: &Shared,
    principal: &Principal,
    project: &str,
    job: &Job,
    fix_id: &str,
) -> Result<(), ApiError> {
    if fixes::project_of(&state.db, fix_id).await?.as_deref() != Some(project) {
        return Ok(());
    }
    let route = scenarios::version(&state.db, project, &job.scenario, &job.hash)
        .await?
        .map_or_else(|| "/".to_owned(), |v| v.scenario.url);

    let mut by_key: BTreeMap<(String, String, String), Capture> = BTreeMap::new();
    let mut cells: Vec<(String, String)> = Vec::new();
    for jc in &job.captures {
        let cap = captures::load(&state.db, &jc.capture).await?;
        let cell = (jc.viewport.clone(), jc.theme.clone());
        if !cells.contains(&cell) {
            cells.push(cell.clone());
        }
        by_key.insert((jc.tree.clone(), cell.0, cell.1), cap);
    }

    let mut view_specs = Vec::new();
    let has_tree = |name: &str| job.trees.iter().any(|t| t.name == name);
    if has_tree(TREE_BASE) && has_tree(TREE_FIX) {
        view_specs.push((ComparisonView::Isolated, TREE_BASE, TREE_FIX, Vec::new()));
    }
    if let Some(approved_tree) = job.trees.iter().find(|t| t.name == TREE_APPROVED)
        && has_tree(TREE_APPROVED_FIX)
    {
        view_specs.push((
            ComparisonView::Integrated,
            TREE_APPROVED,
            TREE_APPROVED_FIX,
            approved_tree.fixes.clone(),
        ));
    }
    if view_specs.is_empty() {
        return Ok(());
    }

    let mut comparisons = Vec::new();
    let mut produced_views = Vec::with_capacity(view_specs.len());
    for (view, before_tree, after_tree, applied_fixes) in view_specs {
        produced_views.push(view);
        for (viewport, theme) in &cells {
            let Some(before_cap) =
                by_key.get(&(before_tree.to_owned(), viewport.clone(), theme.clone()))
            else {
                continue;
            };
            let Some(after_cap) =
                by_key.get(&(after_tree.to_owned(), viewport.clone(), theme.clone()))
            else {
                continue;
            };
            let before_by_id: HashMap<&str, &rupicola_api::Checkpoint> = before_cap
                .checkpoints
                .iter()
                .map(|c| (c.id.as_str(), c))
                .collect();
            for after_cp in &after_cap.checkpoints {
                let Some(prior_cp) = before_by_id.get(after_cp.id.as_str()) else {
                    continue;
                };
                let (diff, changed_ratio) = compute_diff(
                    state,
                    project,
                    &job.scenario,
                    &after_cp.id,
                    &prior_cp.artifact.sha256,
                    &after_cp.artifact.sha256,
                )
                .await?;
                let label = if theme.is_empty() {
                    format!("{route} {viewport} ({})", after_cp.id)
                } else {
                    format!("{route} {viewport} {theme} ({})", after_cp.id)
                };
                comparisons.push(Comparison {
                    label,
                    view,
                    base_revision: before_cap.revision.clone().unwrap_or_default(),
                    revision: after_cap.revision.clone().unwrap_or_default(),
                    applied_fixes: applied_fixes.clone(),
                    stale: false,
                    route: route.clone(),
                    viewport: viewport.clone(),
                    theme: theme.clone(),
                    before: prior_cp.artifact.id.clone(),
                    after: after_cp.artifact.id.clone(),
                    diff,
                    changed_ratio,
                });
            }
        }
    }

    let mut tx = state.db.begin().await?;
    fixes::replace_comparisons_for_views(&mut tx, fix_id, &produced_views, &comparisons).await?;
    if produced_views.contains(&ComparisonView::Isolated) {
        replace_isolated_selections(&mut tx, project, principal, fix_id, &comparisons).await?;
    }
    fixes::touch(&mut tx, fix_id, now_ms()).await?;
    tx.commit().await?;
    Ok(())
}

async fn replace_isolated_selections(
    tx: &mut sqlx::SqliteConnection,
    project: &str,
    principal: &Principal,
    fix_id: &str,
    comparisons: &[Comparison],
) -> Result<(), ApiError> {
    let mut before_items = Vec::new();
    let mut after_items = Vec::new();
    for comp in comparisons
        .iter()
        .filter(|c| c.view == ComparisonView::Isolated)
    {
        if let Some(cap) = fixes::capture_of_artifact(tx, project, &comp.before).await? {
            before_items.push(SelectionItem {
                capture: cap,
                artifact: Some(comp.before.clone()),
                path: None,
                mark: SelectionMark::Proof,
                fragment: None,
            });
        }
        if let Some(cap) = fixes::capture_of_artifact(tx, project, &comp.after).await? {
            after_items.push(SelectionItem {
                capture: cap,
                artifact: Some(comp.after.clone()),
                path: None,
                mark: SelectionMark::Proof,
                fragment: None,
            });
        }
        if let Some(diff_id) = &comp.diff
            && let Some(cap) = fixes::capture_of_artifact(tx, project, diff_id).await?
        {
            after_items.push(SelectionItem {
                capture: cap,
                artifact: Some(diff_id.clone()),
                path: None,
                mark: SelectionMark::Proof,
                fragment: None,
            });
        }
    }
    let mut links = Vec::new();
    for (role, label, items) in [
        (FixSelectionRole::Baseline, BEFORE_LABEL, before_items),
        (FixSelectionRole::SolutionProof, AFTER_LABEL, after_items),
    ] {
        if !items.is_empty() {
            let sel_id = keep(
                tx,
                project,
                principal.handle.as_deref(),
                &NewSelection {
                    label: label.to_owned(),
                    items,
                },
            )
            .await?;
            links.push((role, sel_id));
        }
    }
    fixes::replace_auto_selections(tx, project, fix_id, BEFORE_LABEL, AFTER_LABEL, &links).await
}

/// Fails a job the caller holds.
pub async fn fail(
    state: &AppState,
    principal: &Principal,
    id: &str,
    failure: &JobFailure,
) -> Result<Job, ApiError> {
    project_of(state, principal, id, false).await?;
    check_error(&failure.error)?;
    if jobs::fail(
        &state.db,
        id,
        key(principal),
        failure.error.trim(),
        now_ms(),
    )
    .await?
    {
        one(state, id).await
    } else {
        not_held(state, id).await
    }
}

/// Cancels a job that has not finished. Only a person may.
pub async fn cancel(state: &AppState, principal: &Principal, id: &str) -> Result<Job, ApiError> {
    let project = jobs::project_of(&state.db, id)
        .await?
        .ok_or(ApiError::NotFound)?;
    require(state, principal, &project, Role::Reviewer).await?;
    if !jobs::cancel(&state.db, id, now_ms()).await? {
        return Err(ApiError::Conflict("the job already finished"));
    }
    one(state, id).await
}
