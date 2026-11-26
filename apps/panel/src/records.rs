//! Problems, fixes, and design records as the panel shows them. The backend owns them: this
//! reads them through the [`Api`] and shapes each into the view model the screens render.

use super::{PageResult, backend, media, project::AssetOption};
use rupicola_api::{
    ComparisonView, DesignEntry, Fix, FixStatus, Problem as ApiProblem, ProblemOrigin,
    ProblemStatus, Severity,
};
use std::collections::{BTreeMap, BTreeSet};
use topcoat::context::Cx;

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
    pub view: ComparisonView,
    pub base_revision: String,
    pub revision: String,
    /// An integrated pair the approved fixes have outdated.
    pub stale: bool,
    pub before_url: String,
    pub after_url: String,
    pub diff_url: Option<String>,
}

impl ComparisonPair {
    /// What the button and caption say: the view, the label, the revisions the captures came
    /// from and, when it is out of date, that it is.
    #[must_use]
    pub fn caption(&self) -> String {
        let view = match self.view {
            ComparisonView::Isolated => "Isolated",
            ComparisonView::Integrated => "Integrated",
        };
        let mut parts = vec![view.to_owned(), self.label.clone()];
        if !self.base_revision.is_empty() && !self.revision.is_empty() {
            parts.push(format!("{} → {}", self.base_revision, self.revision));
        }
        if self.stale {
            parts.push("stale, recapture".to_owned());
        }
        parts.join(" · ")
    }
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
                view: pair.view,
                base_revision: pair.base_revision,
                revision: pair.revision,
                stale: pair.stale,
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
    let detailed = backend::fixes_detailed(cx, project)
        .await
        .as_ref()
        .map_err(|e| *e)?;
    let mut list: Vec<FixRecord> = detailed.iter().cloned().map(fix_of).collect();
    list.sort_by_key(|fix| std::cmp::Reverse(fix.created_at_ms));
    Ok(list)
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
    let detailed = backend::problems_detailed(cx, project)
        .await
        .as_ref()
        .map_err(|e| *e)?;
    let mut linked: BTreeMap<&String, &String> = BTreeMap::new();
    for fix in fixes {
        for problem in &fix.problem_ids {
            linked.entry(problem).or_insert(&fix.id);
        }
    }
    let mut list: Vec<Problem> = detailed
        .iter()
        .cloned()
        .map(|problem| {
            let fix = linked.get(&problem.id).copied();
            problem_of(problem, assets, fix)
        })
        .collect();
    list.sort_by_key(|problem| std::cmp::Reverse(problem.created_at_ms));
    Ok(list)
}

/// The design records of `project`, newest first.
///
/// # Errors
///
/// The status of a failed call to the backend.
pub async fn design(cx: &Cx, project: &str) -> PageResult<Vec<DesignEntry>> {
    let mut entries = backend::design(cx, project).await.clone()?;
    entries.sort_by_key(|entry| std::cmp::Reverse(entry.created_at));
    Ok(entries)
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
            git_ref: None,
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
                ..Comparison::default()
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

    #[test]
    fn a_comparison_caption_names_its_view_revisions_and_staleness() {
        let pair = |view, stale, revisions: (&str, &str)| ComparisonPair {
            label: "home".to_owned(),
            view,
            base_revision: revisions.0.to_owned(),
            revision: revisions.1.to_owned(),
            stale,
            before_url: String::new(),
            after_url: String::new(),
            diff_url: None,
        };
        assert_eq!(
            pair(ComparisonView::Isolated, false, ("", "")).caption(),
            "Isolated · home"
        );
        assert_eq!(
            pair(ComparisonView::Integrated, true, ("a1b2c3d", "e4f5a6b")).caption(),
            "Integrated · home · a1b2c3d → e4f5a6b · stale, recapture"
        );
    }
}
