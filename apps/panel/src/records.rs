//! Problems and fixes as the panel shows them. The backend owns them: this reads them through
//! the [`Api`] and shapes each into the view model the screens render, with the id of every
//! artifact standing where a media path used to.

use super::{
    Account, PageResult, Panel, auth, backend, design_record, fix_record, media, problem_record,
    project::AssetOption,
};
use rupicola_api::{Fix, Problem};
use std::collections::BTreeMap;
use std::sync::Arc;
use tokio::task::JoinSet;
use topcoat::context::{Cx, context};

/// Kind shown for evidence the capture lists do not describe.
const DEFAULT_EVIDENCE_KIND: &str = "Screenshot";
/// Author shown when the backend recorded none.
const UNKNOWN_AUTHOR: &str = "unknown";

fn millis(value: i64) -> u64 {
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

fn problem_of(
    problem: Problem,
    assets: &[AssetOption],
    linked_fix: Option<&String>,
) -> problem_record::Problem {
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
            problem_record::EvidenceAsset {
                run_key: known.map(|asset| asset.run_key.clone()).unwrap_or_default(),
                asset_path: id.clone(),
                kind: known
                    .map_or(DEFAULT_EVIDENCE_KIND, |asset| asset.kind)
                    .to_owned(),
                url,
                label: label.clone(),
            }
        })
        .collect();
    problem_record::Problem {
        id: problem.id,
        project_id: problem.project_id,
        title: problem.title,
        status: problem_record::ProblemStatus::from_slug(problem.status.as_str())
            .unwrap_or(problem_record::ProblemStatus::Open),
        severity: problem_record::Severity::from_slug(problem.severity.as_str())
            .unwrap_or(problem_record::Severity::Major),
        origin: match problem.origin {
            rupicola_api::ProblemOrigin::AgentMeasured => {
                problem_record::ProblemOrigin::AgentMeasured
            }
            rupicola_api::ProblemOrigin::AgentVisual => problem_record::ProblemOrigin::AgentVisual,
            rupicola_api::ProblemOrigin::UserReported => {
                problem_record::ProblemOrigin::UserReported
            }
        },
        route: problem.route,
        viewport: problem.viewport,
        theme: problem.theme,
        diagnosis: problem.diagnosis,
        proposed_solution: problem.proposed_solution,
        violated_rules: problem
            .rules
            .into_iter()
            .map(|rule| problem_record::ViolatedRule {
                scope: rule.scope,
                code: rule.code,
                title: rule.title,
            })
            .collect(),
        evidence_assets,
        comments: problem
            .comments
            .into_iter()
            .map(|comment| problem_record::ProblemComment {
                author: author(comment.author),
                text: comment.text,
                created_at_ms: millis(comment.created_at),
            })
            .collect(),
        linked_fix_id: linked_fix.cloned(),
        execution: None,
        author: author(problem.author),
        created_at_ms: millis(problem.created_at),
        updated_at_ms: millis(problem.updated_at),
    }
}

fn fix_of(fix: Fix) -> fix_record::FixRecord {
    fix_record::FixRecord {
        id: fix.id,
        project_id: fix.project_id,
        problem_ids: fix.problems,
        title: fix.title,
        summary: fix.summary,
        status: fix_record::FixStatus::from_slug(fix.status.as_str())
            .unwrap_or(fix_record::FixStatus::PendingReview),
        tactics: fix.tactics,
        modified_files: fix.files,
        comparisons: fix
            .comparisons
            .into_iter()
            .map(|pair| fix_record::ComparisonPair {
                label: pair.label,
                route: pair.route,
                viewport: pair.viewport,
                theme: pair.theme,
                before_url: media::url(&pair.before),
                after_url: media::url(&pair.after),
                diff_url: pair.diff.as_deref().map(media::url),
                changed_ratio: pair.changed_ratio,
            })
            .collect(),
        steps: fix
            .steps
            .into_iter()
            .enumerate()
            .map(|(index, step)| fix_record::FixStep {
                index: index + 1,
                title: step.step.title,
                detail: step.step.detail,
                verification: step.step.verification,
                evidence_url: step.step.evidence.as_deref().map(media::url),
                timestamp_ms: millis(step.created_at),
            })
            .collect(),
        reviews: fix
            .reviews
            .into_iter()
            .map(|review| fix_record::FixReview {
                author: author(review.author),
                action: review.outcome.as_str().to_owned(),
                comment: review.comment,
                created_at_ms: millis(review.created_at),
            })
            .collect(),
        author: author(fix.author),
        created_at_ms: millis(fix.created_at),
        updated_at_ms: millis(fix.updated_at),
    }
}

/// Every fix of `project`, most recently updated first.
///
/// # Errors
///
/// The status of a failed call to the backend.
pub async fn fixes(cx: &Cx, project: &str) -> PageResult<Vec<fix_record::FixRecord>> {
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
    fixes: &[fix_record::FixRecord],
) -> PageResult<Vec<problem_record::Problem>> {
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
    candidate: &problem_record::Problem,
    status: Option<rupicola_api::ProblemStatus>,
) -> PageResult<()> {
    let new = rupicola_api::NewProblem {
        title: candidate.title.clone(),
        severity: candidate
            .severity
            .slug()
            .parse()
            .unwrap_or(rupicola_api::Severity::Major),
        origin: match candidate.origin {
            problem_record::ProblemOrigin::AgentMeasured => {
                rupicola_api::ProblemOrigin::AgentMeasured
            }
            problem_record::ProblemOrigin::AgentVisual => rupicola_api::ProblemOrigin::AgentVisual,
            problem_record::ProblemOrigin::UserReported => {
                rupicola_api::ProblemOrigin::UserReported
            }
        },
        route: candidate.route.clone(),
        viewport: candidate.viewport.clone(),
        theme: candidate.theme.clone(),
        diagnosis: candidate.diagnosis.clone(),
        proposed_solution: candidate.proposed_solution.clone(),
        rules: candidate
            .violated_rules
            .iter()
            .map(|rule| rupicola_api::Rule {
                scope: rule.scope.clone(),
                code: rule.code.clone(),
                title: rule.title.clone(),
            })
            .collect(),
        evidence: candidate
            .evidence_assets
            .iter()
            .map(|asset| artifact_of(&asset.asset_path).to_owned())
            .collect(),
    };
    let created = panel
        .api
        .create_problem(&account.session, &candidate.project_id, &new)
        .await
        .map_err(|e| auth::api_error(&e))?;
    if let Some(status) = status {
        let patch = rupicola_api::ProblemPatch {
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
pub async fn design(cx: &Cx, project: &str) -> PageResult<Vec<design_record::Entry>> {
    let entries = backend::design(cx, project)
        .await
        .as_ref()
        .map_err(|e| *e)?;
    Ok(entries.iter().cloned().map(design_entry_of).collect())
}

fn design_entry_of(record: rupicola_api::DesignEntry) -> design_record::Entry {
    let entry = record.entry;
    design_record::Entry {
        id: record.id,
        timestamp_ms: millis(record.created_at),
        kind: match entry.kind {
            rupicola_api::DesignKind::Brief => design_record::Kind::Brief,
            rupicola_api::DesignKind::Direction => design_record::Kind::Direction,
            rupicola_api::DesignKind::Tactic => design_record::Kind::Tactic,
            rupicola_api::DesignKind::Decision => design_record::Kind::Decision,
        },
        status: match entry.status {
            rupicola_api::DesignStatus::Proposed => design_record::Status::Proposed,
            rupicola_api::DesignStatus::Adopted => design_record::Status::Adopted,
            rupicola_api::DesignStatus::Rejected => design_record::Status::Rejected,
        },
        title: entry.title,
        note: entry.note,
        surface: entry.surface,
        supersedes: entry.supersedes,
        reference: entry.reference,
        evidence: entry.evidence,
        author: record.author,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rupicola_api::{
        Comparison, FixStatus, NewStep, ProblemOrigin, ProblemStatus, Review, Severity, Step,
    };

    const ARTIFACT: &str = "artifact-1";
    const OTHER_ARTIFACT: &str = "artifact-2";
    const RUN: &str = "capture-1";
    const FIX: &str = "fix-1";
    const PROBLEM: &str = "problem-1";
    const SECOND_MS: i64 = 1_000;

    fn problem() -> Problem {
        Problem {
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
        assert_eq!(shaped.status, problem_record::ProblemStatus::NeedsReprocess);
        assert_eq!(shaped.severity, problem_record::Severity::Minor);
        assert_eq!(shaped.linked_fix_id.as_deref(), Some(FIX));
        assert_eq!(shaped.author, UNKNOWN_AUTHOR);
        assert_eq!(shaped.created_at_ms, 1_000);
        let [known, unknown] = shaped.evidence_assets.as_slice() else {
            panic!("two pieces of evidence expected");
        };
        assert_eq!(
            (known.run_key.as_str(), known.kind.as_str()),
            (RUN, "Recording")
        );
        assert_eq!(known.url, media::url(ARTIFACT));
        assert_eq!(
            (unknown.run_key.as_str(), unknown.kind.as_str()),
            ("", DEFAULT_EVIDENCE_KIND)
        );
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
        assert_eq!(shaped.status, fix_record::FixStatus::Approved);
        assert_eq!(shaped.comparisons[0].before_url, media::url(ARTIFACT));
        assert_eq!(shaped.comparisons[0].after_url, media::url(OTHER_ARTIFACT));
        assert_eq!(shaped.steps[0].index, 1);
        assert_eq!(shaped.steps[0].evidence_url, Some(media::url(ARTIFACT)));
        assert_eq!(shaped.reviews[0].action, "approved");
    }
}
