use solution::*;

#[test]
fn empty_map() {
    check!(r#"nothing inside"#, { let m: OaMap<u32, u32> = OaMap::new(); (m.len(), m.is_empty(), m.get(&1).copied()) }, (0, true, None));
}

use std::hash::{BuildHasher, Hasher};

/// A hasher that sends every key to the same slot: all operations become one long probe chain.
#[derive(Default, Clone)]
struct AllCollide;
struct AllCollideHasher;
impl BuildHasher for AllCollide {
    type Hasher = AllCollideHasher;
    fn build_hasher(&self) -> AllCollideHasher {
        AllCollideHasher
    }
}
impl Hasher for AllCollideHasher {
    fn write(&mut self, _: &[u8]) {}
    fn finish(&self) -> u64 {
        7
    }
}

        #[test]
        fn all_keys_collide_but_everything_still_works() {
            let mut m = OaMap::with_hasher(AllCollide);
            for i in 0..200u32 {
                assert_eq!(m.insert(i, i + 1000), None);
            }
            for i in 0..200u32 {
                assert_eq!(m.get(&i), Some(&(i + 1000)));
            }
            check!("an absent key is not found even in a full chain", (m.get(&999), m.len()), (None, 200));
        }

        #[test]
        fn removal_does_not_cut_the_probe_chain() {
            let mut m = OaMap::with_hasher(AllCollide);
            for i in 0..6u32 {
                m.insert(i, i);
            }
            check!("remove one in the middle of the chain", m.remove(&2), Some(2));
            for i in [0u32, 1, 3, 4, 5] {
                assert_eq!(m.get(&i), Some(&i), "key {i} became unreachable after a removal in its chain");
            }
            check!("the removed key is gone", m.get(&2), None);
        }

        #[test]
        fn tombstones_are_reused_and_keys_are_never_duplicated() {
            let mut m = OaMap::with_hasher(AllCollide);
            for i in 0..5u32 {
                m.insert(i, i);
            }
            m.remove(&1);
            m.remove(&3);
            check!("re-inserting a key that sits after a tombstone replaces it", m.insert(4, 40), Some(4));
            check!("a new key", m.insert(9, 90), None);
            check!("len counts live entries only", m.len(), 4);
            check!("no duplicate of key 4", m.iter().filter(|(k, _)| **k == 4).count(), 1);
            check!("sum of values", m.iter().map(|(_, v)| *v).sum::<u32>(), 0 + 40 + 2 + 90);
        }

        #[test]
        fn insert_remove_churn_does_not_grow_the_table() {
            let mut m = OaMap::new();
            for i in 0..100_000u32 {
                m.insert(i, i);
                m.remove(&i);
            }
            check!("empty after churn", m.len(), 0);
            assert!(m.capacity() <= 32, "capacity {} after churn with at most one live entry: tombstones are not being cleaned", m.capacity());
            check!("still works", (m.insert(7, 7), m.get(&7).copied()), (None, Some(7)));
        }

        #[test]
        fn a_table_full_of_tombstones_still_terminates() {
            let r = std::sync::mpsc::channel();
            let tx = r.0;
            std::thread::spawn(move || {
                let mut m = OaMap::new();
                for i in 0..64u32 {
                    m.insert(i, i);
                }
                for i in 0..64u32 {
                    m.remove(&i);
                }
                // lots of tombstones, no live entries: a lookup of an absent key must hit an Empty slot
                let absent = m.get(&1_000_000);
                tx.send(absent.is_none()).ok();
            });
            assert_eq!(r.1.recv_timeout(std::time::Duration::from_secs(5)), Ok(true), "get() looped forever");
        }

        #[test]
        fn drops_each_value_once() {
            use std::cell::Cell;
            use std::rc::Rc;
            #[derive(Clone)]
            struct D(Rc<Cell<usize>>);
            impl Drop for D {
                fn drop(&mut self) {
                    self.0.set(self.0.get() + 1);
                }
            }
            let c = Rc::new(Cell::new(0));
            {
                let mut m = OaMap::new();
                for i in 0..100u32 {
                    m.insert(i, D(c.clone()));
                }
                let replaced = m.insert(5, D(c.clone()));
                drop(replaced);
                check!("the replaced value dropped once", c.get(), 1);
                for i in 0..50u32 {
                    drop(m.remove(&i));
                }
                check!("50 removed, each dropped once", c.get(), 51);
            }
            check!("the map's drop took the remaining 50", c.get(), 101);
        }

        #[test]
        fn random_ops_vs_std_hashmap() {
            let mut rng = anneal_prelude::Rng::new(7001);
            let mut mine: OaMap<u32, u64> = OaMap::new();
            let mut collide: OaMap<u32, u64, AllCollide> = OaMap::with_hasher(AllCollide);
            let mut model: std::collections::HashMap<u32, u64> = std::collections::HashMap::new();
            for step in 0..30_000u64 {
                let key = rng.below(300) as u32;
                match rng.below(3) {
                    0 => {
                        let a = mine.insert(key, step);
                        let b = collide.insert(key, step);
                        let c = model.insert(key, step);
                        assert_eq!((a, b), (c, c), "insert {key}");
                    }
                    1 => {
                        let a = mine.remove(&key);
                        let b = collide.remove(&key);
                        let c = model.remove(&key);
                        assert_eq!((a, b), (c, c), "remove {key}");
                    }
                    _ => {
                        let c = model.get(&key);
                        assert_eq!((mine.get(&key), collide.get(&key)), (c, c), "get {key}");
                    }
                }
                assert_eq!((mine.len(), collide.len()), (model.len(), model.len()));
            }
            let mut a: Vec<_> = mine.iter().map(|(k, v)| (*k, *v)).collect();
            let mut b: Vec<_> = model.iter().map(|(k, v)| (*k, *v)).collect();
            a.sort();
            b.sort();
            check!("same contents", a, b);
        }
