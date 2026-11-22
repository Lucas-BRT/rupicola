//! Evaluation runner and analyzer. `plan` and `analyze` are offline; `run` spawns an agent
//! only with `--execute`. Never part of the distributed `rupicola` binary.
mod harness;
mod run;
mod stats;
mod trigger;
use clap::{Args, Parser, Subcommand};
use harness::{
    R, analyze_trajectory, copy_dir, install_skill, matrix, order, run_id, setup_config, sh,
};
use run::{RunOptions, run};
use rupicola_core::evals::{Task, load};
use serde_json::{Value, json};
use stats::{analyze, parse_conds, trigger_analyze};
use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::fs::{self, File};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};
use trigger::trigger_run;

/// Evaluation run budgets: dollars and seconds per full run and per trigger run,
/// turns a trigger run may take, and the default bootstrap resample count.
const DEFAULT_RUN_USD: f64 = 2.0;
const DEFAULT_RUN_TIMEOUT_SECS: u64 = 1800;
const DEFAULT_TRIGGER_RUN_USD: f64 = 0.3;
const DEFAULT_TRIGGER_TIMEOUT_SECS: u64 = 300;
const DEFAULT_TRIGGER_TURNS: u32 = 2;
const DEFAULT_RESAMPLES: usize = 10_000;
/// Tasks per condition drawn by default when planning.
const DEFAULT_K: usize = 4;
/// Exit code `sh` reports for a command that could not start.
const EXIT_SPAWN_FAILED: i32 = 127;
/// Polling intervals while waiting on child processes.
const SH_POLL: Duration = Duration::from_millis(200);
const AGENT_POLL: Duration = Duration::from_millis(500);
const RUN_POLL: Duration = Duration::from_millis(300);
/// Time limits: copying a repository, a git command, and a task verifier.
const COPY_TIMEOUT: Duration = Duration::from_secs(60);
const GIT_TIMEOUT: Duration = Duration::from_secs(30);
const VERIFIER_TIMEOUT: Duration = Duration::from_secs(600);
/// Characters of command output kept in a result.
const OUTPUT_TAIL_CHARS: usize = 2000;
/// F1 is the harmonic mean: 2PR / (P + R).
const F1_WEIGHT: f64 = 2.0;
/// A sign test is two-sided, so its one-sided tail doubles.
const TWO_SIDED: f64 = 2.0;
/// The 95% bootstrap interval drops 1/40 of the sorted resamples at each end.
const CONFIDENCE_TAIL_DIVISOR: usize = 40;
/// Xorshift64 shift triple (13, 7, 17), a full-period choice.
const XORSHIFT_A: u32 = 13;
const XORSHIFT_B: u32 = 7;
const XORSHIFT_C: u32 = 17;

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
        #[arg(long, default_value_t = DEFAULT_RUN_USD)]
        run_usd: f64,
        #[arg(long, default_value_t = DEFAULT_RUN_TIMEOUT_SECS)]
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
        #[arg(long, default_value_t = DEFAULT_TRIGGER_RUN_USD)]
        run_usd: f64,
        #[arg(long, default_value_t = DEFAULT_TRIGGER_TIMEOUT_SECS)]
        timeout_secs: u64,
        #[arg(long)]
        no_isolate: bool,
        /// Turns the agent may take before the run is cut; the Skill call must happen within them.
        #[arg(long, default_value_t = DEFAULT_TRIGGER_TURNS)]
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
        #[arg(long, default_value_t = DEFAULT_RESAMPLES)]
        resamples: usize,
    },
    /// Paired comparison of two conditions from a results directory.
    Analyze {
        out: PathBuf,
        #[arg(long)]
        base: String,
        #[arg(long)]
        cand: String,
        #[arg(long, default_value_t = DEFAULT_RESAMPLES)]
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
    #[arg(long, default_value_t = DEFAULT_K)]
    k: usize,
    /// Condition as name=none or name=<git rev of this repo>. Repeatable.
    #[arg(long = "cond", required = true)]
    conds: Vec<String>,
    /// Restrict to task ids (repeatable).
    #[arg(long = "task")]
    tasks: Vec<String>,
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
            &RunOptions {
                execute,
                model: &model,
                out: &out,
                max_usd,
                run_usd,
                timeout_secs,
                no_isolate,
            },
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
            &RunOptions {
                execute,
                model: &model,
                out: &out,
                max_usd,
                run_usd,
                timeout_secs,
                no_isolate,
            },
            max_turns,
            keep,
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
