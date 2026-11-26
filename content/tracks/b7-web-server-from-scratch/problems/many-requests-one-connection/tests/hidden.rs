use solution::*;

#[allow(dead_code, unused_imports)]
use std::collections::VecDeque;
#[allow(dead_code, unused_imports)]
use std::io::{self, Read, Write};

/// One thing the client does.
#[derive(Clone)]
#[allow(dead_code, unused_imports)]
enum Step {
    Send(Vec<u8>),
    /// The client sends nothing more until the server's output contains this many replies (interim ones included).
    AwaitReplies(usize),
    Fail(io::ErrorKind),
}

/// A scripted client. It notices a server that waits for input while the client is waiting for the server.
#[allow(dead_code, unused_imports)]
struct Script {
    steps: VecDeque<Step>,
    out: Vec<u8>,
    flushed: usize,
    reads_after_end: usize,
}

#[allow(dead_code, unused_imports)]
impl Script {
    fn new(steps: Vec<Step>) -> Script {
        Script { steps: steps.into(), out: Vec::new(), flushed: 0, reads_after_end: 0 }
    }
    fn replies_seen(&self) -> usize {
        String::from_utf8_lossy(&self.out[..self.flushed]).matches("HTTP/1.1 ").count()
    }
    fn output(&self) -> String {
        String::from_utf8_lossy(&self.out).into_owned()
    }
    /// "200 keep-alive body" for each reply, "100" for permissions.
    fn replies(&self) -> Vec<String> {
        let output = self.output();
        let mut out = Vec::new();
        let mut rest = output.as_str();
        while !rest.is_empty() {
            let (head, after) = rest.split_once("\r\n\r\n").unwrap_or_else(|| panic!("incomplete reply: {rest:?}"));
            let status = head.split(' ').nth(1).expect("status");
            let length: usize = head.lines().find_map(|l| l.strip_prefix("Content-Length: ")).map_or(0, |v| v.parse().unwrap());
            let connection = head.lines().find_map(|l| l.strip_prefix("Connection: ")).map_or(String::new(), |v| format!(" {v}"));
            let body = &after[..length];
            out.push(format!("{status}{connection}{}{body}", if body.is_empty() { "" } else { " " }));
            rest = &after[length..];
        }
        out
    }
}

#[allow(dead_code, unused_imports)]
impl Read for Script {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        loop {
            match self.steps.front_mut() {
                None => {
                    self.reads_after_end += 1;
                    assert!(self.reads_after_end < 5, "the server keeps reading after the end of the stream");
                    return Ok(0);
                }
                Some(Step::AwaitReplies(n)) => {
                    let n = *n;
                    assert!(self.replies_seen() >= n, "the server reads for more input, but the client is waiting for reply number {n}; flushed output so far: {:?}", String::from_utf8_lossy(&self.out[..self.flushed]));
                    self.steps.pop_front();
                }
                Some(Step::Fail(kind)) => {
                    let kind = *kind;
                    self.steps.pop_front();
                    return Err(kind.into());
                }
                Some(Step::Send(bytes)) => {
                    let n = bytes.len().min(buf.len());
                    buf[..n].copy_from_slice(&bytes[..n]);
                    bytes.drain(..n);
                    if bytes.is_empty() {
                        self.steps.pop_front();
                    }
                    return Ok(n);
                }
            }
        }
    }
}

#[allow(dead_code, unused_imports)]
impl Write for Script {
    fn write(&mut self, b: &[u8]) -> io::Result<usize> {
        // a few bytes at a time, as a socket may
        let n = b.len().min(11);
        self.out.extend_from_slice(&b[..n]);
        Ok(n)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.flushed = self.out.len();
        Ok(())
    }
}

#[allow(dead_code, unused_imports)]
fn send(text: impl AsRef<[u8]>) -> Step {
    Step::Send(text.as_ref().to_vec())
}

#[allow(dead_code, unused_imports)]
fn get(path: &str) -> String {
    format!("GET {path} HTTP/1.1\r\nHost: h\r\n\r\n")
}

#[allow(dead_code, unused_imports)]
fn run(steps: Vec<Step>, max: usize) -> (Option<usize>, Vec<String>) {
    let mut script = Script::new(steps);
    let served = serve(&mut script, max, |r| format!("{}:{}", r.path, String::from_utf8_lossy(&r.body))).ok();
    (served, script.replies())
}

#[allow(dead_code, unused_imports)]
fn list(items: &[&str]) -> Vec<String> {
    items.iter().map(|s| s.to_string()).collect()
}

#[test]
fn http_1_0_closes_by_default() {
    let got = run(vec![send("GET /a HTTP/1.0\r\n\r\nGET /b HTTP/1.0\r\n\r\n")], 10);
    check!("plain 1.0", got, (Some(1), list(&["200 close /a:"])));
}

#[test]
fn http_1_0_may_ask_to_stay() {
    let got = run(vec![send("GET /a HTTP/1.0\r\nConnection: Keep-Alive\r\n\r\nGET /b HTTP/1.0\r\n\r\n")], 10);
    check!("1.0 keep-alive then plain", got, (Some(2), list(&["200 keep-alive /a:", "200 close /b:"])));
}

#[test]
fn the_connection_header_is_a_list_in_any_case() {
    for value in ["Close", "upgrade, CLOSE", "keep-alive, close", " close "] {
        let got = run(vec![send(format!("GET /a HTTP/1.1\r\nConnection: {value}\r\n\r\n"))], 10);
        check!(value, got, (Some(1), list(&["200 close /a:"])));
    }
    let got = run(vec![send("GET /a HTTP/1.1\r\nConnection: closed, not-close\r\n\r\n")], 10);
    check!("other tokens", got, (Some(1), list(&["200 keep-alive /a:"])));
    let got = run(vec![send("GET /a HTTP/1.1\r\nConnection: te\r\nConnection: close\r\n\r\n")], 10);
    check!("two Connection headers", got, (Some(1), list(&["200 close /a:"])));
}

#[test]
fn the_end_of_the_stream_with_half_a_request() {
    check!("half", run(vec![send("GET /a HT")], 10), (Some(0), vec![]));
    check!("nothing", run(vec![], 10), (Some(0), vec![]));
}

#[test]
fn a_request_in_single_bytes() {
    let mut steps: Vec<Step> = get("/slow").bytes().map(|b| send([b])).collect();
    steps.push(Step::AwaitReplies(1));
    check!("byte by byte", run(steps, 10), (Some(1), list(&["200 keep-alive /slow:"])));
}

#[test]
fn a_reply_is_not_held_back_by_the_start_of_the_next_request() {
    let second = get("/b");
    let got = run(vec![send(get("/a") + &second[..7]), Step::AwaitReplies(1), send(&second[7..]), Step::AwaitReplies(2)], 10);
    check!("one and a half, then the rest", got.0, Some(2));
}

#[test]
fn permission_is_given_once_per_request() {
    let head = "POST /p HTTP/1.1\r\nExpect: 100-continue\r\nContent-Length: 3\r\n\r\n";
    let steps = vec![send(head), Step::AwaitReplies(1), send("a"), send("b"), send("c"), Step::AwaitReplies(2), send(head), Step::AwaitReplies(3), send("xyz"), Step::AwaitReplies(4)];
    check!("two uploads", run(steps, 10), (Some(2), list(&["100", "200 keep-alive /p:abc", "100", "200 keep-alive /p:xyz"])));
}

#[test]
fn no_permission_needed_when_the_body_came_along() {
    let got = run(vec![send("POST /p HTTP/1.1\r\nExpect: 100-continue\r\nContent-Length: 2\r\n\r\nhi")], 10);
    check!("body already here", got, (Some(1), list(&["200 keep-alive /p:hi"])));
}

#[test]
fn invalid_after_good_ones() {
    let got = run(vec![send(get("/a") + "nonsense\r\n\r\n" + &get("/c"))], 10);
    check!("good, bad, never", got, (Some(1), list(&["200 keep-alive /a:", "400 close bad request"])));
}

#[test]
fn interruptions_are_retried_and_other_errors_returned() {
    use std::io::ErrorKind;
    let got = run(vec![Step::Fail(ErrorKind::Interrupted), send(get("/a")), Step::Fail(ErrorKind::Interrupted), send(get("/b"))], 10);
    check!("interrupted twice", got.0, Some(2));
    let mut script = Script::new(vec![send(get("/a")), Step::Fail(ErrorKind::ConnectionReset)]);
    let got = serve(&mut script, 10, |_| String::new()).map_err(|e| e.kind());
    check!("reset", got, Err(ErrorKind::ConnectionReset));
}

#[test]
fn a_failing_write_is_returned() {
    struct NoWrite(Script);
    impl Read for NoWrite {
        fn read(&mut self, b: &mut [u8]) -> io::Result<usize> {
            self.0.read(b)
        }
    }
    impl Write for NoWrite {
        fn write(&mut self, _: &[u8]) -> io::Result<usize> {
            Err(io::ErrorKind::BrokenPipe.into())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    let got = serve(&mut NoWrite(Script::new(vec![send(get("/a"))])), 10, |_| "x".into()).map_err(|e| e.kind());
    check!("broken pipe", got, Err(io::ErrorKind::BrokenPipe));
}

#[test]
fn nothing_is_read_after_the_last_reply() {
    let mut script = Script::new(vec![send("GET /a HTTP/1.1\r\nConnection: close\r\n\r\n"), Step::Fail(io::ErrorKind::Other)]);
    check!("returns without reading again", serve(&mut script, 10, |_| String::new()).ok(), Some(1));
}

#[test]
fn a_thousand_pipelined_requests() {
    let all: String = (0..1000).map(|i| get(&format!("/{i}"))).collect();
    let got = run(vec![send(all), Step::AwaitReplies(1000)], 5000);
    check!("1000 replies in order", (got.0, got.1.len(), got.1[999].clone()), (Some(1000), 1000, "200 keep-alive /999:".to_string()));
}

#[test]
fn random_pipelines_in_random_pieces() {
    let mut rng = anneal_prelude::Rng::new(71701);
    for _ in 0..150 {
        let count = 1 + rng.below(6);
        let max = 1 + rng.below(7);
        let mut bytes = Vec::new();
        let mut want = Vec::new();
        let mut done = false;
        for i in 0..count {
            let one_zero = rng.below(4) == 0;
            let token = *rng.pick(&["", "close", "keep-alive"]);
            let len = rng.below(10);
            let body = rng.string(len, "ab\r\n");
            let version = if one_zero { "HTTP/1.0" } else { "HTTP/1.1" };
            let header = if token.is_empty() { String::new() } else { format!("Connection: {token}\r\n") };
            bytes.extend_from_slice(format!("PUT /{i} {version}\r\n{header}Content-Length: {len}\r\n\r\n{body}").as_bytes());
            if !done {
                let keep = token != "close" && (!one_zero || token == "keep-alive") && i + 1 < max;
                want.push(format!("200 {} /{i}:{body}", if keep { "keep-alive" } else { "close" }));
                done = !keep;
            }
        }
        let mut steps = Vec::new();
        let mut at = 0;
        while at < bytes.len() {
            let n = (1 + rng.below(40)).min(bytes.len() - at);
            steps.push(Step::Send(bytes[at..at + n].to_vec()));
            at += n;
        }
        let served = want.len();
        check!(format!("max {max}: {:?}", String::from_utf8_lossy(&bytes)), run(steps, max), (Some(served), want));
    }
}
