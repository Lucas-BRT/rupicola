use super::{
    Account, Arc, AssetOption, Body, Cx, Deserialize, EvidenceAsset, PageResult, Panel, Problem,
    ProblemComment, ProblemOrigin, ProblemStatus, Response, Role, Severity, StatusCode,
    TaskExecution, TaskStepLog, ViolatedRule, auth, badge, data_root, db_error, design_record,
    header, pages, problem_record, problem_status_badge, proposal_record, redirect, severity_badge,
    view,
};

#[allow(clippy::too_many_arguments)]
#[expect(
    clippy::too_many_lines,
    reason = "the problems view keeps its markup and presentation branches together"
)]
pub fn render_problems<'a>(
    cx: &'a Cx,
    account: &'a Account,
    role: Role,
    _project_id: &'a str,
    base: &'a str,
    filter: &'a pages::Filter,
    problems: &'a [Problem],
    assets: &'a [AssetOption],
    design_entries: &'a [design_record::Entry],
    mcp_online: bool,
) -> topcoat::view::Child<'a> {
    let status_filter = filter.status.as_deref().unwrap_or("");
    let severity_filter = filter.kind.as_deref().unwrap_or("");
    let q = filter.q.as_deref().unwrap_or("").trim().to_lowercase();
    let prefill_asset = filter.asset.as_deref().unwrap_or("");
    let prefill_name = filter.name.as_deref().unwrap_or("");

    let open_count = problems
        .iter()
        .filter(|p| {
            matches!(
                p.status,
                ProblemStatus::Open | ProblemStatus::NeedsReprocess
            )
        })
        .count();
    let progress_count = problems
        .iter()
        .filter(|p| p.status == ProblemStatus::InProgress)
        .count();
    let intentional_count = problems
        .iter()
        .filter(|p| {
            matches!(
                p.status,
                ProblemStatus::Intentional | ProblemStatus::FalsePositive
            )
        })
        .count();
    let resolved_count = problems
        .iter()
        .filter(|p| p.status == ProblemStatus::Resolved)
        .count();

    let filtered: Vec<&Problem> = problems
        .iter()
        .filter(|p| {
            (status_filter.is_empty() || p.status.slug() == status_filter)
                && (severity_filter.is_empty() || p.severity.slug() == severity_filter)
                && (q.is_empty()
                    || p.title.to_lowercase().contains(&q)
                    || p.diagnosis.to_lowercase().contains(&q)
                    || p.route.to_lowercase().contains(&q))
        })
        .collect();

    topcoat::view::Child::new(view! {
        cx =>
        <section class="studio-counts" aria-label="Problem triage counts">
            <a class=(if open_count > 0 { "metric featured" } else { "metric" }) href=(format!("{base}/problems?status=open"))>
                <small>"Open / Needs reprocess"</small>
                <strong>(open_count)</strong>
            </a>
            <a class="metric" href=(format!("{base}/problems?status=in-progress"))>
                <small>"In progress / Fixing"</small>
                <strong>(progress_count)</strong>
            </a>
            <a class="metric" href=(format!("{base}/problems?status=resolved"))>
                <small>"Resolved · Intentional (" (intentional_count) ")"</small>
                <strong>(resolved_count)</strong>
            </a>
        </section>

        if role != Role::Reader {
            <details class="surface panel-padding design-editor" if !prefill_asset.is_empty() { open="" }>
                <summary>"Report or register a problem from captures"</summary>
                <form class="stack-form" method="post" action=(format!("{base}/problems/create"))>
                    <input type="hidden" name="csrf" value=(&account.csrf)>
                    <label for="prob-title">"Problem title"</label>
                    <input id="prob-title" name="title" required="" maxlength="200"
                        value=(if prefill_name.is_empty() { String::new() } else { format!("Visual issue on {prefill_name}") })
                        placeholder="e.g. Disconnected 14px gap between Settings cards">
                    <div class="form-row-3">
                        <div>
                            <label for="prob-route">"Route"</label>
                            <input id="prob-route" name="route" value="/settings" maxlength="160">
                        </div>
                        <div>
                            <label for="prob-vp">"Viewport"</label>
                            <input id="prob-vp" name="viewport" value="1440×900" maxlength="40">
                        </div>
                        <div>
                            <label for="prob-theme">"Theme"</label>
                            <select id="prob-theme" name="theme">
                                <option value="dark">"Dark"</option>
                                <option value="light">"Light"</option>
                                <option value="dark+light">"Dark + Light"</option>
                            </select>
                        </div>
                    </div>
                    <label for="prob-severity">"Severity"</label>
                    <select id="prob-severity" name="severity">
                        <option value="critical">"Critical (layout break, overflow, broken control)"</option>
                        <option value="major" selected="">"Major (hierarchy, target size, visual seam defect)"</option>
                        <option value="minor">"Minor (alignment, density, contrast)"</option>
                        <option value="polish">"Polish (micro-interaction, copy, rhythm)"</option>
                    </select>
                    <label for="prob-evidence">"Evidence asset from Captures"</label>
                    <select id="prob-evidence" name="evidence_url">
                        if !prefill_asset.is_empty() {
                            <option value=(prefill_asset) selected="">(prefill_name) " (" (prefill_asset) ")"</option>
                        }
                        <option value="">"Select captured screenshot or clip…"</option>
                        for asset in assets.iter().take(80) {
                            <option value=(&asset.url)>(asset.kind) " · " (&asset.name) " (" (&asset.scenario) ")"</option>
                        }
                    </select>
                    <label for="prob-global-rule">"Violated global norm"</label>
                    <select id="prob-global-rule" name="global_rule">
                        <option value="common.md#hairline-grid:Continuous 1px Hairline Structural Seams">"common.md#hairline-grid — Continuous 1px Hairline Structural Seams"</option>
                        <option value="common.md#target-size-24px:Minimum 24×24px Interactive Target Size">"common.md#target-size-24px — Minimum 24×24px Interactive Target Size"</option>
                        <option value="layout.md#no-horizontal-overflow:Zero Document Horizontal Overflow">"layout.md#no-horizontal-overflow — Zero Document Horizontal Overflow"</option>
                        <option value="layout.md#no-sibling-collision:Non-Overlapping Interactive Controls">"layout.md#no-sibling-collision — Non-Overlapping Interactive Controls"</option>
                        <option value="motion.md#hover-stability:Zero Layout Shift on Hover/Focus">"motion.md#hover-stability — Zero Layout Shift on Hover/Focus"</option>
                        <option value="motion.md#instant-tab-switch:Instant Tab & Dialog Transitions">"motion.md#instant-tab-switch — Instant Tab & Dialog Transitions"</option>
                        <option value="content.md#anti-verbosity:Zero Decorative Boilerplate Microcopy">"content.md#anti-verbosity — Zero Decorative Boilerplate Microcopy"</option>
                    </select>
                    <label for="prob-project-rule">"Violated project design norm"</label>
                    <select id="prob-project-rule" name="project_rule">
                        <option value="Design#flyio-copper-broadsheet:Fly.io Copper Broadsheet Direction">"Design#flyio-copper-broadsheet — Fly.io Copper Broadsheet Direction"</option>
                        for d in design_entries.iter().take(12) {
                            <option value=(format!("Design#{}:{}", d.id, d.title))>(d.kind.label()) " · " (&d.title)</option>
                        }
                    </select>
                    <label for="prob-diagnosis">"Diagnosis (what is broken & why)"</label>
                    <textarea id="prob-diagnosis" name="diagnosis" required="" rows="3" maxlength="2000" placeholder="Root cause observed in the capture and DOM/CSS…"></textarea>
                    <label for="prob-solution">"Proposed solution"</label>
                    <textarea id="prob-solution" name="solution" required="" rows="3" maxlength="2000" placeholder="Concrete CSS/HTML/state fix to apply…"></textarea>
                    <button class="button primary" type="submit">"Add problem to queue"</button>
                </form>
            </details>
        }

        <div class="propose-toolbar">
            <p class="muted">
                (filtered.len()) " problems in queue · "
                if mcp_online {
                    <strong>"MCP Agent Online (ready to dispatch fixes)"</strong>
                } else {
                    "MCP offline · triage states persist for CLI & next agent session"
                }
            </p>
            <form class="filters" method="get" action=(format!("{base}/problems"))>
                <label for="p-status">"Status"</label>
                <select id="p-status" name="status">
                    <option value="">"All statuses"</option>
                    for st in ProblemStatus::ALL {
                        <option value=(st.slug()) if status_filter == st.slug() { selected="" }>(st.label())</option>
                    }
                </select>
                <label for="p-sev">"Severity"</label>
                <select id="p-sev" name="kind">
                    <option value="">"All severities"</option>
                    for sev in [Severity::Critical, Severity::Major, Severity::Minor, Severity::Polish] {
                        <option value=(sev.slug()) if severity_filter == sev.slug() { selected="" }>(sev.label())</option>
                    }
                </select>
                <button class="button" type="submit">"Filter"</button>
            </form>
        </div>

        if filtered.is_empty() {
            <section class="empty">
                <h2>"No problems match this filter"</h2>
                <p>"Extract problems from a research capture run in Captures or report a visual issue above."</p>
            </section>
        } else {
            <div class="problem-queue" data-problems-live=(format!("{base}/problems/status.json"))>
                for item in &filtered {
                    <article class="surface problem-card" id=(format!("problem-{}", item.id)) data-problem-card=(&item.id)>
                        <header class="propose-head">
                            <div>
                                <div class="propose-badges">
                                    badge(variant: severity_badge(item.severity), (item.severity.label()))
                                    badge(variant: problem_status_badge(item.status), (item.status.label()))
                                    <span class="tactic-chip">(&item.route) " · " (&item.viewport) " · " (&item.theme)</span>
                                    <span class="tactic-chip">(item.origin.label())</span>
                                </div>
                                <h2>(&item.title)</h2>
                            </div>
                            <time data-epoch-ms=(item.created_at_ms.to_string())></time>
                        </header>

                        if item.execution.is_some() || matches!(item.status, ProblemStatus::InProgress | ProblemStatus::Resolved) {
                            <section class="task-tracker" data-problem-tracker=(&item.id)
                                data-tracker-active=(if item.status == ProblemStatus::InProgress { "true" } else { "false" })>
                                if let Some(exec) = &item.execution {
                                    <div class="tracker-head">
                                        <div class="propose-badges">
                                            <span class=" studio-kind">"AGENT:"</span>
                                            <strong class="tracker-agent" data-tracker-agent="">
                                                (exec.agent_id.as_deref().unwrap_or("Unassigned · Waiting for agent to claim"))
                                            </strong>
                                            <span class="tactic-chip" data-tracker-stage="">
                                                ("STAGE: ") (exec.stage.to_uppercase())
                                            </span>
                                        </div>
                                        <span class="tracker-metrics" data-tracker-metrics="">
                                            ("STEP ") (exec.step_current) (" / ") (exec.step_total) (" · ") (exec.progress_pct) ("%")
                                        </span>
                                    </div>
                                    <progress class="task-progress-bar" max="100" value=(exec.progress_pct.to_string()) data-tracker-bar=""></progress>
                                    <p class="tracker-activity" data-tracker-activity="">(&exec.current_activity)</p>
                                    if exec.agent_id.is_none() && item.status == ProblemStatus::InProgress {
                                        <p class="muted tracker-hint">
                                            "Claim via CLI: "
                                            <code>(format!("rupicola problem claim {} --agent <agent-name>", item.id))</code>
                                            " or MCP tool "
                                            <code>"problem_progress"</code>
                                        </p>
                                    }
                                    if !exec.logs.is_empty() {
                                        <details class="tracker-logs">
                                            <summary>"Task execution log (" (exec.logs.len()) " events)"</summary>
                                            <ol class="steps-list" data-tracker-logs="">
                                                for log in &exec.logs {
                                                    <li>
                                                        <div class="step-head">
                                                            <span class="step-num">(format!("{}%", log.progress_pct))</span>
                                                            <span class="tactic-chip">(&log.stage)</span>
                                                            <span>(&log.detail)</span>
                                                            <time data-epoch-ms=(log.timestamp_ms.to_string())></time>
                                                        </div>
                                                    </li>
                                                }
                                            </ol>
                                        </details>
                                    }
                                } else {
                                    <div class="tracker-head">
                                        <div class="propose-badges">
                                            <span class="studio-kind">"AGENT:"</span>
                                            <strong class="tracker-agent" data-tracker-agent="">
                                                (if item.status == ProblemStatus::Resolved { "rupicola-agent" } else { "Unassigned · Waiting for agent to claim" })
                                            </strong>
                                            <span class="tactic-chip" data-tracker-stage="">
                                                (if item.status == ProblemStatus::Resolved { "STAGE: COMPLETED" } else { "STAGE: QUEUED" })
                                            </span>
                                        </div>
                                        <span class="tracker-metrics" data-tracker-metrics="">
                                            (if item.status == ProblemStatus::Resolved { "STEP 4 / 4 · 100%" } else { "STEP 0 / 4 · 5%" })
                                        </span>
                                    </div>
                                    <progress class="task-progress-bar" max="100" value=(if item.status == ProblemStatus::Resolved { "100" } else { "5" }) data-tracker-bar=""></progress>
                                    <p class="tracker-activity" data-tracker-activity="">
                                        (if item.status == ProblemStatus::Resolved {
                                            "Fix verified and linked in Fixes."
                                        } else {
                                            "Queued for fix — waiting for an agent to claim task via MCP (problem_progress) or CLI (rupicola problem claim)."
                                        })
                                    </p>
                                }
                            </section>
                        }

                        if !item.evidence_assets.is_empty() {
                            <div class="problem-evidence-strip">
                                for ev in &item.evidence_assets {
                                    <a class="problem-thumb" href=(&ev.url) data-viewer=(&item.title)>
                                        <img src=(&ev.url) alt=(&ev.label) loading="lazy">
                                        <span>(&ev.label) " ↗"</span>
                                    </a>
                                }
                            </div>
                        }

                        <div class="problem-rules">
                            for violation in &item.violated_rules {
                                <span class=(if violation.scope == "project" { "rule-chip project-rule" } else { "rule-chip global-rule" })>
                                    <strong>(if violation.scope == "project" { "Project Norm" } else { "Global Norm" })</strong>
                                    " · " (&violation.code) " — " (&violation.title)
                                </span>
                            }
                        </div>

                        <div class="problem-analysis-grid">
                            <div class="problem-box">
                                <small class="studio-kind">"DIAGNOSIS"</small>
                                <p>(&item.diagnosis)</p>
                            </div>
                            <div class="problem-box solution-box">
                                <small class="studio-kind">"PROPOSED SOLUTION"</small>
                                <p>(&item.proposed_solution)</p>
                            </div>
                        </div>

                        if !item.comments.is_empty() {
                            <ul class="problem-comments">
                                for c in &item.comments {
                                    <li>
                                        <strong>(&c.author)</strong> ": " (&c.text)
                                        " " <time data-epoch-ms=(c.created_at_ms.to_string())></time>
                                    </li>
                                }
                            </ul>
                        }

                        <footer class="propose-foot">
                            <div class="review-actions">
                                if let Some(fid) = &item.linked_fix_id {
                                    <a class="button primary" href=(format!("{base}/fixes#fix-{fid}"))>"Compare Before/After Fix ↗"</a>
                                }
                                if role != Role::Reader {
                                    <form method="post" action=(format!("{base}/problems/{}/triage", item.id)) class="review-actions">
                                        <input type="hidden" name="csrf" value=(&account.csrf)>
                                        <button class="button primary" type="submit" name="status" value="in-progress">
                                            (if mcp_online { "Send to agent to fix" } else { "Queue for fix" })
                                        </button>
                                        <button class="button" type="submit" name="status" value="intentional" title="Never flag this route/rule again">
                                            "Mark Intentional"
                                        </button>
                                        <button class="button" type="submit" name="status" value="false-positive">
                                            "False positive"
                                        </button>
                                        <button class="button" type="submit" name="status" value="needs-reprocess">
                                            "Reprocess"
                                        </button>
                                    </form>
                                }
                            </div>
                            if role != Role::Reader {
                                <div class="review-actions">
                                    <details class="triage-note-drawer">
                                        <summary>"Assign agent / update task progress"</summary>
                                        <form method="post" action=(format!("{base}/problems/{}/progress", item.id)) class="stack-form">
                                            <input type="hidden" name="csrf" value=(&account.csrf)>
                                            <label for=(format!("ag-{}", item.id))>"Agent identifier"</label>
                                            <input id=(format!("ag-{}", item.id)) name="agent" maxlength="120"
                                                value=(item.execution.as_ref().and_then(|e| e.agent_id.as_deref()).unwrap_or(if mcp_online { "rupicola-mcp-agent" } else { "codex-agent" }))>
                                            <div class="form-row-3">
                                                <div>
                                                    <label for=(format!("stg-{}", item.id))>"Stage"</label>
                                                    <select id=(format!("stg-{}", item.id)) name="stage">
                                                        <option value="claimed">"1 · Claimed (25%)"</option>
                                                        <option value="fixing" selected="">"2 · Fixing code (50%)"</option>
                                                        <option value="verifying">"3 · Verifying in Playwright (75%)"</option>
                                                        <option value="completed">"4 · Completed (100%)"</option>
                                                    </select>
                                                </div>
                                                <div>
                                                    <label for=(format!("step-{}", item.id))>"Step (of 4)"</label>
                                                    <input id=(format!("step-{}", item.id)) name="step" type="number" min="1" max="10" value="2">
                                                </div>
                                                <div>
                                                    <label for=(format!("pct-{}", item.id))>"Progress %"</label>
                                                    <input id=(format!("pct-{}", item.id)) name="pct" type="number" min="0" max="100" value="50">
                                                </div>
                                            </div>
                                            <label for=(format!("act-{}", item.id))>"Current task activity"</label>
                                            <input id=(format!("act-{}", item.id)) name="activity" required="" maxlength="400" placeholder="e.g. Applying 1px hairline seam fix in panel.css…">
                                            <button class="button primary" type="submit">"Update progress"</button>
                                        </form>
                                    </details>
                                    <details class="triage-note-drawer">
                                        <summary>"Refine solution / add triage note"</summary>
                                        <form method="post" action=(format!("{base}/problems/{}/triage", item.id)) class="stack-form">
                                            <input type="hidden" name="csrf" value=(&account.csrf)>
                                            <input type="hidden" name="status" value="needs-reprocess">
                                            <label for=(format!("sol-{}", item.id))>"Updated solution proposal"</label>
                                            <textarea id=(format!("sol-{}", item.id)) name="solution" rows="2" maxlength="2000">(&item.proposed_solution)</textarea>
                                            <label for=(format!("com-{}", item.id))>"Triage note / guidance for agent"</label>
                                            <input id=(format!("com-{}", item.id)) name="comment" maxlength="1000" placeholder="Clarify intent or constraints…">
                                            <button class="button" type="submit">"Save & flag for reprocess"</button>
                                        </form>
                                    </details>
                                </div>
                            }
                        </footer>
                    </article>
                }
            </div>
        }

        <dialog id="image-viewer" aria-labelledby="image-title">
            <div class="section-head">
                <h2 id="image-title">"Evidence"</h2>
                <form method="dialog"><button class="button" type="submit" autofocus="">"Close"</button></form>
            </div>
            <img alt="">
            <div class="section-head">
                <div class="filters">
                    <button type="button" class="button" data-image-prev="">"Previous"</button>
                    <button type="button" class="button" data-image-next="">"Next"</button>
                </div>
                <a data-image-original="" href=".">"Open original"</a>
            </div>
        </dialog>
    })
}

#[derive(Deserialize)]
pub struct CreateProblemForm {
    pub csrf: String,
    pub title: String,
    pub route: String,
    pub viewport: String,
    pub theme: String,
    pub severity: String,
    pub evidence_url: String,
    pub global_rule: String,
    pub project_rule: String,
    pub diagnosis: String,
    pub solution: String,
}

pub async fn create_problem_action(
    panel: Arc<Panel>,
    account: Account,
    project_id: String,
    form: CreateProblemForm,
) -> PageResult<Response> {
    auth::check_csrf(&account, &form.csrf)?;
    if auth::role(&panel, &account, &project_id).await? == Role::Reader {
        return Err(StatusCode::FORBIDDEN);
    }
    let title = form.title.trim();
    let diagnosis = form.diagnosis.trim();
    let solution = form.solution.trim();
    if title.is_empty() || diagnosis.is_empty() || solution.is_empty() {
        return Err(StatusCode::UNPROCESSABLE_ENTITY);
    }
    let (id, now) = problem_record::next_id();
    let severity = Severity::from_slug(form.severity.trim()).unwrap_or(Severity::Major);
    let mut violated_rules = Vec::new();
    for (scope, raw) in [
        ("global", &form.global_rule),
        ("project", &form.project_rule),
    ] {
        if !raw.trim().is_empty() {
            let (code, rtitle) = raw.split_once(':').unwrap_or((raw.as_str(), raw.as_str()));
            violated_rules.push(ViolatedRule {
                scope: scope.into(),
                code: code.trim().into(),
                title: rtitle.trim().into(),
            });
        }
    }
    let mut evidence_assets = Vec::new();
    if !form.evidence_url.trim().is_empty() {
        evidence_assets.push(EvidenceAsset {
            run_key: String::new(),
            asset_path: form.evidence_url.trim().into(),
            url: form.evidence_url.trim().into(),
            kind: "Screenshot".into(),
            label: format!(
                "{} · {} · {}",
                form.route.trim(),
                form.viewport.trim(),
                form.theme.trim()
            ),
        });
    }
    let problem = Problem {
        id,
        project_id: project_id.clone(),
        title: title.into(),
        status: ProblemStatus::Open,
        severity,
        origin: ProblemOrigin::UserReported,
        route: form.route.trim().into(),
        viewport: form.viewport.trim().into(),
        theme: form.theme.trim().into(),
        diagnosis: diagnosis.into(),
        proposed_solution: solution.into(),
        violated_rules,
        evidence_assets,
        comments: Vec::new(),
        linked_fix_id: None,
        execution: None,
        author: account.handle,
        created_at_ms: now,
        updated_at_ms: now,
    };
    problem_record::save(&data_root(&panel), &problem).map_err(db_error)?;
    Ok(redirect(&format!("/projects/{project_id}/problems")))
}

#[derive(Deserialize)]
pub struct TriageProblemForm {
    #[serde(default)]
    pub csrf: String,
    pub status: String,
    pub comment: Option<String>,
    pub solution: Option<String>,
}

pub async fn triage_problem_action(
    panel: Arc<Panel>,
    account: Account,
    (project_id, problem_id): (String, String),
    form: TriageProblemForm,
) -> PageResult<Response> {
    auth::check_csrf(&account, &form.csrf)?;
    if auth::role(&panel, &account, &project_id).await? == Role::Reader {
        return Err(StatusCode::FORBIDDEN);
    }
    triage_problem(&panel, &account.handle, &project_id, &problem_id, form)?;
    Ok(redirect(&format!(
        "/projects/{project_id}/problems#problem-{problem_id}"
    )))
}

pub(crate) fn triage_problem(
    panel: &Panel,
    actor: &str,
    project_id: &str,
    problem_id: &str,
    form: TriageProblemForm,
) -> PageResult<()> {
    let data = data_root(panel);
    let mut problem =
        problem_record::load(&data, project_id, problem_id).map_err(|_| StatusCode::NOT_FOUND)?;
    let Some(status) = ProblemStatus::from_slug(form.status.trim()) else {
        return Err(StatusCode::UNPROCESSABLE_ENTITY);
    };
    let (_, now) = problem_record::next_id();
    problem.status = status;
    problem.updated_at_ms = now;
    if let Some(sol) = form
        .solution
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
    {
        problem.proposed_solution = sol;
    }
    if let Some(comment) = form
        .comment
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
    {
        problem.comments.push(ProblemComment {
            author: actor.to_owned(),
            text: comment,
            created_at_ms: now,
        });
    }
    if status == ProblemStatus::InProgress {
        let mcp_session = proposal_record::mcp_session_active(&data);
        let (agent_id, stage, pct, step_cur, msg) = if let Some(sess) = mcp_session {
            let client = sess.client.unwrap_or_else(|| "rupicola-mcp-agent".into());
            let label = format!("{client} (PID {})", sess.pid);
            (
                Some(label.clone()),
                "dispatched".to_string(),
                15,
                1,
                format!(
                    "Dispatched to active MCP agent {label} — Step 1/4: Inspecting problem evidence & rules"
                ),
            )
        } else {
            (
                None,
                "queued".to_string(),
                5,
                0,
                format!(
                    "Queued by {} — waiting for an agent to claim (MCP problem_progress or CLI: rupicola problem claim {} --agent <name>)",
                    actor, problem.id
                ),
            )
        };
        let mut exec = problem.execution.take().unwrap_or(TaskExecution {
            agent_id: agent_id.clone(),
            stage: stage.clone(),
            progress_pct: pct,
            step_current: step_cur,
            step_total: 4,
            current_activity: msg.clone(),
            logs: Vec::new(),
            started_at_ms: now,
            updated_at_ms: now,
        });
        if agent_id.is_some() {
            exec.agent_id = agent_id;
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
        problem.execution = Some(exec);
    }
    problem_record::save(&data, &problem).map_err(db_error)?;
    Ok(())
}

#[derive(Deserialize)]
pub struct ProgressProblemForm {
    #[serde(default)]
    pub csrf: String,
    pub agent: String,
    pub stage: String,
    pub step: usize,
    pub pct: u8,
    pub activity: String,
}

pub async fn progress_problem_action(
    panel: Arc<Panel>,
    account: Account,
    (project_id, problem_id): (String, String),
    form: ProgressProblemForm,
) -> PageResult<Response> {
    auth::check_csrf(&account, &form.csrf)?;
    if auth::role(&panel, &account, &project_id).await? == Role::Reader {
        return Err(StatusCode::FORBIDDEN);
    }
    progress_problem(&panel, &project_id, &problem_id, &form)?;
    Ok(redirect(&format!(
        "/projects/{project_id}/problems#problem-{problem_id}"
    )))
}

pub(crate) fn progress_problem(
    panel: &Panel,
    project_id: &str,
    problem_id: &str,
    form: &ProgressProblemForm,
) -> PageResult<()> {
    let data = data_root(panel);
    let mut problem =
        problem_record::load(&data, project_id, problem_id).map_err(|_| StatusCode::NOT_FOUND)?;
    let (_, now) = problem_record::next_id();
    let pct = form.pct.min(100);
    let stage = form.stage.trim().to_string();
    let agent = form.agent.trim().to_string();
    let activity = form.activity.trim().to_string();
    if activity.is_empty() {
        return Err(StatusCode::UNPROCESSABLE_ENTITY);
    }
    problem.status = if pct >= 100 || stage == "completed" {
        ProblemStatus::Resolved
    } else {
        ProblemStatus::InProgress
    };
    problem.updated_at_ms = now;
    let mut exec = problem.execution.take().unwrap_or(TaskExecution {
        agent_id: (!agent.is_empty()).then(|| agent.clone()),
        stage: stage.clone(),
        progress_pct: pct,
        step_current: form.step.clamp(1, 10),
        step_total: 4,
        current_activity: activity.clone(),
        logs: Vec::new(),
        started_at_ms: now,
        updated_at_ms: now,
    });
    if !agent.is_empty() {
        exec.agent_id = Some(agent.clone());
    }
    exec.stage.clone_from(&stage);
    exec.progress_pct = pct;
    exec.step_current = form.step.clamp(1, exec.step_total.max(1));
    exec.current_activity.clone_from(&activity);
    exec.updated_at_ms = now;
    exec.logs.push(TaskStepLog {
        stage,
        progress_pct: pct,
        detail: if agent.is_empty() {
            activity
        } else {
            format!("[{agent}] {activity}")
        },
        timestamp_ms: now,
    });
    problem.execution = Some(exec);
    problem_record::save(&data, &problem).map_err(db_error)?;
    Ok(())
}

pub async fn problems_status_json(
    panel: Arc<Panel>,
    account: Account,
    project_id: String,
) -> PageResult<Response> {
    let _ = auth::role(&panel, &account, &project_id).await?;
    let data = data_root(&panel);
    let problems = problem_record::entries(&data, &project_id).unwrap_or_default();
    let payload = serde_json::json!({
        "project": project_id,
        "problems": problems,
    });
    let bytes = serde_json::to_vec(&payload).map_err(db_error)?;
    Ok(Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "application/json; charset=utf-8")
        .body(Body::from(bytes))
        .unwrap())
}
