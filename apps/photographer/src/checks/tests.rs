use super::*;
use rupicola_protocol::{SnapshotSource, ToolkitKind, WidgetState};

fn rect(x: f64, y: f64, width: f64, height: f64) -> Rect {
    Rect {
        x,
        y,
        width,
        height,
    }
}

fn node(
    id: &str,
    role: &str,
    kind: WidgetKind,
    label: Option<&str>,
    bounds: Rect,
    style_hint: Option<&str>,
    children: Vec<WidgetNode>,
) -> WidgetNode {
    WidgetNode {
        id: id.to_owned(),
        role: role.to_owned(),
        kind,
        label: label.map(str::to_owned),
        value: None,
        bounds,
        state: WidgetState {
            visible: true,
            ..WidgetState::default()
        },
        style_hint: style_hint.map(str::to_owned),
        style: None,
        children,
    }
}

fn snapshot(width: u32, children: Vec<WidgetNode>) -> UiSnapshot {
    UiSnapshot {
        protocol_version: 1,
        app_id: "test".to_owned(),
        toolkit: ToolkitKind::WebDom,
        title: None,
        viewport: [width, 800],
        scale_factor: 1.0,
        timestamp_ms: 0,
        source: SnapshotSource::Dom,
        root: node(
            "root",
            "body",
            WidgetKind::Group,
            None,
            rect(0.0, 0.0, f64::from(width), 800.0),
            None,
            children,
        ),
    }
}

#[test]
fn passes_when_all_tree_checks_are_satisfied() {
    let snap = snapshot(
        390,
        vec![node(
            "btn",
            "button",
            WidgetKind::Button,
            Some("Menu"),
            rect(10.0, 10.0, 44.0, 44.0),
            Some("nav-btn"),
            Vec::new(),
        )],
    );
    let checks = Checks {
        no_horizontal_overflow: true,
        min_target_size: Some(24),
        no_overlap: vec![".nav-btn".to_owned()],
        accessible_names: true,
        contrast: None,
    };
    assert_eq!(evaluate_checks(&checks, "full", &snap).len(), 0);
}

#[test]
fn reports_overflow_small_target_missing_name_and_overlap() {
    let snap = snapshot(
        390,
        vec![
            node(
                "wide",
                "div",
                WidgetKind::Group,
                None,
                rect(0.0, 0.0, 433.0, 40.0),
                Some("box"),
                Vec::new(),
            ),
            node(
                "tiny",
                "button",
                WidgetKind::Button,
                None,
                rect(10.0, 10.0, 16.0, 16.0),
                Some("box"),
                Vec::new(),
            ),
        ],
    );
    let checks = Checks {
        no_horizontal_overflow: true,
        min_target_size: Some(24),
        no_overlap: vec![".box".to_owned()],
        accessible_names: true,
        contrast: None,
    };
    let findings = evaluate_checks(&checks, "full", &snap);
    assert_eq!(findings.len(), 4, "{findings:?}");
    assert!(findings[0].contains("no_horizontal_overflow"));
    assert!(findings[1].contains("min_target_size"));
    assert!(findings[2].contains("accessible_names"));
    assert!(findings[3].contains("no_overlap"));
}

#[test]
fn contrast_check_evaluates_aa_aaa_and_inherited_background() {
    use rupicola_protocol::ComputedStyle;

    let mut parent = node(
        "card",
        "div",
        WidgetKind::Group,
        None,
        rect(0.0, 0.0, 300.0, 200.0),
        None,
        vec![
            {
                let mut ok_btn = node(
                    "ok",
                    "button",
                    WidgetKind::Button,
                    Some("Save"),
                    rect(10.0, 10.0, 80.0, 36.0),
                    None,
                    Vec::new(),
                );
                ok_btn.style = Some(ComputedStyle {
                    color: Some("rgb(255, 255, 255)".to_owned()),
                    background_color: Some("rgba(0, 0, 0, 0)".to_owned()),
                    font_size_px: Some(16.0),
                    ..ComputedStyle::default()
                });
                ok_btn
            },
            {
                // #777777 on #09090b is ~4.4:1: fails AA normal (4.5), passes AA large (3.0)
                let mut muted_small = node(
                    "muted-small",
                    "span",
                    WidgetKind::Text,
                    Some("Caption"),
                    rect(10.0, 60.0, 80.0, 20.0),
                    None,
                    Vec::new(),
                );
                muted_small.style = Some(ComputedStyle {
                    color: Some("#777777".to_owned()),
                    background_color: Some("rgba(0, 0, 0, 0)".to_owned()),
                    font_size_px: Some(14.0),
                    ..ComputedStyle::default()
                });
                muted_small
            },
            {
                let mut muted_large = node(
                    "muted-large",
                    "h1",
                    WidgetKind::Heading,
                    Some("Heading"),
                    rect(10.0, 90.0, 200.0, 32.0),
                    None,
                    Vec::new(),
                );
                muted_large.style = Some(ComputedStyle {
                    color: Some("#777777".to_owned()),
                    background_color: Some("rgba(0, 0, 0, 0)".to_owned()),
                    font_size_px: Some(24.0),
                    ..ComputedStyle::default()
                });
                muted_large
            },
        ],
    );
    parent.style = Some(ComputedStyle {
        background_color: Some("rgb(9, 9, 11)".to_owned()),
        ..ComputedStyle::default()
    });

    let snap = snapshot(390, vec![parent]);
    let aa = Checks {
        contrast: Some(ContrastLevel::Aa),
        ..Checks::default()
    };
    let findings_aa = evaluate_checks(&aa, "full", &snap);
    assert_eq!(findings_aa.len(), 1, "{findings_aa:?}");
    assert!(findings_aa[0].contains("muted-small"));
    assert!(!findings_aa[0].contains("muted-large"));
    assert!(!findings_aa[0].contains("Save"));

    let aaa = Checks {
        contrast: Some(ContrastLevel::Aaa),
        ..Checks::default()
    };
    let findings_aaa = evaluate_checks(&aaa, "full", &snap);
    assert_eq!(findings_aaa.len(), 1, "{findings_aaa:?}");
    assert!(findings_aaa[0].contains("muted-small"));
    assert!(findings_aaa[0].contains("muted-large"));
}

#[test]
fn contrast_check_ignores_sliders_and_non_leaf_containers() {
    use rupicola_protocol::ComputedStyle;

    let mut slider = node(
        "split-slider",
        "slider",
        WidgetKind::Slider,
        Some("Before and after split slider"),
        rect(0.0, 0.0, 200.0, 24.0),
        None,
        Vec::new(),
    );
    slider.style = Some(ComputedStyle {
        color: Some("rgb(120, 120, 120)".to_owned()),
        background_color: Some("rgb(255, 255, 255)".to_owned()),
        font_size_px: Some(14.0),
        ..ComputedStyle::default()
    });

    let mut container = node(
        "hero-diff",
        "div",
        WidgetKind::Group,
        Some("Concatenated child text"),
        rect(0.0, 0.0, 300.0, 120.0),
        None,
        vec![{
            let mut leaf = node(
                "leaf",
                "span",
                WidgetKind::Text,
                Some("Readable"),
                rect(10.0, 10.0, 100.0, 20.0),
                None,
                Vec::new(),
            );
            leaf.style = Some(ComputedStyle {
                color: Some("rgb(17, 17, 19)".to_owned()),
                background_color: Some("rgba(0, 0, 0, 0)".to_owned()),
                font_size_px: Some(16.0),
                ..ComputedStyle::default()
            });
            leaf
        }],
    );
    container.style = Some(ComputedStyle {
        color: Some("rgb(255, 255, 255)".to_owned()),
        background_color: Some("rgb(255, 255, 255)".to_owned()),
        font_size_px: Some(16.0),
        ..ComputedStyle::default()
    });

    let snap = snapshot(390, vec![slider, container]);
    let aa = Checks {
        contrast: Some(ContrastLevel::Aa),
        ..Checks::default()
    };
    assert_eq!(evaluate_checks(&aa, "full", &snap), Vec::<String>::new());
}
