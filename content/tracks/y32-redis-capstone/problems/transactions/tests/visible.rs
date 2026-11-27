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
