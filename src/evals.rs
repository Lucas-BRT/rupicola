//! Offline validation of evaluation task definitions (`evals/tasks/<id>/task.toml`).
//! Nothing here runs a model or executes a verifier command.
use crate::{Diagnostic, FRAMEWORKS, read};
use serde::Deserialize;
use std::collections::BTreeSet;
use std::path::Path;

const KINDS: &[&str] = &["build", "review", "improve", "plan", "trigger"];
const SPLITS: &[&str] = &["dev", "holdout"];

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Task {
    pub id: String,
    /// A framework id from `FRAMEWORKS`, or `none` for non-UI negative controls.
    pub framework: String,
    pub kind: String,
    pub split: String,
    /// Whether the skill ought to be invoked for this prompt.
    pub should_trigger: bool,
    pub prompt: String,
    #[serde(default)]
    pub constraints: Vec<String>,
    #[serde(default)]
    pub verifier: Vec<Verifier>,
    #[serde(default)]
    pub seeded_defect: Vec<Defect>,
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Verifier {
    pub name: String,
    pub command: String,
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Defect {
    pub id: String,
    pub file: String,
    pub description: String,
    /// Lowercase phrases; a defect counts as found when the final report contains any of them.
    pub keywords: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Trigger {
    pub id: String,
    pub category: String,
    pub expect: bool,
    pub split: String,
    pub prompt: String,
}
#[derive(Debug, Deserialize)]
struct Triggers {
    prompt: Vec<Trigger>,
}

/// Loads `evals/triggers.toml`: prompts whose only measured outcome is skill invocation.
pub fn load_triggers(root: &Path) -> Result<Vec<Trigger>, String> {
    let file = root.join("evals/triggers.toml");
    let text = read(&file)?;
    toml::from_str::<Triggers>(&text)
        .map(|t| t.prompt)
        .map_err(|e| format!("{}: {e}", file.display()))
}

fn validate_triggers(root: &Path, out: &mut Vec<Diagnostic>) {
    let file = root.join("evals/triggers.toml");
    if !file.is_file() {
        return;
    }
    let mut err = |message: String| {
        out.push(Diagnostic {
            path: file.clone(),
            message,
        });
    };
    let triggers = match load_triggers(root) {
        Ok(t) => t,
        Err(m) => return err(m),
    };
    let mut seen = BTreeSet::new();
    for t in &triggers {
        if !seen.insert(t.id.as_str()) {
            err(format!("duplicate trigger id: {}", t.id));
        }
        if !matches!(t.category.as_str(), "explicit" | "implicit" | "negative") {
            err(format!(
                "{}: category must be explicit, implicit or negative",
                t.id
            ));
        }
        if (t.category == "negative") == t.expect {
            err(format!(
                "{}: only negative prompts may expect no invocation",
                t.id
            ));
        }
        if !SPLITS.contains(&t.split.as_str()) {
            err(format!("{}: split must be one of {SPLITS:?}", t.id));
        }
        if t.prompt.trim().is_empty() {
            err(format!("{}: prompt is empty", t.id));
        }
    }
}

/// Loads all task definitions in deterministic order; any malformed task is an error.
pub fn load(root: &Path) -> Result<Vec<Task>, String> {
    let dir = root.join("evals/tasks");
    let mut paths: Vec<_> = std::fs::read_dir(&dir)
        .map_err(|e| format!("{}: {e}", dir.display()))?
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .collect();
    paths.sort();
    paths
        .into_iter()
        .map(|p| {
            read(&p.join("task.toml"))
                .and_then(|s| toml::from_str::<Task>(&s).map_err(|e| e.to_string()))
        })
        .collect()
}

/// Validates every task under `<root>/evals/tasks`; an absent directory is not an error.
/// Returns the number of tasks inspected.
pub fn validate(root: &Path, out: &mut Vec<Diagnostic>) -> usize {
    validate_triggers(root, out);
    let dir = root.join("evals/tasks");
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return 0;
    };
    let mut names: Vec<_> = entries.flatten().map(|e| e.path()).collect();
    names.sort();
    let mut seen = BTreeSet::new();
    let mut count = 0;
    for path in names.into_iter().filter(|p| p.is_dir()) {
        count += 1;
        let file = path.join("task.toml");
        let mut err = |message: String| {
            out.push(Diagnostic {
                path: file.clone(),
                message,
            });
        };
        let task =
            match read(&file).and_then(|s| toml::from_str::<Task>(&s).map_err(|e| e.to_string())) {
                Ok(task) => task,
                Err(message) => {
                    err(message);
                    continue;
                }
            };
        let dirname = path.file_name().and_then(|s| s.to_str()).unwrap_or("");
        if task.id != dirname {
            err(format!(
                "id `{}` must equal directory name `{dirname}`",
                task.id
            ));
        }
        if !seen.insert(task.id.clone()) {
            err(format!("duplicate task id: {}", task.id));
        }
        if task.framework != "none" && !FRAMEWORKS.contains(&task.framework.as_str()) {
            err(format!("unknown framework: {}", task.framework));
        }
        if !KINDS.contains(&task.kind.as_str()) {
            err(format!("kind must be one of {KINDS:?}"));
        }
        if !SPLITS.contains(&task.split.as_str()) {
            err(format!("split must be one of {SPLITS:?}"));
        }
        if task.prompt.trim().is_empty() {
            err("prompt is empty".into());
        }
        if task
            .verifier
            .iter()
            .any(|v| v.name.is_empty() || v.command.trim().is_empty())
        {
            err("verifier needs a name and a command".into());
        }
        match task.kind.as_str() {
            "review" => {
                if task.seeded_defect.is_empty() {
                    err("review task needs at least one seeded_defect".into());
                }
                for d in &task.seeded_defect {
                    if d.keywords.is_empty() || d.keywords.iter().any(|k| k.trim().is_empty()) {
                        err(format!("seeded_defect {} needs non-empty keywords", d.id));
                    }
                    if d.id.is_empty() || d.description.trim().is_empty() {
                        err("seeded_defect needs an id and a description".into());
                    }
                    if !path.join("fixture").join(&d.file).is_file() {
                        err(format!(
                            "seeded_defect file missing under fixture/: {}",
                            d.file
                        ));
                    }
                }
            }
            "trigger" => {
                if !task.verifier.is_empty() {
                    err("trigger tasks only measure invocation; remove verifiers".into());
                }
            }
            _ => {
                if task.verifier.is_empty() {
                    err("task needs at least one deterministic verifier".into());
                }
            }
        }
        if task.kind != "trigger" && !task.should_trigger {
            err("only trigger tasks may set should_trigger = false".into());
        }
        if task.kind == "trigger" && task.framework == "none" && task.should_trigger {
            err("a framework-less trigger task must set should_trigger = false".into());
        }
        if task.constraints.iter().any(|c| c.trim().is_empty()) {
            err("constraint is empty".into());
        }
    }
    count
}
