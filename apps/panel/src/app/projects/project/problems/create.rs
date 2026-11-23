//! `POST /projects/{project}/problems/create`

use crate::app::{account, panel, respond};
use crate::{app::projects::project::Project, pipeline};
use topcoat::{
    Result,
    context::Cx,
    router::{content::Form, path_param, response::Response, route},
};

#[route(POST)]
async fn create(cx: &Cx, Form(form): Form<pipeline::CreateProblemForm>) -> Result<Response> {
    respond(
        cx,
        pipeline::create_problem_action(
            panel(cx),
            account(cx),
            path_param::<Project>(cx).into(),
            form,
        )
        .await,
    )
}
