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
            let Some(manifest) = pages::read_run(panel, &run.key) else {
                continue;
            };
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

#[allow(clippy::too_many_arguments)]
#[expect(
    clippy::too_many_lines,
    reason = "the captures view keeps its markup and presentation branches together"
)]
pub fn render_captures<'a>(
    cx: &'a Cx,
    panel: &'a Panel,
    account: &'a Account,
    role: Role,
    _project_id: &'a str,
    base: &'a str,
    filter: &'a pages::Filter,
    runs: &'a [&'a Run],
    assets: &'a [AssetOption],
    problems: &'a [Problem],
) -> topcoat::view::Child<'a> {
    let subview = filter.view.as_deref().unwrap_or("all");
    let health = filter.health.as_deref().unwrap_or("");
    let q = filter.q.as_deref().unwrap_or("").trim().to_lowercase();

    let problem_urls: std::collections::BTreeSet<&str> = problems
        .iter()
        .filter(|p| {
            !matches!(
                p.status,
                ProblemStatus::Intentional | ProblemStatus::FalsePositive
            )
        })
        .flat_map(|p| p.evidence_assets.iter().map(|a| a.url.as_str()))
        .collect();
    let problem_runs: std::collections::BTreeSet<&str> = problems
        .iter()
        .filter(|p| {
            !matches!(
                p.status,
                ProblemStatus::Intentional | ProblemStatus::FalsePositive
            )
        })
        .flat_map(|p| p.evidence_assets.iter().map(|a| a.run_key.as_str()))
        .collect();

    let filtered_runs: Vec<&Run> = runs
        .iter()
        .copied()
        .filter(|r| {
            let has_prob = problem_runs.contains(r.key.as_str()) || r.status == "blocked";
            let matches_health = match health {
                "problems" => has_prob,
                "clean" => !has_prob && r.inspection == "reviewed",
                "unanalyzed" => r.inspection == "pending",
                _ => true,
            };
            matches_health && filter.accepts(r)
        })
        .collect();

    let filtered_assets: Vec<&AssetOption> = assets
        .iter()
        .filter(|a| {
            let has_prob =
                problem_urls.contains(a.url.as_str()) || problem_runs.contains(a.run_key.as_str());
            let matches_health = match health {
                "problems" => has_prob,
                "clean" => !has_prob,
                _ => true,
            };
            let matches_view = match subview {
                "images" => a.kind != "Recording",
                "videos" => a.kind == "Recording",
                _ => true,
            };
            let matches_q = q.is_empty()
                || a.name.to_lowercase().contains(&q)
                || a.scenario.to_lowercase().contains(&q);
            matches_health && matches_view && matches_q
        })
        .collect();

    let open_problems = problems
        .iter()
        .filter(|p| {
            matches!(
                p.status,
                ProblemStatus::Open | ProblemStatus::NeedsReprocess
            )
        })
        .count();

    topcoat::view::Child::new(view! {
        cx =>
        <section class="studio-counts" aria-label="Research summary">
            <div class="metric">
                <small>"Capture runs"</small>
                <strong>(runs.len())</strong>
            </div>
            <div class="metric">
                <small>"Visual assets (shots & clips)"</small>
                <strong>(assets.len())</strong>
            </div>
            <a class=(if open_problems > 0 { "metric featured" } else { "metric" }) href=(format!("{base}/problems"))>
                <small>"Problems identified"</small>
                <strong>(open_problems)</strong>
                <span aria-hidden="true">"↗"</span>
            </a>
        </section>

        if role == Role::Owner {
            <details class="surface panel-padding design-editor">
                <summary>"Run inspection matrix (capture & analysis)"</summary>
                <form class="stack-form" method="post" action=(format!("{base}/probe"))>
                    <input type="hidden" name="csrf" value=(&account.csrf)>
                    <label for="probe-url">"Target URL"</label>
                    <input id="probe-url" name="url" type="url" required="" value=(&panel.origin) maxlength="512">
                    <label for="probe-mode">"Inspection mode"</label>
                    <select id="probe-mode" name="mode">
                        <option value="page-check">"Page check (1/N pages · 1 viewport · D1)"</option>
                        <option value="all-pages">"All pages (auto-crawl · 1 viewport · D1)"</option>
                        <option value="responsive-matrix" selected="">"Responsive matrix (pages × N viewports × themes · D2)"</option>
                        <option value="interaction-map">"Interaction map (selected pages × N viewports · D3)"</option>
                        <option value="exhaustive">"Exhaustive (all pages × N viewports × full interaction map · D3)"</option>
                    </select>
                    <label for="probe-pages">"Pages (comma-separated routes or 'all')"</label>
                    <input id="probe-pages" name="pages" value="all" maxlength="400">
                    <label for="probe-viewports">"Viewports (comma-separated WxH or mobile,tablet,desktop)"</label>
                    <input id="probe-viewports" name="viewports" value="320x844,768x1024,1440x900" maxlength="200">
                    <label for="probe-themes">"Themes"</label>
                    <select id="probe-themes" name="themes">
                        <option value="dark,light" selected="">"Dark + Light"</option>
                        <option value="dark">"Dark only"</option>
                        <option value="light">"Light only"</option>
                    </select>
                    <label for="probe-pipeline">"Pipeline stage behavior"</label>
                    <select id="probe-pipeline" name="pipeline">
                        <option value="capture-and-analyze" selected="">"Capture + populate Problems queue automatically"</option>
                        <option value="capture-only">"Capture assets only (analyze separately later)"</option>
                    </select>
                    <button class="button primary" type="submit">"Start matrix capture"</button>
                </form>
            </details>
        }

        <div class="propose-toolbar">
            <div class="stage-viewports" role="group" aria-label="Capture view">
                for (slug, label) in [("all", "All research"), ("runs", "Matrix runs"), ("images", "Screenshots & sheets"), ("videos", "Interaction clips")] {
                    <a class=(if subview == slug { "button primary" } else { "button" })
                       href=(format!("{base}/captures?view={slug}&health={health}&q={}", filter.q.as_deref().unwrap_or("")))>
                        (label)
                    </a>
                }
            </div>
            <form class="filters" method="get" action=(format!("{base}/captures"))>
                <input type="hidden" name="view" value=(subview)>
                <label for="cap-health">"Screen health"</label>
                <select id="cap-health" name="health">
                    for (val, text) in [("", "All captured screens"), ("clean", "Clean / verified only"), ("problems", "With problems only"), ("unanalyzed", "Unanalyzed")] {
                        <option value=(val) if health == val { selected="" }>(text)</option>
                    }
                </select>
                <label for="cap-q">"Search"</label>
                <input id="cap-q" name="q" type="search" maxlength="100" value=(filter.q.as_deref().unwrap_or("")) placeholder="Route, viewport, scenario…">
                <button class="button" type="submit">"Filter"</button>
            </form>
        </div>

        if subview == "all" || subview == "runs" {
            <section class="surface captures-ledger">
                <div class="section-head">
                    <h2>"Research runs (" (filtered_runs.len()) ")"</h2>
                </div>
                if filtered_runs.is_empty() {
                    <div class="empty"><p>"No capture runs match this filter."</p></div>
                } else {
                    <ul class="run-list">
                        for run in &filtered_runs {
                            <li>
                                <div class="run-row">
                                    <a class="run-link" href=(format!("/run/{}", run.key))>
                                        <span class="run-title">
                                            <strong>(&run.scenario)</strong>
                                            <small>(&run.project) " · " (&run.key)</small>
                                        </span>
                                        <span class="run-state">
                                            badge(
                                                variant: if problem_runs.contains(run.key.as_str()) || run.status == "blocked" {
                                                    BadgeVariant::Destructive
                                                } else {
                                                    BadgeVariant::Secondary
                                                },
                                                (if run.status == "blocked" {
                                                    "Blocked"
                                                } else if problem_runs.contains(run.key.as_str()) {
                                                    "Has problems"
                                                } else if run.inspection == "reviewed" {
                                                    "Clean · Verified"
                                                } else {
                                                    "Unanalyzed"
                                                })
                                            )
                                            <time data-epoch-ms=(run.created_at_ms.to_string())></time>
                                        </span>
                                        <span class="arrow" aria-hidden="true">"↗"</span>
                                    </a>
                                    if role != Role::Reader {
                                        <form method="post" action=(format!("{base}/problems/analyze")) class="inline-action">
                                            <input type="hidden" name="csrf" value=(&account.csrf)>
                                            <input type="hidden" name="run_key" value=(&run.key)>
                                            <button class="button" type="submit">"Extract problems"</button>
                                        </form>
                                    }
                                </div>
                            </li>
                        }
                    </ul>
                }
            </section>
        }

        if subview != "runs" {
            <section class="captures-ledger">
                <div class="section-head">
                    <h2>"Captured visual assets (" (filtered_assets.len()) ")"</h2>
                </div>
                if filtered_assets.is_empty() {
                    <div class="empty"><p>"No visual assets match this filter."</p></div>
                } else {
                    <div class="gallery image-gallery">
                        for (index, item) in filtered_assets.iter().take(120).enumerate() {
                            <article class="asset">
                                if item.kind != "Recording" {
                                    <a class="asset-preview" href=(&item.url) data-viewer=(&item.name)>
                                        <img src=(&item.url) alt=(format!("{} · {}", item.name, item.scenario))
                                            if index >= 4 { loading="lazy" }>
                                    </a>
                                } else {
                                    <a class="asset-preview video-preview" href=(format!("/run/{}", item.run_key)) aria-label=(format!("Open recording {}", item.name))>
                                        <span aria-hidden="true">"▶"</span>
                                    </a>
                                }
                                <div class="asset-caption">
                                    <div class="propose-badges">
                                        <small>(item.kind)</small>
                                        if problem_urls.contains(item.url.as_str()) {
                                            badge(variant: BadgeVariant::Destructive, "Problem linked")
                                        } else {
                                            badge(variant: BadgeVariant::Secondary, "Clean")
                                        }
                                    </div>
                                    <strong>(&item.name)</strong>
                                    <div class="propose-foot">
                                        <a href=(format!("/run/{}", item.run_key))>(&item.scenario)</a>
                                        if role != Role::Reader {
                                            <a class="link-button" href=(format!("{base}/problems?asset={}&name={}", item.url, item.name))>"Flag problem"</a>
                                        }
                                    </div>
                                </div>
                            </article>
                        }
                    </div>
                }
            </section>
        }

        if !filtered_assets.is_empty() {
            <dialog id="image-viewer" aria-labelledby="image-title">
                <div class="section-head">
                    <h2 id="image-title">"Image"</h2>
                    <form method="dialog"><button class="button" type="submit" autofocus="">"Close"</button></form>
                </div>
                <img alt="">
                <div class="section-head">
                    <div class="filters">
                        <button type="button" class="button" data-image-prev="">"Previous"</button>
                        <button type="button" class="button" data-image-next="">"Next"</button>
                    </div>
                    <div class="filters">
                        <a data-image-original="" href=".">"Open original"</a>
                    </div>
                </div>
            </dialog>
        }
    })
}

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

#[allow(clippy::too_many_arguments)]
#[expect(
    clippy::too_many_lines,
    reason = "the fixes view keeps its markup and presentation branches together"
)]
pub fn render_fixes<'a>(
    cx: &'a Cx,
    account: &'a Account,
    role: Role,
    _project_id: &'a str,
    base: &'a str,
    filter: &'a pages::Filter,
    fixes: &'a [FixRecord],
    problems: &'a [Problem],
    assets: &'a [AssetOption],
    mcp_online: bool,
) -> topcoat::view::Child<'a> {
    let status_filter = filter.status.as_deref().unwrap_or("");
    let pending_count = fixes
        .iter()
        .filter(|f| f.status == FixStatus::PendingReview)
        .count();
    let approved_count = fixes
        .iter()
        .filter(|f| f.status == FixStatus::Approved)
        .count();
    let changes_count = fixes
        .iter()
        .filter(|f| f.status == FixStatus::ChangesRequested)
        .count();

    let filtered: Vec<&FixRecord> = fixes
        .iter()
        .filter(|f| status_filter.is_empty() || f.status.slug() == status_filter)
        .collect();

    topcoat::view::Child::new(view! {
        cx =>
        <section class="studio-counts" aria-label="Fixes overview">
            <a class=(if pending_count > 0 { "metric featured" } else { "metric" }) href=(format!("{base}/fixes?status=pending-review"))>
                <small>"Pending approval"</small>
                <strong>(pending_count)</strong>
            </a>
            <a class="metric" href=(format!("{base}/fixes?status=approved"))>
                <small>"Approved fixes"</small>
                <strong>(approved_count)</strong>
            </a>
            <a class="metric" href=(format!("{base}/fixes?status=changes-requested"))>
                <small>"Changes requested"</small>
                <strong>(changes_count)</strong>
            </a>
        </section>

        if role != Role::Reader {
            <details class="surface panel-padding design-editor">
                <summary>"Record a new Before/After Fix comparison"</summary>
                <form class="stack-form" method="post" action=(format!("{base}/fixes/create"))>
                    <input type="hidden" name="csrf" value=(&account.csrf)>
                    <label for="fix-title">"Fix title"</label>
                    <input id="fix-title" name="title" required="" maxlength="200" placeholder="e.g. Continuous 1px hairline seams in Settings & Project Settings">
                    <label for="fix-problem">"Resolves Problem"</label>
                    <select id="fix-problem" name="problem_id">
                        <option value="">"Standalone fix"</option>
                        for p in problems {
                            <option value=(&p.id)>(&p.title) " (" (&p.route) " · " (p.status.label()) ")"</option>
                        }
                    </select>
                    <div class="form-row-3">
                        <div>
                            <label for="fix-route">"Route"</label>
                            <input id="fix-route" name="route" value="/settings" maxlength="160">
                        </div>
                        <div>
                            <label for="fix-vp">"Viewport"</label>
                            <input id="fix-vp" name="viewport" value="1440×900" maxlength="40">
                        </div>
                        <div>
                            <label for="fix-theme">"Theme"</label>
                            <input id="fix-theme" name="theme" value="dark" maxlength="30">
                        </div>
                    </div>
                    <label for="fix-before">"Before capture (initial bug/problem — image or video)"</label>
                    <select id="fix-before" name="before_url" required="">
                        for a in assets.iter().take(100) {
                            <option value=(&a.url)>(a.kind) " · " (&a.name) " · " (&a.scenario)</option>
                        }
                    </select>
                    <label for="fix-after">"After capture (verified solution — image or video)"</label>
                    <select id="fix-after" name="after_url" required="">
                        for a in assets.iter().take(100) {
                            <option value=(&a.url)>(a.kind) " · " (&a.name) " · " (&a.scenario)</option>
                        }
                    </select>
                    <label for="fix-tactics">"Applied tactics (comma-separated)"</label>
                    <input id="fix-tactics" name="tactics" value="hairline-seam-unification,oklch-surface,min-block-size-24px" maxlength="400">
                    <label for="fix-files">"Modified files (comma-separated)"</label>
                    <input id="fix-files" name="files" value="panel/src/panel.css,panel/src/pages.rs" maxlength="400">
                    <label for="fix-summary">"Solution summary"</label>
                    <textarea id="fix-summary" name="summary" required="" rows="2" maxlength="2000"></textarea>
                    <label for="fix-steps">"Intermediate steps (one per line: Step Title :: Detail :: Verification)"</label>
                    <textarea id="fix-steps" name="steps" rows="3" maxlength="3000" placeholder="CSS grid seam fix :: Replaced separate rounded card borders with 1px var(--line) grid :: Re-captured at 1440x900 and 320x844"></textarea>
                    <button class="button primary" type="submit">"Record fix & link problem"</button>
                </form>
            </details>
        }

        <div class="propose-toolbar">
            <p class="muted">(filtered.len()) " recorded fixes with Before/After evidence"</p>
            <form class="filters" method="get" action=(format!("{base}/fixes"))>
                <label for="f-status">"Status"</label>
                <select id="f-status" name="status">
                    <option value="">"All fixes"</option>
                    for st in FixStatus::ALL {
                        <option value=(st.slug()) if status_filter == st.slug() { selected="" }>(st.label())</option>
                    }
                </select>
                <button class="button" type="submit">"Filter"</button>
            </form>
        </div>

        if filtered.is_empty() {
            <section class="empty">
                <h2>"No fixes recorded yet"</h2>
                <p>"When a problem from the Problems queue is fixed and re-captured, its Before/After comparison and intermediate steps appear here."</p>
            </section>
        } else {
            <div class="fixes-list">
                for fix in &filtered {
                    <article class="surface fix-card" id=(format!("fix-{}", fix.id)) data-fix-card=(&fix.id)>
                        <header class="propose-head">
                            <div>
                                <div class="propose-badges">
                                    badge(variant: fix_status_badge(fix.status), (fix.status.label()))
                                    for pid in &fix.problem_ids {
                                        <a class="tactic-chip" href=(format!("{base}/problems#problem-{pid}"))>"Problem #" (pid.chars().take(10).collect::<String>()) " ↗"</a>
                                    }
                                    for file in &fix.modified_files {
                                        <code class="tactic-chip">(file)</code>
                                    }
                                </div>
                                <h2>(&fix.title)</h2>
                                <p class="muted">(&fix.summary)</p>
                            </div>
                            <time data-epoch-ms=(fix.created_at_ms.to_string())></time>
                        </header>

                        if let Some(pair) = fix.comparisons.first() {
                            <section class="fix-comparator" data-comparator="">
                                <div class="stage-bar">
                                    <div class="stage-viewports" role="group" aria-label="Comparison mode">
                                        <button type="button" class="button primary" data-compare-mode="split">"Split Slider"</button>
                                        <button type="button" class="button" data-compare-mode="side">"Side by Side"</button>
                                        if is_video_url(&pair.before_url) || is_video_url(&effective_after_url(fix, pair)) {
                                            <button type="button" class="button" data-compare-mode="video">"Video Player (Slow-Mo)"</button>
                                        }
                                        if pair.diff_url.is_some() {
                                            <button type="button" class="button" data-compare-mode="diff">"Pixel Diff"</button>
                                        }
                                    </div>
                                    if fix.comparisons.len() > 1 {
                                        <div class="stage-viewports" role="group" aria-label="Coordinate pair">
                                            for (idx, cmp) in fix.comparisons.iter().enumerate() {
                                                <button type="button"
                                                    class=(if idx == 0 { "button primary" } else { "button" })
                                                    data-pair-before=(&cmp.before_url)
                                                    data-pair-after=(effective_after_url(fix, cmp))
                                                    data-pair-diff=(cmp.diff_url.as_deref().unwrap_or(""))
                                                    data-pair-label=(&cmp.label)>
                                                    (&cmp.label)
                                                </button>
                                            }
                                        </div>
                                    } else {
                                        <span class="tactic-chip" data-pair-current-label="">
                                            (&pair.label)
                                            if is_video_url(&effective_after_url(fix, pair)) { " · VIDEO MP4" }
                                        </span>
                                    }
                                </div>

                                // 1. Interactive Split Slider View (supports Image vs Image, Image vs Video, Video vs Video)
                                <div class="compare-stage compare-split" data-stage-mode="split">
                                    <div class="split-viewport">
                                        <figure class="split-layer split-before">
                                            <img src=(if is_video_url(&pair.before_url) { "" } else { &pair.before_url })
                                                alt="Before fix" data-img-before=""
                                                if is_video_url(&pair.before_url) { hidden="" }>
                                            <video src=(if is_video_url(&pair.before_url) { &pair.before_url } else { "" })
                                                autoplay="" loop="" muted="" playsinline="" data-vid-before=""
                                                if !is_video_url(&pair.before_url) { hidden="" }></video>
                                            <span class="split-tag tag-before">
                                                (if is_video_url(&pair.before_url) { "BEFORE (VIDEO)" } else { "BEFORE (INITIAL)" })
                                            </span>
                                        </figure>
                                        <figure class="split-layer split-after">
                                            <img src=(if is_video_url(&effective_after_url(fix, pair)) { "" } else { &pair.after_url })
                                                alt="After fix" data-img-after=""
                                                if is_video_url(&effective_after_url(fix, pair)) { hidden="" }>
                                            <video src=(if is_video_url(&effective_after_url(fix, pair)) { effective_after_url(fix, pair) } else { String::new() })
                                                autoplay="" loop="" muted="" playsinline="" data-vid-after=""
                                                if !is_video_url(&effective_after_url(fix, pair)) { hidden="" }></video>
                                            <span class="split-tag tag-after">
                                                (if is_video_url(&effective_after_url(fix, pair)) { "AFTER (VIDEO)" } else { "AFTER (FINAL)" })
                                            </span>
                                        </figure>
                                        <div class="split-divider" aria-hidden="true">
                                            <span class="split-handle">"↔"</span>
                                        </div>
                                    </div>
                                    <div class="split-control-bar">
                                        <label for=(format!("slider-{}", fix.id))>"Split curtain"</label>
                                        <input id=(format!("slider-{}", fix.id)) type="range" min="0" max="100" value="50" class="split-range" data-split-range="">
                                        <output class="tactic-chip" data-split-output="">"50%"</output>
                                    </div>
                                    if is_video_url(&pair.before_url) || is_video_url(&effective_after_url(fix, pair)) {
                                        <div class="split-control-bar video-transport-bar" data-video-transport="">
                                            <button type="button" class="button" data-video-toggle="">"Pause ❚❚"</button>
                                            <button type="button" class="button" data-video-restart="">"Restart ↺"</button>
                                            <button type="button" class="button" data-video-rate="1">"1×"</button>
                                            <button type="button" class="button" data-video-rate="0.5">"0.5×"</button>
                                            <button type="button" class="button" data-video-rate="0.25">"0.25×"</button>
                                            <label for=(format!("vscrub-{}", fix.id))>"Timeline"</label>
                                            <input id=(format!("vscrub-{}", fix.id)) type="range" min="0" max="1000" value="0" class="split-range" data-video-scrub="">
                                            <output class="tactic-chip" data-video-time="">"0.0s"</output>
                                        </div>
                                    }
                                </div>

                                // 2. Side-by-Side View (Image or Video with native controls)
                                <div class="compare-stage compare-side" data-stage-mode="side" hidden="">
                                    <figure class="checkpoint">
                                        <img src=(if is_video_url(&pair.before_url) { "" } else { &pair.before_url })
                                            alt="Before fix" data-side-before=""
                                            if is_video_url(&pair.before_url) { hidden="" }>
                                        <video src=(if is_video_url(&pair.before_url) { &pair.before_url } else { "" })
                                            controls="" autoplay="" loop="" muted="" playsinline="" data-side-vid-before=""
                                            if !is_video_url(&pair.before_url) { hidden="" }></video>
                                        <figcaption>"BEFORE · Initial problem (" (&pair.label) ")"</figcaption>
                                    </figure>
                                    <figure class="checkpoint">
                                        <img src=(if is_video_url(&effective_after_url(fix, pair)) { "" } else { &pair.after_url })
                                            alt="After fix" data-side-after=""
                                            if is_video_url(&effective_after_url(fix, pair)) { hidden="" }>
                                        <video src=(if is_video_url(&effective_after_url(fix, pair)) { effective_after_url(fix, pair) } else { String::new() })
                                            controls="" autoplay="" loop="" muted="" playsinline="" data-side-vid-after=""
                                            if !is_video_url(&effective_after_url(fix, pair)) { hidden="" }></video>
                                        <figcaption>"AFTER · Verified fix (" (&pair.label) ")"</figcaption>
                                    </figure>
                                </div>

                                // 3. Dedicated Full Video Player View (when After or Before is a video)
                                if is_video_url(&pair.before_url) || is_video_url(&effective_after_url(fix, pair)) {
                                    <div class="compare-stage compare-video-full" data-stage-mode="video" hidden="">
                                        <figure class="checkpoint">
                                            <video src=(if is_video_url(&effective_after_url(fix, pair)) { effective_after_url(fix, pair) } else { pair.before_url.clone() })
                                                controls="" autoplay="" loop="" muted="" playsinline="" data-full-video=""></video>
                                            <figcaption>"FULL RECORDING · " (&pair.label) " (use timeline above for 0.25× / 0.5× slow-motion inspection)"</figcaption>
                                        </figure>
                                    </div>
                                }

                                // 4. Pixel Diff View
                                if let Some(diff) = &pair.diff_url {
                                    <div class="compare-stage compare-diff" data-stage-mode="diff" hidden="">
                                        <figure class="checkpoint">
                                            <img src=(diff) alt="Pixel diff" data-diff-img="">
                                            <figcaption>"PIXEL DIFF · Changed regions highlighted"</figcaption>
                                        </figure>
                                    </div>
                                }
                            </section>
                        }

                        if !fix.tactics.is_empty() {
                            <div class="tactic-chips">
                                <small class="studio-kind">"TACTICS APPLIED:"</small>
                                for t in &fix.tactics {
                                    <span class="tactic-chip">(t)</span>
                                }
                            </div>
                        }

                        if !fix.steps.is_empty() {
                            <details class="fix-steps-timeline" open="">
                                <summary>"Intermediate steps & verification loop (" (fix.steps.len()) " steps)"</summary>
                                <ol class="steps-list">
                                    for step in &fix.steps {
                                        <li>
                                            <div class="step-head">
                                                <span class="step-num">(format!("#{:02}", step.index))</span>
                                                <strong>(&step.title)</strong>
                                                <span class="tactic-chip">(&step.verification)</span>
                                            </div>
                                            if !step.detail.is_empty() {
                                                <p class="muted">(&step.detail)</p>
                                            }
                                        </li>
                                    }
                                </ol>
                            </details>
                        }

                        if !fix.reviews.is_empty() {
                            <ul class="problem-comments">
                                for rev in &fix.reviews {
                                    <li>
                                        <strong>(&rev.author)</strong> " (" (&rev.action) "): " (&rev.comment)
                                        " " <time data-epoch-ms=(rev.created_at_ms.to_string())></time>
                                    </li>
                                }
                            </ul>
                        }

                        if role != Role::Reader {
                            <footer class="propose-foot">
                                <form method="post" action=(format!("{base}/fixes/{}/review", fix.id)) class="fix-review-form">
                                    <input type="hidden" name="csrf" value=(&account.csrf)>
                                    <input name="comment" maxlength="1000" placeholder="Optional review note or requested adjustment…">
                                    <button class="button primary" type="submit" name="status" value="approved">"Approve Fix"</button>
                                    <button class="button" type="submit" name="status" value="changes-requested">
                                        (if mcp_online { "Request Changes & Dispatch" } else { "Request Changes" })
                                    </button>
                                </form>
                            </footer>
                        }
                    </article>
                }
            </div>
        }
    })
}

#[derive(Deserialize)]
pub struct AnalyzeRunForm {
    pub csrf: String,
    pub run_key: String,
}

pub async fn analyze_run_action(
    panel: Arc<Panel>,
    account: Account,
    project_id: String,
    form: AnalyzeRunForm,
) -> PageResult<Response> {
    auth::check_csrf(&account, &form.csrf)?;
    if auth::role(&panel, &account, &project_id).await? == Role::Reader {
        return Err(StatusCode::FORBIDDEN);
    }
    let data = data_root(&panel);
    let existing = problem_record::entries(&data, &project_id).unwrap_or_default();
    if let Some(manifest) = pages::read_run(&panel, &form.run_key)
        && manifest.project.id == project_id
    {
        if manifest.findings.is_empty() {
            if let Some(first) = manifest.checkpoints.first() {
                let mut candidate = problem_record::infer_from_finding(
                    &project_id,
                    &form.run_key,
                    &format!(
                        "[{} @ {}×{} {}] Visual inspection requested on capture '{}'",
                        first.id,
                        manifest.environment.width.unwrap_or(1440),
                        manifest.environment.height.unwrap_or(900),
                        manifest.environment.color_scheme,
                        manifest.scenario
                    ),
                    Some(&first.path),
                );
                candidate.status = ProblemStatus::NeedsReprocess;
                let _ = problem_record::save(&data, &candidate);
            }
        } else {
            for f in &manifest.findings {
                let candidate = problem_record::infer_from_finding(
                    &project_id,
                    &form.run_key,
                    &f.note,
                    f.evidence.as_deref(),
                );
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
                if !existing.iter().any(|p| p.diagnosis == candidate.diagnosis) {
                    let _ = problem_record::save(&data, &candidate);
                }
            }
        }
    }
    Ok(redirect(&format!("/projects/{project_id}/problems")))
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
    let data = data_root(&panel);
    let mut problem =
        problem_record::load(&data, &project_id, &problem_id).map_err(|_| StatusCode::NOT_FOUND)?;
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
            author: account.handle.clone(),
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
                    account.handle, problem.id
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
    Ok(redirect(&format!(
        "/projects/{project_id}/problems#problem-{problem_id}"
    )))
}

#[derive(Deserialize)]
pub struct ProgressProblemForm {
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
    let data = data_root(&panel);
    let mut problem =
        problem_record::load(&data, &project_id, &problem_id).map_err(|_| StatusCode::NOT_FOUND)?;
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
    Ok(redirect(&format!(
        "/projects/{project_id}/problems#problem-{problem_id}"
    )))
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

#[derive(Deserialize)]
pub struct CreateFixForm {
    pub csrf: String,
    pub title: String,
    pub problem_id: Option<String>,
    pub route: String,
    pub viewport: String,
    pub theme: String,
    pub before_url: String,
    pub after_url: String,
    pub tactics: String,
    pub files: String,
    pub summary: String,
    pub steps: String,
}

pub async fn create_fix_action(
    panel: Arc<Panel>,
    account: Account,
    project_id: String,
    form: CreateFixForm,
) -> PageResult<Response> {
    auth::check_csrf(&account, &form.csrf)?;
    if auth::role(&panel, &account, &project_id).await? == Role::Reader {
        return Err(StatusCode::FORBIDDEN);
    }
    let title = form.title.trim();
    let summary = form.summary.trim();
    if title.is_empty() || summary.is_empty() {
        return Err(StatusCode::UNPROCESSABLE_ENTITY);
    }
    let data = data_root(&panel);
    let (id, now) = fix_record::next_id();
    let problem_ids: Vec<String> = form
        .problem_id
        .into_iter()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();
    let tactics: Vec<String> = form
        .tactics
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();
    let modified_files: Vec<String> = form
        .files
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();
    let steps: Vec<FixStep> = form
        .steps
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .enumerate()
        .map(|(idx, line)| {
            let parts: Vec<&str> = line.splitn(3, "::").collect();
            let (st_title, detail, verification) = match parts.as_slice() {
                [a, b, c] => (a.trim(), b.trim(), c.trim()),
                [a, b] => (a.trim(), b.trim(), "Verified visually"),
                _ => (line, "", "Verified visually"),
            };
            FixStep {
                index: idx + 1,
                title: st_title.into(),
                detail: detail.into(),
                verification: verification.into(),
                evidence_url: None,
                timestamp_ms: now,
            }
        })
        .collect();
    let route = form.route.trim().to_string();
    let viewport = form.viewport.trim().to_string();
    let theme = form.theme.trim().to_string();
    let record = FixRecord {
        id: id.clone(),
        project_id: project_id.clone(),
        problem_ids: problem_ids.clone(),
        title: title.into(),
        summary: summary.into(),
        status: FixStatus::PendingReview,
        tactics,
        modified_files,
        comparisons: vec![ComparisonPair {
            label: format!("{route} · {viewport} · {theme}"),
            route,
            viewport,
            theme,
            before_url: form.before_url.trim().into(),
            after_url: form.after_url.trim().into(),
            diff_url: None,
            changed_ratio: None,
        }],
        steps,
        reviews: Vec::new(),
        author: account.handle,
        created_at_ms: now,
        updated_at_ms: now,
    };
    fix_record::save(&data, &record).map_err(db_error)?;
    for pid in &problem_ids {
        if let Ok(mut p) = problem_record::load(&data, &project_id, pid) {
            p.status = ProblemStatus::Resolved;
            p.linked_fix_id = Some(id.clone());
            p.updated_at_ms = now;
            let _ = problem_record::save(&data, &p);
        }
    }
    Ok(redirect(&format!("/projects/{project_id}/fixes#fix-{id}")))
}

#[derive(Deserialize)]
pub struct ReviewFixForm {
    pub csrf: String,
    pub status: String,
    pub comment: Option<String>,
}

pub async fn review_fix_action(
    panel: Arc<Panel>,
    account: Account,
    (project_id, fix_id): (String, String),
    form: ReviewFixForm,
) -> PageResult<Response> {
    auth::check_csrf(&account, &form.csrf)?;
    if auth::role(&panel, &account, &project_id).await? == Role::Reader {
        return Err(StatusCode::FORBIDDEN);
    }
    let data = data_root(&panel);
    let mut fix =
        fix_record::load(&data, &project_id, &fix_id).map_err(|_| StatusCode::NOT_FOUND)?;
    let Some(status) = FixStatus::from_slug(form.status.trim()) else {
        return Err(StatusCode::UNPROCESSABLE_ENTITY);
    };
    let (_, now) = fix_record::next_id();
    fix.status = status;
    fix.updated_at_ms = now;
    if let Some(c) = form
        .comment
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
    {
        fix.reviews.push(FixReview {
            author: account.handle,
            action: status.slug().into(),
            comment: c,
            created_at_ms: now,
        });
    }
    fix_record::save(&data, &fix).map_err(db_error)?;
    for pid in &fix.problem_ids {
        if let Ok(mut p) = problem_record::load(&data, &project_id, pid) {
            p.status = if status == FixStatus::ChangesRequested {
                ProblemStatus::InProgress
            } else {
                ProblemStatus::Resolved
            };
            p.updated_at_ms = now;
            let _ = problem_record::save(&data, &p);
        }
    }
    if status == FixStatus::Approved {
        let cfg = storage::load_config(&data);
        if cfg.auto_policy == "on-fix-approved" && !cfg.bucket.trim().is_empty() {
            let report = storage::analyze(&data, Some(&project_id));
            if let Some(fu) = report.by_fix.iter().find(|f| f.fix_id == fix_id) {
                for rname in &fu.referenced_runs {
                    let _ = storage::offload_run(&data, rname);
                }
            }
        }
    }
    Ok(redirect(&format!(
        "/projects/{project_id}/fixes#fix-{fix_id}"
    )))
}

#[expect(
    clippy::too_many_lines,
    reason = "the storage view keeps its markup and presentation branches together"
)]
pub fn render_storage<'a>(
    cx: &'a Cx,
    account: &'a Account,
    role: Role,
    _project_id: &'a str,
    base: &'a str,
    filter: &'a pages::Filter,
    report: &'a storage::StorageReport,
) -> topcoat::view::Child<'a> {
    let subview = filter.view.as_deref().unwrap_or("all");
    let notice = filter.q.as_deref().unwrap_or("");
    let total_reclaimable = report.reclaimable_bytes + report.raw_frames_bytes;
    let bucket_configured = !report.config.bucket.trim().is_empty();

    topcoat::view::Child::new(view! {
        cx =>
        if !notice.is_empty() {
            <p class="notice">(notice)</p>
        }

        <section class="studio-counts" aria-label="Storage telemetry">
            <div class="metric">
                <small>"Local disk used (" (report.total_files) " files)"</small>
                <strong>(storage::format_bytes(report.total_bytes))</strong>
            </div>
            <div class="metric">
                <small>"Pinned by Fixes & Problems"</small>
                <strong>(storage::format_bytes(report.pinned_bytes))</strong>
            </div>
            <div class=(if total_reclaimable > 0 { "metric featured" } else { "metric" })>
                <small>"Reclaimable / Offloadable (" (report.offloaded_runs) " offloaded)"</small>
                <strong>(storage::format_bytes(total_reclaimable))</strong>
            </div>
        </section>

        if role == Role::Owner {
            <details class="surface panel-padding design-editor">
                <summary>
                    (if bucket_configured {
                        format!("External bucket configured ({} · {}) — Retention & Offload policy", report.config.provider, report.config.bucket)
                    } else {
                        "Configure external bucket (S3 / Cloudflare R2 / MinIO / rclone / NAS) & auto-offload policy".to_string()
                    })
                </summary>
                <form class="stack-form" method="post" action=(format!("{base}/storage/config"))>
                    <input type="hidden" name="csrf" value=(&account.csrf)>
                    <div class="form-row-3">
                        <div>
                            <label for="st-provider">"Provider"</label>
                            <select id="st-provider" name="provider">
                                for (val, label) in [
                                    ("s3-compatible", "S3-Compatible (AWS S3 / R2 / MinIO / B2)"),
                                    ("rclone", "rclone Remote (GDrive / GCS / S3 / SFTP)"),
                                    ("filesystem-bucket", "Mounted Bucket / External Volume Path"),
                                ] {
                                    <option value=(val) if report.config.provider == val { selected="" }>(label)</option>
                                }
                            </select>
                        </div>
                        <div>
                            <label for="st-bucket">"Bucket name or volume path"</label>
                            <input id="st-bucket" name="bucket" required="" maxlength="240"
                                value=(&report.config.bucket) placeholder="e.g. rupicola-evidence-prod or /mnt/archive-bucket">
                        </div>
                        <div>
                            <label for="st-prefix">"Key prefix"</label>
                            <input id="st-prefix" name="prefix" maxlength="120" value=(&report.config.prefix)>
                        </div>
                    </div>
                    <div class="form-row-3">
                        <div>
                            <label for="st-endpoint">"Endpoint URL (S3 / R2 / MinIO)"</label>
                            <input id="st-endpoint" name="endpoint_url" maxlength="240"
                                value=(&report.config.endpoint_url) placeholder="https://<account>.r2.cloudflarestorage.com">
                        </div>
                        <div>
                            <label for="st-region">"Region"</label>
                            <input id="st-region" name="region" maxlength="60" value=(&report.config.region)>
                        </div>
                        <div>
                            <label for="st-public">"Public / CDN Base URL (for Before/After links)"</label>
                            <input id="st-public" name="public_base_url" maxlength="240"
                                value=(&report.config.public_base_url) placeholder="https://cdn.example.com/rupicola">
                        </div>
                    </div>
                    <div class="form-row-3">
                        <div>
                            <label for="st-policy">"Automatic offload policy"</label>
                            <select id="st-policy" name="auto_policy">
                                for (val, label) in [
                                    ("manual", "Manual offload only"),
                                    ("on-fix-approved", "Auto-offload when a Fix is Approved"),
                                    ("after-capture", "Always offload runs after capture"),
                                ] {
                                    <option value=(val) if report.config.auto_policy == val { selected="" }>(label)</option>
                                }
                            </select>
                        </div>
                        <div>
                            <label for="st-keep">"Keep latest N unpinned runs on disk"</label>
                            <input id="st-keep" name="keep_latest_runs" type="number" min="0" max="50"
                                value=(report.config.keep_latest_runs.to_string())>
                        </div>
                        <div>
                            <label for="st-del">"After bucket offload"</label>
                            <select id="st-del" name="delete_local">
                                <option value="true" if report.config.delete_local_after_offload { selected="" }>"Delete local PNG/MP4 files (keep manifest stub)"</option>
                                <option value="false" if !report.config.delete_local_after_offload { selected="" }>"Keep local copy + remote backup"</option>
                            </select>
                        </div>
                    </div>
                    <button class="button primary" type="submit">"Save storage & bucket policy"</button>
                </form>
            </details>
        }

        <div class="propose-toolbar">
            <div class="stage-viewports" role="group" aria-label="Storage grouping">
                for (slug, label) in [
                    ("all", "All groups"),
                    ("fix", "By Fix"),
                    ("date", "By Date"),
                    ("run", "By Capture Run"),
                    ("category", "By Category"),
                ] {
                    <a class=(if subview == slug { "button primary" } else { "button" })
                       href=(format!("{base}/storage?view={slug}"))>
                        (label)
                    </a>
                }
            </div>
            if role == Role::Owner {
                <div class="review-actions">
                    <form method="post" action=(format!("{base}/storage/clean")) class="inline-action">
                        <input type="hidden" name="csrf" value=(&account.csrf)>
                        <input type="hidden" name="mode" value="strip-frames">
                        <button class="button" type="submit">
                            "Strip raw clip frames (" (storage::format_bytes(report.raw_frames_bytes)) ")"
                        </button>
                    </form>
                    <form method="post" action=(format!("{base}/storage/clean")) class="inline-action">
                        <input type="hidden" name="csrf" value=(&account.csrf)>
                        <input type="hidden" name="mode" value="prune-unpinned">
                        <input type="hidden" name="keep_latest" value=(report.config.keep_latest_runs.to_string())>
                        <button class="button" type="submit">
                            "Prune unpinned runs (" (storage::format_bytes(report.reclaimable_bytes)) ")"
                        </button>
                    </form>
                    if bucket_configured {
                        <form method="post" action=(format!("{base}/storage/offload")) class="inline-action">
                            <input type="hidden" name="csrf" value=(&account.csrf)>
                            <input type="hidden" name="target" value="all-unpinned">
                            <button class="button primary" type="submit">"Offload unpinned to bucket"</button>
                        </form>
                    }
                </div>
            }
        </div>

        if subview == "all" || subview == "fix" {
            <section class="surface captures-ledger">
                <div class="section-head">
                    <h2>"Storage grouped by Fix (" (report.by_fix.len()) ")"</h2>
                    <p class="muted">"Pinned Before / After / Diff assets & referenced runs"</p>
                </div>
                if report.by_fix.is_empty() {
                    <div class="empty"><p>"No fixes recorded for this project yet."</p></div>
                } else {
                    <ul class="run-list">
                        for item in &report.by_fix {
                            <li>
                                <div class="run-row">
                                    <a class="run-link" href=(format!("{base}/fixes#fix-{}", item.fix_id))>
                                        <span class="run-title">
                                            <strong>(&item.title)</strong>
                                            <small>
                                                (&item.date) " · Runs: "
                                                (if item.referenced_runs.is_empty() { "none".to_string() } else { item.referenced_runs.join(", ") })
                                            </small>
                                        </span>
                                        <span class="run-state">
                                            badge(
                                                variant: if item.offloaded { BadgeVariant::Primary } else { BadgeVariant::Secondary },
                                                (if item.offloaded { "Offloaded to bucket" } else { &item.status })
                                            )
                                            <code>(storage::format_bytes(item.bytes)) " · " (item.files) " pinned assets"</code>
                                        </span>
                                        <span class="arrow" aria-hidden="true">"↗"</span>
                                    </a>
                                    if role == Role::Owner && bucket_configured && !item.offloaded {
                                        <form method="post" action=(format!("{base}/storage/offload")) class="inline-action">
                                            <input type="hidden" name="csrf" value=(&account.csrf)>
                                            <input type="hidden" name="fix_id" value=(&item.fix_id)>
                                            <button class="button" type="submit">"Offload Fix to bucket"</button>
                                        </form>
                                    }
                                </div>
                            </li>
                        }
                    </ul>
                }
            </section>
        }

        if subview == "all" || subview == "date" {
            <section class="surface captures-ledger">
                <div class="section-head">
                    <h2>"Storage grouped by Date (" (report.by_date.len()) " days)"</h2>
                    <p class="muted">"Daily capture volume, fix count, and reclaimable space"</p>
                </div>
                if report.by_date.is_empty() {
                    <div class="empty"><p>"No dated captures recorded yet."</p></div>
                } else {
                    <ul class="run-list">
                        for day in &report.by_date {
                            <li>
                                <div class="run-row">
                                    <div class="run-link">
                                        <span class="run-title">
                                            <strong>(&day.date)</strong>
                                            <small>(day.runs_count) " capture runs · " (day.fixes_count) " fixes · " (day.files) " files"</small>
                                        </span>
                                        <span class="run-state">
                                            <code>"Total: " (storage::format_bytes(day.bytes))</code>
                                            badge(
                                                variant: if day.reclaimable_bytes > 0 { BadgeVariant::Primary } else { BadgeVariant::Secondary },
                                                (format!("Reclaimable: {}", storage::format_bytes(day.reclaimable_bytes)))
                                            )
                                        </span>
                                    </div>
                                    if role == Role::Owner && day.reclaimable_bytes > 0 {
                                        <form method="post" action=(format!("{base}/storage/clean")) class="inline-action">
                                            <input type="hidden" name="csrf" value=(&account.csrf)>
                                            <input type="hidden" name="mode" value="prune-unpinned">
                                            <input type="hidden" name="date" value=(&day.date)>
                                            <input type="hidden" name="keep_latest" value="0">
                                            <button class="button" type="submit">"Clean unpinned (" (&day.date) ")"</button>
                                        </form>
                                    }
                                </div>
                            </li>
                        }
                    </ul>
                }
            </section>
        }

        if subview == "all" || subview == "run" {
            <section class="surface captures-ledger">
                <div class="section-head">
                    <h2>"Storage grouped by Capture Run (" (report.by_run.len()) ")"</h2>
                    <p class="muted">"Pin status, raw keyframe footprint, and per-run offload or cleanup"</p>
                </div>
                if report.by_run.is_empty() {
                    <div class="empty"><p>"No capture runs on disk."</p></div>
                } else {
                    <ul class="run-list">
                        for run in &report.by_run {
                            <li>
                                <div class="run-row">
                                    <a class="run-link" href=(format!("/run/{}", run.run_key))>
                                        <span class="run-title">
                                            <strong>(&run.scenario)</strong>
                                            <small>(&run.run_name) " · " (&run.date) " · " (run.files_count) " files"</small>
                                        </span>
                                        <span class="run-state">
                                            if run.offloaded {
                                                badge(variant: BadgeVariant::Primary, "Offloaded to bucket")
                                            } else if !run.pinned_by_fixes.is_empty() || run.pinned_by_problems > 0 {
                                                badge(
                                                    variant: BadgeVariant::Secondary,
                                                    (format!("Pinned · {} Fix(es), {} Problem(s)", run.pinned_by_fixes.len(), run.pinned_by_problems))
                                                )
                                            } else {
                                                badge(variant: BadgeVariant::Outline, "Reclaimable")
                                            }
                                            <code>(storage::format_bytes(run.total_bytes))</code>
                                        </span>
                                        <span class="arrow" aria-hidden="true">"↗"</span>
                                    </a>
                                    if role == Role::Owner {
                                        <div class="review-actions">
                                            if bucket_configured && !run.offloaded {
                                                <form method="post" action=(format!("{base}/storage/offload")) class="inline-action">
                                                    <input type="hidden" name="csrf" value=(&account.csrf)>
                                                    <input type="hidden" name="run_name" value=(&run.run_name)>
                                                    <button class="button" type="submit">"Offload"</button>
                                                </form>
                                            }
                                            if run.pinned_by_fixes.is_empty() && run.pinned_by_problems == 0 {
                                                <form method="post" action=(format!("{base}/storage/clean")) class="inline-action">
                                                    <input type="hidden" name="csrf" value=(&account.csrf)>
                                                    <input type="hidden" name="mode" value="delete-run">
                                                    <input type="hidden" name="run_name" value=(&run.run_name)>
                                                    <button class="button" type="submit">"Delete"</button>
                                                </form>
                                            }
                                        </div>
                                    }
                                </div>
                            </li>
                        }
                    </ul>
                }
            </section>
        }

        if subview == "all" || subview == "category" {
            <section class="surface captures-ledger">
                <div class="section-head">
                    <h2>"Storage grouped by Asset Category"</h2>
                </div>
                <div class="problem-queue">
                    for cat in &report.by_category {
                        <div class="task-tracker">
                            <div class="tracker-head">
                                <strong>(&cat.category)</strong>
                                <span class="tracker-metrics">
                                    (storage::format_bytes(cat.bytes)) " · " (cat.pct) "%"
                                </span>
                            </div>
                            <progress class="task-progress-bar" max="100" value=(cat.pct.to_string())></progress>
                        </div>
                    }
                </div>
            </section>
        }
    })
}

#[derive(Deserialize)]
pub struct StorageConfigForm {
    pub csrf: String,
    pub provider: String,
    pub bucket: String,
    pub endpoint_url: String,
    pub region: String,
    pub prefix: String,
    pub public_base_url: String,
    pub auto_policy: String,
    pub keep_latest_runs: usize,
    pub delete_local: String,
}

pub fn save_storage_config_action(
    panel: &Panel,
    account: &Account,
    project_id: &str,
    form: &StorageConfigForm,
) -> PageResult<Response> {
    auth::check_csrf(account, &form.csrf)?;
    if !account.owner {
        return Err(StatusCode::FORBIDDEN);
    }
    let cfg = storage::BucketConfig {
        provider: form.provider.trim().into(),
        bucket: form.bucket.trim().into(),
        endpoint_url: form.endpoint_url.trim().into(),
        region: form.region.trim().into(),
        prefix: form.prefix.trim().into(),
        public_base_url: form.public_base_url.trim().into(),
        auto_policy: form.auto_policy.trim().into(),
        delete_local_after_offload: form.delete_local.trim() != "false",
        keep_latest_runs: form.keep_latest_runs.min(100),
    };
    storage::save_config(&data_root(panel), &cfg).map_err(db_error)?;
    Ok(redirect(&format!(
        "/projects/{project_id}/storage?q=Saved+external+bucket+configuration"
    )))
}

#[derive(Deserialize)]
pub struct StorageCleanForm {
    pub csrf: String,
    pub mode: String,
    pub keep_latest: Option<usize>,
    pub date: Option<String>,
    pub run_name: Option<String>,
}

pub fn clean_storage_action(
    panel: &Panel,
    account: &Account,
    project_id: &str,
    form: &StorageCleanForm,
) -> PageResult<Response> {
    auth::check_csrf(account, &form.csrf)?;
    if !account.owner {
        return Err(StatusCode::FORBIDDEN);
    }
    let data = data_root(panel);
    let summary = match form.mode.as_str() {
        "strip-frames" => storage::strip_raw_frames(&data, Some(project_id)).map_err(db_error)?,
        "prune-unpinned" => storage::prune_unpinned(
            &data,
            Some(project_id),
            form.keep_latest.unwrap_or(2),
            form.date.as_deref(),
        )
        .map_err(db_error)?,
        "delete-run" => {
            let r = form
                .run_name
                .as_deref()
                .ok_or(StatusCode::UNPROCESSABLE_ENTITY)?;
            storage::delete_run(&data, r, false).map_err(db_error)?
        }
        _ => return Err(StatusCode::UNPROCESSABLE_ENTITY),
    };
    let encoded_note = summary.note.replace(' ', "+");
    Ok(redirect(&format!(
        "/projects/{project_id}/storage?q={encoded_note}"
    )))
}

#[derive(Deserialize)]
pub struct StorageOffloadForm {
    pub csrf: String,
    pub run_name: Option<String>,
    pub fix_id: Option<String>,
    pub target: Option<String>,
}

pub fn offload_storage_action(
    panel: &Panel,
    account: &Account,
    project_id: &str,
    form: StorageOffloadForm,
) -> PageResult<Response> {
    auth::check_csrf(account, &form.csrf)?;
    if !account.owner {
        return Err(StatusCode::FORBIDDEN);
    }
    let data = data_root(panel);
    let note = if let Some(rname) = form.run_name.filter(|s| !s.trim().is_empty()) {
        match storage::offload_run(&data, rname.trim()) {
            Ok(s) => s.note,
            Err(e) => e,
        }
    } else if let Some(fid) = form.fix_id.filter(|s| !s.trim().is_empty()) {
        let report = storage::analyze(&data, Some(project_id));
        if let Some(f) = report.by_fix.iter().find(|x| x.fix_id == fid) {
            let mut freed = 0u64;
            let mut count = 0usize;
            let mut last_err = None;
            for rname in &f.referenced_runs {
                match storage::offload_run(&data, rname) {
                    Ok(s) => {
                        freed += s.freed_bytes;
                        count += 1;
                    }
                    Err(e) => last_err = Some(e),
                }
            }
            if let Some(e) = last_err {
                e
            } else {
                format!(
                    "Offloaded {count} run(s) for Fix {fid}, freeing {}",
                    storage::format_bytes(freed)
                )
            }
        } else {
            "Fix not found".to_string()
        }
    } else if form.target.as_deref() == Some("all-unpinned") {
        let report = storage::analyze(&data, Some(project_id));
        let mut freed = 0u64;
        let mut count = 0usize;
        let mut last_err = None;
        for run in &report.by_run {
            if run.pinned_by_fixes.is_empty() && run.pinned_by_problems == 0 && !run.offloaded {
                match storage::offload_run(&data, &run.run_name) {
                    Ok(s) => {
                        freed += s.freed_bytes;
                        count += 1;
                    }
                    Err(e) => {
                        last_err = Some(e);
                        break;
                    }
                }
            }
        }
        if let Some(e) = last_err {
            e
        } else {
            format!(
                "Offloaded {count} unpinned run(s), freeing {}",
                storage::format_bytes(freed)
            )
        }
    } else {
        "No offload target specified".to_string()
    };
    let encoded = note.replace(' ', "+");
    Ok(redirect(&format!(
        "/projects/{project_id}/storage?q={encoded}"
    )))
}
