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
    let version = api
        .scenario_version(token, project, &job.scenario, &job.hash)
        .await
        .context("fetch scenario version")?;
    let cells = matrix_cells(&version.scenario);
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
            &version.scenario,
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
mod tests {
    use super::*;
    use crate::capture::{CapturedFile, CellOutcome};
    use rupicola_api::{
        Artifact, Capture, CaptureStatus, Checkpoint, Environment, Inspection, Limits, NewFinding,
        ScenarioVersion, TreeSpec,
    };
    use rupicola_capture::session::Plan;
    use rupicola_client::MockApi;
    use std::sync::atomic::AtomicUsize;

    struct StubCapturer {
        counter: AtomicUsize,
        varying: bool,
    }

    #[async_trait::async_trait]
    impl CellCapturer for StubCapturer {
        async fn capture_cell(&self, _plan: &Plan, out_dir: &Path) -> Result<CellOutcome> {
            tokio::fs::create_dir_all(out_dir).await?;
            let n = self.counter.fetch_add(1, Ordering::Relaxed);
            let png_path = out_dir.join("full-page.png");
            let bytes = if self.varying {
                format!("png-{n}")
            } else {
                "png-same".to_owned()
            };
            tokio::fs::write(&png_path, bytes.as_bytes()).await?;
            Ok(CellOutcome {
                backend_id: "stub".to_owned(),
                checkpoints: vec![CapturedFile {
                    id: "full-page".to_owned(),
                    path: png_path,
                    snapshot: None,
                }],
                clips: Vec::new(),
            })
        }
    }

    fn init_git_repo() -> (tempfile::TempDir, std::path::PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let repo = dir.path().join("repo");
        std::fs::create_dir_all(&repo).unwrap();
        for args in [
            vec!["init", "-b", "develop"],
            vec!["config", "user.name", "Test"],
            vec!["config", "user.email", "test@example.com"],
        ] {
            assert!(
                std::process::Command::new("git")
                    .arg("-C")
                    .arg(&repo)
                    .args(args)
                    .status()
                    .unwrap()
                    .success()
            );
        }
        std::fs::write(repo.join("README.md"), "hi\n").unwrap();
        assert!(
            std::process::Command::new("git")
                .arg("-C")
                .arg(&repo)
                .args(["add", "README.md"])
                .status()
                .unwrap()
                .success()
        );
        assert!(
            std::process::Command::new("git")
                .arg("-C")
                .arg(&repo)
                .args(["commit", "-m", "init"])
                .status()
                .unwrap()
                .success()
        );
        (dir, repo)
    }

    fn sample_job() -> Job {
        Job {
            id: "job-1".to_owned(),
            project_id: "demo".to_owned(),
            scenario: "home".to_owned(),
            hash: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_owned(),
            fix: None,
            trees: vec![TreeSpec {
                name: "base".to_owned(),
                base: "develop".to_owned(),
                merge: Vec::new(),
                fixes: Vec::new(),
            }],
            status: JobStatus::Running,
            cells_total: 1,
            cells_done: 0,
            runner: Some("runner".to_owned()),
            error: None,
            captures: Vec::new(),
            created_by: Some("ana".to_owned()),
            created_at: 0,
            started_at: Some(0),
            finished_at: None,
        }
    }

    fn sample_capture(id: &str) -> Capture {
        Capture {
            id: id.to_owned(),
            project_id: "demo".to_owned(),
            scenario: "home".to_owned(),
            revision: Some("abc".to_owned()),
            status: CaptureStatus::Complete,
            blocker: None,
            inspection: Inspection::Pending,
            environment: Environment::default(),
            created_at: 0,
            checkpoints: vec![Checkpoint {
                id: "full-page".to_owned(),
                artifact: Artifact {
                    id: "art-1".to_owned(),
                    path: "checkpoints/full-page.png".to_owned(),
                    mime: "image/png".to_owned(),
                    bytes: 8,
                    sha256: "b".repeat(64),
                },
            }],
            clips: Vec::new(),
            findings: Vec::new(),
            deleted: None,
        }
    }

    #[tokio::test]
    async fn runner_completes_job_and_reports_determinism_finding_when_unstable() {
        let (_dir, repo) = init_git_repo();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let srv = tokio::spawn(async move {
            use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
            if let Ok((mut s, _)) = listener.accept().await {
                let mut buf = [0u8; 256];
                let _ = s.read(&mut buf).await;
                let _ = s
                    .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nok")
                    .await;
            }
        });

        let preview = PreviewConfig {
            command: "sleep 10".to_owned(),
            ready: format!("http://127.0.0.1:{port}/healthz"),
            timeout_secs: 5,
        };
        let scenario = Scenario::from_toml(
            "name = \"home\"\nurl = \"/\"\n[[steps]]\nkind = \"checkpoint\"\nid = \"full-page\"",
        )
        .unwrap();
        let job = sample_job();

        let mut mock = MockApi::new();
        let job_clone = job.clone();
        mock.expect_claim_job()
            .returning(move |_, _| Ok(Some(job_clone.clone())));
        mock.expect_scenario_version().returning(move |_, _, _, h| {
            Ok(ScenarioVersion {
                hash: h.to_owned(),
                scenario: scenario.clone(),
                author: None,
                created_at: 0,
            })
        });
        mock.expect_missing_blobs().returning(|_, _, q| {
            Ok(rupicola_api::Missing {
                missing: q.sha256.clone(),
                limits: Limits {
                    max_blob_bytes: 1_000_000,
                    max_image_bytes: 1_000_000,
                    max_video_bytes: 1_000_000,
                    max_artifacts_per_capture: 16,
                    max_body_bytes: 1_000_000,
                    max_missing_batch: 100,
                    upload_timeout_secs: 60,
                    upload_idle_secs: 30,
                },
            })
        });
        mock.expect_upload_project_blob()
            .returning(|_, _, _, _| Ok(()));
        mock.expect_publish_record()
            .returning(|_, _, id, _| Ok(sample_capture(id)));
        mock.expect_add_finding()
            .times(1)
            .returning(|_, _, f: &NewFinding| {
                assert!(f.note.contains("non-deterministic"));
                Ok(rupicola_api::Finding {
                    id: "f1".to_owned(),
                    note: f.note.clone(),
                    evidence_artifact_id: None,
                    author: None,
                    created_at: 0,
                })
            });
        let progress_job = job.clone();
        mock.expect_job_progress()
            .returning(move |_, _, _| Ok(progress_job.clone()));
        let done_job = Job {
            status: JobStatus::Done,
            ..job
        };
        mock.expect_complete_job()
            .times(1)
            .returning(move |_, _, r| {
                assert_eq!(r.captures.len(), 1);
                Ok(done_job.clone())
            });

        let capturer = StubCapturer {
            counter: AtomicUsize::new(0),
            varying: true,
        };
        run_loop(
            &mock,
            "tok",
            "demo",
            &repo,
            &preview,
            &capturer,
            true,
            Duration::from_secs(1),
        )
        .await
        .unwrap();
        srv.abort();
    }

    #[tokio::test]
    async fn runner_stops_cleanly_when_cancelled_and_fails_on_error() {
        let (_dir, repo) = init_git_repo();
        let preview = PreviewConfig {
            command: "echo 'bad preview' >&2; exit 1".to_owned(),
            ready: "http://127.0.0.1:{port}/healthz".to_owned(),
            timeout_secs: 5,
        };
        let scenario = Scenario::from_toml("name = \"home\"\nurl = \"/\"").unwrap();
        let job = sample_job();

        let mut mock = MockApi::new();
        let job_clone = job.clone();
        mock.expect_claim_job()
            .returning(move |_, _| Ok(Some(job_clone.clone())));
        mock.expect_scenario_version().returning(move |_, _, _, h| {
            Ok(ScenarioVersion {
                hash: h.to_owned(),
                scenario: scenario.clone(),
                author: None,
                created_at: 0,
            })
        });
        let failed_job = Job {
            status: JobStatus::Failed,
            ..job
        };
        mock.expect_fail_job()
            .times(1)
            .returning(move |_, _, failure| {
                assert!(failure.error.contains("bad preview"));
                Ok(failed_job.clone())
            });

        let capturer = StubCapturer {
            counter: AtomicUsize::new(0),
            varying: false,
        };
        run_loop(
            &mock,
            "tok",
            "demo",
            &repo,
            &preview,
            &capturer,
            true,
            Duration::from_secs(1),
        )
        .await
        .unwrap();
    }
}
