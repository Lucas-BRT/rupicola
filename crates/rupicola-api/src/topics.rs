//! A project's topics: a tree of subjects that captures are filed under. A capture filed under
//! a topic belongs to every topic above it.

use serde::{Deserialize, Serialize};

/// The character that separates the names of a topic's path.
pub const TOPIC_SEPARATOR: char = '/';
/// Longest topic name, in characters.
pub const MAX_TOPIC_NAME_CHARS: usize = 64;

/// One topic of the tree.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct Topic {
    pub id: String,
    pub parent_id: Option<String>,
    pub name: String,
    /// The names from the root down to this topic, joined by `/`.
    pub path: String,
    pub position: i64,
    /// Changes with every rename or move; a change names the generation it read.
    pub generation: i64,
    /// Captures filed directly under this topic, not counting its subtopics.
    pub captures: u64,
}

/// `GET /v1/projects/{id}/topics`: every topic, ordered by path.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct TopicTree {
    pub topics: Vec<Topic>,
}

/// `POST /v1/projects/{id}/topics`: makes every topic of `path` that is missing.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct NewTopic {
    pub path: String,
}

/// `PUT /v1/topics/{id}/name`.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct TopicRename {
    pub name: String,
    pub generation: i64,
}

/// `PUT /v1/topics/{id}/parent`: no parent puts the topic at the root.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct TopicMove {
    pub parent_id: Option<String>,
    pub generation: i64,
}

/// Whether `name` may name a topic: a short, trimmed name with no `/`.
#[must_use]
pub fn valid_topic_name(name: &str) -> bool {
    name == name.trim()
        && (1..=MAX_TOPIC_NAME_CHARS).contains(&name.chars().count())
        && !name.contains(TOPIC_SEPARATOR)
        && name != "."
        && name != ".."
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_topic_name_is_short_trimmed_and_has_no_separator() {
        for good in ["fix", "Onboarding v2", "123", "é"] {
            assert!(valid_topic_name(good), "{good}");
        }
        for bad in ["", " fix", "fix ", "a/b", ".", "..", &"x".repeat(65)] {
            assert!(!valid_topic_name(bad), "{bad:?}");
        }
    }
}
