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
fn no_root() {
    for objects in [vec![], vec![None, object(&[], &[0], 0)]] {
        let mut store = Store { objects: objects.clone(), in_use: vec![false, true] };
        let want: Vec<Finding> = [NoRoot].into_iter().chain((objects.len() > 1).then_some(Lost { id: 1 })).chain([Leaked { page: 1 }]).collect();
        check!(format!("{} objects", objects.len()), audit(&store), want);
        let before = store.clone();
        check!("repair removes nothing", (repair(&mut store), &store), (0, &before));
    }
}

#[test]
fn marking_is_always_safe() {
    let mut store = sound();
    change(&mut store, 1, |o| o.children.push(99));
    store.in_use[2] = false;
    store.in_use[4] = true;
    check!("one fix", repair(&mut store), 1);
    check!("page 2 marked, page 4 left", (store.in_use[2], store.in_use[4]), (true, true));
}

#[test]
fn what_an_unreadable_object_lists_is_not_used() {
    let mut store = sound();
    store.objects[3] = Some(Object { readable: false, children: vec![0, 77, usize::MAX], pages: vec![0, 4, usize::MAX], links: 2 });
    check!("found", audit(&store), vec![Unreadable { id: 3 }, Leaked { page: 3 }]);
}

#[test]
fn every_kind_of_incomplete_view_stops_removal() {
    let breaks: [(&str, fn(&mut Store)); 3] = [
        ("unreadable", |s| change(s, 3, |o| o.readable = false)),
        ("dangling", |s| change(s, 3, |o| o.children.push(1_000_000))),
        ("bad page", |s| change(s, 3, |o| o.pages.push(5))),
    ];
    for (what, damage) in breaks {
        let mut store = sound();
        store.objects.push(object(&[], &[4], 0));
        store.in_use[4] = true;
        change(&mut store, 1, |o| o.links = 9);
        damage(&mut store);
        let before = store.clone();
        check!(format!("{what}: reported"), audit(&store).contains(&Lost { id: 4 }), true);
        check!(format!("{what}: repair"), repair(&mut store), 0);
        check!(format!("{what}: unchanged"), store, before);
    }
}

#[test]
fn bad_and_double_claims() {
    let mut store = sound();
    change(&mut store, 1, |o| o.pages.extend([5, usize::MAX, 2]));
    change(&mut store, 3, |o| o.pages.extend([3, 3]));
    check!("found", audit(&store), vec![BadPage { id: 1, page: 5 }, BadPage { id: 1, page: usize::MAX }, ClaimedTwice { page: 2 }, ClaimedTwice { page: 3 }]);
}

#[test]
fn a_double_claim_does_not_stop_other_repairs() {
    let mut store = sound();
    change(&mut store, 1, |o| o.pages.push(0));
    store.in_use[4] = true;
    check!("the leak goes", repair(&mut store), 1);
    check!("left", audit(&store), vec![ClaimedTwice { page: 0 }]);
}

#[test]
fn each_listing_is_a_reference() {
    let mut store = sound();
    change(&mut store, 1, |o| o.children = vec![3, 3, 3]);
    check!("found", audit(&store), vec![WrongLinks { id: 3, stored: 2, found: 4 }]);
}

#[test]
fn the_root_can_be_referred_to() {
    let mut store = sound();
    change(&mut store, 3, |o| o.children = vec![0, 3]);
    check!("found", audit(&store), vec![WrongLinks { id: 0, stored: 0, found: 1 }, WrongLinks { id: 3, stored: 2, found: 3 }]);
}

#[test]
fn a_long_chain() {
    let n = 300_000;
    let mut store = Store { objects: (0..n).map(|i| if i + 1 < n { object(&[i + 1], &[i], 1) } else { object(&[0], &[i], 1) }).collect(), in_use: vec![true; n] };
    check!("a ring of 300 000 is sound", audit(&store).len(), 0);
    change(&mut store, 0, |o| o.children.clear());
    // n - 1 lost objects, their n - 1 pages leaked, and the root's count is now wrong
    check!("cut at the start: all but the root is lost", audit(&store).len(), 2 * (n - 1) + 1);
}

#[test]
fn random_stores_match_a_slow_check() {
    let mut rng = anneal_prelude::Rng::new(111_701);
    for round in 0..600 {
        let n = rng.below(8);
        let pages = rng.below(8);
        let pick = |rng: &mut anneal_prelude::Rng, bound: usize| (0..rng.below(3)).map(|_| rng.below(bound + 1)).collect::<Vec<_>>();
        let store = Store {
            objects: (0..n)
                .map(|_| (rng.below(6) != 0).then(|| Object { readable: rng.below(7) != 0, children: pick(&mut rng, n), pages: pick(&mut rng, pages), links: rng.below(3) as u32 }))
                .collect(),
            in_use: (0..pages).map(|_| rng.bool()).collect(),
        };
        // reachability by repeating one rule until nothing changes
        let present = |id: usize| matches!(store.objects.get(id), Some(Some(_)));
        let mut reached: std::collections::BTreeSet<usize> = (0..1).filter(|id| present(*id)).collect();
        loop {
            let next: Vec<usize> = reached
                .iter()
                .filter_map(|id| store.objects[*id].as_ref())
                .filter(|o| o.readable)
                .flat_map(|o| o.children.iter().copied())
                .filter(|c| present(*c) && !reached.contains(c))
                .collect();
            if next.is_empty() {
                break;
            }
            reached.extend(next);
        }
        let open: Vec<(usize, &Object)> = reached.iter().map(|id| (*id, store.objects[*id].as_ref().unwrap())).filter(|(_, o)| o.readable).collect();
        let claims: Vec<(usize, usize)> = open.iter().flat_map(|(id, o)| o.pages.iter().map(|p| (*id, *p))).collect();
        let mut want = std::collections::BTreeSet::new();
        if !present(0) {
            want.insert(NoRoot);
        }
        for id in 0..n {
            let Some(o) = &store.objects[id] else { continue };
            if !reached.contains(&id) {
                want.insert(Lost { id });
                continue;
            }
            if !o.readable {
                want.insert(Unreadable { id });
            }
            let found = open.iter().flat_map(|(_, o)| &o.children).filter(|c| **c == id).count() as u32;
            if found != o.links {
                want.insert(WrongLinks { id, stored: o.links, found });
            }
        }
        for (from, o) in &open {
            for to in o.children.iter().filter(|c| !present(**c)) {
                want.insert(Dangling { from: *from, to: *to });
            }
        }
        for (id, page) in &claims {
            if *page >= pages {
                want.insert(BadPage { id: *id, page: *page });
            }
        }
        for page in 0..pages {
            let times = claims.iter().filter(|(_, p)| *p == page).count();
            if times > 1 {
                want.insert(ClaimedTwice { page });
            }
            match (store.in_use[page], times > 0) {
                (false, true) => want.insert(Unmarked { page }),
                (true, false) => want.insert(Leaked { page }),
                _ => false,
            };
        }
        let want: Vec<Finding> = want.into_iter().collect();
        check!(format!("round {round}: {store:?}"), audit(&store), want.clone());

        let incomplete = want.iter().any(|f| matches!(f, NoRoot | Unreadable { .. } | Dangling { .. } | BadPage { .. }));
        let fixable = |f: &Finding| match f {
            Unmarked { .. } => true,
            WrongLinks { .. } | Lost { .. } | Leaked { .. } => !incomplete,
            _ => false,
        };
        let mut repaired = store.clone();
        check!(format!("round {round}: fixed"), repair(&mut repaired), want.iter().filter(|f| fixable(f)).count());
        let left: Vec<Finding> = want.iter().copied().filter(|f| !fixable(f)).collect();
        check!(format!("round {round}: what is left"), audit(&repaired), left);
        for id in &reached {
            let keep = |s: &Store| s.objects[*id].as_ref().map(|o| (o.readable, o.children.clone(), o.pages.clone()));
            check!(format!("round {round}: object {id} keeps its content"), keep(&repaired), keep(&store));
        }
    }
}
