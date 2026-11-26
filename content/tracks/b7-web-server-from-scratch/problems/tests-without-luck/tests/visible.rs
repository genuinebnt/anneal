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
fn the_test_can_wait_for_an_arrival() {
    let gate = Gate::new();
    let through = walker(&gate);
    check!("one arrival", gate.wait_for_arrivals(1, PATIENCE), true);
    check!("not through yet", through.recv_timeout(Duration::from_millis(100)).is_err(), true);
    gate.open();
    check!("through after open", through.recv_timeout(PATIENCE).is_ok(), true);
}

#[test]
fn waiting_for_arrivals_gives_up() {
    check!("nobody comes", Gate::new().wait_for_arrivals(1, Duration::from_millis(50)), false);
}

#[test]
fn release_lets_exactly_that_many_through() {
    let gate = Gate::new();
    let through = walkers(&gate, 5);
    gate.wait_for_arrivals(5, PATIENCE);
    gate.release(2);
    check!("two of five", count_through(&through, 2), 2);
    gate.open();
    check!("the other three", count_through(&through, 3), 3);
}

#[test]
fn a_script_delivers_in_pieces() {
    let mut s = Script::new(vec![data("hello"), data("!")]);
    let got = (read(&mut s, 3), read(&mut s, 3), read(&mut s, 3), read(&mut s, 3));
    check!("reads of 3", got, (Ok(b"hel".to_vec()), Ok(b"lo".to_vec()), Ok(b"!".to_vec()), Ok(vec![])));
}

#[test]
fn quiet_and_interrupted() {
    let mut s = Script::new(vec![Event::Quiet, Event::Interrupt, data("a")]);
    let got = (read(&mut s, 9), read(&mut s, 9), read(&mut s, 9));
    check!("three reads", got, (Err(ErrorKind::WouldBlock), Err(ErrorKind::Interrupted), Ok(b"a".to_vec())));
}

#[test]
fn writes_are_collected() {
    let mut s = Script::new(vec![]);
    s.write_all(b"one ").unwrap();
    s.write_all(b"two").unwrap();
    check!("written", s.written().to_vec(), b"one two".to_vec());
}
