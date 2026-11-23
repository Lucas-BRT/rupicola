use anyhow::{Context, Result, anyhow, bail};
use serde::{Deserialize, Serialize};
use std::{
    env, fs,
    io::Write,
    path::{Path, PathBuf},
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};

const LINK: &str = ".rupicola/project.toml";

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

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Link {
    project: String,
}

fn git_boundary(root: &Path) -> bool {
    let marker = root.join(".git");
    marker.is_file() || marker.join("HEAD").is_file()
}

fn linked() -> Result<Option<Link>> {
    let cwd = env::current_dir()?;
    for root in cwd.ancestors() {
        let path = root.join(LINK);
        match fs::read_to_string(&path) {
            Ok(text) => {
                return toml::from_str(&text)
                    .map(Some)
                    .with_context(|| format!("{}", path.display()));
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

pub fn link(project: String) -> Result<()> {
    if !super::scenario::identifier(&project) {
        bail!("Use a project ID containing only letters, digits, hyphens and underscores");
    }
    let cwd = env::current_dir()?;
    let root = cwd.ancestors().find(|p| git_boundary(p)).unwrap_or(&cwd);
    let path = root.join(LINK);
    fs::create_dir_all(path.parent().unwrap())?;
    let contents = toml::to_string(&Link { project })?;
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

pub fn project() -> Result<Option<(String, String)>> {
    if let Some(link) = linked()? {
        return Ok(Some((link.project.clone(), link.project)));
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
    let [git_dir, checkout] = paths.lines().collect::<Vec<_>>()[..] else {
        bail!("Git did not return the repository directories");
    };
    let common = fs::canonicalize(git_dir)?;
    let root = if common.ends_with(".git") {
        common.parent().unwrap_or(Path::new(checkout))
    } else {
        Path::new(checkout)
    };
    let label = root
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("Git project")
        .to_owned();
    // FNV-1a keeps this local key stable across builds and Git worktrees.
    let hash = common
        .as_os_str()
        .as_encoded_bytes()
        .iter()
        .fold(0xcbf2_9ce4_8422_2325_u64, |hash, byte| {
            (hash ^ u64::from(*byte)).wrapping_mul(0x0100_0000_01b3)
        });
    Ok(Some((format!("git-{hash:016x}"), label)))
}

pub fn detect_project() -> Result<(String, String)> {
    project()?.ok_or_else(|| {
        anyhow!("Run inside a Git checkout, set a project with rupicola link, or provide --out")
    })
}

pub fn output() -> Result<PathBuf> {
    let root = data()?;
    let mut builder = fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder
        .create(&root)
        .with_context(|| format!("{}", root.display()))?;
    let time = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
    Ok(root
        .join("captures")
        .join(format!("{time}-{}", std::process::id())))
}
