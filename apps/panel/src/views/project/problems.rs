use super::{
    AssetOption, Cx, DesignEntry, Problem, ProblemStatus, Role, Severity, badge,
    problem_status_badge, severity_badge, view,
};
use crate::Account;
use crate::components::{
    controls::{SelectOption, select_menu},
    display::{chip, counts, kind_label, link_button, metric, moment},
    form::{csrf, field, select_field, stack_form, text_area},
};
use crate::project::Filter;
use topcoat::{
    Result,
    context::context,
    view::{View, attributes, component},
};

/// The problems tab's content.
#[component]
pub async fn problems_tab(
    cx: &Cx,
    role: Role,
    base: &str,
    filter: Filter,
    problems: Vec<Problem>,
    assets: Vec<AssetOption>,
    design_entries: Vec<DesignEntry>,
) -> Result<impl View> {
    let account = context::<Account>(cx);
    Ok(view! {
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

        let sort_key = filter.sort_key();
        let filtered: Vec<&Problem> = {
            let mut list: Vec<&Problem> = problems
                .iter()
                .filter(|p| {
                    (status_filter.is_empty() || p.status.as_str() == status_filter)
                        && (severity_filter.is_empty() || p.severity.as_str() == severity_filter)
                        && (q.is_empty()
                            || p.title.to_lowercase().contains(&q)
                            || p.diagnosis.to_lowercase().contains(&q)
                            || p.route.to_lowercase().contains(&q))
                })
                .collect();
            match sort_key {
                "oldest" => list.sort_by_key(|p| p.created_at_ms),
                "severity" => {
                    list.sort_by_key(|p| {
                        let rank = match p.severity {
                            Severity::Critical => 0_u8,
                            Severity::Major => 1,
                            Severity::Minor => 2,
                            Severity::Polish => 3,
                        };
                        (rank, std::cmp::Reverse(p.created_at_ms))
                    });
                }
                _ => list.sort_by_key(|p| std::cmp::Reverse(p.created_at_ms)),
            }
            list
        };

        counts(label: "Problem triage counts",
            metric(href: format!("{base}/problems?status=open"), value: open_count, featured: open_count > 0, arrow: false, "Open / Needs reprocess")
            metric(href: format!("{base}/problems?status=in-progress"), value: progress_count, arrow: false, "In progress / Fixing")
            metric(href: format!("{base}/problems?status=resolved"), value: resolved_count, arrow: false, "Resolved · Intentional (" (intentional_count) ")")
        )
        if role != Role::Reader {
            report_form(base: base, token: &account.csrf, prefill_asset: prefill_asset, prefill_name: prefill_name, assets: &assets, design_entries: &design_entries)
        }
        <div id="problems-region" class="skel-stack">
            problem_toolbar(base: base, shown: filtered.len(), status_filter: status_filter, severity_filter: severity_filter, query: filter.q.as_deref().unwrap_or(""), sort: sort_key, can_report: role != Role::Reader)
            <div data-swap-body="rows">
                if filtered.is_empty() {
                    <section class="empty">
                        <h2>"No problems match this filter"</h2>
                        <p>"Extract problems from a research capture run in Captures or report a visual issue above."</p>
                    </section>
                } else {
                    <div class="problem-queue">
                        for item in &filtered {
                            problem_card(item: item, base: base)
                        }
                    </div>
                }
            </div>
        </div>
        image_viewer()
    })
}

/// The form that reports a problem found in a capture.
#[component]
async fn report_form(
    cx: &Cx,
    base: &str,
    token: &str,
    prefill_asset: &str,
    prefill_name: &str,
    assets: &[AssetOption],
    design_entries: &[DesignEntry],
) -> Result<impl View> {
    Ok(view! {
        <dialog id="report-problem-modal" class="panel-modal" aria-labelledby="report-problem-title" if !prefill_asset.is_empty() { data-auto-open="" }>
            <div class="panel-modal-head">
                <h2 id="report-problem-title">"Report problem"</h2>
                <form method="dialog"><button class="button" type="submit">"Close"</button></form>
            </div>
            stack_form(action: format!("{base}/problems/create"), token: token,
                <label for="prob-title">"Problem title"</label>
                <input id="prob-title" name="title" required="" maxlength="200"
                    value=(if prefill_name.is_empty() { String::new() } else { format!("Visual issue on {prefill_name}") })
                    placeholder="e.g. Disconnected 14px gap between Settings cards">
                <div class="form-row-3">
                    <div>
                        field(id: "prob-route", name: "route", label: "Route",
                            attrs: attributes! { value="/settings" maxlength="160" })
                    </div>
                    <div>
                        field(id: "prob-vp", name: "viewport", label: "Viewport",
                            attrs: attributes! { value="1440×900" maxlength="40" })
                    </div>
                    <div>
                        select_field(id: "prob-theme", name: "theme", label: "Theme",
                            <option value="dark">"Dark"</option>
                            <option value="light">"Light"</option>
                            <option value="dark+light">"Dark + Light"</option>
                        )
                    </div>
                </div>
                select_field(id: "prob-severity", name: "severity", label: "Severity",
                    <option value="critical">"Critical (layout break, overflow, broken control)"</option>
                    <option value="major" selected="">"Major (hierarchy, target size, visual seam defect)"</option>
                    <option value="minor">"Minor (alignment, density, contrast)"</option>
                    <option value="polish">"Polish (micro-interaction, copy, rhythm)"</option>
                )
                select_field(id: "prob-evidence", name: "evidence_url", label: "Evidence asset from Captures",
                    if !prefill_asset.is_empty() {
                        <option value=(prefill_asset) selected="">(prefill_name) " (" (prefill_asset) ")"</option>
                    }
                    <option value="">"Select captured screenshot or clip…"</option>
                    for asset in assets.iter().take(80) {
                        <option value=(&asset.url)>(asset.kind) " · " (&asset.name) " (" (&asset.scenario) ")"</option>
                    }
                )
                select_field(id: "prob-global-rule", name: "global_rule", label: "Violated global norm",
                        <option value="common.md#hairline-grid:Continuous 1px Hairline Structural Seams">"common.md#hairline-grid — Continuous 1px Hairline Structural Seams"</option>
                        <option value="common.md#target-size-24px:Minimum 24×24px Interactive Target Size">"common.md#target-size-24px — Minimum 24×24px Interactive Target Size"</option>
                        <option value="layout.md#no-horizontal-overflow:Zero Document Horizontal Overflow">"layout.md#no-horizontal-overflow — Zero Document Horizontal Overflow"</option>
                        <option value="layout.md#no-sibling-collision:Non-Overlapping Interactive Controls">"layout.md#no-sibling-collision — Non-Overlapping Interactive Controls"</option>
                        <option value="motion.md#hover-stability:Zero Layout Shift on Hover/Focus">"motion.md#hover-stability — Zero Layout Shift on Hover/Focus"</option>
                        <option value="motion.md#instant-tab-switch:Instant Tab & Dialog Transitions">"motion.md#instant-tab-switch — Instant Tab & Dialog Transitions"</option>
                        <option value="content.md#anti-verbosity:Zero Decorative Boilerplate Microcopy">"content.md#anti-verbosity — Zero Decorative Boilerplate Microcopy"</option>
                )
                select_field(id: "prob-project-rule", name: "project_rule", label: "Violated project design norm",
                    <option value="">"None"</option>
                    for d in design_entries.iter().take(12) {
                        <option value=(format!("Design#{}:{}", d.id, d.entry.title))>(d.entry.kind.label()) " · " (&d.entry.title)</option>
                    }
                )
                text_area(id: "prob-diagnosis", name: "diagnosis", label: "Diagnosis (what is broken & why)",
                    attrs: attributes! { required="" rows="3" maxlength="2000" placeholder="Root cause observed in the capture and DOM/CSS…" })
                text_area(id: "prob-solution", name: "solution", label: "Proposed solution",
                    attrs: attributes! { required="" rows="3" maxlength="2000" placeholder="Concrete CSS/HTML/state fix to apply…" })
                <button class="button primary" type="submit">"Add problem to queue"</button>
            )
        </dialog>
    })
}

/// The count of problems shown and the status and severity filter.
#[component]
async fn problem_toolbar(
    cx: &Cx,
    base: &str,
    shown: usize,
    status_filter: &str,
    severity_filter: &str,
    query: &str,
    sort: &str,
    can_report: bool,
) -> Result<impl View> {
    let mut status_options = vec![SelectOption::new("", "All statuses")];
    status_options.extend(
        ProblemStatus::ALL
            .iter()
            .map(|st| SelectOption::new(st.as_str(), st.label())),
    );
    let mut severity_options = vec![SelectOption::new("", "All severities")];
    severity_options.extend(
        Severity::ALL
            .iter()
            .map(|sev| SelectOption::new(sev.as_str(), sev.label())),
    );
    let sort_options = [
        SelectOption::new("newest", "Newest first"),
        SelectOption::new("oldest", "Oldest first"),
        SelectOption::new("severity", "Highest severity"),
    ];
    Ok(view! {
        <div class="propose-toolbar">
            <div class="toolbar-lead">
                <p class="muted" data-swap-meta="count">
                    (shown) (if shown == 1 { " problem" } else { " problems" })
                </p>
                if can_report {
                    <button class="button primary" type="button" data-modal-open="report-problem-modal">"+ Report problem"</button>
                }
            </div>
            <form class="filters" method="get" action=(format!("{base}/problems")) data-swap-target="problems-region">
                select_menu(
                    id: "p-status",
                    name: "status",
                    label: "Filter by status",
                    selected: status_filter,
                    options: &status_options,
                    auto_submit: true,
                    compact: true,
                )
                select_menu(
                    id: "p-sev",
                    name: "kind",
                    label: "Filter by severity",
                    selected: severity_filter,
                    options: &severity_options,
                    auto_submit: true,
                    compact: true,
                )
                select_menu(
                    id: "p-sort",
                    name: "sort",
                    label: "Sort problems",
                    selected: sort,
                    options: &sort_options,
                    auto_submit: true,
                    compact: true,
                )
                <label class="sr-only" for="p-q">"Search problems"</label>
                <input id="p-q" name="q" type="search" maxlength="100" value=(query) placeholder="Title, route…">
                <button class="button" type="submit">"Filter"</button>
            </form>
        </div>
    })
}

fn problem_meta(item: &Problem) -> String {
    [item.route.trim(), item.viewport.trim(), item.theme.trim()]
        .into_iter()
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(" · ")
}

#[component]
async fn problem_analysis(cx: &Cx, diagnosis: &str, solution: &str) -> Result<impl View> {
    let has_diagnosis = !diagnosis.trim().is_empty();
    let has_solution = !solution.trim().is_empty();
    Ok(view! {
        if has_diagnosis || has_solution {
            <div class="problem-analysis-grid">
                if has_diagnosis {
                    <div class="problem-box">
                        kind_label("DIAGNOSIS")
                        <p>(diagnosis)</p>
                    </div>
                }
                if has_solution {
                    <div class="problem-box solution-box">
                        kind_label("PROPOSED SOLUTION")
                        <p>(solution)</p>
                    </div>
                }
            </div>
        }
    })
}

/// One problem in the list: minimal scannable summary linking to its detail page.
#[component]
async fn problem_card(cx: &Cx, item: &Problem, base: &str) -> Result<impl View> {
    let detail_href = format!("{base}/problems/{}", item.id);
    let ev_count = item.evidence_assets.len();
    let comment_count = item.comments.len();
    let summary_meta = match (ev_count, comment_count) {
        (0, 0) => String::new(),
        (e, 0) => format!("{e} evidence"),
        (0, c) => format!("{c} comments"),
        (e, c) => format!("{e} evidence · {c} comments"),
    };
    Ok(view! {
        <article class="surface problem-card problem-card-compact" id=(format!("problem-{}", item.id)) data-problem-card=(&item.id)>
            <header class="propose-head">
                <div>
                    <div class="propose-badges">
                        badge(variant: severity_badge(item.severity), (item.severity.label()))
                        badge(variant: problem_status_badge(item.status), (item.status.label()))
                    </div>
                    <h2>
                        <a class="problem-title-link" href=(&detail_href) data-nav-partial="">(&item.title)</a>
                    </h2>
                    if !summary_meta.is_empty() {
                        <p class="muted problem-summary-meta">(summary_meta)</p>
                    }
                </div>
                <div class="review-actions">
                    if let Some(fid) = &item.linked_fix_id {
                        link_button(href: format!("{base}/fixes#fix-{fid}"), "Fix ↗")
                    }
                    link_button(href: &detail_href, "Details ↗")
                </div>
            </header>
        </article>
    })
}

/// Full detail page for a single problem (`/projects/{project}/problems/{problem}`).
#[component]
pub async fn problem_detail(cx: &Cx, role: Role, base: &str, item: Problem) -> Result<impl View> {
    let account = context::<Account>(cx);
    let token = &account.csrf;
    let meta_chip = problem_meta(&item);
    Ok(view! {
        <div class="propose-toolbar">
            <a class="button button-ghost" href=(format!("{base}/problems")) data-nav-partial="">"← All problems"</a>
            moment(epoch_ms: item.created_at_ms)
        </div>
        <article class="surface problem-card" id=(format!("problem-{}", item.id)) data-problem-card=(&item.id)>
            <header class="propose-head">
                <div>
                    <div class="propose-badges">
                        badge(variant: severity_badge(item.severity), (item.severity.label()))
                        badge(variant: problem_status_badge(item.status), (item.status.label()))
                        if !meta_chip.is_empty() {
                            chip((meta_chip))
                        }
                        chip((item.origin.label()))
                    </div>
                    <h2>(&item.title)</h2>
                </div>
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
            if !item.violated_rules.is_empty() {
                <div class="problem-rules">
                    for violation in &item.violated_rules {
                        <span class=(if violation.scope == "project" { "rule-chip project-rule" } else { "rule-chip global-rule" })>
                            <strong>(if violation.scope == "project" { "Project Norm" } else { "Global Norm" })</strong>
                            " · " (&violation.code) " — " (&violation.title)
                        </span>
                    }
                </div>
            }
            problem_analysis(diagnosis: &item.diagnosis, solution: &item.proposed_solution)
            if !item.comments.is_empty() {
                <ul class="problem-comments">
                    for c in &item.comments {
                        <li>
                            <strong>(&c.author)</strong> ": " (&c.text)
                            " " moment(epoch_ms: c.created_at_ms)
                        </li>
                    }
                </ul>
            }
            <footer class="propose-foot">
                <div class="review-actions">
                    if let Some(fid) = &item.linked_fix_id {
                        link_button(href: format!("{base}/fixes#fix-{fid}"), primary: true, "Compare Before/After Fix ↗")
                    }
                    if role != Role::Reader {
                        <form method="post" action=(format!("{base}/problems/{}/triage", item.id)) class="review-actions">
                            csrf(token: token)
                            <button class="button primary" type="submit" name="status" value="in-progress">"Queue for fix"</button>
                            <button class="button" type="submit" name="status" value="intentional" title="Never flag this route/rule again">"Mark Intentional"</button>
                            <button class="button" type="submit" name="status" value="false-positive">"False positive"</button>
                            <button class="button" type="submit" name="status" value="needs-reprocess">"Reprocess"</button>
                        </form>
                    }
                </div>
                if role != Role::Reader {
                    <div class="review-actions">
                        <details class="triage-note-drawer">
                            <summary>"Refine solution / add triage note"</summary>
                            <form method="post" action=(format!("{base}/problems/{}/triage", item.id)) class="stack-form">
                                csrf(token: token)
                                <input type="hidden" name="status" value="needs-reprocess">
                                text_area(id: format!("sol-{}", item.id), name: "solution", label: "Updated solution proposal", value: &item.proposed_solution,
                                    attrs: attributes! { rows="2" maxlength="2000" })
                                field(id: format!("com-{}", item.id), name: "comment", label: "Triage note / guidance for agent",
                                    attrs: attributes! { maxlength="1000" placeholder="Clarify intent or constraints…" })
                                <button class="button" type="submit">"Save & flag for reprocess"</button>
                            </form>
                        </details>
                    </div>
                }
            </footer>
        </article>
        image_viewer()
    })
}

/// The dialog that shows an evidence image at full size.
#[component]
async fn image_viewer(cx: &Cx) -> Result<impl View> {
    Ok(view! {
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
