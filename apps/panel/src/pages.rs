use super::{
    Account, Arc, BTreeMap, Checkpoint, Cx, Deserialize, Html, IntoResponse, PageResult, Panel,
    Response, Run, StatusCode, auth, db_error, design_record, media,
    project::{self, Filter},
    records, redirect, runs, safe_name, view,
};
use crate::components::badge::{BadgeVariant, badge};
use auth::Role;
use rupicola_api::{NewFinding, NewInvite, NewProject, PasswordChange, Redeem};

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

#[expect(
    clippy::too_many_lines,
    reason = "the dashboard view keeps its markup with the data it displays"
)]
pub async fn dashboard(
    panel: Arc<Panel>,
    account: Account,
    filter: Filter,
) -> PageResult<Html<String>> {
    let runs = project::visible_runs(&panel, &account).await?;
    let projects = project::catalog(&panel, &account, &runs).await?;
    let project_count = projects.len();
    let mut activity = Vec::new();
    for (id, label) in &projects {
        let records = records::design(&panel, &account, id).await?;
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
            let checkpoint = pick_hero_checkpoint(&run.manifest.checkpoints)?;
            Some((
                format!("/run/{}", run.key),
                media::url(&checkpoint.path),
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
    let runs = project::visible_runs(&panel, &account).await?;
    let projects = project::catalog(&panel, &account, &runs).await?;
    let search = filter.q.as_deref().unwrap_or("").trim().to_lowercase();
    let empty_catalog = projects.is_empty();
    let total_projects = projects.len();
    let visible: Vec<_> = projects
        .iter()
        .filter(|(id, label)| {
            search.is_empty() || id.contains(&search) || label.to_lowercase().contains(&search)
        })
        .collect();
    let mut summaries = BTreeMap::new();
    for (id, _) in &visible {
        if let Some(entry) = records::design(&panel, &account, id)
            .await?
            .into_iter()
            .next()
        {
            summaries.insert(
                (*id).clone(),
                format!("{} · {}", entry.title, entry.status.label()),
            );
        }
    }
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
    register_project(&panel, &account, &form.id, &form.label).await?;
    Ok(redirect(&format!("/projects/{}", form.id)))
}

pub(crate) async fn register_project(
    panel: &Panel,
    account: &Account,
    id: &str,
    label: &str,
) -> PageResult<()> {
    let label = label.trim();
    if !safe_name(id) || id.len() > 48 || label.is_empty() || label.len() > 400 {
        return Err(StatusCode::UNPROCESSABLE_ENTITY);
    }
    panel
        .api
        .create_project(
            &account.session,
            &NewProject {
                id: id.to_owned(),
                label: label.to_owned(),
            },
        )
        .await
        .map(|_| ())
        .map_err(|e| auth::api_error(&e))
}

#[derive(Deserialize)]
pub struct ProjectAction {
    csrf: String,
    action: String,
    value: String,
}

/// Changes who can reach a project; returns the code of a new invitation.
pub(crate) async fn change_project_access(
    panel: &Panel,
    account: &Account,
    id: &str,
    action: &str,
    value: &str,
) -> PageResult<Option<String>> {
    let value = value.trim();
    let api = &panel.api;
    match action {
        "invite" => {
            let role = match value {
                "reader" => rupicola_api::Role::Reader,
                "reviewer" => rupicola_api::Role::Reviewer,
                _ => return Err(StatusCode::UNPROCESSABLE_ENTITY),
            };
            let issued = api
                .create_invite(&account.session, id, &NewInvite { role, hours: None })
                .await
                .map_err(|e| auth::api_error(&e))?;
            Ok(Some(issued.code))
        }
        "revoke-member" => {
            api.remove_member(&account.session, id, value)
                .await
                .map_err(|e| auth::api_error(&e))?;
            Ok(None)
        }
        _ => Err(StatusCode::UNPROCESSABLE_ENTITY),
    }
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
    let url = format!("/projects/{id}/settings");
    let Some(secret) =
        change_project_access(&panel, &account, &id, &form.action, &form.value).await?
    else {
        return Ok(redirect(&url));
    };
    let cx = &Cx::default();
    let content = shell!(cx, "Copy credential", account, panel, "projects", {
        <nav class="breadcrumbs" aria-label="Breadcrumb"><a href=(&url)>"Project settings"</a></nav>
        <h1>"Invitation created"</h1>
        <section class="surface panel-padding credential-slip">
            <p>"Share the code and the registration URL with the invitee. The code expires in 24 hours."</p>
            <p><code>(format!("{}/register", panel.origin))</code></p>
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
    auth::sign_out_others(&panel, &account).await?;
    Ok(redirect("/settings"))
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
    if !auth::equal(&form.new_password, &form.confirm_password) {
        return Err(StatusCode::UNPROCESSABLE_ENTITY);
    }
    let change = PasswordChange {
        current: form.current_password,
        new: form.new_password,
    };
    auth::change_password(&panel, &account, &change).await?;
    Ok(redirect("/settings"))
}

#[derive(Deserialize)]
pub struct Review {
    csrf: String,
    note: String,
    evidence: Option<String>,
}
pub async fn review(
    panel: Arc<Panel>,
    account: Account,
    id: String,
    form: Review,
) -> PageResult<Response> {
    auth::check_csrf(&account, &form.csrf)?;
    let run = runs::find(&panel, &account, &id).await?;
    if auth::role(&panel, &account, &run.project_id).await? == Role::Reader {
        return Err(StatusCode::FORBIDDEN);
    }
    record_inspection(&panel, &account, &run, &form.note, form.evidence.as_deref()).await?;
    Ok(redirect(&format!("/run/{id}#review")))
}

/// Records a review note on a capture; the evidence is the id of one of its artifacts.
pub(crate) async fn record_inspection(
    panel: &Panel,
    account: &Account,
    run: &Run,
    note: &str,
    evidence: Option<&str>,
) -> PageResult<()> {
    let note = note.trim();
    let evidence = evidence.filter(|artifact| !artifact.is_empty());
    if note.is_empty()
        || note.len() > 8000
        || evidence
            .as_ref()
            .is_some_and(|artifact| !media::declared(&run.manifest, artifact))
    {
        return Err(StatusCode::UNPROCESSABLE_ENTITY);
    }
    panel
        .api
        .add_finding(
            &account.session,
            &run.key,
            &NewFinding {
                note: note.to_owned(),
                evidence_artifact_id: evidence.map(str::to_owned),
            },
        )
        .await
        .map(|_| ())
        .map_err(|e| auth::api_error(&e))
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
    if account.owner {
        return Err(StatusCode::UNPROCESSABLE_ENTITY);
    }
    panel
        .api
        .redeem_invite(&account.session, &Redeem { code: form.code })
        .await
        .map_err(|e| auth::api_error(&e))?;
    Ok(redirect("/projects"))
}

#[derive(Deserialize)]
pub struct DesignForm {
    #[serde(default)]
    csrf: String,
    kind: rupicola_api::DesignKind,
    status: rupicola_api::DesignStatus,
    title: String,
    note: String,
    surface: Option<String>,
    reference: Option<String>,
    evidence: Option<String>,
    supersedes: Option<String>,
}

pub async fn record_design(
    panel: Arc<Panel>,
    account: Account,
    id: String,
    form: DesignForm,
) -> PageResult<Response> {
    auth::check_csrf(&account, &form.csrf)?;
    if !safe_name(&id) {
        return Err(StatusCode::NOT_FOUND);
    }
    if auth::role(&panel, &account, &id).await? == Role::Reader {
        return Err(StatusCode::FORBIDDEN);
    }
    save_design(&panel, &account, &id, form).await?;
    Ok(redirect(&format!("/projects/{id}/design")))
}

async fn save_design(
    panel: &Panel,
    account: &Account,
    id: &str,
    form: DesignForm,
) -> PageResult<String> {
    let clean =
        |value: Option<String>| value.map(|s| s.trim().to_owned()).filter(|s| !s.is_empty());
    let reference = clean(form.reference);
    let title = form.title.trim();
    let note = form.note.trim();
    if title.is_empty()
        || note.is_empty()
        || reference.as_ref().is_some_and(|x| {
            !url::Url::parse(x)
                .is_ok_and(|u| matches!(u.scheme(), "http" | "https") && u.host().is_some())
        })
    {
        return Err(StatusCode::UNPROCESSABLE_ENTITY);
    }
    let new = rupicola_api::NewDesignEntry {
        kind: form.kind,
        status: form.status,
        title: title.into(),
        note: note.into(),
        surface: clean(form.surface),
        supersedes: clean(form.supersedes),
        reference,
        evidence: clean(form.evidence),
    };
    panel
        .api
        .add_design(&account.session, id, &new)
        .await
        .map(|entry| entry.id)
        .map_err(|e| auth::api_error(&e))
}
