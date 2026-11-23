//! The browser checks of a design document: the examples are opened in Chromium at several
//! widths and states, and what was seen comes back as a report with screenshots.

use super::document::{VALIDATOR, contrast, fingerprint, render_screen, valid};
use rupicola_api::{Document, Outcome};
use serde::{Deserialize, Serialize};
use std::{
    io::Write,
    process::{Command, Stdio},
};

/// Checks that pass when the examples are compared with an adopted version, and when not.
const CHECKS_WITH_BASELINE: usize = 18;
const CHECKS_WITHOUT_BASELINE: usize = 16;
/// Screenshots that come with those checks.
const SHOTS_WITH_BASELINE: usize = 24;
const SHOTS_WITHOUT_BASELINE: usize = 12;
/// Largest report the validator may print, in bytes.
const MAX_REPORT_BYTES: usize = 18 * 1024 * 1024;

/// Variables the validator may read; it gets nothing else.
const ENVIRONMENT: [&str; 6] = [
    "PATH",
    "HOME",
    "TMPDIR",
    "PLAYWRIGHT_BROWSERS_PATH",
    "RUPICOLA_PLAYWRIGHT_MODULE",
    "RUPICOLA_CHROMIUM",
];

/// One screenshot, as base64 PNG.
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

/// Validates `doc`: its tokens and contrast first, then the examples in a browser, against the
/// `baseline` screen when a system is adopted.
pub fn validate(doc: &Document, baseline: Option<&str>) -> Report {
    let mut result = if !valid(doc) {
        Report::error(
            Outcome::Failed,
            "The design document is incomplete or contains invalid tokens.",
        )
    } else if let Err(error) = contrast(doc) {
        Report::error(Outcome::Failed, error)
    } else {
        run_browser(&render_screen(doc), baseline)
            .unwrap_or_else(|error| Report::error(Outcome::Blocked, error.to_string()))
    };
    result.fingerprint = fingerprint(doc);
    result
}

fn run_browser(html: &str, baseline: Option<&str>) -> anyhow::Result<Report> {
    let input = serde_json::json!({ "html": html, "baseline": baseline }).to_string();
    let mut command = Command::new("node");
    command.env_clear();
    for key in ENVIRONMENT {
        if let Some(value) = std::env::var_os(key) {
            command.env(key, value);
        }
    }
    let mut child = command
        .args(["-e", VALIDATOR])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|_| anyhow::anyhow!("Node.js is unavailable on the server host."))?;
    let sent = child
        .stdin
        .take()
        .ok_or_else(|| anyhow::anyhow!("Cannot open validator input."))?
        .write_all(input.as_bytes());
    let output = child
        .wait_with_output()
        .map_err(|_| anyhow::anyhow!("Browser validator did not finish."))?;
    sent.map_err(|_| anyhow::anyhow!("Cannot send the preview to the validator."))?;
    if !output.status.success() || output.stdout.len() > MAX_REPORT_BYTES {
        anyhow::bail!("Browser validator timed out or returned an invalid report.");
    }
    let report: Report = serde_json::from_slice(&output.stdout)
        .map_err(|_| anyhow::anyhow!("Invalid browser validation report."))?;
    let (checks, shots) = if baseline.is_some() {
        (CHECKS_WITH_BASELINE, SHOTS_WITH_BASELINE)
    } else {
        (CHECKS_WITHOUT_BASELINE, SHOTS_WITHOUT_BASELINE)
    };
    if report.passed()
        && (report.checks.len() != checks
            || report.shots.len() != shots
            || report.browser.is_none())
    {
        anyhow::bail!("Browser validator omitted mandatory evidence.");
    }
    Ok(report)
}
