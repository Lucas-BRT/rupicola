//! Scenarios: the declarative capture definitions a repository owns.
//!
//! A scenario is a TOML file in the repository. The CLI parses it, the server keeps the parsed
//! definition under the hash of its canonical JSON, and a runner expands it into captures.

use crate::CaptureStep;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fmt::{self, Write as _},
    str::FromStr,
};

/// Viewport used when a scenario's matrix lists none.
pub const DEFAULT_VIEWPORT: Viewport = Viewport {
    width: 1440,
    height: 900,
    scale: 1,
};

/// A viewport the matrix captures at, written `WIDTHxHEIGHT` or `WIDTHxHEIGHT@SCALE`.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
#[serde(try_from = "String", into = "String")]
pub struct Viewport {
    pub width: u32,
    pub height: u32,
    /// Device pixel ratio, a whole number of at least 1.
    pub scale: u32,
}

/// The text is not a viewport.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
#[error("`{0}` is not a viewport; write WIDTHxHEIGHT or WIDTHxHEIGHT@SCALE, such as 390x844@2")]
pub struct InvalidViewport(pub String);

impl FromStr for Viewport {
    type Err = InvalidViewport;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        let invalid = || InvalidViewport(text.to_owned());
        let (size, scale) = match text.split_once('@') {
            Some((size, scale)) => (size, scale.parse().map_err(|_| invalid())?),
            None => (text, 1),
        };
        let (width, height) = size.split_once('x').ok_or_else(invalid)?;
        let viewport = Self {
            width: width.parse().map_err(|_| invalid())?,
            height: height.parse().map_err(|_| invalid())?,
            scale,
        };
        if viewport.width == 0 || viewport.height == 0 || viewport.scale == 0 {
            return Err(invalid());
        }
        Ok(viewport)
    }
}

impl fmt::Display for Viewport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}x{}", self.width, self.height)?;
        if self.scale != 1 {
            write!(f, "@{}", self.scale)?;
        }
        Ok(())
    }
}

impl TryFrom<String> for Viewport {
    type Error = InvalidViewport;

    fn try_from(text: String) -> Result<Self, Self::Error> {
        text.parse()
    }
}

impl From<Viewport> for String {
    fn from(viewport: Viewport) -> Self {
        viewport.to_string()
    }
}

/// How the browser is set up for every capture of a scenario.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ScenarioEnvironment {
    #[serde(default)]
    pub reduced_motion: bool,
    #[serde(default)]
    pub locale: Option<String>,
}

/// The viewports and themes a scenario is captured at. One capture per pair; no viewports
/// means [`DEFAULT_VIEWPORT`] and no themes means the page as it loads.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Matrix {
    #[serde(default)]
    pub viewports: Vec<Viewport>,
    #[serde(default)]
    pub themes: Vec<String>,
}

impl Matrix {
    /// Captures the matrix makes: every viewport with every theme.
    #[must_use]
    pub fn cells(&self) -> usize {
        self.viewports.len().max(1) * self.themes.len().max(1)
    }
}

/// What it takes to put the page in a theme the browser's color scheme cannot reach, for sites
/// that pick the theme themselves. A theme with no setup sets the color scheme instead.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ThemeSetup {
    #[serde(default)]
    pub steps: Vec<CaptureStep>,
}

text_enum! {
    /// Contrast level a check holds text to.
    ContrastLevel {
        /// WCAG AA: 4.5:1 for normal text, 3:1 for large text.
        Aa => "aa",
        /// WCAG AAA: 7:1 for normal text, 4.5:1 for large text.
        Aaa => "aaa",
    }
}

/// Checks a runner makes on each captured page and reports as findings.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Checks {
    /// Nothing sticks out past the viewport's width.
    #[serde(default)]
    pub no_horizontal_overflow: bool,
    /// Every pointer target is at least this many CSS pixels on each side.
    #[serde(default)]
    pub min_target_size: Option<u32>,
    /// No two elements matching these selectors overlap.
    #[serde(default)]
    pub no_overlap: Vec<String>,
    /// Every control has an accessible name.
    #[serde(default)]
    pub accessible_names: bool,
    /// Text meets the contrast level. Needs computed style from the capture backend.
    #[serde(default)]
    pub contrast: Option<ContrastLevel>,
}

/// A capture definition: the page, how to show it, the matrix, the steps and the checks.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Scenario {
    /// Identifies the scenario in its project.
    pub name: String,
    /// Page to open, relative to the base URL of the tree being captured.
    pub url: String,
    #[serde(default)]
    pub environment: ScenarioEnvironment,
    #[serde(default)]
    pub matrix: Matrix,
    /// Setup of the themes the matrix names, by theme name.
    #[serde(default)]
    pub themes: BTreeMap<String, ThemeSetup>,
    /// Steps run once per capture, after the theme is applied.
    #[serde(default)]
    pub steps: Vec<CaptureStep>,
    #[serde(default)]
    pub checks: Checks,
}

impl Scenario {
    /// Hash of the definition's canonical JSON, so reformatting the file does not make a new
    /// version. Field order is the declaration order and maps are sorted.
    #[must_use]
    pub fn hash(&self) -> String {
        let canonical = serde_json::to_vec(self).unwrap_or_default();
        Sha256::digest(canonical)
            .iter()
            .fold(String::with_capacity(64), |mut hex, byte| {
                let _ = write!(hex, "{byte:02x}");
                hex
            })
    }

    /// Parses a definition from the TOML of its file.
    ///
    /// # Errors
    ///
    /// When the text is not TOML, or has a field or value the definition does not allow.
    pub fn from_toml(text: &str) -> Result<Self, toml::de::Error> {
        toml::from_str(text)
    }
}

/// One stored version of a scenario.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct ScenarioVersion {
    pub hash: String,
    pub scenario: Scenario,
    pub author: Option<String>,
    pub created_at: i64,
}

/// A scenario in a project's list: its latest version.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct ScenarioSummary {
    pub name: String,
    pub hash: String,
    pub url: String,
    /// Captures a run of the latest version makes.
    pub cells: usize,
    pub created_at: i64,
}

#[cfg(test)]
mod tests {
    use super::*;

    const EXAMPLE: &str = r#"
        name = "home"
        url = "/"

        [environment]
        reduced_motion = true

        [matrix]
        viewports = ["390x844@2", "1280x800"]
        themes = ["dark", "light"]

        [themes.light]
        steps = [{ kind = "click", selector = "[data-theme-toggle]" }]

        [[steps]]
        kind = "ready"
        selector = "h1"

        [[steps]]
        kind = "checkpoint"
        id = "full"
        full_page = true

        [checks]
        no_horizontal_overflow = true
        min_target_size = 24
        contrast = "aa"
    "#;

    #[test]
    fn a_viewport_reads_and_writes_the_same_text() {
        assert_eq!(
            "390x844@2".parse(),
            Ok(Viewport {
                width: 390,
                height: 844,
                scale: 2
            })
        );
        assert_eq!("1280x800".parse::<Viewport>().unwrap().scale, 1);
        for text in ["390x844@2", "1280x800"] {
            assert_eq!(text.parse::<Viewport>().unwrap().to_string(), text);
        }
        for bad in ["", "390", "0x800", "390x844@0", "axb", "390x844@x"] {
            assert!(bad.parse::<Viewport>().is_err(), "{bad}");
        }
    }

    #[test]
    fn the_example_file_parses_with_defaults_for_what_it_omits() {
        let scenario = Scenario::from_toml(EXAMPLE).unwrap();
        assert_eq!(scenario.name, "home");
        assert_eq!(scenario.matrix.cells(), 4);
        assert_eq!(scenario.steps.len(), 2);
        assert_eq!(scenario.themes["light"].steps.len(), 1);
        assert_eq!(scenario.checks.contrast, Some(ContrastLevel::Aa));
        assert_eq!(scenario.checks.no_overlap.len(), 0);
    }

    #[test]
    fn an_empty_matrix_is_one_capture() {
        let scenario = Scenario::from_toml("name = \"a\"\nurl = \"/\"").unwrap();
        assert_eq!(scenario.matrix.cells(), 1);
    }

    #[test]
    fn a_misspelled_field_is_an_error_not_a_silent_default() {
        let text = "name = \"a\"\nurl = \"/\"\n[checks]\nno_horizontal_overflo = true";
        assert!(Scenario::from_toml(text).is_err());
    }

    #[test]
    fn the_hash_ignores_formatting_and_follows_content() {
        let reordered = "url = \"/\"\nname = \"a\"";
        let spaced = "name   =   \"a\"\n\n\nurl = \"/\"\n";
        let first = Scenario::from_toml(reordered).unwrap().hash();
        assert_eq!(first, Scenario::from_toml(spaced).unwrap().hash());
        assert_eq!(first.len(), 64);
        let changed = Scenario::from_toml("name = \"a\"\nurl = \"/docs\"").unwrap();
        assert_ne!(first, changed.hash());
    }
}
