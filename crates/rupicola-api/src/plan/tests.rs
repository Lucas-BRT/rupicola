use super::*;
use crate::DEFAULT_VIEWPORT;

#[test]
fn default_scenario_cell_uses_default_checkpoint() {
    let scenario = Scenario::from_toml("name = \"home\"\nurl = \"/\"").unwrap();
    let plan =
        scenario_cell_plan(&scenario, "http://127.0.0.1:4732/", DEFAULT_VIEWPORT, None).unwrap();
    assert_eq!(plan.target.command, "http://127.0.0.1:4732/");
    assert_eq!(plan.environment.viewport, [1440, 900]);
    assert_eq!(plan.environment.color_scheme, None);
    assert_eq!(
        plan.steps,
        vec![Step::Checkpoint {
            id: "home".to_owned(),
            full_page: false,
        }]
    );
}

#[test]
fn repo_dogfooding_scenarios_parse_and_compile_every_matrix_cell() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.rupicola/scenarios");
    let mut count = 0usize;
    for entry in std::fs::read_dir(&dir).expect("read .rupicola/scenarios") {
        let path = entry.unwrap().path();
        if path.extension().and_then(|e| e.to_str()) != Some("toml") {
            continue;
        }
        let raw = std::fs::read_to_string(&path).unwrap();
        let scenario = Scenario::from_toml(&raw)
            .unwrap_or_else(|e| panic!("invalid scenario {}: {e}", path.display()));
        let viewports = if scenario.matrix.viewports.is_empty() {
            vec![DEFAULT_VIEWPORT]
        } else {
            scenario.matrix.viewports.clone()
        };
        let themes: Vec<Option<&str>> = if scenario.matrix.themes.is_empty() {
            vec![None]
        } else {
            scenario
                .matrix
                .themes
                .iter()
                .map(String::as_str)
                .map(Some)
                .collect()
        };
        for vp in &viewports {
            for theme in &themes {
                let plan = scenario_cell_plan(&scenario, "http://127.0.0.1:4731", *vp, *theme)
                    .unwrap_or_else(|e| panic!("{} plan failed: {e}", path.display()));
                assert!(
                    !plan.steps.is_empty(),
                    "{} produced empty plan",
                    path.display()
                );
            }
        }
        count += 1;
    }
    assert!(
        count >= 3,
        "expected at least 3 dogfooding scenarios, found {count}"
    );
}

#[test]
fn theme_without_steps_sets_color_scheme() {
    let scenario = Scenario::from_toml(
        r#"
        name = "home"
        url = "/docs"
        [matrix]
        themes = ["dark", "light"]
        "#,
    )
    .unwrap();
    let dark = scenario_cell_plan(
        &scenario,
        "http://127.0.0.1:4732",
        DEFAULT_VIEWPORT,
        Some("dark"),
    )
    .unwrap();
    assert_eq!(dark.environment.color_scheme, Some(ColorScheme::Dark));
    assert_eq!(dark.target.command, "http://127.0.0.1:4732/docs");
}

#[test]
fn theme_with_steps_runs_after_leading_ready_and_before_checkpoint() {
    let scenario = Scenario::from_toml(
        r#"
        name = "site-home"
        url = "/"
        [matrix]
        viewports = ["390x844@2"]
        themes = ["dark", "light"]
        [themes.light]
        steps = [{ kind = "click", selector = "[data-theme-toggle]" }]
        [[steps]]
        kind = "ready"
        selector = "h1"
        [[steps]]
        kind = "checkpoint"
        id = "full-page"
        full_page = true
        "#,
    )
    .unwrap();
    let vp: Viewport = "390x844@2".parse().unwrap();
    let light = scenario_cell_plan(&scenario, "http://127.0.0.1:5000", vp, Some("light")).unwrap();
    assert_eq!(light.environment.color_scheme, None);
    assert_eq!(light.environment.viewport, [390, 844]);
    assert!((light.environment.scale - 2.0).abs() < f64::EPSILON);
    assert_eq!(
        light.steps,
        vec![
            Step::Wait {
                condition: WaitCondition::Visible {
                    target_id: "css=h1".to_owned(),
                },
            },
            Step::Action {
                action: UiAction::Click {
                    target_id: "css=[data-theme-toggle]".to_owned(),
                },
            },
            Step::Checkpoint {
                id: "full-page".to_owned(),
                full_page: true,
            },
        ]
    );
}

#[test]
fn invalid_theme_setup_step_is_rejected() {
    let scenario = Scenario::from_toml(
        r#"
        name = "bad"
        url = "/"
        [matrix]
        themes = ["light"]
        [themes.light]
        steps = [{ kind = "checkpoint", id = "x" }]
        "#,
    )
    .unwrap();
    assert!(
        scenario_cell_plan(
            &scenario,
            "http://127.0.0.1:5000",
            DEFAULT_VIEWPORT,
            Some("light")
        )
        .is_err()
    );
}

#[test]
fn ui_tree_is_full_when_checks_are_configured_and_off_otherwise() {
    let plain = Scenario::from_toml("name = \"a\"\nurl = \"/\"").unwrap();
    let plain_plan =
        scenario_cell_plan(&plain, "http://127.0.0.1:5000", DEFAULT_VIEWPORT, None).unwrap();
    assert_eq!(plain_plan.environment.ui_tree, UiTreeMode::Off);

    let checked =
        Scenario::from_toml("name = \"b\"\nurl = \"/\"\n[checks]\nno_horizontal_overflow = true")
            .unwrap();
    let checked_plan =
        scenario_cell_plan(&checked, "http://127.0.0.1:5000", DEFAULT_VIEWPORT, None).unwrap();
    assert_eq!(checked_plan.environment.ui_tree, UiTreeMode::Full);
}
