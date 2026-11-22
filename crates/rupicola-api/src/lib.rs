//! Contract between the Rupicola backend and its clients (web panel, CLI, desktop GUI).
//!
//! Every route the backend serves takes and returns types from this crate, so a
//! client and the server cannot drift apart.

use serde::{Deserialize, Serialize};

/// Path prefix of every versioned API route.
pub const PREFIX: &str = "/v1";

/// Response of `GET /healthz`.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct Health {
    pub status: HealthStatus,
    /// Backend package version.
    pub version: String,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum HealthStatus {
    Ok,
}

/// Body of every non-2xx API response.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct ErrorBody {
    /// Stable, machine-readable error code, such as `not-found`.
    pub code: String,
    /// Human-readable explanation.
    pub message: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn health_uses_a_stable_wire_format() {
        let health = Health {
            status: HealthStatus::Ok,
            version: "0.1.1".into(),
        };
        let json = serde_json::to_string(&health).unwrap();
        assert_eq!(json, r#"{"status":"ok","version":"0.1.1"}"#);
        assert_eq!(serde_json::from_str::<Health>(&json).unwrap(), health);
    }
}
