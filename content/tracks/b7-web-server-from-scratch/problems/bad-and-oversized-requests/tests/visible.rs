use solution::*;

#[allow(dead_code, unused_imports)]
const SMALL: Limits = Limits { max_line: 30, max_head: 120, max_headers: 3, max_body: 100 };
#[allow(dead_code, unused_imports)]
const WIDE: Limits = Limits { max_line: 2000, max_head: 8000, max_headers: 50, max_body: 10_000 };

#[allow(dead_code, unused_imports)]
fn show(bytes: &[u8]) -> String {
    format!("{:?}", String::from_utf8_lossy(&bytes[..bytes.len().min(200)]))
}

#[allow(dead_code, unused_imports)]
fn status(bytes: &[u8], limits: &Limits) -> Option<u16> {
    check_head(bytes, limits).err().map(Refusal::status)
}

#[allow(dead_code, unused_imports)]
fn accepted(bytes: &[u8], limits: &Limits) -> Head {
    match check_head(bytes, limits) {
        Ok(Some(head)) => head,
        other => panic!("expected {} to be accepted, got {other:?}", show(bytes)),
    }
}

/// Every strict prefix waits; the whole is accepted.
#[allow(dead_code, unused_imports)]
fn waits_then_accepts(bytes: &[u8], limits: &Limits) {
    for cut in 0..bytes.len() {
        check!(format!("first {cut} bytes of {}", show(bytes)), check_head(&bytes[..cut], limits), Ok(None));
    }
    accepted(bytes, limits);
}

#[test]
fn a_good_head() {
    let bytes = b"POST /notes HTTP/1.1\r\nHost: h\r\nContent-Length: 4\r\n\r\nbody";
    let head = accepted(bytes, &WIDE);
    check!(show(bytes), (head.method.as_str(), head.target.as_str(), head.body_len, head.head_len), ("POST", "/notes", 4, bytes.len() - 4));
}

#[test]
fn an_unfinished_head_waits() {
    check!("no empty line yet", check_head(b"GET / HTTP/1.1\r\nHost: h\r\n", &WIDE), Ok(None));
}

#[test]
fn a_long_line_is_refused_before_it_ends() {
    let bytes = [b'a'; 40];
    check!("40 bytes, limit 30, no line end", status(&bytes, &SMALL), Some(414));
}

#[test]
fn a_big_body_is_refused_from_its_announcement() {
    check!("Content-Length 101 of 100", status(b"POST / HTTP/1.1\r\nHost: h\r\nContent-Length: 101\r\n\r\n", &SMALL), Some(413));
}

#[test]
fn a_space_before_the_colon_is_refused() {
    check!("Content-Length :", status(b"POST / HTTP/1.1\r\nHost: h\r\nContent-Length : 4\r\n\r\n", &WIDE), Some(400));
}

#[test]
fn two_different_lengths_are_refused() {
    check!("4 and 5", status(b"POST / HTTP/1.1\r\nHost: h\r\nContent-Length: 4\r\nContent-Length: 5\r\n\r\n", &WIDE), Some(400));
}
