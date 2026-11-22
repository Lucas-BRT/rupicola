use rupicola_core::capture_pipeline::{
    CaptureBackend, CaptureBackendKind, CaptureBundle, CapturePipelineError, CapturePlan,
    CaptureStep, CapturedCheckpoint, MiddleendEnv, MiddlewareKind, Orchestrator, TargetKind,
    TriggerSource, WidgetBoxJson,
};
use rupicola_protocol::{Rect, ToolkitKind, WidgetBox, WidgetState};
use std::path::Path;

struct MockWebBackend;

impl CaptureBackend for MockWebBackend {
    fn kind(&self) -> CaptureBackendKind {
        CaptureBackendKind::Playwright
    }

    fn supports(&self, target: &TargetKind) -> bool {
        matches!(target, TargetKind::Web { .. })
    }

    fn execute(
        &self,
        plan: &CapturePlan,
        output_dir: &Path,
    ) -> Result<CaptureBundle, CapturePipelineError> {
        let widget_box = WidgetBox {
            id: "cta".into(),
            role: "button".into(),
            label: Some("Install".into()),
            bounds: Rect {
                x: 10.0,
                y: 20.0,
                width: 100.0,
                height: 40.0,
            },
            state: WidgetState {
                visible: true,
                enabled: true,
                ..WidgetState::default()
            },
        };
        Ok(CaptureBundle {
            plan: plan.clone(),
            checkpoints: vec![CapturedCheckpoint {
                id: "hero".into(),
                label: "Hero".into(),
                image_path: output_dir.join("hero.png"),
                dimensions: [1440, 900],
                boxes: vec![WidgetBoxJson::from_widget_box(
                    &widget_box,
                    plan.middleend.scale_factor,
                )],
            }],
            clips: vec![],
            snapshots: vec![],
            elapsed_ms: 12,
            warnings: vec![],
        })
    }
}

#[test]
fn orchestrator_validates_and_dispatches_capture_plan() {
    let mut orch = Orchestrator::new();
    orch.register(Box::new(MockWebBackend));

    let plan = CapturePlan {
        project_id: "demo".into(),
        scenario: "proposal-preview".into(),
        trigger: TriggerSource::McpAgent,
        target: TargetKind::Web {
            base_url: "http://127.0.0.1:4732".into(),
            isolated_preview_route: Some("/__rupicola/preview/p1".into()),
        },
        backend: CaptureBackendKind::Playwright,
        middleend: MiddleendEnv {
            runtime: "chromium".into(),
            viewport: [1440, 900],
            scale_factor: 1.0,
            theme: "light".into(),
            locale: Some("pt-BR".into()),
            reduced_motion: false,
        },
        middleware: MiddlewareKind::WebDom,
        steps: vec![
            CaptureStep::Navigate {
                target: "/__rupicola/preview/p1".into(),
            },
            CaptureStep::Snapshot {
                id: "hero".into(),
                label: "Hero".into(),
            },
        ],
        baseline_run_id: Some("0/baseline".into()),
    };

    let bundle = orch.run(&plan, Path::new("/tmp/out")).unwrap();
    assert_eq!(bundle.checkpoints.len(), 1);
    assert_eq!(bundle.checkpoints[0].boxes[0].pixel_box, [10, 20, 100, 40]);
}

#[test]
fn plan_rejects_mismatched_target_and_backend() {
    let bad = CapturePlan {
        project_id: "demo".into(),
        scenario: "bad".into(),
        trigger: TriggerSource::ManualCli,
        target: TargetKind::Desktop {
            command: "my-app".into(),
            args: vec![],
            protocol_socket: None,
            background: true,
        },
        backend: CaptureBackendKind::Playwright,
        middleend: MiddleendEnv {
            runtime: "wayland".into(),
            viewport: [1280, 720],
            scale_factor: 1.0,
            theme: "dark".into(),
            locale: None,
            reduced_motion: false,
        },
        middleware: MiddlewareKind::RupicolaProtocol(ToolkitKind::Iced),
        steps: vec![CaptureStep::Snapshot {
            id: "s1".into(),
            label: "Main".into(),
        }],
        baseline_run_id: None,
    };
    assert!(bad.validate().is_err());
}
