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
fn idle_is_measured_from_the_last_reply() {
    let r = run(vec![at(400, "a\n"), at(800, "b\n"), at(1200, "c\n")], 500, 1000);
    check!("three lines, 400 ms apart", r.output.as_str(), "ok a\nok b\nok c\n");
    check!("then idle", r.ending, Ending::Idle);
    on_time(&r, 1700);
}

#[test]
fn the_line_limit_starts_with_the_line_not_the_connection() {
    let r = run(vec![at(900, "sl"), at(1500, "ow\n"), close(1550)], 1000, 800);
    check!("a line from 900 to 1500", (r.output.as_str(), r.ending), ("ok slow\n", Ending::ClientLeft));
}

#[test]
fn a_line_may_take_longer_than_the_idle_limit() {
    let r = run(vec![at(100, "a"), at(1500, "b\n"), close(1600)], 300, 5000);
    check!("slow line, short idle limit", (r.output.as_str(), r.ending), ("ok ab\n", Ending::ClientLeft));
}

#[test]
fn each_line_gets_its_own_time() {
    let events = vec![at(0, "a"), at(600, "\n"), at(700, "b"), at(1300, "\n"), at(1400, "c"), at(2000, "\n"), close(2050)];
    let r = run(events, 500, 800);
    check!("three lines of 600 ms each", (r.output.as_str(), r.ending), ("ok a\nok b\nok c\n", Ending::ClientLeft));
}

#[test]
fn a_new_line_in_the_same_read_starts_then() {
    let events = vec![at(0, "first"), at(900, "\nsec"), at(1500, "ond\n"), close(1600)];
    let r = run(events, 5000, 1000);
    check!("second line from 900 to 1500", (r.output.as_str(), r.ending), ("ok first\nok second\n", Ending::ClientLeft));
}

#[test]
fn and_its_time_runs_out() {
    let mut events = vec![at(0, "first"), at(900, "\nx")];
    events.extend(drip(1000, 100, 100));
    let r = run(events, 5000, 1000);
    check!("second line begins at 900 and drips", (r.output.as_str(), r.ending), ("ok first\nerr too slow\n", Ending::TooSlow));
    on_time(&r, 1900);
}

#[test]
fn dripping_finished_in_time_is_fine() {
    let mut events = drip(0, 90, 10);
    events.push(at(900, "\n"));
    events.push(close(950));
    let r = run(events, 200, 1000);
    check!("10 bytes in 900 ms", (r.output.as_str(), r.ending), ("ok aaaaaaaaaa\n", Ending::ClientLeft));
}

#[test]
fn lines_completed_by_the_read_that_notices_are_answered() {
    // nothing returns between 0 and 80, then the line ends in the first read after the limit of 50
    let r = run(vec![at(0, "x"), at(80, "\n"), close(90)], 1000, 50);
    check!("finished by the noticing read", (r.output.as_str(), r.ending), ("ok x\n", Ending::ClientLeft));
}

#[test]
fn the_clock_need_not_start_at_zero() {
    let r = run_from(70_000, vec![at(70_100, "a\n")], 400, 400);
    check!("starting at 70 s", r.output.as_str(), "ok a\n");
    on_time(&r, 70_500);
}

#[test]
fn leaving_in_the_middle_of_a_line() {
    let r = run(vec![at(10, "abc"), close(20)], 1000, 1000);
    check!("half a line then close", (r.ending, r.output.as_str()), (Ending::ClientLeft, ""));
}

#[test]
fn other_errors_are_returned() {
    struct Reset;
    impl Read for Reset {
        fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
            Err(ErrorKind::ConnectionReset.into())
        }
    }
    impl Write for Reset {
        fn write(&mut self, b: &[u8]) -> io::Result<usize> {
            Ok(b.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    let limits = Limits { idle: Duration::from_secs(1), line: Duration::from_secs(1) };
    check!("reset", serve(&mut Reset, || Duration::ZERO, limits).map_err(|e| e.kind()), Err(ErrorKind::ConnectionReset));
}

#[test]
fn a_long_line_in_many_reads() {
    let long = "z".repeat(10_000);
    let r = run(vec![at(10, &long), at(20, "\n"), close(30)], 1000, 1000);
    check!("10000 bytes", r.output == format!("ok {long}\n"), true);
}

#[test]
fn random_timelines_against_a_model() {
    let mut rng = anneal_prelude::Rng::new(71801);
    for _ in 0..400 {
        let (idle, line) = (1000, 1500);
        // the timeline is made while the expected run is worked out, so that no event falls close to a limit,
        // where either answer would be acceptable
        let mut events = Vec::new();
        let mut time = 0;
        let mut idle_since = 0;
        let mut began: Option<u64> = None;
        let mut pending = String::new();
        let mut output = String::new();
        let mut steps = rng.below(10);
        let (ending, due) = loop {
            let gap = if rng.below(4) == 0 { 8 + rng.below(12) } else { 1 + rng.below(6) } as u64;
            let mut when = time + gap * 100 + 50;
            let deadline = match began {
                Some(b) => b + line,
                None => idle_since + idle,
            };
            if when + 100 > deadline && when < deadline + 150 {
                when = deadline + 150;
            }
            time = when;
            let text = if steps == 0 { "" } else { *rng.pick(&["a", "bb", "\n", "c\n", "d\ne"]) };
            steps = steps.saturating_sub(1);
            events.push(at(when, text));
            if when > deadline {
                break (if began.is_some() { Ending::TooSlow } else { Ending::Idle }, deadline);
            }
            if text.is_empty() {
                break (Ending::ClientLeft, when);
            }
            began = began.or(Some(when));
            pending.push_str(text);
            while let Some(i) = pending.find('\n') {
                output.push_str(&format!("ok {}", &pending[..=i]));
                pending.drain(..=i);
                idle_since = when;
                began = (!pending.is_empty()).then_some(when);
            }
        };
        if ending == Ending::TooSlow {
            output.push_str("err too slow\n");
        }
        let shown: Vec<(u64, String)> = events.iter().map(|(t, b)| (*t, String::from_utf8_lossy(b).into_owned())).collect();
        let got = run(events.clone(), idle, line);
        check!(format!("idle {idle}, line {line}, events {shown:?}"), (got.ending, got.output.clone()), (ending, output));
        check!(format!("ended at {} for events {shown:?}", got.ended_at), got.ended_at >= due && got.ended_at <= due + SLICE, true);
    }
}
