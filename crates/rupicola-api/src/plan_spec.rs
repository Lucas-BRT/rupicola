//! Canonical, immutable specification of how a capture was executed, used for
//! deterministic recapture and server-side before/after proof parity verification.

use crate::{CaptureStep, Scenario, Viewport, record::canonical_json};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fmt::Write as _;

/// Default device pixel ratio in thousandths (`1000` = `1.0x`).
pub const DEFAULT_SCALE_MILLI: u32 = 1000;
/// Default browser page zoom in thousandths (`1000` = `100%`).
pub const DEFAULT_ZOOM_MILLI: u32 = 1000;

const fn default_scale_milli() -> u32 {
    DEFAULT_SCALE_MILLI
}

const fn default_zoom_milli() -> u32 {
    DEFAULT_ZOOM_MILLI
}

/// Normalizes a route or full URL into a host-independent path (+ optional query/fragment)
/// starting with `/`, so local dev URLs (`http://127.0.0.1:4731/settings?a=1`) and preview
/// URLs (`https://preview.example/settings?a=1`) produce identical plan digests.
#[must_use]
pub fn normalize_url_path(raw: &str) -> String {
    let trimmed = raw.trim();
    let without_origin = if let Some((_, rest)) = trimmed.split_once("://") {
        match rest.find(['/', '?', '#']) {
            Some(idx) => &rest[idx..],
            None => "/",
        }
    } else {
        trimmed
    };
    if without_origin.is_empty() {
        return "/".to_owned();
    }
    if without_origin.starts_with('/') {
        without_origin.to_owned()
    } else {
        format!("/{without_origin}")
    }
}

/// Frozen execution parameters of a single capture, independent of host/port.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct CapturePlanSpec {
    /// Host-independent route path + optional query/fragment (e.g. `"/settings?tab=limits"`).
    pub url_path: String,
    /// Viewport dimensions `[width, height]` in logical pixels.
    pub viewport: [u32; 2],
    /// Device pixel ratio in thousandths (`2000` = `2.0x`).
    #[serde(default = "default_scale_milli")]
    pub scale_milli: u32,
    /// Page zoom level in thousandths (`1000` = `100%`, `1250` = `125%`).
    #[serde(default = "default_zoom_milli")]
    pub zoom_milli: u32,
    /// Preferred color scheme (`"light"`, `"dark"`), when set.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color_scheme: Option<String>,
    /// Whether `prefers-reduced-motion: reduce` was enabled.
    #[serde(default)]
    pub reduced_motion: bool,
    /// Browser locale (e.g. `"en-US"`), when set.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub locale: Option<String>,
    /// IANA timezone (e.g. `"UTC"`), when set.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timezone: Option<String>,
    /// Ordered capture steps executed (including any theme setup steps).
    #[serde(default)]
    pub steps: Vec<CaptureStep>,
    /// Scenario name when derived from a stored scenario (informational, excluded from digest).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scenario_name: Option<String>,
    /// Canonical scenario hash when derived from a stored scenario (informational, excluded from digest).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scenario_hash: Option<String>,
}

/// One execution parameter that differs between two [`CapturePlanSpec`]s.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct PlanMismatch {
    pub field: String,
    pub before: String,
    pub after: String,
}

#[derive(Serialize)]
struct DigestProjection<'a> {
    url_path: String,
    viewport: [u32; 2],
    scale_milli: u32,
    zoom_milli: u32,
    color_scheme: Option<String>,
    reduced_motion: bool,
    locale: Option<String>,
    timezone: Option<String>,
    steps: &'a [CaptureStep],
}

fn clean_opt(raw: Option<&str>) -> Option<String> {
    raw.map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
}

impl CapturePlanSpec {
    /// Builds the frozen execution spec for one `(viewport, theme)` cell of `scenario`.
    #[must_use]
    pub fn from_scenario_cell(
        scenario: &Scenario,
        scenario_hash: Option<&str>,
        viewport: Viewport,
        theme: Option<&str>,
    ) -> Self {
        let mut color_scheme = None;
        let mut theme_steps: Vec<CaptureStep> = Vec::new();
        if let Some(theme_name) = theme {
            let custom = scenario
                .themes
                .get(theme_name)
                .map(|setup| setup.steps.as_slice())
                .filter(|steps| !steps.is_empty());
            if let Some(steps) = custom {
                theme_steps = steps.to_vec();
            } else if theme_name.eq_ignore_ascii_case("light") {
                color_scheme = Some("light".to_owned());
            } else if theme_name.eq_ignore_ascii_case("dark") {
                color_scheme = Some("dark".to_owned());
            }
        }

        let leading_ready = scenario
            .steps
            .iter()
            .take_while(|step| matches!(step, CaptureStep::Ready { .. }))
            .count();

        let steps = if theme_steps.is_empty() {
            scenario.steps.clone()
        } else {
            let mut combined = Vec::with_capacity(scenario.steps.len() + theme_steps.len());
            combined.extend_from_slice(&scenario.steps[..leading_ready]);
            combined.extend(theme_steps);
            combined.extend_from_slice(&scenario.steps[leading_ready..]);
            combined
        };

        Self {
            url_path: normalize_url_path(&scenario.url),
            viewport: [viewport.width, viewport.height],
            scale_milli: viewport.scale.saturating_mul(DEFAULT_SCALE_MILLI),
            zoom_milli: DEFAULT_ZOOM_MILLI,
            color_scheme,
            reduced_motion: scenario.environment.reduced_motion,
            locale: clean_opt(scenario.environment.locale.as_deref()),
            timezone: None,
            steps,
            scenario_name: Some(scenario.name.clone()),
            scenario_hash: scenario_hash.map(str::to_owned),
        }
    }

    /// Returns a normalized copy with canonical `url_path` and trimmed optional strings.
    #[must_use]
    pub fn normalized(&self) -> Self {
        Self {
            url_path: normalize_url_path(&self.url_path),
            viewport: self.viewport,
            scale_milli: if self.scale_milli == 0 {
                DEFAULT_SCALE_MILLI
            } else {
                self.scale_milli
            },
            zoom_milli: if self.zoom_milli == 0 {
                DEFAULT_ZOOM_MILLI
            } else {
                self.zoom_milli
            },
            color_scheme: clean_opt(self.color_scheme.as_deref()).map(|s| s.to_ascii_lowercase()),
            reduced_motion: self.reduced_motion,
            locale: clean_opt(self.locale.as_deref()),
            timezone: clean_opt(self.timezone.as_deref()),
            steps: self.steps.clone(),
            scenario_name: self.scenario_name.clone(),
            scenario_hash: self.scenario_hash.clone(),
        }
    }

    /// Deterministic SHA-256 hex digest of the execution parameters (host-independent).
    ///
    /// # Panics
    ///
    /// Never: `CapturePlanSpec` contains only whole numbers, strings, booleans, and enums.
    #[must_use]
    pub fn digest(&self) -> String {
        let norm = self.normalized();
        let proj = DigestProjection {
            url_path: norm.url_path,
            viewport: norm.viewport,
            scale_milli: norm.scale_milli,
            zoom_milli: norm.zoom_milli,
            color_scheme: norm.color_scheme,
            reduced_motion: norm.reduced_motion,
            locale: norm.locale,
            timezone: norm.timezone,
            steps: &norm.steps,
        };
        let value = serde_json::to_value(&proj).expect("CapturePlanSpec serializes to JSON");
        let canonical = canonical_json(&value).expect("CapturePlanSpec uses whole numbers only");
        Sha256::digest(canonical.as_bytes()).iter().fold(
            String::with_capacity(64),
            |mut hex, byte| {
                let _ = write!(hex, "{byte:02x}");
                hex
            },
        )
    }

    /// Compares execution parameters against `other`, returning every mismatched field.
    #[must_use]
    pub fn diff(&self, other: &Self) -> Vec<PlanMismatch> {
        let a = self.normalized();
        let b = other.normalized();
        let mut out = Vec::new();
        if a.url_path != b.url_path {
            out.push(PlanMismatch {
                field: "url_path".to_owned(),
                before: a.url_path,
                after: b.url_path,
            });
        }
        if a.viewport != b.viewport {
            out.push(PlanMismatch {
                field: "viewport".to_owned(),
                before: format!("{}x{}", a.viewport[0], a.viewport[1]),
                after: format!("{}x{}", b.viewport[0], b.viewport[1]),
            });
        }
        if a.scale_milli != b.scale_milli {
            out.push(PlanMismatch {
                field: "scale_milli".to_owned(),
                before: a.scale_milli.to_string(),
                after: b.scale_milli.to_string(),
            });
        }
        if a.zoom_milli != b.zoom_milli {
            out.push(PlanMismatch {
                field: "zoom_milli".to_owned(),
                before: a.zoom_milli.to_string(),
                after: b.zoom_milli.to_string(),
            });
        }
        if a.color_scheme != b.color_scheme {
            out.push(PlanMismatch {
                field: "color_scheme".to_owned(),
                before: a.color_scheme.unwrap_or_else(|| "none".to_owned()),
                after: b.color_scheme.unwrap_or_else(|| "none".to_owned()),
            });
        }
        if a.reduced_motion != b.reduced_motion {
            out.push(PlanMismatch {
                field: "reduced_motion".to_owned(),
                before: a.reduced_motion.to_string(),
                after: b.reduced_motion.to_string(),
            });
        }
        if a.locale != b.locale {
            out.push(PlanMismatch {
                field: "locale".to_owned(),
                before: a.locale.unwrap_or_else(|| "none".to_owned()),
                after: b.locale.unwrap_or_else(|| "none".to_owned()),
            });
        }
        if a.timezone != b.timezone {
            out.push(PlanMismatch {
                field: "timezone".to_owned(),
                before: a.timezone.unwrap_or_else(|| "none".to_owned()),
                after: b.timezone.unwrap_or_else(|| "none".to_owned()),
            });
        }
        if a.steps != b.steps {
            out.push(PlanMismatch {
                field: "steps".to_owned(),
                before: format!("{} step(s)", a.steps.len()),
                after: format!("{} step(s)", b.steps.len()),
            });
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::DEFAULT_VIEWPORT;

    fn sample_spec() -> CapturePlanSpec {
        CapturePlanSpec {
            url_path: "/settings?tab=limits".to_owned(),
            viewport: [1440, 900],
            scale_milli: 2000,
            zoom_milli: 1000,
            color_scheme: Some("dark".to_owned()),
            reduced_motion: true,
            locale: Some("en-US".to_owned()),
            timezone: Some("UTC".to_owned()),
            steps: vec![CaptureStep::Checkpoint {
                id: "main".to_owned(),
                full_page: false,
            }],
            scenario_name: Some("settings".to_owned()),
            scenario_hash: Some("abc".to_owned()),
        }
    }

    #[test]
    fn normalize_url_path_strips_host_and_port() {
        assert_eq!(
            normalize_url_path("http://127.0.0.1:4731/projects/demo?x=1#top"),
            "/projects/demo?x=1#top"
        );
        assert_eq!(
            normalize_url_path("https://preview.rupicola.dev/projects/demo?x=1#top"),
            "/projects/demo?x=1#top"
        );
        assert_eq!(normalize_url_path("http://localhost:3000"), "/");
        assert_eq!(normalize_url_path("  settings "), "/settings");
        assert_eq!(normalize_url_path(""), "/");
    }

    #[test]
    fn digest_is_host_independent_and_ignores_informational_scenario_fields() {
        let mut a = sample_spec();
        a.url_path = "http://127.0.0.1:4731/settings?tab=limits".to_owned();
        let mut b = sample_spec();
        b.url_path = "https://staging.example.com/settings?tab=limits".to_owned();
        b.scenario_name = None;
        b.scenario_hash = None;
        assert_eq!(a.digest(), b.digest());
        assert_eq!(a.diff(&b), Vec::<PlanMismatch>::new());
    }

    #[test]
    fn digest_and_diff_detect_zoom_page_viewport_theme_and_steps() {
        let base = sample_spec();

        let mut zoomed = base.clone();
        zoomed.zoom_milli = 1250;
        assert_ne!(base.digest(), zoomed.digest());
        assert_eq!(
            base.diff(&zoomed),
            vec![PlanMismatch {
                field: "zoom_milli".to_owned(),
                before: "1000".to_owned(),
                after: "1250".to_owned(),
            }]
        );

        let mut wrong_page = base.clone();
        wrong_page.url_path = "/projects".to_owned();
        assert_ne!(base.digest(), wrong_page.digest());
        assert_eq!(base.diff(&wrong_page)[0].field, "url_path");

        let mut wrong_vp = base.clone();
        wrong_vp.viewport = [390, 844];
        assert_ne!(base.digest(), wrong_vp.digest());
        assert_eq!(base.diff(&wrong_vp)[0].field, "viewport");

        let mut wrong_theme = base.clone();
        wrong_theme.color_scheme = Some("light".to_owned());
        assert_ne!(base.digest(), wrong_theme.digest());
        assert_eq!(base.diff(&wrong_theme)[0].field, "color_scheme");

        let mut wrong_steps = base.clone();
        wrong_steps.steps.push(CaptureStep::ScrollTo {
            selector: None,
            x: 0,
            y: 400,
        });
        assert_ne!(base.digest(), wrong_steps.digest());
        assert_eq!(base.diff(&wrong_steps)[0].field, "steps");
    }

    #[test]
    fn from_scenario_cell_places_theme_steps_after_leading_ready() {
        let scenario = Scenario::from_toml(
            r#"
            name = "home"
            url = "/dashboard"

            [themes.light]
            steps = [{ kind = "click", selector = "[data-theme]" }]

            [[steps]]
            kind = "ready"
            selector = "main"

            [[steps]]
            kind = "checkpoint"
            id = "hero"
            "#,
        )
        .unwrap();
        let spec = CapturePlanSpec::from_scenario_cell(
            &scenario,
            Some("h1"),
            DEFAULT_VIEWPORT,
            Some("light"),
        );
        assert_eq!(spec.url_path, "/dashboard");
        assert_eq!(spec.viewport, [1440, 900]);
        assert_eq!(spec.steps.len(), 3);
        assert!(matches!(spec.steps[0], CaptureStep::Ready { .. }));
        assert!(matches!(spec.steps[1], CaptureStep::Click { .. }));
        assert!(matches!(spec.steps[2], CaptureStep::Checkpoint { .. }));
    }
}
