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
fn the_whole_table() {
    let table = [
        (ApiError::MissingKey, 400, "missing_idempotency_key"),
        (ApiError::InvalidJson("line 1".into()), 422, "invalid_json"),
        (ApiError::KeyReuse, 409, "idempotency_key_reuse"),
        (ApiError::NotFound("abc".into()), 404, "job_not_found"),
        (ApiError::NotCancellable("succeeded".into()), 409, "not_cancellable"),
        (ApiError::BadQuery("limit=x".into()), 400, "bad_query"),
        (ApiError::ShuttingDown, 503, "shutting_down"),
        (ApiError::NoRoute, 404, "no_route"),
        (ApiError::MethodNotAllowed, 405, "method_not_allowed"),
    ];
    for (error, status, code) in table {
        let shown = format!("{error:?}");
        check!(shown, shape(error), (status, code.to_string(), json!([])));
    }
}

#[test]
fn messages_carry_what_is_known() {
    for (error, needle) in [(ApiError::NotFound("7031".into()), "7031"), (ApiError::NotCancellable("failed".into()), "failed"), (ApiError::BadQuery("limit=nine".into()), "limit=nine")] {
        let message = reply(error.clone()).2["error"]["message"].as_str().unwrap_or("").to_string();
        check!(format!("message of {error:?} ({message:?}) contains {needle:?}"), message.contains(needle), true);
    }
}

#[test]
fn the_limits_are_inclusive() {
    check!("1 x 8192, work_ms 10000", parse(r#"{"source":"a","width":1,"height":8192,"work_ms":10000}"#), Parsed::Ok(spec("a", 1, 8192, "png", 10000)));
    check!("8192 x 1, work_ms 0", parse(r#"{"source":"a","width":8192,"height":1,"work_ms":0}"#), Parsed::Ok(spec("a", 8192, 1, "png", 0)));
    check!("8193 and 0", parse(r#"{"source":"a","width":8193,"height":0}"#), invalid(&["width", "height"]));
    check!("work_ms 10001", parse(r#"{"source":"a","width":1,"height":1,"work_ms":10001}"#), invalid(&["work_ms"]));
}

#[test]
fn five_problems_in_field_order() {
    check!("all bad, written backwards", parse(r#"{"work_ms":99999,"format":"PNG","height":0,"width":0,"source":""}"#), invalid(&["source", "width", "height", "format", "work_ms"]));
}

#[test]
fn the_source_is_kept_as_sent() {
    check!("spaces around a name", parse(r#"{"source":" a b ","width":1,"height":1}"#), Parsed::Ok(spec(" a b ", 1, 1, "png", 20)));
    check!("only white space", parse("{\"source\":\" \\t\\n\",\"width\":1,\"height\":1}"), invalid(&["source"]));
}

#[test]
fn formats_are_exact() {
    for format in ["png", "jpg", "webp"] {
        check!(format, parse(&format!(r#"{{"source":"a","width":1,"height":1,"format":"{format}"}}"#)), Parsed::Ok(spec("a", 1, 1, format, 20)));
    }
    for format in ["jpeg", "Jpg", " png", ""] {
        check!(format!("format {format:?}"), parse(&format!(r#"{{"source":"a","width":1,"height":1,"format":"{format}"}}"#)), invalid(&["format"]));
    }
}

#[test]
fn what_does_not_fit_the_types() {
    for body in [
        "", "{", "null", "[]", "\"job\"", r#"{}"#, r#"["a.png",1,1,"png",20]"#, r#" ["a.png",10,20] "#,
        r#"{"width":1,"height":1}"#,
        r#"{"source":"a","height":1}"#,
        r#"{"source":"a","width":1}"#,
        r#"{"source":null,"width":1,"height":1}"#,
        r#"{"source":"a","width":"1","height":1}"#,
        r#"{"source":"a","width":-1,"height":1}"#,
        r#"{"source":"a","width":1.5,"height":1}"#,
        r#"{"source":"a","width":4294967296,"height":1}"#,
        r#"{"source":"a","width":1,"height":1,"format":null}"#,
        r#"{"source":"a","width":1,"height":1,"work_ms":-5}"#,
        r#"{"source":"a","width":1,"height":1,"priority":9}"#,
        r#"{"source":"a","width":1,"height":1} {}"#,
        r#"{"source":"a","width":0,"height":1,"extra":true}"#,
    ] {
        check!(format!("body {body}"), parse(body), Parsed::InvalidJson);
    }
}

#[test]
fn bytes_that_are_not_text() {
    check!("invalid UTF-8", matches!(parse_spec(&[b'{', 0xff, b'}']), Err(ApiError::InvalidJson(_))), true);
}

#[test]
fn errors_can_be_compared_and_copied() {
    let a = ApiError::Validation(vec!["width: x".into()]);
    check!("clone equals the original", a.clone() == a, true);
    check!("different variants differ", ApiError::KeyReuse == ApiError::MissingKey, false);
}

#[test]
fn random_jobs_match_a_model() {
    let mut rng = anneal_prelude::Rng::new(22001);
    for _ in 0..400 {
        let source = *rng.pick(&["", " ", "a", " x ", "cat.png"]);
        let width = *rng.pick(&[0u32, 1, 640, 8192, 8193, u32::MAX]);
        let height = *rng.pick(&[0u32, 1, 480, 8192, 8193]);
        let format = rng.bool().then(|| *rng.pick(&["png", "jpg", "webp", "gif", "Webp"]));
        let work_ms = rng.bool().then(|| *rng.pick(&[0u64, 7, 10_000, 10_001, u64::MAX]));
        let mut body = json!({"source": source, "width": width, "height": height});
        if let Some(f) = format {
            body["format"] = json!(f);
        }
        if let Some(w) = work_ms {
            body["work_ms"] = json!(w);
        }
        let (format, work_ms) = (format.unwrap_or("png"), work_ms.unwrap_or(20));
        let mut bad = Vec::new();
        if source.trim().is_empty() {
            bad.push("source");
        }
        for (name, v) in [("width", width), ("height", height)] {
            if v == 0 || v > 8192 {
                bad.push(name);
            }
        }
        if !["png", "jpg", "webp"].contains(&format) {
            bad.push("format");
        }
        if work_ms > 10_000 {
            bad.push("work_ms");
        }
        let want = if bad.is_empty() { Parsed::Ok(spec(source, width, height, format, work_ms)) } else { invalid(&bad) };
        check!(format!("{body}"), parse(&body.to_string()), want);
    }
}
