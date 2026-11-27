from y32_parts.common import *

SOLUTION = solution(4)

VISIBLE_HELPERS = ENGINE + r'''
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
'''

VISIBLE = [
    VISIBLE_HELPERS,
    r'''
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
''',
]

HIDDEN = [
    VISIBLE_HELPERS,
    r'''
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
''',
]

STATEMENT = r'''
**Capstone, step 4 of 10.** Two things are missing from the keyspace: the fifth kind of value, and a way to find keys. The starter holds the reference code of steps 1 to 3. It also gives `Ranked`, a set kept in score order, and `glob_matches`, which matches a key pattern.

@SPOILERS@

**Sorted sets.** A sorted set holds distinct members, each with a score. Its order is by score, then by the member's bytes. They obey all the rules of the other collections, and `TYPE` calls them `zset`.

A set may hold 100000 members, so no command may walk past members it does not return.

- A score is any `f64` text Rust can parse, `inf`, `+inf` and `-inf` included, but **not NaN**. `-0` is `0`. Anything else is `ERR value is not a valid float`.
- A score is written back as Rust's `{}` writes an `f64`: `3`, `12.5`, `inf`.

| Command | Reply |
|---|---|
| `ZADD key score member [score member ...]` | the number of **new** members; a lone score is `ERR syntax error` |
| `ZREM key member [member ...]` | how many it removed |
| `ZCARD key` | the number of members |
| `ZSCORE key member` | the score as a string, or nil |
| `ZRANK key member` | the place in the order, from 0, or nil |
| `ZRANGE key start stop [WITHSCORES]` | members by place, indexes as in `LRANGE` |
| `ZRANGEBYSCORE key min max [WITHSCORES]` | members with a score from `min` to `max` |
| `ZCOUNT key min max` | how many that range holds |
| `ZINCRBY key amount member` | the new score; a missing member starts at 0 |

- `ZADD` checks every score before it changes anything.
- With `WITHSCORES`, each member is followed by its score. Any other word there is `ERR syntax error`.
- In `min` and `max`, a leading `(` excludes that end. A bad one is `ERR min or max is not a float`. A range with `min` above `max` is empty.
- If `ZINCRBY` would give NaN: `ERR resulting score is not a number (NaN)`, and nothing changes.

**`KEYS pattern`** returns the keys that `glob_matches` accepts, in byte order.

**`SCAN cursor [MATCH pattern] [COUNT n]`** walks the keys a few at a time. It replies `Array([Bulk(next_cursor), Array(keys)])`. `COUNT` comes from a stranger: every count up to `9223372036854775807` is valid, however few keys there are. Do not reserve memory or loop by it.

- The caller starts with cursor `0`, passes back each cursor it gets, and is done when it gets `0`. Other cursors may be any text you like, without spaces. A cursor you could not have given is `ERR invalid cursor` (the tests try only `hello`).
- One call **looks at** at most `n` keys (10 if not given), so it returns at most `n`. `MATCH` filters what was looked at; a call may return nothing and still not be done. With no keys added, `k` keys take no more than `k / n`, rounded up, plus one calls.
- A key that exists from the first call to the last is returned **exactly once**, whatever is added or removed between calls. You keep no state for a scan.
- `n` below 1, a missing value or another option is `ERR syntax error`. An `n` that is not an integer is the integer error.

Neither command returns an expired key.
'''

PROBLEM = dict(
    slug="sorted-sets-and-finding-keys", title="Capstone 4: sorted sets and finding keys", level="hard", stage="protocol-and-data",
    tags=["sorted set", "order statistics", "glob", "cursor pagination", "BTreeMap range"],
    statement=STATEMENT.replace("@SPOILERS@", spoilers(4)), starter=starter(4), solution=SOLUTION, visible=VISIBLE, hidden=HIDDEN,
    teaches=[
        "Two questions about scores become one: how many members come before this point?",
        "A cursor that names the last key seen survives inserts and deletes; a position does not.",
        "Floats need decisions before they can be ordered: NaN, -0 and infinity.",
    ],
    follow_up="1) Add `ZRANGEBYSCORE .. LIMIT offset count`. What must stay O(log n)? 2) Your `SCAN` leans on an ordered map. Redis uses a hash table and reverses the bits of its cursor: what does that guarantee, and what does it give up? 3) Why is `KEYS` dangerous on a busy single-threaded server?",
    hints=[
        ("approach", "For a score range, find the place where it starts and the place where it ends. For `SCAN`, ask what the cursor can name that stays meaningful when other keys vanish."),
        ("rust", "`Ranked::count_while` takes a test on the score that holds for a prefix of the order. `BTreeMap::range((Bound::Excluded(last), Bound::Unbounded))` continues after a key. Keys are bytes, so encode the cursor, for example in hex."),
        ("edge", "A `-0` score must compare equal to `0` in the order. An excluded `max` below an included `min` must not give a negative count."),
    ],
    notes=("A range by score is two counts of a prefix, and their difference is its size. The scan cursor encodes the last key examined, so the next call starts just after it in the ordered map, whatever changed in between.",
           "O(log n + m) per sorted-set command; O(count · log n) per SCAN", "O(data)"),
    wrong={
        "cursor_is_a_position": broken(SOLUTION, (
            "        let after = match args[1].as_slice() {\n            b\"0\" => None,\n            [b'1', hex @ ..] => Some(from_hex(hex).ok_or_else(bad_cursor)?),\n            _ => return Err(bad_cursor()),\n        };",
            "        let after: Option<usize> = match args[1].as_slice() {\n            b\"0\" => None,\n            other => Some(std::str::from_utf8(other).ok().and_then(|s| s.parse().ok()).ok_or_else(bad_cursor)?),\n        };"),
            ("        use std::ops::Bound;\n        let from = after.map_or(Bound::Unbounded, Bound::Excluded);\n        let mut keys = self.map.range((from, Bound::Unbounded)).map(|(k, _)| k);\n        let (mut found, mut last) = (Vec::new(), None);\n        for key in keys.by_ref().take(count) {\n            last = Some(key);",
             "        let start = after.unwrap_or(0);\n        let mut keys = self.map.keys().skip(start);\n        let (mut found, mut seen) = (Vec::new(), start);\n        for key in keys.by_ref().take(count) {\n            seen += 1;"),
            ("        let cursor = match (keys.next(), last) {\n            (Some(_), Some(last)) => {\n                let hex: String = last.iter().map(|b| format!(\"{b:02x}\")).collect();\n                format!(\"1{hex}\")\n            }\n            _ => \"0\".to_string(),\n        };",
             "        let cursor = match keys.next() {\n            Some(_) => seen.to_string(),\n            None => \"0\".to_string(),\n        };")),
        "count_limits_matches": broken(SOLUTION, (
            "        for key in keys.by_ref().take(count) {\n            last = Some(key);\n            if !self.expired(key) && pattern.is_none_or(|p| glob_matches(p, key)) {\n                found.push(key.clone());\n            }\n        }",
            "        while found.len() < count {\n            let Some(key) = keys.next() else { break };\n            last = Some(key);\n            if !self.expired(key) && pattern.is_none_or(|p| glob_matches(p, key)) {\n                found.push(key.clone());\n            }\n        }")),
        "minus_zero_kept": broken(SOLUTION, ("    (!score.is_nan()).then_some(score + 0.0)", "    (!score.is_nan()).then_some(score)")),
        "scan_reserves_by_count": broken(SOLUTION, (
            "        let (mut found, mut last) = (Vec::new(), None);",
            "        let (mut found, mut last) = (Vec::with_capacity(count), None);")),
        "count_walks_the_set": broken(SOLUTION, (
            "                if name == \"ZCOUNT\" {\n                    return Ok(Frame::Int((end - start) as i64));\n                }",
            "                if name == \"ZCOUNT\" {\n                    let all = self.sorted(&args[1], false)?.map_or_else(Vec::new, |z| z.range(0, z.len()));\n                    let inside = all.iter().filter(|(_, s)| (*s > low.0 || (!low.1 && *s == low.0)) && (*s < high.0 || (!high.1 && *s == high.0)));\n                    return Ok(Frame::Int(inside.count() as i64));\n                }")),
        "zadd_checks_as_it_goes": broken(SOLUTION, (
            "                let pairs = args[2..]\n                    .chunks(2)\n                    .map(|pair| Ok((score_arg(&pair[0])?, &pair[1])))\n                    .collect::<Result<Vec<_>, Frame>>()?;\n                let set = self.sorted(&args[1], true)?.expect(\"created\");\n                let (mut added, mut changed) = (0, false);\n                for (score, member) in pairs {",
            "                self.sorted(&args[1], true)?;\n                let (mut added, mut changed) = (0, false);\n                for pair in args[2..].chunks(2) {\n                    let (score, member) = (score_arg(&pair[0])?, &pair[1]);\n                    let set = self.sorted(&args[1], true)?.expect(\"created\");")),
        "keys_includes_expired": broken(SOLUTION, (
            "                let live = self.map.keys().filter(|k| !self.expired(k));", "                let live = self.map.keys();")),
    },
)
