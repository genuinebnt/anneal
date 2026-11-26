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
fn a_line_arriving_one_byte_at_a_time() {
    let mut c = Client::connect(shout(64));
    for b in b"slowly does it\r\n" {
        c.send(&[*b]);
    }
    check!("byte by byte", c.line(), "SLOWLY DOES IT\n".to_string());
}

#[test]
fn only_one_carriage_return_is_removed() {
    check!("two CRs, an inner one, an empty line", exchange(shout(64), b"a\r\r\nb\rc\n\r\n\n"), "A\r\nB\rC\n\n\n".to_string());
}

#[test]
fn bytes_that_are_not_text() {
    check!("bad then good", exchange(shout(64), b"\xff\xfe\nok\n"), "ERR not text\nOK\n".to_string());
}

#[test]
fn a_character_split_across_writes() {
    let mut c = Client::connect(shout(64));
    for b in "né€\n".bytes() {
        c.send(&[b]);
        std::thread::sleep(Duration::from_millis(2));
    }
    check!("multi-byte characters", c.line(), "NÉ€\n".to_string());
}

#[test]
fn lines_at_the_limit_are_fine() {
    check!("8 bytes with CRLF and LF", exchange(shout(8), b"12345678\r\n87654321\n"), "12345678\n87654321\n".to_string());
}

#[test]
fn a_limit_that_splits_between_cr_and_lf() {
    let mut c = Client::connect(shout(8));
    c.send(b"12345678\r");
    std::thread::sleep(Duration::from_millis(30));
    c.send(b"\n");
    check!("8 bytes, CR, then LF later", c.line(), "12345678\n".to_string());
}

#[test]
fn too_long_is_answered_before_the_line_ends() {
    let mut c = Client::connect(shout(100));
    c.send(&[b'a'; 101]);
    check!("101 bytes and no line end", c.rest(), "ERR too long\n".to_string());
}

#[test]
fn the_error_survives_everything_already_sent() {
    let addr = shout(16);
    for _ in 0..5 {
        let mut c = Client::connect(addr);
        c.try_send(&vec![b'z'; 60_000]);
        check!("60000 bytes, limit 16", c.rest(), "ERR too long\n".to_string());
    }
}

#[test]
fn earlier_lines_are_answered_before_the_error() {
    let addr = shout(8);
    let mut c = Client::connect(addr);
    let mut bytes = b"ok\nfine\n".to_vec();
    bytes.extend_from_slice(&vec![b'z'; 40_000]);
    bytes.extend_from_slice(b"\nnever\n");
    c.try_send(&bytes);
    check!("two good, one long", c.rest(), "OK\nFINE\nERR too long\n".to_string());
}

#[test]
fn an_unfinished_line_is_ignored() {
    check!("no final newline", exchange(shout(64), b"done\nhalf"), "DONE\n".to_string());
}

#[test]
fn clients_that_leave_do_not_stop_the_service() {
    let addr = shout(64);
    drop(Client::connect(addr));
    let mut half = Client::connect(addr);
    half.send(b"no newline");
    drop(half);
    // leaves without reading its replies, so the server's later writes fail
    let mut rude = Client::connect(addr);
    rude.send(&b"x\n".repeat(20_000));
    drop(rude);
    check!("the next client", exchange(addr, b"still here\n"), "STILL HERE\n".to_string());
}

#[test]
fn a_panicking_handler_does_not_stop_the_service() {
    let addr = shout(64);
    let mut c = Client::connect(addr);
    c.send(b"panic\n");
    c.finish();
    let mut out = Vec::new();
    let _ = c.stream.read_to_end(&mut out);
    check!("next client", exchange(addr, b"y\n"), "Y\n".to_string());
}

#[test]
fn a_large_reply_arrives_whole() {
    let addr = start(64, |line| line.repeat(100_000)).expect("start");
    let got = exchange(addr, b"abcdefgh\nabcdefgh\n");
    check!("two replies of 800000 bytes", got == format!("{0}\n{0}\n", "abcdefgh".repeat(100_000)), true);
}

#[test]
fn it_listens_on_loopback() {
    let addr = shout(4);
    check!("address", (addr.ip().is_loopback(), addr.port() != 0), (true, true));
}

#[test]
fn random_lines_in_random_pieces() {
    let mut rng = anneal_prelude::Rng::new(71601);
    let addr = shout(40);
    for _ in 0..60 {
        let mut bytes = Vec::new();
        let mut want = String::new();
        for _ in 0..rng.below(6) {
            let len = rng.below(41);
            let line = rng.string(len, "abc XY.");
            bytes.extend_from_slice(line.as_bytes());
            bytes.extend_from_slice(if rng.bool() { b"\r\n" } else { b"\n" });
            want.push_str(&line.to_uppercase());
            want.push('\n');
        }
        let mut c = Client::connect(addr);
        let mut at = 0;
        while at < bytes.len() {
            let n = (1 + rng.below(30)).min(bytes.len() - at);
            c.send(&bytes[at..at + n]);
            at += n;
        }
        c.finish();
        check!(format!("{:?}", String::from_utf8_lossy(&bytes)), c.rest(), want);
    }
}
