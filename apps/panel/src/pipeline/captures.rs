use super::{
    Account, Arc, AssetOption, BadgeVariant, Cx, Deserialize, PageResult, Panel, Problem,
    ProblemStatus, Response, Role, Run, StatusCode, auth, badge, data_root, db_error, pages,
    problem_record, redirect, view,
};

#[allow(clippy::too_many_arguments)]
#[expect(
    clippy::too_many_lines,
    reason = "the captures view keeps its markup and presentation branches together"
)]
pub fn render_captures<'a>(
    cx: &'a Cx,
    account: &'a Account,
    role: Role,
    _project_id: &'a str,
    base: &'a str,
    filter: &'a pages::Filter,
    runs: &'a [&'a Run],
    assets: &'a [AssetOption],
    problems: &'a [Problem],
) -> topcoat::view::Child<'a> {
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

    topcoat::view::Child::new(view! {
        cx =>
        <section class="studio-counts" aria-label="Research summary">
            <div class="metric">
                <small>"Capture runs"</small>
                <strong>(runs.len())</strong>
            </div>
            <div class="metric">
                <small>"Visual assets (shots & clips)"</small>
                <strong>(assets.len())</strong>
            </div>
            <a class=(if open_problems > 0 { "metric featured" } else { "metric" }) href=(format!("{base}/problems"))>
                <small>"Problems identified"</small>
                <strong>(open_problems)</strong>
                <span aria-hidden="true">"↗"</span>
            </a>
        </section>

        <div class="propose-toolbar">
            <div class="stage-viewports" role="group" aria-label="Capture view">
                for (slug, label) in [("all", "All research"), ("runs", "Matrix runs"), ("images", "Screenshots & sheets"), ("videos", "Interaction clips")] {
                    <a class=(if subview == slug { "button primary" } else { "button" })
                       href=(format!("{base}/captures?view={slug}&health={health}&q={}", filter.q.as_deref().unwrap_or("")))>
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
                <input id="cap-q" name="q" type="search" maxlength="100" value=(filter.q.as_deref().unwrap_or("")) placeholder="Route, viewport, scenario…">
                <button class="button" type="submit">"Filter"</button>
            </form>
        </div>

        if subview == "all" || subview == "runs" {
            <section class="surface captures-ledger">
                <div class="section-head">
                    <h2>"Research runs (" (filtered_runs.len()) ")"</h2>
                </div>
                if filtered_runs.is_empty() {
                    <div class="empty"><p>"No capture runs match this filter."</p></div>
                } else {
                    <ul class="run-list">
                        for run in &filtered_runs {
                            <li>
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
                                            <time data-epoch-ms=(run.created_at_ms.to_string())></time>
                                        </span>
                                        <span class="arrow" aria-hidden="true">"↗"</span>
                                    </a>
                                    if role != Role::Reader {
                                        <form method="post" action=(format!("{base}/problems/analyze")) class="inline-action">
                                            <input type="hidden" name="csrf" value=(&account.csrf)>
                                            <input type="hidden" name="run_key" value=(&run.key)>
                                            <button class="button" type="submit">"Extract problems"</button>
                                        </form>
                                    }
                                </div>
                            </li>
                        }
                    </ul>
                }
            </section>
        }

        if subview != "runs" {
            <section class="captures-ledger">
                <div class="section-head">
                    <h2>"Captured visual assets (" (filtered_assets.len()) ")"</h2>
                </div>
                if filtered_assets.is_empty() {
                    <div class="empty"><p>"No visual assets match this filter."</p></div>
                } else {
                    <div class="gallery image-gallery">
                        for (index, item) in filtered_assets.iter().take(120).enumerate() {
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
                                        if role != Role::Reader {
                                            <a class="link-button" href=(format!("{base}/problems?asset={}&name={}", item.url, item.name))>"Flag problem"</a>
                                        }
                                    </div>
                                </div>
                            </article>
                        }
                    </div>
                }
            </section>
        }

        if !filtered_assets.is_empty() {
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
        }
    })
}

#[derive(Deserialize)]
pub struct AnalyzeRunForm {
    pub csrf: String,
    pub run_key: String,
}

pub async fn analyze_run_action(
    panel: Arc<Panel>,
    account: Account,
    project_id: String,
    form: AnalyzeRunForm,
) -> PageResult<Response> {
    auth::check_csrf(&account, &form.csrf)?;
    if auth::role(&panel, &account, &project_id).await? == Role::Reader {
        return Err(StatusCode::FORBIDDEN);
    }
    match analyze_run(&panel, &account, &project_id, &form.run_key).await {
        Ok(()) | Err(StatusCode::NOT_FOUND) => {}
        Err(error) => return Err(error),
    }
    Ok(redirect(&format!("/projects/{project_id}/problems")))
}

pub(crate) async fn analyze_run(
    panel: &Panel,
    account: &Account,
    project_id: &str,
    run_key: &str,
) -> PageResult<()> {
    let data = data_root(panel);
    let existing = problem_record::entries(&data, project_id).unwrap_or_default();
    let manifest = crate::runs::find(panel, account, run_key)
        .await
        .map(|run| run.manifest)
        .ok()
        .filter(|manifest| manifest.project.id == project_id)
        .ok_or(StatusCode::NOT_FOUND)?;
    if manifest.findings.is_empty() {
        if let Some(first) = manifest.checkpoints.first() {
            let mut candidate = problem_record::infer_from_finding(
                project_id,
                run_key,
                &format!(
                    "[{} @ {}×{} {}] Visual inspection requested on capture '{}'",
                    first.id,
                    manifest.environment.width.unwrap_or(1440),
                    manifest.environment.height.unwrap_or(900),
                    manifest.environment.color_scheme,
                    manifest.scenario
                ),
                Some(&first.path),
            );
            candidate.status = ProblemStatus::NeedsReprocess;
            if !existing
                .iter()
                .any(|problem| problem.diagnosis == candidate.diagnosis)
            {
                problem_record::save(&data, &candidate).map_err(db_error)?;
            }
        }
    } else {
        for f in &manifest.findings {
            let candidate = problem_record::infer_from_finding(
                project_id,
                run_key,
                &f.note,
                f.evidence.as_deref(),
            );
            let rule_code = candidate
                .violated_rules
                .first()
                .map_or("", |r| r.code.as_str());
            if problem_record::is_suppressed(
                &existing,
                &candidate.route,
                rule_code,
                &candidate.title,
            ) {
                continue;
            }
            if !existing.iter().any(|p| p.diagnosis == candidate.diagnosis) {
                problem_record::save(&data, &candidate).map_err(db_error)?;
            }
        }
    }
    Ok(())
}
