use super::{
    Account, BadgeVariant, Cx, Deserialize, PageResult, Panel, Response, Role, StatusCode, auth,
    badge, data_root, db_error, pages, redirect, storage, view,
};

#[expect(
    clippy::too_many_lines,
    reason = "the storage view keeps its markup and presentation branches together"
)]
pub fn render_storage<'a>(
    cx: &'a Cx,
    account: &'a Account,
    role: Role,
    _project_id: &'a str,
    base: &'a str,
    filter: &'a pages::Filter,
    report: &'a storage::StorageReport,
) -> topcoat::view::Child<'a> {
    let subview = filter.view.as_deref().unwrap_or("all");
    let notice = filter.q.as_deref().unwrap_or("");
    let total_reclaimable = report.reclaimable_bytes + report.raw_frames_bytes;
    let bucket_configured = !report.config.bucket.trim().is_empty();

    topcoat::view::Child::new(view! {
        cx =>
        if !notice.is_empty() {
            <p class="notice">(notice)</p>
        }

        <section class="studio-counts" aria-label="Storage telemetry">
            <div class="metric">
                <small>"Local disk used (" (report.total_files) " files)"</small>
                <strong>(storage::format_bytes(report.total_bytes))</strong>
            </div>
            <div class="metric">
                <small>"Pinned by Fixes & Problems"</small>
                <strong>(storage::format_bytes(report.pinned_bytes))</strong>
            </div>
            <div class=(if total_reclaimable > 0 { "metric featured" } else { "metric" })>
                <small>"Reclaimable / Offloadable (" (report.offloaded_runs) " offloaded)"</small>
                <strong>(storage::format_bytes(total_reclaimable))</strong>
            </div>
        </section>

        if role == Role::Owner {
            <details class="surface panel-padding design-editor">
                <summary>
                    (if bucket_configured {
                        format!("External bucket configured ({} · {}) — Retention & Offload policy", report.config.provider, report.config.bucket)
                    } else {
                        "Configure external bucket (S3 / Cloudflare R2 / MinIO / rclone / NAS) & auto-offload policy".to_string()
                    })
                </summary>
                <form class="stack-form" method="post" action=(format!("{base}/storage/config"))>
                    <input type="hidden" name="csrf" value=(&account.csrf)>
                    <div class="form-row-3">
                        <div>
                            <label for="st-provider">"Provider"</label>
                            <select id="st-provider" name="provider">
                                for (val, label) in [
                                    ("s3-compatible", "S3-Compatible (AWS S3 / R2 / MinIO / B2)"),
                                    ("rclone", "rclone Remote (GDrive / GCS / S3 / SFTP)"),
                                    ("filesystem-bucket", "Mounted Bucket / External Volume Path"),
                                ] {
                                    <option value=(val) if report.config.provider == val { selected="" }>(label)</option>
                                }
                            </select>
                        </div>
                        <div>
                            <label for="st-bucket">"Bucket name or volume path"</label>
                            <input id="st-bucket" name="bucket" required="" maxlength="240"
                                value=(&report.config.bucket) placeholder="e.g. rupicola-evidence-prod or /mnt/archive-bucket">
                        </div>
                        <div>
                            <label for="st-prefix">"Key prefix"</label>
                            <input id="st-prefix" name="prefix" maxlength="120" value=(&report.config.prefix)>
                        </div>
                    </div>
                    <div class="form-row-3">
                        <div>
                            <label for="st-endpoint">"Endpoint URL (S3 / R2 / MinIO)"</label>
                            <input id="st-endpoint" name="endpoint_url" maxlength="240"
                                value=(&report.config.endpoint_url) placeholder="https://<account>.r2.cloudflarestorage.com">
                        </div>
                        <div>
                            <label for="st-region">"Region"</label>
                            <input id="st-region" name="region" maxlength="60" value=(&report.config.region)>
                        </div>
                        <div>
                            <label for="st-public">"Public / CDN Base URL (for Before/After links)"</label>
                            <input id="st-public" name="public_base_url" maxlength="240"
                                value=(&report.config.public_base_url) placeholder="https://cdn.example.com/rupicola">
                        </div>
                    </div>
                    <div class="form-row-3">
                        <div>
                            <label for="st-policy">"Automatic offload policy"</label>
                            <select id="st-policy" name="auto_policy">
                                for (val, label) in [
                                    ("manual", "Manual offload only"),
                                    ("on-fix-approved", "Auto-offload when a Fix is Approved"),
                                    ("after-capture", "Always offload runs after capture"),
                                ] {
                                    <option value=(val) if report.config.auto_policy == val { selected="" }>(label)</option>
                                }
                            </select>
                        </div>
                        <div>
                            <label for="st-keep">"Keep latest N unpinned runs on disk"</label>
                            <input id="st-keep" name="keep_latest_runs" type="number" min="0" max="50"
                                value=(report.config.keep_latest_runs.to_string())>
                        </div>
                        <div>
                            <label for="st-del">"After bucket offload"</label>
                            <select id="st-del" name="delete_local">
                                <option value="true" if report.config.delete_local_after_offload { selected="" }>"Delete local PNG/MP4 files (keep manifest stub)"</option>
                                <option value="false" if !report.config.delete_local_after_offload { selected="" }>"Keep local copy + remote backup"</option>
                            </select>
                        </div>
                    </div>
                    <button class="button primary" type="submit">"Save storage & bucket policy"</button>
                </form>
            </details>
        }

        <div class="propose-toolbar">
            <div class="stage-viewports" role="group" aria-label="Storage grouping">
                for (slug, label) in [
                    ("all", "All groups"),
                    ("fix", "By Fix"),
                    ("date", "By Date"),
                    ("run", "By Capture Run"),
                    ("category", "By Category"),
                ] {
                    <a class=(if subview == slug { "button primary" } else { "button" })
                       href=(format!("{base}/storage?view={slug}"))>
                        (label)
                    </a>
                }
            </div>
            if role == Role::Owner {
                <div class="review-actions">
                    <form method="post" action=(format!("{base}/storage/clean")) class="inline-action">
                        <input type="hidden" name="csrf" value=(&account.csrf)>
                        <input type="hidden" name="mode" value="strip-frames">
                        <button class="button" type="submit">
                            "Strip raw clip frames (" (storage::format_bytes(report.raw_frames_bytes)) ")"
                        </button>
                    </form>
                    <form method="post" action=(format!("{base}/storage/clean")) class="inline-action">
                        <input type="hidden" name="csrf" value=(&account.csrf)>
                        <input type="hidden" name="mode" value="prune-unpinned">
                        <input type="hidden" name="keep_latest" value=(report.config.keep_latest_runs.to_string())>
                        <button class="button" type="submit">
                            "Prune unpinned runs (" (storage::format_bytes(report.reclaimable_bytes)) ")"
                        </button>
                    </form>
                    if bucket_configured {
                        <form method="post" action=(format!("{base}/storage/offload")) class="inline-action">
                            <input type="hidden" name="csrf" value=(&account.csrf)>
                            <input type="hidden" name="target" value="all-unpinned">
                            <button class="button primary" type="submit">"Offload unpinned to bucket"</button>
                        </form>
                    }
                </div>
            }
        </div>

        if subview == "all" || subview == "fix" {
            <section class="surface captures-ledger">
                <div class="section-head">
                    <h2>"Storage grouped by Fix (" (report.by_fix.len()) ")"</h2>
                    <p class="muted">"Pinned Before / After / Diff assets & referenced runs"</p>
                </div>
                if report.by_fix.is_empty() {
                    <div class="empty"><p>"No fixes recorded for this project yet."</p></div>
                } else {
                    <ul class="run-list">
                        for item in &report.by_fix {
                            <li>
                                <div class="run-row">
                                    <a class="run-link" href=(format!("{base}/fixes#fix-{}", item.fix_id))>
                                        <span class="run-title">
                                            <strong>(&item.title)</strong>
                                            <small>
                                                (&item.date) " · Runs: "
                                                (if item.referenced_runs.is_empty() { "none".to_string() } else { item.referenced_runs.join(", ") })
                                            </small>
                                        </span>
                                        <span class="run-state">
                                            badge(
                                                variant: if item.offloaded { BadgeVariant::Primary } else { BadgeVariant::Secondary },
                                                (if item.offloaded { "Offloaded to bucket" } else { &item.status })
                                            )
                                            <code>(storage::format_bytes(item.bytes)) " · " (item.files) " pinned assets"</code>
                                        </span>
                                        <span class="arrow" aria-hidden="true">"↗"</span>
                                    </a>
                                    if role == Role::Owner && bucket_configured && !item.offloaded {
                                        <form method="post" action=(format!("{base}/storage/offload")) class="inline-action">
                                            <input type="hidden" name="csrf" value=(&account.csrf)>
                                            <input type="hidden" name="fix_id" value=(&item.fix_id)>
                                            <button class="button" type="submit">"Offload Fix to bucket"</button>
                                        </form>
                                    }
                                </div>
                            </li>
                        }
                    </ul>
                }
            </section>
        }

        if subview == "all" || subview == "date" {
            <section class="surface captures-ledger">
                <div class="section-head">
                    <h2>"Storage grouped by Date (" (report.by_date.len()) " days)"</h2>
                    <p class="muted">"Daily capture volume, fix count, and reclaimable space"</p>
                </div>
                if report.by_date.is_empty() {
                    <div class="empty"><p>"No dated captures recorded yet."</p></div>
                } else {
                    <ul class="run-list">
                        for day in &report.by_date {
                            <li>
                                <div class="run-row">
                                    <div class="run-link">
                                        <span class="run-title">
                                            <strong>(&day.date)</strong>
                                            <small>(day.runs_count) " capture runs · " (day.fixes_count) " fixes · " (day.files) " files"</small>
                                        </span>
                                        <span class="run-state">
                                            <code>"Total: " (storage::format_bytes(day.bytes))</code>
                                            badge(
                                                variant: if day.reclaimable_bytes > 0 { BadgeVariant::Primary } else { BadgeVariant::Secondary },
                                                (format!("Reclaimable: {}", storage::format_bytes(day.reclaimable_bytes)))
                                            )
                                        </span>
                                    </div>
                                    if role == Role::Owner && day.reclaimable_bytes > 0 {
                                        <form method="post" action=(format!("{base}/storage/clean")) class="inline-action">
                                            <input type="hidden" name="csrf" value=(&account.csrf)>
                                            <input type="hidden" name="mode" value="prune-unpinned">
                                            <input type="hidden" name="date" value=(&day.date)>
                                            <input type="hidden" name="keep_latest" value="0">
                                            <button class="button" type="submit">"Clean unpinned (" (&day.date) ")"</button>
                                        </form>
                                    }
                                </div>
                            </li>
                        }
                    </ul>
                }
            </section>
        }

        if subview == "all" || subview == "run" {
            <section class="surface captures-ledger">
                <div class="section-head">
                    <h2>"Storage grouped by Capture Run (" (report.by_run.len()) ")"</h2>
                    <p class="muted">"Pin status, raw keyframe footprint, and per-run offload or cleanup"</p>
                </div>
                if report.by_run.is_empty() {
                    <div class="empty"><p>"No capture runs on disk."</p></div>
                } else {
                    <ul class="run-list">
                        for run in &report.by_run {
                            <li>
                                <div class="run-row">
                                    <a class="run-link" href=(format!("/run/{}", run.run_key))>
                                        <span class="run-title">
                                            <strong>(&run.scenario)</strong>
                                            <small>(&run.run_name) " · " (&run.date) " · " (run.files_count) " files"</small>
                                        </span>
                                        <span class="run-state">
                                            if run.offloaded {
                                                badge(variant: BadgeVariant::Primary, "Offloaded to bucket")
                                            } else if !run.pinned_by_fixes.is_empty() || run.pinned_by_problems > 0 {
                                                badge(
                                                    variant: BadgeVariant::Secondary,
                                                    (format!("Pinned · {} Fix(es), {} Problem(s)", run.pinned_by_fixes.len(), run.pinned_by_problems))
                                                )
                                            } else {
                                                badge(variant: BadgeVariant::Outline, "Reclaimable")
                                            }
                                            <code>(storage::format_bytes(run.total_bytes))</code>
                                        </span>
                                        <span class="arrow" aria-hidden="true">"↗"</span>
                                    </a>
                                    if role == Role::Owner {
                                        <div class="review-actions">
                                            if bucket_configured && !run.offloaded {
                                                <form method="post" action=(format!("{base}/storage/offload")) class="inline-action">
                                                    <input type="hidden" name="csrf" value=(&account.csrf)>
                                                    <input type="hidden" name="run_name" value=(&run.run_name)>
                                                    <button class="button" type="submit">"Offload"</button>
                                                </form>
                                            }
                                            if run.pinned_by_fixes.is_empty() && run.pinned_by_problems == 0 {
                                                <form method="post" action=(format!("{base}/storage/clean")) class="inline-action">
                                                    <input type="hidden" name="csrf" value=(&account.csrf)>
                                                    <input type="hidden" name="mode" value="delete-run">
                                                    <input type="hidden" name="run_name" value=(&run.run_name)>
                                                    <button class="button" type="submit">"Delete"</button>
                                                </form>
                                            }
                                        </div>
                                    }
                                </div>
                            </li>
                        }
                    </ul>
                }
            </section>
        }

        if subview == "all" || subview == "category" {
            <section class="surface captures-ledger">
                <div class="section-head">
                    <h2>"Storage grouped by Asset Category"</h2>
                </div>
                <div class="problem-queue">
                    for cat in &report.by_category {
                        <div class="task-tracker">
                            <div class="tracker-head">
                                <strong>(&cat.category)</strong>
                                <span class="tracker-metrics">
                                    (storage::format_bytes(cat.bytes)) " · " (cat.pct) "%"
                                </span>
                            </div>
                            <progress class="task-progress-bar" max="100" value=(cat.pct.to_string())></progress>
                        </div>
                    }
                </div>
            </section>
        }
    })
}

#[derive(Deserialize)]
pub struct StorageConfigForm {
    pub csrf: String,
    pub provider: String,
    pub bucket: String,
    pub endpoint_url: String,
    pub region: String,
    pub prefix: String,
    pub public_base_url: String,
    pub auto_policy: String,
    pub keep_latest_runs: usize,
    pub delete_local: String,
}

pub fn save_storage_config_action(
    panel: &Panel,
    account: &Account,
    project_id: &str,
    form: &StorageConfigForm,
) -> PageResult<Response> {
    auth::check_csrf(account, &form.csrf)?;
    if !account.owner {
        return Err(StatusCode::FORBIDDEN);
    }
    let cfg = storage::BucketConfig {
        provider: form.provider.trim().into(),
        bucket: form.bucket.trim().into(),
        endpoint_url: form.endpoint_url.trim().into(),
        region: form.region.trim().into(),
        prefix: form.prefix.trim().into(),
        public_base_url: form.public_base_url.trim().into(),
        auto_policy: form.auto_policy.trim().into(),
        delete_local_after_offload: form.delete_local.trim() != "false",
        keep_latest_runs: form.keep_latest_runs.min(100),
    };
    storage::save_config(&data_root(panel), &cfg).map_err(db_error)?;
    Ok(redirect(&format!(
        "/projects/{project_id}/storage?q=Saved+external+bucket+configuration"
    )))
}

#[derive(Deserialize)]
pub struct StorageCleanForm {
    pub csrf: String,
    pub mode: String,
    pub keep_latest: Option<usize>,
    pub date: Option<String>,
    pub run_name: Option<String>,
}

pub fn clean_storage_action(
    panel: &Panel,
    account: &Account,
    project_id: &str,
    form: &StorageCleanForm,
) -> PageResult<Response> {
    auth::check_csrf(account, &form.csrf)?;
    if !account.owner {
        return Err(StatusCode::FORBIDDEN);
    }
    let data = data_root(panel);
    let summary = match form.mode.as_str() {
        "strip-frames" => storage::strip_raw_frames(&data, Some(project_id)).map_err(db_error)?,
        "prune-unpinned" => storage::prune_unpinned(
            &data,
            Some(project_id),
            form.keep_latest.unwrap_or(2),
            form.date.as_deref(),
        )
        .map_err(db_error)?,
        "delete-run" => {
            let r = form
                .run_name
                .as_deref()
                .ok_or(StatusCode::UNPROCESSABLE_ENTITY)?;
            storage::delete_run(&data, r, false).map_err(db_error)?
        }
        _ => return Err(StatusCode::UNPROCESSABLE_ENTITY),
    };
    let encoded_note = summary.note.replace(' ', "+");
    Ok(redirect(&format!(
        "/projects/{project_id}/storage?q={encoded_note}"
    )))
}

#[derive(Deserialize)]
pub struct StorageOffloadForm {
    pub csrf: String,
    pub run_name: Option<String>,
    pub fix_id: Option<String>,
    pub target: Option<String>,
}

pub fn offload_storage_action(
    panel: &Panel,
    account: &Account,
    project_id: &str,
    form: StorageOffloadForm,
) -> PageResult<Response> {
    auth::check_csrf(account, &form.csrf)?;
    if !account.owner {
        return Err(StatusCode::FORBIDDEN);
    }
    let data = data_root(panel);
    let note = if let Some(rname) = form.run_name.filter(|s| !s.trim().is_empty()) {
        match storage::offload_run(&data, rname.trim()) {
            Ok(s) => s.note,
            Err(e) => e.to_string(),
        }
    } else if let Some(fid) = form.fix_id.filter(|s| !s.trim().is_empty()) {
        let report = storage::analyze(&data, Some(project_id));
        if let Some(f) = report.by_fix.iter().find(|x| x.fix_id == fid) {
            let mut freed = 0u64;
            let mut count = 0usize;
            let mut last_err = None;
            for rname in &f.referenced_runs {
                match storage::offload_run(&data, rname) {
                    Ok(s) => {
                        freed += s.freed_bytes;
                        count += 1;
                    }
                    Err(e) => last_err = Some(e),
                }
            }
            if let Some(e) = last_err {
                e.to_string()
            } else {
                format!(
                    "Offloaded {count} run(s) for Fix {fid}, freeing {}",
                    storage::format_bytes(freed)
                )
            }
        } else {
            "Fix not found".to_string()
        }
    } else if form.target.as_deref() == Some("all-unpinned") {
        let report = storage::analyze(&data, Some(project_id));
        let mut freed = 0u64;
        let mut count = 0usize;
        let mut last_err = None;
        for run in &report.by_run {
            if run.pinned_by_fixes.is_empty() && run.pinned_by_problems == 0 && !run.offloaded {
                match storage::offload_run(&data, &run.run_name) {
                    Ok(s) => {
                        freed += s.freed_bytes;
                        count += 1;
                    }
                    Err(e) => {
                        last_err = Some(e);
                        break;
                    }
                }
            }
        }
        if let Some(e) = last_err {
            e.to_string()
        } else {
            format!(
                "Offloaded {count} unpinned run(s), freeing {}",
                storage::format_bytes(freed)
            )
        }
    } else {
        "No offload target specified".to_string()
    };
    let encoded = note.replace(' ', "+");
    Ok(redirect(&format!(
        "/projects/{project_id}/storage?q={encoded}"
    )))
}
