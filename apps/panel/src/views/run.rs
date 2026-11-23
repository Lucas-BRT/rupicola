use crate::components::{
    display::link_button,
    form::stack_form,
    layout::{Nav, breadcrumbs, page_title, workspace},
};
use crate::{Account, PageResult, Panel, auth, media};
use rupicola_api::CaptureSummary;
use rupicola_core::capture_manifest::{Checkpoint, Clip, Finding, Manifest};
use std::collections::BTreeMap;
use topcoat::{
    Result,
    context::Cx,
    router::content::Html,
    view::{View, component, view},
};

/// What the capture page shows.
pub struct Capture<'a> {
    pub manifest: &'a Manifest,
    pub candidates: &'a [CaptureSummary],
    pub requested: Option<String>,
    pub before_images: &'a BTreeMap<String, String>,
    pub environments_differ: bool,
    pub comparing: bool,
    pub may_review: bool,
    pub width: &'a str,
    pub height: &'a str,
    pub project_url: &'a str,
    pub form_url: &'a str,
}

pub async fn render(
    cx: &Cx,
    account: &Account,
    panel: &Panel,
    data: Capture<'_>,
) -> PageResult<Html<String>> {
    let Capture {
        manifest: run,
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
    let content = view! { cx =>
        workspace(title: &run.scenario, account: account, panel: panel, active: Nav::Projects,
            breadcrumbs(
                <a href="/projects">"Projects"</a><span aria-hidden="true">"/"</span>
                <a href=(project_url)>(&run.project.label)</a>
            )
            page_title(title: &run.scenario, class: "project-masthead",
                link_button(href: format!("{project_url}/captures"), "Project captures")
            )
            summary(run: run, width: width, height: height)
            if !candidates.is_empty() {
                comparison_control(form_url: form_url, candidates: candidates, requested: requested.as_deref())
            }
            if environments_differ {
                <p class="notice">"The two runs used different capture settings. Check their environment before judging differences."</p>
            }
            if !run.findings.is_empty() {
                findings(items: &run.findings)
            }
            if !run.checkpoints.is_empty() {
                screenshots(checkpoints: &run.checkpoints, before_images: before_images, comparing: comparing)
            }
            if !run.clips.is_empty() {
                recordings(clips: &run.clips, width: width, height: height)
            }
            if may_review {
                review_form(form_url: form_url, token: &account.csrf, checkpoints: &run.checkpoints)
            }
        )
    };
    auth::page(cx, content).await
}

/// The capture's state, environment and revision.
#[component]
async fn summary(run: &Manifest, width: &str, height: &str) -> Result<impl View> {
    Ok(view! {
        <p class="metadata run-metadata"><span class="status">(&run.status)</span> <span>"Inspection " (&run.inspection)</span> <code>(&run.environment.renderer)</code></p>
        <p class="muted">
            (format!("{} × {}", width, height))
            if !run.environment.color_scheme.is_empty() { " · " (&run.environment.color_scheme) }
            if run.environment.reduced_motion { " · Reduced motion" }
        </p>
        if let Some(revision) = &run.revision { <p>"Revision " <code>(revision)</code></p> }
        if let Some(blocker) = &run.blocker { <p class="blocker">(blocker)</p> }
    })
}

/// The choice of an earlier capture of the same scenario to compare with.
#[component]
async fn comparison_control(
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
async fn findings(items: &[Finding]) -> Result<impl View> {
    let url = media::url;
    Ok(view! {
        <section>
            <h2>"Findings"</h2>
            <ul>
                for finding in items {
                    <li>
                        (&finding.note)
                        if let Some(evidence) = &finding.evidence {
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
                    <div class=(if before_images.contains_key(item.id.as_str()) { "checkpoint pair" } else { "checkpoint" })>
                        if let Some(previous) = before_images.get(item.id.as_str()) {
                            <figure>
                                <a href=(previous)>
                                    <img src=(previous) alt=(format!("Previous run: {}", item.id)) loading="lazy">
                                </a>
                                <figcaption>"Previous run · " (&item.id)</figcaption>
                            </figure>
                        }
                        <figure>
                            <a href=(url(&item.path))>
                                <img src=(url(&item.path)) alt=(format!("Screenshot: {}", item.id)) loading="lazy">
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
async fn recordings(clips: &[Clip], width: &str, height: &str) -> Result<impl View> {
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
async fn clip_card(clip: &Clip, width: &str, height: &str) -> Result<impl View> {
    let url = media::url;
    Ok(view! {
        <article>
            <h3>(&clip.id)</h3>
            if let Some(video) = &clip.video_path {
                <video controls="" playsinline="" preload="metadata" loading="lazy" src=(url(video))
                    width=(width) height=(height) aria-label=(&clip.id)
                    if let Some(first) = clip.frames.first() { poster=(url(&first.path)) }></video>
            }
            if !clip.frames.is_empty() {
                <div class="frames">
                    for frame in &clip.frames {
                        <figure>
                            <a href=(url(&frame.path))>
                                <img src=(url(&frame.path)) alt=(format!("{} at {:.0} ms", clip.id, frame.target_ms)) loading="lazy">
                            </a>
                            <figcaption>
                                if clip.video_path.is_some() {
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
                if let Some(path) = &clip.sheet_path { <a href=(url(path))>"Contact sheet"</a> }
                if let Some(path) = &clip.analysis_path { <a href=(url(path))>"Frame analysis"</a> }
            </p>
        </article>
    })
}

/// The form that records an inspection note, optionally pointing at a screenshot.
#[component]
async fn review_form(form_url: &str, token: &str, checkpoints: &[Checkpoint]) -> Result<impl View> {
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
                            <option value=(&checkpoint.path)>(&checkpoint.id)</option>
                        }
                    </select>
                }
                <button type="submit" class="button primary">"Save inspection"</button>
            )
        </section>
    })
}
