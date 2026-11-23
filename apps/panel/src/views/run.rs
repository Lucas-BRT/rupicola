use crate::{Account, PageResult, Panel, auth, media};
use rupicola_api::CaptureSummary;
use rupicola_core::capture_manifest::Manifest;
use std::collections::BTreeMap;
use topcoat::{context::Cx, router::content::Html, view::view};

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

#[expect(
    clippy::too_many_lines,
    reason = "the capture detail view keeps its markup and presentation branches together"
)]
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
    let url = media::url;
    let content = shell!(cx, &run.scenario, account, panel, "projects", {
                    <nav class="breadcrumbs" aria-label="Breadcrumb">
                      <a href="/projects">"Projects"</a><span aria-hidden="true">"/"</span>
                      <a href=(project_url)>(&run.project.label)</a>
                    </nav>
                    <header class="page-title project-masthead">
                        <h1>(&run.scenario)</h1>
                        <a class="button" href=(format!("{project_url}/captures"))>"Project captures"</a>
                    </header>
                    <p class="metadata run-metadata"><span class="status">(&run.status)</span> <span>"Inspection " (&run.inspection)</span> <code>(&run.environment.renderer)</code></p>
                    <p class="muted">
                        (format!("{} × {}", width, height))
                        if !run.environment.color_scheme.is_empty() { " · " (&run.environment.color_scheme) }
                        if run.environment.reduced_motion { " · Reduced motion" }
                    </p>
                    if let Some(revision) = &run.revision { <p>"Revision " <code>(revision)</code></p> }
                    if let Some(blocker) = &run.blocker { <p class="blocker">(blocker)</p> }
                    if !candidates.is_empty() {
                        <form class="filters comparison-control" method="get" action=(form_url)>
                            <label for="before">"Compare with"</label>
                            <select id="before" name="before">
                                <option value="">"Current run only"</option>
                                for candidate in candidates {
                                    <option value=(&candidate.id) data-epoch-ms=(candidate.created_at.to_string())
                                        data-time-suffix=(candidate.id.chars().take(16).collect::<String>())
                                        if requested.as_ref() == Some(&candidate.id) { selected="" }>
                                        (&candidate.id)
                                    </option>
                                }
                            </select>
                            <button class="button" type="submit">"Apply"</button>
                        </form>
                    }
                    if environments_differ {
                        <p class="notice">"The two runs used different capture settings. Check their environment before judging differences."</p>
                    }
                    if !run.findings.is_empty() {
                        <section>
                            <h2>"Findings"</h2>
                            <ul>
                                for finding in &run.findings {
                                    <li>
                                        (&finding.note)
                                        if let Some(evidence) = &finding.evidence {
                                            " " <a href=(url(evidence))>"Evidence"</a>
                                        }
                                    </li>
                                }
                            </ul>
                        </section>
                    }
                    if !run.checkpoints.is_empty() {
                        <section>
                            <h2>"Screenshots"</h2>
                            <div class=(if comparing { "compare" } else { "frames" })>
                                for item in &run.checkpoints {
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
                    }
                    if !run.clips.is_empty() {
                        <section>
                            <h2>"Recordings"</h2>
                            for clip in &run.clips {
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
                            }
                        </section>
                    }
                    if may_review {
                        <section class="surface review" id="review">
                            <h2>"Record inspection"</h2>
                            <form method="post" action=(form_url) class="stack-form">
                                <input type="hidden" name="csrf" value=(&account.csrf)>
                                <label for="note">"Observation"</label>
                                <textarea id="note" name="note" required="" maxlength="2000" rows="3"></textarea>
                                if !run.checkpoints.is_empty() {
                                    <label for="evidence">"Evidence"</label>
                                    <select id="evidence" name="evidence">
                                        <option value="">"No checkpoint"</option>
                                        for checkpoint in &run.checkpoints {
                                            <option value=(&checkpoint.path)>(&checkpoint.id)</option>
                                        }
                                    </select>
                                }
                                <button type="submit" class="button primary">"Save inspection"</button>
                            </form>
                        </section>
                    }
    });
    auth::page(cx, content).await
}
