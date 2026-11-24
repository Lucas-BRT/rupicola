//! Design systems: versioned documents of tokens and guidance that a project reviews, adopts
//! and shows as examples.

use serde::{Deserialize, Serialize};

text_enum! {
    /// The typeface family headings use.
    #[derive(Default)]
    HeadingStyle {
        #[default]
        Editorial => "editorial",
        Product => "product",
    }
}

impl HeadingStyle {
    /// The name shown to people.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Editorial => "Editorial serif",
            Self::Product => "Product sans",
        }
    }

    /// The CSS font stack.
    #[must_use]
    pub const fn font(self) -> &'static str {
        match self {
            Self::Editorial => "Georgia,serif",
            Self::Product => "system-ui,sans-serif",
        }
    }
}

text_enum! {
    /// Where a version stands in review.
    SystemPhase {
        Draft => "draft",
        Review => "review",
        Approved => "approved",
        Changes => "changes",
    }
}

impl SystemPhase {
    /// The name shown to people.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Draft => "Draft",
            Self::Review => "In review",
            Self::Approved => "Approved",
            Self::Changes => "Changes requested",
        }
    }
}

text_enum! {
    /// How a validation ended.
    Outcome {
        Passed => "passed",
        Failed => "failed",
        Blocked => "blocked",
    }
}

/// Guidance for the components the examples show.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(default)]
pub struct Guidance {
    pub button: String,
    pub form: String,
    pub table: String,
    pub comparison: String,
}

impl Default for Guidance {
    fn default() -> Self {
        Self {
            button: "Name the action. Keep one primary action per form. Show keyboard focus and disable repeat submission.".into(),
            form: "Label each field. Require a decision, and show the result beside the action.".into(),
            table: "Expose row and column headers. Keep horizontal scroll inside the table.".into(),
            comparison: "Use matching views for before and after. Offer side by side, overlay and clips. Restore focus when media returns.".into(),
        }
    }
}

/// The content of one version: identity, tokens, example texts and guidance.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(default)]
pub struct Document {
    pub name: String,
    pub brand: String,
    pub landing_title: String,
    pub queue_title: String,
    pub queue_item: String,
    pub settings_title: String,
    pub direction: String,
    pub usage: String,
    pub screen_title: String,
    pub scenario: String,
    pub before_label: String,
    pub after_label: String,
    /// Colors are `#rrggbb`.
    pub canvas: String,
    pub surface: String,
    pub ink: String,
    pub accent: String,
    pub action_ink: String,
    pub success: String,
    pub error: String,
    pub spacing: u8,
    pub radius: u8,
    pub heading: HeadingStyle,
    pub guidance: Guidance,
}

impl Default for Document {
    fn default() -> Self {
        Self {
            name: "Evidence review".into(),
            brand: "Rupicola".into(),
            landing_title: "Interfaces under observation.".into(),
            queue_title: "Review captures".into(),
            queue_item: "Compact notification settings".into(),
            settings_title: "Workspace settings".into(),
            direction: "Warm paper, dark plumage and crest orange. Keep the evidence in focus.".into(),
            usage: "Use orange for the primary action. Keep labels short and use words alongside status colors.".into(),
            screen_title: "Compare evidence".into(),
            scenario: "Settings · Compact window · Example data".into(),
            before_label: "Before".into(),
            after_label: "After".into(),
            canvas: "#f7f1e7".into(),
            surface: "#fffaf2".into(),
            ink: "#231d19".into(),
            accent: "#ff681e".into(),
            action_ink: "#231d19".into(),
            success: "#40543c".into(),
            error: "#9c3617".into(),
            spacing: 16,
            radius: 6,
            heading: HeadingStyle::default(),
            guidance: Guidance::default(),
        }
    }
}

/// A comment or decision on a version.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct SystemNote {
    pub actor: String,
    /// Unix milliseconds.
    pub time: i64,
    pub target: String,
    pub action: String,
    pub text: String,
}

/// What a validation found. The screenshots are read one by one, by name.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct SystemReport {
    pub outcome: Outcome,
    /// The browser that ran the checks.
    pub browser: Option<String>,
    pub checks: Vec<String>,
    /// Names of the captures that came with the report.
    pub shots: Vec<String>,
    pub error: Option<String>,
    /// The adopted version the examples were compared with, if any.
    pub baseline: Option<String>,
}

/// One version of a project's design system.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct SystemVersion {
    pub id: String,
    pub parent: Option<String>,
    pub author: String,
    pub phase: SystemPhase,
    /// Changes with every write; a write names the one it read.
    pub generation: i64,
    pub document: Document,
    pub notes: Vec<SystemNote>,
    pub report: Option<SystemReport>,
    /// Whether the validation covers this document and the adopted version, so the version
    /// may be sent for review.
    pub ready: bool,
}

/// One row of the version list.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct SystemItem {
    pub id: String,
    pub name: String,
    pub phase: SystemPhase,
}

/// `GET /v1/projects/{id}/design-systems`: the versions, newest first, and which one is adopted.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct SystemList {
    pub items: Vec<SystemItem>,
    pub active: Option<String>,
}

/// `PUT /v1/projects/{id}/design-systems/{version}/document`
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct DocumentEdit {
    pub generation: i64,
    pub document: Document,
}

text_enum! {
    /// What can be done to a version.
    SystemAction {
        Validate => "validate",
        Submit => "submit",
        Comment => "comment",
        Approve => "approve",
        Changes => "changes",
        Adopt => "adopt",
        Variation => "variation",
    }
}

/// `POST /v1/projects/{id}/design-systems/{version}/actions`
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct SystemActionForm {
    pub action: SystemAction,
    pub generation: i64,
    /// Part of the document a comment is about.
    #[serde(default)]
    pub target: String,
    #[serde(default)]
    pub note: String,
}
