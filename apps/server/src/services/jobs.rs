//! Capture jobs: asking for one, a runner taking it, reporting and finishing it.

use crate::{
    domain::{
        ApiError, Principal,
        job::{LEASE_MS, check_error, validate},
        secret::now_ms,
    },
    repositories::{captures, jobs, scenarios},
    services::access::{require, require_publisher, require_reader},
    state::AppState,
};
use rupicola_api::{Job, JobFailure, JobProgress, JobResult, JobStatus, NewJob, Role};

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
pub async fn complete(
    state: &AppState,
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
    if held {
        one(state, id).await
    } else {
        not_held(state, id).await
    }
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
