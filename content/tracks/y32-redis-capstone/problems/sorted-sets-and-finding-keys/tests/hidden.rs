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
fn scores_and_how_they_are_written() {
    script(&mut Engine::new(), &[
        ("ZADD z 3.0 a", int(1)), ("ZSCORE z a", bulk("3")),
        ("ZADD z -0 b", int(1)), ("ZSCORE z b", bulk("0")),
        ("ZADD z 1e3 c", int(1)), ("ZSCORE z c", bulk("1000")),
        ("ZADD z inf d +inf e -inf f", int(3)),
        ("ZSCORE z d", bulk("inf")), ("ZSCORE z f", bulk("-inf")),
        ("ZADD z 0.5 g", int(1)), ("ZSCORE z g", bulk("0.5")),
        ("ZRANGE z 0 -1", arr(&["f", "b", "g", "a", "c", "d", "e"])),
        ("ZINCRBY z -inf d", err("ERR resulting score is not a number (NaN)")),
        ("ZSCORE z d", bulk("inf")),
        ("ZINCRBY z 5 d", bulk("inf")),
        ("ZINCRBY new -0 m", bulk("0")),
        ("ZRANGE new 0 -1 WITHSCORES", arr(&["m", "0"])),
    ]);
}

#[test]
fn minus_zero_is_zero() {
    script(&mut Engine::new(), &[
        ("ZADD z 0 b -0 a", int(2)),
        ("ZRANGE z 0 -1", arr(&["a", "b"])),
        ("ZCOUNT z 0 0", int(2)),
        ("ZCOUNT z (-0 9", int(0)),
    ]);
}

#[test]
fn a_bad_score_changes_nothing() {
    script(&mut Engine::new(), &[
        ("ZADD z 1 a nan b", err("ERR value is not a valid float")),
        ("ZADD z 1 a x b", err("ERR value is not a valid float")),
        ("EXISTS z", int(0)),
        ("ZADD z 1 a 2", err("ERR syntax error")),
        ("ZINCRBY z NaN a", err("ERR value is not a valid float")),
        ("EXISTS z", int(0)),
        ("ZRANGEBYSCORE z x 1", err("ERR min or max is not a float")),
        ("ZCOUNT z 1 (", err("ERR min or max is not a float")),
        ("ZCOUNT z nan 1", err("ERR min or max is not a float")),
        ("ZRANGE z 0 -1 NOPE", err("ERR syntax error")),
        ("ZRANGEBYSCORE z 0 1 NOPE", err("ERR syntax error")),
        ("ZRANGE z a 1", err(NOT_INT)),
    ]);
}

#[test]
fn sorted_sets_follow_the_rules_of_other_collections() {
    script(&mut Engine::new(), &[
        ("SET s v", ok()), ("ZADD s 1 a", err(WRONGTYPE)), ("ZSCORE s a", err(WRONGTYPE)), ("ZCOUNT s 0 1", err(WRONGTYPE)),
        ("ZRANGE s 0 -1", err(WRONGTYPE)), ("ZRANGEBYSCORE s 0 1", err(WRONGTYPE)), ("ZREM s a", err(WRONGTYPE)),
        ("ZADD z 1 a", int(1)), ("ZADD z 1 a 1 a", int(0)), ("ZREM z a", int(1)), ("TYPE z", simple("none")),
        ("ZREM z a", int(0)), ("ZCARD z", int(0)), ("ZRANGE z 0 -1", arr(&[])), ("ZRANGEBYSCORE z 0 1", arr(&[])),
        ("ZRANK z a", Frame::Null), ("ZSCORE z a", Frame::Null), ("ZCOUNT z 0 1", int(0)), ("DBSIZE", int(1)),
        ("ZADD t 1 a 2 a", int(1)), ("ZSCORE t a", bulk("2")), ("PEXPIRE t 50", int(1)), ("ZADD t 3 b", int(1)), ("PTTL t", int(50)),
    ]);
}

#[test]
fn expired_sorted_sets_and_keys_are_gone() {
    let mut e = Engine::new();
    run(&mut e, "ZADD z 1 a");
    run(&mut e, "PEXPIRE z 10");
    run(&mut e, "SET other v");
    run(&mut e, "SET zz v PX 5");
    check!("ZSCORE", at(&mut e, 10, "ZSCORE z a"), Frame::Null);
    check!("KEYS", at(&mut e, 10, "KEYS *"), arr(&["other"]));
    run(&mut e, "SET zzz v PX 5");
    let (mut cursor, mut found) = ("0".to_string(), Vec::new());
    for _ in 0..5 {
        let Frame::Array(parts) = at(&mut e, 10, &format!("SCAN {cursor} COUNT 100")) else { panic!("SCAN reply") };
        let [Frame::Bulk(next), Frame::Array(keys)] = &parts[..] else { panic!("SCAN reply") };
        found.extend(keys.clone());
        cursor = String::from_utf8_lossy(next).into_owned();
        if cursor == "0" {
            break;
        }
    }
    check!("SCAN", (cursor, found), ("0".to_string(), vec![bulk("other")]));
}

#[test]
fn random_sorted_sets_match_a_model() {
    let mut rng = Rng::new(0x3206);
    let names = ["a", "b", "c", "d", "e", "f", "g", "h"];
    for run_number in 0..150 {
        let mut e = Engine::new();
        let mut model: std::collections::BTreeMap<&str, i64> = Default::default();
        let mut history = Vec::new();
        for _ in 0..50 {
            let name = *rng.pick(&names);
            let score = rng.int(-4, 4);
            let line = match rng.below(4) {
                0 => {
                    model.remove(name);
                    format!("ZREM z {name}")
                }
                1 => {
                    *model.entry(name).or_insert(0) += score;
                    format!("ZINCRBY z {score} {name}")
                }
                _ => {
                    model.insert(name, score);
                    format!("ZADD z {score} {name}")
                }
            };
            history.push(line.clone());
            run(&mut e, &line);
            let mut sorted: Vec<(i64, &str)> = model.iter().map(|(n, s)| (*s, *n)).collect();
            sorted.sort();
            let ctx = |q: &str| format!("run {run_number}: after {history:?}: {q}");
            check!(ctx("ZCARD z"), run(&mut e, "ZCARD z"), int(sorted.len() as i64));
            let probe = *rng.pick(&names);
            let rank = sorted.iter().position(|(_, n)| *n == probe);
            check!(ctx(&format!("ZRANK z {probe}")), run(&mut e, &format!("ZRANK z {probe}")), rank.map_or(Frame::Null, |r| int(r as i64)));
            let (low, high) = (rng.int(-6, 6), rng.int(-6, 6));
            let (lx, hx) = (rng.bool(), rng.bool());
            let inside: Vec<&str> = sorted
                .iter()
                .filter(|(s, _)| (if lx { *s > low } else { *s >= low }) && (if hx { *s < high } else { *s <= high }))
                .map(|(_, n)| *n)
                .collect();
            let range = format!("{}{low} {}{high}", if lx { "(" } else { "" }, if hx { "(" } else { "" });
            check!(ctx(&format!("ZRANGEBYSCORE z {range}")), run(&mut e, &format!("ZRANGEBYSCORE z {range}")), arr(&inside));
            check!(ctx(&format!("ZCOUNT z {range}")), run(&mut e, &format!("ZCOUNT z {range}")), int(inside.len() as i64));
            let (a, b) = (rng.int(-10, 10), rng.int(-10, 10));
            let n = sorted.len() as i64;
            let from = if a < 0 { (n + a).max(0) } else { a };
            let to = if b < 0 { n + b } else { b.min(n - 1) };
            let mut want = Vec::new();
            for i in from..=to {
                if i < n {
                    want.push(sorted[i as usize].1.to_string());
                    want.push(sorted[i as usize].0.to_string());
                }
            }
            let want: Vec<&str> = want.iter().map(String::as_str).collect();
            check!(ctx(&format!("ZRANGE z {a} {b} WITHSCORES")), run(&mut e, &format!("ZRANGE z {a} {b} WITHSCORES")), arr(&want));
        }
    }
}

#[test]
fn a_large_sorted_set_stays_fast() {
    // 100000 members: a command that walks the whole set cannot finish in time
    let mut e = Engine::new();
    let n = 100_000i64;
    for i in 0..n {
        run(&mut e, &format!("ZADD z {} m{i:06}", (i * 7919) % n));
    }
    for i in 0..n {
        let score = (i * 7919) % n;
        check!(format!("ZRANK of the member with score {score}"), run(&mut e, &format!("ZRANK z m{i:06}")), int(score));
        check!(format!("ZCOUNT -inf {score}"), run(&mut e, &format!("ZCOUNT z -inf {score}")), int(score + 1));
        let reply = run(&mut e, &format!("ZRANGE z {score} {score}"));
        check!(format!("ZRANGE {score} {score}"), reply, arr(&[&format!("m{i:06}")]));
    }
}

#[test]
fn scan_options_and_cursors() {
    let mut e = Engine::new();
    check!("an empty keyspace", run(&mut e, "SCAN 0"), Frame::Array(vec![bulk("0"), arr(&[])]));
    for line in ["SCAN 0 COUNT 0", "SCAN 0 COUNT -1", "SCAN 0 COUNT", "SCAN 0 MATCH", "SCAN 0 BOGUS 1"] {
        check!(line, run(&mut e, line), err("ERR syntax error"));
    }
    check!("COUNT is a number", run(&mut e, "SCAN 0 COUNT x"), err(NOT_INT));
    check!("a cursor SCAN never gave", run(&mut e, "SCAN hello"), err("ERR invalid cursor"));
}

#[test]
fn scan_counts_far_larger_than_the_keyspace() {
    let mut e = Engine::new();
    for line in ["SET k1 v", "SET k2 v", "SET k3 v"] {
        run(&mut e, line);
    }
    for count in ["1000000000000", "4611686018427387904", "9223372036854775807"] {
        let line = format!("SCAN 0 COUNT {count}");
        check!(line.clone(), run(&mut e, &line), Frame::Array(vec![bulk("0"), arr(&["k1", "k2", "k3"])]));
        let line = format!("SCAN 0 MATCH *2 COUNT {count}");
        check!(line.clone(), run(&mut e, &line), Frame::Array(vec![bulk("0"), arr(&["k2"])]));
    }
    check!("a COUNT beyond i64", run(&mut e, "SCAN 0 COUNT 9223372036854775808"), err(NOT_INT));
    check!("the most negative COUNT", run(&mut e, "SCAN 0 COUNT -9223372036854775808"), err("ERR syntax error"));
}

#[test]
fn count_limits_the_keys_looked_at_not_the_matches() {
    let mut e = Engine::new();
    for i in 0..100 {
        run(&mut e, &format!("SET k{i:03} v"));
    }
    let (keys, calls) = scan_all(&mut e, " MATCH nothing* COUNT 10", 0);
    check!("no key matches", keys.len(), 0);
    check!(format!("{calls} calls to look at 100 keys, 10 at a time"), (10..=11).contains(&calls), true);
    let (keys, calls) = scan_all(&mut e, "", 10);
    check!("the default COUNT is 10", (10..=11).contains(&calls), true);
    check!("every key", keys.len(), 100);
    let (mut keys, _) = scan_all(&mut e, " count 33 match k0?7", 33);
    keys.sort();
    check!("MATCH k0?7", keys, (0..10).map(|i| format!("k0{i}7")).collect::<Vec<_>>());
}

#[test]
fn keys_that_stay_are_seen_exactly_once_while_others_come_and_go() {
    let mut rng = Rng::new(0x3207);
    for run_number in 0..100 {
        let mut e = Engine::new();
        let stable: Vec<String> = (0..20).map(|i| format!("stay{i:02}")).collect();
        for key in &stable {
            run(&mut e, &format!("SET {key} v"));
        }
        let mut passing: Vec<String> = (0..20).map(|_| rng.string(5, "asz")).collect();
        passing.retain(|k| !k.is_empty());
        for key in &passing {
            run(&mut e, &format!("SET {key} v"));
        }
        let count = rng.below(5) + 1;
        let (mut cursor, mut seen, mut calls) = ("0".to_string(), Vec::new(), 0);
        loop {
            let (next, found) = scan_once(&mut e, &format!("SCAN {cursor} COUNT {count}"));
            seen.extend(found);
            calls += 1;
            assert!(calls < 2000, "run {run_number}: SCAN never finished");
            if next == "0" {
                break;
            }
            cursor = next;
            // churn between the calls, the keys just returned included
            if calls < 100 {
                for _ in 0..rng.below(4) {
                    if let Some(victim) = passing.pop() {
                        run(&mut e, &format!("DEL {victim}"));
                    }
                }
                if rng.bool() {
                    let key = format!("new{}", rng.string(4, "ab"));
                    run(&mut e, &format!("SET {key} v"));
                }
            }
        }
        let mut stayed: Vec<String> = seen.into_iter().filter(|k| k.starts_with("stay")).collect();
        stayed.sort();
        check!(format!("run {run_number}, COUNT {count}"), stayed, stable);
    }
}

#[test]
fn keys_with_bytes_that_are_not_text_can_be_scanned() {
    let mut e = Engine::new();
    let keys: Vec<Vec<u8>> = vec![vec![], vec![0], vec![0, 0], vec![0xff, 0xfe], b"0".to_vec(), b" ".to_vec()];
    for key in &keys {
        e.execute(&[b"SET".to_vec(), key.clone(), b"v".to_vec()], 0);
    }
    let (mut cursor, mut seen) = (b"0".to_vec(), Vec::new());
    for _ in 0..20 {
        let reply = e.execute(&[b"SCAN".to_vec(), cursor.clone(), b"COUNT".to_vec(), b"1".to_vec()], 0);
        let Frame::Array(parts) = reply else { panic!("SCAN reply") };
        let [Frame::Bulk(next), Frame::Array(found)] = &parts[..] else { panic!("SCAN reply") };
        seen.extend(found.clone());
        cursor = next.clone();
        if cursor == b"0" {
            break;
        }
    }
    check!("the scan finished", cursor, b"0".to_vec());
    let mut want: Vec<Frame> = keys.into_iter().map(Frame::Bulk).collect();
    seen.sort_by_key(|f| format!("{f:?}"));
    want.sort_by_key(|f| format!("{f:?}"));
    check!("each key once", seen, want);
}
