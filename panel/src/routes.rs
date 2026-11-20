use super::*;
use topcoat::{
    Result,
    context::{app_context, request_context},
    router::{
        Next, OriginPolicy, content::Form, layer, parse_query_params, path_param, request, route,
    },
};

path_param!(project);
path_param!(section);
path_param!(proposal);
path_param!(problem);
path_param!(fix);
path_param!(asset);
path_param!(root: usize);
path_param!(name);
path_param!(*file);

pub fn router(panel: Arc<Panel>) -> Router {
    Router::builder()
        .origin_policy(OriginPolicy::new().trust_origins([panel.origin.clone()]))
        .app_context(panel)
        .layer(workspace)
        .route(dashboard)
        .route(projects)
        .route(create_project)
        .route(project)
        .route(project_section)
        .route(project_action)
        .route(create_proposal)
        .route(review_proposal)
        .route(preview_proposal)
        .route(proposal_asset)
        .route(analyze_run_problems)
        .route(create_problem)
        .route(triage_problem)
        .route(progress_problem)
        .route(problems_status)
        .route(create_fix)
        .route(review_fix)
        .route(save_storage_config)
        .route(clean_storage)
        .route(offload_storage)
        .route(trigger_probe)
        .route(record_design)
        .route(settings)
        .route(account_action)
        .route(machine_tokens)
        .route(change_password)
        .route(accept_invitation)
        .route(healthz)
        .route(login)
        .route(sign_in)
        .route(register)
        .route(sign_up)
        .route(logout)
        .route(run)
        .route(inspect)
        .route(artifact)
        .route(publish)
        .route(global_mcp)
        .route(remote_mcp)
        .route(styles)
        .route(script)
        .build()
}

fn panel(cx: &Cx) -> Arc<Panel> {
    app_context::<Arc<Panel>>(cx).clone()
}

fn account(cx: &Cx) -> Account {
    request_context::<Account>(cx).clone()
}

fn respond(cx: &Cx, result: PageResult<impl IntoResponse>) -> Result<Response> {
    match result {
        Ok(response) => response.into_response(cx),
        Err(error) => Ok(status(error)),
    }
}

#[layer("/")]
async fn workspace(cx: &Cx, body: Body, next: Next<'_>) -> Result<Response> {
    let panel = panel(cx);
    let headers = request::headers(cx);
    let path = request::uri(cx).path();
    let received = headers
        .get(header::HOST)
        .and_then(|value| value.to_str().ok());
    let local_alias = panel.localhost_origin.strip_prefix("http://");
    let local_loopback = panel
        .localhost_origin
        .strip_prefix("http://localhost:")
        .map(|p| format!("127.0.0.1:{p}"));
    let is_healthz = path == "/healthz" && received == local_loopback.as_deref();
    if !is_healthz && received != Some(&panel.host) && (panel.secure || received != local_alias) {
        return Ok(status(StatusCode::BAD_REQUEST));
    }
    let mut response = match auth::authenticate(&panel, path, headers, request::method(cx)).await {
        Ok(Some(account)) => next.run(&cx.with(account), body).await?,
        Ok(None) => next.run(cx, body).await?,
        Err(StatusCode::UNAUTHORIZED) => auth::unauthenticated(path),
        Err(code) => status(code),
    };
    let headers = response.headers_mut();
    if !headers.contains_key(header::CONTENT_SECURITY_POLICY) {
        headers.insert(header::CONTENT_SECURITY_POLICY,
            "default-src 'self'; style-src 'self' 'unsafe-inline'; object-src 'none'; frame-ancestors 'none'; base-uri 'none'; form-action 'self'".parse().unwrap());
    }
    headers.insert(header::X_CONTENT_TYPE_OPTIONS, "nosniff".parse().unwrap());
    headers.insert(header::REFERRER_POLICY, "same-origin".parse().unwrap());
    headers.insert(header::CACHE_CONTROL, "private, no-store".parse().unwrap());
    if panel.secure {
        headers.insert(
            header::STRICT_TRANSPORT_SECURITY,
            "max-age=31536000".parse().unwrap(),
        );
    }
    Ok(response)
}

#[route(GET "/dashboard")]
async fn dashboard(cx: &Cx) -> Result<Response> {
    respond(
        cx,
        pages::dashboard(panel(cx), account(cx), parse_query_params(cx)?).await,
    )
}

#[route(GET "/projects")]
async fn projects(cx: &Cx) -> Result<Response> {
    respond(
        cx,
        pages::projects(panel(cx), account(cx), parse_query_params(cx)?).await,
    )
}

#[route(POST "/projects")]
async fn create_project(cx: &Cx, Form(form): Form<pages::CreateProject>) -> Result<Response> {
    respond(
        cx,
        pages::create_project(panel(cx), account(cx), form).await,
    )
}

#[route(GET "/projects/{project}")]
async fn project(cx: &Cx) -> Result<Response> {
    respond(
        cx,
        pages::project(panel(cx), account(cx), path_param::<Project>(cx).into()).await,
    )
}

#[route(GET "/projects/{project}/{section}")]
async fn project_section(cx: &Cx) -> Result<Response> {
    let path = (
        path_param::<Project>(cx).into(),
        path_param::<Section>(cx).into(),
    );
    respond(
        cx,
        pages::project_section(panel(cx), account(cx), path, parse_query_params(cx)?).await,
    )
}

#[route(POST "/projects/{project}/{section}")]
async fn project_action(cx: &Cx, Form(form): Form<pages::ProjectAction>) -> Result<Response> {
    let path = (
        path_param::<Project>(cx).into(),
        path_param::<Section>(cx).into(),
    );
    respond(
        cx,
        pages::project_action(panel(cx), account(cx), path, form).await,
    )
}

#[route(GET "/settings")]
async fn settings(cx: &Cx) -> Result<Response> {
    respond(cx, pages::settings(panel(cx), account(cx)).await)
}

#[route(POST "/settings")]
async fn account_action(cx: &Cx, Form(form): Form<auth::Csrf>) -> Result<Response> {
    respond(
        cx,
        pages::account_action(panel(cx), account(cx), form).await,
    )
}

#[route(POST "/settings/tokens")]
async fn machine_tokens(cx: &Cx, Form(form): Form<pages::ProjectAction>) -> Result<Response> {
    respond(
        cx,
        pages::machine_tokens_action(panel(cx), account(cx), form).await,
    )
}

#[route(POST "/settings/password")]
async fn change_password(cx: &Cx, Form(form): Form<pages::PasswordChangeForm>) -> Result<Response> {
    respond(
        cx,
        pages::change_password(panel(cx), account(cx), form).await,
    )
}

#[route(GET "/healthz")]
async fn healthz() -> Result<&'static str> {
    Ok("ok")
}

#[route(POST "/settings/invitation")]
async fn accept_invitation(cx: &Cx, Form(form): Form<pages::Invitation>) -> Result<Response> {
    respond(
        cx,
        pages::accept_invitation(panel(cx), account(cx), form).await,
    )
}

#[route(GET "/login")]
async fn login(cx: &Cx) -> Result<Response> {
    respond(cx, auth::login_page(panel(cx)).await)
}

#[route(POST "/login")]
async fn sign_in(cx: &Cx, Form(form): Form<auth::Credentials>) -> Result<Response> {
    respond(
        cx,
        auth::login(panel(cx), request::headers(cx).clone(), form).await,
    )
}

#[route(GET "/register")]
async fn register(cx: &Cx) -> Result<Response> {
    respond(cx, auth::register_page(panel(cx)).await)
}

#[route(POST "/register")]
async fn sign_up(cx: &Cx, Form(form): Form<auth::Credentials>) -> Result<Response> {
    respond(
        cx,
        auth::register(panel(cx), request::headers(cx).clone(), form).await,
    )
}

#[route(POST "/logout")]
async fn logout(cx: &Cx, Form(form): Form<auth::Csrf>) -> Result<Response> {
    respond(cx, auth::logout(panel(cx), account(cx), form).await)
}

fn run_path(cx: &Cx) -> Option<(usize, String)> {
    Some((*path_param::<Root>(cx).ok()?, path_param::<Name>(cx).into()))
}

#[route(GET "/run/{root}/{name}")]
async fn run(cx: &Cx) -> Result<Response> {
    let Some(path) = run_path(cx) else {
        return Ok(status(StatusCode::NOT_FOUND));
    };
    respond(
        cx,
        super::detail(parse_query_params(cx)?, account(cx), panel(cx), path).await,
    )
}

#[route(POST "/run/{root}/{name}")]
async fn inspect(cx: &Cx, Form(form): Form<pages::Review>) -> Result<Response> {
    let Some(path) = run_path(cx) else {
        return Ok(status(StatusCode::NOT_FOUND));
    };
    respond(cx, pages::review(panel(cx), account(cx), path, form).await)
}

#[route([GET, HEAD] "/media/{root}/{name}/{*file}")]
async fn artifact(cx: &Cx) -> Result<Response> {
    let Some((root, name)) = run_path(cx) else {
        return Ok(status(StatusCode::NOT_FOUND));
    };
    let parts: Vec<_> = path_param::<File>(cx).collect();
    if parts.iter().any(|part| part.contains('/')) {
        return Ok(status(StatusCode::NOT_FOUND));
    }
    let mut request = Request::new(Body::empty());
    *request.method_mut() = request::method(cx).clone();
    *request.headers_mut() = request::headers(cx).clone();
    respond(
        cx,
        media::file(
            account(cx),
            panel(cx),
            (root, name, parts.join("/")),
            request,
        )
        .await,
    )
}

#[route(POST "/api/publish/{project}")]
async fn publish(cx: &Cx, body: Body) -> Result<Response> {
    respond(
        cx,
        ingest::publish(
            panel(cx),
            path_param::<Project>(cx).into(),
            request::headers(cx).clone(),
            body,
        )
        .await,
    )
}

#[route(GET "/panel.css")]
async fn styles() -> Result<([(http::HeaderName, &'static str); 1], &'static str)> {
    Ok((
        [(header::CONTENT_TYPE, "text/css; charset=utf-8")],
        include_str!(concat!(env!("OUT_DIR"), "/tailwind.css")),
    ))
}

#[route(GET "/panel.js")]
async fn script() -> Result<([(http::HeaderName, &'static str); 1], &'static str)> {
    Ok((
        [(header::CONTENT_TYPE, "text/javascript; charset=utf-8")],
        include_str!("panel.js"),
    ))
}

#[route(POST "/projects/{project}/proposes/create")]
async fn create_proposal(cx: &Cx, body: Body) -> Result<Response> {
    respond(
        cx,
        proposes::create(
            panel(cx),
            account(cx),
            path_param::<Project>(cx).into(),
            request::headers(cx).clone(),
            body,
        )
        .await,
    )
}

#[route(POST "/projects/{project}/proposes/{proposal}/review")]
async fn review_proposal(cx: &Cx, Form(form): Form<proposes::ReviewForm>) -> Result<Response> {
    respond(
        cx,
        proposes::review(
            panel(cx),
            account(cx),
            (
                path_param::<Project>(cx).into(),
                path_param::<Proposal>(cx).into(),
            ),
            form,
        )
        .await,
    )
}

#[route(GET "/projects/{project}/proposes/{proposal}/preview")]
async fn preview_proposal(cx: &Cx) -> Result<Response> {
    respond(
        cx,
        proposes::preview(
            panel(cx),
            account(cx),
            (
                path_param::<Project>(cx).into(),
                path_param::<Proposal>(cx).into(),
            ),
        )
        .await,
    )
}

#[route(GET "/projects/{project}/proposes/{proposal}/assets/{asset}")]
async fn proposal_asset(cx: &Cx) -> Result<Response> {
    respond(
        cx,
        proposes::asset(
            panel(cx),
            account(cx),
            (
                path_param::<Project>(cx).into(),
                path_param::<Proposal>(cx).into(),
                path_param::<Asset>(cx).into(),
            ),
        )
        .await,
    )
}

#[route(POST "/projects/{project}/probe")]
async fn trigger_probe(cx: &Cx, Form(form): Form<pages::ProbeForm>) -> Result<Response> {
    respond(
        cx,
        pages::trigger_probe(
            panel(cx),
            account(cx),
            path_param::<Project>(cx).into(),
            form,
        )
        .await,
    )
}

#[route(POST "/projects/{project}/design/record")]
async fn record_design(cx: &Cx, Form(form): Form<pages::DesignForm>) -> Result<Response> {
    respond(
        cx,
        pages::record_design(
            panel(cx),
            account(cx),
            path_param::<Project>(cx).into(),
            form,
        )
        .await,
    )
}

#[route(POST "/projects/{project}/problems/analyze")]
async fn analyze_run_problems(
    cx: &Cx,
    Form(form): Form<pipeline::AnalyzeRunForm>,
) -> Result<Response> {
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

#[route(POST "/projects/{project}/problems/create")]
async fn create_problem(
    cx: &Cx,
    Form(form): Form<pipeline::CreateProblemForm>,
) -> Result<Response> {
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

#[route(POST "/projects/{project}/problems/{problem}/triage")]
async fn triage_problem(
    cx: &Cx,
    Form(form): Form<pipeline::TriageProblemForm>,
) -> Result<Response> {
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

#[route(POST "/projects/{project}/problems/{problem}/progress")]
async fn progress_problem(
    cx: &Cx,
    Form(form): Form<pipeline::ProgressProblemForm>,
) -> Result<Response> {
    respond(
        cx,
        pipeline::progress_problem_action(
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

#[route(GET "/projects/{project}/problems/status.json")]
async fn problems_status(cx: &Cx) -> Result<Response> {
    respond(
        cx,
        pipeline::problems_status_json(panel(cx), account(cx), path_param::<Project>(cx).into())
            .await,
    )
}

#[route(POST "/projects/{project}/fixes/create")]
async fn create_fix(cx: &Cx, Form(form): Form<pipeline::CreateFixForm>) -> Result<Response> {
    respond(
        cx,
        pipeline::create_fix_action(
            panel(cx),
            account(cx),
            path_param::<Project>(cx).into(),
            form,
        )
        .await,
    )
}

#[route(POST "/projects/{project}/fixes/{fix}/review")]
async fn review_fix(cx: &Cx, Form(form): Form<pipeline::ReviewFixForm>) -> Result<Response> {
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

#[route(POST "/projects/{project}/storage/config")]
async fn save_storage_config(
    cx: &Cx,
    Form(form): Form<pipeline::StorageConfigForm>,
) -> Result<Response> {
    respond(
        cx,
        pipeline::save_storage_config_action(
            panel(cx),
            account(cx),
            path_param::<Project>(cx).into(),
            form,
        )
        .await,
    )
}

#[route(POST "/projects/{project}/storage/clean")]
async fn clean_storage(cx: &Cx, Form(form): Form<pipeline::StorageCleanForm>) -> Result<Response> {
    respond(
        cx,
        pipeline::clean_storage_action(
            panel(cx),
            account(cx),
            path_param::<Project>(cx).into(),
            form,
        )
        .await,
    )
}

#[route(POST "/projects/{project}/storage/offload")]
async fn offload_storage(
    cx: &Cx,
    Form(form): Form<pipeline::StorageOffloadForm>,
) -> Result<Response> {
    respond(
        cx,
        pipeline::offload_storage_action(
            panel(cx),
            account(cx),
            path_param::<Project>(cx).into(),
            form,
        )
        .await,
    )
}

#[route(POST "/mcp")]
async fn global_mcp(cx: &Cx, body: Body) -> Result<Response> {
    respond(
        cx,
        mcp::handle_global(panel(cx), request::headers(cx), body).await,
    )
}

#[route(POST "/mcp/{project}")]
async fn remote_mcp(cx: &Cx, body: Body) -> Result<Response> {
    respond(
        cx,
        mcp::handle(
            panel(cx),
            path_param::<Project>(cx).into(),
            request::headers(cx),
            body,
        )
        .await,
    )
}
