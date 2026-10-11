//! What an installed CLI asks the app to update itself: the version of a served build and a course's bundle (no database needed).

use std::fs;

use anneal_api::cli_updates::router;
use anneal_content::bundle;
use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use http_body_util::BodyExt;
use sha2::{Digest, Sha256};
use tower::ServiceExt;

struct Fixture {
    _dir: tempfile::TempDir,
    app: axum::Router,
}

fn fixture() -> Fixture {
    let dir = tempfile::tempdir().unwrap();
    let courses = dir.path().join("courses");
    let downloads = dir.path().join("downloads");
    for (path, text) in [
        ("demo/course.toml", "id = \"demo\"\n"),
        ("demo/modules/1a/stage.md", "# stage\n"),
        ("demo/template/src/lib.rs", "// start\n"),
        ("demo/reference/src/lib.rs", "// THE SOLUTION\n"),
        ("demo/reference/tests/secret.rs", "// hidden\n"),
    ] {
        let p = courses.join(path);
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, text).unwrap();
    }
    fs::create_dir_all(&downloads).unwrap();
    fs::write(downloads.join("anneal-linux-x64"), b"not really a binary").unwrap();
    Fixture { app: router(Some(courses), Some(downloads)), _dir: dir }
}

async fn get(app: &axum::Router, uri: &str, if_none_match: Option<&str>) -> (StatusCode, axum::http::HeaderMap, Vec<u8>) {
    let mut req = Request::builder().uri(uri);
    if let Some(tag) = if_none_match {
        req = req.header(header::IF_NONE_MATCH, tag);
    }
    let res = app.clone().oneshot(req.body(Body::empty()).unwrap()).await.unwrap();
    let (parts, body) = res.into_parts();
    (parts.status, parts.headers, body.collect().await.unwrap().to_bytes().to_vec())
}

#[tokio::test]
async fn a_served_build_is_identified_by_its_sha256() {
    let f = fixture();
    let (status, _, body) = get(&f.app, "/cli/linux-x64/version", None).await;
    assert_eq!(status, StatusCode::OK);
    let v: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let want: String = Sha256::digest(b"not really a binary").iter().map(|b| format!("{b:02x}")).collect();
    assert_eq!((v["sha256"].as_str(), v["size"].as_u64()), (Some(want.as_str()), Some(19)));
    // a target with no build, and anything that is not a target name, are 404s
    assert_eq!(get(&f.app, "/cli/macos-arm64/version", None).await.0, StatusCode::NOT_FOUND);
    assert_eq!(get(&f.app, "/cli/..%2Fetc/version", None).await.0, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn a_bundle_holds_the_learner_files_and_never_the_reference() {
    let f = fixture();
    let (status, headers, body) = get(&f.app, "/courses-bundle/demo", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(headers["x-anneal-bundle-format"], bundle::FORMAT.to_string());
    let paths: Vec<&str> = bundle::unpack(&body).unwrap().into_iter().map(|(p, _)| p).collect();
    assert_eq!(paths, ["demo/course.toml", "demo/modules/1a/stage.md", "demo/template/src/lib.rs"]);
    assert!(!String::from_utf8_lossy(&body).contains("THE SOLUTION"));
    // the version endpoint names the same fingerprint, and it is the ETag
    let (_, _, v) = get(&f.app, "/courses-bundle/demo/version", None).await;
    let v: serde_json::Value = serde_json::from_slice(&v).unwrap();
    let fingerprint = v["fingerprint"].as_str().unwrap().to_owned();
    assert_eq!(fingerprint, format!("{:016x}", bundle::fingerprint(&body)));
    assert_eq!(headers[header::ETAG], format!("\"{fingerprint}\""));
    // asking again with the ETag gets no body; a different one gets the bundle
    let (status, _, again) = get(&f.app, "/courses-bundle/demo", Some(&format!("\"{fingerprint}\""))).await;
    assert_eq!((status, again.len()), (StatusCode::NOT_MODIFIED, 0));
    assert_eq!(get(&f.app, "/courses-bundle/demo", Some("\"other\"")).await.0, StatusCode::OK);
}

#[tokio::test]
async fn an_unknown_or_odd_course_id_is_a_404() {
    let f = fixture();
    for id in ["nope", "..", "DEMO", "demo%2F..%2Fx"] {
        assert_eq!(get(&f.app, &format!("/courses-bundle/{id}"), None).await.0, StatusCode::NOT_FOUND, "{id}");
    }
}
