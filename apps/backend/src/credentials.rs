//! Credentials other than signing in: project invitations (and registering with
//! one), machine tokens for agents, capture devices, and a person's own sessions.
//! Secrets are shown once; only their SHA-256 is stored and used as the id.

use crate::{
    auth::{self, Principal, now_ms, token},
    error::ApiError,
    projects::{access, audit, parse_role, person, role_text},
    state::Shared,
};
use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use rupicola_api::{
    Credential, Invite, IssuedCredential, IssuedInvite, NewCredential, NewInvite, Redeem, Register,
    Role, SessionToken, TokenKind,
};

const HOUR_MS: i64 = 60 * 60 * 1000;
/// Longest label of a machine token or device.
const MAX_LABEL_CHARS: usize = 100;
/// Validity of an invitation when the owner does not choose one.
const DEFAULT_INVITE_HOURS: u32 = 24;
/// Shortest and longest validity of an invitation (up to one week).
const INVITE_HOURS: std::ops::RangeInclusive<i64> = 1..=168;
/// Lockout key for invalid invitation codes, which name no account.
const INVITE_LOCKOUT_KEY: &str = "__register__";

fn clean_label(label: &str) -> Result<String, ApiError> {
    let label = label.trim();
    if (1..=MAX_LABEL_CHARS).contains(&label.chars().count()) {
        Ok(label.to_owned())
    } else {
        Err(ApiError::invalid(format!(
            "the label must have 1–{MAX_LABEL_CHARS} characters"
        )))
    }
}

fn kind_text(kind: TokenKind) -> &'static str {
    match kind {
        TokenKind::Session => "session",
        TokenKind::Machine => "machine",
        TokenKind::Device => "device",
    }
}

fn parse_kind(text: &str) -> Option<TokenKind> {
    match text {
        "session" => Some(TokenKind::Session),
        "machine" => Some(TokenKind::Machine),
        "device" => Some(TokenKind::Device),
        _ => None,
    }
}

type CredentialRow = (
    String,
    String,
    String,
    Option<String>,
    i64,
    Option<i64>,
    Option<i64>,
);

fn credential(
    (id, kind, label, project_id, created_at, expires_at, last_used_at): CredentialRow,
) -> Option<Credential> {
    Some(Credential {
        id,
        kind: parse_kind(&kind)?,
        label,
        project_id,
        created_at,
        expires_at,
        last_used_at,
    })
}

/// Inserts a new non-expiring credential and returns its one-time secret.
async fn issue(
    state: &Shared,
    kind: TokenKind,
    handle: Option<&str>,
    project: Option<&str>,
    label: String,
) -> Result<IssuedCredential, ApiError> {
    let secret = token();
    let credential = Credential {
        id: auth::digest(&secret),
        kind,
        label,
        project_id: project.map(str::to_owned),
        created_at: now_ms(),
        expires_at: None,
        last_used_at: None,
    };
    let mut tx = state.db.begin().await?;
    sqlx::query(
        "INSERT INTO tokens (digest, kind, handle, project_id, label, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
    )
    .bind(&credential.id)
    .bind(kind_text(kind))
    .bind(handle)
    .bind(project)
    .bind(&credential.label)
    .bind(credential.created_at)
    .execute(&mut *tx)
    .await?;
    let actor = handle.unwrap_or("system");
    let event = match kind {
        TokenKind::Device => format!("Registered device {}", credential.label),
        _ => format!("Created machine token {}", credential.label),
    };
    audit(&mut *tx, project, actor, &event).await?;
    tx.commit().await?;
    Ok(IssuedCredential {
        token: secret,
        credential,
    })
}

/// Only a signed-in person may manage credentials: a machine token or device
/// cannot mint or list others.
fn session_person(principal: &Principal) -> Result<&str, ApiError> {
    if principal.kind == TokenKind::Session {
        person(principal)
    } else {
        Err(ApiError::Forbidden)
    }
}

// ---- machine tokens and sessions ------------------------------------------------

/// `POST /v1/tokens`: a machine token for an agent acting as the caller.
pub async fn create_token(
    State(state): State<Shared>,
    principal: Principal,
    Json(form): Json<NewCredential>,
) -> Result<(StatusCode, Json<IssuedCredential>), ApiError> {
    let handle = session_person(&principal)?;
    let label = clean_label(&form.label)?;
    let issued = issue(&state, TokenKind::Machine, Some(handle), None, label).await?;
    Ok((StatusCode::CREATED, Json(issued)))
}

/// `GET /v1/tokens`: the caller's sessions and machine tokens.
pub async fn list_tokens(
    State(state): State<Shared>,
    principal: Principal,
) -> Result<Json<Vec<Credential>>, ApiError> {
    let handle = session_person(&principal)?;
    let rows: Vec<CredentialRow> = sqlx::query_as(
        "SELECT digest, kind, label, project_id, created_at, expires_at, last_used_at
         FROM tokens WHERE handle = ?1 AND (expires_at IS NULL OR expires_at > ?2)
         ORDER BY created_at DESC",
    )
    .bind(handle)
    .bind(now_ms())
    .fetch_all(&state.db)
    .await?;
    Ok(Json(rows.into_iter().filter_map(credential).collect()))
}

/// `DELETE /v1/tokens/{id}`: revoke one of the caller's sessions or machine tokens.
pub async fn delete_token(
    State(state): State<Shared>,
    principal: Principal,
    Path(id): Path<String>,
) -> Result<StatusCode, ApiError> {
    let handle = session_person(&principal)?.to_owned();
    let mut tx = state.db.begin().await?;
    let label: Option<String> =
        sqlx::query_scalar("DELETE FROM tokens WHERE digest = ?1 AND handle = ?2 RETURNING label")
            .bind(&id)
            .bind(&handle)
            .fetch_optional(&mut *tx)
            .await?;
    let label = label.ok_or(ApiError::NotFound)?;
    audit(
        &mut *tx,
        None,
        &handle,
        &format!("Revoked credential {label}"),
    )
    .await?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

// ---- devices -------------------------------------------------------------------

/// `POST /v1/projects/{id}/devices`: owners register a capture device.
pub async fn create_device(
    State(state): State<Shared>,
    principal: Principal,
    Path(project): Path<String>,
    Json(form): Json<NewCredential>,
) -> Result<(StatusCode, Json<IssuedCredential>), ApiError> {
    access(&state, &principal, &project)
        .await?
        .require(Role::Owner)?;
    session_person(&principal)?;
    let label = clean_label(&form.label)?;
    let issued = issue(&state, TokenKind::Device, None, Some(&project), label).await?;
    Ok((StatusCode::CREATED, Json(issued)))
}

/// `GET /v1/projects/{id}/devices`
pub async fn list_devices(
    State(state): State<Shared>,
    principal: Principal,
    Path(project): Path<String>,
) -> Result<Json<Vec<Credential>>, ApiError> {
    access(&state, &principal, &project)
        .await?
        .require(Role::Owner)?;
    let rows: Vec<CredentialRow> = sqlx::query_as(
        "SELECT digest, kind, label, project_id, created_at, expires_at, last_used_at
         FROM tokens WHERE kind = 'device' AND project_id = ?1 ORDER BY created_at DESC",
    )
    .bind(&project)
    .fetch_all(&state.db)
    .await?;
    Ok(Json(rows.into_iter().filter_map(credential).collect()))
}

/// `DELETE /v1/projects/{id}/devices/{device}`
pub async fn delete_device(
    State(state): State<Shared>,
    principal: Principal,
    Path((project, device)): Path<(String, String)>,
) -> Result<StatusCode, ApiError> {
    access(&state, &principal, &project)
        .await?
        .require(Role::Owner)?;
    let actor = session_person(&principal)?.to_owned();
    let mut tx = state.db.begin().await?;
    let label: Option<String> = sqlx::query_scalar(
        "DELETE FROM tokens WHERE digest = ?1 AND kind = 'device' AND project_id = ?2
         RETURNING label",
    )
    .bind(&device)
    .bind(&project)
    .fetch_optional(&mut *tx)
    .await?;
    let label = label.ok_or(ApiError::NotFound)?;
    audit(
        &mut *tx,
        Some(&project),
        &actor,
        &format!("Revoked device {label}"),
    )
    .await?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

// ---- invitations ---------------------------------------------------------------

/// `POST /v1/projects/{id}/invites`: owners invite someone with a role.
pub async fn create_invite(
    State(state): State<Shared>,
    principal: Principal,
    Path(project): Path<String>,
    Json(form): Json<NewInvite>,
) -> Result<(StatusCode, Json<IssuedInvite>), ApiError> {
    access(&state, &principal, &project)
        .await?
        .require(Role::Owner)?;
    let actor = session_person(&principal)?.to_owned();
    let hours = i64::from(form.hours.unwrap_or(DEFAULT_INVITE_HOURS));
    if !INVITE_HOURS.contains(&hours) {
        return Err(ApiError::invalid(format!(
            "an invitation lasts {}–{} hours",
            INVITE_HOURS.start(),
            INVITE_HOURS.end()
        )));
    }
    let code = token();
    let invite = Invite {
        id: auth::digest(&code),
        role: form.role,
        created_by: Some(actor.clone()),
        expires_at: now_ms() + hours * HOUR_MS,
    };
    let mut tx = state.db.begin().await?;
    sqlx::query(
        "INSERT INTO invites (digest, project_id, role, created_by, expires_at)
         VALUES (?1, ?2, ?3, ?4, ?5)",
    )
    .bind(&invite.id)
    .bind(&project)
    .bind(role_text(invite.role))
    .bind(&actor)
    .bind(invite.expires_at)
    .execute(&mut *tx)
    .await?;
    audit(
        &mut *tx,
        Some(&project),
        &actor,
        &format!("Invited a {}", role_text(invite.role)),
    )
    .await?;
    tx.commit().await?;
    Ok((StatusCode::CREATED, Json(IssuedInvite { code, invite })))
}

/// `GET /v1/projects/{id}/invites`: pending invitations.
pub async fn list_invites(
    State(state): State<Shared>,
    principal: Principal,
    Path(project): Path<String>,
) -> Result<Json<Vec<Invite>>, ApiError> {
    access(&state, &principal, &project)
        .await?
        .require(Role::Owner)?;
    let rows: Vec<(String, String, Option<String>, i64)> = sqlx::query_as(
        "SELECT digest, role, created_by, expires_at FROM invites
         WHERE project_id = ?1 AND expires_at > ?2 ORDER BY expires_at",
    )
    .bind(&project)
    .bind(now_ms())
    .fetch_all(&state.db)
    .await?;
    Ok(Json(
        rows.into_iter()
            .filter_map(|(id, role, created_by, expires_at)| {
                Some(Invite {
                    id,
                    role: parse_role(&role)?,
                    created_by,
                    expires_at,
                })
            })
            .collect(),
    ))
}

/// `DELETE /v1/projects/{id}/invites/{invite}`
pub async fn delete_invite(
    State(state): State<Shared>,
    principal: Principal,
    Path((project, invite)): Path<(String, String)>,
) -> Result<StatusCode, ApiError> {
    access(&state, &principal, &project)
        .await?
        .require(Role::Owner)?;
    let actor = session_person(&principal)?.to_owned();
    let mut tx = state.db.begin().await?;
    let removed = sqlx::query("DELETE FROM invites WHERE digest = ?1 AND project_id = ?2")
        .bind(&invite)
        .bind(&project)
        .execute(&mut *tx)
        .await?
        .rows_affected();
    if removed == 0 {
        return Err(ApiError::NotFound);
    }
    audit(&mut *tx, Some(&project), &actor, "Withdrew an invitation").await?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Consumes a valid invitation inside `tx`, returning its project and role.
async fn take_invite(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    code: &str,
) -> Result<Option<(String, String)>, ApiError> {
    Ok(sqlx::query_as(
        "DELETE FROM invites WHERE digest = ?1 AND expires_at > ?2
         RETURNING project_id, role",
    )
    .bind(auth::digest(code.trim()))
    .bind(now_ms())
    .fetch_optional(&mut **tx)
    .await?)
}

/// `POST /v1/register`: create an account with an invitation code and sign in.
pub async fn register(
    State(state): State<Shared>,
    Json(form): Json<Register>,
) -> Result<(StatusCode, Json<SessionToken>), ApiError> {
    if let Some(retry_after) = state.lockout.retry_after(INVITE_LOCKOUT_KEY).await {
        return Err(ApiError::TooManyRequests { retry_after });
    }
    let handle = form.handle.trim().to_ascii_lowercase();
    if !auth::valid_handle(&handle) || !auth::PASSWORD_LEN.contains(&form.password.len()) {
        return Err(auth::account_rules());
    }
    let password = state.passwords.hash(form.password).await?;
    let mut tx = state.db.begin().await?;
    let Some((project, role)) = take_invite(&mut tx, &form.code).await? else {
        drop(tx);
        state.lockout.fail(INVITE_LOCKOUT_KEY).await;
        return Err(ApiError::Forbidden);
    };
    let created = sqlx::query(
        "INSERT INTO accounts (handle, password_hash, admin, created_at) VALUES (?1, ?2, 0, ?3)
         ON CONFLICT(handle) DO NOTHING",
    )
    .bind(&handle)
    .bind(&password)
    .bind(now_ms())
    .execute(&mut *tx)
    .await?
    .rows_affected();
    if created == 0 {
        // Rolling back keeps the invitation usable with another username.
        return Err(ApiError::Conflict("this username is taken"));
    }
    sqlx::query("INSERT INTO members (project_id, handle, role) VALUES (?1, ?2, ?3)")
        .bind(&project)
        .bind(&handle)
        .bind(&role)
        .execute(&mut *tx)
        .await?;
    audit(
        &mut *tx,
        Some(&project),
        &handle,
        &format!("Joined as {role}"),
    )
    .await?;
    tx.commit().await?;
    Ok((
        StatusCode::CREATED,
        Json(auth::issue_session(&state, &handle).await?),
    ))
}

/// `POST /v1/invites/redeem`: a signed-in person joins the inviting project. An
/// existing membership keeps the higher of the two roles.
pub async fn redeem(
    State(state): State<Shared>,
    principal: Principal,
    Json(form): Json<Redeem>,
) -> Result<Json<rupicola_api::Project>, ApiError> {
    let handle = session_person(&principal)?.to_owned();
    if let Some(retry_after) = state.lockout.retry_after(INVITE_LOCKOUT_KEY).await {
        return Err(ApiError::TooManyRequests { retry_after });
    }
    let mut tx = state.db.begin().await?;
    let Some((project, role)) = take_invite(&mut tx, &form.code).await? else {
        drop(tx);
        state.lockout.fail(INVITE_LOCKOUT_KEY).await;
        return Err(ApiError::Forbidden);
    };
    let current: Option<String> =
        sqlx::query_scalar("SELECT role FROM members WHERE project_id = ?1 AND handle = ?2")
            .bind(&project)
            .bind(&handle)
            .fetch_optional(&mut *tx)
            .await?;
    let offered = parse_role(&role).ok_or(ApiError::invalid("unknown role"))?;
    let kept = current
        .as_deref()
        .and_then(parse_role)
        .map_or(offered, |held| held.max(offered));
    sqlx::query(
        "INSERT INTO members (project_id, handle, role) VALUES (?1, ?2, ?3)
         ON CONFLICT(project_id, handle) DO UPDATE SET role = excluded.role",
    )
    .bind(&project)
    .bind(&handle)
    .bind(role_text(kept))
    .execute(&mut *tx)
    .await?;
    audit(
        &mut *tx,
        Some(&project),
        &handle,
        &format!("Joined as {}", role_text(kept)),
    )
    .await?;
    tx.commit().await?;
    crate::projects::get(State(state), principal, Path(project)).await
}

#[cfg(test)]
mod tests {
    use crate::testkit::*;
    use axum::http::StatusCode;
    use rupicola_api::{
        Credential, IssuedCredential, IssuedInvite, Me, Member, Project, Role, TokenKind,
    };

    /// Ana owns `demo`. Returns the port, the state and Ana's session.
    async fn demo() -> (u16, crate::state::Shared, String) {
        let (port, state) = start_custom(settings(), None, true).await;
        seed_account(&state, "ana", false).await;
        let ana = sign_in(port, "ana").await;
        call(
            port,
            "POST",
            "/v1/projects",
            Some(&ana),
            Some(r#"{"id":"demo","label":"Demo"}"#),
        )
        .await;
        (port, state, ana)
    }

    async fn invite(port: u16, owner: &str, role: &str) -> String {
        let issued = call(
            port,
            "POST",
            "/v1/projects/demo/invites",
            Some(owner),
            Some(&format!(r#"{{"role":"{role}"}}"#)),
        )
        .await;
        assert_eq!(status(&issued), StatusCode::CREATED, "{issued}");
        json::<IssuedInvite>(&issued).code
    }

    fn registration(code: &str, handle: &str) -> String {
        format!(r#"{{"code":"{code}","handle":"{handle}","password":"{PASSWORD}"}}"#)
    }

    #[tokio::test]
    async fn an_invitation_registers_one_account_into_the_project() {
        let (port, _state, ana) = demo().await;
        let code = invite(port, &ana, "reviewer").await;

        let taken = call(
            port,
            "POST",
            "/v1/register",
            None,
            Some(&registration(&code, "ana")),
        )
        .await;
        assert_eq!(status(&taken), StatusCode::CONFLICT, "{taken}");
        // A taken username does not burn the code.
        let joined = call(
            port,
            "POST",
            "/v1/register",
            None,
            Some(&registration(&code, "bruno")),
        )
        .await;
        assert_eq!(status(&joined), StatusCode::CREATED, "{joined}");
        let bruno = token_of(&joined);
        let project: Project =
            json(&call(port, "GET", "/v1/projects/demo", Some(&bruno), None).await);
        assert_eq!(project.role, Some(Role::Reviewer));

        let reused = call(
            port,
            "POST",
            "/v1/register",
            None,
            Some(&registration(&code, "carla")),
        )
        .await;
        assert_eq!(status(&reused), StatusCode::FORBIDDEN, "{reused}");
    }

    #[tokio::test]
    async fn expired_or_withdrawn_invitations_do_not_work() {
        let (port, state, ana) = demo().await;
        let expired = invite(port, &ana, "reader").await;
        sqlx::query("UPDATE invites SET expires_at = 0")
            .execute(&state.db)
            .await
            .unwrap();
        let late = call(
            port,
            "POST",
            "/v1/register",
            None,
            Some(&registration(&expired, "bruno")),
        )
        .await;
        assert_eq!(status(&late), StatusCode::FORBIDDEN, "{late}");

        let withdrawn = invite(port, &ana, "reader").await;
        let pending: Vec<rupicola_api::Invite> =
            json(&call(port, "GET", "/v1/projects/demo/invites", Some(&ana), None).await);
        assert_eq!(pending.len(), 1);
        let gone = call(
            port,
            "DELETE",
            &format!("/v1/projects/demo/invites/{}", pending[0].id),
            Some(&ana),
            None,
        )
        .await;
        assert_eq!(status(&gone), StatusCode::NO_CONTENT, "{gone}");
        let refused = call(
            port,
            "POST",
            "/v1/register",
            None,
            Some(&registration(&withdrawn, "bruno")),
        )
        .await;
        assert_eq!(status(&refused), StatusCode::FORBIDDEN, "{refused}");
    }

    #[tokio::test]
    async fn redeeming_keeps_the_higher_role() {
        let (port, state, ana) = demo().await;
        seed_account(&state, "bruno", false).await;
        let bruno = sign_in(port, "bruno").await;
        call(
            port,
            "PUT",
            "/v1/projects/demo/members/bruno",
            Some(&ana),
            Some(r#"{"role":"reviewer"}"#),
        )
        .await;
        let code = invite(port, &ana, "reader").await;

        let joined = call(
            port,
            "POST",
            "/v1/invites/redeem",
            Some(&bruno),
            Some(&format!(r#"{{"code":"{code}"}}"#)),
        )
        .await;
        assert_eq!(status(&joined), StatusCode::OK, "{joined}");
        let members: Vec<Member> =
            json(&call(port, "GET", "/v1/projects/demo/members", Some(&ana), None).await);
        assert!(members.contains(&Member {
            handle: "bruno".into(),
            role: Role::Reviewer
        }));
    }

    #[tokio::test]
    async fn machine_tokens_act_for_their_owner_until_revoked() {
        let (port, _state, ana) = demo().await;
        let issued: IssuedCredential = json(
            &call(
                port,
                "POST",
                "/v1/tokens",
                Some(&ana),
                Some(r#"{"label":"Laptop agent"}"#),
            )
            .await,
        );
        let machine = issued.token;

        let me: Me = json(&call(port, "GET", "/v1/me", Some(&machine), None).await);
        assert_eq!(
            (me.kind, me.handle.as_deref()),
            (TokenKind::Machine, Some("ana"))
        );
        let project: Project =
            json(&call(port, "GET", "/v1/projects/demo", Some(&machine), None).await);
        assert_eq!(project.role, Some(Role::Owner));
        // An agent cannot mint more credentials.
        let minted = call(
            port,
            "POST",
            "/v1/tokens",
            Some(&machine),
            Some(r#"{"label":"Another"}"#),
        )
        .await;
        assert_eq!(status(&minted), StatusCode::FORBIDDEN, "{minted}");

        let listed: Vec<Credential> =
            json(&call(port, "GET", "/v1/tokens", Some(&ana), None).await);
        assert!(
            listed
                .iter()
                .any(|item| item.id == issued.credential.id && item.label == "Laptop agent")
        );
        let revoked = call(
            port,
            "DELETE",
            &format!("/v1/tokens/{}", issued.credential.id),
            Some(&ana),
            None,
        )
        .await;
        assert_eq!(status(&revoked), StatusCode::NO_CONTENT, "{revoked}");
        let after = call(port, "GET", "/v1/me", Some(&machine), None).await;
        assert_eq!(status(&after), StatusCode::UNAUTHORIZED, "{after}");
    }

    #[tokio::test]
    async fn a_device_only_publishes_to_its_own_project() {
        let (port, _state, ana) = demo().await;
        call(
            port,
            "POST",
            "/v1/projects",
            Some(&ana),
            Some(r#"{"id":"other","label":"Other"}"#),
        )
        .await;
        let issued: IssuedCredential = json(
            &call(
                port,
                "POST",
                "/v1/projects/demo/devices",
                Some(&ana),
                Some(r#"{"label":"CI runner"}"#),
            )
            .await,
        );
        let device = issued.token;

        let me: Me = json(&call(port, "GET", "/v1/me", Some(&device), None).await);
        assert_eq!(
            (me.kind, me.project.as_deref()),
            (TokenKind::Device, Some("demo"))
        );
        let blocked = r#"{"scenario":"ci","status":"blocked","blocker":"server down"}"#;
        let published = call(
            port,
            "POST",
            "/v1/projects/demo/captures",
            Some(&device),
            Some(blocked),
        )
        .await;
        assert_eq!(status(&published), StatusCode::CREATED, "{published}");
        let read = call(
            port,
            "GET",
            "/v1/projects/demo/captures",
            Some(&device),
            None,
        )
        .await;
        assert_eq!(status(&read), StatusCode::FORBIDDEN, "{read}");
        let elsewhere = call(
            port,
            "POST",
            "/v1/projects/other/captures",
            Some(&device),
            Some(blocked),
        )
        .await;
        assert_eq!(status(&elsewhere), StatusCode::NOT_FOUND, "{elsewhere}");

        let revoked = call(
            port,
            "DELETE",
            &format!("/v1/projects/demo/devices/{}", issued.credential.id),
            Some(&ana),
            None,
        )
        .await;
        assert_eq!(status(&revoked), StatusCode::NO_CONTENT, "{revoked}");
        let after = call(port, "GET", "/v1/me", Some(&device), None).await;
        assert_eq!(status(&after), StatusCode::UNAUTHORIZED, "{after}");
    }

    #[tokio::test]
    async fn only_owners_issue_project_credentials() {
        let (port, state, ana) = demo().await;
        seed_account(&state, "bruno", false).await;
        let bruno = sign_in(port, "bruno").await;
        call(
            port,
            "PUT",
            "/v1/projects/demo/members/bruno",
            Some(&ana),
            Some(r#"{"role":"reviewer"}"#),
        )
        .await;
        for (path, body) in [
            ("/v1/projects/demo/invites", r#"{"role":"owner"}"#),
            ("/v1/projects/demo/devices", r#"{"label":"Sneaky"}"#),
        ] {
            let response = call(port, "POST", path, Some(&bruno), Some(body)).await;
            assert_eq!(
                status(&response),
                StatusCode::FORBIDDEN,
                "{path}: {response}"
            );
        }
    }
}
