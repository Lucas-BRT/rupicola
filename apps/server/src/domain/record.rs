//! What a published capture record must satisfy beyond its own checks: the address it is
//! sent to, the project's limits, the files the project holds and the parents it has.

use super::ApiError;
use rupicola_api::record::{CaptureRecord, MemberRole};
use std::collections::HashMap;

/// Fails unless the record is the capture `id` of `project`.
pub fn check_address(record: &CaptureRecord, project: &str, id: &str) -> Result<(), ApiError> {
    if record.project == project && record.id == id {
        Ok(())
    } else {
        Err(ApiError::invalid(format!(
            "the record is capture {} of project {}, not the one the address names",
            record.id, record.project
        )))
    }
}

/// Fails for what this server cannot keep yet, or more files than the project allows.
pub fn check_members(record: &CaptureRecord, most: usize) -> Result<(), ApiError> {
    if record
        .members
        .iter()
        .any(|member| member.role == MemberRole::UiSnapshot)
    {
        return Err(ApiError::invalid(
            "this server does not keep UI snapshots yet; publish the capture without them",
        ));
    }
    if record.members.len() > most {
        return Err(ApiError::invalid(format!(
            "the capture has {} files; the project allows {most}",
            record.members.len()
        )));
    }
    Ok(())
}

/// Fails unless every file is one the project holds (`held`, by digest, with its size), at
/// the size the record says.
pub fn check_files(record: &CaptureRecord, held: &HashMap<String, i64>) -> Result<(), ApiError> {
    let missing: Vec<&str> = record
        .members
        .iter()
        .filter(|member| !held.contains_key(&member.sha256))
        .map(|member| member.sha256.as_str())
        .collect();
    if !missing.is_empty() {
        return Err(ApiError::invalid(format!(
            "upload these files to the project first: {}",
            missing.join(", ")
        )));
    }
    if let Some(member) = record
        .members
        .iter()
        .find(|member| i64::try_from(member.bytes).ok() != held.get(&member.sha256).copied())
    {
        return Err(ApiError::invalid(format!(
            "{} says {} bytes, but its file has another size",
            member.path, member.bytes
        )));
    }
    Ok(())
}

/// Fails unless every parent is among the project's live captures `found`.
pub fn check_parents(parents: &[&str], found: &[String]) -> Result<(), ApiError> {
    let absent: Vec<&str> = parents
        .iter()
        .copied()
        .filter(|parent| !found.iter().any(|id| id == parent))
        .collect();
    if absent.is_empty() {
        Ok(())
    } else {
        Err(ApiError::invalid(format!(
            "publish these parent captures first: {}",
            absent.join(", ")
        )))
    }
}
