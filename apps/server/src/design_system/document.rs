//! What a design document must satisfy: token completeness and WCAG contrast checks.

use rupicola_api::Document;
use rupicola_core::visual::Rgb;
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

fn color(text: &str) -> Option<Rgb> {
    (text.len() == TOKEN_COLOR_LEN && text.starts_with('#'))
        .then(|| text.parse().ok())
        .flatten()
}

/// Whether every text is present and bounded and every token is a plain color or size.
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
pub fn fingerprint(doc: &Document) -> String {
    let mut hash = Sha256::new();
    hash.update(serde_json::to_vec(doc).expect("document contains only serializable fields"));
    format!("{:x}", hash.finalize())
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
