use super::{
    Account, Role,
    csrf::{
        FORM_AGE, FORM_COOKIE, SESSION_COOKIE, cookie, cookie_header, csrf_for, equal, now, token,
    },
    errors::{
        BAD_LOGIN, BAD_SETUP_CODE, LOGIN_LIMITED, REGISTER_LIMITED, SETUP_REFUSED, api_error,
    },
    same_origin,
};
use crate::views::auth::sign_in_sheet;
use crate::{
    Arc, Cx, Deserialize, Html, IntoResponse, PageResult, Panel, Response, StatusCode, ViewExt,
    db_error, redirect, status, view,
};
use http::{HeaderMap, Method, header};
use rupicola_api::{Credentials as SignIn, Me, PasswordChange, Register, Setup, TokenKind};
use rupicola_client::{Api, ClientError};

const MS_PER_SEC: i64 = 1000;

/// The account a session token belongs to.
///
/// # Errors
///
/// 401 when the backend does not know the token or it is not a person's session.
pub async fn identify(api: &dyn Api, session: &str) -> PageResult<Account> {
    let me = api.me(session).await.map_err(|e| api_error(&e))?;
    account_of(me, session)
}

fn account_of(me: Me, session: &str) -> PageResult<Account> {
    match (me.kind, me.handle) {
        (TokenKind::Session, Some(handle)) => Ok(Account {
            handle,
            owner: me.admin,
            session: session.to_owned(),
            csrf: csrf_for(session),
        }),
        _ => Err(StatusCode::UNAUTHORIZED),
    }
}

pub async fn authenticate(
    panel: &Panel,
    path: &str,
    headers: &HeaderMap,
    method: &Method,
) -> PageResult<Option<Account>> {
    if matches!(
        path,
        "/panel.css" | "/panel.js" | "/login" | "/register" | "/healthz"
    ) {
        return Ok(None);
    }
    let Some(session) = cookie(headers, SESSION_COOKIE) else {
        return Err(StatusCode::UNAUTHORIZED);
    };
    let account = identify(panel.api.as_ref(), &session).await?;
    if *method != Method::GET && *method != Method::HEAD && !same_origin(panel, headers) {
        return Err(StatusCode::FORBIDDEN);
    }
    Ok(Some(account))
}

pub fn unauthenticated(path: &str) -> Response {
    if path.starts_with("/media/") {
        status(StatusCode::NOT_FOUND)
    } else {
        redirect("/login")
    }
}

/// What `account` may do in `project`, given the `projects` it can see; an administrator may do
/// everything.
///
/// # Errors
///
/// 404 when the account is not a member, so a project's existence does not leak.
pub fn role_in(
    projects: &[rupicola_api::Project],
    account: &Account,
    project: &str,
) -> PageResult<Role> {
    if account.owner {
        return Ok(Role::Owner);
    }
    projects
        .iter()
        .find(|candidate| candidate.id == project)
        .and_then(|candidate| candidate.role)
        .map(Role::from)
        .ok_or(StatusCode::NOT_FOUND)
}

/// What `account` may do in `project`, asking the backend; for the actions, which run once
/// per request. Pages use [`role_of`], which shares the answer.
///
/// # Errors
///
/// 404 when the account is not a member.
pub async fn role(panel: &Panel, account: &Account, project: &str) -> PageResult<Role> {
    if account.owner {
        return Ok(Role::Owner);
    }
    let projects = panel
        .api
        .projects(&account.session)
        .await
        .map_err(|e| api_error(&e))?;
    role_in(&projects, account, project)
}

/// What the signed-in account may do in `project`, sharing the list of projects with the rest
/// of the request.
///
/// # Errors
///
/// 404 when the account is not a member.
pub async fn role_of(cx: &Cx, project: &str) -> PageResult<Role> {
    let account = topcoat::context::context::<Account>(cx);
    if account.owner {
        return Ok(Role::Owner);
    }
    let projects = crate::backend::projects(cx)
        .await
        .as_ref()
        .map_err(|e| *e)?;
    role_in(projects, account, project)
}

async fn setup_required(panel: &Panel) -> PageResult<bool> {
    panel.api.setup_required().await.map_err(|e| api_error(&e))
}

pub async fn login_page(panel: Arc<Panel>) -> PageResult<Response> {
    if setup_required(&panel).await? {
        return Ok(redirect("/register"));
    }
    render(&panel, false, false, "", None, StatusCode::OK).await
}

pub async fn register_page(panel: Arc<Panel>) -> PageResult<Response> {
    let initial = setup_required(&panel).await?;
    render(&panel, true, initial, "", None, StatusCode::OK).await
}

#[derive(Deserialize)]
pub struct Credentials {
    handle: String,
    password: String,
    csrf: String,
    code: Option<String>,
}

fn check_form(panel: &Panel, headers: &HeaderMap, form: &Credentials) -> PageResult<()> {
    if !same_origin(panel, headers)
        || cookie(headers, FORM_COOKIE).is_none_or(|token| !equal(&token, &form.csrf))
    {
        return Err(StatusCode::FORBIDDEN);
    }
    Ok(())
}

pub async fn login(
    panel: Arc<Panel>,
    headers: HeaderMap,
    form: Credentials,
) -> PageResult<Response> {
    check_form(&panel, &headers, &form)?;
    let handle = form.handle.trim().to_ascii_lowercase();
    let attempt = SignIn {
        handle: handle.clone(),
        password: form.password,
    };
    match panel.api.sign_in(&attempt).await {
        Ok(session) => Ok(start_session(&panel, &session)),
        Err(ClientError::Unauthorized | ClientError::Invalid(_)) => {
            render(
                &panel,
                false,
                false,
                &handle,
                Some(BAD_LOGIN),
                StatusCode::UNAUTHORIZED,
            )
            .await
        }
        Err(ClientError::RateLimited { .. }) => {
            render(
                &panel,
                false,
                false,
                &handle,
                Some(LOGIN_LIMITED),
                StatusCode::TOO_MANY_REQUESTS,
            )
            .await
        }
        Err(error) => Err(api_error(&error)),
    }
}

pub async fn register(
    panel: Arc<Panel>,
    headers: HeaderMap,
    form: Credentials,
) -> PageResult<Response> {
    check_form(&panel, &headers, &form)?;
    let handle = form.handle.trim().to_ascii_lowercase();
    let initial = setup_required(&panel).await?;
    let code = form.code.as_deref().unwrap_or("").trim().to_owned();
    let result = if initial {
        panel
            .api
            .setup(&Setup {
                handle: handle.clone(),
                password: form.password,
                code: (!code.is_empty()).then_some(code),
            })
            .await
    } else {
        panel
            .api
            .register(&Register {
                code,
                handle: handle.clone(),
                password: form.password,
            })
            .await
    };
    let (message, status) = match result {
        Ok(session) => return Ok(start_session(&panel, &session)),
        Err(ClientError::Invalid(message)) => (message, StatusCode::UNPROCESSABLE_ENTITY),
        Err(ClientError::Forbidden) => (BAD_SETUP_CODE.to_owned(), StatusCode::FORBIDDEN),
        Err(ClientError::Conflict(_)) => (SETUP_REFUSED.to_owned(), StatusCode::CONFLICT),
        Err(ClientError::RateLimited { .. }) => {
            (REGISTER_LIMITED.to_owned(), StatusCode::TOO_MANY_REQUESTS)
        }
        Err(error) => return Err(api_error(&error)),
    };
    render(&panel, true, initial, &handle, Some(&message), status).await
}

/// Keeps the backend's session token in a cookie and goes to the dashboard.
fn start_session(panel: &Panel, session: &rupicola_api::SessionToken) -> Response {
    let age = ((session.expires_at / MS_PER_SEC) - now()).max(1);
    let mut response = redirect("/dashboard");
    response.headers_mut().append(
        header::SET_COOKIE,
        cookie_header(panel, SESSION_COOKIE, &session.token, age),
    );
    response
        .headers_mut()
        .append(header::SET_COOKIE, cookie_header(panel, FORM_COOKIE, "", 0));
    response
}

#[derive(Deserialize)]
pub struct Csrf {
    pub csrf: String,
}

pub async fn logout(panel: Arc<Panel>, account: Account, form: Csrf) -> PageResult<Response> {
    super::check_csrf(&account, &form.csrf)?;
    // A session the backend already ended is the outcome asked for.
    let _ = panel.api.sign_out(&account.session).await;
    let mut response = redirect("/login");
    response.headers_mut().insert(
        header::SET_COOKIE,
        cookie_header(&panel, SESSION_COOKIE, "", 0),
    );
    Ok(response)
}

/// Ends the person's other sessions.
pub async fn sign_out_others(panel: &Panel, account: &Account) -> PageResult<()> {
    panel
        .api
        .sign_out_others(&account.session)
        .await
        .map_err(|e| api_error(&e))
}

/// Changes the person's password.
pub async fn change_password(
    panel: &Panel,
    account: &Account,
    change: &PasswordChange,
) -> PageResult<()> {
    panel
        .api
        .change_password(&account.session, change)
        .await
        .map_err(|e| api_error(&e))
}

async fn render(
    panel: &Panel,
    registering: bool,
    initial: bool,
    handle: &str,
    error: Option<&str>,
    status: StatusCode,
) -> PageResult<Response> {
    let nonce = token();
    let form_cookie = cookie_header(panel, FORM_COOKIE, &nonce, FORM_AGE);
    let title = match (registering, initial) {
        (true, true) => "Set up Rupicola",
        (true, false) => "Accept an invitation",
        _ => "Sign in",
    };
    let cx = &Cx::default();
    let content = view! {
        sign_in_sheet(title: title, registering: registering, initial: initial, handle: handle, error: error, nonce: &nonce)
    }
    .single()
    .await
    .map_err(db_error)?;
    let mut response = Html(content.render(cx))
        .into_response(cx)
        .map_err(db_error)?;
    *response.status_mut() = status;
    response
        .headers_mut()
        .insert(header::SET_COOKIE, form_cookie);
    Ok(response)
}
