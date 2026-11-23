//! The rules of a project's topic tree: names, paths, and the refusals a change can meet.

use super::ApiError;
use rupicola_api::{TOPIC_SEPARATOR, Topic, valid_topic_name};
use std::collections::HashMap;

pub const NAME_TAKEN_MESSAGE: &str = "another topic there already has that name";
pub const STALE_MESSAGE: &str = "the topic changed since it was read; read it again";

/// The names of a path, once each one can name a topic.
pub fn names(path: &str) -> Result<Vec<&str>, ApiError> {
    let names: Vec<&str> = path.split(TOPIC_SEPARATOR).collect();
    if let Some(bad) = names.iter().find(|name| !valid_topic_name(name)) {
        return Err(ApiError::invalid(format!(
            "`{bad}` cannot name a topic: use 1 to 64 characters, no `/`, no spaces at the ends"
        )));
    }
    Ok(names)
}

pub fn check_name(name: &str) -> Result<(), ApiError> {
    if valid_topic_name(name) {
        Ok(())
    } else {
        Err(ApiError::invalid(
            "a topic name has 1 to 64 characters, no `/` and no spaces at the ends",
        ))
    }
}

/// Names joined into a path.
pub fn join(names: &[String]) -> String {
    names.join(&TOPIC_SEPARATOR.to_string())
}

/// Every topic with its path filled in from the others, ordered by path. `topics` come with
/// an empty path.
pub fn tree(mut topics: Vec<Topic>) -> Vec<Topic> {
    let parents: HashMap<String, (Option<String>, String)> = topics
        .iter()
        .map(|topic| {
            (
                topic.id.clone(),
                (topic.parent_id.clone(), topic.name.clone()),
            )
        })
        .collect();
    let path_of = |start: &str| {
        let mut names = Vec::new();
        let mut id = start.to_owned();
        while let Some((parent, name)) = parents.get(&id) {
            names.push(name.clone());
            match parent {
                Some(above) => id.clone_from(above),
                None => break,
            }
        }
        names.reverse();
        join(&names)
    };
    for topic in &mut topics {
        topic.path = path_of(&topic.id);
    }
    topics.sort_by_cached_key(|topic| topic.path.to_lowercase());
    topics
}

/// How a change to a topic ended.
pub enum Change {
    Done,
    /// The topic is no longer the generation the change read.
    Stale,
    /// A sibling already has the name.
    NameTaken,
    /// The topic would go under itself.
    Cycle,
}

impl Change {
    /// Fails for anything but [`Change::Done`].
    pub fn done(self) -> Result<(), ApiError> {
        match self {
            Self::Done => Ok(()),
            Self::Stale => Err(ApiError::Conflict(STALE_MESSAGE)),
            Self::NameTaken => Err(ApiError::Conflict(NAME_TAKEN_MESSAGE)),
            Self::Cycle => Err(ApiError::invalid(
                "a topic cannot go under itself or one of its subtopics",
            )),
        }
    }
}
