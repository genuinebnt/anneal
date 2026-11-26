//! The HOSTILE tests: safe code that behaves as badly as safe code is allowed to. One test per way of attacking.

use shelf::script::{Ledger, Liar};
use shelf::{NoRoom, Shelf};
use std::cell::Cell;
use std::panic::{self, AssertUnwindSafe};
use std::rc::Rc;

#[test]
fn iterator_that_claims_too_little() {
    let shelf = Shelf::<String, 4>::new();
    let seven = (0..7).map(|i| i.to_string());
    assert_eq!(shelf.extend_exact(Liar { claimed: 0, inner: seven }), Ok(4));
    assert!(shelf.iter().eq(["0", "1", "2", "3"].iter()));
}

#[test]
fn iterator_that_claims_too_much() {
    let shelf = Shelf::<String, 4>::new();
    let one = std::iter::once("x".to_string());
    assert_eq!(shelf.extend_exact(Liar { claimed: usize::MAX, inner: one }), Err(NoRoom));
    assert!(shelf.is_empty());
}

#[test]
fn iterator_that_pushes_while_being_consumed() {
    let shelf = Shelf::<String, 4>::new();
    let items = (0..4).map(|i| {
        let _ = shelf.push(format!("inner-{i}"));
        format!("outer-{i}")
    });
    // The map's `len()` is honest (4) and there is room for 4 when it is checked. The room disappears afterwards.
    assert_eq!(shelf.extend_exact(items), Ok(2));
    assert!(shelf.iter().eq(["inner-0", "outer-0", "inner-1", "outer-1"].iter()));
}

#[test]
fn closure_that_pushes_and_keeps_the_reference() {
    let shelf = Shelf::<String, 4>::new();
    let kept: Cell<Option<&String>> = Cell::new(None);
    let stored = shelf.fill_with(1, |_| {
        kept.set(shelf.push("from inside".to_string()).ok());
        "from outside".to_string()
    });
    assert_eq!(stored, 1);
    assert_eq!(kept.get().map(String::as_str), Some("from inside"));
    assert!(shelf.iter().eq(["from inside", "from outside"].iter()));
}

#[test]
fn closure_that_reads_the_shelf_while_it_is_filling() {
    let shelf = Shelf::<String, 4>::new();
    shelf.fill_with(3, |k| {
        assert_eq!(shelf.len(), k, "only finished elements are visible");
        let seen: Vec<usize> = shelf.iter().map(String::len).collect(); // reads every element the shelf claims to have
        assert_eq!(seen.len(), k);
        format!("v{k}")
    });
}

#[test]
fn closure_that_panics_half_way() {
    let ledger = Rc::new(Ledger::default());
    {
        let shelf = Shelf::<_, 4>::new();
        let result = panic::catch_unwind(AssertUnwindSafe(|| {
            shelf.fill_with(4, |k| {
                assert!(k < 2, "boom");
                ledger.make()
            })
        }));
        assert!(result.is_err());
        assert_eq!(shelf.len(), 2, "the two finished elements stay, nothing else is claimed");
    } // the shelf is dropped here: it must drop exactly the elements it owns
    ledger.assert_all_dropped_once();
}

/// Counts its drops. An armed value panics the first time it is dropped, and only then, so that a second drop shows up
/// as a count of 2 instead of a panic during unwinding (which would abort the whole test binary).
struct Grenade<'a> {
    drops: &'a Cell<u32>,
    armed: bool,
}

impl Drop for Grenade<'_> {
    fn drop(&mut self) {
        self.drops.set(self.drops.get() + 1);
        assert!(!(self.armed && self.drops.get() == 1), "destructor panic");
    }
}

#[test]
fn destructor_that_panics_during_clear() {
    // No heap memory in `Grenade`: elements that are never reached stay in the shelf, so nothing is lost or reported as leaked.
    let counts = [Cell::new(0), Cell::new(0), Cell::new(0)];
    let mut shelf = Shelf::<Grenade<'_>, 3>::new();
    for (i, drops) in counts.iter().enumerate() {
        assert!(shelf.push(Grenade { drops, armed: i == 1 }).is_ok());
    }
    assert!(panic::catch_unwind(AssertUnwindSafe(|| shelf.clear())).is_err());
    let left = shelf.len();
    drop(shelf);
    assert_eq!(counts.each_ref().map(Cell::get), [1, 1, 1], "each destructor ran exactly once");
    assert_eq!(left, 1, "after the panic, only element 0 was still owned");
}

#[test]
fn zero_capacity_zero_sized_and_over_aligned_elements() {
    let none = Shelf::<String, 0>::new();
    assert!(none.push("x".into()).is_err());
    assert_eq!(none.fill_with(3, |_| unreachable!("there is no room, so nothing is made")), 0);

    let units = Shelf::<(), 3>::new();
    assert_eq!(units.fill_with(5, |_| ()), 3);
    assert_eq!(units.iter().count(), 3);

    #[repr(align(64))]
    struct Wide(u8);
    let wide = Shelf::<Wide, 3>::new();
    for i in 0..3 {
        let r = wide.push(Wide(i)).ok().unwrap();
        assert_eq!(std::ptr::from_ref(r) as usize % 64, 0);
    }
    assert_eq!(wide.iter().map(|w| u32::from(w.0)).sum::<u32>(), 3);
}
