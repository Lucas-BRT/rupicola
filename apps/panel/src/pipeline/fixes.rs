use super::{
    Account, Arc, AssetOption, Cx, Deserialize, FixRecord, FixStatus, PageResult, Panel, Problem,
    Response, Role, StatusCode, auth, badge, effective_after_url, fix_status_badge, is_video_url,
    pages, redirect, view,
};

/// Verification recorded for a step written without one.
const DEFAULT_VERIFICATION: &str = "Verified visually";

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
                    <input id="fix-files" name="files" value="apps/panel/src/panel.css,apps/panel/src/pages.rs" maxlength="400">
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
                                        "Request Changes"
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
    let problems: Vec<String> = form
        .problem_id
        .into_iter()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();
    let list = |text: &str| -> Vec<String> {
        text.split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect()
    };
    let steps: Vec<rupicola_api::NewStep> = form
        .steps
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(|line| {
            let parts: Vec<&str> = line.splitn(3, "::").collect();
            let (st_title, detail, verification) = match parts.as_slice() {
                [a, b, c] => (a.trim(), b.trim(), c.trim()),
                [a, b] => (a.trim(), b.trim(), DEFAULT_VERIFICATION),
                _ => (line, "", DEFAULT_VERIFICATION),
            };
            rupicola_api::NewStep {
                title: st_title.into(),
                detail: detail.into(),
                verification: verification.into(),
                evidence: None,
            }
        })
        .collect();
    let route = form.route.trim().to_string();
    let viewport = form.viewport.trim().to_string();
    let theme = form.theme.trim().to_string();
    let (before, after) = (form.before_url.trim(), form.after_url.trim());
    let comparisons = if before.is_empty() || after.is_empty() {
        Vec::new()
    } else {
        vec![rupicola_api::Comparison {
            label: format!("{route} · {viewport} · {theme}"),
            route,
            viewport,
            theme,
            before: crate::records::artifact_of(before).to_owned(),
            after: crate::records::artifact_of(after).to_owned(),
            diff: None,
            changed_ratio: None,
        }]
    };
    let new = rupicola_api::NewFix {
        title: title.into(),
        summary: summary.into(),
        problems,
        tactics: list(&form.tactics),
        files: list(&form.files),
        comparisons,
        steps,
    };
    let fix = panel
        .api
        .create_fix(&account.session, &project_id, &new)
        .await
        .map_err(|e| auth::api_error(&e))?;
    Ok(redirect(&format!(
        "/projects/{project_id}/fixes#fix-{}",
        fix.id
    )))
}

#[derive(Deserialize)]
pub struct ReviewFixForm {
    #[serde(default)]
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
    let outcome = form
        .status
        .trim()
        .parse()
        .map_err(|_| StatusCode::UNPROCESSABLE_ENTITY)?;
    let review = rupicola_api::NewReview {
        outcome,
        comment: form.comment.unwrap_or_default().trim().to_owned(),
    };
    panel
        .api
        .review_fix(&account.session, &fix_id, &review)
        .await
        .map_err(|e| auth::api_error(&e))?;
    Ok(redirect(&format!(
        "/projects/{project_id}/fixes#fix-{fix_id}"
    )))
}
