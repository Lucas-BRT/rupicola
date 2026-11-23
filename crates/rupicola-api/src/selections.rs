//! Selections: frozen lists of anchors into captures that problems and fixes cite as proof,
//! without copying anything. Changing the proof means making a new selection.

use serde::{Deserialize, Serialize};

/// Most anchors one selection may hold.
pub const MAX_SELECTION_ITEMS: usize = 64;
/// Longest selection label, in characters.
pub const MAX_SELECTION_LABEL_CHARS: usize = 200;

text_enum! {
    /// What an anchor is in a selection.
    SelectionMark {
        /// It shows the problem, or that the fix works.
        Proof => "proof",
        /// It is there for orientation only.
        Context => "context",
    }
}

/// The part of a file an anchor points at: a span of a video, a region of an image.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Fragment {
    /// Start and end, in milliseconds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub time_ms: Option<[u64; 2]>,
    /// Left, top, width and height, in the image's logical units.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rect: Option<[i64; 4]>,
}

/// One anchor: a whole capture, or one of its files named by its id or by its path in the
/// capture.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct SelectionItem {
    pub capture: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub artifact: Option<String>,
    /// The file's path in the capture, for a client that knows the record but not the ids.
    /// The server answers with the artifact id.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    pub mark: SelectionMark,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fragment: Option<Fragment>,
}

/// `POST /v1/projects/{id}/selections`, and the selections a new problem or fix brings.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct NewSelection {
    pub label: String,
    pub items: Vec<SelectionItem>,
}

/// A selection as kept.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct Selection {
    pub id: String,
    pub label: String,
    pub author: Option<String>,
    pub created_at: i64,
    pub items: Vec<SelectionItem>,
}

impl Selection {
    /// The files this selection holds up as proof.
    pub fn proof(&self) -> impl Iterator<Item = &str> {
        self.items
            .iter()
            .filter(|item| item.mark == SelectionMark::Proof)
            .filter_map(|item| item.artifact.as_deref())
    }
}
