//! Signing in and out, the first administrator, and resolving a bearer token into
//! the [`Principal`] behind it.

use crate::{
    domain::{
        ApiError, Principal,
        account::{PASSWORD_LEN, SESSION_AGE_MS, account_rules, normal_handle, valid_handle},
        secret::{self, now_ms},
    },
    repositories::{accounts, tokens},
    state::AppState,
};
use rupicola_api::{Credentials, PasswordChange, SessionToken, Setup, TokenKind};
use subtle::ConstantTimeEq;

/// Lockout key shared by setup attempts, which have no account yet.
const SETUP_LOCKOUT_KEY: &str = "__setup__";

/// The owner of the live token whose secret is `secret`, and a note that it was used.
pub async fn principal(state: &AppState, secret: &str) -> Result<Principal, ApiError> {
    if !secret::well_formed(secret) {
        return Err(ApiError::Unauthorized);
    }
    let digest = secret::digest(secret);
    let bearer = tokens::bearer(&state.db, &digest, now_ms())
        .await?
        .ok_or(ApiError::Unauthorized)?;
    let kind = match bearer.kind.as_str() {
        "session" => TokenKind::Session,
        "machine" => TokenKind::Machine,
        "device" => TokenKind::Device,
        _ => return Err(ApiError::Unauthorized),
    };
    tokens::touch(&state.db, &digest, now_ms()).await?;
    Ok(Principal {
        kind,
        handle: bearer.handle,
        admin: bearer.admin,
        project: bearer.project_id,
        digest,
    })
}

pub async fn sign_in(state: &AppState, form: Credentials) -> Result<SessionToken, ApiError> {
    let handle = normal_handle(&form.handle);
    state.lockout.check(&handle).await?;
    let stored = if valid_handle(&handle) && form.password.len() <= *PASSWORD_LEN.end() {
        accounts::password_hash(&state.db, &handle).await?
    } else {
        None
    };
    if !state.passwords.verify(stored, form.password).await? {
        state.lockout.fail(&handle).await;
        return Err(ApiError::Unauthorized);
    }
    state.lockout.clear(&handle).await;
    issue_session(state, &handle).await
}

/// A new session for `handle`; expired tokens are swept on the way.
pub async fn issue_session(state: &AppState, handle: &str) -> Result<SessionToken, ApiError> {
    let token = secret::token();
    let created_at = now_ms();
    let expires_at = created_at + SESSION_AGE_MS;
    tokens::purge_expired(&state.db, created_at).await?;
    tokens::insert_session(
        &state.db,
        &secret::digest(&token),
        handle,
        created_at,
        expires_at,
    )
    .await?;
    Ok(SessionToken { token, expires_at })
}

pub async fn sign_out(state: &AppState, principal: &Principal) -> Result<(), ApiError> {
    if principal.kind != TokenKind::Session {
        return Err(ApiError::Forbidden);
    }
    tokens::delete(&state.db, &principal.digest).await?;
    Ok(())
}

/// Changes the signed-in person's password and ends their other sessions. Machine
/// tokens and the current session stay valid.
pub async fn change_password(
    state: &AppState,
    principal: &Principal,
    form: PasswordChange,
) -> Result<(), ApiError> {
    let handle = principal.signed_in()?;
    state.lockout.check(handle).await?;
    if !PASSWORD_LEN.contains(&form.new.len()) {
        return Err(account_rules());
    }
    let stored = accounts::password_hash(&state.db, handle).await?;
    if !state.passwords.verify(stored, form.current).await? {
        state.lockout.fail(handle).await;
        return Err(ApiError::invalid("the current password is incorrect"));
    }
    state.lockout.clear(handle).await;
    let password = state.passwords.hash(form.new).await?;
    let mut tx = state.db.begin().await?;
    accounts::set_password(&mut *tx, handle, &password).await?;
    tokens::delete_other_sessions(&mut *tx, handle, &principal.digest).await?;
    tx.commit().await?;
    tracing::info!(handle, "changed a password");
    Ok(())
}

/// Ends every other session of the signed-in person.
pub async fn sign_out_others(state: &AppState, principal: &Principal) -> Result<(), ApiError> {
    let handle = principal.signed_in()?;
    tokens::delete_other_sessions(&state.db, handle, &principal.digest).await?;
    Ok(())
}

/// Whether the first administrator still has to be created.
pub async fn setup_required(state: &AppState) -> Result<bool, ApiError> {
    Ok(!accounts::admin_exists(&state.db).await?)
}

/// Creates the first administrator and signs them in. Refused once one exists, and
/// without the setup code unless the server only listens on loopback.
pub async fn setup(state: &AppState, form: Setup) -> Result<SessionToken, ApiError> {
    let handle = normal_handle(&form.handle);
    state.lockout.check(SETUP_LOCKOUT_KEY).await?;
    let code_ok = match (&state.setup_code, form.code.as_deref()) {
        (Some(expected), Some(code)) => bool::from(
            expected
                .as_bytes()
                .ct_eq(secret::digest(code.trim()).as_bytes()),
        ),
        (Some(_), None) => false,
        (None, _) => state.open_setup,
    };
    if !code_ok {
        state.lockout.fail(SETUP_LOCKOUT_KEY).await;
        return Err(ApiError::Forbidden);
    }
    if !valid_handle(&handle) || !PASSWORD_LEN.contains(&form.password.len()) {
        return Err(account_rules());
    }
    let password = state.passwords.hash(form.password).await?;
    if !accounts::create_first_admin(&state.db, &handle, &password, now_ms()).await? {
        return Err(ApiError::Conflict("an administrator already exists"));
    }
    tracing::info!(handle, "created the first administrator account");
    issue_session(state, &handle).await
}
