use serde::{Deserialize, Serialize};
use std::{
    env, fs,
    io::Write,
    path::{Path, PathBuf},
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};

const UNIT: &str = "rupicola.service";
const LINK: &str = ".rupicola/project.toml";

fn directory(variable: &str, fallback: &str) -> Result<PathBuf, String> {
    if let Some(path) = env::var_os(variable)
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
    {
        return Ok(path);
    }
    env::home_dir()
        .map(|p| p.join(fallback))
        .ok_or_else(|| format!("Cannot resolve {variable} or the home directory"))
}

pub fn data() -> Result<PathBuf, String> {
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

fn linked() -> Result<Option<Link>, String> {
    let cwd = env::current_dir().map_err(|e| e.to_string())?;
    for root in cwd.ancestors() {
        let path = root.join(LINK);
        match fs::read_to_string(&path) {
            Ok(text) => {
                return toml::from_str(&text)
                    .map(Some)
                    .map_err(|e| format!("{}: {e}", path.display()));
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(format!("{}: {error}", path.display())),
        }
        if git_boundary(root) {
            break;
        }
    }
    Ok(None)
}

pub fn link(project: String) -> Result<(), String> {
    if !super::scenario::identifier(&project) {
        return Err(
            "Use a project ID containing only letters, digits, hyphens and underscores".into(),
        );
    }
    let cwd = env::current_dir().map_err(|e| e.to_string())?;
    let root = cwd.ancestors().find(|p| git_boundary(p)).unwrap_or(&cwd);
    let path = root.join(LINK);
    fs::create_dir_all(path.parent().unwrap()).map_err(|e| e.to_string())?;
    let contents = toml::to_string(&Link { project }).map_err(|e| e.to_string())?;
    match fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
    {
        Ok(mut file) => file
            .write_all(contents.as_bytes())
            .map_err(|e| e.to_string())?,
        Err(error)
            if error.kind() == std::io::ErrorKind::AlreadyExists
                && fs::read_to_string(&path).is_ok_and(|s| s == contents) => {}
        Err(error) => return Err(format!("{}: {error}", path.display())),
    }
    println!("Linked {}", path.display());
    Ok(())
}

pub fn project() -> Result<Option<(String, String)>, String> {
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
        .map_err(|error| format!("Detect Git project: {error}"))?;
    if !output.status.success() {
        return Ok(None);
    }
    let paths = String::from_utf8(output.stdout).map_err(|e| e.to_string())?;
    let [git_dir, checkout] = paths.lines().collect::<Vec<_>>()[..] else {
        return Err("Git did not return the repository directories".into());
    };
    let common = fs::canonicalize(git_dir).map_err(|e| e.to_string())?;
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

pub fn detect_project() -> Result<(String, String), String> {
    project()?.ok_or_else(|| {
        "Run inside a Git checkout, set a project with rupicola link, or provide --out".into()
    })
}

pub fn output() -> Result<PathBuf, String> {
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
        .map_err(|e| format!("{}: {e}", root.display()))?;
    let time = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_nanos();
    Ok(root
        .join("captures")
        .join(format!("{time}-{}", std::process::id())))
}

fn unit_arg(path: &Path) -> Result<String, String> {
    let text = path
        .to_str()
        .filter(|s| !s.chars().any(char::is_control))
        .ok_or("Unsupported service path")?;
    Ok(format!(
        "\"{}\"",
        text.replace('\\', "\\\\")
            .replace('"', "\\\"")
            .replace('%', "%%")
    ))
}

fn systemctl(args: &[&str]) -> Result<(), String> {
    let status = Command::new("systemctl")
        .arg("--user")
        .args(args)
        .status()
        .map_err(|e| e.to_string())?;
    if status.success() {
        Ok(())
    } else {
        Err(format!(
            "systemctl --user {} exited with {status}",
            args.join(" ")
        ))
    }
}

pub fn setup(print_unit: bool) -> Result<(), String> {
    if !cfg!(target_os = "linux") {
        return Err("Background setup currently supports Linux with systemd --user".into());
    }
    let panel = env::var_os("PATH")
        .into_iter()
        .flat_map(|path| env::split_paths(&path).collect::<Vec<_>>())
        .map(|path| path.join("rupicola-panel"))
        .find(|path| path.is_file())
        .ok_or("Install rupicola-panel and add it to PATH")?;
    let executable = fs::canonicalize(panel).map_err(|e| e.to_string())?;
    let data = data()?;
    let service = format!(
        "[Unit]\nDescription=Rupicola evidence panel\n\n[Service]\nExecStart=:{} --captures {} --db {}\nRestart=on-failure\nUMask=0077\n\n[Install]\nWantedBy=default.target\n",
        unit_arg(&executable)?,
        unit_arg(&data.join("captures"))?,
        unit_arg(&data.join("panel.db"))?
    );
    if print_unit {
        print!("{service}");
        return Ok(());
    }
    systemctl(&["list-jobs", "--no-legend"])?;
    let unit = directory("XDG_CONFIG_HOME", ".config")?
        .join("systemd/user")
        .join(UNIT);
    fs::create_dir_all(unit.parent().unwrap()).map_err(|e| e.to_string())?;
    match fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&unit)
    {
        Ok(mut file) => file
            .write_all(service.as_bytes())
            .map_err(|e| e.to_string())?,
        Err(error)
            if error.kind() == std::io::ErrorKind::AlreadyExists
                && fs::read_to_string(&unit).is_ok_and(|s| s == service) => {}
        Err(error) => return Err(format!("{}: {error}", unit.display())),
    }
    systemctl(&["daemon-reload"])?;
    systemctl(&["enable", "--now", UNIT])?;
    println!("Panel: http://127.0.0.1:4731");
    Ok(())
}

pub fn doctor() -> Result<(), String> {
    println!("Capture directory: {}", data()?.join("captures").display());
    match project()? {
        Some((id, label)) => println!("Project: {label} ({id})"),
        None => println!("No project detected in the current directory"),
    }
    systemctl(&["is-active", UNIT])
}
