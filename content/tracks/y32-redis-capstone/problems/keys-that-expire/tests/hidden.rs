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

/// An engine where `k` held something that expired at time 100.
#[allow(dead_code)]
fn stale(setup: &str) -> Engine {
    let mut e = Engine::new();
    run(&mut e, setup);
    check!(format!("{setup}, then PEXPIREAT"), run(&mut e, "PEXPIREAT k 100"), int(1));
    e
}

#[test]
fn every_command_treats_an_expired_key_as_missing() {
    let cases: Vec<(&str, &str, Frame)> = vec![
        ("SET k v", "GET k", Frame::Null),
        ("SET k v", "STRLEN k", int(0)),
        ("SET k v", "APPEND k ab", int(2)),
        ("SET k 41", "INCR k", int(1)),
        ("SET k 41", "DECR k", int(-1)),
        ("SET k 41", "INCRBY k 5", int(5)),
        ("SET k v", "MGET k", Frame::Array(vec![Frame::Null])),
        ("SET k v", "SET k w NX", ok()),
        ("SET k v", "SET k w XX", Frame::Null),
        ("SET k v", "TYPE k", simple("none")),
        ("SET k v", "EXISTS k k", int(0)),
        ("SET k v", "DEL k", int(0)),
        ("SET k v", "DBSIZE", int(0)),
        ("SET k v", "TTL k", int(-2)),
        ("SET k v", "PTTL k", int(-2)),
        ("SET k v", "EXPIRE k 10", int(0)),
        ("SET k v", "PEXPIRE k 10", int(0)),
        ("SET k v", "PEXPIREAT k 900", int(0)),
        ("SET k v", "PERSIST k", int(0)),
        ("SET k v", "RENAME k other", err("ERR no such key")),
        ("RPUSH k a", "LLEN k", int(0)),
        ("RPUSH k a", "LPOP k", Frame::Null),
        ("RPUSH k a", "RPOP k 1", Frame::NullArray),
        ("RPUSH k a", "LRANGE k 0 -1", arr(&[])),
        ("RPUSH k a", "RPUSH k b", int(1)),
        ("RPUSH k a", "LPUSH k b", int(1)),
        ("RPUSH k a", "SADD k b", int(1)),
        ("HSET k f 1", "HGET k f", Frame::Null),
        ("HSET k f 1", "HLEN k", int(0)),
        ("HSET k f 1", "HGETALL k", arr(&[])),
        ("HSET k f 1", "HDEL k f", int(0)),
        ("HSET k f 1", "HSET k f 2", int(1)),
        ("HSET k f 7", "HINCRBY k f 1", int(1)),
        ("SADD k a", "SCARD k", int(0)),
        ("SADD k a", "SMEMBERS k", arr(&[])),
        ("SADD k a", "SISMEMBER k a", int(0)),
        ("SADD k a", "SREM k a", int(0)),
        ("SADD k a", "SADD k a", int(1)),
        ("SADD k a", "SUNION k", arr(&[])),
        ("SADD k a", "SINTER k k", arr(&[])),
    ];
    for (setup, line, want) in cases {
        let mut e = stale(setup);
        check!(format!("{setup}; expired; {line}"), at(&mut e, 100, line), want);
    }
}

#[test]
fn a_key_is_alive_until_its_deadline_exactly() {
    let mut e = stale("SET k v");
    check!("one millisecond early", at(&mut e, 99, "GET k"), bulk("v"));
    check!("on time", at(&mut e, 100, "GET k"), Frame::Null);
}

#[test]
fn a_key_made_again_has_no_time_limit() {
    for (setup, again) in [("SET k v", "INCR k"), ("RPUSH k a", "RPUSH k b"), ("SET k v", "MSET k w"), ("SADD k a", "SET k w KEEPTTL")] {
        let mut e = stale(setup);
        at(&mut e, 200, again);
        check!(format!("{setup}; expired; {again}; PTTL"), at(&mut e, 200, "PTTL k"), int(-1));
    }
}

#[test]
fn which_commands_drop_the_time_limit() {
    script(&mut Engine::new(), &[
        ("SET k v EX 10", ok()), ("MSET k w", ok()), ("TTL k", int(-1)),
        ("EXPIRE k 10", int(1)), ("SET k x XX", ok()), ("TTL k", int(-1)),
        ("EXPIRE k 10", int(1)), ("DEL k", int(1)), ("RPUSH k a", int(1)), ("TTL k", int(-1)),
        ("EXPIRE k 10", int(1)), ("LPOP k", bulk("a")), ("RPUSH k b", int(1)), ("TTL k", int(-1)),
        ("EXPIRE k 10", int(1)), ("FLUSHALL", ok()), ("SADD k m", int(1)), ("TTL k", int(-1)),
        ("HSET h f 1", int(1)), ("EXPIRE h 10", int(1)), ("HINCRBY h f 1", int(2)), ("HSET h g 1", int(1)), ("TTL h", int(10)),
        // renaming a key with no limit over one with a limit
        ("SET plain v", ok()), ("SET timed v EX 10", ok()), ("RENAME plain timed", ok()), ("TTL timed", int(-1)),
        ("EXPIRE timed 10", int(1)), ("RENAME timed timed", ok()), ("TTL timed", int(10)),
    ]);
}

#[test]
fn renaming_over_an_expired_key() {
    let mut e = Engine::new();
    run(&mut e, "SET old x PX 5");
    run(&mut e, "SET new y");
    check!("RENAME", at(&mut e, 10, "RENAME new old"), ok());
    check!("PTTL", at(&mut e, 10, "PTTL old"), int(-1));
}

#[test]
fn ttl_rounds_to_the_nearest_second() {
    for (ms, want) in [(1u64, 0i64), (499, 0), (500, 1), (1000, 1), (1499, 1), (1500, 2)] {
        let mut e = Engine::new();
        run(&mut e, &format!("SET k v PX {ms}"));
        check!(format!("TTL with {ms} ms left"), run(&mut e, "TTL k"), int(want));
    }
}

#[test]
fn times_in_the_past() {
    let mut e = Engine::new();
    check!("SET PXAT in the past is accepted", at(&mut e, 5000, "SET k v PXAT 4000"), ok());
    check!("and the key is already gone", at(&mut e, 5000, "GET k"), Frame::Null);
    for line in ["EXPIRE k 0", "PEXPIRE k 0", "PEXPIRE k -9223372036854775808", "PEXPIREAT k 5000", "PEXPIREAT k -5", "EXPIRE k -100"] {
        check!("SET", at(&mut e, 5000, "SET k v"), ok());
        check!(line, at(&mut e, 5000, line), int(1));
        check!(format!("{line}, then time goes back"), at(&mut e, 0, "EXISTS k"), int(0));
    }
}

#[test]
fn expire_times_that_cannot_be() {
    let mut e = Engine::new();
    at(&mut e, 1000, "SET k v");
    for (line, name) in [
        ("SET k w EX -1", "set"), ("SET k w PX 0", "set"), ("SET k w PXAT 0", "set"),
        ("SET k w EX 9223372036854775807", "set"),
        ("EXPIRE k 9223372036854775807", "expire"), ("PEXPIRE k 9223372036854775807", "pexpire"),
        ("EXPIRE k -9223372036854775808", "expire"),
    ] {
        check!(line, at(&mut e, 1000, line), err(&format!("ERR invalid expire time in '{name}' command")));
    }
    check!("nothing changed", at(&mut e, 1000, "GET k"), bulk("v"));
    check!("nothing changed", at(&mut e, 1000, "PTTL k"), int(-1));
    check!("not a number", at(&mut e, 1000, "EXPIRE k soon"), err(NOT_INT));
    check!("not a number", at(&mut e, 1000, "SET k w PX 1.5"), err(NOT_INT));
}

#[test]
fn bad_set_options() {
    for line in [
        "SET k v EX 1 PX 5", "SET k v PX 5 PXAT 9", "SET k v EX 1 EX 1", "SET k v KEEPTTL EX 1", "SET k v PX 1 KEEPTTL",
        "SET k v XX NX", "SET k v BOGUS", "SET k v PX", "SET k v NX extra",
    ] {
        check!(line, run(&mut Engine::new(), line), err("ERR syntax error"));
    }
    script(&mut Engine::new(), &[("SET k v nx px 10", ok()), ("set k w Xx kEePtTl", ok()), ("PTTL k", int(10))]);
}

#[test]
fn a_failed_condition_changes_no_time_limit() {
    script(&mut Engine::new(), &[("SET k v PX 100", ok()), ("SET k w NX PX 5", Frame::Null), ("PTTL k", int(100)), ("GET k", bulk("v"))]);
}

#[test]
fn cycles_remove_only_expired_keys_and_count_them() {
    let mut e = Engine::new();
    for i in 0..100 {
        run(&mut e, &format!("SET old{i} v PX 10"));
    }
    for i in 0..200 {
        run(&mut e, &format!("SET young{i} v PX 1000"));
        run(&mut e, &format!("SET forever{i} v"));
    }
    let mut dice = Seeded(Rng::new(0x3203));
    let total: usize = (0..2000).map(|_| e.cycle(10, &mut dice)).sum();
    check!("2000 cycles remove all 100 expired keys, once each", total, 100);
    check!("the rest stay", at(&mut e, 10, "DBSIZE"), int(400));
}

#[test]
fn one_cycle_does_bounded_work() {
    let mut e = Engine::new();
    for i in 0..5000 {
        run(&mut e, &format!("SET k{i} v PX 1"));
    }
    let mut dice = Seeded(Rng::new(0x3204));
    let first = e.cycle(1, &mut dice);
    check!(format!("one cycle removed {first}; it stops after 16 rounds of at most 20"), first > 0 && first <= 320, true);
    let rest: usize = (0..200).map(|_| e.cycle(1, &mut dice)).sum();
    check!("later cycles finish the job", first + rest, 5000);
}

#[test]
fn a_lazy_removal_is_not_counted_again() {
    let mut e = stale("SET k v");
    check!("GET removes it", at(&mut e, 100, "GET k"), Frame::Null);
    check!("a cycle finds nothing", e.cycle(100, &mut Seeded(Rng::new(2))), 0);
}

#[test]
fn many_keys_with_time_limits() {
    let mut e = Engine::new();
    for i in 0..60_000u64 {
        run(&mut e, &format!("SET k{i} v PX {}", 1 + i % 100));
    }
    let mut dice = Seeded(Rng::new(0x3205));
    let mut removed = 0;
    for now in 1..=100 {
        removed += e.cycle(now, &mut dice);
    }
    for i in 0..60_000u64 {
        check!("every key is gone", at(&mut e, 200, &format!("EXISTS k{i}")), int(0));
    }
    check!("cycles took their share", removed > 0, true);
}
