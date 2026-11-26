//! `/projects/{project}/fixes/{fix}`

mod review;

use super::super::{current, tab_metadata};
use crate::StatusCode;
use crate::app::fail;
use crate::components::skeleton::list_tab_skeleton;
use crate::records;
use crate::views::project::{Tab, fix_detail};
use topcoat::{
    Result,
    context::Cx,
    metadata::Metadata,
    router::{module_param, page},
    view::{View, component, error_boundary, suspense, view},
};

module_param!(pub fix);

#[page(with(Metadata::from_fn(async |cx| tab_metadata(cx, Tab::Fixes).await)))]
async fn page(cx: &Cx) -> Result<impl View> {
    Ok(view! {
        suspense(
            fallback: view! { list_tab_skeleton() },
            error_boundary(
                fallback: |_| Ok(view! {
                    <p class="notice" role="alert">"Fix details could not be loaded. Try again in a moment."</p>
                }),
                loaded_fix()
            )
        )
    })
}

#[component]
async fn loaded_fix(cx: &Cx) -> Result<impl View> {
    let project = current(cx).await.as_ref().map_err(|e| fail(*e))?;
    let fixes = records::fixes(cx, &project.id).await.map_err(fail)?;
    let target = fix(cx);
    let item = fixes
        .into_iter()
        .find(|f| f.id == target)
        .ok_or_else(|| fail(StatusCode::NOT_FOUND))?;
    let open_jobs = crate::backend::open_jobs(cx, &project.id)
        .await
        .as_ref()
        .ok()
        .cloned()
        .unwrap_or_default();
    let job = open_jobs
        .into_iter()
        .find(|j| j.fix.as_deref() == Some(item.id.as_str()));
    let (linked_problems, baseline) = if item.problem_ids.is_empty() {
        (Vec::new(), Vec::new())
    } else {
        let assets = project.assets();
        let problems = records::problems(cx, &project.id, &assets, &[])
            .await
            .unwrap_or_default();
        let matched: Vec<_> = problems
            .into_iter()
            .filter(|p| item.problem_ids.contains(&p.id))
            .collect();
        let linked = matched
            .iter()
            .map(|p| (p.id.clone(), p.title.clone()))
            .collect();
        let baseline = if item.comparisons.is_empty() {
            let mut seen = std::collections::BTreeSet::new();
            matched
                .into_iter()
                .flat_map(|p| p.evidence_assets)
                .filter(|ev| seen.insert(ev.url.clone()))
                .collect()
        } else {
            Vec::new()
        };
        (linked, baseline)
    };
    Ok(view! {
        fix_detail(
            role: project.role,
            base: &project.base,
            fix: item,
            job: job,
            linked_problems: linked_problems,
            baseline: baseline,
        )
    })
}
