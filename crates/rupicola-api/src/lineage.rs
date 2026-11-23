//! The lineage of a capture as it holds now: the parents its record declared, or the ones an
//! edit set since, with the history of every edit.

use crate::record::{ParentKind, ParentRef};
use serde::{Deserialize, Serialize};

text_enum! {
    /// Where a parent came from.
    ParentOrigin {
        /// The capture's record declared it.
        Record => "record",
        /// A person set it after the capture was published.
        Edit => "edit",
    }
}

/// One parent of a capture.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct ParentEdge {
    pub id: String,
    pub kind: ParentKind,
    pub origin: ParentOrigin,
}

/// One change of a capture's parents.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct LineageEvent {
    /// The generation the change produced.
    pub generation: i64,
    pub actor: String,
    pub reason: String,
    pub before: Vec<ParentRef>,
    pub after: Vec<ParentRef>,
    pub created_at: i64,
}

/// `GET /v1/captures/{id}/parents`.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct Lineage {
    pub parents: Vec<ParentEdge>,
    /// Changes with every edit; an edit names the generation it read.
    pub generation: i64,
    /// Every edit, oldest first.
    pub history: Vec<LineageEvent>,
}

/// `PUT /v1/captures/{id}/parents`: replaces the parents of a capture.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct ParentsEdit {
    pub parents: Vec<ParentRef>,
    /// The generation the edit was made from; a newer one means someone changed it first.
    pub generation: i64,
    /// Why the lineage changes, kept in its history.
    pub reason: String,
}
