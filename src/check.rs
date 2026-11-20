use crate::{Diagnostic, FRAMEWORKS, TOPICS, files, read};
use pulldown_cmark::{Event, Parser, Tag};
use serde::Serialize;
use std::path::Path;

#[derive(Debug, Serialize)]
pub struct Check {
    pub schema_version: u8,
    pub markdown_files: usize,
    pub eval_tasks: usize,
    pub diagnostics: Vec<Diagnostic>,
}
/// # Errors
///
/// Returns an error if the skill directory cannot be resolved or traversed, or a required file cannot be read.
pub fn check(root: &Path) -> Result<Check, String> {
    let root = root.canonicalize().map_err(|e| e.to_string())?;
    if !root.is_dir() {
        return Err("check expects the skill directory".into());
    }
    let mut result = Check {
        schema_version: 1,
        markdown_files: 0,
        eval_tasks: 0,
        diagnostics: Vec::new(),
    };
    let mut error = |path, message| result.diagnostics.push(Diagnostic { path, message });
    let skill_path = root.join("SKILL.md");
    let skill = read(&skill_path)?;
    let normalized_skill = skill.replace("\r\n", "\n");
    let frontmatter = normalized_skill
        .strip_prefix("---\n")
        .and_then(|s| s.split_once("\n---"));
    if let Some((metadata, _)) = frontmatter {
        if !metadata.lines().any(|s| s.trim() == "name: rupicola") {
            error(skill_path.clone(), "skill name must be rupicola".into());
        }
        if !metadata
            .lines()
            .any(|s| s.starts_with("description:") && !s.trim_end().ends_with(':'))
        {
            error(skill_path.clone(), "description is required".into());
        }
        if let Some(description) = metadata
            .lines()
            .find_map(|s| s.strip_prefix("description:"))
        {
            let description = description.trim();
            if description.chars().count() > 1024 {
                error(
                    skill_path.clone(),
                    "description exceeds 1024 characters".into(),
                );
            }
            if description.contains(['<', '>']) {
                error(
                    skill_path.clone(),
                    "description must not contain XML or HTML tags".into(),
                );
            }
            if !description.contains("Use ") || !description.contains("Not for") {
                error(
                    skill_path.clone(),
                    "description must say when to use the skill and when not to".into(),
                );
            }
        }
    } else {
        error(skill_path.clone(), "missing YAML frontmatter".into());
    }
    let required = FRAMEWORKS
        .iter()
        .map(|id| format!("reference/frameworks/{id}.md"))
        .chain(TOPICS.iter().map(|id| format!("reference/ux/{id}.md")))
        .chain(
            [
                "plan",
                "build",
                "review",
                "improve",
                "scan",
                "context",
                "follow-up",
            ]
            .map(|id| format!("reference/workflows/{id}.md")),
        )
        .chain(["linux", "windows", "macos"].map(|id| format!("reference/platforms/{id}.md")))
        .chain(["web", "desktop", "webview"].map(|id| format!("reference/environments/{id}.md")))
        .chain([
            "reference/rust/integration.md".to_owned(),
            "reference/architecture.md".to_owned(),
            "reference/reports.md".to_owned(),
        ]);
    for relative in required {
        if !root.join(&relative).is_file() {
            error(root.join(&relative), "required module is missing".into());
        }
        if !Parser::new(&skill).any(|event| matches!(event, Event::Start(Tag::Link { dest_url, .. }) if dest_url.as_ref() == relative)) {
            error(skill_path.clone(), format!("missing direct module link: {relative}"));
        }
    }
    for path in files(&root, true)? {
        let extension = path.extension().and_then(|s| s.to_str()).unwrap_or("");
        if matches!(extension, "js" | "mjs" | "cjs" | "ts" | "sh" | "py")
            && ![
                "src/browser-capture.cjs",
                "src/cosmic-capture.sh",
                "src/browser-probe.cjs",
                "panel/src/panel.js",
                "panel/dev.sh",
                "site/src/site.js",
                "site/src/install.sh",
            ]
            .iter()
            .any(|allowed| path == root.join(allowed))
        {
            error(
                path.clone(),
                "non-Rust executable source is not part of this project".into(),
            );
        }
        if extension != "md" {
            continue;
        }
        result.markdown_files += 1;
        let text = match read(&path) {
            Ok(text) => text,
            Err(message) => {
                error(path, message);
                continue;
            }
        };
        for event in Parser::new(&text) {
            if let Event::Start(Tag::Link { dest_url, .. } | Tag::Image { dest_url, .. }) = event {
                let target = dest_url.as_ref();
                if target.starts_with('#')
                    || target.starts_with("//")
                    || target.split('/').next().is_some_and(|s| s.contains(':'))
                {
                    continue;
                }
                let target = target.split('#').next().unwrap_or("");
                let target = target.split('?').next().unwrap_or("");
                if target.is_empty() {
                    continue;
                }
                let Some(parent) = path.parent() else {
                    error(path.clone(), "document has no parent directory".into());
                    continue;
                };
                let resolved = parent.join(target);
                match resolved.canonicalize() {
                    Ok(p) if p.starts_with(&root) => {}
                    Ok(_) => error(
                        path.clone(),
                        format!("local link escapes the skill directory: {target}"),
                    ),
                    Err(_) => error(path.clone(), format!("missing local link: {target}")),
                }
            }
        }
    }
    let sources_path = root.join("reference/sources.toml");
    match read(&sources_path)
        .and_then(|s| toml::from_str::<toml::Value>(&s).map_err(|e| e.to_string()))
    {
        Ok(sources) => {
            let entries = sources.get("framework").and_then(toml::Value::as_array);
            for framework in FRAMEWORKS {
                let matches = entries
                    .into_iter()
                    .flatten()
                    .filter(|entry| {
                        entry.get("id").and_then(toml::Value::as_str) == Some(framework)
                            && entry
                                .get("url")
                                .and_then(toml::Value::as_str)
                                .is_some_and(|url| url.starts_with("https://"))
                    })
                    .count();
                if matches != 1 {
                    error(
                        sources_path.clone(),
                        format!("expected one source record for {framework}"),
                    );
                }
            }
        }
        Err(message) => error(sources_path, message),
    }
    let mut eval_diagnostics = Vec::new();
    result.eval_tasks = crate::evals::validate(&root, &mut eval_diagnostics);
    result.diagnostics.extend(eval_diagnostics);
    Ok(result)
}
