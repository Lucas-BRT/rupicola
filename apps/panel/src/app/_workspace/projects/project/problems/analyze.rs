//! `POST /projects/{project}/problems/analyze`

use crate::app::_workspace::projects::project::project as project_param;
use crate::app::{account, panel, respond};
use crate::records::Problem;
use crate::{PageResult, StatusCode, auth, auth::Role, redirect};
use rupicola_api::{NewProblem, ProblemOrigin, ProblemStatus, Rule, Severity};
use serde::Deserialize;
use topcoat::{
    Result,
    context::Cx,
    router::{content::Form, response::Response, route},
};

const MAX_TITLE_CHARS: usize = 88;
const TRUNCATED_TITLE_CHARS: usize = 85;

#[route(POST)]
async fn analyze(cx: &Cx, Form(form): Form<AnalyzeRunForm>) -> Result<Response> {
    respond(
        cx,
        analyze_run_action(cx, project_param(cx).into(), form).await,
    )
}

#[derive(Deserialize)]
pub struct AnalyzeRunForm {
    pub csrf: String,
    pub run_key: String,
}

pub async fn analyze_run_action(
    cx: &Cx,
    project_id: String,
    form: AnalyzeRunForm,
) -> PageResult<Response> {
    let (panel, account) = (panel(cx), account(cx));
    auth::check_csrf(&account, &form.csrf)?;
    if auth::role(&panel, &account, &project_id).await? == Role::Reader {
        return Err(StatusCode::FORBIDDEN);
    }
    match analyze_run(cx, &project_id, &form.run_key).await {
        Ok(()) | Err(StatusCode::NOT_FOUND) => {}
        Err(error) => return Err(error),
    }
    Ok(redirect(&format!("/projects/{project_id}/problems")))
}

pub(crate) async fn analyze_run(cx: &Cx, project_id: &str, run_key: &str) -> PageResult<()> {
    let (panel, account) = (panel(cx), account(cx));
    let (panel, account) = (&panel, &account);
    let existing = crate::records::problems(cx, project_id, &[], &[]).await?;
    let capture = crate::runs::find(cx, run_key)
        .await
        .map(|run| run.capture)
        .ok()
        .filter(|capture| capture.project_id == project_id)
        .ok_or(StatusCode::NOT_FOUND)?;
    if capture.findings.is_empty() {
        if let Some(first) = capture.checkpoints.first() {
            let candidate = infer_from_finding(
                &format!(
                    "[{} @ {}×{} {}] Visual inspection requested on capture '{}'",
                    first.id,
                    capture.environment.width.unwrap_or(1440),
                    capture.environment.height.unwrap_or(900),
                    capture.environment.color_scheme.as_deref().unwrap_or(""),
                    capture.scenario
                ),
                Some(&first.artifact.id),
            );
            if !existing
                .iter()
                .any(|problem| problem.diagnosis == candidate.diagnosis)
            {
                crate::records::open_inferred(
                    panel,
                    account,
                    project_id,
                    &candidate,
                    Some(ProblemStatus::NeedsReprocess),
                )
                .await?;
            }
        }
    } else {
        for finding in &capture.findings {
            let candidate =
                infer_from_finding(&finding.note, finding.evidence_artifact_id.as_deref());
            let rule_code = candidate.rules.first().map_or("", |r| r.code.as_str());
            if is_suppressed(&existing, &candidate.route, rule_code, &candidate.title) {
                continue;
            }
            if !existing.iter().any(|p| p.diagnosis == candidate.diagnosis) {
                crate::records::open_inferred(panel, account, project_id, &candidate, None).await?;
            }
        }
    }
    Ok(())
}

fn is_suppressed(existing: &[Problem], route: &str, rule_code: &str, title: &str) -> bool {
    existing.iter().any(|p| {
        matches!(
            p.status,
            ProblemStatus::Intentional | ProblemStatus::FalsePositive
        ) && p.route == route
            && (p.title.eq_ignore_ascii_case(title)
                || p.violated_rules.iter().any(|r| r.code == rule_code))
    })
}

fn infer_rule(text: &str) -> (Severity, &'static str, &'static str, &'static str) {
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

fn infer_from_finding(note: &str, evidence_artifact_id: Option<&str>) -> NewProblem {
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

    let title = if body.len() > MAX_TITLE_CHARS {
        format!(
            "{}…",
            body.chars().take(TRUNCATED_TITLE_CHARS).collect::<String>()
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
