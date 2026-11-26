//! Job execution loop: claims queued jobs, builds worktrees, starts previews, captures
//! matrices, checks determinism, and reports progress/completion.

use crate::{
    capture::{CellCapturer, CellContext, capture_and_publish, check_determinism},
    preview::{PreviewRuntime, ProcessPreviewRuntime},
    worktree::{GitWorktreeCheckout, TreeCheckout},
};
use anyhow::{Context, Result, bail};
use rupicola_api::{
    DEFAULT_VIEWPORT, Job, JobCapture, JobFailure, JobProgress, JobResult, JobStatus,
    PreviewConfig, Scenario, Viewport,
};
use rupicola_client::Api;
use std::{
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU32, Ordering},
    },
    time::Duration,
};
use tracing::{info, warn};

const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(30);
const MAX_JOB_ERROR_CHARS: usize = 2000;

fn matrix_cells(scenario: &Scenario) -> Vec<(Viewport, Option<String>)> {
    let viewports: Vec<Viewport> = if scenario.matrix.viewports.is_empty() {
        vec![DEFAULT_VIEWPORT]
    } else {
        scenario.matrix.viewports.clone()
    };
    let themes: Vec<Option<String>> = if scenario.matrix.themes.is_empty() {
        vec![None]
    } else {
        scenario.matrix.themes.iter().cloned().map(Some).collect()
    };
    let mut cells = Vec::with_capacity(viewports.len() * themes.len());
    for viewport in &viewports {
        for theme in &themes {
            cells.push((*viewport, theme.clone()));
        }
    }
    cells
}

fn truncate_error(msg: &str) -> String {
    msg.chars().take(MAX_JOB_ERROR_CHARS).collect()
}

/// Polls for queued jobs in `project` and runs them until stopped (or once if `once` is set).
///
/// # Errors
///
/// Fails when `claim_job` itself fails. Job execution errors are reported via `fail_job`.
#[allow(clippy::too_many_arguments)]
pub async fn run_loop<A: Api + 'static>(
    api: &A,
    token: &str,
    project: &str,
    repo_root: &Path,
    preview: &PreviewConfig,
    capturer: &dyn CellCapturer,
    once: bool,
    poll_interval: Duration,
) -> Result<()> {
    info!(project, "photographer listening for capture jobs");
    loop {
        if let Some(job) = api.claim_job(token, project).await? {
            info!(job_id = %job.id, scenario = %job.scenario, "claimed job");
            if let Err(err) =
                execute_job(api, token, project, repo_root, preview, capturer, &job).await
            {
                warn!(job_id = %job.id, error = %err, "job failed");
                let _ = api
                    .fail_job(
                        token,
                        &job.id,
                        &JobFailure {
                            error: truncate_error(&format!("{err:#}")),
                        },
                    )
                    .await;
            }
            if once {
                break;
            }
        } else {
            if once {
                info!("no queued jobs");
                break;
            }
            tokio::select! {
                () = tokio::time::sleep(poll_interval) => {}
                _ = tokio::signal::ctrl_c() => {
                    info!("shutting down photographer");
                    break;
                }
            }
        }
    }
    Ok(())
}

async fn execute_job<A: Api>(
    api: &A,
    token: &str,
    project: &str,
    repo_root: &Path,
    preview_cfg: &PreviewConfig,
    capturer: &dyn CellCapturer,
    job: &Job,
) -> Result<()> {
    let (scenario, cells) = if let Some(spec) = &job.plan_spec {
        let viewport = Viewport {
            width: spec.viewport[0],
            height: spec.viewport[1],
            scale: (spec.scale_milli / rupicola_api::DEFAULT_SCALE_MILLI).max(1),
        };
        let synthetic = Scenario {
            name: job.scenario.clone(),
            url: spec.url_path.clone(),
            environment: rupicola_api::ScenarioEnvironment {
                reduced_motion: spec.reduced_motion,
                locale: spec.locale.clone(),
            },
            matrix: rupicola_api::Matrix::default(),
            themes: std::collections::BTreeMap::new(),
            steps: spec.steps.clone(),
            checks: rupicola_api::Checks::default(),
        };
        (synthetic, vec![(viewport, spec.color_scheme.clone())])
    } else {
        let version = api
            .scenario_version(token, project, &job.scenario, &job.hash)
            .await
            .context("fetch scenario version")?;
        let cells = matrix_cells(&version.scenario);
        (version.scenario, cells)
    };
    let scratch = tempfile::tempdir().context("create runner scratch directory")?;

    let cells_done = Arc::new(AtomicU32::new(0));
    let cancelled = Arc::new(AtomicBool::new(false));

    let outcome = tokio::select! {
        res = run_job_trees(
            api,
            token,
            project,
            repo_root,
            preview_cfg,
            capturer,
            job,
            &scenario,
            &cells,
            scratch.path(),
            &cells_done,
            &cancelled,
        ) => res,
        () = async {
            loop {
                tokio::time::sleep(HEARTBEAT_INTERVAL).await;
                if cancelled.load(Ordering::Relaxed) {
                    continue;
                }
                let done = cells_done.load(Ordering::Relaxed);
                if let Ok(updated) = api
                    .job_progress(token, &job.id, &JobProgress { cells_done: done })
                    .await
                    && updated.status == JobStatus::Cancelled
                {
                    cancelled.store(true, Ordering::Relaxed);
                }
            }
        } => unreachable!(),
    };

    if cancelled.load(Ordering::Relaxed) {
        info!(job_id = %job.id, "job was cancelled; stopped cleanly");
        return Ok(());
    }

    let job_captures = outcome?;
    api.complete_job(
        token,
        &job.id,
        &JobResult {
            captures: job_captures,
        },
    )
    .await
    .context("complete job")?;
    info!(job_id = %job.id, "job completed");
    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn run_job_trees<A: Api>(
    api: &A,
    token: &str,
    project: &str,
    repo_root: &Path,
    preview_cfg: &PreviewConfig,
    capturer: &dyn CellCapturer,
    job: &Job,
    scenario: &Scenario,
    cells: &[(Viewport, Option<String>)],
    scratch_root: &Path,
    cells_done: &Arc<AtomicU32>,
    cancelled: &Arc<AtomicBool>,
) -> Result<Vec<JobCapture>> {
    let mut job_captures = Vec::with_capacity(job.trees.len() * cells.len());
    let mut checked_determinism = false;

    for tree in &job.trees {
        if cancelled.load(Ordering::Relaxed) {
            break;
        }
        let mut worktree = GitWorktreeCheckout
            .checkout(repo_root, scratch_root, tree)
            .await?;
        let preview = match ProcessPreviewRuntime
            .start(worktree.path(), repo_root, preview_cfg)
            .await
        {
            Ok(proc) => proc,
            Err(err) => {
                let _ = worktree.remove().await;
                return Err(err);
            }
        };

        let base_url = preview.base_url();
        let tree_result: Result<()> = async {
            for (idx, (viewport, theme)) in cells.iter().enumerate() {
                if cancelled.load(Ordering::Relaxed) {
                    break;
                }
                let cell_dir = scratch_root
                    .join("captures")
                    .join(&tree.name)
                    .join(idx.to_string());
                tokio::fs::create_dir_all(&cell_dir).await?;

                let ctx = CellContext {
                    project,
                    scenario,
                    scenario_hash: &job.hash,
                    base_url: &base_url,
                    revision: worktree.revision(),
                    viewport: *viewport,
                    theme: theme.as_deref(),
                    frozen_spec: job.plan_spec.as_ref(),
                    parent_capture_id: job.parent_capture_id.as_deref(),
                };
                let (capture, cp_hashes) =
                    capture_and_publish(api, token, capturer, &ctx, &cell_dir).await?;

                if !checked_determinism {
                    checked_determinism = true;
                    let det_dir = scratch_root.join("determinism");
                    tokio::fs::create_dir_all(&det_dir).await?;
                    let _ = check_determinism(
                        api,
                        token,
                        capturer,
                        &ctx,
                        &cp_hashes,
                        &capture.id,
                        &det_dir,
                    )
                    .await;
                }

                job_captures.push(JobCapture {
                    capture: capture.id,
                    tree: tree.name.clone(),
                    viewport: viewport.to_string(),
                    theme: theme.clone().unwrap_or_default(),
                });

                let done = cells_done.fetch_add(1, Ordering::Relaxed) + 1;
                let updated = api
                    .job_progress(token, &job.id, &JobProgress { cells_done: done })
                    .await?;
                if updated.status == JobStatus::Cancelled {
                    cancelled.store(true, Ordering::Relaxed);
                    break;
                }
            }
            Ok(())
        }
        .await;

        preview.stop().await;
        worktree.remove().await?;
        tree_result?;
    }

    if !cancelled.load(Ordering::Relaxed) && job_captures.len() != usize::try_from(job.cells_total)?
    {
        bail!(
            "captured {} cells but job expected {}",
            job_captures.len(),
            job.cells_total
        );
    }

    Ok(job_captures)
}

#[cfg(test)]
mod tests;
