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

fn small() -> Limits {
    Limits { max_request_line: 30, max_header_bytes: 200, max_headers: 4, max_body: 16 }
}

/// A prefix of a good request is never refused.
fn every_prefix_waits(bytes: &[u8], lim: &Limits) {
    for cut in 0..bytes.len() {
        let got = parse_request(&bytes[..cut], lim);
        assert!(matches!(got, Parsed::Incomplete { .. }), "first {cut} bytes of {:?} gave {got:?}", String::from_utf8_lossy(bytes));
    }
    let got = parse_request(bytes, lim);
    assert!(matches!(got, Parsed::Done(_, n) if n == bytes.len()), "{:?} gave {got:?}", String::from_utf8_lossy(bytes));
}

#[test]
fn every_prefix_of_plain_and_chunked_requests_waits() {
    let lim = Limits::default();
    every_prefix_waits(b"GET /a?b HTTP/1.1\r\nHost: h\r\n\r\n", &lim);
    every_prefix_waits(b"POST / HTTP/1.1\r\nHost: h\r\nContent-Length: 4\r\n\r\n\r\n\r\n", &lim);
    every_prefix_waits(b"POST / HTTP/1.1\r\nHost: h\r\nTransfer-Encoding: Chunked\r\n\r\nA\r\n\r\n23456789\r\n1;name=value\r\nz\r\n0\r\nT: 1\r\nU: 2\r\n\r\n", &lim);
}

#[test]
fn a_request_exactly_at_every_limit_arrives_byte_by_byte() {
    let lim = small();
    // 30 bytes of request line, four headers, a 16-byte body
    let line = format!("POST /{} HTTP/1.1", "a".repeat(30 - 15));
    assert_eq!(line.len(), 30);
    let mut bytes = format!("{line}\r\nHost: h\r\nA: 1\r\nB: 2\r\nContent-Length: 16\r\n\r\n").into_bytes();
    bytes.extend_from_slice(&[b'x'; 16]);
    every_prefix_waits(&bytes, &lim);
}

#[test]
fn one_past_a_limit_is_refused() {
    let lim = small();
    let status = |b: &[u8]| match parse_request(b, &lim) {
        Parsed::Bad(e) => Some(e.status()),
        _ => None,
    };
    let long_line = format!("GET /{} HTTP/1.1\r\n", "a".repeat(31 - 14));
    check!("request line of 31 bytes", status(format!("{long_line}Host: h\r\n\r\n").as_bytes()), Some(414));
    check!("request line of 31 bytes, CRLF not here yet", status(&long_line.as_bytes()[..31]), Some(414));
    check!("five headers", status(b"GET / HTTP/1.1\r\nHost: h\r\nA: 1\r\nB: 1\r\nC: 1\r\nD: 1\r\n\r\n"), Some(431));
    check!("body of 17", status(b"POST / HTTP/1.1\r\nHost: h\r\nContent-Length: 17\r\n\r\n"), Some(413));
}

#[test]
fn an_endless_head_is_refused_without_waiting_for_its_end() {
    let lim = Limits::default();
    let line = vec![b'a'; 5000];
    check!("5000 bytes and no line end", parse_request(&line, &lim), Parsed::Bad(HttpError::UriTooLong));
    let mut head = b"GET / HTTP/1.1\r\n".to_vec();
    while head.len() <= 9000 {
        head.extend_from_slice(b"X: yyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyy\r\n");
    }
    check!("9000 bytes of headers and no blank line", parse_request(&head, &lim), Parsed::Bad(HttpError::HeaderTooLarge));
}

#[test]
fn ambiguous_lengths_are_400() {
    for headers in [
        "Content-Length: 1\r\nContent-Length: 2",
        "Content-Length: +3",
        "Content-Length: -1",
        "Content-Length: 3, 3",
        "Content-Length:",
        "Content-Length: 1x",
        "Transfer-Encoding: gzip",
        "Transfer-Encoding: gzip, chunked",
        "Transfer-Encoding: chunked\r\nTransfer-Encoding: chunked",
        "Transfer-Encoding: chunked\r\nTransfer-Encoding: identity",
        "X: 1\r\n folded",
        "X: 1\r\n\tfolded: yes",
    ] {
        bad(400, format!("POST / HTTP/1.1\r\nHost: h\r\n{headers}\r\n\r\n0\r\n\r\n").as_bytes());
    }
    bad(400, b"POST / HTTP/1.0\r\nTransfer-Encoding: chunked\r\n\r\n0\r\n\r\n");
}

#[test]
fn equal_repeated_lengths_are_accepted_and_numbers_too_big_are_413() {
    check!("same length twice", whole(b"POST / HTTP/1.1\r\nHost: h\r\nContent-Length: 2\r\nContent-Length: 2\r\n\r\nok").body, b"ok".to_vec());
    bad(413, b"POST / HTTP/1.1\r\nHost: h\r\nContent-Length: 99999999999999999999999\r\n\r\n");
}

#[test]
fn chunks_that_are_wrong() {
    let head = "POST / HTTP/1.1\r\nHost: h\r\nTransfer-Encoding: chunked\r\n\r\n";
    for body in ["g\r\n", "+5\r\nhello\r\n0\r\n\r\n", " 5\r\nhello\r\n0\r\n\r\n", "5 \r\nhello\r\n0\r\n\r\n", "\r\n", ";a\r\n", "000000005\r\nhello\r\n0\r\n\r\n", "5\r\nhelloXX0\r\n\r\n", "3\r\nhello\r\n0\r\n\r\n"] {
        bad(400, format!("{head}{body}").as_bytes());
    }
}

#[test]
fn a_long_chunk_line_is_refused_however_it_arrives() {
    let head = "POST / HTTP/1.1\r\nHost: h\r\nTransfer-Encoding: chunked\r\n\r\n";
    let ext = "e".repeat(200);
    bad(400, format!("{head}1;{ext}\r\nz\r\n0\r\n\r\n").as_bytes());
    bad(400, format!("{head}1;{ext}").as_bytes());
    // 64 bytes is still allowed, split anywhere
    every_prefix_waits(format!("{head}1;{}\r\nz\r\n0\r\n\r\n", "e".repeat(62)).as_bytes(), &Limits::default());
}

#[test]
fn chunked_bodies_obey_the_body_limit_as_soon_as_a_size_is_announced() {
    let head = "POST / HTTP/1.1\r\nHost: h\r\nTransfer-Encoding: chunked\r\n\r\n";
    bad(413, format!("{head}FFFFFF\r\n").as_bytes());
    let lim = small();
    let two = format!("{head}10\r\n{}\r\n1\r\n", "a".repeat(16));
    check!("16 then 1 more with a limit of 16", parse_request(two.as_bytes(), &lim), Parsed::Bad(HttpError::BodyTooLarge));
}

#[test]
fn endless_trailers_are_refused() {
    let mut bytes = b"POST / HTTP/1.1\r\nHost: h\r\nTransfer-Encoding: chunked\r\n\r\n0\r\n".to_vec();
    for i in 0..2000 {
        bytes.extend_from_slice(format!("T{i}: padding-padding\r\n").as_bytes());
    }
    bad(431, &bytes);
}

#[test]
fn expect_continue_is_reported_only_while_the_body_is_missing() {
    let head = b"POST / HTTP/1.1\r\nHost: h\r\nTransfer-Encoding: chunked\r\nExpect: 100-Continue\r\n\r\n";
    check!("chunked, nothing yet", parse(head), Parsed::Incomplete { expect_continue: true });
    check!("head incomplete", parse(&head[..head.len() - 1]), Parsed::Incomplete { expect_continue: false });
    bad(413, b"POST / HTTP/1.1\r\nHost: h\r\nContent-Length: 9999999\r\nExpect: 100-continue\r\n\r\n");
}

#[test]
fn random_bytes_never_panic_and_refusals_are_final() {
    let mut rng = anneal_prelude::Rng::new(81002);
    let lim = small();
    let pieces: [&str; 14] = ["GET ", "POST ", "/", " HTTP/1.1", "\r\n", "\r", "\n", "Host: h", "Content-Length: ", "3", "Transfer-Encoding: chunked", ": ", "a", "0"];
    for _ in 0..400 {
        let mut bytes = Vec::new();
        let n = rng.below(40);
        for _ in 0..n {
            bytes.extend_from_slice(rng.pick(&pieces).as_bytes());
        }
        let mut first_refusal = None;
        for cut in 0..=bytes.len() {
            let got = parse_request(&bytes[..cut], &lim);
            if let Some(e) = first_refusal {
                check!(format!("{:?}: refused earlier, now at {cut} bytes", String::from_utf8_lossy(&bytes)), got, Parsed::Bad(e));
            } else if let Parsed::Bad(e) = got {
                first_refusal = Some(e);
            } else if let Parsed::Done(_, used) = got {
                assert!(used <= cut);
                break;
            }
        }
    }
}

#[test]
fn random_chunkings_of_a_body_decode_to_the_same_bytes() {
    let mut rng = anneal_prelude::Rng::new(81003);
    for _ in 0..200 {
        let len = rng.below(60);
        let body: Vec<u8> = rng.vec(len, 0, 255);
        let mut bytes = b"POST / HTTP/1.1\r\nHost: h\r\nTransfer-Encoding: chunked\r\n\r\n".to_vec();
        let mut at = 0;
        while at < body.len() {
            let n = (1 + rng.below(20)).min(body.len() - at);
            let size = if rng.bool() { format!("{n:x}") } else { format!("{n:X}") };
            bytes.extend_from_slice(size.as_bytes());
            bytes.extend_from_slice(b"\r\n");
            bytes.extend_from_slice(&body[at..at + n]);
            bytes.extend_from_slice(b"\r\n");
            at += n;
        }
        bytes.extend_from_slice(b"0\r\n\r\n");
        check!(format!("body of {len} bytes"), whole(&bytes).body, body);
        let cut = rng.below(bytes.len());
        assert!(matches!(parse(&bytes[..cut]), Parsed::Incomplete { .. }), "cut at {cut}");
    }
}

#[test]
fn a_whole_trailer_section_obeys_the_limit_too() {
    let lim = small();
    let head = "POST / HTTP/1.1\r\nHost: h\r\nTransfer-Encoding: chunked\r\n\r\n0\r\n";
    // one trailer line and the empty line: exactly 200 bytes together
    let at = format!("{head}T: {}\r\n\r\n", "x".repeat(200 - 7));
    every_prefix_waits(at.as_bytes(), &lim);
    let over = format!("{head}T: {}\r\n\r\n", "x".repeat(200 - 6));
    check!("a complete trailer section of 201 bytes, limit 200", parse_request(over.as_bytes(), &lim), Parsed::Bad(HttpError::HeaderTooLarge));
}
