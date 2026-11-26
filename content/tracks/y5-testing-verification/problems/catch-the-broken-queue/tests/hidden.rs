use solution::*;

use std::collections::VecDeque;

#[allow(dead_code)]
fn rejects<Q: BoundedQueue>(make: impl Fn(usize) -> Q) -> bool {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| check(make))).is_err()
}

/// A queue with one switchable bug.
#[derive(Clone, Copy, PartialEq)]
enum Bug {
    None,
    RefusesTheWrongValue,
    HoldsOneFewer,
    CapacityOneHoldsTwo,
    MergesEqualNeighbours,
    ZeroLooksEmpty,
    MaxIsRefused,
    LenForgetsAPopAfterFull,
    IsEmptyAfterAnyPop,
    LosesOrderAfterManyPops,
    FailedPushStillCounts,
    PeekStaleAfterDrain,
    WrapsWrongWhenFullTwice,
    BigCapacityIsCut,
}

struct Faulty {
    bug: Bug,
    cap: usize,
    items: VecDeque<u32>,
    pops: usize,
    popped_last: bool,
    ghost: usize,
    times_full: usize,
    last_front: Option<u32>,
}

#[allow(dead_code)]
fn faulty(bug: Bug) -> impl Fn(usize) -> Faulty {
    move |cap| Faulty { bug, cap, items: VecDeque::new(), pops: 0, popped_last: false, ghost: 0, times_full: 0, last_front: None }
}

impl BoundedQueue for Faulty {
    fn push(&mut self, v: u32) -> Result<(), u32> {
        self.popped_last = false;
        let limit = match self.bug {
            Bug::HoldsOneFewer if self.cap > 1 => self.cap - 1,
            Bug::CapacityOneHoldsTwo if self.cap == 1 => 2,
            Bug::BigCapacityIsCut => self.cap.min(4),
            _ => self.cap,
        };
        if self.items.len() >= limit {
            if self.bug == Bug::FailedPushStillCounts {
                self.ghost += 1;
            }
            return Err(if self.bug == Bug::RefusesTheWrongValue { *self.items.back().unwrap() } else { v });
        }
        if self.bug == Bug::MaxIsRefused && v == u32::MAX {
            return Err(v);
        }
        if self.bug == Bug::MergesEqualNeighbours && self.items.back() == Some(&v) {
            return Ok(());
        }
        self.items.push_back(v);
        if self.items.len() == self.cap {
            self.times_full += 1;
            if self.bug == Bug::WrapsWrongWhenFullTwice && self.times_full == 3 && self.cap > 2 {
                let n = self.items.len();
                self.items.swap(n - 1, n - 2);
            }
        }
        Ok(())
    }

    fn pop(&mut self) -> Option<u32> {
        let was_full = self.items.len() == self.cap;
        let v = self.items.pop_front();
        if v.is_some() {
            self.pops += 1;
            self.popped_last = true;
            self.last_front = v;
            if self.bug == Bug::LenForgetsAPopAfterFull && was_full && self.cap >= 3 {
                self.ghost += 1;
            }
            if self.bug == Bug::LosesOrderAfterManyPops && self.pops == 100 && self.items.len() > 1 {
                self.items.rotate_left(1);
            }
        }
        match v {
            Some(0) if self.bug == Bug::ZeroLooksEmpty => None,
            other => other,
        }
    }

    fn peek(&self) -> Option<u32> {
        match self.items.front() {
            None if self.bug == Bug::PeekStaleAfterDrain => self.last_front,
            other => other.copied(),
        }
    }

    fn len(&self) -> usize {
        self.items.len() + self.ghost
    }

    fn is_empty(&self) -> bool {
        if self.bug == Bug::IsEmptyAfterAnyPop && self.popped_last {
            return true;
        }
        self.len() == 0
    }
}

#[test]
fn accepts_the_same_queue_without_a_bug() {
    check!("check(the correct version) panics", rejects(faulty(Bug::None)), false);
}

#[test]
fn rejects_wrong_value_handed_back() {
    check!("a full push returns some other value", rejects(faulty(Bug::RefusesTheWrongValue)), true);
}

#[test]
fn rejects_one_slot_short() {
    check!("holds cap - 1 items", rejects(faulty(Bug::HoldsOneFewer)), true);
}

#[test]
fn rejects_capacity_one_bug() {
    check!("only capacity 1 is wrong", rejects(faulty(Bug::CapacityOneHoldsTwo)), true);
}

#[test]
fn rejects_lost_duplicates() {
    check!("an equal value pushed twice is stored once", rejects(faulty(Bug::MergesEqualNeighbours)), true);
}

#[test]
fn rejects_zero_as_a_sentinel() {
    check!("popping 0 reports empty", rejects(faulty(Bug::ZeroLooksEmpty)), true);
}

#[test]
fn rejects_max_as_a_sentinel() {
    check!("u32::MAX cannot be stored", rejects(faulty(Bug::MaxIsRefused)), true);
}

#[test]
fn rejects_stale_len() {
    check!("len drifts after popping from a full queue", rejects(faulty(Bug::LenForgetsAPopAfterFull)), true);
}

#[test]
fn rejects_wrong_is_empty() {
    check!("is_empty is true right after any pop", rejects(faulty(Bug::IsEmptyAfterAnyPop)), true);
}

#[test]
fn rejects_a_bug_after_a_hundred_pops() {
    check!("order breaks at the 100th pop of one queue", rejects(faulty(Bug::LosesOrderAfterManyPops)), true);
}

#[test]
fn rejects_counting_refused_pushes() {
    check!("a refused push changes len", rejects(faulty(Bug::FailedPushStillCounts)), true);
}

#[test]
fn rejects_stale_peek_when_empty() {
    check!("peek on a drained queue shows an old item", rejects(faulty(Bug::PeekStaleAfterDrain)), true);
}

#[test]
fn rejects_the_third_time_full() {
    check!("order breaks the third time the queue fills", rejects(faulty(Bug::WrapsWrongWhenFullTwice)), true);
}

#[test]
fn rejects_a_hidden_maximum() {
    check!("never holds more than 4, whatever the capacity", rejects(faulty(Bug::BigCapacityIsCut)), true);
}

#[test]
fn is_deterministic() {
    use std::cell::RefCell;
    struct Recorder<'a>(VecDeque<u32>, usize, &'a RefCell<Vec<(usize, u32)>>);
    impl BoundedQueue for Recorder<'_> {
        fn push(&mut self, v: u32) -> Result<(), u32> {
            self.2.borrow_mut().push((self.1, v));
            if self.0.len() == self.1 { return Err(v); }
            self.0.push_back(v);
            Ok(())
        }
        fn pop(&mut self) -> Option<u32> { self.0.pop_front() }
        fn peek(&self) -> Option<u32> { self.0.front().copied() }
        fn len(&self) -> usize { self.0.len() }
    }
    let (a, b) = (RefCell::new(Vec::new()), RefCell::new(Vec::new()));
    check(|c| Recorder(VecDeque::new(), c, &a));
    check(|c| Recorder(VecDeque::new(), c, &b));
    check!("two runs push the same values in the same order", *a.borrow() == *b.borrow(), true);
}
