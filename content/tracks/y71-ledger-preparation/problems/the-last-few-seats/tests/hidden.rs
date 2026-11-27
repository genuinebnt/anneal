use solution::*;

/// Something that happens just before a numbered call is answered.
#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq)]
enum Meanwhile {
    /// The other window sells these seats, if there are that many.
    Sells { id: u64, seats: u32 },
    /// Somebody hands seats back.
    HandsBack(u32),
    /// The reply is lost. A write does not happen.
    Loses,
    /// A write happens, and then the reply is lost.
    LosesAfterwards,
}

#[allow(dead_code)]
#[derive(Debug)]
struct Fake {
    view: View,
    /// (call number from 0, what happens just before it)
    script: Vec<(usize, Meanwhile)>,
    calls: Vec<&'static str>,
    /// How often our own sale id was written.
    ours: u64,
    written: u32,
    /// The true state at each `version` call: (version, seats left).
    asked: Vec<(u64, u32)>,
}

#[allow(dead_code)]
impl Fake {
    fn new(left: u32, script: &[(usize, Meanwhile)]) -> Fake {
        Fake {
            view: View { version: 0, left, sales: Default::default() },
            script: script.to_vec(),
            calls: Vec::new(),
            ours: 0,
            written: 0,
            asked: Vec::new(),
        }
    }

    /// Runs what is due, and says whether this reply is lost (and whether a write still happens).
    fn due(&mut self, name: &'static str) -> (bool, bool) {
        let n = self.calls.len();
        self.calls.push(name);
        assert!(n < 10_000, "far too many calls: is the loop bounded?");
        let mut lost = (false, false);
        let now: Vec<Meanwhile> = self.script.iter().filter(|(at, _)| *at == n).map(|(_, m)| *m).collect();
        for m in now {
            match m {
                Meanwhile::Sells { id, seats } => {
                    if seats <= self.view.left && !self.view.sales.contains_key(&id) {
                        self.view.left -= seats;
                        self.view.sales.insert(id, seats);
                        self.view.version += 1;
                    }
                }
                Meanwhile::HandsBack(seats) => {
                    self.view.left += seats;
                    self.view.version += 1;
                }
                Meanwhile::Loses => lost = (true, false),
                Meanwhile::LosesAfterwards => lost = (true, true),
            }
        }
        lost
    }
}

impl Register for Fake {
    fn read(&mut self) -> Result<View, Lost> {
        if self.due("read").0 {
            return Err(Lost);
        }
        Ok(self.view.clone())
    }
    fn version(&mut self) -> Result<u64, Lost> {
        let lost = self.due("version").0;
        self.asked.push((self.view.version, self.view.left));
        if lost {
            return Err(Lost);
        }
        Ok(self.view.version)
    }
    fn add(&mut self, expected: u64, sale: Sale) -> Result<(), AddError> {
        let (lost, happens) = self.due("add");
        if lost && !happens {
            return Err(AddError::Lost);
        }
        if expected != self.view.version {
            return Err(if lost { AddError::Lost } else { AddError::Moved });
        }
        assert!(sale.seats <= self.view.left, "sold more seats than there are");
        assert!(!self.view.sales.contains_key(&sale.id), "this sale is already in the register");
        self.view.left -= sale.seats;
        self.view.sales.insert(sale.id, sale.seats);
        self.view.version += 1;
        self.ours += 1;
        self.written += 1;
        if lost {
            return Err(AddError::Lost);
        }
        Ok(())
    }
}

#[allow(dead_code)]
const SALE: Sale = Sale { id: 7, seats: 3 };

#[test]
fn zero_attempts_make_no_call() {
    let mut r = Fake::new(10, &[]);
    check!("no attempts", sell(&mut r, SALE, 0), Err(SellError::GaveUp));
    check!("calls", r.calls, Vec::<&str>::new());
}

#[test]
fn it_gives_up_after_the_last_attempt() {
    let script: Vec<(usize, Meanwhile)> = (0..4u64).map(|i| (2 * i as usize + 1, Meanwhile::Sells { id: 100 + i, seats: 1 })).collect();
    let mut r = Fake::new(100, &script);
    check!("four conflicts, four attempts", sell(&mut r, SALE, 4), Err(SellError::GaveUp));
    check!("reads", r.calls.iter().filter(|c| **c == "read").count(), 4);
    check!("calls", r.calls.len(), 8);
}

#[test]
fn lost_reads_use_up_attempts() {
    let mut r = Fake::new(10, &[(0, Meanwhile::Loses), (1, Meanwhile::Loses)]);
    check!("two attempts, both lost", sell(&mut r, SALE, 2), Err(SellError::GaveUp));
    check!("calls", r.calls, vec!["read", "read"]);
}

#[test]
fn the_very_last_attempt_can_succeed() {
    let mut r = Fake::new(10, &[(0, Meanwhile::Loses), (1, Meanwhile::Loses)]);
    check!("third time", sell(&mut r, SALE, 3), Ok(Sold::Now));
}

#[test]
fn a_lost_version_reply_is_not_a_no() {
    let mut r = Fake::new(1, &[(1, Meanwhile::Loses)]);
    check!("answer after a second look", sell(&mut r, SALE, 5), Err(SellError::NotEnough { left: 1 }));
    check!("calls", r.calls, vec!["read", "version", "read", "version"]);
    let mut r = Fake::new(1, &[(1, Meanwhile::Loses)]);
    check!("only one attempt", sell(&mut r, SALE, 1), Err(SellError::GaveUp));
}

#[test]
fn a_lost_add_that_did_not_happen_is_sent_again() {
    let mut r = Fake::new(10, &[(1, Meanwhile::Loses)]);
    check!("sale", sell(&mut r, SALE, 5), Ok(Sold::Now));
    check!("written once", r.ours, 1);
}

#[test]
fn a_lost_add_on_the_last_attempt_is_unknown() {
    let mut r = Fake::new(10, &[(1, Meanwhile::LosesAfterwards)]);
    check!("no attempt left to look", sell(&mut r, SALE, 1), Err(SellError::GaveUp));
    check!("the phone asks again", sell(&mut r, SALE, 1), Ok(Sold::Already));
    check!("written once", r.ours, 1);
}

#[test]
fn exactly_enough_seats() {
    let mut r = Fake::new(3, &[]);
    check!("3 of 3", sell(&mut r, SALE, 1), Ok(Sold::Now));
}

#[test]
fn the_id_is_checked_before_the_seats() {
    let mut r = Fake::new(1, &[(0, Meanwhile::Sells { id: 7, seats: 1 })]);
    check!("theirs has our id and another size", sell(&mut r, SALE, 3), Err(SellError::IdReused));
    check!("no confirming call needed", r.calls, vec!["read"]);
}

#[test]
fn the_count_in_a_no_is_the_one_that_was_confirmed() {
    let mut r = Fake::new(1, &[(1, Meanwhile::HandsBack(1))]);
    check!("one more came back", sell(&mut r, SALE, 5), Err(SellError::NotEnough { left: 2 }));
}

#[test]
fn random_days() {
    let mut rng = anneal_prelude::Rng::new(7106);
    for _ in 0..3000 {
        let left = rng.below(8) as u32;
        let mut script = Vec::new();
        for _ in 0..rng.below(7) {
            let at = rng.below(10);
            let m = match rng.below(4) {
                0 => Meanwhile::Sells { id: 100 + rng.below(50) as u64, seats: 1 + rng.below(4) as u32 },
                1 => Meanwhile::HandsBack(1 + rng.below(4) as u32),
                2 => Meanwhile::Loses,
                _ => Meanwhile::LosesAfterwards,
            };
            script.push((at, m));
        }
        let attempts = rng.below(5) as u32;
        let sale = Sale { id: 7, seats: 1 + rng.below(5) as u32 };
        let input = format!("left {left}, sale {sale:?}, attempts {attempts}, script {script:?}");
        let mut r = Fake::new(left, &script);
        let got = sell(&mut r, sale, attempts);
        let reads = r.calls.iter().filter(|c| **c == "read").count();
        check!(format!("at most {attempts} reads: {input}"), reads <= attempts as usize, true);
        check!(format!("written at most once: {input}"), r.ours <= 1, true);
        match got {
            Ok(_) => check!(input.clone(), r.view.sales.get(&7).copied(), Some(sale.seats)),
            Err(SellError::NotEnough { left }) => {
                check!(format!("our sale is not there: {input}"), r.view.sales.contains_key(&7), false);
                let confirmed = r.asked.last().copied();
                check!(format!("the count was true when confirmed: {input}"), confirmed.map(|c| c.1), Some(left));
                check!(format!("and it was too few: {input}"), left < sale.seats, true);
                check!(format!("nothing is asked after the answer: {input}"), r.calls.last(), Some(&"version"));
            }
            Err(SellError::IdReused) => check!(input.clone(), true, false),
            Err(SellError::GaveUp) => check!(format!("gave up early: {input}"), reads, attempts as usize),
        }
        if got == Ok(Sold::Now) {
            check!(input.clone(), r.written, 1);
        }
    }
}
