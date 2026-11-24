//! Capture search query parsing and limits.

use crate::domain::ApiError;
use rupicola_api::{CaptureStatus, Inspection};

/// Captures in a page unless the search asks for another number, and the most it may ask.
pub const DEFAULT_LIMIT: u32 = 50;
const MAX_LIMIT: u32 = 200;
/// What separates the instant from the id in a cursor.
pub const CURSOR_SEPARATOR: char = '.';
/// The `include` value that asks for every capture in full.
const INCLUDE_DETAILS: &str = "details";

/// What a search asks for, read from its query string.
#[derive(Debug, Default, PartialEq)]
pub struct Search {
    pub limit: u32,
    pub cursor: Option<(i64, String)>,
    pub topic: Option<String>,
    pub direct: bool,
    pub series: Option<String>,
    pub surface: Option<String>,
    pub facets: Vec<(String, String)>,
    pub status: Option<CaptureStatus>,
    pub inspection: Option<Inspection>,
    pub since: Option<i64>,
    pub unreferenced: bool,
    pub details: bool,
}

impl Search {
    /// Reads a query string; a parameter it does not know, or a value it cannot read, is an
    /// error rather than something silently ignored.
    ///
    /// # Errors
    ///
    /// [`ApiError::Invalid`] when any parameter or value is malformed or unknown.
    pub fn parse(query: Option<&str>) -> Result<Self, ApiError> {
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
