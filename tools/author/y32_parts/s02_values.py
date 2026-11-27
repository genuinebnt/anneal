from y32_parts.common import *

SOLUTION = solution(2)

VISIBLE = [
    ENGINE,
    r'''
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
''',
]

HIDDEN = [
    ENGINE,
    r'''
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
''',
]

STATEMENT = r'''
**Capstone, step 2 of 10.** The server holds keys, and each key holds one kind of value. This step runs one command against those keys. The starter holds the reference code of step 1.

@SPOILERS@

Write `Engine`. `execute(args, now)` runs one command and returns its reply.

`args[0]` is the command's name, in any letter case. Ignore `now` until the next step. Keys, values, fields and members are any bytes.

**Refusals.** A refused command changes nothing.

- An unknown name, or no arguments at all: an error starting `ERR unknown command`.
- Too few or too many arguments: `ERR wrong number of arguments for 'name' command`, with the name in lower case.
- A key that holds another kind of value: `WRONGTYPE Operation against a key holding the wrong kind of value`. `MGET`, the commands about keys, `SET` and `MSET` never say this.
- An argument or a stored string that should be an integer and is not: `ERR value is not an integer or out of range`. An integer is an optional `-` then digits, within `i64`.

**Replies** below are `Frame` values. "Nil" is `Null`. `OK` is `Simple("OK")`. Strings you return are `Bulk`.

| Command | Reply |
|---|---|
| `PING [message]` | `Simple("PONG")`, or the message |
| `ECHO message` | the message |
| `SET key value` | `OK`; replaces any kind |
| `GET key` | the string, or nil |
| `APPEND key value` | new length; a missing key starts empty |
| `STRLEN key` | length, 0 if missing |
| `INCR` / `DECR key`, `INCRBY key n` | the new number; missing counts as 0; if it does not fit: `ERR increment or decrement would overflow` |
| `MSET key value [key value ...]` | `OK`; an odd pair is a wrong number of arguments |
| `MGET key [key ...]` | array, nil for a missing key or another kind |
| `DEL key [key ...]` | how many keys were removed |
| `EXISTS key [key ...]` | how many arguments name a key; a repeat counts again |
| `TYPE key` | `Simple`: `none`, `string`, `list`, `hash` or `set` |
| `RENAME from to` | `OK`; `to` is replaced (renaming a key to itself keeps it); a missing `from` is `ERR no such key` |
| `DBSIZE` | how many keys |
| `FLUSHALL` | `OK`; removes every key |

**Lists.** `LPUSH` / `RPUSH key value [value ...]` add one value at a time at the left or right end, and reply with the length. `LLEN key` is the length. `LRANGE key start stop` is the elements from `start` to `stop`, both included.

`LPOP` / `RPOP key` reply with the removed element, or nil. With a count, `LPOP key n` replies with an array of up to `n` elements in the order removed, or `NullArray` for a missing key. A negative count is `ERR value is out of range, must be positive`. The count is checked before the key. The count comes from a stranger: every count up to `9223372036854775807` is valid, however short the list. Do not reserve memory or loop by the count; the work depends only on what the list holds.

In `LRANGE` and `LTRIM key start stop`, a negative index counts from the end (-1 is the last). Indexes past either end are clamped, and a range can be empty. `LTRIM` keeps only the range and replies `OK`.

**Hashes.** `HSET key field value [field value ...]` replies with the number of new fields; a field with no value is a wrong number of arguments.

`HGET` is the value or nil. `HDEL key field [field ...]` replies with how many it removed. `HLEN` counts fields. `HGETALL` is `field, value, ...` ordered by field.

`HINCRBY key field n` works as `INCRBY` on one field. A stored value that is not an integer is `ERR hash value is not an integer`.

**Sets.** `SADD` / `SREM key member [member ...]` reply with how many were added or removed. `SISMEMBER` is 1 or 0, `SCARD` counts, `SMEMBERS` lists in byte order.

`SINTER`, `SUNION` and `SDIFF key [key ...]` reply with the result in byte order. A missing key is an empty set, and **every** key is checked for its kind. `SDIFF` is the first set without the others.

**A key never holds an empty list, hash or set.** Removing the last element removes the key. Commands that only read or remove never create a key.
'''

PROBLEM = dict(
    slug="values-and-commands", title="Capstone 2: values and commands", level="medium", stage="protocol-and-data",
    tags=["command dispatch", "enum", "BTreeMap", "VecDeque", "HashSet", "error replies"],
    statement=STATEMENT.replace("@SPOILERS@", spoilers(2)), starter=starter(2), solution=SOLUTION, visible=VISIBLE, hidden=HIDDEN,
    teaches=[
        "An enum of value kinds plus one typed accessor per kind puts the wrong-kind check in one place.",
        "Check the name and argument count in one table before any command runs.",
        "Keep one invariant everywhere: no empty collection stays in the keyspace.",
    ],
    follow_up="1) How would a table of `(name, arity, writes, function pointer)` compare with your `match`? What does Redis use the `writes` flag for? 2) `SUNION` clones every set. How would you do it with borrowed data? 3) Which commands here cost more than O(1), and what could a client do with that on a single-threaded server?",
    hints=[
        ("approach", "Make one function that decides arity from the name. Then give each kind one accessor that returns the value, nothing, or the wrong-kind error. Most commands become a few lines."),
        ("rust", "`let Some(list) = self.list(key, false)? else { .. };` reads well if refusals are the `Err` side of `Result<Frame, Frame>`. `VecDeque` has `range(a..=b)`, `truncate` and `drain`; `BTreeSet` has `intersection`, `union` and `difference`."),
        ("edge", "`start + len` can overflow for `i64::MIN`; use saturating arithmetic. Parse every argument before you create a key, or a refused command leaves an empty one behind."),
    ],
    notes=("One enum holds the five kinds, and a small macro makes the typed accessors. All removal goes through one path that drops empty collections. Commands return `Result<Frame, Frame>`, so `?` turns a refusal into the reply.",
           "O(1) for most commands; ranges and set operations are linear", "O(data)"),
    wrong={
        "empty_collections_stay": broken(SOLUTION, (
            "        if self.map.get(key).is_some_and(Value::is_empty_collection) {\n            self.remove_key(key);\n        }\n", "")),
        "hincrby_creates_before_checking": broken(SOLUTION, (
            "            \"HINCRBY\" => {\n                let by = int_arg(&args[3])?;",
            "            \"HINCRBY\" => {\n                self.hash(&args[1], true)?;\n                let by = int_arg(&args[3])?;")),
        "set_operations_stop_at_a_missing_key": broken(SOLUTION, (
            "        for key in keys {\n            sets.push(self.set_of(key, false)?.cloned().unwrap_or_default());",
            "        for key in keys {\n            let missing = !self.map.contains_key(key);\n            sets.push(self.set_of(key, false)?.cloned().unwrap_or_default());\n            if missing && name == \"SINTER\" {\n                break;\n            }")),
        "negative_stop_clamped_to_zero": broken(SOLUTION, (
            "    let stop = if stop < 0 {\n        len.saturating_add(stop)", "    let stop = if stop < 0 {\n        len.saturating_add(stop).max(0)")),
        "pop_reserves_by_count": broken(SOLUTION, (
            "        let mut out: Vec<Vec<u8>> = (0..take)\n            .filter_map(|_| {\n                if left {\n                    list.pop_front()\n                } else {\n                    list.pop_back()\n                }\n            })\n            .collect();",
            "        let mut out: Vec<Vec<u8>> = Vec::with_capacity(count.unwrap_or(1));\n        out.extend((0..take).filter_map(|_| {\n            if left {\n                list.pop_front()\n            } else {\n                list.pop_back()\n            }\n        }));")),
        "wrapping_counter": broken(SOLUTION, (
            "        let new = old.checked_add(by).ok_or_else(|| error(OVERFLOW))?;\n        // only", "        let new = old.wrapping_add(by);\n        // only")),
        "unknown_commands_ignored": broken(SOLUTION, ("        None => Some(error(\"ERR unknown command\")),", "        None => None,"),
            ("            _ => return Err(error(\"ERR unknown command\")),", "            _ => ok(),")),
    },
)
