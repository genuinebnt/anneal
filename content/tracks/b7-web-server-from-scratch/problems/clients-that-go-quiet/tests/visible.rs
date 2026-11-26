use solution::*;

#[allow(dead_code, unused_imports)]
use std::cell::Cell;
#[allow(dead_code, unused_imports)]
use std::collections::VecDeque;
#[allow(dead_code, unused_imports)]
use std::io::{self, ErrorKind, Read, Write};
#[allow(dead_code, unused_imports)]
use std::rc::Rc;
#[allow(dead_code, unused_imports)]
use std::time::Duration;

#[allow(dead_code, unused_imports)]
const SLICE: u64 = 100;

/// A client on a simulated clock. `events` are (time in ms, bytes); empty bytes mean the client closes.
/// A read returns the next bytes if they are due within one slice; otherwise a slice passes and nothing has arrived.
#[allow(dead_code, unused_imports)]
struct Wire {
    clock: Rc<Cell<u64>>,
    events: VecDeque<(u64, Vec<u8>)>,
    out: Vec<u8>,
    flushed: usize,
    /// (time, text) for each flush that sent something new.
    sent: Vec<(u64, String)>,
    quiet_reads: usize,
}

#[allow(dead_code, unused_imports)]
impl Read for Wire {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let now = self.clock.get();
        assert!(now < 100_000, "100 seconds have passed and the connection is still being served");
        if let Some((due, bytes)) = self.events.front_mut() {
            if *due <= now + SLICE {
                self.clock.set(now.max(*due));
                let n = bytes.len().min(buf.len());
                buf[..n].copy_from_slice(&bytes[..n]);
                bytes.drain(..n);
                if bytes.is_empty() {
                    self.events.pop_front();
                }
                return Ok(n);
            }
        }
        self.clock.set(now + SLICE);
        self.quiet_reads += 1;
        Err(match self.quiet_reads % 5 {
            0 => ErrorKind::Interrupted,
            1 | 2 => ErrorKind::WouldBlock,
            _ => ErrorKind::TimedOut,
        }
        .into())
    }
}

#[allow(dead_code, unused_imports)]
impl Write for Wire {
    fn write(&mut self, b: &[u8]) -> io::Result<usize> {
        let n = b.len().min(5);
        self.out.extend_from_slice(&b[..n]);
        Ok(n)
    }
    fn flush(&mut self) -> io::Result<()> {
        if self.out.len() > self.flushed {
            self.sent.push((self.clock.get(), String::from_utf8_lossy(&self.out[self.flushed..]).into_owned()));
            self.flushed = self.out.len();
        }
        Ok(())
    }
}

#[derive(Debug)]
#[allow(dead_code, unused_imports)]
struct Run {
    ending: Ending,
    ended_at: u64,
    output: String,
    sent: Vec<(u64, String)>,
}

#[allow(dead_code, unused_imports)]
fn run_from(start: u64, events: Vec<(u64, Vec<u8>)>, idle: u64, line: u64) -> Run {
    let clock = Rc::new(Cell::new(start));
    let mut wire = Wire { clock: clock.clone(), events: events.into(), out: Vec::new(), flushed: 0, sent: Vec::new(), quiet_reads: 0 };
    let c = clock.clone();
    let limits = Limits { idle: Duration::from_millis(idle), line: Duration::from_millis(line) };
    let ending = serve(&mut wire, move || Duration::from_millis(c.get()), limits).expect("serve failed");
    assert_eq!(wire.flushed, wire.out.len(), "output left unflushed");
    Run { ending, ended_at: clock.get(), output: String::from_utf8_lossy(&wire.out).into_owned(), sent: wire.sent }
}

#[allow(dead_code, unused_imports)]
fn run(events: Vec<(u64, Vec<u8>)>, idle: u64, line: u64) -> Run {
    run_from(0, events, idle, line)
}

#[allow(dead_code, unused_imports)]
fn at(time: u64, text: &str) -> (u64, Vec<u8>) {
    (time, text.as_bytes().to_vec())
}

#[allow(dead_code, unused_imports)]
fn close(time: u64) -> (u64, Vec<u8>) {
    (time, Vec::new())
}

/// `count` single bytes, `gap` ms apart.
#[allow(dead_code, unused_imports)]
fn drip(from: u64, gap: u64, count: u64) -> Vec<(u64, Vec<u8>)> {
    (0..count).map(|i| (from + i * gap, vec![b'a'])).collect()
}

/// Ended no earlier than `due` and by the first read that returned afterwards.
#[allow(dead_code, unused_imports)]
fn on_time(run: &Run, due: u64) {
    check!(format!("ended at {} ms; the limit passed at {due} ms", run.ended_at), run.ended_at >= due && run.ended_at <= due + SLICE, true);
}

#[test]
fn lines_are_answered() {
    let r = run(vec![at(50, "one\ntwo\n"), close(80)], 1000, 1000);
    check!("two lines then goodbye", (r.output.as_str(), r.ending), ("ok one\nok two\n", Ending::ClientLeft));
}

#[test]
fn a_silent_client_is_idle() {
    let r = run(vec![], 500, 2000);
    check!("nothing sent", (r.ending, r.output.as_str()), (Ending::Idle, ""));
    on_time(&r, 500);
}

#[test]
fn a_half_line_followed_by_silence_is_too_slow() {
    let r = run(vec![at(100, "hal")], 5000, 700);
    check!("half a line", (r.ending, r.output.as_str()), (Ending::TooSlow, "err too slow\n"));
    on_time(&r, 800);
}

#[test]
fn a_dripping_client_is_too_slow() {
    let r = run(drip(0, 50, 1000), 300, 1000);
    check!("a byte every 50 ms", r.ending, Ending::TooSlow);
    on_time(&r, 1000);
}

#[test]
fn the_answer_comes_at_once() {
    let r = run(vec![at(200, "now\n")], 1000, 1000);
    check!("when the reply was sent", r.sent.first().cloned(), Some((200, "ok now\n".to_string())));
}
