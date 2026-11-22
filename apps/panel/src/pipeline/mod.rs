use super::{
    Arc, Body, Cx, Deserialize, FilePath, PageResult, Panel, PathBuf, Response, Run, StatusCode,
    auth, db_error, design_record, fix_record, header, pages, problem_record, proposal_record,
    redirect, storage, view,
};
use crate::components::badge::{BadgeVariant, badge};
use auth::{Account, Role};
use fix_record::{ComparisonPair, FixRecord, FixReview, FixStatus, FixStep};
use problem_record::{
    EvidenceAsset, Problem, ProblemComment, ProblemOrigin, ProblemStatus, Severity, TaskExecution,
    TaskStepLog, ViolatedRule,
};

pub struct AssetOption {
    pub run_key: String,
    pub scenario: String,
    pub name: String,
    pub url: String,
    pub kind: &'static str,
}

pub fn data_root(panel: &Panel) -> PathBuf {
    panel
        .design
        .parent()
        .map_or_else(|| PathBuf::from(".rupicola"), FilePath::to_path_buf)
}

pub fn sync_legacy_findings(panel: &Panel, project_id: &str, runs: &[&Run]) -> Vec<Problem> {
    let data = data_root(panel);
    let mut existing = problem_record::entries(&data, project_id).unwrap_or_default();
    if existing.is_empty() {
        for run in runs {
            let manifest = &run.manifest;
            if manifest.project.id != project_id {
                continue;
            }
            for item in &manifest.findings {
                let candidate = problem_record::infer_from_finding(
                    project_id,
                    &run.key,
                    &item.note,
                    item.evidence.as_deref(),
                );
                if existing.iter().any(|p| p.diagnosis == candidate.diagnosis) {
                    continue;
                }
                if problem_record::save(&data, &candidate).is_ok() {
                    existing.push(candidate);
                }
            }
        }
        existing.sort_by_key(|a| std::cmp::Reverse(a.created_at_ms));
    }
    existing
}

fn severity_badge(sev: Severity) -> BadgeVariant {
    match sev {
        Severity::Critical => BadgeVariant::Destructive,
        Severity::Major => BadgeVariant::Primary,
        Severity::Minor | Severity::Polish => BadgeVariant::Secondary,
    }
}

fn problem_status_badge(st: ProblemStatus) -> BadgeVariant {
    match st {
        ProblemStatus::Open | ProblemStatus::NeedsReprocess => BadgeVariant::Destructive,
        ProblemStatus::InProgress => BadgeVariant::Primary,
        ProblemStatus::Resolved | ProblemStatus::Intentional | ProblemStatus::FalsePositive => {
            BadgeVariant::Secondary
        }
    }
}

fn fix_status_badge(st: FixStatus) -> BadgeVariant {
    match st {
        FixStatus::Approved => BadgeVariant::Primary,
        FixStatus::ChangesRequested => BadgeVariant::Destructive,
        FixStatus::PendingReview | FixStatus::Superseded => BadgeVariant::Secondary,
    }
}

fn is_video_url(url: &str) -> bool {
    let path = url.split('?').next().unwrap_or(url);
    matches!(path.rsplit_once('.'), Some((_, "mp4" | "webm")))
}

fn extract_media_url_from_text(text: &str) -> Option<String> {
    let idx = text.find("/media/")?;
    let token: String = text[idx..]
        .chars()
        .take_while(|ch| !ch.is_whitespace())
        .collect();
    let clean = token.trim_end_matches(['.', ',', ';', ')']);
    if matches!(clean.rsplit_once('.'), Some((_, "mp4" | "webm" | "png"))) {
        Some(clean.to_string())
    } else {
        None
    }
}

fn effective_after_url(fix: &FixRecord, pair: &ComparisonPair) -> String {
    if is_video_url(&pair.after_url) {
        return pair.after_url.clone();
    }
    for rev in fix.reviews.iter().rev() {
        if let Some(u) = extract_media_url_from_text(&rev.comment)
            && is_video_url(&u)
        {
            return u;
        }
    }
    for st in fix.steps.iter().rev() {
        if let Some(ev) = &st.evidence_url
            && is_video_url(ev)
        {
            return ev.clone();
        }
    }
    pair.after_url.clone()
}

pub mod captures;
pub mod fixes;
pub mod problems;
pub mod storage_view;

pub(crate) use captures::analyze_run;
pub use captures::{AnalyzeRunForm, analyze_run_action, render_captures};
pub(crate) use fixes::review_fix;
pub use fixes::{CreateFixForm, ReviewFixForm, create_fix_action, render_fixes, review_fix_action};
pub use problems::{
    CreateProblemForm, ProgressProblemForm, TriageProblemForm, create_problem_action,
    problems_status_json, progress_problem_action, render_problems, triage_problem_action,
};
pub(crate) use problems::{progress_problem, triage_problem};
pub use storage_view::{
    StorageCleanForm, StorageConfigForm, StorageOffloadForm, clean_storage_action,
    offload_storage_action, render_storage, save_storage_config_action,
};
