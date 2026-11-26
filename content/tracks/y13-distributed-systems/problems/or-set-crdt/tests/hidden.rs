use solution::*;

use std::collections::BTreeSet;

#[test]
fn empty_sets() {
    let mut a: OrSet<u8> = OrSet::new(1);
    let b = OrSet::new(2);
    a.merge(&b);
    check!("two empty sets", (a.items(), a.contains(&0)), (vec![], false));
}

#[test]
fn removing_the_unseen_does_nothing() {
    let (mut a, mut b) = (OrSet::new(1), OrSet::new(2));
    b.add(7);
    check!("a removes what it has never seen", a.remove(&7), false);
    a.merge(&b);
    check!("it arrives anyway", a.items(), vec![7]);
}

#[test]
fn an_old_copy_does_not_resurrect() {
    let mut a = OrSet::new(1);
    a.add('x');
    let old = a.clone();
    a.remove(&'x');
    a.merge(&old);
    check!("merge the copy taken before the remove", a.items(), vec![]);
}

#[test]
fn remove_covers_every_seen_addition() {
    let (mut a, mut b) = (OrSet::new(1), OrSet::new(2));
    a.add(5);
    a.add(5);
    b.add(5);
    a.merge(&b);
    a.remove(&5);
    b.merge(&a);
    check!("three additions seen, one remove", (a.items(), b.items()), (vec![], vec![]));
}

#[test]
fn same_item_added_independently() {
    let (mut a, mut b) = (OrSet::new(1), OrSet::new(2));
    a.add(1);
    b.add(1);
    a.remove(&1);
    a.merge(&b);
    check!("a's remove does not cover b's unseen add", a.items(), vec![1]);
}

#[test]
fn merging_never_takes_the_other_replica_id() {
    // After merging, each replica must still make additions no one else can make.
    let (mut a, mut b) = (OrSet::new(1), OrSet::new(2));
    a.add("k");
    b.merge(&a);
    a.merge(&b);
    a.remove(&"k");
    b.add("k");
    a.add("j");
    a.merge(&b);
    check!("b's new add survives", a.items(), vec!["j", "k"]);
}

#[test]
fn items_are_sorted_and_unique() {
    let (mut a, mut b) = (OrSet::new(9), OrSet::new(3));
    for x in [5, 1, 5, 3] {
        a.add(x);
        b.add(x);
    }
    a.merge(&b);
    check!("duplicates across two replicas", a.items(), vec![1, 3, 5]);
}

#[test]
fn works_with_owned_strings() {
    let mut a: OrSet<String> = OrSet::new(1);
    a.add("é".to_string());
    check!("a String element", (a.contains(&"é".to_string()), a.items()), (true, vec!["é".to_string()]));
}

#[test]
fn merge_laws_hold() {
    let mut rng = anneal_prelude::Rng::new(8121);
    for _ in 0..200 {
        let mut sets: Vec<OrSet<u8>> = (0..3).map(OrSet::new).collect();
        for _ in 0..20 {
            let i = rng.below(3);
            let x = rng.below(3) as u8;
            match rng.below(3) {
                0 => sets[i].add(x),
                1 => {
                    sets[i].remove(&x);
                }
                _ => {
                    let other = sets[rng.below(3)].clone();
                    sets[i].merge(&other);
                }
            }
        }
        let [a, b, c] = [&sets[0], &sets[1], &sets[2]];
        let join = |x: &OrSet<u8>, y: &OrSet<u8>| {
            let mut r = x.clone();
            r.merge(y);
            r
        };
        check!("commutative", join(a, b).items(), join(b, a).items());
        check!("associative", join(&join(a, b), c).items(), join(a, &join(b, c)).items());
        let ab = join(a, b);
        check!("idempotent", join(&ab, b).items(), ab.items());
        check!("merge with itself", join(a, a).items(), a.items());
    }
}

/// The definition itself: each replica remembers which additions and which removals it has heard of.
#[derive(Clone, Default)]
struct Heard {
    adds: BTreeSet<(u8, u32)>,
    removed: BTreeSet<u32>,
}

impl Heard {
    fn items(&self) -> Vec<u8> {
        let set: BTreeSet<u8> = self.adds.iter().filter(|(_, id)| !self.removed.contains(id)).map(|(x, _)| *x).collect();
        set.into_iter().collect()
    }
}

#[test]
fn random_histories_match_the_definition() {
    let mut rng = anneal_prelude::Rng::new(8122);
    for _ in 0..500 {
        let n = 2 + rng.below(3);
        let mut sets: Vec<OrSet<u8>> = (0..n as u32).map(OrSet::new).collect();
        let mut model: Vec<Heard> = vec![Heard::default(); n];
        // Old states, to be delivered late and repeatedly.
        let mut mail: Vec<(OrSet<u8>, Heard)> = Vec::new();
        let mut next_id = 0;
        let mut trace = Vec::new();
        for _ in 0..25 {
            let i = rng.below(n);
            let x = rng.below(3) as u8;
            match rng.below(6) {
                0 | 1 => {
                    trace.push(format!("r{i} add {x}"));
                    next_id += 1;
                    sets[i].add(x);
                    model[i].adds.insert((x, next_id));
                }
                2 => {
                    trace.push(format!("r{i} remove {x}"));
                    let was = model[i].items().contains(&x);
                    let seen: Vec<u32> = model[i].adds.iter().filter(|a| a.0 == x).map(|a| a.1).collect();
                    model[i].removed.extend(seen);
                    check!(format!("{trace:?}: remove returned"), sets[i].remove(&x), was);
                }
                3 => {
                    trace.push(format!("snapshot r{i} as message {}", mail.len()));
                    mail.push((sets[i].clone(), model[i].clone()));
                }
                4 if !mail.is_empty() => {
                    let m = rng.below(mail.len());
                    trace.push(format!("r{i} receives message {m}"));
                    sets[i].merge(&mail[m].0);
                    let heard = mail[m].1.clone();
                    model[i].adds.extend(heard.adds);
                    model[i].removed.extend(heard.removed);
                }
                _ => {
                    let j = rng.below(n);
                    trace.push(format!("r{i} merges r{j}"));
                    let other = sets[j].clone();
                    sets[i].merge(&other);
                    let heard = model[j].clone();
                    model[i].adds.extend(heard.adds);
                    model[i].removed.extend(heard.removed);
                }
            }
            check!(format!("{trace:?}"), sets[i].items(), model[i].items());
        }
    }
}
