//! `/me`, `/sessions` and `/setup`.

use crate::{
    domain::{ApiError, Principal},
    services::auth,
    state::Shared,
};
use axum::{Json, extract::State, http::StatusCode};
use rupicola_api::{Credentials, Me, PasswordChange, SessionToken, Setup, SetupStatus};

/// `POST /v1/sessions`
pub async fn sign_in(
    State(state): State<Shared>,
    Json(form): Json<Credentials>,
) -> Result<(StatusCode, Json<SessionToken>), ApiError> {
    Ok((
        StatusCode::CREATED,
        Json(auth::sign_in(&state, form).await?),
    ))
}

/// `DELETE /v1/sessions/current`
pub async fn sign_out(
    State(state): State<Shared>,
    principal: Principal,
) -> Result<StatusCode, ApiError> {
    auth::sign_out(&state, &principal).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// `GET /v1/me`
pub async fn me(principal: Principal) -> Json<Me> {
    Json(Me {
        kind: principal.kind,
        handle: principal.handle,
        admin: principal.admin,
        project: principal.project,
    })
}

/// `PUT /v1/me/password`
pub async fn change_password(
    State(state): State<Shared>,
    principal: Principal,
    Json(form): Json<PasswordChange>,
) -> Result<StatusCode, ApiError> {
    auth::change_password(&state, &principal, form).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// `DELETE /v1/sessions/others`
pub async fn sign_out_others(
    State(state): State<Shared>,
    principal: Principal,
) -> Result<StatusCode, ApiError> {
    auth::sign_out_others(&state, &principal).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// `GET /v1/setup`
pub async fn setup_status(State(state): State<Shared>) -> Result<Json<SetupStatus>, ApiError> {
    Ok(Json(SetupStatus {
        required: auth::setup_required(&state).await?,
    }))
}

/// `POST /v1/setup`
pub async fn setup(
    State(state): State<Shared>,
    Json(form): Json<Setup>,
) -> Result<(StatusCode, Json<SessionToken>), ApiError> {
    Ok((StatusCode::CREATED, Json(auth::setup(&state, form).await?)))
}
