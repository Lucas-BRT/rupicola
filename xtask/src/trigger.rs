use super::{
    Command, Duration, File, Instant, Matrix, Path, R, RUN_POLL, RunOptions, Stdio, Value,
    analyze_trajectory, fs, install_skill, json, parse_conds, setup_config,
};

/// One `trigger` command: the matrix, the shared flags and the agent setup.
pub(crate) struct TriggerRun<'a> {
    pub(crate) m: &'a Matrix,
    pub(crate) opts: &'a RunOptions<'a>,
    pub(crate) config: Option<&'a Path>,
    pub(crate) max_turns: u32,
    pub(crate) keep: bool,
}

impl TriggerRun<'_> {
    /// Runs one prompt under one condition in a stub project and returns its record.
    fn once(
        &self,
        t: &rupicola_core::evals::Trigger,
        name: &str,
        rev: &str,
        rep: usize,
    ) -> R<Value> {
        let id = format!("trig__{}__{name}__{rep}", t.id);
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
        fs::write(work.join("src/main.rs"), "fn main() {}\n").map_err(|error| error.to_string())?;
        let with_skill = rev != "none";
        if with_skill {
            install_skill(&self.m.root, rev, &work.join(".claude/skills/rupicola"))?;
        }
        let traj = self.opts.out.join(format!("{id}.jsonl"));
        self.run_agent(&t.prompt, &work, &traj)?;
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
        o.insert("model".into(), json!(self.opts.model));
        o.insert("config_error".into(), json!(loaded != with_skill));
        let _ = fs::remove_dir_all(&root);
        if !self.keep {
            let _ = fs::remove_file(&traj);
        }
        Ok(rec)
    }

    /// Runs the agent read-only on `prompt`, streaming its trajectory to `traj`,
    /// and kills it when the time limit passes.
    fn run_agent(&self, prompt: &str, work: &Path, traj: &Path) -> R<()> {
        let mut cmd = Command::new("claude");
        if let Some(dir) = self.config {
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
                self.opts.model,
                "--max-turns",
                &self.max_turns.to_string(),
            ])
            .args([
                "--permission-mode",
                "plan",
                "--allowedTools",
                "Read Glob Grep Skill",
            ])
            .args(["--max-budget-usd", &self.opts.run_usd.to_string()])
            .current_dir(work)
            .stdin(Stdio::null())
            .stdout(File::create(traj).map_err(|error| error.to_string())?)
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
                return Ok(());
            }
            if start.elapsed() > Duration::from_secs(self.opts.timeout_secs) {
                let _ = child.kill();
                let _ = child.wait();
                return Ok(());
            }
            std::thread::sleep(RUN_POLL);
        }
    }
}

#[expect(
    clippy::cast_precision_loss,
    reason = "the dry-run cost is a displayed estimate based on a floating-point unit price"
)]
pub(crate) fn trigger_run(m: &Matrix, opts: &RunOptions, max_turns: u32, keep: bool) -> R<()> {
    let prompts: Vec<_> = rupicola_core::evals::load_triggers(&m.root)
        .map_err(|e| format!("{e:#}"))?
        .into_iter()
        .filter(|t| t.split == m.split && (m.tasks.is_empty() || m.tasks.contains(&t.id)))
        .collect();
    let conds = parse_conds(&m.conds)?;
    let total = prompts.len() * conds.len() * m.k;
    if !opts.execute {
        println!(
            "dry run: {total} runs ({} prompts x {} conditions x k={}); worst-case cost {:.2} USD; pass --execute to start",
            prompts.len(),
            conds.len(),
            m.k,
            total as f64 * opts.run_usd
        );
        return Ok(());
    }
    let (config, _cleanup) = setup_config(opts.no_isolate)?;
    fs::create_dir_all(opts.out).map_err(|error| error.to_string())?;
    fs::write(
        opts.out.join("manifest.json"),
        serde_json::to_string_pretty(
            &json!({"model": opts.model, "conditions": m.conds, "split": m.split, "k": m.k}),
        )
        .unwrap(),
    )
    .map_err(|error| error.to_string())?;
    let run = TriggerRun {
        m,
        opts,
        config: config.as_deref(),
        max_turns,
        keep,
    };
    let mut spent = 0.0;
    for rep in 0..m.k {
        for t in &prompts {
            for (name, rev) in &conds {
                let id = format!("trig__{}__{name}__{rep}", t.id);
                let file = opts.out.join(format!("{id}.json"));
                if let Ok(old) = fs::read_to_string(&file) {
                    spent += serde_json::from_str::<Value>(&old)
                        .ok()
                        .and_then(|v| v["cost_usd"].as_f64())
                        .unwrap_or(0.0);
                    continue;
                }
                if spent >= opts.max_usd {
                    return Err(format!(
                        "stopped: reported cost {spent:.2} reached --max-usd; rerun to resume"
                    ));
                }
                let rec = run.once(t, name, rev, rep)?;
                spent += rec["cost_usd"].as_f64().unwrap_or(0.0);
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
