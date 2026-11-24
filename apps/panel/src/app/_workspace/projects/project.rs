//! `/projects/{project}`: the overview. The layout puts the heading and the tabs around every
//! page below; each child module is one tab or one of its actions.

mod captures;
mod design;
mod findings;
mod fixes;
mod images;
mod problems;
mod runs;
mod settings;
mod videos;

use crate::app::{_workspace::title, fail};
use crate::{
    backend::Failure,
    project::{Context, ProjectMeta, Proof},
    records,
    views::project::{
        Tab,
        overview::{Overview, overview_tab},
        project_frame,
    },
};
use topcoat::{
    Result,
    context::{Cx, memoize},
    metadata::Metadata,
    router::{Slot, layout, module_param, page, request},
    view::{View, view},
};

module_param!(pub project);

/// Lightweight metadata for the project, loaded once and shared by the layout and metadata.
#[memoize]
pub async fn current_meta(cx: &Cx) -> Result<ProjectMeta, Failure> {
    ProjectMeta::load(cx, project(cx)).await
}

/// The project the request is about, loaded once and shared by pages that need its captures.
#[memoize]
pub async fn current(cx: &Cx) -> Result<Context, Failure> {
    Context::load(cx, project(cx)).await
}

/// The metadata of a project's tab: the project's name, behind the tab's when it is not the
/// overview.
///
/// # Errors
///
/// The error of failing to load the project.
pub async fn tab_metadata(cx: &Cx, tab: Tab) -> Result<Metadata> {
    let project = current_meta(cx).await.as_ref().map_err(|e| fail(*e))?;
    Ok(Metadata::new().title(title(&if tab == Tab::Overview {
        project.label.clone()
    } else {
        format!("{} · {}", tab.heading(), project.label)
    })))
}

#[layout]
async fn frame(cx: &Cx, slot: Slot<'_>) -> Result<impl View> {
    let project = current_meta(cx).await.as_ref().map_err(|e| fail(*e))?;
    let tab = Tab::of(request::uri(cx).path(), &project.base);
    Ok(view! { project_frame(project: project, tab: tab, (slot)) })
}

/// Recent captures listed on the overview.
const RECENT_CAPTURES: usize = 8;

#[page(with(Metadata::from_fn(async |cx| tab_metadata(cx, Tab::Overview).await)))]
async fn overview(cx: &Cx) -> Result<impl View> {
    let project = current(cx).await.as_ref().map_err(|e| fail(*e))?;
    let id = &project.id;
    let related = project.related();
    let all_assets = project.assets();
    let design = records::design(cx, id).await.map_err(fail)?;
    let fix_count = crate::backend::fixes(cx, id)
        .await
        .as_ref()
        .map_err(|e| fail(*e))?
        .len();
    let open_problems_count = crate::backend::problems(cx, id)
        .await
        .as_ref()
        .map_err(|e| fail(*e))?
        .iter()
        .filter(|problem| {
            matches!(
                problem.status,
                rupicola_api::ProblemStatus::Open | rupicola_api::ProblemStatus::NeedsReprocess
            )
        })
        .count();
    let recent: Vec<_> = related.iter().copied().take(RECENT_CAPTURES).collect();
    let latest_project_evidence = Proof::newest(related.iter().copied());
    let data = Overview {
        base: &project.base,
        latest_project_evidence,
        records: design,
        captures: related.len(),
        assets: all_assets.len(),
        open_problems_count,
        fix_count,
        recent,
    };
    Ok(view! { overview_tab(data: data) })
}
