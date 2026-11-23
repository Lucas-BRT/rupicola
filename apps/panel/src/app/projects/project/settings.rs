//! `GET` and `POST /projects/{project}/settings`

use crate::app::{account, panel, respond};
use crate::{
    Account, Arc, PageResult, Panel,
    app::projects::project::Project,
    auth::Role,
    pages,
    project::{Context, access_lists},
    views::{self, project::Tab},
};
use topcoat::{
    Result,
    context::Cx,
    router::{
        content::{Form, Html},
        path_param,
        response::Response,
        route,
    },
};

#[route(GET)]
async fn page(cx: &Cx) -> Result<Response> {
    respond(
        cx,
        show(panel(cx), account(cx), path_param::<Project>(cx).into()).await,
    )
}

async fn show(panel: Arc<Panel>, account: Account, id: String) -> PageResult<Html<String>> {
    let project = Context::load(&panel, &account, &id).await?;
    let (members, audit) = if project.role == Role::Owner {
        access_lists(&panel, &account, &id).await?
    } else {
        (Vec::new(), Vec::new())
    };
    let cx = &Cx::default();
    let data = views::project::settings::Settings {
        base: &project.base,
        role: project.role,
        members: &members,
        audit: &audit,
    };
    let body = views::project::settings::render(cx, &account, data);
    views::project::frame(cx, &account, &panel, &project, Tab::Settings, body).await
}

#[route(POST)]
async fn change_access(cx: &Cx, Form(form): Form<pages::ProjectAction>) -> Result<Response> {
    let path = (path_param::<Project>(cx).into(), "settings".into());
    respond(
        cx,
        pages::project_action(panel(cx), account(cx), path, form).await,
    )
}
