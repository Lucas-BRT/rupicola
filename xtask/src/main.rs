//! Evaluation runner and analyzer. `plan` and `analyze` are offline; `run` spawns an agent
//! only with `--execute`. Never part of the distributed `rupicola` binary.
use clap::{Args, Parser, Subcommand};
use rupicola::evals::{Task, load};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::fs::{self, File};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

#[derive(Parser)]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}
#[derive(Subcommand)]
enum Cmd {
    /// Print the run matrix; spawns nothing.
    Plan(Matrix),
    /// Execute the matrix (dry run unless --execute). Resumable: finished runs are skipped.
    Run {
        #[command(flatten)]
        m: Matrix,
        #[arg(long)]
        execute: bool,
        /// Exact model id; recorded in every run and required.
        #[arg(long)]
        model: String,
        #[arg(long, default_value = "evals/results/run")]
        out: PathBuf,
        /// Stop before starting a run when cumulative reported cost reaches this.
        #[arg(long)]
        max_usd: f64,
        /// Per-run cost cap passed to the agent.
        #[arg(long, default_value_t = 2.0)]
        run_usd: f64,
        #[arg(long, default_value_t = 1800)]
        timeout_secs: u64,
        /// Do not isolate the agent configuration. User-level skills, hooks and plugins then
        /// leak into every condition and the run is not a clean comparison.
        #[arg(long)]
        no_isolate: bool,
    },
    /// Measure skill invocation on evals/triggers.toml (cheap: stops after two turns).
    Trigger {
        #[command(flatten)]
        m: Matrix,
        #[arg(long)]
        execute: bool,
        #[arg(long)]
        model: String,
        #[arg(long, default_value = "evals/results/trigger")]
        out: PathBuf,
        #[arg(long)]
        max_usd: f64,
        #[arg(long, default_value_t = 0.3)]
        run_usd: f64,
        #[arg(long, default_value_t = 300)]
        timeout_secs: u64,
        #[arg(long)]
        no_isolate: bool,
        /// Turns the agent may take before the run is cut; the Skill call must happen within them.
        #[arg(long, default_value_t = 2)]
        max_turns: u32,
        /// Keep each run's trajectory next to its result (for inspection).
        #[arg(long)]
        keep: bool,
    },
    /// Precision, recall and F1 of invocation per condition, with a paired comparison.
    TriggerAnalyze {
        out: PathBuf,
        #[arg(long)]
        base: String,
        #[arg(long)]
        cand: String,
        #[arg(long, default_value_t = 10000)]
        resamples: usize,
    },
    /// Paired comparison of two conditions from a results directory.
    Analyze {
        out: PathBuf,
        #[arg(long)]
        base: String,
        #[arg(long)]
        cand: String,
        #[arg(long, default_value_t = 10000)]
        resamples: usize,
        /// Also write rows.csv for external mixed-effects modelling.
        #[arg(long)]
        csv: bool,
    },
}
#[derive(Args, Clone)]
struct Matrix {
    #[arg(long, default_value = ".")]
    root: PathBuf,
    #[arg(long, default_value = "dev")]
    split: String,
    #[arg(long, default_value_t = 4)]
    k: usize,
    /// Condition as name=none or name=<git rev of this repo>. Repeatable.
    #[arg(long = "cond", required = true)]
    conds: Vec<String>,
    /// Restrict to task ids (repeatable).
    #[arg(long = "task")]
    tasks: Vec<String>,
}

struct Cleanup(Option<PathBuf>);
impl Drop for Cleanup {
    fn drop(&mut self) {
        if let Some(dir) = &self.0 {
            let _ = fs::remove_dir_all(dir);
        }
    }
}

/// A clean configuration directory holding only the credential keeps user-level skills,
/// hooks, plugins and MCP servers out of every condition.
fn setup_config(no_isolate: bool) -> R<(Option<PathBuf>, Cleanup)> {
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

type R<T> = Result<T, String>;

#[allow(clippy::type_complexity)]
fn matrix(m: &Matrix) -> R<(Vec<Task>, Vec<(String, String)>)> {
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
                .ok_or(format!("bad --cond {c}; use name=none or name=<rev>"))
        })
        .collect::<R<Vec<_>>>()?;
    Ok((tasks, conds))
}
fn run_id(task: &str, cond: &str, rep: usize) -> String {
    format!("{task}__{cond}__{rep}")
}
/// Deterministic order interleaving conditions and repetitions, so drift hits all conditions alike.
fn order(tasks: &[Task], conds: &[(String, String)], k: usize) -> Vec<(usize, usize, usize)> {
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

fn copy_dir(from: &Path, to: &Path) -> R<()> {
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
fn sh(dir: &Path, cmd: &str, timeout: Duration) -> (i32, String) {
    let child = Command::new("sh")
        .args(["-c", cmd])
        .current_dir(dir)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn();
    let Ok(mut child) = child else {
        return (127, "spawn failed".into());
    };
    let start = Instant::now();
    loop {
        match child.try_wait() {
            Ok(None) if start.elapsed() > timeout => {
                let _ = child.kill();
                break;
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(200)),
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
                .take(2000)
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
fn install_skill(root: &Path, rev: &str, dest: &Path) -> R<()> {
    fs::create_dir_all(dest).map_err(|error| error.to_string())?;
    let skip = "--exclude=evals --exclude=xtask --exclude=target --exclude=.git";
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
    let (code, out) = sh(root, &cmd, Duration::from_secs(60));
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

fn analyze_trajectory(path: &Path) -> Value {
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

#[allow(clippy::too_many_arguments)]
#[expect(
    clippy::cast_precision_loss,
    reason = "evaluation rates and statistical estimates are computed in floating point"
)]
fn execute_one(
    task: &Task,
    task_dir: &Path,
    cond: &(String, String),
    repo: &Path,
    skill_rev: Option<&str>,
    config_dir: Option<&Path>,
    rep: usize,
    model: &str,
    out: &Path,
    run_usd: f64,
    timeout: Duration,
) -> R<Value> {
    let id = run_id(&task.id, &cond.0, rep);
    // Each run gets its own root so files the agent writes next to the working directory
    // (`../x`) cannot reach another run; the whole root is deleted afterwards.
    let root = std::env::temp_dir().join("rupicola-eval").join(&id);
    let _ = fs::remove_dir_all(&root);
    let work = root.join("w");
    fs::create_dir_all(&work).map_err(|error| error.to_string())?;
    let fixture = task_dir.join("fixture");
    if fixture.is_dir() {
        copy_dir(&fixture, &work)?;
    }
    let git = "git -c user.name=eval -c user.email=eval@local";
    sh(
        &work,
        &format!("git init -q && git add -A && {git} commit -q --allow-empty -m base"),
        Duration::from_secs(30),
    );
    if let Some(rev) = skill_rev {
        install_skill(repo, rev, &work.join(".claude/skills/rupicola"))?;
    }
    let traj = out.join(format!("{id}.jsonl"));
    let mut cmd = Command::new("claude");
    if let Some(dir) = config_dir {
        cmd.env("CLAUDE_CONFIG_DIR", dir);
    }
    let mut child = cmd
        .args([
            "-p",
            &task.prompt,
            "--output-format",
            "stream-json",
            "--verbose",
            "--model",
            model,
        ])
        .args([
            "--permission-mode",
            "acceptEdits",
            "--allowedTools",
            "Bash Edit Write Read Glob Grep Skill",
        ])
        .args(["--max-budget-usd", &run_usd.to_string()])
        .current_dir(&work)
        .stdin(Stdio::null())
        .stdout(File::create(&traj).map_err(|error| error.to_string())?)
        .stderr(Stdio::null())
        .spawn()
        .map_err(|x| format!("cannot start `claude`: {x}"))?;
    let start = Instant::now();
    let mut timed_out = false;
    loop {
        match child.try_wait().map_err(|error| error.to_string())? {
            Some(_) => break,
            None if start.elapsed() > timeout => {
                let _ = child.kill();
                let _ = child.wait();
                timed_out = true;
                break;
            }
            None => std::thread::sleep(Duration::from_millis(500)),
        }
    }
    let mut record = analyze_trajectory(&traj);
    let mut verifiers = Vec::new();
    for v in &task.verifier {
        let (code, tail) = sh(&work, &v.command, Duration::from_secs(600));
        verifiers.push(json!({"name": v.name, "exit": code, "tail": tail}));
    }
    // The condition's setup must have held: the skill is loaded exactly when the condition has it.
    let loaded = record["skills_loaded"]
        .as_array()
        .is_some_and(|a| a.iter().any(|x| x == "rupicola"));
    let config_error = loaded != skill_rev.is_some();
    let report = record["final_text"].as_str().unwrap_or("").to_lowercase();
    let found: Vec<&str> = task
        .seeded_defect
        .iter()
        .filter(|d| {
            d.keywords
                .iter()
                .any(|k| report.contains(&k.to_lowercase()))
        })
        .map(|d| d.id.as_str())
        .collect();
    let recall = (!task.seeded_defect.is_empty())
        .then(|| found.len() as f64 / task.seeded_defect.len() as f64);
    let pass = if config_error {
        false
    } else if task.kind == "trigger" {
        record["skill_invoked"].as_bool() == Some(task.should_trigger)
    } else {
        // A review is only as good as the seeded defects it names.
        !timed_out && verifiers.iter().all(|v| v["exit"] == 0) && recall.is_none_or(|r| r >= 1.0)
    };
    let (_, diff) = sh(
        &work,
        "git add -A && git diff --cached --name-only",
        Duration::from_secs(30),
    );
    let o = record.as_object_mut().unwrap();
    o.insert("task".into(), json!(task.id));
    o.insert("cond".into(), json!(cond.0));
    o.insert("rep".into(), json!(rep));
    o.insert("model".into(), json!(model));
    o.insert("kind".into(), json!(task.kind));
    o.insert("framework".into(), json!(task.framework));
    o.insert("pass".into(), json!(pass));
    o.insert("timed_out".into(), json!(timed_out));
    o.insert("config_error".into(), json!(config_error));
    o.insert("defects_found".into(), json!(found));
    o.insert("review_recall".into(), json!(recall));
    o.insert("verifiers".into(), json!(verifiers));
    // Free-text constraints are not machine-checked yet; the changed files are kept for audit.
    o.insert(
        "changed_files".into(),
        json!(
            diff.lines()
                .filter(|f| !f.starts_with("vendor/")
                    && !f.starts_with("target/")
                    && *f != "Cargo.lock")
                .collect::<Vec<_>>()
        ),
    );
    let _ = fs::remove_dir_all(&root);
    Ok(record)
}

#[allow(clippy::too_many_arguments)]
#[expect(
    clippy::cast_precision_loss,
    reason = "the dry-run cost is a displayed estimate based on a floating-point unit price"
)]
fn run(
    m: &Matrix,
    execute: bool,
    model: &str,
    out: &Path,
    max_usd: f64,
    run_usd: f64,
    timeout: u64,
    no_isolate: bool,
) -> R<()> {
    let (tasks, conds) = matrix(m)?;
    let list = order(&tasks, &conds, m.k);
    if !execute {
        println!(
            "dry run: {} runs ({} tasks x {} conditions x k={}); worst-case cost {:.2} USD; pass --execute to start",
            list.len(),
            tasks.len(),
            conds.len(),
            m.k,
            list.len() as f64 * run_usd
        );
        return Ok(());
    }
    let (config, _cleanup) = setup_config(no_isolate)?;
    fs::create_dir_all(out).map_err(|error| error.to_string())?;
    fs::write(out.join("manifest.json"), serde_json::to_string_pretty(&json!({"model": model, "conditions": m.conds, "split": m.split, "k": m.k, "run_usd": run_usd, "max_usd": max_usd})).unwrap()).map_err(|error| error.to_string())?;
    let mut spent = 0.0;
    for (n, (t, c, rep)) in list.into_iter().enumerate() {
        let (task, cond) = (&tasks[t], &conds[c]);
        let file = out.join(format!("{}.json", run_id(&task.id, &cond.0, rep)));
        if let Ok(old) = fs::read_to_string(&file) {
            spent += serde_json::from_str::<Value>(&old)
                .ok()
                .and_then(|v| v["cost_usd"].as_f64())
                .unwrap_or(0.0);
            continue;
        }
        if spent >= max_usd {
            return Err(format!(
                "stopped: reported cost {spent:.2} reached --max-usd; rerun to resume"
            ));
        }
        let task_dir = m.root.join("evals/tasks").join(&task.id);
        let rec = execute_one(
            task,
            &task_dir,
            cond,
            &m.root,
            (cond.1 != "none").then_some(cond.1.as_str()),
            config.as_deref(),
            rep,
            model,
            out,
            run_usd,
            Duration::from_secs(timeout),
        )?;
        spent += rec["cost_usd"].as_f64().unwrap_or(0.0);
        if rec["config_error"] == true {
            eprintln!(
                "warning: skill presence did not match the condition in this run; it counts as a failure and the setup needs fixing"
            );
        }
        println!(
            "[{}] {} pass={} cost={:.3}",
            n + 1,
            file.file_name().unwrap().to_string_lossy(),
            rec["pass"],
            rec["cost_usd"].as_f64().unwrap_or(0.0)
        );
        fs::write(&file, serde_json::to_string_pretty(&rec).unwrap())
            .map_err(|error| error.to_string())?;
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
#[expect(
    clippy::cast_precision_loss,
    reason = "the dry-run cost is a displayed estimate based on a floating-point unit price"
)]
fn trigger_run(
    m: &Matrix,
    max_turns: u32,
    keep: bool,
    execute: bool,
    model: &str,
    out: &Path,
    max_usd: f64,
    run_usd: f64,
    timeout: u64,
    no_isolate: bool,
) -> R<()> {
    let prompts: Vec<_> = rupicola::evals::load_triggers(&m.root)?
        .into_iter()
        .filter(|t| t.split == m.split && (m.tasks.is_empty() || m.tasks.contains(&t.id)))
        .collect();
    let conds = parse_conds(&m.conds)?;
    let total = prompts.len() * conds.len() * m.k;
    if !execute {
        println!(
            "dry run: {total} runs ({} prompts x {} conditions x k={}); worst-case cost {:.2} USD; pass --execute to start",
            prompts.len(),
            conds.len(),
            m.k,
            total as f64 * run_usd
        );
        return Ok(());
    }
    let (config, _cleanup) = setup_config(no_isolate)?;
    fs::create_dir_all(out).map_err(|error| error.to_string())?;
    fs::write(
        out.join("manifest.json"),
        serde_json::to_string_pretty(
            &json!({"model": model, "conditions": m.conds, "split": m.split, "k": m.k}),
        )
        .unwrap(),
    )
    .map_err(|error| error.to_string())?;
    let mut spent = 0.0;
    for rep in 0..m.k {
        for t in &prompts {
            for (name, rev) in &conds {
                let id = format!("trig__{}__{name}__{rep}", t.id);
                let file = out.join(format!("{id}.json"));
                if let Ok(old) = fs::read_to_string(&file) {
                    spent += serde_json::from_str::<Value>(&old)
                        .ok()
                        .and_then(|v| v["cost_usd"].as_f64())
                        .unwrap_or(0.0);
                    continue;
                }
                if spent >= max_usd {
                    return Err(format!(
                        "stopped: reported cost {spent:.2} reached --max-usd; rerun to resume"
                    ));
                }
                let root = std::env::temp_dir().join("rupicola-eval").join(&id);
                let _ = fs::remove_dir_all(&root);
                let work = root.join("w");
                fs::create_dir_all(&work).map_err(|error| error.to_string())?;
                // A stub project, so "my app" prompts do not become a hunt for missing code.
                fs::create_dir_all(work.join("src")).map_err(|error| error.to_string())?;
                fs::write(
                    work.join("Cargo.toml"),
                    "[package]\nname = \"app\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
                )
                .map_err(|error| error.to_string())?;
                fs::write(work.join("src/main.rs"), "fn main() {}\n")
                    .map_err(|error| error.to_string())?;
                let with_skill = rev != "none";
                if with_skill {
                    install_skill(&m.root, rev, &work.join(".claude/skills/rupicola"))?;
                }
                let traj = out.join(format!("{id}.jsonl"));
                let mut cmd = Command::new("claude");
                if let Some(dir) = &config {
                    cmd.env("CLAUDE_CONFIG_DIR", dir);
                }
                let mut child = cmd
                    .args([
                        "-p",
                        &t.prompt,
                        "--output-format",
                        "stream-json",
                        "--verbose",
                        "--model",
                        model,
                        "--max-turns",
                        &max_turns.to_string(),
                    ])
                    .args([
                        "--permission-mode",
                        "plan",
                        "--allowedTools",
                        "Read Glob Grep Skill",
                    ])
                    .args(["--max-budget-usd", &run_usd.to_string()])
                    .current_dir(&work)
                    .stdin(Stdio::null())
                    .stdout(File::create(&traj).map_err(|error| error.to_string())?)
                    .stderr(Stdio::null())
                    .spawn()
                    .map_err(|x| format!("cannot start `claude`: {x}"))?;
                let start = Instant::now();
                loop {
                    if child
                        .try_wait()
                        .map_err(|error| error.to_string())?
                        .is_some()
                    {
                        break;
                    }
                    if start.elapsed() > Duration::from_secs(timeout) {
                        let _ = child.kill();
                        let _ = child.wait();
                        break;
                    }
                    std::thread::sleep(Duration::from_millis(300));
                }
                let mut rec = analyze_trajectory(&traj);
                let loaded = rec["skills_loaded"]
                    .as_array()
                    .is_some_and(|a| a.iter().any(|x| x == "rupicola"));
                let o = rec.as_object_mut().unwrap();
                o.remove("final_text");
                o.insert("prompt_id".into(), json!(t.id));
                o.insert("category".into(), json!(t.category));
                o.insert("expect".into(), json!(t.expect));
                o.insert("cond".into(), json!(name));
                o.insert("rep".into(), json!(rep));
                o.insert("model".into(), json!(model));
                o.insert("config_error".into(), json!(loaded != with_skill));
                spent += rec["cost_usd"].as_f64().unwrap_or(0.0);
                let _ = fs::remove_dir_all(&root);
                if !keep {
                    let _ = fs::remove_file(&traj);
                }
                println!(
                    "{id} invoked={} expect={} cost={:.4}",
                    rec["skill_invoked"],
                    t.expect,
                    rec["cost_usd"].as_f64().unwrap_or(0.0)
                );
                fs::write(&file, serde_json::to_string_pretty(&rec).unwrap())
                    .map_err(|error| error.to_string())?;
            }
        }
    }
    Ok(())
}

/// Reads a boolean outcome, falling back to `skill_invoked` for results written before `skill_engaged` existed.
fn hit(r: &Value, key: &str) -> bool {
    r.get(key).unwrap_or(&r["skill_invoked"]) == true
}

fn parse_conds(conds: &[String]) -> R<Vec<(String, String)>> {
    conds
        .iter()
        .map(|c| {
            c.split_once('=')
                .map(|(a, b)| (a.to_owned(), b.to_owned()))
                .ok_or(format!(
                    "bad --cond {c}; use name=none, name=worktree or name=<rev>"
                ))
        })
        .collect()
}

/// (precision, recall, f1, false-positive rate) from invocation counts on a prompt subset.
#[expect(
    clippy::cast_precision_loss,
    reason = "evaluation rates and statistical estimates are computed in floating point"
)]
fn prf(rows: &[(bool, f64)]) -> (f64, f64, f64, f64) {
    let tp: f64 = rows.iter().filter(|r| r.0).map(|r| r.1).sum();
    let pos = rows.iter().filter(|r| r.0).count() as f64;
    let fp: f64 = rows.iter().filter(|r| !r.0).map(|r| r.1).sum();
    let neg = rows.iter().filter(|r| !r.0).count() as f64;
    let precision = if tp + fp > 0.0 { tp / (tp + fp) } else { 0.0 };
    let recall = if pos > 0.0 { tp / pos } else { 0.0 };
    let f1 = if precision + recall > 0.0 {
        2.0 * precision * recall / (precision + recall)
    } else {
        0.0
    };
    (
        precision,
        recall,
        f1,
        if neg > 0.0 { fp / neg } else { 0.0 },
    )
}

fn trigger_analyze(out: &Path, base: &str, cand: &str, resamples: usize) -> R<()> {
    for (key, label) in [
        ("skill_invoked", "Skill tool called"),
        ("skill_engaged", "engaged (tool called or skill files read)"),
    ] {
        println!("\n=== {label} ===");
        trigger_metric(out, base, cand, resamples, key)?;
    }
    Ok(())
}

#[expect(
    clippy::cast_precision_loss,
    reason = "evaluation rates and statistical estimates are computed in floating point"
)]
fn trigger_metric(out: &Path, base: &str, cand: &str, resamples: usize, key: &str) -> R<()> {
    type Rates = BTreeMap<String, Vec<f64>>;

    if resamples == 0 {
        return Err("provide at least one bootstrap resample".into());
    }
    let mut rows: Vec<Value> = Vec::new();
    for entry in fs::read_dir(out)
        .map_err(|error| error.to_string())?
        .flatten()
    {
        let p = entry.path();
        if p.file_name().and_then(|n| n.to_str()).is_some_and(|n| {
            n.starts_with("trig__") && matches!(n.rsplit_once('.'), Some((_, "json")))
        }) {
            rows.push(
                serde_json::from_str(&fs::read_to_string(&p).map_err(|error| error.to_string())?)
                    .map_err(|error| error.to_string())?,
            );
        }
    }
    let bad = rows.iter().filter(|r| r["config_error"] == true).count();
    if bad > 0 {
        println!("WARNING: {bad} run(s) had a setup mismatch and are excluded.");
    }
    rows.retain(|r| r["config_error"] != true);
    // prompt -> (category, expect, per-cond invocation rate over repetitions)
    let mut prompts: BTreeMap<String, (String, bool, Rates)> = BTreeMap::new();
    for r in &rows {
        let entry = prompts
            .entry(r["prompt_id"].as_str().unwrap_or("").to_owned())
            .or_insert_with(|| {
                (
                    r["category"].as_str().unwrap_or("").to_owned(),
                    r["expect"] == true,
                    BTreeMap::new(),
                )
            });
        entry
            .2
            .entry(r["cond"].as_str().unwrap_or("").to_owned())
            .or_default()
            .push(f64::from(u8::from(hit(r, key))));
    }
    let rate = |p: &(String, bool, Rates), c: &str| {
        p.2.get(c).map(|v| v.iter().sum::<f64>() / v.len() as f64)
    };
    let ids: Vec<&String> = prompts
        .iter()
        .filter(|(_, p)| rate(p, base).is_some() && rate(p, cand).is_some())
        .map(|(k, _)| k)
        .collect();
    if ids.is_empty() {
        return Err("no prompt has both conditions".into());
    }
    let subset = |ids: &[&String], cat: Option<&str>, c: &str| -> Vec<(bool, f64)> {
        ids.iter()
            .filter_map(|id| {
                let p = &prompts[*id];
                (cat.is_none_or(|x| p.0 == x)).then(|| (p.1, rate(p, c).unwrap()))
            })
            .collect()
    };
    for c in [base, cand] {
        let all = prf(&subset(&ids, None, c));
        println!(
            "{c}: precision={:.3} recall={:.3} F1={:.3} false-positive-rate={:.3}",
            all.0, all.1, all.2, all.3
        );
        for cat in ["explicit", "implicit", "negative"] {
            let v = subset(&ids, Some(cat), c);
            let m = v.iter().map(|x| x.1).sum::<f64>() / v.len().max(1) as f64;
            println!("   {cat:<9} n={:<3} invoked rate={m:.3}", v.len());
        }
    }
    let metric = |ids: &[&String], f: fn((f64, f64, f64, f64)) -> f64, c: &str| {
        f(prf(&subset(ids, None, c)))
    };
    let imp_recall = |ids: &[&String], c: &str| {
        let v: Vec<(bool, f64)> = subset(ids, Some("implicit"), c);
        v.iter().map(|x| x.1).sum::<f64>() / v.len().max(1) as f64
    };
    let mut rng = Rng(0x9E37_79B9_7F4A_7C15);
    let (mut df1, mut dimp, mut dfpr) = (Vec::new(), Vec::new(), Vec::new());
    for _ in 0..resamples {
        let sample: Vec<&String> = (0..ids.len()).map(|_| ids[rng.index(ids.len())]).collect();
        df1.push(metric(&sample, |x| x.2, cand) - metric(&sample, |x| x.2, base));
        dimp.push(imp_recall(&sample, cand) - imp_recall(&sample, base));
        dfpr.push(metric(&sample, |x| x.3, cand) - metric(&sample, |x| x.3, base));
    }
    for (name, mut v, point) in [
        (
            "delta F1",
            df1,
            metric(&ids, |x| x.2, cand) - metric(&ids, |x| x.2, base),
        ),
        (
            "delta implicit recall",
            dimp,
            imp_recall(&ids, cand) - imp_recall(&ids, base),
        ),
        (
            "delta false-positive rate",
            dfpr,
            metric(&ids, |x| x.3, cand) - metric(&ids, |x| x.3, base),
        ),
    ] {
        v.sort_by(f64::total_cmp);
        let [low, high] = confidence_bounds(&v);
        println!(
            "{name} ({cand}-{base}) = {point:+.3}  95% prompt-cluster bootstrap CI [{low:+.3}, {high:+.3}]"
        );
    }
    // Exact McNemar on (prompt, rep) pairs, positives and negatives separately.
    for (label, want) in [("positives", true), ("negatives", false)] {
        let (mut b, mut c) = (0u64, 0u64);
        let mut cell: BTreeMap<(String, u64), (Option<bool>, Option<bool>)> = BTreeMap::new();
        for r in rows.iter().filter(|r| (r["expect"] == true) == want) {
            let slot = cell
                .entry((
                    r["prompt_id"].as_str().unwrap_or("").to_owned(),
                    r["rep"].as_u64().unwrap_or(0),
                ))
                .or_default();
            match r["cond"].as_str() {
                Some(x) if x == base => slot.0 = Some(hit(r, key)),
                Some(x) if x == cand => slot.1 = Some(hit(r, key)),
                _ => {}
            }
        }
        for (x, y) in cell.values() {
            if let (Some(x), Some(y)) = (x, y) {
                b += u64::from(*x && !*y);
                c += u64::from(!*x && *y);
            }
        }
        println!(
            "{label}: discordant {base}-only={b} {cand}-only={c} exact McNemar p={:.4}",
            mcnemar_p(b, c)
        );
    }
    println!(
        "Thresholds from the plan: implicit recall >= 0.85, false-positive rate <= 0.10, explicit recall not lower. Judge them on the holdout split, not the one used for tuning."
    );
    Ok(())
}

#[expect(
    clippy::cast_precision_loss,
    reason = "evaluation rates and statistical estimates are computed in floating point"
)]
fn ln_choose(n: u64, k: u64) -> f64 {
    let lf = |x: u64| (1..=x).map(|i| (i as f64).ln()).sum::<f64>();
    lf(n) - lf(k) - lf(n - k)
}
/// Exact two-sided sign test on discordant pairs (`McNemar` exact).
#[expect(
    clippy::cast_precision_loss,
    reason = "evaluation rates and statistical estimates are computed in floating point"
)]
fn mcnemar_p(b: u64, c: u64) -> f64 {
    let n = b + c;
    if n == 0 {
        return 1.0;
    }
    let m = b.min(c);
    let tail: f64 = (0..=m)
        .map(|i| (ln_choose(n, i) - n as f64 * 2f64.ln()).exp())
        .sum();
    (2.0 * tail).min(1.0)
}
fn confidence_bounds(sorted: &[f64]) -> [f64; 2] {
    let last = sorted.len() - 1;
    // Floor of 2.5% and 97.5% of the last index.
    [sorted[last / 40], sorted[last - last.div_ceil(40)]]
}

struct Rng(u64);
impl Rng {
    fn index(&mut self, len: usize) -> usize {
        let bound = u64::try_from(len).expect("array length fits in u64");
        usize::try_from(self.next() % bound).expect("remainder is less than array length")
    }

    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
}

#[expect(
    clippy::cast_precision_loss,
    reason = "evaluation rates and statistical estimates are computed in floating point"
)]
fn analyze(out: &Path, base: &str, cand: &str, resamples: usize, csv: bool) -> R<()> {
    if resamples == 0 {
        return Err("provide at least one bootstrap resample".into());
    }
    let mut rows: Vec<Value> = Vec::new();
    for entry in fs::read_dir(out)
        .map_err(|error| error.to_string())?
        .flatten()
    {
        let p = entry.path();
        if p.extension().is_some_and(|x| x == "json")
            && p.file_name().is_some_and(|n| n != "manifest.json")
        {
            rows.push(
                serde_json::from_str(&fs::read_to_string(&p).map_err(|error| error.to_string())?)
                    .map_err(|error| error.to_string())?,
            );
        }
    }
    let mut models: Vec<String> = rows
        .iter()
        .filter_map(|r| r["model"].as_str().map(str::to_owned))
        .collect();
    models.sort();
    models.dedup();
    if models.len() > 1 {
        return Err(format!(
            "results mix models {models:?}; analyze one model per directory"
        ));
    }
    if csv {
        let mut s = String::from(
            "task,cond,rep,kind,framework,pass,false_verified,skill_invoked,cost_usd,turns,input_tokens,output_tokens\n",
        );
        for r in &rows {
            let f = |k: &str| r[k].to_string().trim_matches('"').to_owned();
            writeln!(
                s,
                "{},{},{},{},{},{},{},{},{},{},{},{}",
                f("task"),
                f("cond"),
                f("rep"),
                f("kind"),
                f("framework"),
                f("pass"),
                f("false_verified"),
                f("skill_invoked"),
                f("cost_usd"),
                f("turns"),
                f("input_tokens"),
                f("output_tokens")
            )
            .expect("writing to a String cannot fail");
        }
        fs::write(out.join("rows.csv"), s).map_err(|error| error.to_string())?;
    }
    // (task, rep) -> pass per condition
    let mut cell: BTreeMap<(String, u64), (Option<bool>, Option<bool>)> = BTreeMap::new();
    let mut per_task: BTreeMap<String, (Vec<f64>, Vec<f64>)> = BTreeMap::new();
    for r in &rows {
        let (t, rep, pass) = (
            r["task"].as_str().unwrap_or("").to_owned(),
            r["rep"].as_u64().unwrap_or(0),
            r["pass"].as_bool().unwrap_or(false),
        );
        let slot = cell.entry((t.clone(), rep)).or_default();
        let entry = per_task.entry(t).or_default();
        match r["cond"].as_str() {
            Some(c) if c == base => {
                slot.0 = Some(pass);
                entry.0.push(f64::from(u8::from(pass)));
            }
            Some(c) if c == cand => {
                slot.1 = Some(pass);
                entry.1.push(f64::from(u8::from(pass)));
            }
            _ => {}
        }
    }
    let (mut b, mut c, mut pairs) = (0u64, 0u64, 0u64);
    for (x, y) in cell.values() {
        if let (Some(x), Some(y)) = (x, y) {
            pairs += 1;
            b += u64::from(*x && !*y);
            c += u64::from(!*x && *y);
        }
    }
    let mean = |v: &Vec<f64>| v.iter().sum::<f64>() / v.len().max(1) as f64;
    let diffs: Vec<f64> = per_task
        .values()
        .filter(|(x, y)| !x.is_empty() && !y.is_empty())
        .map(|(x, y)| mean(y) - mean(x))
        .collect();
    if diffs.is_empty() {
        return Err("no task has both conditions".into());
    }
    let mut rng = Rng(0x9E37_79B9_7F4A_7C15);
    let mut boots: Vec<f64> = (0..resamples)
        .map(|_| {
            (0..diffs.len())
                .map(|_| diffs[rng.index(diffs.len())])
                .sum::<f64>()
                / diffs.len() as f64
        })
        .collect();
    boots.sort_by(f64::total_cmp);
    let [low, high] = confidence_bounds(&boots);
    let rate = |cond: &str, key: &str| {
        let v: Vec<f64> = rows
            .iter()
            .filter(|r| r["cond"] == cond)
            .map(|r| f64::from(u8::from(r[key] == true)))
            .collect();
        mean(&v)
    };
    let cost = |cond: &str| {
        mean(
            &rows
                .iter()
                .filter(|r| r["cond"] == cond)
                .filter_map(|r| r["cost_usd"].as_f64())
                .collect(),
        )
    };
    let pass_k = |cond: &str| {
        let mut by: BTreeMap<&str, bool> = BTreeMap::new();
        for r in rows.iter().filter(|r| r["cond"] == cond) {
            let a = by.entry(r["task"].as_str().unwrap_or("")).or_insert(true);
            *a &= r["pass"] == true;
        }
        by.values().filter(|x| **x).count() as f64 / by.len().max(1) as f64
    };
    println!(
        "model: {}  paired (task,rep) cells: {pairs}  tasks: {}",
        models.first().map_or("?", String::as_str),
        diffs.len()
    );
    println!(
        "{base}: pass={:.3} pass^k={:.3} false_verified={:.3} cost={:.3}",
        rate(base, "pass"),
        pass_k(base),
        rate(base, "false_verified"),
        cost(base)
    );
    println!(
        "{cand}: pass={:.3} pass^k={:.3} false_verified={:.3} cost={:.3}",
        rate(cand, "pass"),
        pass_k(cand),
        rate(cand, "false_verified"),
        cost(cand)
    );
    println!(
        "delta pass ({cand}-{base}) = {:+.3}  95% cluster-bootstrap CI [{:+.3}, {:+.3}] ({resamples} resamples over tasks)",
        mean(&diffs),
        low,
        high
    );
    println!(
        "discordant pairs: {base}-only={b} {cand}-only={c}  exact McNemar p={:.4}",
        mcnemar_p(b, c)
    );
    println!(
        "Not computed here: mixed-effects logistic model (use --csv), review recall, constraint violations. Pairs within a task are correlated; trust the cluster CI over the McNemar p."
    );
    Ok(())
}

fn main() {
    let result = match Cli::parse().cmd {
        Cmd::Plan(m) => matrix(&m).map(|(tasks, conds)| {
            for (t, c, rep) in order(&tasks, &conds, m.k) {
                println!("{}", run_id(&tasks[t].id, &conds[c].0, rep));
            }
        }),
        Cmd::Run {
            m,
            execute,
            model,
            out,
            max_usd,
            run_usd,
            timeout_secs,
            no_isolate,
        } => run(
            &m,
            execute,
            &model,
            &out,
            max_usd,
            run_usd,
            timeout_secs,
            no_isolate,
        ),
        Cmd::Trigger {
            m,
            execute,
            model,
            out,
            max_usd,
            run_usd,
            timeout_secs,
            no_isolate,
            max_turns,
            keep,
        } => trigger_run(
            &m,
            max_turns,
            keep,
            execute,
            &model,
            &out,
            max_usd,
            run_usd,
            timeout_secs,
            no_isolate,
        ),
        Cmd::TriggerAnalyze {
            out,
            base,
            cand,
            resamples,
        } => trigger_analyze(&out, &base, &cand, resamples),
        Cmd::Analyze {
            out,
            base,
            cand,
            resamples,
            csv,
        } => analyze(&out, &base, &cand, resamples, csv),
    };
    if let Err(message) = result {
        eprintln!("{message}");
        std::process::exit(1);
    }
}
