use crate::{
    PageResult, Panel, StatusCode, db_error, fix_record as fixes, ingest, media, pipeline,
    problem_record as problems, read_manifest, run_dir, safe_name,
};
use serde::Deserialize;
use serde_json::{Value, json};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Request<T> {
    project: String,
    key: String,
    record: T,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ViolatedRuleInput {
    #[serde(default = "default_rule_scope")]
    scope: String,
    code: String,
    title: String,
}

fn default_rule_scope() -> String {
    "global".into()
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ProblemInput {
    title: String,
    diagnosis: String,
    solution: String,
    severity: problems::Severity,
    #[serde(default)]
    route: String,
    #[serde(default)]
    viewport: String,
    #[serde(default)]
    theme: String,
    #[serde(default)]
    evidence: Vec<String>,
    #[serde(default)]
    violated_rules: Vec<ViolatedRuleInput>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FixInput {
    title: String,
    summary: String,
    comparisons: Vec<fixes::ComparisonPair>,
    #[serde(default)]
    problem_ids: Vec<String>,
    #[serde(default)]
    tactics: Vec<String>,
    #[serde(default)]
    modified_files: Vec<String>,
}

pub(super) const UPDATES: &[(&str, &str, &[&str], &[&str])] = &[
    (
        "problem_triage",
        "Update problem status and optionally add a comment or solution",
        &["status"],
        &["comment", "solution"],
    ),
    (
        "problem_progress",
        "Record progress for a problem",
        &["agent", "stage", "step", "pct", "activity"],
        &[],
    ),
    (
        "fix_review",
        "Record a review decision on a fix",
        &["status"],
        &["comment"],
    ),
    (
        "propose_enrich",
        "Submit extracted design guidance and an optional HTML or base64 image preview",
        &["extracted"],
        &[
            "html_content",
            "image_base64",
            "image_format",
            "viewport",
            "stack",
        ],
    ),
    (
        "propose_review",
        "Comment on, request a variation of, approve or reject a legacy proposal",
        &["action"],
        &["comment"],
    ),
    (
        "problem_analyze",
        "Create problem candidates from the published run specified by id",
        &[],
        &[],
    ),
    (
        "capture_review",
        "Record a finding on an authorized run. Use the run key as id and a declared artifact path as evidence.",
        &["note"],
        &["evidence"],
    ),
];

pub(super) fn update_tools() -> impl Iterator<Item = Value> {
    UPDATES.iter().map(|(name, description, required, optional)| {
        let properties: serde_json::Map<String, Value> = required.iter().chain(*optional).map(|key| {
            let schema = match *key {
                "step" | "pct" => json!({"type":"integer","minimum":0}),
                "extracted" => json!({"type":"object","properties":{
                    "summary":{"type":"string"},"layout_structure":{"type":"string"},"refined_prompt":{"type":"string"},
                    "palette_hex":{"type":"array","items":{"type":"string"}},
                    "identified_tactics":{"type":"array","items":{"type":"string"}},
                    "target_requirements":{"type":"array","items":{"type":"string"}}}}),
                "image_format" => json!({"enum":["png","jpg","webp"]}),
                _ => json!({"type":"string"}),
            };
            ((*key).to_owned(), schema)
        }).collect();
        json!({"name":name,"description":description,"inputSchema":{"type":"object",
            "additionalProperties":false,"required":["project","id","record"],"properties":{
                "project":{"type":"string"},"id":{"type":"string"},
                "record":{"type":"object","properties":properties,"required":required,"additionalProperties":false}}}})
    })
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Update<T> {
    project: String,
    id: String,
    record: T,
}

fn update_input<'a, T: Deserialize<'a>>(args: &'a Value) -> PageResult<Update<T>> {
    let data = Update::<T>::deserialize(args).map_err(|_| StatusCode::BAD_REQUEST)?;
    if !safe_name(&data.project) || data.project.len() > 100 || !problems::valid_id(&data.id) {
        return Err(StatusCode::BAD_REQUEST);
    }
    Ok(data)
}

pub(super) fn update(panel: &Panel, actor: &str, tool: &str, args: &Value) -> PageResult<Value> {
    let Some((_, _, required, optional)) = UPDATES.iter().find(|(name, _, _, _)| *name == tool)
    else {
        return Err(StatusCode::BAD_REQUEST);
    };
    if args
        .get("record")
        .and_then(Value::as_object)
        .is_none_or(|record| {
            record
                .keys()
                .any(|key| !required.contains(&key.as_str()) && !optional.contains(&key.as_str()))
        })
    {
        return Err(StatusCode::BAD_REQUEST);
    }
    match tool {
        "problem_triage" => {
            let data = update_input(args)?;
            pipeline::triage_problem(panel, actor, &data.project, &data.id, data.record)?;
            Ok(json!({"id":data.id}))
        }
        "problem_progress" => {
            let data = update_input(args)?;
            pipeline::progress_problem(panel, &data.project, &data.id, &data.record)?;
            let root = pipeline::data_root(panel);
            let skill_modules = problems::load(&root, &data.project, &data.id)
                .map(|problem| problems::recommended_skill_modules(&problem))
                .unwrap_or_default();
            Ok(json!({"id": data.id, "skill_modules": skill_modules}))
        }
        "problem_analyze" => {
            let data = Update::<serde::de::IgnoredAny>::deserialize(args)
                .map_err(|_| StatusCode::BAD_REQUEST)?;
            if !safe_name(&data.project) || data.project.len() > 100 || data.id.len() > 256 {
                return Err(StatusCode::BAD_REQUEST);
            }
            pipeline::analyze_run(panel, &data.project, &data.id)?;
            Ok(json!({"run":data.id}))
        }
        "capture_review" => {
            #[derive(Deserialize)]
            struct FindingInput {
                note: String,
                evidence: Option<String>,
            }
            let data =
                Update::<FindingInput>::deserialize(args).map_err(|_| StatusCode::BAD_REQUEST)?;
            if !safe_name(&data.project) || data.project.len() > 100 || data.id.len() > 256 {
                return Err(StatusCode::BAD_REQUEST);
            }
            let (root, name) = data.id.split_once('/').ok_or(StatusCode::BAD_REQUEST)?;
            let root = root.parse().map_err(|_| StatusCode::BAD_REQUEST)?;
            let dir = run_dir(panel, root, name).ok_or(StatusCode::NOT_FOUND)?;
            let manifest = read_manifest(&dir)
                .filter(|m| m.project.id == data.project)
                .ok_or(StatusCode::NOT_FOUND)?;
            if let Some(path) = data.record.evidence.as_deref() {
                media::resolve_artifact(dir.clone(), &manifest, path)?;
            }
            crate::pages::append_capture_review(
                &dir,
                &manifest,
                actor,
                &data.record.note,
                data.record.evidence.as_deref(),
            )?;
            Ok(json!({"run":data.id}))
        }
        "propose_enrich" => {
            let data = update_input(args)?;
            crate::proposes::enrich(panel, &data.project, &data.id, data.record)?;
            Ok(json!({"id":data.id}))
        }
        "propose_review" => {
            let data = update_input(args)?;
            crate::proposes::review_authorized(
                panel,
                actor,
                &data.project,
                &data.id,
                &data.record,
            )?;
            Ok(json!({"id":data.id}))
        }
        "fix_review" => {
            let data = update_input(args)?;
            pipeline::review_fix(panel, actor, &data.project, &data.id, data.record)?;
            Ok(json!({"id":data.id}))
        }
        _ => Err(StatusCode::BAD_REQUEST),
    }
}

pub(super) const DESIGN_TOOLS: [&str; 4] = [
    "design_system_action",
    "design_system_edit",
    "design_record",
    "project_create",
];

pub(super) fn design_tools() -> [Value; 4] {
    [
        json!({"name":"design_system_action","description":"Create, vary, validate, submit, review or adopt a design system version",
        "inputSchema":{"type":"object","required":["project","record"],"additionalProperties":false,"properties":{
            "project":{"type":"string"},"record":{"type":"object","required":["action"],"additionalProperties":false,"properties":{
                "action":{"enum":["create","variation","validate","submit","comment","approve","changes","adopt"]},
                "version":{"type":"string"},"generation":{"type":"integer"},"target":{"type":"string"},"note":{"type":"string"}
            }}
        }}}),
        json!({"name":"design_system_edit","description":"Patch a draft document using the ID and generation returned by design_system_get",
        "inputSchema":{"type":"object","required":["project","id","generation","changes"],"additionalProperties":false,"properties":{
            "project":{"type":"string"},"id":{"type":"string"},"generation":{"type":"integer"},"changes":{"type":"object","minProperties":1}
        }}}),
        json!({"name":"project_create","description":"Register a project with its ID and label",
            "inputSchema":{"type":"object","required":["project","label"],"additionalProperties":false,
                "properties":{"project":{"type":"string"},"label":{"type":"string"}}}}),
        json!({"name":"design_record","description":"Record a brief, direction, tactic or decision",
            "inputSchema":{"type":"object","required":["project","record"],"additionalProperties":false,"properties":{
                "project":{"type":"string"},"record":{"type":"object","required":["kind","status","title","note"],"additionalProperties":false,
                "properties":{"kind":{"enum":["brief","direction","tactic","decision"]},"status":{"enum":["proposed","adopted","rejected"]},
                    "title":{"type":"string"},"note":{"type":"string"},"evidence":{"type":"string"},
                    "reference":{"type":"string"},"surface":{"type":"string"},"supersedes":{"type":"string"}}}}}}),
    ]
}

pub(super) async fn design(
    panel: &Panel,
    actor: &str,
    tool: &str,
    args: &Value,
) -> PageResult<Value> {
    let project = args
        .get("project")
        .and_then(Value::as_str)
        .filter(|id| safe_name(id) && id.len() <= 100)
        .ok_or(StatusCode::BAD_REQUEST)?;
    if tool == "project_create" {
        let label = args
            .get("label")
            .and_then(Value::as_str)
            .filter(|_| args.as_object().is_some_and(|obj| obj.len() == 2))
            .ok_or(StatusCode::BAD_REQUEST)?;
        crate::pages::register_project(panel, project, label).await?;
        return Ok(json!({"id":project}));
    }
    if tool == "design_record" {
        let record = args
            .get("record")
            .and_then(Value::as_object)
            .ok_or(StatusCode::BAD_REQUEST)?;
        if args.as_object().is_none_or(|a| a.len() != 2)
            || record.keys().any(|k| {
                ![
                    "kind",
                    "status",
                    "title",
                    "note",
                    "evidence",
                    "reference",
                    "surface",
                    "supersedes",
                ]
                .contains(&k.as_str())
            })
        {
            return Err(StatusCode::BAD_REQUEST);
        }
        let form = crate::pages::DesignForm::deserialize(&args["record"])
            .map_err(|_| StatusCode::BAD_REQUEST)?;
        let id = crate::pages::record_design_mcp(panel, project, actor, form).await?;
        return Ok(json!({"id":id}));
    }
    {
        let db = panel.index.lock().await;
        require_project(&db, panel, project).await?;
    }
    let response = if tool == "design_system_action" {
        let record = args
            .get("record")
            .and_then(Value::as_object)
            .ok_or(StatusCode::BAD_REQUEST)?;
        if args.as_object().is_none_or(|a| a.len() != 2)
            || record.keys().any(|k| {
                !["action", "version", "generation", "target", "note"].contains(&k.as_str())
            })
        {
            return Err(StatusCode::BAD_REQUEST);
        }
        crate::design_system::mcp_action(panel, actor, project, &args["record"]).await?
    } else {
        if args.as_object().is_none_or(|a| a.len() != 4) {
            return Err(StatusCode::BAD_REQUEST);
        }
        let id = args
            .get("id")
            .and_then(Value::as_str)
            .filter(|id| problems::valid_id(id))
            .ok_or(StatusCode::BAD_REQUEST)?;
        let generation = args
            .get("generation")
            .and_then(Value::as_i64)
            .ok_or(StatusCode::BAD_REQUEST)?;
        crate::design_system::mcp_edit(panel, project, id, generation, &args["changes"]).await?
    };
    let location = response
        .headers()
        .get(http::header::LOCATION)
        .and_then(|value| value.to_str().ok())
        .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;
    let id = location
        .rsplit_once("&version=")
        .map(|(_, id)| id)
        .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;
    crate::design_system::mcp_read(panel, project, Some(id)).await
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ProposalInput {
    title: String,
    kind: crate::proposal_record::Kind,
    #[serde(default)]
    problem_solved: String,
    #[serde(default)]
    rationale: String,
    #[serde(default)]
    prompt: String,
    surface: Option<String>,
    parent_id: Option<String>,
}

pub(super) const CREATES: [&str; 3] = ["problem_create", "fix_create", "propose_create"];

pub(super) fn tools() -> [Value; 3] {
    [
        ("problem_create", "Publish a problem citing Rupicola UI/UX skill rules (reference/common.md, reference/ux/*.md) and optional same-project evidence", json!({
            "title":{"type":"string"}, "diagnosis":{"type":"string"}, "solution":{"type":"string"},
            "severity":{"enum":["critical","major","minor","polish"]}, "evidence":{"type":"array","items":{"type":"string"}},
            "route":{"type":"string"}, "viewport":{"type":"string"}, "theme":{"type":"string"},
            "violated_rules":{"type":"array","description":"Rupicola UI/UX or project rules violated (inferred from diagnosis if omitted)","items":{"type":"object","properties":{
                "scope":{"enum":["global","project"]},"code":{"type":"string"},"title":{"type":"string"}
            },"required":["code","title"],"additionalProperties":false}}
        }), json!(["title","diagnosis","solution","severity"])),
        ("fix_create", "Publish a fix for review with same-project before and after media and applied Rupicola UI/UX skill tactics", json!({
            "title":{"type":"string"}, "summary":{"type":"string"},
            "comparisons":{"type":"array","items":{"type":"object","properties":{
                "label":{"type":"string"},"route":{"type":"string"},"viewport":{"type":"string"},"theme":{"type":"string"},
                "before_url":{"type":"string"},"after_url":{"type":"string"},"diff_url":{"type":"string"},
                "changed_ratio":{"type":"number"}
            },"required":["label","route","viewport","theme","before_url","after_url"],"additionalProperties":false}},
            "problem_ids":{"type":"array","items":{"type":"string"}},
            "tactics":{"type":"array","minItems":1,"description":"Applied Rupicola UI/UX skill tactics (from reference/tactics/*.md or reference/ux/*.md)","items":{"type":"string"}},
            "modified_files":{"type":"array","items":{"type":"string"}}
        }), json!(["title","summary","comparisons","tactics"])),
        ("propose_create", "Create a legacy proposal draft with a stable retry key", json!({
            "title":{"type":"string"},"kind":{"enum":["palette","component","page","layout","application","design-system","motion","other"]},
            "problem_solved":{"type":"string"},"rationale":{"type":"string"},"prompt":{"type":"string"},
            "surface":{"type":"string"},"parent_id":{"type":"string"}
        }), json!(["title","kind"])),
    ].map(|(name, description, properties, required)| json!({
        "name":name,"description":description,"inputSchema":{"type":"object","additionalProperties":false,
            "required":["project","key","record"],"properties":{
                "project":{"type":"string"},"key":{"type":"string","description":"Stable retry key. Reusing it returns the original record."},
                "record":{"type":"object","properties":properties,"required":required,"additionalProperties":false}}}
    }))
}

fn request<'a, T: Deserialize<'a>>(args: &'a Value) -> PageResult<(Request<T>, String)> {
    let input = Request::<T>::deserialize(args).map_err(|_| StatusCode::BAD_REQUEST)?;
    if !safe_name(&input.project) || input.project.len() > 100 || !problems::valid_id(&input.key) {
        return Err(StatusCode::BAD_REQUEST);
    }
    let id = format!("mcp-{}", crate::auth::digest(&input.key));
    Ok((input, id))
}

pub(super) fn evidence_location<'a>(
    panel: &Panel,
    project: &str,
    url: &'a str,
) -> PageResult<(std::path::PathBuf, String, &'a str)> {
    let mut parts = url
        .strip_prefix("/media/")
        .ok_or(StatusCode::BAD_REQUEST)?
        .splitn(3, '/');
    let (Some(root), Some(name), Some(path)) = (parts.next(), parts.next(), parts.next()) else {
        return Err(StatusCode::BAD_REQUEST);
    };
    let index: usize = root.parse().map_err(|_| StatusCode::BAD_REQUEST)?;
    if root != index.to_string() || !ingest::valid_file(path) {
        return Err(StatusCode::BAD_REQUEST);
    }
    let dir = run_dir(panel, index, name).ok_or(StatusCode::NOT_FOUND)?;
    let manifest = read_manifest(&dir)
        .filter(|m| m.project.id == project)
        .ok_or(StatusCode::NOT_FOUND)?;
    let file = media::resolve_artifact(dir, &manifest, path)?;
    Ok((file, format!("{root}/{name}"), path))
}
fn evidence(panel: &Panel, project: &str, url: &str) -> PageResult<problems::EvidenceAsset> {
    let (_, run_key, path) = evidence_location(panel, project, url)?;
    if path.ends_with("json") {
        return Err(StatusCode::BAD_REQUEST);
    }
    Ok(problems::EvidenceAsset {
        run_key,
        asset_path: path.into(),
        url: url.into(),
        kind: if path.ends_with("png") {
            "Screenshot"
        } else {
            "Video"
        }
        .into(),
        label: path.into(),
    })
}

pub(super) async fn require_project(
    db: &turso::Connection,
    panel: &Panel,
    project: &str,
) -> PageResult<()> {
    if crate::design_record::project(&panel.design.join(project))
        .is_ok_and(|meta| meta.id == project)
    {
        return Ok(());
    }
    let mut rows = db
        .query("SELECT 1 FROM projects WHERE id=?", [project])
        .await
        .map_err(db_error)?;
    if rows.next().await.map_err(db_error)?.is_none() {
        return Err(StatusCode::NOT_FOUND);
    }
    Ok(())
}

pub(super) async fn create(
    panel: &Panel,
    author: &str,
    tool: &str,
    args: &Value,
) -> PageResult<Value> {
    let db = panel.index.lock().await;
    if tool == "problem_create" {
        let (req, id) = request::<ProblemInput>(args)?;
        require_project(&db, panel, &req.project).await?;
        publish_problem(panel, author, req, id)
    } else if tool == "propose_create" {
        let (req, id) = request::<ProposalInput>(args)?;
        require_project(&db, panel, &req.project).await?;
        publish_proposal(panel, author, req, id)
    } else {
        let (req, id) = request::<FixInput>(args)?;
        require_project(&db, panel, &req.project).await?;
        publish_fix(panel, author, req, id)
    }
}

fn publish_proposal(
    panel: &Panel,
    author: &str,
    req: Request<ProposalInput>,
    id: String,
) -> PageResult<Value> {
    use crate::proposal_record::{self, Origin, Proposal, Status};
    let root = pipeline::data_root(panel)
        .join(proposal_record::DIRECTORY)
        .join(req.project);
    match proposal_record::read_proposal(&root.join(format!("{id}.json"))) {
        Ok(_) => return Ok(json!({"id":id,"created":false})),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => (),
        Err(err) => return Err(db_error(err)),
    }
    let input = req.record;
    if !nonempty(&input.title) || input.title.len() > 200 {
        return Err(StatusCode::BAD_REQUEST);
    }
    if let Some(parent) = &input.parent_id {
        if !proposal_record::safe_id(parent) {
            return Err(StatusCode::BAD_REQUEST);
        }
        proposal_record::read_proposal(&root.join(format!("{parent}.json")))
            .map_err(|_| StatusCode::NOT_FOUND)?;
    }
    let (_, now) = problems::next_id();
    let proposal = Proposal {
        id,
        parent_id: input.parent_id,
        timestamp_ms: now,
        author: Some(author.into()),
        origin: Origin::Agent,
        kind: input.kind,
        status: Status::Draft,
        title: input.title,
        problem_solved: input.problem_solved,
        rationale: input.rationale,
        prompt: input.prompt,
        surface: input.surface,
        tactics: vec![],
        references: vec![],
        attachments: vec![],
        artifact: None,
        extracted: None,
        comments: vec![],
        dispatched_at_ms: None,
    };
    proposal_record::write_proposal(&root, &proposal).map_err(db_error)?;
    Ok(json!({"id":proposal.id,"created":true}))
}

fn nonempty(value: &str) -> bool {
    !value.trim().is_empty()
}

fn publish_problem(
    panel: &Panel,
    author: &str,
    req: Request<ProblemInput>,
    id: String,
) -> PageResult<Value> {
    let data = pipeline::data_root(panel);
    let project = req.project;
    match problems::load(&data, &project, &id) {
        Ok(_) => return Ok(json!({"id":id,"created":false})),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => (),
        Err(err) => return Err(db_error(err)),
    }
    let p = req.record;
    if ![&p.title, &p.diagnosis, &p.solution]
        .into_iter()
        .all(|value| nonempty(value))
        || p.evidence.len() > 16
        || p.violated_rules.len() > 16
    {
        return Err(StatusCode::BAD_REQUEST);
    }
    let mut violated_rules = Vec::with_capacity(p.violated_rules.len().max(1));
    for rule in p.violated_rules {
        if !nonempty(&rule.code)
            || !nonempty(&rule.title)
            || !matches!(rule.scope.as_str(), "global" | "project")
        {
            return Err(StatusCode::BAD_REQUEST);
        }
        violated_rules.push(problems::ViolatedRule {
            scope: rule.scope,
            code: rule.code.trim().into(),
            title: rule.title.trim().into(),
        });
    }
    if violated_rules.is_empty() {
        let (_, code, title, _) = problems::infer_rule(&format!("{} {}", p.title, p.diagnosis));
        violated_rules.push(problems::ViolatedRule {
            scope: "global".into(),
            code: code.into(),
            title: title.into(),
        });
    }
    let assets = p
        .evidence
        .iter()
        .map(|url| evidence(panel, &project, url))
        .collect::<Result<Vec<_>, _>>()?;
    let (_, now) = problems::next_id();
    let record = problems::Problem {
        id,
        project_id: project,
        title: p.title,
        status: problems::ProblemStatus::Open,
        severity: p.severity,
        origin: problems::ProblemOrigin::AgentVisual,
        route: p.route,
        viewport: p.viewport,
        theme: p.theme,
        diagnosis: p.diagnosis,
        proposed_solution: p.solution,
        violated_rules,
        evidence_assets: assets,
        comments: vec![],
        linked_fix_id: None,
        execution: None,
        author: author.into(),
        created_at_ms: now,
        updated_at_ms: now,
    };
    problems::save(&data, &record).map_err(db_error)?;
    let skill_modules = problems::recommended_skill_modules(&record);
    Ok(json!({
        "id": record.id,
        "created": true,
        "violated_rules": record.violated_rules,
        "skill_modules": skill_modules,
    }))
}

fn publish_fix(
    panel: &Panel,
    author: &str,
    req: Request<FixInput>,
    id: String,
) -> PageResult<Value> {
    let data = pipeline::data_root(panel);
    let project = req.project;
    match fixes::load(&data, &project, &id) {
        Ok(_) => return Ok(json!({"id":id,"created":false})),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => (),
        Err(err) => return Err(db_error(err)),
    }
    let f = req.record;
    let tactics: Vec<String> = f
        .tactics
        .into_iter()
        .map(|t| t.trim().to_owned())
        .filter(|t| !t.is_empty())
        .collect();
    if !nonempty(&f.title)
        || !nonempty(&f.summary)
        || !(1..=16).contains(&f.comparisons.len())
        || tactics.is_empty()
    {
        return Err(StatusCode::BAD_REQUEST);
    }
    for problem in &f.problem_ids {
        problems::load(&data, &project, problem).map_err(|_| StatusCode::NOT_FOUND)?;
    }
    for pair in &f.comparisons {
        evidence(panel, &project, &pair.before_url)?;
        evidence(panel, &project, &pair.after_url)?;
        if let Some(url) = &pair.diff_url {
            evidence(panel, &project, url)?;
        }
    }
    let (_, now) = fixes::next_id();
    let record = fixes::FixRecord {
        id,
        project_id: project,
        problem_ids: f.problem_ids,
        title: f.title,
        summary: f.summary,
        status: fixes::FixStatus::PendingReview,
        tactics,
        modified_files: f.modified_files,
        comparisons: f.comparisons,
        steps: vec![],
        reviews: vec![],
        author: author.into(),
        created_at_ms: now,
        updated_at_ms: now,
    };
    fixes::save(&data, &record).map_err(db_error)?;
    Ok(json!({"id":record.id,"created":true}))
}

pub(super) async fn capture(
    panel: &Panel,
    digest: &str,
    args: &Value,
    permit: tokio::sync::OwnedSemaphorePermit,
) -> PageResult<Value> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Capture {
        project: String,
        bundle: ingest::Bundle,
    }
    let data = Capture::deserialize(args).map_err(|_| StatusCode::BAD_REQUEST)?;
    let (_, bundle) =
        ingest::store_bundle(panel, data.project, digest, data.bundle, permit).await?;
    Ok(bundle.0)
}

pub(super) async fn storage_run_action(panel: &Panel, args: &Value) -> PageResult<Value> {
    #[derive(Deserialize)]
    #[serde(rename_all = "lowercase")]
    enum Action {
        Offload,
        Delete,
    }
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Request {
        project: String,
        run: String,
        action: Action,
    }
    let request = Request::deserialize(args).map_err(|_| StatusCode::BAD_REQUEST)?;
    if !safe_name(&request.project) || !safe_name(&request.run) {
        return Err(StatusCode::BAD_REQUEST);
    }
    let data = pipeline::data_root(panel);
    tokio::task::spawn_blocking(move || {
        use crate::storage;
        let capture = data.join("captures").join(&request.run);
        if !std::fs::symlink_metadata(&capture).is_ok_and(|meta| meta.is_dir())
            || read_manifest(&capture).is_none_or(|manifest| manifest.project.id != request.project)
        {
            return Err(StatusCode::NOT_FOUND);
        }
        let summary = match request.action {
            Action::Offload => storage::offload_run(&data, &request.run).map_err(db_error)?,
            Action::Delete => storage::delete_run(&data, &request.run, false).map_err(|error| {
                match error.kind() {
                    std::io::ErrorKind::PermissionDenied => StatusCode::CONFLICT,
                    std::io::ErrorKind::NotFound => StatusCode::NOT_FOUND,
                    _ => db_error(error),
                }
            })?,
        };
        serde_json::to_value(summary).map_err(db_error)
    })
    .await
    .map_err(db_error)?
}

pub(super) async fn project_rename(panel: &Panel, actor: &str, args: &Value) -> PageResult<Value> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Rename {
        project: String,
        label: String,
    }
    let input = Rename::deserialize(args).map_err(|_| StatusCode::BAD_REQUEST)?;
    let label = input.label.trim();
    if !safe_name(&input.project)
        || input.project.len() > 100
        || label.is_empty()
        || label.len() > 400
    {
        return Err(StatusCode::BAD_REQUEST);
    }
    let db = panel.index.lock().await;
    require_project(&db, panel, &input.project).await?;
    let project = input.project.as_str();
    db.transactional_batch([
        ("INSERT INTO projects (id, label) VALUES (?, ?) ON CONFLICT(id) DO UPDATE SET label=excluded.label", Vec::from(turso::params![project, label])),
        ("INSERT INTO audit(project, actor, event, occurred) VALUES (?, ?, 'Project label changed', ?)", Vec::from(turso::params![project, actor, crate::auth::now()])),
    ], crate::TransactionBehavior::Immediate).await.map_err(db_error)?;
    Ok(json!({"id":project,"label":label}))
}

pub(super) async fn project_access(panel: &Panel, actor: &str, args: &Value) -> PageResult<Value> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Access {
        project: String,
        action: String,
        value: Option<String>,
    }
    let input = Access::deserialize(args).map_err(|_| StatusCode::BAD_REQUEST)?;
    if !safe_name(&input.project) || input.project.len() > 100 {
        return Err(StatusCode::BAD_REQUEST);
    }
    let db = panel.index.lock().await;
    require_project(&db, panel, &input.project).await?;
    let project = input.project.as_str();
    if input.action == "list" && input.value.is_none() {
        let mut members = Vec::new();
        let mut rows = db
            .query(
                "SELECT handle, role FROM members WHERE project=? ORDER BY handle LIMIT 100",
                [project],
            )
            .await
            .map_err(db_error)?;
        while let Some(row) = rows.next().await.map_err(db_error)? {
            members.push(json!({"handle":row.get::<String>(0).map_err(db_error)?, "role":row.get::<String>(1).map_err(db_error)?}));
        }
        let mut devices = Vec::new();
        let mut rows = db.query("SELECT digest, label, created FROM devices WHERE project=? ORDER BY created DESC, digest LIMIT 100", [project]).await.map_err(db_error)?;
        while let Some(row) = rows.next().await.map_err(db_error)? {
            devices.push(json!({"id":row.get::<String>(0).map_err(db_error)?,"label":row.get::<String>(1).map_err(db_error)?,"created":row.get::<i64>(2).map_err(db_error)?}));
        }
        let mut audit = Vec::new();
        let mut rows = db.query("SELECT actor, event, occurred FROM audit WHERE project=? ORDER BY id DESC LIMIT 100", [project]).await.map_err(db_error)?;
        while let Some(row) = rows.next().await.map_err(db_error)? {
            audit.push(json!({"actor":row.get::<String>(0).map_err(db_error)?,"event":row.get::<String>(1).map_err(db_error)?,"occurred":row.get::<i64>(2).map_err(db_error)?}));
        }
        return Ok(json!({"members": members, "devices": devices, "audit": audit}));
    }
    let value = input
        .value
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .ok_or(StatusCode::BAD_REQUEST)?;
    let secret =
        crate::pages::update_project_access(panel, &db, actor, project, &input.action, value)
            .await?;
    if input.action.starts_with("revoke-") {
        Ok(json!({"action":input.action}))
    } else {
        Ok(json!({"action":input.action,"credential":secret}))
    }
}

pub(super) fn storage_config(panel: &Panel, args: &Value) -> PageResult<Value> {
    use crate::storage;
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Config {
        config: Option<storage::BucketConfig>,
    }
    let input = Config::deserialize(args).map_err(|_| StatusCode::BAD_REQUEST)?;
    let data = pipeline::data_root(panel);
    let current = storage::load_config(&data);
    let Some(config) = input.config else {
        return serde_json::to_value(current).map_err(db_error);
    };
    let valid_url = |value: &str| {
        value.is_empty()
            || url::Url::parse(value).is_ok_and(|url| {
                matches!(url.scheme(), "http" | "https")
                    && url.host().is_some()
                    && url.username().is_empty()
                    && url.password().is_none()
                    && url.query().is_none()
                    && url.fragment().is_none()
            })
    };
    let valid_bucket = match config.provider.as_str() {
        "s3-compatible" => config.bucket.is_empty() || config.bucket.split('.').all(safe_name),
        "rclone" => config.bucket.is_empty() || safe_name(config.bucket.trim_end_matches(':')),
        "filesystem-bucket" => {
            current.provider == config.provider && current.bucket == config.bucket
        }
        _ => false,
    };
    if !valid_bucket
        || config.keep_latest_runs > 100
        || !matches!(
            config.auto_policy.as_str(),
            "manual" | "on-fix-approved" | "after-capture"
        )
        || !valid_url(&config.endpoint_url)
        || !valid_url(&config.public_base_url)
        || config
            .prefix
            .split('/')
            .any(|part| !part.is_empty() && !safe_name(part))
    {
        return Err(StatusCode::BAD_REQUEST);
    }
    storage::save_config(&data, &config).map_err(db_error)?;
    serde_json::to_value(config).map_err(db_error)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retries_use_the_same_record_id() {
        let mut input = json!({
            "project": "design", "key": "review-12",
            "record": {"title":"Overflow","diagnosis":"Long title","solution":"Wrap","severity":"major"}
        });
        let (_, first) = request::<ProblemInput>(&input).unwrap();
        let (_, retry) = request::<ProblemInput>(&input).unwrap();
        assert_eq!(first, retry);
        assert!(problems::valid_id(&first));
        input["key"] = json!("review-13");
        assert_ne!(first, request::<ProblemInput>(&input).unwrap().1);
        input["project"] = json!("../other");
        assert!(matches!(
            request::<ProblemInput>(&input),
            Err(StatusCode::BAD_REQUEST)
        ));
    }

    #[test]
    fn problem_infers_skill_rules_and_fix_requires_tactics() {
        let inferred = problems::infer_rule("Horizontal overflow on 375px viewport");
        assert_eq!(inferred.1, "layout.md#no-horizontal-overflow");

        let problem = problems::infer_from_finding(
            "design",
            "run-1",
            "[/ @ 375×812 dark] target-size too small on submit button",
            None,
        );
        let modules = problems::recommended_skill_modules(&problem);
        assert!(modules.contains(&"reference/common.md"));
        assert!(modules.contains(&"reference/ux/accessibility.md"));
        assert!(modules.contains(&"reference/tactics/accessibility.md"));

        let fix_input = json!({
            "project": "design",
            "key": "fix-1",
            "record": {
                "title": "Enlarge touch target",
                "summary": "Set min-block-size to 24px",
                "comparisons": [{
                    "label": "Submit",
                    "route": "/",
                    "viewport": "375×812",
                    "theme": "dark",
                    "before_url": "/media/0/run/before.png",
                    "after_url": "/media/0/run/after.png"
                }],
                "tactics": ["   "]
            }
        });
        let (parsed, _) = request::<FixInput>(&fix_input).unwrap();
        let non_empty_tactics: Vec<String> = parsed
            .record
            .tactics
            .into_iter()
            .map(|t| t.trim().to_owned())
            .filter(|t| !t.is_empty())
            .collect();
        assert_eq!(non_empty_tactics, Vec::<String>::new());
    }
}
