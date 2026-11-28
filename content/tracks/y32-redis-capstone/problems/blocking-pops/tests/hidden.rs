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
fn milliseconds_come_from_the_digits_as_written() {
    // in f64, 4.03 * 1000.0 and 1.001 * 1000.0 are a little above 4030 and below 1001
    let mut core = Core::new();
    let [a, b, c] = clients(&mut core);
    core.request(a, args("BLPOP q 4.03"), 0);
    core.request(b, args("BLPOP q 403e-2"), 0);
    core.request(c, args("BLPOP q 1.001"), 0);
    check!("1.001 s is 1001 ms", core.tick(1000), vec![]);
    check!("and is up then", to(&core.tick(1001), c), vec![Frame::NullArray]);
    check!("4.03 s is not yet up", core.tick(4029), vec![]);
    let who: Vec<ClientId> = core.tick(4030).iter().map(|x| x.to).collect();
    check!("4.03 s and 403e-2 s are both 4030 ms", who, vec![a, b]);
}

#[test]
fn a_timeout_too_small_for_f64_is_one_millisecond() {
    // as an f64, 1e-400 is 0, which would mean forever
    let mut core = Core::new();
    let [a, b] = clients(&mut core);
    core.request(a, args("BLPOP q 1e-400"), 50);
    check!("not yet", core.tick(50), vec![]);
    check!("one millisecond later", to(&core.tick(51), a), vec![Frame::NullArray]);
    check!("a minus sign makes it negative", to(&core.request(b, args("BLPOP q -1e-400"), 60), b), vec![err("ERR timeout is negative")]);
    check!("all zeros wait for ever", core.request(b, args("BLPOP q -0.0e-400"), 60), vec![]);
    check!("one is waiting", core.blocked(), 1);
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
