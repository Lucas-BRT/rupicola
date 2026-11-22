use crate::{PageResult, Panel, StatusCode, db_error, pipeline, safe_name};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::BTreeSet,
    fs, io,
    path::{Path, PathBuf},
};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Projects {
    source: String,
    target: String,
    #[serde(default)]
    revoke_source_access: bool,
}

fn parse(args: &Value) -> PageResult<Projects> {
    let projects = Projects::deserialize(args).map_err(|_| StatusCode::BAD_REQUEST)?;
    let source = projects.source.as_str();
    let target = projects.target.as_str();
    if source == target
        || [source, target]
            .iter()
            .any(|id| !safe_name(id) || id.len() > 100)
    {
        return Err(StatusCode::BAD_REQUEST);
    }

    Ok(projects)
}

pub(super) async fn preview(panel: &Panel, args: &Value) -> PageResult<Value> {
    let projects = parse(args)?;
    let source = projects.source.as_str();
    let target = projects.target.as_str();
    let db = panel.index.lock().await;
    for project in [source, target] {
        super::publish::require_project(&db, panel, project).await?;
    }
    let mut database = serde_json::Map::new();
    for (name, sql) in [
        ("members", "SELECT count(*) FROM members WHERE project=?"),
        ("devices", "SELECT count(*) FROM devices WHERE project=?"),
        ("invites", "SELECT count(*) FROM invites WHERE project=?"),
        ("audit_events", "SELECT count(*) FROM audit WHERE project=?"),
        (
            "captures",
            "SELECT count(*) FROM captures WHERE project_id=?",
        ),
        (
            "design_versions",
            "SELECT count(*) FROM design_versions_v1 WHERE project=?",
        ),
        (
            "design_adoptions",
            "SELECT count(*) FROM design_adoption_v1 WHERE project=?",
        ),
    ] {
        let mut counts = [0; 2];
        for (index, project) in [source, target].into_iter().enumerate() {
            let mut rows = db.query(sql, [project]).await.map_err(db_error)?;
            if let Some(row) = rows.next().await.map_err(db_error)? {
                counts[index] = row.get::<i64>(0).map_err(db_error)?;
            }
        }
        database.insert(
            name.to_owned(),
            json!({"source":counts[0],"target":counts[1]}),
        );
    }
    let mut database_conflicts = serde_json::Map::new();
    for (name, sql) in [
        (
            "members",
            "SELECT count(*) FROM members a JOIN members b ON a.handle=b.handle WHERE a.project=? AND b.project=?",
        ),
        (
            "design_versions",
            "SELECT count(*) FROM design_versions_v1 a JOIN design_versions_v1 b ON a.id=b.id WHERE a.project=? AND b.project=?",
        ),
        (
            "design_adoptions",
            "SELECT count(*) FROM design_adoption_v1 a CROSS JOIN design_adoption_v1 b WHERE a.project=? AND b.project=?",
        ),
    ] {
        let mut rows = db.query(sql, [source, target]).await.map_err(db_error)?;
        if let Some(row) = rows.next().await.map_err(db_error)? {
            database_conflicts.insert(name.to_owned(), json!(row.get::<i64>(0).map_err(db_error)?));
        }
    }
    drop(db);

    let data = pipeline::data_root(panel);
    let design = panel.design.clone();
    let captures = [0, 1].map(|index| {
        database["captures"][["source", "target"][index]]
            .as_i64()
            .unwrap_or_default()
    });
    let inventory = tokio::task::spawn_blocking(move || files(&projects, &data, &design, captures))
        .await
        .map_err(db_error)?
        .map_err(db_error)?;
    Ok(
        json!({"mode":"preview","inventory":inventory,"database":database,"database_conflicts":database_conflicts}),
    )
}

pub(super) async fn apply(panel: &Panel, actor: &str, args: &Value) -> PageResult<Value> {
    let projects = parse(args)?;
    let source = projects.source.as_str();
    let target = projects.target.as_str();
    let db = panel.index.lock().await;
    let path = pipeline::data_root(panel).join(".project-merge.json");
    let pending = read_journal(&path)?;
    if pending.as_ref().is_some_and(|journal| {
        journal.source != source
            || journal.target != target
            || journal.revoke_source_access != projects.revoke_source_access
    }) {
        return Err(StatusCode::CONFLICT);
    }
    let label = project_label(&db, target)
        .await?
        .ok_or(StatusCode::NOT_FOUND)?;
    if project_label(&db, source).await?.is_none() {
        if pending.is_none() {
            return Err(StatusCode::NOT_FOUND);
        }
        fs::remove_file(path).map_err(db_error)?;
        return Ok(json!({"mode":"applied","source":source,"target":target}));
    }
    let data = pipeline::data_root(panel);
    check_record_moves(&projects, &data, &panel.design, pending.is_some())?;
    if !can_merge_database(&db, source, target, projects.revoke_source_access).await? {
        return Err(StatusCode::CONFLICT);
    }
    let journal = pending.unwrap_or_else(|| MergeJournal {
        source: source.to_owned(),
        target: target.to_owned(),
        revoke_source_access: projects.revoke_source_access,
    });
    write_json(&path, &journal).map_err(db_error)?;
    move_records(&projects, &data, &panel.design, &label)?;
    commit_database_merge(&db, source, target, actor, projects.revoke_source_access).await?;
    fs::remove_file(path).map_err(db_error)?;
    Ok(json!({"mode":"applied","source":source,"target":target}))
}

async fn can_merge_database(
    db: &turso::Connection,
    source: &str,
    target: &str,
    revoke_source_access: bool,
) -> PageResult<bool> {
    let mut rows = db
        .query(
            "SELECT 1 WHERE (?3=0 AND (EXISTS(SELECT 1 FROM invites WHERE project=?1)
            OR EXISTS(SELECT 1 FROM devices WHERE project=?1)
            OR EXISTS(SELECT 1 FROM members a WHERE a.project=?1 AND NOT EXISTS(
                SELECT 1 FROM members b WHERE b.project=?2 AND b.handle=a.handle AND b.role=a.role))))
            OR EXISTS(SELECT 1 FROM design_versions_v1 a JOIN design_versions_v1 b ON a.id=b.id
                WHERE a.project=?1 AND b.project=?2)
            OR EXISTS(SELECT 1 FROM design_adoption_v1 a CROSS JOIN design_adoption_v1 b
                WHERE a.project=?1 AND b.project=?2)",
            turso::params![source, target, i64::from(revoke_source_access)],
        )
        .await
        .map_err(db_error)?;
    Ok(rows.next().await.map_err(db_error)?.is_none())
}

async fn commit_database_merge(
    db: &turso::Connection,
    source: &str,
    target: &str,
    actor: &str,
    revoke_source_access: bool,
) -> PageResult<()> {
    db.transactional_batch(
        [
            ("UPDATE captures SET project_id=?1, project_label=(SELECT label FROM projects WHERE id=?1) WHERE project_id=?2",
                Vec::from(turso::params![target, source])),
            (
                "UPDATE artifacts SET project_id=? WHERE project_id=?",
                Vec::from(turso::params![target, source]),
            ),
            (
                "UPDATE design_versions_v1 SET project=? WHERE project=?",
                Vec::from(turso::params![target, source]),
            ),
            (
                "UPDATE design_adoption_v1 SET project=? WHERE project=?",
                Vec::from(turso::params![target, source]),
            ),
            (
                "UPDATE audit SET project=? WHERE project=?",
                Vec::from(turso::params![target, source]),
            ),
            (
                "DELETE FROM devices WHERE project=?1 AND ?2=1",
                Vec::from(turso::params![source, i64::from(revoke_source_access)]),
            ),
            (
                "DELETE FROM invites WHERE project=?1 AND ?2=1",
                Vec::from(turso::params![source, i64::from(revoke_source_access)]),
            ),
            (
                "DELETE FROM members WHERE project=?",
                Vec::from(turso::params![source]),
            ),
            (
                "DELETE FROM projects WHERE id=?",
                Vec::from(turso::params![source]),
            ),
            (
                "INSERT INTO audit(project, actor, event, occurred) VALUES (?, ?, ?, ?)",
                Vec::from(turso::params![
                    target,
                    actor,
                    if revoke_source_access {
                        format!("Merged project {source} and revoked source access")
                    } else {
                        format!("Merged project {source}")
                    },
                    crate::auth::now()
                ]),
            ),
        ],
        crate::TransactionBehavior::Immediate,
    )
    .await
    .map_err(db_error)?;
    Ok(())
}

/// Marks a merge in progress so an interrupted record move can resume.
#[derive(Deserialize, Serialize)]
struct MergeJournal {
    source: String,
    target: String,
    #[serde(default)]
    revoke_source_access: bool,
}

fn write_json(path: &Path, value: &impl Serialize) -> io::Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| io::Error::other("Missing parent directory"))?;
    let mut tmp = tempfile::NamedTempFile::new_in(parent)?;
    serde_json::to_writer(&mut tmp, value).map_err(io::Error::other)?;
    tmp.as_file().sync_all()?;
    tmp.persist(path).map_err(|error| error.error)?;
    fs::File::open(parent)?.sync_all()
}

fn read_journal(path: &Path) -> PageResult<Option<MergeJournal>> {
    match fs::read(path) {
        Ok(bytes) => serde_json::from_slice(&bytes).map(Some).map_err(db_error),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(db_error(error)),
    }
}

async fn project_label(db: &turso::Connection, project: &str) -> PageResult<Option<String>> {
    let mut rows = db
        .query("SELECT label FROM projects WHERE id=?", [project])
        .await
        .map_err(db_error)?;
    rows.next()
        .await
        .map_err(db_error)?
        .map(|row| row.get(0).map_err(db_error))
        .transpose()
}

fn record_roots(data: &Path, design: &Path) -> [(&'static str, PathBuf); 4] {
    [
        ("problems", data.join(crate::problem_record::DIRECTORY)),
        ("fixes", data.join(crate::fix_record::DIRECTORY)),
        ("proposals", data.join(crate::proposal_record::DIRECTORY)),
        ("design", design.to_path_buf()),
    ]
}

fn check_record_moves(
    projects: &Projects,
    data: &Path,
    design: &Path,
    resuming: bool,
) -> PageResult<()> {
    for (kind, root) in record_roots(data, design) {
        let source = root.join(&projects.source);
        let target = root.join(&projects.target);
        for name in record_paths(kind, &source)? {
            check_target_path(&target, &name, !resuming)?;
        }
    }
    Ok(())
}

fn move_records(projects: &Projects, data: &Path, design: &Path, label: &str) -> PageResult<()> {
    use std::io::Write as _;
    for (kind, root) in record_roots(data, design) {
        let source = root.join(&projects.source);
        let target = root.join(&projects.target);
        for name in record_paths(kind, &source)? {
            check_target_path(&target, &name, false)?;
            let path = source.join(&name);
            let output = target.join(&name);
            let raw = fs::read(&path).map_err(db_error)?;
            let bytes = match kind {
                "problems" | "fixes" => rewrite_record(&raw, "project_id", projects, None)?,
                "design" if name == Path::new("project.json") => {
                    rewrite_record(&raw, "id", projects, Some(label))?
                }
                "proposals" if name.starts_with(crate::proposal_record::DISPATCH_DIR) => {
                    rewrite_dispatch(&raw, projects)?
                }
                _ => raw,
            };
            let parent = output.parent().ok_or(StatusCode::CONFLICT)?;
            fs::create_dir_all(parent).map_err(db_error)?;
            let mut staged = tempfile::NamedTempFile::new_in(parent).map_err(db_error)?;
            staged.write_all(&bytes).map_err(db_error)?;
            staged.as_file().sync_all().map_err(db_error)?;
            if let Err(error) = staged.persist_noclobber(&output) {
                if error.error.kind() != io::ErrorKind::AlreadyExists {
                    return Err(db_error(error.error));
                }
                if !fs::symlink_metadata(&output).is_ok_and(|meta| meta.is_file())
                    || fs::read(&output).map_err(db_error)? != bytes
                {
                    return Err(StatusCode::CONFLICT);
                }
            }
            fs::File::open(parent)
                .and_then(|dir| dir.sync_all())
                .map_err(db_error)?;
            fs::remove_file(&path).map_err(db_error)?;
            fs::File::open(path.parent().ok_or(StatusCode::CONFLICT)?)
                .and_then(|dir| dir.sync_all())
                .map_err(db_error)?;
        }
    }
    Ok(())
}

fn record_paths(kind: &str, root: &Path) -> PageResult<BTreeSet<PathBuf>> {
    let mut pending = vec![PathBuf::new()];
    let mut paths = BTreeSet::new();
    while let Some(relative) = pending.pop() {
        for filename in inventory(&root.join(&relative)).map_err(db_error)? {
            let path = relative.join(filename);
            let metadata = fs::symlink_metadata(root.join(&path)).map_err(db_error)?;
            if kind == "proposals" && metadata.is_dir() && proposal_directory(&path) {
                pending.push(path);
                continue;
            }
            let asset = kind == "proposals"
                && path.components().count() == 3
                && path
                    .parent()
                    .and_then(Path::file_name)
                    .and_then(|part| part.to_str())
                    == Some("assets");
            if !metadata.is_file()
                || metadata.len() > 16 * 1024 * 1024
                || (!asset && path.extension().and_then(|ext| ext.to_str()) != Some("json"))
                || (kind == "proposals"
                    && path == Path::new(crate::proposal_record::MCP_SESSION_FILE))
            {
                return Err(StatusCode::CONFLICT);
            }
            paths.insert(path);
        }
    }
    Ok(paths)
}

fn proposal_directory(path: &Path) -> bool {
    let mut pieces = path.iter();
    let Some(first) = pieces.next().and_then(|part| part.to_str()) else {
        return false;
    };
    match (pieces.next(), pieces.next()) {
        (None, None) => crate::proposal_record::safe_id(first),
        (Some(assets), None) => {
            crate::proposal_record::safe_id(first)
                && first != crate::proposal_record::DISPATCH_DIR
                && assets == "assets"
        }
        _ => false,
    }
}

fn check_target_path(root: &Path, file: &Path, must_be_new: bool) -> PageResult<()> {
    for parent in file.ancestors().skip(1) {
        match fs::symlink_metadata(root.join(parent)) {
            Ok(metadata) if !metadata.is_dir() => return Err(StatusCode::CONFLICT),
            Ok(_) => (),
            Err(error) if error.kind() == io::ErrorKind::NotFound => (),
            Err(error) => return Err(db_error(error)),
        }
    }
    match fs::symlink_metadata(root.join(file)) {
        Ok(metadata) if must_be_new || !metadata.is_file() => Err(StatusCode::CONFLICT),
        Ok(_) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(db_error(error)),
    }
}

fn rewrite_dispatch(raw: &[u8], projects: &Projects) -> PageResult<Vec<u8>> {
    let rewritten = rewrite_record(raw, "project_id", projects, None)?;
    let mut value: Value = serde_json::from_slice(&rewritten).map_err(db_error)?;
    if let Some(prompt) = value["structured_prompt"].as_str() {
        let header = prompt.lines().next().unwrap_or_default();
        let old = format!(" do projeto `{}`", projects.source);
        if let Some(prefix) = header.strip_suffix(&old) {
            let suffix = &prompt[header.len()..];
            value["structured_prompt"] =
                json!(format!("{prefix} do projeto `{}`{suffix}", projects.target));
        }
    }
    serde_json::to_vec(&value).map_err(db_error)
}

fn rewrite_record(
    raw: &[u8],
    field: &str,
    projects: &Projects,
    label: Option<&str>,
) -> PageResult<Vec<u8>> {
    let mut value: Value = serde_json::from_slice(raw).map_err(db_error)?;
    if value[field] != projects.source {
        return Err(StatusCode::CONFLICT);
    }
    value[field] = json!(projects.target);
    if let Some(label) = label {
        value["label"] = json!(label);
    }
    serde_json::to_vec(&value).map_err(db_error)
}

fn files(projects: &Projects, data: &Path, design: &Path, captures: [i64; 2]) -> io::Result<Value> {
    let mut sections = serde_json::Map::new();
    for (name, root) in record_roots(data, design) {
        let source = inventory(&root.join(&projects.source))?;
        let target = inventory(&root.join(&projects.target))?;
        let collisions = source.intersection(&target);
        sections.insert(
            name.to_owned(),
            json!({
                "source_entries":source.len(),"target_entries":target.len(),
                "collision_count":collisions.clone().count(),
                "collision_examples":collisions.take(25).collect::<Vec<_>>(),
            }),
        );
    }
    Ok(
        json!({"source":projects.source,"target":projects.target,"sections":sections,
        "captures":{"source":captures[0],"target":captures[1]}}),
    )
}

fn inventory(path: &Path) -> io::Result<BTreeSet<std::ffi::OsString>> {
    match fs::symlink_metadata(path) {
        Ok(meta) if !meta.is_dir() => Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "project inventory requires a directory",
        )),
        Ok(_) => fs::read_dir(path)?
            .map(|entry| entry.map(|entry| entry.file_name()))
            .collect(),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(BTreeSet::new()),
        Err(error) => Err(error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn merge_moves_proposal_assets_and_retargets_pending_dispatch() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("proposals");
        let source = root.join("old");
        let target = root.join("new");
        fs::create_dir_all(source.join("proposal/assets")).unwrap();
        fs::create_dir_all(source.join("dispatch")).unwrap();
        fs::create_dir_all(target.join("dispatch")).unwrap();
        fs::write(source.join("proposal/assets/example.png"), b"data").unwrap();
        write_json(
            &source.join("dispatch/request.json"),
            &json!({"project_id":"old","proposal_id":"proposal",
                "structured_prompt":"Processar proposta `proposal` do projeto `old`
Keep context"}),
        )
        .unwrap();
        let projects = Projects {
            source: "old".into(),
            target: "new".into(),
            revoke_source_access: false,
        };
        let design = temp.path().join("design");
        check_record_moves(&projects, temp.path(), &design, false).unwrap();
        move_records(&projects, temp.path(), &design, "New").unwrap();
        let item: Value =
            serde_json::from_slice(&fs::read(target.join("dispatch/request.json")).unwrap())
                .unwrap();
        assert_eq!(item["project_id"], "new");
        assert_eq!(
            item["structured_prompt"],
            "Processar proposta `proposal` do projeto `new`
Keep context"
        );
        assert_eq!(
            fs::read(target.join("proposal/assets/example.png")).unwrap(),
            b"data"
        );
        assert!(!source.join("proposal/assets/example.png").exists());
    }

    #[test]
    fn record_merge_preserves_ids_and_refuses_collisions() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("problems");
        let source = root.join("old");
        let target = root.join("new");
        fs::create_dir_all(&source).unwrap();
        fs::create_dir_all(&target).unwrap();
        let original = json!({"id":"p1","project_id":"old","linked_fix_id":"fix1"});
        write_json(&source.join("p1.json"), &original).unwrap();
        write_json(&target.join("p1.json"), &json!({"id":"different"})).unwrap();
        let projects = Projects {
            source: "old".into(),
            target: "new".into(),
            revoke_source_access: false,
        };
        let design = temp.path().join("design");
        assert_eq!(
            check_record_moves(&projects, temp.path(), &design, false),
            Err(StatusCode::CONFLICT)
        );
        fs::remove_file(target.join("p1.json")).unwrap();
        check_record_moves(&projects, temp.path(), &design, false).unwrap();
        move_records(&projects, temp.path(), &design, "New").unwrap();
        let result: Value =
            serde_json::from_slice(&fs::read(target.join("p1.json")).unwrap()).unwrap();
        assert_eq!(
            result,
            json!({"id":"p1","project_id":"new","linked_fix_id":"fix1"})
        );
        assert!(!source.join("p1.json").exists());
    }

    #[tokio::test]
    async fn capture_merge_moves_captures_and_artifacts_to_the_target() {
        let (dir, db) = crate::db::test_db().await;
        crate::db::migrate(&db).await.unwrap();
        crate::auth::init(&db).await.unwrap();
        crate::design_system::init(&db).await.unwrap();
        db.execute_batch("INSERT INTO projects VALUES ('old','Old'), ('new','New');")
            .await
            .unwrap();
        let blobs = crate::blobs::Blobs::open(dir.path().join("blobs")).unwrap();
        let shot = dir.path().join("main.png");
        fs::write(&shot, b"evidence").unwrap();
        let manifest: rupicola_core::capture_manifest::Manifest = serde_json::from_value(json!({
            "version":1,"project":{"id":"old","label":"Old"},"scenario":"demo",
            "revision":null,"created_at_ms":0,"status":"complete","blocker":null,
            "inspection":"pending","environment":{"renderer":"test"},
            "checkpoints":[{"id":"main","path":"main.png"}],"clips":[]}))
        .unwrap();
        crate::capture_store::insert(
            &db,
            &crate::capture_store::NewCapture {
                id: "c1".into(),
                manifest,
                legacy: None,
                artifacts: vec![crate::capture_store::NewArtifact {
                    role: "checkpoint",
                    owner_id: "main".into(),
                    path: "main.png".into(),
                    stored: blobs.put_file(&shot).unwrap(),
                    target_ms: None,
                    video_ms: None,
                }],
            },
        )
        .await
        .unwrap();
        commit_database_merge(&db, "old", "new", "owner", true)
            .await
            .unwrap();
        for sql in [
            "SELECT project_id || '/' || project_label FROM captures",
            "SELECT project_id FROM artifacts",
        ] {
            let mut rows = db.query(sql, ()).await.unwrap();
            let value: String = rows.next().await.unwrap().unwrap().get(0).unwrap();
            assert!(value == "new/New" || value == "new", "{sql} -> {value}");
        }
    }

    #[tokio::test]
    async fn merge_preserves_target_access_and_rejects_new_access() {
        let db = turso::Builder::new_local(":memory:")
            .build()
            .await
            .unwrap()
            .connect()
            .unwrap();
        crate::auth::init(&db).await.unwrap();
        crate::design_system::init(&db).await.unwrap();
        crate::db::migrate(&db).await.unwrap();
        db.execute_batch(
            "INSERT INTO projects VALUES ('old','Old'), ('new','New');
            INSERT INTO members VALUES ('old','reader','reader');
            INSERT INTO audit(project,actor,event,occurred) VALUES ('old','owner','Original',0);",
        )
        .await
        .unwrap();
        assert!(!can_merge_database(&db, "old", "new", false).await.unwrap());
        db.execute("INSERT INTO members VALUES ('new','reader','reader')", ())
            .await
            .unwrap();
        assert!(can_merge_database(&db, "old", "new", false).await.unwrap());
        commit_database_merge(&db, "old", "new", "owner", false)
            .await
            .unwrap();
        let mut rows = db
            .query("SELECT id FROM projects ORDER BY id", ())
            .await
            .unwrap();
        assert_eq!(
            rows.next()
                .await
                .unwrap()
                .unwrap()
                .get::<String>(0)
                .unwrap(),
            "new"
        );
        assert!(rows.next().await.unwrap().is_none());
        let mut rows = db
            .query("SELECT count(*) FROM audit WHERE project='new'", ())
            .await
            .unwrap();
        assert_eq!(
            rows.next().await.unwrap().unwrap().get::<i64>(0).unwrap(),
            2
        );
    }

    #[tokio::test]
    async fn explicit_revocation_keeps_destination_credentials() {
        let db = turso::Builder::new_local(":memory:")
            .build()
            .await
            .unwrap()
            .connect()
            .unwrap();
        crate::auth::init(&db).await.unwrap();
        crate::design_system::init(&db).await.unwrap();
        crate::db::migrate(&db).await.unwrap();
        db.execute_batch("
            INSERT INTO projects VALUES ('old','Old'), ('new','New');
            INSERT INTO members VALUES ('old','reader','reader'), ('new','reader','reviewer');
            INSERT INTO devices VALUES ('old-token','old','source',0,NULL), ('new-token','new','target',0,NULL);
            INSERT INTO invites VALUES ('old-invite','old','reader',123), ('new-invite','new','reader',123);")
            .await.unwrap();
        assert!(!can_merge_database(&db, "old", "new", false).await.unwrap());
        assert!(can_merge_database(&db, "old", "new", true).await.unwrap());
        commit_database_merge(&db, "old", "new", "owner", true)
            .await
            .unwrap();
        for (query, expected) in [
            "SELECT digest FROM devices",
            "SELECT digest FROM invites",
            "SELECT role FROM members",
        ]
        .into_iter()
        .zip(["new-token", "new-invite", "reviewer"])
        {
            let mut rows = db.query(query, ()).await.unwrap();
            assert_eq!(
                rows.next()
                    .await
                    .unwrap()
                    .unwrap()
                    .get::<String>(0)
                    .unwrap(),
                expected
            );
            assert!(rows.next().await.unwrap().is_none());
        }
    }

    #[test]
    fn preview_counts_collisions_without_changing_files() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("problems");
        let source = root.join("old");
        let target = root.join("new");
        fs::create_dir_all(&source).unwrap();
        fs::create_dir_all(&target).unwrap();
        fs::write(source.join("shared.json"), "original").unwrap();
        fs::write(target.join("shared.json"), "target").unwrap();
        fs::write(source.join("unique.json"), "unique").unwrap();

        let projects = Projects {
            source: "old".into(),
            target: "new".into(),
            revoke_source_access: false,
        };
        let result = files(&projects, temp.path(), &temp.path().join("design"), [0, 0]).unwrap();
        assert_eq!(result["sections"]["problems"]["source_entries"], 2);
        assert_eq!(result["sections"]["problems"]["target_entries"], 1);
        assert_eq!(result["sections"]["problems"]["collision_count"], 1);
        assert_eq!(
            fs::read_to_string(source.join("shared.json")).unwrap(),
            "original"
        );
        assert_eq!(
            fs::read_to_string(target.join("shared.json")).unwrap(),
            "target"
        );
        assert!(!target.join("unique.json").exists());
    }
}
