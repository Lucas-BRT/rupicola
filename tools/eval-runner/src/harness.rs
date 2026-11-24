use crate::{EXIT_SPAWN_FAILED, Matrix, OUTPUT_TAIL_CHARS, SH_POLL};
use anyhow::{Context as _, Result, bail};
use rupicola_core::evals::{Task, load};
use serde_json::{Value, json};
use std::fs;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

#[cfg(unix)]
const CONFIG_DIR_MODE: u32 = 0o700;
#[cfg(unix)]
const CREDENTIALS_FILE_MODE: u32 = 0o600;
const TAR_EXCLUDES: &[&str] = &[
    "--exclude=evals",
    "--exclude=tools/eval-runner",
    "--exclude=target",
    "--exclude=.git",
];

/// A clean configuration directory holding only the credential keeps user-level skills,
/// hooks, plugins and MCP servers out of every condition.
pub(crate) fn setup_config(no_isolate: bool) -> Result<Option<tempfile::TempDir>> {
    if no_isolate {
        return Ok(None);
    }
    let home = std::env::var_os("HOME").context("HOME is not set")?;
    let creds = Path::new(&home).join(".claude/.credentials.json");
    let dir = tempfile::Builder::new()
        .prefix("rupicola-eval-config-")
        .tempdir()
        .context("create isolated eval config tempdir")?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        fs::set_permissions(dir.path(), fs::Permissions::from_mode(CONFIG_DIR_MODE))
            .context("set isolated eval config directory permissions")?;
    }
    let dst = dir.path().join(".credentials.json");
    fs::copy(&creds, &dst).with_context(|| {
        format!(
            "cannot copy {} (use --no-isolate to skip isolation)",
            creds.display()
        )
    })?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        fs::set_permissions(&dst, fs::Permissions::from_mode(CREDENTIALS_FILE_MODE))
            .context("set isolated credentials file permissions")?;
    }
    Ok(Some(dir))
}

/// A condition: its name and the git revision of the skill it installs (`none`
/// for no skill).
pub(crate) type Condition = (String, String);

pub(crate) fn matrix(m: &Matrix) -> Result<(Vec<Task>, Vec<Condition>)> {
    let tasks: Vec<Task> = load(&m.root)?
        .into_iter()
        .filter(|t| t.split == m.split && (m.tasks.is_empty() || m.tasks.contains(&t.id)))
        .collect();
    let conds = m
        .conds
        .iter()
        .map(|c| {
            c.split_once('=')
                .map(|(a, b)| (a.to_owned(), b.to_owned()))
                .with_context(|| format!("bad --cond {c}; use name=none or name=<rev>"))
        })
        .collect::<Result<Vec<_>>>()?;
    Ok((tasks, conds))
}
pub(crate) fn run_id(task: &str, cond: &str, rep: usize) -> String {
    format!("{task}__{cond}__{rep}")
}
/// Deterministic order interleaving conditions and repetitions, so drift hits all conditions alike.
pub(crate) fn order(
    tasks: &[Task],
    conds: &[(String, String)],
    k: usize,
) -> Vec<(usize, usize, usize)> {
    let mut v = Vec::new();
    for rep in 0..k {
        for t in 0..tasks.len() {
            for c in 0..conds.len() {
                v.push((t, c, rep));
            }
        }
    }
    v
}

pub(crate) fn copy_dir(from: &Path, to: &Path) -> Result<()> {
    fs::create_dir_all(to)?;
    for entry in fs::read_dir(from)? {
        let entry = entry?;
        let (src, dst) = (entry.path(), to.join(entry.file_name()));
        if src.is_dir() {
            copy_dir(&src, &dst)?;
        } else {
            fs::copy(&src, &dst)?;
        }
    }
    Ok(())
}
pub(crate) fn sh(dir: &Path, cmd: &str, timeout: Duration) -> (i32, String) {
    let child = Command::new("sh")
        .args(["-c", cmd])
        .current_dir(dir)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn();
    let Ok(mut child) = child else {
        return (EXIT_SPAWN_FAILED, "spawn failed".into());
    };
    let start = Instant::now();
    loop {
        match child.try_wait() {
            Ok(None) if start.elapsed() > timeout => {
                let _ = child.kill();
                break;
            }
            Ok(None) => std::thread::sleep(SH_POLL),
            Ok(Some(_)) | Err(_) => break,
        }
    }
    match child.wait_with_output() {
        Ok(o) => {
            let mut text = String::from_utf8_lossy(&o.stderr).into_owned();
            text.push_str(&String::from_utf8_lossy(&o.stdout));
            let tail: String = text
                .chars()
                .rev()
                .take(OUTPUT_TAIL_CHARS)
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
                .collect();
            (o.status.code().unwrap_or(-1), tail)
        }
        Err(_) => (-1, "wait failed".into()),
    }
}

fn extract_tar_stream(stdin: Stdio, dest: &Path, rev: &str) -> Result<()> {
    let extract = Command::new("tar")
        .arg("-x")
        .args(TAR_EXCLUDES)
        .arg("-C")
        .arg(dest)
        .stdin(stdin)
        .output()
        .context("run tar extract")?;
    if !extract.status.success() {
        bail!(
            "cannot extract skill at {rev}: {}",
            String::from_utf8_lossy(&extract.stderr)
        );
    }
    Ok(())
}

/// Extracts a skill version into `dest`.
pub(crate) fn install_skill(root: &Path, rev: &str, dest: &Path) -> Result<()> {
    fs::create_dir_all(dest)?;
    // `worktree` takes the current files, tracked or not, so uncommitted edits can be measured.
    let (rev, desc_file) = match rev.split_once('+') {
        Some((r, f)) => (r, Some(f)),
        None => (rev, None),
    };
    if rev == "worktree" {
        let mut git = Command::new("git")
            .arg("-C")
            .arg(root)
            .args(["ls-files", "-co", "--exclude-standard", "-z"])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .context("spawn git ls-files")?;
        let git_stdout = git.stdout.take().context("git ls-files stdout")?;
        let mut tar_create = Command::new("tar")
            .current_dir(root)
            .args(["--null", "-T", "-", "-c"])
            .stdin(Stdio::from(git_stdout))
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .context("spawn tar create")?;
        let tar_stdout = tar_create.stdout.take().context("tar create stdout")?;
        let extract_res = extract_tar_stream(Stdio::from(tar_stdout), dest, rev);
        let git_out = git.wait_with_output().context("wait git ls-files")?;
        let tar_out = tar_create.wait_with_output().context("wait tar create")?;
        if !git_out.status.success() {
            bail!(
                "cannot list worktree files: {}",
                String::from_utf8_lossy(&git_out.stderr)
            );
        }
        if !tar_out.status.success() {
            bail!(
                "cannot archive worktree files: {}",
                String::from_utf8_lossy(&tar_out.stderr)
            );
        }
        extract_res?;
    } else {
        let mut git = Command::new("git")
            .arg("-C")
            .arg(root)
            .arg("archive")
            .arg(rev)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .context("spawn git archive")?;
        let git_stdout = git.stdout.take().context("git archive stdout")?;
        let extract_res = extract_tar_stream(Stdio::from(git_stdout), dest, rev);
        let git_out = git.wait_with_output().context("wait git archive")?;
        if !git_out.status.success() {
            bail!(
                "cannot archive skill at {rev}: {}",
                String::from_utf8_lossy(&git_out.stderr)
            );
        }
        extract_res?;
    }
    // `<rev>+<file>` swaps the description for the file's text, to compare wordings.
    if let Some(file) = desc_file {
        let text = fs::read_to_string(root.join(file)).with_context(|| file.to_owned())?;
        let skill = dest.join("SKILL.md");
        let body = fs::read_to_string(&skill)?;
        let replaced: Vec<String> = body
            .lines()
            .map(|l| {
                if l.starts_with("description:") {
                    format!("description: {}", text.trim())
                } else {
                    l.to_owned()
                }
            })
            .collect();
        fs::write(&skill, replaced.join("\n"))?;
    }
    Ok(())
}

pub(crate) fn analyze_trajectory(path: &Path) -> Value {
    let text = fs::read_to_string(path).unwrap_or_default();
    let (mut invoked, mut engaged, mut ran_cargo) = (false, false, false);
    let mut skills_loaded: Option<Vec<String>> = None;
    let mut result = json!({});
    for line in text.lines() {
        let Ok(v) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        if v["type"] == "system" && v["subtype"] == "init" {
            skills_loaded = v["skills"].as_array().map(|a| {
                a.iter()
                    .filter_map(|s| s.as_str().map(str::to_owned))
                    .collect()
            });
        }
        if v["type"] == "assistant" {
            for item in v["message"]["content"].as_array().into_iter().flatten() {
                if item["type"] != "tool_use" {
                    continue;
                }
                if item["name"] == "Skill"
                    && item["input"]["skill"]
                        .as_str()
                        .is_some_and(|n| n.ends_with("rupicola"))
                {
                    invoked = true;
                }
                if item["name"] == "Read"
                    && item["input"]["file_path"]
                        .as_str()
                        .is_some_and(|p| p.contains("skills/rupicola"))
                {
                    engaged = true;
                }
                if item["name"] == "Bash"
                    && item["input"]["command"]
                        .as_str()
                        .is_some_and(|c| c.contains("cargo "))
                {
                    ran_cargo = true;
                }
            }
        }
        if v["type"] == "result" {
            result = v;
        }
    }
    let final_text = result["result"].as_str().unwrap_or("").to_lowercase();
    // "unverified" and "not verified" are the opposite of a verification claim.
    let mut claim_text = final_text.clone();
    for neg in [
        "unverified",
        "not verified",
        "never verified",
        "could not be verified",
        "cannot be verified",
        "can't be verified",
    ] {
        claim_text = claim_text.replace(neg, "");
    }
    let claims = [
        "verified",
        "tests pass",
        "compiles",
        "builds successfully",
        "cargo check pass",
    ]
    .iter()
    .any(|w| claim_text.contains(w));
    json!({
        // Structured: a `Skill` tool call whose input names rupicola.
        "skill_invoked": invoked,
        // Looser: the Skill tool was called or the agent read the skill's own files directly.
        "skill_engaged": invoked || engaged,
        // Skills the harness reported at start; used to audit that each condition's setup held.
        "skills_loaded": skills_loaded,
        // Heuristic: a verification claim in the final message with no cargo command in the trajectory.
        "false_verified": claims && !ran_cargo,
        "cost_usd": result["total_cost_usd"],
        "turns": result["num_turns"],
        "duration_ms": result["duration_ms"],
        "input_tokens": result["usage"]["input_tokens"],
        "output_tokens": result["usage"]["output_tokens"],
        "final_text": result["result"],
        "completed": result["subtype"] == "success",
    })
}
