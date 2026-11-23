//! The capture graph's schema: what the database itself refuses, so no code has to.

use super::{MIGRATOR, open_memory};
use sqlx::{
    AssertSqlSafe, SqlitePool,
    sqlite::{SqliteConnectOptions, SqlitePoolOptions},
};

const DEMO: &str = "demo";
const OTHER: &str = "other";
/// The last migration before the capture graph.
const BEFORE_GRAPH: i64 = 5;
/// The last migration before problem evidence became selections.
const BEFORE_EVIDENCE_MOVED: i64 = 6;
/// The last migration before fix comparisons also became selections.
const BEFORE_COMPARISONS_MOVED: i64 = 7;

fn sha(digit: char) -> String {
    digit.to_string().repeat(64)
}

async fn run(db: &SqlitePool, sql: &str) {
    sqlx::raw_sql(AssertSqlSafe(sql.to_owned()))
        .execute(db)
        .await
        .unwrap_or_else(|error| panic!("{sql}\n{error}"));
}

/// The error text of a statement the database must refuse.
async fn refused(db: &SqlitePool, sql: &str) -> String {
    sqlx::raw_sql(AssertSqlSafe(sql.to_owned()))
        .execute(db)
        .await
        .err()
        .unwrap_or_else(|| panic!("accepted: {sql}"))
        .to_string()
}

async fn count(db: &SqlitePool, table: &str) -> i64 {
    sqlx::query_scalar(AssertSqlSafe(format!("SELECT count(*) FROM {table}")))
        .fetch_one(db)
        .await
        .unwrap()
}

fn capture(id: &str, project: &str) -> String {
    format!(
        "INSERT INTO captures (id, project_id, scenario, status, inspection, environment, created_at)
         VALUES ('{id}', '{project}', 'Home', 'complete', 'pending', '{{}}', 0);"
    )
}

fn artifact(id: &str, capture: &str, path: &str, digit: char) -> String {
    format!(
        "INSERT INTO artifacts (id, capture_id, role, owner_id, position, path, sha256, bytes, mime)
         VALUES ('{id}', '{capture}', 'checkpoint', 'home', 0, '{path}', '{}', 10, 'image/png');",
        sha(digit)
    )
}

fn problem(id: &str, project: &str) -> String {
    format!(
        "INSERT INTO problems (id, project_id, title, status, severity, origin, route, viewport,
           theme, diagnosis, proposed_solution, created_at, updated_at)
         VALUES ('{id}', '{project}', 'Low contrast', 'open', 'major', 'user-reported', '', '', '',
           '', '', 0, 0);"
    )
}

/// Two projects, three captures in the first and one in the second, each with a file.
async fn seeded() -> SqlitePool {
    let db = open_memory().await;
    run(
        &db,
        &format!(
            "INSERT INTO projects VALUES ('{DEMO}', 'Demo', 0);
             INSERT INTO projects VALUES ('{OTHER}', 'Other', 0);
             {} {} {} {}
             {} {} {} {}",
            capture("c1", DEMO),
            capture("c2", DEMO),
            capture("c3", DEMO),
            capture("c9", OTHER),
            artifact("a1", "c1", "checkpoints/home.png", 'a'),
            artifact("a2", "c2", "checkpoints/home.png", 'b'),
            artifact("a3", "c3", "checkpoints/home.png", 'c'),
            artifact("a9", "c9", "checkpoints/home.png", 'd'),
        ),
    )
    .await;
    db
}

#[tokio::test]
async fn blobs_are_filled_in_from_the_files_already_stored() {
    let options = SqliteConnectOptions::new()
        .in_memory(true)
        .foreign_keys(true);
    let db = SqlitePoolOptions::new()
        .min_connections(1)
        .max_connections(1)
        .idle_timeout(None)
        .max_lifetime(None)
        .connect_with(options)
        .await
        .unwrap();
    MIGRATOR.run_to(BEFORE_GRAPH, &db).await.unwrap();
    run(
        &db,
        &format!(
            "INSERT INTO projects VALUES ('{DEMO}', 'Demo', 0);
             INSERT INTO projects VALUES ('{OTHER}', 'Other', 0);
             {} {} {} {} {}",
            capture("c1", DEMO),
            capture("c9", OTHER),
            artifact("a1", "c1", "checkpoints/home.png", 'a'),
            artifact("a2", "c1", "checkpoints/menu.png", 'b'),
            artifact("a9", "c9", "checkpoints/home.png", 'a'),
        ),
    )
    .await;
    MIGRATOR.run(&db).await.unwrap();
    assert_eq!(count(&db, "blobs").await, 2, "one row per digest");
    assert_eq!(
        count(&db, "project_blobs").await,
        3,
        "each project knows the digests it holds, including a shared one"
    );
    let known: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM project_blobs WHERE project_id = ?1 AND sha256 = ?2",
    )
    .bind(OTHER)
    .bind(sha('b'))
    .fetch_one(&db)
    .await
    .unwrap();
    assert_eq!(known, 0, "a project does not know another project's blob");
}

#[tokio::test]
async fn problem_evidence_becomes_a_selection_the_problem_cites() {
    let options = SqliteConnectOptions::new()
        .in_memory(true)
        .foreign_keys(true);
    let db = SqlitePoolOptions::new()
        .min_connections(1)
        .max_connections(1)
        .idle_timeout(None)
        .max_lifetime(None)
        .connect_with(options)
        .await
        .unwrap();
    MIGRATOR.run_to(BEFORE_EVIDENCE_MOVED, &db).await.unwrap();
    run(
        &db,
        &format!(
            "INSERT INTO projects VALUES ('{DEMO}', 'Demo', 0);
             {} {} {} {}
             INSERT INTO problem_evidence VALUES ('p1', 'a1');
             INSERT INTO problem_evidence VALUES ('p1', 'a2');",
            capture("c1", DEMO),
            artifact("a1", "c1", "checkpoints/home.png", 'a'),
            artifact("a2", "c1", "checkpoints/menu.png", 'b'),
            problem("p1", DEMO),
        ),
    )
    .await;
    MIGRATOR.run(&db).await.unwrap();
    let cited: Vec<(String, String, String)> = sqlx::query_as(
        "SELECT s.label, i.artifact_id, i.mark FROM problem_selections ps
         JOIN selections s ON s.id = ps.selection_id
         JOIN selection_items i ON i.selection_id = s.id
         WHERE ps.problem_id = 'p1' ORDER BY i.position",
    )
    .fetch_all(&db)
    .await
    .unwrap();
    let proof = |artifact: &str| {
        (
            "Evidence".to_owned(),
            artifact.to_owned(),
            "proof".to_owned(),
        )
    };
    assert_eq!(cited, [proof("a1"), proof("a2")]);
    let gone: Option<String> = sqlx::query_scalar(
        "SELECT name FROM sqlite_master WHERE type = 'table' AND name = 'problem_evidence'",
    )
    .fetch_optional(&db)
    .await
    .unwrap();
    assert_eq!(gone, None);
}

#[tokio::test]
async fn fix_comparisons_become_a_baseline_and_a_proof_selection() {
    let options = SqliteConnectOptions::new()
        .in_memory(true)
        .foreign_keys(true);
    let db = SqlitePoolOptions::new()
        .min_connections(1)
        .max_connections(1)
        .idle_timeout(None)
        .max_lifetime(None)
        .connect_with(options)
        .await
        .unwrap();
    MIGRATOR
        .run_to(BEFORE_COMPARISONS_MOVED, &db)
        .await
        .unwrap();
    run(
        &db,
        &format!(
            "INSERT INTO projects VALUES ('{DEMO}', 'Demo', 0);
             {} {} {} {}
             INSERT INTO fixes VALUES ('f1', '{DEMO}', 'Raise contrast', '', 'pending-review', NULL, 0, 0);
             INSERT INTO fix_comparisons (id, fix_id, position, label, route, viewport, theme,
               before_artifact_id, after_artifact_id, diff_artifact_id)
             VALUES ('k1', 'f1', 0, 'home', '', '', '', 'a1', 'a2', 'a3');",
            capture("c1", DEMO),
            artifact("a1", "c1", "checkpoints/before.png", 'a'),
            artifact("a2", "c1", "checkpoints/after.png", 'b'),
            artifact("a3", "c1", "checkpoints/diff.png", 'c'),
        ),
    )
    .await;
    MIGRATOR.run(&db).await.unwrap();
    let cited: Vec<(String, String, String)> = sqlx::query_as(
        "SELECT fs.role, s.label, i.artifact_id FROM fix_selections fs
         JOIN selections s ON s.id = fs.selection_id
         JOIN selection_items i ON i.selection_id = s.id
         WHERE fs.fix_id = 'f1' ORDER BY fs.position, i.position",
    )
    .fetch_all(&db)
    .await
    .unwrap();
    let row = |role: &str, label: &str, artifact: &str| {
        (role.to_owned(), label.to_owned(), artifact.to_owned())
    };
    assert_eq!(
        cited,
        [
            row("baseline", "Before", "a1"),
            row("solution-proof", "After", "a2"),
            row("solution-proof", "After", "a3"),
        ]
    );
}

#[tokio::test]
async fn lineage_refuses_loops_self_parents_and_other_projects() {
    let db = seeded().await;
    run(
        &db,
        "INSERT INTO capture_parents VALUES ('demo', 'c2', 'c1', 'previous', 0, 'record');
         INSERT INTO capture_parents VALUES ('demo', 'c3', 'c2', 'previous', 0, 'record');",
    )
    .await;
    let loop_closed = refused(
        &db,
        "INSERT INTO capture_parents VALUES ('demo', 'c1', 'c3', 'baseline', 0, 'edit')",
    )
    .await;
    assert!(loop_closed.contains("cycle"), "{loop_closed}");
    // The trigger answers before the CHECK does; both refuse a capture as its own parent.
    assert!(
        refused(
            &db,
            "INSERT INTO capture_parents VALUES ('demo', 'c1', 'c1', 'baseline', 0, 'edit')"
        )
        .await
        .contains("cycle")
    );
    assert!(
        refused(
            &db,
            "INSERT INTO capture_parents VALUES ('demo', 'c3', 'c9', 'input', 1, 'edit')"
        )
        .await
        .contains("FOREIGN KEY")
    );
    assert!(
        refused(
            &db,
            "INSERT INTO capture_parents VALUES ('demo', 'c3', 'missing', 'input', 1, 'edit')"
        )
        .await
        .contains("FOREIGN KEY")
    );
    // A capture with two parents is a diamond, not a loop.
    run(
        &db,
        "INSERT INTO capture_parents VALUES ('demo', 'c3', 'c1', 'input', 1, 'edit')",
    )
    .await;
    assert_eq!(count(&db, "capture_parents").await, 3);
}

#[tokio::test]
async fn a_capture_with_children_stays_and_a_childless_one_takes_its_edges_along() {
    let db = seeded().await;
    run(
        &db,
        "INSERT INTO capture_parents VALUES ('demo', 'c2', 'c1', 'previous', 0, 'record');
         INSERT INTO capture_parents VALUES ('demo', 'c3', 'c2', 'previous', 0, 'record');",
    )
    .await;
    assert!(
        refused(&db, "DELETE FROM captures WHERE id = 'c1'")
            .await
            .contains("FOREIGN KEY")
    );
    run(&db, "DELETE FROM captures WHERE id = 'c3'").await;
    assert_eq!(count(&db, "capture_parents").await, 1);
}

#[tokio::test]
async fn topics_are_unique_among_siblings_and_cannot_form_a_loop() {
    let db = seeded().await;
    run(
        &db,
        "INSERT INTO topics (id, project_id, parent_id, name, created_at) VALUES
           ('fix', 'demo', NULL, 'fix', 0),
           ('onb', 'demo', NULL, 'onboarding', 0),
           ('v2', 'demo', 'onb', 'v2', 0),
           ('step1', 'demo', 'v2', 'step1', 0),
           ('other-fix', 'other', NULL, 'Fix', 0);",
    )
    .await;
    for clash in [
        "INSERT INTO topics (id, project_id, parent_id, name, created_at) VALUES ('x', 'demo', NULL, 'FIX', 0)",
        "INSERT INTO topics (id, project_id, parent_id, name, created_at) VALUES ('x', 'demo', 'onb', 'V2', 0)",
    ] {
        assert!(refused(&db, clash).await.contains("UNIQUE"), "{clash}");
    }
    // The same name under another parent is fine.
    run(
        &db,
        "INSERT INTO topics (id, project_id, parent_id, name, created_at) VALUES ('fix-v2', 'demo', 'fix', 'v2', 0)",
    )
    .await;
    for bad in ["a/b", ""] {
        let sql = format!(
            "INSERT INTO topics (id, project_id, parent_id, name, created_at) VALUES ('x', 'demo', NULL, '{bad}', 0)"
        );
        assert!(refused(&db, &sql).await.contains("CHECK"), "{bad:?}");
    }
    let under_itself = refused(
        &db,
        "UPDATE topics SET parent_id = 'step1' WHERE id = 'onb'",
    )
    .await;
    assert!(under_itself.contains("cycle"), "{under_itself}");
    assert!(
        refused(&db, "UPDATE topics SET parent_id = 'v2' WHERE id = 'v2'")
            .await
            .contains("cycle"),
        "a topic cannot be its own parent"
    );
    assert!(
        refused(
            &db,
            "UPDATE topics SET parent_id = 'other-fix' WHERE id = 'v2'"
        )
        .await
        .contains("FOREIGN KEY"),
        "a topic cannot move under another project's"
    );
}

#[tokio::test]
async fn a_subtree_is_one_query_and_moving_a_topic_moves_its_captures_with_it() {
    let db = seeded().await;
    run(
        &db,
        "INSERT INTO topics (id, project_id, parent_id, name, created_at) VALUES
           ('fix', 'demo', NULL, 'fix', 0),
           ('onb', 'demo', NULL, 'onboarding', 0),
           ('v2', 'demo', 'onb', 'v2', 0),
           ('step1', 'demo', 'v2', 'step1', 0);
         INSERT INTO capture_topics VALUES ('demo', 'step1', 'c1', NULL, 0);
         INSERT INTO capture_topics VALUES ('demo', 'fix', 'c2', NULL, 0);",
    )
    .await;
    let under = |root: &'static str| {
        format!(
            "WITH RECURSIVE tree(id) AS (
               SELECT id FROM topics WHERE id = '{root}'
               UNION ALL
               SELECT t.id FROM topics t JOIN tree ON t.parent_id = tree.id)
             SELECT group_concat(capture_id) FROM (
               SELECT DISTINCT ct.capture_id
               FROM capture_topics ct JOIN tree ON ct.topic_id = tree.id
               ORDER BY ct.capture_id)"
        )
    };
    let names = |sql: String| {
        let db = db.clone();
        async move {
            let text: Option<String> = sqlx::query_scalar(AssertSqlSafe(sql))
                .fetch_one(&db)
                .await
                .unwrap();
            text.unwrap_or_default()
        }
    };
    assert_eq!(names(under("onb")).await, "c1");
    assert_eq!(names(under("fix")).await, "c2");
    run(
        &db,
        "UPDATE topics SET parent_id = 'fix', generation = generation + 1 WHERE id = 'v2'",
    )
    .await;
    assert_eq!(names(under("fix")).await, "c1,c2");
    assert_eq!(names(under("onb")).await, "");
    assert!(
        refused(
            &db,
            "INSERT INTO capture_topics VALUES ('demo', 'fix', 'c9', NULL, 0)"
        )
        .await
        .contains("FOREIGN KEY"),
        "a topic cannot hold another project's capture"
    );
    assert!(
        refused(&db, "DELETE FROM topics WHERE id = 'fix'")
            .await
            .contains("FOREIGN KEY"),
        "a topic with children cannot go alone"
    );
    run(&db, "DELETE FROM topics WHERE id IN ('fix', 'v2', 'step1')").await;
    assert_eq!(count(&db, "capture_topics").await, 0);
}

#[tokio::test]
async fn a_selection_cites_only_files_of_its_own_projects_captures() {
    let db = seeded().await;
    run(
        &db,
        "INSERT INTO selections VALUES ('s1', 'demo', 'Menu contrast', NULL, 0);
         INSERT INTO selection_items VALUES ('demo', 's1', 0, 'c1', 'a1', 'proof', '{\"t\":[200,900]}');
         INSERT INTO selection_items VALUES ('demo', 's1', 1, 'c2', NULL, 'context', NULL);",
    )
    .await;
    for (what, sql) in [
        (
            "a file of another capture",
            "INSERT INTO selection_items VALUES ('demo', 's1', 2, 'c1', 'a2', 'proof', NULL)",
        ),
        (
            "a capture of another project",
            "INSERT INTO selection_items VALUES ('demo', 's1', 2, 'c9', 'a9', 'proof', NULL)",
        ),
        (
            "an unknown mark",
            "INSERT INTO selection_items VALUES ('demo', 's1', 2, 'c1', 'a1', 'maybe', NULL)",
        ),
        (
            "a fragment that is not JSON",
            "INSERT INTO selection_items VALUES ('demo', 's1', 2, 'c1', 'a1', 'proof', '{')",
        ),
    ] {
        assert!(!refused(&db, sql).await.is_empty(), "{what}");
    }
    assert_eq!(count(&db, "selection_items").await, 2);
}

#[tokio::test]
async fn problems_and_fixes_cite_selections_of_their_own_project_only() {
    let db = seeded().await;
    run(
        &db,
        &format!(
            "INSERT INTO selections VALUES ('s1', 'demo', 'Proof', NULL, 0);
             {} {}
             INSERT INTO problem_selections VALUES ('demo', 'p1', 's1', 0);",
            problem("p1", DEMO),
            problem("p9", OTHER),
        ),
    )
    .await;
    assert!(
        refused(
            &db,
            "INSERT INTO problem_selections VALUES ('other', 'p9', 's1', 0)"
        )
        .await
        .contains("FOREIGN KEY")
    );
    assert!(
        refused(
            &db,
            "INSERT INTO problem_selections VALUES ('demo', 'p9', 's1', 0)"
        )
        .await
        .contains("FOREIGN KEY")
    );
    run(
        &db,
        "INSERT INTO fixes VALUES ('f1', 'demo', 'Raise contrast', '', 'pending-review', NULL, 0, 0);
         INSERT INTO fix_selections VALUES ('demo', 'f1', 's1', 'solution-proof', 0);",
    )
    .await;
    assert!(
        refused(
            &db,
            "UPDATE fix_selections SET role = 'both' WHERE fix_id = 'f1'"
        )
        .await
        .contains("CHECK")
    );
}

#[tokio::test]
async fn a_cited_capture_cannot_be_deleted_but_can_be_tombstoned_and_a_project_still_goes() {
    let db = seeded().await;
    run(
        &db,
        "INSERT INTO selections VALUES ('s1', 'demo', 'Proof', NULL, 0);
         INSERT INTO selection_items VALUES ('demo', 's1', 0, 'c1', 'a1', 'proof', NULL);",
    )
    .await;
    assert!(
        refused(&db, "DELETE FROM captures WHERE id = 'c1'")
            .await
            .contains("FOREIGN KEY"),
        "something still cites it"
    );
    run(
        &db,
        "UPDATE captures SET deleted_at = 1, deleted_by = 'ana', delete_reason = 'wrong build'
           WHERE id = 'c1';
         UPDATE artifacts SET purged_at = 1 WHERE capture_id = 'c1';",
    )
    .await;
    assert_eq!(
        count(&db, "selection_items").await,
        1,
        "the citation still resolves"
    );
    run(&db, "DELETE FROM projects WHERE id = 'demo'").await;
    for table in [
        "captures WHERE project_id = 'demo'",
        "selection_items",
        "selections",
        "capture_facets",
    ] {
        assert_eq!(count(&db, table).await, 0, "{table}");
    }
    assert_eq!(
        count(&db, "captures").await,
        1,
        "the other project is untouched"
    );
}

#[tokio::test]
async fn facets_settings_and_audit_detail_are_kept_with_their_owners() {
    let db = seeded().await;
    run(
        &db,
        "INSERT INTO capture_facets VALUES ('demo', 'appearance.color_scheme', 'dark', 'c1');
         INSERT INTO capture_facets VALUES ('demo', 'platform.family', 'web', 'c1');
         INSERT INTO server_settings VALUES ('blob_limit', '536870912', 0, 'ana', 0);
         INSERT INTO project_settings VALUES ('demo', 'cited_capture_deletion', '\"confirm\"', 0, 'ana', 0);
         INSERT INTO audit_events VALUES ('e1', 'demo', 'ana', 'Removed a capture', 0, '{\"citations\":1}');",
    )
    .await;
    assert!(
        refused(
            &db,
            "INSERT INTO capture_facets VALUES ('other', 'k', 'v', 'c1')"
        )
        .await
        .contains("FOREIGN KEY"),
        "a facet belongs to its capture's project"
    );
    for bad in [
        "INSERT INTO server_settings VALUES ('x', 'not json', 0, NULL, 0)",
        "INSERT INTO project_settings VALUES ('demo', 'x', '{', 0, NULL, 0)",
        "INSERT INTO audit_events VALUES ('e2', 'demo', 'ana', 'x', 0, '{')",
    ] {
        assert!(refused(&db, bad).await.contains("CHECK"), "{bad}");
    }
    run(&db, "DELETE FROM captures WHERE id = 'c1'").await;
    assert_eq!(count(&db, "capture_facets").await, 0);
    run(&db, "DELETE FROM projects WHERE id = 'demo'").await;
    assert_eq!(count(&db, "project_settings").await, 0);
    assert_eq!(count(&db, "audit_events").await, 0);
}

#[tokio::test]
async fn a_digest_and_a_kind_are_checked() {
    let db = seeded().await;
    assert!(
        refused(&db, "UPDATE captures SET digest = 'short' WHERE id = 'c1'")
            .await
            .contains("CHECK")
    );
    assert!(
        refused(&db, "UPDATE captures SET kind = 'import' WHERE id = 'c1'")
            .await
            .contains("CHECK")
    );
    run(
        &db,
        &format!(
            "UPDATE captures SET digest = '{}', kind = 'derived' WHERE id = 'c1'",
            sha('e')
        ),
    )
    .await;
}

/// The plan SQLite chooses for `sql`, one line per step.
async fn plan(db: &SqlitePool, sql: &str) -> String {
    let rows: Vec<(i64, i64, i64, String)> =
        sqlx::query_as(AssertSqlSafe(format!("EXPLAIN QUERY PLAN {sql}")))
            .fetch_all(db)
            .await
            .unwrap();
    rows.into_iter()
        .map(|(_, _, _, detail)| detail)
        .collect::<Vec<_>>()
        .join("\n")
}

#[tokio::test]
async fn the_questions_the_graph_exists_to_answer_use_indexes() {
    let db = seeded().await;
    for (what, table, sql) in [
        (
            "captures by environment facet",
            "capture_facets",
            "SELECT capture_id FROM capture_facets
             WHERE project_id = 'demo' AND key = 'appearance.color_scheme' AND value = 'dark'",
        ),
        (
            "the line of captures of a scenario",
            "captures",
            "SELECT id FROM captures WHERE project_id = 'demo' AND series = 'Home@x'
             ORDER BY created_at",
        ),
        (
            "the children of a capture",
            "capture_parents",
            "SELECT capture_id FROM capture_parents WHERE parent_id = 'c1'",
        ),
        (
            "who cites a capture",
            "selection_items",
            "SELECT selection_id FROM selection_items WHERE capture_id = 'c1'",
        ),
        (
            "who cites a file",
            "selection_items",
            "SELECT selection_id FROM selection_items WHERE artifact_id = 'a1'",
        ),
        (
            "who compares against a file",
            "fix_comparisons",
            "SELECT fix_id FROM fix_comparisons WHERE before_artifact_id = 'a1'",
        ),
        (
            "the files of a screen",
            "artifacts",
            "SELECT capture_id FROM artifacts WHERE surface_id = 's'",
        ),
        (
            "what a project already holds",
            "project_blobs",
            "SELECT sha256 FROM project_blobs WHERE project_id = 'demo'
             AND sha256 IN ('a', 'b')",
        ),
    ] {
        let steps = plan(&db, sql).await;
        let scans_the_table = steps
            .lines()
            .any(|line| line.starts_with(&format!("SCAN {table}")));
        assert!(!scans_the_table, "{what} scans {table}:\n{steps}");
    }
    // The check must be able to fail: without its index the same question is a scan.
    run(&db, "DROP INDEX idx_selection_items_artifact").await;
    // A different literal, so the connection's cache of prepared statements cannot answer.
    let steps = plan(
        &db,
        "SELECT selection_id FROM selection_items WHERE artifact_id = 'a2'",
    )
    .await;
    assert!(steps.starts_with("SCAN selection_items"), "{steps}");
}
