use crate::{
    fix_record::{self, ComparisonPair, FixRecord, FixReview, FixStatus, FixStep},
    problem_record::{
        self, EvidenceAsset, Problem, ProblemComment, ProblemOrigin, ProblemStatus, Severity,
        TaskExecution, TaskStepLog, ViolatedRule,
    },
    service, storage,
};
use anyhow::{Context, Result, anyhow, bail};
use std::fs;

#[derive(clap::Subcommand)]
#[allow(clippy::large_enum_variant)]
pub enum ProblemAction {
    /// List problem records in the project triage queue.
    List {
        #[arg(long)]
        project: Option<String>,
        #[arg(long, value_enum)]
        status: Option<ProblemStatus>,
        #[arg(long)]
        json: bool,
    },
    /// Show a single problem record.
    Get {
        id: String,
        #[arg(long)]
        project: Option<String>,
        #[arg(long)]
        json: bool,
    },
    /// Add a structured problem to the project queue.
    Add {
        #[arg(long)]
        title: String,
        #[arg(long, default_value = "/")]
        route: String,
        #[arg(long, default_value = "1440×900")]
        viewport: String,
        #[arg(long, default_value = "dark")]
        theme: String,
        #[arg(long, value_enum, default_value = "major")]
        severity: Severity,
        #[arg(long)]
        diagnosis: String,
        #[arg(long)]
        solution: String,
        /// Violated rules in format "scope:code:title" or "code".
        #[arg(long = "rule")]
        rules: Vec<String>,
        /// Evidence asset URLs (e.g. /media/0/run/checkpoints/shot.png).
        #[arg(long = "evidence")]
        evidence: Vec<String>,
        #[arg(long)]
        project: Option<String>,
    },
    /// Update problem status (e.g. intentional, false-positive, needs-reprocess, in-progress, resolved).
    Triage {
        id: String,
        #[arg(long, value_enum)]
        status: ProblemStatus,
        #[arg(long)]
        comment: Option<String>,
        #[arg(long)]
        solution: Option<String>,
        #[arg(long)]
        agent: Option<String>,
        #[arg(long)]
        project: Option<String>,
    },
    /// Claim a queued problem for an agent and initialize its live execution tracker.
    Claim {
        id: String,
        #[arg(long)]
        agent: String,
        #[arg(long, default_value = "Inspecting problem evidence and violated norms")]
        activity: String,
        #[arg(long, default_value_t = 4)]
        total_steps: usize,
        #[arg(long)]
        project: Option<String>,
    },
    /// Report live task progress for an in-progress problem fix.
    Progress {
        id: String,
        #[arg(long)]
        agent: Option<String>,
        /// Stage slug: queued | dispatched | claimed | fixing | verifying | completed
        #[arg(long, default_value = "fixing")]
        stage: String,
        /// Completion percentage (0..=100).
        #[arg(long, default_value_t = 50)]
        pct: u8,
        #[arg(long, default_value_t = 2)]
        step: usize,
        #[arg(long, default_value_t = 4)]
        total_steps: usize,
        #[arg(long)]
        activity: String,
        #[arg(long)]
        project: Option<String>,
    },
    /// Analyze a capture run directory or run name and promote its findings into structured problems.
    AnalyzeRun {
        run: String,
        #[arg(long)]
        project: Option<String>,
        #[arg(long)]
        json: bool,
    },
}

#[derive(clap::Subcommand)]
#[allow(clippy::large_enum_variant)]
pub enum FixAction {
    /// List recorded fixes and before/after comparisons.
    List {
        #[arg(long)]
        project: Option<String>,
        #[arg(long, value_enum)]
        status: Option<FixStatus>,
        #[arg(long)]
        json: bool,
    },
    /// Show a single fix record.
    Get {
        id: String,
        #[arg(long)]
        project: Option<String>,
        #[arg(long)]
        json: bool,
    },
    /// Record a fix with Before/After assets, applied tactics, and intermediate steps.
    Record {
        #[arg(long)]
        title: String,
        #[arg(long)]
        summary: String,
        #[arg(long = "problem")]
        problems: Vec<String>,
        #[arg(long, default_value = "/")]
        route: String,
        #[arg(long, default_value = "1440×900")]
        viewport: String,
        #[arg(long, default_value = "dark")]
        theme: String,
        #[arg(long)]
        before_url: String,
        #[arg(long)]
        after_url: String,
        #[arg(long)]
        diff_url: Option<String>,
        #[arg(long = "tactic")]
        tactics: Vec<String>,
        #[arg(long = "file")]
        files: Vec<String>,
        /// Intermediate step in format "Title :: Detail :: Verification".
        #[arg(long = "step")]
        steps: Vec<String>,
        #[arg(long)]
        project: Option<String>,
    },
    /// Approve, request changes, or update media on a recorded fix.
    Review {
        id: String,
        #[arg(long, value_enum)]
        status: FixStatus,
        #[arg(long)]
        comment: Option<String>,
        #[arg(long)]
        before_url: Option<String>,
        #[arg(long)]
        after_url: Option<String>,
        #[arg(long)]
        diff_url: Option<String>,
        #[arg(long)]
        project: Option<String>,
    },
}

#[derive(clap::Subcommand)]
pub enum StorageAction {
    /// Report disk usage grouped by project, category, date, fix, and capture run.
    Status {
        #[arg(long)]
        project: Option<String>,
        #[arg(long)]
        json: bool,
    },
    /// Configure external bucket offload (S3/R2/MinIO/rclone/filesystem-bucket).
    Config {
        /// s3-compatible | rclone | filesystem-bucket
        #[arg(long, default_value = "s3-compatible")]
        provider: String,
        #[arg(long)]
        bucket: String,
        #[arg(long, default_value = "https://s3.amazonaws.com")]
        endpoint_url: String,
        #[arg(long, default_value = "auto")]
        region: String,
        #[arg(long, default_value = "rupicola")]
        prefix: String,
        #[arg(long, default_value = "")]
        public_base_url: String,
        /// manual | on-fix-approved | after-capture
        #[arg(long, default_value = "manual")]
        auto_policy: String,
        #[arg(long, default_value_t = true)]
        delete_local: bool,
        #[arg(long, default_value_t = 2)]
        keep_latest_runs: usize,
    },
    /// Clean up local storage (strip raw frames, prune unpinned runs, or clean by date).
    Clean {
        /// strip-frames | prune-unpinned | delete-run
        #[arg(long, default_value = "strip-frames")]
        mode: String,
        #[arg(long)]
        project: Option<String>,
        #[arg(long, default_value_t = 2)]
        keep_latest: usize,
        #[arg(long)]
        date: Option<String>,
        #[arg(long)]
        run: Option<String>,
        #[arg(long)]
        force: bool,
        #[arg(long)]
        json: bool,
    },
    /// Offload a capture run or all runs of a fix to the configured external bucket.
    Offload {
        #[arg(long)]
        run: Option<String>,
        #[arg(long)]
        fix: Option<String>,
        #[arg(long)]
        project: Option<String>,
        #[arg(long)]
        json: bool,
    },
}

fn resolve_project(explicit: Option<String>) -> Result<String> {
    match explicit {
        Some(id) if problem_record::valid_id(&id) => Ok(id),
        Some(_) => bail!("Invalid project ID"),
        None => Ok(service::detect_project()?.0),
    }
}

pub fn parse_rule(raw: &str) -> ViolatedRule {
    let parts: Vec<&str> = raw.splitn(3, ':').collect();
    match parts.as_slice() {
        [scope, code, title] => ViolatedRule {
            scope: scope.trim().to_string(),
            code: code.trim().to_string(),
            title: title.trim().to_string(),
        },
        [code, title] => ViolatedRule {
            scope: if code.starts_with("Design") {
                "project"
            } else {
                "global"
            }
            .into(),
            code: code.trim().to_string(),
            title: title.trim().to_string(),
        },
        _ => ViolatedRule {
            scope: if raw.starts_with("Design") {
                "project"
            } else {
                "global"
            }
            .into(),
            code: raw.trim().to_string(),
            title: raw.trim().to_string(),
        },
    }
}

#[expect(
    clippy::too_many_lines,
    reason = "the CLI dispatcher keeps problem state transitions and their output together"
)]
pub fn run_problem(action: ProblemAction) -> Result<u8> {
    let data = service::data()?;
    match action {
        ProblemAction::List {
            project,
            status,
            json,
        } => {
            let pid = resolve_project(project)?;
            let mut list = problem_record::entries(&data, &pid)?;
            if let Some(st) = status {
                list.retain(|p| p.status == st);
            }
            if json {
                println!("{}", serde_json::to_string_pretty(&list)?);
            } else {
                for p in &list {
                    println!(
                        "{} [{} · {}] ({} @ {} {}) {}",
                        p.id,
                        p.status.label(),
                        p.severity.label(),
                        p.route,
                        p.viewport,
                        p.theme,
                        p.title
                    );
                }
            }
            Ok(0)
        }
        ProblemAction::Get { id, project, json } => {
            let pid = resolve_project(project)?;
            let p = problem_record::load(&data, &pid, &id)?;
            if json {
                println!("{}", serde_json::to_string_pretty(&p)?);
            } else {
                println!("{} — {} ({})", p.id, p.title, p.status.label());
                println!("Diagnosis: {}", p.diagnosis);
                println!("Proposed solution: {}", p.proposed_solution);
            }
            Ok(0)
        }
        ProblemAction::Add {
            title,
            route,
            viewport,
            theme,
            severity,
            diagnosis,
            solution,
            rules,
            evidence,
            project,
        } => {
            let pid = resolve_project(project)?;
            let (id, now) = problem_record::next_id();
            let violated_rules = if rules.is_empty() {
                vec![ViolatedRule {
                    scope: "global".into(),
                    code: "visual-verification.md#visual-inspection".into(),
                    title: "Visual Craft & Structural Hierarchy".into(),
                }]
            } else {
                rules.iter().map(|r| parse_rule(r)).collect()
            };
            let evidence_assets = evidence
                .into_iter()
                .map(|url| EvidenceAsset {
                    run_key: String::new(),
                    asset_path: url.clone(),
                    url,
                    kind: "Screenshot".into(),
                    label: format!("{route} · {viewport} · {theme}"),
                })
                .collect();
            let item = Problem {
                id: id.clone(),
                project_id: pid,
                title,
                status: ProblemStatus::Open,
                severity,
                origin: ProblemOrigin::AgentVisual,
                route,
                viewport,
                theme,
                diagnosis,
                proposed_solution: solution,
                violated_rules,
                evidence_assets,
                comments: Vec::new(),
                linked_fix_id: None,
                execution: None,
                author: "agent".into(),
                created_at_ms: now,
                updated_at_ms: now,
            };
            problem_record::save(&data, &item)?;
            println!("{id}");
            Ok(0)
        }
        ProblemAction::Triage {
            id,
            status,
            comment,
            solution,
            agent,
            project,
        } => {
            let pid = resolve_project(project)?;
            let mut item = problem_record::load(&data, &pid, &id)?;
            let (_, now) = problem_record::next_id();
            item.status = status;
            item.updated_at_ms = now;
            if let Some(sol) = solution.filter(|s| !s.trim().is_empty()) {
                item.proposed_solution = sol;
            }
            if let Some(text) = comment.filter(|s| !s.trim().is_empty()) {
                item.comments.push(ProblemComment {
                    author: "cli".into(),
                    text,
                    created_at_ms: now,
                });
            }
            if status == ProblemStatus::InProgress {
                let agent_name = agent.filter(|s| !s.trim().is_empty());
                let (stage, pct, step_cur, msg) = if let Some(ref a) = agent_name {
                    (
                        "claimed".to_string(),
                        25,
                        1,
                        format!("Claimed by {a} — inspecting evidence and preparing fix"),
                    )
                } else {
                    (
                        "queued".to_string(),
                        5,
                        0,
                        "Queued for fix — waiting for an agent to claim task".to_string(),
                    )
                };
                let mut exec = item.execution.take().unwrap_or(TaskExecution {
                    agent_id: agent_name.clone(),
                    stage: stage.clone(),
                    progress_pct: pct,
                    step_current: step_cur,
                    step_total: 4,
                    current_activity: msg.clone(),
                    logs: Vec::new(),
                    started_at_ms: now,
                    updated_at_ms: now,
                });
                if agent_name.is_some() {
                    exec.agent_id = agent_name;
                }
                exec.stage.clone_from(&stage);
                exec.progress_pct = pct;
                exec.step_current = step_cur;
                exec.current_activity.clone_from(&msg);
                exec.updated_at_ms = now;
                exec.logs.push(TaskStepLog {
                    stage,
                    progress_pct: pct,
                    detail: msg,
                    timestamp_ms: now,
                });
                item.execution = Some(exec);
            }
            problem_record::save(&data, &item)?;
            println!("Updated problem {id} -> {}", status.slug());
            Ok(0)
        }
        ProblemAction::Claim {
            id,
            agent,
            activity,
            total_steps,
            project,
        } => {
            let pid = resolve_project(project)?;
            let mut item = problem_record::load(&data, &pid, &id)?;
            let (_, now) = problem_record::next_id();
            item.status = ProblemStatus::InProgress;
            item.updated_at_ms = now;
            let total = total_steps.max(1);
            let mut exec = item.execution.take().unwrap_or(TaskExecution {
                agent_id: Some(agent.clone()),
                stage: "claimed".into(),
                progress_pct: 25,
                step_current: 1,
                step_total: total,
                current_activity: activity.clone(),
                logs: Vec::new(),
                started_at_ms: now,
                updated_at_ms: now,
            });
            exec.agent_id = Some(agent.clone());
            exec.stage = "claimed".into();
            exec.progress_pct = 25;
            exec.step_current = 1;
            exec.step_total = total;
            exec.current_activity.clone_from(&activity);
            exec.updated_at_ms = now;
            exec.logs.push(TaskStepLog {
                stage: "claimed".into(),
                progress_pct: 25,
                detail: format!("[{agent}] {activity}"),
                timestamp_ms: now,
            });
            item.execution = Some(exec);
            problem_record::save(&data, &item)?;
            println!("Claimed problem {id} for agent {agent}");
            Ok(0)
        }
        ProblemAction::Progress {
            id,
            agent,
            stage,
            pct,
            step,
            total_steps,
            activity,
            project,
        } => {
            let pid = resolve_project(project)?;
            let mut item = problem_record::load(&data, &pid, &id)?;
            let (_, now) = problem_record::next_id();
            let clamped_pct = pct.min(100);
            if clamped_pct >= 100 || stage == "completed" {
                item.status = ProblemStatus::Resolved;
            } else {
                item.status = ProblemStatus::InProgress;
            }
            item.updated_at_ms = now;
            let total = total_steps.max(1);
            let step_cur = step.min(total);
            let mut exec = item.execution.take().unwrap_or(TaskExecution {
                agent_id: agent.clone(),
                stage: stage.clone(),
                progress_pct: clamped_pct,
                step_current: step_cur,
                step_total: total,
                current_activity: activity.clone(),
                logs: Vec::new(),
                started_at_ms: now,
                updated_at_ms: now,
            });
            if let Some(a) = agent.filter(|s| !s.trim().is_empty()) {
                exec.agent_id = Some(a);
            }
            exec.stage.clone_from(&stage);
            exec.progress_pct = clamped_pct;
            exec.step_current = step_cur;
            exec.step_total = total;
            exec.current_activity.clone_from(&activity);
            exec.updated_at_ms = now;
            exec.logs.push(TaskStepLog {
                stage,
                progress_pct: clamped_pct,
                detail: activity,
                timestamp_ms: now,
            });
            item.execution = Some(exec);
            problem_record::save(&data, &item)?;
            println!("Updated progress on {id}: {clamped_pct}% ({step_cur}/{total})");
            Ok(0)
        }
        ProblemAction::AnalyzeRun { run, project, json } => {
            let pid = resolve_project(project)?;
            let run_name = run.trim_start_matches("0/");
            let manifest_path = data.join("captures").join(run_name).join("manifest.json");
            let bytes =
                fs::read(&manifest_path).with_context(|| format!("{}", manifest_path.display()))?;
            let val: serde_json::Value = serde_json::from_slice(&bytes)?;
            let existing = problem_record::entries(&data, &pid).unwrap_or_default();
            let mut created = Vec::new();
            let run_key = format!("0/{run_name}");
            for finding in val["findings"].as_array().into_iter().flatten() {
                let Some(note) = finding["note"].as_str() else {
                    continue;
                };
                let evidence = finding["evidence"].as_str();
                let candidate = problem_record::infer_from_finding(&pid, &run_key, note, evidence);
                let rule_code = candidate
                    .violated_rules
                    .first()
                    .map_or("", |r| r.code.as_str());
                if problem_record::is_suppressed(
                    &existing,
                    &candidate.route,
                    rule_code,
                    &candidate.title,
                ) {
                    continue;
                }
                if existing.iter().any(|p| p.diagnosis == candidate.diagnosis) {
                    continue;
                }
                problem_record::save(&data, &candidate)?;
                created.push(candidate);
            }
            if json {
                println!("{}", serde_json::to_string_pretty(&created)?);
            } else {
                println!("Created {} problem(s) from run {run_name}", created.len());
            }
            Ok(0)
        }
    }
}

#[expect(
    clippy::too_many_lines,
    reason = "the CLI dispatcher keeps fix state transitions and their output together"
)]
pub fn run_fix(action: FixAction) -> Result<u8> {
    let data = service::data()?;
    match action {
        FixAction::List {
            project,
            status,
            json,
        } => {
            let pid = resolve_project(project)?;
            let mut list = fix_record::entries(&data, &pid)?;
            if let Some(st) = status {
                list.retain(|f| f.status == st);
            }
            if json {
                println!("{}", serde_json::to_string_pretty(&list)?);
            } else {
                for f in &list {
                    println!(
                        "{} [{}] {} ({} comparisons, {} steps)",
                        f.id,
                        f.status.label(),
                        f.title,
                        f.comparisons.len(),
                        f.steps.len()
                    );
                }
            }
            Ok(0)
        }
        FixAction::Get { id, project, json } => {
            let pid = resolve_project(project)?;
            let f = fix_record::load(&data, &pid, &id)?;
            if json {
                println!("{}", serde_json::to_string_pretty(&f)?);
            } else {
                println!("{} — {} ({})", f.id, f.title, f.status.label());
                println!("{}", f.summary);
            }
            Ok(0)
        }
        FixAction::Record {
            title,
            summary,
            problems,
            route,
            viewport,
            theme,
            before_url,
            after_url,
            diff_url,
            tactics,
            files,
            steps,
            project,
        } => {
            let pid = resolve_project(project)?;
            let (id, now) = fix_record::next_id();
            let parsed_steps: Vec<FixStep> = steps
                .iter()
                .enumerate()
                .map(|(i, raw)| {
                    let parts: Vec<&str> = raw.splitn(3, "::").collect();
                    let (st_title, detail, verification) = match parts.as_slice() {
                        [a, b, c] => (a.trim(), b.trim(), c.trim()),
                        [a, b] => (a.trim(), b.trim(), "Verified visually"),
                        _ => (raw.trim(), "", "Verified visually"),
                    };
                    FixStep {
                        index: i + 1,
                        title: st_title.to_string(),
                        detail: detail.to_string(),
                        verification: verification.to_string(),
                        evidence_url: None,
                        timestamp_ms: now,
                    }
                })
                .collect();
            let pair = ComparisonPair {
                label: format!("{route} · {viewport} · {theme}"),
                route,
                viewport,
                theme,
                before_url,
                after_url,
                diff_url,
                changed_ratio: None,
            };
            let record = FixRecord {
                id: id.clone(),
                project_id: pid.clone(),
                problem_ids: problems.clone(),
                title,
                summary,
                status: FixStatus::PendingReview,
                tactics,
                modified_files: files,
                comparisons: vec![pair],
                steps: parsed_steps,
                reviews: Vec::new(),
                author: "agent".into(),
                created_at_ms: now,
                updated_at_ms: now,
            };
            fix_record::save(&data, &record)?;
            for prob_id in &problems {
                if let Ok(mut p) = problem_record::load(&data, &pid, prob_id) {
                    p.status = ProblemStatus::Resolved;
                    p.linked_fix_id = Some(id.clone());
                    p.updated_at_ms = now;
                    let total = record.steps.len().max(1);
                    let mut exec = p.execution.take().unwrap_or(TaskExecution {
                        agent_id: Some("rupicola-agent".into()),
                        stage: "completed".into(),
                        progress_pct: 100,
                        step_current: total,
                        step_total: total,
                        current_activity: format!("Fix recorded: {}", record.title),
                        logs: Vec::new(),
                        started_at_ms: now,
                        updated_at_ms: now,
                    });
                    if exec.agent_id.is_none() {
                        exec.agent_id = Some("rupicola-agent".into());
                    }
                    exec.stage = "completed".into();
                    exec.progress_pct = 100;
                    exec.step_current = total;
                    exec.step_total = total;
                    exec.current_activity =
                        format!("Verified & recorded Fix #{id} with Before/After comparison");
                    exec.updated_at_ms = now;
                    for st in &record.steps {
                        let pct = u8::try_from((st.index.saturating_mul(100) / total).min(100))
                            .expect("progress is bounded by 100");
                        exec.logs.push(TaskStepLog {
                            stage: if st.index == total {
                                "completed".into()
                            } else {
                                "fixing".into()
                            },
                            progress_pct: pct,
                            detail: format!(
                                "Step {}/{}: {} — {}",
                                st.index, total, st.title, st.verification
                            ),
                            timestamp_ms: st.timestamp_ms,
                        });
                    }
                    p.execution = Some(exec);
                    let _ = problem_record::save(&data, &p);
                }
            }
            println!("{id}");
            Ok(0)
        }
        FixAction::Review {
            id,
            status,
            comment,
            before_url,
            after_url,
            diff_url,
            project,
        } => {
            let pid = resolve_project(project)?;
            let mut f = fix_record::load(&data, &pid, &id)?;
            let (_, now) = fix_record::next_id();
            f.status = status;
            f.updated_at_ms = now;
            if let Some(pair) = f.comparisons.first_mut() {
                if let Some(b) = before_url.filter(|s| !s.trim().is_empty()) {
                    pair.before_url = b;
                }
                if let Some(a) = after_url.filter(|s| !s.trim().is_empty()) {
                    pair.after_url = a;
                }
                if let Some(d) = diff_url.filter(|s| !s.trim().is_empty()) {
                    pair.diff_url = Some(d);
                }
            }
            if let Some(c) = comment.filter(|s| !s.trim().is_empty()) {
                if let Some(pair) = f.comparisons.first_mut()
                    && let Some(idx) = c.find("/media/")
                {
                    let token: String = c[idx..]
                        .chars()
                        .take_while(|ch| !ch.is_whitespace())
                        .collect();
                    let clean = token.trim_end_matches(['.', ',', ';', ')']);
                    if matches!(clean.rsplit_once('.'), Some((_, "mp4" | "webm" | "png"))) {
                        pair.after_url = clean.to_string();
                    }
                }
                f.reviews.push(FixReview {
                    author: "user".into(),
                    action: status.slug().into(),
                    comment: c,
                    created_at_ms: now,
                });
            }
            fix_record::save(&data, &f)?;
            for prob_id in &f.problem_ids {
                if let Ok(mut p) = problem_record::load(&data, &pid, prob_id) {
                    p.status = if status == FixStatus::ChangesRequested {
                        ProblemStatus::InProgress
                    } else {
                        ProblemStatus::Resolved
                    };
                    p.updated_at_ms = now;
                    let _ = problem_record::save(&data, &p);
                }
            }
            println!("Updated fix {id} -> {}", status.slug());
            Ok(0)
        }
    }
}

#[expect(
    clippy::too_many_lines,
    reason = "the CLI dispatcher keeps storage subcommands and their output together"
)]
pub fn run_storage(action: StorageAction) -> Result<u8> {
    let data = service::data()?;
    match action {
        StorageAction::Status { project, json } => {
            let report = storage::analyze(&data, project.as_deref());
            if json {
                println!("{}", serde_json::to_string_pretty(&report)?);
            } else {
                println!(
                    "Total storage: {} ({} files) | Pinned: {} | Reclaimable: {} | Raw frames: {}",
                    storage::format_bytes(report.total_bytes),
                    report.total_files,
                    storage::format_bytes(report.pinned_bytes),
                    storage::format_bytes(report.reclaimable_bytes),
                    storage::format_bytes(report.raw_frames_bytes),
                );
                for d in &report.by_date {
                    println!(
                        "  Date {}: {} ({} runs, {} fixes, {} reclaimable)",
                        d.date,
                        storage::format_bytes(d.bytes),
                        d.runs_count,
                        d.fixes_count,
                        storage::format_bytes(d.reclaimable_bytes)
                    );
                }
            }
            Ok(0)
        }
        StorageAction::Config {
            provider,
            bucket,
            endpoint_url,
            region,
            prefix,
            public_base_url,
            auto_policy,
            delete_local,
            keep_latest_runs,
        } => {
            let cfg = storage::BucketConfig {
                provider,
                bucket,
                endpoint_url,
                region,
                prefix,
                public_base_url,
                auto_policy,
                delete_local_after_offload: delete_local,
                keep_latest_runs,
            };
            storage::save_config(&data, &cfg)?;
            println!("Saved external bucket configuration");
            Ok(0)
        }
        StorageAction::Clean {
            mode,
            project,
            keep_latest,
            date,
            run,
            force,
            json,
        } => {
            let summary = match mode.as_str() {
                "strip-frames" => storage::strip_raw_frames(&data, project.as_deref())?,
                "prune-unpinned" => storage::prune_unpinned(
                    &data,
                    project.as_deref(),
                    keep_latest,
                    date.as_deref(),
                )?,
                "delete-run" => {
                    let r =
                        run.ok_or_else(|| anyhow!("--run is required with --mode delete-run"))?;
                    storage::delete_run(&data, &r, force)?
                }
                _ => bail!("Unsupported clean mode: {mode}"),
            };
            if json {
                println!("{}", serde_json::to_string_pretty(&summary)?);
            } else {
                println!("{}", summary.note);
            }
            Ok(0)
        }
        StorageAction::Offload {
            run,
            fix,
            project,
            json,
        } => {
            let summary = if let Some(r) = run {
                storage::offload_run(&data, &r)?
            } else if let Some(fid) = fix {
                let pid = resolve_project(project)?;
                let report = storage::analyze(&data, Some(&pid));
                let f = report
                    .by_fix
                    .iter()
                    .find(|x| x.fix_id == fid)
                    .ok_or_else(|| anyhow!("Fix not found"))?;
                let mut total_freed = 0u64;
                let mut total_files = 0usize;
                let mut runs = 0usize;
                for rname in &f.referenced_runs {
                    let s = storage::offload_run(&data, rname)?;
                    total_freed += s.freed_bytes;
                    total_files += s.affected_files;
                    runs += 1;
                }
                storage::ActionSummary {
                    action: "offload-fix".into(),
                    freed_bytes: total_freed,
                    affected_runs: runs,
                    affected_files: total_files,
                    note: format!(
                        "Offloaded {} run(s) for Fix {fid}, freeing {}",
                        runs,
                        storage::format_bytes(total_freed)
                    ),
                }
            } else {
                bail!("Specify --run <run_name> or --fix <fix_id>");
            };
            if json {
                println!("{}", serde_json::to_string_pretty(&summary)?);
            } else {
                println!("{}", summary.note);
            }
            Ok(0)
        }
    }
}
