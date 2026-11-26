//! Scenario rules: what a pushed definition must satisfy before the server keeps it.

use super::ApiError;
use rupicola_api::{Scenario, Viewport, plan};
use rupicola_capture::session::valid_id;
use std::collections::BTreeSet;

/// Longest scenario name, in characters.
const MAX_NAME_CHARS: usize = 64;
/// Longest page path, in characters.
const MAX_URL_CHARS: usize = 2048;
/// Largest viewport side, in CSS pixels.
const MAX_VIEWPORT_SIDE: u32 = 4096;
/// Largest device pixel ratio.
const MAX_SCALE: u32 = 3;
/// Viewports, themes and selectors one scenario may list of each kind.
const MAX_LISTED: usize = 16;
/// Captures one run of a scenario may make.
const MAX_CELLS: usize = 64;
/// Longest locale tag or selector, in characters.
const MAX_SHORT_TEXT_CHARS: usize = 200;
/// Largest minimum target size a check may ask for, in CSS pixels.
const MAX_TARGET_SIZE: u32 = 200;

fn invalid(message: impl Into<String>) -> ApiError {
    ApiError::invalid(message.into())
}

/// The page path: a path on the tree's own origin, never another site.
fn check_url(url: &str) -> Result<(), ApiError> {
    let local = url.starts_with('/') && !url.starts_with("//");
    let clean = !url
        .chars()
        .any(|c| c.is_whitespace() || c.is_control() || c == '\\');
    if !local || !clean || url.chars().count() > MAX_URL_CHARS {
        return Err(invalid(
            "the url must be a path on the captured tree, such as `/` or `/docs`",
        ));
    }
    Ok(())
}

fn check_viewports(viewports: &[Viewport]) -> Result<(), ApiError> {
    let mut seen = BTreeSet::new();
    for viewport in viewports {
        let sides = 1..=MAX_VIEWPORT_SIDE;
        if !sides.contains(&viewport.width)
            || !sides.contains(&viewport.height)
            || !(1..=MAX_SCALE).contains(&viewport.scale)
        {
            return Err(invalid(format!(
                "viewport {viewport} must have sides up to {MAX_VIEWPORT_SIDE} and a scale up to {MAX_SCALE}"
            )));
        }
        if !seen.insert(viewport.to_string()) {
            return Err(invalid(format!("viewport {viewport} is listed twice")));
        }
    }
    Ok(())
}

fn check_themes(scenario: &Scenario) -> Result<(), ApiError> {
    let matrix = &scenario.matrix;
    let mut seen = BTreeSet::new();
    for theme in &matrix.themes {
        if !valid_id(theme) || !seen.insert(theme.as_str()) {
            return Err(invalid(format!(
                "theme `{theme}` must be letters, digits, `-` or `_`, and listed once"
            )));
        }
    }
    for (theme, setup) in &scenario.themes {
        if !seen.contains(theme.as_str()) {
            return Err(invalid(format!(
                "`themes.{theme}` is not in the matrix's themes"
            )));
        }
        plan::build_setup_steps(&setup.steps).map_err(|err| invalid(err.to_string()))?;
    }
    Ok(())
}

fn check_checks(scenario: &Scenario) -> Result<(), ApiError> {
    let checks = &scenario.checks;
    if checks
        .min_target_size
        .is_some_and(|size| !(1..=MAX_TARGET_SIZE).contains(&size))
    {
        return Err(invalid(format!(
            "`min_target_size` must be 1 to {MAX_TARGET_SIZE}"
        )));
    }
    if checks.no_overlap.len() > MAX_LISTED
        || checks
            .no_overlap
            .iter()
            .any(|selector| selector.trim().is_empty() || selector.len() > MAX_SHORT_TEXT_CHARS)
    {
        return Err(invalid(format!(
            "`no_overlap` takes up to {MAX_LISTED} non-empty selectors"
        )));
    }
    Ok(())
}

/// Checks a definition before it is stored.
///
/// # Errors
///
/// [`ApiError::Invalid`] naming the first field that is out of bounds.
pub fn validate(scenario: &Scenario) -> Result<(), ApiError> {
    if !valid_id(&scenario.name) || scenario.name.chars().count() > MAX_NAME_CHARS {
        return Err(invalid(format!(
            "the name must be 1 to {MAX_NAME_CHARS} letters, digits, `-` or `_`"
        )));
    }
    check_url(&scenario.url)?;
    if scenario
        .environment
        .locale
        .as_ref()
        .is_some_and(|locale| locale.trim().is_empty() || locale.len() > MAX_SHORT_TEXT_CHARS)
    {
        return Err(invalid("the locale must be a short, non-empty tag"));
    }
    let matrix = &scenario.matrix;
    if matrix.viewports.len() > MAX_LISTED || matrix.themes.len() > MAX_LISTED {
        return Err(invalid(format!(
            "the matrix lists at most {MAX_LISTED} viewports and {MAX_LISTED} themes"
        )));
    }
    if matrix.cells() > MAX_CELLS {
        return Err(invalid(format!(
            "the matrix makes {} captures; at most {MAX_CELLS}",
            matrix.cells()
        )));
    }
    check_viewports(&matrix.viewports)?;
    check_themes(scenario)?;
    plan::build_steps(&scenario.steps).map_err(|err| invalid(err.to_string()))?;
    check_checks(scenario)
}

#[cfg(test)]
mod tests {
    use super::*;

    const BASE: &str = "name = \"home\"\nurl = \"/\"\n";

    fn parse(extra: &str) -> Scenario {
        Scenario::from_toml(&format!("{BASE}{extra}")).unwrap()
    }

    fn refused(extra: &str) -> bool {
        matches!(validate(&parse(extra)), Err(ApiError::Invalid(_)))
    }

    #[test]
    fn a_minimal_definition_is_valid() {
        assert!(validate(&parse("")).is_ok());
    }

    #[test]
    fn the_name_and_url_are_bounded() {
        let named = |name: &str, url: &str| {
            validate(&Scenario::from_toml(&format!("name = \"{name}\"\nurl = \"{url}\"")).unwrap())
        };
        assert!(named("home page", "/").is_err());
        assert!(named("", "/").is_err());
        for url in [
            "docs",
            "//evil.test/",
            "http://evil.test/",
            "/a b",
            "/a\\\\b",
        ] {
            assert!(named("home", url).is_err(), "{url}");
        }
        assert!(named("home", "/docs?tab=2").is_ok());
    }

    #[test]
    fn the_matrix_is_bounded_and_has_no_duplicates() {
        assert!(refused("[matrix]\nviewports = [\"5000x800\"]"));
        assert!(refused("[matrix]\nviewports = [\"390x844@9\"]"));
        assert!(refused("[matrix]\nviewports = [\"390x844\", \"390x844\"]"));
        assert!(refused("[matrix]\nthemes = [\"dark\", \"dark\"]"));
        let many = (1..=9)
            .map(|n| format!("\"{}x800\"", 300 + n))
            .collect::<Vec<_>>()
            .join(",");
        let themes = (1..=8)
            .map(|n| format!("\"t{n}\""))
            .collect::<Vec<_>>()
            .join(",");
        assert!(refused(&format!(
            "[matrix]\nviewports = [{many}]\nthemes = [{themes}]"
        )));
    }

    #[test]
    fn a_theme_setup_must_be_in_the_matrix_and_only_act() {
        assert!(refused(
            "[matrix]\nthemes = [\"dark\"]\n[themes.light]\nsteps = []"
        ));
        assert!(refused(
            "[matrix]\nthemes = [\"light\"]\n[themes.light]\nsteps = [{ kind = \"checkpoint\", id = \"x\" }]"
        ));
        assert!(
            validate(&parse(
                "[matrix]\nthemes = [\"light\"]\n[themes.light]\nsteps = [{ kind = \"click\", selector = \"#toggle\" }]"
            ))
            .is_ok()
        );
    }

    #[test]
    fn steps_follow_the_capture_run_rules() {
        assert!(refused(
            "[[steps]]\nkind = \"checkpoint\"\nid = \"a\"\n[[steps]]\nkind = \"checkpoint\"\nid = \"a\""
        ));
        assert!(refused("[[steps]]\nkind = \"click\"\nselector = \"#x\""));
        assert!(refused(
            "[[steps]]\nkind = \"wait\"\nms = 99999\n[[steps]]\nkind = \"checkpoint\"\nid = \"a\""
        ));
    }

    #[test]
    fn checks_are_bounded() {
        assert!(refused("[checks]\nmin_target_size = 0"));
        assert!(refused("[checks]\nno_overlap = [\"\"]"));
        assert!(
            validate(&parse(
                "[checks]\nmin_target_size = 24\nno_overlap = [\".a\"]"
            ))
            .is_ok()
        );
    }
}
