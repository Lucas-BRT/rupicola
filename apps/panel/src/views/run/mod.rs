mod inspector;
use inspector::inspector_rail;

use crate::components::{
    display::link_button,
    layout::{breadcrumbs, page_title},
};
use crate::media;
use rupicola_api::{
    Capture as ApiCapture, CaptureDiff, CaptureSummary, Checkpoint, Citations, Clip, LineageGraph,
    MotionDiffReport, ProblemSummary, Topic,
};
use std::collections::BTreeMap;
use topcoat::{
    Result,
    context::Cx,
    view::{View, component, view},
};

/// What the capture page shows.
pub struct Capture {
    pub record: ApiCapture,
    pub project_label: String,
    pub candidates: Vec<CaptureSummary>,
    pub requested: Option<String>,
    pub before_images: BTreeMap<String, String>,
    pub before_artifact_ids: BTreeMap<String, String>,
    pub environments_differ: bool,
    pub comparing: bool,
    pub may_review: bool,
    pub width: String,
    pub height: String,
    pub project_url: String,
    pub form_url: String,
    pub citations: Option<Citations>,
    pub lineage: Option<LineageGraph>,
    pub diff: Option<CaptureDiff>,
    pub diff_artifact_id: Option<String>,
    pub motion_report: Option<MotionDiffReport>,
    pub topics: Vec<Topic>,
    pub open_problems: Vec<ProblemSummary>,
}

/// The capture page's content.
#[component]
pub async fn run_page(cx: &Cx, data: &Capture) -> Result<impl View> {
    let Capture {
        record: run,
        project_label,
        before_images,
        environments_differ,
        comparing,
        width,
        height,
        project_url,
        ..
    } = data;
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
            inspector_rail(data: data)
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
