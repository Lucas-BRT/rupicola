//! Problems, fixes, and design records as the panel shows them. The backend owns them: this
//! reads them through the [`Api`] and shapes each into the view model the screens render.

use super::{Account, PageResult, Panel, auth, backend, media, project::AssetOption};
use rupicola_api::{
    DesignEntry, Fix, FixStatus, NewProblem, Problem as ApiProblem, ProblemOrigin, ProblemPatch,
    ProblemStatus, Severity,
};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;
use tokio::task::JoinSet;
use topcoat::context::{Cx, context};

/// Author shown when the backend recorded none.
const UNKNOWN_AUTHOR: &str = "unknown";

/// A captured media asset linked to a problem.
#[derive(Clone, Debug)]
pub struct EvidenceAsset {
    pub run_key: String,
    pub url: String,
    pub label: String,
}

/// A global or project rule cited by a problem.
#[derive(Clone, Debug)]
pub struct ViolatedRule {
    pub scope: String,
    pub code: String,
    pub title: String,
}

/// A triage comment on a problem.
#[derive(Clone, Debug)]
pub struct ProblemComment {
    pub author: String,
    pub text: String,
    pub created_at_ms: u64,
}

/// Panel view model for a tracked problem.
#[derive(Clone, Debug)]
pub struct Problem {
    pub id: String,
    pub title: String,
    pub status: ProblemStatus,
    pub severity: Severity,
    pub origin: ProblemOrigin,
    pub route: String,
    pub viewport: String,
    pub theme: String,
    pub diagnosis: String,
    pub proposed_solution: String,
    pub violated_rules: Vec<ViolatedRule>,
    pub evidence_assets: Vec<EvidenceAsset>,
    pub comments: Vec<ProblemComment>,
    pub linked_fix_id: Option<String>,
    pub created_at_ms: u64,
}

/// A before/after screenshot or video comparison on a fix.
#[derive(Clone, Debug)]
pub struct ComparisonPair {
    pub label: String,
    pub before_url: String,
    pub after_url: String,
    pub diff_url: Option<String>,
}

/// One implementation or verification step recorded on a fix.
#[derive(Clone, Debug)]
pub struct FixStep {
    pub index: usize,
    pub title: String,
    pub detail: String,
    pub verification: String,
    pub evidence_url: Option<String>,
}

/// One review action recorded on a fix.
#[derive(Clone, Debug)]
pub struct FixReview {
    pub author: String,
    pub action: String,
    pub comment: String,
    pub created_at_ms: u64,
}

/// Panel view model for a recorded fix.
#[derive(Clone, Debug)]
pub struct FixRecord {
    pub id: String,
    pub problem_ids: Vec<String>,
    pub title: String,
    pub summary: String,
    pub status: FixStatus,
    pub tactics: Vec<String>,
    pub modified_files: Vec<String>,
    pub comparisons: Vec<ComparisonPair>,
    pub steps: Vec<FixStep>,
    pub reviews: Vec<FixReview>,
    pub created_at_ms: u64,
}

pub(crate) fn millis(value: i64) -> u64 {
    u64::try_from(value).unwrap_or_default()
}

fn author(name: Option<String>) -> String {
    name.unwrap_or_else(|| UNKNOWN_AUTHOR.to_owned())
}

/// The artifact a media URL of this panel serves; text that is no such URL is returned as is.
pub fn artifact_of(url: &str) -> &str {
    let prefix = media::url("");
    url.strip_prefix(prefix.as_str()).unwrap_or(url)
}

/// IDs of design entries replaced by a newer record.
#[must_use]
pub fn superseded_design(entries: &[DesignEntry]) -> BTreeSet<&str> {
    entries
        .iter()
        .filter_map(|entry| entry.entry.supersedes.as_deref())
        .collect()
}

fn problem_of(problem: ApiProblem, assets: &[AssetOption], linked_fix: Option<&String>) -> Problem {
    let label = format!(
        "{} · {} · {}",
        problem.route, problem.viewport, problem.theme
    );
    let evidence_assets = problem
        .evidence
        .iter()
        .map(|id| {
            let url = media::url(id);
            let known = assets.iter().find(|asset| asset.url == url);
            EvidenceAsset {
                run_key: known.map(|asset| asset.run_key.clone()).unwrap_or_default(),
                url,
                label: label.clone(),
            }
        })
        .collect();
    Problem {
        id: problem.id,
        title: problem.title,
        status: problem.status,
        severity: problem.severity,
        origin: problem.origin,
        route: problem.route,
        viewport: problem.viewport,
        theme: problem.theme,
        diagnosis: problem.diagnosis,
        proposed_solution: problem.proposed_solution,
        violated_rules: problem
            .rules
            .into_iter()
            .map(|rule| ViolatedRule {
                scope: rule.scope,
                code: rule.code,
                title: rule.title,
            })
            .collect(),
        evidence_assets,
        comments: problem
            .comments
            .into_iter()
            .map(|comment| ProblemComment {
                author: author(comment.author),
                text: comment.text,
                created_at_ms: millis(comment.created_at),
            })
            .collect(),
        linked_fix_id: linked_fix.cloned(),
        created_at_ms: millis(problem.created_at),
    }
}

fn fix_of(fix: Fix) -> FixRecord {
    FixRecord {
        id: fix.id,
        problem_ids: fix.problems,
        title: fix.title,
        summary: fix.summary,
        status: fix.status,
        tactics: fix.tactics,
        modified_files: fix.files,
        comparisons: fix
            .comparisons
            .into_iter()
            .map(|pair| ComparisonPair {
                label: pair.label,
                before_url: media::url(&pair.before),
                after_url: media::url(&pair.after),
                diff_url: pair.diff.as_deref().map(media::url),
            })
            .collect(),
        steps: fix
            .steps
            .into_iter()
            .enumerate()
            .map(|(index, step)| FixStep {
                index: index + 1,
                title: step.step.title,
                detail: step.step.detail,
                verification: step.step.verification,
                evidence_url: step.step.evidence.as_deref().map(media::url),
            })
            .collect(),
        reviews: fix
            .reviews
            .into_iter()
            .map(|review| FixReview {
                author: author(review.author),
                action: review.outcome.as_str().to_owned(),
                comment: review.comment,
                created_at_ms: millis(review.created_at),
            })
            .collect(),
        created_at_ms: millis(fix.created_at),
    }
}

/// Every fix of `project`, most recently updated first.
///
/// # Errors
///
/// The status of a failed call to the backend.
pub async fn fixes(cx: &Cx, project: &str) -> PageResult<Vec<FixRecord>> {
    let summaries = backend::fixes(cx, project).await.as_ref().map_err(|e| *e)?;
    let (panel, account) = (context::<Arc<Panel>>(cx), context::<Account>(cx));
    let mut reading = JoinSet::new();
    for (position, summary) in summaries.iter().enumerate() {
        let api = Arc::clone(&panel.api);
        let session = account.session.clone();
        let id = summary.id.clone();
        reading.spawn(async move { (position, api.fix(&session, &id).await) });
    }
    let mut found = Vec::new();
    while let Some(joined) = reading.join_next().await {
        let (position, fix) = joined.map_err(super::db_error)?;
        found.push((position, fix_of(fix.map_err(|e| auth::api_error(&e))?)));
    }
    found.sort_by_key(|(position, _)| *position);
    Ok(found.into_iter().map(|(_, fix)| fix).collect())
}

/// Every problem of `project`, most recently updated first. `fixes` says which fix each
/// problem is linked to; `assets` describes the captured files its evidence points at.
///
/// # Errors
///
/// The status of a failed call to the backend.
pub async fn problems(
    cx: &Cx,
    project: &str,
    assets: &[AssetOption],
    fixes: &[FixRecord],
) -> PageResult<Vec<Problem>> {
    let summaries = backend::problems(cx, project)
        .await
        .as_ref()
        .map_err(|e| *e)?;
    let (panel, account) = (context::<Arc<Panel>>(cx), context::<Account>(cx));
    let mut linked: BTreeMap<&String, &String> = BTreeMap::new();
    for fix in fixes {
        for problem in &fix.problem_ids {
            linked.entry(problem).or_insert(&fix.id);
        }
    }
    let mut reading = JoinSet::new();
    for (position, summary) in summaries.iter().enumerate() {
        let api = Arc::clone(&panel.api);
        let session = account.session.clone();
        let id = summary.id.clone();
        reading.spawn(async move { (position, api.problem(&session, &id).await) });
    }
    let mut found = Vec::new();
    while let Some(joined) = reading.join_next().await {
        let (position, problem) = joined.map_err(super::db_error)?;
        let problem = problem.map_err(|e| auth::api_error(&e))?;
        let fix = linked.get(&problem.id).copied();
        found.push((position, problem_of(problem, assets, fix)));
    }
    found.sort_by_key(|(position, _)| *position);
    Ok(found.into_iter().map(|(_, problem)| problem).collect())
}

/// Opens on the backend the problem inferred from a capture's finding, and moves it to
/// `status` when that is not the status a new problem starts in.
///
/// # Errors
///
/// The status of a failed call to the backend.
pub async fn open_inferred(
    panel: &Panel,
    account: &Account,
    project_id: &str,
    new: &NewProblem,
    status: Option<ProblemStatus>,
) -> PageResult<()> {
    let created = panel
        .api
        .create_problem(&account.session, project_id, new)
        .await
        .map_err(|e| auth::api_error(&e))?;
    if let Some(status) = status {
        let patch = ProblemPatch {
            status: Some(status),
            severity: None,
            proposed_solution: None,
        };
        panel
            .api
            .update_problem(&account.session, &created.id, &patch)
            .await
            .map_err(|e| auth::api_error(&e))?;
    }
    Ok(())
}

/// The design records of `project`, newest first.
///
/// # Errors
///
/// The status of a failed call to the backend.
pub async fn design(cx: &Cx, project: &str) -> PageResult<Vec<DesignEntry>> {
    backend::design(cx, project).await.clone()
}

#[cfg(test)]
mod tests {
    use super::*;
    use rupicola_api::{Comparison, NewStep, Review, Step};

    const ARTIFACT: &str = "artifact-1";
    const OTHER_ARTIFACT: &str = "artifact-2";
    const RUN: &str = "capture-1";
    const FIX: &str = "fix-1";
    const PROBLEM: &str = "problem-1";
    const SECOND_MS: i64 = 1_000;

    fn problem() -> ApiProblem {
        ApiProblem {
            id: PROBLEM.to_owned(),
            project_id: "demo".to_owned(),
            title: "Gap".to_owned(),
            status: ProblemStatus::NeedsReprocess,
            severity: Severity::Minor,
            origin: ProblemOrigin::AgentVisual,
            route: "/".to_owned(),
            viewport: "1440×900".to_owned(),
            theme: "dark".to_owned(),
            diagnosis: String::new(),
            proposed_solution: String::new(),
            rules: vec![],
            evidence: vec![ARTIFACT.to_owned(), OTHER_ARTIFACT.to_owned()],
            selections: vec![],
            comments: vec![],
            author: None,
            created_at: SECOND_MS,
            updated_at: SECOND_MS,
        }
    }

    #[test]
    fn a_media_url_names_its_artifact() {
        assert_eq!(artifact_of(&media::url(ARTIFACT)), ARTIFACT);
        assert_eq!(artifact_of(ARTIFACT), ARTIFACT);
    }

    #[test]
    fn evidence_is_described_by_the_capture_lists_when_they_know_it() {
        let assets = [AssetOption {
            run_key: RUN.to_owned(),
            scenario: String::new(),
            name: "home".to_owned(),
            url: media::url(ARTIFACT),
            kind: "Recording",
        }];
        let shaped = problem_of(problem(), &assets, Some(&FIX.to_owned()));
        assert_eq!(shaped.status, ProblemStatus::NeedsReprocess);
        assert_eq!(shaped.severity, Severity::Minor);
        assert_eq!(shaped.linked_fix_id.as_deref(), Some(FIX));
        assert_eq!(shaped.created_at_ms, 1_000);
        let [known, unknown] = shaped.evidence_assets.as_slice() else {
            panic!("two pieces of evidence expected");
        };
        assert_eq!(known.run_key.as_str(), RUN);
        assert_eq!(known.url, media::url(ARTIFACT));
        assert_eq!(unknown.run_key.as_str(), "");
    }

    #[test]
    fn a_fix_links_its_files_by_media_url_and_numbers_its_steps() {
        let fix = Fix {
            id: FIX.to_owned(),
            project_id: "demo".to_owned(),
            title: "Close the gap".to_owned(),
            summary: String::new(),
            status: FixStatus::Approved,
            problems: vec![PROBLEM.to_owned()],
            tactics: vec![],
            files: vec![],
            comparisons: vec![Comparison {
                label: "home".to_owned(),
                route: String::new(),
                viewport: String::new(),
                theme: String::new(),
                before: ARTIFACT.to_owned(),
                after: OTHER_ARTIFACT.to_owned(),
                diff: None,
                changed_ratio: None,
            }],
            selections: vec![],
            steps: vec![Step {
                step: NewStep {
                    title: "Edit".to_owned(),
                    detail: String::new(),
                    verification: String::new(),
                    evidence: Some(ARTIFACT.to_owned()),
                },
                created_at: SECOND_MS,
            }],
            reviews: vec![Review {
                author: Some("ada".to_owned()),
                outcome: FixStatus::Approved,
                comment: "ok".to_owned(),
                created_at: SECOND_MS,
            }],
            author: None,
            created_at: SECOND_MS,
            updated_at: SECOND_MS,
        };
        let shaped = fix_of(fix);
        assert_eq!(shaped.status, FixStatus::Approved);
        assert_eq!(shaped.comparisons[0].before_url, media::url(ARTIFACT));
        assert_eq!(shaped.comparisons[0].after_url, media::url(OTHER_ARTIFACT));
        assert_eq!(shaped.steps[0].index, 1);
        assert_eq!(shaped.steps[0].evidence_url, Some(media::url(ARTIFACT)));
        assert_eq!(shaped.reviews[0].action, "approved");
        assert_eq!(shaped.reviews[0].author, "ada");
    }
}
