#![allow(
    clippy::too_many_lines,
    reason = "the captures view keeps its filtering and layout together"
)]

use super::{AssetOption, BadgeVariant, Cx, Problem, ProblemStatus, Role, Run, badge, view};
use crate::Account;
use crate::components::{
    display::{counts, empty, metric, moment, section_head, stat},
    form::{csrf, field, select_field, stack_form},
};
use crate::project::Filter;
use std::collections::BTreeSet;
use topcoat::{
    Result,
    context::context,
    view::{View, attributes, component},
};

/// The captures tab's content.
#[component]
pub async fn captures_tab(
    cx: &Cx,
    role: Role,
    base: &str,
    filter: Filter,
    runs: Vec<&Run>,
    assets: Vec<AssetOption>,
    problems: Vec<Problem>,
) -> Result<impl View> {
    let account = context::<Account>(cx);
    Ok(view! {
        let subview = filter.view.as_deref().unwrap_or("all");
        let health = filter.health.as_deref().unwrap_or("");
        let q = filter.q.as_deref().unwrap_or("").trim().to_lowercase();

        let problem_urls: std::collections::BTreeSet<&str> = problems
            .iter()
            .filter(|p| {
                !matches!(
                    p.status,
                    ProblemStatus::Intentional | ProblemStatus::FalsePositive
                )
            })
            .flat_map(|p| p.evidence_assets.iter().map(|a| a.url.as_str()))
            .collect();
        let problem_runs: std::collections::BTreeSet<&str> = problems
            .iter()
            .filter(|p| {
                !matches!(
                    p.status,
                    ProblemStatus::Intentional | ProblemStatus::FalsePositive
                )
            })
            .flat_map(|p| p.evidence_assets.iter().map(|a| a.run_key.as_str()))
            .collect();

        let filtered_runs: Vec<&Run> = runs
            .iter()
            .copied()
            .filter(|r| {
                let has_prob = problem_runs.contains(r.key.as_str()) || r.status == "blocked";
                let matches_health = match health {
                    "problems" => has_prob,
                    "clean" => !has_prob && r.inspection == "reviewed",
                    "unanalyzed" => r.inspection == "pending",
                    _ => true,
                };
                matches_health && filter.accepts(r)
            })
            .collect();

        let filtered_assets: Vec<&AssetOption> = assets
            .iter()
            .filter(|a| {
                let has_prob =
                    problem_urls.contains(a.url.as_str()) || problem_runs.contains(a.run_key.as_str());
                let matches_health = match health {
                    "problems" => has_prob,
                    "clean" => !has_prob,
                    _ => true,
                };
                let matches_view = match subview {
                    "images" => a.kind != "Recording",
                    "videos" => a.kind == "Recording",
                    _ => true,
                };
                let matches_q = q.is_empty()
                    || a.name.to_lowercase().contains(&q)
                    || a.scenario.to_lowercase().contains(&q);
                matches_health && matches_view && matches_q
            })
            .collect();

        let open_problems = problems
            .iter()
            .filter(|p| {
                matches!(
                    p.status,
                    ProblemStatus::Open | ProblemStatus::NeedsReprocess
                )
            })
            .count();

        counts(label: "Research summary",
            stat(value: runs.len(), "Capture runs")
            stat(value: assets.len(), "Visual assets (shots & clips)")
            metric(href: format!("{base}/problems"), value: open_problems, featured: open_problems > 0, "Problems identified")
        )
        if role != Role::Reader {
            run_capture_form(base: base, token: &account.csrf)
        }
        capture_toolbar(base: base, subview: subview, health: health, query: filter.q.as_deref().unwrap_or(""))
        if subview == "all" || subview == "runs" {
            <section class="surface captures-ledger">
                section_head(title: format!("Research runs ({})", filtered_runs.len()))
                if filtered_runs.is_empty() {
                    empty(message: "No capture runs match this filter.")
                } else {
                    <ul class="run-list">
                        for run in &filtered_runs {
                            <li>
                                run_row(run: run, base: base, problem_runs: &problem_runs, can_extract: role != Role::Reader, token: &account.csrf)
                            </li>
                        }
                    </ul>
                }
            </section>
        }
        if subview != "runs" {
            <section class="captures-ledger">
                section_head(title: format!("Captured visual assets ({})", filtered_assets.len()))
                if filtered_assets.is_empty() {
                    empty(message: "No visual assets match this filter.")
                } else {
                    <div class="gallery image-gallery">
                        for (index, item) in filtered_assets.iter().take(120).enumerate() {
                            asset_card(item: item, index: index, base: base, problem_urls: &problem_urls, can_flag: role != Role::Reader)
                        }
                    </div>
                }
            </section>
        }
        if !filtered_assets.is_empty() {
            image_dialog()
        }
    })
}

/// The view switch and the health and search filter.
#[component]
async fn capture_toolbar(
    cx: &Cx,
    base: &str,
    subview: &str,
    health: &str,
    query: &str,
) -> Result<impl View> {
    Ok(view! {
        <div class="propose-toolbar">
            <div class="stage-viewports" role="group" aria-label="Capture view">
                for (slug, label) in [("all", "All research"), ("runs", "Matrix runs"), ("images", "Screenshots & sheets"), ("videos", "Interaction clips")] {
                    <a class=(if subview == slug { "button primary" } else { "button" })
                       href=(format!("{base}/captures?view={slug}&health={health}&q={query}"))>
                        (label)
                    </a>
                }
            </div>
            <form class="filters" method="get" action=(format!("{base}/captures"))>
                <input type="hidden" name="view" value=(subview)>
                <label for="cap-health">"Screen health"</label>
                <select id="cap-health" name="health">
                    for (val, text) in [("", "All captured screens"), ("clean", "Clean / verified only"), ("problems", "With problems only"), ("unanalyzed", "Unanalyzed")] {
                        <option value=(val) if health == val { selected="" }>(text)</option>
                    }
                </select>
                <label for="cap-q">"Search"</label>
                <input id="cap-q" name="q" type="search" maxlength="100" value=(query) placeholder="Route, viewport, scenario…">
                <button class="button" type="submit">"Filter"</button>
            </form>
        </div>
    })
}

/// One capture run, with the button that extracts problems from it.
#[component]
async fn run_row(
    cx: &Cx,
    run: &Run,
    base: &str,
    problem_runs: &BTreeSet<&str>,
    can_extract: bool,
    token: &str,
) -> Result<impl View> {
    Ok(view! {
        <div class="run-row">
            <a class="run-link" href=(format!("/run/{}", run.key))>
                <span class="run-title">
                    <strong>(&run.scenario)</strong>
                    <small>(&run.project) " · " (&run.key)</small>
                </span>
                <span class="run-state">
                    badge(
                        variant: if problem_runs.contains(run.key.as_str()) || run.status == "blocked" {
                            BadgeVariant::Destructive
                        } else {
                            BadgeVariant::Secondary
                        },
                        (if run.status == "blocked" {
                            "Blocked"
                        } else if problem_runs.contains(run.key.as_str()) {
                            "Has problems"
                        } else if run.inspection == "reviewed" {
                            "Clean · Verified"
                        } else {
                            "Unanalyzed"
                        })
                    )
                    moment(epoch_ms: run.created_at_ms)
                </span>
                <span class="arrow" aria-hidden="true">"↗"</span>
            </a>
            if can_extract {
                <form method="post" action=(format!("{base}/problems/analyze")) class="inline-action">
                    csrf(token: token)
                    <input type="hidden" name="run_key" value=(&run.key)>
                    <button class="button" type="submit">"Extract problems"</button>
                </form>
            }
        </div>
    })
}

/// One captured screenshot, sheet or recording.
#[component]
async fn asset_card(
    cx: &Cx,
    item: &AssetOption,
    index: usize,
    base: &str,
    problem_urls: &BTreeSet<&str>,
    can_flag: bool,
) -> Result<impl View> {
    Ok(view! {
        <article class="asset">
            if item.kind != "Recording" {
                <a class="asset-preview" href=(&item.url) data-viewer=(&item.name)>
                    <img src=(&item.url) alt=(format!("{} · {}", item.name, item.scenario))
                        if index >= 4 { loading="lazy" }>
                </a>
            } else {
                <a class="asset-preview video-preview" href=(format!("/run/{}", item.run_key)) aria-label=(format!("Open recording {}", item.name))>
                    <span aria-hidden="true">"▶"</span>
                </a>
            }
            <div class="asset-caption">
                <div class="propose-badges">
                    <small>(item.kind)</small>
                    if problem_urls.contains(item.url.as_str()) {
                        badge(variant: BadgeVariant::Destructive, "Problem linked")
                    } else {
                        badge(variant: BadgeVariant::Secondary, "Clean")
                    }
                </div>
                <strong>(&item.name)</strong>
                <div class="propose-foot">
                    <a href=(format!("/run/{}", item.run_key))>(&item.scenario)</a>
                    if can_flag {
                        <a class="link-button" href=(format!("{base}/problems?asset={}&name={}", item.url, item.name))>"Flag problem"</a>
                    }
                </div>
            </div>
        </article>
    })
}

/// Form for triggering a server-side capture run.
#[component]
async fn run_capture_form(cx: &Cx, base: &str, token: &str) -> Result<impl View> {
    Ok(view! {
        <details class="surface panel-padding">
            <summary>"Run capture"</summary>
            stack_form(action: format!("{base}/captures/run"), token: token,
                <div class="form-row-2">
                    <div>
                        field(id: "cap-scenario", name: "scenario", label: "Scenario",
                            attrs: attributes! { placeholder="home" maxlength="120" })
                    </div>
                    <div>
                        field(id: "cap-url", name: "url", label: "URL",
                            attrs: attributes! { type="url" placeholder="http://127.0.0.1:3000" required="" })
                    </div>
                </div>
                <div class="form-row-3">
                    <div>
                        field(id: "cap-width", name: "width", label: "Width",
                            attrs: attributes! { type="number" min="1" placeholder="1440" })
                    </div>
                    <div>
                        field(id: "cap-height", name: "height", label: "Height",
                            attrs: attributes! { type="number" min="1" placeholder="900" })
                    </div>
                    <div>
                        select_field(id: "cap-scheme", name: "color_scheme", label: "Color scheme",
                            <option value="">"Default"</option>
                            <option value="light">"Light"</option>
                            <option value="dark">"Dark"</option>
                        )
                    </div>
                </div>
                <button type="submit" class="button primary">"Start run"</button>
            )
        </details>
    })
}

/// The dialog that shows a screenshot at full size.
#[component]
async fn image_dialog(cx: &Cx) -> Result<impl View> {
    Ok(view! {
        <dialog id="image-viewer" aria-labelledby="image-title">
            <div class="section-head">
                <h2 id="image-title">"Image"</h2>
                <form method="dialog"><button class="button" type="submit" autofocus="">"Close"</button></form>
            </div>
            <img alt="">
            <div class="section-head">
                <div class="filters">
                    <button type="button" class="button" data-image-prev="">"Previous"</button>
                    <button type="button" class="button" data-image-next="">"Next"</button>
                </div>
                <div class="filters">
                    <a data-image-original="" href=".">"Open original"</a>
                </div>
            </div>
        </dialog>
    })
}
