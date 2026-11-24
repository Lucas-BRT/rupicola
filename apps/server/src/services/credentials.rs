//! Credentials other than signing in: project invitations (and registering with
//! one), machine tokens for agents, capture devices, and a person's own sessions.
//! Secrets are shown once; only their SHA-256 is stored and used as the id.

use crate::{
    domain::{
        ApiError, Principal,
        account::{PASSWORD_LEN, account_rules, normal_handle, valid_handle},
        credential::{DEFAULT_INVITE_HOURS, HOUR_MS, check_invite_hours, clean_label},
        secret::{self, now_ms},
    },
    repositories::{
        accounts, audit,
        credentials::{self, InviteGrant},
        projects,
    },
    services::{access::require, auth::issue_session},
    state::AppState,
};
use rupicola_api::{
    Credential, Invite, IssuedCredential, IssuedInvite, NewInvite, Project, Register, Role,
    SessionToken, TokenKind,
};
use sqlx::SqliteConnection;

/// Lockout key for invalid invitation codes, which name no account.
const INVITE_LOCKOUT_KEY: &str = "__register__";

/// Inserts a new non-expiring credential and returns its one-time secret.
async fn issue(
    state: &AppState,
    kind: TokenKind,
    handle: Option<&str>,
    project: Option<&str>,
    label: String,
    actor: Option<&str>,
) -> Result<IssuedCredential, ApiError> {
    let token = secret::token();
    let credential = Credential {
        id: secret::digest(&token),
        kind,
        label,
        project_id: project.map(str::to_owned),
        created_at: now_ms(),
        expires_at: None,
        last_used_at: None,
    };
    let mut tx = state.db.begin().await?;
    credentials::insert(&mut *tx, &credential, handle).await?;
    let event = match kind {
        TokenKind::Device => format!("Registered device {}", credential.label),
        _ => format!("Created machine token {}", credential.label),
    };
    audit::record(
        &mut *tx,
        project,
        actor.or(handle).unwrap_or("system"),
        &event,
    )
    .await?;
    tx.commit().await?;
    Ok(IssuedCredential { token, credential })
}

/// A machine token for an agent acting as the caller.
pub async fn create_token(
    state: &AppState,
    principal: &Principal,
    label: &str,
) -> Result<IssuedCredential, ApiError> {
    let handle = principal.session_person()?;
    let label = clean_label(label)?;
    issue(
        state,
        TokenKind::Machine,
        Some(handle),
        None,
        label,
        Some(handle),
    )
    .await
}

/// The caller's sessions and machine tokens.
pub async fn list_tokens(
    state: &AppState,
    principal: &Principal,
) -> Result<Vec<Credential>, ApiError> {
    let handle = principal.session_person()?;
    Ok(credentials::of_person(&state.db, handle, now_ms()).await?)
}

/// Revokes one of the caller's sessions or machine tokens.
pub async fn delete_token(
    state: &AppState,
    principal: &Principal,
    id: &str,
) -> Result<(), ApiError> {
    let handle = principal.session_person()?;
    let mut tx = state.db.begin().await?;
    let label = credentials::delete_of_person(&mut *tx, id, handle)
        .await?
        .ok_or(ApiError::NotFound)?;
    audit::record(
        &mut *tx,
        None,
        handle,
        &format!("Revoked credential {label}"),
    )
    .await?;
    tx.commit().await?;
    Ok(())
}

/// Owners register a capture device.
pub async fn create_device(
    state: &AppState,
    principal: &Principal,
    project: &str,
    label: &str,
) -> Result<IssuedCredential, ApiError> {
    require(state, principal, project, Role::Owner).await?;
    let actor = principal.session_person()?;
    let label = clean_label(label)?;
    issue(
        state,
        TokenKind::Device,
        None,
        Some(project),
        label,
        Some(actor),
    )
    .await
}

pub async fn list_devices(
    state: &AppState,
    principal: &Principal,
    project: &str,
) -> Result<Vec<Credential>, ApiError> {
    require(state, principal, project, Role::Owner).await?;
    Ok(credentials::devices(&state.db, project).await?)
}

pub async fn delete_device(
    state: &AppState,
    principal: &Principal,
    project: &str,
    device: &str,
) -> Result<(), ApiError> {
    require(state, principal, project, Role::Owner).await?;
    let actor = principal.session_person()?;
    let mut tx = state.db.begin().await?;
    let label = credentials::delete_device(&mut *tx, device, project)
        .await?
        .ok_or(ApiError::NotFound)?;
    audit::record(
        &mut *tx,
        Some(project),
        actor,
        &format!("Revoked device {label}"),
    )
    .await?;
    tx.commit().await?;
    Ok(())
}

/// Owners invite someone with a role.
pub async fn create_invite(
    state: &AppState,
    principal: &Principal,
    project: &str,
    form: NewInvite,
) -> Result<IssuedInvite, ApiError> {
    require(state, principal, project, Role::Owner).await?;
    let actor = principal.session_person()?;
    let hours = i64::from(form.hours.unwrap_or(DEFAULT_INVITE_HOURS));
    check_invite_hours(hours)?;
    let code = secret::token();
    let invite = Invite {
        id: secret::digest(&code),
        role: form.role,
        created_by: Some(actor.to_owned()),
        expires_at: now_ms() + hours * HOUR_MS,
    };
    let mut tx = state.db.begin().await?;
    credentials::insert_invite(&mut *tx, &invite, project).await?;
    audit::record(
        &mut *tx,
        Some(project),
        actor,
        &format!("Invited a {}", invite.role.as_str()),
    )
    .await?;
    tx.commit().await?;
    Ok(IssuedInvite { code, invite })
}

/// Pending invitations.
pub async fn list_invites(
    state: &AppState,
    principal: &Principal,
    project: &str,
) -> Result<Vec<Invite>, ApiError> {
    require(state, principal, project, Role::Owner).await?;
    Ok(credentials::invites(&state.db, project, now_ms()).await?)
}

pub async fn delete_invite(
    state: &AppState,
    principal: &Principal,
    project: &str,
    invite: &str,
) -> Result<(), ApiError> {
    require(state, principal, project, Role::Owner).await?;
    let actor = principal.session_person()?;
    let mut tx = state.db.begin().await?;
    if !credentials::delete_invite(&mut *tx, invite, project).await? {
        return Err(ApiError::NotFound);
    }
    audit::record(&mut *tx, Some(project), actor, "Withdrew an invitation").await?;
    tx.commit().await?;
    Ok(())
}

/// Consumes the invitation `code` names, counting a wrong code against the lockout.
async fn take_invite(
    state: &AppState,
    db: &mut SqliteConnection,
    code: &str,
) -> Result<InviteGrant, ApiError> {
    if let Some(grant) =
        credentials::take_invite(db, &secret::digest(code.trim()), now_ms()).await?
    {
        return Ok(grant);
    }
    state.lockout.fail(INVITE_LOCKOUT_KEY).await;
    Err(ApiError::Forbidden)
}

/// Creates an account with an invitation code and signs it in.
pub async fn register(state: &AppState, form: Register) -> Result<SessionToken, ApiError> {
    state.lockout.check(INVITE_LOCKOUT_KEY).await?;
    let handle = normal_handle(&form.handle);
    if !valid_handle(&handle) || !PASSWORD_LEN.contains(&form.password.len()) {
        return Err(account_rules());
    }
    let password = state.passwords.hash(form.password).await?;
    let mut tx = state.db.begin().await?;
    let grant = take_invite(state, &mut tx, &form.code).await?;
    if !accounts::create(&mut *tx, &handle, &password, now_ms()).await? {
        // Rolling back keeps the invitation usable with another username.
        return Err(ApiError::Conflict("this username is taken"));
    }
    let role = grant
        .role
        .parse::<Role>()
        .map_err(|_| ApiError::invalid("unknown role"))?;
    projects::put_member(&mut *tx, &grant.project_id, &handle, role).await?;
    audit::record(
        &mut *tx,
        Some(&grant.project_id),
        &handle,
        &format!("Joined as {}", grant.role),
    )
    .await?;
    tx.commit().await?;
    issue_session(state, &handle).await
}

/// A signed-in person joins the inviting project. An existing membership keeps the
/// higher of the two roles.
pub async fn redeem(
    state: &AppState,
    principal: &Principal,
    code: &str,
) -> Result<Project, ApiError> {
    let handle = principal.session_person()?;
    state.lockout.check(INVITE_LOCKOUT_KEY).await?;
    let mut tx = state.db.begin().await?;
    let grant = take_invite(state, &mut tx, code).await?;
    let offered = grant
        .role
        .parse::<Role>()
        .map_err(|_| ApiError::invalid("unknown role"))?;
    let kept = projects::role(&mut *tx, &grant.project_id, Some(handle))
        .await?
        .as_deref()
        .and_then(|text| text.parse::<Role>().ok())
        .map_or(offered, |held| held.max(offered));
    projects::put_member(&mut *tx, &grant.project_id, handle, kept).await?;
    audit::record(
        &mut *tx,
        Some(&grant.project_id),
        handle,
        &format!("Joined as {}", kept.as_str()),
    )
    .await?;
    tx.commit().await?;
    crate::services::projects::get(state, principal, &grant.project_id).await
}
