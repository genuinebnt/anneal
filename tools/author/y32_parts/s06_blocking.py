from y32_parts.common import *

SOLUTION = solution(6)

HELPERS = CORE + OUTPUTS + r'''
/// `[key, value]`, the reply of a blocking pop.
#[allow(dead_code)]
fn popped(key: &str, value: &str) -> Frame {
    arr(&[key, value])
}
'''

VISIBLE = [
    HELPERS,
    r'''
#[test]
fn a_pop_that_can_answer_at_once() {
    let mut core = Core::new();
    let a = core.connect();
    session(&mut core, a, &[
        ("RPUSH q 1 2 3", int(3)),
        ("BLPOP empty q 0", popped("q", "1")),
        ("BRPOP q 5", popped("q", "3")),
        ("LRANGE q 0 -1", arr(&["2"])),
    ]);
    check!("nobody waits", core.blocked(), 0);
}

#[test]
fn a_push_wakes_a_blocked_client() {
    let mut core = Core::new();
    let [a, b] = clients(&mut core);
    check!("BLPOP on a missing key says nothing yet", send(&mut core, a, "BLPOP q 0"), vec![]);
    check!("one client waits", core.blocked(), 1);
    let out = send(&mut core, b, "RPUSH q job");
    check!("the pusher", to(&out, b), vec![int(1)]);
    check!("the waiter", to(&out, a), vec![popped("q", "job")]);
    check!("nobody waits", core.blocked(), 0);
    session(&mut core, b, &[("EXISTS q", int(0))]);
}

#[test]
fn the_longest_waiting_client_is_served_first() {
    let mut core = Core::new();
    let [a, b, c, d] = clients(&mut core);
    send(&mut core, c, "BLPOP q 0");
    send(&mut core, a, "BLPOP q 0");
    send(&mut core, b, "BLPOP q 0");
    let out = send(&mut core, d, "RPUSH q 1 2");
    check!("c came first", to(&out, c), vec![popped("q", "1")]);
    check!("then a", to(&out, a), vec![popped("q", "2")]);
    check!("b still waits", (to(&out, b), core.blocked()), (vec![], 1));
}

#[test]
fn a_timeout() {
    let mut core = Core::new();
    let a = core.connect();
    send(&mut core, a, "BLPOP q 1.5");
    check!("too early", core.tick(1499), vec![]);
    check!("the time is up", to(&core.tick(1500), a), vec![Frame::NullArray]);
    check!("only once", core.tick(9999), vec![]);
    check!("nobody waits", core.blocked(), 0);
}

#[test]
fn commands_sent_while_blocked_wait_their_turn() {
    let mut core = Core::new();
    let [a, b] = clients(&mut core);
    send(&mut core, a, "BLPOP q 0");
    check!("held", send(&mut core, a, "SET done yes"), vec![]);
    check!("held too", send(&mut core, a, "GET done"), vec![]);
    session(&mut core, b, &[("EXISTS done", int(0))]);
    let out = send(&mut core, b, "LPUSH q x");
    check!("all three replies, in order", to(&out, a), vec![popped("q", "x"), ok(), bulk("yes")]);
}

#[test]
fn inside_a_transaction_nothing_blocks() {
    let mut core = Core::new();
    let a = core.connect();
    session(&mut core, a, &[
        ("RPUSH q 1", int(1)),
        ("MULTI", ok()),
        ("BLPOP q 0", queued()),
        ("BLPOP q 0", queued()),
        ("EXEC", Frame::Array(vec![popped("q", "1"), Frame::NullArray])),
    ]);
}
''',
]

HIDDEN = [
    HELPERS,
    r'''
#[test]
fn bad_arguments() {
    let mut core = Core::new();
    let a = core.connect();
    session(&mut core, a, &[
        ("BLPOP q", err("ERR wrong number of arguments for 'blpop' command")),
        ("BRPOP 0", err("ERR wrong number of arguments for 'brpop' command")),
        ("BLPOP q soon", err("ERR timeout is not a float or out of range")),
        ("BLPOP q nan", err("ERR timeout is not a float or out of range")),
        ("BLPOP q inf", err("ERR timeout is not a float or out of range")),
        ("BLPOP q -0.1", err("ERR timeout is negative")),
        ("SET text v", ok()),
        ("BLPOP text 0", err(WRONGTYPE)),
        ("BLPOP missing text 0", err(WRONGTYPE)),
        // a bad timeout is refused even when there is something to pop
        ("RPUSH full x", int(1)),
        ("BLPOP full -1", err("ERR timeout is negative")),
        // keys after the one that answers are not looked at
        ("BLPOP full text 0", popped("full", "x")),
        ("MULTI", ok()),
        ("BLPOP text 0", queued()),
        ("EXEC", Frame::Array(vec![err(WRONGTYPE)])),
    ]);
    check!("nobody waits", core.blocked(), 0);
}

#[test]
fn which_end_is_popped_follows_the_command_that_waited() {
    let mut core = Core::new();
    let [a, b, c] = clients(&mut core);
    send(&mut core, a, "BRPOP q 0");
    send(&mut core, b, "BLPOP q 0");
    let out = send(&mut core, c, "RPUSH q 1 2 3");
    check!("BRPOP takes from the right", to(&out, a), vec![popped("q", "3")]);
    check!("BLPOP takes from the left", to(&out, b), vec![popped("q", "1")]);
    session(&mut core, c, &[("LRANGE q 0 -1", arr(&["2"]))]);
}

#[test]
fn a_woken_client_leaves_every_queue() {
    let mut core = Core::new();
    let [a, b, c] = clients(&mut core);
    send(&mut core, a, "BLPOP x y z 0");
    send(&mut core, b, "BLPOP y 0");
    check!("a takes from y first", to(&send(&mut core, c, "RPUSH y 1"), a), vec![popped("y", "1")]);
    let out = send(&mut core, c, "RPUSH x 2");
    check!("a no longer waits for x", (to(&out, a), to(&out, c)), (vec![], vec![int(1)]));
    check!("b is next on y", to(&send(&mut core, c, "RPUSH y 3"), b), vec![popped("y", "3")]);
}

#[test]
fn a_key_named_twice() {
    let mut core = Core::new();
    let [a, b] = clients(&mut core);
    send(&mut core, a, "BLPOP q q 0");
    check!("one pop", to(&send(&mut core, b, "RPUSH q 1 2"), a), vec![popped("q", "1")]);
    session(&mut core, b, &[("LLEN q", int(1))]);
}

#[test]
fn a_waiter_is_served_before_the_pushers_next_command() {
    let mut core = Core::new();
    let [a, b] = clients(&mut core);
    send(&mut core, a, "BLPOP q 0");
    send(&mut core, b, "LPUSH q only");
    session(&mut core, b, &[("LPOP q", Frame::Null)]);
}

#[test]
fn a_transaction_serves_waiters_only_when_it_is_over() {
    let mut core = Core::new();
    let [a, b] = clients(&mut core);
    send(&mut core, a, "BLPOP q 0");
    for line in ["MULTI", "RPUSH q gone", "DEL q", "EXEC"] {
        check!(format!("{line}: nothing for the waiter"), to(&send(&mut core, b, line), a), vec![]);
    }
    check!("still waiting", core.blocked(), 1);
    send(&mut core, b, "MULTI");
    send(&mut core, b, "RPUSH q 1 2");
    send(&mut core, b, "LPOP q");
    let out = send(&mut core, b, "EXEC");
    check!("the transaction popped first", to(&out, b), vec![Frame::Array(vec![int(2), bulk("1")])]);
    check!("then the waiter got what was left", to(&out, a), vec![popped("q", "2")]);
}

#[test]
fn keys_pushed_in_one_transaction_are_served_in_the_order_they_were_pushed() {
    let mut core = Core::new();
    let [a, b] = clients(&mut core);
    send(&mut core, a, "BLPOP m n 0");
    send(&mut core, b, "MULTI");
    send(&mut core, b, "RPUSH n first");
    send(&mut core, b, "RPUSH m second");
    check!("n changed first", to(&send(&mut core, b, "EXEC"), a), vec![popped("n", "first")]);
}

#[test]
fn any_way_a_list_appears_wakes_a_waiter() {
    let mut core = Core::new();
    let [a, b] = clients(&mut core);
    send(&mut core, b, "RPUSH source v");
    send(&mut core, a, "BLPOP target 0");
    check!("RENAME", to(&send(&mut core, b, "RENAME source target"), a), vec![popped("target", "v")]);
}

#[test]
fn a_key_of_another_kind_wakes_nobody() {
    let mut core = Core::new();
    let [a, b] = clients(&mut core);
    send(&mut core, a, "BLPOP q 0");
    for line in ["SET q text", "DEL q", "SADD q member", "DEL q"] {
        check!(format!("{line}"), to(&send(&mut core, b, line), a), vec![]);
    }
    check!("still waiting", core.blocked(), 1);
    check!("a list at last", to(&send(&mut core, b, "RPUSH q x"), a), vec![popped("q", "x")]);
}

#[test]
fn a_held_blocking_pop_blocks_again() {
    let mut core = Core::new();
    let [a, b] = clients(&mut core);
    send(&mut core, a, "BLPOP q 0");
    send(&mut core, a, "BLPOP r 0");
    send(&mut core, a, "PING");
    let out = send(&mut core, b, "RPUSH q 1");
    check!("only the first pop is answered", to(&out, a), vec![popped("q", "1")]);
    check!("waiting again", core.blocked(), 1);
    check!("and still holding", send(&mut core, a, "ECHO later"), vec![]);
    let out = send(&mut core, b, "RPUSH r 2");
    check!("the rest, in order", to(&out, a), vec![popped("r", "2"), simple("PONG"), bulk("later")]);
}

#[test]
fn a_held_pop_that_can_answer_does_so() {
    let mut core = Core::new();
    let [a, b] = clients(&mut core);
    send(&mut core, a, "BLPOP q 0");
    send(&mut core, a, "BLPOP q 0");
    let out = send(&mut core, b, "RPUSH q 1 2");
    check!("both", to(&out, a), vec![popped("q", "1"), popped("q", "2")]);
}

#[test]
fn all_waiters_of_a_push_are_served_before_any_of_them_goes_on() {
    let mut core = Core::new();
    let [a, b, c] = clients(&mut core);
    send(&mut core, a, "BLPOP q 0");
    send(&mut core, a, "LPOP q");
    send(&mut core, b, "BLPOP q 0");
    let out = send(&mut core, c, "RPUSH q 1 2");
    check!("b gets the second value", to(&out, b), vec![popped("q", "2")]);
    check!("a's own LPOP finds nothing", to(&out, a), vec![popped("q", "1"), Frame::Null]);
}

#[test]
fn what_a_woken_client_does_can_wake_another() {
    let mut core = Core::new();
    let [a, b, c, d] = clients(&mut core);
    send(&mut core, a, "BLPOP step1 0");
    send(&mut core, a, "RPUSH step2 from-a");
    send(&mut core, b, "BLPOP step2 0");
    send(&mut core, b, "RPUSH step3 from-b");
    send(&mut core, c, "BLPOP step3 0");
    let out = send(&mut core, d, "RPUSH step1 go");
    check!("a", to(&out, a), vec![popped("step1", "go"), int(1)]);
    check!("b", to(&out, b), vec![popped("step2", "from-a"), int(1)]);
    check!("c", to(&out, c), vec![popped("step3", "from-b")]);
}

#[test]
fn quit_while_held() {
    let mut core = Core::new();
    let [a, b] = clients(&mut core);
    send(&mut core, a, "BLPOP q 0");
    send(&mut core, a, "QUIT");
    send(&mut core, a, "SET after v");
    let out = send(&mut core, b, "RPUSH q 1");
    let mine: Vec<(Frame, bool)> = out.iter().filter(|d| d.to == a).map(|d| (d.frame.clone(), d.close)).collect();
    check!("the pop, then QUIT", mine, vec![(popped("q", "1"), false), (ok(), true)]);
    session(&mut core, b, &[("EXISTS after", int(0))]);
}

#[test]
fn a_client_that_leaves_takes_nothing_with_it() {
    let mut core = Core::new();
    let [a, b, c] = clients(&mut core);
    send(&mut core, a, "BLPOP q 5");
    send(&mut core, a, "SET held v");
    send(&mut core, b, "BLPOP q 0");
    core.disconnect(a);
    check!("one waiter left", core.blocked(), 1);
    let out = send(&mut core, c, "RPUSH q 1");
    check!("b is served", (to(&out, a), to(&out, b)), (vec![], vec![popped("q", "1")]));
    check!("no timeout for the one who left", core.tick(10_000), vec![]);
    session(&mut core, c, &[("EXISTS held", int(0))]);
}

#[test]
fn timeouts_fire_in_the_order_of_their_deadlines() {
    let mut core = Core::new();
    let [a, b, c, d] = clients(&mut core);
    send(&mut core, a, "BLPOP q 3");
    send(&mut core, b, "BLPOP q 1");
    send(&mut core, c, "BLPOP q 2");
    send(&mut core, d, "BLPOP q 1");
    let who: Vec<ClientId> = core.tick(2000).iter().map(|x| x.to).collect();
    check!("b and d at 1000 in the order they waited, then c", who, vec![b, d, c]);
    check!("a is left", core.blocked(), 1);
}

#[test]
fn timeouts_are_counted_from_the_call_and_round_up() {
    let mut core = Core::new();
    let a = core.connect();
    core.request(a, args("BLPOP q 0.0005"), 100);
    check!("not yet", core.tick(100), vec![]);
    check!("half a millisecond is one", to(&core.tick(101), a), vec![Frame::NullArray]);
}

#[test]
fn zero_and_huge_timeouts_never_end() {
    let mut core = Core::new();
    let [a, b, c] = clients(&mut core);
    core.request(a, args("BLPOP q 0"), 1_000_000);
    core.request(b, args("BLPOP q 1e300"), 1_000_000);
    core.request(c, args("BLPOP q 1e16"), u64::MAX - 5);
    check!("a very late tick", core.tick(u64::MAX - 1), vec![]);
    check!("all three wait", core.blocked(), 3);
}

#[test]
fn after_a_timeout_held_commands_run() {
    let mut core = Core::new();
    let [a, b] = clients(&mut core);
    send(&mut core, a, "BLPOP q 1");
    send(&mut core, a, "RPUSH other v");
    send(&mut core, b, "BLPOP other 0");
    let out = core.tick(1000);
    check!("a times out and goes on", to(&out, a), vec![Frame::NullArray, int(1)]);
    check!("which wakes b", to(&out, b), vec![popped("other", "v")]);
}

#[test]
fn a_served_pop_changes_a_watched_key() {
    let mut core = Core::new();
    let [a, b, c] = clients(&mut core);
    send(&mut core, a, "BLPOP q 0");
    send(&mut core, c, "WATCH q");
    send(&mut core, b, "MULTI");
    send(&mut core, b, "RPUSH q 1");
    send(&mut core, b, "EXEC");
    send(&mut core, c, "MULTI");
    check!("c's EXEC", to(&send(&mut core, c, "EXEC"), c), vec![Frame::NullArray]);
}

#[test]
fn an_expired_list_cannot_be_popped() {
    let mut core = Core::new();
    let a = core.connect();
    say(&mut core, a, "RPUSH q 1");
    say(&mut core, a, "PEXPIRE q 10");
    check!("BLPOP at 10 blocks", core.request(a, args("BLPOP q 0"), 10), vec![]);
}

#[test]
fn random_pushes_and_waits_hand_out_every_value_once_in_order() {
    let mut rng = Rng::new(0x3209);
    for run_number in 0..200 {
        let mut core = Core::new();
        let pusher = core.connect();
        let mut free: Vec<ClientId> = (0..5).map(|_| core.connect()).collect();
        // one key: waiters in order, values in order
        let (mut waiting, mut values) = (std::collections::VecDeque::new(), std::collections::VecDeque::new());
        let mut next = 0;
        let mut history = Vec::new();
        for _ in 0..60 {
            let (out, want): (Vec<Delivery>, Vec<(ClientId, Frame)>) = if rng.bool() && !free.is_empty() {
                let id = free.remove(rng.below(free.len()));
                history.push(format!("{id}: BLPOP q 0"));
                let want = match values.pop_front() {
                    Some(v) => {
                        free.push(id);
                        vec![(id, popped("q", &format!("{v}")))]
                    }
                    None => {
                        waiting.push_back(id);
                        vec![]
                    }
                };
                (send(&mut core, id, "BLPOP q 0"), want)
            } else {
                let n = rng.below(3) + 1;
                let mut line = "RPUSH q".to_string();
                for _ in 0..n {
                    line.push_str(&format!(" {next}"));
                    values.push_back(next);
                    next += 1;
                }
                history.push(line.clone());
                let mut want = vec![(pusher, int(values.len() as i64))];
                while !waiting.is_empty() && !values.is_empty() {
                    let (id, v) = (waiting.pop_front().unwrap(), values.pop_front().unwrap());
                    want.push((id, popped("q", &format!("{v}"))));
                    free.push(id);
                }
                (send(&mut core, pusher, &line), want)
            };
            let mut got: Vec<(ClientId, Frame)> = out.into_iter().map(|d| (d.to, d.frame)).collect();
            let mut want = want;
            got.sort_by_key(|g| g.0);
            want.sort_by_key(|w| w.0);
            check!(format!("run {run_number}: {history:?}"), got, want);
            check!(format!("run {run_number}: blocked after {history:?}"), core.blocked(), waiting.len());
        }
    }
}

#[test]
fn very_many_blocked_clients_stay_fast() {
    let mut core = Core::new();
    let pusher = core.connect();
    let n = 60_000;
    let waiters: Vec<ClientId> = (0..n).map(|_| core.connect()).collect();
    for (i, &id) in waiters.iter().enumerate() {
        core.request(id, args("BLPOP q other 0"), 0);
        if i % 2 == 1 {
            // a tick with nothing due must not look at everyone
            check!("nothing is due", core.tick(i as u64), vec![]);
        }
    }
    // half leave, from the back
    for &id in waiters.iter().skip(n / 2).rev() {
        core.disconnect(id);
    }
    for (i, &id) in waiters.iter().take(n / 2).enumerate() {
        let out = core.request(pusher, args("RPUSH q v"), 0);
        let woken: Vec<ClientId> = out.iter().map(|d| d.to).filter(|&to| to != pusher).collect();
        check!(format!("push {i}"), woken, vec![id]);
    }
}
''',
]

STATEMENT = r'''
**Capstone, step 6 of 10.** A worker wants the next job as soon as there is one, without asking again and again. So a pop can now **wait**. The starter holds the reference code of steps 1 to 5, and `Waiting`, which keeps track of who waits for what.

@SPOILERS@

**`BLPOP key [key ...] timeout`** and **`BRPOP`** pop from the left or right of the **first** key, in argument order, that holds a list. The reply is `[key, value]`. Keys are checked in order: another kind before a list is the wrong-kind error. Otherwise the client is **blocked**, and gets nothing yet.

- `timeout` is a number of seconds, possibly with a fraction; 0 means forever. Not a finite number: `ERR timeout is not a float or out of range`. Below 0: `ERR timeout is negative`. It is checked before the keys.
- The deadline is `now` plus the timeout in milliseconds, rounded up. A deadline too far for `u64` must not overflow or panic.
- Inside `EXEC` a blocking pop never waits: with nothing to pop it replies `NullArray`.

**Waking.** After a command, or after a whole `EXEC`, every key that **changed** and now holds a list can serve waiters. This includes a list that arrived by `RENAME`.

- Waiters are served one at a time, longest waiting first, while the list has elements. Each gets `[key, value]` from its own end.
- Changed keys are served in the order they changed.
- A waiter is served **before** the next command of whoever pushed.
- A served pop is a change like any other, so it dirties watches.

**Held commands.** While blocked, a client may send more commands. `request` keeps them, `QUIT` included, and returns nothing for them.

- When the client is answered, they run in order, as if sent then. One that blocks keeps the rest held.
- First serve every waiter that the changed keys allow. Then run the held commands of the woken clients, in the order the clients were woken.
- After each held command, serve whoever it enables, before the next one. Its effects may wake further clients.

All deliveries this causes are returned by the call that caused it, in `request` or `tick`. Each client's deliveries must be in order. The order **between** different clients is free.

**`tick(now)`** answers `NullArray` to every blocked client whose deadline is `now` or earlier. Deliveries for these come earliest deadline first, then longest waiting. Their held commands then run as above.

**`blocked()`** is the number of blocked clients. `disconnect` removes a blocked client and its held commands: it takes no element and gets no reply.

The new commands need at least two arguments after the name. There may be tens of thousands of blocked clients: no push, tick or disconnect may look through all of them.
'''

PROBLEM = dict(
    slug="blocking-pops", title="Capstone 6: blocking pops", level="hard", stage="many-clients",
    tags=["blocking pop", "wait queues", "FIFO fairness", "timeouts", "continuations"],
    statement=STATEMENT.replace("@SPOILERS@", spoilers(6)), starter=starter(6), solution=SOLUTION, visible=VISIBLE, hidden=HIDDEN,
    teaches=[
        "Blocking without blocking a thread: store what the client waits for and what it still wants to do.",
        "Order is part of the contract: who is served, and when held commands resume.",
        "Collect work in queues and drain them in a loop; do not recurse from one wake-up into the next.",
    ],
    follow_up="1) Add `BLMOVE source target`: what can now chain without any held command? 2) What does real Redis do when one push serves several clients, and how does its log record it? 3) A client's held commands are unbounded. What limit would you set, and who should enforce it?",
    hints=[
        ("approach", "Treat waking as work to do later: a queue of changed keys, and a queue of woken clients. After the command, loop: serve a changed key if there is one, otherwise run one held command of the first woken client."),
        ("rust", "`Waiting::wake(key)` already picks the right client and removes it from every queue. A blocked pop that is answered at once can run a plain `LPOP` through the engine. A `bool` as the ticket remembers which end."),
        ("edge", "Ask the engine whether the key holds a list before you wake anyone, or you take a waiter out for a string. A woken client that blocks again must stop running its held commands. `now + millis` can overflow."),
    ],
    notes=("A blocked client is a record in the waiting room plus a queue of its unrun commands. After each command, one loop first serves every changed key, then resumes woken clients one command at a time, so a chain of wake-ups never recurses and every push is served before anything else happens.",
           "O(log w) per wake-up, w waiters", "O(blocked clients + held commands)"),
    wrong={
        "newest_waiter_first": broken(SOLUTION, (
            "        let id = *self.queues.get(key)?.values().next()?;", "        let id = *self.queues.get(key)?.values().next_back()?;")),
        "held_commands_run_at_once": broken(SOLUTION, (
            "        if self.waiting.contains(id) {\n            if let Some(client) = self.clients.get_mut(&id) {\n                client.held.push_back(args);\n            }\n            return out;\n        }\n", "")),
        "only_pushes_wake": broken(SOLUTION, (
            "            self.ready.push_back(key);\n", "            let _ = key;\n"),
            ("    /// A command for the keyspace alone.\n    fn apply(&mut self, args: &[Vec<u8>], now: u64) -> Frame {\n        let frame = self.engine.execute(args, now);\n        self.note_changes();\n",
             "    /// A command for the keyspace alone.\n    fn apply(&mut self, args: &[Vec<u8>], now: u64) -> Frame {\n        let frame = self.engine.execute(args, now);\n        self.note_changes();\n        if upper(&args[0]).ends_with(\"PUSH\") {\n            self.ready.push_back(args[1].clone());\n        }\n")),
        "deadline_overflows": broken(SOLUTION, (
            "now.saturating_add((timeout * 1000.0).ceil() as u64)", "now + (timeout * 1000.0).ceil() as u64")),
        "timeout_rounds_down": broken(SOLUTION, ("(timeout * 1000.0).ceil() as u64", "(timeout * 1000.0) as u64")),
        "held_commands_run_as_each_is_woken": broken(SOLUTION, (
            "                    self.woken.push_back(id);\n                }\n                self.note_changes();",
            "                    self.woken.push_back(id);\n                    while let Some(args) = self.clients.get_mut(&id).filter(|_| !self.waiting.contains(id)).and_then(|c| c.held.pop_front()) {\n                        self.step(id, args, now, out);\n                    }\n                }\n                self.note_changes();")),
        "disconnect_forgets_the_waiting_room": broken(SOLUTION, (
            "        self.unwatch(id);\n        self.waiting.cancel(id);\n", "        self.unwatch(id);\n")),
    },
)
