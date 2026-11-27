from y32_parts.common import *

SOLUTION = solution(3)

VISIBLE = [
    ENGINE,
    DICE,
    r'''
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
''',
]

HIDDEN = [
    ENGINE,
    DICE,
    r'''
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
''',
]

STATEMENT = r'''
**Capstone, step 3 of 10.** A cache forgets. Any key may now have a deadline, after which it is gone. The starter holds the reference code of steps 1 and 2, and `Deadlines`, which stores deadlines and sweeps through them.

@SPOILERS@

Make deadlines work in `Engine`. All times are in milliseconds, and `now` is the current one. Time comes only from `now`.

**The rule.** A key with deadline `t` exists while `now < t`. From `now == t` on, **every command treats it as missing**, including those that only count keys, like `DBSIZE`. A key made again has no deadline.

| Command | Reply |
|---|---|
| `EXPIRE key seconds`, `PEXPIRE key ms` | sets the deadline that far from `now`; 1, or 0 for a missing key |
| `PEXPIREAT key t` | sets the deadline to `t`; 1 or 0 |
| `PTTL key` | milliseconds left; -1 with no deadline, -2 for a missing key |
| `TTL key` | the same in seconds, rounded to the nearest, halves upwards |
| `PERSIST key` | removes the deadline; 1, or 0 if there was none or no key |

- A deadline that is not in the future, negative ones included, removes the key at once and replies 1.
- If the deadline cannot be computed within `i64`, reply `ERR invalid expire time in 'expire' command` (or `'pexpire'`), and change nothing.

**`SET key value [options]`**, in any letter case:

- `NX` sets only if the key is missing, `XX` only if it exists. When the condition fails, reply nil and change nothing.
- `EX seconds` and `PX ms` set a deadline from `now`. `PXAT t` sets the deadline `t`. A `t` in the past is accepted, and the key is then already gone.
- `KEEPTTL` keeps the old deadline.
- A value of 0 or less, or a deadline that does not fit in `u64`, is `ERR invalid expire time in 'set' command`. A value that is not an integer is the usual integer error.
- `ERR syntax error`: an unknown option, a missing value, `NX` with `XX`, two of `EX`, `PX` and `PXAT`, or one of them with `KEEPTTL`.

**Which commands keep a deadline.** Commands that change a value in place keep it: `APPEND`, the counters, and the list, hash and set commands. `SET` without `KEEPTTL` and `MSET` clear it. `RENAME` moves the source's deadline with its value, and the target's old one is lost.

**`cycle(now, dice)`** frees keys that nobody asks for. It runs one `Deadlines::sweep` with at most 16 rounds, removes those keys, and returns how many it removed. `dice.below(n)` gives a random number in `0..n`.
'''

PROBLEM = dict(
    slug="keys-that-expire", title="Capstone 3: keys that expire", level="medium", stage="protocol-and-data",
    tags=["TTL", "lazy expiry", "active expiry", "injected time"],
    statement=STATEMENT.replace("@SPOILERS@", spoilers(3)), starter=starter(3), solution=SOLUTION, visible=VISIBLE, hidden=HIDDEN,
    teaches=[
        "Lazy expiry works only if every path to a key goes through one check.",
        "Passing time in as an argument makes expiry testable to the millisecond.",
        "Which operations keep a deadline is part of the contract, and each one needs deciding.",
    ],
    follow_up="1) `DBSIZE` here counts live keys, which costs a pass over them. Real Redis counts stored keys instead. What does each choice cost? 2) The system clock can jump backwards. What happens to deadlines, and what would a monotonic clock change? 3) Why can a replica not simply expire keys by its own clock?",
    hints=[
        ("approach", "Find the narrowest place every key access passes. Remove an expired key there, then let the command carry on as if it never existed."),
        ("rust", "Your typed accessors are one such place. Commands that go to the map directly need the same call first. `checked_mul` and `checked_add` give the expire-time errors."),
        ("edge", "`DEL`, `RENAME` (both keys), `MSET` and `TYPE` do not go through a typed accessor. `SET .. KEEPTTL` on an expired key must not find the old deadline."),
    ],
    notes=("Every access first removes the key if its deadline has passed, so no command can see a stale one. `cycle` reuses the sweep from the preparation part, so memory held by keys nobody asks for is also freed.",
           "O(1) extra per access; a cycle looks at no more than 320 keys", "O(keys with deadlines)"),
    wrong={
        "gone_only_after_the_deadline": broken(SOLUTION, (
            "        self.deadlines.get(key).is_some_and(|at| at <= self.now)", "        self.deadlines.get(key).is_some_and(|at| at < self.now)")),
        "del_sees_expired_keys": broken(SOLUTION, (
            "                for key in &args[1..] {\n                    self.purge(key);\n                    if self.remove_key(key) {",
            "                for key in &args[1..] {\n                    if self.remove_key(key) {")),
        "dbsize_counts_stored_keys": broken(SOLUTION, (
            "                let live = self.map.keys().filter(|k| !self.expired(k)).count();", "                let live = self.map.len();")),
        "rename_leaves_the_deadline_behind": broken(SOLUTION, (
            "                if let Some(at) = deadline {\n                    self.deadlines.set(&args[2], at);\n                }\n", "                let _ = deadline;\n")),
        "increment_clears_the_deadline": broken(SOLUTION, (
            "        self.map\n            .insert(key.to_vec(), Value::Str(new.to_string().into_bytes()));\n        self.touch(key);",
            "        self.put(key, new.to_string().into_bytes(), false, None);")),
        "ttl_rounds_down": broken(SOLUTION, ("                    ms.saturating_add(500) / 1000", "                    ms / 1000")),
        "cycle_removes_one_round": broken(SOLUTION, ("        let removed = self.deadlines.sweep(now, dice, 16);", "        let removed = self.deadlines.sweep(now, dice, usize::MAX);")),
    },
)
