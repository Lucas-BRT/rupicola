//! Design system domain rules: token/contrast validation, fingerprinting and version lifecycle.

use crate::domain::{ApiError, secret::now_ms};
use rupicola_api::{
    Document, Outcome, SystemAction, SystemActionForm, SystemNote, SystemPhase, SystemReport,
    SystemVersion,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// WCAG 2.2 AA contrast for normal text.
const MIN_TEXT_CONTRAST: f64 = 4.5;
/// Design tokens store colors as `#rrggbb`: a hash and six hex digits.
const TOKEN_COLOR_LEN: usize = 7;
/// Longest of the short texts, in bytes.
const SHORT_TEXT: usize = 120;
/// Longest example scenario, in bytes.
const SCENARIO_TEXT: usize = 200;
/// Longest label of a before or after capture, in bytes.
const LABEL_TEXT: usize = 48;
/// Longest direction or usage text, in bytes.
const LONG_TEXT: usize = 2500;
/// Longest guidance text, in bytes.
const GUIDANCE_TEXT: usize = 1400;
/// Range of the base spacing, in pixels.
const SPACING: std::ops::RangeInclusive<u8> = 8..=32;
/// Largest corner radius, in pixels.
const MAX_RADIUS: u8 = 16;

const CHANNEL_MAX_F: f64 = 255.0;
const SRGB_LINEAR_THRESHOLD: f64 = 0.04045;
const SRGB_LINEAR_SLOPE: f64 = 12.92;
const SRGB_OFFSET: f64 = 0.055;
const SRGB_GAMMA: f64 = 2.4;
const LUMA_RED: f64 = 0.2126;
const LUMA_GREEN: f64 = 0.7152;
const LUMA_BLUE: f64 = 0.0722;
const CONTRAST_FLARE: f64 = 0.05;
const HEX_RADIX: u32 = 16;

/// Notes one version may collect.
pub const MAX_NOTES: usize = 1000;
/// Longest note, in bytes.
const MAX_NOTE_BYTES: usize = 4000;
/// Parts of the document a comment may be about; any other target means the whole document.
const TARGETS: [&str; 6] = [
    "identity",
    "tokens",
    "components",
    "comparison",
    "media-unavailable",
    "form",
];

#[derive(Clone, Copy)]
struct Rgb {
    r: u8,
    g: u8,
    b: u8,
}

impl Rgb {
    fn luminance(self) -> f64 {
        LUMA_RED * linear(self.r) + LUMA_GREEN * linear(self.g) + LUMA_BLUE * linear(self.b)
    }

    fn contrast(self, other: Self) -> f64 {
        let (a, b) = (self.luminance(), other.luminance());
        let (hi, lo) = if a > b { (a, b) } else { (b, a) };
        (hi + CONTRAST_FLARE) / (lo + CONTRAST_FLARE)
    }
}

fn linear(channel: u8) -> f64 {
    let c = f64::from(channel) / CHANNEL_MAX_F;
    if c <= SRGB_LINEAR_THRESHOLD {
        c / SRGB_LINEAR_SLOPE
    } else {
        ((c + SRGB_OFFSET) / (1.0 + SRGB_OFFSET)).powf(SRGB_GAMMA)
    }
}

fn color(text: &str) -> Option<Rgb> {
    let hex = text
        .strip_prefix('#')
        .filter(|_| text.len() == TOKEN_COLOR_LEN)?;
    let byte = |range: std::ops::Range<usize>| u8::from_str_radix(&hex[range], HEX_RADIX).ok();
    Some(Rgb {
        r: byte(0..2)?,
        g: byte(2..4)?,
        b: byte(4..6)?,
    })
}

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
    #[must_use]
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

    #[must_use]
    pub fn passed(&self) -> bool {
        self.outcome == Outcome::Passed
    }
}

/// A design system version as held in memory and persisted.
pub struct Stored {
    pub id: String,
    pub generation: i64,
    pub parent: Option<String>,
    pub author: String,
    pub phase: SystemPhase,
    pub document: Document,
    pub notes: Vec<SystemNote>,
    pub report: Option<Report>,
}

impl Stored {
    #[must_use]
    pub fn draft(parent: Option<String>, author: String, document: Document) -> Self {
        Self {
            id: uuid::Uuid::now_v7().to_string(),
            generation: 0,
            parent,
            author,
            phase: SystemPhase::Draft,
            document,
            notes: Vec::new(),
            report: None,
        }
    }

    /// Whether the validation covers this document and the adopted version.
    #[must_use]
    pub fn ready(&self, adopted: Option<&str>) -> bool {
        self.report.as_ref().is_some_and(|report| {
            report.passed()
                && report.baseline.as_deref() == adopted
                && report.fingerprint == fingerprint(&self.document)
        })
    }

    pub fn submit(&mut self, adopted: Option<&str>) -> Result<(), ApiError> {
        if self.phase != SystemPhase::Draft || !self.ready(adopted) {
            return Err(ApiError::Conflict(
                "validate this version again before sending it for review",
            ));
        }
        self.phase = SystemPhase::Review;
        Ok(())
    }

    pub fn note(&mut self, actor: &str, action: &str, target: &str, text: String) {
        self.notes.push(SystemNote {
            actor: actor.to_owned(),
            time: now_ms() / 1000,
            action: action.to_owned(),
            target: target.to_owned(),
            text,
        });
    }

    pub fn review(&mut self, actor: &str, form: &SystemActionForm) -> Result<(), ApiError> {
        if form.note.len() > MAX_NOTE_BYTES || self.notes.len() >= MAX_NOTES {
            return Err(ApiError::invalid(
                "the note is too long or the version has too many",
            ));
        }
        let text = form.note.trim();
        let target = if TARGETS.contains(&form.target.as_str()) {
            form.target.as_str()
        } else {
            "document"
        };
        match form.action {
            SystemAction::Comment if self.phase != SystemPhase::Draft && !text.is_empty() => (),
            SystemAction::Approve if self.phase == SystemPhase::Review => {
                self.phase = SystemPhase::Approved;
            }
            SystemAction::Changes if self.phase == SystemPhase::Review && !text.is_empty() => {
                self.phase = SystemPhase::Changes;
            }
            _ => return Err(ApiError::Conflict("that cannot be done in this phase")),
        }
        self.note(actor, form.action.as_str(), target, text.to_owned());
        Ok(())
    }

    #[must_use]
    pub fn dto(self, adopted: Option<&str>) -> SystemVersion {
        let ready = self.ready(adopted);
        SystemVersion {
            id: self.id,
            parent: self.parent,
            author: self.author,
            phase: self.phase,
            generation: self.generation,
            document: self.document,
            notes: self.notes,
            report: self.report.map(|report| SystemReport {
                outcome: report.outcome,
                browser: report.browser,
                checks: report.checks,
                shots: report.shots.into_iter().map(|shot| shot.name).collect(),
                error: report.error,
                baseline: report.baseline,
            }),
            ready,
        }
    }
}

/// Whether every text is present and bounded and every token is a plain color or size.
#[must_use]
pub fn valid(doc: &Document) -> bool {
    [
        (&doc.name, SHORT_TEXT),
        (&doc.brand, SHORT_TEXT),
        (&doc.landing_title, SHORT_TEXT),
        (&doc.queue_title, SHORT_TEXT),
        (&doc.queue_item, SHORT_TEXT),
        (&doc.settings_title, SHORT_TEXT),
        (&doc.direction, LONG_TEXT),
        (&doc.usage, LONG_TEXT),
        (&doc.screen_title, SHORT_TEXT),
        (&doc.scenario, SCENARIO_TEXT),
        (&doc.before_label, LABEL_TEXT),
        (&doc.after_label, LABEL_TEXT),
        (&doc.guidance.button, GUIDANCE_TEXT),
        (&doc.guidance.form, GUIDANCE_TEXT),
        (&doc.guidance.table, GUIDANCE_TEXT),
        (&doc.guidance.comparison, GUIDANCE_TEXT),
    ]
    .into_iter()
    .all(|(value, limit)| !value.trim().is_empty() && value.len() <= limit)
        && SPACING.contains(&doc.spacing)
        && doc.radius <= MAX_RADIUS
        && [
            &doc.canvas,
            &doc.surface,
            &doc.ink,
            &doc.accent,
            &doc.action_ink,
            &doc.success,
            &doc.error,
        ]
        .into_iter()
        .all(|value| color(value).is_some())
}

/// The first text pair whose contrast is below WCAG AA.
pub fn contrast(doc: &Document) -> Result<(), &'static str> {
    for (front, back, error) in [
        (
            &doc.ink,
            &doc.canvas,
            "Text on canvas needs contrast of at least 4.5:1.",
        ),
        (
            &doc.ink,
            &doc.surface,
            "Text on surfaces needs contrast of at least 4.5:1.",
        ),
        (
            &doc.action_ink,
            &doc.accent,
            "Primary action text needs contrast of at least 4.5:1.",
        ),
        (
            &doc.success,
            &doc.surface,
            "Success messages need contrast of at least 4.5:1.",
        ),
        (
            &doc.error,
            &doc.surface,
            "Error messages need contrast of at least 4.5:1.",
        ),
    ] {
        let a = color(front).ok_or("Invalid foreground color.")?;
        let b = color(back).ok_or("Invalid background color.")?;
        if a.contrast(b) < MIN_TEXT_CONTRAST {
            return Err(error);
        }
    }
    Ok(())
}

/// Identifies what a validation covered: the document itself.
#[must_use]
pub fn fingerprint(doc: &Document) -> String {
    let mut hash = Sha256::new();
    hash.update(serde_json::to_vec(doc).expect("document contains only serializable fields"));
    format!("{:x}", hash.finalize())
}

/// Validates `doc`: its tokens and WCAG contrast pairs.
#[must_use]
pub fn validate(doc: &Document) -> Report {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invalid_tokens_are_rejected() {
        let mut document = Document::default();
        assert!(valid(&document));
        document.accent = "#f0f; };".into();
        assert!(!valid(&document));
    }

    #[test]
    fn a_low_contrast_pair_is_named() {
        let document = Document {
            action_ink: "#ff681e".into(),
            ..Document::default()
        };
        assert!(valid(&document));
        assert_eq!(
            contrast(&document),
            Err("Primary action text needs contrast of at least 4.5:1.")
        );
        assert_eq!(contrast(&Document::default()), Ok(()));
    }
}
