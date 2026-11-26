//! Executing one matrix cell plan with `rupicola-capture`, uploading its blobs, and
//! publishing an immutable [`CaptureRecord`].

use crate::checks::evaluate_checks;
use anyhow::{Context, Result, bail};
use async_trait::async_trait;
use rupicola_api::{
    Capture, CapturePlanSpec, CaptureStatus, MissingQuery, NewFinding, Scenario, Viewport,
    plan::{plan_from_spec, scenario_cell_plan},
    record::{
        Appearance, CaptureRecord, Display, Environment as RecordEnvironment, Host, Member,
        MemberRole, ParentKind, ParentRef, Platform, PlatformFamily, RECORD_FORMAT, RecordKind,
        ScenarioRef, Software,
    },
};
use rupicola_capture::{
    asynchronous::{CaptureBackend, CdpBackend, CdpDiscovery, probe_environment, run_plan},
    session::{ColorScheme, Plan},
};
use rupicola_client::Api;
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fmt::Write as _,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};
use uuid::Uuid;

const PNG_MIME: &str = "image/png";
const MP4_MIME: &str = "video/mp4";
const MILLI_SCALE: u32 = 1000;

/// One captured checkpoint or clip produced by [`CellCapturer`].
#[derive(Clone, Debug)]
pub struct CapturedFile {
    pub id: String,
    pub path: PathBuf,
    pub snapshot: Option<rupicola_protocol::UiSnapshot>,
}

/// Output of running one matrix cell [`Plan`].
#[derive(Clone, Debug)]
pub struct CellOutcome {
    pub backend_id: String,
    pub checkpoints: Vec<CapturedFile>,
    pub clips: Vec<CapturedFile>,
}

/// Parameters describing one matrix cell to capture and publish.
pub struct CellContext<'a> {
    pub project: &'a str,
    pub scenario: &'a Scenario,
    pub scenario_hash: &'a str,
    pub base_url: &'a str,
    pub revision: &'a str,
    pub viewport: Viewport,
    pub theme: Option<&'a str>,
    /// When set, executes this frozen plan spec verbatim instead of compiling `scenario`.
    pub frozen_spec: Option<&'a CapturePlanSpec>,
    /// Source capture id when running a deterministic recapture job.
    pub parent_capture_id: Option<&'a str>,
}

/// Abstracts running a capture [`Plan`] so the job loop can be tested without Chromium.
#[async_trait]
pub trait CellCapturer: Send + Sync {
    async fn capture_cell(&self, plan: &Plan, out_dir: &Path) -> Result<CellOutcome>;
}

/// Real capturer backed by `rupicola-capture`'s in-house Chromium CDP backend.
pub struct ProtocolCapturer {
    backend: Box<dyn CaptureBackend>,
}

impl ProtocolCapturer {
    /// Uses an explicitly configured Chromium binary, or discovers one from the host.
    ///
    /// # Errors
    ///
    /// Fails when the configured path is not a file or no Chromium CDP backend is available.
    pub async fn discover_with_browser_bin(browser_bin: Option<PathBuf>) -> Result<Self> {
        if let Some(browser_bin) = browser_bin {
            anyhow::ensure!(
                tokio::fs::metadata(&browser_bin).await?.is_file(),
                "configured Chromium binary is not a file: {}",
                browser_bin.display()
            );
            return Ok(Self {
                backend: Box::new(CdpBackend::new(browser_bin)),
            });
        }
        if let Some(cdp) = CdpBackend::discover().await {
            return Ok(Self {
                backend: Box::new(cdp),
            });
        }
        let reports = probe_environment().await;
        let guidance = reports
            .into_iter()
            .filter(|r| r.id == "web-chromium-cdp")
            .map(|r| format!("{}: {}", r.label, r.install_guidance))
            .collect::<Vec<_>>()
            .join("; ");
        bail!(
            "Chromium CDP is unavailable; this runner does not fall back to Playwright ({guidance})"
        )
    }
}

#[async_trait]
impl CellCapturer for ProtocolCapturer {
    async fn capture_cell(&self, plan: &Plan, out_dir: &Path) -> Result<CellOutcome> {
        let bundle = run_plan(self.backend.as_ref(), plan, out_dir)
            .await
            .map_err(|err| anyhow::anyhow!("capture failed: {err}"))?;
        Ok(CellOutcome {
            backend_id: self.backend.id().to_owned(),
            checkpoints: bundle
                .checkpoints
                .into_iter()
                .map(|c| CapturedFile {
                    id: c.id,
                    path: c.png,
                    snapshot: c.snapshot,
                })
                .collect(),
            clips: bundle
                .clips
                .into_iter()
                .map(|c| CapturedFile {
                    id: c.id,
                    path: c.mp4,
                    snapshot: None,
                })
                .collect(),
        })
    }
}

fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .fold(String::with_capacity(64), |mut hex, b| {
            let _ = write!(hex, "{b:02x}");
            hex
        })
}

fn now_ms() -> i64 {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    i64::try_from(millis).unwrap_or(i64::MAX)
}

async fn stage_files(
    outcome: &CellOutcome,
) -> Result<(Vec<(Member, Vec<u8>)>, BTreeMap<String, String>)> {
    let mut staged = Vec::with_capacity(outcome.checkpoints.len() + outcome.clips.len());
    let mut checkpoint_hashes = BTreeMap::new();

    for cp in &outcome.checkpoints {
        let bytes = tokio::fs::read(&cp.path)
            .await
            .with_context(|| format!("read checkpoint {}", cp.path.display()))?;
        let sha256 = sha256_hex(&bytes);
        checkpoint_hashes.insert(cp.id.clone(), sha256.clone());
        staged.push((
            Member {
                path: format!("checkpoints/{}.png", cp.id),
                sha256,
                bytes: u64::try_from(bytes.len()).unwrap_or(u64::MAX),
                mime: PNG_MIME.to_owned(),
                role: MemberRole::Checkpoint,
                owner: cp.id.clone(),
                surface: None,
                locator: BTreeMap::new(),
                target_ms: None,
                video_ms: None,
            },
            bytes,
        ));
    }

    for clip in &outcome.clips {
        let bytes = tokio::fs::read(&clip.path)
            .await
            .with_context(|| format!("read clip {}", clip.path.display()))?;
        let sha256 = sha256_hex(&bytes);
        staged.push((
            Member {
                path: format!("clips/{}.mp4", clip.id),
                sha256,
                bytes: u64::try_from(bytes.len()).unwrap_or(u64::MAX),
                mime: MP4_MIME.to_owned(),
                role: MemberRole::ClipVideo,
                owner: clip.id.clone(),
                surface: None,
                locator: BTreeMap::new(),
                target_ms: None,
                video_ms: None,
            },
            bytes,
        ));
    }

    Ok((staged, checkpoint_hashes))
}

async fn upload_missing_blobs<A: Api>(
    api: &A,
    token: &str,
    project: &str,
    staged: &[(Member, Vec<u8>)],
) -> Result<()> {
    let unique_hashes: Vec<String> = staged
        .iter()
        .map(|(m, _)| m.sha256.clone())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    if unique_hashes.is_empty() {
        return Ok(());
    }
    let missing = api
        .missing_blobs(
            token,
            project,
            &MissingQuery {
                sha256: unique_hashes,
            },
        )
        .await?;
    let missing_set: BTreeSet<String> = missing.missing.into_iter().collect();
    let mut uploaded = BTreeSet::new();
    for (member, bytes) in staged {
        if missing_set.contains(&member.sha256) && uploaded.insert(member.sha256.clone()) {
            api.upload_project_blob(token, project, &member.sha256, bytes.clone())
                .await?;
        }
    }
    Ok(())
}

fn build_capture_record(
    ctx: &CellContext<'_>,
    plan: &Plan,
    backend_id: &str,
    mut members: Vec<Member>,
) -> CaptureRecord {
    members.sort_by(|a, b| a.path.cmp(&b.path));
    let color_scheme = ctx.theme.map(str::to_owned).or_else(|| {
        plan.environment.color_scheme.map(|s| match s {
            ColorScheme::Light => "light".to_owned(),
            ColorScheme::Dark => "dark".to_owned(),
        })
    });
    let series = format!(
        "{}@{}@{}",
        ctx.scenario.name,
        ctx.viewport,
        ctx.theme.unwrap_or("default")
    );
    CaptureRecord {
        format: RECORD_FORMAT,
        id: Uuid::now_v7().to_string(),
        project: ctx.project.to_owned(),
        kind: RecordKind::Scenario,
        scenario: ScenarioRef {
            name: ctx.scenario.name.clone(),
            sha256: Some(ctx.scenario_hash.to_owned()),
        },
        revision: Some(ctx.revision.to_owned()),
        series,
        parents: ctx
            .parent_capture_id
            .map(|id| {
                vec![ParentRef {
                    id: id.to_owned(),
                    kind: ParentKind::Baseline,
                }]
            })
            .unwrap_or_default(),
        captured_at: now_ms(),
        status: CaptureStatus::Complete,
        blocker: None,
        environment: RecordEnvironment {
            platform: Some(Platform {
                family: Some(PlatformFamily::Web),
                os_version: None,
            }),
            device: None,
            display: Some(Display {
                width: Some(ctx.viewport.width),
                height: Some(ctx.viewport.height),
                scale_milli: Some(ctx.viewport.scale.saturating_mul(MILLI_SCALE)),
                orientation: None,
            }),
            appearance: Some(Appearance {
                color_scheme,
                contrast: None,
                font_scale_milli: None,
                reduced_motion: Some(ctx.scenario.environment.reduced_motion),
                locale: ctx.scenario.environment.locale.clone(),
            }),
            software: Some(Software {
                backend: Some(backend_id.to_owned()),
                renderer: Some(backend_id.to_owned()),
                app_version: None,
                app_build: None,
            }),
            host: Some(Host {
                os: Some(std::env::consts::OS.to_owned()),
            }),
            extra: BTreeMap::new(),
        },
        plan_spec: Some(match ctx.frozen_spec {
            Some(spec) => spec.normalized(),
            None => CapturePlanSpec::from_scenario_cell(
                ctx.scenario,
                Some(ctx.scenario_hash),
                ctx.viewport,
                ctx.theme,
            ),
        }),
        members,
    }
}

/// Captures one `(viewport, theme)` cell, uploads missing blobs, and publishes its [`CaptureRecord`].
///
/// Returns `(capture, checkpoint_hashes)` where `checkpoint_hashes` maps checkpoint id to SHA-256.
///
/// # Errors
///
/// Fails when plan building, capture execution, blob upload, or record publishing fails.
pub async fn capture_and_publish<A: Api>(
    api: &A,
    token: &str,
    capturer: &dyn CellCapturer,
    ctx: &CellContext<'_>,
    scratch_dir: &Path,
) -> Result<(Capture, BTreeMap<String, String>)> {
    let plan = match ctx.frozen_spec {
        Some(spec) => plan_from_spec(spec, ctx.base_url, false)
            .map_err(|err| anyhow::anyhow!("invalid frozen plan_spec: {err}"))?,
        None => scenario_cell_plan(ctx.scenario, ctx.base_url, ctx.viewport, ctx.theme)
            .map_err(|err| anyhow::anyhow!("invalid scenario cell plan: {err}"))?,
    };
    let outcome = capturer.capture_cell(&plan, scratch_dir).await?;
    let (staged, checkpoint_hashes) = stage_files(&outcome).await?;
    upload_missing_blobs(api, token, ctx.project, &staged).await?;
    let members: Vec<Member> = staged.into_iter().map(|(m, _)| m).collect();
    let record = build_capture_record(ctx, &plan, &outcome.backend_id, members);
    let capture = api
        .publish_record(token, ctx.project, &record.id, &record)
        .await?;

    for cp in &outcome.checkpoints {
        let Some(snapshot) = &cp.snapshot else {
            continue;
        };
        let evidence_artifact_id = capture
            .checkpoints
            .iter()
            .find(|c| c.id == cp.id)
            .map(|c| c.artifact.id.clone());
        for note in evaluate_checks(&ctx.scenario.checks, &cp.id, snapshot) {
            let _ = api
                .add_finding(
                    token,
                    &capture.id,
                    &NewFinding {
                        note,
                        evidence_artifact_id: evidence_artifact_id.clone(),
                    },
                )
                .await;
        }
    }

    Ok((capture, checkpoint_hashes))
}

/// Captures the first cell a second time and flags the capture if any checkpoint PNG hash differs.
pub async fn check_determinism<A: Api>(
    api: &A,
    token: &str,
    capturer: &dyn CellCapturer,
    ctx: &CellContext<'_>,
    first_hashes: &BTreeMap<String, String>,
    capture_id: &str,
    scratch_dir: &Path,
) -> Result<()> {
    let plan = match ctx.frozen_spec {
        Some(spec) => plan_from_spec(spec, ctx.base_url, false)
            .map_err(|err| anyhow::anyhow!("invalid frozen plan_spec: {err}"))?,
        None => scenario_cell_plan(ctx.scenario, ctx.base_url, ctx.viewport, ctx.theme)
            .map_err(|err| anyhow::anyhow!("invalid scenario cell plan: {err}"))?,
    };
    let second = capturer.capture_cell(&plan, scratch_dir).await?;
    let mut diffs = Vec::new();
    for cp in &second.checkpoints {
        let bytes = tokio::fs::read(&cp.path)
            .await
            .with_context(|| format!("read determinism checkpoint {}", cp.path.display()))?;
        let hash = sha256_hex(&bytes);
        if first_hashes.get(&cp.id).is_some_and(|first| first != &hash) {
            diffs.push(cp.id.clone());
        }
    }
    if !diffs.is_empty() {
        let note = format!(
            "scenario `{}` is non-deterministic: checkpoint(s) {} differed between consecutive captures of the base tree",
            ctx.scenario.name,
            diffs.join(", ")
        );
        api.add_finding(
            token,
            capture_id,
            &NewFinding {
                note,
                evidence_artifact_id: None,
            },
        )
        .await?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::ProtocolCapturer;

    #[tokio::test]
    async fn explicit_chromium_binary_selects_the_cdp_backend() {
        let dir = tempfile::tempdir().unwrap();
        let browser = dir.path().join("chromium");
        tokio::fs::write(&browser, b"browser").await.unwrap();

        let capturer = ProtocolCapturer::discover_with_browser_bin(Some(browser))
            .await
            .unwrap();

        assert_eq!(capturer.backend.id(), "web-chromium-cdp");
    }

    #[tokio::test]
    async fn explicit_chromium_path_must_be_a_file() {
        let dir = tempfile::tempdir().unwrap();
        let result =
            ProtocolCapturer::discover_with_browser_bin(Some(dir.path().join("missing"))).await;

        assert!(result.is_err());
    }
}
