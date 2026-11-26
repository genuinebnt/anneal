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

/// Takes a few bytes per call and remembers what has been flushed.
#[derive(Clone)]
struct Sink {
    state: Rc<RefCell<(Vec<u8>, usize)>>,
    step: usize,
    room: usize,
}

impl Sink {
    fn new(step: usize, room: usize) -> Sink {
        Sink { state: Rc::default(), step, room }
    }
}

impl Write for Sink {
    fn write(&mut self, b: &[u8]) -> io::Result<usize> {
        let mut s = self.state.borrow_mut();
        if s.0.len() >= self.room {
            return Err(io::Error::new(io::ErrorKind::BrokenPipe, "gone"));
        }
        let n = b.len().min(self.step).min(self.room - s.0.len());
        s.0.extend_from_slice(&b[..n]);
        Ok(n)
    }
    fn flush(&mut self) -> io::Result<()> {
        let mut s = self.state.borrow_mut();
        s.1 = s.0.len();
        Ok(())
    }
}

#[test]
fn a_sink_that_takes_one_byte_at_a_time() {
    for content in [Content::Full(vec![7; 50]), pieces(vec![vec![7; 20], vec![7; 30]])] {
        let mut sink = Sink::new(1, usize::MAX);
        send(&mut sink, reply(200, &[], content), false).unwrap();
        let state = sink.state.borrow();
        check!("all 50 bytes arrive", body_of(&state.0).len(), 50);
    }
}

#[test]
fn each_piece_is_flushed_before_the_next_is_made() {
    let sink = Sink::new(5, usize::MAX);
    let seen = sink.state.clone();
    let content = Content::Pieces(Box::new((0..6).map(move |i| {
        let s = seen.borrow();
        let flushed = String::from_utf8_lossy(&s.0[..s.1]).into_owned();
        for earlier in 0..i {
            assert!(flushed.contains(&format!("<{earlier}>\r\n")), "piece {earlier} was not flushed when piece {i} was asked for; flushed: {flushed:?}");
        }
        format!("<{i}>").into_bytes()
    })));
    let mut w = sink.clone();
    send(&mut w, reply(200, &[], content), false).unwrap();
}

#[test]
fn the_end_is_flushed_too() {
    for content in [Content::Full(b"abc".to_vec()), pieces(vec![b"abc".to_vec()]), Content::Full(Vec::new())] {
        let mut sink = Sink::new(1000, usize::MAX);
        send(&mut sink, reply(200, &[], content), false).unwrap();
        let s = sink.state.borrow();
        check!("flushed bytes", s.1, s.0.len());
    }
}

#[test]
fn a_failing_sink_stops_the_pieces() {
    let pulled = Rc::new(RefCell::new(0));
    let p = pulled.clone();
    let content = Content::Pieces(Box::new((0..1_000_000).map(move |_| {
        *p.borrow_mut() += 1;
        assert!(*p.borrow() < 10_000, "still asking for pieces long after the sink failed");
        vec![b'x'; 50]
    })));
    let got = send(&mut Sink::new(1000, 500), reply(200, &[], content), false).map_err(|e| e.kind());
    check!("error", got, Err(io::ErrorKind::BrokenPipe));
    let n = *pulled.borrow();
    assert!(n < 20, "{n} pieces were made although the sink failed after 500 bytes");
}

#[test]
fn a_failing_sink_on_a_full_body() {
    for room in [0, 5, 30] {
        let got = send(&mut Sink::new(4, room), reply(200, &[], Content::Full(vec![1; 100])), false).map_err(|e| e.kind());
        check!(format!("room for {room} bytes"), got, Err(io::ErrorKind::BrokenPipe));
    }
}

#[test]
fn empty_pieces_do_not_end_the_body() {
    let out = sent(reply(200, &[], pieces(vec![vec![], b"a".to_vec(), vec![], b"b".to_vec()])), false);
    check!("empty pieces", body_of(out.as_bytes()), b"ab".to_vec());
}

#[test]
fn no_pieces_at_all() {
    check!("empty stream", body_of(sent(reply(200, &[], pieces(vec![])), false).as_bytes()), Vec::<u8>::new());
}

#[test]
fn head_only_for_pieces_asks_for_nothing_it_must_send() {
    let out = sent(reply(200, &[], pieces(vec![b"secret".to_vec()])), true);
    check!("head only, chunked", out, "HTTP/1.1 200 Reason\r\nTransfer-Encoding: chunked\r\n\r\n".to_string());
}

#[test]
fn statuses_without_a_body() {
    for status in [100, 101, 204, 304] {
        let out = sent(reply(status, &[], pieces(vec![b"x".to_vec()])), false);
        check!(format!("{status}"), out, format!("HTTP/1.1 {status} Reason\r\n\r\n"));
    }
    check!("200 with nothing", sent(reply(200, &[], Content::Full(vec![])), false), "HTTP/1.1 200 Reason\r\nContent-Length: 0\r\n\r\n".to_string());
}

#[test]
fn anything_that_would_break_the_framing_is_refused_before_writing() {
    let bad: Vec<Reply> = vec![
        Reply { status: 200, reason: "OK\r\nX: 1".into(), headers: vec![], content: Content::Full(vec![]) },
        reply(200, &[("A\n", "1")], Content::Full(vec![])),
        reply(200, &[("A", "1\r")], Content::Full(vec![])),
        reply(200, &[("Ok", "1"), ("B", "x\ny")], Content::Full(vec![])),
        reply(200, &[("content-LENGTH", "9")], Content::Full(vec![])),
        reply(200, &[("Transfer-Encoding", "chunked")], pieces(vec![])),
        reply(304, &[("X", "\r\n")], Content::Full(vec![])),
    ];
    for (i, r) in bad.into_iter().enumerate() {
        let mut out = Vec::new();
        let got = send(&mut out, r, false).map_err(|e| e.kind());
        check!(format!("bad reply {i}"), (got, out.len()), (Err(io::ErrorKind::InvalidInput), 0));
    }
}

#[test]
fn lengths_count_bytes_and_repeated_headers_stay() {
    let out = sent(reply(200, &[("S", "a"), ("S", "b")], Content::Full("é€".as_bytes().to_vec())), false);
    check!("two headers, 5 bytes", out, "HTTP/1.1 200 Reason\r\nS: a\r\nS: b\r\nContent-Length: 5\r\n\r\né€".to_string());
}

#[test]
fn random_replies() {
    let mut rng = anneal_prelude::Rng::new(70901);
    for _ in 0..200 {
        let mut list = Vec::new();
        for _ in 0..rng.below(5) {
            let len = rng.below(200);
            list.push(rng.vec::<u8>(len, 0, 255));
        }
        let want = list.concat();
        let chunked = rng.bool();
        let content = if chunked { pieces(list) } else { Content::Full(want.clone()) };
        let total = want.len();
        let step = 1 + rng.below(40);
        let mut sink = Sink::new(step, usize::MAX);
        send(&mut sink, reply(200, &[], content), false).unwrap();
        let got = body_of(&sink.state.borrow().0);
        check!(format!("chunked {chunked}, {step} bytes per write, {total} body bytes"), got, want);
    }
}
