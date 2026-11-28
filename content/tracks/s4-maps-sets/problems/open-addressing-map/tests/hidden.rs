use solution::*;

#[test]
fn ten_thousand_keys_survive_resizes() {
    let mut m = OpenMap::new();
    for k in 0..10_000u64 {
        m.insert(k * 7919, k);
    }
    let all_found = (0..10_000u64).all(|k| m.get(k * 7919) == Some(&k));
    check!("insert 10000 keys", (m.len(), all_found), (10_000, true));
}

#[test]
fn remove_keeps_probe_chains_intact() {
    let mut m = OpenMap::new();
    for k in 0..100u64 {
        m.insert(k, k);
    }
    for k in (0..100u64).step_by(2) {
        m.remove(k);
    }
    let odd_found = (1..100u64).step_by(2).all(|k| m.get(k) == Some(&k));
    let even_gone = (0..100u64).step_by(2).all(|k| m.get(k).is_none());
    check!("insert 0..100, remove the even keys", (m.len(), odd_found, even_gone), (50, true, true));
}

#[test]
fn churn_reuses_tombstones() {
    let mut m = OpenMap::new();
    for round in 0..1000u64 {
        m.insert(round, round);
        m.remove(round);
    }
    m.insert(42, 1);
    check!("1000 insert/remove rounds", (m.len(), m.get(42).copied()), (1, Some(1)));
}

#[test]
fn extreme_keys() {
    let mut m = OpenMap::new();
    m.insert(0, "zero");
    m.insert(u64::MAX, "max");
    check!("insert 0 and u64::MAX", (m.get(0).copied(), m.get(u64::MAX).copied(), m.get(1).copied(), m.len()), (Some("zero"), Some("max"), None, 2));
}

#[test]
fn owned_values_come_back() {
    let mut m = OpenMap::new();
    m.insert(1, String::from("one"));
    let old = m.insert(1, String::from("uno"));
    check!("insert 1 → \"one\", then 1 → \"uno\", then remove 1", (old, m.remove(1), m.len()), (Some("one".to_string()), Some("uno".to_string()), 0));
}

#[test]
fn replace_after_resizes() {
    let mut m = OpenMap::new();
    for k in 0..100u64 {
        m.insert(k, k);
    }
    check!("insert 0..100, then insert 50 → 0", (m.insert(50, 0), m.get(50).copied(), m.len()), (Some(50), Some(0), 100));
}

#[test]
fn reinsert_past_tombstones() {
    let mut m = OpenMap::new();
    for k in 0..100u64 {
        m.insert(k, k);
    }
    for k in (0..100u64).step_by(2) {
        m.remove(k);
    }
    let all_replaced = (1..100u64).step_by(2).all(|k| m.insert(k, k + 1000) == Some(k));
    let all_new = (1..100u64).step_by(2).all(|k| m.get(k) == Some(&(k + 1000)));
    check!("insert 0..100, remove the even keys, insert every odd key again", (m.len(), all_replaced, all_new), (50, true, true));
}

#[test]
fn remove_everything() {
    let mut m = OpenMap::new();
    for k in 0..1000u64 {
        m.insert(k * 3, k);
    }
    let all_removed = (0..1000u64).all(|k| m.remove(k * 3) == Some(k));
    m.insert(5, 5);
    check!("insert 1000 keys, remove them all, insert 5", (all_removed, m.len(), m.get(0).copied(), m.get(5).copied()), (true, 1, None, Some(5)));
}

#[test]
fn random_vs_hashmap() {
    let mut rng = anneal_prelude::Rng::new(4010);
    for _ in 0..200 {
        let mut m = OpenMap::new();
        let mut model = std::collections::HashMap::new();
        let mut log = Vec::new();
        let n = rng.below(40);
        for _ in 0..n {
            let k = rng.below(12) as u64;
            match rng.below(3) {
                0 | 1 => {
                    let v = rng.below(100) as u32;
                    log.push(format!("insert {k} → {v}"));
                    check!(log.join(", "), m.insert(k, v), model.insert(k, v));
                }
                _ => {
                    log.push(format!("remove {k}"));
                    check!(log.join(", "), m.remove(k), model.remove(&k));
                }
            }
            let q = rng.below(12) as u64;
            check!(format!("{}; get {q}", log.join(", ")), (m.get(q).copied(), m.len()), (model.get(&q).copied(), model.len()));
        }
    }
}

#[test]
fn scale_200k() {
    let mut m = OpenMap::new();
    for k in 0..200_000u64 {
        m.insert(k * 13, k);
    }
    let found = (0..200_000u64).filter(|&k| m.get(k * 13) == Some(&k)).count();
    let removed = (0..200_000u64).step_by(2).filter(|&k| m.remove(k * 13).is_some()).count();
    check!("insert 200000 keys, get each, remove half", (found, removed, m.len()), (200_000, 100_000, 100_000));
}

fn room_left<V>(m: &OpenMap<V>) -> bool {
    m.empty_slots() * 4 >= m.capacity()
}

#[test]
fn tombstones_count_toward_the_load() {
    let mut m = OpenMap::new();
    for k in 0..10u64 {
        m.insert(k, k);
    }
    let mut first_bad = None;
    for k in 10..20_010u64 {
        m.insert(k, k);
        let after_insert = room_left(&m);
        m.remove(k);
        if first_bad.is_none() && !(after_insert && room_left(&m)) {
            first_bad = Some(k);
        }
    }
    check!("keep 10 keys, insert and remove 20000 more: first key after which fewer than a quarter of the slots are Empty", (first_bad, m.len()), (None, 10));
}

#[test]
fn churn_does_not_grow_the_table() {
    let mut m = OpenMap::new();
    for k in 0..100_000u64 {
        m.insert(k, k);
        m.remove(k);
    }
    let small = m.capacity() <= 16;
    check!("100000 rounds of insert k, remove k (largest len 1): capacity() <= 16, and a quarter empty", (small, room_left(&m), m.len()), (true, true, 0));
}

#[test]
fn capacity_follows_the_largest_len() {
    let mut m = OpenMap::new();
    for k in 0..1000u64 {
        m.insert(k, k);
    }
    for k in 0..1000u64 {
        m.remove(k);
    }
    for k in 1000..50_000u64 {
        m.insert(k, k);
        m.remove(k);
    }
    let bounded = m.capacity() <= 16 * 1000;
    check!("insert 1000 keys, remove them, then 49000 insert/remove rounds: capacity() <= 16000, and a quarter empty", (bounded, room_left(&m), m.len()), (true, true, 0));
}

#[test]
fn random_ops_keep_both_rules() {
    let mut rng = anneal_prelude::Rng::new(5101);
    for _ in 0..50 {
        let mut m = OpenMap::new();
        let mut peak = 0usize;
        let mut ok = true;
        let keys = 1 + rng.below(40) as u64;
        for _ in 0..400 {
            let k = rng.below(keys as usize) as u64;
            if rng.below(2) == 0 {
                m.insert(k, k);
            } else {
                m.remove(k);
            }
            peak = peak.max(m.len());
            ok &= room_left(&m) && m.capacity() <= 16 * peak.max(1);
        }
        check!(format!("400 random inserts and removes over {keys} keys: both rules after every call"), ok, true);
    }
}
