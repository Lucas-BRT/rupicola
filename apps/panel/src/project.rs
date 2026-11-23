//! What every page of a project needs: who may see it, what it is called and which files its
//! captures hold.

use crate::{Account, PageResult, Panel, Run, auth, media, runs, safe_name};
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
