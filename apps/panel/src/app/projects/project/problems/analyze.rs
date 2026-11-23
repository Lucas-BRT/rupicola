//! `POST /projects/{project}/problems/analyze`

use crate::app::{account, panel, respond};
use crate::{app::projects::project::Project, pipeline};
use topcoat::{
    Result,
    context::Cx,
    router::{content::Form, path_param, response::Response, route},
};

#[route(POST)]
async fn analyze(cx: &Cx, Form(form): Form<pipeline::AnalyzeRunForm>) -> Result<Response> {
    respond(
        cx,
        pipeline::analyze_run_action(
            panel(cx),
            account(cx),
            path_param::<Project>(cx).into(),
            form,
        )
        .await,
    )
}
