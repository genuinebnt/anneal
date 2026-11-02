use solution::*;

#[test]
fn empty_set() {
    check!(r#"nothing inserted"#, { let s = BitSet::new(); (s.len(), s.is_empty(), s.iter().next(), s.contains(0), s.next_set_from(0)) }, (0, true, None, false, None));
}

#[test]
fn equal_despite_trailing_words() {
    check!(r#"{1} that once held 500 equals {1}"#, { let mut a = BitSet::new(); a.insert(1); a.insert(500); a.remove(500); let mut b = BitSet::new(); b.insert(1); (a == b, b == a, a.len() == b.len()) }, (true, true, true));
}

#[test]
fn unequal_sets() {
    check!(r#"different members and different empties"#, { let a: BitSet = [1].into_iter().collect(); let b: BitSet = [2].into_iter().collect(); let mut c = BitSet::new(); c.insert(700); c.remove(700); (a == b, a == c, BitSet::new() == c) }, (false, false, true));
}

#[test]
fn intersect_with_shorter_other() {
    check!(r#"self has a member beyond other's words"#, { let mut a: BitSet = [1, 2, 300].into_iter().collect(); let b: BitSet = [2].into_iter().collect(); a.intersect_with(&b); a.iter().collect::<Vec<_>>() }, vec![2]);
}

#[test]
fn intersect_with_longer_other() {
    check!(r#"other is longer"#, { let mut a: BitSet = [1, 2].into_iter().collect(); let b: BitSet = [2, 300].into_iter().collect(); a.intersect_with(&b); a.iter().collect::<Vec<_>>() }, vec![2]);
}

#[test]
fn union_grows_self() {
    check!(r#"other is longer"#, { let mut a: BitSet = [1].into_iter().collect(); let b: BitSet = [500].into_iter().collect(); a.union_with(&b); a.iter().collect::<Vec<_>>() }, vec![1, 500]);
}

#[test]
fn difference_with_longer_other() {
    check!(r#"other is longer"#, { let mut a: BitSet = [1, 2].into_iter().collect(); let b: BitSet = [2, 900].into_iter().collect(); a.difference_with(&b); a.iter().collect::<Vec<_>>() }, vec![1]);
}

#[test]
fn word_boundaries() {
    check!(r#"members at 63, 64, 127, 128"#, { let s: BitSet = [63, 64, 127, 128].into_iter().collect(); (s.iter().collect::<Vec<_>>(), s.next_set_from(64), s.next_set_from(65), s.next_set_from(128), s.next_set_from(129)) }, (vec![63, 64, 127, 128], Some(64), Some(127), Some(128), None));
}

#[test]
fn full_word() {
    check!(r#"all 64 bits of word 0 and a gap"#, { let s: BitSet = (0..64).chain(192..200).collect(); (s.len(), s.iter().count(), s.next_set_from(64), s.iter().last()) }, (72, 72, Some(192), Some(199)));
}

#[test]
fn with_capacity_is_empty() {
    check!(r#"preallocated, nothing in it"#, { let s = BitSet::with_capacity(10_000); (s.len(), s.is_empty(), s.contains(9_999), s.iter().count()) }, (0, true, false, 0));
}

#[test]
fn random_vs_btreeset() {
    use std::collections::BTreeSet;
    let mut rng = anneal_prelude::Rng::new(6101);
    for _ in 0..300 {
        let span = [10usize, 64, 130, 400][rng.below(4)];
        let mut s = BitSet::new();
        let mut model = BTreeSet::new();
        let mut log = Vec::new();
        for _ in 0..rng.below(40) {
            let i = rng.below(span);
            if rng.below(3) == 0 {
                log.push(format!("remove {i}"));
                check!(format!("{}", log.join(", ")), s.remove(i), model.remove(&i));
            } else {
                log.push(format!("insert {i}"));
                check!(format!("{}", log.join(", ")), s.insert(i), model.insert(i));
            }
        }
        let from = rng.below(span + 5);
        check!(format!("{}: iter, len, next_set_from({from})", log.join(", ")), (s.iter().collect::<Vec<_>>(), s.len(), s.next_set_from(from)), (model.iter().copied().collect::<Vec<_>>(), model.len(), model.range(from..).next().copied()));
        let other: BitSet = (0..rng.below(20)).map(|_| rng.below(span + 70)).collect();
        let omodel: BTreeSet<usize> = other.iter().collect();
        let (mut u, mut x, mut d) = (s.clone(), s.clone(), s.clone());
        u.union_with(&other);
        x.intersect_with(&other);
        d.difference_with(&other);
        check!(format!("{}: algebra with {omodel:?}", log.join(", ")),
            (u.iter().collect::<Vec<_>>(), x.iter().collect::<Vec<_>>(), d.iter().collect::<Vec<_>>()),
            (model.union(&omodel).copied().collect::<Vec<_>>(), model.intersection(&omodel).copied().collect::<Vec<_>>(), model.difference(&omodel).copied().collect::<Vec<_>>()));
    }
}

#[test]
fn a_million_members() {
    let mut s = BitSet::with_capacity(4_000_000);
    for i in (0..4_000_000).step_by(4) {
        s.insert(i);
    }
    let sum: usize = s.iter().sum();
    check!("members 0, 4, 8 .. 3999996: len and sum", (s.len(), sum), (1_000_000, 1_999_998_000_000));
}
