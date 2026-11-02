from author import T, write_track


def sub(s, old, new):
    """str.replace that fails loudly when `old` isn't there (a wrong solution that silently equals the reference)."""
    assert old in s, f"not found: {old[:60]!r}"
    return s.replace(old, new)


P = []

# ---------------------------------------------------------------------------------------------------------------
INTERN = """
use std::collections::HashMap;
use std::rc::Rc;

/// A handle for an interned string: four bytes, `Copy`, compared and hashed as an integer.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Symbol(pub u32);

/// Each distinct string is stored once. Symbols are dense, handed out in first-seen order from 0.
#[derive(Default)]
pub struct Interner {
    map: HashMap<Rc<str>, Symbol>,
    strings: Vec<Rc<str>>,
}

impl Interner {
    pub fn new() -> Self {
        Interner::default()
    }

    pub fn intern(&mut self, s: &str) -> Symbol {
        if let Some(&sym) = self.map.get(s) {
            return sym;
        }
        let sym = Symbol(self.strings.len() as u32);
        let rc: Rc<str> = Rc::from(s);
        self.strings.push(rc.clone());
        self.map.insert(rc, sym);
        sym
    }

    /// The string behind a symbol this interner handed out.
    pub fn resolve(&self, sym: Symbol) -> &str {
        &self.strings[sym.0 as usize]
    }

    pub fn get(&self, s: &str) -> Option<Symbol> {
        self.map.get(s).copied()
    }

    /// Number of distinct strings.
    pub fn len(&self) -> usize {
        self.strings.len()
    }

    pub fn is_empty(&self) -> bool {
        self.strings.is_empty()
    }
}
"""

INTERN_STARTER = """
/// A handle for an interned string: four bytes, `Copy`, compared and hashed as an integer.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Symbol(pub u32);

/// Each distinct string is stored once. Symbols are dense, handed out in first-seen order from 0.
pub struct Interner {
    // TODO: what you need to go from `&str` to `Symbol` and back
}

impl Interner {
    pub fn new() -> Self {
        todo!()
    }

    /// The symbol for `s`, creating it the first time `s` is seen.
    pub fn intern(&mut self, s: &str) -> Symbol {
        todo!()
    }

    /// The string behind a symbol this interner handed out.
    pub fn resolve(&self, sym: Symbol) -> &str {
        todo!()
    }

    /// The symbol for `s` if it was interned already; never creates one.
    pub fn get(&self, s: &str) -> Option<Symbol> {
        todo!()
    }

    /// Number of distinct strings.
    pub fn len(&self) -> usize {
        todo!()
    }

    pub fn is_empty(&self) -> bool {
        todo!()
    }
}
"""

P.append(dict(
    slug="string-interner", title="A string interner returning Symbol(u32)", level="medium", stage="arenas-and-pools",
    tags=["interning", "Symbol(u32)", "Rc<str>", "HashMap", "borrow", "compilers"],
    teaches=["Why compilers intern identifiers: equality becomes an integer compare, a `Symbol` is `Copy`, and each name is stored once.",
             "Looking up a `HashMap<Rc<str>, _>` with a `&str` (the `Borrow<str>` impl) so a hit allocates nothing.",
             "A dense id doubles as an index into a `Vec`: the pattern behind rustc's `Symbol`, `IndexVec`, and every string table."],
    statement="""
        Build the interner a compiler front end uses for identifiers. `intern(s)` returns the same `Symbol` for equal
        strings and a new one for a string it has not seen; symbols are **dense and first-seen ordered** (the first string
        is `Symbol(0)`, the next new one `Symbol(1)`, ...). `resolve` goes back from a symbol to its text, `get` looks a string up
        without creating anything, and `len` counts distinct strings.

        Looking up a string that is already interned must not allocate. Strings may be empty or Unicode, and are stored exactly
        as given (no trimming, no case folding).
    """,
    starter=INTERN_STARTER,
    solution=INTERN,
    visible=[
        T("same_string_same_symbol", "intern twice", "{ let mut i = Interner::new(); let a = i.intern(\"foo\"); let b = i.intern(\"foo\"); (a == b, i.len()) }", "(true, 1)"),
        T("different_strings_differ", "foo and bar", "{ let mut i = Interner::new(); let a = i.intern(\"foo\"); let b = i.intern(\"bar\"); (a == b, i.len()) }", "(false, 2)"),
        T("dense_first_seen", "symbols 0, 1, 0, 2", "{ let mut i = Interner::new(); [i.intern(\"a\").0, i.intern(\"b\").0, i.intern(\"a\").0, i.intern(\"c\").0] }", "[0, 1, 0, 2]"),
        T("round_trip", "resolve gives the text back", "{ let mut i = Interner::new(); let s = i.intern(\"hello\"); let t = i.intern(\"world\"); (i.resolve(s).to_string(), i.resolve(t).to_string()) }", "(\"hello\".to_string(), \"world\".to_string())"),
        T("get_never_creates", "get on a stranger", "{ let mut i = Interner::new(); let a = i.intern(\"x\"); (i.get(\"x\") == Some(a), i.get(\"y\"), i.len()) }", "(true, None, 1)"),
    ],
    hidden=[
        T("empty_interner", "nothing interned", "{ let i = Interner::new(); (i.len(), i.is_empty(), i.get(\"\")) }", "(0, true, None)"),
        T("empty_string", "the empty string is a string", "{ let mut i = Interner::new(); let e = i.intern(\"\"); let a = i.intern(\"a\"); (i.resolve(e).len(), e == a, i.intern(\"\") == e, i.len()) }", "(0, false, true, 2)"),
        T("stored_exactly", "no trimming or case folding", "{ let mut i = Interner::new(); let a = i.intern(\"Foo\"); let b = i.intern(\"foo\"); let c = i.intern(\" foo\"); let d = i.intern(\"foo \"); (a == b, b == c, c == d, i.len(), i.resolve(c).to_string()) }", "(false, false, false, 4, \" foo\".to_string())"),
        T("unicode", "multi-byte strings", "{ let mut i = Interner::new(); let a = i.intern(\"héllo\"); let b = i.intern(\"日本語\"); let c = i.intern(\"héllo\"); (a == c, i.resolve(b).to_string(), i.len()) }", "(true, \"日本語\".to_string(), 2)"),
        T("symbol_is_four_bytes", "Symbol layout", "(std::mem::size_of::<Symbol>(), std::mem::size_of::<Option<Symbol>>() >= 4)", "(4, true)"),
        T("symbols_are_copy_and_hashable", "use as map keys", "{ let mut i = Interner::new(); let a = i.intern(\"k\"); let mut m = std::collections::HashMap::new(); m.insert(a, 1); let b = a; (m[&b], m.contains_key(&i.intern(\"k\"))) }", "(1, true)"),
        T("owned_and_borrowed_inputs", "a String's slice and a literal", "{ let mut i = Interner::new(); let owned = String::from(\"same\"); let a = i.intern(&owned); drop(owned); let b = i.intern(\"same\"); (a == b, i.resolve(a).to_string()) }", "(true, \"same\".to_string())"),
        """
        #[test]
        fn random_vs_model() {
            use std::collections::HashMap;
            let mut rng = anneal_prelude::Rng::new(6501);
            let mut i = Interner::new();
            let mut model: HashMap<String, u32> = HashMap::new();
            for _ in 0..30_000 {
                let len = rng.below(6) as usize;
                let s: String = (0..len).map(|_| (b'a' + rng.below(4) as u8) as char).collect();
                let want = {
                    let n = model.len() as u32;
                    *model.entry(s.clone()).or_insert(n)
                };
                let got = i.intern(&s);
                check!("symbol equals the first-seen index of the string", got.0, want);
                check!("resolve round-trips", i.resolve(got).to_string(), s);
            }
            check!("len equals the number of distinct strings", i.len(), model.len());
        }

        #[test]
        fn many_long_strings_are_stored_once() {
            let mut i = Interner::new();
            let words: Vec<String> = (0..5_000).map(|n| format!("identifier_number_{n:08}")).collect();
            let first: Vec<Symbol> = words.iter().map(|w| i.intern(w)).collect();
            let second: Vec<Symbol> = words.iter().map(|w| i.intern(w)).collect();
            check!("second pass hits every time", first == second, true);
            check!("len after two passes", i.len(), 5_000);
        }
        """,
    ],
    wrong=dict(
        no_dedup=sub(INTERN, "if let Some(&sym) = self.map.get(s) {\n            return sym;\n        }\n", ""),
        starts_at_one=sub(INTERN, "let sym = Symbol(self.strings.len() as u32);", "let sym = Symbol(self.strings.len() as u32 + 1);"),
        trims_input=sub(INTERN, "pub fn intern(&mut self, s: &str) -> Symbol {\n", "pub fn intern(&mut self, s: &str) -> Symbol {\n        let s = s.trim();\n"),
        get_creates=sub(INTERN, "pub fn get(&self, s: &str) -> Option<Symbol> {\n        self.map.get(s).copied()\n    }", "pub fn get(&self, s: &str) -> Option<Symbol> {\n        self.map.get(s).copied().or(Some(Symbol(0)))\n    }"),
    ),
    hints=[("approach", "Two structures: a `Vec` of strings indexed by the symbol, and a map from string to symbol. A symbol is just the length of the `Vec` at the moment the string was first seen."),
           ("rust", "`HashMap<Rc<str>, Symbol>` lets `map.get(s)` take a plain `&str`, because `Rc<str>: Borrow<str>`. Keep one `Rc<str>` in the `Vec` and a clone of it in the map so the text exists once."),
           ("edge", "Check the map before allocating anything: the common call is a hit. Build the `Rc<str>` only on a miss.")],
    notes=("An interner trades one hash lookup at creation for O(1) integer equality and tiny, `Copy` identifiers afterwards. rustc's `Symbol` is exactly this (plus a global table); `string-cache`, `lasso` and `ustr` are the crates. The cost to know: interned text lives as long as the interner, so it suits identifiers, not unbounded user input.", "O(len) per intern", "O(total distinct bytes)"),
    follow_up="1) Make `resolve` return `&'static str` by leaking an arena: when is that acceptable (a compiler run) and when not (a server)? 2) Replace `Rc<str>` with one big `String` plus `(start, len)` spans and show it stores each string once in a single allocation. 3) Make it thread safe with a sharded `RwLock` map: where does contention appear? 4) Why does `Symbol(u32)` plus `Option` still fit in 8 bytes, and how would `NonZeroU32` make `Option<Symbol>` 4 bytes? 5) How do `lasso`'s `RodeoReader`/`Rodeo` split read-mostly and build phases?",
    related=["F2", "F4", "D6"],
))

# ---------------------------------------------------------------------------------------------------------------
SLAB = """
/// A handle to a slot: the slot number plus the generation it had when it was handed out.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Key {
    pub idx: u32,
    pub gen: u32,
}

enum Slot<T> {
    Full(T),
    Empty { next_free: Option<u32> },
}

struct Entry<T> {
    gen: u32,
    slot: Slot<T>,
}

/// Values in a `Vec`, addressed by `Key`. Freed slots are reused (last freed, first reused); each reuse bumps the
/// slot's generation, so a key to a removed value can never reach the value that took its place.
pub struct Slab<T> {
    entries: Vec<Entry<T>>,
    free_head: Option<u32>,
    len: usize,
}

impl<T> Slab<T> {
    pub fn new() -> Self {
        Slab { entries: Vec::new(), free_head: None, len: 0 }
    }

    pub fn insert(&mut self, value: T) -> Key {
        self.len += 1;
        match self.free_head {
            Some(idx) => {
                let e = &mut self.entries[idx as usize];
                let Slot::Empty { next_free } = e.slot else { unreachable!("free list points at a full slot") };
                self.free_head = next_free;
                e.slot = Slot::Full(value);
                Key { idx, gen: e.gen }
            }
            None => {
                let idx = self.entries.len() as u32;
                self.entries.push(Entry { gen: 0, slot: Slot::Full(value) });
                Key { idx, gen: 0 }
            }
        }
    }

    pub fn get(&self, key: Key) -> Option<&T> {
        match self.entries.get(key.idx as usize) {
            Some(Entry { gen, slot: Slot::Full(v) }) if *gen == key.gen => Some(v),
            _ => None,
        }
    }

    pub fn get_mut(&mut self, key: Key) -> Option<&mut T> {
        match self.entries.get_mut(key.idx as usize) {
            Some(Entry { gen, slot: Slot::Full(v) }) if *gen == key.gen => Some(v),
            _ => None,
        }
    }

    pub fn remove(&mut self, key: Key) -> Option<T> {
        let e = self.entries.get_mut(key.idx as usize)?;
        if e.gen != key.gen || matches!(e.slot, Slot::Empty { .. }) {
            return None;
        }
        let old = std::mem::replace(&mut e.slot, Slot::Empty { next_free: self.free_head });
        e.gen += 1; // every key handed out so far for this slot is now stale
        self.free_head = Some(key.idx);
        self.len -= 1;
        match old {
            Slot::Full(v) => Some(v),
            Slot::Empty { .. } => unreachable!(),
        }
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Slots ever allocated (full or free). Reuse keeps this from growing under churn.
    pub fn slots(&self) -> usize {
        self.entries.len()
    }

    /// Live entries in slot order.
    pub fn iter(&self) -> impl Iterator<Item = (Key, &T)> {
        self.entries.iter().enumerate().filter_map(|(i, e)| match &e.slot {
            Slot::Full(v) => Some((Key { idx: i as u32, gen: e.gen }, v)),
            Slot::Empty { .. } => None,
        })
    }
}
"""

SLAB_STARTER = """
/// A handle to a slot: the slot number plus the generation it had when it was handed out.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Key {
    pub idx: u32,
    pub gen: u32,
}

/// Values in a `Vec`, addressed by `Key`. Freed slots are reused (last freed, first reused); each reuse bumps the
/// slot's generation, so a key to a removed value can never reach the value that took its place.
pub struct Slab<T> {
    // TODO: entries with a generation each, a free list threaded through the empty ones, and a live count
    _marker: std::marker::PhantomData<T>,
}

impl<T> Slab<T> {
    pub fn new() -> Self {
        todo!()
    }

    pub fn insert(&mut self, value: T) -> Key {
        todo!()
    }

    /// `None` for a key whose value was removed, even if its slot has been reused since.
    pub fn get(&self, key: Key) -> Option<&T> {
        todo!()
    }

    pub fn get_mut(&mut self, key: Key) -> Option<&mut T> {
        todo!()
    }

    pub fn remove(&mut self, key: Key) -> Option<T> {
        todo!()
    }

    pub fn len(&self) -> usize {
        todo!()
    }

    pub fn is_empty(&self) -> bool {
        todo!()
    }

    /// Slots ever allocated (full or free). Reuse keeps this from growing under churn.
    pub fn slots(&self) -> usize {
        todo!()
    }

    /// Live entries in slot order.
    pub fn iter(&self) -> impl Iterator<Item = (Key, &T)> {
        std::iter::empty()
    }
}
"""

P.append(dict(
    slug="generational-slab", title="A slab with generational keys", level="medium", stage="arenas-and-pools",
    tags=["slab", "generational index", "free list", "ABA", "mem::replace", "let-else"],
    teaches=["Replacing pointers with `(index, generation)` keys: no lifetimes, no `Rc`, and a stale key is detected instead of aliasing a new value.",
             "A free list threaded through the empty slots themselves (an enum where `Empty` carries the next free index), so reuse costs no extra memory.",
             "The same ABA problem as lock-free stacks, solved the same way: a version counter next to the slot."],
    statement="""
        Implement `Slab<T>`: a `Vec` of slots addressed by `Key { idx, gen }`.

        `insert` reuses the most recently freed slot (LIFO) or appends a new one. `remove` returns the value and bumps the
        slot's generation. Every `Key` handed out earlier for that slot is now **stale**: `get`, `get_mut` and `remove` must
        return `None` for it, **even after the slot has been reused** by a different value. `len` counts live values,
        `slots` counts slots ever allocated (a churn workload must not grow it), and `iter` yields live entries in slot order.
    """,
    starter=SLAB_STARTER,
    solution=SLAB,
    visible=[
        T("insert_get", "two values", "{ let mut s = Slab::new(); let a = s.insert(\"a\"); let b = s.insert(\"b\"); (s.get(a).copied(), s.get(b).copied(), s.len()) }", "(Some(\"a\"), Some(\"b\"), 2)"),
        T("remove_returns_value", "remove once, twice", "{ let mut s = Slab::new(); let a = s.insert(10); (s.remove(a), s.remove(a), s.get(a).copied(), s.len()) }", "(Some(10), None, None, 0)"),
        T("slot_is_reused", "remove then insert", "{ let mut s = Slab::new(); let a = s.insert(1); s.remove(a); let b = s.insert(2); (a.idx == b.idx, s.slots()) }", "(true, 1)"),
        T("stale_key_is_dead_after_reuse", "old key must not see the new value", "{ let mut s = Slab::new(); let a = s.insert(1); s.remove(a); let b = s.insert(2); (a == b, s.get(a).copied(), s.get(b).copied(), s.remove(a)) }", "(false, None, Some(2), None)"),
        T("get_mut", "modify in place", "{ let mut s = Slab::new(); let a = s.insert(vec![1]); s.get_mut(a).unwrap().push(2); s.get(a).cloned() }", "Some(vec![1, 2])"),
    ],
    hidden=[
        T("lifo_reuse", "free 0 then 2, next two inserts take 2 then 0", "{ let mut s = Slab::new(); let k: Vec<Key> = (0..3).map(|i| s.insert(i)).collect(); s.remove(k[0]); s.remove(k[2]); let a = s.insert(10); let b = s.insert(11); let c = s.insert(12); (a.idx, b.idx, c.idx, s.slots()) }", "(2, 0, 3, 4)"),
        T("generation_bumps_every_reuse", "same slot four times", "{ let mut s = Slab::new(); let mut gens = vec![]; for i in 0..4 { let k = s.insert(i); gens.push(k.gen); s.remove(k); } gens }", "vec![0, 1, 2, 3]"),
        T("unknown_key", "a key from nowhere", "{ let mut s: Slab<i32> = Slab::new(); let k = Key { idx: 9, gen: 0 }; (s.get(k).copied(), s.get_mut(k).is_none(), s.remove(k)) }", "(None, true, None)"),
        T("double_remove_cannot_hit_the_new_value", "stale remove after reuse", "{ let mut s = Slab::new(); let a = s.insert(1); s.remove(a); let _b = s.insert(2); (s.remove(a), s.len()) }", "(None, 1)"),
        T("iter_in_slot_order", "live entries only", "{ let mut s = Slab::new(); let k: Vec<Key> = (0..5).map(|i| s.insert(i * 10)).collect(); s.remove(k[1]); s.remove(k[3]); s.iter().map(|(_, v)| *v).collect::<Vec<_>>() }", "vec![0, 20, 40]"),
        T("iter_keys_work", "keys from iter reach the values", "{ let mut s = Slab::new(); for i in 0..4 { s.insert(i); } let ok = s.iter().all(|(k, v)| s.get(k) == Some(v)); ok }", "true"),
        T("drops_removed_values_once", "an Rc's count tracks ownership", "{ let rc = std::rc::Rc::new(()); let mut s = Slab::new(); let k = s.insert(rc.clone()); let k2 = s.insert(rc.clone()); let before = std::rc::Rc::strong_count(&rc); drop(s.remove(k)); let mid = std::rc::Rc::strong_count(&rc); let _ = k2; drop(s); (before, mid, std::rc::Rc::strong_count(&rc)) }", "(3, 2, 1)"),
        """
        #[test]
        fn churn_does_not_grow() {
            let mut s = Slab::new();
            let mut live = Vec::new();
            for i in 0..1_000 {
                live.push(s.insert(i));
            }
            for round in 0..200 {
                for _ in 0..500 {
                    let k = live.swap_remove((round * 7) % live.len());
                    s.remove(k);
                }
                for i in 0..500 {
                    live.push(s.insert(i));
                }
            }
            check!("a thousand live values never need more than a thousand slots", (s.len(), s.slots()), (1_000, 1_000));
        }

        #[test]
        fn random_vs_model() {
            use std::collections::HashMap;
            let mut rng = anneal_prelude::Rng::new(6502);
            let mut s: Slab<u64> = Slab::new();
            let mut model: HashMap<Key, u64> = HashMap::new();
            let mut dead: Vec<Key> = Vec::new();
            for step in 0..20_000u64 {
                match rng.below(4) {
                    0 | 1 => {
                        let k = s.insert(step);
                        check!("a fresh key was not alive before", model.insert(k, step).is_none(), true);
                    }
                    2 if !model.is_empty() => {
                        let keys: Vec<Key> = model.keys().copied().collect();
                        let k = keys[rng.below(keys.len()) as usize];
                        check!("remove returns what was stored", s.remove(k), model.remove(&k));
                        dead.push(k);
                    }
                    _ if !dead.is_empty() => {
                        let k = dead[rng.below(dead.len()) as usize];
                        check!("a stale key finds nothing", (s.get(k).is_none(), s.remove(k)), (true, None));
                    }
                    _ => {}
                }
                check!("len matches the model", s.len(), model.len());
            }
            for (k, v) in &model {
                check!("every live key still reads its value", s.get(*k), Some(v));
            }
        }
        """,
    ],
    wrong=dict(
        no_generation_bump=sub(SLAB, "e.gen += 1; // every key handed out so far for this slot is now stale\n", ""),
        get_ignores_generation=sub(SLAB, "Some(Entry { gen, slot: Slot::Full(v) }) if *gen == key.gen => Some(v),\n            _ => None,\n        }\n    }\n\n    pub fn get_mut", "Some(Entry { slot: Slot::Full(v), .. }) => Some(v),\n            _ => None,\n        }\n    }\n\n    pub fn get_mut"),
        remove_ignores_generation=sub(SLAB, "if e.gen != key.gen || matches!(e.slot, Slot::Empty { .. }) {", "if matches!(e.slot, Slot::Empty { .. }) {"),
        never_reuses_slots=sub(SLAB, "self.free_head = Some(key.idx);\n        self.len -= 1;", "self.len -= 1;"),
        fifo_free_list=sub(SLAB, "let old = std::mem::replace(&mut e.slot, Slot::Empty { next_free: self.free_head });", "let old = std::mem::replace(&mut e.slot, Slot::Empty { next_free: self.free_head.filter(|_| false) });"),
    ),
    hints=[("approach", "Each entry is `{ gen: u32, slot: Full(T) | Empty { next_free: Option<u32> } }`. `free_head` is the most recently freed index. Insert pops the head (or pushes a new entry); remove replaces the slot with `Empty` pointing at the old head, bumps `gen`, and becomes the new head."),
           ("rust", "`std::mem::replace(&mut e.slot, Slot::Empty { .. })` takes the value out of a slot you only have `&mut` to. A key is valid iff the entry exists, is `Full`, and `entry.gen == key.gen`; a `match` with a guard says that in one arm."),
           ("edge", "Bump the generation when you free (not when you reuse) so even a key to the freed-but-not-yet-reused slot is already dead. Check the generation in `remove` too, or a stale remove deletes the new occupant.")],
    notes=("This is the core of `slotmap`, `generational-arena`, tokio's `slab` plus tags, and every ECS entity id (`bevy`'s `Entity` is index plus generation). It replaces raw references in graphs and game worlds: you get O(1) access, cache-friendly storage and memory safety without lifetimes. The generation is the ABA fix from lock-free stacks, applied to ids.", "O(1) per operation", "O(slots)"),
    follow_up="1) What happens when a generation overflows `u32`? Make it `NonZeroU32`, retire the slot after the last generation, and discuss why wrapping is a security bug. 2) Make `iter` faster after heavy removal (a bitmap of live slots, using the F5 bitset). 3) Add `Index<Key>` and `retain`. 4) Compare with `Vec<Option<T>>` plus a free `Vec<usize>`: what do you give up (the free list inside the slots) and gain? 5) Where does tokio use a slab for I/O registrations, and why does it add a generation tag to the token?",
    related=["F2", "D6", "C3"],
))

# ---------------------------------------------------------------------------------------------------------------
ARENA = """
use std::cell::RefCell;

/// Allocates values that live until the arena is dropped and hands out `&mut` to each, through `&self`.
/// Storage is a list of chunks that never reallocate once created, so earlier references stay valid while you
/// keep allocating. Each new chunk doubles the previous one.
pub struct Arena<T> {
    chunks: RefCell<Vec<Vec<T>>>,
}

impl<T> Arena<T> {
    pub fn new() -> Self {
        Arena { chunks: RefCell::new(Vec::new()) }
    }

    pub fn alloc(&self, value: T) -> &mut T {
        let mut chunks = self.chunks.borrow_mut();
        let full = match chunks.last() {
            Some(c) => c.len() == c.capacity(),
            None => true,
        };
        if full {
            let cap = chunks.last().map_or(4, |c| c.capacity() * 2);
            chunks.push(Vec::with_capacity(cap));
        }
        let chunk = chunks.last_mut().unwrap();
        chunk.push(value);
        let ptr: *mut T = chunk.last_mut().unwrap();
        // SAFETY: the chunk's buffer never reallocates (we only push while len < capacity) and is only freed when the
        // arena drops, which the returned lifetime (tied to `&self`) prevents from happening first.
        unsafe { &mut *ptr }
    }

    /// Values allocated so far.
    pub fn len(&self) -> usize {
        self.chunks.borrow().iter().map(|c| c.len()).sum()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn chunk_count(&self) -> usize {
        self.chunks.borrow().len()
    }
}
"""

ARENA_STARTER = """
/// Allocates values that live until the arena is dropped and hands out `&mut` to each, through `&self`.
/// Storage is a list of chunks that never reallocate once created, so earlier references stay valid while you
/// keep allocating. Each new chunk doubles the previous one (the first holds 4).
pub struct Arena<T> {
    // TODO: interior mutability, because `alloc` takes `&self`
    _marker: std::marker::PhantomData<T>,
}

impl<T> Arena<T> {
    pub fn new() -> Self {
        todo!()
    }

    pub fn alloc(&self, value: T) -> &mut T {
        todo!()
    }

    /// Values allocated so far.
    pub fn len(&self) -> usize {
        todo!()
    }

    pub fn is_empty(&self) -> bool {
        todo!()
    }

    pub fn chunk_count(&self) -> usize {
        todo!()
    }
}
"""

P.append(dict(
    slug="typed-arena", title="A typed arena that hands out references through &self", level="hard", stage="arena-and-allocator",
    tags=["arena", "unsafe", "lifetimes", "interior mutability", "RefCell", "drop order", "typed-arena"],
    teaches=["Why `fn alloc(&self, v: T) -> &mut T` is sound only because the storage never moves: the invariant that justifies one `unsafe` block.",
             "Chunked growth (double each time) as the way to keep addresses stable while amortising allocation: the design of `typed-arena` and `bumpalo`.",
             "Building cyclic graphs with plain `&'a Node<'a>` references and `Cell`, which an arena makes possible and `Box` cannot."],
    statement="""
        Implement `Arena<T>`, the allocator behind compiler ASTs and graphs. `alloc(&self, value)` stores the value and returns
        `&mut T` that lives as long as the **borrow of the arena**, so you may keep thousands of those references at once while
        allocating more.

        That only works if values never move, so a `Vec<T>` that reallocates is not an option. Keep a list of chunks: allocate
        into the last chunk while it has room, else start a new chunk with **double** the previous capacity (the first holds 4).
        `len` counts values, `chunk_count` the chunks. Dropping the arena drops every value exactly once and not before.

        Write the `unsafe` block with a `// SAFETY:` comment that states the invariant it relies on.
    """,
    starter=ARENA_STARTER,
    solution=ARENA,
    visible=[
        T("alloc_and_read", "three values", "{ let a = Arena::new(); let x = a.alloc(1); let y = a.alloc(2); let z = a.alloc(3); (*x, *y, *z, a.len()) }", "(1, 2, 3, 3)"),
        T("mutate_through_the_reference", "write after alloc", "{ let a = Arena::new(); let x = a.alloc(String::from(\"a\")); x.push('b'); x.clone() }", "\"ab\".to_string()"),
        T("empty", "fresh arena", "{ let a: Arena<u8> = Arena::new(); (a.len(), a.is_empty(), a.chunk_count()) }", "(0, true, 0)"),
        T("first_chunk_holds_four", "4 values then a 5th", "{ let a = Arena::new(); for i in 0..4 { a.alloc(i); } let c4 = a.chunk_count(); a.alloc(4); (c4, a.chunk_count()) }", "(1, 2)"),
        T("many_references_alive_together", "10k refs held across later allocations", "{ let a = Arena::new(); let refs: Vec<&mut u64> = (0..10_000u64).map(|i| a.alloc(i)).collect(); refs.iter().map(|r| **r).sum::<u64>() }", "49_995_000"),
    ],
    hidden=[
        T("chunks_double", "chunk count for 4 + 8 + 16 + 32 values", "{ let a = Arena::new(); for i in 0..60 { a.alloc(i); } a.chunk_count() }", "4"),
        T("chunks_grow_geometrically", "100k values need few chunks", "{ let a = Arena::new(); for i in 0..100_000u32 { a.alloc(i); } (a.len(), a.chunk_count() <= 16) }", "(100_000, true)"),
        T("references_stay_valid_after_growth", "the very first ref after 50k more allocs", "{ let a = Arena::new(); let first = a.alloc(7u64); for i in 0..50_000u64 { a.alloc(i); } *first += 1; *first }", "8"),
        T("strings_keep_their_heap_data", "values with their own allocations", "{ let a = Arena::new(); let refs: Vec<&mut String> = (0..1000).map(|i| a.alloc(format!(\"item-{i}\"))).collect(); (refs[0].clone(), refs[999].clone(), refs.iter().map(|s| s.len()).sum::<usize>()) }", "(\"item-0\".to_string(), \"item-999\".to_string(), 7890)"),
        T("zero_sized_values", "ZSTs are allowed", "{ let a = Arena::new(); for _ in 0..100 { a.alloc(()); } a.len() }", "100"),
        """
        #[test]
        fn drops_every_value_once_and_only_when_the_arena_goes() {
            use std::cell::Cell;
            use std::rc::Rc;
            struct D(Rc<Cell<usize>>);
            impl Drop for D {
                fn drop(&mut self) {
                    self.0.set(self.0.get() + 1);
                }
            }
            let count = Rc::new(Cell::new(0));
            {
                let a = Arena::new();
                for _ in 0..1_000 {
                    a.alloc(D(count.clone()));
                }
                check!("nothing dropped while the arena lives", count.get(), 0);
            }
            check!("all 1000 dropped exactly once", count.get(), 1_000);
        }

        #[test]
        fn graph_with_cycles_and_plain_references() {
            use std::cell::Cell;
            struct Node<'a> {
                id: u32,
                next: Cell<Option<&'a Node<'a>>>,
            }
            let arena = Arena::new();
            let nodes: Vec<&Node> = (0..100).map(|id| &*arena.alloc(Node { id, next: Cell::new(None) })).collect();
            for i in 0..100 {
                nodes[i].next.set(Some(nodes[(i + 1) % 100])); // a ring: every node points at the next, the last at the first
            }
            let mut cur = nodes[0];
            let mut ids = Vec::new();
            for _ in 0..205 {
                ids.push(cur.id);
                cur = cur.next.get().unwrap();
            }
            check!("walking the ring wraps around", (ids[0], ids[99], ids[100], ids[204]), (0, 99, 0, 4));
        }

        #[test]
        fn nested_arenas_hold_references_into_each_other() {
            let strings: Arena<String> = Arena::new();
            let views: Arena<&str> = Arena::new();
            let mut refs: Vec<&&str> = Vec::new();
            for i in 0..50 {
                let s: &String = strings.alloc(format!("s{i}"));
                refs.push(&*views.alloc(s.as_str()));
            }
            check!("a view into a string from another arena", (*refs[0], *refs[49]), ("s0", "s49"));
        }
        """,
    ],
    wrong=dict(
        leaks_values=sub(ARENA, "let ptr: *mut T = chunk.last_mut().unwrap();", "let ptr: *mut T = Box::leak(Box::new(chunk.pop().unwrap()));\n        chunk.reserve(0);\n        let _ = &chunk;"),
        fixed_size_chunks=sub(ARENA, "chunks.last().map_or(4, |c| c.capacity() * 2)", "4"),
        len_counts_chunks=sub(ARENA, "self.chunks.borrow().iter().map(|c| c.len()).sum()", "self.chunks.borrow().len()"),
    ),
    hints=[("approach", "`RefCell<Vec<Vec<T>>>`. To allocate: if the last chunk is full (or there is none) push a `Vec::with_capacity(previous * 2)` (4 the first time); then `push` the value into the last chunk and take a raw pointer to it."),
           ("rust", "`let ptr: *mut T = chunk.last_mut().unwrap();` then `unsafe { &mut *ptr }` detaches the reference from the `RefCell` borrow. It is sound because pushing into a chunk with spare capacity never reallocates, and the outer `Vec<Vec<T>>` only moves the chunk headers, not their heap buffers."),
           ("edge", "Do not implement `Drop` yourself: the field's own drop glue drops every value once, which keeps the cyclic-graph test compiling (a `Drop` impl would demand `T` outlive the arena). Compare `len() == capacity()` to decide when a chunk is full, never `len() >= some constant`.")],
    notes=("`typed-arena` is exactly this (plus `iter_mut` and `alloc_extend`); `bumpalo` generalises it to mixed types by handing out bytes from one bump pointer. Arenas win when many objects share one lifetime: an AST per compilation, a request's scratch data, a graph. Allocation is a pointer bump, deallocation is free, locality is good, and `&'a` references make cyclic structures expressible in safe code.", "O(1) amortised per alloc", "O(n) plus up to 2x slack in the last chunk"),
    follow_up="1) Why must `alloc` take `&self`, and what stops you from calling `arena.alloc(...)` while holding the `RefCell` borrow on re-entry (e.g. a `T::clone` that allocates)? 2) Show the use-after-free that a single `Vec<T>` would cause with a test under Miri. 3) `typed-arena` needs a nightly `#[may_dangle]` to allow cycles with `Drop` types: explain dropck. 4) Make a `Bump` that allocates mixed types from one buffer with correct alignment (`Layout::pad_to_align`). 5) Why is `Arena<T>` not `Sync`, and what would a sharded per-thread arena look like?",
    related=["Y2", "F2", "D6"],
))

# ---------------------------------------------------------------------------------------------------------------
BUDDY = """
use std::collections::BTreeSet;

/// A buddy allocator over `2^max_order` units. Blocks are powers of two and aligned to their size, so a block's
/// buddy is found by flipping one bit of its offset.
pub struct Buddy {
    max_order: u32,
    free: Vec<BTreeSet<usize>>, // free[k]: offsets of free blocks of 2^k units
}

fn order_for(size: usize) -> u32 {
    size.max(1).next_power_of_two().trailing_zeros()
}

impl Buddy {
    pub fn new(max_order: u32) -> Self {
        let mut free = vec![BTreeSet::new(); max_order as usize + 1];
        free[max_order as usize].insert(0);
        Buddy { max_order, free }
    }

    /// Offset of a block of at least `size` units (rounded up to a power of two, 0 counts as 1), lowest address first.
    pub fn alloc(&mut self, size: usize) -> Option<usize> {
        if size > (1usize << self.max_order) {
            return None;
        }
        let want = order_for(size);
        let mut k = (want..=self.max_order).find(|&k| !self.free[k as usize].is_empty())?;
        let off = self.free[k as usize].pop_first().unwrap();
        while k > want {
            k -= 1;
            self.free[k as usize].insert(off + (1 << k)); // keep the lower half, free the upper half
        }
        Some(off)
    }

    /// Return a block obtained from `alloc(size)` (the same `size`). Merges with its buddy while the buddy is free.
    pub fn free(&mut self, offset: usize, size: usize) {
        let mut off = offset;
        let mut k = order_for(size);
        while k < self.max_order {
            let buddy = off ^ (1 << k);
            if !self.free[k as usize].remove(&buddy) {
                break;
            }
            off = off.min(buddy);
            k += 1;
        }
        self.free[k as usize].insert(off);
    }

    /// Total units currently free.
    pub fn free_units(&self) -> usize {
        self.free.iter().enumerate().map(|(k, s)| s.len() << k).sum()
    }

    /// The largest block `alloc` could hand out right now, in units (0 if none).
    pub fn largest_free(&self) -> usize {
        (0..=self.max_order).rev().find(|&k| !self.free[k as usize].is_empty()).map_or(0, |k| 1 << k)
    }
}
"""

BUDDY_STARTER = """
/// A buddy allocator over `2^max_order` units. Blocks are powers of two and aligned to their size, so a block's
/// buddy is found by flipping one bit of its offset.
pub struct Buddy {
    // TODO: a set of free offsets per order
}

impl Buddy {
    pub fn new(max_order: u32) -> Self {
        todo!()
    }

    /// Offset of a block of at least `size` units (rounded up to a power of two, 0 counts as 1), lowest address first.
    /// `None` if nothing big enough is free.
    pub fn alloc(&mut self, size: usize) -> Option<usize> {
        todo!()
    }

    /// Return a block obtained from `alloc(size)` (the same `size`). Merges with its buddy while the buddy is free.
    pub fn free(&mut self, offset: usize, size: usize) {
        todo!()
    }

    /// Total units currently free.
    pub fn free_units(&self) -> usize {
        todo!()
    }

    /// The largest block `alloc` could hand out right now, in units (0 if none).
    pub fn largest_free(&self) -> usize {
        todo!()
    }
}
"""

P.append(dict(
    slug="buddy-allocator", title="A buddy allocator with coalescing", level="hard", stage="arena-and-allocator",
    tags=["buddy allocator", "power of two", "XOR buddy", "free lists", "BTreeSet", "fragmentation", "kernel"],
    teaches=["Power-of-two blocks aligned to their size: the buddy of the block at `off` of order `k` is at `off ^ (1 << k)`, one XOR.",
             "Split on allocation, merge on free: how the Linux page allocator keeps large blocks available without scanning memory.",
             "External fragmentation made visible: `free_units` can be large while `largest_free` is small."],
    statement="""
        Implement a buddy allocator over `2^max_order` abstract units (offsets, not real memory).

        `alloc(size)` rounds `size` up to a power of two (0 counts as 1), takes the **lowest-addressed** free block of the
        smallest sufficient order, splits it down to the wanted order (keeping the lower half, freeing the upper half at each
        level) and returns its offset, or `None`. `free(offset, size)` returns a block and **coalesces**: while the block's
        buddy (`offset ^ block_size`) is free, merge them into the next order. `free_units` and `largest_free` report the state.

        Offsets of an order-`k` block are always multiples of `2^k`, and live blocks never overlap.
    """,
    starter=BUDDY_STARTER,
    solution=BUDDY,
    visible=[
        T("fresh_allocator", "one big free block", "{ let b = Buddy::new(4); (b.free_units(), b.largest_free()) }", "(16, 16)"),
        T("first_allocations_are_lowest", "1, 1, 2, 4 units in 8", "{ let mut b = Buddy::new(3); [b.alloc(1), b.alloc(1), b.alloc(2), b.alloc(4)] }", "[Some(0), Some(1), Some(2), Some(4)]"),
        T("rounds_up_to_power_of_two", "5 units takes 8", "{ let mut b = Buddy::new(4); let a = b.alloc(5); (a, b.free_units()) }", "(Some(0), 8)"),
        T("exhaustion", "everything allocated", "{ let mut b = Buddy::new(2); let all = b.alloc(4); (all, b.alloc(1), b.free_units()) }", "(Some(0), None, 0)"),
        T("free_coalesces_back", "alloc 1, free it, alloc the whole range", "{ let mut b = Buddy::new(3); let a = b.alloc(1).unwrap(); b.free(a, 1); (b.largest_free(), b.alloc(8)) }", "(8, Some(0))"),
    ],
    hidden=[
        T("too_big", "larger than the region", "{ let mut b = Buddy::new(3); (b.alloc(9), b.alloc(8), b.alloc(usize::MAX)) }", "(None, Some(0), None)"),
        T("zero_counts_as_one", "alloc(0)", "{ let mut b = Buddy::new(2); (b.alloc(0), b.alloc(0), b.free_units()) }", "(Some(0), Some(1), 2)"),
        T("partial_coalescing", "free only one of two buddies", "{ let mut b = Buddy::new(2); let a = b.alloc(1).unwrap(); let c = b.alloc(1).unwrap(); b.free(a, 1); (b.largest_free(), b.free_units(), c) }", "(2, 3, 1)"),
        T("buddy_of_a_right_hand_block", "free the right half first, then the left", "{ let mut b = Buddy::new(2); let l = b.alloc(2).unwrap(); let r = b.alloc(2).unwrap(); b.free(r, 2); let mid = b.largest_free(); b.free(l, 2); (r, mid, b.largest_free()) }", "(2, 2, 4)"),
        T("cascading_merge", "merge climbs several orders", "{ let mut b = Buddy::new(4); let v: Vec<usize> = (0..16).map(|_| b.alloc(1).unwrap()).collect(); for o in v { b.free(o, 1); } (b.largest_free(), b.alloc(16)) }", "(16, Some(0))"),
        T("fragmentation_is_visible", "free every other unit", "{ let mut b = Buddy::new(3); let v: Vec<usize> = (0..8).map(|_| b.alloc(1).unwrap()).collect(); for o in v.iter().step_by(2) { b.free(*o, 1); } (b.free_units(), b.largest_free(), b.alloc(2)) }", "(4, 1, None)"),
        T("lowest_address_first", "two free blocks of the same order", "{ let mut b = Buddy::new(3); let v: Vec<usize> = (0..4).map(|_| b.alloc(2).unwrap()).collect(); b.free(v[2], 2); b.free(v[0], 2); (b.alloc(2), b.alloc(2), b.alloc(2)) }", "(Some(0), Some(4), None)"),
        T("alignment", "an order-k block is k-aligned", "{ let mut b = Buddy::new(6); let a = b.alloc(1).unwrap(); let c = b.alloc(8).unwrap(); let d = b.alloc(4).unwrap(); let e = b.alloc(16).unwrap(); (a % 1, c % 8, d % 4, e % 16) }", "(0, 0, 0, 0)"),
        """
        #[test]
        fn random_vs_a_unit_bitmap() {
            let mut rng = anneal_prelude::Rng::new(6503);
            let order = 9u32;
            let total = 1usize << order;
            let mut b = Buddy::new(order);
            let mut used = vec![false; total];
            let mut live: Vec<(usize, usize)> = Vec::new(); // (offset, requested size)
            for _ in 0..20_000 {
                if live.is_empty() || rng.below(100) < 55 {
                    let size = 1 + rng.below(40) as usize;
                    let block = size.next_power_of_two();
                    let got = b.alloc(size);
                    // the model: a block exists iff some aligned run of `block` free units exists
                    let exists = (0..total).step_by(block).any(|s| used[s..s + block].iter().all(|u| !u));
                    check!("alloc succeeds iff an aligned free block of that size exists", got.is_some(), exists);
                    if let Some(off) = got {
                        check!("the block is aligned to its size", off % block, 0);
                        check!("the block does not overlap live ones", used[off..off + block].iter().any(|u| *u), false);
                        for u in &mut used[off..off + block] {
                            *u = true;
                        }
                        live.push((off, size));
                    }
                } else {
                    let (off, size) = live.swap_remove(rng.below(live.len()) as usize);
                    let block = size.next_power_of_two();
                    b.free(off, size);
                    for u in &mut used[off..off + block] {
                        *u = false;
                    }
                }
                let free = used.iter().filter(|u| !**u).count();
                check!("free_units matches the bitmap", b.free_units(), free);
            }
            for (off, size) in live {
                b.free(off, size);
            }
            check!("everything freed coalesces into one block", (b.free_units(), b.largest_free()), (total, total));
        }
        """,
    ],
    wrong=dict(
        no_coalescing=sub(BUDDY, "if !self.free[k as usize].remove(&buddy) {\n                break;\n            }\n            off = off.min(buddy);\n            k += 1;", "break;"),
        buddy_is_next_block=sub(BUDDY, "let buddy = off ^ (1 << k);", "let buddy = off + (1 << k);"),
        rounds_down=sub(BUDDY, "size.max(1).next_power_of_two().trailing_zeros()", "(usize::BITS - 1 - size.max(1).leading_zeros())"),
        split_leaks_the_upper_half=sub(BUDDY, "self.free[k as usize].insert(off + (1 << k)); // keep the lower half, free the upper half", "let _ = off + (1 << k);"),
        takes_the_highest_block=sub(BUDDY, "let off = self.free[k as usize].pop_first().unwrap();", "let off = self.free[k as usize].pop_last().unwrap();"),
    ),
    hints=[("approach", "`free[k]` is a `BTreeSet` of offsets of free blocks of `2^k` units; start with `free[max_order] = {0}`. Alloc: find the smallest `k >= want` with a free block, `pop_first`, then while `k > want` drop one order and push the upper half `off + (1 << k)` onto `free[k]`."),
           ("rust", "`size.max(1).next_power_of_two().trailing_zeros()` is the order. Free: loop `buddy = off ^ (1 << k)`; if `free[k].remove(&buddy)` succeeded the pair merges (`off = off.min(buddy)`, `k += 1`); otherwise insert `off` at order `k` and stop."),
           ("edge", "Stop merging at `max_order` (the whole region has no buddy). `alloc(size)` with `size > 2^max_order` must be `None` before `next_power_of_two` can overflow.")],
    notes=("Linux's page allocator is a buddy system per zone and order (`/proc/buddyinfo` prints the free-list lengths); `jemalloc`/`tcmalloc` use size classes instead because buddy rounding wastes up to half of each block (internal fragmentation) and large frees need the buddy to be free to merge. Kernel code likes it because splitting and merging are a handful of bit operations with no searching.", "O(max_order * log n) per operation", "O(number of free blocks)"),
    follow_up="1) Compute the worst-case internal fragmentation of this allocator and how a slab (the first problem in this track) layers on top to fix it for small objects. 2) Replace each `BTreeSet` with an intrusive free list stored inside the free blocks, plus a bitmap per order for the buddy test. 3) Add `realloc(offset, old, new)` that grows in place when the buddy chain allows it. 4) Detect double free and a size that does not match the allocation: what extra metadata do you need? 5) Read `/proc/buddyinfo` on a Linux box and explain what a lot of free order-0 blocks and no order-10 blocks means for huge pages.",
    related=["F2", "F5", "Y2"],
))

STAGES = [("arenas-and-pools", "Arenas and pools", "medium"), ("arena-and-allocator", "Build an allocator", "hard")]

if __name__ == "__main__":
    n = write_track("f3-memory-allocation", "F3", "Memory & allocation", "F", "sde3", 3,
                    "Where memory comes from and how to hand it out: interning, generational slabs, a typed arena and a buddy allocator.",
                    STAGES, P)
    print("F3", n)
