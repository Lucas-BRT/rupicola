mod stage;
use super::{AssetOption, Cx, FixRecord, FixStatus, Problem, Role, badge, fix_status_badge, view};
use crate::Account;
use crate::components::{
    controls::{SelectOption, select_menu},
    display::{chip, counts, link_button, metric, moment},
    form::{csrf, field, select_field, stack_form, text_area},
};
use crate::project::Filter;
use crate::records::EvidenceAsset;
use rupicola_api::{Job, JobStatus};
use stage::{baseline_stage, comparator, effective_after_url, is_video_url};
use topcoat::{
    Result,
    context::context,
    view::{View, attributes, component},
};

/// The fixes tab's content.
#[component]
pub async fn fixes_tab(
    cx: &Cx,
    role: Role,
    base: &str,
    filter: Filter,
    fixes: Vec<FixRecord>,
    problems: Vec<Problem>,
    assets: Vec<AssetOption>,
    open_jobs: Vec<Job>,
) -> Result<impl View> {
    let account = context::<Account>(cx);
    Ok(view! {
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

        let sort_key = filter.sort_key();
        let filtered: Vec<&FixRecord> = {
            let mut list: Vec<&FixRecord> = fixes
                .iter()
                .filter(|f| status_filter.is_empty() || f.status.as_str() == status_filter)
                .collect();
            match sort_key {
                "oldest" => list.sort_by_key(|f| f.created_at_ms),
                "title" => list.sort_by(|a, b| {
                    a.title
                        .to_lowercase()
                        .cmp(&b.title.to_lowercase())
                        .then_with(|| b.created_at_ms.cmp(&a.created_at_ms))
                }),
                _ => list.sort_by_key(|f| std::cmp::Reverse(f.created_at_ms)),
            }
            list
        };

        counts(label: "Fixes overview",
            metric(href: format!("{base}/fixes?status=pending-review"), value: pending_count, featured: pending_count > 0, arrow: false, "Pending approval")
            metric(href: format!("{base}/fixes?status=approved"), value: approved_count, arrow: false, "Approved fixes")
            metric(href: format!("{base}/fixes?status=changes-requested"), value: changes_count, arrow: false, "Changes requested")
        )
        if role != Role::Reader {
            record_form(base: base, token: &account.csrf, problems: &problems, assets: &assets)
        }
        <div id="fixes-region" class="skel-stack">
            fixes_toolbar(base: base, shown: filtered.len(), status_filter: status_filter, sort_key: sort_key)
            <div data-swap-body="rows">
                if filtered.is_empty() {
                    <section class="empty">
                        <h2>"No fixes recorded yet"</h2>
                        <p>"When a problem from the Problems queue is fixed and re-captured, its Before/After comparison and intermediate steps appear here."</p>
                    </section>
                } else {
                    <div class="fixes-list">
                        for fix in &filtered {
                            let fix_job = open_jobs
                                .iter()
                                .find(|j| j.fix.as_deref() == Some(fix.id.as_str()));
                            let baseline = if fix.comparisons.is_empty() {
                                linked_baseline_first(fix, &problems)
                            } else {
                                None
                            };
                            fix_card(fix: fix, base: base, job: fix_job, baseline: baseline)
                        }
                    </div>
                }
            </div>
        </div>
    })
}

#[component]
async fn fixes_toolbar(
    cx: &Cx,
    base: &str,
    shown: usize,
    status_filter: &str,
    sort_key: &str,
) -> Result<impl View> {
    let fix_status_options: Vec<SelectOption> = std::iter::once(SelectOption::new("", "All fixes"))
        .chain(
            FixStatus::ALL
                .iter()
                .map(|st| SelectOption::new(st.as_str(), st.label())),
        )
        .collect();
    let fix_sort_options = [
        SelectOption::new("newest", "Newest first"),
        SelectOption::new("oldest", "Oldest first"),
        SelectOption::new("title", "Title A–Z"),
    ];
    Ok(view! {
        <div class="propose-toolbar">
            <p class="muted" data-swap-meta="count">(shown) " recorded fixes with Before/After evidence"</p>
            <form class="filters" method="get" action=(format!("{base}/fixes")) data-swap-target="fixes-region">
                select_menu(
                    id: "f-status",
                    name: "status",
                    label: "Filter fixes by status",
                    selected: status_filter,
                    options: &fix_status_options,
                    auto_submit: true,
                    compact: true,
                )
                select_menu(
                    id: "f-sort",
                    name: "sort",
                    label: "Sort fixes",
                    selected: sort_key,
                    options: &fix_sort_options,
                    auto_submit: true,
                    compact: true,
                )
                <noscript><button class="button" type="submit">"Filter"</button></noscript>
            </form>
        </div>
    })
}

/// The form that records a fix with its before and after captures.
#[component]
async fn record_form(
    cx: &Cx,
    base: &str,
    token: &str,
    problems: &[Problem],
    assets: &[AssetOption],
) -> Result<impl View> {
    Ok(view! {
        <details class="surface panel-padding design-editor">
            <summary>"Record a new Before/After Fix comparison"</summary>
            stack_form(action: format!("{base}/fixes/create"), token: token,
                field(id: "fix-title", name: "title", label: "Fix title",
                    attrs: attributes! { required="" maxlength="200" placeholder="e.g. Continuous 1px hairline seams in Settings & Project Settings" })
                select_field(id: "fix-problem", name: "problem_id", label: "Resolves Problem",
                    <option value="">"Standalone fix"</option>
                    for p in problems {
                        <option value=(&p.id)>(&p.title) " (" (&p.route) " · " (p.status.label()) ")"</option>
                    }
                )
                <div class="form-row-3">
                    <div>
                        field(id: "fix-route", name: "route", label: "Route",
                            attrs: attributes! { value="/settings" maxlength="160" })
                    </div>
                    <div>
                        field(id: "fix-vp", name: "viewport", label: "Viewport",
                            attrs: attributes! { value="1440×900" maxlength="40" })
                    </div>
                    <div>
                        field(id: "fix-theme", name: "theme", label: "Theme",
                            attrs: attributes! { value="dark" maxlength="30" })
                    </div>
                </div>
                select_field(id: "fix-before", name: "before_url", label: "Before capture (initial bug/problem — image or video)",
                    attrs: attributes! { required="" },
                    for a in assets.iter().take(100) {
                        <option value=(&a.url)>(a.kind) " · " (&a.name) " · " (&a.scenario)</option>
                    }
                )
                select_field(id: "fix-after", name: "after_url", label: "After capture (verified solution — image or video)",
                    attrs: attributes! { required="" },
                    for a in assets.iter().take(100) {
                        <option value=(&a.url)>(a.kind) " · " (&a.name) " · " (&a.scenario)</option>
                    }
                )
                field(id: "fix-tactics", name: "tactics", label: "Applied tactics (comma-separated)",
                    attrs: attributes! { value="hairline-seam-unification,oklch-surface,min-block-size-24px" maxlength="400" })
                field(id: "fix-files", name: "files", label: "Modified files (comma-separated)",
                    attrs: attributes! { value="apps/panel/assets/panel.css,apps/panel/src/views/settings.rs" maxlength="400" })
                text_area(id: "fix-summary", name: "summary", label: "Solution summary",
                    attrs: attributes! { required="" rows="2" maxlength="2000" })
                text_area(id: "fix-steps", name: "steps", label: "Intermediate steps (one per line: Step Title :: Detail :: Verification)",
                    attrs: attributes! { rows="3" maxlength="3000" placeholder="CSS grid seam fix :: Replaced separate rounded card borders with 1px var(--line) grid :: Re-captured at 1440x900 and 320x844" })
                <button class="button primary" type="submit">"Record fix & link problem"</button>
            )
        </details>
    })
}

fn linked_baseline_first<'a>(
    fix: &FixRecord,
    problems: &'a [Problem],
) -> Option<&'a EvidenceAsset> {
    problems
        .iter()
        .filter(|p| fix.problem_ids.contains(&p.id))
        .flat_map(|p| p.evidence_assets.iter())
        .next()
}

/// One fix in the list: minimal scannable summary linking to its detail page.
#[component]
async fn fix_card(
    cx: &Cx,
    fix: &FixRecord,
    base: &str,
    job: Option<&Job>,
    baseline: Option<&EvidenceAsset>,
) -> Result<impl View> {
    let detail_href = format!("{base}/fixes/{}", fix.id);
    let job_text = job.map(|j| {
        if j.status == JobStatus::Queued {
            "Queued for recapture".to_owned()
        } else {
            format!("Recapturing · {}/{}", j.cells_done, j.cells_total)
        }
    });
    let summary_meta = format!("{} steps · {} reviews", fix.steps.len(), fix.reviews.len());
    Ok(view! {
        <article class="surface fix-card problem-card-compact" id=(format!("fix-{}", fix.id)) data-fix-card=(&fix.id)>
            <header class="propose-head">
                <div>
                    <div class="propose-badges">
                        badge(variant: fix_status_badge(fix.status), (fix.status.label()))
                        <span class="fix-job-badge" data-fix-job-badge=(&fix.id) if job_text.is_none() { hidden="" }>(job_text.as_deref().unwrap_or(""))</span>
                    </div>
                    <h2>
                        <a class="problem-title-link" href=(&detail_href) data-nav-partial="">(&fix.title)</a>
                    </h2>
                    <p class="muted problem-summary-meta">(&fix.summary)</p>
                    <p class="muted problem-summary-meta">(summary_meta)</p>
                </div>
                <div class="review-actions">
                    link_button(href: &detail_href, "Compare ↗")
                </div>
            </header>
            if let Some(pair) = fix.comparisons.first() {
                let after = effective_after_url(fix, pair);
                let before_video = is_video_url(&pair.before_url);
                let after_video = is_video_url(&after);
                <a class="fix-preview-strip" href=(&detail_href) data-nav-partial="" aria-label=(format!("Compare before and after for {}", fix.title))>
                    <div class="fix-preview-thumb">
                        if before_video {
                            <video src=(&pair.before_url) muted="" playsinline="" preload="metadata"></video>
                        } else {
                            <img src=(&pair.before_url) alt="Before fix preview" loading="lazy">
                        }
                        <span class="split-tag tag-before">"BEFORE"</span>
                    </div>
                    <div class="fix-preview-thumb">
                        if after_video {
                            <video src=(&after) muted="" playsinline="" preload="metadata"></video>
                        } else {
                            <img src=(&after) alt="After fix preview" loading="lazy">
                        }
                        <span class="split-tag tag-after">"AFTER"</span>
                    </div>
                </a>
            } else if let Some(ev) = baseline {
                let before_video = is_video_url(&ev.url);
                <a class="fix-preview-strip" href=(&detail_href) data-nav-partial="" aria-label=(format!("Inspect baseline evidence for {}", fix.title))>
                    <div class="fix-preview-thumb">
                        if before_video {
                            <video src=(&ev.url) muted="" playsinline="" preload="metadata"></video>
                        } else {
                            <img src=(&ev.url) alt=(format!("Before fix baseline: {}", ev.label)) loading="lazy">
                        }
                        <span class="split-tag tag-before">"BEFORE"</span>
                    </div>
                    <div class="fix-preview-thumb fix-preview-pending">
                        <span class="split-tag tag-after">"AFTER"</span>
                        <span class="muted">"Awaiting after capture"</span>
                    </div>
                </a>
            }
        </article>
    })
}

/// Full detail page for a single fix (`/projects/{project}/fixes/{fix}`).
#[component]
pub async fn fix_detail(
    cx: &Cx,
    role: Role,
    base: &str,
    fix: FixRecord,
    job: Option<Job>,
    linked_problems: Vec<(String, String)>,
    baseline: Vec<EvidenceAsset>,
) -> Result<impl View> {
    let account = context::<Account>(cx);
    let token = &account.csrf;
    let job_text = job.as_ref().map(|j| {
        if j.status == JobStatus::Queued {
            "Queued for recapture".to_owned()
        } else {
            format!("Recapturing {}/{}", j.cells_done, j.cells_total)
        }
    });
    let has_rail = !linked_problems.is_empty()
        || !fix.problem_ids.is_empty()
        || !fix.modified_files.is_empty()
        || !fix.tactics.is_empty()
        || !fix.steps.is_empty()
        || !fix.reviews.is_empty();
    Ok(view! {
        <div class="propose-toolbar">
            <a class="button button-ghost" href=(format!("{base}/fixes")) data-nav-partial="">"← All fixes"</a>
            <div class="toolbar-lead">
                moment(epoch_ms: fix.created_at_ms)
                if has_rail {
                    <button
                        class="button"
                        type="button"
                        data-rail-toggle=""
                        data-label-hide="Hide details"
                        data-label-show="Show details"
                        aria-expanded="true">"Hide details"</button>
                }
            </div>
        </div>
        <div class=(if has_rail { "stage-shell fix-detail-shell" } else { "fix-detail-shell" }) data-stage-shell="">
            <article class="surface fix-card" id=(format!("fix-{}", fix.id)) data-fix-card=(&fix.id)>
                <header class="fix-header">
                    <div class="propose-badges">
                        badge(variant: fix_status_badge(fix.status), (fix.status.label()))
                        <span class="fix-job-badge" data-fix-job-badge=(&fix.id) if job_text.is_none() { hidden="" }>(job_text.as_deref().unwrap_or(""))</span>
                    </div>
                    <h2>(&fix.title)</h2>
                    if !fix.summary.is_empty() {
                        <p class="fix-summary muted">(&fix.summary)</p>
                    }
                </header>
                if let Some(pair) = fix.comparisons.first() {
                    comparator(fix: &fix, pair: pair)
                } else if !baseline.is_empty() {
                    baseline_stage(baseline: &baseline)
                } else {
                    <div class="empty">
                        <p class="muted">"No before/after capture pair attached to this fix yet."</p>
                    </div>
                }
                if role != Role::Reader {
                    <footer class="propose-foot">
                        <form method="post" action=(format!("{base}/fixes/{}/review", fix.id)) class="fix-review-form">
                            csrf(token: token)
                            <input name="comment" maxlength="1000" placeholder="Add a review note…">
                            <button class="button primary" type="submit" name="status" value="approved">"Approve"</button>
                            <button class="button" type="submit" name="status" value="changes-requested">"Request changes"</button>
                        </form>
                    </footer>
                }
            </article>
            if has_rail {
                fix_meta_rail(base: base, fix: &fix, linked_problems: &linked_problems)
            }
        </div>
    })
}

/// Collapsible right-hand metadata rail for a single fix.
#[component]
async fn fix_meta_rail(
    cx: &Cx,
    base: &str,
    fix: &FixRecord,
    linked_problems: &[(String, String)],
) -> Result<impl View> {
    Ok(view! {
        <aside class="canvas-rail fix-meta-rail" aria-label="Fix details">
            if !linked_problems.is_empty() {
                <section class="fix-meta-section">
                    <h3>"Linked problems"</h3>
                    <ul class="fix-meta-list">
                        for (pid, title) in linked_problems {
                            <li>
                                <a href=(format!("{base}/problems/{pid}")) data-nav-partial="">(title)</a>
                            </li>
                        }
                    </ul>
                </section>
            } else if !fix.problem_ids.is_empty() {
                <section class="fix-meta-section">
                    <h3>"Linked problems"</h3>
                    <ul class="fix-meta-list">
                        for pid in &fix.problem_ids {
                            <li>
                                <a class="mono" href=(format!("{base}/problems/{pid}")) data-nav-partial="">(pid)</a>
                            </li>
                        }
                    </ul>
                </section>
            }
            if !fix.modified_files.is_empty() {
                <section class="fix-meta-section">
                    <h3>"Modified files"</h3>
                    <ul class="fix-meta-list">
                        for file in &fix.modified_files {
                            <li><code class="fix-meta-path">(file)</code></li>
                        }
                    </ul>
                </section>
            }
            if !fix.tactics.is_empty() {
                <section class="fix-meta-section">
                    <h3>"Tactics"</h3>
                    <div class="tactic-chips">
                        for t in &fix.tactics {
                            chip((t))
                        }
                    </div>
                </section>
            }
            if !fix.steps.is_empty() {
                <section class="fix-meta-section">
                    <details class="fix-steps-timeline">
                        <summary>"Steps (" (fix.steps.len()) ")"</summary>
                        <ol class="steps-list">
                            for step in &fix.steps {
                                <li>
                                    <div class="step-head">
                                        <span class="step-num">(format!("#{:02}", step.index))</span>
                                        <strong>(&step.title)</strong>
                                        if !step.verification.is_empty() {
                                            chip((&step.verification))
                                        }
                                    </div>
                                    if !step.detail.is_empty() {
                                        <p class="muted">(&step.detail)</p>
                                    }
                                </li>
                            }
                        </ol>
                    </details>
                </section>
            }
            if !fix.reviews.is_empty() {
                <section class="fix-meta-section">
                    <h3>"Reviews"</h3>
                    <ul class="fix-meta-list">
                        for rev in &fix.reviews {
                            <li class="fix-review-item">
                                <div class="fix-review-meta">
                                    <strong>(&rev.author)</strong>
                                    <span class="muted">(&rev.action)</span>
                                </div>
                                if !rev.comment.is_empty() {
                                    <p class="muted">(&rev.comment)</p>
                                }
                                moment(epoch_ms: rev.created_at_ms)
                            </li>
                        }
                    </ul>
                </section>
            }
        </aside>
    })
}
