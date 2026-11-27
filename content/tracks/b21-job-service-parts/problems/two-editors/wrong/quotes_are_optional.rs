use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Page {
    title: String,
    #[serde(default)]
    body: String,
}

/// Slug to (version, page).
type Pages = Arc<Mutex<HashMap<String, (u64, Page)>>>;

type Hook = Arc<dyn Fn(&str) + Send + Sync>;

#[derive(Clone)]
struct App {
    pages: Pages,
    hook: Hook,
}

fn tag(version: u64) -> String {
    format!("\"v{version}\"")
}

pub fn router() -> Router {
    router_with_hook(|_| {})
}

/// `router()`, calling `hook(slug)` right after each precondition that is found to hold.
pub fn router_with_hook(hook: impl Fn(&str) + Send + Sync + 'static) -> Router {
    Router::new()
        .route("/pages/{slug}", get(read).put(write))
        .with_state(App {
            pages: Pages::default(),
            hook: Arc::new(hook),
        })
}

async fn read(State(app): State<App>, Path(slug): Path<String>) -> Response {
    match app.pages.lock().unwrap().get(&slug) {
        Some((version, page)) => {
            ([(header::ETAG, tag(*version))], Json(page.clone())).into_response()
        }
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

async fn write(
    State(app): State<App>,
    Path(slug): Path<String>,
    headers: HeaderMap,
    bytes: Bytes,
) -> Response {
    let if_match = headers.get(header::IF_MATCH);
    let if_none = headers.get(header::IF_NONE_MATCH);
    match (if_match, if_none) {
        (None, None) => return StatusCode::PRECONDITION_REQUIRED.into_response(),
        (Some(_), Some(_)) => return StatusCode::BAD_REQUEST.into_response(),
        (None, Some(v)) if v.as_bytes() != b"*" => return StatusCode::BAD_REQUEST.into_response(),
        _ => {}
    }
    // serde also reads a struct from an array, such as `["title", "body"]`, so ask for an object first.
    let first = bytes.iter().find(|byte| !b" \t\r\n".contains(byte));
    if first != Some(&b'{') {
        return StatusCode::BAD_REQUEST.into_response();
    }
    let Ok(page) = serde_json::from_slice::<Page>(&bytes) else {
        return StatusCode::BAD_REQUEST.into_response();
    };
    // One lock for the comparison and the write: nothing can happen between them.
    let mut pages = app.pages.lock().unwrap();
    let current = pages.get(&slug).map(|(version, _)| *version);
    let next = match (if_match, current) {
        (None, None) => 1,
        (Some(wanted), Some(v)) if wanted.to_str().is_ok_and(|w| w.trim_matches('"') == tag(v).trim_matches('"')) => v + 1,
        (_, Some(v)) => {
            return (StatusCode::PRECONDITION_FAILED, [(header::ETAG, tag(v))]).into_response();
        }
        (Some(_), None) => return StatusCode::PRECONDITION_FAILED.into_response(),
    };
    (app.hook)(&slug); // the precondition holds, and the lock is still held
    pages.insert(slug.clone(), (next, page.clone()));
    let etag = [(header::ETAG, tag(next))];
    if next == 1 {
        let location = [(header::LOCATION, format!("/pages/{slug}"))];
        (StatusCode::CREATED, etag, location, Json(page)).into_response()
    } else {
        (StatusCode::OK, etag, Json(page)).into_response()
    }
}
