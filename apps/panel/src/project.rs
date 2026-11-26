//! What every page of a project needs: who may see it, what it is called and which files its
//! captures hold.

use crate::{
    Account, PageResult, Panel, Run, StatusCode, auth, auth::Role, backend, media, runs, safe_name,
};
use rupicola_api::Checkpoint;
use serde::Deserialize;
use std::collections::BTreeMap;
use topcoat::context::Cx;

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
    pub sort: Option<String>,
}

impl Filter {
    #[must_use]
    pub fn sort_key(&self) -> &str {
        self.sort.as_deref().unwrap_or("newest")
    }

    pub fn sort_runs(&self, runs: &mut [&Run]) {
        match self.sort_key() {
            "oldest" => runs.sort_by_key(|r| r.created_at_ms),
            "scenario" => {
                runs.sort_by(|a, b| {
                    a.scenario
                        .to_lowercase()
                        .cmp(&b.scenario.to_lowercase())
                        .then_with(|| b.created_at_ms.cmp(&a.created_at_ms))
                });
            }
            "findings" => {
                runs.sort_by_key(|r| {
                    (
                        std::cmp::Reverse(r.capture.findings.len()),
                        std::cmp::Reverse(r.created_at_ms),
                    )
                });
            }
            _ => runs.sort_by_key(|r| std::cmp::Reverse(r.created_at_ms)),
        }
    }

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
pub async fn visible_runs(cx: &Cx) -> PageResult<Vec<Run>> {
    runs::list(cx).await
}

/// The projects the account can see, with their labels, without loading captures.
pub async fn project_catalog(cx: &Cx) -> PageResult<BTreeMap<String, String>> {
    let mut projects = BTreeMap::new();
    for project in backend::projects(cx).await.as_ref().map_err(|e| *e)? {
        projects.insert(project.id.clone(), project.label.clone());
    }
    Ok(projects)
}

/// The projects the account can see, with their labels.
pub async fn catalog(cx: &Cx, runs: &[Run]) -> PageResult<BTreeMap<String, String>> {
    let mut projects = project_catalog(cx).await?;
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
        let capture = &run.capture;
        if capture.project_id != id {
            continue;
        }
        for item in &capture.checkpoints {
            all_assets.push(AssetOption {
                run_key: run.key.clone(),
                scenario: run.scenario.clone(),
                name: item.id.clone(),
                url: media::url(&item.artifact.id),
                kind: "Screenshot",
            });
        }
        for clip in &capture.clips {
            if let Some(video) = &clip.video {
                all_assets.push(AssetOption {
                    run_key: run.key.clone(),
                    scenario: run.scenario.clone(),
                    name: clip.id.clone(),
                    url: media::url(&video.id),
                    kind: "Recording",
                });
            }
            if let Some(sheet) = &clip.sheet {
                all_assets.push(AssetOption {
                    run_key: run.key.clone(),
                    scenario: run.scenario.clone(),
                    name: format!("{} · sheet", clip.id),
                    url: media::url(&sheet.id),
                    kind: "Contact sheet",
                });
            }
        }
    }
    all_assets
}

/// Lightweight project metadata without loading its captures.
pub struct ProjectMeta {
    pub id: String,
    pub label: String,
    pub base: String,
    pub role: Role,
}

impl ProjectMeta {
    /// Loads project metadata and the caller's role without fetching captures.
    ///
    /// # Errors
    ///
    /// 404 when the id is malformed or the account cannot see the project.
    pub async fn load(cx: &Cx, id: &str) -> PageResult<Self> {
        if !safe_name(id) {
            return Err(StatusCode::NOT_FOUND);
        }
        let role = auth::role_of(cx, id).await?;
        let project = backend::projects(cx)
            .await
            .as_ref()
            .map_err(|e| *e)?
            .iter()
            .find(|project| project.id == id)
            .ok_or(StatusCode::NOT_FOUND)?;
        Ok(Self {
            base: format!("/projects/{id}"),
            id: project.id.clone(),
            label: project.label.clone(),
            role,
        })
    }
}

/// A project as its pages see it.
pub struct Context {
    pub id: String,
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
    pub async fn load(cx: &Cx, id: &str) -> PageResult<Self> {
        if !safe_name(id) {
            return Err(StatusCode::NOT_FOUND);
        }
        let role = auth::role_of(cx, id).await?;
        let project = backend::projects(cx)
            .await
            .as_ref()
            .map_err(|e| *e)?
            .iter()
            .find(|project| project.id == id)
            .ok_or(StatusCode::NOT_FOUND)?;
        let runs = runs::of_project(cx, project).await?;
        Ok(Self {
            base: format!("/projects/{id}"),
            id: project.id.clone(),
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

/// The newest verified capture's screenshot and the page it links to.
pub struct Proof {
    pub href: String,
    pub image: String,
    pub scenario: String,
}

impl Proof {
    /// Of `runs`, newest first, the first complete one that has a screenshot to show.
    pub fn newest<'a>(runs: impl IntoIterator<Item = &'a Run>) -> Option<Self> {
        runs.into_iter()
            .filter(|run| run.status == "complete")
            .find_map(|run| {
                let checkpoint = pick_hero_checkpoint(&run.capture.checkpoints)?;
                Some(Self {
                    href: format!("/run/{}", run.key),
                    image: media::url(&checkpoint.artifact.id),
                    scenario: run.scenario.clone(),
                })
            })
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
            detail: member.role.to_string(),
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
