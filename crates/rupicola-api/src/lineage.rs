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

/// One capture in a lineage graph.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct LineageNode {
    pub id: String,
    pub scenario: String,
    pub series: Option<String>,
    pub created_at: i64,
    /// Whether the capture was removed; it stays in the graph as a tombstone.
    pub deleted: bool,
}

/// One parent link in a lineage graph.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct LineageLink {
    pub child: String,
    pub parent: String,
    pub kind: ParentKind,
    pub origin: ParentOrigin,
}

/// `GET /v1/captures/{id}/lineage`: the captures a capture descends from and those that descend
/// from it, up to a depth either way.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct LineageGraph {
    pub root: String,
    pub nodes: Vec<LineageNode>,
    pub links: Vec<LineageLink>,
}

/// A file present in both captures but with other content.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct ChangedFile {
    pub path: String,
    pub before: String,
    pub after: String,
}

/// An environment facet that differs between two captures; a side without it is `None`.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct FacetChange {
    pub key: String,
    pub before: Option<String>,
    pub after: Option<String>,
}

/// `GET /v1/captures/{a}/compare/{b}`: what changed from capture `a` to capture `b`, file by
/// file (matched by path) and facet by facet.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct CaptureDiff {
    pub before: String,
    pub after: String,
    pub same: Vec<String>,
    pub changed: Vec<ChangedFile>,
    pub added: Vec<String>,
    pub removed: Vec<String>,
    pub facets: Vec<FacetChange>,
}
