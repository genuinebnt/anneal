from y32_parts.common import *

SOLUTION = solution(7)

HELPERS = CORE + OUTPUTS + r'''
/// A confirmation: `[kind, name, count]`.
#[allow(dead_code)]
fn confirm(kind: &str, name: &str, count: i64) -> Frame {
    Frame::Array(vec![bulk(kind), bulk(name), int(count)])
}
#[allow(dead_code)]
fn nameless(kind: &str, count: i64) -> Frame {
    Frame::Array(vec![bulk(kind), Frame::Null, int(count)])
}
#[allow(dead_code)]
fn message(channel: &str, text: &str) -> Frame {
    arr(&["message", channel, text])
}
#[allow(dead_code)]
fn pmessage(pattern: &str, channel: &str, text: &str) -> Frame {
    arr(&["pmessage", pattern, channel, text])
}
#[allow(dead_code)]
fn not_allowed(name: &str) -> Frame {
    err(&format!(
        "ERR Can't execute '{name}': only (P)SUBSCRIBE / (P)UNSUBSCRIBE / PING / QUIT are allowed in this context"
    ))
}
'''

VISIBLE = [
    HELPERS,
    r'''
#[test]
fn subscribe_and_publish() {
    let mut core = Core::new();
    let [reader, writer] = clients(&mut core);
    let out = send(&mut core, reader, "SUBSCRIBE news sport");
    check!("one confirmation each", to(&out, reader), vec![confirm("subscribe", "news", 1), confirm("subscribe", "sport", 2)]);
    let out = send(&mut core, writer, "PUBLISH news hello");
    check!("the writer hears how many", to(&out, writer), vec![int(1)]);
    check!("the reader gets the message", to(&out, reader), vec![message("news", "hello")]);
    session(&mut core, writer, &[("PUBLISH weather rain", int(0))]);
}

#[test]
fn messages_are_marked_as_published() {
    let mut core = Core::new();
    let [reader, writer] = clients(&mut core);
    let confirmations = send(&mut core, reader, "SUBSCRIBE c");
    check!("a confirmation is a reply", confirmations.iter().any(|d| d.published), false);
    let out = send(&mut core, writer, "PUBLISH c m");
    let flags: Vec<(ClientId, bool)> = out.iter().map(|d| (d.to, d.published)).collect();
    check!("the message is published, the count is not", flags.contains(&(reader, true)) && flags.contains(&(writer, false)), true);
}

#[test]
fn patterns() {
    let mut core = Core::new();
    let [reader, writer] = clients(&mut core);
    send(&mut core, reader, "PSUBSCRIBE job.* *.done");
    send(&mut core, reader, "SUBSCRIBE job.done");
    let out = send(&mut core, writer, "PUBLISH job.done 7");
    check!("three frames", to(&out, writer), vec![int(3)]);
    check!("the channel first, then patterns in byte order", to(&out, reader), vec![
        message("job.done", "7"),
        pmessage("*.done", "job.done", "7"),
        pmessage("job.*", "job.done", "7"),
    ]);
}

#[test]
fn a_subscriber_may_do_little_else() {
    let mut core = Core::new();
    let a = core.connect();
    send(&mut core, a, "SUBSCRIBE c");
    session(&mut core, a, &[
        ("GET k", not_allowed("get")),
        ("PUBLISH c x", not_allowed("publish")),
        ("MULTI", not_allowed("multi")),
        ("PING", arr(&["pong", ""])),
        ("ping hi", arr(&["pong", "hi"])),
    ]);
    send(&mut core, a, "UNSUBSCRIBE c");
    session(&mut core, a, &[("GET k", Frame::Null), ("PING", simple("PONG"))]);
}

#[test]
fn unsubscribe_from_everything() {
    let mut core = Core::new();
    let a = core.connect();
    send(&mut core, a, "SUBSCRIBE b a");
    send(&mut core, a, "PSUBSCRIBE p*");
    check!("all channels, in byte order", to(&send(&mut core, a, "UNSUBSCRIBE"), a), vec![confirm("unsubscribe", "a", 2), confirm("unsubscribe", "b", 1)]);
    check!("none left to name", to(&send(&mut core, a, "UNSUBSCRIBE"), a), vec![nameless("unsubscribe", 1)]);
    check!("patterns", to(&send(&mut core, a, "PUNSUBSCRIBE"), a), vec![confirm("punsubscribe", "p*", 0)]);
}
''',
]

HIDDEN = [
    HELPERS,
    r'''
#[test]
fn arity() {
    let mut core = Core::new();
    let a = core.connect();
    for (line, name) in [("SUBSCRIBE", "subscribe"), ("PSUBSCRIBE", "psubscribe"), ("PUBLISH c", "publish"), ("PUBLISH c m extra", "publish")] {
        check!(line, say(&mut core, a, line), err(&format!("ERR wrong number of arguments for '{name}' command")));
    }
}

#[test]
fn repeats_and_strangers() {
    let mut core = Core::new();
    let a = core.connect();
    check!("a repeat counts once", to(&send(&mut core, a, "SUBSCRIBE c c"), a), vec![confirm("subscribe", "c", 1), confirm("subscribe", "c", 1)]);
    check!("a channel and a pattern of one name are two", to(&send(&mut core, a, "PSUBSCRIBE c"), a), vec![confirm("psubscribe", "c", 2)]);
    check!("unsubscribing a stranger still confirms", to(&send(&mut core, a, "UNSUBSCRIBE other c other"), a), vec![
        confirm("unsubscribe", "other", 2), confirm("unsubscribe", "c", 1), confirm("unsubscribe", "other", 1),
    ]);
    check!("nothing to unsubscribe", to(&send(&mut core, a, "UNSUBSCRIBE"), a), vec![nameless("unsubscribe", 1)]);
}

#[test]
fn a_client_that_never_subscribed_may_unsubscribe() {
    let mut core = Core::new();
    let a = core.connect();
    check!("UNSUBSCRIBE", to(&send(&mut core, a, "UNSUBSCRIBE"), a), vec![nameless("unsubscribe", 0)]);
    check!("PUNSUBSCRIBE", to(&send(&mut core, a, "PUNSUBSCRIBE"), a), vec![nameless("punsubscribe", 0)]);
    session(&mut core, a, &[("SET k v", ok())]);
}

#[test]
fn every_listener_hears_once_per_subscription() {
    let mut core = Core::new();
    let [a, b, c, writer] = clients(&mut core);
    send(&mut core, a, "SUBSCRIBE news");
    send(&mut core, b, "SUBSCRIBE news");
    send(&mut core, b, "PSUBSCRIBE n[ae]ws");
    send(&mut core, c, "PSUBSCRIBE n[ae]ws");
    send(&mut core, c, "SUBSCRIBE news.old");
    let out = send(&mut core, writer, "PUBLISH news x");
    check!("count", to(&out, writer), vec![int(4)]);
    check!("a", to(&out, a), vec![message("news", "x")]);
    check!("b", to(&out, b), vec![message("news", "x"), pmessage("n[ae]ws", "news", "x")]);
    check!("c", to(&out, c), vec![pmessage("n[ae]ws", "news", "x")]);
}

#[test]
fn leaving_ends_every_subscription() {
    let mut core = Core::new();
    let [a, b, writer] = clients(&mut core);
    send(&mut core, a, "SUBSCRIBE c d");
    send(&mut core, a, "PSUBSCRIBE *");
    send(&mut core, b, "SUBSCRIBE c");
    core.disconnect(a);
    let out = send(&mut core, writer, "PUBLISH c x");
    check!("only b", (to(&out, a), to(&out, b), to(&out, writer)), (vec![], vec![message("c", "x")], vec![int(1)]));
    session(&mut core, writer, &[("PUBLISH d x", int(0))]);
    send(&mut core, b, "QUIT");
    session(&mut core, writer, &[("PUBLISH c x", int(0))]);
}

#[test]
fn publish_inside_a_transaction_happens_at_exec() {
    let mut core = Core::new();
    let [reader, writer] = clients(&mut core);
    send(&mut core, reader, "SUBSCRIBE c");
    send(&mut core, writer, "MULTI");
    let out = send(&mut core, writer, "PUBLISH c 1");
    check!("queued, nothing sent", (to(&out, writer), to(&out, reader)), (vec![queued()], vec![]));
    send(&mut core, writer, "SET k v");
    send(&mut core, writer, "PUBLISH c 2");
    let out = send(&mut core, writer, "EXEC");
    check!("replies", to(&out, writer), vec![Frame::Array(vec![int(1), ok(), int(1)])]);
    check!("both messages", to(&out, reader), vec![message("c", "1"), message("c", "2")]);
    check!("marked", out.iter().filter(|d| d.to == reader).all(|d| d.published), true);
}

#[test]
fn a_watch_that_failed_publishes_nothing() {
    let mut core = Core::new();
    let [reader, writer, other] = clients(&mut core);
    send(&mut core, reader, "SUBSCRIBE c");
    send(&mut core, writer, "WATCH k");
    send(&mut core, other, "SET k v");
    send(&mut core, writer, "MULTI");
    send(&mut core, writer, "PUBLISH c 1");
    check!("EXEC", to(&send(&mut core, writer, "EXEC"), reader), vec![]);
}

#[test]
fn subscribing_inside_a_transaction_is_refused_and_spoils_it() {
    for name in ["SUBSCRIBE", "UNSUBSCRIBE", "PSUBSCRIBE", "PUNSUBSCRIBE"] {
        let mut core = Core::new();
        let a = core.connect();
        say(&mut core, a, "MULTI");
        check!(name, say(&mut core, a, &format!("{} c", name.to_lowercase())), err(&format!("ERR {name} is not allowed in transactions")));
        check!("EXEC", say(&mut core, a, "EXEC"), err("EXECABORT Transaction discarded because of previous errors."));
    }
}

#[test]
fn ping_in_a_transaction_is_ordinary() {
    let mut core = Core::new();
    let a = core.connect();
    session(&mut core, a, &[("MULTI", ok()), ("PING", queued()), ("EXEC", Frame::Array(vec![simple("PONG")]))]);
}

#[test]
fn unknown_commands_in_subscriber_mode() {
    let mut core = Core::new();
    let a = core.connect();
    send(&mut core, a, "PSUBSCRIBE x");
    for line in ["EXEC", "DISCARD", "WATCH k", "UNWATCH", "BLPOP q 0", "SET k v", "ECHO x", "DBSIZE", "GET", "ECHO a b c"] {
        let name = line.split(' ').next().unwrap().to_lowercase();
        check!(line, say(&mut core, a, line), not_allowed(&name));
    }
    check!("nobody blocked", core.blocked(), 0);
    let out = send(&mut core, a, "QUIT");
    check!("QUIT works", out, vec![Delivery { to: a, frame: ok(), published: false, close: true }]);
}

#[test]
fn a_woken_client_can_publish() {
    let mut core = Core::new();
    let [worker, reader, boss] = clients(&mut core);
    send(&mut core, reader, "SUBSCRIBE done");
    send(&mut core, worker, "BLPOP jobs 0");
    send(&mut core, worker, "PUBLISH done ok");
    let out = send(&mut core, boss, "RPUSH jobs j1");
    check!("worker", to(&out, worker), vec![arr(&["jobs", "j1"]), int(1)]);
    check!("reader", to(&out, reader), vec![message("done", "ok")]);
}

#[test]
fn a_woken_client_can_subscribe_and_then_hold_back() {
    let mut core = Core::new();
    let [a, boss] = clients(&mut core);
    send(&mut core, a, "BLPOP q 0");
    send(&mut core, a, "SUBSCRIBE c");
    send(&mut core, a, "GET k");
    let out = send(&mut core, boss, "RPUSH q v");
    check!("a", to(&out, a), vec![arr(&["q", "v"]), confirm("subscribe", "c", 1), not_allowed("get")]);
}

#[test]
fn binary_channels_and_payloads() {
    let mut core = Core::new();
    let [reader, writer] = clients(&mut core);
    let channel = vec![0u8, 13, 10, 255];
    core.request(reader, vec![b"SUBSCRIBE".to_vec(), channel.clone()], 0);
    let out = core.request(writer, vec![b"PUBLISH".to_vec(), channel.clone(), vec![0, 0]], 0);
    check!("payload", to(&out, reader), vec![Frame::Array(vec![bulk("message"), Frame::Bulk(channel), Frame::Bulk(vec![0, 0])])]);
}

#[test]
fn random_subscriptions_match_a_model() {
    use std::collections::BTreeSet;
    let mut rng = Rng::new(0x320a);
    let names = ["a", "b", "ab"];
    let patterns = ["a*", "?", "[ab]b", "*"];
    let fits = |p: &str, c: &str| match p {
        "a*" => c.starts_with('a'),
        "?" => c.len() == 1,
        "[ab]b" => c == "ab",
        _ => true,
    };
    for run_number in 0..150 {
        let mut core = Core::new();
        let ids: [ClientId; 3] = clients(&mut core);
        let publisher = core.connect();
        let mut subs: Vec<(BTreeSet<&str>, BTreeSet<&str>)> = vec![Default::default(); 3];
        let mut history = Vec::new();
        for _ in 0..40 {
            let i = rng.below(3);
            let is_pattern = rng.bool();
            let name = if is_pattern { *rng.pick(&patterns) } else { *rng.pick(&names) };
            if rng.below(3) < 2 {
                let add = rng.below(3) > 0;
                let verb = match (add, is_pattern) {
                    (true, false) => "SUBSCRIBE",
                    (true, true) => "PSUBSCRIBE",
                    (false, false) => "UNSUBSCRIBE",
                    (false, true) => "PUNSUBSCRIBE",
                };
                let set = if is_pattern { &mut subs[i].1 } else { &mut subs[i].0 };
                if add { set.insert(name); } else { set.remove(name); }
                let count = (subs[i].0.len() + subs[i].1.len()) as i64;
                history.push(format!("{i}: {verb} {name}"));
                let out = send(&mut core, ids[i], &format!("{verb} {name}"));
                check!(format!("run {run_number}: {history:?}"), to(&out, ids[i]), vec![confirm(&verb.to_lowercase(), name, count)]);
            } else {
                let channel = *rng.pick(&names);
                history.push(format!("PUBLISH {channel}"));
                let out = send(&mut core, publisher, &format!("PUBLISH {channel} m"));
                let mut total = 0;
                for (i, (channels, pats)) in subs.iter().enumerate() {
                    let mut want = Vec::new();
                    if channels.contains(channel) {
                        want.push(message(channel, "m"));
                    }
                    for p in pats.iter().filter(|p| fits(p, channel)) {
                        want.push(pmessage(p, channel, "m"));
                    }
                    total += want.len() as i64;
                    check!(format!("run {run_number}, client {i}: {history:?}"), to(&out, ids[i]), want);
                }
                check!(format!("run {run_number}: count after {history:?}"), to(&out, publisher), vec![int(total)]);
            }
        }
    }
}

#[test]
fn very_many_channels_stay_fast() {
    // a publish that looks at every client cannot finish in time
    let mut core = Core::new();
    let writer = core.connect();
    let n = 60_000;
    let readers: Vec<ClientId> = (0..n)
        .map(|i| {
            let id = core.connect();
            core.request(id, args(&format!("SUBSCRIBE c{i}")), 0);
            id
        })
        .collect();
    for (i, &id) in readers.iter().enumerate() {
        let out = send(&mut core, writer, &format!("PUBLISH c{i} x"));
        check!(format!("channel {i}"), out.iter().filter(|d| d.published).map(|d| d.to).collect::<Vec<_>>(), vec![id]);
    }
    for &id in readers.iter() {
        core.disconnect(id);
    }
    session(&mut core, writer, &[("PUBLISH c0 x", int(0))]);
}
''',
]

STATEMENT = r'''
**Capstone, step 7 of 10.** Clients can now talk to each other: some listen on named channels, and others publish to them. The starter holds the reference code of steps 1 to 6.

@SPOILERS@

**Subscriptions.** A client may subscribe to **channels** by name and to **patterns**, which are matched with `glob_matches`. Names are any bytes. Its **count** is the number of channels and patterns it is subscribed to.

**`SUBSCRIBE channel [channel ...]`** replies once per argument, in order: `["subscribe", channel, count]`, with the count after that one (an `Int`; the others are `Bulk`). Subscribing again changes nothing but is confirmed again.

**`UNSUBSCRIBE [channel ...]`** replies `["unsubscribe", channel, count]` for each argument, subscribed or not.

- With no arguments it leaves every channel, with one reply each, in byte order.
- With no arguments and no channels, it replies once: `["unsubscribe", Null, count]`.

**`PSUBSCRIBE`** and **`PUNSUBSCRIBE`** do the same for patterns, with the words `psubscribe` and `punsubscribe`.

**`PUBLISH channel message`** sends to every listener, and replies with the number of message frames sent.

- A subscriber of the channel gets `["message", channel, message]`.
- For **each** matching pattern it subscribed to, a client gets `["pmessage", pattern, channel, message]`. It may so receive one message several times.
- To one client: the `message` frame first, then the `pmessage` frames in byte order of their patterns.

These message frames have `published` set. Every other delivery does not. Each client receives its deliveries in the order the commands ran.

**Subscriber mode.** While a client's count is above 0, it may only send the four subscription commands, `PING` and `QUIT`. Anything else, even with the wrong number of arguments, replies:

`ERR Can't execute 'name': only (P)SUBSCRIBE / (P)UNSUBSCRIBE / PING / QUIT are allowed in this context`

with the name in lower case. `PING [message]` then replies the array `["pong", message]`, with an empty string if there is none.

**With the earlier steps:**

- `PUBLISH` may be queued, and is sent at `EXEC`.
- Any of the four subscription commands inside `MULTI` replies `ERR NAME is not allowed in transactions`, in upper case, and spoils the transaction.
- Held commands of a woken client may publish and subscribe.
- Leaving, by `disconnect` or `QUIT`, ends all subscriptions.

`SUBSCRIBE`, `PSUBSCRIBE` need at least one name; `PUBLISH` takes exactly two arguments. Tens of thousands of clients may be subscribed: `PUBLISH` must not look at clients that do not listen to that channel. It may try every pattern.
'''

PROBLEM = dict(
    slug="publish-and-subscribe", title="Capstone 7: publish and subscribe", level="hard", stage="many-clients",
    tags=["pub/sub", "fan-out", "connection modes", "patterns"],
    statement=STATEMENT.replace("@SPOILERS@", spoilers(7)), starter=starter(7), solution=SOLUTION, visible=VISIBLE, hidden=HIDDEN,
    teaches=[
        "Index listeners by channel and keep the index in step with each client's own list.",
        "A connection can be in a mode that changes which commands it may send.",
        "Every new feature has to be checked against transactions, blocking and leaving.",
    ],
    follow_up="1) Pattern matching tries every pattern on each publish. How could you index patterns by their literal prefix? 2) Pub/sub here delivers at most once. What would at-least-once need? 3) Why does subscriber mode exist in RESP2, and what does RESP3 change?",
    hints=[
        ("approach", "Keep two views: for each client what it listens to, and for each channel or pattern who listens. Every change updates both, and leaving walks the client's own lists."),
        ("rust", "`BTreeMap<Vec<u8>, BTreeSet<ClientId>>` keeps patterns in byte order, which gives the delivery order for free. Remove a name's entry when its set empties."),
        ("edge", "Check the subscriber gate before arity and before the transaction queue. `UNSUBSCRIBE` with no arguments must collect the names before it changes the set."),
    ],
    notes=("Two indexes, by client and by name, are updated together. Publishing looks up the channel and then tries each pattern, in byte order, which also fixes each client's frame order.",
           "O(listeners + patterns) per publish", "O(subscriptions)"),
    wrong={
        "count_ignores_patterns": broken(SOLUTION, (
            "        self.channels.len() + self.patterns.len()\n    }", "        self.channels.len().max(self.patterns.len())\n    }")),
        "one_frame_per_client": broken(SOLUTION, (
            "        (out.len() - start) as i64", "        let mut seen = BTreeSet::new();\n        let mut i = start;\n        while i < out.len() {\n            if seen.insert(out[i].to) {\n                i += 1;\n            } else {\n                out.remove(i);\n            }\n        }\n        (out.len() - start) as i64")),
        "disconnect_leaves_patterns": broken(SOLUTION, (
            "                (channels, &mut self.channels),\n                (patterns, &mut self.patterns),\n            ] {", "                (channels, &mut self.channels),\n            ] {\n                let _ = &patterns;")),
        "no_subscriber_gate_for_unknown_arity": broken(SOLUTION, (
            "        if subscriber && !allowed {", "        if subscriber && !allowed && refusal.is_none() {")),
        "subscribe_is_queued": broken(SOLUTION, (
            "                name.ends_with(\"SUBSCRIBE\").then(|| error(&not_here))", "                name.ends_with(\"NEVER\").then(|| error(&not_here))")),
        "publish_walks_every_client": broken(SOLUTION, (
            "        for &to in self.channels.get(channel).into_iter().flatten() {\n            out.push(message(to, &[b\"message\", channel, payload]));\n        }",
            "        let mut ids: Vec<&ClientId> = self.clients.keys().collect();\n        ids.sort();\n        for &to in ids {\n            if self.clients[&to].channels.contains(channel) {\n                out.push(message(to, &[b\"message\", channel, payload]));\n            }\n        }")),
    },
)
