use crate::{Diagnostic, files, read};
use serde::Serialize;
use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
};
use toml::{Table, Value};

#[derive(Debug, Clone, Serialize)]
pub struct Dependency {
    pub alias: String,
    pub package: String,
    pub requirement: Option<String>,
    pub source: Option<String>,
    pub optional: bool,
    pub target: Option<String>,
    pub framework: Option<String>,
}
#[derive(Debug, Clone, Serialize)]
pub struct Package {
    pub manifest: PathBuf,
    pub name: String,
    pub frameworks: BTreeSet<String>,
    pub dependencies: Vec<Dependency>,
}
#[derive(Debug, Serialize)]
pub struct Inventory {
    pub schema_version: u8,
    pub packages: Vec<Package>,
    pub diagnostics: Vec<Diagnostic>,
    pub configuration: &'static str,
}

#[must_use]
pub fn framework(package: &str) -> Option<&'static str> {
    match package {
        "cosmic" | "libcosmic" => Some("libcosmic"),
        "gpui" | "gpui_platform" => Some("gpui"),
        "iced" => Some("iced"),
        "topcoat" => Some("topcoat"),
        "leptos" => Some("leptos"),
        "yew" => Some("yew"),
        "tauri" => Some("tauri"),
        "dioxus" | "dioxus-web" | "dioxus-desktop" => Some("dioxus"),
        "egui" | "eframe" => Some("egui"),
        "slint" => Some("slint"),
        "gtk4" => Some("gtk4"),
        "libadwaita" => Some("libadwaita"),
        "relm4" => Some("relm4"),
        _ => None,
    }
}

fn parse(path: &Path) -> Result<Value, String> {
    toml::from_str(&read(path)?).map_err(|e| format!("{}: {e}", path.display()))
}

fn workspace_dependencies(path: &Path, doc: &Value) -> Result<Value, String> {
    if let Some(ws) = doc.get("workspace") {
        return Ok(ws
            .get("dependencies")
            .cloned()
            .unwrap_or(Value::Table(Table::default())));
    }
    // An explicit workspace path wins over ancestor discovery.
    if let Some(relative) = doc
        .get("package")
        .and_then(|p| p.get("workspace"))
        .and_then(Value::as_str)
    {
        let manifest = path.parent().unwrap().join(relative).join("Cargo.toml");
        return Ok(parse(&manifest)?
            .get("workspace")
            .and_then(|w| w.get("dependencies"))
            .cloned()
            .unwrap_or(Value::Table(Table::default())));
    }
    for dir in path.parent().unwrap().ancestors().skip(1) {
        let manifest = dir.join("Cargo.toml");
        if manifest.is_file() {
            let parent = parse(&manifest)?;
            if let Some(ws) = parent.get("workspace") {
                return Ok(ws
                    .get("dependencies")
                    .cloned()
                    .unwrap_or(Value::Table(Table::default())));
            }
        }
    }
    Ok(Value::Table(Table::default()))
}

/// # Errors
///
/// Returns an error if the Cargo manifest cannot be read or its TOML or dependency declarations are invalid.
pub fn package(path: &Path) -> Result<Option<Package>, String> {
    let doc = parse(path)?;
    let Some(name) = doc
        .get("package")
        .and_then(|p| p.get("name"))
        .and_then(Value::as_str)
    else {
        return Ok(None);
    };
    let workspace = workspace_dependencies(path, &doc)?;
    let mut dependencies = Vec::new();
    let mut add = |table: &Value, target: Option<String>| -> Result<(), String> {
        let Some(table) = table.as_table() else {
            return Err("dependency section must be a table".into());
        };
        for (alias, spec) in table {
            let inherited = spec.get("workspace").and_then(Value::as_bool) == Some(true);
            let resolved = if inherited {
                workspace
                    .get(alias)
                    .ok_or_else(|| format!("unresolved workspace dependency {alias}"))?
            } else {
                spec
            };
            if !resolved.is_str() && !resolved.is_table() {
                return Err(format!("invalid dependency {alias}"));
            }
            let package = resolved
                .get("package")
                .and_then(Value::as_str)
                .unwrap_or(alias)
                .to_owned();
            let requirement = resolved
                .as_str()
                .or_else(|| resolved.get("version").and_then(Value::as_str))
                .map(str::to_owned);
            let source = ["git", "path", "registry"].iter().find_map(|key| {
                resolved
                    .get(key)
                    .and_then(Value::as_str)
                    .map(|v| format!("{key}:{v}"))
            });
            dependencies.push(Dependency {
                alias: alias.clone(),
                framework: framework(&package).map(str::to_owned),
                package,
                requirement,
                source,
                optional: spec
                    .get("optional")
                    .and_then(Value::as_bool)
                    .or_else(|| resolved.get("optional").and_then(Value::as_bool))
                    .unwrap_or(false),
                target: target.clone(),
            });
        }
        Ok(())
    };
    if let Some(table) = doc.get("dependencies") {
        add(table, None)?;
    }
    if let Some(targets) = doc.get("target").and_then(Value::as_table) {
        for (target, sections) in targets {
            if let Some(table) = sections.get("dependencies") {
                add(table, Some(target.clone()))?;
            }
        }
    }
    let frameworks = dependencies
        .iter()
        .filter_map(|d| d.framework.clone())
        .collect();
    Ok(Some(Package {
        manifest: path.to_owned(),
        name: name.to_owned(),
        frameworks,
        dependencies,
    }))
}

/// # Errors
///
/// Returns an error if the search root cannot be resolved or traversed.
pub fn discover(root: &Path) -> Result<Inventory, String> {
    let root = root.canonicalize().map_err(|e| e.to_string())?;
    let mut manifests: BTreeSet<PathBuf> = files(&root, false)?
        .into_iter()
        .filter(|p| p.file_name().is_some_and(|n| n == "Cargo.toml"))
        .collect();
    // A source-file/subdirectory request also identifies its enclosing package.
    let start = if root.is_file() {
        root.parent().unwrap_or(&root)
    } else {
        &root
    };
    for dir in start.ancestors() {
        let manifest = dir.join("Cargo.toml");
        if manifest.is_file() {
            manifests.insert(manifest);
            break;
        }
    }
    let mut out = Inventory {
        schema_version: 1,
        packages: Vec::new(),
        diagnostics: Vec::new(),
        configuration: "Declared dependencies only; optional features and target conditions are not evaluated. Versions are manifest requirements, not resolved versions. No build or network access is performed.",
    };
    for manifest in manifests {
        match package(&manifest) {
            Ok(Some(package)) => out.packages.push(package),
            Ok(None) => {}
            Err(message) => out.diagnostics.push(Diagnostic {
                path: manifest,
                message,
            }),
        }
    }
    Ok(out)
}
