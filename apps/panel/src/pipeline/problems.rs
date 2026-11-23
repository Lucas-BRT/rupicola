use super::{
    Account, Arc, AssetOption, Cx, Deserialize, PageResult, Panel, Problem, ProblemStatus,
    Response, Role, Severity, StatusCode, auth, badge, design_record, pages, problem_status_badge,
    redirect, severity_badge, view,
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
                "triage states persist for CLI & agent sessions"
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
            <div class="problem-queue">
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
                                            "Queue for fix"
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
    let severity = form
        .severity
        .trim()
        .parse()
        .unwrap_or(rupicola_api::Severity::Major);
    let mut rules = Vec::new();
    for (scope, raw) in [
        ("global", &form.global_rule),
        ("project", &form.project_rule),
    ] {
        if !raw.trim().is_empty() {
            let (code, rule_title) = raw.split_once(':').unwrap_or((raw.as_str(), raw.as_str()));
            rules.push(rupicola_api::Rule {
                scope: scope.into(),
                code: code.trim().into(),
                title: rule_title.trim().into(),
            });
        }
    }
    let evidence: Vec<String> = Some(form.evidence_url.trim())
        .filter(|url| !url.is_empty())
        .map(|url| crate::records::artifact_of(url).to_owned())
        .into_iter()
        .collect();
    let new = rupicola_api::NewProblem {
        title: title.into(),
        severity,
        origin: rupicola_api::ProblemOrigin::UserReported,
        route: form.route.trim().into(),
        viewport: form.viewport.trim().into(),
        theme: form.theme.trim().into(),
        diagnosis: diagnosis.into(),
        proposed_solution: solution.into(),
        rules,
        evidence,
    };
    panel
        .api
        .create_problem(&account.session, &project_id, &new)
        .await
        .map_err(|e| auth::api_error(&e))?;
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
    let status = form
        .status
        .trim()
        .parse()
        .map_err(|_| StatusCode::UNPROCESSABLE_ENTITY)?;
    let nonblank = |text: Option<String>| {
        text.map(|text| text.trim().to_owned())
            .filter(|text| !text.is_empty())
    };
    let patch = rupicola_api::ProblemPatch {
        status: Some(status),
        severity: None,
        proposed_solution: nonblank(form.solution),
    };
    panel
        .api
        .update_problem(&account.session, &problem_id, &patch)
        .await
        .map_err(|e| auth::api_error(&e))?;
    if let Some(text) = nonblank(form.comment) {
        panel
            .api
            .comment_problem(
                &account.session,
                &problem_id,
                &rupicola_api::NewComment { text },
            )
            .await
            .map_err(|e| auth::api_error(&e))?;
    }
    Ok(redirect(&format!(
        "/projects/{project_id}/problems#problem-{problem_id}"
    )))
}
