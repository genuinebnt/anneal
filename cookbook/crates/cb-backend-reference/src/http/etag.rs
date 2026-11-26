//! Conditional requests (RFC 9110 section 13). One mechanism, two jobs:
//! `If-None-Match` saves bandwidth on reads (304), `If-Match` prevents lost updates on writes (412).
//! Recipe: `examples/web/http_caching_conditional.rs`.

use axum::Json;
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use serde::Serialize;

use crate::error::{AppError, AppResult};

#[must_use]
pub fn version_etag(version: i64) -> String {
    format!("\"v{version}\"")
}

/// The version the client believes it is changing. Missing = 428: allowing unconditional writes would
/// let any client that forgets the header silently overwrite others.
pub fn require_if_match(headers: &HeaderMap) -> AppResult<i64> {
    let raw = headers.get(header::IF_MATCH).ok_or(AppError::PreconditionRequired)?;
    // If-Match uses STRONG comparison, so a weak (`W/`) or unrecognised tag never matches. `*` would
    // switch the lost-update protection off, so it is deliberately not honoured.
    raw.to_str()
        .ok()
        .and_then(|s| s.trim().strip_prefix("\"v")?.strip_suffix('"')?.parse().ok())
        .ok_or(AppError::PreconditionFailed)
}

/// `If-None-Match` uses WEAK comparison and may list several tags.
#[must_use]
pub fn none_match_hits(headers: &HeaderMap, etag: &str) -> bool {
    headers
        .get_all(header::IF_NONE_MATCH)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .flat_map(|v| v.split(','))
        .map(str::trim)
        .any(|t| t == "*" || t.strip_prefix("W/").unwrap_or(t) == etag)
}

fn with_etag(mut res: Response, etag: &str) -> Response {
    if let Ok(v) = HeaderValue::from_str(etag) {
        res.headers_mut().insert(header::ETAG, v);
    }
    // `private`: never in a shared cache (responses are per tenant). `no-cache`: revalidate every time.
    res.headers_mut().insert(header::CACHE_CONTROL, HeaderValue::from_static("private, no-cache"));
    res
}

/// A GET response that answers 304 when the client already has this version.
pub fn conditional<T: Serialize>(headers: &HeaderMap, etag: &str, value: &T) -> Response {
    if none_match_hits(headers, etag) {
        return with_etag(StatusCode::NOT_MODIFIED.into_response(), etag);
    }
    with_etag(Json(value).into_response(), etag)
}

pub fn tagged<T: Serialize>(status: StatusCode, etag: &str, value: &T) -> Response {
    with_etag((status, Json(value)).into_response(), etag)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn headers(name: header::HeaderName, v: &str) -> HeaderMap {
        let mut h = HeaderMap::new();
        h.insert(name, HeaderValue::from_str(v).unwrap());
        h
    }

    #[test]
    fn if_match_is_required_and_strong() {
        assert!(matches!(require_if_match(&HeaderMap::new()), Err(AppError::PreconditionRequired)));
        assert_eq!(require_if_match(&headers(header::IF_MATCH, &version_etag(7))).unwrap(), 7);
        for bad in ["*", "W/\"v7\"", "v7", "\"7\"", "\"vx\"", "\"v1\", \"v2\""] {
            let r = require_if_match(&headers(header::IF_MATCH, bad));
            assert!(matches!(r, Err(AppError::PreconditionFailed)), "{bad}");
        }
    }

    #[test]
    fn if_none_match_is_weak_and_takes_lists() {
        for hit in ["\"v2\"", "W/\"v2\"", "\"v1\", \"v2\"", "*"] {
            assert!(none_match_hits(&headers(header::IF_NONE_MATCH, hit), "\"v2\""), "{hit}");
        }
        assert!(!none_match_hits(&headers(header::IF_NONE_MATCH, "\"v1\""), "\"v2\""));
        assert!(!none_match_hits(&HeaderMap::new(), "\"v2\""));
    }
}
