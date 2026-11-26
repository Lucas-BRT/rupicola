#![allow(
    clippy::too_many_lines,
    reason = "the captures view keeps its filtering and layout together"
)]

use super::{AssetOption, BadgeVariant, Cx, Problem, ProblemStatus, Role, Run, badge, view};
use crate::Account;
use crate::components::{
    controls::{SegmentedOption, SelectOption, query_url, segmented_control, select_menu},
    display::{counts, empty, metric, moment, section_head, stat},
    form::csrf,
};
use crate::project::Filter;
use rupicola_api::Topic;
use std::collections::BTreeSet;
use topcoat::{
    Result,
    context::context,
    view::{View, component},
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
    topics: Vec<Topic>,
    topic_run_ids: Option<BTreeSet<String>>,
) -> Result<impl View> {
    let account = context::<Account>(cx);
    Ok(view! {
        let subview = filter.view.as_deref().unwrap_or("all");
        let health = filter.health.as_deref().unwrap_or("");
        let topic_id = filter.topic.as_deref().unwrap_or("");
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

        let filtered_runs: Vec<&Run> = {
            let mut list: Vec<&Run> = runs
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
                    let matches_topic = topic_run_ids
                        .as_ref()
                        .is_none_or(|ids| ids.contains(&r.key));
                    matches_health && matches_topic && filter.accepts(r)
                })
                .collect();
            filter.sort_runs(&mut list);
            list
        };

        let filtered_assets: Vec<&AssetOption> = {
            let mut list: Vec<&AssetOption> = assets
                .iter()
                .filter(|a| {
                    let has_prob = problem_urls.contains(a.url.as_str())
                        || problem_runs.contains(a.run_key.as_str());
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
                    let matches_topic = topic_run_ids
                        .as_ref()
                        .is_none_or(|ids| ids.contains(&a.run_key));
                    matches_health && matches_view && matches_q && matches_topic
                })
                .collect();
            if filter.sort_key() == "oldest" {
                list.reverse();
            }
            list
        };

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
        <div id="captures-region" class="skel-stack">
            capture_toolbar(base: base, subview: subview, health: health, topic: topic_id, query: filter.q.as_deref().unwrap_or(""), sort: filter.sort_key())
            if !topics.is_empty() || role != Role::Reader {
                topics_bar(
                    base: base,
                    topics: &topics,
                    active_topic: topic_id,
                    subview: subview,
                    health: health,
                    query: filter.q.as_deref().unwrap_or(""),
                    sort: filter.sort_key(),
                    can_create: role != Role::Reader,
                    token: &account.csrf,
                )
            }
            <div class="skel-stack" data-swap-body="rows">
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
            </div>
        </div>
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
    topic: &str,
    query: &str,
    sort: &str,
) -> Result<impl View> {
    let captures_url = format!("{base}/captures");
    let sort_param = if sort == "newest" { "" } else { sort };
    let view_tabs = [
        ("all", "All research"),
        ("runs", "Matrix runs"),
        ("images", "Screenshots"),
        ("videos", "Clips"),
    ]
    .into_iter()
    .map(|(slug, label)| {
        let view_param = if slug == "all" { "" } else { slug };
        let href = query_url(
            &captures_url,
            &[
                ("view", view_param),
                ("health", health),
                ("topic", topic),
                ("q", query),
                ("sort", sort_param),
            ],
        );
        SegmentedOption::new(label, href, subview == slug)
    })
    .collect::<Vec<_>>();
    let health_options = [
        SelectOption::new("", "All captured screens"),
        SelectOption::new("clean", "Clean / verified only"),
        SelectOption::new("problems", "With problems only"),
        SelectOption::new("unanalyzed", "Unanalyzed"),
    ];
    let sort_options = [
        SelectOption::new("newest", "Newest first"),
        SelectOption::new("oldest", "Oldest first"),
        SelectOption::new("scenario", "Scenario A–Z"),
        SelectOption::new("findings", "Most findings"),
    ];
    Ok(view! {
        <div class="propose-toolbar">
            segmented_control(
                label: "Capture view",
                options: &view_tabs,
                swap_target: "captures-region",
            )
            <form class="filters" method="get" action=(&captures_url) data-swap-target="captures-region">
                <input type="hidden" name="view" value=(if subview == "all" { "" } else { subview }) data-swap-meta="view">
                if !topic.is_empty() {
                    <input type="hidden" name="topic" value=(topic)>
                }
                select_menu(
                    id: "cap-health",
                    name: "health",
                    label: "Screen health",
                    selected: health,
                    options: &health_options,
                    auto_submit: true,
                    compact: true,
                )
                select_menu(
                    id: "cap-sort",
                    name: "sort",
                    label: "Sort captures",
                    selected: sort,
                    options: &sort_options,
                    auto_submit: true,
                    compact: true,
                )
                <label class="sr-only" for="cap-q">"Search"</label>
                <input id="cap-q" name="q" type="search" maxlength="100" value=(query) placeholder="Route, viewport, scenario…">
                <button class="button" type="submit">"Filter"</button>
            </form>
        </div>
    })
}

/// Topic filter chips and inline topic creation form.
#[component]
async fn topics_bar(
    cx: &Cx,
    base: &str,
    topics: &[Topic],
    active_topic: &str,
    subview: &str,
    health: &str,
    query: &str,
    sort: &str,
    can_create: bool,
    token: &str,
) -> Result<impl View> {
    let captures_url = format!("{base}/captures");
    let view_param = if subview == "all" { "" } else { subview };
    let sort_param = if sort == "newest" { "" } else { sort };
    let all_href = query_url(
        &captures_url,
        &[
            ("view", view_param),
            ("health", health),
            ("q", query),
            ("sort", sort_param),
        ],
    );
    Ok(view! {
        <div class="propose-toolbar" aria-label="Topics">
            <nav class="filters" aria-label="Filter by topic">
                <a
                    class=(if active_topic.is_empty() { "button primary" } else { "button" })
                    href=(all_href)
                    data-swap-target="captures-region"
                >
                    "All topics"
                </a>
                for t in topics {
                    let href = query_url(
                        &captures_url,
                        &[
                            ("view", view_param),
                            ("health", health),
                            ("topic", t.id.as_str()),
                            ("q", query),
                            ("sort", sort_param),
                        ],
                    );
                    <a
                        class=(if active_topic == t.id { "button primary" } else { "button" })
                        href=(href)
                        data-swap-target="captures-region"
                    >
                        (format!("{} ({})", t.path, t.captures))
                    </a>
                }
            </nav>
            if can_create {
                <form class="filters" method="post" action=(&captures_url)>
                    csrf(token: token)
                    <label class="sr-only" for="new-topic-path">"New topic path"</label>
                    <input id="new-topic-path" name="path" type="text" required="" maxlength="120" placeholder="auth/login">
                    <button class="button" type="submit">"New topic"</button>
                </form>
            }
        </div>
    })
}

fn run_env_summary(run: &Run) -> String {
    let env = &run.capture.environment;
    let mut parts = Vec::new();
    if let (Some(w), Some(h)) = (env.width, env.height) {
        parts.push(format!("{w}×{h}"));
    }
    if let Some(scheme) = &env.color_scheme {
        parts.push(scheme.clone());
    }
    if parts.is_empty() {
        run.key.clone()
    } else {
        parts.join(" · ")
    }
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
                    <small>(run_env_summary(run))</small>
                </span>
                <span class="run-state">
                    let finding_count = run.capture.findings.len();
                    let status_label = if run.status == "blocked" {
                        "Blocked".to_owned()
                    } else if problem_runs.contains(run.key.as_str()) {
                        "Has problems".to_owned()
                    } else if finding_count == 1 {
                        "1 finding".to_owned()
                    } else if finding_count > 1 {
                        format!("{finding_count} findings")
                    } else if run.inspection == "reviewed" {
                        "Clean · Verified".to_owned()
                    } else {
                        "Unanalyzed".to_owned()
                    };
                    badge(
                        variant: if problem_runs.contains(run.key.as_str()) || run.status == "blocked" {
                            BadgeVariant::Destructive
                        } else if finding_count > 0 {
                            BadgeVariant::Primary
                        } else {
                            BadgeVariant::Secondary
                        },
                        (status_label)
                    )
                    moment(epoch_ms: run.created_at_ms)
                </span>
                <span class="arrow" aria-hidden="true">"↗"</span>
            </a>
            if can_extract {
                <form method="post" action=(format!("{base}/problems/analyze")) class="inline-action">
                    csrf(token: token)
                    <input type="hidden" name="run_key" value=(&run.key)>
                    <button class="button button-ghost" type="submit" aria-label="Extract problems from capture">"Extract"</button>
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
