//! BLOCK 5 (part): keyset pagination with opaque cursors, and the filter/sort allow-list.
//!
//! Why keyset (a.k.a. seek) instead of OFFSET: `OFFSET n` makes the DB walk and discard n rows
//! (O(n) per page) and shifts under concurrent inserts (duplicates or skips). Keyset says
//! "rows strictly after (sort_value, id)", which an index answers in O(log n) and is stable.
//! The id is the tie-breaker so the order is total even when many rows share a sort value.
//! Pitfalls: cursors are opaque to clients but NOT trusted (decode errors are 400s, SQL stays parameterised,
//! tenant scoping is applied independently); the cursor records the sort spec so a cursor from one
//! ordering cannot be replayed against another. Column names come from the allow-list, never from input.

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use serde::{Deserialize, Serialize};

use crate::error::{AppError, AppResult, Validator};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "t", content = "v")]
pub enum SortValue {
    Int(i64),
    Text(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Cursor {
    /// Sort spec the cursor was minted for, e.g. `-created_at`.
    pub sort: String,
    pub value: SortValue,
    pub id: String,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
#[non_exhaustive]
pub enum CursorError {
    #[error("cursor is not valid base64")]
    Encoding,
    #[error("cursor payload is malformed")]
    Payload,
    #[error("cursor was issued for a different sort order")]
    SortMismatch,
}

impl Cursor {
    #[must_use]
    pub fn encode(&self) -> String {
        // Serialising plain data cannot fail; fall back to empty rather than panic in a request path.
        URL_SAFE_NO_PAD.encode(serde_json::to_vec(self).unwrap_or_default())
    }

    pub fn decode(s: &str, expected_sort: &str) -> Result<Self, CursorError> {
        let raw = URL_SAFE_NO_PAD.decode(s).map_err(|_| CursorError::Encoding)?;
        let c: Cursor = serde_json::from_slice(&raw).map_err(|_| CursorError::Payload)?;
        if c.sort != expected_sort {
            return Err(CursorError::SortMismatch);
        }
        Ok(c)
    }
}

impl From<CursorError> for AppError {
    fn from(e: CursorError) -> Self {
        AppError::BadRequest(e.to_string())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortField {
    CreatedAt,
    UpdatedAt,
    Priority,
    Title,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Sort {
    pub field: SortField,
    pub desc: bool,
}

impl Sort {
    pub const DEFAULT: Sort = Sort { field: SortField::CreatedAt, desc: true };

    /// `-created_at` (descending) or `created_at`. Anything outside the allow-list is rejected.
    pub fn parse(s: &str) -> Option<Self> {
        let (desc, name) = s.strip_prefix('-').map_or((false, s), |n| (true, n));
        let field = match name {
            "created_at" => SortField::CreatedAt,
            "updated_at" => SortField::UpdatedAt,
            "priority" => SortField::Priority,
            "title" => SortField::Title,
            _ => return None,
        };
        Some(Self { field, desc })
    }

    #[must_use]
    pub fn spec(&self) -> String {
        format!("{}{}", if self.desc { "-" } else { "" }, self.column())
    }

    /// `&'static str` on purpose: only literals can reach SQL text.
    #[must_use]
    pub fn column(&self) -> &'static str {
        match self.field {
            SortField::CreatedAt => "created_at",
            SortField::UpdatedAt => "updated_at",
            SortField::Priority => "priority",
            SortField::Title => "title",
        }
    }

    #[must_use]
    pub fn direction(&self) -> &'static str {
        if self.desc { "DESC" } else { "ASC" }
    }

    /// Comparison operator for "strictly after the cursor" in this direction.
    #[must_use]
    pub fn after_op(&self) -> &'static str {
        if self.desc { "<" } else { ">" }
    }
}

pub const TASK_STATUSES: [&str; 3] = ["open", "in_progress", "done"];
pub const MAX_LIMIT: u32 = 100;
pub const DEFAULT_LIMIT: u32 = 20;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskFilter {
    pub statuses: Vec<String>,
    pub assignee: Option<String>,
    pub project_id: Option<String>,
    pub priority_min: Option<i64>,
    pub q: Option<String>,
    pub sort: Sort,
    pub limit: u32,
    pub cursor: Option<String>,
}

impl Default for TaskFilter {
    fn default() -> Self {
        Self {
            statuses: vec![],
            assignee: None,
            project_id: None,
            priority_min: None,
            q: None,
            sort: Sort::DEFAULT,
            limit: DEFAULT_LIMIT,
            cursor: None,
        }
    }
}

/// Parse `?status=open,done&assignee=u1&priority_min=2&q=foo&sort=-priority&limit=50&cursor=...`.
/// Strict: unknown parameters are errors so typos (`staus=open`) never silently return everything.
pub fn parse_task_filter(pairs: &[(String, String)]) -> AppResult<TaskFilter> {
    let mut f = TaskFilter::default();
    let mut v = Validator::new();
    for (k, val) in pairs {
        match k.as_str() {
            "status" => {
                f.statuses = val.split(',').map(str::to_string).collect();
                for s in &f.statuses {
                    v.one_of("status", s, &TASK_STATUSES);
                }
            }
            "assignee" => f.assignee = Some(val.clone()),
            "project_id" => f.project_id = Some(val.clone()),
            "priority_min" => match val.parse::<i64>() {
                Ok(n) => f.priority_min = Some(n),
                Err(_) => v.fail("priority_min", "invalid_number", "must be an integer"),
            },
            "q" => f.q = Some(val.clone()),
            "sort" => match Sort::parse(val) {
                Some(s) => f.sort = s,
                None => v.fail(
                    "sort",
                    "invalid_value",
                    "must be one of created_at, updated_at, priority, title (prefix - for descending)",
                ),
            },
            "limit" => match val.parse::<u32>() {
                Ok(n) if (1..=MAX_LIMIT).contains(&n) => f.limit = n,
                _ => v.fail("limit", "out_of_range", format!("must be between 1 and {MAX_LIMIT}")),
            },
            "cursor" => f.cursor = Some(val.clone()),
            other => v.fail(other, "unknown_parameter", "unknown query parameter"),
        }
    }
    v.finish()?;
    Ok(f)
}

/// Turn free user text into a SAFE FTS5 MATCH expression: every word is quoted (so operators like
/// `AND`, `NEAR`, `-`, `"` lose their meaning) and the last word becomes a prefix match.
/// Returns `None` when nothing searchable remains. Pitfall avoided: passing raw input to MATCH
/// throws syntax errors on `foo"` or `a OR` and lets users run column filters.
#[must_use]
pub fn fts_query(q: &str) -> Option<String> {
    let words: Vec<String> = q
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .take(8)
        .map(|w| format!("\"{w}\""))
        .collect();
    let (last, init) = words.split_last()?;
    let mut parts: Vec<String> = init.to_vec();
    parts.push(format!("{last}*"));
    Some(parts.join(" "))
}

#[derive(Debug, serde::Serialize)]
pub struct Page<T> {
    pub items: Vec<T>,
    pub next_cursor: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// xorshift64: seeded, dependency-free randomness for property-style loops.
    pub(crate) struct Rng(pub u64);
    impl Rng {
        pub(crate) fn next(&mut self) -> u64 {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 7;
            self.0 ^= self.0 << 17;
            self.0
        }
        pub(crate) fn string(&mut self, max: usize) -> String {
            const ALPHABET: &[char] =
                &['a', 'Z', '0', ' ', '"', '*', '-', '\'', 'é', '日', '\n', '%', '(', ')', ':', '^'];
            let n = (self.next() % (max as u64 + 1)) as usize;
            (0..n).map(|_| ALPHABET[(self.next() % ALPHABET.len() as u64) as usize]).collect()
        }
    }

    fn pairs(v: &[(&str, &str)]) -> Vec<(String, String)> {
        v.iter().map(|(a, b)| (a.to_string(), b.to_string())).collect()
    }

    #[test]
    fn cursor_round_trips_for_random_values() {
        let mut rng = Rng(0x9E37_79B9_7F4A_7C15);
        for i in 0..2_000 {
            let value = if i % 2 == 0 { SortValue::Int(rng.next() as i64) } else { SortValue::Text(rng.string(40)) };
            let c = Cursor { sort: "-created_at".into(), value, id: rng.string(30) };
            let s = c.encode();
            assert!(s.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_'), "url-safe: {s}");
            assert_eq!(Cursor::decode(&s, "-created_at").unwrap(), c);
        }
    }

    #[test]
    fn decoding_garbage_never_panics_and_always_errors() {
        let mut rng = Rng(42);
        for _ in 0..5_000 {
            let junk = rng.string(60);
            assert!(Cursor::decode(&junk, "-created_at").is_err(), "{junk:?}");
        }
        assert_eq!(Cursor::decode("e30", "x"), Err(CursorError::Payload)); // "{}"
    }

    #[test]
    fn mutated_cursors_are_rejected_or_stay_well_formed() {
        let c = Cursor { sort: "title".into(), value: SortValue::Text("abc".into()), id: "id-1".into() };
        let good = c.encode();
        let mut rng = Rng(7);
        for _ in 0..2_000 {
            let mut bytes = good.clone().into_bytes();
            let i = (rng.next() % bytes.len() as u64) as usize;
            bytes[i] = b"AZaz09-_="[(rng.next() % 9) as usize];
            // Either rejected, or decodes to SOME well-formed cursor; it must never panic.
            let _ = Cursor::decode(&String::from_utf8(bytes).unwrap(), "title");
        }
    }

    #[test]
    fn cursor_for_another_sort_is_rejected() {
        let c = Cursor { sort: "-priority".into(), value: SortValue::Int(1), id: "a".into() };
        assert_eq!(Cursor::decode(&c.encode(), "-created_at"), Err(CursorError::SortMismatch));
    }

    #[test]
    fn sort_parse_and_spec_round_trip() {
        for s in ["created_at", "-created_at", "updated_at", "-priority", "title"] {
            assert_eq!(Sort::parse(s).unwrap().spec(), s);
        }
        for bad in ["", "-", "id", "created_at; DROP TABLE tasks", "--title", "Title"] {
            assert!(Sort::parse(bad).is_none(), "{bad}");
        }
    }

    #[test]
    fn filter_parses_valid_input() {
        let f = parse_task_filter(&pairs(&[
            ("status", "open,done"),
            ("priority_min", "2"),
            ("sort", "-priority"),
            ("limit", "5"),
        ]))
        .unwrap();
        assert_eq!(f.statuses, vec!["open", "done"]);
        assert_eq!(f.priority_min, Some(2));
        assert_eq!(f.limit, 5);
        assert!(f.sort.desc);
        assert_eq!(parse_task_filter(&[]).unwrap(), TaskFilter::default());
    }

    #[test]
    fn filter_reports_every_problem_and_unknown_params() {
        let e = parse_task_filter(&pairs(&[("status", "nope"), ("limit", "0"), ("staus", "open"), ("sort", "x")]))
            .unwrap_err();
        let AppError::Validation(list) = e else { panic!() };
        assert_eq!(list.len(), 4, "{list:?}");
    }

    #[test]
    fn random_filters_never_panic_and_respect_bounds() {
        let mut rng = Rng(99);
        let keys = ["status", "limit", "sort", "priority_min", "q", "cursor", "assignee", "bogus"];
        for _ in 0..3_000 {
            let n = rng.next() % 5;
            let ps: Vec<(String, String)> = (0..n)
                .map(|_| (keys[(rng.next() % keys.len() as u64) as usize].to_string(), rng.string(8)))
                .collect();
            if let Ok(f) = parse_task_filter(&ps) {
                assert!((1..=MAX_LIMIT).contains(&f.limit));
                assert!(f.statuses.iter().all(|s| TASK_STATUSES.contains(&s.as_str())));
            }
        }
    }

    #[test]
    fn fts_query_is_always_safe() {
        assert_eq!(fts_query("fix login").unwrap(), "\"fix\" \"login\"*");
        assert_eq!(fts_query("   ").as_deref(), None);
        let mut rng = Rng(5);
        for _ in 0..3_000 {
            let q = rng.string(30);
            if let Some(out) = fts_query(&q) {
                // After stripping our own quotes/stars/spaces only word characters may remain.
                assert!(
                    out.chars().all(|c| c.is_alphanumeric() || matches!(c, '"' | '*' | ' ')),
                    "{out:?} from {q:?}"
                );
                assert_eq!(out.matches('"').count() % 2, 0);
            }
        }
    }
}
