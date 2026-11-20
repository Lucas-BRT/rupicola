use super::{
    Account, Arc, BTreeMap, Checkpoint, Connection, Cx, Deserialize, Html, IntoResponse, Manifest,
    PageResult, Panel, PathBuf, Response, Run, StatusCode, TransactionBehavior, Value, auth,
    db_error, design_record, fix_record, fs, media, pipeline, problem_record, proposal_record,
    proposes, read_index, read_manifest, redirect, run_dir, safe_name, storage, view,
};
use crate::components::badge::{BadgeVariant, badge};
use auth::Role;
use std::io::Write;

async fn visible_runs(panel: &Panel, account: &Account) -> PageResult<Vec<Run>> {
    let mut runs = read_index(panel).await.map_err(db_error)?;
    if !account.owner {
        let allowed = auth::readable_projects(panel, account).await?;
        runs.retain(|run| allowed.contains(&run.project_id));
    }
    Ok(runs)
}

async fn catalog(
    panel: &Panel,
    account: &Account,
    runs: &[Run],
) -> PageResult<BTreeMap<String, String>> {
    let db = panel.index.lock().await;
    let (sql, params) = if account.owner {
        ("SELECT id, label FROM projects", Vec::<Value>::new())
    } else {
        (
            "SELECT p.id, p.label FROM projects p JOIN members m ON p.id = m.project WHERE m.handle = ?",
            vec![Value::Text(account.handle.clone())],
        )
    };
    let mut rows = db.query(sql, params).await.map_err(db_error)?;
    let mut projects = BTreeMap::new();
    while let Some(row) = rows.next().await.map_err(db_error)? {
        projects.insert(row.get(0).map_err(db_error)?, row.get(1).map_err(db_error)?);
    }
    if account.owner {
        match fs::read_dir(&panel.design) {
            Ok(dirs) => {
                for dir in dirs {
                    let path = dir.map_err(db_error)?.path();
                    if let Ok(project) = design_record::project(&path)
                        && safe_name(&project.id)
                        && path.file_name().and_then(|name| name.to_str()) == Some(&project.id)
                    {
                        projects.entry(project.id).or_insert(project.label);
                    }
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(db_error(error)),
        }
    }
    for run in runs {
        if safe_name(&run.project_id) {
            projects
                .entry(run.project_id.clone())
                .or_insert_with(|| run.project.clone());
        }
    }
    Ok(projects)
}

fn run_list<'a>(cx: &'a Cx, runs: &'a [&'a Run], empty: &'a str) -> topcoat::view::Child<'a> {
    topcoat::view::Child::new(view! {
        cx =>
        if runs.is_empty() {
            <div class="empty"><p>(empty)</p></div>
        } else {
            <ul class="run-list">
                for run in runs {
                    <li>
                        <a href=(format!("/run/{}", run.key))>
                            <span class="run-title"><strong>(&run.scenario)</strong><small>(&run.project)</small></span>
                            <span class="run-state">
                                badge(
                                    variant: if run.status == "blocked" { BadgeVariant::Destructive } else { BadgeVariant::Secondary },
                                    (if run.status == "blocked" { "Blocked" } else if run.inspection == "reviewed" { "Reviewed" } else { "Needs review" })
                                )
                                <time data-epoch-ms=(run.created_at_ms.to_string())></time>
                            </span>
                            <span class="arrow" aria-hidden="true">"↗"</span>
                        </a>
                    </li>
                }
            </ul>
        }
    })
}

#[derive(Default, Deserialize)]
pub struct Filter {
    pub project: Option<String>,
    pub state: Option<String>,
    pub q: Option<String>,
    pub kind: Option<String>,
    pub status: Option<String>,
    pub view: Option<String>,
    pub health: Option<String>,
    pub asset: Option<String>,
    pub name: Option<String>,
}

impl Filter {
    pub fn accepts(&self, run: &Run) -> bool {
        let state = self.state.as_deref().unwrap_or("");
        let query = self.q.as_deref().unwrap_or("").trim().to_lowercase();
        self.project
            .as_deref()
            .is_none_or(|p| p.is_empty() || p == run.project_id)
            && match state {
                "pending" => run.inspection == "pending" && run.status == "complete",
                "blocked" => run.status == "blocked",
                "reviewed" => run.inspection == "reviewed",
                _ => true,
            }
            && (query.is_empty()
                || run.scenario.to_lowercase().contains(&query)
                || run.project.to_lowercase().contains(&query))
    }
}

fn pick_hero_checkpoint(checkpoints: &[Checkpoint]) -> Option<&Checkpoint> {
    checkpoints
        .iter()
        .find(|c| {
            c.id != "matrix-contact-sheet"
                && (c.id.contains("1440x900") || c.id.starts_with("desktop"))
        })
        .or_else(|| {
            checkpoints
                .iter()
                .find(|c| c.id != "matrix-contact-sheet" && !c.id.contains("320x"))
        })
        .or_else(|| checkpoints.first())
}

fn project_design(panel: &Panel, id: &str) -> Vec<design_record::Entry> {
    let root = panel.design.join(id);
    if design_record::project(&root).is_ok_and(|project| project.id == id) {
        design_record::entries(&root).unwrap_or_default()
    } else {
        Vec::new()
    }
}

pub async fn dashboard(
    panel: Arc<Panel>,
    account: Account,
    filter: Filter,
) -> PageResult<Html<String>> {
    let runs = visible_runs(&panel, &account).await?;
    let projects = catalog(&panel, &account, &runs).await?;
    let project_count = projects.len();
    let mut activity = Vec::new();
    for (id, label) in &projects {
        let records = project_design(&panel, id);
        let superseded = design_record::superseded(&records);
        activity.extend(
            records
                .iter()
                .filter(|entry| !superseded.contains(entry.id.as_str()))
                .take(8)
                .map(|entry| {
                    (
                        id,
                        label,
                        entry.timestamp_ms,
                        entry.title.clone(),
                        entry.kind,
                        entry.status,
                    )
                }),
        );
    }
    activity.sort_by_key(|entry| std::cmp::Reverse(entry.2));
    activity.truncate(8);
    let pending = runs
        .iter()
        .filter(|r| r.status == "complete" && r.inspection == "pending")
        .count();
    let blocked = runs.iter().filter(|r| r.status == "blocked").count();
    let items: Vec<_> = runs.iter().filter(|r| filter.accepts(r)).take(30).collect();
    let latest_evidence = runs
        .iter()
        .filter(|run| run.status == "complete")
        .find_map(|run| {
            let manifest = read_run(&panel, &run.key)?;
            let checkpoint = pick_hero_checkpoint(&manifest.checkpoints)?;
            Some((
                format!("/run/{}", run.key),
                format!("/media/{}/{}", run.key, checkpoint.path),
                &run.scenario,
            ))
        });
    let cx = &Cx::default();
    let list = run_list(cx, &items, "No matching captures.");
    let content = shell!(cx, "Dashboard", account, panel, "dashboard", {
        <header class="page-title studio-heading"><h1>"Dashboard"</h1><a class="button primary" href="/projects">"Browse projects"</a></header>
        <div class="studio-board">
        <div class="studio-evidence">
        if let Some((href, image, scenario)) = &latest_evidence {
            <section class="studio-proof">
                <h2>"Latest evidence"</h2>
                <a href=(href)>
                    <div class="studio-proof-image"><img src=(image) alt="" decoding="async"></div>
                    <span>(scenario) <span aria-hidden="true">"↗"</span></span>
                </a>
            </section>
        }
        <section class="studio-counts" aria-label="Workspace activity">
            <a class=(if pending > 0 { "metric featured" } else { "metric" }) href="/?state=pending"><small>"Needs review"</small><strong>(pending)</strong><span aria-hidden="true">"↗"</span></a>
            <a class="metric" href="/?state=blocked"><small>"Blocked captures"</small><strong>(blocked)</strong><span aria-hidden="true">"↗"</span></a>
            <a class="metric" href="/projects"><small>"Projects"</small><strong>(project_count)</strong><span aria-hidden="true">"↗"</span></a>
        </section>
        <section class="surface studio-queue">
            <div class="section-head">
                <h2>"Capture queue"</h2>
                <form class="filters" method="get" action="/">
                    <label for="state">"Status"</label>
                    <select id="state" name="state">
                        for (value, text) in [("", "All captures"), ("pending", "Needs review"), ("blocked", "Blocked"), ("reviewed", "Reviewed")] {
                            <option value=(value) if filter.state.as_deref().unwrap_or("") == value { selected="" }>(text)</option>
                        }
                    </select>
                    <button class="button" type="submit">"Filter"</button>
                </form>
            </div>
            (list)
        </section>
        </div>
        <section class="studio-ledger">
            <h2>"Recent design activity"</h2>
            if activity.is_empty() {
                <p>"No design records yet."</p>
                <a href="/projects">"Open projects"</a>
            } else {
                <ol class="studio-activity">
                    for (id, label, timestamp, title, kind, status) in &activity {
                        <li><small class="studio-kind">(kind.label())</small>
                            <a class="studio-record" href=(format!("/projects/{id}/design"))>(title)</a>
                            <p class="muted">(label) " · " (status.label()) " · "
                                <time data-epoch-ms=(timestamp.to_string())></time>
                            </p>
                        </li>
                    }
                </ol>
            }
        </section>
        </div>
    });
    auth::page(cx, content).await
}

pub async fn projects(
    panel: Arc<Panel>,
    account: Account,
    filter: Filter,
) -> PageResult<Html<String>> {
    let runs = visible_runs(&panel, &account).await?;
    let projects = catalog(&panel, &account, &runs).await?;
    let search = filter.q.as_deref().unwrap_or("").trim().to_lowercase();
    let empty_catalog = projects.is_empty();
    let total_projects = projects.len();
    let visible: Vec<_> = projects
        .iter()
        .filter(|(id, label)| {
            search.is_empty() || id.contains(&search) || label.to_lowercase().contains(&search)
        })
        .collect();
    let summaries: BTreeMap<_, _> = visible
        .iter()
        .filter_map(|(id, _)| {
            project_design(&panel, id).into_iter().next().map(|entry| {
                (
                    (*id).clone(),
                    format!("{} · {}", entry.title, entry.status.label()),
                )
            })
        })
        .collect();
    let cx = &Cx::default();
    let content = shell!(cx, "Projects", account, panel, "projects", {
        <div class="page-title studio-heading"><div><h1>"Projects"</h1><p class="muted">(visible.len()) " of " (total_projects) (if total_projects == 1 { " project" } else { " projects" })</p></div>
            <form class="filters" method="get" action="/projects">
                <label for="q">"Search"</label><input type="search" id="q" name="q" value=(filter.q.as_deref().unwrap_or("")) maxlength="100">
                <button type="submit" class="button">"Search"</button>
            </form>
        </div>
        if visible.is_empty() { <div class="empty"><p>(if search.is_empty() { "No projects yet." } else { "No projects match that search." })</p></div> }
        <div class="project-index">
            for (id, label) in visible {
                <a class="project-card" href=(format!("/projects/{id}"))>
                    <div class="project-emblem" aria-hidden="true">"▱"</div>
                    <div><h2>(label)</h2>if id != label { <small>(id)</small> }</div>
                    <span class="project-count">
                        if let Some(summary) = summaries.get(id) {
                            (summary)
                        } else { "No design records yet" }
                    </span>
                    <span class="arrow" aria-hidden="true">"↗"</span>
                </a>
            }
        </div>
        if account.owner {
            <details class="surface new-project" if empty_catalog { open="" }>
                <summary>"Create a project"</summary>
                <form class="stack-form" method="post" action="/projects">
                    <input type="hidden" name="csrf" value=(&account.csrf)>
                    <label for="label">"Project name"</label><input id="label" name="label" required="" maxlength="100">
                    <label for="project-id">"Project ID"</label><input id="project-id" name="id" required="" maxlength="48" pattern=("[a-zA-Z0-9_\\-]+") aria-describedby="id-help">
                    <small id="id-help">"Use the same ID in capture scenarios and publication commands."</small>
                    <button type="submit" class="button primary">"Create project"</button>
                </form>
            </details>
        }
    });
    auth::page(cx, content).await
}

#[derive(Deserialize)]
pub struct CreateProject {
    csrf: String,
    id: String,
    label: String,
}

pub async fn create_project(
    panel: Arc<Panel>,
    account: Account,
    form: CreateProject,
) -> PageResult<Response> {
    auth::check_csrf(&account, &form.csrf)?;
    if !account.owner {
        return Err(StatusCode::FORBIDDEN);
    }
    let label = form.label.trim();
    if !safe_name(&form.id) || form.id.len() > 48 || label.is_empty() || label.len() > 400 {
        return Err(StatusCode::UNPROCESSABLE_ENTITY);
    }
    let db = panel.index.lock().await;
    let created = db
        .execute(
            "INSERT OR IGNORE INTO projects VALUES (?, ?)",
            [form.id.as_str(), label],
        )
        .await
        .map_err(db_error)?;
    if created == 0 {
        return Err(StatusCode::CONFLICT);
    }
    Ok(redirect(&format!("/projects/{}", form.id)))
}

pub async fn project(panel: Arc<Panel>, account: Account, id: String) -> PageResult<Html<String>> {
    project_view(panel, account, id, "overview".into(), Filter::default()).await
}

pub async fn project_section(
    panel: Arc<Panel>,
    account: Account,
    (id, section): (String, String),
    filter: Filter,
) -> PageResult<Html<String>> {
    project_view(panel, account, id, section, filter).await
}

pub fn read_run(panel: &Panel, key: &str) -> Option<Manifest> {
    let (root, name) = key.split_once('/')?;
    read_manifest(&run_dir(panel, root.parse().ok()?, name)?)
}

async fn project_view(
    panel: Arc<Panel>,
    account: Account,
    id: String,
    raw_section: String,
    mut filter: Filter,
) -> PageResult<Html<String>> {
    if !safe_name(&id)
        || !matches!(
            raw_section.as_str(),
            "overview"
                | "captures"
                | "problems"
                | "fixes"
                | "design"
                | "proposes"
                | "storage"
                | "runs"
                | "images"
                | "videos"
                | "findings"
                | "settings"
        )
    {
        return Err(StatusCode::NOT_FOUND);
    }
    let section = match raw_section.as_str() {
        "runs" => {
            if filter.view.is_none() {
                filter.view = Some("runs".into());
            }
            "captures".to_string()
        }
        "images" => {
            if filter.view.is_none() {
                filter.view = Some("images".into());
            }
            "captures".to_string()
        }
        "videos" => {
            if filter.view.is_none() {
                filter.view = Some("videos".into());
            }
            "captures".to_string()
        }
        "findings" => "problems".to_string(),
        other => other.to_string(),
    };
    let role = auth::role(&panel, &account, &id).await?;
    let runs = visible_runs(&panel, &account).await?;
    let label = catalog(&panel, &account, &runs)
        .await?
        .remove(&id)
        .ok_or(StatusCode::NOT_FOUND)?;
    let related: Vec<_> = runs.iter().filter(|run| run.project_id == id).collect();
    let base = format!("/projects/{id}");
    let design = if matches!(
        section.as_str(),
        "design" | "proposes" | "overview" | "problems"
    ) {
        let path = panel.design.join(&id);
        match design_record::project(&path) {
            Ok(project) if project.id == id => design_record::entries(&path).map_err(db_error)?,
            Ok(_) => return Err(StatusCode::NOT_FOUND),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Vec::new(),
            Err(error) => return Err(db_error(error)),
        }
    } else {
        Vec::new()
    };
    let obsolete = design_record::superseded(&design);
    let current = design
        .iter()
        .filter(|entry| !obsolete.contains(entry.id.as_str()))
        .count();
    let records = &design;
    let superseded = &obsolete;
    let captures: BTreeMap<_, _> = related
        .iter()
        .filter_map(|run| {
            let (root, name) = run.key.split_once('/')?;
            let root = panel.roots.get(root.parse::<usize>().ok()?)?;
            let shared = panel.design.parent()?.join("captures");
            (root == &shared).then_some((name, format!("/run/{}", run.key)))
        })
        .collect();

    let mut all_assets = Vec::new();
    for run in &related {
        let Some(manifest) = read_run(&panel, &run.key) else {
            continue;
        };
        if manifest.project.id != id {
            continue;
        }
        for item in &manifest.checkpoints {
            all_assets.push(pipeline::AssetOption {
                run_key: run.key.clone(),
                scenario: run.scenario.clone(),
                name: item.id.clone(),
                url: format!("/media/{}/{}", run.key, item.path),
                kind: "Screenshot",
            });
        }
        for clip in &manifest.clips {
            if let Some(path) = &clip.video_path {
                all_assets.push(pipeline::AssetOption {
                    run_key: run.key.clone(),
                    scenario: run.scenario.clone(),
                    name: clip.id.clone(),
                    url: format!("/media/{}/{path}", run.key),
                    kind: "Recording",
                });
            }
            if let Some(path) = &clip.sheet_path {
                all_assets.push(pipeline::AssetOption {
                    run_key: run.key.clone(),
                    scenario: run.scenario.clone(),
                    name: format!("{} · sheet", clip.id),
                    url: format!("/media/{}/{path}", run.key),
                    kind: "Contact sheet",
                });
            }
        }
    }

    let problems_list = pipeline::sync_legacy_findings(&panel, &id, &related);
    let fixes_list = fix_record::entries(&pipeline::data_root(&panel), &id).unwrap_or_default();
    let mcp_online = proposal_record::mcp_session_active(&pipeline::data_root(&panel)).is_some();

    let (members, devices, audit) = if section == "settings" && role == Role::Owner {
        access_lists(&panel, &id).await?
    } else {
        (Vec::new(), Vec::new(), Vec::new())
    };
    let open_problems_count = problems_list
        .iter()
        .filter(|p| {
            matches!(
                p.status,
                problem_record::ProblemStatus::Open | problem_record::ProblemStatus::NeedsReprocess
            )
        })
        .count();
    let selected: Vec<_> = related
        .iter()
        .copied()
        .filter(|run| filter.accepts(run))
        .take(if section == "overview" { 8 } else { 100 })
        .collect();
    let cx = &Cx::default();
    let list = run_list(
        cx,
        &selected,
        "No matching captures. Send a capture with this project's ID.",
    );
    let section_title = match section.as_str() {
        "captures" => "Captures",
        "problems" => "Problems",
        "fixes" => "Fixes",
        "design" => "Design",
        "proposes" => "Proposes",
        "storage" => "Storage",
        "settings" => "Project settings",
        _ => "Overview",
    };
    let page_title = if section == "overview" {
        label.clone()
    } else {
        format!("{section_title} · {label}")
    };
    let latest_project_evidence = if section == "overview" {
        related
            .iter()
            .filter(|run| run.status == "complete")
            .find_map(|run| {
                let manifest = read_run(&panel, &run.key)?;
                let checkpoint = pick_hero_checkpoint(&manifest.checkpoints)?;
                Some((
                    format!("/run/{}", run.key),
                    format!("/media/{}/{}", run.key, checkpoint.path),
                    run.scenario.clone(),
                ))
            })
    } else {
        None
    };
    let propose_state = if section == "proposes" {
        Some(proposes::load(
            &panel,
            &id,
            proposes::ProposeFilter {
                kind: filter.kind.clone(),
                status: filter.status.clone(),
            },
        )?)
    } else {
        None
    };
    let storage_report = if section == "storage" {
        Some(storage::analyze(&pipeline::data_root(&panel), Some(&id)))
    } else {
        None
    };
    let content = shell!(cx, &page_title, account, panel, "projects", {
        <nav class="breadcrumbs" aria-label="Breadcrumb"><a href="/projects">"Projects"</a><span aria-hidden="true">"/"</span><span>(&label)</span></nav>
        <header class="page-title project-masthead">
            <div><h1>(if section == "overview" { label.as_str() } else { section_title })</h1>
                if section == "overview" && label != id { <code class="muted">(&id)</code> }
            </div>
        </header>
        <nav class="tabs" aria-label="Project">
            for (slug, title) in [
                ("overview", "Overview"),
                ("captures", "Captures"),
                ("problems", "Problems"),
                ("fixes", "Fixes"),
                ("proposes", "Proposes"),
                ("design", "Design"),
                ("storage", "Storage"),
                ("settings", "Settings"),
            ] {
                if slug != "settings" || role == Role::Owner {
                    <a href=(if slug == "overview" { base.clone() } else { format!("{base}/{slug}") })
                        if section == slug { aria-current="page" }>(title)</a>
                }
            }
        </nav>
        if section == "overview" {
            <div class="overview-hero">
                if let Some((href, image, scenario)) = &latest_project_evidence {
                    <section class="studio-proof">
                        <div class="section-head">
                            <h2>"Latest evidence"</h2>
                            <span class="tactic-chip">"1440×900 · verified"</span>
                        </div>
                        <a href=(href)>
                            <div class="studio-proof-image"><img src=(image) alt="" decoding="async"></div>
                            <span>(scenario) <span aria-hidden="true">"↗"</span></span>
                        </a>
                    </section>
                }
                <section class="project-brief">
                    <div class="section-head">
                        <h2>"Design context"</h2>
                        <a class="button" href=(format!("{base}/design"))>"Open design"</a>
                    </div>
                    if let Some(entry) = records.iter().find(|entry| !superseded.contains(entry.id.as_str()) && matches!(entry.kind, design_record::Kind::Brief | design_record::Kind::Direction)) {
                        <div class="brief-featured">
                            <p class="studio-kind">(entry.kind.label()) " · " (entry.status.label())</p>
                            <h3>(&entry.title)</h3>
                            <p class="project-brief-note">(&entry.note)</p>
                        </div>
                    }
                    if records.iter().any(|entry| !superseded.contains(entry.id.as_str()) && matches!(entry.kind, design_record::Kind::Tactic | design_record::Kind::Decision)) {
                        <ul class="brief-ledger">
                            for entry in records.iter().filter(|entry| !superseded.contains(entry.id.as_str()) && matches!(entry.kind, design_record::Kind::Tactic | design_record::Kind::Decision)).take(3) {
                                <li>
                                    <span class="tactic-chip">(entry.kind.label())</span>
                                    <strong>(&entry.title)</strong>
                                </li>
                            }
                        </ul>
                    }
                    <p class="muted brief-foot">(current) " active design records"</p>
                </section>
            </div>
            <section class="studio-counts" aria-label="Project pipeline">
                <a class="metric" href=(format!("{base}/captures"))><small>"Captures (" (all_assets.len()) " assets)"</small><strong>(related.len())</strong><span aria-hidden="true">"↗"</span></a>
                <a class=(if open_problems_count > 0 { "metric featured" } else { "metric" }) href=(format!("{base}/problems"))><small>"Problems queue"</small><strong>(open_problems_count)</strong><span aria-hidden="true">"↗"</span></a>
                <a class="metric" href=(format!("{base}/fixes"))><small>"Recorded fixes"</small><strong>(fixes_list.len())</strong><span aria-hidden="true">"↗"</span></a>
            </section>
            <section class="surface captures-ledger">
                <div class="section-head">
                    <h2>"Recent captures"</h2>
                    <a class="button" href=(format!("{base}/captures"))>"Open research hub"</a>
                </div>
                (list)
            </section>
        }
        if section == "captures" {
            (pipeline::render_captures(
                cx,
                &panel,
                &account,
                role,
                &id,
                &base,
                &filter,
                &related,
                &all_assets,
                &problems_list,
            ))
        }
        if section == "problems" {
            (pipeline::render_problems(
                cx,
                &account,
                role,
                &id,
                &base,
                &filter,
                &problems_list,
                &all_assets,
                records,
                mcp_online,
            ))
        }
        if section == "fixes" {
            (pipeline::render_fixes(
                cx,
                &account,
                role,
                &id,
                &base,
                &filter,
                &fixes_list,
                &problems_list,
                &all_assets,
                mcp_online,
            ))
        }
        if let Some(report) = &storage_report {
            (pipeline::render_storage(
                cx,
                &account,
                role,
                &id,
                &base,
                &filter,
                report,
            ))
        }
        if let Some(state) = &propose_state {
            (proposes::render_section(cx, &account, role, &id, &base, state))
        }
        if section == "design" {
            if role == Role::Owner {
                <details class="surface panel-padding design-editor">
                    <summary>"Add or revise a design record"</summary>
                    <form class="stack-form" method="post" action=(format!("{base}/design/record"))>
                        <input type="hidden" name="csrf" value=(&account.csrf)>
                        <label for="design-kind">"Record type"</label>
                        <select id="design-kind" name="kind">
                            <option value="brief">"Brief"</option><option value="direction">"Art direction"</option>
                            <option value="tactic">"Tactic"</option><option value="decision">"Decision"</option>
                        </select>
                        <label for="design-title">"Title"</label><input id="design-title" name="title" required="" maxlength="200">
                        <label for="design-note">"Rationale"</label><textarea id="design-note" name="note" required="" maxlength="4000" rows="5"></textarea>
                        <label for="design-status">"Status"</label>
                        <select id="design-status" name="status"><option value="proposed">"Proposed"</option><option value="adopted">"Adopted"</option><option value="rejected">"Rejected"</option></select>
                        <label for="design-surface">"Surface (optional)"</label><input id="design-surface" name="surface" maxlength="200">
                        <label for="design-reference">"Reference URL (optional)"</label><input id="design-reference" name="reference" type="url" maxlength="2048">
                        <label for="design-evidence">"Capture evidence (optional)"</label>
                        <select id="design-evidence" name="evidence"><option value="">"None"</option>
                            for (name, _) in &captures {
                                <option value=(*name)>(related.iter().find(|run| run.key.ends_with(&format!("/{name}"))).map(|run| run.scenario.as_str()).unwrap_or(name))</option>
                            }
                        </select>
                        if !records.is_empty() {
                            <label for="design-replaces">"Replace an active record of the same type (optional)"</label>
                            <select id="design-replaces" name="supersedes"><option value="">"None"</option>
                                for entry in records.iter().filter(|entry| !superseded.contains(entry.id.as_str())) {
                                    <option value=(&entry.id)>(entry.kind.label()) " · " (&entry.title)</option>
                                }
                            </select>
                        }
                        <button class="button primary" type="submit">"Save design record"</button>
                    </form>
                </details>
            }
            if records.is_empty() {
                <section class="empty">
                    <h2>"No design records yet"</h2>
                    <p>"Ask your agent to record the brief, art direction, tactics and decisions as the design takes shape."</p>
                </section>
            }
            for kind in [design_record::Kind::Brief, design_record::Kind::Direction, design_record::Kind::Tactic, design_record::Kind::Decision] {
                if records.iter().any(|entry| entry.kind == kind && !superseded.contains(entry.id.as_str())) {
                    <section class="design-chapter">
                        <h2>(kind.label())</h2>
                        <ul class="findings-list">
                            for entry in records.iter().filter(|entry| entry.kind == kind && !superseded.contains(entry.id.as_str())) {
                                <li>
                                    <h3>(&entry.title)</h3>
                                    <p class="muted">(entry.status.label())
                                        if let Some(surface) = &entry.surface { " · " (surface) }
                                        if let Some(author) = &entry.author { " · " (author) }
                                        " · " <time data-epoch-ms=(entry.timestamp_ms.to_string())></time>
                                    </p>
                                    <p class="whitespace-pre-wrap">(&entry.note)</p>
                                    if let Some(url) = entry.reference.as_deref().filter(|url| url.starts_with("https://") || url.starts_with("http://")) {
                                        <a href=(url) rel="noreferrer">"Design reference"</a>
                                    }
                                    if let Some(run) = entry.evidence.as_deref().and_then(|run| captures.get(run)) {
                                        " " <a href=(run)>"Capture evidence"</a>
                                    }
                                </li>
                            }
                        </ul>
                    </section>
                }
            }
            if !superseded.is_empty() {
                <section class="surface panel-padding">
                    <details>
                        <summary>"Decision and design history"</summary>
                        <ul class="findings-list">
                            for entry in records.iter().filter(|entry| superseded.contains(entry.id.as_str())) {
                                <li>
                                    <h3>(&entry.title)</h3>
                                    <p class="muted">(entry.kind.label()) " · " (entry.status.label()) " · Superseded"</p>
                                    <p class="whitespace-pre-wrap">(&entry.note)</p>
                                </li>
                            }
                        </ul>
                    </details>
                </section>
            }
        }
        if section == "settings" {
            <h2>"Access & publication"</h2>
            if role != Role::Owner {
                <div class="empty"><p>"Only the workspace owner can manage project access."</p></div>
            } else {
                <div class="settings-workbench">
                    <section class="surface panel-padding">
                        <h3>"Members"</h3>
                        if members.is_empty() { <p class="muted">"No invited members."</p> }
                        for item in &members {
                            <form method="post" action=(format!("{base}/settings")) class="entry">
                                <input type="hidden" name="csrf" value=(&account.csrf)>
                                <input type="hidden" name="action" value="revoke-member">
                                <input type="hidden" name="value" value=(&item.key)>
                                <span><strong>(&item.label)</strong><small>(&item.detail)</small></span>
                                <button class="button" type="submit">"Remove"</button>
                            </form>
                        }
                        <form method="post" action=(format!("{base}/settings")) class="stack-form">
                            <input type="hidden" name="csrf" value=(&account.csrf)>
                            <input type="hidden" name="action" value="invite">
                            <label for="role">"Invitation role"</label>
                            <select name="value" id="role"><option value="reader">"Reader"</option><option value="reviewer">"Reviewer"</option></select>
                            <button class="button" type="submit">"Create invitation"</button>
                        </form>
                    </section>
                    <section class="surface panel-padding">
                        <h3>"Machines"</h3>
                        if panel.uploads.is_none() {
                            <p class="muted">"Enable a managed upload directory to publish captures from another machine."</p>
                        } else {
                            if devices.is_empty() { <p class="muted">"No publishing credentials."</p> }
                            for item in &devices {
                                <form method="post" action=(format!("{base}/settings")) class="entry">
                                    <input type="hidden" name="csrf" value=(&account.csrf)>
                                    <input type="hidden" name="action" value="revoke-device">
                                    <input type="hidden" name="value" value=(&item.key)>
                                    <span><strong>(&item.label)</strong><small>(&item.detail)</small></span>
                                    <button class="button" type="submit">"Revoke"</button>
                                </form>
                            }
                            <form method="post" action=(format!("{base}/settings")) class="stack-form">
                                <input type="hidden" name="csrf" value=(&account.csrf)>
                                <input type="hidden" name="action" value="device">
                                <label for="device-label">"Machine name"</label>
                                <input id="device-label" name="value" required="" maxlength="100">
                                <button class="button" type="submit">"Create publishing credential"</button>
                            </form>
                        }
                    </section>
                </div>
                <section class="surface panel-padding">
                    <h3>"Recent activity"</h3>
                    if audit.is_empty() { <p class="muted">"No access or publish events yet."</p> }
                    <ul class="audit">
                        for item in &audit {
                            <li><span>(&item.label) " · " (&item.detail)</span><time data-epoch-ms=(&item.key)></time></li>
                        }
                    </ul>
                </section>
            }
        }
    });
    auth::page(cx, content).await
}

struct AccessItem {
    label: String,
    detail: String,
    key: String,
}

async fn query_access_items(
    db: &Connection,
    sql: &str,
    param: &str,
) -> PageResult<Vec<AccessItem>> {
    let mut rows = db.query(sql, [param]).await.map_err(db_error)?;
    let mut items = Vec::new();
    while let Some(row) = rows.next().await.map_err(db_error)? {
        items.push(AccessItem {
            label: row.get(0).map_err(db_error)?,
            detail: row.get(1).map_err(db_error)?,
            key: row.get(2).map_err(db_error)?,
        });
    }
    Ok(items)
}

async fn access_lists(
    panel: &Panel,
    id: &str,
) -> PageResult<(Vec<AccessItem>, Vec<AccessItem>, Vec<AccessItem>)> {
    let db = panel.index.lock().await;
    let mut groups = Vec::new();
    for sql in [
        "SELECT handle, role, handle FROM members WHERE project = ? ORDER BY handle",
        "SELECT label, CASE WHEN last_seen IS NULL THEN 'Awaiting first upload' ELSE 'Has published' END, digest FROM devices WHERE project = ? ORDER BY created DESC",
        "SELECT event, actor, CAST(occurred * 1000 AS TEXT) FROM audit WHERE project = ? ORDER BY id DESC LIMIT 30",
    ] {
        groups.push(query_access_items(&db, sql, id).await?);
    }
    let mut groups = groups.into_iter();
    Ok((
        groups.next().unwrap(),
        groups.next().unwrap(),
        groups.next().unwrap(),
    ))
}

#[derive(Deserialize)]
pub struct ProjectAction {
    csrf: String,
    action: String,
    value: String,
}

pub async fn project_action(
    panel: Arc<Panel>,
    account: Account,
    (id, section): (String, String),
    form: ProjectAction,
) -> PageResult<Response> {
    auth::check_csrf(&account, &form.csrf)?;
    if section != "settings" || !safe_name(&id) {
        return Err(StatusCode::NOT_FOUND);
    }
    if !account.owner {
        return Err(StatusCode::FORBIDDEN);
    }
    let runs = visible_runs(&panel, &account).await?;
    let label = catalog(&panel, &account, &runs)
        .await?
        .remove(&id)
        .ok_or(StatusCode::NOT_FOUND)?;
    let secret = auth::token();
    let digest = auth::digest(&secret);
    let url = format!("/projects/{id}/settings");
    let value = form.value.trim();
    {
        let db = panel.index.lock().await;
        db.execute(
            "INSERT OR IGNORE INTO projects VALUES (?, ?)",
            [id.as_str(), label.as_str()],
        )
        .await
        .map_err(db_error)?;
        let (sql, params, event) = match form.action.as_str() {
            "device" if panel.uploads.is_some() && !value.is_empty() && value.len() <= 400 => (
                "INSERT INTO devices VALUES (?, ?, ?, ?, NULL)",
                Vec::from(turso::params![
                    digest,
                    id.clone(),
                    value.to_owned(),
                    auth::now()
                ]),
                "Machine credential created",
            ),
            "invite" if matches!(value, "reader" | "reviewer") => (
                "INSERT INTO invites VALUES (?, ?, ?, ?)",
                Vec::from(turso::params![
                    digest,
                    id.clone(),
                    value.to_owned(),
                    auth::now() + 86400
                ]),
                "Invitation created",
            ),
            "revoke-device" => (
                "DELETE FROM devices WHERE digest = ? AND project = ?",
                Vec::from(turso::params![value.to_owned(), id.clone()]),
                "Machine credential revoked",
            ),
            "revoke-member" => (
                "DELETE FROM members WHERE handle = ? AND project = ?",
                Vec::from(turso::params![value.to_owned(), id.clone()]),
                "Member access removed",
            ),
            _ => return Err(StatusCode::UNPROCESSABLE_ENTITY),
        };
        db.transactional_batch([
            (sql, params),
            ("INSERT INTO audit(project, actor, event, occurred) SELECT ?, ?, ?, ? WHERE changes() > 0",
             Vec::from(turso::params![id.clone(), account.handle.clone(), event.to_owned(), auth::now()])),
        ], TransactionBehavior::Immediate).await.map_err(db_error)?;
    }
    if form.action.starts_with("revoke-") {
        return Ok(redirect(&url));
    }
    let cx = &Cx::default();
    let content = shell!(cx, "Copy credential", account, panel, "projects", {
        <nav class="breadcrumbs" aria-label="Breadcrumb"><a href=(&url)>"Project settings"</a></nav>
        <h1>(if form.action == "invite" { "Invitation created" } else { "Machine credential created" })</h1>
        <section class="surface panel-padding credential-slip">
            if form.action == "invite" {
                <p>"Share the code and the registration URL with the invitee. The code expires in 24 hours."</p>
                <p><code>(format!("{}/register", panel.origin))</code></p>
            } else {
                <p>"Save this credential securely. It authorizes publishing captures for this project."</p>
                <p><code>(format!("{}/api/publish/{id}", panel.origin))</code></p>
            }
            <label for="secret">"Copy this code now"</label>
            <input id="secret" class="secret" readonly="" value=(&secret)>
            <p><a class="button" href=(&url)>"Return to settings"</a></p>
        </section>
    });
    auth::page(cx, content)
        .await?
        .into_response(cx)
        .map_err(db_error)
}

pub async fn settings(panel: Arc<Panel>, account: Account) -> PageResult<Html<String>> {
    let (session_count, tokens) = {
        let db = panel.index.lock().await;
        let count: i64 = db
            .query(
                "SELECT COUNT(*) FROM sessions WHERE handle = ? AND expires > ?",
                Vec::from(turso::params![account.handle.clone(), auth::now()]),
            )
            .await
            .map_err(db_error)?
            .next()
            .await
            .map_err(db_error)?
            .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?
            .get(0)
            .map_err(db_error)?;
        let tokens = if account.owner {
            query_access_items(
                &db,
                "SELECT label, CASE WHEN last_seen IS NULL THEN 'Awaiting first publish' ELSE 'Active' END, digest FROM machine_tokens WHERE owner = ? ORDER BY created DESC",
                account.handle.as_str(),
            )
            .await?
        } else {
            Vec::new()
        };
        (count, tokens)
    };
    let cx = &Cx::default();
    let content = shell!(cx, "Settings", account, panel, "settings", {
        <div class="page-title studio-heading"><h1>"Settings"</h1></div>
        <div class="settings-workbench">
            <section class="surface panel-padding">
                <h2>"Appearance"</h2>
                <div class="stack-form">
                    <label for="appearance">"Theme on this device"</label>
                    <select id="appearance" data-theme-choice=""><option value="dark">"Dark"</option><option value="light">"Light"</option><option value="system">"System"</option></select>
                </div>
            </section>
            <section class="surface panel-padding">
                <h2>"Account"</h2><p><strong>(&account.handle)</strong></p>
                <p class="muted">(session_count) (if session_count == 1 { " active session" } else { " active sessions" })</p>
                <form method="post" action="/settings">
                    <input type="hidden" name="csrf" value=(&account.csrf)>
                    <button type="submit" class="button">"Sign out other sessions"</button>
                </form>
                <details class="triage-note-drawer">
                    <summary>"Change password"</summary>
                    <form class="stack-form" method="post" action="/settings/password">
                        <input type="hidden" name="csrf" value=(&account.csrf)>
                        <label for="current-password">"Current password"</label>
                        <input id="current-password" name="current_password" type="password" required="" maxlength="1024" autocomplete="current-password">
                        <label for="new-password">"New password (at least 12 characters)"</label>
                        <input id="new-password" name="new_password" type="password" required="" minlength="12" maxlength="1024" autocomplete="new-password">
                        <label for="confirm-password">"Confirm new password"</label>
                        <input id="confirm-password" name="confirm_password" type="password" required="" minlength="12" maxlength="1024" autocomplete="new-password">
                        <button type="submit" class="button primary">"Update password"</button>
                    </form>
                </details>
            </section>
            if !account.owner {
                <section class="surface panel-padding">
                    <h2>"Join a project"</h2>
                    <form class="stack-form" method="post" action="/settings/invitation">
                        <input type="hidden" name="csrf" value=(&account.csrf)>
                        <label for="invite-code">"Invitation code"</label>
                        <input id="invite-code" name="code" type="password" required="" maxlength="128">
                        <button type="submit" class="button primary">"Join project"</button>
                    </form>
                </section>
            }
            if account.owner {
                <section class="surface panel-padding">
                    <h2>"Service"</h2>
                    <p>(if panel.secure { "Cloud workspace" } else { "Local workspace" })</p>
                    <p><code>(&panel.origin)</code></p>
                    <p class="muted">(panel.roots.len()) (if panel.roots.len() == 1 { " capture directory configured" } else { " capture directories configured" })</p>
                    <p class="muted">(if panel.uploads.is_some() { "Publication enabled" } else { "Publication disabled" })</p>
                </section>
                <section class="surface panel-padding">
                    <h2>"Global machine tokens"</h2>
                    if panel.uploads.is_none() {
                        <p class="muted">"Enable a managed upload directory to publish captures from any project."</p>
                    } else {
                        <p class="muted">"Authorizes publishing captures and auto-creating new projects across all repos from this machine."</p>
                        if tokens.is_empty() { <p class="muted">"No global machine tokens."</p> }
                        for item in &tokens {
                            <form method="post" action="/settings/tokens" class="entry">
                                <input type="hidden" name="csrf" value=(&account.csrf)>
                                <input type="hidden" name="action" value="revoke">
                                <input type="hidden" name="value" value=(&item.key)>
                                <span><strong>(&item.label)</strong><small>(&item.detail)</small></span>
                                <button class="button" type="submit">"Revoke"</button>
                            </form>
                        }
                        <form method="post" action="/settings/tokens" class="stack-form">
                            <input type="hidden" name="csrf" value=(&account.csrf)>
                            <input type="hidden" name="action" value="create">
                            <label for="global-machine-label">"Machine name"</label>
                            <input id="global-machine-label" name="value" required="" maxlength="100" placeholder="notebook">
                            <button class="button primary" type="submit">"Create global machine token"</button>
                        </form>
                    }
                </section>
            }
        </div>
    });
    auth::page(cx, content).await
}

pub async fn account_action(
    panel: Arc<Panel>,
    account: Account,
    form: auth::Csrf,
) -> PageResult<Response> {
    auth::check_csrf(&account, &form.csrf)?;
    let db = panel.index.lock().await;
    db.execute(
        "DELETE FROM sessions WHERE handle = ? AND id <> ?",
        [account.handle.as_str(), account.session.as_str()],
    )
    .await
    .map_err(db_error)?;
    Ok(redirect("/settings"))
}

pub async fn machine_tokens_action(
    panel: Arc<Panel>,
    account: Account,
    form: ProjectAction,
) -> PageResult<Response> {
    auth::check_csrf(&account, &form.csrf)?;
    if !account.owner {
        return Err(StatusCode::FORBIDDEN);
    }
    let value = form.value.trim();
    match form.action.as_str() {
        "create" if panel.uploads.is_some() && !value.is_empty() && value.len() <= 100 => {
            let secret = auth::token();
            let digest = auth::digest(&secret);
            {
                let db = panel.index.lock().await;
                db.execute(
                    "INSERT INTO machine_tokens (digest, owner, label, created, last_seen) VALUES (?, ?, ?, ?, NULL)",
                    Vec::from(turso::params![
                        digest,
                        account.handle.clone(),
                        value.to_owned(),
                        auth::now()
                    ]),
                )
                .await
                .map_err(db_error)?;
            }
            let cx = &Cx::default();
            let content = shell!(cx, "Global machine token", account, panel, "settings", {
                <section class="surface panel-padding">
                    <h2>"Global machine token"</h2>
                    <p>"Save this credential securely. It authorizes publishing captures and auto-creating projects for your account."</p>
                    <p><code>(format!("{}/api/publish/<project>", panel.origin))</code></p>
                    <pre class="credential">(secret)</pre>
                    <p><a class="button" href="/settings">"Return to settings"</a></p>
                </section>
            });
            auth::page(cx, content)
                .await?
                .into_response(cx)
                .map_err(db_error)
        }
        "revoke" => {
            let db = panel.index.lock().await;
            db.execute(
                "DELETE FROM machine_tokens WHERE digest = ? AND owner = ?",
                [value, account.handle.as_str()],
            )
            .await
            .map_err(db_error)?;
            Ok(redirect("/settings"))
        }
        _ => Err(StatusCode::UNPROCESSABLE_ENTITY),
    }
}

#[derive(Deserialize)]
pub struct PasswordChangeForm {
    csrf: String,
    current_password: String,
    new_password: String,
    confirm_password: String,
}

pub async fn change_password(
    panel: Arc<Panel>,
    account: Account,
    form: PasswordChangeForm,
) -> PageResult<Response> {
    auth::check_csrf(&account, &form.csrf)?;
    if !(12..=1024).contains(&form.new_password.len())
        || !auth::equal(&form.new_password, &form.confirm_password)
        || form.current_password.len() > 1024
    {
        return Err(StatusCode::UNPROCESSABLE_ENTITY);
    }
    let stored = {
        let db = panel.index.lock().await;
        let mut rows = db
            .query(
                "SELECT password FROM accounts WHERE handle = ?",
                [account.handle.as_str()],
            )
            .await
            .map_err(db_error)?;
        rows.next()
            .await
            .map_err(db_error)?
            .ok_or(StatusCode::UNAUTHORIZED)?
            .get::<String>(0)
            .map_err(db_error)?
    };
    let permit = panel
        .password_work
        .clone()
        .try_acquire_owned()
        .map_err(|_| StatusCode::TOO_MANY_REQUESTS)?;
    let current = form.current_password;
    let new_pw = form.new_password;
    let new_hash = tokio::task::spawn_blocking(move || -> PageResult<String> {
        use argon2::{Argon2, PasswordHash, PasswordVerifier};

        let _permit = permit;
        let verified = PasswordHash::new(&stored).is_ok_and(|hash| {
            Argon2::default()
                .verify_password(current.as_bytes(), &hash)
                .is_ok()
        });
        if !verified {
            return Err(StatusCode::FORBIDDEN);
        }
        auth::password_hash(&new_pw).map_err(db_error)
    })
    .await
    .map_err(db_error)??;
    let db = panel.index.lock().await;
    db.transactional_batch(
        [
            (
                "UPDATE accounts SET password = ? WHERE handle = ?",
                vec![Value::Text(new_hash), Value::Text(account.handle.clone())],
            ),
            (
                "DELETE FROM sessions WHERE handle = ? AND id <> ?",
                vec![Value::Text(account.handle), Value::Text(account.session)],
            ),
        ],
        TransactionBehavior::Immediate,
    )
    .await
    .map_err(db_error)?;
    Ok(redirect("/settings"))
}

#[derive(Deserialize)]
pub struct Review {
    csrf: String,
    note: String,
    evidence: Option<String>,
}
struct ReviewLock(PathBuf);
impl Drop for ReviewLock {
    fn drop(&mut self) {
        let _ = fs::remove_dir(&self.0);
    }
}

pub async fn review(
    panel: Arc<Panel>,
    account: Account,
    (root, name): (usize, String),
    form: Review,
) -> PageResult<Response> {
    auth::check_csrf(&account, &form.csrf)?;
    let dir = run_dir(&panel, root, &name).ok_or(StatusCode::NOT_FOUND)?;
    let manifest = read_manifest(&dir).ok_or(StatusCode::NOT_FOUND)?;
    if auth::role(&panel, &account, &manifest.project.id).await? == Role::Reader {
        return Err(StatusCode::FORBIDDEN);
    }
    let note = form.note.trim();
    let evidence = form.evidence.filter(|path| !path.is_empty());
    if note.is_empty()
        || note.len() > 8000
        || evidence
            .as_ref()
            .is_some_and(|path| !media::declared(&manifest, path))
    {
        return Err(StatusCode::UNPROCESSABLE_ENTITY);
    }
    let lock = dir.join(".review.lock");
    fs::create_dir(&lock).map_err(|error| {
        if error.kind() == std::io::ErrorKind::AlreadyExists {
            StatusCode::CONFLICT
        } else {
            db_error(error)
        }
    })?;
    let _lock = ReviewLock(lock);
    let current = read_manifest(&dir).ok_or(StatusCode::NOT_FOUND)?;
    if manifest.project.id != current.project.id {
        return Err(StatusCode::CONFLICT);
    }
    let path = dir.join("manifest.json");
    let mut json: serde_json::Value =
        serde_json::from_slice(&fs::read(&path).map_err(db_error)?).map_err(db_error)?;
    if json.get("findings").is_none() {
        json["findings"] = serde_json::json!([]);
    }
    json["findings"]
        .as_array_mut()
        .ok_or(StatusCode::CONFLICT)?
        .push(serde_json::json!({"note": note, "evidence": evidence, "author": account.handle}));
    json["inspection"] = serde_json::json!("reviewed");
    let bytes = serde_json::to_vec_pretty(&json).map_err(db_error)?;
    if bytes.len() > 1024 * 1024 {
        return Err(StatusCode::PAYLOAD_TOO_LARGE);
    }
    let temp = dir.join(format!(".review-{}.tmp", auth::token()));
    let result = (|| {
        let mut file = fs::File::options()
            .create_new(true)
            .write(true)
            .open(&temp)?;
        file.write_all(&bytes)?;
        file.sync_all()?;
        fs::rename(&temp, &path)
    })();
    if let Err(error) = result {
        let _ = fs::remove_file(&temp);
        return Err(db_error(error));
    }
    Ok(redirect(&format!("/run/{root}/{name}#review")))
}

#[derive(Deserialize)]
pub struct Invitation {
    csrf: String,
    code: String,
}

pub async fn accept_invitation(
    panel: Arc<Panel>,
    account: Account,
    form: Invitation,
) -> PageResult<Response> {
    auth::check_csrf(&account, &form.csrf)?;
    if account.owner || form.code.len() != 64 {
        return Err(StatusCode::UNPROCESSABLE_ENTITY);
    }
    let digest = auth::digest(&form.code);
    let db = panel.index.lock().await;
    let result = db.transactional_batch([
        ("INSERT OR IGNORE INTO members SELECT project, ?, role FROM invites WHERE digest = ? AND expires > ?",
         Vec::from(turso::params![account.handle, digest.clone(), auth::now()])),
        ("DELETE FROM invites WHERE digest = ? AND changes() = 1", Vec::from(turso::params![digest])),
    ], TransactionBehavior::Immediate).await.map_err(db_error)?;
    if result[0].rows_affected() == 0 {
        return Err(StatusCode::CONFLICT);
    }
    Ok(redirect("/projects"))
}

#[derive(Deserialize)]
pub struct DesignForm {
    csrf: String,
    kind: design_record::Kind,
    status: design_record::Status,
    title: String,
    note: String,
    surface: Option<String>,
    reference: Option<String>,
    evidence: Option<String>,
    supersedes: Option<String>,
    return_to: Option<String>,
}

pub async fn record_design(
    panel: Arc<Panel>,
    account: Account,
    id: String,
    form: DesignForm,
) -> PageResult<Response> {
    auth::check_csrf(&account, &form.csrf)?;
    if !account.owner {
        return Err(StatusCode::FORBIDDEN);
    }
    if !safe_name(&id) {
        return Err(StatusCode::NOT_FOUND);
    }
    let runs = visible_runs(&panel, &account).await?;
    let label = catalog(&panel, &account, &runs)
        .await?
        .remove(&id)
        .ok_or(StatusCode::NOT_FOUND)?;
    let clean =
        |value: Option<String>| value.map(|s| s.trim().to_owned()).filter(|s| !s.is_empty());
    let reference = clean(form.reference);
    let surface = clean(form.surface);
    let evidence = clean(form.evidence);
    let supersedes = clean(form.supersedes);
    let title = form.title.trim();
    let note = form.note.trim();
    if title.is_empty()
        || title.chars().count() > 200
        || note.is_empty()
        || note.chars().count() > 4000
        || surface.as_ref().is_some_and(|x| x.chars().count() > 200)
        || reference.as_ref().is_some_and(|x| {
            x.len() > 2048
                || !url::Url::parse(x)
                    .is_ok_and(|u| matches!(u.scheme(), "http" | "https") && u.host().is_some())
        })
        || evidence.as_ref().is_some_and(|name| {
            !safe_name(name)
                || !runs.iter().any(|run| {
                    let Some((index, file)) = run.key.split_once('/') else {
                        return false;
                    };
                    let root = index.parse::<usize>().ok().and_then(|i| panel.roots.get(i));
                    run.project_id == id
                        && name == file
                        && panel
                            .design
                            .parent()
                            .is_some_and(|parent| root == Some(&parent.join("captures")))
                })
        })
    {
        return Err(StatusCode::UNPROCESSABLE_ENTITY);
    }
    let _guard = panel.index.lock().await;
    let root = panel.design.join(&id);
    fs::create_dir_all(&root).map_err(db_error)?;
    let persist = |path: &std::path::Path, value: serde_json::Value| -> PageResult<()> {
        let mut tmp = tempfile::NamedTempFile::new_in(&root).map_err(db_error)?;
        serde_json::to_writer(&mut tmp, &value).map_err(db_error)?;
        tmp.as_file().sync_all().map_err(db_error)?;
        tmp.persist_noclobber(path).map_err(db_error)?;
        Ok(())
    };
    match design_record::project(&root) {
        Ok(project) if project.id == id => {}
        Ok(_) => return Err(StatusCode::CONFLICT),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            persist(
                &root.join("project.json"),
                serde_json::json!({"id":id,"label":label}),
            )?;
        }
        Err(error) => return Err(db_error(error)),
    }
    let records = design_record::entries(&root).map_err(db_error)?;
    let obsolete = design_record::superseded(&records);
    if supersedes.as_ref().is_some_and(|old| {
        obsolete.contains(old.as_str())
            || !records
                .iter()
                .any(|entry| entry.id == *old && entry.kind == form.kind)
    }) {
        return Err(StatusCode::UNPROCESSABLE_ENTITY);
    }
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(db_error)?;
    let entry = design_record::Entry {
        id: format!("{}-{}", now.as_nanos(), std::process::id()),
        timestamp_ms: u64::try_from(now.as_millis()).unwrap_or(u64::MAX),
        kind: form.kind,
        status: form.status,
        title: title.into(),
        note: note.into(),
        surface,
        reference,
        evidence,
        supersedes,
        author: Some(account.handle),
    };
    persist(
        &root.join(format!("{}.json", entry.id)),
        serde_json::to_value(entry).map_err(db_error)?,
    )?;
    let target = if form.return_to.as_deref() == Some("proposes") {
        format!("/projects/{id}/proposes")
    } else {
        format!("/projects/{id}/design")
    };
    Ok(redirect(&target))
}

#[derive(Deserialize)]
pub struct ProbeForm {
    csrf: String,
    url: String,
    mode: String,
    pages: String,
    viewports: String,
    themes: String,
    pipeline: Option<String>,
}

pub async fn trigger_probe(
    panel: Arc<Panel>,
    account: Account,
    id: String,
    form: ProbeForm,
) -> PageResult<Response> {
    auth::check_csrf(&account, &form.csrf)?;
    if !account.owner {
        return Err(StatusCode::FORBIDDEN);
    }
    if !safe_name(&id) {
        return Err(StatusCode::NOT_FOUND);
    }
    let url = form.url.trim().to_owned();
    if (!url.starts_with("http://") && !url.starts_with("https://")) || url.len() > 512 {
        return Err(StatusCode::UNPROCESSABLE_ENTITY);
    }
    let mode = match form.mode.as_str() {
        "page-check" | "all-pages" | "responsive-matrix" | "interaction-map" | "exhaustive" => {
            form.mode
        }
        _ => return Err(StatusCode::UNPROCESSABLE_ENTITY),
    };
    let root = panel
        .roots
        .first()
        .cloned()
        .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(db_error)?
        .as_millis();
    let run_name = format!("probe-{ts}");
    let out_dir = root.join(&run_name);
    let pages = form.pages.trim().to_owned();
    let viewports = form.viewports.trim().to_owned();
    let themes = form.themes.trim().to_owned();
    let auto_analyze = form.pipeline.as_deref() != Some("capture-only");
    let project_id = id.clone();

    std::thread::spawn(move || {
        let candidates = [
            std::env::current_exe()
                .ok()
                .map(|p| p.with_file_name("rupicola")),
            Some(PathBuf::from("./target/debug/rupicola")),
            std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".cargo/bin/rupicola")),
        ];
        let Some(bin) = candidates.into_iter().flatten().find(|p| p.is_file()) else {
            return;
        };
        let _ = std::process::Command::new(&bin)
            .args([
                "probe",
                "--url",
                &url,
                "--mode",
                &mode,
                "--pages",
                if pages.is_empty() { "/" } else { &pages },
                "--viewports",
                &viewports,
                "--themes",
                if themes.is_empty() { "dark" } else { &themes },
                "--out",
            ])
            .arg(&out_dir)
            .status();
        let manifest_path = out_dir.join("manifest.json");
        if let Ok(bytes) = fs::read(&manifest_path)
            && let Ok(mut val) = serde_json::from_slice::<serde_json::Value>(&bytes)
        {
            val["project"] = serde_json::json!({ "id": project_id, "label": project_id });
            if let Ok(updated) = serde_json::to_vec_pretty(&val) {
                let _ = fs::write(&manifest_path, updated);
            }
        }
        if auto_analyze {
            let _ = std::process::Command::new(&bin)
                .args([
                    "problem",
                    "analyze-run",
                    &run_name,
                    "--project",
                    &project_id,
                ])
                .status();
        }
    });

    Ok(redirect(&format!("/projects/{id}/captures")))
}
