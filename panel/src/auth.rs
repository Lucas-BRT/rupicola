use super::*;
use crate::components::button::button;
use argon2::{
    Argon2, PasswordHash, PasswordHasher, PasswordVerifier,
    password_hash::{SaltString, rand_core::OsRng},
};
use http::{HeaderMap, HeaderValue, Method};
use rand::RngCore;
use sha2::{Digest, Sha256};
use std::io::Read;
use subtle::ConstantTimeEq;
use topcoat::view::{View, attributes};

const SESSION_AGE: i64 = 7 * 24 * 60 * 60;

#[derive(Clone)]
pub struct Account {
    pub handle: String,
    pub owner: bool,
    pub session: String,
    pub csrf: String,
}

pub async fn init(db: &Connection) -> Result<(), BoxError> {
    db.execute_batch(
        "CREATE TABLE IF NOT EXISTS accounts (
            handle TEXT PRIMARY KEY, password TEXT NOT NULL, owner INTEGER NOT NULL
        );
        CREATE TABLE IF NOT EXISTS sessions (
            id TEXT PRIMARY KEY, handle TEXT NOT NULL, csrf TEXT NOT NULL, expires INTEGER NOT NULL
        );
        CREATE TABLE IF NOT EXISTS projects (
            id TEXT PRIMARY KEY, label TEXT NOT NULL
        );
        CREATE TABLE IF NOT EXISTS members (
            project TEXT NOT NULL, handle TEXT NOT NULL, role TEXT NOT NULL, PRIMARY KEY(project, handle)
        );
        CREATE TABLE IF NOT EXISTS invites (
            digest TEXT PRIMARY KEY, project TEXT NOT NULL, role TEXT NOT NULL, expires INTEGER NOT NULL
        );
        CREATE TABLE IF NOT EXISTS devices (
            digest TEXT PRIMARY KEY, project TEXT NOT NULL, label TEXT NOT NULL, created INTEGER NOT NULL,
            last_seen INTEGER
        );
        CREATE TABLE IF NOT EXISTS audit (
            id INTEGER PRIMARY KEY, project TEXT NOT NULL, actor TEXT NOT NULL, event TEXT NOT NULL,
            occurred INTEGER NOT NULL
        );",
    ).await?;
    Ok(())
}

pub fn token() -> String {
    let mut bytes = [0u8; 32];
    OsRng.fill_bytes(&mut bytes);
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

pub fn digest(secret: &str) -> String {
    Sha256::digest(secret.as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

pub fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
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
    if path.starts_with("/api/publish/") {
        return Ok(None);
    }
    let Some(id) = cookie(headers, "rp_session") else {
        return Err(StatusCode::UNAUTHORIZED);
    };
    let id = digest(&id);
    let account = {
        let db = panel.index.lock().await;
        let mut rows = db.query(
            "SELECT a.handle, a.owner, s.csrf FROM sessions s JOIN accounts a ON s.handle = a.handle
             WHERE s.id = ? AND s.expires > ?",
            Vec::from(turso::params![id.clone(), now()]),
        ).await.map_err(db_error)?;
        if let Some(row) = rows.next().await.map_err(db_error)? {
            Some(Account {
                handle: row.get(0).map_err(db_error)?,
                owner: row.get::<i64>(1).map_err(db_error)? == 1,
                csrf: row.get(2).map_err(db_error)?,
                session: id,
            })
        } else {
            None
        }
    };
    let Some(account) = account else {
        return Err(StatusCode::UNAUTHORIZED);
    };
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

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Role {
    Owner,
    Reviewer,
    Reader,
}

pub async fn role(panel: &Panel, account: &Account, project: &str) -> PageResult<Role> {
    if account.owner {
        return Ok(Role::Owner);
    }
    let db = panel.index.lock().await;
    let mut rows = db
        .query(
            "SELECT role FROM members WHERE project = ? AND handle = ?",
            [project, account.handle.as_str()],
        )
        .await
        .map_err(db_error)?;
    match rows
        .next()
        .await
        .map_err(db_error)?
        .map(|row| row.get::<String>(0))
        .transpose()
        .map_err(db_error)?
        .as_deref()
    {
        Some("reviewer") => Ok(Role::Reviewer),
        Some("reader") => Ok(Role::Reader),
        _ => Err(StatusCode::NOT_FOUND),
    }
}

pub async fn readable_projects(panel: &Panel, account: &Account) -> PageResult<Vec<String>> {
    let db = panel.index.lock().await;
    let mut rows = db
        .query(
            "SELECT project FROM members WHERE handle = ?",
            [account.handle.as_str()],
        )
        .await
        .map_err(db_error)?;
    let mut projects = Vec::new();
    while let Some(row) = rows.next().await.map_err(db_error)? {
        projects.push(row.get(0).map_err(db_error)?);
    }
    Ok(projects)
}

pub async fn owner_exists(db: &Connection) -> Result<bool, BoxError> {
    Ok(db
        .query("SELECT 1 FROM accounts WHERE owner = 1 LIMIT 1", ())
        .await?
        .next()
        .await?
        .is_some())
}

async fn has_owner(panel: &Panel) -> PageResult<bool> {
    let db = panel.index.lock().await;
    owner_exists(&db).await.map_err(db_error)
}

pub async fn login_page(panel: Arc<Panel>) -> PageResult<Response> {
    if !has_owner(&panel).await? {
        return Ok(redirect("/register"));
    }
    render(&panel, false, false, "", None, StatusCode::OK).await
}

pub async fn register_page(panel: Arc<Panel>) -> PageResult<Response> {
    render(
        &panel,
        true,
        !has_owner(&panel).await?,
        "",
        None,
        StatusCode::OK,
    )
    .await
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
        || cookie(headers, "rp_form").is_none_or(|token| !equal(&token, &form.csrf))
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

const RATE_WINDOW_SECS: i64 = 300;
const MAX_FAILED_ATTEMPTS: u32 = 8;

async fn is_rate_limited(panel: &Panel, key: &str) -> bool {
    let mut map = panel.login_attempts.lock().await;
    let current = now();
    map.retain(|_, (_, ts)| current - *ts < RATE_WINDOW_SECS);
    map.get(key).is_some_and(|(count, ts)| {
        *count >= MAX_FAILED_ATTEMPTS && current - *ts < RATE_WINDOW_SECS
    })
}

async fn record_auth_failure(panel: &Panel, key: &str) {
    let mut map = panel.login_attempts.lock().await;
    let current = now();
    let entry = map.entry(key.to_owned()).or_insert((0, current));
    if current - entry.1 >= RATE_WINDOW_SECS {
        *entry = (1, current);
    } else {
        entry.0 = entry.0.saturating_add(1);
    }
}

async fn clear_auth_failures(panel: &Panel, key: &str) {
    panel.login_attempts.lock().await.remove(key);
}

pub async fn login(
    panel: Arc<Panel>,
    headers: HeaderMap,
    form: Credentials,
) -> PageResult<Response> {
    check_form(&panel, &headers, &form)?;
    let handle = form.handle.trim().to_ascii_lowercase();
    if is_rate_limited(&panel, &handle).await {
        return render(
            &panel,
            false,
            false,
            &handle,
            Some("Too many failed sign-in attempts. Wait 5 minutes before trying again."),
            StatusCode::TOO_MANY_REQUESTS,
        )
        .await;
    }
    let failure = "The username or password is incorrect.";
    if !valid_handle(&handle) || form.password.len() > 1024 {
        return render(
            &panel,
            false,
            false,
            &handle,
            Some(failure),
            StatusCode::UNAUTHORIZED,
        )
        .await;
    }
    let stored = {
        let db = panel.index.lock().await;
        let mut rows = db
            .query(
                "SELECT password FROM accounts WHERE handle = ?",
                [handle.as_str()],
            )
            .await
            .map_err(db_error)?;
        rows.next()
            .await
            .map_err(db_error)?
            .map(|row| row.get::<String>(0))
            .transpose()
            .map_err(db_error)?
    };
    let exists = stored.is_some();
    let hash = stored.unwrap_or_else(|| panel.dummy_password.clone());
    let permit = panel
        .password_work
        .clone()
        .try_acquire_owned()
        .map_err(|_| StatusCode::TOO_MANY_REQUESTS)?;
    let verified = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        PasswordHash::new(&hash).is_ok_and(|hash| {
            Argon2::default()
                .verify_password(form.password.as_bytes(), &hash)
                .is_ok()
        })
    })
    .await
    .map_err(db_error)?;
    if !exists || !verified {
        record_auth_failure(&panel, &handle).await;
        return render(
            &panel,
            false,
            false,
            &handle,
            Some(failure),
            StatusCode::UNAUTHORIZED,
        )
        .await;
    }
    clear_auth_failures(&panel, &handle).await;
    create_session(&panel, &handle).await
}

fn valid_handle(handle: &str) -> bool {
    (3..=48).contains(&handle.len()) && safe_name(handle)
}

pub fn password_hash(password: &str) -> Result<String, argon2::password_hash::Error> {
    Argon2::default()
        .hash_password(password.as_bytes(), &SaltString::generate(&mut OsRng))
        .map(|hash| hash.to_string())
}

pub async fn register(
    panel: Arc<Panel>,
    headers: HeaderMap,
    form: Credentials,
) -> PageResult<Response> {
    check_form(&panel, &headers, &form)?;
    let handle = form.handle.trim().to_ascii_lowercase();
    let initial = !has_owner(&panel).await?;
    if is_rate_limited(&panel, "__register__").await {
        return render(
            &panel,
            true,
            initial,
            &handle,
            Some("Too many registration attempts. Wait 5 minutes before trying again."),
            StatusCode::TOO_MANY_REQUESTS,
        )
        .await;
    }
    if !valid_handle(&handle) || !(12..=1024).contains(&form.password.len()) {
        return render(&panel, true, initial, &handle,
            Some("Use a username with 3–48 letters, numbers, hyphens or underscores and a password of at least 12 characters."),
            StatusCode::UNPROCESSABLE_ENTITY).await;
    }
    let code = form.code.as_deref().unwrap_or("").trim();
    let invite = digest(code);
    if initial
        && (panel.secure || panel.bootstrap.is_some())
        && panel
            .bootstrap
            .as_ref()
            .is_none_or(|expected| !equal(expected, &invite))
    {
        record_auth_failure(&panel, "__register__").await;
        return render(
            &panel,
            true,
            initial,
            &handle,
            Some("The setup code is invalid."),
            StatusCode::FORBIDDEN,
        )
        .await;
    }
    let permit = panel
        .password_work
        .clone()
        .try_acquire_owned()
        .map_err(|_| StatusCode::TOO_MANY_REQUESTS)?;
    let password = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        password_hash(&form.password)
    })
    .await
    .map_err(db_error)?
    .map_err(db_error)?;
    let created = {
        let db = panel.index.lock().await;
        if initial {
            db.execute(
                "INSERT OR IGNORE INTO accounts SELECT ?, ?, 1 WHERE NOT EXISTS (SELECT 1 FROM accounts WHERE owner = 1)",
                [handle.as_str(), password.as_str()],
            ).await.map_err(db_error)? != 0
        } else {
            let expires = now();
            let results = db.transactional_batch([
                ("INSERT OR IGNORE INTO accounts SELECT ?, ?, 0 WHERE EXISTS (SELECT 1 FROM invites WHERE digest = ? AND expires > ?)",
                 Vec::from(turso::params![handle.clone(), password, invite.clone(), expires])),
                ("INSERT INTO members SELECT project, ?, role FROM invites WHERE digest = ? AND expires > ? AND changes() = 1",
                 Vec::from(turso::params![handle.clone(), invite.clone(), expires])),
                ("DELETE FROM invites WHERE digest = ? AND changes() = 1", Vec::from(turso::params![invite])),
            ], TransactionBehavior::Immediate).await.map_err(db_error)?;
            results[0].rows_affected() != 0
        }
    };
    if !created {
        return render(
            &panel,
            true,
            initial,
            &handle,
            Some("This setup or invitation could not be used. Check the code and username."),
            StatusCode::CONFLICT,
        )
        .await;
    }
    create_session(&panel, &handle).await
}

async fn create_session(panel: &Panel, handle: &str) -> PageResult<Response> {
    let session = token();
    let db = panel.index.lock().await;
    db.execute("DELETE FROM sessions WHERE expires <= ?", [now()])
        .await
        .map_err(db_error)?;
    db.execute(
        "INSERT INTO sessions VALUES (?, ?, ?, ?)",
        Vec::from(turso::params![
            digest(&session),
            handle.to_owned(),
            token(),
            now() + SESSION_AGE
        ]),
    )
    .await
    .map_err(db_error)?;
    let mut response = redirect("/");
    response.headers_mut().append(
        header::SET_COOKIE,
        cookie_header(panel, "rp_session", &session, SESSION_AGE),
    );
    response
        .headers_mut()
        .append(header::SET_COOKIE, cookie_header(panel, "rp_form", "", 0));
    Ok(response)
}

#[derive(Deserialize)]
pub struct Csrf {
    pub csrf: String,
}

pub async fn logout(panel: Arc<Panel>, account: Account, form: Csrf) -> PageResult<Response> {
    check_csrf(&account, &form.csrf)?;
    let db = panel.index.lock().await;
    db.execute(
        "DELETE FROM sessions WHERE id = ?",
        [account.session.as_str()],
    )
    .await
    .map_err(db_error)?;
    let mut response = redirect("/login");
    response.headers_mut().insert(
        header::SET_COOKIE,
        cookie_header(&panel, "rp_session", "", 0),
    );
    Ok(response)
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
    let form_cookie = cookie_header(panel, "rp_form", &nonce, 1800);
    let title = match (registering, initial) {
        (true, true) => "Set up Rupicola",
        (true, false) => "Accept an invitation",
        _ => "Sign in",
    };
    let cx = &Cx::default();
    let content = view! {
        cx =>
        <!DOCTYPE html>
        <html lang="en">
            <head>
                <meta charset="utf-8">
                <meta name="viewport" content="width=device-width, initial-scale=1">
                <title>(title) " · Rupicola"</title>
                <link rel="stylesheet" href="/panel.css">
                <script src="/panel.js" defer=""></script>
            </head>
            <body class="auth-page">
                <div class="auth-brand">
                    <span class="mark" aria-hidden="true">"R"</span>
                    <strong>"Rupicola"</strong>
                </div>
                <main class="auth-layout">
                    <div class="auth-visual" aria-hidden="true">
                        <div class="viewfinder"><i></i><i></i><i></i><i></i><div class="aperture"></div></div>
                    </div>
                    <div class="auth-content">
                        <h1>(title)</h1>
                        if registering && initial {
                            <p class="muted">"Create the owner account for this workspace."</p>
                        }
                        if let Some(error) = error {
                            <p id="auth-error" class="notice error" role="alert" tabindex="-1" data-form-error="">(error)</p>
                        }
                        <form class="stack-form" method="post" action=(if registering { "/register" } else { "/login" })>
                            <input type="hidden" name="csrf" value=(&nonce)>
                            <label for="handle">"Username"</label>
                            <input id="handle" name="handle" value=(handle) required="" maxlength="48" autocomplete="username"
                                autocapitalize="none" spellcheck="false" if error.is_some() { aria-describedby="auth-error" }>
                            <label for="password">"Password"</label>
                            <input id="password" name="password" required="" type="password" maxlength="1024"
                                autocomplete=(if registering { "new-password" } else { "current-password" })
                                if registering { minlength="12" aria-describedby="password-help" }>
                            if registering { <small id="password-help">"At least 12 characters."</small> }
                            if registering && (!initial || panel.secure || panel.bootstrap.is_some()) {
                                <label for="code">(if initial { "Setup code" } else { "Invitation code" })</label>
                                <input id="code" name="code" required="" type="password" maxlength="128" autocomplete="off">
                            }
                            button(attrs: attributes! { type="submit" class="mt-3 w-full" }, (if registering { "Create account" } else { "Sign in" }))
                        </form>
                        if !registering {
                            <details class="recovery"><summary>"Need to reset your password?"</summary>
                                <p class="muted">"Ask the workspace operator to reset your password from the panel host."</p>
                            </details>
                        }
                        <a href=(if registering { "/login" } else { "/register" })>
                            (if registering { "Return to sign in" } else { "Have an invitation?" })
                        </a>
                    </div>
                </main>
            </body>
        </html>
    }.single().await.map_err(db_error)?;
    let mut response = Html(content.render(cx))
        .into_response(cx)
        .map_err(db_error)?;
    *response.status_mut() = status;
    response
        .headers_mut()
        .insert(header::SET_COOKIE, form_cookie);
    Ok(response)
}

pub async fn page(cx: &Cx, view: impl View) -> PageResult<Html<String>> {
    let content = view.single().await.map_err(db_error)?;
    Ok(Html(content.render(cx)))
}

pub async fn recover(db: &Connection, username: &str) -> Result<(), BoxError> {
    let handle = username.trim().to_ascii_lowercase();
    if !valid_handle(&handle) {
        return Err("invalid username".into());
    }
    let mut password = String::new();
    std::io::Read::take(std::io::stdin().lock(), 1026).read_to_string(&mut password)?;
    let password = password.trim_end_matches(['\r', '\n']);
    if !(12..=1024).contains(&password.len()) {
        return Err("supply a 12–1024 byte password on standard input".into());
    }
    let hash = password_hash(password)?;
    let changed = db
        .transactional_batch(
            [
                (
                    "UPDATE accounts SET password = ? WHERE handle = ?",
                    vec![Value::Text(hash), Value::Text(handle.clone())],
                ),
                (
                    "DELETE FROM sessions WHERE handle = ?",
                    vec![Value::Text(handle)],
                ),
            ],
            TransactionBehavior::Immediate,
        )
        .await?;
    if changed[0].rows_affected() == 0 {
        return Err("account not found".into());
    }
    println!("Password reset and sessions revoked.");
    Ok(())
}
