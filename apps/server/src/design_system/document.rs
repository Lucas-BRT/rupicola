//! What a design document must satisfy, and the example screen made from it.

use base64::{Engine as _, engine::general_purpose::STANDARD};
use rupicola_api::Document;
use rupicola_core::visual::Rgb;
use sha2::{Digest, Sha256};

const SCREEN: &str = include_str!("../../design/screen.html");
const EXAMPLES_HTML: &str = include_str!("../../design/examples.html");
const EXAMPLES_CSS: &str = include_str!("../../design/examples.css");
const EXAMPLES_JS: &str = include_str!("../../design/examples.js");

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

fn escaped(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

fn image(mime: &str, bytes: &[u8]) -> String {
    format!("data:{mime};base64,{}", STANDARD.encode(bytes))
}

/// The example pages for `doc`, as one self-contained HTML document.
pub fn render_screen(doc: &Document) -> String {
    [
        ("@@EXAMPLES_HTML@@", EXAMPLES_HTML.to_owned()),
        ("@@EXAMPLES_CSS@@", EXAMPLES_CSS.to_owned()),
        ("@@EXAMPLES_JS@@", EXAMPLES_JS.to_owned()),
        ("@@CANVAS@@", doc.canvas.clone()),
        ("@@SURFACE@@", doc.surface.clone()),
        ("@@INK@@", doc.ink.clone()),
        ("@@ACCENT@@", doc.accent.clone()),
        ("@@ACTION_INK@@", doc.action_ink.clone()),
        ("@@SPACE@@", doc.spacing.to_string()),
        ("@@RADIUS@@", doc.radius.to_string()),
        ("@@HEADING@@", doc.heading.font().to_owned()),
        ("@@HEADING_LABEL@@", doc.heading.label().to_owned()),
        ("@@SUCCESS@@", doc.success.clone()),
        ("@@ERROR@@", doc.error.clone()),
        (
            "@@BEFORE_PNG@@",
            image(
                "image/png",
                include_bytes!("../../design/fixtures/before.png"),
            ),
        ),
        (
            "@@AFTER_PNG@@",
            image(
                "image/png",
                include_bytes!("../../design/fixtures/after.png"),
            ),
        ),
        (
            "@@BEFORE_WEBM@@",
            image(
                "video/webm",
                include_bytes!("../../design/fixtures/before.webm"),
            ),
        ),
        (
            "@@AFTER_WEBM@@",
            image(
                "video/webm",
                include_bytes!("../../design/fixtures/after.webm"),
            ),
        ),
        ("@@SYSTEM_NAME@@", escaped(&doc.name)),
        ("@@BRAND_ATTR@@", escaped(&doc.brand)),
        ("@@BRAND@@", escaped(&doc.brand)),
        ("@@LANDING_TITLE@@", escaped(&doc.landing_title)),
        ("@@QUEUE_TITLE@@", escaped(&doc.queue_title)),
        ("@@QUEUE_ITEM@@", escaped(&doc.queue_item)),
        ("@@SETTINGS_TITLE@@", escaped(&doc.settings_title)),
        ("@@BUTTON_NOTES@@", escaped(&doc.guidance.button)),
        ("@@FORM_NOTES@@", escaped(&doc.guidance.form)),
        ("@@TABLE_NOTES@@", escaped(&doc.guidance.table)),
        ("@@COMPARISON_NOTES@@", escaped(&doc.guidance.comparison)),
        ("@@TITLE@@", escaped(&doc.screen_title)),
        ("@@SCENARIO@@", escaped(&doc.scenario)),
        ("@@BEFORE_LABEL@@", escaped(&doc.before_label)),
        ("@@AFTER_LABEL@@", escaped(&doc.after_label)),
    ]
    .into_iter()
    .fold(SCREEN.to_owned(), |html, (key, value)| {
        html.replace(key, &value)
    })
}

/// Identifies what a validation covered: the document and the screen made from it.
pub fn fingerprint(doc: &Document) -> String {
    let mut hash = Sha256::new();
    hash.update(serde_json::to_vec(doc).expect("document contains only serializable fields"));
    hash.update(render_screen(doc).as_bytes());
    format!("{:x}", hash.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn screen_escapes_user_text_and_rejects_css_in_tokens() {
        let mut document = Document {
            screen_title: "</title><script>bad()</script>".into(),
            ..Document::default()
        };
        let rendered = render_screen(&document);
        assert!(rendered.contains("&lt;/title&gt;&lt;script&gt;bad()"));
        assert!(!rendered.contains("<script>bad()"));
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
