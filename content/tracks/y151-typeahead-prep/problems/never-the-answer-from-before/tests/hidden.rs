use solution::*;
use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::thread::{self, JoinHandle};

#[test]
fn shared_between_threads() {
    fn needs<T: Send + Sync>() {}
    needs::<Shelf<String, Vec<u8>>>();
}

/// A thread whose work for `key` has begun and waits to be told to finish with `value`.
fn held(shelf: &Arc<Shelf<u32, u32>>, key: u32, value: u32) -> (Sender<()>, JoinHandle<(u32, bool)>) {
    let shelf = shelf.clone();
    let (began, has_begun) = channel::<()>();
    let (finish, may_finish): (Sender<()>, Receiver<()>) = channel();
    let thread = thread::spawn(move || {
        shelf.get_or_work_out(&key, || {
            began.send(()).unwrap();
            may_finish.recv().unwrap();
            value
        })
    });
    has_begun.recv().unwrap();
    (finish, thread)
}

#[test]
fn a_change_while_another_thread_is_working() {
    let shelf = Arc::new(Shelf::new());
    let (finish, thread) = held(&shelf, 1, 10);
    shelf.changed(|&k| k == 1);
    finish.send(()).unwrap();
    check!("its caller", thread.join().unwrap(), (10, false));
    check!("not stored", shelf.get_or_work_out(&1, || 11), (11, false));
}

#[test]
fn two_pieces_of_old_work_for_one_key() {
    let shelf = Arc::new(Shelf::new());
    let (finish_a, a) = held(&shelf, 1, 10);
    let (finish_b, b) = held(&shelf, 1, 20);
    shelf.changed(|&k| k == 1);
    finish_a.send(()).unwrap();
    check!("a", a.join().unwrap(), (10, false));
    finish_b.send(()).unwrap();
    check!("b", b.join().unwrap(), (20, false));
    check!("neither was stored", shelf.get_or_work_out(&1, || 30), (30, false));
}

#[test]
fn old_work_that_ends_last_does_not_replace_the_new_answer() {
    let shelf = Arc::new(Shelf::new());
    let (finish_old, old) = held(&shelf, 1, 10);
    shelf.changed(|&k| k == 1);
    check!("new work while the old still runs", shelf.get_or_work_out(&1, || 11), (11, false));
    finish_old.send(()).unwrap();
    old.join().unwrap();
    check!("the new one stayed", shelf.get_or_work_out(&1, || 12), (11, true));
}

#[test]
fn work_begun_after_the_change_is_kept() {
    let shelf = Arc::new(Shelf::new());
    let (finish_old, old) = held(&shelf, 1, 10);
    shelf.changed(|&k| k == 1);
    let (finish_new, new) = held(&shelf, 1, 11);
    finish_old.send(()).unwrap();
    old.join().unwrap();
    finish_new.send(()).unwrap();
    new.join().unwrap();
    check!("stored", shelf.get_or_work_out(&1, || 12), (11, true));
}

#[test]
fn many_untouched_pieces_of_work_are_all_stored() {
    let shelf = Arc::new(Shelf::new());
    let working: Vec<_> = (0..8).map(|k| held(&shelf, k, k * 10)).collect();
    shelf.changed(|&k| k == 3 || k == 5);
    shelf.changed(|_| false);
    for (finish, thread) in working {
        finish.send(()).unwrap();
        thread.join().unwrap();
    }
    for k in 0..8 {
        let touched = k == 3 || k == 5;
        check!(format!("key {k}"), shelf.get_or_work_out(&k, || 999), if touched { (999, false) } else { (k * 10, true) });
    }
}

#[test]
fn a_panic_in_the_work_leaves_the_shelf_usable() {
    let shelf: Shelf<u32, u32> = Shelf::new();
    shelf.get_or_work_out(&2, || 20);
    for _ in 0..3 {
        let broke = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| shelf.get_or_work_out(&1, || panic!("no data"))));
        check!("the panic reaches the caller", broke.is_err(), true);
    }
    shelf.changed(|&k| k == 1);
    check!("1", shelf.get_or_work_out(&1, || 10), (10, false));
    check!("1 stored", shelf.get_or_work_out(&1, || 11), (10, true));
    check!("2 kept", shelf.get_or_work_out(&2, || 21), (20, true));
}

/// A model: the facts, what the shelf must hold, and changes that happen while answers are being made.
struct Model<'a> {
    shelf: &'a Shelf<u32, u32>,
    stored: RefCell<HashMap<u32, u32>>,
    /// for each key, how many changes have touched it
    version: RefCell<[u32; 5]>,
    rng: RefCell<anneal_prelude::Rng>,
    wrong: RefCell<Option<String>>,
}

impl Model<'_> {
    fn change(&self) {
        let (first, second) = {
            let mut rng = self.rng.borrow_mut();
            (rng.below(32), rng.below(32))
        };
        let mask = first & second;
        for k in 0..5 {
            if mask >> k & 1 == 1 {
                self.version.borrow_mut()[k] += 1;
            }
        }
        self.shelf.changed(|&k| mask >> k & 1 == 1);
        self.stored.borrow_mut().retain(|&k, _| mask >> k & 1 == 0);
    }

    fn ask(&self, depth: usize) {
        let key = self.rng.borrow_mut().below(5) as u32;
        let expected = self.stored.borrow().get(&key).copied();
        let started = self.version.borrow()[key as usize];
        let value = self.rng.borrow_mut().below(1000) as u32;
        let got = self.shelf.get_or_work_out(&key, || {
            let steps = self.rng.borrow_mut().below(3);
            for _ in 0..steps {
                let nested = self.rng.borrow_mut().below(3);
                if nested == 0 && depth < 3 {
                    self.ask(depth + 1);
                } else {
                    self.change();
                }
            }
            value
        });
        let want = match expected {
            Some(v) => (v, true),
            None => (value, false),
        };
        if got != want && self.wrong.borrow().is_none() {
            *self.wrong.borrow_mut() = Some(format!("key {key}: got {got:?}, want {want:?}"));
        }
        if expected.is_none() && self.version.borrow()[key as usize] == started {
            self.stored.borrow_mut().insert(key, value);
        }
    }
}

#[test]
fn agrees_with_a_model_when_work_is_nested_and_interrupted() {
    for seed in 0..600 {
        let shelf = Shelf::new();
        let model = Model {
            shelf: &shelf,
            stored: RefCell::new(HashMap::new()),
            version: RefCell::new([0; 5]),
            rng: RefCell::new(anneal_prelude::Rng::new(15109 + seed)),
            wrong: RefCell::new(None),
        };
        for _ in 0..20 {
            let change = model.rng.borrow_mut().below(4) == 0;
            if change {
                model.change();
            } else {
                model.ask(0);
            }
        }
        check!(format!("seed {seed}"), model.wrong.borrow().clone(), None);
    }
}
