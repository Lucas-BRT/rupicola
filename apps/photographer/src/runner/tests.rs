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
