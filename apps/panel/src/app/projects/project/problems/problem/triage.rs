//! `POST /projects/{project}/problems/{problem}/triage`

use crate::app::{account, panel, respond};
use crate::{
    app::projects::project::Project, app::projects::project::problems::problem::Problem, pipeline,
};
use topcoat::{
    Result,
    context::Cx,
    router::{content::Form, path_param, response::Response, route},
};

#[route(POST)]
async fn triage(cx: &Cx, Form(form): Form<pipeline::TriageProblemForm>) -> Result<Response> {
    respond(
        cx,
        pipeline::triage_problem_action(
            panel(cx),
            account(cx),
            (
                path_param::<Project>(cx).into(),
                path_param::<Problem>(cx).into(),
            ),
            form,
        )
        .await,
    )
}
