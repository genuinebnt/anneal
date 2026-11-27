use solution::Finding::*;
use solution::*;

fn object(children: &[usize], pages: &[usize], links: u32) -> Option<Object> {
    Some(Object { readable: true, children: children.to_vec(), pages: pages.to_vec(), links })
}

/// root -> 1, 2; 1 -> 3; 2 -> 3 (shared). Each has one page; page 4 is free.
fn sound() -> Store {
    Store {
        objects: vec![object(&[1, 2], &[0], 0), object(&[3], &[1], 1), object(&[3], &[2], 1), object(&[], &[3], 2)],
        in_use: vec![true, true, true, true, false],
    }
}

fn change(store: &mut Store, id: usize, f: impl FnOnce(&mut Object)) {
    f(store.objects[id].as_mut().unwrap());
}

#[test]
fn a_sound_store_has_nothing_to_report() {
    let mut store = sound();
    check!("audit", audit(&store), vec![]);
    check!("repair", repair(&mut store), 0);
    check!("unchanged", store, sound());
}

#[test]
fn counts_and_marks_are_put_right() {
    let mut store = sound();
    change(&mut store, 3, |o| o.links = 7);
    store.in_use[1] = false;
    store.in_use[4] = true;
    check!("found", audit(&store), vec![WrongLinks { id: 3, stored: 7, found: 2 }, Unmarked { page: 1 }, Leaked { page: 4 }]);
    check!("fixed", repair(&mut store), 3);
    check!("sound again", store, sound());
}

#[test]
fn what_nothing_leads_to_is_lost() {
    let mut store = sound();
    // two objects that refer only to each other
    store.objects.push(object(&[5], &[4], 1));
    store.objects.push(object(&[4], &[], 1));
    store.in_use[4] = true;
    check!("found", audit(&store), vec![Lost { id: 4 }, Lost { id: 5 }, Leaked { page: 4 }]);
    check!("fixed", repair(&mut store), 3);
    check!("gone", (store.objects[4].clone(), store.objects[5].clone(), store.in_use[4]), (None, None, false));
}

#[test]
fn nothing_is_thrown_away_when_part_cannot_be_read() {
    let mut store = sound();
    change(&mut store, 1, |o| o.readable = false);
    change(&mut store, 2, |o| o.children.clear());
    let before = store.clone();
    // what object 1 lists cannot be believed, so its page and object 3 look unused
    check!("found", audit(&store), vec![Unreadable { id: 1 }, Lost { id: 3 }, Leaked { page: 1 }, Leaked { page: 3 }]);
    check!("fixed nothing", repair(&mut store), 0);
    check!("unchanged", store, before);
}

#[test]
fn a_reference_to_nothing() {
    let mut store = sound();
    change(&mut store, 2, |o| o.children.push(9));
    check!("found", audit(&store), vec![Dangling { from: 2, to: 9 }]);
}
