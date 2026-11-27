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

/// Runs a command that must produce exactly one reply, to the caller, and returns it.
#[allow(dead_code)]
fn say_at(core: &mut Core, id: ClientId, now: u64, line: &str) -> Frame {
    let out = core.request(id, args(line), now);
    assert!(
        out.len() == 1 && out[0].to == id && !out[0].published && !out[0].close,
        "{line:?} should produce one ordinary reply to client {id}, but produced {out:?}"
    );
    out[0].frame.clone()
}
#[allow(dead_code)]
fn say(core: &mut Core, id: ClientId, line: &str) -> Frame {
    say_at(core, id, 0, line)
}
#[allow(dead_code)]
fn session(core: &mut Core, id: ClientId, steps: &[(&str, Frame)]) {
    for (i, (line, want)) in steps.iter().enumerate() {
        check!(format!("client {id}, command {}: {line}", i + 1), say(core, id, line), want.clone());
    }
}
#[allow(dead_code)]
fn queued() -> Frame {
    simple("QUEUED")
}

#[allow(dead_code)]
struct Seeded(Rng);
impl Dice for Seeded {
    fn below(&mut self, n: usize) -> usize {
        self.0.below(n)
    }
}

#[test]
fn every_client_gets_its_own_id() {
    let mut core = Core::new();
    let a = core.connect();
    let b = core.connect();
    core.disconnect(a);
    let c = core.connect();
    check!(format!("ids {a}, {b}, {c}"), a != b && a != c && b != c, true);
    check!("a client that left gets nothing", core.request(a, args("PING"), 0), vec![]);
    check!("an unknown client gets nothing", core.request(999_999, args("PING"), 0), vec![]);
    check!("no arguments get nothing", core.request(b, vec![], 0), vec![]);
}

#[test]
fn control_commands_out_of_place() {
    let mut core = Core::new();
    let a = core.connect();
    session(&mut core, a, &[
        ("EXEC", err("ERR EXEC without MULTI")),
        ("DISCARD", err("ERR DISCARD without MULTI")),
        ("UNWATCH", ok()),
        ("MULTI", ok()),
        ("SET k v", queued()),
        ("MULTI", err("ERR MULTI calls can not be nested")),
        ("WATCH k", err("ERR WATCH inside MULTI is not allowed")),
        ("EXEC extra", err("ERR wrong number of arguments for 'exec' command")),
        ("WATCH", err("ERR wrong number of arguments for 'watch' command")),
        ("EXEC", Frame::Array(vec![ok()])),
        ("MULTI", ok()),
        ("EXEC", arr(&[])),
        ("WATCH", err("ERR wrong number of arguments for 'watch' command")),
        ("MULTI x", err("ERR wrong number of arguments for 'multi' command")),
        ("EXEC", err("ERR EXEC without MULTI")),
    ]);
}

#[test]
fn unknown_commands_spoil_the_transaction_and_it_ends_there() {
    let mut core = Core::new();
    let a = core.connect();
    say(&mut core, a, "MULTI");
    let reply = say(&mut core, a, "NOPE");
    check!(format!("{reply:?}"), is_error(&reply, "ERR unknown command"), true);
    session(&mut core, a, &[
        ("SET k v", queued()),
        ("EXEC", err("EXECABORT Transaction discarded because of previous errors.")),
        ("EXISTS k", int(0)),
        // the next transaction starts clean
        ("MULTI", ok()),
        ("PING", queued()),
        ("EXEC", Frame::Array(vec![simple("PONG")])),
    ]);
}

#[test]
fn discard_forgets_everything() {
    let mut core = Core::new();
    let (a, b) = (core.connect(), core.connect());
    session(&mut core, a, &[("WATCH k", ok()), ("MULTI", ok()), ("SET k v", queued()), ("NOPE", err("ERR unknown command")), ("DISCARD", ok()), ("GET k", Frame::Null)]);
    session(&mut core, b, &[("SET k other", ok())]);
    session(&mut core, a, &[("MULTI", ok()), ("GET k", queued()), ("EXEC", Frame::Array(vec![bulk("other")]))]);
}

/// Client `a` watches `w`; `b` runs `line` after `setup`. Does `a`'s EXEC run?
#[allow(dead_code)]
fn survives(setup: &[&str], line: &str) -> bool {
    let mut core = Core::new();
    let (a, b) = (core.connect(), core.connect());
    for command in setup {
        say(&mut core, b, command);
    }
    say(&mut core, a, "WATCH w");
    say(&mut core, b, line);
    say(&mut core, a, "MULTI");
    say(&mut core, a, "PING");
    match say(&mut core, a, "EXEC") {
        Frame::NullArray => false,
        reply => {
            assert_eq!(reply, Frame::Array(vec![simple("PONG")]), "after {setup:?} and {line}");
            true
        }
    }
}

#[test]
fn every_kind_of_change_to_a_watched_key_counts() {
    let cases: &[(&[&str], &str)] = &[
        (&[], "SET w 1"), (&["SET w 1"], "SET w 1"), (&["SET w 1"], "DEL w"), (&["SET w 1"], "APPEND w x"),
        (&["SET w 1"], "INCR w"), (&["SET w 1"], "MSET other 1 w 2"), (&["SET w 1"], "PEXPIRE w 1000"),
        (&["SET w 1"], "PEXPIRE w -1"), (&["SET w 1 PX 1000"], "PERSIST w"), (&["SET w 1"], "RENAME w z"),
        (&["SET z 1"], "RENAME z w"), (&["SET w 1"], "FLUSHALL"), (&[], "LPUSH w a"), (&["RPUSH w a b"], "LPOP w"),
        (&["RPUSH w a b"], "RPOP w 2"), (&[], "HSET w f v"), (&["HSET w f v"], "HSET w f v"), (&["HSET w f v g v"], "HDEL w f"),
        (&["HSET w f 1"], "HINCRBY w f 1"), (&[], "SADD w a"), (&["SADD w a b"], "SREM w a"), (&[], "ZADD w 1 a"),
        (&["ZADD w 1 a"], "ZADD w 2 a"), (&["ZADD w 1 a 2 b"], "ZREM w a"), (&["ZADD w 1 a"], "ZINCRBY w 1 a"),
    ];
    for (setup, line) in cases {
        check!(format!("{setup:?}, WATCH w, then {line}: does EXEC still run?"), survives(setup, line), false);
    }
}

#[test]
fn commands_that_change_nothing_do_not_count() {
    let cases: &[(&[&str], &str)] = &[
        (&[], "DEL w"), (&[], "GET w"), (&["SET w 1"], "GET w"), (&["SET w 1"], "SET w 2 NX"), (&[], "SET w 2 XX"),
        (&[], "LPOP w"), (&["RPUSH w a"], "LPOP w 0"), (&["RPUSH w a"], "LRANGE w 0 -1"), (&["SADD w a"], "SADD w a"),
        (&["SADD w a"], "SREM w b"), (&["HSET w f v"], "HDEL w g"), (&["ZADD w 1 a"], "ZADD w 1 a"),
        (&["ZADD w 1 a"], "ZREM w b"), (&["ZADD w 1 a"], "ZINCRBY w 0 a"), (&["SET w 1"], "PERSIST w"), (&[], "EXPIRE w 5"),
        (&["SET w text"], "INCR w"), (&["SET w text"], "LPUSH w a"), (&[], "RENAME w z"), (&["SET w 1"], "SET other 1"),
        (&["SET other 1"], "FLUSHALL"), (&[], "FLUSHALL"),
    ];
    for (setup, line) in cases {
        check!(format!("{setup:?}, WATCH w, then {line}: does EXEC still run?"), survives(setup, line), true);
    }
}

#[test]
fn a_clients_own_write_counts_too() {
    let mut core = Core::new();
    let a = core.connect();
    session(&mut core, a, &[("WATCH k", ok()), ("SET k v", ok()), ("MULTI", ok()), ("PING", queued()), ("EXEC", Frame::NullArray)]);
}

#[test]
fn writes_made_by_the_transaction_itself_do_not_abort_it() {
    let mut core = Core::new();
    let a = core.connect();
    session(&mut core, a, &[
        ("WATCH k", ok()), ("MULTI", ok()), ("SET k 1", queued()), ("INCR k", queued()),
        ("EXEC", Frame::Array(vec![ok(), int(2)])),
        // and nothing stays watched or dirty afterwards
        ("MULTI", ok()), ("INCR k", queued()), ("EXEC", Frame::Array(vec![int(3)])),
    ]);
}

#[test]
fn exec_and_unwatch_end_the_watch() {
    let mut core = Core::new();
    let (a, b) = (core.connect(), core.connect());
    session(&mut core, a, &[("WATCH k", ok()), ("MULTI", ok()), ("EXEC", arr(&[]))]);
    session(&mut core, b, &[("SET k 1", ok())]);
    session(&mut core, a, &[("MULTI", ok()), ("EXEC", arr(&[])), ("WATCH k other", ok())]);
    session(&mut core, b, &[("SET other 1", ok())]);
    // the change already happened, but UNWATCH forgets it
    session(&mut core, a, &[("UNWATCH", ok()), ("MULTI", ok()), ("EXEC", arr(&[]))]);
}

#[test]
fn an_aborted_exec_also_unwatches() {
    let mut core = Core::new();
    let (a, b) = (core.connect(), core.connect());
    session(&mut core, a, &[("WATCH k", ok()), ("MULTI", ok())]);
    say(&mut core, a, "NOPE");
    say(&mut core, a, "EXEC");
    session(&mut core, b, &[("SET k 1", ok())]);
    session(&mut core, a, &[("MULTI", ok()), ("EXEC", arr(&[]))]);
}

#[test]
fn unwatch_inside_a_transaction_is_queued_and_changes_nothing() {
    let mut core = Core::new();
    let (a, b) = (core.connect(), core.connect());
    session(&mut core, a, &[("WATCH k", ok()), ("MULTI", ok()), ("UNWATCH", queued()), ("PING", queued())]);
    session(&mut core, b, &[("SET k 1", ok())]);
    session(&mut core, a, &[("EXEC", Frame::NullArray), ("MULTI", ok()), ("UNWATCH", queued()), ("EXEC", Frame::Array(vec![ok()]))]);
}

#[test]
fn a_watched_key_that_expires_counts_even_if_nobody_touches_it() {
    let mut core = Core::new();
    let a = core.connect();
    say(&mut core, a, "SET k v PX 100");
    say(&mut core, a, "WATCH k");
    say(&mut core, a, "MULTI");
    say(&mut core, a, "PING");
    check!("EXEC at 100, after the deadline", say_at(&mut core, a, 100, "EXEC"), Frame::NullArray);
}

#[test]
fn a_key_that_had_already_expired_when_watched_does_not_count() {
    let mut core = Core::new();
    let a = core.connect();
    say(&mut core, a, "SET k v PX 100");
    say_at(&mut core, a, 150, "WATCH k");
    say_at(&mut core, a, 150, "MULTI");
    say_at(&mut core, a, 150, "PING");
    check!("EXEC", say_at(&mut core, a, 150, "EXEC"), Frame::Array(vec![simple("PONG")]));
}

#[test]
fn a_cycle_that_removes_a_watched_key_counts() {
    let mut core = Core::new();
    let a = core.connect();
    say(&mut core, a, "SET k v PX 100");
    say(&mut core, a, "WATCH k");
    check!("the cycle", core.cycle(100, &mut Seeded(Rng::new(1))), 1);
    // even a new key of that name does not undo it
    say_at(&mut core, a, 101, "SET unrelated 1");
    say_at(&mut core, a, 101, "MULTI");
    check!("EXEC", say_at(&mut core, a, 101, "EXEC"), Frame::NullArray);
}

#[test]
fn queued_commands_run_at_the_time_of_exec() {
    let mut core = Core::new();
    let a = core.connect();
    say(&mut core, a, "SET k v PX 100");
    say(&mut core, a, "MULTI");
    say(&mut core, a, "GET k");
    say(&mut core, a, "SET t v PX 10");
    say(&mut core, a, "PTTL t");
    check!("EXEC at 500", say_at(&mut core, a, 500, "EXEC"), Frame::Array(vec![Frame::Null, ok(), int(10)]));
}

#[test]
fn quit_inside_a_transaction() {
    let mut core = Core::new();
    let (a, b) = (core.connect(), core.connect());
    session(&mut core, a, &[("MULTI", ok()), ("SET k v", queued())]);
    let out = core.request(a, args("QUIT"), 0);
    check!("QUIT is not queued", out, vec![Delivery { to: a, frame: ok(), published: false, close: true }]);
    session(&mut core, b, &[("EXISTS k", int(0))]);
}

#[test]
fn many_clients_transfer_money_without_losing_any() {
    let mut rng = Rng::new(0x3208);
    for run_number in 0..60 {
        let mut core = Core::new();
        let boss = core.connect();
        let accounts = 3;
        for i in 0..accounts {
            say(&mut core, boss, &format!("SET acct{i} 100"));
        }
        // each client: WATCH both, read both, MULTI, write both, EXEC. Steps of different clients interleave.
        struct Transfer {
            id: ClientId,
            from: usize,
            to: usize,
            step: usize,
            balances: [i64; 2],
        }
        let mut transfers: Vec<Transfer> = (0..4)
            .map(|_| {
                let from = rng.below(accounts);
                let to = (from + 1 + rng.below(accounts - 1)) % accounts;
                Transfer { id: core.connect(), from, to, step: 0, balances: [0, 0] }
            })
            .collect();
        let mut done = 0;
        let mut history = Vec::new();
        for _ in 0..2000 {
            if done == 25 {
                break;
            }
            let i = rng.below(transfers.len());
            let t = &mut transfers[i];
            let line = match t.step {
                0 => format!("WATCH acct{} acct{}", t.from, t.to),
                1 => format!("GET acct{}", t.from),
                2 => format!("GET acct{}", t.to),
                3 => "MULTI".to_string(),
                4 => format!("SET acct{} {}", t.from, t.balances[0] - 7),
                5 => format!("SET acct{} {}", t.to, t.balances[1] + 7),
                _ => "EXEC".to_string(),
            };
            history.push(format!("{}: {line}", t.id));
            let reply = say(&mut core, t.id, &line);
            if t.step == 1 || t.step == 2 {
                let Frame::Bulk(b) = &reply else { panic!("GET gave {reply:?}") };
                t.balances[t.step - 1] = String::from_utf8_lossy(b).parse().expect("a number");
            }
            if t.step == 6 {
                done += usize::from(reply != Frame::NullArray);
                t.step = 0;
                t.from = rng.below(accounts);
                t.to = (t.from + 1 + rng.below(accounts - 1)) % accounts;
            } else {
                t.step += 1;
            }
        }
        let total: i64 = (0..accounts)
            .map(|i| match say(&mut core, boss, &format!("GET acct{i}")) {
                Frame::Bulk(b) => String::from_utf8_lossy(&b).parse().unwrap_or(0),
                _ => 0,
            })
            .sum();
        check!(format!("run {run_number}: total money after {history:?}"), total, 300);
        check!(format!("run {run_number}: some transfers went through"), done > 0, true);
    }
}

#[test]
fn many_watchers_and_many_writes_stay_fast() {
    let mut core = Core::new();
    let writer = core.connect();
    let watchers: Vec<ClientId> = (0..20_000).map(|_| core.connect()).collect();
    for (i, &id) in watchers.iter().enumerate() {
        say(&mut core, id, &format!("WATCH k{i}"));
    }
    // a write that looks at every watcher cannot finish in time
    for i in 0..100_000 {
        say(&mut core, writer, &format!("SET free{} v", i % 50));
    }
    for (i, &id) in watchers.iter().enumerate() {
        if i % 2 == 0 {
            say(&mut core, writer, &format!("SET k{i} v"));
        }
        say(&mut core, id, "MULTI");
        let reply = say(&mut core, id, "EXEC");
        check!(format!("watcher {i}"), reply == Frame::NullArray, i % 2 == 0);
    }
}
