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
fn a_quiet_day() {
    let mut r = Fake::new(10, &[]);
    check!("sell 3 of 10", sell(&mut r, SALE, 5), Ok(Sold::Now));
    check!("left", r.view.left, 7);
    check!("calls", r.calls, vec!["read", "add"]);
}

#[test]
fn the_other_window_writes_first() {
    let mut r = Fake::new(10, &[(1, Meanwhile::Sells { id: 1, seats: 2 })]);
    check!("they sell before our add", sell(&mut r, SALE, 5), Ok(Sold::Now));
    check!("left", r.view.left, 5);
    check!("calls", r.calls, vec!["read", "add", "read", "add"]);
}

#[test]
fn a_sure_no() {
    let mut r = Fake::new(2, &[]);
    check!("sell 3 of 2", sell(&mut r, SALE, 5), Err(SellError::NotEnough { left: 2 }));
    check!("calls", r.calls, vec!["read", "version"]);
}

#[test]
fn seats_come_back_while_we_say_no() {
    let mut r = Fake::new(2, &[(1, Meanwhile::HandsBack(4))]);
    check!("handed back before we answer", sell(&mut r, SALE, 5), Ok(Sold::Now));
    check!("calls", r.calls, vec!["read", "version", "read", "add"]);
}

#[test]
fn the_reply_to_a_sale_is_lost() {
    let mut r = Fake::new(3, &[(1, Meanwhile::LosesAfterwards)]);
    check!("written, reply lost", sell(&mut r, SALE, 5), Ok(Sold::Already));
    check!("written once", r.ours, 1);
}

#[test]
fn the_phone_sends_it_again() {
    let mut r = Fake::new(3, &[]);
    check!("first", sell(&mut r, SALE, 5), Ok(Sold::Now));
    check!("again, with no seats left", sell(&mut r, SALE, 5), Ok(Sold::Already));
    check!("same id, other number", sell(&mut r, Sale { id: 7, seats: 1 }, 5), Err(SellError::IdReused));
}
