//! Who is signed in, and what they may do. The backend owns accounts, sessions and
//! membership; the panel keeps the backend's session token in a cookie and asks the backend
//! on every request.

use super::{
    Arc, Cx, Deserialize, Html, IntoResponse, PageResult, Panel, Response, StatusCode, ViewExt,
    db_error, header, redirect, status, view,
};
use crate::views::auth::sign_in_sheet;
use http::{HeaderMap, HeaderValue, Method};
use rupicola_api::{Credentials as SignIn, Me, PasswordChange, Register, Setup, TokenKind};
use rupicola_client::{Api, ClientError};
use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;

/// Cookie that holds the backend's session token.
const SESSION_COOKIE: &str = "rp_session";
/// Cookie that ties a sign-in or registration form to the browser that loaded it.
const FORM_COOKIE: &str = "rp_form";
/// How long a loaded sign-in form stays valid, in seconds.
const FORM_AGE: i64 = 1800;
/// Mixed into a session token to derive the CSRF token of its forms.
const CSRF_SEED: &str = "rupicola-csrf:";
const MS_PER_SEC: i64 = 1000;
const BAD_LOGIN: &str = "The username or password is incorrect.";
const LOGIN_LIMITED: &str = "Too many failed sign-in attempts. Wait 5 minutes before trying again.";
const REGISTER_LIMITED: &str =
    "Too many registration attempts. Wait 5 minutes before trying again.";
const BAD_SETUP_CODE: &str = "The setup or invitation code is invalid.";
const SETUP_REFUSED: &str =
    "This setup or invitation could not be used. Check the code and username.";

/// A signed-in person, as the panel sees them for one request.
#[derive(Clone)]
pub struct Account {
    pub handle: String,
    /// Administers the whole backend.
    pub owner: bool,
    /// The backend's session token.
    pub session: String,
    pub csrf: String,
}

pub fn token() -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut bytes = [0u8; 32];
    getrandom::fill(&mut bytes).expect("the operating system provides random bytes");
    let mut token = String::with_capacity(2 * bytes.len());
    for byte in bytes {
        token.push(char::from(HEX[usize::from(byte >> 4)]));
        token.push(char::from(HEX[usize::from(byte & 0xf)]));
    }
    token
}

pub fn digest(secret: &str) -> String {
    format!("{:x}", Sha256::digest(secret.as_bytes()))
}

pub fn now() -> i64 {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    i64::try_from(secs).unwrap_or(i64::MAX)
}

/// The CSRF token of the forms of `session`: stable for the session and unguessable
/// without it, so the panel keeps no state.
fn csrf_for(session: &str) -> String {
    digest(&format!("{CSRF_SEED}{session}"))
}

fn cookie(headers: &HeaderMap, name: &str) -> Option<String> {
    let (_, value) = headers
        .get_all(header::COOKIE)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .flat_map(|value| value.split(';'))
        .filter_map(|value| value.trim().split_once('='))
        .find(|(key, _)| *key == name)?;
    (value.len() == 64 && value.bytes().all(|b| b.is_ascii_hexdigit())).then(|| value.to_owned())
}

pub fn cookie_header(panel: &Panel, name: &str, value: &str, age: i64) -> HeaderValue {
    let secure = if panel.secure { "; Secure" } else { "" };
    format!("{name}={value}; Path=/; HttpOnly; SameSite=Lax; Max-Age={age}{secure}")
        .parse()
        .unwrap()
}

/// The status a failed call to the backend becomes.
pub fn api_error(error: &ClientError) -> StatusCode {
    match error {
        ClientError::Unauthorized => StatusCode::UNAUTHORIZED,
        ClientError::Forbidden => StatusCode::FORBIDDEN,
        ClientError::NotFound => StatusCode::NOT_FOUND,
        ClientError::Invalid(_) => StatusCode::UNPROCESSABLE_ENTITY,
        ClientError::Conflict(_) => StatusCode::CONFLICT,
        ClientError::RateLimited { .. } => StatusCode::TOO_MANY_REQUESTS,
        ClientError::Unavailable(_) | ClientError::Malformed(_) => StatusCode::BAD_GATEWAY,
    }
}

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

pub fn same_origin(panel: &Panel, headers: &HeaderMap) -> bool {
    headers
        .get(header::ORIGIN)
        .and_then(|s| s.to_str().ok())
        .is_some_and(|origin| {
            origin == panel.origin || (!panel.secure && origin == panel.localhost_origin)
        })
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Role {
    Owner,
    Reviewer,
    Reader,
}

impl From<rupicola_api::Role> for Role {
    fn from(role: rupicola_api::Role) -> Self {
        match role {
            rupicola_api::Role::Owner => Self::Owner,
            rupicola_api::Role::Reviewer => Self::Reviewer,
            rupicola_api::Role::Reader => Self::Reader,
        }
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

pub fn equal(a: &str, b: &str) -> bool {
    bool::from(a.as_bytes().ct_eq(b.as_bytes()))
}

pub fn check_csrf(account: &Account, csrf: &str) -> PageResult<()> {
    if equal(&account.csrf, csrf) {
        Ok(())
    } else {
        Err(StatusCode::FORBIDDEN)
    }
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
    check_csrf(&account, &form.csrf)?;
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

#[cfg(test)]
mod tests {
    use super::*;
    use rupicola_api::Project;
    use rupicola_client::MockApi;

    const SESSION: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
    const OTHER_SESSION: &str = "fedcba9876543210fedcba9876543210fedcba9876543210fedcba9876543210";
    const HANDLE: &str = "ada";
    const PROJECT: &str = "demo";
    const OTHER_PROJECT: &str = "elsewhere";

    fn me(kind: TokenKind, admin: bool) -> Me {
        Me {
            kind,
            handle: Some(HANDLE.to_owned()),
            admin,
            project: None,
        }
    }

    fn account(owner: bool) -> Account {
        Account {
            handle: HANDLE.to_owned(),
            owner,
            session: SESSION.to_owned(),
            csrf: csrf_for(SESSION),
        }
    }

    fn project(id: &str, role: rupicola_api::Role) -> Project {
        Project {
            id: id.to_owned(),
            label: id.to_owned(),
            created_at: 0,
            role: Some(role),
        }
    }

    #[tokio::test]
    async fn a_session_becomes_an_account_with_a_csrf_token_of_its_own() {
        let mut api = MockApi::new();
        api.expect_me()
            .times(1)
            .returning(|_| Ok(me(TokenKind::Session, true)));
        let account = identify(&api, SESSION).await.unwrap();
        assert_eq!(account.handle, HANDLE);
        assert!(account.owner);
        assert_eq!(account.session, SESSION);
        assert_eq!(account.csrf, csrf_for(SESSION));
        assert_ne!(csrf_for(SESSION), csrf_for(OTHER_SESSION));
        assert!(check_csrf(&account, &csrf_for(SESSION)).is_ok());
        assert_eq!(
            check_csrf(&account, &csrf_for(OTHER_SESSION)),
            Err(StatusCode::FORBIDDEN)
        );
    }

    #[tokio::test]
    async fn only_a_persons_session_is_an_account() {
        for kind in [TokenKind::Machine, TokenKind::Device] {
            let mut api = MockApi::new();
            api.expect_me().returning(move |_| Ok(me(kind, false)));
            assert_eq!(
                identify(&api, SESSION).await.err(),
                Some(StatusCode::UNAUTHORIZED)
            );
        }
    }

    #[tokio::test]
    async fn what_the_backend_says_becomes_a_status() {
        let mut api = MockApi::new();
        api.expect_me()
            .returning(|_| Err(ClientError::Unauthorized));
        assert_eq!(
            identify(&api, SESSION).await.err(),
            Some(StatusCode::UNAUTHORIZED)
        );
        let mut api = MockApi::new();
        api.expect_me()
            .returning(|_| Err(ClientError::Unavailable("down".to_owned())));
        assert_eq!(
            identify(&api, SESSION).await.err(),
            Some(StatusCode::BAD_GATEWAY)
        );
    }

    #[test]
    fn an_administrator_owns_every_project_without_membership() {
        assert_eq!(role_in(&[], &account(true), PROJECT), Ok(Role::Owner));
    }

    #[test]
    fn a_member_has_the_role_the_backend_gives_and_a_stranger_finds_nothing() {
        let projects = [project(PROJECT, rupicola_api::Role::Reviewer)];
        assert_eq!(
            role_in(&projects, &account(false), PROJECT),
            Ok(Role::Reviewer)
        );
        assert_eq!(
            role_in(&projects, &account(false), OTHER_PROJECT),
            Err(StatusCode::NOT_FOUND)
        );
    }
}
