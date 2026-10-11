//! What an installed `anneal` asks the app so it can update itself (docs/CLI_INSTALL.md). Public, like `/install.sh` and `/downloads`: the
//! CLI has to be able to ask before there is anything to sign in with, and everything here is already public (the binary and the course
//! files it embeds).
//!
//! - `GET /cli/{target}/version`: the SHA-256 and size of the binary the app serves for `linux-x64`, `macos-arm64` or `macos-x64`.
//!   `anneal self-update` compares it with its own and downloads `/downloads/anneal-{target}` when they differ.
//! - `GET /courses-bundle/{course}/version`: the fingerprint of a course's learner-facing files and the bundle format.
//! - `GET /courses-bundle/{course}`: those files as one blob (`anneal_content::bundle`), with the fingerprint as its ETag, so
//!   `anneal course update` brings in new stages and tests without a new binary.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::SystemTime;

use axum::extract::{Path as UrlPath, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use serde_json::json;
use sha2::{Digest, Sha256};

use anneal_content::bundle;

const TARGETS: [&str; 3] = ["linux-x64", "macos-arm64", "macos-x64"];

struct Shared {
    courses: Option<PathBuf>,
    downloads: Option<PathBuf>,
    /// A course's packed bundle, built on first request: the files only change with a deploy.
    bundles: Mutex<HashMap<String, Arc<Packed>>>,
    /// A served binary's hash, by target, valid while its modification time and size stay the same.
    hashes: Mutex<HashMap<String, (SystemTime, u64, String)>>,
}

struct Packed {
    blob: Vec<u8>,
    fingerprint: String,
}

/// The routes. `courses` is the directory holding `<id>/course.toml`; `downloads` holds `anneal-<os>-<arch>`.
pub fn router(courses: Option<PathBuf>, downloads: Option<PathBuf>) -> Router {
    let shared = Arc::new(Shared { courses, downloads, bundles: Mutex::default(), hashes: Mutex::default() });
    Router::new()
        .route("/cli/{target}/version", get(cli_version))
        .route("/courses-bundle/{course}/version", get(bundle_version))
        .route("/courses-bundle/{course}", get(bundle_blob))
        .with_state(shared)
}

fn plain(status: StatusCode, message: &str) -> Response {
    (status, [(header::CACHE_CONTROL, "no-cache")], message.to_owned()).into_response()
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

async fn cli_version(State(s): State<Arc<Shared>>, UrlPath(target): UrlPath<String>) -> Response {
    if !TARGETS.contains(&target.as_str()) {
        return plain(StatusCode::NOT_FOUND, "no such target");
    }
    let Some(dir) = s.downloads.clone() else { return plain(StatusCode::NOT_FOUND, "this app serves no CLI builds") };
    let path = dir.join(format!("anneal-{target}"));
    let Ok(meta) = std::fs::metadata(&path) else { return plain(StatusCode::NOT_FOUND, "no build for this target") };
    let (mtime, size) = (meta.modified().unwrap_or(SystemTime::UNIX_EPOCH), meta.len());
    let cached = s.hashes.lock().unwrap().get(&target).filter(|(m, n, _)| *m == mtime && *n == size).map(|(.., h)| h.clone());
    let sha256 = match cached {
        Some(h) => h,
        None => {
            let Ok(Ok(h)) = tokio::task::spawn_blocking(move || file_sha256(&path)).await else { return plain(StatusCode::INTERNAL_SERVER_ERROR, "could not read the build") };
            s.hashes.lock().unwrap().insert(target.clone(), (mtime, size, h.clone()));
            h
        }
    };
    ([(header::CACHE_CONTROL, "no-cache")], Json(json!({ "target": target, "sha256": sha256, "size": size }))).into_response()
}

fn file_sha256(path: &Path) -> std::io::Result<String> {
    use std::io::Read;
    let mut file = std::fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 1 << 16];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            return Ok(hex(&hasher.finalize()));
        }
        hasher.update(&buf[..n]);
    }
}

/// The packed bundle of `course`; `None` for a bad id or an unknown course.
fn packed(s: &Shared, course: &str) -> Option<Arc<Packed>> {
    let id_ok = !course.is_empty() && course.len() <= 64 && course.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-');
    let dir = s.courses.as_ref().filter(|_| id_ok)?;
    if let Some(p) = s.bundles.lock().unwrap().get(course) {
        return Some(p.clone());
    }
    let files = bundle::collect(dir, Some(course));
    if files.is_empty() {
        return None;
    }
    let blob = bundle::pack(&files);
    let packed = Arc::new(Packed { fingerprint: format!("{:016x}", bundle::fingerprint(&blob)), blob });
    s.bundles.lock().unwrap().insert(course.to_owned(), packed.clone());
    Some(packed)
}

async fn bundle_version(State(s): State<Arc<Shared>>, UrlPath(course): UrlPath<String>) -> Response {
    match packed(&s, &course) {
        Some(p) => ([(header::CACHE_CONTROL, "no-cache")], Json(json!({ "course": course, "fingerprint": p.fingerprint, "format": bundle::FORMAT, "size": p.blob.len() }))).into_response(),
        None => plain(StatusCode::NOT_FOUND, "no such course"),
    }
}

async fn bundle_blob(State(s): State<Arc<Shared>>, UrlPath(course): UrlPath<String>, headers: HeaderMap) -> Response {
    let Some(p) = packed(&s, &course) else { return plain(StatusCode::NOT_FOUND, "no such course") };
    let etag = format!("\"{}\"", p.fingerprint);
    let mut res = if headers.get(header::IF_NONE_MATCH).and_then(|v| v.to_str().ok()).is_some_and(|v| v.split(',').any(|t| t.trim() == etag)) {
        StatusCode::NOT_MODIFIED.into_response()
    } else {
        ([(header::CONTENT_TYPE, "application/octet-stream")], p.blob.clone()).into_response()
    };
    let h = res.headers_mut();
    h.insert(header::ETAG, HeaderValue::from_str(&etag).expect("hex is a valid header value"));
    h.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-cache"));
    h.insert("x-anneal-bundle-format", HeaderValue::from_str(&bundle::FORMAT.to_string()).expect("digits"));
    res
}
