//! What every page of a project needs: who may see it, what it is called and which files its
//! captures hold.

use crate::{
    Account, PageResult, Panel, Run, StatusCode, auth, auth::Role, media, runs, safe_name,
};
use rupicola_core::capture_manifest::Checkpoint;
use serde::Deserialize;
use std::collections::BTreeMap;

/// A file of a capture that a problem, fix or design record can cite.
pub struct AssetOption {
    pub run_key: String,
    pub scenario: String,
    pub name: String,
    pub url: String,
    pub kind: &'static str,
}

/// Query options shared by the project's tabs.
#[derive(Default, Deserialize)]
pub struct Filter {
    pub project: Option<String>,
    pub state: Option<String>,
    pub q: Option<String>,
    pub kind: Option<String>,
    pub status: Option<String>,
    pub view: Option<String>,
    pub version: Option<String>,
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

/// Every capture the account can see.
pub async fn visible_runs(panel: &Panel, account: &Account) -> PageResult<Vec<Run>> {
    runs::list(panel, account).await
}

/// The projects the account can see, with their labels.
pub async fn catalog(
    panel: &Panel,
    account: &Account,
    runs: &[Run],
) -> PageResult<BTreeMap<String, String>> {
    let mut projects = BTreeMap::new();
    for project in panel
        .api
        .projects(&account.session)
        .await
        .map_err(|e| auth::api_error(&e))?
    {
        projects.insert(project.id, project.label);
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

/// The files of the project's captures, newest capture first.
pub fn assets(related: &[&Run], id: &str) -> Vec<AssetOption> {
    let mut all_assets = Vec::new();
    for run in related {
        let manifest = &run.manifest;
        if manifest.project.id != id {
            continue;
        }
        for item in &manifest.checkpoints {
            all_assets.push(AssetOption {
                run_key: run.key.clone(),
                scenario: run.scenario.clone(),
                name: item.id.clone(),
                url: media::url(&item.path),
                kind: "Screenshot",
            });
        }
        for clip in &manifest.clips {
            if let Some(path) = &clip.video_path {
                all_assets.push(AssetOption {
                    run_key: run.key.clone(),
                    scenario: run.scenario.clone(),
                    name: clip.id.clone(),
                    url: media::url(path),
                    kind: "Recording",
                });
            }
            if let Some(path) = &clip.sheet_path {
                all_assets.push(AssetOption {
                    run_key: run.key.clone(),
                    scenario: run.scenario.clone(),
                    name: format!("{} · sheet", clip.id),
                    url: media::url(path),
                    kind: "Contact sheet",
                });
            }
        }
    }
    all_assets
}

/// A project as its pages see it.
pub struct Context {
    pub id: String,
    pub label: String,
    /// The project's own URL; its tabs hang below it.
    pub base: String,
    pub role: Role,
    /// The project's recent captures, newest first.
    pub runs: Vec<Run>,
}

impl Context {
    /// The project `id` for `account`.
    ///
    /// # Errors
    ///
    /// 404 when the id is malformed or the account cannot see the project.
    pub async fn load(panel: &Panel, account: &Account, id: &str) -> PageResult<Self> {
        if !safe_name(id) {
            return Err(StatusCode::NOT_FOUND);
        }
        let role = auth::role(panel, account, id).await?;
        let project = panel
            .api
            .projects(&account.session)
            .await
            .map_err(|e| auth::api_error(&e))?
            .into_iter()
            .find(|project| project.id == id)
            .ok_or(StatusCode::NOT_FOUND)?;
        let runs = runs::of_project(panel, account, &project).await?;
        Ok(Self {
            base: format!("/projects/{id}"),
            id: project.id,
            label: project.label,
            role,
            runs,
        })
    }

    /// The captures that belong to this project.
    pub fn related(&self) -> Vec<&Run> {
        self.runs.iter().collect()
    }

    /// The captured files this project's records can cite.
    pub fn assets(&self) -> Vec<AssetOption> {
        assets(&self.related(), &self.id)
    }
}

pub fn pick_hero_checkpoint(checkpoints: &[Checkpoint]) -> Option<&Checkpoint> {
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

pub struct AccessItem {
    pub label: String,
    pub detail: String,
    pub key: String,
}

/// A project's members and its recent access events, as the settings screen lists them.
pub async fn access_lists(
    panel: &Panel,
    account: &Account,
    id: &str,
) -> PageResult<(Vec<AccessItem>, Vec<AccessItem>)> {
    let members = panel
        .api
        .members(&account.session, id)
        .await
        .map_err(|e| auth::api_error(&e))?
        .into_iter()
        .map(|member| AccessItem {
            key: member.handle.clone(),
            label: member.handle,
            detail: role_name(member.role).to_owned(),
        })
        .collect();
    let audit = panel
        .api
        .audit(&account.session, id)
        .await
        .map_err(|e| auth::api_error(&e))?
        .into_iter()
        .map(|event| AccessItem {
            key: event.occurred_at.to_string(),
            label: event.event,
            detail: event.actor,
        })
        .collect();
    Ok((members, audit))
}

const fn role_name(role: rupicola_api::Role) -> &'static str {
    match role {
        rupicola_api::Role::Owner => "owner",
        rupicola_api::Role::Reviewer => "reviewer",
        rupicola_api::Role::Reader => "reader",
    }
}
