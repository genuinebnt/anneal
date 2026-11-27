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

/// Runs SCAN to the end and returns every key it gave, and the number of calls.
#[allow(dead_code)]
fn scan_all(engine: &mut Engine, options: &str, most_per_call: usize) -> (Vec<String>, usize) {
    let (mut cursor, mut keys, mut calls) = ("0".to_string(), Vec::new(), 0);
    loop {
        let line = format!("SCAN {cursor}{options}");
        let (next, found) = scan_once(engine, &line);
        calls += 1;
        assert!(found.len() <= most_per_call, "{line} returned {} keys, more than {most_per_call}", found.len());
        keys.extend(found);
        assert!(calls < 100_000, "SCAN never finished");
        if next == "0" {
            return (keys, calls);
        }
        cursor = next;
    }
}
#[allow(dead_code)]
fn scan_once(engine: &mut Engine, line: &str) -> (String, Vec<String>) {
    let reply = run(engine, line);
    let Frame::Array(parts) = &reply else { panic!("{line} must reply with [cursor, keys], got {reply:?}") };
    let [Frame::Bulk(next), Frame::Array(found)] = &parts[..] else { panic!("{line} must reply with [cursor, keys], got {reply:?}") };
    let found = found.iter().map(|f| match f {
        Frame::Bulk(b) => String::from_utf8_lossy(b).into_owned(),
        other => panic!("a key must be a bulk string, got {other:?}"),
    });
    (String::from_utf8_lossy(next).into_owned(), found.collect())
}

#[test]
fn a_leaderboard() {
    script(&mut Engine::new(), &[
        ("ZADD board 50 ann 20 bob 50 cat", int(3)),
        ("ZRANGE board 0 -1", arr(&["bob", "ann", "cat"])),
        ("ZRANGE board 0 -1 WITHSCORES", arr(&["bob", "20", "ann", "50", "cat", "50"])),
        ("ZADD board 75 bob 10 dan", int(1)),
        ("ZRANGE board -2 -1", arr(&["cat", "bob"])),
        ("ZSCORE board bob", bulk("75")),
        ("ZRANK board dan", int(0)),
        ("ZRANK board bob", int(3)),
        ("ZRANK board eve", Frame::Null),
        ("ZCARD board", int(4)),
        ("ZINCRBY board 2.5 dan", bulk("12.5")),
        ("ZREM board ann eve", int(1)),
        ("ZCARD board", int(3)),
        ("TYPE board", simple("zset")),
    ]);
}

#[test]
fn ranges_by_score() {
    script(&mut Engine::new(), &[
        ("ZADD z 1 a 2 b 2 c 3 d", int(4)),
        ("ZRANGEBYSCORE z 2 3", arr(&["b", "c", "d"])),
        ("ZRANGEBYSCORE z (2 3", arr(&["d"])),
        ("ZRANGEBYSCORE z -inf (2", arr(&["a"])),
        ("ZRANGEBYSCORE z 2 2 WITHSCORES", arr(&["b", "2", "c", "2"])),
        ("ZCOUNT z -inf +inf", int(4)),
        ("ZCOUNT z (1 (3", int(2)),
        ("ZCOUNT z 3 1", int(0)),
    ]);
}

#[test]
fn a_bad_score() {
    script(&mut Engine::new(), &[
        ("ZADD z nan a", err("ERR value is not a valid float")),
        ("ZADD z 1 a two b", err("ERR value is not a valid float")),
        ("EXISTS z", int(0)),
        ("ZADD z 1.5 a", int(1)),
        ("ZSCORE z a", bulk("1.5")),
    ]);
}

#[test]
fn keys_by_pattern() {
    let mut e = Engine::new();
    for key in ["user:2", "user:1", "user:10", "job:1"] {
        run(&mut e, &format!("SET {key} v"));
    }
    check!("KEYS user:*", run(&mut e, "KEYS user:*"), arr(&["user:1", "user:10", "user:2"]));
    check!("KEYS user:?", run(&mut e, "KEYS user:?"), arr(&["user:1", "user:2"]));
    check!("KEYS *", run(&mut e, "KEYS *"), arr(&["job:1", "user:1", "user:10", "user:2"]));
}

#[test]
fn scan_visits_every_key() {
    let mut e = Engine::new();
    for i in 0..25 {
        run(&mut e, &format!("SET k{i:02} v"));
    }
    let (mut keys, _) = scan_all(&mut e, " COUNT 7", 7);
    keys.sort();
    check!("SCAN with COUNT 7", keys, (0..25).map(|i| format!("k{i:02}")).collect::<Vec<_>>());
}
