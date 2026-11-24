//! Reads from the backend that more than one part of a request needs. Each is memoized, so the
//! first caller asks the backend and the others share the answer for the rest of the request.

use crate::{Account, Panel, auth};
use http::StatusCode;
use rupicola_api::{
    Capture, CaptureSummary, DesignEntry, Fix, FixSummary, Problem, ProblemSummary, Project,
    SystemList, SystemVersion,
};
use std::sync::Arc;
use topcoat::context::{Cx, context, memoize};

/// A failed read: the status the page answers with.
pub type Failure = StatusCode;

fn api(cx: &Cx) -> (&Arc<Panel>, &Account) {
    (context::<Arc<Panel>>(cx), context::<Account>(cx))
}

/// The projects the signed-in account can see.
#[memoize]
pub async fn projects(cx: &Cx) -> Result<Vec<Project>, Failure> {
    let (panel, account) = api(cx);
    panel
        .api
        .projects(&account.session)
        .await
        .map_err(|e| auth::api_error(&e))
}

/// The captures of `project`, newest first, without their files.
#[memoize]
pub async fn captures(cx: &Cx, project: &str) -> Result<Vec<CaptureSummary>, Failure> {
    let (panel, account) = api(cx);
    panel
        .api
        .captures(&account.session, project)
        .await
        .map_err(|e| auth::api_error(&e))
}

/// One capture with its checkpoints, clips and findings.
#[memoize]
pub async fn capture(cx: &Cx, id: &str) -> Result<Capture, Failure> {
    let (panel, account) = api(cx);
    panel
        .api
        .capture(&account.session, id)
        .await
        .map_err(|e| auth::api_error(&e))
}

/// The problems of `project`, most recently updated first.
#[memoize]
pub async fn problems(cx: &Cx, project: &str) -> Result<Vec<ProblemSummary>, Failure> {
    let (panel, account) = api(cx);
    panel
        .api
        .problems(&account.session, project)
        .await
        .map_err(|e| auth::api_error(&e))
}

/// Every problem of `project` in full, most recently updated first.
#[memoize]
pub async fn problems_detailed(cx: &Cx, project: &str) -> Result<Vec<Problem>, Failure> {
    let (panel, account) = api(cx);
    panel
        .api
        .problems_detailed(&account.session, project)
        .await
        .map_err(|e| auth::api_error(&e))
}

/// The fixes of `project`, most recently updated first.
#[memoize]
pub async fn fixes(cx: &Cx, project: &str) -> Result<Vec<FixSummary>, Failure> {
    let (panel, account) = api(cx);
    panel
        .api
        .fixes(&account.session, project)
        .await
        .map_err(|e| auth::api_error(&e))
}

/// Every fix of `project` in full, most recently updated first.
#[memoize]
pub async fn fixes_detailed(cx: &Cx, project: &str) -> Result<Vec<Fix>, Failure> {
    let (panel, account) = api(cx);
    panel
        .api
        .fixes_detailed(&account.session, project)
        .await
        .map_err(|e| auth::api_error(&e))
}

/// The design records of `project`, newest first.
#[memoize]
pub async fn design(cx: &Cx, project: &str) -> Result<Vec<DesignEntry>, Failure> {
    let (panel, account) = api(cx);
    panel
        .api
        .design(&account.session, project)
        .await
        .map_err(|e| auth::api_error(&e))
}

/// The design-system versions of `project`, and which is adopted.
#[memoize]
pub async fn design_systems(cx: &Cx, project: &str) -> Result<SystemList, Failure> {
    let (panel, account) = api(cx);
    panel
        .api
        .design_systems(&account.session, project)
        .await
        .map_err(|e| auth::api_error(&e))
}

/// One design-system version.
#[memoize]
pub async fn design_system(cx: &Cx, project: &str, id: &str) -> Result<SystemVersion, Failure> {
    let (panel, account) = api(cx);
    panel
        .api
        .design_system(&account.session, project, id)
        .await
        .map_err(|e| auth::api_error(&e))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::Account;
    use rupicola_client::MockApi;

    const SESSION: &str = "session";

    fn panel(api: MockApi) -> Arc<Panel> {
        Arc::new(Panel {
            origin: String::new(),
            localhost_origin: String::new(),
            host: String::new(),
            secure: false,
            api: Arc::new(api),
        })
    }

    #[tokio::test]
    async fn the_backend_is_asked_once_for_what_a_request_reads_twice() {
        let mut api = MockApi::new();
        api.expect_projects().times(1).returning(|_| Ok(vec![]));
        api.expect_captures().times(2).returning(|_, _| Ok(vec![]));
        let cx = Cx::default().with(panel(api)).with(Account {
            handle: "ada".to_owned(),
            owner: false,
            session: SESSION.to_owned(),
            csrf: String::new(),
        });
        assert_eq!(projects(&cx).await, &Ok(vec![]));
        assert_eq!(projects(&cx).await, &Ok(vec![]));
        // Another argument is another read; the same one is shared.
        captures(&cx, "a").await.as_ref().unwrap();
        captures(&cx, "b").await.as_ref().unwrap();
        captures(&cx, "a").await.as_ref().unwrap();
    }
}
