from y32_parts.common import *

SOLUTION = solution(5)

VISIBLE = [
    CORE,
    r'''
#[test]
fn queue_then_run() {
    let mut core = Core::new();
    let a = core.connect();
    session(&mut core, a, &[
        ("MULTI", ok()),
        ("SET k 1", queued()),
        ("INCR k", queued()),
        ("GET k", queued()),
        ("EXEC", Frame::Array(vec![ok(), int(2), bulk("2")])),
        ("GET k", bulk("2")),
    ]);
}

#[test]
fn nobody_sees_a_queue_and_nobody_gets_in_between() {
    let mut core = Core::new();
    let (a, b) = (core.connect(), core.connect());
    session(&mut core, a, &[("MULTI", ok()), ("SET k from-a", queued())]);
    session(&mut core, b, &[("GET k", Frame::Null), ("SET k from-b", ok())]);
    session(&mut core, a, &[("EXEC", Frame::Array(vec![ok()]))]);
    session(&mut core, b, &[("GET k", bulk("from-a"))]);
}

#[test]
fn watch_stops_a_lost_update() {
    let mut core = Core::new();
    let (a, b) = (core.connect(), core.connect());
    session(&mut core, a, &[("SET n 10", ok()), ("WATCH n", ok()), ("GET n", bulk("10"))]);
    session(&mut core, b, &[("INCR n", int(11))]);
    session(&mut core, a, &[
        ("MULTI", ok()),
        ("SET n 100", queued()),
        ("EXEC", Frame::NullArray),
        ("GET n", bulk("11")),
    ]);
}

#[test]
fn a_command_that_cannot_be_queued_spoils_the_transaction() {
    let mut core = Core::new();
    let a = core.connect();
    session(&mut core, a, &[("MULTI", ok()), ("SET k v", queued()), ("GET", err("ERR wrong number of arguments for 'get' command"))]);
    session(&mut core, a, &[
        ("EXEC", err("EXECABORT Transaction discarded because of previous errors.")),
        ("GET k", Frame::Null),
        ("EXEC", err("ERR EXEC without MULTI")),
    ]);
}

#[test]
fn an_error_while_running_fails_alone() {
    let mut core = Core::new();
    let a = core.connect();
    session(&mut core, a, &[
        ("MULTI", ok()),
        ("SET k text", queued()),
        ("INCR k", queued()),
        ("SET other 1", queued()),
        ("EXEC", Frame::Array(vec![ok(), err(NOT_INT), ok()])),
        ("GET other", bulk("1")),
    ]);
}

#[test]
fn quit() {
    let mut core = Core::new();
    let a = core.connect();
    let out = core.request(a, args("QUIT"), 0);
    check!("QUIT", out, vec![Delivery { to: a, frame: ok(), published: false, close: true }]);
    check!("a client that quit is gone", core.request(a, args("PING"), 0), vec![]);
}
''',
]

HIDDEN = [
    CORE,
    DICE,
    r'''
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
''',
]

STATEMENT = r'''
**Capstone, step 5 of 10.** Many clients now share the keyspace, and some need several commands to happen as one. The starter holds the reference code of steps 1 to 4.

@SPOILERS@

Two things in the starter's engine are for you. `Engine::changed` lists, in order, every key that was **changed** since you last emptied it, expiry included. `check_command(args)` is the engine's refusal for an unknown name or a wrong number of arguments.

Write `Core`, the front door for all clients.

- `connect()` returns an id never used before. `disconnect(id)` forgets the client and everything about it.
- `request(id, args, now)` runs one command of that client. It returns what must be sent as `Delivery { to, frame, published, close }`. `published` and `close` are `false` unless said otherwise.

  For now there is exactly one delivery, to `id`. It returns nothing for an unknown client or empty `args`.
- `cycle(now, dice)` is `Engine::cycle`.

Command names ignore case. All commands of the earlier steps work as before. The new ones:

**`QUIT`** (any number of arguments) replies `OK` with `close` set, and disconnects the client. It does so even inside a transaction.

**`MULTI`** replies `OK`, and starts a queue. While it is open, every command except `MULTI`, `EXEC`, `DISCARD`, `WATCH` and `QUIT` is **queued** and replies `Simple("QUEUED")`. It runs only at `EXEC`, at the time of `EXEC`.

- If a command is unknown or has the wrong number of arguments, reply with that error at once. The transaction is then **spoiled**.
- A wrong number of arguments to `MULTI`, `EXEC`, `DISCARD` or `WATCH` is refused, and changes nothing about an open transaction.

**`EXEC`** ends the transaction and runs the queued commands in order, with nothing in between. It replies with the array of their replies; an empty queue gives an empty array. A command that fails while running puts its error in its place, and the others still run.

- A spoiled transaction runs nothing: `EXECABORT Transaction discarded because of previous errors.`
- A transaction whose watch failed (below) runs nothing, and replies `NullArray`. Spoiled is checked first.

**`DISCARD`** drops the queue and replies `OK`.

**`WATCH key [key ...]`** replies `OK`. If any watched key **changes** before `EXEC`, whoever changed it (the watcher included), that `EXEC` runs nothing.

- A command that changes nothing does not count. Examples:
    - a refused command, `DEL` of a missing key, or `SET` whose condition fails;
    - adding a member that is there, or removing one that is not;
    - a pop of zero, or `PERSIST` with no deadline;
    - a `ZADD` or `ZINCRBY` that leaves every score as it was.
- Writing the same string again with `SET` does count, as does `HSET`. `LTRIM` of an existing list may count either way.
- `FLUSHALL` changes every key that existed.
- Expiry counts, **even if nothing touched the key since**, when its deadline has passed by `EXEC` or a `cycle` removed it. A key already past its deadline when watched does not count for that.

`EXEC` and `DISCARD` end all of the client's watches, however `EXEC` turns out. So does **`UNWATCH`**, which replies `OK`. Inside a transaction, `UNWATCH` is queued, replies `OK` at `EXEC` and has no other effect.

**Errors:** `ERR MULTI calls can not be nested`, `ERR WATCH inside MULTI is not allowed`, `ERR EXEC without MULTI`, `ERR DISCARD without MULTI`. The first two leave the transaction as it was.

There may be many thousands of clients with watches. A write must not look at clients that do not watch that key.
'''

PROBLEM = dict(
    slug="transactions", title="Capstone 5: transactions", level="hard", stage="many-clients",
    tags=["transactions", "optimistic concurrency", "WATCH", "state machine", "sans-IO"],
    statement=STATEMENT.replace("@SPOILERS@", spoilers(5)), starter=starter(5), solution=SOLUTION, visible=VISIBLE, hidden=HIDDEN,
    teaches=[
        "A core that takes commands and returns deliveries has no sockets, so every interleaving of clients can be tested.",
        "Optimistic transactions do not lock: they watch, and give up when something moved.",
        "Index watchers by key, so a write pays only for those who care.",
    ],
    follow_up="1) Redis does not roll back a command that fails inside EXEC. What would rollback need? 2) How could a client retry a failed EXEC without starving under heavy contention? 3) Compare WATCH with the first-committer-wins rule of snapshot isolation.",
    hints=[
        ("approach", "Keep per client an optional queue, a spoiled flag, the watched keys and a stale flag. Keep a second map from key to watching clients. After every command, drain the engine's changed keys and mark their watchers."),
        ("rust", "`Option<Vec<Args>>` says whether a transaction is open. `std::mem::take` empties `changed` while you update clients. `check_command` knows the engine's commands, so give your own commands their arity first."),
        ("edge", "Before `EXEC` decides, make each watched key expire if it is due. Do the same when a key is watched, before you register the watch. Remove the watch before the queued commands run."),
    ],
    notes=("Each client's state is a small record, and a key-to-watchers index turns the engine's list of changed keys into stale flags. EXEC first expires watched keys that are due, then checks the flags, ends the watches and runs the queue without interruption, since the core runs one command at a time.",
           "O(watchers of the changed keys) per command", "O(clients + queued commands + watches)"),
    wrong={
        "exec_ignores_unnoticed_expiry": broken(SOLUTION, (
            "        // a watched key may have expired without anyone touching it\n        for key in &watched {\n            self.engine.expire_now(key, now);\n        }\n", "        let _ = watched;\n")),
        "spoiled_transaction_still_runs": broken(SOLUTION, (
            "        let frame = if aborted {", "        let frame = if aborted && commands.is_empty() {")),
        "watches_stay_after_exec": broken(SOLUTION, (
            "        let (aborted, stale) = (std::mem::take(&mut client.aborted), client.stale);\n        self.unwatch(id);\n",
            "        let (aborted, stale) = (std::mem::take(&mut client.aborted), client.stale);\n        client.stale = false;\n")),
        "write_commands_always_count": broken(SOLUTION, (
            "    /// A command for the keyspace alone.\n    fn apply(&mut self, args: &[Vec<u8>], now: u64) -> Frame {\n        let frame = self.engine.execute(args, now);\n",
            "    /// A command for the keyspace alone.\n    fn apply(&mut self, args: &[Vec<u8>], now: u64) -> Frame {\n        let frame = self.engine.execute(args, now);\n        if !matches!(upper(&args[0]).as_str(), \"GET\" | \"LRANGE\" | \"EXISTS\" | \"PING\") {\n            if let Some(key) = args.get(1) {\n                self.engine.changed.push(key.clone());\n            }\n        }\n")),
        "a_runtime_error_stops_exec": broken(SOLUTION, (
            "            let replies = commands.iter().map(|c| self.run_queued(id, c, now));\n            let replies = replies.collect();\n            Frame::Array(replies)",
            "            let mut replies = Vec::new();\n            for c in &commands {\n                replies.push(self.run_queued(id, c, now));\n                if matches!(replies.last(), Some(Frame::Error(_))) {\n                    break;\n                }\n            }\n            Frame::Array(replies)")),
        "cycle_does_not_mark_watchers": broken(SOLUTION, (
            "        let removed = self.engine.cycle(now, dice);\n        self.note_changes();\n", "        let removed = self.engine.cycle(now, dice);\n        self.engine.changed.clear();\n")),
    },
)
