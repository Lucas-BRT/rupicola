//! Finding a project's captures: by topic, series, screen, environment facet, status, date and
//! whether anything cites them, one page at a time, optionally with every capture in full.

use crate::{
    domain::Access, domain::ApiError, domain::Principal, services::access::access, state::Shared,
};
use axum::{
    Json,
    extract::{Path, RawQuery, State},
};
use rupicola_api::{CapturePage, CaptureStatus, Inspection, Role};
use sqlx::{QueryBuilder, Sqlite};

/// Captures in a page unless the search asks for another number, and the most it may ask.
const DEFAULT_LIMIT: u32 = 50;
const MAX_LIMIT: u32 = 200;
/// What separates the instant from the id in a cursor.
const CURSOR_SEPARATOR: char = '.';
/// The `include` value that asks for every capture in full.
const INCLUDE_DETAILS: &str = "details";

/// What a search asks for, read from its query string.
#[derive(Debug, Default, PartialEq)]
struct Search {
    limit: u32,
    cursor: Option<(i64, String)>,
    topic: Option<String>,
    direct: bool,
    series: Option<String>,
    surface: Option<String>,
    facets: Vec<(String, String)>,
    status: Option<CaptureStatus>,
    inspection: Option<Inspection>,
    since: Option<i64>,
    unreferenced: bool,
    details: bool,
}

impl Search {
    /// Reads a query string; a parameter it does not know, or a value it cannot read, is an
    /// error rather than something silently ignored.
    fn parse(query: Option<&str>) -> Result<Self, ApiError> {
        let mut search = Self {
            limit: DEFAULT_LIMIT,
            ..Self::default()
        };
        for pair in query
            .unwrap_or_default()
            .split('&')
            .filter(|pair| !pair.is_empty())
        {
            let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
            let (key, value) = (decode(key)?, decode(value)?);
            let bad = |what: &str| ApiError::invalid(format!("`{key}` {what}"));
            match key.as_str() {
                "limit" => {
                    search.limit = value
                        .parse()
                        .ok()
                        .filter(|limit| (1..=MAX_LIMIT).contains(limit))
                        .ok_or_else(|| bad(&format!("must be 1 to {MAX_LIMIT}")))?;
                }
                "cursor" => {
                    let (at, id) = value
                        .split_once(CURSOR_SEPARATOR)
                        .and_then(|(at, id)| Some((at.parse().ok()?, id.to_owned())))
                        .ok_or_else(|| bad("is not a cursor this server gave"))?;
                    search.cursor = Some((at, id));
                }
                "topic" => search.topic = Some(value),
                "direct" => {
                    search.direct = flag(&value).ok_or_else(|| bad("must be true or false"))?;
                }
                "series" => search.series = Some(value),
                "surface" => search.surface = Some(value),
                "facet" => {
                    let (facet, wanted) = value
                        .split_once('=')
                        .ok_or_else(|| bad("must be key=value"))?;
                    search.facets.push((facet.to_owned(), wanted.to_owned()));
                }
                "status" => {
                    search.status = Some(
                        value
                            .parse()
                            .map_err(|_| bad("must be complete or blocked"))?,
                    );
                }
                "inspection" => {
                    search.inspection = Some(
                        value
                            .parse()
                            .map_err(|_| bad("must be pending or reviewed"))?,
                    );
                }
                "since" => {
                    search.since = Some(value.parse().map_err(|_| bad("must be milliseconds"))?);
                }
                "unreferenced" => {
                    search.unreferenced =
                        flag(&value).ok_or_else(|| bad("must be true or false"))?;
                }
                "include" if value == INCLUDE_DETAILS => search.details = true,
                _ => return Err(bad("is not something a search can ask for")),
            }
        }
        Ok(search)
    }

    /// The ids and instants of the captures this search finds, one more than a page so the
    /// caller knows whether another page follows.
    fn query(&self, project: &str) -> QueryBuilder<Sqlite> {
        let mut sql = QueryBuilder::new(
            "SELECT c.id, c.created_at FROM captures c WHERE c.deleted_at IS NULL AND c.project_id = ",
        );
        sql.push_bind(project);
        if let Some((at, id)) = &self.cursor {
            sql.push(" AND (c.created_at, c.id) < (")
                .push_bind(*at)
                .push(", ")
                .push_bind(id.as_str())
                .push(")");
        }
        if let Some(topic) = &self.topic {
            if self.direct {
                sql.push(" AND c.id IN (SELECT capture_id FROM capture_topics WHERE topic_id = ")
                    .push_bind(topic.as_str())
                    .push(")");
            } else {
                sql.push(
                    " AND c.id IN (WITH RECURSIVE tree(id) AS (SELECT ",
                )
                .push_bind(topic.as_str())
                .push(
                    " UNION ALL SELECT t.id FROM topics t JOIN tree ON t.parent_id = tree.id)
                     SELECT ct.capture_id FROM capture_topics ct JOIN tree ON ct.topic_id = tree.id)",
                );
            }
        }
        if let Some(series) = &self.series {
            sql.push(" AND c.series = ").push_bind(series.as_str());
        }
        if let Some(surface) = &self.surface {
            sql.push(
                " AND EXISTS (SELECT 1 FROM artifacts a JOIN surfaces s ON s.id = a.surface_id
                   WHERE a.capture_id = c.id AND s.key = ",
            )
            .push_bind(surface.as_str())
            .push(")");
        }
        for (key, value) in &self.facets {
            sql.push(" AND EXISTS (SELECT 1 FROM capture_facets f WHERE f.capture_id = c.id AND f.key = ")
                .push_bind(key.as_str())
                .push(" AND f.value = ")
                .push_bind(value.as_str())
                .push(")");
        }
        if let Some(status) = self.status {
            sql.push(" AND c.status = ").push_bind(status.as_str());
        }
        if let Some(inspection) = self.inspection {
            sql.push(" AND c.inspection = ")
                .push_bind(inspection.as_str());
        }
        if let Some(since) = self.since {
            sql.push(" AND c.created_at >= ").push_bind(since);
        }
        if self.unreferenced {
            sql.push(
                " AND NOT EXISTS (SELECT 1 FROM selection_items i WHERE i.capture_id = c.id)
                  AND NOT EXISTS (SELECT 1 FROM artifacts a WHERE a.capture_id = c.id AND (
                    EXISTS (SELECT 1 FROM fix_comparisons k WHERE a.id IN
                      (k.before_artifact_id, k.after_artifact_id, k.diff_artifact_id))
                    OR EXISTS (SELECT 1 FROM fix_steps t WHERE t.evidence_artifact_id = a.id)
                    OR EXISTS (SELECT 1 FROM design_entries d WHERE d.evidence = a.id)))",
            );
        }
        sql.push(" ORDER BY c.created_at DESC, c.id DESC LIMIT ")
            .push_bind(i64::from(self.limit) + 1);
        sql
    }
}

/// `true`, `false`, `1` or `0`.
fn flag(value: &str) -> Option<bool> {
    match value {
        "true" | "1" => Some(true),
        "false" | "0" => Some(false),
        _ => None,
    }
}

/// A query-string component with its `+` as spaces and its `%XX` escapes decoded.
fn decode(raw: &str) -> Result<String, ApiError> {
    let bytes = raw.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'+' => out.push(b' '),
            b'%' => {
                let hex = bytes
                    .get(index + 1..index + 3)
                    .and_then(|pair| std::str::from_utf8(pair).ok())
                    .and_then(|pair| u8::from_str_radix(pair, 16).ok())
                    .ok_or_else(|| ApiError::invalid("the query has a broken % escape"))?;
                out.push(hex);
                index += 2;
            }
            byte => out.push(byte),
        }
        index += 1;
    }
    String::from_utf8(out).map_err(|_| ApiError::invalid("the query is not UTF-8"))
}

/// `GET /v1/projects/{id}/captures/search`: one page of the captures that match, newest first,
/// with a cursor for the next page and, with `include=details`, every capture in full.
pub async fn search(
    State(state): State<Shared>,
    principal: Principal,
    Path(project): Path<String>,
    RawQuery(query): RawQuery,
) -> Result<Json<CapturePage>, ApiError> {
    match access(&state, &principal, &project).await? {
        Access::Device => {}
        person @ Access::Person(_) => person.require(Role::Reader)?,
    }
    let search = Search::parse(query.as_deref())?;
    let mut rows: Vec<(String, i64)> = search
        .query(&project)
        .build_query_as()
        .fetch_all(&state.db)
        .await?;
    let page = usize::try_from(search.limit).unwrap_or(usize::MAX);
    let next = (rows.len() > page).then(|| {
        rows.truncate(page);
        rows.last()
            .map(|(id, at)| format!("{at}{CURSOR_SEPARATOR}{id}"))
            .unwrap_or_default()
    });
    let ids: Vec<String> = rows.into_iter().map(|(id, _)| id).collect();
    let items = crate::captures::summaries(&state, &ids).await?;
    let mut details = Vec::new();
    if search.details {
        for id in &ids {
            details.push(crate::captures::load(&state, id).await?);
        }
    }
    Ok(Json(CapturePage {
        items,
        details,
        next,
    }))
}

#[cfg(test)]
mod parse_tests {
    use super::*;

    #[test]
    fn a_query_string_is_read_into_a_search() {
        let search = Search::parse(Some(
            "limit=2&topic=t1&direct=true&facet=appearance.color_scheme%3Ddark&facet=platform.family=web\
             &status=blocked&since=10&unreferenced=1&include=details&cursor=5.abc&surface=settings%2Fprofile",
        ))
        .unwrap();
        assert_eq!(search.limit, 2);
        assert_eq!(search.cursor, Some((5, "abc".to_owned())));
        assert_eq!(search.topic.as_deref(), Some("t1"));
        assert!(search.direct && search.unreferenced && search.details);
        assert_eq!(
            search.facets,
            [
                ("appearance.color_scheme".to_owned(), "dark".to_owned()),
                ("platform.family".to_owned(), "web".to_owned()),
            ]
        );
        assert_eq!(search.status, Some(CaptureStatus::Blocked));
        assert_eq!(search.surface.as_deref(), Some("settings/profile"));
        assert_eq!(Search::parse(None).unwrap().limit, DEFAULT_LIMIT);
    }

    #[test]
    fn what_a_search_cannot_read_is_refused() {
        for bad in [
            "limit=0",
            "limit=201",
            "cursor=nope",
            "facet=novalue",
            "status=lost",
            "colour=red",
            "x=%zz",
        ] {
            assert!(Search::parse(Some(bad)).is_err(), "{bad}");
        }
        assert_eq!(decode("a+b%2Fc").unwrap(), "a b/c");
    }
}

#[cfg(test)]
mod tests {
    use crate::testkit::*;
    use axum::http::StatusCode;
    use rupicola_api::{CapturePage, CaptureStatus, Topic, record::Appearance};
    use serde_json::json;

    const A: &str = "01936f2c-aaaa-7bbb-8ccc-0000000008a1";
    const B: &str = "01936f2c-aaaa-7bbb-8ccc-0000000008b1";
    const C: &str = "01936f2c-aaaa-7bbb-8ccc-0000000008c1";

    /// Ana owns `demo` with three captures: A (dark, on the home screen), B (light, another
    /// series, no screen) and C (blocked, no files). Carla is outside.
    async fn demo() -> (u16, [String; 2]) {
        let (port, state) = start_custom(settings(), None, true).await;
        for handle in ["ana", "carla"] {
            seed_account(&state, handle, false).await;
        }
        let [ana, carla] = [sign_in(port, "ana").await, sign_in(port, "carla").await];
        call(
            port,
            "POST",
            "/v1/projects",
            Some(&ana),
            Some(r#"{"id":"demo","label":"Demo"}"#),
        )
        .await;
        let sha = stream_blob(port, &ana, "demo", b"pixels").await;
        publish_record(port, &ana, &capture_record(A, &sha, 6, Vec::new())).await;
        let mut light = capture_record(B, &sha, 6, Vec::new());
        light.series = "Settings@web".to_owned();
        light.environment.appearance = Some(Appearance {
            color_scheme: Some("light".to_owned()),
            ..Appearance::default()
        });
        light.members[0].surface = None;
        publish_record(port, &ana, &light).await;
        let mut blocked = capture_record(C, &sha, 6, Vec::new());
        blocked.status = CaptureStatus::Blocked;
        blocked.blocker = Some("ffmpeg is missing".to_owned());
        blocked.members.clear();
        let published = publish_record(port, &ana, &blocked).await;
        assert_eq!(status(&published), StatusCode::CREATED, "{published}");
        (port, [ana, carla])
    }

    async fn search(port: u16, token: &str, query: &str) -> String {
        call(
            port,
            "GET",
            &format!("/v1/projects/demo/captures/search?{query}"),
            Some(token),
            None,
        )
        .await
    }

    async fn ids(port: u16, token: &str, query: &str) -> Vec<String> {
        let response = search(port, token, query).await;
        assert_eq!(status(&response), StatusCode::OK, "{query}: {response}");
        let mut found: Vec<String> = json::<CapturePage>(&response)
            .items
            .into_iter()
            .map(|capture| capture.id)
            .collect();
        found.sort();
        found
    }

    #[tokio::test]
    async fn pages_follow_one_another_by_cursor_without_repeating() {
        let (port, [ana, _]) = demo().await;
        let first: CapturePage = json(&search(port, &ana, "limit=2").await);
        assert_eq!(first.items.len(), 2);
        let cursor = first.next.expect("another page follows");
        let second: CapturePage =
            json(&search(port, &ana, &format!("limit=2&cursor={cursor}")).await);
        assert_eq!(second.items.len(), 1);
        assert_eq!(second.next, None, "the last page says so");
        let mut seen: Vec<String> = first
            .items
            .iter()
            .chain(&second.items)
            .map(|c| c.id.clone())
            .collect();
        seen.sort();
        assert_eq!(seen, [A, B, C]);
    }

    #[tokio::test]
    async fn each_filter_narrows_the_search() {
        let (port, [ana, _]) = demo().await;
        assert_eq!(
            ids(port, &ana, "facet=appearance.color_scheme%3Ddark").await,
            [A, C]
        );
        assert_eq!(
            ids(
                port,
                &ana,
                "facet=appearance.color_scheme=light&facet=display.width=800"
            )
            .await,
            [B]
        );
        assert_eq!(ids(port, &ana, "series=Settings%40web").await, [B]);
        assert_eq!(ids(port, &ana, "surface=home").await, [A]);
        assert_eq!(ids(port, &ana, "status=blocked").await, [C]);
        assert_eq!(
            ids(port, &ana, "inspection=reviewed").await,
            Vec::<String>::new()
        );
        let topic: Topic = json(
            &call(
                port,
                "POST",
                "/v1/projects/demo/topics",
                Some(&ana),
                Some(r#"{"path":"fix/123"}"#),
            )
            .await,
        );
        call(
            port,
            "PUT",
            &format!("/v1/topics/{}/captures/{B}", topic.id),
            Some(&ana),
            None,
        )
        .await;
        let fix = topic.parent_id.clone().unwrap();
        assert_eq!(
            ids(port, &ana, &format!("topic={fix}")).await,
            [B],
            "the subtree counts"
        );
        assert_eq!(
            ids(port, &ana, &format!("topic={fix}&direct=true")).await,
            Vec::<String>::new()
        );
        call(
            port,
            "POST",
            "/v1/projects/demo/selections",
            Some(&ana),
            Some(
                &json!({ "label": "Proof", "items": [{ "capture": A, "mark": "proof" }] })
                    .to_string(),
            ),
        )
        .await;
        assert_eq!(ids(port, &ana, "unreferenced=true").await, [B, C]);
    }

    #[tokio::test]
    async fn details_come_with_the_page_in_the_same_order() {
        let (port, [ana, _]) = demo().await;
        let page: CapturePage = json(&search(port, &ana, "include=details").await);
        let listed: Vec<&str> = page.items.iter().map(|c| c.id.as_str()).collect();
        let detailed: Vec<&str> = page.details.iter().map(|c| c.id.as_str()).collect();
        assert_eq!(listed, detailed);
        assert_eq!(
            page.details
                .iter()
                .map(|c| c.checkpoints.len())
                .sum::<usize>(),
            2
        );
        let plain: CapturePage = json(&search(port, &ana, "").await);
        assert_eq!(plain.details, Vec::new());
    }

    #[tokio::test]
    async fn a_search_it_cannot_read_or_may_not_see_is_refused() {
        let (port, [ana, carla]) = demo().await;
        assert_eq!(
            status(&search(port, &ana, "colour=red").await),
            StatusCode::UNPROCESSABLE_ENTITY
        );
        assert_eq!(
            status(&search(port, &ana, "limit=999").await),
            StatusCode::UNPROCESSABLE_ENTITY
        );
        assert_eq!(
            status(&search(port, &carla, "").await),
            StatusCode::NOT_FOUND
        );
    }
}
