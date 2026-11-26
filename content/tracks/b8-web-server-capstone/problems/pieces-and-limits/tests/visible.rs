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

const TWO: &[u8] = b"GET /a HTTP/1.1\r\nHost: h\r\n\r\nGET /b HTTP/1.1\r\nHost: h\r\n\r\n";

#[test]
fn half_a_request_is_incomplete() {
    check!("head without the blank line", parse(b"GET / HTTP/1.1\r\nHost: a\r\n"), Parsed::Incomplete { expect_continue: false });
}

#[test]
fn pipelined_requests_report_the_bytes_used() {
    let used = match parse(TWO) {
        Parsed::Done(r, used) => (r.path, used),
        other => panic!("{other:?}"),
    };
    check!("two requests", used, ("/a".to_string(), 28));
}

#[test]
fn a_long_request_line_is_414() {
    bad(414, format!("GET /{} HTTP/1.1\r\nHost: h\r\n\r\n", "a".repeat(3000)).as_bytes());
}

#[test]
fn both_framings_at_once_are_400() {
    bad(400, b"POST / HTTP/1.1\r\nHost: h\r\nContent-Length: 3\r\nTransfer-Encoding: chunked\r\n\r\n0\r\n\r\n");
}

#[test]
fn a_chunked_body_is_joined() {
    let r = whole(b"POST / HTTP/1.1\r\nHost: h\r\nTransfer-Encoding: chunked\r\n\r\n5\r\nhello\r\n6;x=1\r\n world\r\n0\r\nT: x\r\n\r\n");
    check!("two chunks and a trailer", r.body, b"hello world".to_vec());
}

#[test]
fn a_huge_length_is_refused_before_the_body() {
    bad(413, b"POST / HTTP/1.1\r\nHost: h\r\nContent-Length: 10000000\r\n\r\n");
}

#[test]
fn waiting_for_a_body_after_expect_continue() {
    check!(
        "head complete, body absent",
        parse(b"POST / HTTP/1.1\r\nHost: h\r\nContent-Length: 4\r\nExpect: 100-continue\r\n\r\n"),
        Parsed::Incomplete { expect_continue: true }
    );
}
