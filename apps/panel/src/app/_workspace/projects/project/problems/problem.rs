//! `/projects/{project}/problems/{problem}`

mod triage;

use super::super::{current, tab_metadata};
use crate::StatusCode;
use crate::app::{account, fail, panel};
use crate::components::skeleton::list_tab_skeleton;
use crate::records;
use crate::views::project::{Tab, problem_detail};
use topcoat::{
    Result,
    context::Cx,
    metadata::Metadata,
    router::{module_param, page},
    view::{View, component, error_boundary, suspense, view},
};

module_param!(pub problem);

#[page(with(Metadata::from_fn(async |cx| tab_metadata(cx, Tab::Problems).await)))]
async fn page(cx: &Cx) -> Result<impl View> {
    Ok(view! {
        suspense(
            fallback: view! { list_tab_skeleton() },
            error_boundary(
                fallback: |_| Ok(view! {
                    <p class="notice" role="alert">"Problem details could not be loaded. Try again in a moment."</p>
                }),
                loaded_problem()
            )
        )
    })
}

#[component]
async fn loaded_problem(cx: &Cx) -> Result<impl View> {
    let project = current(cx).await.as_ref().map_err(|e| fail(*e))?;
    let assets = project.assets();
    let fixes = records::fixes(cx, &project.id).await.map_err(fail)?;
    let problems = records::problems(cx, &project.id, &assets, &fixes)
        .await
        .map_err(fail)?;
    let target = problem(cx);
    let item = problems
        .into_iter()
        .find(|p| p.id == target)
        .ok_or_else(|| fail(StatusCode::NOT_FOUND))?;
    let (p, acc) = (panel(cx), account(cx));
    let selections = p
        .api
        .problem(&acc.session, target)
        .await
        .map(|prob| prob.selections)
        .unwrap_or_default();
    Ok(view! {
        problem_detail(role: project.role, base: &project.base, item: item, selections: selections)
    })
}
