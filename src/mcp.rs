use crate::{proposal_record, scenario, service};
use serde_json::{Value, json};
use std::{
    io::{self, BufRead, Write},
    path::PathBuf,
    process::Command,
};

#[derive(Default)]
struct Preview(Option<std::process::Child>, Option<String>);

impl Drop for Preview {
    fn drop(&mut self) {
        if let Some(mut child) = self.0.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

fn import_bundle(source: &std::path::Path) -> Result<Value, String> {
    use std::{fs, io::Read, path::Component};
    let read = |path: &std::path::Path, limit: u64| -> Result<Vec<u8>, String> {
        let file = fs::File::open(path).map_err(|e| e.to_string())?;
        if !file.metadata().map_err(|e| e.to_string())?.is_file() {
            return Err("Expected a regular file".into());
        }
        let mut bytes = Vec::new();
        file.take(limit + 1)
            .read_to_end(&mut bytes)
            .map_err(|e| e.to_string())?;
        if bytes.len() as u64 > limit {
            return Err("Bundle exceeds size limit".into());
        }
        Ok(bytes)
    };
    let root = source.canonicalize().map_err(|e| e.to_string())?;
    let resolve = |name: &str| -> Result<PathBuf, String> {
        let relative = std::path::Path::new(name);
        if relative
            .components()
            .any(|c| !matches!(c, Component::Normal(_)))
        {
            return Err("Invalid bundle path".into());
        }
        let file = root
            .join(relative)
            .canonicalize()
            .map_err(|e| e.to_string())?;
        if !file.starts_with(&root) || !file.is_file() {
            return Err("Bundle file is outside source or is not regular".into());
        }
        Ok(file)
    };
    let bytes = read(&resolve("manifest.json")?, 1024 * 1024)?;
    let value: Value = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
    let _: scenario::BrowserResult = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
    if value["version"] != 1 || value["project"]["id"] != service::detect_project()?.0 {
        return Err("Expected a v1 capture belonging to this project".into());
    }
    let mut paths = std::collections::BTreeSet::new();
    for checkpoint in value["checkpoints"].as_array().into_iter().flatten() {
        if let Some(path) = checkpoint["path"].as_str() {
            paths.insert(path);
        }
    }
    for clip in value["clips"].as_array().into_iter().flatten() {
        for key in ["analysis_path", "sheet_path", "video_path"] {
            if let Some(path) = clip[key].as_str() {
                paths.insert(path);
            }
        }
        for frame in clip["frames"].as_array().into_iter().flatten() {
            if let Some(path) = frame["path"].as_str() {
                paths.insert(path);
            }
        }
    }
    if paths.len() > 256 {
        return Err("Bundle exceeds 256 assets".into());
    }
    let out = service::output()?;
    let staging = out.with_extension("import");
    let mut builder = fs::DirBuilder::new();
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    fs::create_dir_all(out.parent().unwrap()).map_err(|e| e.to_string())?;
    builder.create(&staging).map_err(|e| e.to_string())?;
    let result = (|| -> Result<(), String> {
        let mut remaining = 40 * 1024 * 1024;
        for path in paths {
            if !matches!(
                std::path::Path::new(path)
                    .extension()
                    .and_then(|v| v.to_str()),
                Some("png" | "json" | "mp4")
            ) {
                return Err("Unsupported bundle asset".into());
            }
            let asset = read(&resolve(path)?, remaining)?;
            remaining -= asset.len() as u64;
            let destination = staging.join(path);
            fs::create_dir_all(destination.parent().unwrap()).map_err(|e| e.to_string())?;
            fs::write(destination, asset).map_err(|e| e.to_string())?;
        }
        fs::write(staging.join("manifest.json"), bytes).map_err(|e| e.to_string())?;
        fs::rename(&staging, &out).map_err(|e| e.to_string())
    })();
    if result.is_err() {
        let _ = fs::remove_dir_all(&staging);
    }
    result?;
    Ok(json!({"run":out.file_name().and_then(|name| name.to_str()),"manifest":value}))
}

const TOOLS: &[(&str, &str, &[&str], &[&str])] = &[
    (
        "project_context",
        "Read and register design context for the current checkout",
        &[],
        &[],
    ),
    (
        "design_record",
        "Record a design entry with optional capture evidence",
        &["kind", "title", "note"],
        &["status", "surface", "reference", "supersedes", "run"],
    ),
    (
        "ui_discover",
        "Discover declared UI dependencies",
        &[],
        &["path"],
    ),
    (
        "ui_scan",
        "Scan Rust UI syntax for candidates",
        &[],
        &["path"],
    ),
    (
        "skill_check",
        "Validate the Rupicola skill catalog",
        &[],
        &["path"],
    ),
    (
        "capture_run",
        "Capture a structured web scenario and save it to the current project",
        &["scenario"],
        &["video"],
    ),
    (
        "evidence_list",
        "List persisted captures for the current project",
        &[],
        &[],
    ),
    (
        "capture_review",
        "Record an inspection finding against a capture",
        &["run", "note"],
        &["artifact"],
    ),
    (
        "artifact_read",
        "Read an evidence PNG or JSON artifact",
        &["run", "artifact"],
        &[],
    ),
    (
        "service_health",
        "Check the local panel service and checkout",
        &[],
        &[],
    ),
    (
        "capture_cosmic",
        "Capture a trusted local COSMIC applet in a nested compositor",
        &["applet", "desktop_entry", "app_id", "fixtures"],
        &["symbolic_icon", "revision", "x", "y"],
    ),
    (
        "evidence_import",
        "Import a local manifest directory into this project's panel",
        &["path"],
        &[],
    ),
    (
        "preview_start",
        "Start this MCP session's isolated Rupicola panel on loopback with optional demo project",
        &[],
        &["demo"],
    ),
    (
        "preview_status",
        "Report this MCP session's preview URL",
        &[],
        &[],
    ),
    (
        "preview_stop",
        "Stop this MCP session's isolated panel",
        &[],
        &[],
    ),
    (
        "propose_list",
        "List proposals and pending agent dispatches for the current project",
        &[],
        &["project", "status"],
    ),
    (
        "propose_get",
        "Read a proposal, its structured prompt, comments, and attachment paths",
        &["id"],
        &["project"],
    ),
    (
        "propose_enrich",
        "Save structured extraction (palette, tactics, layout, prompt) and/or rendered HTML/image artifact onto a proposal",
        &["id"],
        &[
            "project",
            "summary",
            "palette_hex",
            "layout_structure",
            "identified_tactics",
            "refined_prompt",
            "target_requirements",
            "html_content",
            "image_path",
            "viewport",
            "stack",
        ],
    ),
    (
        "propose_await",
        "Wait for a user-dispatched proposal from the panel (long-poll)",
        &[],
        &["project", "timeout_secs"],
    ),
    (
        "probe_run",
        "Run a combinatorial UI inspection matrix (mode: page-check | all-pages | responsive-matrix | interaction-map | exhaustive)",
        &["url"],
        &["mode", "pages", "viewports", "themes", "max_pages", "video"],
    ),
    (
        "problem_list",
        "List problems in the project's triage queue",
        &[],
        &["project", "status"],
    ),
    (
        "problem_add",
        "Record a visual problem with evidence assets, violated rules, and a proposed solution",
        &["title", "diagnosis", "solution"],
        &[
            "project", "route", "viewport", "theme", "severity", "rule", "evidence",
        ],
    ),
    (
        "problem_triage",
        "Update a problem's status (open | in-progress | intentional | false-positive | needs-reprocess | resolved)",
        &["id", "status"],
        &["project", "comment", "solution", "agent"],
    ),
    (
        "problem_progress",
        "Claim or update live task progress (agent, stage, pct, step, total_steps, activity) for a problem being fixed",
        &["id", "activity"],
        &["project", "agent", "stage", "pct", "step", "total_steps"],
    ),
    (
        "fix_list",
        "List recorded fixes and before/after comparisons",
        &[],
        &["project", "status"],
    ),
    (
        "fix_record",
        "Record a completed fix with before/after asset URLs, applied tactics, and intermediate steps",
        &["title", "summary", "before_url", "after_url"],
        &[
            "project", "problem", "route", "viewport", "theme", "diff_url", "tactic", "file",
            "step",
        ],
    ),
    (
        "storage_status",
        "Report disk storage usage grouped by project, category, date, fix, and capture run",
        &[],
        &["project"],
    ),
    (
        "storage_clean",
        "Clean up local storage (mode: strip-frames | prune-unpinned | delete-run)",
        &[],
        &["mode", "project", "keep_latest", "date", "run"],
    ),
    (
        "storage_offload",
        "Offload a capture run or fix's assets to the configured external bucket and free local disk",
        &[],
        &["run", "fix", "project"],
    ),
];

fn cli(args: &[String]) -> Result<String, String> {
    let result = Command::new(std::env::current_exe().map_err(|e| e.to_string())?)
        .args(args)
        .output()
        .map_err(|e| e.to_string())?;
    if result.status.success() {
        Ok(String::from_utf8_lossy(&result.stdout).trim().into())
    } else {
        Err(String::from_utf8_lossy(&result.stderr).trim().into())
    }
}

fn manifest(run: &str) -> Result<(PathBuf, Value), String> {
    if !scenario::identifier(run) {
        return Err("Invalid run ID".into());
    }
    let path = service::data()?
        .join("captures")
        .join(run)
        .join("manifest.json");
    let value: Value = serde_json::from_slice(&std::fs::read(&path).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    if value["project"]["id"] != service::detect_project()?.0 {
        return Err("Capture belongs to another project".into());
    }
    Ok((path, value))
}

fn tool(name: &str, args: &Value, preview: &mut Preview) -> Result<Value, String> {
    let arg = |key: &str| {
        args[key]
            .as_str()
            .ok_or_else(|| format!("Expected string: {key}"))
    };
    let output = match name {
        "project_context" => cli(&["design".into(), "open".into(), "--json".into()])?,
        "service_health" => cli(&["doctor".into()])?,
        "ui_scan" | "ui_discover" | "skill_check" => {
            let command = match name {
                "ui_scan" => "scan",
                "ui_discover" => "discover",
                _ => "check",
            };
            cli(&[
                command.into(),
                "--json".into(),
                "--".into(),
                args["path"].as_str().unwrap_or(".").into(),
            ])?
        }
        "design_record" => {
            let mut command = vec!["design".into(), "add".into()];
            for key in [
                "title",
                "note",
                "status",
                "surface",
                "reference",
                "supersedes",
            ] {
                if let Some(value) = args[key].as_str() {
                    command.push(format!("--{key}={value}"));
                }
            }
            if let Some(run) = args["run"].as_str() {
                command.push(format!("--evidence={}", manifest(run)?.0.display()));
            }
            command.extend(["--".into(), arg("kind")?.into()]);
            cli(&command)?
        }
        "preview_start" => {
            if preview.0.is_some() {
                return Err("Stop the existing preview first".into());
            }
            let root = service::output()?.with_extension("preview");
            let mut builder = std::fs::DirBuilder::new();
            #[cfg(unix)]
            {
                use std::os::unix::fs::DirBuilderExt;
                builder.mode(0o700);
            }
            std::fs::create_dir_all(root.parent().unwrap()).map_err(|e| e.to_string())?;
            builder.create(&root).map_err(|e| e.to_string())?;
            if args["demo"].as_bool().unwrap_or(false) {
                let fixture = root.join("design/demo");
                std::fs::create_dir_all(&fixture).map_err(|e| e.to_string())?;
                std::fs::write(
                    fixture.join("project.json"),
                    r#"{"id":"demo","label":"Example project"}"#,
                )
                .map_err(|e| e.to_string())?;
            }
            let binary = std::env::current_exe()
                .map_err(|e| e.to_string())?
                .with_file_name("rupicola-panel");
            let mut child = Command::new(binary)
                .args(["--port", "0", "--db"])
                .arg(root.join("panel.db"))
                .arg("--captures")
                .arg(root.join("captures"))
                .env_remove("RUPICOLA_SETUP_CODE")
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::piped())
                .spawn()
                .map_err(|e| e.to_string())?;
            let stdout = child.stdout.take().unwrap();
            let (send, receive) = std::sync::mpsc::channel();
            std::thread::spawn(move || {
                let mut reader = io::BufReader::new(stdout);
                let mut line = String::new();
                let _ = reader.read_line(&mut line);
                let _ = send.send(line);
                let _ = io::copy(&mut reader, &mut io::sink());
            });
            let url = receive
                .recv_timeout(std::time::Duration::from_secs(15))
                .ok()
                .and_then(|line| {
                    line.split_whitespace()
                        .find(|word| word.starts_with("http://127.0.0.1:"))
                        .map(str::to_owned)
                });
            let Some(url) = url else {
                let _ = child.kill();
                let _ = child.wait();
                return Err("Preview did not announce a loopback URL within 15 seconds".into());
            };
            *preview = Preview(Some(child), Some(url.clone()));
            json!({"url":url,"fixture":args["demo"] == true}).to_string()
        }
        "preview_status" => {
            if let Some(child) = preview.0.as_mut() {
                if child.try_wait().map_err(|e| e.to_string())?.is_some() {
                    *preview = Preview::default();
                }
            }
            json!({"url":preview.1}).to_string()
        }
        "preview_stop" => {
            *preview = Preview::default();
            "Preview stopped".into()
        }
        "evidence_import" => import_bundle(std::path::Path::new(arg("path")?))?.to_string(),
        "capture_cosmic" => {
            let (project_id, project_label) = service::detect_project()?;
            let coordinate = |key: &str, default| -> Result<u32, String> {
                if args[key].is_null() {
                    return Ok(default);
                }
                args[key]
                    .as_u64()
                    .and_then(|v| u32::try_from(v).ok())
                    .ok_or_else(|| format!("Invalid coordinate: {key}"))
            };
            let out = service::output()?;
            let capture = scenario::cosmic(
                scenario::CosmicCapture {
                    applet: arg("applet")?.into(),
                    desktop_entry: arg("desktop_entry")?.into(),
                    app_id: arg("app_id")?.into(),
                    fixtures: arg("fixtures")?.into(),
                    symbolic_icon: args["symbolic_icon"].as_str().map(PathBuf::from),
                    x: coordinate("x", 640)?,
                    y: coordinate("y", 18)?,
                    project_id,
                    project_label,
                    revision: args["revision"].as_str().map(str::to_owned),
                },
                &out,
            )?;
            json!({"run":out.file_name().and_then(|name| name.to_str()),"manifest":capture})
                .to_string()
        }
        "capture_run" => {
            let mut input: scenario::Scenario =
                serde_json::from_value(args["scenario"].clone()).map_err(|e| e.to_string())?;
            let (id, label) = service::detect_project()?;
            input.set_project(id, label);
            let out = service::output()?;
            let module = std::env::var_os("RUPICOLA_PLAYWRIGHT_MODULE").map(PathBuf::from);
            let capture = scenario::web(
                input,
                &out,
                module.as_deref(),
                8,
                args["video"].as_bool().unwrap_or(false),
            )?;
            json!({"run":out.file_name().and_then(|name| name.to_str()), "manifest":capture})
                .to_string()
        }
        "evidence_list" => {
            let dir = service::data()?.join("captures");
            let mut runs = Vec::new();
            if dir.exists() {
                for entry in std::fs::read_dir(dir).map_err(|e| e.to_string())?.flatten() {
                    if let Some(id) = entry.file_name().to_str() {
                        if let Ok((_, value)) = manifest(id) {
                            runs.push(json!({"run":id,"scenario":value["scenario"],"status":value["status"],"inspection":value["inspection"]}));
                        }
                    }
                }
            }
            json!(runs).to_string()
        }
        "capture_review" => {
            let (path, _) = manifest(arg("run")?)?;
            scenario::review(
                &path,
                arg("note")?.into(),
                args["artifact"].as_str().map(PathBuf::from),
            )?;
            "Inspection recorded".into()
        }
        "artifact_read" => {
            let (path, value) = manifest(arg("run")?)?;
            let artifact = arg("artifact")?;
            let listed = value["checkpoints"]
                .as_array()
                .into_iter()
                .flatten()
                .any(|x| x["path"] == artifact)
                || value["clips"].as_array().into_iter().flatten().any(|x| {
                    ["analysis_path", "sheet_path"]
                        .iter()
                        .any(|key| x[key] == artifact)
                        || x["frames"]
                            .as_array()
                            .into_iter()
                            .flatten()
                            .any(|frame| frame["path"] == artifact)
                });
            if !listed {
                return Err("Artifact is absent from manifest".into());
            }
            let relative = std::path::Path::new(artifact);
            if relative
                .components()
                .any(|c| !matches!(c, std::path::Component::Normal(_)))
            {
                return Err("Invalid artifact path".into());
            }
            let root = path
                .parent()
                .unwrap()
                .canonicalize()
                .map_err(|e| e.to_string())?;
            let file = root
                .join(relative)
                .canonicalize()
                .map_err(|e| e.to_string())?;
            if !file.starts_with(root)
                || std::fs::metadata(&file).map_err(|e| e.to_string())?.len() > 8 * 1024 * 1024
            {
                return Err("Artifact outside run or larger than 8 MiB".into());
            }
            let bytes = std::fs::read(file).map_err(|e| e.to_string())?;
            if artifact.ends_with(".png") {
                return Ok(
                    json!({"content":[{"type":"image","mimeType":"image/png","data":base64(&bytes)}]}),
                );
            }
            String::from_utf8(bytes).map_err(|e| e.to_string())?
        }
        "propose_list" => {
            let project_id = match args["project"].as_str() {
                Some(id) if proposal_record::safe_id(id) => id.to_owned(),
                Some(_) => return Err("Invalid project ID".into()),
                None => service::detect_project()?.0,
            };
            let root = service::data()?
                .join(proposal_record::DIRECTORY)
                .join(&project_id);
            let mut items = proposal_record::entries(&root).map_err(|e| e.to_string())?;
            if let Some(status) = args["status"].as_str() {
                items.retain(|p| p.status.slug() == status);
            }
            let dispatches = proposal_record::dispatches(&root).map_err(|e| e.to_string())?;
            serde_json::to_string_pretty(&json!({
                "project": project_id,
                "proposals": items,
                "pending_dispatches": dispatches,
            }))
            .map_err(|e| e.to_string())?
        }
        "propose_get" => {
            let id = arg("id")?;
            if !proposal_record::safe_id(id) {
                return Err("Invalid proposal ID".into());
            }
            let project_id = match args["project"].as_str() {
                Some(p) if proposal_record::safe_id(p) => p.to_owned(),
                Some(_) => return Err("Invalid project ID".into()),
                None => service::detect_project()?.0,
            };
            let root = service::data()?
                .join(proposal_record::DIRECTORY)
                .join(&project_id);
            let p = proposal_record::read_proposal(&root.join(format!("{id}.json")))
                .map_err(|e| e.to_string())?;
            let parent = p
                .parent_id
                .as_deref()
                .filter(|pid| proposal_record::safe_id(pid))
                .and_then(|pid| {
                    proposal_record::read_proposal(&root.join(format!("{pid}.json"))).ok()
                });
            let prompt = proposal_record::structured_prompt(&project_id, &p, parent.as_ref());
            serde_json::to_string_pretty(&json!({
                "project": project_id,
                "proposal": p,
                "structured_prompt": prompt,
                "assets_dir": root.join(&p.id).join("assets"),
            }))
            .map_err(|e| e.to_string())?
        }
        "propose_enrich" => {
            let id = arg("id")?;
            if !proposal_record::safe_id(id) {
                return Err("Invalid proposal ID".into());
            }
            let project_id = match args["project"].as_str() {
                Some(p) if proposal_record::safe_id(p) => p.to_owned(),
                Some(_) => return Err("Invalid project ID".into()),
                None => service::detect_project()?.0,
            };
            let root = service::data()?
                .join(proposal_record::DIRECTORY)
                .join(&project_id);
            let path = root.join(format!("{id}.json"));
            let mut p = proposal_record::read_proposal(&path).map_err(|e| e.to_string())?;
            let assets_dir = root.join(id).join("assets");
            if let Some(html) = args["html_content"].as_str() {
                if html.len() > 2 * 1024 * 1024 {
                    return Err("html_content exceeds 2 MiB".into());
                }
                proposal_record::private_dir(&assets_dir).map_err(|e| e.to_string())?;
                std::fs::write(assets_dir.join("preview.html"), html).map_err(|e| e.to_string())?;
                p.artifact = Some(proposal_record::Artifact {
                    kind: proposal_record::ArtifactKind::Html,
                    entry: "preview.html".into(),
                    viewport: args["viewport"]
                        .as_str()
                        .map(str::to_owned)
                        .or_else(|| Some("1440x900".into())),
                    stack: args["stack"]
                        .as_str()
                        .map(str::to_owned)
                        .or_else(|| Some("web".into())),
                });
            } else if let Some(img) = args["image_path"].as_str() {
                let src = PathBuf::from(img);
                let meta = std::fs::metadata(&src).map_err(|e| e.to_string())?;
                if !meta.is_file() || meta.len() > 16 * 1024 * 1024 {
                    return Err("image_path must be a regular file <= 16 MiB".into());
                }
                let ext = src
                    .extension()
                    .and_then(|e| e.to_str())
                    .filter(|e| matches!(*e, "png" | "jpg" | "jpeg" | "webp"))
                    .unwrap_or("png");
                let entry = format!("render.{ext}");
                proposal_record::private_dir(&assets_dir).map_err(|e| e.to_string())?;
                std::fs::copy(&src, assets_dir.join(&entry)).map_err(|e| e.to_string())?;
                p.artifact = Some(proposal_record::Artifact {
                    kind: proposal_record::ArtifactKind::Image,
                    entry,
                    viewport: args["viewport"].as_str().map(str::to_owned),
                    stack: args["stack"].as_str().map(str::to_owned),
                });
            }
            let mut ext = p.extracted.take().unwrap_or_default();
            if let Some(s) = args["summary"].as_str() {
                ext.summary = Some(s.to_owned());
            }
            if let Some(arr) = args["palette_hex"].as_array() {
                ext.palette_hex = arr
                    .iter()
                    .filter_map(|v| v.as_str().map(str::to_owned))
                    .collect();
            }
            if let Some(s) = args["layout_structure"].as_str() {
                ext.layout_structure = Some(s.to_owned());
            }
            if let Some(arr) = args["identified_tactics"].as_array() {
                ext.identified_tactics = arr
                    .iter()
                    .filter_map(|v| v.as_str().map(str::to_owned))
                    .collect();
            }
            if let Some(s) = args["refined_prompt"].as_str() {
                ext.refined_prompt = Some(s.to_owned());
            }
            if let Some(arr) = args["target_requirements"].as_array() {
                ext.target_requirements = arr
                    .iter()
                    .filter_map(|v| v.as_str().map(str::to_owned))
                    .collect();
            }
            p.extracted = Some(ext);
            p.status = proposal_record::Status::Proposed;
            proposal_record::write_proposal(&root, &p).map_err(|e| e.to_string())?;
            format!("Enriched proposal {id}")
        }
        "propose_await" => {
            let project_id = match args["project"].as_str() {
                Some(p) if proposal_record::safe_id(p) => p.to_owned(),
                Some(_) => return Err("Invalid project ID".into()),
                None => service::detect_project()?.0,
            };
            let timeout = args["timeout_secs"].as_u64().unwrap_or(25).min(120);
            let root = service::data()?
                .join(proposal_record::DIRECTORY)
                .join(&project_id);
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(timeout);
            loop {
                let mut list = proposal_record::dispatches(&root).map_err(|e| e.to_string())?;
                if let Some(dispatch) = list.pop() {
                    let _ = std::fs::remove_file(
                        root.join(proposal_record::DISPATCH_DIR)
                            .join(format!("{}.json", dispatch.id)),
                    );
                    let proposal = proposal_record::read_proposal(
                        &root.join(format!("{}.json", dispatch.proposal_id)),
                    )
                    .ok();
                    return Ok(json!({
                        "content": [{
                            "type": "text",
                            "text": serde_json::to_string_pretty(&json!({
                                "dispatch": dispatch,
                                "proposal": proposal,
                                "assets_dir": root.join(&dispatch.proposal_id).join("assets"),
                            })).map_err(|e| e.to_string())?
                        }]
                    }));
                }
                if std::time::Instant::now() >= deadline {
                    return Ok(json!({"content":[{"type":"text","text":"No pending dispatches"}]}));
                }
                std::thread::sleep(std::time::Duration::from_millis(400));
            }
        }
        "probe_run" => {
            let mut command = vec![
                "probe".into(),
                "--json".into(),
                format!("--url={}", arg("url")?),
            ];
            if let Some(m) = args["mode"].as_str() {
                command.push(format!("--mode={m}"));
            }
            if let Some(p) = args["pages"].as_str() {
                command.push(format!("--pages={p}"));
            }
            if let Some(v) = args["viewports"].as_str() {
                command.push(format!("--viewports={v}"));
            }
            if let Some(t) = args["themes"].as_str() {
                command.push(format!("--themes={t}"));
            }
            if let Some(mp) = args["max_pages"].as_u64() {
                command.push(format!("--max-pages={mp}"));
            }
            if args["video"].as_bool().unwrap_or(false) {
                command.push("--video".into());
            }
            cli(&command)?
        }
        "problem_list" => {
            let mut command = vec!["problem".into(), "list".into(), "--json".into()];
            if let Some(p) = args["project"].as_str() {
                command.push(format!("--project={p}"));
            }
            if let Some(s) = args["status"].as_str() {
                command.push(format!("--status={s}"));
            }
            cli(&command)?
        }
        "problem_add" => {
            let mut command = vec![
                "problem".into(),
                "add".into(),
                format!("--title={}", arg("title")?),
                format!("--diagnosis={}", arg("diagnosis")?),
                format!("--solution={}", arg("solution")?),
            ];
            for (k, flag) in [
                ("project", "--project"),
                ("route", "--route"),
                ("viewport", "--viewport"),
                ("theme", "--theme"),
                ("severity", "--severity"),
                ("rule", "--rule"),
                ("evidence", "--evidence"),
            ] {
                if let Some(v) = args[k].as_str() {
                    command.push(format!("{flag}={v}"));
                }
            }
            cli(&command)?
        }
        "problem_triage" => {
            let mut command = vec![
                "problem".into(),
                "triage".into(),
                arg("id")?.into(),
                format!("--status={}", arg("status")?),
            ];
            for (k, flag) in [
                ("project", "--project"),
                ("comment", "--comment"),
                ("solution", "--solution"),
                ("agent", "--agent"),
            ] {
                if let Some(v) = args[k].as_str() {
                    command.push(format!("{flag}={v}"));
                }
            }
            cli(&command)?
        }
        "problem_progress" => {
            let default_agent = format!("mcp-agent (PID {})", std::process::id());
            let agent = args["agent"].as_str().unwrap_or(&default_agent);
            let mut command = vec![
                "problem".into(),
                "progress".into(),
                arg("id")?.into(),
                format!("--agent={agent}"),
                format!("--activity={}", arg("activity")?),
            ];
            if let Some(p) = args["project"].as_str() {
                command.push(format!("--project={p}"));
            }
            if let Some(st) = args["stage"].as_str() {
                command.push(format!("--stage={st}"));
            }
            if let Some(pct) = args["pct"].as_u64() {
                command.push(format!("--pct={pct}"));
            }
            if let Some(step) = args["step"].as_u64() {
                command.push(format!("--step={step}"));
            }
            if let Some(tot) = args["total_steps"].as_u64() {
                command.push(format!("--total-steps={tot}"));
            }
            cli(&command)?
        }
        "fix_list" => {
            let mut command = vec!["fix".into(), "list".into(), "--json".into()];
            if let Some(p) = args["project"].as_str() {
                command.push(format!("--project={p}"));
            }
            if let Some(s) = args["status"].as_str() {
                command.push(format!("--status={s}"));
            }
            cli(&command)?
        }
        "fix_record" => {
            let mut command = vec![
                "fix".into(),
                "record".into(),
                format!("--title={}", arg("title")?),
                format!("--summary={}", arg("summary")?),
                format!("--before-url={}", arg("before_url")?),
                format!("--after-url={}", arg("after_url")?),
            ];
            for (k, flag) in [
                ("project", "--project"),
                ("problem", "--problem"),
                ("route", "--route"),
                ("viewport", "--viewport"),
                ("theme", "--theme"),
                ("diff_url", "--diff-url"),
                ("tactic", "--tactic"),
                ("file", "--file"),
                ("step", "--step"),
            ] {
                if let Some(v) = args[k].as_str() {
                    command.push(format!("{flag}={v}"));
                }
            }
            cli(&command)?
        }
        "storage_status" => {
            let mut command = vec!["storage".into(), "status".into(), "--json".into()];
            if let Some(p) = args["project"].as_str() {
                command.push(format!("--project={p}"));
            }
            cli(&command)?
        }
        "storage_clean" => {
            let mut command = vec!["storage".into(), "clean".into(), "--json".into()];
            for (k, flag) in [
                ("mode", "--mode"),
                ("project", "--project"),
                ("date", "--date"),
                ("run", "--run"),
            ] {
                if let Some(v) = args[k].as_str() {
                    command.push(format!("{flag}={v}"));
                }
            }
            if let Some(kl) = args["keep_latest"].as_u64() {
                command.push(format!("--keep-latest={kl}"));
            }
            cli(&command)?
        }
        "storage_offload" => {
            let mut command = vec!["storage".into(), "offload".into(), "--json".into()];
            for (k, flag) in [("run", "--run"), ("fix", "--fix"), ("project", "--project")] {
                if let Some(v) = args[k].as_str() {
                    command.push(format!("{flag}={v}"));
                }
            }
            cli(&command)?
        }
        _ => return Err("Unknown tool".into()),
    };
    Ok(json!({"content":[{"type":"text","text":output}]}))
}

fn base64(bytes: &[u8]) -> String {
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut result = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for group in bytes.chunks(3) {
        let a = group[0];
        let b = group.get(1).copied().unwrap_or(0);
        let c = group.get(2).copied().unwrap_or(0);
        result.push(ALPHABET[(a >> 2) as usize] as char);
        result.push(ALPHABET[(((a & 3) << 4) | (b >> 4)) as usize] as char);
        result.push(if group.len() > 1 {
            ALPHABET[(((b & 15) << 2) | (c >> 6)) as usize] as char
        } else {
            '='
        });
        result.push(if group.len() > 2 {
            ALPHABET[(c & 63) as usize] as char
        } else {
            '='
        });
    }
    result
}

struct SessionGuard(Option<PathBuf>);

impl SessionGuard {
    fn register() -> Self {
        let Ok(data) = service::data() else {
            return Self(None);
        };
        let path = data.join(proposal_record::MCP_SESSION_FILE);
        let started_at_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);
        let session = proposal_record::McpSession {
            pid: std::process::id(),
            project_id: service::project().ok().flatten().map(|p| p.0),
            client: None,
            started_at_ms,
        };
        if proposal_record::write_atomic(&path, &session).is_ok() {
            Self(Some(path))
        } else {
            Self(None)
        }
    }
}

impl Drop for SessionGuard {
    fn drop(&mut self) {
        if let Some(path) = self.0.take() {
            if proposal_record::read_json::<proposal_record::McpSession>(&path, 16 * 1024)
                .is_ok_and(|s| s.pid == std::process::id())
            {
                let _ = std::fs::remove_file(path);
            }
        }
    }
}

pub fn serve() -> io::Result<()> {
    let _session = SessionGuard::register();
    let mut preview = Preview::default();
    let mut stdout = io::stdout().lock();
    for line in io::stdin().lock().lines() {
        let line = line?;
        let request: Value = match serde_json::from_str(&line) {
            Ok(value) => value,
            Err(_) => {
                writeln!(
                    stdout,
                    "{}",
                    json!({"jsonrpc":"2.0","id":null,"error":{"code":-32700,"message":"Parse error"}})
                )?;
                stdout.flush()?;
                continue;
            }
        };
        let Some(id) = request.get("id") else {
            continue;
        };
        let params = &request["params"];
        let body = match request["method"].as_str() {
            Some("initialize") => {
                let version = params["protocolVersion"].as_str().unwrap_or("");
                let version = match version {
                    "2024-11-05" | "2025-03-26" | "2025-06-18" | "2025-11-25" => version,
                    _ => "2025-11-25",
                };
                json!({"result":{"protocolVersion":version,"capabilities":{"tools":{}},"serverInfo":{"name":"rupicola","version":env!("CARGO_PKG_VERSION")}}})
            }
            Some("ping") => json!({"result":{}}),
            Some("tools/list") => {
                let tools: Vec<_> = TOOLS.iter().map(|(name, description, required, optional)| {
                    let properties: serde_json::Map<_,_> = required.iter().chain(*optional).map(|key|
                        ((*key).into(), json!({"type":match *key {
                            "scenario" => "object",
                            "video" | "demo" => "boolean",
                            "x" | "y" | "timeout_secs" | "max_pages" | "pct" | "step" | "total_steps" => "integer",
                            "palette_hex" | "identified_tactics" | "target_requirements" => "array",
                            _ => "string"
                        }}))
                    ).collect();
                    json!({"name":name,"description":description,"inputSchema":{"type":"object","properties":properties,"required":required}})
                }).collect();
                json!({"result":{"tools":tools}})
            }
            Some("tools/call") => {
                let result = tool(
                    params["name"].as_str().unwrap_or(""),
                    &params["arguments"],
                    &mut preview,
                )
                .unwrap_or_else(
                    |error| json!({"isError":true,"content":[{"type":"text","text":error}]}),
                );
                json!({"result":result})
            }
            _ => json!({"error":{"code":-32601,"message":"Method not found"}}),
        };
        let mut reply = json!({"jsonrpc":"2.0","id":id});
        reply
            .as_object_mut()
            .unwrap()
            .extend(body.as_object().unwrap().clone());
        writeln!(stdout, "{reply}")?;
        stdout.flush()?;
    }
    Ok(())
}
