use solution::*;

#[allow(dead_code)]
fn limits() -> Limits {
    Limits::default()
}

#[allow(dead_code)]
fn parse(bytes: &[u8]) -> Parsed {
    parse_request(bytes, &Limits::default())
}

/// The request in `bytes`, which must be exactly one whole request.
#[allow(dead_code)]
fn whole(bytes: &[u8]) -> Request {
    match parse(bytes) {
        Parsed::Done(request, used) => {
            assert_eq!(used, bytes.len(), "bytes used by {:?}", String::from_utf8_lossy(bytes));
            request
        }
        other => panic!("expected a whole request from {:?}, got {other:?}", String::from_utf8_lossy(bytes)),
    }
}

#[allow(dead_code)]
fn bad(status: u16, bytes: &[u8]) {
    let got = match parse(bytes) {
        Parsed::Bad(e) => Some(e.status()),
        _ => None,
    };
    check!(format!("{:?}", String::from_utf8_lossy(&bytes[..bytes.len().min(200)])), got, Some(status));
}

/// A request built by hand, for calling handlers directly.
#[allow(dead_code)]
fn request(method: &str, path: &str, headers: &[(&str, &str)]) -> Request {
    Request {
        method: method.to_string(),
        path: path.to_string(),
        query: String::new(),
        minor: 1,
        headers: headers.iter().map(|(k, v)| (k.to_ascii_lowercase(), v.to_string())).collect(),
        body: Vec::new(),
    }
}

fn sample() -> Router {
    let mut r = Router::new();
    r.add("GET", "/", |_, _| Response::text(200, "root"));
    r.add("GET", "/hello", |_, _| Response::text(200, "hello"));
    r.add("GET", "/users/:id/posts/:post", |_, p| Response::text(200, format!("{} {}", p.get("id").unwrap_or("?"), p.get("post").unwrap_or("?"))));
    r.add("GET", "/files/*path", |_, p| Response::text(200, format!("[{}]", p.get("path").unwrap_or("?"))));
    r.add("POST", "/hello", |req, _| Response::new(200).bytes(req.body.clone()));
    r.add("PUT", "/only-put", |_, _| Response::new(204));
    r
}

fn body(r: &Response) -> String {
    match &r.body {
        Body::Bytes(b) => String::from_utf8_lossy(b).into_owned(),
        Body::Empty => String::new(),
        Body::Stream(_) => "<stream>".into(),
    }
}

fn get(r: &Router, path: &str) -> (u16, String) {
    let out = r.dispatch(&request("GET", path, &[]));
    (out.status, body(&out))
}

#[test]
fn literal_routes() {
    check!("/ and /hello", (get(&sample(), "/"), get(&sample(), "/hello")), ((200, "root".to_string()), (200, "hello".to_string())));
}

#[test]
fn path_values_are_captured() {
    check!("/users/7/posts/42", get(&sample(), "/users/7/posts/42"), (200, "7 42".to_string()));
}

#[test]
fn an_unknown_path_is_404() {
    check!("/nope", get(&sample(), "/nope").0, 404);
}

#[test]
fn a_known_path_with_another_method_is_405() {
    let out = sample().dispatch(&request("DELETE", "/hello", &[]));
    check!("DELETE /hello", (out.status, out.header_value("allow").map(str::to_string)), (405, Some("GET, POST".to_string())));
}

#[test]
fn a_panic_becomes_500() {
    let mut r = Router::new();
    r.add("GET", "/boom", |_, _| panic!("handler bug"));
    let app = App::new(r).wrap(catch_panics());
    check!("/boom", app.handle(&request("GET", "/boom", &[])).status, 500);
}

#[test]
fn request_ids_are_kept_or_made() {
    let app = App::new(sample()).wrap(request_ids());
    let kept = app.handle(&request("GET", "/hello", &[("X-Request-Id", "abc-1_2")]));
    check!("the client's id", kept.header_value("x-request-id").map(str::to_string), Some("abc-1_2".to_string()));
    let made = app.handle(&request("GET", "/hello", &[]));
    check!("a new id", made.header_value("x-request-id").map(|v| v.starts_with("req-")), Some(true));
}
