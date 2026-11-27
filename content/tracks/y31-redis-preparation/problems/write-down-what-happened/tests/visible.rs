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
fn a_hold_is_logged_as_what_it_did() {
    let mut hall = Hall::new(3);
    check!("the dice choose among free seats", hall.run(&[hold("ann", 50)], 100, &mut High), (vec![Reply::Done(2)], vec![held(2, "ann", 150)]));
    check!("seat 2", hall.seats()[2].clone(), Seat::Held { who: "ann".into(), until: 150 });
}

#[test]
fn several_changes_form_one_unit() {
    let mut hall = Hall::new(3);
    let (replies, log) = hall.run(&[hold("ann", 50), buy("ann"), hold("ann", 5)], 0, &mut First);
    check!("replies", replies, vec![Reply::Done(0), Reply::Done(0), Reply::Refused]);
    check!("log", log, vec![Entry::Begin, held(0, "ann", 50), sold(0), Entry::End]);
}

#[test]
fn a_lapsed_hold_is_freed_first_and_logged() {
    let mut hall = Hall::new(1);
    hall.run(&[hold("ann", 10)], 0, &mut First);
    check!("bob at 10", hall.run(&[hold("bob", 10)], 10, &mut First), (vec![Reply::Done(0)], vec![Entry::Begin, freed(0), held(0, "bob", 20), Entry::End]));
}

#[test]
fn nothing_changed_nothing_logged() {
    let mut hall = Hall::new(1);
    check!("buy without a hold", hall.run(&[buy("x"), release("x")], 0, &mut First), (vec![Reply::Refused, Reply::Refused], vec![]));
}

#[test]
fn replay_rebuilds_the_hall() {
    let mut hall = Hall::new(4);
    let mut log = hall.run(&[hold("ann", 100), hold("bob", 5)], 0, &mut Seeded(Rng::new(1))).1;
    log.extend(hall.run(&[buy("ann")], 50, &mut First).1);
    check!("replay", Hall::replay(4, &log).map(seen), Ok(seen((hall, log.len()))));
}

#[test]
fn a_unit_without_its_end_does_not_count() {
    let log = [held(0, "a", 9), Entry::Begin, held(1, "b", 9), sold(1)];
    let (hall, used) = Hall::replay(2, &log).expect("a torn end is fine");
    check!("used", used, 1);
    check!("seat 1", hall.seats()[1].clone(), Seat::Free);
}
