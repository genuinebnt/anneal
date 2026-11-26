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
fn a_head_exactly_at_every_limit() {
    // a 30-byte line, three headers
    let line = format!("GET /{} HTTP/1.1", "a".repeat(16));
    assert_eq!(line.len(), 30);
    // 70 bytes without the padding in front of the value of A
    let pad = 120 - 70;
    let bytes = format!("{line}\r\nHost: h\r\nA: {}1\r\nContent-Length: 100\r\n\r\n", " ".repeat(pad));
    assert_eq!(bytes.len(), 120);
    waits_then_accepts(bytes.as_bytes(), &SMALL);
}

#[test]
fn each_limit_plus_one() {
    let line = format!("GET /{} HTTP/1.1", "a".repeat(17));
    assert_eq!(line.len(), 31);
    check!("line of 31, complete", status(format!("{line}\r\nHost: h\r\n\r\n").as_bytes(), &SMALL), Some(414));
    check!("line of 31, nothing after it", status(line.as_bytes(), &SMALL), Some(414));
    check!("line of 31 and half a CRLF", status(format!("{line}\r").as_bytes(), &SMALL), Some(414));
    let head = format!("GET / HTTP/1.1\r\nHost: h\r\nA: {}\r\n\r\n", "x".repeat(121 - 32));
    assert_eq!(head.len(), 121);
    check!("head of 121", status(head.as_bytes(), &SMALL), Some(431));
    check!("four headers", status(b"GET / HTTP/1.1\r\nHost: h\r\nA: 1\r\nB: 1\r\nC: 1\r\n\r\n", &SMALL), Some(431));
}

#[test]
fn an_endless_head_is_refused_before_it_ends() {
    let mut bytes = b"GET / HTTP/1.1\r\n".to_vec();
    while bytes.len() <= 120 {
        bytes.extend_from_slice(b"A: bbbbbbbb\r\n");
    }
    check!("headers without end", status(&bytes, &SMALL), Some(431));
}

#[test]
fn huge_numbers_are_too_large_not_bad() {
    check!("23 digits", status(b"POST / HTTP/1.1\r\nHost: h\r\nContent-Length: 99999999999999999999999\r\n\r\n", &WIDE), Some(413));
    check!("usize::MAX", status(format!("POST / HTTP/1.1\r\nHost: h\r\nContent-Length: {}\r\n\r\n", usize::MAX).as_bytes(), &WIDE), Some(413));
}

#[test]
fn lengths_that_are_not_plain_digits() {
    for value in ["+4", "-4", "4 4", "4,4", "0x4", "", "4.0", "four"] {
        let bytes = format!("POST / HTTP/1.1\r\nHost: h\r\nContent-Length: {value}\r\n\r\n");
        check!(show(bytes.as_bytes()), status(bytes.as_bytes(), &WIDE), Some(400));
    }
}

#[test]
fn equal_lengths_and_leading_zeros_are_fine() {
    check!("4 twice", accepted(b"POST / HTTP/1.1\r\nHost: h\r\nContent-Length: 4\r\ncontent-length: 04\r\n\r\n", &WIDE).body_len, 4);
}

#[test]
fn transfer_encoding_is_refused_in_any_case() {
    for header in ["Transfer-Encoding: chunked", "transfer-encoding: identity", "TRANSFER-ENCODING:"] {
        let bytes = format!("POST / HTTP/1.1\r\nHost: h\r\n{header}\r\n\r\n");
        check!(header, status(bytes.as_bytes(), &WIDE), Some(400));
    }
}

#[test]
fn other_refusals() {
    for text in [
        "GET / HTTP/1.1\r\n\r\n",
        "GET / HTTP/1.1\r\nHost: h\r\n folded\r\n\r\n",
        "GET / HTTP/1.1\r\nHost: h\r\n\tfolded: 1\r\n\r\n",
        "GET / HTTP/1.1\r\nHost: h\r\nnocolon\r\n\r\n",
        "GET / HTTP/1.1\r\nHost: h\r\n: v\r\n\r\n",
        "GET / HTTP/1.1\r\nHost: h\r\nA B: v\r\n\r\n",
        "GET  / HTTP/1.1\r\nHost: h\r\n\r\n",
        "GET / HTTP/3\r\nHost: h\r\n\r\n",
        "get / HTTP/1.1\r\nHost: h\r\n\r\n",
        "GET x HTTP/1.1\r\nHost: h\r\n\r\n",
    ] {
        check!(show(text.as_bytes()), status(text.as_bytes(), &WIDE), Some(400));
    }
    check!("not UTF-8", status(b"GET / HTTP/1.1\r\nHost: \xff\r\n\r\n", &WIDE), Some(400));
}

#[test]
fn http_1_0_needs_no_host_and_body_bytes_are_not_examined() {
    let head = accepted(b"POST / HTTP/1.0\r\nContent-Length: 2\r\n\r\n\xff\xfeextra", &WIDE);
    check!("1.0 with binary body", (head.body_len, head.head_len), (2, 38));
}

#[test]
fn status_codes() {
    check!("in order", [Refusal::BadRequest, Refusal::LineTooLong, Refusal::HeadTooLarge, Refusal::BodyTooLarge].map(Refusal::status), [400, 414, 431, 413]);
}

#[test]
fn random_good_heads_wait_then_pass() {
    let mut rng = anneal_prelude::Rng::new(70701);
    for _ in 0..150 {
        let len = rng.below(15);
        let target = format!("/{}", rng.string(len, "abc/?"));
        let mut text = format!("GET {target} HTTP/1.1\r\nHost: h\r\n");
        for i in 0..rng.below(3) {
            let n = rng.below(8);
            text.push_str(&format!("X{i}: {}\r\n", rng.string(n, "ab: \t")));
        }
        text.push_str("\r\n");
        let limits = Limits { max_line: 4 + target.len() + 9 + rng.below(3), max_head: text.len() + rng.below(3), max_headers: 3 + rng.below(2), max_body: 0 };
        waits_then_accepts(text.as_bytes(), &limits);
    }
}

#[test]
fn random_junk_never_changes_its_mind() {
    let mut rng = anneal_prelude::Rng::new(70702);
    let pieces = ["GET ", "/", " HTTP/1.1", "\r\n", "\r", "\n", "Host: h", "Content-Length: ", "7", "999", "Transfer-Encoding: x", ":", " ", "aaaaaaaaaaaaaaaaaaaa"];
    for _ in 0..400 {
        let mut bytes = Vec::new();
        for _ in 0..rng.below(25) {
            bytes.extend_from_slice(rng.pick(&pieces).as_bytes());
        }
        let mut refused = None;
        for cut in 0..=bytes.len() {
            let got = check_head(&bytes[..cut], &SMALL);
            if let Some(r) = refused {
                check!(format!("{} was refused at an earlier cut; now {cut} bytes", show(&bytes)), got, Err(r));
            } else if let Err(r) = got {
                refused = Some(r);
            } else if matches!(got, Ok(Some(_))) {
                break;
            }
        }
    }
}
