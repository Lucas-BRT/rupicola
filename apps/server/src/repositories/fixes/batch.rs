use super::{ComparisonRow, FixRow, ReviewRow, StepRow, approved_ids, comparisons_of};
use crate::{
    domain::ApiError,
    repositories::{MAX_LIST_ITEMS, push_in, selections},
};
use rupicola_api::{Fix, FixSelection, Review, Step};
use sqlx::SqlitePool;
use std::collections::HashMap;

#[derive(sqlx::FromRow)]
struct KeyedFixRow {
    id: String,
    #[sqlx(flatten)]
    row: FixRow,
}

#[derive(sqlx::FromRow)]
struct KeyedTextRow {
    fix_id: String,
    value: String,
}

#[derive(sqlx::FromRow)]
struct KeyedComparisonRow {
    fix_id: String,
    #[sqlx(flatten)]
    row: ComparisonRow,
}

#[derive(sqlx::FromRow)]
struct KeyedStepRow {
    fix_id: String,
    #[sqlx(flatten)]
    row: StepRow,
}

#[derive(sqlx::FromRow)]
struct KeyedReviewRow {
    fix_id: String,
    #[sqlx(flatten)]
    row: ReviewRow,
}

#[derive(sqlx::FromRow)]
struct KeyedFixSelectionRow {
    fix_id: String,
    selection_id: String,
    role: String,
}

async fn batch_texts(
    db: &SqlitePool,
    prefix: &str,
    order: &str,
    ids: &[String],
) -> Result<HashMap<String, Vec<String>>, ApiError> {
    let mut qb = sqlx::QueryBuilder::new(prefix);
    push_in(&mut qb, ids);
    qb.push(order);
    let rows: Vec<KeyedTextRow> = qb.build_query_as().fetch_all(db).await?;
    let mut by_id: HashMap<String, Vec<String>> = HashMap::new();
    for item in rows {
        by_id.entry(item.fix_id).or_default().push(item.value);
    }
    Ok(by_id)
}

async fn batch_fix_selections(
    db: &SqlitePool,
    ids: &[String],
) -> Result<HashMap<String, Vec<FixSelection>>, ApiError> {
    use std::collections::HashSet;
    let mut qb = sqlx::QueryBuilder::new(
        "SELECT fix_id, selection_id, role FROM fix_selections WHERE fix_id",
    );
    push_in(&mut qb, ids);
    qb.push(" ORDER BY fix_id, position");
    let sel_rows: Vec<KeyedFixSelectionRow> = qb.build_query_as().fetch_all(db).await?;
    let mut links_by_id: HashMap<String, Vec<(String, String)>> = HashMap::new();
    let mut unique_sel_ids = HashSet::new();
    let mut all_sel_ids = Vec::new();
    for item in sel_rows {
        if unique_sel_ids.insert(item.selection_id.clone()) {
            all_sel_ids.push(item.selection_id.clone());
        }
        links_by_id
            .entry(item.fix_id)
            .or_default()
            .push((item.selection_id, item.role));
    }
    let selections_by_id: HashMap<String, rupicola_api::Selection> = if all_sel_ids.is_empty() {
        HashMap::new()
    } else {
        selections::load(db, &all_sel_ids)
            .await?
            .into_iter()
            .map(|s| (s.id.clone(), s))
            .collect()
    };
    let mut out = HashMap::with_capacity(links_by_id.len());
    for (fix_id, links) in links_by_id {
        let mut list = Vec::with_capacity(links.len());
        for (sid, role_str) in links {
            if let Some(selection) = selections_by_id.get(&sid) {
                let role = role_str.parse().map_err(anyhow::Error::from)?;
                list.push(FixSelection {
                    role,
                    selection: selection.clone(),
                });
            }
        }
        out.insert(fix_id, list);
    }
    Ok(out)
}

async fn batch_comparisons(
    db: &SqlitePool,
    ids: &[String],
) -> Result<HashMap<String, Vec<ComparisonRow>>, ApiError> {
    let mut qb = sqlx::QueryBuilder::new(
        "SELECT fix_id, label, route, viewport, theme, before_artifact_id, after_artifact_id,
                diff_artifact_id, changed_ratio, view, base_revision, revision, applied_fixes
         FROM fix_comparisons WHERE fix_id",
    );
    push_in(&mut qb, ids);
    qb.push(" ORDER BY fix_id, position");
    let rows: Vec<KeyedComparisonRow> = qb.build_query_as().fetch_all(db).await?;
    let mut by_id: HashMap<String, Vec<ComparisonRow>> = HashMap::new();
    for item in rows {
        by_id.entry(item.fix_id).or_default().push(item.row);
    }
    Ok(by_id)
}

/// Lists every fix of `project` in full, newest first, in bounded batch queries.
pub async fn list_details(db: &SqlitePool, project: &str) -> Result<Vec<Fix>, ApiError> {
    let fix_rows: Vec<KeyedFixRow> = sqlx::query_as(
        "SELECT id, project_id, title, summary, git_ref, status, mismatch_override_reason, author, created_at, updated_at
         FROM fixes WHERE project_id = ?1 ORDER BY updated_at DESC, id DESC LIMIT ?2",
    )
    .bind(project)
    .bind(MAX_LIST_ITEMS)
    .fetch_all(db)
    .await?;
    if fix_rows.is_empty() {
        return Ok(Vec::new());
    }
    let ids: Vec<String> = fix_rows.iter().map(|r| r.id.clone()).collect();

    let mut problems_by_id = batch_texts(
        db,
        "SELECT fix_id, problem_id AS value FROM fix_problems WHERE fix_id",
        " ORDER BY fix_id, problem_id",
        &ids,
    )
    .await?;
    let mut tactics_by_id = batch_texts(
        db,
        "SELECT fix_id, tactic AS value FROM fix_tactics WHERE fix_id",
        " ORDER BY fix_id, tactic",
        &ids,
    )
    .await?;
    let mut files_by_id = batch_texts(
        db,
        "SELECT fix_id, path AS value FROM fix_files WHERE fix_id",
        " ORDER BY fix_id, path",
        &ids,
    )
    .await?;

    let mut comparisons_by_id = batch_comparisons(db, &ids).await?;
    let approved = approved_ids(db, project).await?;

    let mut qb = sqlx::QueryBuilder::new(
        "SELECT fix_id, title, detail, verification, evidence_artifact_id, created_at
         FROM fix_steps WHERE fix_id",
    );
    push_in(&mut qb, &ids);
    qb.push(" ORDER BY fix_id, position");
    let step_rows: Vec<KeyedStepRow> = qb.build_query_as().fetch_all(db).await?;
    let mut steps_by_id: HashMap<String, Vec<StepRow>> = HashMap::new();
    for item in step_rows {
        steps_by_id.entry(item.fix_id).or_default().push(item.row);
    }

    let mut qb = sqlx::QueryBuilder::new(
        "SELECT fix_id, author, outcome, comment, created_at FROM fix_reviews WHERE fix_id",
    );
    push_in(&mut qb, &ids);
    qb.push(" ORDER BY fix_id, created_at, id");
    let review_rows: Vec<KeyedReviewRow> = qb.build_query_as().fetch_all(db).await?;
    let mut reviews_by_id: HashMap<String, Vec<ReviewRow>> = HashMap::new();
    for item in review_rows {
        reviews_by_id.entry(item.fix_id).or_default().push(item.row);
    }

    let mut selections_by_fix = batch_fix_selections(db, &ids).await?;

    let mut detailed = Vec::with_capacity(fix_rows.len());
    for keyed in fix_rows {
        let id = keyed.id;
        let row = keyed.row;
        let fix_selections = selections_by_fix.remove(&id).unwrap_or_default();
        let reviews = reviews_by_id
            .remove(&id)
            .unwrap_or_default()
            .into_iter()
            .map(Review::try_from)
            .collect::<Result<Vec<_>, _>>()?;
        detailed.push(Fix {
            id: id.clone(),
            project_id: row.project_id,
            title: row.title,
            summary: row.summary,
            git_ref: row.git_ref,
            status: row.status.parse().map_err(anyhow::Error::from)?,
            problems: problems_by_id.remove(&id).unwrap_or_default(),
            tactics: tactics_by_id.remove(&id).unwrap_or_default(),
            files: files_by_id.remove(&id).unwrap_or_default(),
            comparisons: comparisons_of(
                comparisons_by_id.remove(&id).unwrap_or_default(),
                &id,
                &approved,
            )?,
            selections: fix_selections,
            steps: steps_by_id
                .remove(&id)
                .unwrap_or_default()
                .into_iter()
                .map(Step::from)
                .collect(),
            reviews,
            mismatch_override_reason: row.mismatch_override_reason,
            author: row.author,
            created_at: row.created_at,
            updated_at: row.updated_at,
        });
    }
    Ok(detailed)
}
