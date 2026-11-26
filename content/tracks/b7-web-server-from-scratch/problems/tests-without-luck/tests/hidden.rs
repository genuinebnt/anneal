use solution::*;

#[allow(dead_code, unused_imports)]
use std::io::{ErrorKind, Read, Write};
#[allow(dead_code, unused_imports)]
use std::sync::mpsc::{channel, Receiver};
#[allow(dead_code, unused_imports)]
use std::time::{Duration, Instant};

#[allow(dead_code, unused_imports)]
const PATIENCE: Duration = Duration::from_secs(3);

/// A thread that calls `pass` once, and reports when it gets through.
#[allow(dead_code, unused_imports)]
fn walker(gate: &Gate) -> Receiver<()> {
    let gate = gate.clone();
    let (done, through) = channel();
    std::thread::spawn(move || {
        gate.pass();
        let _ = done.send(());
    });
    through
}

/// One channel for many walkers.
#[allow(dead_code, unused_imports)]
fn walkers(gate: &Gate, count: usize) -> Receiver<()> {
    let (done, through) = channel();
    for _ in 0..count {
        let (gate, done) = (gate.clone(), done.clone());
        std::thread::spawn(move || {
            gate.pass();
            let _ = done.send(());
        });
    }
    through
}

#[allow(dead_code, unused_imports)]
fn count_through(rx: &Receiver<()>, expected: usize) -> usize {
    let mut got = 0;
    while got < expected && rx.recv_timeout(PATIENCE).is_ok() {
        got += 1;
    }
    // and no more than that
    while rx.recv_timeout(Duration::from_millis(100)).is_ok() {
        got += 1;
    }
    got
}

/// What one read into a buffer of `size` bytes gives.
#[allow(dead_code, unused_imports)]
fn read(script: &mut Script, size: usize) -> Result<Vec<u8>, ErrorKind> {
    let mut buf = vec![0u8; size];
    script.read(&mut buf).map(|n| buf[..n].to_vec()).map_err(|e| e.kind())
}

#[allow(dead_code, unused_imports)]
fn data(bytes: &str) -> Event {
    Event::Data(bytes.as_bytes().to_vec())
}

#[test]
fn open_before_anyone_arrives() {
    let gate = Gate::new();
    gate.open();
    let through = walkers(&gate, 3);
    check!("all three pass an open gate", count_through(&through, 3), 3);
}

#[test]
fn permits_given_early_are_kept() {
    let gate = Gate::new();
    gate.release(1);
    gate.release(1);
    let through = walkers(&gate, 4);
    check!("two of four", count_through(&through, 2), 2);
    gate.release(2);
    check!("and then the rest", count_through(&through, 2), 2);
}

#[test]
fn release_zero_lets_nobody_through() {
    let gate = Gate::new();
    let through = walker(&gate);
    gate.wait_for_arrivals(1, PATIENCE);
    gate.release(0);
    check!("nobody", through.recv_timeout(Duration::from_millis(100)).is_err(), true);
    gate.release(1);
    check!("one", through.recv_timeout(PATIENCE).is_ok(), true);
}

#[test]
fn arrivals_are_counted_whether_waiting_or_gone() {
    let gate = Gate::new();
    check!("none yet", gate.arrived(), 0);
    gate.release(2);
    let through = walkers(&gate, 3);
    check!("three arrive", gate.wait_for_arrivals(3, PATIENCE), true);
    count_through(&through, 2);
    check!("still three after two left", gate.arrived(), 3);
    gate.open();
}

#[test]
fn waiting_returns_as_soon_as_the_arrival_happens() {
    let gate = Gate::new();
    let g = gate.clone();
    let t = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(50));
        g.pass();
    });
    let began = Instant::now();
    let got = gate.wait_for_arrivals(1, Duration::from_secs(8));
    check!("arrived", got, true);
    check!(format!("returned after {:?}, long before the 8 s timeout", began.elapsed()), began.elapsed() < Duration::from_secs(4), true);
    gate.open();
    t.join().unwrap();
}

#[test]
fn too_few_arrivals_is_false_after_the_full_timeout() {
    let gate = Gate::new();
    gate.open();
    gate.pass();
    let began = Instant::now();
    check!("one of two", gate.wait_for_arrivals(2, Duration::from_millis(100)), false);
    check!("waited the whole time", began.elapsed() >= Duration::from_millis(100), true);
    check!("one of one", gate.wait_for_arrivals(1, Duration::ZERO), true);
}

#[test]
fn clones_share_the_gate() {
    let gate = Gate::new();
    let other = gate.clone();
    let through = walker(&gate);
    check!("seen through the clone", other.wait_for_arrivals(1, PATIENCE), true);
    other.open();
    check!("opened through the clone", through.recv_timeout(PATIENCE).is_ok(), true);
}

#[test]
fn stepping_one_at_a_time() {
    let gate = Gate::new();
    let g = gate.clone();
    let (tx, rx) = channel();
    let t = std::thread::spawn(move || {
        for i in 0..200 {
            g.pass();
            tx.send(i).unwrap();
        }
    });
    for i in 0..200 {
        assert!(gate.wait_for_arrivals(i + 1, PATIENCE), "arrival {i}");
        assert!(rx.try_recv().is_err(), "step {i} ran before it was released");
        gate.release(1);
        check!("the released step", rx.recv_timeout(PATIENCE).ok(), Some(i));
    }
    t.join().unwrap();
}

#[test]
fn an_empty_data_event_is_not_the_end() {
    let mut s = Script::new(vec![data(""), data("x"), data("")]);
    check!("x then end", (read(&mut s, 8), read(&mut s, 8)), (Ok(b"x".to_vec()), Ok(vec![])));
}

#[test]
fn events_are_not_merged() {
    let mut s = Script::new(vec![data("ab"), data("cd")]);
    check!("one event per read", (read(&mut s, 100), read(&mut s, 100)), (Ok(b"ab".to_vec()), Ok(b"cd".to_vec())));
}

#[test]
fn the_end_stays() {
    let mut s = Script::new(vec![data("a"), Event::End, data("never")]);
    let got = (read(&mut s, 8), read(&mut s, 8), read(&mut s, 8));
    check!("a, end, end", got, (Ok(b"a".to_vec()), Ok(vec![]), Ok(vec![])));
}

#[test]
fn an_empty_buffer_takes_nothing() {
    let mut s = Script::new(vec![Event::Quiet, data("a")]);
    check!("empty buffer", read(&mut s, 0), Ok(vec![]));
    check!("the quiet is still there", read(&mut s, 4), Err(ErrorKind::WouldBlock));
}

#[test]
fn unread_counts_the_bytes_left() {
    let mut s = Script::new(vec![data("abcde"), Event::Quiet, data("fg")]);
    check!("all", s.unread(), 7);
    let _ = read(&mut s, 2);
    check!("after two", s.unread(), 5);
}

#[test]
fn limited_writes() {
    let mut s = Script::new(vec![]);
    s.limit_writes(3);
    check!("one call", s.write(b"abcdefgh").map_err(|e| e.kind()), Ok(3));
    s.write_all(b"12345678").unwrap();
    s.flush().unwrap();
    check!("all of it", s.written().to_vec(), b"abc12345678".to_vec());
}

#[test]
fn random_scripts_give_back_their_data() {
    let mut rng = anneal_prelude::Rng::new(72001);
    for _ in 0..300 {
        let mut events = Vec::new();
        let mut want: Vec<Result<Vec<u8>, ErrorKind>> = Vec::new();
        let size = 1 + rng.below(8);
        for _ in 0..rng.below(8) {
            match rng.below(4) {
                0 => {
                    events.push(Event::Quiet);
                    want.push(Err(ErrorKind::WouldBlock));
                }
                1 => {
                    events.push(Event::Interrupt);
                    want.push(Err(ErrorKind::Interrupted));
                }
                _ => {
                    let len = rng.below(20);
                    let bytes: Vec<u8> = rng.vec(len, 0, 255);
                    for piece in bytes.chunks(size) {
                        want.push(Ok(piece.to_vec()));
                    }
                    events.push(Event::Data(bytes));
                }
            }
        }
        want.push(Ok(Vec::new()));
        let shown = format!("{events:?} read {size} at a time");
        let mut script = Script::new(events);
        let got: Vec<_> = (0..want.len()).map(|_| read(&mut script, size)).collect();
        check!(shown, got, want);
    }
}
