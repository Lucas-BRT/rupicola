//! `/projects`: the projects themselves and everything that belongs to one, under
//! `/projects/{id}`.

use super::super::handlers::{
    captures, credentials, design, design_systems as design_system_handlers, fixes, jobs, limits,
    problems, projects, publish, removal, scenarios, search, selections, settings, topics,
};
use crate::state::Shared;
use axum::{Router, routing};

pub fn routes() -> Router<Shared> {
    Router::new()
        .route("/", routing::get(projects::list).post(projects::create))
        .nest("/{id}", project())
}

/// `/projects/{id}`, with paths relative to it.
fn project() -> Router<Shared> {
    Router::new()
        .route(
            "/",
            routing::get(projects::get)
                .patch(projects::update)
                .delete(projects::delete),
        )
        .route("/audit", routing::get(projects::audit_log))
        .route("/limits", routing::get(limits::get))
        .route(
            "/deletion-policy",
            routing::get(removal::policy).put(removal::set_policy),
        )
        .route("/blobs/missing", routing::post(captures::missing))
        .route(
            "/problems",
            routing::get(problems::list).post(problems::create),
        )
        .route("/problems/details", routing::get(problems::list_details))
        .route("/fixes", routing::get(fixes::list).post(fixes::create))
        .route("/fixes/details", routing::get(fixes::list_details))
        .route("/jobs", routing::get(jobs::list).post(jobs::create))
        .route("/jobs/claim", routing::post(jobs::claim))
        .route(
            "/scenarios",
            routing::get(scenarios::list).post(scenarios::put),
        )
        .route("/scenarios/{name}", routing::get(scenarios::latest))
        .route("/scenarios/{name}/{hash}", routing::get(scenarios::version))
        .route("/design", routing::get(design::list).post(design::create))
        .route("/selections", routing::post(selections::create))
        .route("/topics", routing::get(topics::tree).post(topics::create))
        .nest("/members", members())
        .nest("/settings", project_settings())
        .route(
            "/capture-settings",
            routing::get(settings::capture_project).patch(settings::update_capture_project),
        )
        .nest("/captures", project_captures())
        .nest("/design-systems", design_systems())
        .nest("/devices", devices())
        .nest("/invites", invites())
}

fn members() -> Router<Shared> {
    Router::new()
        .route("/", routing::get(projects::members))
        .route(
            "/{handle}",
            routing::put(projects::put_member).delete(projects::delete_member),
        )
}

fn project_settings() -> Router<Shared> {
    Router::new()
        .route("/", routing::get(settings::project))
        .route(
            "/{key}",
            routing::put(settings::set_project).delete(settings::reset_project),
        )
}

fn project_captures() -> Router<Shared> {
    Router::new()
        .route("/", routing::get(captures::list))
        .route("/search", routing::get(search::search))
        .route("/{capture}", routing::put(publish::put))
        .route(
            "/{capture}/analyze",
            routing::post(problems::analyze_capture),
        )
}

fn design_systems() -> Router<Shared> {
    Router::new()
        .route(
            "/",
            routing::get(design_system_handlers::list).post(design_system_handlers::create),
        )
        .route("/{version}", routing::get(design_system_handlers::get))
        .route(
            "/{version}/document",
            routing::put(design_system_handlers::edit),
        )
        .route(
            "/{version}/actions",
            routing::post(design_system_handlers::act),
        )
}

fn devices() -> Router<Shared> {
    Router::new()
        .route(
            "/",
            routing::get(credentials::list_devices).post(credentials::create_device),
        )
        .route("/{device}", routing::delete(credentials::delete_device))
}

fn invites() -> Router<Shared> {
    Router::new()
        .route(
            "/",
            routing::get(credentials::list_invites).post(credentials::create_invite),
        )
        .route("/{invite}", routing::delete(credentials::delete_invite))
}
