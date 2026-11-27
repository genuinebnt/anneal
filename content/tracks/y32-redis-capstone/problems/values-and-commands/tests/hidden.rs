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
fn every_command_checks_how_many_arguments_it_has() {
    // name, an accepted and a refused number of arguments (the name not counted)
    let table: [(&str, &[usize], &[usize]); 38] = [
        ("PING", &[0, 1], &[2]), ("ECHO", &[1], &[0, 2]), ("GET", &[1], &[0, 2]), ("SET", &[2], &[0, 1]),
        ("APPEND", &[2], &[1, 3]), ("STRLEN", &[1], &[0, 2]), ("INCR", &[1], &[0, 2]), ("DECR", &[1], &[0, 2]),
        ("INCRBY", &[2], &[1, 3]), ("MGET", &[1, 3], &[0]), ("MSET", &[2, 4], &[0, 1, 3]),
        ("DEL", &[1, 3], &[0]), ("EXISTS", &[1, 3], &[0]), ("TYPE", &[1], &[0, 2]), ("RENAME", &[2], &[1, 3]),
        ("DBSIZE", &[0], &[1]), ("FLUSHALL", &[0], &[1]),
        ("LPUSH", &[2, 4], &[0, 1]), ("RPUSH", &[2, 4], &[0, 1]), ("LPOP", &[1, 2], &[0, 3]), ("RPOP", &[1, 2], &[0, 3]),
        ("LLEN", &[1], &[0, 2]), ("LRANGE", &[3], &[2, 4]), ("LTRIM", &[3], &[2, 4]),
        ("HSET", &[3, 5], &[1, 2, 4]), ("HGET", &[2], &[1, 3]), ("HDEL", &[2, 4], &[1]), ("HLEN", &[1], &[0, 2]),
        ("HGETALL", &[1], &[0, 2]), ("HINCRBY", &[3], &[2, 4]),
        ("SADD", &[2, 4], &[1]), ("SREM", &[2, 4], &[1]), ("SISMEMBER", &[2], &[1, 3]), ("SCARD", &[1], &[0, 2]),
        ("SMEMBERS", &[1], &[0, 2]), ("SINTER", &[1, 3], &[0]), ("SUNION", &[1, 3], &[0]), ("SDIFF", &[1, 3], &[0]),
    ];
    for (name, good, bad) in table {
        for (counts, want) in [(good, false), (bad, true)] {
            for &n in counts {
                let line = std::iter::once(name).chain(std::iter::repeat("1").take(n)).collect::<Vec<_>>().join(" ");
                let reply = run(&mut Engine::new(), &line);
                let refused = reply == err(&format!("ERR wrong number of arguments for '{}' command", name.to_lowercase()));
                check!(format!("{line} gave {reply:?}; is that an arity error?"), refused, want);
            }
        }
    }
}

#[test]
fn no_command_at_all() {
    let reply = Engine::new().execute(&[], 0);
    check!(format!("{reply:?}"), is_error(&reply, "ERR"), true);
}

#[test]
fn counters_refuse_what_is_not_a_number() {
    script(&mut Engine::new(), &[
        ("SET a abc", ok()),
        ("INCR a", err(NOT_INT)),
        ("GET a", bulk("abc")),
        ("SET b +1", ok()),
        ("INCR b", err(NOT_INT)),
        ("INCRBY c 1.5", err(NOT_INT)),
        ("EXISTS c", int(0)),
        ("SET max 9223372036854775807", ok()),
        ("INCR max", err("ERR increment or decrement would overflow")),
        ("GET max", bulk("9223372036854775807")),
        ("SET min -9223372036854775808", ok()),
        ("DECR min", err("ERR increment or decrement would overflow")),
        ("INCRBY min -1", err("ERR increment or decrement would overflow")),
        ("HSET h f x", int(1)),
        ("HINCRBY h f 1", err("ERR hash value is not an integer")),
        ("HINCRBY h g nope", err(NOT_INT)),
        ("HINCRBY h g -5", int(-5)),
        ("HINCRBY h g 2", int(-3)),
        ("HGET h g", bulk("-3")),
        ("HINCRBY h g -9223372036854775808", err("ERR increment or decrement would overflow")),
        ("HINCRBY fresh g nope", err(NOT_INT)),
        ("EXISTS fresh", int(0)),
    ]);
}

#[test]
fn pop_counts() {
    script(&mut Engine::new(), &[
        ("RPUSH l a b", int(2)),
        ("LPOP l 0", arr(&[])),
        ("LPOP l -1", err("ERR value is out of range, must be positive")),
        ("LPOP l x", err(NOT_INT)),
        ("LPOP missing x", err(NOT_INT)),
        ("LPOP l 9", arr(&["a", "b"])),
        ("TYPE l", simple("none")),
        ("RPOP l", Frame::Null),
        ("RPOP l 0", Frame::NullArray),
    ]);
}

#[test]
fn pop_counts_far_larger_than_the_list() {
    let mut e = Engine::new();
    for count in ["1000000000000", "4611686018427387904", "9223372036854775807"] {
        run(&mut e, "RPUSH l a b c");
        check!(format!("LPOP l {count}"), run(&mut e, &format!("LPOP l {count}")), arr(&["a", "b", "c"]));
        run(&mut e, "RPUSH l a b c");
        check!(format!("RPOP l {count}"), run(&mut e, &format!("RPOP l {count}")), arr(&["c", "b", "a"]));
        check!(format!("LPOP of a missing key, count {count}"), run(&mut e, &format!("LPOP l {count}")), Frame::NullArray);
    }
    script(&mut e, &[
        ("LPOP l 9223372036854775808", err(NOT_INT)),
        ("RPOP l -9223372036854775808", err("ERR value is out of range, must be positive")),
        ("DBSIZE", int(0)),
    ]);
}

#[test]
fn ranges_at_the_edges() {
    let mut e = Engine::new();
    run(&mut e, "RPUSH l 0 1 2 3 4");
    let model = ["0", "1", "2", "3", "4"];
    let ends = [i64::MIN, -100, -6, -5, -4, -1, 0, 1, 4, 5, 100, i64::MAX];
    for start in ends {
        for stop in ends {
            let a = if start < 0 { (start as i128 + 5).max(0) } else { start as i128 };
            let b = if stop < 0 { stop as i128 + 5 } else { (stop as i128).min(4) };
            let want: Vec<&str> = (0..5i128).filter(|i| a <= *i && *i <= b).map(|i| model[i as usize]).collect();
            check!(format!("LRANGE l {start} {stop}"), run(&mut e, &format!("LRANGE l {start} {stop}")), arr(&want));
            let mut trimmed = Engine::new();
            run(&mut trimmed, "RPUSH l 0 1 2 3 4");
            check!(format!("LTRIM l {start} {stop}"), run(&mut trimmed, &format!("LTRIM l {start} {stop}")), ok());
            check!(format!("after LTRIM l {start} {stop}"), run(&mut trimmed, "LRANGE l 0 -1"), arr(&want));
            check!(format!("EXISTS after LTRIM l {start} {stop}"), run(&mut trimmed, "EXISTS l"), int(i64::from(!want.is_empty())));
        }
    }
    script(&mut e, &[("LRANGE missing 0 -1", arr(&[])), ("LTRIM missing 0 -1", ok()), ("EXISTS missing", int(0)), ("LRANGE l a 1", err(NOT_INT))]);
}

#[test]
fn no_key_holds_an_empty_collection() {
    script(&mut Engine::new(), &[
        ("SADD s a", int(1)), ("SREM s a", int(1)), ("TYPE s", simple("none")),
        ("HSET h f v", int(1)), ("HDEL h f", int(1)), ("TYPE h", simple("none")),
        ("RPUSH l a", int(1)), ("RPOP l", bulk("a")), ("TYPE l", simple("none")),
        ("DBSIZE", int(0)),
        // reading or removing from a missing key creates nothing
        ("SREM s a", int(0)), ("HDEL h f", int(0)), ("HGET h f", Frame::Null), ("SCARD s", int(0)),
        ("HGETALL h", arr(&[])), ("SMEMBERS s", arr(&[])), ("LLEN l", int(0)), ("SINTER s", arr(&[])),
        ("DBSIZE", int(0)),
    ]);
}

#[test]
fn commands_about_keys() {
    script(&mut Engine::new(), &[
        ("SET a 1", ok()), ("SADD b x", int(1)), ("HSET c f v", int(1)),
        ("TYPE a", simple("string")), ("TYPE b", simple("set")), ("TYPE c", simple("hash")),
        ("DBSIZE", int(3)),
        ("EXISTS a a b nothing", int(3)),
        ("DEL a a nothing", int(1)),
        ("RENAME nothing b", err("ERR no such key")),
        ("SCARD b", int(1)),
        ("RENAME b c", ok()),
        ("TYPE c", simple("set")),
        ("EXISTS b", int(0)),
        ("RENAME c c", ok()),
        ("SMEMBERS c", arr(&["x"])),
        ("SET d v", ok()),
        ("FLUSHALL", ok()),
        ("DBSIZE", int(0)),
        ("TYPE c", simple("none")),
    ]);
}

#[test]
fn writing_a_string_replaces_any_kind() {
    script(&mut Engine::new(), &[
        ("RPUSH a x", int(1)), ("SET a 1", ok()), ("GET a", bulk("1")),
        ("SADD b x", int(1)), ("MSET b 2 c 3", ok()), ("GET b", bulk("2")),
        ("HSET h f v", int(1)),
        ("MGET h c", Frame::Array(vec![Frame::Null, bulk("3")])),
        ("APPEND h x", err(WRONGTYPE)),
        ("STRLEN h", err(WRONGTYPE)),
    ]);
}

#[test]
fn every_key_of_a_set_operation_is_checked() {
    script(&mut Engine::new(), &[
        ("SADD s a b", int(2)),
        ("SET text v", ok()),
        ("SINTER missing text", err(WRONGTYPE)),
        ("SUNION s text", err(WRONGTYPE)),
        ("SDIFF missing s text", err(WRONGTYPE)),
        ("SINTER s missing", arr(&[])),
        ("SDIFF s missing", arr(&["a", "b"])),
        ("SDIFF missing s", arr(&[])),
        ("SUNION missing s missing", arr(&["a", "b"])),
        ("SDIFF s s", arr(&[])),
        ("SCARD s", int(2)),
    ]);
}

#[test]
fn keys_and_values_are_any_bytes() {
    let mut e = Engine::new();
    let key = vec![0u8, 255, b' ', b'\r', b'\n'];
    let value: Vec<u8> = (0..=255).collect();
    check!("SET", e.execute(&[b"SET".to_vec(), key.clone(), value.clone()], 0), ok());
    check!("GET", e.execute(&[b"GET".to_vec(), key.clone()], 0), Frame::Bulk(value));
    check!("STRLEN", e.execute(&[b"STRLEN".to_vec(), key], 0), int(256));
}

#[test]
fn random_lists_sets_and_counters_match_a_model() {
    use std::collections::{BTreeMap, BTreeSet, VecDeque};
    #[derive(Clone)]
    enum Model {
        List(VecDeque<String>),
        Set(BTreeSet<String>),
        Counter(i64),
    }
    let mut rng = Rng::new(0x3210);
    for run_number in 0..200 {
        let mut e = Engine::new();
        let mut model: BTreeMap<String, Model> = BTreeMap::new();
        let mut history = Vec::new();
        for _ in 0..60 {
            let key = rng.pick(&["a", "b", "c"]).to_string();
            let word = rng.pick(&["x", "y", "z"]).to_string();
            let n = rng.int(-4, 4);
            let wrong = err(WRONGTYPE);
            let (line, want) = match rng.below(9) {
                0 | 1 => {
                    let left = rng.bool();
                    let want = match model.entry(key.clone()).or_insert_with(|| Model::List(VecDeque::new())) {
                        Model::List(l) => {
                            if left { l.push_front(word.clone()) } else { l.push_back(word.clone()) }
                            int(l.len() as i64)
                        }
                        _ => wrong,
                    };
                    (format!("{} {key} {word}", if left { "LPUSH" } else { "RPUSH" }), want)
                }
                2 => {
                    let left = rng.bool();
                    let want = match model.get_mut(&key) {
                        Some(Model::List(l)) => {
                            let v = if left { l.pop_front() } else { l.pop_back() };
                            let v = bulk(&v.expect("never empty"));
                            if l.is_empty() { model.remove(&key); }
                            v
                        }
                        None => Frame::Null,
                        _ => wrong,
                    };
                    (format!("{} {key}", if left { "LPOP" } else { "RPOP" }), want)
                }
                3 => {
                    let want = match model.entry(key.clone()).or_insert_with(|| Model::Set(BTreeSet::new())) {
                        Model::Set(s) => int(i64::from(s.insert(word.clone()))),
                        _ => wrong,
                    };
                    (format!("SADD {key} {word}"), want)
                }
                4 => {
                    let want = match model.get_mut(&key) {
                        Some(Model::Set(s)) => {
                            let had = s.remove(&word);
                            if s.is_empty() { model.remove(&key); }
                            int(i64::from(had))
                        }
                        None => int(0),
                        _ => wrong,
                    };
                    (format!("SREM {key} {word}"), want)
                }
                5 => {
                    let want = match model.entry(key.clone()).or_insert(Model::Counter(0)) {
                        Model::Counter(c) => {
                            *c += n;
                            int(*c)
                        }
                        _ => wrong,
                    };
                    (format!("INCRBY {key} {n}"), want)
                }
                6 => (format!("DEL {key}"), int(i64::from(model.remove(&key).is_some()))),
                7 => {
                    let other = rng.pick(&["a", "b", "c"]).to_string();
                    let want = match model.remove(&key) {
                        Some(v) => {
                            model.insert(other.clone(), v);
                            ok()
                        }
                        None => err("ERR no such key"),
                    };
                    (format!("RENAME {key} {other}"), want)
                }
                _ => {
                    let want = match model.get(&key) {
                        None => simple("none"),
                        Some(Model::List(_)) => simple("list"),
                        Some(Model::Set(_)) => simple("set"),
                        Some(Model::Counter(_)) => simple("string"),
                    };
                    (format!("TYPE {key}"), want)
                }
            };
            history.push(line.clone());
            check!(format!("run {run_number}: {history:?}"), run(&mut e, &line), want);
        }
        check!(format!("run {run_number}: DBSIZE after {history:?}"), run(&mut e, "DBSIZE"), int(model.len() as i64));
    }
}
