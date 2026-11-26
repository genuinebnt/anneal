//! RECIPE 171 (memory): a slab / object pool with a free list through vacant slots and GENERATIONAL typed handles.
//! LEVEL: advanced
//! Use in: ECS entity storage, connection/session tables, IO resource tables, graph nodes.   Run: cargo run --release --example slab_generational
#![allow(dead_code)]
//!
//! WHAT: a `Vec<Entry<T>>` where each slot is either `Occupied { ver, val }` or `Vacant { ver, next_free }`. Vacant slots form an
//! intrusive singly linked LIST through `next_free` (no second Vec): insert pops the head (O(1)), remove pushes the slot back.
//! Slots are reused, so memory is dense and stable, and a handle is just two integers.
//!
//! WHY GENERATIONS: with plain indices, `remove(3)` then `insert(x)` reuses slot 3, and an old copy of index 3 now silently
//! refers to the NEW object (the ABA problem / a use-after-free on indices; in a game it means "the bullet hits the wrong
//! entity", in a server "the reply goes to someone else's connection"). Fix: each slot has a generation counter bumped on
//! remove; a `Handle` stores `(index, generation)` and `get` compares. A stale handle yields `None` (a clean error) instead of
//! aliasing. Caveat: the u32 generation wraps after 2^32 reuses of ONE slot, after which a very old handle could match again;
//! use u64 or retire the slot if that matters.
//!
//! TYPED: `Handle<T>` carries `PhantomData<fn() -> T>` so a `Handle<Conn>` cannot index a `Slab<Player>` (compile error) and
//! the handle is `Copy`/`Send`/`Sync` regardless of T. Clone/Eq/Hash are written by hand to avoid derive's `T: Clone` bounds.
//! Verified by a MODEL TEST: 50k seeded random ops against `HashMap`, with an oracle that says exactly which of ALL handles
//! ever issued (including stale ones) must be valid.
//!
//! COMMON USES: entity-component systems in game engines (the `slotmap` and `generational-arena` crates implement this idea;
//! bevy's `Entity` is an index plus a generation); connection tables in servers (an event loop stores the handle as the
//! poll token); tokio's IO driver keeps registered IO resources in an internal slab and uses the slot index as the token;
//! graph/tree nodes referenced by handle instead of pointers (see arena_ids, lru_cache).
//! Read: https://docs.rs/slotmap and https://docs.rs/slab and https://docs.rs/generational-arena
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::marker::PhantomData;

const NIL: u32 = u32::MAX;

struct Handle<T> {
    index: u32,
    ver: u32,
    _m: PhantomData<fn() -> T>,
}
impl<T> Clone for Handle<T> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<T> Copy for Handle<T> {}
impl<T> PartialEq for Handle<T> {
    fn eq(&self, o: &Self) -> bool {
        self.index == o.index && self.ver == o.ver
    }
}
impl<T> Eq for Handle<T> {}
impl<T> Hash for Handle<T> {
    fn hash<H: Hasher>(&self, h: &mut H) {
        (self.index, self.ver).hash(h)
    }
}
impl<T> std::fmt::Debug for Handle<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Handle({}v{})", self.index, self.ver)
    }
}

enum Entry<T> {
    Occupied { ver: u32, val: T },
    Vacant { ver: u32, next_free: u32 },
}

struct Slab<T> {
    entries: Vec<Entry<T>>,
    free_head: u32,
    len: usize,
}

impl<T> Slab<T> {
    fn new() -> Self {
        Slab {
            entries: Vec::new(),
            free_head: NIL,
            len: 0,
        }
    }
    fn len(&self) -> usize {
        self.len
    }
    fn capacity_slots(&self) -> usize {
        self.entries.len()
    }

    fn insert(&mut self, val: T) -> Handle<T> {
        self.len += 1;
        if self.free_head != NIL {
            let index = self.free_head;
            let slot = &mut self.entries[index as usize];
            let Entry::Vacant { ver, next_free } = *slot else {
                unreachable!("free list points at an occupied slot")
            };
            self.free_head = next_free;
            *slot = Entry::Occupied { ver, val };
            Handle {
                index,
                ver,
                _m: PhantomData,
            }
        } else {
            assert!(self.entries.len() < NIL as usize, "slab full");
            let index = self.entries.len() as u32;
            self.entries.push(Entry::Occupied { ver: 0, val });
            Handle {
                index,
                ver: 0,
                _m: PhantomData,
            }
        }
    }

    fn get(&self, h: Handle<T>) -> Option<&T> {
        match self.entries.get(h.index as usize)? {
            Entry::Occupied { ver, val } if *ver == h.ver => Some(val),
            _ => None,
        }
    }
    fn get_mut(&mut self, h: Handle<T>) -> Option<&mut T> {
        match self.entries.get_mut(h.index as usize)? {
            Entry::Occupied { ver, val } if *ver == h.ver => Some(val),
            _ => None,
        }
    }
    fn contains(&self, h: Handle<T>) -> bool {
        self.get(h).is_some()
    }

    fn remove(&mut self, h: Handle<T>) -> Option<T> {
        if !self.contains(h) {
            return None; // stale or never valid: leave the slot alone
        }
        let vacant = Entry::Vacant {
            ver: h.ver.wrapping_add(1),
            next_free: self.free_head,
        };
        let old = std::mem::replace(&mut self.entries[h.index as usize], vacant);
        self.free_head = h.index;
        self.len -= 1;
        match old {
            Entry::Occupied { val, .. } => Some(val),
            Entry::Vacant { .. } => unreachable!(),
        }
    }

    fn iter(&self) -> impl Iterator<Item = (Handle<T>, &T)> {
        self.entries
            .iter()
            .enumerate()
            .filter_map(|(i, e)| match e {
                Entry::Occupied { ver, val } => Some((
                    Handle {
                        index: i as u32,
                        ver: *ver,
                        _m: PhantomData,
                    },
                    val,
                )),
                Entry::Vacant { .. } => None,
            })
    }
    fn iter_mut(&mut self) -> impl Iterator<Item = (Handle<T>, &mut T)> {
        self.entries
            .iter_mut()
            .enumerate()
            .filter_map(|(i, e)| match e {
                Entry::Occupied { ver, val } => Some((
                    Handle {
                        index: i as u32,
                        ver: *ver,
                        _m: PhantomData,
                    },
                    val,
                )),
                Entry::Vacant { .. } => None,
            })
    }

    /// Keep only entries for which `f` returns true; removed slots join the free list (and bump their generation).
    fn retain(&mut self, mut f: impl FnMut(Handle<T>, &mut T) -> bool) {
        for i in 0..self.entries.len() {
            let keep = match &mut self.entries[i] {
                Entry::Occupied { ver, val } => f(
                    Handle {
                        index: i as u32,
                        ver: *ver,
                        _m: PhantomData,
                    },
                    val,
                ),
                Entry::Vacant { .. } => true,
            };
            if !keep {
                let Entry::Occupied { ver, .. } = self.entries[i] else {
                    unreachable!()
                };
                self.entries[i] = Entry::Vacant {
                    ver: ver.wrapping_add(1),
                    next_free: self.free_head,
                };
                self.free_head = i as u32;
                self.len -= 1;
            }
        }
    }

    /// Internal consistency: free list visits every vacant slot exactly once, count matches.
    fn check_invariants(&self) {
        let vacant = self
            .entries
            .iter()
            .filter(|e| matches!(e, Entry::Vacant { .. }))
            .count();
        assert_eq!(self.len + vacant, self.entries.len());
        let (mut n, mut cur) = (0, self.free_head);
        while cur != NIL {
            let Entry::Vacant { next_free, .. } = self.entries[cur as usize] else {
                panic!("free list hits occupied slot")
            };
            cur = next_free;
            n += 1;
            assert!(n <= vacant, "free list cycle");
        }
        assert_eq!(n, vacant, "vacant slot missing from free list");
    }
}

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
}

struct Conn {
    peer: &'static str,
    bytes: u64,
}

fn main() {
    // 1. the ABA problem, and the fix
    let mut slab: Slab<&str> = Slab::new();
    let a = slab.insert("alice");
    let b = slab.insert("bob");
    assert_eq!(slab.remove(a), Some("alice"));
    let c = slab.insert("carol"); // reuses a's slot
    assert_eq!(a.index, c.index);
    assert_ne!(a.ver, c.ver);
    assert_eq!(slab.get(a), None, "stale handle must not see carol");
    assert_eq!(slab.get(c), Some(&"carol"));
    assert_eq!(slab.remove(a), None);
    assert_eq!(slab.get(b), Some(&"bob"));
    println!(
        "stale {a:?} -> None, fresh {c:?} -> carol (a plain index would have returned carol for alice's id)"
    );

    // 2. connection table: handle as token, iteration, retain
    let mut conns: Slab<Conn> = Slab::new();
    let hs: Vec<_> = ["10.0.0.1", "10.0.0.2", "10.0.0.3", "10.0.0.4"]
        .iter()
        .map(|p| conns.insert(Conn { peer: p, bytes: 0 }))
        .collect();
    for (i, h) in hs.iter().enumerate() {
        conns.get_mut(*h).unwrap().bytes += 100 * (i as u64 + 1);
    }
    for (_, c) in conns.iter_mut() {
        c.bytes += 1;
    }
    conns.retain(|_, c| c.bytes > 250); // drop idle peers
    let left: Vec<_> = conns.iter().map(|(_, c)| (c.peer, c.bytes)).collect();
    assert_eq!(left, vec![("10.0.0.3", 301), ("10.0.0.4", 401)]);
    assert!(!conns.contains(hs[0]) && conns.contains(hs[3]));
    conns.check_invariants();
    println!("connection table after retain: {left:?}");

    // 3. model test with a handle-validity oracle
    let mut slab: Slab<(u64, u64)> = Slab::new(); // (unique id, value)
    let mut model: HashMap<u64, u64> = HashMap::new(); // id -> value
    let mut issued: Vec<(Handle<(u64, u64)>, u64)> = Vec::new(); // EVERY handle ever returned, with its id
    let mut rng = Rng(0xdead_beef_cafe_f00d);
    let (mut next_id, mut stale_hits, mut reuse) = (0u64, 0usize, 0usize);
    for step in 0..50_000 {
        match rng.below(100) {
            0..=39 => {
                let (id, v) = (next_id, rng.next());
                next_id += 1;
                let h = slab.insert((id, v));
                if issued
                    .iter()
                    .rev()
                    .take(64)
                    .any(|(o, _)| o.index == h.index)
                {
                    reuse += 1;
                }
                model.insert(id, v);
                issued.push((h, id));
            }
            40..=64 if !issued.is_empty() => {
                let (h, id) = issued[rng.below(issued.len())];
                let alive = model.contains_key(&id);
                let got = slab.remove(h);
                assert_eq!(got.is_some(), alive, "remove validity");
                if let Some((gid, v)) = got {
                    assert_eq!((gid, Some(v)), (id, model.remove(&id)));
                }
            }
            65..=89 if !issued.is_empty() => {
                let (h, id) = issued[rng.below(issued.len())];
                match (slab.get(h), model.get(&id)) {
                    (Some(&(gid, v)), Some(&mv)) => assert_eq!((gid, v), (id, mv)),
                    (None, None) => stale_hits += 1,
                    other => panic!("handle/model disagree at step {step}: {other:?}"),
                }
            }
            90..=95 if !issued.is_empty() => {
                let (h, id) = issued[rng.below(issued.len())];
                if let Some(e) = slab.get_mut(h) {
                    e.1 = e.1.wrapping_add(1);
                    *model.get_mut(&id).unwrap() = e.1;
                } else {
                    assert!(!model.contains_key(&id));
                }
            }
            96..=98 => {
                let mut dropped = Vec::new();
                slab.retain(|_, (id, v)| {
                    let keep = *v % 5 != 0;
                    if !keep {
                        dropped.push(*id);
                    }
                    keep
                });
                for id in dropped {
                    assert!(model.remove(&id).is_some());
                }
            }
            _ => {
                let seen: HashMap<u64, u64> = slab.iter().map(|(_, &(id, v))| (id, v)).collect();
                assert_eq!(seen, model, "iteration disagrees with model");
                slab.check_invariants();
            }
        }
        assert_eq!(slab.len(), model.len());
    }
    slab.check_invariants();
    // final oracle sweep over every handle ever issued
    let valid = issued
        .iter()
        .filter(|(h, id)| slab.get(*h).is_some() == model.contains_key(id))
        .count();
    assert_eq!(valid, issued.len());
    println!(
        "model test: 50000 ops, {} handles issued, {} live, {} slots, {} slot reuses seen, {} stale lookups correctly None",
        issued.len(),
        slab.len(),
        slab.capacity_slots(),
        reuse,
        stale_hits
    );
    assert!(stale_hits > 100 && reuse > 100 && slab.capacity_slots() < issued.len());
    println!("ok");
}
