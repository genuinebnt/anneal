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
