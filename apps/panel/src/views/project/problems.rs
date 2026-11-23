use super::{
    AssetOption, Cx, DesignEntry, Problem, ProblemStatus, Role, Severity, badge,
    problem_status_badge, severity_badge, view,
};
use crate::Account;
use crate::components::{
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

        let filtered: Vec<&Problem> = problems
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

        counts(label: "Problem triage counts",
            metric(href: format!("{base}/problems?status=open"), value: open_count, featured: open_count > 0, arrow: false, "Open / Needs reprocess")
            metric(href: format!("{base}/problems?status=in-progress"), value: progress_count, arrow: false, "In progress / Fixing")
            metric(href: format!("{base}/problems?status=resolved"), value: resolved_count, arrow: false, "Resolved · Intentional (" (intentional_count) ")")
        )
        if role != Role::Reader {
            report_form(base: base, token: &account.csrf, prefill_asset: prefill_asset, prefill_name: prefill_name, assets: &assets, design_entries: &design_entries)
        }
        problem_toolbar(base: base, shown: filtered.len(), status_filter: status_filter, severity_filter: severity_filter)
        if filtered.is_empty() {
            <section class="empty">
                <h2>"No problems match this filter"</h2>
                <p>"Extract problems from a research capture run in Captures or report a visual issue above."</p>
            </section>
        } else {
            <div class="problem-queue">
                for item in &filtered {
                    problem_card(item: item, base: base, role: role, token: &account.csrf)
                }
            </div>
        }
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
        <details class="surface panel-padding design-editor" if !prefill_asset.is_empty() { open="" }>
            <summary>"Report or register a problem from captures"</summary>
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
                    <option value="Design#flyio-copper-broadsheet:Fly.io Copper Broadsheet Direction">"Design#flyio-copper-broadsheet — Fly.io Copper Broadsheet Direction"</option>
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
        </details>
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
) -> Result<impl View> {
    Ok(view! {
        <div class="propose-toolbar">
            <p class="muted">
                (shown) " problems in queue · "
                "triage states persist for CLI & agent sessions"
            </p>
            <form class="filters" method="get" action=(format!("{base}/problems"))>
                <label for="p-status">"Status"</label>
                <select id="p-status" name="status">
                    <option value="">"All statuses"</option>
                    for st in ProblemStatus::ALL {
                        <option value=(st.as_str()) if status_filter == st.as_str() { selected="" }>(st.label())</option>
                    }
                </select>
                <label for="p-sev">"Severity"</label>
                <select id="p-sev" name="kind">
                    <option value="">"All severities"</option>
                    for sev in Severity::ALL {
                        <option value=(sev.as_str()) if severity_filter == sev.as_str() { selected="" }>(sev.label())</option>
                    }
                </select>
                <button class="button" type="submit">"Filter"</button>
            </form>
        </div>
    })
}

/// One problem: its labels, evidence, rules, diagnosis, discussion and triage controls.
#[component]
async fn problem_card(
    cx: &Cx,
    item: &Problem,
    base: &str,
    role: Role,
    token: &str,
) -> Result<impl View> {
    Ok(view! {
        <article class="surface problem-card" id=(format!("problem-{}", item.id)) data-problem-card=(&item.id)>
            <header class="propose-head">
                <div>
                    <div class="propose-badges">
                        badge(variant: severity_badge(item.severity), (item.severity.label()))
                        badge(variant: problem_status_badge(item.status), (item.status.label()))
                        chip((&item.route) " · " (&item.viewport) " · " (&item.theme))
                        chip((item.origin.label()))
                    </div>
                    <h2>(&item.title)</h2>
                </div>
                moment(epoch_ms: item.created_at_ms)
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
                    kind_label("DIAGNOSIS")
                    <p>(&item.diagnosis)</p>
                </div>
                <div class="problem-box solution-box">
                    kind_label("PROPOSED SOLUTION")
                    <p>(&item.proposed_solution)</p>
                </div>
            </div>
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
