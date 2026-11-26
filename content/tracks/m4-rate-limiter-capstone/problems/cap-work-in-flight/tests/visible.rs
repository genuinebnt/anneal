use solution::*;

#[test]
fn up_to_the_limit() {
    let gate = Gate::new(2);
    let a = gate.try_acquire();
    let b = gate.try_acquire();
    check!("(a, b, c)", (a.is_some(), b.is_some(), gate.try_acquire().is_some()), (true, true, false));
    drop(a);
    check!("a place came back", gate.try_acquire().is_some(), true);
}

#[test]
fn clones_share_one_gate() {
    let gate = Gate::new(1);
    let other = gate.clone();
    let _held = other.try_acquire().unwrap();
    check!("(in flight, more)", (gate.in_flight(), gate.try_acquire().is_some()), (1, false));
}

#[test]
fn lowering_the_limit_takes_nothing_back() {
    let gate = Gate::new(3);
    let mut held: Vec<Permit> = (0..3).map(|_| gate.try_acquire().unwrap()).collect();
    gate.set_limit(1);
    check!("(limit, in flight)", (gate.limit(), gate.in_flight()), (1, 3));
    held.truncate(1);
    check!("one held, limit one", gate.try_acquire().is_some(), false);
    held.clear();
    check!("now there is room", gate.try_acquire().is_some(), true);
}

#[test]
fn a_policy_of_the_callers_own() {
    let gate = Gate::new(4);
    let half = |in_flight: u32, limit: u32| in_flight < limit / 2;
    let a = gate.try_acquire_when(half);
    let b = gate.try_acquire_when(half);
    check!("third background job", gate.try_acquire_when(half).is_some(), false);
    check!("urgent work still passes", gate.try_acquire().is_some(), true);
    drop((a, b));
}

#[test]
fn the_peak_stays() {
    let gate = Gate::new(5);
    let held: Vec<Permit> = (0..3).map(|_| gate.try_acquire().unwrap()).collect();
    drop(held);
    check!("(in flight, peak)", (gate.in_flight(), gate.peak()), (0, 3));
}

#[test]
fn the_limit_drops_while_the_policy_is_thinking() {
    let gate = Gate::new(3);
    let got = gate.try_acquire_when(|_, _| {
        gate.set_limit(0);
        true
    });
    check!("(permit, in flight)", (got.is_some(), gate.in_flight()), (false, 0));
}
