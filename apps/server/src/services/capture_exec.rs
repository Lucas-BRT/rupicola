//! Capture execution orchestration (`CaptureExecutionOps`) strictly consuming
//! `rupicola-capture`'s in-house Chromium CDP backend (zero in-repo capture drivers).

use crate::{
    domain::{ApiError, Principal, secret::now_ms},
    services::{access, captures, publish, scenarios},
    state::Shared,
};
use rupicola_api::{
    Capture, CaptureRunRequest, CaptureStatus, CaptureStep, Checks, DEFAULT_VIEWPORT, Matrix,
    RecordClipRequest, Role, Scenario, ScenarioEnvironment, SnapshotCheckpointRequest, Viewport,
    plan::scenario_cell_plan,
    record::{
        Appearance, CaptureRecord, Display, Environment as RecordEnvironment, Host, Member,
        MemberRole, Platform, PlatformFamily, RECORD_FORMAT, RecordKind, ScenarioRef, Software,
    },
};
use rupicola_capture::{
    asynchronous::{CaptureBackend, CdpBackend, CdpDiscovery, probe_environment, run_plan},
    session::{
        ColorScheme, Environment, NetworkPolicy, Plan, Step, Target, UiTreeMode, WaitCondition,
    },
};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, fmt::Write as _};
use uuid::Uuid;

fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .fold(String::with_capacity(64), |mut hex, b| {
            let _ = write!(hex, "{b:02x}");
            hex
        })
}

fn parse_viewport(raw: Option<&str>) -> Viewport {
    let Some(raw) = raw.map(str::trim).filter(|s| !s.is_empty()) else {
        return DEFAULT_VIEWPORT;
    };
    let (dims, scale) = raw
        .split_once('@')
        .map_or((raw, 1u32), |(d, s)| (d, s.trim().parse().unwrap_or(1)));
    dims.split_once(['x', 'X', '×'])
        .and_then(|(w, h)| {
            Some(Viewport {
                width: w.trim().parse().ok()?,
                height: h.trim().parse().ok()?,
                scale: scale.max(1),
            })
        })
        .unwrap_or(DEFAULT_VIEWPORT)
}

async fn discover_cdp() -> Result<CdpBackend, ApiError> {
    if let Some(cdp) = CdpBackend::discover().await {
        return Ok(cdp);
    }
    let reports = probe_environment().await;
    let guidance = reports
        .into_iter()
        .filter(|r| r.id == "web-chromium-cdp")
        .map(|r| format!("{}: {}", r.label, r.install_guidance))
        .collect::<Vec<_>>()
        .join("; ");
    Err(ApiError::invalid(format!(
        "Chromium CDP is unavailable; no Playwright fallback is used ({guidance})"
    )))
}

async fn collect_bundle_members(
    state: &Shared,
    project: &str,
    bundle: rupicola_capture::session::Bundle,
) -> Result<Vec<Member>, ApiError> {
    let mut members = Vec::new();
    for cp in bundle.checkpoints {
        let png = tokio::fs::read(&cp.png)
            .await
            .map_err(anyhow::Error::from)?;
        let sha = sha256_hex(&png);
        let len = u64::try_from(png.len()).map_err(anyhow::Error::from)?;
        captures::put_project_blob(state, project, sha.clone(), png).await?;
        members.push(Member {
            path: format!("checkpoints/{}.png", cp.id),
            role: MemberRole::Checkpoint,
            mime: "image/png".to_owned(),
            sha256: sha,
            bytes: len,
            owner: cp.id.clone(),
            target_ms: None,
            video_ms: None,
            surface: None,
            locator: BTreeMap::new(),
        });
        if let Some(snapshot) = cp.snapshot {
            let json = serde_json::to_vec(&snapshot).map_err(anyhow::Error::from)?;
            let snap_sha = sha256_hex(&json);
            let snap_len = u64::try_from(json.len()).map_err(anyhow::Error::from)?;
            captures::put_project_blob(state, project, snap_sha.clone(), json).await?;
            members.push(Member {
                path: format!("checkpoints/{}.ui-snapshot.json", cp.id),
                role: MemberRole::UiSnapshot,
                mime: "application/json".to_owned(),
                sha256: snap_sha,
                bytes: snap_len,
                owner: cp.id,
                target_ms: None,
                video_ms: None,
                surface: None,
                locator: BTreeMap::new(),
            });
        }
    }
    for clip in bundle.clips {
        let mp4 = tokio::fs::read(&clip.mp4)
            .await
            .map_err(anyhow::Error::from)?;
        let sha = sha256_hex(&mp4);
        let len = u64::try_from(mp4.len()).map_err(anyhow::Error::from)?;
        captures::put_project_blob(state, project, sha.clone(), mp4).await?;
        members.push(Member {
            path: format!("clips/{}.mp4", clip.id),
            role: MemberRole::ClipVideo,
            mime: "video/mp4".to_owned(),
            sha256: sha,
            bytes: len,
            owner: clip.id,
            target_ms: None,
            video_ms: None,
            surface: None,
            locator: BTreeMap::new(),
        });
    }
    Ok(members)
}

async fn execute_and_publish(
    state: &Shared,
    principal: &Principal,
    project: &str,
    scenario_name: &str,
    viewport: Viewport,
    theme: Option<&str>,
    plan: &Plan,
) -> Result<Capture, ApiError> {
    access::require_publisher(state, principal, project, Role::Reviewer).await?;
    let backend = discover_cdp().await?;
    let out_dir = tempfile::tempdir().map_err(anyhow::Error::from)?;
    let bundle = run_plan(&backend, plan, out_dir.path())
        .await
        .map_err(|e| ApiError::invalid(format!("rupicola-capture failed: {e}")))?;
    let members = collect_bundle_members(state, project, bundle).await?;

    let id = Uuid::now_v7().to_string();
    let theme_tag = theme.unwrap_or("default");
    let record = CaptureRecord {
        format: RECORD_FORMAT,
        id: id.clone(),
        project: project.to_owned(),
        kind: RecordKind::Scenario,
        scenario: ScenarioRef {
            name: scenario_name.to_owned(),
            sha256: None,
        },
        revision: None,
        series: format!(
            "{scenario_name}@{}x{}@{theme_tag}",
            viewport.width, viewport.height
        ),
        parents: Vec::new(),
        captured_at: now_ms(),
        status: CaptureStatus::Complete,
        blocker: None,
        environment: RecordEnvironment {
            platform: Some(Platform {
                family: Some(PlatformFamily::Web),
                os_version: None,
            }),
            display: Some(Display {
                width: Some(viewport.width),
                height: Some(viewport.height),
                scale_milli: Some(viewport.scale * 1000),
                orientation: None,
            }),
            appearance: theme.map(|scheme| Appearance {
                color_scheme: Some(scheme.to_owned()),
                reduced_motion: None,
                locale: None,
                font_scale_milli: None,
                contrast: None,
            }),
            software: Some(Software {
                backend: Some(backend.id().to_owned()),
                renderer: None,
                app_version: None,
                app_build: None,
            }),
            host: Some(Host {
                os: Some(std::env::consts::OS.to_owned()),
            }),
            device: None,
            extra: BTreeMap::new(),
        },
        members,
    };

    let published = publish::publish(state, principal, project, &id, &record).await?;
    Ok(match published {
        publish::Published::New(cap) | publish::Published::Same(cap) => cap,
    })
}

/// Runs a stored project scenario via `rupicola-capture` and publishes the resulting capture.
pub async fn run_scenario(
    state: &Shared,
    principal: &Principal,
    req: &CaptureRunRequest,
) -> Result<Capture, ApiError> {
    let version = scenarios::latest(state, principal, &req.project, &req.scenario).await?;
    let viewport = parse_viewport(req.viewport.as_deref());
    let plan = scenario_cell_plan(
        &version.scenario,
        &req.base_url,
        viewport,
        req.theme.as_deref(),
    )
    .map_err(|e| ApiError::invalid(e.to_string()))?;
    execute_and_publish(
        state,
        principal,
        &req.project,
        &req.scenario,
        viewport,
        req.theme.as_deref(),
        &plan,
    )
    .await
}

/// Records a video clip of a URL via `rupicola-capture` and publishes the resulting capture.
pub async fn record_clip(
    state: &Shared,
    principal: &Principal,
    req: &RecordClipRequest,
) -> Result<Capture, ApiError> {
    let viewport = parse_viewport(req.viewport.as_deref());
    let mut env = Environment::new([viewport.width, viewport.height]);
    env.scale = f64::from(viewport.scale);
    env.color_scheme = match req.theme.as_deref() {
        Some("dark") => Some(ColorScheme::Dark),
        Some("light") => Some(ColorScheme::Light),
        _ => None,
    };
    let fps = req.fps.clamp(1, 60);
    let interval_ms = u64::from((1000 / fps).max(1));
    env.frame_interval_ms = Some(interval_ms);
    env.network_policy = NetworkPolicy::Unrestricted;
    let plan = Plan {
        target: Target {
            command: req.url.clone(),
            args: Vec::new(),
        },
        environment: env,
        steps: vec![
            Step::StartRecording,
            Step::Wait {
                condition: WaitCondition::Delay {
                    ms: req.duration_ms.clamp(50, 30_000),
                },
            },
            Step::StopRecording {
                id: "clip".to_owned(),
            },
        ],
        allow_degraded: false,
    };
    execute_and_publish(
        state,
        principal,
        &req.project,
        "record-clip",
        viewport,
        req.theme.as_deref(),
        &plan,
    )
    .await
}

/// Captures a single checkpoint PNG (+ optional UI snapshot) via `rupicola-capture`.
pub async fn snapshot_checkpoint(
    state: &Shared,
    principal: &Principal,
    req: &SnapshotCheckpointRequest,
) -> Result<Capture, ApiError> {
    let viewport = parse_viewport(req.viewport.as_deref());
    let label = req.label.trim();
    let label = if label.is_empty() {
        "checkpoint"
    } else {
        label
    };
    let scenario = Scenario {
        name: "snapshot".to_owned(),
        url: req.url.clone(),
        environment: ScenarioEnvironment::default(),
        matrix: Matrix::default(),
        themes: BTreeMap::new(),
        steps: vec![CaptureStep::Checkpoint {
            id: label.to_owned(),
            full_page: false,
        }],
        checks: Checks::default(),
    };
    let mut plan = scenario_cell_plan(&scenario, "", viewport, req.theme.as_deref())
        .map_err(|e| ApiError::invalid(e.to_string()))?;
    if req.include_ui_snapshot {
        plan.environment.ui_tree = UiTreeMode::Full;
    }
    execute_and_publish(
        state,
        principal,
        &req.project,
        "snapshot",
        viewport,
        req.theme.as_deref(),
        &plan,
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::parse_viewport;
    use rupicola_api::{DEFAULT_VIEWPORT, Viewport};

    #[test]
    fn parse_viewport_supports_scale_unicode_x_and_fallback() {
        assert_eq!(
            parse_viewport(Some("390x844@2")),
            Viewport {
                width: 390,
                height: 844,
                scale: 2,
            }
        );
        assert_eq!(
            parse_viewport(Some("1280×720")),
            Viewport {
                width: 1280,
                height: 720,
                scale: 1,
            }
        );
        assert_eq!(parse_viewport(None), DEFAULT_VIEWPORT);
        assert_eq!(parse_viewport(Some("not-a-viewport")), DEFAULT_VIEWPORT);
    }
}
