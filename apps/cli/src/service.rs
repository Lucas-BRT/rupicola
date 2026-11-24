use anyhow::{Context, Result, anyhow, bail};
use serde::{Deserialize, Serialize};
use std::{
    env, fs,
    io::Write,
    path::{Path, PathBuf},
    process::Command,
};

const LINK: &str = ".rupicola/project.toml";
const MAX_IDENTIFIER_LEN: usize = 64;
/// Default seconds a preview command is given to start and pass its readiness check.
pub const DEFAULT_PREVIEW_TIMEOUT_SECS: u64 = 180;

const fn default_preview_timeout_secs() -> u64 {
    DEFAULT_PREVIEW_TIMEOUT_SECS
}

/// How the runner builds and waits for a preview of a repository tree.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PreviewConfig {
    pub command: String,
    pub ready: String,
    #[serde(default = "default_preview_timeout_secs")]
    pub timeout_secs: u64,
}

/// Repository configuration in `.rupicola/project.toml`.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectConfig {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preview: Option<PreviewConfig>,
}

impl ProjectConfig {
    /// Parses `.rupicola/project.toml` text.
    ///
    /// # Errors
    ///
    /// Fails when the text is invalid TOML or contains unknown fields.
    pub fn from_toml(text: &str) -> Result<Self, toml::de::Error> {
        toml::from_str(text)
    }
}

fn valid_identifier(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_IDENTIFIER_LEN
        && value.as_bytes()[0].is_ascii_alphanumeric()
        && value
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_')
}

fn directory(variable: &str, fallback: &str) -> Result<PathBuf> {
    if let Some(path) = env::var_os(variable)
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
    {
        return Ok(path);
    }
    env::home_dir()
        .map(|p| p.join(fallback))
        .ok_or_else(|| anyhow!("Cannot resolve {variable} or the home directory"))
}

pub fn data() -> Result<PathBuf> {
    Ok(directory("XDG_DATA_HOME", ".local/share")?.join("rupicola"))
}

fn git_boundary(root: &Path) -> bool {
    let marker = root.join(".git");
    marker.is_file() || marker.join("HEAD").is_file()
}

/// Reads `.rupicola/project.toml` walking up from the current directory to the Git boundary.
///
/// # Errors
///
/// Fails when the file exists but cannot be read or parsed.
pub fn project_config() -> Result<Option<(PathBuf, ProjectConfig)>> {
    let cwd = env::current_dir()?;
    for root in cwd.ancestors() {
        let path = root.join(LINK);
        match fs::read_to_string(&path) {
            Ok(text) => {
                let config = ProjectConfig::from_toml(&text)
                    .with_context(|| format!("{}", path.display()))?;
                return Ok(Some((root.to_path_buf(), config)));
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error).with_context(|| format!("{}", path.display())),
        }
        if git_boundary(root) {
            break;
        }
    }
    Ok(None)
}

/// Returns the root directory of the current Git checkout.
///
/// # Errors
///
/// Fails when not inside a Git checkout.
pub fn repo_root() -> Result<PathBuf> {
    let output = Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .output()
        .context("Locate Git repository root")?;
    if !output.status.success() {
        bail!("Run inside a Git checkout");
    }
    let root = String::from_utf8(output.stdout)?.trim().to_owned();
    Ok(PathBuf::from(root))
}

pub fn link(project: String) -> Result<()> {
    if !valid_identifier(&project) {
        bail!("Use a project ID containing only letters, digits, hyphens and underscores");
    }
    let cwd = env::current_dir()?;
    let root = cwd.ancestors().find(|p| git_boundary(p)).unwrap_or(&cwd);
    let path = root.join(LINK);
    let parent = path.parent().context("link path has no parent directory")?;
    fs::create_dir_all(parent)?;
    let contents = toml::to_string(&ProjectConfig {
        project: Some(project),
        ..ProjectConfig::default()
    })?;
    match fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
    {
        Ok(mut file) => file.write_all(contents.as_bytes())?,
        Err(error)
            if error.kind() == std::io::ErrorKind::AlreadyExists
                && fs::read_to_string(&path).is_ok_and(|s| s == contents) => {}
        Err(error) => return Err(error).with_context(|| format!("{}", path.display())),
    }
    println!("Linked {}", path.display());
    Ok(())
}

/// A project as the CLI names it: a stable key.
pub struct ProjectName {
    pub id: String,
}

pub fn project() -> Result<Option<ProjectName>> {
    if let Some((_, config)) = project_config()?
        && let Some(id) = config.project
    {
        return Ok(Some(ProjectName { id }));
    }
    let output = Command::new("git")
        .args([
            "rev-parse",
            "--path-format=absolute",
            "--git-common-dir",
            "--show-toplevel",
        ])
        .output()
        .context("Detect Git project")?;
    if !output.status.success() {
        return Ok(None);
    }
    let paths = String::from_utf8(output.stdout)?;
    let [git_dir, _] = paths.lines().collect::<Vec<_>>()[..] else {
        bail!("Git did not return the repository directories");
    };
    let common = fs::canonicalize(git_dir)?;
    // FNV-1a keeps this local key stable across builds and Git worktrees.
    let hash = common
        .as_os_str()
        .as_encoded_bytes()
        .iter()
        .fold(0xcbf2_9ce4_8422_2325_u64, |hash, byte| {
            (hash ^ u64::from(*byte)).wrapping_mul(0x0100_0000_01b3)
        });
    Ok(Some(ProjectName {
        id: format!("git-{hash:016x}"),
    }))
}

pub fn detect_project() -> Result<ProjectName> {
    project()?.ok_or_else(|| {
        anyhow!("Run inside a Git checkout, pass --project, or set a project with rupicola link")
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn project_config_parses_minimal_link_and_full_preview() {
        let minimal = ProjectConfig::from_toml(r#"project = "demo""#).unwrap();
        assert_eq!(minimal.project.as_deref(), Some("demo"));
        assert_eq!(minimal.base, None);
        assert_eq!(minimal.preview, None);

        let full = ProjectConfig::from_toml(
            r#"
            project = "rupicola-site"
            base = "develop"
            [preview]
            command = "cargo run -q -p rupicola-site -- --port {port}"
            ready = "http://127.0.0.1:{port}/healthz"
            "#,
        )
        .unwrap();
        let preview = full.preview.unwrap();
        assert_eq!(preview.timeout_secs, DEFAULT_PREVIEW_TIMEOUT_SECS);
        assert_eq!(full.base.as_deref(), Some("develop"));
    }

    #[test]
    fn project_config_rejects_unknown_fields() {
        assert!(ProjectConfig::from_toml(r#"unknown = "value""#).is_err());
    }
}
