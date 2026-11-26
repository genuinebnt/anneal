use solution::*;

#[allow(dead_code, unused_imports)]
use std::cell::RefCell;
#[allow(dead_code, unused_imports)]
use std::io::{self, Write};
#[allow(dead_code, unused_imports)]
use std::rc::Rc;

#[allow(dead_code, unused_imports)]
fn reply(status: u16, headers: &[(&str, &str)], content: Content) -> Reply {
    Reply { status, reason: "Reason".into(), headers: headers.iter().map(|(a, b)| (a.to_string(), b.to_string())).collect(), content }
}

#[allow(dead_code, unused_imports)]
fn pieces(list: Vec<Vec<u8>>) -> Content {
    Content::Pieces(Box::new(list.into_iter()))
}

#[allow(dead_code, unused_imports)]
fn sent(r: Reply, head_only: bool) -> String {
    let mut out = Vec::new();
    send(&mut out, r, head_only).expect("sending to a Vec");
    String::from_utf8_lossy(&out).into_owned()
}

/// The body of a chunked or length-framed message, checking the framing.
#[allow(dead_code, unused_imports)]
fn body_of(message: &[u8]) -> Vec<u8> {
    let end = message.windows(4).position(|w| w == b"\r\n\r\n").expect("no empty line");
    let head = String::from_utf8_lossy(&message[..end]).to_ascii_lowercase();
    let mut rest = &message[end + 4..];
    if head.contains("transfer-encoding: chunked") {
        let mut body = Vec::new();
        loop {
            let line_end = rest.windows(2).position(|w| w == b"\r\n").expect("chunk size line");
            let size = usize::from_str_radix(std::str::from_utf8(&rest[..line_end]).expect("size is text"), 16).expect("size is hex");
            rest = &rest[line_end + 2..];
            if size == 0 {
                assert_eq!(rest, b"\r\n", "after the last chunk");
                return body;
            }
            body.extend_from_slice(&rest[..size]);
            assert_eq!(&rest[size..size + 2], b"\r\n", "after a chunk");
            rest = &rest[size + 2..];
        }
    }
    let n: usize = head.lines().find_map(|l| l.strip_prefix("content-length: ")).expect("no length").parse().unwrap();
    assert_eq!(rest.len(), n, "Content-Length against the bytes sent");
    rest.to_vec()
}

#[test]
fn a_full_body() {
    let out = sent(reply(200, &[("X-A", "1")], Content::Full(b"hello".to_vec())), false);
    check!("200 with 5 bytes", out, "HTTP/1.1 200 Reason\r\nX-A: 1\r\nContent-Length: 5\r\n\r\nhello".to_string());
}

#[test]
fn pieces_are_sent_in_chunks() {
    let out = sent(reply(200, &[], pieces(vec![b"hello".to_vec(), b" world, again".to_vec()])), false);
    check!("two pieces", out, "HTTP/1.1 200 Reason\r\nTransfer-Encoding: chunked\r\n\r\n5\r\nhello\r\nd\r\n world, again\r\n0\r\n\r\n".to_string());
}

#[test]
fn head_only_sends_no_body() {
    let out = sent(reply(200, &[], Content::Full(b"hello".to_vec())), true);
    check!("head only", out, "HTTP/1.1 200 Reason\r\nContent-Length: 5\r\n\r\n".to_string());
}

#[test]
fn a_304_has_no_framing() {
    let out = sent(reply(304, &[("ETag", "x")], Content::Full(b"old".to_vec())), false);
    check!("304", out, "HTTP/1.1 304 Reason\r\nETag: x\r\n\r\n".to_string());
}

#[test]
fn a_line_break_in_a_value_is_refused() {
    let mut out = Vec::new();
    let got = send(&mut out, reply(200, &[("Location", "/a\r\nSet-Cookie: x=1")], Content::Full(Vec::new())), false).map_err(|e| e.kind());
    check!("CRLF in a header value", (got, out.len()), (Err(io::ErrorKind::InvalidInput), 0));
}
