//! Problem domain rules: field bounds for filing, triaging and commenting.

use crate::domain::ApiError;
use rupicola_api::{
    NewProblem, Problem, ProblemOrigin, ProblemPatch, ProblemStatus, Rule, Severity,
};

/// Longest problem title, in characters.
const MAX_TITLE_CHARS: usize = 200;
/// Longest title synthesized when inferring a problem from a capture finding.
const INFERRED_MAX_TITLE_CHARS: usize = 88;
const INFERRED_TRUNCATED_TITLE_CHARS: usize = 85;
/// Longest diagnosis, proposed solution or comment, in characters.
const MAX_TEXT_CHARS: usize = 8000;
/// Longest route, viewport or theme label, in characters.
const MAX_LABEL_CHARS: usize = 256;
/// Rules one problem may cite.
const MAX_RULES: usize = 32;
/// Evidence artifacts one problem may cite.
const MAX_EVIDENCE: usize = 64;
/// Selections a new problem may bring.
const MAX_SELECTIONS: usize = 16;
/// The label of the selection a problem's evidence files become.
pub const EVIDENCE_LABEL: &str = "Evidence";

/// Bounds on text fields, as an error message when one is out of range.
pub fn check_len(field: &str, value: &str, max: usize, required: bool) -> Result<(), ApiError> {
    let min = usize::from(required);
    if (min..=max).contains(&value.chars().count()) {
        Ok(())
    } else {
        Err(ApiError::invalid(format!(
            "{field} must have {min}–{max} characters"
        )))
    }
}

/// Field bounds of a new problem.
///
/// # Errors
///
/// [`ApiError::Invalid`] when any field or collection exceeds its limit.
pub fn validate(form: &NewProblem) -> Result<(), ApiError> {
    check_len("the title", form.title.trim(), MAX_TITLE_CHARS, true)?;
    check_len("the diagnosis", &form.diagnosis, MAX_TEXT_CHARS, false)?;
    check_len(
        "the proposed solution",
        &form.proposed_solution,
        MAX_TEXT_CHARS,
        false,
    )?;
    for (field, value) in [
        ("the route", &form.route),
        ("the viewport", &form.viewport),
        ("the theme", &form.theme),
    ] {
        check_len(field, value, MAX_LABEL_CHARS, false)?;
    }
    if form.evidence.is_empty() && form.selections.is_empty() {
        return Err(ApiError::invalid(
            "a problem requires at least one evidence artifact or selection",
        ));
    }
    if form.rules.len() > MAX_RULES
        || form.evidence.len() > MAX_EVIDENCE
        || form.selections.len() > MAX_SELECTIONS
    {
        return Err(ApiError::invalid(format!(
            "a problem takes at most {MAX_RULES} rules, {MAX_EVIDENCE} evidence files and \
             {MAX_SELECTIONS} selections"
        )));
    }
    Ok(())
}

/// Field bounds of a problem patch.
pub fn validate_patch(patch: &ProblemPatch) -> Result<(), ApiError> {
    if let Some(solution) = &patch.proposed_solution {
        check_len("the proposed solution", solution, MAX_TEXT_CHARS, false)?;
    }
    Ok(())
}

/// Trims and validates a problem comment.
pub fn validate_comment(text: &str) -> Result<&str, ApiError> {
    let text = text.trim();
    check_len("the comment", text, MAX_TEXT_CHARS, true)?;
    Ok(text)
}

/// Returns whether a candidate problem matches an existing intentional or false-positive entry.
#[must_use]
pub fn is_suppressed(existing: &[Problem], route: &str, rule_code: &str, title: &str) -> bool {
    existing.iter().any(|problem| {
        matches!(
            problem.status,
            ProblemStatus::Intentional | ProblemStatus::FalsePositive
        ) && problem.route == route
            && (problem.title.eq_ignore_ascii_case(title)
                || problem.rules.iter().any(|rule| rule.code == rule_code))
    })
}

/// Infers severity, global rule code, rule title and default solution from a capture finding note.
#[must_use]
pub fn infer_rule(text: &str) -> (Severity, &'static str, &'static str, &'static str) {
    let lower = text.to_lowercase();
    if lower.contains("target-size") {
        (
            Severity::Major,
            "common.md#target-size-24px",
            "WCAG 2.5.8 / Rupicola Minimum Target Size (24×24px)",
            "Apply `min-block-size: 24px; min-inline-size: 24px; display: inline-flex; align-items: center;` on interactive controls and links.",
        )
    } else if lower.contains("contrast") {
        (
            Severity::Critical,
            "common.md#contrast-min",
            "WCAG 2.1 SC 1.4.3 / Rupicola Contrast >= 4.5:1",
            "Adjust foreground or surface tokens to ensure >= 4.5:1 contrast ratio.",
        )
    } else if lower.contains("overflow") || lower.contains("clipping") {
        (
            Severity::Critical,
            "common.md#overflow-clipping",
            "Layout Overflow / Horizontal Clipping",
            "Inspect container sizing, overflow wrapping, and flex/grid constraints to eliminate unintended overflow.",
        )
    } else if lower.contains("whitespace") || lower.contains("overlap") {
        (
            Severity::Major,
            "common.md#layout-overlap",
            "Overlapping Elements / Spacing Collapse",
            "Review z-index, positioning, and flex gap to restore clean visual separation.",
        )
    } else if lower.contains("motion") || lower.contains("stall") || lower.contains("jump") {
        (
            Severity::Minor,
            "motion.md#hover-stability",
            "Spatial Motion & Hover Stability",
            "Use `cubic-bezier(0.16, 1, 0.3, 1)` (150–220ms) for spatial motion or instant 0ms switches for tabs/dialogs.",
        )
    } else if lower.contains("broken images")
        || lower.contains("http 4")
        || lower.contains("http 5")
    {
        (
            Severity::Critical,
            "visual-verification.md#asset-integrity",
            "Zero Broken Media Assets or Failed Route Responses",
            "Verify asset paths, wait for lazy image decoding, and ensure all referenced routes/media return HTTP 200.",
        )
    } else if lower.contains("hairline") || lower.contains("seam") || lower.contains("gap") {
        (
            Severity::Minor,
            "common.md#hairline-grid",
            "Continuous 1px Hairline Structural Seams",
            "Use `gap: 1px; background: var(--line); border: 1px solid var(--line)` with `border-radius: 0` on inner cells.",
        )
    } else {
        (
            Severity::Minor,
            "visual-verification.md#visual-inspection",
            "Visual Craft & Structural Hierarchy Standard",
            "Inspect the referenced checkpoint asset and align spacing, contrast, and hierarchy with the active project Design direction.",
        )
    }
}

/// Converts a capture finding note and optional artifact id into a [`NewProblem`].
#[must_use]
pub fn infer_from_finding(note: &str, evidence_artifact_id: Option<&str>) -> NewProblem {
    let origin = if note.starts_with("measured") {
        ProblemOrigin::AgentMeasured
    } else {
        ProblemOrigin::AgentVisual
    };

    let mut route = "/".to_owned();
    let mut viewport = "1440×900".to_owned();
    let mut theme = "dark".to_owned();
    let mut body = note.trim().to_owned();

    if let Some(start) = note.find('[')
        && let Some(end) = note[start..].find(']')
    {
        let inside = &note[start + 1..start + end];
        note[start + end + 1..]
            .trim_start_matches([':', ' ', '·'])
            .clone_into(&mut body);
        if let Some((r, rest)) = inside.split_once('@') {
            r.trim().clone_into(&mut route);
            let mut parts = rest.split_whitespace();
            if let Some(vp) = parts.next() {
                vp.clone_into(&mut viewport);
            }
            if let Some(th) = parts.next() {
                th.clone_into(&mut theme);
            }
        } else {
            inside.trim().clone_into(&mut route);
        }
    }

    let (severity, rule_code, rule_title, solution) = infer_rule(note);

    let title = if body.len() > INFERRED_MAX_TITLE_CHARS {
        format!(
            "{}…",
            body.chars()
                .take(INFERRED_TRUNCATED_TITLE_CHARS)
                .collect::<String>()
        )
    } else if body.is_empty() {
        "Visual inspection finding".to_owned()
    } else {
        body
    };

    NewProblem {
        title,
        severity,
        origin,
        route,
        viewport,
        theme,
        diagnosis: note.to_owned(),
        proposed_solution: solution.to_owned(),
        rules: vec![Rule {
            scope: "global".into(),
            code: rule_code.into(),
            title: rule_title.into(),
        }],
        evidence: evidence_artifact_id
            .filter(|id| !id.is_empty())
            .map(|id| vec![id.to_owned()])
            .unwrap_or_default(),
        selections: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn infer_parses_coordinates_origin_and_rule() {
        let problem = infer_from_finding(
            "measured [/settings @ 320x844 light]: Overflow on mobile table",
            Some("art-1"),
        );
        assert_eq!(problem.origin, ProblemOrigin::AgentMeasured);
        assert_eq!(problem.severity, Severity::Critical);
        assert_eq!(problem.route, "/settings");
        assert_eq!(problem.viewport, "320x844");
        assert_eq!(problem.theme, "light");
        assert_eq!(problem.title, "Overflow on mobile table");
        assert_eq!(problem.rules[0].code, "common.md#overflow-clipping");
        assert_eq!(problem.evidence, ["art-1"]);
    }
}
