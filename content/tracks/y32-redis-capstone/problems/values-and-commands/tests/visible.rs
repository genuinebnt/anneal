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

#[test]
fn strings() {
    script(&mut Engine::new(), &[
        ("GET k", Frame::Null),
        ("SET k hello", ok()),
        ("GET k", bulk("hello")),
        ("APPEND k !", int(6)),
        ("STRLEN k", int(6)),
        ("STRLEN nothing", int(0)),
        ("APPEND new ab", int(2)),
        ("MSET a 1 b 2", ok()),
        ("MGET a nothing b", Frame::Array(vec![bulk("1"), Frame::Null, bulk("2")])),
        ("INCR a", int(2)),
        ("DECR fresh", int(-1)),
        ("INCRBY a 40", int(42)),
        ("GET a", bulk("42")),
    ]);
}

#[test]
fn names_ignore_case_and_ping_answers() {
    script(&mut Engine::new(), &[
        ("ping", simple("PONG")),
        ("PiNg hi", bulk("hi")),
        ("echo x", bulk("x")),
        ("set k v", ok()),
        ("gEt k", bulk("v")),
    ]);
}

#[test]
fn a_list() {
    script(&mut Engine::new(), &[
        ("RPUSH l b c", int(2)),
        ("LPUSH l a z", int(4)),
        ("LRANGE l 0 -1", arr(&["z", "a", "b", "c"])),
        ("LRANGE l -2 100", arr(&["b", "c"])),
        ("LPOP l", bulk("z")),
        ("RPOP l 2", arr(&["c", "b"])),
        ("LLEN l", int(1)),
        ("LPOP l", bulk("a")),
        ("EXISTS l", int(0)),
        ("LPOP l", Frame::Null),
        ("LPOP l 2", Frame::NullArray),
    ]);
}

#[test]
fn a_hash_and_a_set() {
    script(&mut Engine::new(), &[
        ("HSET h b 2 a 1", int(2)),
        ("HSET h a 9 c 3", int(1)),
        ("HGET h a", bulk("9")),
        ("HGETALL h", arr(&["a", "9", "b", "2", "c", "3"])),
        ("HDEL h a zzz", int(1)),
        ("HLEN h", int(2)),
        ("SADD s c a c b", int(3)),
        ("SMEMBERS s", arr(&["a", "b", "c"])),
        ("SADD t b d", int(2)),
        ("SINTER s t", arr(&["b"])),
        ("SUNION s t", arr(&["a", "b", "c", "d"])),
        ("SDIFF s t", arr(&["a", "c"])),
        ("SISMEMBER s a", int(1)),
        ("SREM s a q", int(1)),
        ("SCARD s", int(2)),
    ]);
}

#[test]
fn the_wrong_kind_of_value() {
    script(&mut Engine::new(), &[
        ("RPUSH l x", int(1)),
        ("GET l", err(WRONGTYPE)),
        ("INCR l", err(WRONGTYPE)),
        ("HSET l f v", err(WRONGTYPE)),
        ("LRANGE l 0 -1", arr(&["x"])),
        ("TYPE l", simple("list")),
    ]);
}

#[test]
fn a_command_that_cannot_run() {
    let mut e = Engine::new();
    let reply = run(&mut e, "NOPE x");
    check!(format!("NOPE x gave {reply:?}"), is_error(&reply, "ERR unknown command"), true);
    script(&mut e, &[
        ("GET", err("ERR wrong number of arguments for 'get' command")),
        ("gEt a b", err("ERR wrong number of arguments for 'get' command")),
    ]);
}
