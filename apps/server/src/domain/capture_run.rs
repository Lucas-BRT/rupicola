//! Shared capture step validation and default viewport constants.

use crate::domain::ApiError;
use rupicola_api::{CaptureStep, plan};

pub const DEFAULT_WIDTH: u32 = 1440;
pub const DEFAULT_HEIGHT: u32 = 900;

/// Checks a scenario's steps: bounded, with distinct checkpoint and clip ids and at least one of them.
///
/// # Errors
///
/// [`ApiError::Invalid`] when the steps would not make a valid run.
pub fn check_steps(steps: &[CaptureStep]) -> Result<(), ApiError> {
    plan::build_steps(steps)
        .map(|_| ())
        .map_err(|err| ApiError::invalid(err.to_string()))
}

/// Checks the steps that put a page in a theme: actions only, since a theme's setup captures
/// nothing.
///
/// # Errors
///
/// [`ApiError::Invalid`] for a checkpoint or clip, or an action that would not run.
pub fn check_setup_steps(steps: &[CaptureStep]) -> Result<(), ApiError> {
    plan::build_setup_steps(steps)
        .map(|_| ())
        .map_err(|err| ApiError::invalid(err.to_string()))
}
