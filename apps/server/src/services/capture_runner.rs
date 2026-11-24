//! Executes declarative web captures on the server using the external `rupicola-capture`
//! crate, storing the resulting blobs directly in `state.blobs` and SQLite.

use crate::{
    domain::{
        ApiError, Principal,
        blob::{BlobKind, MAGIC_LEN},
        capture_run::{ParsedTarget, is_forbidden_ip, to_plan},
    },
    services::{access::require_publisher, captures, limits},
    state::Shared,
};
use rupicola_api::{
    BlobRef, Capture, CaptureStatus, Limits, NewCapture, NewCheckpoint, NewClip, Role, RunCapture,
};
use rupicola_capture::asynchronous::{
    CaptureBackend, CdpBackend, CdpDiscovery, WebBackend, WebDiscovery, run_plan,
};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{net::IpAddr, path::Path, sync::atomic::Ordering};
use tokio::io::AsyncReadExt as _;

const DEFAULT_BROWSER_ENGINE: &str = "chromium";
const MISSING_BACKEND_MSG: &str = "no headless web capture backend is available on this server \
    (configure RUPICOLA_PLAYWRIGHT_DIR with node, or install chromium)";
const CAPABILITIES_DOCS_URL: &str = "https://github.com/Lucas-BRT/rupicola-protocol#readme";
const PLAYWRIGHT_INSTALL_HINT: &str =
    "Install Node.js and Playwright, and set RUPICOLA_PLAYWRIGHT_DIR";
const CDP_INSTALL_HINT: &str = "Install Chromium or Google Chrome on the host OS \
    (e.g. apt install chromium / dnf install chromium / brew install --cask chromium)";

/// Availability of a single server-side capture backend.
#[derive(Clone, Debug, Serialize)]
pub struct BackendCapability {
    pub id: &'static str,
    pub label: &'static str,
    pub available: bool,
    pub install_hint: Option<&'static str>,
}

/// Zero-install probe of available `rupicola-capture` backends on this host.
#[derive(Clone, Debug, Serialize)]
pub struct CaptureCapabilities {
    pub ready: bool,
    pub backends: Vec<BackendCapability>,
    pub docs_url: &'static str,
}

/// Probes available `rupicola-capture` backends without installing anything.
pub async fn capabilities() -> CaptureCapabilities {
    let playwright = WebBackend::discover(DEFAULT_BROWSER_ENGINE).await.is_some();
    let cdp = CdpBackend::discover().await.is_some();
    let backends = vec![
        BackendCapability {
            id: "playwright",
            label: "Playwright (Node.js)",
            available: playwright,
            install_hint: (!playwright).then_some(PLAYWRIGHT_INSTALL_HINT),
        },
        BackendCapability {
            id: "cdp",
            label: "Chromium / Chrome (CDP)",
            available: cdp,
            install_hint: (!cdp).then_some(CDP_INSTALL_HINT),
        },
    ];
    let ready = backends.iter().any(|backend| backend.available);
    CaptureCapabilities {
        ready,
        backends,
        docs_url: CAPABILITIES_DOCS_URL,
    }
}

async fn verify_resolved_target(target: &ParsedTarget, allow_local: bool) -> Result<(), ApiError> {
    if allow_local || target.host.parse::<IpAddr>().is_ok() {
        return Ok(());
    }
    let addrs = tokio::net::lookup_host((target.host.as_str(), target.port))
        .await
        .map_err(|_| ApiError::invalid(format!("cannot resolve capture host `{}`", target.host)))?;
    let mut resolved_any = false;
    for addr in addrs {
        resolved_any = true;
        if is_forbidden_ip(addr.ip()) {
            return Err(ApiError::invalid(
                "capture target resolves to a forbidden local, private, or metadata address",
            ));
        }
    }
    if !resolved_any {
        return Err(ApiError::invalid(format!(
            "capture host `{}` resolved to no addresses",
            target.host
        )));
    }
    Ok(())
}

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
    project: &str,
    project_limits: &Limits,
    rel_path: String,
    disk_path: &Path,
) -> Result<BlobRef, ApiError> {
    let meta = tokio::fs::metadata(disk_path)
        .await
        .map_err(anyhow::Error::from)?;
    let size = meta.len();
    if size > project_limits.max_blob_bytes {
        return Err(ApiError::PayloadTooLarge);
    }
    let mut file = tokio::fs::File::open(disk_path)
        .await
        .map_err(anyhow::Error::from)?;
    let mut head = [0u8; MAGIC_LEN];
    let n = file.read(&mut head).await.map_err(anyhow::Error::from)?;
    if size > BlobKind::of(&head[..n]).limit(project_limits) {
        return Err(ApiError::PayloadTooLarge);
    }
    drop(file);
    let data = tokio::fs::read(disk_path)
        .await
        .map_err(anyhow::Error::from)?;
    let sha256 = format!("{:x}", Sha256::digest(&data));
    captures::put_project_blob(state, project, sha256.clone(), data.into()).await?;
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
    let allow_local = state.allow_local_captures.load(Ordering::Relaxed);
    let (plan, target) = to_plan(&form, allow_local)?;
    verify_resolved_target(&target, allow_local).await?;
    let project_limits = limits::effective(state, project).await?;
    let _permit = state
        .capture_semaphore
        .clone()
        .try_acquire_owned()
        .map_err(|_| ApiError::Overloaded)?;

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
                let file = store_artifact(state, project, &project_limits, rel, &item.png).await?;
                checkpoints.push(NewCheckpoint {
                    id: item.id.clone(),
                    file,
                });
            }
            let mut clips = Vec::with_capacity(bundle.clips.len());
            for clip in &bundle.clips {
                let rel = format!("clips/{}.mp4", clip.id);
                let video = store_artifact(state, project, &project_limits, rel, &clip.mp4).await?;
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
