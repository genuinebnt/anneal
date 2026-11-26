//! Typed extractors whose rejections are problem+json like every other error, plus content negotiation.
//! Recipe: `examples/web/backend_request_validation_extractors.rs`.

use axum::body::Bytes;
use axum::extract::{FromRequest, FromRequestParts, Query, Request};
use axum::http::request::Parts;
use axum::http::{HeaderMap, StatusCode, header};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Deserializer};

use crate::error::AppError;

/// Is `value` (a Content-Type) JSON? Accepts parameters and `+json` suffixes.
#[must_use]
pub fn is_json_content_type(value: &str) -> bool {
    value.parse::<mime::Mime>().is_ok_and(|m| {
        m.type_() == mime::APPLICATION && (m.subtype() == mime::JSON || m.suffix() == Some(mime::JSON))
    })
}

/// May we answer with `application/json`? The MOST SPECIFIC matching range decides, so
/// `application/json;q=0, */*` is a refusal.
#[must_use]
pub fn accepts_json(headers: &HeaderMap) -> bool {
    let mut ranges = headers
        .get_all(header::ACCEPT)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .flat_map(|v| v.split(','))
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .peekable();
    if ranges.peek().is_none() {
        return true;
    }
    ranges
        .filter_map(|r| r.parse::<mime::Mime>().ok())
        .filter_map(|m| {
            let specificity = match (m.type_().as_str(), m.subtype().as_str()) {
                ("application", "json") => 3,
                ("application", "*") => 2,
                ("*", "*") => 1,
                _ => return None,
            };
            let q = m.get_param("q").and_then(|q| q.as_str().parse::<f32>().ok()).unwrap_or(1.0);
            Some((specificity, q > 0.0))
        })
        .max_by_key(|(specificity, _)| *specificity)
        .is_some_and(|(_, ok)| ok)
}

/// Like `axum::Json`, with our error format. 415 wrong type, 413 too big, 400 broken JSON, 422 wrong shape.
pub struct ApiJson<T>(pub T);

impl<T: DeserializeOwned, S: Send + Sync> FromRequest<S> for ApiJson<T> {
    type Rejection = AppError;

    async fn from_request(req: Request, state: &S) -> Result<Self, Self::Rejection> {
        let content_type = req.headers().get(header::CONTENT_TYPE).and_then(|v| v.to_str().ok());
        // Requiring the header also blocks the classic cross-site form post (`text/plain` body).
        if !content_type.is_some_and(is_json_content_type) {
            return Err(AppError::UnsupportedMediaType("expected application/json".into()));
        }
        let bytes = Bytes::from_request(req, state).await.map_err(|e| {
            if e.status() == StatusCode::PAYLOAD_TOO_LARGE {
                AppError::PayloadTooLarge
            } else {
                AppError::BadRequest("could not read the request body".into())
            }
        })?;
        serde_json::from_slice(&bytes).map(Self).map_err(|e| match e.classify() {
            serde_json::error::Category::Data => AppError::Unprocessable(e.to_string()),
            _ => AppError::BadRequest(format!("invalid JSON: {e}")),
        })
    }
}

/// The query string as ordered pairs, for the strict filter parser.
pub struct QueryPairs(pub Vec<(String, String)>);

impl<S: Send + Sync> FromRequestParts<S> for QueryPairs {
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        Query::<Vec<(String, String)>>::from_request_parts(parts, state)
            .await
            .map(|Query(q)| Self(q))
            .map_err(|_| AppError::BadRequest("malformed query string".into()))
    }
}

/// For PATCH: tells "field absent" (`None`) apart from "field is null" (`Some(None)`).
/// Use with `#[serde(default, deserialize_with = "double_option")]`.
pub fn double_option<'de, T: Deserialize<'de>, D: Deserializer<'de>>(d: D) -> Result<Option<Option<T>>, D::Error> {
    Option::<T>::deserialize(d).map(Some)
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;

    fn accept(v: &str) -> bool {
        let mut h = HeaderMap::new();
        h.insert(header::ACCEPT, HeaderValue::from_str(v).unwrap());
        accepts_json(&h)
    }

    #[test]
    fn negotiation() {
        assert!(accepts_json(&HeaderMap::new()));
        for ok in ["*/*", "application/json", "application/*", "text/html, application/json;q=0.5"] {
            assert!(accept(ok), "{ok}");
        }
        for no in ["text/html", "application/xml", "application/json;q=0, */*", "*/*;q=0", "garbage"] {
            assert!(!accept(no), "{no}");
        }
    }

    #[test]
    fn json_content_types() {
        for ok in ["application/json", "application/json; charset=utf-8", "application/merge-patch+json"] {
            assert!(is_json_content_type(ok), "{ok}");
        }
        for no in ["text/plain", "application/x-www-form-urlencoded", "json", ""] {
            assert!(!is_json_content_type(no), "{no}");
        }
    }

    #[test]
    fn patch_distinguishes_absent_from_null() {
        #[derive(Deserialize)]
        struct P {
            #[serde(default, deserialize_with = "double_option")]
            a: Option<Option<String>>,
        }
        let parse = |s: &str| serde_json::from_str::<P>(s).unwrap().a;
        assert_eq!(parse("{}"), None);
        assert_eq!(parse(r#"{"a":null}"#), Some(None));
        assert_eq!(parse(r#"{"a":"x"}"#), Some(Some("x".into())));
    }
}
