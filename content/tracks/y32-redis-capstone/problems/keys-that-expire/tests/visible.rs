use solution::*;

#[allow(unused_imports)]
use anneal_prelude::Rng;


#[allow(dead_code)]
fn args(line: &str) -> Vec<Vec<u8>> {
    line.split(' ').map(|w| w.as_bytes().to_vec()).collect()
}
#[allow(dead_code)]
fn bulk(s: &str) -> Frame {
    Frame::Bulk(s.as_bytes().to_vec())
}
#[allow(dead_code)]
fn int(n: i64) -> Frame {
    Frame::Int(n)
}
#[allow(dead_code)]
fn arr(items: &[&str]) -> Frame {
    Frame::Array(items.iter().map(|s| bulk(s)).collect())
}
#[allow(dead_code)]
fn ok() -> Frame {
    Frame::Simple("OK".to_string())
}
#[allow(dead_code)]
fn simple(s: &str) -> Frame {
    Frame::Simple(s.to_string())
}
#[allow(dead_code)]
fn err(s: &str) -> Frame {
    Frame::Error(s.to_string())
}
#[allow(dead_code)]
const WRONGTYPE: &str = "WRONGTYPE Operation against a key holding the wrong kind of value";
#[allow(dead_code)]
const NOT_INT: &str = "ERR value is not an integer or out of range";
/// Every error reply must start with `prefix`.
#[allow(dead_code)]
fn is_error(frame: &Frame, prefix: &str) -> bool {
    matches!(frame, Frame::Error(text) if text.starts_with(prefix))
}

#[allow(dead_code)]
fn at(engine: &mut Engine, now: u64, line: &str) -> Frame {
    engine.execute(&args(line), now)
}
#[allow(dead_code)]
fn run(engine: &mut Engine, line: &str) -> Frame {
    at(engine, 0, line)
}
/// Runs each line and checks its reply.
#[allow(dead_code)]
fn script(engine: &mut Engine, steps: &[(&str, Frame)]) {
    for (i, (line, want)) in steps.iter().enumerate() {
        check!(format!("command {} of the script: {line}", i + 1), run(engine, line), want.clone());
    }
}

#[allow(dead_code)]
struct Seeded(Rng);
impl Dice for Seeded {
    fn below(&mut self, n: usize) -> usize {
        self.0.below(n)
    }
}

#[test]
fn a_key_with_a_time_limit() {
    let mut e = Engine::new();
    check!("SET at 1000", at(&mut e, 1000, "SET k v PX 500"), ok());
    check!("GET at 1499", at(&mut e, 1499, "GET k"), bulk("v"));
    check!("PTTL at 1499", at(&mut e, 1499, "PTTL k"), int(1));
    check!("GET at 1500", at(&mut e, 1500, "GET k"), Frame::Null);
    check!("PTTL at 1500", at(&mut e, 1500, "PTTL k"), int(-2));
}

#[test]
fn expire_ttl_persist() {
    script(&mut Engine::new(), &[
        ("TTL k", int(-2)),
        ("EXPIRE k 10", int(0)),
        ("SET k v", ok()),
        ("TTL k", int(-1)),
        ("PERSIST k", int(0)),
        ("EXPIRE k 10", int(1)),
        ("TTL k", int(10)),
        ("PTTL k", int(10000)),
        ("PERSIST k", int(1)),
        ("TTL k", int(-1)),
        ("PEXPIRE k 1500", int(1)),
        ("TTL k", int(2)),
        ("PEXPIREAT k 400", int(1)),
        ("PTTL k", int(400)),
        ("EXPIRE k -1", int(1)),
        ("EXISTS k", int(0)),
    ]);
}

#[test]
fn set_options() {
    script(&mut Engine::new(), &[
        ("SET k a XX", Frame::Null),
        ("SET k a nx", ok()),
        ("SET k b NX", Frame::Null),
        ("SET k b XX EX 5", ok()),
        ("TTL k", int(5)),
        ("SET k c KEEPTTL", ok()),
        ("TTL k", int(5)),
        ("SET k d", ok()),
        ("TTL k", int(-1)),
        ("SET k e NX XX", err("ERR syntax error")),
        ("SET k e EX", err("ERR syntax error")),
        ("SET k e EX 0", err("ERR invalid expire time in 'set' command")),
        ("GET k", bulk("d")),
    ]);
}

#[test]
fn changing_a_value_keeps_its_time_limit() {
    script(&mut Engine::new(), &[
        ("SET n 1 PX 100", ok()),
        ("INCR n", int(2)),
        ("APPEND n 0", int(2)),
        ("PTTL n", int(100)),
        ("RPUSH l a", int(1)),
        ("PEXPIRE l 200", int(1)),
        ("RPUSH l b", int(2)),
        ("PTTL l", int(200)),
        ("RENAME l n", ok()),
        ("PTTL n", int(200)),
    ]);
}

#[test]
fn a_cycle_removes_keys_nobody_asks_for() {
    let mut e = Engine::new();
    for i in 0..10 {
        run(&mut e, &format!("SET k{i} v PX 10"));
    }
    check!("nothing has expired at 9", e.cycle(9, &mut Seeded(Rng::new(1))), 0);
    check!("all ten go at 10", e.cycle(10, &mut Seeded(Rng::new(1))), 10);
    check!("none are left", e.cycle(10, &mut Seeded(Rng::new(1))), 0);
}
