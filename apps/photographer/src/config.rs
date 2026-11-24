//! Configuration resolution for `rupicola-photographer`: `.rupicola/project.toml`,
//! Git repository root / project detection, server URL, and bearer token.

use anyhow::{Context, Result, anyhow, bail};
use rupicola_api::{DEFAULT_PREVIEW_TIMEOUT_SECS, PreviewConfig, ProjectConfig};
use std::{
    env, fs,
    io::ErrorKind,
    path::{Path, PathBuf},
    process::Command,
};

const PROJECT_CONFIG_PATH: &str = ".rupicola/project.toml";
const SERVER_URL_ENV: &str = "RUPICOLA_SERVER_URL";
const RUNNER_TOKEN_ENV: &str = "RUPICOLA_RUNNER_TOKEN";
const MCP_TOKEN_ENV: &str = "RUPICOLA_MCP_TOKEN";
const TOKEN_FILE: &str = "auth-token";
const SERVER_FILE: &str = "server-url";
const MCP_SUFFIX: &str = "/mcp";
const FNV1A_OFFSET_BASIS: u64 = 0xcbf2_9ce4_8422_2325;
const FNV1A_PRIME: u64 = 0x0100_0000_01b3;

fn data_dir() -> Result<PathBuf> {
    if let Some(path) = env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
    {
        return Ok(path.join("rupicola"));
    }
    env::home_dir()
        .map(|p| p.join(".local/share/rupicola"))
        .ok_or_else(|| anyhow!("Cannot resolve XDG_DATA_HOME or the home directory"))
}

fn read_trimmed(path: &Path) -> Result<Option<String>> {
    match fs::read_to_string(path) {
        Ok(text) => Ok(Some(text.trim().to_owned()).filter(|s| !s.is_empty())),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error).with_context(|| format!("Read {}", path.display())),
    }
}

fn git_boundary(root: &Path) -> bool {
    let marker = root.join(".git");
    marker.is_file() || marker.join("HEAD").is_file()
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
    Ok(PathBuf::from(String::from_utf8(output.stdout)?.trim()))
}

/// Reads `.rupicola/project.toml` walking up from the current directory to the Git boundary.
///
/// # Errors
///
/// Fails when the file exists but cannot be read or parsed.
pub fn load_project_config() -> Result<Option<ProjectConfig>> {
    let cwd = env::current_dir()?;
    for root in cwd.ancestors() {
        let path = root.join(PROJECT_CONFIG_PATH);
        match fs::read_to_string(&path) {
            Ok(text) => {
                let config = ProjectConfig::from_toml(&text)
                    .with_context(|| format!("{}", path.display()))?;
                return Ok(Some(config));
            }
            Err(error) if error.kind() == ErrorKind::NotFound => {}
            Err(error) => return Err(error).with_context(|| format!("{}", path.display())),
        }
        if git_boundary(root) {
            break;
        }
    }
    Ok(None)
}

/// Resolves the target project ID from CLI flag, `.rupicola/project.toml`, or Git common dir hash.
///
/// # Errors
///
/// Fails when not inside a Git checkout and no project is configured.
pub fn resolve_project(flag: Option<&str>, config: Option<&ProjectConfig>) -> Result<String> {
    if let Some(project) = flag {
        return Ok(project.to_owned());
    }
    if let Some(project) = config.and_then(|c| c.project.as_deref()) {
        return Ok(project.to_owned());
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
        bail!("Pass --project or run inside a Git checkout with .rupicola/project.toml");
    }
    let paths = String::from_utf8(output.stdout)?;
    let [git_dir, _] = paths.lines().collect::<Vec<_>>()[..] else {
        bail!("Git did not return the repository directories");
    };
    let common = fs::canonicalize(git_dir)?;
    let hash = common
        .as_os_str()
        .as_encoded_bytes()
        .iter()
        .fold(FNV1A_OFFSET_BASIS, |hash, byte| {
            (hash ^ u64::from(*byte)).wrapping_mul(FNV1A_PRIME)
        });
    Ok(format!("git-{hash:016x}"))
}

/// Normalizes a backend base URL by trimming whitespace, trailing `/`, and trailing `/mcp`.
#[must_use]
pub fn normalize_server_url(raw: &str) -> String {
    let base = raw.trim().trim_end_matches('/');
    base.strip_suffix(MCP_SUFFIX).unwrap_or(base).to_owned()
}

/// Resolves the backend base URL from CLI flag / `RUPICOLA_SERVER_URL` or the stored `server-url`.
///
/// # Errors
///
/// Fails when no backend URL is configured.
pub fn resolve_server_url(flag: Option<&str>) -> Result<String> {
    if let Some(raw) = flag {
        return Ok(normalize_server_url(raw));
    }
    if let Ok(raw) = env::var(SERVER_URL_ENV)
        && !raw.trim().is_empty()
    {
        return Ok(normalize_server_url(&raw));
    }
    if let Some(raw) = read_trimmed(&data_dir()?.join(SERVER_FILE))? {
        return Ok(normalize_server_url(&raw));
    }
    bail!("no backend: pass --server-url, set {SERVER_URL_ENV} or run `rupicola login`")
}

/// Resolves the bearer token from CLI flag / `RUPICOLA_RUNNER_TOKEN`, stored `auth-token`, or
/// `RUPICOLA_MCP_TOKEN`.
///
/// # Errors
///
/// Fails when no valid token is found.
pub fn resolve_token(flag: Option<&str>) -> Result<String> {
    if let Some(token) = flag.map(str::trim).filter(|t| !t.is_empty()) {
        return validate_token(token);
    }
    if let Ok(token) = env::var(RUNNER_TOKEN_ENV)
        && !token.trim().is_empty()
    {
        return validate_token(&token);
    }
    let path = data_dir()?.join(TOKEN_FILE);
    if let Some(token) = read_trimmed(&path)? {
        return validate_token(&token);
    }
    if let Ok(token) = env::var(MCP_TOKEN_ENV)
        && !token.trim().is_empty()
    {
        return validate_token(&token);
    }
    bail!(
        "Cannot read {}, {RUNNER_TOKEN_ENV} and {MCP_TOKEN_ENV} are unset; pass --token or run `rupicola login`",
        path.display()
    )
}

fn validate_token(raw: &str) -> Result<String> {
    let token = raw.trim();
    if !token.is_empty() && token.bytes().all(|b| b.is_ascii_graphic()) {
        Ok(token.to_owned())
    } else {
        bail!("Rupicola auth token must be non-empty printable ASCII without whitespace")
    }
}

/// Merges CLI preview overrides with `[preview]` from `.rupicola/project.toml`.
///
/// # Errors
///
/// Fails when `command` or `ready` is missing from both CLI flags and `.rupicola/project.toml`.
pub fn resolve_preview(
    config: Option<&ProjectConfig>,
    command_override: Option<String>,
    ready_override: Option<String>,
    timeout_override: Option<u64>,
) -> Result<PreviewConfig> {
    let file_preview = config.and_then(|c| c.preview.as_ref());
    let command = command_override
        .or_else(|| file_preview.map(|p| p.command.clone()))
        .context("missing `[preview].command` in .rupicola/project.toml or --preview-command")?;
    let ready = ready_override
        .or_else(|| file_preview.map(|p| p.ready.clone()))
        .context("missing `[preview].ready` in .rupicola/project.toml or --preview-ready")?;
    let timeout_secs = timeout_override
        .or_else(|| file_preview.map(|p| p.timeout_secs))
        .unwrap_or(DEFAULT_PREVIEW_TIMEOUT_SECS);
    Ok(PreviewConfig {
        command,
        ready,
        timeout_secs,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_server_urls() {
        assert_eq!(
            normalize_server_url("http://127.0.0.1:4730/"),
            "http://127.0.0.1:4730"
        );
        assert_eq!(
            normalize_server_url("https://rupicola.example.com/mcp"),
            "https://rupicola.example.com"
        );
    }

    #[test]
    fn preview_overrides_take_precedence_over_file_config() {
        let cfg = ProjectConfig {
            project: Some("demo".to_owned()),
            base: None,
            preview: Some(PreviewConfig {
                command: "file-cmd".to_owned(),
                ready: "http://127.0.0.1:{port}/healthz".to_owned(),
                timeout_secs: 90,
            }),
        };
        let merged = resolve_preview(Some(&cfg), Some("cli-cmd".to_owned()), None, None).unwrap();
        assert_eq!(merged.command, "cli-cmd");
        assert_eq!(merged.ready, "http://127.0.0.1:{port}/healthz");
        assert_eq!(merged.timeout_secs, 90);
        assert!(resolve_preview(None, None, None, None).is_err());
    }
}
