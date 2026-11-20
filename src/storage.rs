#![allow(dead_code)]
use crate::{fix_record, problem_record, proposal_record};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs, io,
    path::{Path, PathBuf},
    process::Command,
};

pub const CONFIG_FILE: &str = "storage-config.json";

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct BucketConfig {
    /// "s3-compatible" | "rclone" | "filesystem-bucket"
    pub provider: String,
    pub bucket: String,
    pub endpoint_url: String,
    pub region: String,
    pub prefix: String,
    pub public_base_url: String,
    /// "manual" | "on-fix-approved" | "after-capture"
    pub auto_policy: String,
    #[serde(default)]
    pub delete_local_after_offload: bool,
    #[serde(default)]
    pub keep_latest_runs: usize,
}

impl Default for BucketConfig {
    fn default() -> Self {
        Self {
            provider: "s3-compatible".into(),
            bucket: String::new(),
            endpoint_url: "https://s3.amazonaws.com".into(),
            region: "auto".into(),
            prefix: "rupicola".into(),
            public_base_url: String::new(),
            auto_policy: "manual".into(),
            delete_local_after_offload: true,
            keep_latest_runs: 2,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct CategoryUsage {
    pub category: String,
    pub bytes: u64,
    pub files: usize,
    pub pct: u8,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct DateUsage {
    pub date: String,
    pub bytes: u64,
    pub reclaimable_bytes: u64,
    pub files: usize,
    pub runs_count: usize,
    pub fixes_count: usize,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct FixUsage {
    pub fix_id: String,
    pub title: String,
    pub status: String,
    pub date: String,
    pub created_at_ms: u64,
    pub bytes: u64,
    pub files: usize,
    pub referenced_runs: Vec<String>,
    pub offloaded: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct RunUsage {
    pub run_name: String,
    pub run_key: String,
    pub project_id: String,
    pub scenario: String,
    pub date: String,
    pub created_at_ms: u64,
    pub total_bytes: u64,
    pub raw_frames_bytes: u64,
    pub files_count: usize,
    pub pinned_by_fixes: Vec<String>,
    pub pinned_by_problems: usize,
    pub offloaded: bool,
    pub remote_base_url: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ProjectUsage {
    pub project_id: String,
    pub total_bytes: u64,
    pub pinned_bytes: u64,
    pub reclaimable_bytes: u64,
    pub runs_count: usize,
    pub fixes_count: usize,
    pub proposals_bytes: u64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct StorageReport {
    pub total_bytes: u64,
    pub total_files: usize,
    pub pinned_bytes: u64,
    pub reclaimable_bytes: u64,
    pub raw_frames_bytes: u64,
    pub offloaded_runs: usize,
    pub config: BucketConfig,
    pub by_category: Vec<CategoryUsage>,
    pub by_date: Vec<DateUsage>,
    pub by_fix: Vec<FixUsage>,
    pub by_run: Vec<RunUsage>,
    pub by_project: Vec<ProjectUsage>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ActionSummary {
    pub action: String,
    pub freed_bytes: u64,
    pub affected_runs: usize,
    pub affected_files: usize,
    pub note: String,
}

#[must_use]
pub fn load_config(data_dir: &Path) -> BucketConfig {
    let path = data_dir.join(CONFIG_FILE);
    fs::read(&path)
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default()
}

/// # Errors
///
/// Returns an error if the configuration directory or file cannot be created, or serialization or writing fails.
pub fn save_config(data_dir: &Path, cfg: &BucketConfig) -> io::Result<()> {
    fs::create_dir_all(data_dir)?;
    let path = data_dir.join(CONFIG_FILE);
    let tmp = data_dir.join(format!(".storage-cfg-{}.tmp", std::process::id()));
    let bytes = serde_json::to_vec_pretty(cfg)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    fs::write(&tmp, bytes)?;
    fs::rename(&tmp, &path)
}

#[must_use]
#[expect(
    clippy::cast_precision_loss,
    reason = "displayed byte counts are rounded to one or two decimal places"
)]
pub fn format_bytes(bytes: u64) -> String {
    const KIB: f64 = 1024.0;
    const MIB: f64 = 1024.0 * 1024.0;
    const GIB: f64 = 1024.0 * 1024.0 * 1024.0;
    let b = bytes as f64;
    if b >= GIB {
        format!("{:.2} GiB", b / GIB)
    } else if b >= MIB {
        format!("{:.2} MiB", b / MIB)
    } else if b >= KIB {
        format!("{:.1} KiB", b / KIB)
    } else {
        format!("{bytes} B")
    }
}

fn epoch_ms_to_ymd(ms: u64) -> String {
    let secs = i64::try_from(ms / 1000).expect("u64 milliseconds fit in i64 seconds");
    let days = secs.div_euclid(86_400);
    // Civil date from days since 1970-01-01 (Howard Hinnant algorithm)
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = if m <= 2 { y + 1 } else { y };
    format!("{year:04}-{m:02}-{d:02}")
}

fn dir_walk_stats(dir: &Path) -> (u64, usize, u64) {
    let mut total_bytes = 0u64;
    let mut files = 0usize;
    let mut raw_frame_bytes = 0u64;
    let mut stack = vec![dir.to_path_buf()];
    while let Some(curr) = stack.pop() {
        let Ok(read) = fs::read_dir(&curr) else {
            continue;
        };
        for entry in read.flatten() {
            let Ok(ft) = entry.file_type() else {
                continue;
            };
            if ft.is_symlink() {
                continue;
            }
            let path = entry.path();
            if ft.is_dir() {
                stack.push(path);
            } else if ft.is_file() {
                let len = entry.metadata().map_or(0, |m| m.len());
                total_bytes += len;
                files += 1;
                let fname = entry.file_name();
                let s = fname.to_string_lossy();
                if s.starts_with("frame-") && s.ends_with(".png") {
                    raw_frame_bytes += len;
                }
            }
        }
    }
    (total_bytes, files, raw_frame_bytes)
}

fn extract_run_and_rel(media_url: &str) -> Option<(String, String)> {
    // Format: /media/0/<run_name>/<rel_path>
    let trimmed = media_url.strip_prefix("/media/")?;
    let (_root_idx, rest) = trimmed.split_once('/')?;
    let (run_name, rel) = rest.split_once('/')?;
    Some((run_name.to_string(), rel.to_string()))
}

#[expect(
    clippy::too_many_lines,
    reason = "the report aggregates capture sizes, references, and retention totals together"
)]
pub fn analyze(data_dir: &Path, project_filter: Option<&str>) -> StorageReport {
    let config = load_config(data_dir);
    let captures_dir = data_dir.join("captures");

    // 1. Collect all fixes and problems across projects
    let mut all_fixes = Vec::new();
    let mut all_problems = Vec::new();

    if let Ok(read) = fs::read_dir(data_dir.join(fix_record::DIRECTORY)) {
        for entry in read.flatten() {
            if entry.file_type().is_ok_and(|t| t.is_dir()) {
                let pid = entry.file_name().to_string_lossy().to_string();
                if project_filter.is_some_and(|f| f != pid) {
                    continue;
                }
                if let Ok(list) = fix_record::entries(data_dir, &pid) {
                    all_fixes.extend(list);
                }
            }
        }
    }

    if let Ok(read) = fs::read_dir(data_dir.join(problem_record::DIRECTORY)) {
        for entry in read.flatten() {
            if entry.file_type().is_ok_and(|t| t.is_dir()) {
                let pid = entry.file_name().to_string_lossy().to_string();
                if project_filter.is_some_and(|f| f != pid) {
                    continue;
                }
                if let Ok(list) = problem_record::entries(data_dir, &pid) {
                    all_problems.extend(list);
                }
            }
        }
    }

    // Map run_name -> list of fix titles/ids that depend on it
    let mut run_to_fixes: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut run_to_problems: BTreeMap<String, usize> = BTreeMap::new();

    let mut by_fix = Vec::new();
    let mut fix_referenced_bytes = 0u64;

    for fix in &all_fixes {
        let mut referenced_runs = BTreeSet::new();
        let mut fix_bytes = 0u64;
        let mut fix_files = 0usize;
        let mut offloaded = false;

        let mut urls = Vec::new();
        for cmp in &fix.comparisons {
            urls.push(cmp.before_url.as_str());
            urls.push(cmp.after_url.as_str());
            if let Some(d) = &cmp.diff_url {
                urls.push(d.as_str());
            }
        }
        for st in &fix.steps {
            if let Some(ev) = &st.evidence_url {
                urls.push(ev.as_str());
            }
        }

        let mut seen_urls = BTreeSet::new();
        for u in urls {
            if !seen_urls.insert(u) {
                continue;
            }
            if u.starts_with("http://") || u.starts_with("https://") {
                offloaded = true;
            }
            if let Some((run_name, rel)) = extract_run_and_rel(u) {
                referenced_runs.insert(run_name.clone());
                let file_path = captures_dir.join(&run_name).join(&rel);
                if let Ok(meta) = fs::metadata(&file_path) {
                    fix_bytes += meta.len();
                    fix_files += 1;
                }
            }
        }

        for r in &referenced_runs {
            run_to_fixes
                .entry(r.clone())
                .or_default()
                .push(fix.id.clone());
        }

        fix_referenced_bytes += fix_bytes;
        by_fix.push(FixUsage {
            fix_id: fix.id.clone(),
            title: fix.title.clone(),
            status: fix.status.label().to_string(),
            date: epoch_ms_to_ymd(fix.created_at_ms),
            created_at_ms: fix.created_at_ms,
            bytes: fix_bytes,
            files: fix_files,
            referenced_runs: referenced_runs.into_iter().collect(),
            offloaded,
        });
    }

    for prob in &all_problems {
        for ev in &prob.evidence_assets {
            if let Some((run_name, _)) = extract_run_and_rel(&ev.url) {
                *run_to_problems.entry(run_name).or_default() += 1;
            } else if let Some((_, rname)) = ev.run_key.split_once('/') {
                *run_to_problems.entry(rname.to_string()).or_default() += 1;
            }
        }
    }

    // 2. Scan capture runs
    let mut by_run = Vec::new();
    let mut total_captures_bytes = 0u64;
    let mut total_captures_files = 0usize;
    let mut total_raw_frames_bytes = 0u64;
    let mut pinned_bytes = 0u64;
    let mut reclaimable_bytes = 0u64;
    let mut offloaded_runs = 0usize;

    let mut project_map: BTreeMap<String, ProjectUsage> = BTreeMap::new();
    let mut date_map: BTreeMap<String, DateUsage> = BTreeMap::new();

    if let Ok(read) = fs::read_dir(&captures_dir) {
        for entry in read.flatten() {
            let Ok(ft) = entry.file_type() else {
                continue;
            };
            if ft.is_symlink() || !ft.is_dir() {
                continue;
            }
            let run_name = entry.file_name().to_string_lossy().to_string();
            let run_dir = entry.path();
            let manifest_path = run_dir.join("manifest.json");
            let Ok(m_bytes) = fs::read(&manifest_path) else {
                continue;
            };
            let Ok(val) = serde_json::from_slice::<serde_json::Value>(&m_bytes) else {
                continue;
            };
            let pid = val["project"]["id"]
                .as_str()
                .unwrap_or("unknown")
                .to_string();
            if project_filter.is_some_and(|f| f != pid) {
                continue;
            }
            let scenario = val["scenario"].as_str().unwrap_or(&run_name).to_string();
            let created_at_ms = val["created_at_ms"].as_u64().unwrap_or(0);
            let offloaded = val["offloaded"].as_bool().unwrap_or(false);
            let remote_base_url = val["remote_base_url"].as_str().map(str::to_owned);
            if offloaded {
                offloaded_runs += 1;
            }

            let (bytes, files, raw_frames) = dir_walk_stats(&run_dir);
            total_captures_bytes += bytes;
            total_captures_files += files;
            total_raw_frames_bytes += raw_frames;

            let fixes_pin = run_to_fixes.get(&run_name).cloned().unwrap_or_default();
            let probs_pin = run_to_problems.get(&run_name).copied().unwrap_or(0);
            let is_pinned = !fixes_pin.is_empty() || probs_pin > 0;
            if is_pinned {
                pinned_bytes += bytes;
            } else {
                reclaimable_bytes += bytes;
            }

            let date = if created_at_ms > 0 {
                epoch_ms_to_ymd(created_at_ms)
            } else {
                "unknown".into()
            };

            let d_entry = date_map.entry(date.clone()).or_insert_with(|| DateUsage {
                date: date.clone(),
                bytes: 0,
                reclaimable_bytes: 0,
                files: 0,
                runs_count: 0,
                fixes_count: 0,
            });
            d_entry.bytes += bytes;
            if !is_pinned {
                d_entry.reclaimable_bytes += bytes;
            }
            d_entry.files += files;
            d_entry.runs_count += 1;

            let p_entry = project_map
                .entry(pid.clone())
                .or_insert_with(|| ProjectUsage {
                    project_id: pid.clone(),
                    total_bytes: 0,
                    pinned_bytes: 0,
                    reclaimable_bytes: 0,
                    runs_count: 0,
                    fixes_count: 0,
                    proposals_bytes: 0,
                });
            p_entry.total_bytes += bytes;
            if is_pinned {
                p_entry.pinned_bytes += bytes;
            } else {
                p_entry.reclaimable_bytes += bytes;
            }
            p_entry.runs_count += 1;

            by_run.push(RunUsage {
                run_name: run_name.clone(),
                run_key: format!("0/{run_name}"),
                project_id: pid,
                scenario,
                date,
                created_at_ms,
                total_bytes: bytes,
                raw_frames_bytes: raw_frames,
                files_count: files,
                pinned_by_fixes: fixes_pin,
                pinned_by_problems: probs_pin,
                offloaded,
                remote_base_url,
            });
        }
    }

    for f in &by_fix {
        if let Some(d) = date_map.get_mut(&f.date) {
            d.fixes_count += 1;
        }
    }
    for fix in &all_fixes {
        if let Some(p) = project_map.get_mut(&fix.project_id) {
            p.fixes_count += 1;
        }
    }

    // 3. Proposals & Metadata usage
    let proposals_root = match project_filter {
        Some(pid) => data_dir.join(proposal_record::DIRECTORY).join(pid),
        None => data_dir.join(proposal_record::DIRECTORY),
    };
    let (proposals_bytes, proposals_files, _) = dir_walk_stats(&proposals_root);
    let (problems_bytes, problems_files, _) =
        dir_walk_stats(&data_dir.join(problem_record::DIRECTORY));
    let (fixes_meta_bytes, fixes_meta_files, _) =
        dir_walk_stats(&data_dir.join(fix_record::DIRECTORY));
    let (design_bytes, design_files, _) = dir_walk_stats(&data_dir.join("design"));

    if let Some(pid) = project_filter
        && let Some(p) = project_map.get_mut(pid)
    {
        p.proposals_bytes = proposals_bytes;
        p.total_bytes += proposals_bytes;
    }

    let total_bytes =
        total_captures_bytes + proposals_bytes + problems_bytes + fixes_meta_bytes + design_bytes;
    let total_files =
        total_captures_files + proposals_files + problems_files + fixes_meta_files + design_files;

    let pct_of = |b: u64| -> u8 {
        b.saturating_mul(100)
            .checked_div(total_bytes)
            .unwrap_or(0)
            .min(100) as u8
    };

    let screenshots_bytes = total_captures_bytes.saturating_sub(total_raw_frames_bytes);
    let meta_bytes = problems_bytes + fixes_meta_bytes + design_bytes;

    let by_category = vec![
        CategoryUsage {
            category: "Checkpoints, Contact Sheets & Diffs (PNG)".into(),
            bytes: screenshots_bytes,
            files: total_captures_files,
            pct: pct_of(screenshots_bytes),
        },
        CategoryUsage {
            category: "Raw Animation Keyframes (samples/*/frame-*.png)".into(),
            bytes: total_raw_frames_bytes,
            files: 0,
            pct: pct_of(total_raw_frames_bytes),
        },
        CategoryUsage {
            category: "Pinned Fix Comparisons (Before / After / Diff)".into(),
            bytes: fix_referenced_bytes,
            files: by_fix.iter().map(|f| f.files).sum(),
            pct: pct_of(fix_referenced_bytes),
        },
        CategoryUsage {
            category: "Proposes & Sandboxed HTML/Image Previews".into(),
            bytes: proposals_bytes,
            files: proposals_files,
            pct: pct_of(proposals_bytes),
        },
        CategoryUsage {
            category: "Pipeline Ledger JSON (Problems, Fixes, Design)".into(),
            bytes: meta_bytes,
            files: problems_files + fixes_meta_files + design_files,
            pct: pct_of(meta_bytes),
        },
    ];

    by_run.sort_by_key(|r| std::cmp::Reverse(r.created_at_ms));
    let mut by_date: Vec<DateUsage> = date_map.into_values().collect();
    by_date.sort_by(|a, b| b.date.cmp(&a.date));
    let mut by_project: Vec<ProjectUsage> = project_map.into_values().collect();
    by_project.sort_by_key(|p| std::cmp::Reverse(p.total_bytes));

    StorageReport {
        total_bytes,
        total_files,
        pinned_bytes,
        reclaimable_bytes,
        raw_frames_bytes: total_raw_frames_bytes,
        offloaded_runs,
        config,
        by_category,
        by_date,
        by_fix,
        by_run,
        by_project,
    }
}

/// Strip intermediate `samples/*/frame-*.png` files while keeping `sheet.png`, `diff.png`, `clip.mp4`, and `analysis.json`.
///
/// # Errors
///
/// No errors are currently returned. Files that cannot be removed are skipped.
pub fn strip_raw_frames(
    data_dir: &Path,
    project_filter: Option<&str>,
) -> io::Result<ActionSummary> {
    let report = analyze(data_dir, project_filter);
    let captures_dir = data_dir.join("captures");
    let mut freed = 0u64;
    let mut files = 0usize;
    let mut runs = 0usize;

    for run in &report.by_run {
        let samples_dir = captures_dir.join(&run.run_name).join("samples");
        let Ok(clips) = fs::read_dir(&samples_dir) else {
            continue;
        };
        let mut touched_run = false;
        for clip in clips.flatten() {
            let clip_dir = clip.path();
            if !clip_dir.is_dir() {
                continue;
            }
            if !clip_dir.join("sheet.png").exists() && !clip_dir.join("clip.mp4").exists() {
                continue;
            }
            if let Ok(entries) = fs::read_dir(&clip_dir) {
                for f in entries.flatten() {
                    let name = f.file_name();
                    let s = name.to_string_lossy();
                    if s.starts_with("frame-") && s.ends_with(".png") {
                        let len = f.metadata().map_or(0, |m| m.len());
                        if fs::remove_file(f.path()).is_ok() {
                            freed += len;
                            files += 1;
                            touched_run = true;
                        }
                    }
                }
            }
        }
        if touched_run {
            runs += 1;
        }
    }

    Ok(ActionSummary {
        action: "strip-raw-frames".into(),
        freed_bytes: freed,
        affected_runs: runs,
        affected_files: files,
        note: format!(
            "Stripped {} raw animation frames across {} runs, freeing {} (MP4 & contact sheets preserved)",
            files,
            runs,
            format_bytes(freed)
        ),
    })
}

/// Delete unpinned runs (respecting `keep_latest` most recent runs and optional date filter).
///
/// # Errors
///
/// No errors are currently returned. Runs that cannot be removed are skipped.
pub fn prune_unpinned(
    data_dir: &Path,
    project_filter: Option<&str>,
    keep_latest: usize,
    only_date: Option<&str>,
) -> io::Result<ActionSummary> {
    let report = analyze(data_dir, project_filter);
    let captures_dir = data_dir.join("captures");
    let mut freed = 0u64;
    let mut files = 0usize;
    let mut runs = 0usize;

    for (idx, run) in report.by_run.iter().enumerate() {
        if idx < keep_latest && only_date.is_none() {
            continue;
        }
        if let Some(target_date) = only_date
            && run.date != target_date
        {
            continue;
        }
        if !run.pinned_by_fixes.is_empty() || run.pinned_by_problems > 0 {
            continue;
        }
        let dir = captures_dir.join(&run.run_name);
        if fs::remove_dir_all(&dir).is_ok() {
            freed += run.total_bytes;
            files += run.files_count;
            runs += 1;
        }
    }

    Ok(ActionSummary {
        action: "prune-unpinned".into(),
        freed_bytes: freed,
        affected_runs: runs,
        affected_files: files,
        note: format!(
            "Pruned {} unpinned capture runs ({} files), freeing {}",
            runs,
            files,
            format_bytes(freed)
        ),
    })
}

/// Delete a single run directory (if not pinned, or if `force` is true).
///
/// # Errors
///
/// Returns an error if the run is missing, pinned without force, or cannot be removed.
pub fn delete_run(data_dir: &Path, run_name: &str, force: bool) -> io::Result<ActionSummary> {
    let report = analyze(data_dir, None);
    let Some(run) = report.by_run.iter().find(|r| r.run_name == run_name) else {
        return Err(io::Error::new(io::ErrorKind::NotFound, "run not found"));
    };
    if !force && (!run.pinned_by_fixes.is_empty() || run.pinned_by_problems > 0) {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "run is pinned by active Fixes or Problems",
        ));
    }
    let dir = data_dir.join("captures").join(run_name);
    fs::remove_dir_all(&dir)?;
    Ok(ActionSummary {
        action: "delete-run".into(),
        freed_bytes: run.total_bytes,
        affected_runs: 1,
        affected_files: run.files_count,
        note: format!("Deleted run {run_name} ({})", format_bytes(run.total_bytes)),
    })
}

/// Offload a capture run (or all runs referenced by a Fix) to the configured external bucket and optionally delete local media files.
///
/// # Errors
///
/// Returns an error if the bucket is unconfigured, the manifest cannot be read or written, or the transfer command fails.
#[expect(
    clippy::too_many_lines,
    reason = "the workflow keeps transfer, manifest update, and optional local cleanup in execution order"
)]
pub fn offload_run(data_dir: &Path, run_name: &str) -> Result<ActionSummary, String> {
    let cfg = load_config(data_dir);
    if cfg.bucket.trim().is_empty() {
        return Err(
            "External bucket is not configured yet. Configure Bucket name & endpoint first.".into(),
        );
    }
    let run_dir = data_dir.join("captures").join(run_name);
    let manifest_path = run_dir.join("manifest.json");
    let m_bytes = fs::read(&manifest_path).map_err(|e| e.to_string())?;
    let mut manifest: serde_json::Value =
        serde_json::from_slice(&m_bytes).map_err(|e| e.to_string())?;
    let project_id = manifest["project"]["id"]
        .as_str()
        .unwrap_or("default")
        .to_string();

    let clean_prefix = cfg.prefix.trim_matches('/');
    let remote_rel = if clean_prefix.is_empty() {
        format!("{project_id}/{run_name}")
    } else {
        format!("{clean_prefix}/{project_id}/{run_name}")
    };

    // Execute upload according to provider
    match cfg.provider.as_str() {
        "filesystem-bucket" => {
            let dest = PathBuf::from(&cfg.bucket).join(&remote_rel);
            fs::create_dir_all(&dest).map_err(|e| e.to_string())?;
            copy_dir_recursive(&run_dir, &dest).map_err(|e| e.to_string())?;
        }
        "rclone" => {
            let target = format!(
                "{}:{clean_prefix}/{project_id}/{run_name}",
                cfg.bucket.trim_end_matches(':')
            );
            let status = Command::new("rclone")
                .args(["copy", &run_dir.to_string_lossy(), &target])
                .status()
                .map_err(|e| format!("rclone execution failed: {e}"))?;
            if !status.success() {
                return Err("rclone copy returned non-zero status".into());
            }
        }
        _ => {
            // s3-compatible via aws s3 sync
            let s3_uri = format!("s3://{}/{}", cfg.bucket.trim_matches('/'), remote_rel);
            let mut cmd = Command::new("aws");
            if !cfg.endpoint_url.trim().is_empty() && cfg.endpoint_url != "https://s3.amazonaws.com"
            {
                cmd.args(["--endpoint-url", cfg.endpoint_url.trim()]);
            }
            if !cfg.region.trim().is_empty() {
                cmd.args(["--region", cfg.region.trim()]);
            }
            let status = cmd
                .args(["s3", "sync", &run_dir.to_string_lossy(), &s3_uri])
                .status()
                .map_err(|e| format!("aws s3 sync failed ({e}); ensure aws-cli is installed or use filesystem-bucket / rclone"))?;
            if !status.success() {
                return Err("aws s3 sync returned non-zero status".into());
            }
        }
    }

    let public_base = if cfg.public_base_url.trim().is_empty() {
        format!("s3://{}/{}", cfg.bucket.trim_matches('/'), remote_rel)
    } else {
        format!(
            "{}/{}",
            cfg.public_base_url.trim_end_matches('/'),
            remote_rel
        )
    };

    let mut freed = 0u64;
    let mut files = 0usize;

    if cfg.delete_local_after_offload {
        for sub in ["checkpoints", "samples"] {
            let sub_dir = run_dir.join(sub);
            if sub_dir.exists() {
                let (b, f, _) = dir_walk_stats(&sub_dir);
                if fs::remove_dir_all(&sub_dir).is_ok() {
                    freed += b;
                    files += f;
                }
            }
        }
    }

    manifest["offloaded"] = serde_json::json!(true);
    manifest["remote_base_url"] = serde_json::json!(public_base);
    if let Ok(updated) = serde_json::to_vec_pretty(&manifest) {
        let _ = fs::write(&manifest_path, updated);
    }

    // Rewrite local /media/0/<run_name>/... URLs in Fixes & Problems if public_base_url is HTTP(S)
    if public_base.starts_with("http://") || public_base.starts_with("https://") {
        let needle = format!("/media/0/{run_name}/");
        let replacement = format!("{public_base}/");
        if let Ok(mut fixes) = fix_record::entries(data_dir, &project_id) {
            for fix in &mut fixes {
                let mut changed = false;
                for cmp in &mut fix.comparisons {
                    if cmp.before_url.starts_with(&needle) {
                        cmp.before_url = cmp.before_url.replacen(&needle, &replacement, 1);
                        changed = true;
                    }
                    if cmp.after_url.starts_with(&needle) {
                        cmp.after_url = cmp.after_url.replacen(&needle, &replacement, 1);
                        changed = true;
                    }
                    if let Some(d) = &mut cmp.diff_url
                        && d.starts_with(&needle)
                    {
                        *d = d.replacen(&needle, &replacement, 1);
                        changed = true;
                    }
                }
                if changed {
                    let _ = fix_record::save(data_dir, fix);
                }
            }
        }
    }

    Ok(ActionSummary {
        action: "offload-run".into(),
        freed_bytes: freed,
        affected_runs: 1,
        affected_files: files,
        note: format!(
            "Offloaded run {run_name} to {public_base} and freed {} locally",
            format_bytes(freed)
        ),
    })
}

fn copy_dir_recursive(src: &Path, dst: &Path) -> io::Result<()> {
    fs::create_dir_all(dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let ft = entry.file_type()?;
        if ft.is_symlink() {
            continue;
        }
        let target = dst.join(entry.file_name());
        if ft.is_dir() {
            copy_dir_recursive(&entry.path(), &target)?;
        } else if ft.is_file() {
            fs::copy(entry.path(), target)?;
        }
    }
    Ok(())
}
