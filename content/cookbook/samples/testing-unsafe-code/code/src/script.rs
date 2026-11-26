//! A byte string drives a `Shelf` and a model side by side, including hostile behaviour from "the caller". The same function
//! serves a seeded test (`tests/script.rs`), a Miri run and a coverage-guided fuzz target.
//!
//! Everything here is safe code. If it can make the shelf commit undefined behaviour, the shelf is unsound.

use crate::{NoRoom, Shelf};
use std::cell::RefCell;
use std::panic::{self, AssertUnwindSafe};
use std::rc::Rc;

pub const CAP: usize = 6;

/// Counts drops per element id. The oracle for leaks and double drops that also works without Miri.
#[derive(Default)]
pub struct Ledger(RefCell<Vec<u32>>);

/// An element that owns heap memory (so Miri notices a bad read, double free or leak) and reports its own drop.
pub struct Tracked {
    pub text: String,
    id: usize,
    ledger: Rc<Ledger>,
}

impl Ledger {
    pub fn make(self: &Rc<Self>) -> Tracked {
        let id = {
            let mut drops = self.0.borrow_mut();
            drops.push(0);
            drops.len() - 1
        };
        Tracked { text: format!("item-{id}"), id, ledger: Rc::clone(self) }
    }

    /// Every element ever made must have been dropped exactly once.
    pub fn assert_all_dropped_once(&self) {
        for (id, &n) in self.0.borrow().iter().enumerate() {
            assert_eq!(n, 1, "element {id} was dropped {n} times");
        }
    }
}

impl Drop for Tracked {
    fn drop(&mut self) {
        self.ledger.0.borrow_mut()[self.id] += 1;
    }
}

/// An `ExactSizeIterator` that lies: `len()` returns `claimed`, whatever it really yields. Writing this needs no `unsafe`.
pub struct Liar<I> {
    pub claimed: usize,
    pub inner: I,
}

impl<I: Iterator> Iterator for Liar<I> {
    type Item = I::Item;
    fn next(&mut self) -> Option<I::Item> {
        self.inner.next()
    }
    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.claimed, Some(self.claimed))
    }
}

impl<I: Iterator> ExactSizeIterator for Liar<I> {
    fn len(&self) -> usize {
        self.claimed
    }
}

/// Unwinds like a panic, but does not call the panic hook. `libfuzzer-sys` installs a hook that aborts the process, so an
/// injected `panic!` would end a fuzz run; this one reaches our `catch_unwind`.
fn injected_panic() -> ! {
    panic::resume_unwind(Box::new("injected"))
}

struct Bytes<'a>(std::slice::Iter<'a, u8>);

impl Bytes<'_> {
    fn next(&mut self) -> Option<u8> {
        self.0.next().copied()
    }
    fn below(&mut self, n: usize) -> usize {
        usize::from(self.next().unwrap_or(0)) % n
    }
}

/// What a script did, so a test can show that the hostile paths really ran.
#[derive(Default, Debug)]
pub struct Reached {
    pub lies: usize,
    pub reentrant_pushes: usize,
    pub reentrant_reads: usize,
    pub panics: usize,
    pub clears: usize,
}

impl std::ops::AddAssign for Reached {
    fn add_assign(&mut self, o: Reached) {
        self.lies += o.lies;
        self.reentrant_pushes += o.reentrant_pushes;
        self.reentrant_reads += o.reentrant_reads;
        self.panics += o.panics;
        self.clears += o.clears;
    }
}

pub fn run(data: &[u8]) -> Reached {
    let ledger = Rc::new(Ledger::default());
    let mut bytes = Bytes(data.iter());
    let mut reached = Reached::default();
    let mut shelf: Shelf<Tracked, CAP> = Shelf::new();
    // An "epoch" ends at `clear`, which needs `&mut shelf`, so no reference can survive it (the compiler checks that).
    loop {
        if !epoch(&shelf, &ledger, &mut bytes, &mut reached) {
            break;
        }
        shelf.clear();
        reached.clears += 1;
        assert!(shelf.is_empty());
    }
    drop(shelf);
    ledger.assert_all_dropped_once();
    reached
}

/// Returns true if the script asked for a `clear`, false when it ran out of bytes.
fn epoch(shelf: &Shelf<Tracked, CAP>, ledger: &Rc<Ledger>, bytes: &mut Bytes<'_>, reached: &mut Reached) -> bool {
    let mut model: Vec<String> = Vec::new();
    // References handed out by the shelf, each with the text it had then. All must keep reading that text.
    let mut held: Vec<(&Tracked, String)> = Vec::new();
    while let Some(op) = bytes.next() {
        match op % 5 {
            0 => {
                let value = ledger.make();
                let text = value.text.clone();
                match shelf.push(value) {
                    Ok(r) => {
                        model.push(text.clone());
                        held.push((r, text));
                    }
                    Err(back) => {
                        assert_eq!(model.len(), CAP, "push refused a value although there was room");
                        assert_eq!(back.text, text, "push must give back the value it refused");
                    }
                }
            }
            1 => {
                let i = bytes.below(CAP + 2);
                assert_eq!(shelf.get(i).map(|t| &t.text), model.get(i), "get({i})");
            }
            2 => {
                let (claimed, actual) = (bytes.below(CAP + 3), bytes.below(CAP + 3));
                reached.lies += usize::from(claimed != actual);
                let room = CAP - model.len();
                let items: Vec<Tracked> = (0..actual).map(|_| ledger.make()).collect();
                let texts: Vec<String> = items.iter().map(|t| t.text.clone()).collect();
                let got = shelf.extend_exact(Liar { claimed, inner: items.into_iter() });
                if claimed > room {
                    assert_eq!(got, Err(NoRoom));
                } else {
                    assert_eq!(got, Ok(actual.min(room)));
                    model.extend(texts.into_iter().take(room));
                }
            }
            3 => {
                let count = bytes.below(4);
                let caught = panic::catch_unwind(AssertUnwindSafe(|| {
                    shelf.fill_with(count, |_| {
                        match bytes.below(4) {
                            // Re-entrant push: keep the reference, it is checked after every operation.
                            1 => {
                                let value = ledger.make();
                                let text = value.text.clone();
                                if let Ok(r) = shelf.push(value) {
                                    reached.reentrant_pushes += 1;
                                    model.push(text.clone());
                                    held.push((r, text));
                                }
                            }
                            // Re-entrant read of everything the shelf says it has.
                            2 => {
                                reached.reentrant_reads += 1;
                                assert!(shelf.iter().map(|t| &t.text).eq(model.iter()));
                                assert_eq!(shelf.len(), model.len());
                            }
                            3 => injected_panic(),
                            _ => {}
                        }
                        let value = ledger.make();
                        // The value is stored only if `make` did not just fill the shelf.
                        if model.len() < CAP {
                            model.push(value.text.clone());
                        }
                        value
                    })
                }));
                if let Err(payload) = caught {
                    // Only our own panic is expected; a failed assertion inside the closure must still fail the run.
                    if payload.downcast_ref::<&str>() != Some(&"injected") {
                        panic::resume_unwind(payload);
                    }
                    reached.panics += 1;
                }
            }
            _ => {
                if bytes.below(4) == 0 {
                    return true;
                }
            }
        }
        // The oracle, after every operation.
        assert_eq!(shelf.len(), model.len(), "length");
        assert!(shelf.iter().map(|t| &t.text).eq(model.iter()), "contents differ from the model");
        for (r, text) in &held {
            assert_eq!(&r.text, text, "a reference handed out earlier changed under its owner");
        }
    }
    false
}
