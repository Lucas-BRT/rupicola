//! `POST /projects/{project}/fixes/{fix}/review`

use crate::app::{account, panel, respond};
use crate::{app::projects::project::Project, app::projects::project::fixes::fix::Fix, pipeline};
use topcoat::{
    Result,
    context::Cx,
    router::{content::Form, path_param, response::Response, route},
};

#[route(POST)]
async fn review(cx: &Cx, Form(form): Form<pipeline::ReviewFixForm>) -> Result<Response> {
    respond(
        cx,
        pipeline::review_fix_action(
            panel(cx),
            account(cx),
            (
                path_param::<Project>(cx).into(),
                path_param::<Fix>(cx).into(),
            ),
            form,
        )
        .await,
    )
}
