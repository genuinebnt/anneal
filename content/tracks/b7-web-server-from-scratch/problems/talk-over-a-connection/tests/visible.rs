use solution::*;

#[allow(dead_code, unused_imports)]
use std::io::{BufRead, BufReader, Read, Write};
#[allow(dead_code, unused_imports)]
use std::net::{Shutdown, SocketAddr, TcpStream};
#[allow(dead_code, unused_imports)]
use std::time::Duration;

/// Only a broken server gets near it.
#[allow(dead_code, unused_imports)]
const PATIENCE: Duration = Duration::from_secs(3);

#[allow(dead_code, unused_imports)]
fn shout(max_line: usize) -> SocketAddr {
    start(max_line, |line| {
        assert!(line != "panic", "asked to panic");
        line.to_uppercase()
    })
    .expect("start")
}

#[allow(dead_code, unused_imports)]
struct Client {
    stream: BufReader<TcpStream>,
}

#[allow(dead_code, unused_imports)]
impl Client {
    fn connect(addr: SocketAddr) -> Client {
        let s = TcpStream::connect_timeout(&addr, PATIENCE).expect("connect");
        s.set_read_timeout(Some(PATIENCE)).unwrap();
        s.set_write_timeout(Some(PATIENCE)).unwrap();
        s.set_nodelay(true).unwrap();
        Client { stream: BufReader::new(s) }
    }
    fn send(&mut self, bytes: &[u8]) {
        self.stream.get_mut().write_all(bytes).expect("client write");
    }
    fn try_send(&mut self, bytes: &[u8]) {
        let _ = self.stream.get_mut().write_all(bytes);
    }
    fn line(&mut self) -> String {
        let mut line = String::new();
        self.stream.read_line(&mut line).unwrap_or_else(|e| panic!("no reply line: {e}"));
        line
    }
    /// Everything until the server closes; a reset or a timeout fails the test.
    fn rest(&mut self) -> String {
        let mut out = String::new();
        if let Err(e) = self.stream.read_to_string(&mut out) {
            panic!("the connection did not end cleanly: {e}; read so far {out:?}");
        }
        out
    }
    fn finish(&mut self) {
        let _ = self.stream.get_ref().shutdown(Shutdown::Write);
    }
}

/// Sends everything, ends this side and returns all the replies.
#[allow(dead_code, unused_imports)]
fn exchange(addr: SocketAddr, bytes: &[u8]) -> String {
    let mut c = Client::connect(addr);
    c.send(bytes);
    c.finish();
    c.rest()
}

#[test]
fn one_line() {
    check!("hello", exchange(shout(64), b"hello\n"), "HELLO\n".to_string());
}

#[test]
fn many_lines_in_one_write() {
    check!("three lines", exchange(shout(64), b"a\nb\r\nc\n"), "A\nB\nC\n".to_string());
}

#[test]
fn a_reply_comes_before_more_input_is_sent() {
    let mut c = Client::connect(shout(64));
    for word in ["one", "two"] {
        c.send(format!("{word}\n").as_bytes());
        check!(word, c.line(), format!("{}\n", word.to_uppercase()));
    }
}

#[test]
fn clients_are_served_in_turn() {
    let addr = shout(64);
    for i in 0..5 {
        check!(format!("client {i}"), exchange(addr, b"x\n"), "X\n".to_string());
    }
}

#[test]
fn a_line_that_is_too_long() {
    check!("9 bytes, limit 8", exchange(shout(8), b"123456789\n"), "ERR too long\n".to_string());
}
