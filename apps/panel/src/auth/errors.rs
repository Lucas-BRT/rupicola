use crate::StatusCode;
use rupicola_client::ClientError;

pub(super) const BAD_LOGIN: &str = "The username or password is incorrect.";
pub(super) const LOGIN_LIMITED: &str =
    "Too many failed sign-in attempts. Wait 5 minutes before trying again.";
pub(super) const REGISTER_LIMITED: &str =
    "Too many registration attempts. Wait 5 minutes before trying again.";
pub(super) const BAD_SETUP_CODE: &str = "The setup or invitation code is invalid.";
pub(super) const SETUP_REFUSED: &str =
    "This setup or invitation could not be used. Check the code and username.";

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
