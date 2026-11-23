//! Validation of a design document: token completeness and WCAG contrast checks.
//! Browser rendering and screenshot capture live in `rupicola-protocol`.

use super::document::{contrast, fingerprint, valid};
use rupicola_api::{Document, Outcome};
use serde::{Deserialize, Serialize};

/// One screenshot, as base64 PNG (stored in historical reports).
#[derive(Clone, Deserialize, Serialize)]
pub struct Shot {
    pub name: String,
    pub png: String,
}

/// What a validation found.
#[derive(Clone, Deserialize, Serialize)]
pub struct Report {
    /// Identifies the document and checks the report covers.
    #[serde(default)]
    pub fingerprint: String,
    pub baseline: Option<String>,
    pub outcome: Outcome,
    pub browser: Option<String>,
    pub checks: Vec<String>,
    pub shots: Vec<Shot>,
    pub error: Option<String>,
}

impl Report {
    pub fn error(outcome: Outcome, error: impl Into<String>) -> Self {
        Self {
            fingerprint: String::new(),
            baseline: None,
            outcome,
            browser: None,
            checks: Vec::new(),
            shots: Vec::new(),
            error: Some(error.into()),
        }
    }

    pub fn passed(&self) -> bool {
        self.outcome == Outcome::Passed
    }
}

/// Validates `doc`: its tokens and WCAG contrast pairs.
pub fn validate(doc: &Document, _baseline: Option<&str>) -> Report {
    let mut result = if !valid(doc) {
        Report::error(
            Outcome::Failed,
            "The design document is incomplete or contains invalid tokens.",
        )
    } else if let Err(error) = contrast(doc) {
        Report::error(Outcome::Failed, error)
    } else {
        Report {
            fingerprint: String::new(),
            baseline: None,
            outcome: Outcome::Passed,
            browser: None,
            checks: vec![
                "Tokens are valid".to_owned(),
                "WCAG AA contrast pairs pass".to_owned(),
            ],
            shots: Vec::new(),
            error: None,
        }
    };
    result.fingerprint = fingerprint(doc);
    result
}
