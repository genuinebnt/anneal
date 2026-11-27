use solution::*;
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;

/// An item that writes down when it is dropped.
#[derive(Debug)]
struct Traced(u32, Rc<RefCell<Vec<u32>>>);

impl Drop for Traced {
    fn drop(&mut self) {
        self.1.borrow_mut().push(self.0);
    }
}

#[test]
fn renaming_between_two_names_of_one_item_changes_nothing() {
    let mut shelf = Shelf::new();
    shelf.add("a", 1).unwrap();
    shelf.also("a", "b").unwrap();
    check!("a -> b", shelf.rename("a", "b"), Ok(None));
    check!("both names stay", (shelf.names_of("a"), shelf.names_of("b")), (2, 2));
}

#[test]
fn renaming_a_name_onto_itself() {
    let mut shelf = Shelf::new();
    shelf.add("a", 1).unwrap();
    check!("a -> a", shelf.rename("a", "a"), Ok(None));
    check!("still there", (shelf.get("a"), shelf.names_of("a")), (Some(&1), 1));
    check!("missing onto itself", shelf.rename("z", "z"), Err(ShelfError::Missing));
}

#[test]
fn replacing_one_of_two_names_keeps_the_item() {
    let mut shelf = Shelf::new();
    shelf.add("x", 1).unwrap();
    shelf.also("x", "y").unwrap();
    shelf.add("z", 2).unwrap();
    check!("rename z onto y", shelf.rename("z", "y"), Ok(None));
    check!("x keeps the first item, y has the second", (shelf.get("x"), shelf.get("y"), shelf.get("z")), (Some(&1), Some(&2), None));
    check!("counts", (shelf.names_of("x"), shelf.names_of("y"), shelf.len()), (1, 1, 2));
}

#[test]
fn rename_to_a_free_name_keeps_the_count() {
    let mut shelf = Shelf::new();
    shelf.add("a", 1).unwrap();
    shelf.also("a", "b").unwrap();
    check!("a -> c", shelf.rename("a", "c"), Ok(None));
    check!("counts", (shelf.names_of("a"), shelf.names_of("b"), shelf.names_of("c")), (0, 2, 2));
}

#[test]
fn also_checks_the_old_name_first() {
    let mut shelf = Shelf::new();
    shelf.add("a", 1).unwrap();
    check!("both wrong", shelf.also("no", "a"), Err(ShelfError::Missing));
    check!("onto itself", shelf.also("a", "a"), Err(ShelfError::Taken));
    check!("one name", shelf.names_of("a"), 1);
}

#[test]
fn a_refused_add_drops_only_the_new_item() {
    let log = Rc::new(RefCell::new(Vec::new()));
    let mut shelf = Shelf::new();
    shelf.add("a", Traced(1, log.clone())).unwrap();
    check!("taken", shelf.add("a", Traced(2, log.clone())), Err(ShelfError::Taken));
    check!("dropped", log.borrow().clone(), vec![2]);
}

#[test]
fn nothing_is_kept_after_its_last_name() {
    let log = Rc::new(RefCell::new(Vec::new()));
    let mut shelf = Shelf::new();
    for i in 0..5 {
        shelf.add(&i.to_string(), Traced(i, log.clone())).unwrap();
    }
    drop(shelf.remove("0"));
    drop(shelf.rename("1", "2"));
    check!("0 and 2 are gone", log.borrow().clone(), vec![0, 2]);
}

#[test]
fn a_name_can_be_used_again() {
    let mut shelf = Shelf::new();
    shelf.add("a", 1).unwrap();
    shelf.remove("a").unwrap();
    check!("add", shelf.add("a", 2), Ok(()));
    check!("new item", shelf.get("a"), Some(&2));
}

#[test]
fn odd_names_are_just_names() {
    let mut shelf = Shelf::new();
    for (i, name) in ["", " ", "a/b", "..", "é", "e\u{301}"].iter().enumerate() {
        check!(format!("add {name:?}"), shelf.add(name, i), Ok(()));
    }
    check!("six items", shelf.len(), 6);
}

#[test]
fn random_operations_match_a_model() {
    let mut rng = anneal_prelude::Rng::new(111_501);
    for round in 0..200 {
        let mut shelf: Shelf<u64> = Shelf::new();
        // name -> item; an item is just its unique number here
        let mut model: BTreeMap<String, u64> = BTreeMap::new();
        let mut made = 0;
        for step in 0..100 {
            let name = |rng: &mut anneal_prelude::Rng| ["a", "b", "c", "d"][rng.below(4)].to_string();
            let (p, q) = (name(&mut rng), name(&mut rng));
            let what = format!("round {round} step {step} ({p}, {q})");
            let count = |m: &BTreeMap<String, u64>, id: u64| m.values().filter(|v| **v == id).count();
            let gone = |m: &BTreeMap<String, u64>, id: u64| (count(m, id) == 0).then_some(id);
            match rng.below(4) {
                0 => {
                    made += 1;
                    let want = if model.contains_key(&p) { Err(ShelfError::Taken) } else { Ok(()) };
                    check!(format!("{what}: add"), shelf.add(&p, made), want);
                    if want.is_ok() {
                        model.insert(p.clone(), made);
                    }
                }
                1 => {
                    let want = match model.get(&p).copied() {
                        None => Err(ShelfError::Missing),
                        Some(_) if model.contains_key(&q) => Err(ShelfError::Taken),
                        Some(id) => {
                            model.insert(q.clone(), id);
                            Ok(())
                        }
                    };
                    check!(format!("{what}: also"), shelf.also(&p, &q), want);
                }
                2 => {
                    let want = model.remove(&p).map(|id| gone(&model, id)).ok_or(ShelfError::Missing);
                    check!(format!("{what}: remove"), shelf.remove(&p), want);
                }
                _ => {
                    let want = match model.get(&p).copied() {
                        None => Err(ShelfError::Missing),
                        Some(id) if model.get(&q) == Some(&id) => Ok(None),
                        Some(id) => {
                            model.remove(&p);
                            let old = model.insert(q.clone(), id);
                            Ok(old.and_then(|o| gone(&model, o)))
                        }
                    };
                    check!(format!("{what}: rename"), shelf.rename(&p, &q), want);
                }
            }
            let mut items: Vec<u64> = model.values().copied().collect();
            items.sort();
            items.dedup();
            check!(format!("{what}: items kept"), (shelf.len(), shelf.is_empty()), (items.len(), items.is_empty()));
            for name in ["a", "b", "c", "d"] {
                let want = model.get(name).copied();
                check!(format!("{what}: {name}"), (shelf.get(name).copied(), shelf.names_of(name)), (want, want.map_or(0, |id| count(&model, id))));
            }
        }
    }
}
