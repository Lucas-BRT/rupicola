//! Executes declarative web captures on the server using the external `rupicola-capture`
//! crate, storing the resulting blobs directly in `state.blobs` and SQLite.

use crate::{
    domain::{ApiError, Principal, capture_run::to_plan},
    services::{access::require_publisher, captures},
    state::Shared,
};
use rupicola_api::{
    BlobRef, Capture, CaptureStatus, NewCapture, NewCheckpoint, NewClip, Role, RunCapture,
};
use rupicola_capture::asynchronous::{
    CaptureBackend, CdpBackend, CdpDiscovery, WebBackend, WebDiscovery, run_plan,
};
use sha2::{Digest, Sha256};
use std::path::Path;

const DEFAULT_BROWSER_ENGINE: &str = "chromium";
const MISSING_BACKEND_MSG: &str = "no headless web capture backend is available on this server \
    (configure RUPICOLA_PLAYWRIGHT_DIR with node, or install chromium)";

async fn discover_backend() -> Option<Box<dyn CaptureBackend>> {
    if let Some(web) = WebBackend::discover(DEFAULT_BROWSER_ENGINE).await {
        return Some(Box::new(web));
    }
    if let Some(cdp) = CdpBackend::discover().await {
        return Some(Box::new(cdp));
    }
    None
}

async fn store_artifact(
    state: &Shared,
    rel_path: String,
    disk_path: &Path,
) -> Result<BlobRef, ApiError> {
    let data = tokio::fs::read(disk_path)
        .await
        .map_err(anyhow::Error::from)?;
    let sha256 = format!("{:x}", Sha256::digest(&data));
    captures::put_blob(state.clone(), sha256.clone(), data.into()).await?;
    Ok(BlobRef {
        path: rel_path,
        sha256,
    })
}

/// Runs `form` on the server's external capture backend, stores every captured artifact as a
/// blob, and persists the resulting capture (`complete` or `blocked`) under `project`.
pub async fn run(
    state: &Shared,
    principal: &Principal,
    project: &str,
    form: RunCapture,
) -> Result<Capture, ApiError> {
    require_publisher(state, principal, project, Role::Reviewer).await?;
    let plan = to_plan(&form)?;
    let mut env = form.environment;
    env.width.get_or_insert(plan.environment.viewport[0]);
    env.height.get_or_insert(plan.environment.viewport[1]);

    let Some(backend) = discover_backend().await else {
        return captures::create(
            state,
            principal,
            project,
            NewCapture {
                scenario: form.scenario,
                revision: form.revision,
                status: CaptureStatus::Blocked,
                blocker: Some(MISSING_BACKEND_MSG.to_owned()),
                environment: env,
                checkpoints: Vec::new(),
                clips: Vec::new(),
            },
        )
        .await;
    };

    let backend_id = backend.id().to_owned();
    env.renderer.get_or_insert_with(|| backend_id.clone());
    env.browser.get_or_insert(backend_id);

    let tempdir = tempfile::tempdir().map_err(anyhow::Error::from)?;
    let outcome = run_plan(backend.as_ref(), &plan, tempdir.path()).await;
    let new_capture = match outcome {
        Ok(bundle) => {
            let mut checkpoints = Vec::with_capacity(bundle.checkpoints.len());
            for item in &bundle.checkpoints {
                let rel = format!("checkpoints/{}.png", item.id);
                let file = store_artifact(state, rel, &item.png).await?;
                checkpoints.push(NewCheckpoint {
                    id: item.id.clone(),
                    file,
                });
            }
            let mut clips = Vec::with_capacity(bundle.clips.len());
            for clip in &bundle.clips {
                let rel = format!("clips/{}.mp4", clip.id);
                let video = store_artifact(state, rel, &clip.mp4).await?;
                clips.push(NewClip {
                    id: clip.id.clone(),
                    video: Some(video),
                    sheet: None,
                    analysis: None,
                    frames: Vec::new(),
                });
            }
            NewCapture {
                scenario: form.scenario,
                revision: form.revision,
                status: CaptureStatus::Complete,
                blocker: (!bundle.warnings.is_empty()).then(|| bundle.warnings.join("; ")),
                environment: env,
                checkpoints,
                clips,
            }
        }
        Err(error) => NewCapture {
            scenario: form.scenario,
            revision: form.revision,
            status: CaptureStatus::Blocked,
            blocker: Some(error.to_string()),
            environment: env,
            checkpoints: Vec::new(),
            clips: Vec::new(),
        },
    };
    captures::create(state, principal, project, new_capture).await
}
