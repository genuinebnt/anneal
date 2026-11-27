use solution::*;

use std::collections::VecDeque;

fn s(m: &str) -> String {
    m.to_string()
}

#[allow(dead_code)]
fn card(out: Option<&str>, held: Option<&str>, queue: &[&str]) -> Book {
    Book {
        out_to: out.map(s),
        held_for: held.map(s),
        queue: queue.iter().map(|m| s(m)).collect::<VecDeque<_>>(),
        retired: false,
        lends: 0,
    }
}

#[test]
fn reserving_what_is_already_yours() {
    let book = card(Some("ann"), None, &["bo"]);
    for m in ["ann", "bo"] {
        check!(m, decide(&book, &Command::Reserve { member: s(m) }), Err(Refusal::AlreadyYours));
    }
    let held = card(None, Some("ann"), &[]);
    check!("set aside for ann", decide(&held, &Command::Reserve { member: s("ann") }), Err(Refusal::AlreadyYours));
    check!("someone else queues", decide(&held, &Command::Reserve { member: s("bo") }), Ok(vec![Event::Queued { member: s("bo") }]));
}

#[test]
fn refusals_come_in_the_stated_order() {
    let mut book = card(Some("ann"), None, &[]);
    book.retired = true;
    check!("lend", decide(&book, &Command::Lend { member: s("bo") }), Err(Refusal::Retired));
    check!("reserve", decide(&book, &Command::Reserve { member: s("ann") }), Err(Refusal::Retired));
    check!("retire", decide(&book, &Command::Retire), Err(Refusal::Retired));
    let both = card(Some("ann"), Some("cy"), &[]);
    check!("out and held", decide(&both, &Command::Lend { member: s("bo") }), Err(Refusal::OnLoan));
}

#[test]
fn the_holder_borrows_and_the_hold_ends() {
    let book = card(None, Some("ann"), &["bo"]);
    let events = decide(&book, &Command::Lend { member: s("ann") });
    check!("events", events.clone(), Ok(vec![Event::Lent { member: s("ann") }]));
    let after = events.unwrap().iter().fold(book, evolve);
    check!("held for", after.held_for.clone(), None);
    check!("queue keeps bo", after.queue.len(), 1);
}

#[test]
fn leaving_the_queue_from_the_middle() {
    let book = card(Some("ann"), None, &["bo", "cy", "di"]);
    check!("cy cancels", decide(&book, &Command::Cancel { member: s("cy") }), Ok(vec![Event::LeftQueue { member: s("cy") }]));
    let after = evolve(book, &Event::LeftQueue { member: s("cy") });
    check!("queue", after, card(Some("ann"), None, &["bo", "di"]));
}

#[test]
fn a_return_with_nobody_waiting_is_one_event() {
    let book = card(Some("ann"), None, &[]);
    check!("return", decide(&book, &Command::Return { member: s("ann") }), Ok(vec![Event::Returned]));
}

#[test]
fn dropping_the_only_hold_is_one_event() {
    let book = card(None, Some("ann"), &[]);
    check!("cancel", decide(&book, &Command::Cancel { member: s("ann") }), Ok(vec![Event::HoldDropped]));
}

#[test]
fn applying_odd_histories_follows_the_table() {
    let doubled = replay(&[
        Event::Queued { member: s("bo") },
        Event::Queued { member: s("cy") },
        Event::Queued { member: s("bo") },
    ]);
    check!("left removes all", evolve(doubled.clone(), &Event::LeftQueue { member: s("bo") }), card(None, None, &["cy"]));
    check!("set aside removes all", evolve(doubled.clone(), &Event::SetAside { member: s("bo") }), card(None, Some("bo"), &["cy"]));
    let gone = evolve(evolve(doubled, &Event::SetAside { member: s("cy") }), &Event::Retired);
    check!("retired empties", (gone.held_for, gone.queue.len(), gone.retired), (None, 0, true));
    let mut full = Book { lends: u32::MAX, ..Book::default() };
    full = evolve(full, &Event::Lent { member: s("ann") });
    check!("lends stops at the top", full.lends, u32::MAX);
}

#[test]
fn nothing_panics_on_any_history() {
    let mut rng = anneal_prelude::Rng::new(7101);
    let names = ["a", "b", "c"];
    for _ in 0..300 {
        let len = rng.below(12);
        let mut events = Vec::new();
        for _ in 0..len {
            let member = s(rng.pick(&names));
            events.push(match rng.below(7) {
                0 => Event::Lent { member },
                1 => Event::Returned,
                2 => Event::Queued { member },
                3 => Event::LeftQueue { member },
                4 => Event::SetAside { member },
                5 => Event::HoldDropped,
                _ => Event::Retired,
            });
        }
        let book = replay(&events);
        let mut stepped = Book::default();
        for e in &events {
            stepped = evolve(stepped, e);
        }
        check!(format!("{events:?}"), book, stepped);
    }
}

#[test]
fn random_days_keep_the_desk_consistent_and_replayable() {
    let mut rng = anneal_prelude::Rng::new(7102);
    let names = ["a", "b", "c", "d"];
    for _ in 0..300 {
        let mut book = Book::default();
        let mut history = Vec::new();
        let mut asked = Vec::new();
        for _ in 0..30 {
            let member = s(rng.pick(&names));
            let command = match rng.below(20) {
                0..=5 => Command::Lend { member },
                6..=10 => Command::Return { member },
                11..=15 => Command::Reserve { member },
                16..=18 => Command::Cancel { member },
                _ => Command::Retire,
            };
            asked.push(command.clone());
            if let Ok(events) = decide(&book, &command) {
                assert!(!events.is_empty(), "an accepted command records something: {asked:?}");
                for e in events {
                    book = evolve(book, &e);
                    history.push(e);
                }
            }
            let input = format!("{asked:?}");
            check!(input.clone(), replay(&history), book.clone());
            check!(format!("never out and held: {input}"), book.out_to.is_some() && book.held_for.is_some(), false);
            if !book.queue.is_empty() {
                check!(format!("nobody queues for a free book: {input}"), book.out_to.is_some() || book.held_for.is_some(), true);
            }
            let mut seen: Vec<&String> = book.queue.iter().chain(&book.held_for).chain(&book.out_to).collect();
            let all = seen.len();
            seen.sort();
            seen.dedup();
            check!(format!("each member once: {input}"), seen.len(), all);
        }
    }
}
