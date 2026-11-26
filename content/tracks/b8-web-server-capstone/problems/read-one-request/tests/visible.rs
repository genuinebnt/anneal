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

#[test]
fn a_simple_get() {
    let r = whole(b"GET /hello HTTP/1.1\r\nHost: example\r\n\r\n");
    check!("GET /hello", (r.method.as_str(), r.path.as_str(), r.query.as_str(), r.minor, r.body.len()), ("GET", "/hello", "", 1, 0));
}

#[test]
fn headers_are_lower_cased_and_trimmed() {
    let r = whole(b"GET / HTTP/1.1\r\nHost:  a.example \r\nX-Tag:\tone\r\n\r\n");
    check!("two headers", r.headers.clone(), vec![("host".to_string(), "a.example".to_string()), ("x-tag".to_string(), "one".to_string())]);
    check!("lookup in any case", (r.header("X-TAG"), r.header("x-tag"), r.header("missing")), (Some("one"), Some("one"), None));
}

#[test]
fn the_query_is_split_off_at_the_first_question_mark() {
    let r = whole(b"GET /find?q=a?b&x=1 HTTP/1.1\r\nHost: h\r\n\r\n");
    check!("/find?q=a?b&x=1", (r.path.as_str(), r.query.as_str()), ("/find", "q=a?b&x=1"));
}

#[test]
fn a_body_of_the_announced_length() {
    let r = whole(b"POST /up HTTP/1.1\r\nHost: h\r\nContent-Length: 5\r\n\r\nhello");
    check!("POST with 5 bytes", r.body, b"hello".to_vec());
}

#[test]
fn a_request_line_without_a_version_is_bad() {
    bad(400, b"GET /\r\nHost: h\r\n\r\n");
}

#[test]
fn http_1_1_needs_a_host() {
    bad(400, b"GET / HTTP/1.1\r\n\r\n");
}
