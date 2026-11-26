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
fn two_requests_one_after_the_other() {
    let got = run(vec![send(get("/a")), Step::AwaitReplies(1), send(get("/b")), Step::AwaitReplies(2)], 10);
    check!("a, wait, b", got, (Some(2), list(&["200 keep-alive /a:", "200 keep-alive /b:"])));
}

#[test]
fn two_requests_in_one_piece() {
    let got = run(vec![send(get("/a") + &get("/b")), Step::AwaitReplies(2)], 10);
    check!("pipelined", got, (Some(2), list(&["200 keep-alive /a:", "200 keep-alive /b:"])));
}

#[test]
fn close_ends_the_connection() {
    let first = "GET /a HTTP/1.1\r\nConnection: close\r\n\r\n";
    let got = run(vec![send(first.to_string() + &get("/b"))], 10);
    check!("close, then another", got, (Some(1), list(&["200 close /a:"])));
}

#[test]
fn the_limit_ends_it_too() {
    let got = run(vec![send(get("/1") + &get("/2") + &get("/3"))], 2);
    check!("limit 2", got, (Some(2), list(&["200 keep-alive /1:", "200 close /2:"])));
}

#[test]
fn permission_before_the_body() {
    let head = "POST /p HTTP/1.1\r\nExpect: 100-continue\r\nContent-Length: 2\r\n\r\n";
    let got = run(vec![send(head), Step::AwaitReplies(1), send("ok"), Step::AwaitReplies(2)], 10);
    check!("100 then 200", got, (Some(1), list(&["100", "200 keep-alive /p:ok"])));
}

#[test]
fn invalid_gets_400() {
    check!("junk", run(vec![send("junk\r\n\r\n")], 10), (Some(0), list(&["400 close bad request"])));
}
