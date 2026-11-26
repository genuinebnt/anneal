use solution::*;

use std::collections::VecDeque;

/// A correct queue, built on a ring so that it is not a copy of the obvious model.
#[allow(dead_code)]
struct Ring {
    slots: Vec<Option<u32>>,
    head: usize,
    used: usize,
}

#[allow(dead_code)]
impl Ring {
    fn new(cap: usize) -> Self {
        Ring { slots: vec![None; cap], head: 0, used: 0 }
    }
}

impl BoundedQueue for Ring {
    fn push(&mut self, v: u32) -> Result<(), u32> {
        if self.used == self.slots.len() {
            return Err(v);
        }
        let i = (self.head + self.used) % self.slots.len();
        self.slots[i] = Some(v);
        self.used += 1;
        Ok(())
    }
    fn pop(&mut self) -> Option<u32> {
        if self.used == 0 {
            return None;
        }
        let v = self.slots[self.head].take();
        self.head = (self.head + 1) % self.slots.len();
        self.used -= 1;
        v
    }
    fn peek(&self) -> Option<u32> {
        if self.used == 0 { None } else { self.slots[self.head] }
    }
    fn len(&self) -> usize {
        self.used
    }
}

/// True when `check` panics.
#[allow(dead_code)]
fn rejects<Q: BoundedQueue>(make: impl Fn(usize) -> Q) -> bool {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| check(make))).is_err()
}

struct Lifo(Vec<u32>, usize);
impl BoundedQueue for Lifo {
    fn push(&mut self, v: u32) -> Result<(), u32> {
        if self.0.len() == self.1 { return Err(v); }
        self.0.push(v);
        Ok(())
    }
    fn pop(&mut self) -> Option<u32> { self.0.pop() }
    fn peek(&self) -> Option<u32> { self.0.last().copied() }
    fn len(&self) -> usize { self.0.len() }
}

struct OneTooMany(VecDeque<u32>, usize);
impl BoundedQueue for OneTooMany {
    fn push(&mut self, v: u32) -> Result<(), u32> {
        if self.0.len() > self.1 { return Err(v); }
        self.0.push_back(v);
        Ok(())
    }
    fn pop(&mut self) -> Option<u32> { self.0.pop_front() }
    fn peek(&self) -> Option<u32> { self.0.front().copied() }
    fn len(&self) -> usize { self.0.len() }
}

struct Overwrites(VecDeque<u32>, usize);
impl BoundedQueue for Overwrites {
    fn push(&mut self, v: u32) -> Result<(), u32> {
        if self.0.len() == self.1 { self.0.pop_front(); }
        self.0.push_back(v);
        Ok(())
    }
    fn pop(&mut self) -> Option<u32> { self.0.pop_front() }
    fn peek(&self) -> Option<u32> { self.0.front().copied() }
    fn len(&self) -> usize { self.0.len() }
}

struct PeeksNewest(VecDeque<u32>, usize);
impl BoundedQueue for PeeksNewest {
    fn push(&mut self, v: u32) -> Result<(), u32> {
        if self.0.len() == self.1 { return Err(v); }
        self.0.push_back(v);
        Ok(())
    }
    fn pop(&mut self) -> Option<u32> { self.0.pop_front() }
    fn peek(&self) -> Option<u32> { self.0.back().copied() }
    fn len(&self) -> usize { self.0.len() }
}

#[test]
fn accepts_a_correct_queue() {
    check!("check(a correct ring buffer) panics", rejects(Ring::new), false);
}

#[test]
fn rejects_lifo_order() {
    check!("a stack posing as a queue is rejected", rejects(|c| Lifo(Vec::new(), c)), true);
}

#[test]
fn rejects_one_item_too_many() {
    check!("accepts cap + 1 items", rejects(|c| OneTooMany(VecDeque::new(), c)), true);
}

#[test]
fn rejects_overwriting_when_full() {
    check!("drops the oldest item instead of refusing", rejects(|c| Overwrites(VecDeque::new(), c)), true);
}

#[test]
fn rejects_wrong_peek() {
    check!("peek shows the newest item", rejects(|c| PeeksNewest(VecDeque::new(), c)), true);
}
