use super::{
    AGENT_POLL, Command, Duration, File, GIT_TIMEOUT, Instant, Matrix, Path, PathBuf, R, Stdio,
    Task, VERIFIER_TIMEOUT, Value, analyze_trajectory, copy_dir, fs, install_skill, json, matrix,
    order, run_id, setup_config, sh,
};

/// Flags shared by the `run` and `trigger` commands.
pub(crate) struct RunOptions<'a> {
    pub(crate) execute: bool,
    pub(crate) model: &'a str,
    pub(crate) out: &'a Path,
    pub(crate) max_usd: f64,
    pub(crate) run_usd: f64,
    pub(crate) timeout_secs: u64,
    pub(crate) no_isolate: bool,
}

/// What every agent run shares: the repository, agent configuration, model, output
/// directory and budgets.
pub(crate) struct RunEnv<'a> {
    pub(crate) repo: &'a Path,
    pub(crate) config_dir: Option<&'a Path>,
    pub(crate) model: &'a str,
    pub(crate) out: &'a Path,
    pub(crate) run_usd: f64,
    pub(crate) timeout: Duration,
}

/// Creates the run's isolated working tree: the task fixture as a fresh git
/// repository, plus the skill when the condition has it. Returns the root to
/// delete afterwards and the working directory inside it.
pub(crate) fn prepare_workspace(
    task_dir: &Path,
    id: &str,
    repo: &Path,
    skill_rev: Option<&str>,
) -> R<(PathBuf, PathBuf)> {
    // Each run gets its own root so files the agent writes next to the working directory
    // (`../x`) cannot reach another run; the whole root is deleted afterwards.
    let root = std::env::temp_dir().join("rupicola-eval").join(id);
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
        GIT_TIMEOUT,
    );
    if let Some(rev) = skill_rev {
        install_skill(repo, rev, &work.join(".claude/skills/rupicola"))?;
    }
    Ok((root, work))
}

/// Runs the agent on `prompt` in `work`, streaming its trajectory to `traj`.
/// Returns whether it was killed for exceeding the time limit.
pub(crate) fn run_agent(prompt: &str, work: &Path, traj: &Path, env: &RunEnv) -> R<bool> {
    let mut cmd = Command::new("claude");
    if let Some(dir) = env.config_dir {
        cmd.env("CLAUDE_CONFIG_DIR", dir);
    }
    let mut child = cmd
        .args([
            "-p",
            prompt,
            "--output-format",
            "stream-json",
            "--verbose",
            "--model",
            env.model,
        ])
        .args([
            "--permission-mode",
            "acceptEdits",
            "--allowedTools",
            "Bash Edit Write Read Glob Grep Skill",
        ])
        .args(["--max-budget-usd", &env.run_usd.to_string()])
        .current_dir(work)
        .stdin(Stdio::null())
        .stdout(File::create(traj).map_err(|error| error.to_string())?)
        .stderr(Stdio::null())
        .spawn()
        .map_err(|x| format!("cannot start `claude`: {x}"))?;
    let start = Instant::now();
    loop {
        match child.try_wait().map_err(|error| error.to_string())? {
            Some(_) => return Ok(false),
            None if start.elapsed() > env.timeout => {
                let _ = child.kill();
                let _ = child.wait();
                return Ok(true);
            }
            None => std::thread::sleep(AGENT_POLL),
        }
    }
}

/// Seeded defects whose keywords appear in the agent's final report.
pub(crate) fn defects_found<'t>(task: &'t Task, record: &Value) -> Vec<&'t str> {
    let report = record["final_text"].as_str().unwrap_or("").to_lowercase();
    task.seeded_defect
        .iter()
        .filter(|d| {
            d.keywords
                .iter()
                .any(|k| report.contains(&k.to_lowercase()))
        })
        .map(|d| d.id.as_str())
        .collect()
}

#[expect(
    clippy::cast_precision_loss,
    reason = "evaluation rates and statistical estimates are computed in floating point"
)]
pub(crate) fn execute_one(
    task: &Task,
    task_dir: &Path,
    cond: &(String, String),
    rep: usize,
    env: &RunEnv,
) -> R<Value> {
    let skill_rev = (cond.1 != "none").then_some(cond.1.as_str());
    let id = run_id(&task.id, &cond.0, rep);
    let (root, work) = prepare_workspace(task_dir, &id, env.repo, skill_rev)?;
    let traj = env.out.join(format!("{id}.jsonl"));
    let timed_out = run_agent(&task.prompt, &work, &traj, env)?;
    let mut record = analyze_trajectory(&traj);
    let mut verifiers = Vec::new();
    for v in &task.verifier {
        let (code, tail) = sh(&work, &v.command, VERIFIER_TIMEOUT);
        verifiers.push(json!({"name": v.name, "exit": code, "tail": tail}));
    }
    // The condition's setup must have held: the skill is loaded exactly when the condition has it.
    let loaded = record["skills_loaded"]
        .as_array()
        .is_some_and(|a| a.iter().any(|x| x == "rupicola"));
    let config_error = loaded != skill_rev.is_some();
    let found = defects_found(task, &record);
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
        GIT_TIMEOUT,
    );
    let o = record.as_object_mut().unwrap();
    o.insert("task".into(), json!(task.id));
    o.insert("cond".into(), json!(cond.0));
    o.insert("rep".into(), json!(rep));
    o.insert("model".into(), json!(env.model));
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

#[expect(
    clippy::cast_precision_loss,
    reason = "the dry-run cost is a displayed estimate based on a floating-point unit price"
)]
pub(crate) fn run(m: &Matrix, opts: &RunOptions) -> R<()> {
    let RunOptions {
        execute,
        model,
        out,
        max_usd,
        run_usd,
        timeout_secs: timeout,
        no_isolate,
    } = *opts;
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
    let env = RunEnv {
        repo: &m.root,
        config_dir: config.as_deref(),
        model,
        out,
        run_usd,
        timeout: Duration::from_secs(timeout),
    };
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
        let rec = execute_one(task, &task_dir, cond, rep, &env)?;
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
