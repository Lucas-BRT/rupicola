use super::{
    COPY_TIMEOUT, Command, Duration, EXIT_SPAWN_FAILED, Instant, Matrix, OUTPUT_TAIL_CHARS, Path,
    PathBuf, SH_POLL, Stdio, Task, Value, fs, json, load,
};

pub(crate) struct Cleanup(Option<PathBuf>);
impl Drop for Cleanup {
    fn drop(&mut self) {
        if let Some(dir) = &self.0 {
            let _ = fs::remove_dir_all(dir);
        }
    }
}

/// A clean configuration directory holding only the credential keeps user-level skills,
/// hooks, plugins and MCP servers out of every condition.
pub(crate) fn setup_config(no_isolate: bool) -> R<(Option<PathBuf>, Cleanup)> {
    let config = (!no_isolate).then(|| std::env::temp_dir().join("rupicola-eval-config"));
    let cleanup = Cleanup(config.clone());
    if let Some(dir) = &config {
        let home = std::env::var_os("HOME").ok_or("HOME is not set")?;
        let creds = Path::new(&home).join(".claude/.credentials.json");
        let _ = fs::remove_dir_all(dir);
        fs::create_dir_all(dir).map_err(|error| error.to_string())?;
        fs::copy(&creds, dir.join(".credentials.json")).map_err(|x| {
            format!(
                "cannot copy {} (use --no-isolate to skip isolation): {x}",
                creds.display()
            )
        })?;
    }
    Ok((config, cleanup))
}

pub(crate) type R<T> = Result<T, String>;

/// A condition: its name and the git revision of the skill it installs (`none`
/// for no skill).
pub(crate) type Condition = (String, String);

pub(crate) fn matrix(m: &Matrix) -> R<(Vec<Task>, Vec<Condition>)> {
    let tasks: Vec<Task> = load(&m.root)
        .map_err(|e| format!("{e:#}"))?
        .into_iter()
        .filter(|t| t.split == m.split && (m.tasks.is_empty() || m.tasks.contains(&t.id)))
        .collect();
    let conds = m
        .conds
        .iter()
        .map(|c| {
            c.split_once('=')
                .map(|(a, b)| (a.to_owned(), b.to_owned()))
                .ok_or(format!("bad --cond {c}; use name=none or name=<rev>"))
        })
        .collect::<R<Vec<_>>>()?;
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

pub(crate) fn copy_dir(from: &Path, to: &Path) -> R<()> {
    fs::create_dir_all(to).map_err(|error| error.to_string())?;
    for entry in fs::read_dir(from)
        .map_err(|error| error.to_string())?
        .flatten()
    {
        let (src, dst) = (entry.path(), to.join(entry.file_name()));
        if src.is_dir() {
            copy_dir(&src, &dst)?;
        } else {
            fs::copy(&src, &dst).map_err(|error| error.to_string())?;
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

/// Extracts a skill version into `dest`.
pub(crate) fn install_skill(root: &Path, rev: &str, dest: &Path) -> R<()> {
    fs::create_dir_all(dest).map_err(|error| error.to_string())?;
    let skip = "--exclude=evals --exclude=tools/eval-runner --exclude=target --exclude=.git";
    // `worktree` takes the current files, tracked or not, so uncommitted edits can be measured.
    let (rev, desc_file) = match rev.split_once('+') {
        Some((r, f)) => (r, Some(f)),
        None => (rev, None),
    };
    let source = if rev == "worktree" {
        format!(
            "git -C '{}' ls-files -co --exclude-standard -z | tar --null -T - -c",
            root.display()
        )
    } else {
        format!("git -C '{}' archive {rev}", root.display())
    };
    let cmd = format!(
        "cd '{}' && {source} | tar -x {skip} -C '{}'",
        root.display(),
        dest.display()
    );
    let (code, out) = sh(root, &cmd, COPY_TIMEOUT);
    if code != 0 {
        return Err(format!("cannot extract skill at {rev}: {out}"));
    }
    // `<rev>+<file>` swaps the description for the file's text, to compare wordings.
    if let Some(file) = desc_file {
        let text = fs::read_to_string(root.join(file)).map_err(|x| format!("{file}: {x}"))?;
        let skill = dest.join("SKILL.md");
        let body = fs::read_to_string(&skill).map_err(|error| error.to_string())?;
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
        fs::write(&skill, replaced.join("\n")).map_err(|error| error.to_string())?;
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
