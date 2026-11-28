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
fn nothing_yet() {
    let r = Reports::default();
    check!("empty", (r.next_pos(), r.top_spenders(3).len(), r.day(0)), (0, 0, Day::default()));
    check!("before the first line", Reports::as_of(&sample(), 0), r);
}

#[test]
fn taking_back_a_deposit_is_not_spending() {
    let mut s = MemStore::new();
    add(&mut s, "a", 0, Event::Opened);
    add(&mut s, "a", 0, dep(10));
    add(&mut s, "a", 5 * DAY, rev(-10));
    let r = Reports::from_log(s.read_all(0));
    check!("spent", r.top_spenders(1), vec![("a".to_string(), 0)]);
    check!("its day", r.day(5), day(0, 0, 0, -10, 1));
    check!("the old day", r.day(0), day(10, 0, 0, 0, 2));
}

#[test]
fn receiving_is_not_spending_or_depositing() {
    let mut s = MemStore::new();
    add(&mut s, "a", 0, inn(7));
    let r = Reports::from_log(s.read_all(0));
    check!("day", r.day(0), day(0, 0, 0, 0, 1));
    check!("balance and spent", (r.balance("a"), r.top_spenders(1)), (7, vec![("a".to_string(), 0)]));
}

#[test]
fn ties_are_ordered_by_name() {
    let mut s = MemStore::new();
    for name in ["m", "a", "z", "b"] {
        add(&mut s, name, 0, wd(5));
    }
    add(&mut s, "z", 0, wd(1));
    let r = Reports::from_log(s.read_all(0));
    let names: Vec<String> = r.top_spenders(3).into_iter().map(|p| p.0).collect();
    check!("top 3", names, vec!["z", "a", "b"]);
}

#[test]
fn old_lines_count_like_new_ones() {
    let mut s = MemStore::new();
    let payload = serde_json::json!({"type":"Withdrawn","data":{"amt":4}});
    let old = NewEvent { cmd: CmdId::Client(1), print: 0, ts: 0, schema: 1, payload };
    s.append("a", 0, vec![old]).unwrap();
    let r = Reports::from_log(s.read_all(0));
    check!("withdrawn", r.day(0).withdrawn, 4);
}

#[test]
fn totals_do_not_overflow() {
    let mut s = MemStore::new();
    for name in ["a", "b", "c"] {
        add(&mut s, name, u64::MAX, dep(i64::MAX));
        add(&mut s, name, u64::MAX, wd(i64::MAX));
        add(&mut s, name, u64::MAX, dep(i64::MAX));
        add(&mut s, name, u64::MAX, wd(i64::MAX));
    }
    let r = Reports::from_log(s.read_all(0));
    let m = i128::from(i64::MAX);
    check!("day", r.day(u64::MAX / DAY), day(6 * m, 6 * m, 0, 0, 12));
    check!("spent", r.top_spenders(1), vec![("a".to_string(), 2 * m)]);
}

#[test]
fn random_histories() {
    let mut rng = anneal_prelude::Rng::new(7205);
    for _ in 0..200 {
        let mut s = MemStore::new();
        let mut plain: Vec<(String, u64, Event)> = Vec::new();
        let mut live = Reports::default();
        for _ in 0..rng.below(40) {
            let stream = rng.pick(&["a", "b", "c"]).to_string();
            let amount = 1 + rng.below(50) as i64;
            let event = match rng.below(6) {
                0 => dep(amount),
                1 => wd(amount),
                2 => out(amount),
                3 => inn(amount),
                4 => rev(if rng.bool() { amount } else { -amount }),
                _ => Event::Opened,
            };
            let ts = rng.below(4) as u64 * DAY + rng.below(100) as u64;
            add(&mut s, &stream, ts, event.clone());
            plain.push((stream, ts / DAY, event));
            if rng.below(3) == 0 {
                live.catch_up(&s);
                check!(format!("{plain:?}"), live.clone(), Reports::as_of(&s, live.next_pos()));
            }
        }
        live.catch_up(&s);
        let input = format!("{plain:?}");
        check!(input.clone(), live.clone(), Reports::from_log(s.read_all(0)));
        for d in 0..4 {
            let mut want = Day::default();
            for (_, at, e) in plain.iter().filter(|p| p.1 == d) {
                let _ = at;
                want.events += 1;
                match e {
                    Event::Deposited { amount } => want.deposited += i128::from(*amount),
                    Event::Withdrawn { amount, .. } => want.withdrawn += i128::from(*amount),
                    Event::TransferredOut { amount, .. } => want.transferred += i128::from(*amount),
                    Event::Reversed { delta, .. } => want.reversed += i128::from(*delta),
                    _ => {}
                }
            }
            check!(input.clone(), live.day(d), want);
        }
        let mut spent: Vec<(String, i128)> = s
            .streams()
            .into_iter()
            .map(|name| {
                let total: i128 = plain
                    .iter()
                    .filter(|p| p.0 == name)
                    .map(|p| match &p.2 {
                        Event::Withdrawn { amount, .. } | Event::TransferredOut { amount, .. } => i128::from(*amount),
                        Event::Reversed { delta, .. } if *delta > 0 => -i128::from(*delta),
                        _ => 0,
                    })
                    .sum();
                (name, total)
            })
            .collect();
        spent.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
        check!(input, live.top_spenders(10), spent);
    }
}

#[test]
fn catching_up_one_line_at_a_time_stays_quick() {
    let mut s = MemStore::new();
    let mut r = Reports::default();
    for i in 0..30_000u64 {
        add(&mut s, &format!("acct-{}", i % 3000), i, dep(1));
        r.catch_up(&s);
    }
    check!("one account", r.balance("acct-0"), 10);
}

#[test]
fn balances_do_not_stop_at_the_largest_i64() {
    let mut s = MemStore::new();
    add(&mut s, "up", 0, dep(i64::MAX));
    add(&mut s, "up", 0, inn(i64::MAX));
    add(&mut s, "up", 0, wd(i64::MAX));
    add(&mut s, "down", 0, wd(i64::MAX));
    add(&mut s, "down", 0, out(i64::MAX));
    add(&mut s, "down", 0, dep(i64::MAX));
    let r = Reports::from_log(s.read_all(0));
    let m = i128::from(i64::MAX);
    check!("up", r.balance("up"), m);
    check!("down", r.balance("down"), -m);
}
