//! Capture search filter model and limits.

use rupicola_api::{CaptureStatus, Inspection};

/// Captures in a page unless the search asks for another number, and the most it may ask.
pub const DEFAULT_LIMIT: u32 = 50;
pub const MAX_LIMIT: u32 = 200;
/// What separates the instant from the id in a cursor.
pub const CURSOR_SEPARATOR: char = '.';

/// What a search asks for.
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
