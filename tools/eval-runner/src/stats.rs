use super::*;

/// Reads a boolean outcome, falling back to `skill_invoked` for results written before `skill_engaged` existed.
pub(crate) fn hit(r: &Value, key: &str) -> bool {
    r.get(key).unwrap_or(&r["skill_invoked"]) == true
}

pub(crate) fn parse_conds(conds: &[String]) -> R<Vec<(String, String)>> {
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
pub(crate) fn prf(rows: &[(bool, f64)]) -> (f64, f64, f64, f64) {
    let tp: f64 = rows.iter().filter(|r| r.0).map(|r| r.1).sum();
    let pos = rows.iter().filter(|r| r.0).count() as f64;
    let fp: f64 = rows.iter().filter(|r| !r.0).map(|r| r.1).sum();
    let neg = rows.iter().filter(|r| !r.0).count() as f64;
    let precision = if tp + fp > 0.0 { tp / (tp + fp) } else { 0.0 };
    let recall = if pos > 0.0 { tp / pos } else { 0.0 };
    let f1 = if precision + recall > 0.0 {
        F1_WEIGHT * precision * recall / (precision + recall)
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

pub(crate) fn trigger_analyze(out: &Path, base: &str, cand: &str, resamples: usize) -> R<()> {
    for (key, label) in [
        ("skill_invoked", "Skill tool called"),
        ("skill_engaged", "engaged (tool called or skill files read)"),
    ] {
        println!("\n=== {label} ===");
        trigger_metric(out, base, cand, resamples, key)?;
    }
    Ok(())
}

/// A prompt subset's (expected, rate) pairs for one condition, optionally for one
/// category.
pub(crate) trait Subset: Fn(&[&String], Option<&str>, &str) -> Vec<(bool, f64)> {}
impl<F: Fn(&[&String], Option<&str>, &str) -> Vec<(bool, f64)>> Subset for F {}

/// Prints one condition's precision, recall, F1 and false-positive rate, then its
/// invocation rate per prompt category.
#[expect(
    clippy::cast_precision_loss,
    reason = "evaluation rates and statistical estimates are computed in floating point"
)]
pub(crate) fn print_condition_summary(c: &str, subset: &dyn Subset, ids: &[&String]) {
    let all = prf(&subset(ids, None, c));
    println!(
        "{c}: precision={:.3} recall={:.3} F1={:.3} false-positive-rate={:.3}",
        all.0, all.1, all.2, all.3
    );
    for cat in ["explicit", "implicit", "negative"] {
        let v = subset(ids, Some(cat), c);
        let m = v.iter().map(|x| x.1).sum::<f64>() / v.len().max(1) as f64;
        println!("   {cat:<9} n={:<3} invoked rate={m:.3}", v.len());
    }
}

/// Per-condition invocation rates, one value per repetition.
pub(crate) type Rates = BTreeMap<String, Vec<f64>>;
/// A prompt's category, whether the skill is expected to fire, and its rates.
pub(crate) type PromptRates = (String, bool, Rates);

/// Groups result rows by prompt id.
pub(crate) fn group_prompts(rows: &[Value], key: &str) -> BTreeMap<String, PromptRates> {
    let mut prompts: BTreeMap<String, PromptRates> = BTreeMap::new();
    for r in rows {
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
    prompts
}

/// Reads every `trig__*.json` result in `out`, warning about and dropping runs
/// whose setup did not match their condition.
pub(crate) fn load_trigger_rows(out: &Path) -> R<Vec<Value>> {
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
    Ok(rows)
}

#[expect(
    clippy::cast_precision_loss,
    reason = "evaluation rates and statistical estimates are computed in floating point"
)]
pub(crate) fn trigger_metric(
    out: &Path,
    base: &str,
    cand: &str,
    resamples: usize,
    key: &str,
) -> R<()> {
    if resamples == 0 {
        return Err("provide at least one bootstrap resample".into());
    }
    let rows = load_trigger_rows(out)?;
    let prompts = group_prompts(&rows, key);
    let rate =
        |p: &PromptRates, c: &str| p.2.get(c).map(|v| v.iter().sum::<f64>() / v.len() as f64);
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
        print_condition_summary(c, &subset, &ids);
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
pub(crate) fn ln_choose(n: u64, k: u64) -> f64 {
    let lf = |x: u64| (1..=x).map(|i| (i as f64).ln()).sum::<f64>();
    lf(n) - lf(k) - lf(n - k)
}
/// Exact two-sided sign test on discordant pairs (`McNemar` exact).
#[expect(
    clippy::cast_precision_loss,
    reason = "evaluation rates and statistical estimates are computed in floating point"
)]
pub(crate) fn mcnemar_p(b: u64, c: u64) -> f64 {
    let n = b + c;
    if n == 0 {
        return 1.0;
    }
    let m = b.min(c);
    let tail: f64 = (0..=m)
        .map(|i| (ln_choose(n, i) - n as f64 * std::f64::consts::LN_2).exp())
        .sum();
    (TWO_SIDED * tail).min(1.0)
}
pub(crate) fn confidence_bounds(sorted: &[f64]) -> [f64; 2] {
    let last = sorted.len() - 1;
    // Floor of 2.5% and 97.5% of the last index.
    [
        sorted[last / CONFIDENCE_TAIL_DIVISOR],
        sorted[last - last.div_ceil(CONFIDENCE_TAIL_DIVISOR)],
    ]
}

pub(crate) struct Rng(u64);
impl Rng {
    fn index(&mut self, len: usize) -> usize {
        let bound = u64::try_from(len).expect("array length fits in u64");
        usize::try_from(self.next() % bound).expect("remainder is less than array length")
    }

    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << XORSHIFT_A;
        self.0 ^= self.0 >> XORSHIFT_B;
        self.0 ^= self.0 << XORSHIFT_C;
        self.0
    }
}

/// Reads every result file in `out` (all `.json` except the manifest).
pub(crate) fn load_result_rows(out: &Path) -> R<Vec<Value>> {
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
    Ok(rows)
}

/// Writes `rows.csv` for external mixed-effects modelling.
pub(crate) fn write_rows_csv(out: &Path, rows: &[Value]) -> R<()> {
    let mut s = String::from(
        "task,cond,rep,kind,framework,pass,false_verified,skill_invoked,cost_usd,turns,input_tokens,output_tokens\n",
    );
    for r in rows {
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

    Ok(())
}

/// 95% bootstrap interval of the mean of `diffs`, resampled `resamples` times.
#[expect(
    clippy::cast_precision_loss,
    reason = "evaluation rates and statistical estimates are computed in floating point"
)]
pub(crate) fn bootstrap_mean_ci(diffs: &[f64], resamples: usize) -> [f64; 2] {
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
    confidence_bounds(&boots)
}

/// Mean of `v`, zero when empty.
#[expect(
    clippy::cast_precision_loss,
    reason = "evaluation rates and statistical estimates are computed in floating point"
)]
pub(crate) fn mean(v: &[f64]) -> f64 {
    v.iter().sum::<f64>() / v.len().max(1) as f64
}

/// Two conditions compared task by task and run by run.
pub(crate) struct Pairing {
    /// Pairs where only the base condition passed, and where only the candidate did.
    pub(crate) base_only: u64,
    pub(crate) cand_only: u64,
    /// (task, rep) cells that ran under both conditions.
    pub(crate) pairs: u64,
    /// Per-task pass samples for the base and the candidate.
    pub(crate) per_task: BTreeMap<String, (Vec<f64>, Vec<f64>)>,
}

impl Pairing {
    fn of(rows: &[Value], base: &str, cand: &str) -> Self {
        // (task, rep) -> pass per condition
        let mut cell: BTreeMap<(String, u64), (Option<bool>, Option<bool>)> = BTreeMap::new();
        let mut per_task: BTreeMap<String, (Vec<f64>, Vec<f64>)> = BTreeMap::new();
        for r in rows {
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
        Self {
            base_only: b,
            cand_only: c,
            pairs,
            per_task,
        }
    }
}

/// Prints a condition's pass rate, pass^k, false-verified rate and mean cost.
#[expect(
    clippy::cast_precision_loss,
    reason = "evaluation rates and statistical estimates are computed in floating point"
)]
pub(crate) fn print_condition_line(cond: &str, rows: &[Value]) {
    let rate = |key: &str| {
        let v: Vec<f64> = rows
            .iter()
            .filter(|r| r["cond"] == cond)
            .map(|r| f64::from(u8::from(r[key] == true)))
            .collect();
        mean(&v)
    };
    let cost = mean(
        &rows
            .iter()
            .filter(|r| r["cond"] == cond)
            .filter_map(|r| r["cost_usd"].as_f64())
            .collect::<Vec<_>>(),
    );
    let mut by: BTreeMap<&str, bool> = BTreeMap::new();
    for r in rows.iter().filter(|r| r["cond"] == cond) {
        let all_passed = by.entry(r["task"].as_str().unwrap_or("")).or_insert(true);
        *all_passed &= r["pass"] == true;
    }
    let pass_k = by.values().filter(|x| **x).count() as f64 / by.len().max(1) as f64;
    println!(
        "{cond}: pass={:.3} pass^k={pass_k:.3} false_verified={:.3} cost={cost:.3}",
        rate("pass"),
        rate("false_verified"),
    );
}

pub(crate) fn analyze(out: &Path, base: &str, cand: &str, resamples: usize, csv: bool) -> R<()> {
    if resamples == 0 {
        return Err("provide at least one bootstrap resample".into());
    }
    let rows = load_result_rows(out)?;
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
        write_rows_csv(out, &rows)?;
    }
    let Pairing {
        base_only: b,
        cand_only: c,
        pairs,
        per_task,
    } = Pairing::of(&rows, base, cand);
    let diffs: Vec<f64> = per_task
        .values()
        .filter(|(x, y)| !x.is_empty() && !y.is_empty())
        .map(|(x, y)| mean(y) - mean(x))
        .collect();
    if diffs.is_empty() {
        return Err("no task has both conditions".into());
    }
    let [low, high] = bootstrap_mean_ci(&diffs, resamples);
    println!(
        "model: {}  paired (task,rep) cells: {pairs}  tasks: {}",
        models.first().map_or("?", String::as_str),
        diffs.len()
    );
    print_condition_line(base, &rows);
    print_condition_line(cand, &rows);
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
