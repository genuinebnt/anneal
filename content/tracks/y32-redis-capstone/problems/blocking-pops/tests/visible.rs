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

/// `[key, value]`, the reply of a blocking pop.
#[allow(dead_code)]
fn popped(key: &str, value: &str) -> Frame {
    arr(&[key, value])
}

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
