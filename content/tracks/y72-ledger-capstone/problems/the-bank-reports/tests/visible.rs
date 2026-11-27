use solution::*;

const DAY: u64 = 86_400;

#[allow(dead_code)]
fn add(store: &mut MemStore, stream: &str, ts: u64, event: Event) {
    let at = store.version(stream);
    let new = NewEvent::current(CmdId::Client(at), 0, ts, &event);
    store.append(stream, at, vec![new]).unwrap();
}

#[allow(dead_code)]
fn dep(amount: i64) -> Event {
    Event::Deposited { amount }
}
#[allow(dead_code)]
fn wd(amount: i64) -> Event {
    Event::Withdrawn { amount, reason: "r".into() }
}
#[allow(dead_code)]
fn out(amount: i64) -> Event {
    Event::TransferredOut { transfer: 1, to: "z".into(), amount }
}
#[allow(dead_code)]
fn inn(amount: i64) -> Event {
    Event::TransferredIn { transfer: 1, from: "z".into(), amount }
}
#[allow(dead_code)]
fn rev(delta: i64) -> Event {
    Event::Reversed { target: 2, delta }
}

#[allow(dead_code)]
fn day(deposited: i128, withdrawn: i128, transferred: i128, reversed: i128, events: u64) -> Day {
    Day { deposited, withdrawn, transferred, reversed, events }
}

#[allow(dead_code)]
fn sample() -> MemStore {
    let mut s = MemStore::new();
    add(&mut s, "ann", 0, Event::Opened);
    add(&mut s, "ann", 10, dep(100));
    add(&mut s, "bo", 20, Event::Opened);
    add(&mut s, "ann", DAY - 1, wd(30));
    add(&mut s, "ann", DAY, out(20));
    add(&mut s, "bo", DAY + 5, inn(20));
    add(&mut s, "ann", 2 * DAY, rev(30));
    s
}

#[test]
fn balances_and_days() {
    let s = sample();
    let mut r = Reports::default();
    r.catch_up(&s);
    check!("balances", (r.balance("ann"), r.balance("bo"), r.balance("nobody")), (80, 20, 0));
    check!("day 0", r.day(0), day(100, 30, 0, 0, 4));
    check!("day 1", r.day(1), day(0, 0, 20, 0, 2));
    check!("next", r.next_pos(), 7);
}

#[test]
fn a_reversal_counts_on_its_own_day() {
    let mut r = Reports::default();
    r.catch_up(&sample());
    check!("day 2", r.day(2), day(0, 0, 0, 30, 1));
    check!("day 0 is unchanged history", r.day(0).withdrawn, 30);
}

#[test]
fn spending() {
    let mut r = Reports::default();
    r.catch_up(&sample());
    // ann: withdrew 30 (undone later) and sent 20; bo spent nothing
    check!("top", r.top_spenders(5), vec![("ann".to_string(), 20), ("bo".to_string(), 0)]);
    check!("top 1", r.top_spenders(1).len(), 1);
}

#[test]
fn following_equals_starting_again() {
    let mut s = sample();
    let mut r = Reports::default();
    r.catch_up(&s);
    add(&mut s, "bo", 3 * DAY, wd(5));
    r.catch_up(&s);
    r.catch_up(&s);
    check!("rebuilt", r, Reports::from_log(s.read_all(0)));
}

#[test]
fn as_it_was() {
    let s = sample();
    let r = Reports::as_of(&s, 4);
    check!("ann before the transfer", r.balance("ann"), 70);
    check!("next", r.next_pos(), 4);
    check!("past the end", Reports::as_of(&s, 999), Reports::from_log(s.read_all(0)));
}
