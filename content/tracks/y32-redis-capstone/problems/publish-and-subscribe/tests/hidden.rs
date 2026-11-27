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

/// The frames meant for one client, in order.
#[allow(dead_code)]
fn to(out: &[Delivery], id: ClientId) -> Vec<Frame> {
    out.iter().filter(|d| d.to == id).map(|d| d.frame.clone()).collect()
}
#[allow(dead_code)]
fn send(core: &mut Core, id: ClientId, line: &str) -> Vec<Delivery> {
    core.request(id, args(line), 0)
}
#[allow(dead_code)]
fn clients<const N: usize>(core: &mut Core) -> [ClientId; N] {
    std::array::from_fn(|_| core.connect())
}

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
