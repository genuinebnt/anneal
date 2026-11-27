use solution::*;


use anneal_prelude::Rng;

#[allow(dead_code)]
struct Seeded(Rng);
impl Dice for Seeded {
    fn below(&mut self, n: usize) -> usize {
        self.0.below(n)
    }
}

/// Always answers `0`, or the last choice if asked for `High`.
#[allow(dead_code)]
struct First;
impl Dice for First {
    fn below(&mut self, _: usize) -> usize {
        0
    }
}
#[allow(dead_code)]
struct High;
impl Dice for High {
    fn below(&mut self, n: usize) -> usize {
        n - 1
    }
}

/// What a replay shows: the seats and the number of entries used.
#[allow(dead_code)]
fn seen((hall, used): (Hall, usize)) -> (Vec<Seat>, usize) {
    (hall.seats().to_vec(), used)
}

#[allow(dead_code)]
fn hold(who: &str, ms: u64) -> Command {
    Command::Hold { who: who.into(), ms }
}
#[allow(dead_code)]
fn buy(who: &str) -> Command {
    Command::Buy { who: who.into() }
}
#[allow(dead_code)]
fn release(who: &str) -> Command {
    Command::Release { who: who.into() }
}
#[allow(dead_code)]
fn held(seat: usize, who: &str, until: u64) -> Entry {
    Entry::Change(Effect::Held { seat, who: who.into(), until })
}
#[allow(dead_code)]
fn sold(seat: usize) -> Entry {
    Entry::Change(Effect::Sold { seat })
}
#[allow(dead_code)]
fn freed(seat: usize) -> Entry {
    Entry::Change(Effect::Freed { seat })
}

#[test]
fn the_rules_of_each_command() {
    let mut hall = Hall::new(2);
    let r = |hall: &mut Hall, c: Command| hall.run(&[c], 0, &mut First).0[0];
    check!("ann holds 0", r(&mut hall, hold("ann", 100)), Reply::Done(0));
    check!("one seat each", r(&mut hall, hold("ann", 100)), Reply::Refused);
    check!("bob holds 1", r(&mut hall, hold("bob", 100)), Reply::Done(1));
    check!("full", r(&mut hall, hold("cat", 100)), Reply::Refused);
    check!("ann buys", r(&mut hall, buy("ann")), Reply::Done(0));
    check!("twice", r(&mut hall, buy("ann")), Reply::Refused);
    check!("a buyer still has a seat", r(&mut hall, hold("ann", 100)), Reply::Refused);
    check!("ann gives back a sold seat", r(&mut hall, release("ann")), Reply::Done(0));
    check!("and may hold again", r(&mut hall, hold("ann", 100)), Reply::Done(0));
}

#[test]
fn sold_seats_never_lapse() {
    let mut hall = Hall::new(1);
    hall.run(&[hold("ann", 10), buy("ann")], 0, &mut First);
    check!("much later", hall.run(&[], u64::MAX, &mut First), (vec![], vec![]));
}

#[test]
fn lapsed_holds_are_freed_in_seat_order_even_with_no_command() {
    let mut hall = Hall::new(3);
    hall.run(&[hold("a", 30), hold("b", 20), hold("c", 40)], 0, &mut First);
    check!("at 30", hall.run(&[], 30, &mut First), (vec![], vec![Entry::Begin, freed(0), freed(1), Entry::End]));
    check!("at 40, one change is not wrapped", hall.run(&[], 40, &mut First), (vec![], vec![freed(2)]));
}

#[test]
fn a_buy_of_a_lapsed_hold_is_refused_after_freeing_it() {
    let mut hall = Hall::new(1);
    hall.run(&[hold("a", 5)], 0, &mut First);
    check!("too late", hall.run(&[buy("a")], 5, &mut First), (vec![Reply::Refused], vec![freed(0)]));
}

#[test]
fn holds_made_in_this_run_do_not_lapse_until_the_next() {
    let mut hall = Hall::new(1);
    check!("a hold of 0 ms", hall.run(&[hold("a", 0), buy("a")], 7, &mut First).0, vec![Reply::Done(0), Reply::Done(0)]);
    let mut hall = Hall::new(1);
    hall.run(&[hold("a", 0)], 7, &mut First);
    check!("next run", hall.run(&[buy("a")], 7, &mut First), (vec![Reply::Refused], vec![freed(0)]));
}

#[test]
fn a_hold_that_never_ends() {
    let mut hall = Hall::new(1);
    let (_, log) = hall.run(&[hold("a", u64::MAX)], 5, &mut First);
    check!("no overflow", log, vec![held(0, "a", u64::MAX)]);
}

#[test]
fn the_dice_pick_among_the_free_seats_in_order() {
    /// Records each question and answers 1.
    struct Second(Vec<usize>);
    impl Dice for Second {
        fn below(&mut self, n: usize) -> usize {
            self.0.push(n);
            1.min(n - 1)
        }
    }
    let mut hall = Hall::new(4);
    let mut dice = Second(Vec::new());
    let (replies, _) = hall.run(&[hold("a", 9), hold("b", 9), hold("c", 9), hold("d", 9), hold("e", 9)], 0, &mut dice);
    check!("second free seat each time", replies, vec![Reply::Done(1), Reply::Done(2), Reply::Done(3), Reply::Done(0), Reply::Refused]);
    check!("one question per hold, over the free seats; none when full", dice.0, vec![4, 3, 2, 1]);
}

#[test]
fn damaged_logs() {
    let cases: Vec<(&str, Vec<Entry>, usize)> = vec![
        ("a seat that does not exist", vec![held(2, "a", 1)], 0),
        ("holding a taken seat", vec![held(0, "a", 1), held(0, "b", 1)], 1),
        ("two seats for one person", vec![held(0, "a", 1), held(1, "a", 1)], 1),
        ("selling a free seat", vec![sold(0)], 0),
        ("selling twice", vec![held(0, "a", 1), sold(0), sold(0)], 2),
        ("freeing a free seat", vec![freed(1)], 0),
        ("freeing far away", vec![freed(usize::MAX)], 0),
        ("an End alone", vec![held(0, "a", 1), Entry::End], 1),
        ("Begin inside a unit", vec![Entry::Begin, held(0, "a", 1), Entry::Begin], 2),
        ("a bad change in a whole unit", vec![Entry::Begin, held(0, "a", 1), sold(1), Entry::End], 2),
    ];
    for (what, log, at) in cases {
        check!(what, Hall::replay(2, &log).map(|_| ()), Err(BadLog { at }));
    }
}

#[test]
fn what_an_unfinished_unit_holds_is_not_judged() {
    let log = [held(0, "a", 1), Entry::Begin, sold(1), freed(99)];
    check!("ignored", Hall::replay(2, &log).map(|(_, used)| used), Ok(1));
}

#[test]
fn an_empty_unit_counts() {
    let log = [Entry::Begin, Entry::End];
    check!("both", Hall::replay(1, &log).map(seen), Ok((vec![Seat::Free], 2)));
}

#[allow(dead_code)]
fn random_command(rng: &mut Rng) -> Command {
    let who = format!("p{}", rng.below(6));
    match rng.below(4) {
        0 | 1 => Command::Hold { who, ms: rng.below(20) as u64 },
        2 => Command::Buy { who },
        _ => Command::Release { who },
    }
}

#[test]
fn every_cut_of_a_random_history_replays_to_the_last_whole_unit() {
    let mut rng = Rng::new(0x310a);
    for run in 0..300 {
        let size = rng.below(5);
        let mut hall = Hall::new(size);
        let mut log: Vec<Entry> = Vec::new();
        // the hall after each unit, with the log length then
        let mut states = vec![(0, hall.seats().to_vec())];
        let mut now = 0;
        let mut history = Vec::new();
        for _ in 0..30 {
            now += rng.below(10) as u64;
            let n = rng.below(4);
            let commands: Vec<Command> = (0..n).map(|_| random_command(&mut rng)).collect();
            history.push(format!("{now}: {commands:?}"));
            let (replies, entries) = hall.run(&commands, now, &mut Seeded(Rng::new(rng.next_u64())));
            check!(format!("run {run}: a reply per command, {history:?}"), replies.len(), commands.len());
            if !entries.is_empty() {
                log.extend(entries);
                states.push((log.len(), hall.seats().to_vec()));
            }
        }
        for cut in 0..=log.len() {
            let (end, want) = states.iter().rev().find(|(end, _)| *end <= cut).expect("the start");
            check!(
                format!("run {run}, size {size}: the first {cut} entries of {log:?}"),
                Hall::replay(size, &log[..cut]).map(seen),
                Ok((want.clone(), *end))
            );
        }
    }
}

#[test]
fn a_replayed_hall_carries_on() {
    let mut hall = Hall::new(2);
    let (_, log) = hall.run(&[hold("a", 10)], 0, &mut First);
    let (mut again, _) = Hall::replay(2, &log).expect("replay");
    check!("same behaviour", again.run(&[hold("a", 1), buy("a")], 3, &mut First), hall.run(&[hold("a", 1), buy("a")], 3, &mut First));
}
