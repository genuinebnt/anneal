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
fn http_1_0_needs_no_host() {
    check!("HTTP/1.0 without Host", whole(b"GET / HTTP/1.0\r\n\r\n").minor, 0);
}

#[test]
fn request_lines_that_are_wrong() {
    for line in ["GET  / HTTP/1.1", "GET / HTTP/1.1 ", " GET / HTTP/1.1", "get / HTTP/1.1", "G3T / HTTP/1.1", " / HTTP/1.1", "GET hello HTTP/1.1", "GET / HTTP/2.0", "GET / http/1.1", "GET /", ""] {
        bad(400, format!("{line}\r\nHost: h\r\n\r\n").as_bytes());
    }
}

#[test]
fn headers_that_are_wrong() {
    for line in ["no colon", ": empty name", "Bad Name: v", "Name : v", "N\tame: v"] {
        bad(400, format!("GET / HTTP/1.1\r\nHost: h\r\n{line}\r\n\r\n").as_bytes());
    }
}

#[test]
fn a_value_may_hold_colons_and_inner_spaces() {
    let r = whole(b"GET / HTTP/1.1\r\nHost: a:80\r\nX-V:  one  two : three \r\n\r\n");
    check!("colons and spaces", (r.header("host"), r.header("x-v")), (Some("a:80"), Some("one  two : three")));
}

#[test]
fn the_first_of_two_equal_names_wins_the_lookup_and_both_are_kept() {
    let r = whole(b"GET / HTTP/1.1\r\nHost: h\r\nX: 1\r\nx: 2\r\n\r\n");
    check!("repeated header", (r.header("X"), r.headers.len()), (Some("1"), 3));
}

#[test]
fn a_binary_body_may_contain_blank_lines() {
    let body = [0u8, 255, 13, 10, 13, 10, 7];
    let mut bytes = b"POST / HTTP/1.1\r\nHost: h\r\nContent-Length: 7\r\n\r\n".to_vec();
    bytes.extend_from_slice(&body);
    check!("7 bytes of binary", whole(&bytes).body, body.to_vec());
}

#[test]
fn a_head_that_is_not_utf8_is_bad() {
    bad(400, b"GET /\xff HTTP/1.1\r\nHost: h\r\n\r\n");
}

#[test]
fn bytes_used_stops_at_the_end_of_the_first_request() {
    let two = b"POST /a HTTP/1.1\r\nHost: h\r\nContent-Length: 2\r\n\r\nhiGET /b HTTP/1.1\r\nHost: h\r\n\r\n";
    let first = match parse(two) {
        Parsed::Done(r, used) => (r.path, r.body, used),
        other => panic!("{other:?}"),
    };
    let first_len = two.len() - b"GET /b HTTP/1.1\r\nHost: h\r\n\r\n".len();
    check!("two requests in one buffer", first, ("/a".to_string(), b"hi".to_vec(), first_len));
}

#[test]
fn status_codes_of_the_errors() {
    check!("statuses", [HttpError::BadRequest, HttpError::UriTooLong, HttpError::HeaderTooLarge, HttpError::BodyTooLarge].map(HttpError::status), [400, 414, 431, 413]);
}

#[test]
fn random_well_formed_requests() {
    let mut rng = anneal_prelude::Rng::new(81001);
    for _ in 0..200 {
        let method = *rng.pick(&["GET", "POST", "PUT", "DELETE", "HEAD"]);
        let len = rng.below(8);
        let path = format!("/{}", rng.string(len, "ab/-._~"));
        let body_len = rng.below(20);
        let body: Vec<u8> = rng.vec(body_len, 0, 255);
        let mut bytes = format!("{method} {path} HTTP/1.1\r\nHost: h\r\n").into_bytes();
        let mut want = vec![("host".to_string(), "h".to_string())];
        for i in 0..rng.below(4) {
            let value_len = rng.below(6);
            let value = rng.string(value_len, "ab=;,: ").trim().to_string();
            bytes.extend_from_slice(format!("X-{i}: {value}\r\n").as_bytes());
            want.push((format!("x-{i}"), value));
        }
        if !body.is_empty() {
            bytes.extend_from_slice(format!("Content-Length: {}\r\n", body.len()).as_bytes());
            want.push(("content-length".to_string(), body.len().to_string()));
        }
        bytes.extend_from_slice(b"\r\n");
        bytes.extend_from_slice(&body);
        let r = whole(&bytes);
        check!(format!("{:?}", String::from_utf8_lossy(&bytes)), (r.method, r.path, r.headers, r.body), (method.to_string(), path, want, body));
    }
}
