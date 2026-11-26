use solution::*;

#[allow(dead_code, unused_imports)]
fn message(verb: &str, name: &str, headers: &[(&str, &str)], body: &[u8]) -> Message {
    Message { verb: verb.into(), name: name.into(), headers: headers.iter().map(|(a, b)| (a.to_string(), b.to_string())).collect(), body: body.to_vec() }
}

#[allow(dead_code, unused_imports)]
fn show(bytes: &[u8]) -> String {
    format!("{:?}", String::from_utf8_lossy(&bytes[..bytes.len().min(300)]))
}

#[test]
fn every_prefix_needs_more() {
    let bytes = b"PUT n MSG/1\r\nX: y\r\nContent-Length: 6\r\n\r\n\r\n\r\nab";
    for cut in 0..bytes.len() {
        check!(show(&bytes[..cut]), read_one(&bytes[..cut]), Read::NeedMore);
    }
    check!(show(bytes), matches!(read_one(bytes), Read::Whole(m, n) if n == bytes.len() && m.body == b"\r\n\r\nab"), true);
}

#[test]
fn a_body_may_be_any_bytes() {
    let mut bytes = b"PUT n MSG/1\r\nContent-Length: 4\r\n\r\n".to_vec();
    bytes.extend_from_slice(&[0xff, 0xfe, 0, 10]);
    let mut buf = bytes.clone();
    buf.extend_from_slice(b"GET z MSG/1\r\n\r\n\xff");
    let got = take_all(&mut buf).map(|v| v.into_iter().map(|m| m.body).collect::<Vec<_>>());
    check!("binary bodies", (got, buf), (Ok(vec![vec![0xff, 0xfe, 0, 10], vec![]]), vec![0xff]));
}

#[test]
fn headers_keep_order_case_folded_and_trimmed() {
    let bytes = b"GET n MSG/1\r\nB-Key:\t two words \r\nA:\r\nb-key: again: here\r\n\r\n";
    check!(show(bytes), read_one(bytes), Read::Whole(message("GET", "n", &[("b-key", "two words"), ("a", ""), ("b-key", "again: here")], b""), bytes.len()));
}

#[test]
fn first_lines_that_are_invalid() {
    for line in ["GET a", "GET  a MSG/1", "GET a MSG/1 ", " GET a MSG/1", "GET a msg/1", "GET  MSG/1", " a MSG/1", "", "GET a b MSG/1"] {
        let bytes = format!("{line}\r\n\r\n").into_bytes();
        check!(show(&bytes), read_one(&bytes), Read::Invalid);
    }
}

#[test]
fn header_lines_that_are_invalid() {
    for line in ["novalue", ": x"] {
        let bytes = format!("GET a MSG/1\r\n{line}\r\n\r\n").into_bytes();
        check!(show(&bytes), read_one(&bytes), Read::Invalid);
    }
}

#[test]
fn lengths_that_are_invalid() {
    for headers in ["Content-Length: +1", "Content-Length: -1", "Content-Length: 1.0", "Content-Length:", "Content-Length: 1\r\ncontent-length: 1", "Content-Length: 99999999999999999999999999"] {
        let bytes = format!("GET a MSG/1\r\n{headers}\r\n\r\nx").into_bytes();
        check!(show(&bytes), read_one(&bytes), Read::Invalid);
    }
}

#[test]
fn the_largest_length_just_waits() {
    let bytes = format!("GET a MSG/1\r\nContent-Length: {}\r\n\r\nx", usize::MAX).into_bytes();
    check!(show(&bytes), read_one(&bytes), Read::NeedMore);
}

#[test]
fn a_head_that_is_not_text_is_invalid() {
    check!("0xff in the name", read_one(b"GET \xff MSG/1\r\n\r\n"), Read::Invalid);
}

#[test]
fn nothing_is_invalid_before_the_head_is_complete() {
    check!("nonsense so far", read_one(b"complete nonsense\r\nmore"), Read::NeedMore);
}

#[test]
fn random_streams_cut_anywhere_give_the_same_requests() {
    let mut rng = anneal_prelude::Rng::new(70601);
    for _ in 0..300 {
        let mut stream = Vec::new();
        let mut want = Vec::new();
        for _ in 0..rng.below(5) {
            let verb = rng.pick(&["GET", "PUT", "x"]).to_string();
            let name_len = 1 + rng.below(5);
            let name = rng.string(name_len, "abc/.");
            let body_len = rng.below(12);
            let body: Vec<u8> = if rng.bool() { rng.vec(body_len, 0, 255) } else { rng.string(body_len, "\r\n:a").into_bytes() };
            let mut headers = Vec::new();
            if !body.is_empty() || rng.bool() {
                headers.push(("content-length".to_string(), body.len().to_string()));
            }
            let mut text = format!("{verb} {name} MSG/1\r\n");
            for (k, v) in &headers {
                text.push_str(&format!("{}: {v}\r\n", k.to_uppercase()));
            }
            text.push_str("\r\n");
            stream.extend_from_slice(text.as_bytes());
            stream.extend_from_slice(&body);
            want.push(Message { verb, name, headers, body });
        }
        let mut buf = Vec::new();
        let mut got = Vec::new();
        let mut at = 0;
        let mut cuts = Vec::new();
        while at < stream.len() {
            let n = (1 + rng.below(25)).min(stream.len() - at);
            buf.extend_from_slice(&stream[at..at + n]);
            at += n;
            cuts.push(at);
            match take_all(&mut buf) {
                Ok(mut more) => got.append(&mut more),
                Err(e) => panic!("valid stream {} reported invalid: {e:?}", show(&stream)),
            }
        }
        check!(format!("{} cut at {cuts:?}", show(&stream)), (got, buf.len()), (want, 0));
    }
}

#[test]
fn many_small_requests_at_once() {
    let mut buf = b"GET a MSG/1\r\n\r\n".repeat(20_000);
    check!("20000 requests in one buffer", take_all(&mut buf).map(|v| v.len()), Ok(20_000));
    check!("all used", buf.len(), 0);
}
