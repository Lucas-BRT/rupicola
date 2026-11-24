use crate::components::{
    display::link_button,
    form::stack_form,
    layout::{breadcrumbs, page_title},
};
use crate::{Account, media};
use rupicola_api::{Capture as ApiCapture, CaptureSummary, Checkpoint, Clip, Finding};
use std::collections::BTreeMap;
use topcoat::{
    Result,
    context::{Cx, context},
    view::{View, component, view},
};

/// What the capture page shows.
pub struct Capture {
    pub record: ApiCapture,
    pub project_label: String,
    pub candidates: Vec<CaptureSummary>,
    pub requested: Option<String>,
    pub before_images: BTreeMap<String, String>,
    pub environments_differ: bool,
    pub comparing: bool,
    pub may_review: bool,
    pub width: String,
    pub height: String,
    pub project_url: String,
    pub form_url: String,
}

/// The capture page's content.
#[component]
pub async fn run_page(cx: &Cx, data: &Capture) -> Result<impl View> {
    let Capture {
        record: run,
        project_label,
        candidates,
        requested,
        before_images,
        environments_differ,
        comparing,
        may_review,
        width,
        height,
        project_url,
        form_url,
    } = data;
    let account = context::<Account>(cx);
    Ok(view! {
        breadcrumbs(
            <a href="/projects">"Projects"</a><span aria-hidden="true">"/"</span>
            <a href=(project_url)>(project_label)</a>
        )
        page_title(title: &run.scenario, class: "project-masthead",
            <div class="filters">
                <button class="button" type="button" data-rail-toggle="" aria-expanded="true">"Hide Inspector"</button>
                link_button(href: format!("{project_url}/captures"), "All captures")
            </div>
        )
        <div class="stage-shell" data-stage-shell="">
            <section class="canvas-stage" data-stage-canvas="" aria-label="Visual capture canvas">
                <div class="floating-hud" role="toolbar" aria-label="Stage viewport">
                    <button class="button primary" type="button" data-stage-vp="100%">"Fit"</button>
                    <button class="button" type="button" data-stage-vp="1440px">"1440px"</button>
                    <button class="button" type="button" data-stage-vp="768px">"768px"</button>
                    <button class="button" type="button" data-stage-vp="320px">"320px"</button>
                </div>
                <div class="stage-viewport-frame">
                    if *environments_differ {
                        <p class="notice">"The two runs used different capture settings. Check their environment before judging differences."</p>
                    }
                    if !run.checkpoints.is_empty() {
                        screenshots(checkpoints: &run.checkpoints, before_images: before_images, comparing: *comparing)
                    }
                    if !run.clips.is_empty() {
                        recordings(clips: &run.clips, width: width, height: height)
                    }
                    if run.checkpoints.is_empty() && run.clips.is_empty() {
                        <section class="empty">
                            <h2>"No media artifacts stored"</h2>
                            <p class="muted">"Check the inspector blocker or trigger a new capture run."</p>
                        </section>
                    }
                </div>
            </section>
            <aside class="canvas-rail" aria-label="Inspection and comparison controls">
                summary(run: run, width: width, height: height)
                if !candidates.is_empty() {
                    comparison_control(form_url: form_url, candidates: candidates, requested: requested.as_deref())
                }
                if !run.findings.is_empty() {
                    findings(items: &run.findings)
                }
                if *may_review {
                    review_form(form_url: form_url, token: &account.csrf, checkpoints: &run.checkpoints)
                }
            </aside>
        </div>
        if !run.checkpoints.is_empty() || !run.clips.is_empty() {
            <nav class="filmstrip" aria-label="Capture filmstrip">
                for checkpoint in &run.checkpoints {
                    <a class="film-thumb" href=(format!("#cp-{}", checkpoint.id)) data-film-pick="">
                        <img src=(media::url(&checkpoint.artifact.id)) alt=(&checkpoint.id) loading="lazy">
                        <strong class="mono">(&checkpoint.id)</strong>
                    </a>
                }
                for clip in &run.clips {
                    <a class="film-thumb" href=(format!("#clip-{}", clip.id)) data-film-pick="">
                        if let Some(first) = clip.frames.first() {
                            <img src=(media::url(&first.artifact.id)) alt=(&clip.id) loading="lazy">
                        }
                        <strong class="mono">"▶ " (&clip.id)</strong>
                    </a>
                }
            </nav>
        }
    })
}

/// The capture's state, environment and revision.
#[component]
async fn summary(cx: &Cx, run: &ApiCapture, width: &str, height: &str) -> Result<impl View> {
    let scheme = run.environment.color_scheme.as_deref().unwrap_or("");
    Ok(view! {
        <p class="metadata run-metadata"><span class="status">(run.status.as_str())</span> <span>"Inspection " (run.inspection.as_str())</span> if let Some(renderer) = &run.environment.renderer { <code>(renderer)</code> }</p>
        <p class="muted">
            (format!("{} × {}", width, height))
            if !scheme.is_empty() { " · " (scheme) }
            if run.environment.reduced_motion { " · Reduced motion" }
        </p>
        if let Some(revision) = &run.revision { <p>"Revision " <code>(revision)</code></p> }
        if let Some(blocker) = &run.blocker { <p class="blocker">(blocker)</p> }
    })
}

/// The choice of an earlier capture of the same scenario to compare with.
#[component]
async fn comparison_control(
    cx: &Cx,
    form_url: &str,
    candidates: &[CaptureSummary],
    requested: Option<&str>,
) -> Result<impl View> {
    Ok(view! {
        <form class="filters comparison-control" method="get" action=(form_url)>
            <label for="before">"Compare with"</label>
            <select id="before" name="before">
                <option value="">"Current run only"</option>
                for candidate in candidates {
                    <option value=(&candidate.id) data-epoch-ms=(candidate.created_at.to_string())
                        data-time-suffix=(candidate.id.chars().take(16).collect::<String>())
                        if requested == Some(candidate.id.as_str()) { selected="" }>
                        (&candidate.id)
                    </option>
                }
            </select>
            <button class="button" type="submit">"Apply"</button>
        </form>
    })
}

/// The review notes left on the capture.
#[component]
async fn findings(cx: &Cx, items: &[Finding]) -> Result<impl View> {
    let url = media::url;
    Ok(view! {
        <section>
            <h2>"Findings"</h2>
            <ul>
                for finding in items {
                    <li>
                        (&finding.note)
                        if let Some(evidence) = &finding.evidence_artifact_id {
                            " " <a href=(url(evidence))>"Evidence"</a>
                        }
                    </li>
                }
            </ul>
        </section>
    })
}

/// The capture's screenshots, each beside the earlier run's when comparing.
#[component]
async fn screenshots(
    cx: &Cx,
    checkpoints: &[Checkpoint],
    before_images: &BTreeMap<String, String>,
    comparing: bool,
) -> Result<impl View> {
    let url = media::url;
    Ok(view! {
        <section>
            <h2>"Screenshots"</h2>
            <div class=(if comparing { "compare" } else { "frames" })>
                for item in checkpoints {
                    <div id=(format!("cp-{}", item.id)) class=(if before_images.contains_key(item.id.as_str()) { "checkpoint pair" } else { "checkpoint" })>
                        if let Some(previous) = before_images.get(item.id.as_str()) {
                            <figure>
                                <a href=(previous)>
                                    <img src=(previous) alt=(format!("Previous run: {}", item.id)) loading="lazy">
                                </a>
                                <figcaption>"Previous run · " (&item.id)</figcaption>
                            </figure>
                        }
                        <figure>
                            <a href=(url(&item.artifact.id))>
                                <img src=(url(&item.artifact.id)) alt=(format!("Screenshot: {}", item.id)) loading="lazy">
                            </a>
                            <figcaption>
                                if before_images.contains_key(item.id.as_str()) { "Current run · " }
                                (&item.id)
                            </figcaption>
                        </figure>
                    </div>
                }
            </div>
        </section>
    })
}

/// The capture's recordings.
#[component]
async fn recordings(cx: &Cx, clips: &[Clip], width: &str, height: &str) -> Result<impl View> {
    Ok(view! {
        <section>
            <h2>"Recordings"</h2>
            for clip in clips {
                clip_card(clip: clip, width: width, height: height)
            }
        </section>
    })
}

/// One recording: the video, its retained frames and its analysis files.
#[component]
async fn clip_card(cx: &Cx, clip: &Clip, width: &str, height: &str) -> Result<impl View> {
    let url = media::url;
    Ok(view! {
        <article id=(format!("clip-{}", clip.id))>
            <h3>(&clip.id)</h3>
            if let Some(video) = &clip.video {
                <video controls="" playsinline="" preload="metadata" loading="lazy" src=(url(&video.id))
                    width=(width) height=(height) aria-label=(&clip.id)
                    if let Some(first) = clip.frames.first() { poster=(url(&first.artifact.id)) }></video>
            }
            if !clip.frames.is_empty() {
                <div class="frames">
                    for frame in &clip.frames {
                        <figure>
                            <a href=(url(&frame.artifact.id))>
                                <img src=(url(&frame.artifact.id)) alt=(format!("{} at {:.0} ms", clip.id, frame.target_ms)) loading="lazy">
                            </a>
                            <figcaption>
                                if clip.video.is_some() {
                                    <button type="button" data-seek=(format!("{}", frame.video_ms.unwrap_or(frame.target_ms) / 1000.0))>
                                        (format!("{:.0} ms", frame.video_ms.unwrap_or(frame.target_ms)))
                                    </button>
                                } else {
                                    (format!("{:.0} ms", frame.target_ms))
                                }
                            </figcaption>
                        </figure>
                    }
                </div>
            }
            <p class="downloads">
                if let Some(sheet) = &clip.sheet { <a href=(url(&sheet.id))>"Contact sheet"</a> }
                if let Some(analysis) = &clip.analysis { <a href=(url(&analysis.id))>"Frame analysis"</a> }
            </p>
        </article>
    })
}

/// The form that records an inspection note, optionally pointing at a screenshot.
#[component]
async fn review_form(
    cx: &Cx,
    form_url: &str,
    token: &str,
    checkpoints: &[Checkpoint],
) -> Result<impl View> {
    Ok(view! {
        <section class="surface review" id="review">
            <h2>"Record inspection"</h2>
            stack_form(action: form_url, token: token,
                <label for="note">"Observation"</label>
                <textarea id="note" name="note" required="" maxlength="2000" rows="3"></textarea>
                if !checkpoints.is_empty() {
                    <label for="evidence">"Evidence"</label>
                    <select id="evidence" name="evidence">
                        <option value="">"No checkpoint"</option>
                        for checkpoint in checkpoints {
                            <option value=(&checkpoint.artifact.id)>(&checkpoint.id)</option>
                        }
                    </select>
                }
                <button type="submit" class="button primary">"Save inspection"</button>
            )
        </section>
    })
}
