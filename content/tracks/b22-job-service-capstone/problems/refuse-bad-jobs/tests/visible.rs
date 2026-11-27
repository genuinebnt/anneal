use solution::*;

#[allow(unused_imports)]
use axum::body::to_bytes;
#[allow(unused_imports)]
use axum::response::IntoResponse;
#[allow(unused_imports)]
use serde_json::{Value, json};

#[allow(dead_code)]
/// (status, content type, JSON body) of an error's response.
pub fn reply(error: ApiError) -> (u16, String, Value) {
    let response = error.into_response();
    let status = response.status().as_u16();
    let content_type = response.headers().get("content-type").map(|v| v.to_str().unwrap().to_string()).unwrap_or_default();
    let runtime = tokio::runtime::Builder::new_current_thread().build().unwrap();
    let bytes = runtime.block_on(to_bytes(response.into_body(), 1 << 20)).unwrap();
    (status, content_type, serde_json::from_slice(&bytes).unwrap_or(Value::Null))
}

#[allow(dead_code)]
/// (status, code, details), after checking that the body has exactly the agreed shape and a message.
pub fn shape(error: ApiError) -> (u16, String, Value) {
    let (status, content_type, body) = reply(error);
    assert!(content_type.starts_with("application/json"), "content type {content_type:?}");
    let inner = &body["error"];
    let keys = |v: &Value| v.as_object().map(|o| o.keys().cloned().collect::<Vec<_>>().join(",")).unwrap_or_default();
    assert_eq!((keys(&body), keys(inner)), ("error".to_string(), "code,details,message".to_string()), "shape of {body}");
    assert!(inner["message"].as_str().is_some_and(|m| !m.is_empty()), "message in {body}");
    (status, inner["code"].as_str().unwrap_or("?").to_string(), inner["details"].clone())
}

#[allow(dead_code)]
/// What a parse came to: the spec, "invalid_json", or the field each detail begins with.
#[derive(Debug, PartialEq)]
pub enum Parsed {
    Ok(Spec),
    InvalidJson,
    Invalid(Vec<String>),
    Other(String),
}

#[allow(dead_code)]
pub fn parse(body: &str) -> Parsed {
    match parse_spec(body.as_bytes()) {
        Ok(spec) => Parsed::Ok(spec),
        Err(ApiError::InvalidJson(_)) => Parsed::InvalidJson,
        Err(ApiError::Validation(details)) => Parsed::Invalid(details.iter().map(|d| match d.split_once(": ") {
            Some((field, problem)) if !problem.is_empty() => field.to_string(),
            _ => format!("not \"field: problem\": {d}"),
        }).collect()),
        Err(other) => Parsed::Other(format!("{other:?}")),
    }
}

#[allow(dead_code)]
pub fn spec(source: &str, width: u32, height: u32, format: &str, work_ms: u64) -> Spec {
    Spec { source: source.to_string(), width, height, format: format.to_string(), work_ms }
}

#[allow(dead_code)]
pub fn invalid(fields: &[&str]) -> Parsed {
    Parsed::Invalid(fields.iter().map(|f| f.to_string()).collect())
}

#[test]
fn a_full_job() {
    check!("every field given", parse(r#"{"source":"cat.png","width":640,"height":480,"format":"webp","work_ms":5}"#), Parsed::Ok(spec("cat.png", 640, 480, "webp", 5)));
}

#[test]
fn defaults() {
    check!("format and work_ms left out", parse(r#"{"source":"cat.png","width":1,"height":2}"#), Parsed::Ok(spec("cat.png", 1, 2, "png", 20)));
}

#[test]
fn every_problem_at_once() {
    check!("four bad fields", parse(r#"{"source":"  ","width":0,"height":9000,"format":"gif"}"#), invalid(&["source", "width", "height", "format"]));
}

#[test]
fn a_misspelled_field() {
    check!("\"formt\"", parse(r#"{"source":"a","width":1,"height":1,"formt":"jpg"}"#), Parsed::InvalidJson);
}

#[test]
fn one_error_shape() {
    let (status, code, details) = shape(ApiError::NotFound("42".to_string()));
    check!("NotFound: (status, code, details)", (status, code, details), (404, "job_not_found".to_string(), json!([])));
}

#[test]
fn validation_details_reach_the_client() {
    let got = shape(ApiError::Validation(vec!["width: too big".to_string(), "format: unknown".to_string()]));
    check!("Validation: (status, code, details)", got, (422, "validation_failed".to_string(), json!(["width: too big", "format: unknown"])));
}
