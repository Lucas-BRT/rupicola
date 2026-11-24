use super::{AssetOption, Cx, FixRecord, FixStatus, Problem, Role, badge, fix_status_badge, view};
use crate::Account;
use crate::components::{
    display::{chip, counts, kind_label, metric, moment},
    form::{csrf, field, select_field, stack_form, text_area},
};
use crate::project::Filter;
use crate::records::ComparisonPair;
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

        let filtered: Vec<&FixRecord> = fixes
            .iter()
            .filter(|f| status_filter.is_empty() || f.status.as_str() == status_filter)
            .collect();

        counts(label: "Fixes overview",
            metric(href: format!("{base}/fixes?status=pending-review"), value: pending_count, featured: pending_count > 0, arrow: false, "Pending approval")
            metric(href: format!("{base}/fixes?status=approved"), value: approved_count, arrow: false, "Approved fixes")
            metric(href: format!("{base}/fixes?status=changes-requested"), value: changes_count, arrow: false, "Changes requested")
        )
        if role != Role::Reader {
            record_form(base: base, token: &account.csrf, problems: &problems, assets: &assets)
        }
        <div class="propose-toolbar">
            <p class="muted">(filtered.len()) " recorded fixes with Before/After evidence"</p>
            <form class="filters" method="get" action=(format!("{base}/fixes"))>
                <label for="f-status">"Status"</label>
                <select id="f-status" name="status">
                    <option value="">"All fixes"</option>
                    for st in FixStatus::ALL {
                        <option value=(st.as_str()) if status_filter == st.as_str() { selected="" }>(st.label())</option>
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
                    fix_card(fix: fix, base: base, role: role, token: &account.csrf)
                }
            </div>
        }
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

/// One fix: its labels, comparison, tactics, steps, reviews and review controls.
#[component]
async fn fix_card(
    cx: &Cx,
    fix: &FixRecord,
    base: &str,
    role: Role,
    token: &str,
) -> Result<impl View> {
    Ok(view! {
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
                moment(epoch_ms: fix.created_at_ms)
            </header>
            if let Some(pair) = fix.comparisons.first() {
                comparator(fix: fix, pair: pair)
            }
            if !fix.tactics.is_empty() {
                <div class="tactic-chips">
                    kind_label("TACTICS APPLIED:")
                    for t in &fix.tactics {
                        chip((t))
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
                                    chip((&step.verification))
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
                            " " moment(epoch_ms: rev.created_at_ms)
                        </li>
                    }
                </ul>
            }
            if role != Role::Reader {
                <footer class="propose-foot">
                    <form method="post" action=(format!("{base}/fixes/{}/review", fix.id)) class="fix-review-form">
                        csrf(token: token)
                        <input name="comment" maxlength="1000" placeholder="Optional review note or requested adjustment…">
                        <button class="button primary" type="submit" name="status" value="approved">"Approve Fix"</button>
                        <button class="button" type="submit" name="status" value="changes-requested">"Request Changes"</button>
                    </form>
                </footer>
            }
        </article>
    })
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

/// The before and after of a fix, in the modes the capture supports.
#[component]
async fn comparator(cx: &Cx, fix: &FixRecord, pair: &ComparisonPair) -> Result<impl View> {
    let after = effective_after_url(fix, pair);
    let before_video = is_video_url(&pair.before_url);
    let after_video = is_video_url(&after);
    Ok(view! {
        <section class="fix-comparator" data-comparator="">
            <div class="stage-bar">
                <div class="stage-viewports" role="group" aria-label="Comparison mode">
                    <button type="button" class="button primary" data-compare-mode="split">"Split Slider"</button>
                    <button type="button" class="button" data-compare-mode="side">"Side by Side"</button>
                    if before_video || after_video {
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
                                data-pair-label=(cmp.caption())>
                                (cmp.caption())
                            </button>
                        }
                    </div>
                } else {
                    <span class="tactic-chip" data-pair-current-label="">
                        (pair.caption())
                        if after_video { " · VIDEO MP4" }
                    </span>
                }
            </div>
        split_stage(fix_id: &fix.id, pair: pair, after: &after, before_video: before_video, after_video: after_video)
        side_stage(pair: pair, after: &after, before_video: before_video, after_video: after_video)
        if before_video || after_video {
            full_stage(pair: pair, after: &after, after_video: after_video)
        }
        if let Some(diff) = &pair.diff_url {
            diff_stage(diff: diff)
        }
        </section>
    })
}

/// The slider that wipes between the before and the after.
#[component]
async fn split_stage(
    cx: &Cx,
    fix_id: &str,
    pair: &ComparisonPair,
    after: &str,
    before_video: bool,
    after_video: bool,
) -> Result<impl View> {
    Ok(view! {
        <div class="compare-stage compare-split" data-stage-mode="split">
            <div class="split-viewport">
                <figure class="split-layer split-before">
                    <img src=(if before_video { "" } else { &pair.before_url })
                        alt="Before fix" data-img-before=""
                        if before_video { hidden="" }>
                    <video src=(if before_video { &pair.before_url } else { "" })
                        autoplay="" loop="" muted="" playsinline="" data-vid-before=""
                        if !before_video { hidden="" }></video>
                    <span class="split-tag tag-before">
                        (if before_video { "BEFORE (VIDEO)" } else { "BEFORE (INITIAL)" })
                    </span>
                </figure>
                <figure class="split-layer split-after">
                    <img src=(if after_video { "" } else { &pair.after_url })
                        alt="After fix" data-img-after=""
                        if after_video { hidden="" }>
                    <video src=(if after_video { after } else { "" })
                        autoplay="" loop="" muted="" playsinline="" data-vid-after=""
                        if !after_video { hidden="" }></video>
                    <span class="split-tag tag-after">
                        (if after_video { "AFTER (VIDEO)" } else { "AFTER (FINAL)" })
                    </span>
                </figure>
                <div class="split-divider" aria-hidden="true">
                    <span class="split-handle">"↔"</span>
                </div>
            </div>
            <div class="split-control-bar">
                <label for=(format!("slider-{}", fix_id))>"Split curtain"</label>
                <input id=(format!("slider-{}", fix_id)) type="range" min="0" max="100" value="50" class="split-range" data-split-range="">
                <output class="tactic-chip" data-split-output="">"50%"</output>
            </div>
            if before_video || after_video {
                <div class="split-control-bar video-transport-bar" data-video-transport="">
                    <button type="button" class="button" data-video-toggle="">"Pause ❚❚"</button>
                    <button type="button" class="button" data-video-restart="">"Restart ↺"</button>
                    <button type="button" class="button" data-video-rate="1">"1×"</button>
                    <button type="button" class="button" data-video-rate="0.5">"0.5×"</button>
                    <button type="button" class="button" data-video-rate="0.25">"0.25×"</button>
                    <label for=(format!("vscrub-{}", fix_id))>"Timeline"</label>
                    <input id=(format!("vscrub-{}", fix_id)) type="range" min="0" max="1000" value="0" class="split-range" data-video-scrub="">
                    <output class="tactic-chip" data-video-time="">"0.0s"</output>
                </div>
            }
        </div>


    })
}

/// The before and the after side by side.
#[component]
async fn side_stage(
    cx: &Cx,
    pair: &ComparisonPair,
    after: &str,
    before_video: bool,
    after_video: bool,
) -> Result<impl View> {
    Ok(view! {
        <div class="compare-stage compare-side" data-stage-mode="side" hidden="">
            <figure class="checkpoint">
                <img src=(if before_video { "" } else { &pair.before_url })
                    alt="Before fix" data-side-before=""
                    if before_video { hidden="" }>
                <video src=(if before_video { &pair.before_url } else { "" })
                    controls="" autoplay="" loop="" muted="" playsinline="" data-side-vid-before=""
                    if !before_video { hidden="" }></video>
                <figcaption>"BEFORE · Initial problem (" (&pair.label) ")"</figcaption>
            </figure>
            <figure class="checkpoint">
                <img src=(if after_video { "" } else { &pair.after_url })
                    alt="After fix" data-side-after=""
                    if after_video { hidden="" }>
                <video src=(if after_video { after } else { "" })
                    controls="" autoplay="" loop="" muted="" playsinline="" data-side-vid-after=""
                    if !after_video { hidden="" }></video>
                <figcaption>"AFTER · Verified fix (" (&pair.label) ")"</figcaption>
            </figure>
        </div>


    })
}

/// The recording alone, at full size.
#[component]
async fn full_stage(
    cx: &Cx,
    pair: &ComparisonPair,
    after: &str,
    after_video: bool,
) -> Result<impl View> {
    Ok(view! {
            <div class="compare-stage compare-video-full" data-stage-mode="video" hidden="">
                <figure class="checkpoint">
                    <video src=(if after_video { after } else { pair.before_url.as_str() })
                        controls="" autoplay="" loop="" muted="" playsinline="" data-full-video=""></video>
                    <figcaption>"FULL RECORDING · " (&pair.label) " (use timeline above for 0.25× / 0.5× slow-motion inspection)"</figcaption>
                </figure>
            </div>
    })
}

/// The pixel difference between the two.
#[component]
async fn diff_stage(cx: &Cx, diff: &str) -> Result<impl View> {
    Ok(view! {
        <div class="compare-stage compare-diff" data-stage-mode="diff" hidden="">
            <figure class="checkpoint">
                <img src=(diff) alt="Pixel diff" data-diff-img="">
                <figcaption>"PIXEL DIFF · Changed regions highlighted"</figcaption>
            </figure>
        </div>
    })
}
