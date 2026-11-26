use crate::records::{ComparisonPair, EvidenceAsset, FixRecord};
use topcoat::{
    Result,
    context::Cx,
    view::{View, component, view},
};

#[component]
pub(super) async fn baseline_stage(cx: &Cx, baseline: &[EvidenceAsset]) -> Result<impl View> {
    Ok(view! {
        <section class="fix-comparator" aria-label="Baseline capture">
            <div class="stage-bar">
                <span class="muted">"Baseline capture — awaiting after capture"</span>
            </div>
            <div class="compare-side">
                for ev in baseline.iter().take(4) {
                    let is_vid = is_video_url(&ev.url);
                    <figure class="checkpoint">
                        if is_vid {
                            <video src=(&ev.url) controls="" autoplay="" loop="" muted="" playsinline=""></video>
                        } else {
                            <img src=(&ev.url) alt=(format!("Before: {}", ev.label))>
                        }
                        <figcaption>
                            "Before — " <a href=(format!("/run/{}", ev.run_key))>"Open capture ↗"</a>
                        </figcaption>
                    </figure>
                }
            </div>
        </section>
    })
}

pub(super) fn is_video_url(url: &str) -> bool {
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

pub(super) fn effective_after_url(fix: &FixRecord, pair: &ComparisonPair) -> String {
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
pub(super) async fn comparator(
    cx: &Cx,
    fix: &FixRecord,
    pair: &ComparisonPair,
) -> Result<impl View> {
    let after = effective_after_url(fix, pair);
    let before_video = is_video_url(&pair.before_url);
    let after_video = is_video_url(&after);
    Ok(view! {
        <section class="fix-comparator" data-comparator="" data-zoom="fit">
            <div class="stage-bar">
                <div class="stage-viewports" role="group" aria-label="Comparison mode">
                    <button type="button" class="button primary" data-compare-mode="split">"Split"</button>
                    <button type="button" class="button" data-compare-mode="side">"Side by side"</button>
                    if before_video || after_video {
                        <button type="button" class="button" data-compare-mode="video">"Video"</button>
                    }
                    if pair.diff_url.is_some() {
                        <button type="button" class="button" data-compare-mode="diff">"Pixel diff"</button>
                    }
                </div>
                <div class="stage-viewports" role="group" aria-label="Zoom and view">
                    if fix.comparisons.len() > 1 {
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
                    }
                    <div class="compare-zoom-group" role="group" aria-label="Zoom level">
                        <button type="button" class="button primary" data-compare-zoom="fit">"Fit"</button>
                        <button type="button" class="button" data-compare-zoom="100">"100%"</button>
                        <button type="button" class="button" data-compare-zoom="150">"150%"</button>
                        <button type="button" class="button" data-compare-zoom="200">"200%"</button>
                    </div>
                    <button type="button" class="button" data-compare-fullscreen="" aria-pressed="false">"Fullscreen"</button>
                </div>
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
            <div class="compare-scroll">
                <div class="split-viewport">
                    <figure class="split-layer split-before">
                        <img src=(if before_video { "" } else { &pair.before_url })
                            alt="Before" data-img-before=""
                            if before_video { hidden="" }>
                        <video src=(if before_video { &pair.before_url } else { "" })
                            autoplay="" loop="" muted="" playsinline="" data-vid-before=""
                            if !before_video { hidden="" }></video>
                        <span class="split-tag tag-before">
                            (if before_video { "Before (video)" } else { "Before" })
                        </span>
                    </figure>
                    <figure class="split-layer split-after">
                        <img src=(if after_video { "" } else { &pair.after_url })
                            alt="After" data-img-after=""
                            if after_video { hidden="" }>
                        <video src=(if after_video { after } else { "" })
                            autoplay="" loop="" muted="" playsinline="" data-vid-after=""
                            if !after_video { hidden="" }></video>
                        <span class="split-tag tag-after">
                            (if after_video { "After (video)" } else { "After" })
                        </span>
                    </figure>
                    <div class="split-divider" aria-hidden="true">
                        <span class="split-handle">"↔"</span>
                    </div>
                </div>
            </div>
            <div class="split-control-bar">
                <label class="sr-only" for=(format!("slider-{}", fix_id))>"Split position"</label>
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
                <div class="compare-scroll">
                    <img src=(if before_video { "" } else { &pair.before_url })
                        alt="Before" data-side-before=""
                        if before_video { hidden="" }>
                    <video src=(if before_video { &pair.before_url } else { "" })
                        controls="" autoplay="" loop="" muted="" playsinline="" data-side-vid-before=""
                        if !before_video { hidden="" }></video>
                </div>
                <figcaption>"Before"</figcaption>
            </figure>
            <figure class="checkpoint">
                <div class="compare-scroll">
                    <img src=(if after_video { "" } else { &pair.after_url })
                        alt="After" data-side-after=""
                        if after_video { hidden="" }>
                    <video src=(if after_video { after } else { "" })
                        controls="" autoplay="" loop="" muted="" playsinline="" data-side-vid-after=""
                        if !after_video { hidden="" }></video>
                </div>
                <figcaption>"After"</figcaption>
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
                    <figcaption>"Recording"</figcaption>
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
                <div class="compare-scroll">
                    <img src=(diff) alt="Pixel diff" data-diff-img="">
                </div>
                <figcaption>"Pixel diff"</figcaption>
            </figure>
        </div>
    })
}
