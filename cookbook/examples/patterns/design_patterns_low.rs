//! RECIPE 214 (patterns): low-level, data-oriented patterns: arenas, interning, SoA, zero-copy, pools, bitfields, inline storage.
//! LEVEL: advanced
//! Use in: compilers, game/ECS code, parsers, databases, network stacks, anything where allocation count and cache lines matter.   Run: cargo run --release --example design_patterns_low
#![allow(dead_code)]
//!
//! Idea: replace pointer-chasing object graphs with flat arrays and small integers. Benefits: fewer allocations, better cache
//! locality, trivially `Copy` handles, no lifetimes or `Rc<RefCell<..>>` tangles, and the borrow checker stays happy because
//! relationships are INDICES, not references. Every claim below is asserted (`size_of`, counted allocations), not asserted in prose.
//!
//!  1. arena + typed indices   `IdxVec<I, T>`: a `Vec` you can only index with the right newtype (no mixing node ids and type ids)
//!  2. string interning        `Symbol(u32)`: O(1) equality/hash, one allocation per distinct string
//!  3. struct-of-arrays        columns with a row-view accessor (scan one field without dragging the others through cache)
//!  4. zero-copy view          `Header<'a>` borrows from `&'a [u8]`: parsing allocates nothing
//!  5. index-linked list       doubly linked list inside a `Vec` with a free list (stable ids, no `unsafe`, no `Rc`)
//!  6. inline small vector     enum of `[T; N]` inline or heap `Vec`: no allocation until it spills
//!  7. object pool + RAII      buffers go back to the pool on `Drop`; allocation count stays flat
//!  8. bitfield packing        several small fields in one `u32` with masks and shifts
//!  9. `Cow`                   borrow when unchanged, allocate only when modified
//! 10. tagged-union enum vs `Box<dyn Trait>`   one allocation for N values vs N allocations (plus `#[repr(u8)]` tags)
//! 11. split borrows by index  two `&mut` into one `Vec` via `split_at_mut` instead of `unsafe` or `RefCell`
//!
//! COMMON USES: rustc/rust-analyzer style id-indexed IRs and symbol interners, ECS and game engines (struct-of-arrays), database
//! columnar engines, zero-copy packet/record parsing, LRU caches, buffer pools in servers, `smallvec`-style inline storage.
//! Read: https://nnethercote.github.io/perf-book/ , https://doc.rust-lang.org/std/borrow/enum.Cow.html , https://docs.rs/smallvec , https://docs.rs/bumpalo
use std::alloc::{GlobalAlloc, Layout, System};
use std::borrow::Cow;
use std::cell::RefCell;
use std::collections::HashMap;
use std::hint::black_box;
use std::marker::PhantomData;
use std::mem::size_of;
use std::ops::{Deref, DerefMut, Index, IndexMut};
use std::rc::Rc;
use std::sync::atomic::{AtomicUsize, Ordering::Relaxed};

// ------------------------------------------------------ counting allocator
struct Counting;
static ALLOCS: AtomicUsize = AtomicUsize::new(0);
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, l: Layout) -> *mut u8 {
        ALLOCS.fetch_add(1, Relaxed);
        // SAFETY: forwarding the caller's layout unchanged to the system allocator.
        unsafe { System.alloc(l) }
    }
    unsafe fn dealloc(&self, p: *mut u8, l: Layout) {
        // SAFETY: `p` was allocated by `System` with layout `l` in `alloc`/`realloc` above.
        unsafe { System.dealloc(p, l) }
    }
    unsafe fn realloc(&self, p: *mut u8, l: Layout, n: usize) -> *mut u8 {
        ALLOCS.fetch_add(1, Relaxed);
        // SAFETY: same pointer/layout contract as `dealloc`; `n` forwarded unchanged.
        unsafe { System.realloc(p, l, n) }
    }
}
#[global_allocator]
static GLOBAL: Counting = Counting;
/// Allocation calls made by `f` (do not print inside `f`).
fn allocs<R>(f: impl FnOnce() -> R) -> usize {
    let a0 = ALLOCS.load(Relaxed);
    black_box(f());
    ALLOCS.load(Relaxed) - a0
}

// ------------------------------------------------ 1. arena + typed indices
trait Idx: Copy {
    fn new(i: usize) -> Self;
    fn get(self) -> usize;
}
macro_rules! idx_type {
    ($name:ident) => {
        #[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
        struct $name(u32);
        impl Idx for $name {
            fn new(i: usize) -> Self {
                $name(u32::try_from(i).expect("index overflow"))
            }
            fn get(self) -> usize {
                self.0 as usize
            }
        }
    };
}
struct IdxVec<I: Idx, T> {
    items: Vec<T>,
    _m: PhantomData<fn(I)>,
}
impl<I: Idx, T> Default for IdxVec<I, T> {
    fn default() -> Self {
        IdxVec {
            items: Vec::new(),
            _m: PhantomData,
        }
    }
}
impl<I: Idx, T> IdxVec<I, T> {
    fn push(&mut self, t: T) -> I {
        self.items.push(t);
        I::new(self.items.len() - 1)
    }
    fn len(&self) -> usize {
        self.items.len()
    }
    fn iter_enumerated(&self) -> impl Iterator<Item = (I, &T)> {
        self.items.iter().enumerate().map(|(i, t)| (I::new(i), t))
    }
}
impl<I: Idx, T> Index<I> for IdxVec<I, T> {
    type Output = T;
    fn index(&self, i: I) -> &T {
        &self.items[i.get()]
    }
}
impl<I: Idx, T> IndexMut<I> for IdxVec<I, T> {
    fn index_mut(&mut self, i: I) -> &mut T {
        &mut self.items[i.get()]
    }
}
idx_type!(NodeId);
idx_type!(TypeId);
/// A tree whose edges are `NodeId`s: no `Box`, no `Rc<RefCell>`, cycles/parent links are free, one allocation (amortised).
struct Node {
    ty: TypeId,
    parent: Option<NodeId>,
    children: Vec<NodeId>,
}
fn demo_arena() {
    let mut types: IdxVec<TypeId, &str> = IdxVec::default();
    let int = types.push("int");
    let mut nodes: IdxVec<NodeId, Node> = IdxVec::default();
    let root = nodes.push(Node {
        ty: int,
        parent: None,
        children: vec![],
    });
    for _ in 0..3 {
        let c = nodes.push(Node {
            ty: int,
            parent: Some(root),
            children: vec![],
        });
        nodes[root].children.push(c);
    }
    let kid = nodes[root].children[1];
    assert_eq!(nodes[kid].parent, Some(root));
    assert_eq!(types[nodes[kid].ty], "int");
    // `nodes[int]` would not compile: TypeId is not NodeId. That is the whole point of the newtype.
    assert_eq!(nodes.iter_enumerated().count(), nodes.len());
    assert_eq!(size_of::<NodeId>(), 4);
    assert_eq!(
        size_of::<Option<NodeId>>(),
        8,
        "u32 has no niche: Option doubles it (use NonZeroU32 to fix)"
    );
    assert_eq!(size_of::<usize>(), 8);
}

// ----------------------------------------------------- 2. string interning
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
struct Symbol(u32);
#[derive(Default)]
struct Interner {
    map: HashMap<Rc<str>, Symbol>,
    names: Vec<Rc<str>>,
}
impl Interner {
    fn intern(&mut self, s: &str) -> Symbol {
        if let Some(&sym) = self.map.get(s) {
            return sym; // hit: zero allocations
        }
        let rc: Rc<str> = Rc::from(s);
        let sym = Symbol(self.names.len() as u32);
        self.names.push(rc.clone());
        self.map.insert(rc, sym);
        sym
    }
    fn resolve(&self, s: Symbol) -> &str {
        &self.names[s.0 as usize]
    }
}
fn demo_intern() {
    let mut i = Interner::default();
    let a = i.intern("identifier_with_long_name");
    let b = i.intern("other");
    assert_ne!(a, b);
    let n = allocs(|| {
        for _ in 0..1000 {
            assert_eq!(i.intern("identifier_with_long_name"), a); // equality is a u32 compare
        }
    });
    assert_eq!(n, 0, "re-interning an existing string allocates nothing");
    assert_eq!(i.resolve(b), "other");
    assert_eq!(size_of::<Symbol>(), 4);
    assert_eq!(size_of::<String>(), 24);
}

// ---------------------------------------------------- 3. struct-of-arrays
#[derive(Default)]
struct Particles {
    x: Vec<f32>,
    y: Vec<f32>,
    mass: Vec<f32>,
    alive: Vec<bool>,
}
struct Row<'a> {
    x: &'a f32,
    y: &'a f32,
    mass: &'a f32,
}
struct RowMut<'a> {
    x: &'a mut f32,
    y: &'a mut f32,
}
impl Particles {
    fn push(&mut self, x: f32, y: f32, mass: f32) -> usize {
        self.x.push(x);
        self.y.push(y);
        self.mass.push(mass);
        self.alive.push(true);
        self.x.len() - 1
    }
    fn row(&self, i: usize) -> Row<'_> {
        Row {
            x: &self.x[i],
            y: &self.y[i],
            mass: &self.mass[i],
        }
    }
    fn row_mut(&mut self, i: usize) -> RowMut<'_> {
        RowMut {
            x: &mut self.x[i],
            y: &mut self.y[i],
        } // disjoint fields: the borrow checker allows both
    }
}
struct ParticleAos {
    x: f32,
    y: f32,
    mass: f32,
    alive: bool,
}
fn demo_soa() {
    let mut p = Particles::default();
    for i in 0..100 {
        p.push(i as f32, 2.0 * i as f32, 1.0);
    }
    let r = p.row_mut(10);
    *r.x += 0.5;
    *r.y += 0.5;
    let r = p.row(10);
    assert_eq!((*r.x, *r.y, *r.mass), (10.5, 20.5, 1.0));
    let sum: f32 = p.x.iter().sum(); // touches 4 bytes/row, contiguous (auto-vectorises)
    assert_eq!(sum, (0..100).sum::<i32>() as f32 + 0.5);
    // bytes dragged through cache per row when only `x` is needed: AoS pays the whole struct (stride), SoA pays 4.
    assert_eq!(size_of::<ParticleAos>(), 16);
    assert_eq!(size_of::<f32>(), 4);
    assert_eq!(
        allocs(|| Particles {
            x: Vec::with_capacity(100),
            ..Default::default()
        }),
        1
    );
}

// ----------------------------------------------------------- 4. zero-copy
#[derive(Debug, PartialEq)]
struct Header<'a> {
    version: u8,
    flags: u8,
    name: &'a str,
    payload: &'a [u8],
}
/// Wire format: version u8, flags u8, name_len u8, name bytes, payload_len u16 BE, payload bytes.
fn parse(buf: &[u8]) -> Option<(Header<'_>, &[u8])> {
    let (&version, rest) = buf.split_first()?;
    let (&flags, rest) = rest.split_first()?;
    let (&nlen, rest) = rest.split_first()?;
    let (name, rest) = rest.split_at_checked(nlen as usize)?;
    let (plen, rest) = rest.split_first_chunk::<2>()?;
    let (payload, rest) = rest.split_at_checked(u16::from_be_bytes(*plen) as usize)?;
    Some((
        Header {
            version,
            flags,
            name: std::str::from_utf8(name).ok()?,
            payload,
        },
        rest,
    ))
}
fn demo_zero_copy() {
    let mut wire = vec![1u8, 0b101, 3];
    wire.extend_from_slice(b"abc");
    wire.extend_from_slice(&5u16.to_be_bytes());
    wire.extend_from_slice(b"hello");
    wire.extend_from_slice(b"TRAILING");
    let mut out = None;
    let n = allocs(|| out = parse(&wire));
    assert_eq!(n, 0, "parsing borrows; it never copies");
    let (h, rest) = out.unwrap();
    assert_eq!(
        h,
        Header {
            version: 1,
            flags: 5,
            name: "abc",
            payload: b"hello"
        }
    );
    assert_eq!(rest, b"TRAILING");
    // the view really points INTO the buffer
    let base = wire.as_ptr() as usize;
    assert_eq!(h.payload.as_ptr() as usize, base + 3 + 3 + 2);
    assert!(
        parse(&wire[..8]).is_none(),
        "truncated input is an Option, not a panic"
    );
}

// --------------------------------------------- 5. index-linked list in a Vec
const NIL: u32 = u32::MAX;
struct Slot<T> {
    val: Option<T>,
    prev: u32,
    next: u32,
}
struct LinkedSlab<T> {
    slots: Vec<Slot<T>>,
    head: u32,
    tail: u32,
    free: u32, // free list threaded through `next`
    len: usize,
}
impl<T> LinkedSlab<T> {
    fn new() -> Self {
        LinkedSlab {
            slots: vec![],
            head: NIL,
            tail: NIL,
            free: NIL,
            len: 0,
        }
    }
    fn push_front(&mut self, val: T) -> u32 {
        let slot = Slot {
            val: Some(val),
            prev: NIL,
            next: self.head,
        };
        let id = if self.free != NIL {
            let id = self.free;
            self.free = self.slots[id as usize].next;
            self.slots[id as usize] = slot;
            id
        } else {
            self.slots.push(slot);
            (self.slots.len() - 1) as u32
        };
        if self.head != NIL {
            self.slots[self.head as usize].prev = id;
        } else {
            self.tail = id;
        }
        self.head = id;
        self.len += 1;
        id
    }
    fn remove(&mut self, id: u32) -> Option<T> {
        let val = self.slots[id as usize].val.take()?;
        let (p, n) = (self.slots[id as usize].prev, self.slots[id as usize].next);
        if p != NIL {
            self.slots[p as usize].next = n
        } else {
            self.head = n
        }
        if n != NIL {
            self.slots[n as usize].prev = p
        } else {
            self.tail = p
        }
        self.slots[id as usize].next = self.free;
        self.free = id;
        self.len -= 1;
        Some(val)
    }
    fn iter(&self) -> impl Iterator<Item = &T> {
        let mut cur = self.head;
        std::iter::from_fn(move || {
            if cur == NIL {
                return None;
            }
            let s = &self.slots[cur as usize];
            cur = s.next;
            s.val.as_ref()
        })
    }
}
fn demo_list() {
    let mut l = LinkedSlab::new();
    let a = l.push_front("a");
    let b = l.push_front("b");
    let c = l.push_front("c"); // list: c b a
    assert_eq!(l.iter().copied().collect::<String>(), "cba");
    assert_eq!(l.remove(b), Some("b"));
    assert_eq!(l.remove(b), None, "stale id is detected, not UB");
    assert_eq!(l.iter().copied().collect::<String>(), "ca");
    let d = l.push_front("d"); // reuses b's slot: no growth
    assert_eq!(d, b);
    assert_eq!(l.slots.len(), 3);
    assert_eq!((l.len, l.tail, a, c), (3, a, a, c));
}

// ------------------------------------------------- 6. inline small vector
enum SmallVec4 {
    Inline { len: u8, buf: [u32; 4] },
    Heap(Vec<u32>),
}
impl SmallVec4 {
    fn new() -> Self {
        SmallVec4::Inline {
            len: 0,
            buf: [0; 4],
        }
    }
    fn push(&mut self, x: u32) {
        match self {
            SmallVec4::Inline { len, buf } if (*len as usize) < 4 => {
                buf[*len as usize] = x;
                *len += 1;
            }
            SmallVec4::Inline { len, buf } => {
                let mut v = Vec::with_capacity(8);
                v.extend_from_slice(&buf[..*len as usize]);
                v.push(x);
                *self = SmallVec4::Heap(v);
            }
            SmallVec4::Heap(v) => v.push(x),
        }
    }
    fn as_slice(&self) -> &[u32] {
        match self {
            SmallVec4::Inline { len, buf } => &buf[..*len as usize],
            SmallVec4::Heap(v) => v,
        }
    }
}
fn demo_inline() {
    let n = allocs(|| {
        let mut s = SmallVec4::new();
        for i in 0..4 {
            s.push(i);
        }
        assert_eq!(s.as_slice(), &[0, 1, 2, 3]);
    });
    assert_eq!(n, 0, "up to 4 elements: no heap");
    let mut spilled = SmallVec4::new();
    let n = allocs(|| (0..5).for_each(|i| spilled.push(i)));
    assert_eq!(n, 1, "the 5th push spills with one allocation");
    assert_eq!(spilled.as_slice().len(), 5);
    assert!(
        size_of::<SmallVec4>() <= 32,
        "inline capacity costs only {} bytes",
        size_of::<SmallVec4>()
    );
}

// ----------------------------------------------- 7. object pool + RAII guard
struct Pool {
    free: RefCell<Vec<Vec<u8>>>,
}
struct Pooled<'p> {
    buf: Option<Vec<u8>>,
    pool: &'p Pool,
}
impl Pool {
    fn get(&self) -> Pooled<'_> {
        let buf = self
            .free
            .borrow_mut()
            .pop()
            .unwrap_or_else(|| Vec::with_capacity(4096));
        Pooled {
            buf: Some(buf),
            pool: self,
        }
    }
}
impl Deref for Pooled<'_> {
    type Target = Vec<u8>;
    fn deref(&self) -> &Vec<u8> {
        self.buf.as_ref().unwrap()
    }
}
impl DerefMut for Pooled<'_> {
    fn deref_mut(&mut self) -> &mut Vec<u8> {
        self.buf.as_mut().unwrap()
    }
}
impl Drop for Pooled<'_> {
    fn drop(&mut self) {
        if let Some(mut b) = self.buf.take() {
            b.clear(); // keep capacity, drop contents
            self.pool.free.borrow_mut().push(b);
        }
    }
}
fn demo_pool() {
    let pool = Pool {
        free: RefCell::new(Vec::new()),
    };
    let naive = allocs(|| {
        for i in 0..1000u32 {
            let mut v = Vec::with_capacity(4096);
            v.extend_from_slice(&i.to_le_bytes());
            black_box(&v);
        }
    });
    let pooled = allocs(|| {
        for i in 0..1000u32 {
            let mut b = pool.get();
            b.extend_from_slice(&i.to_le_bytes());
            black_box(&b);
        } // guard dropped: buffer returned
    });
    assert_eq!(naive, 1000);
    assert!(pooled <= 2, "pool allocates once then reuses ({pooled})");
    assert_eq!(pool.free.borrow().len(), 1);
    assert!(pool.get().is_empty(), "returned buffers are cleared");
}

// ----------------------------------------------------- 8. bitfield packing
/// kind: 3 bits | prio: 5 bits | len: 12 bits | flag: 1 bit  -> 21 bits in one u32.
#[derive(Clone, Copy, PartialEq, Debug)]
struct Packed(u32);
impl Packed {
    const KIND: (u32, u32) = (0, 3);
    const PRIO: (u32, u32) = (3, 5);
    const LEN: (u32, u32) = (8, 12);
    const FLAG: (u32, u32) = (20, 1);
    const fn get(self, (shift, bits): (u32, u32)) -> u32 {
        (self.0 >> shift) & ((1 << bits) - 1)
    }
    const fn set(self, (shift, bits): (u32, u32), v: u32) -> Packed {
        let mask = ((1 << bits) - 1) << shift;
        Packed((self.0 & !mask) | ((v << shift) & mask))
    }
}
struct Unpacked {
    kind: u8,
    prio: u8,
    len: u16,
    flag: bool,
}
fn demo_bitfield() {
    let p = Packed(0)
        .set(Packed::KIND, 5)
        .set(Packed::PRIO, 17)
        .set(Packed::LEN, 3000)
        .set(Packed::FLAG, 1);
    assert_eq!(
        (
            p.get(Packed::KIND),
            p.get(Packed::PRIO),
            p.get(Packed::LEN),
            p.get(Packed::FLAG)
        ),
        (5, 17, 3000, 1)
    );
    let q = p.set(Packed::PRIO, 0);
    assert_eq!(
        (q.get(Packed::KIND), q.get(Packed::LEN)),
        (5, 3000),
        "neighbours untouched"
    );
    assert_eq!(
        p.set(Packed::KIND, 0xFF).get(Packed::PRIO),
        17,
        "overflowing value is masked, no bleed"
    );
    assert_eq!(size_of::<Packed>(), 4);
    assert_eq!(size_of::<Unpacked>(), 6, "plain struct is 50% bigger");
    let mut x = 0x1234_5678u64;
    for _ in 0..1000 {
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        let (k, pr, l) = (
            (x & 7) as u32,
            ((x >> 8) & 31) as u32,
            ((x >> 16) & 4095) as u32,
        );
        let p = Packed(0)
            .set(Packed::KIND, k)
            .set(Packed::PRIO, pr)
            .set(Packed::LEN, l);
        assert_eq!(
            (p.get(Packed::KIND), p.get(Packed::PRIO), p.get(Packed::LEN)),
            (k, pr, l)
        );
    }
}

// ------------------------------------------------------------------- 9. Cow
fn normalize(s: &str) -> Cow<'_, str> {
    if s.contains('\t') {
        Cow::Owned(s.replace('\t', "    "))
    } else {
        Cow::Borrowed(s)
    }
}
fn demo_cow() {
    let clean = "no tabs here";
    let mut out = None;
    assert_eq!(allocs(|| out = Some(normalize(clean))), 0);
    assert!(matches!(out, Some(Cow::Borrowed(_))));
    let mut dirty = None;
    let n = allocs(|| dirty = Some(normalize("a\tb")));
    assert!(
        (1..=2).contains(&n),
        "modifying allocates ({n}): that is the price of ownership"
    );
    assert_eq!(dirty.unwrap(), "a    b");
}

// ------------------------------ 10. tagged-union enum (repr(u8)) vs trait object
#[repr(u8)]
enum Op {
    Push(u32) = 1,
    Add = 2,
    Mul = 3,
}
enum Shape {
    Circle(f32),
    Rect(f32, f32),
}
impl Shape {
    fn area(&self) -> f32 {
        match self {
            Shape::Circle(r) => 3.0 * r * r,
            Shape::Rect(w, h) => w * h,
        }
    }
}
trait Area {
    fn area(&self) -> f32;
}
struct Circle(f32);
struct Rect(f32, f32);
impl Area for Circle {
    fn area(&self) -> f32 {
        3.0 * self.0 * self.0
    }
}
impl Area for Rect {
    fn area(&self) -> f32 {
        self.0 * self.1
    }
}
fn demo_tagged() {
    assert_eq!(size_of::<Op>(), 8, "1-byte tag + padded u32 payload");
    let add = Op::Add;
    // SAFETY: `Op` is `#[repr(u8)]`, so it is a union of `#[repr(C)]` structs that each start with the u8 tag.
    let tag = unsafe { *(&add as *const Op as *const u8) };
    assert_eq!(
        tag, 2,
        "explicit discriminants are real and stable with repr(u8)"
    );
    let _ = (Op::Push(1), Op::Mul);
    assert_eq!(size_of::<Shape>(), 12);
    let (e, d);
    let mut sum_e = 0.0;
    let mut sum_d = 0.0;
    let mut es = Vec::new();
    e = allocs(|| {
        es = Vec::with_capacity(1000);
        for i in 0..1000 {
            es.push(if i % 2 == 0 {
                Shape::Circle(1.0)
            } else {
                Shape::Rect(2.0, 3.0)
            });
        }
    });
    let mut ds: Vec<Box<dyn Area>> = Vec::new();
    d = allocs(|| {
        ds = Vec::with_capacity(1000);
        for i in 0..1000 {
            ds.push(if i % 2 == 0 {
                Box::new(Circle(1.0))
            } else {
                Box::new(Rect(2.0, 3.0))
            });
        }
    });
    es.iter().for_each(|s| sum_e += s.area());
    ds.iter().for_each(|s| sum_d += s.area());
    assert_eq!(sum_e, sum_d);
    assert_eq!(e, 1, "enum: one allocation for the whole Vec");
    assert_eq!(d, 1001, "trait objects: one per element plus the Vec");
}

// -------------------------------------------------- 11. split borrows by index
/// Two `&mut` into one slice, proven disjoint by `split_at_mut`. The same trick as a plain index API: entities refer to each
/// other by index, and the rare "touch two at once" case uses this helper.
fn pair_mut<T>(v: &mut [T], i: usize, j: usize) -> (&mut T, &mut T) {
    assert_ne!(i, j, "indices must differ");
    if i < j {
        let (a, b) = v.split_at_mut(j);
        (&mut a[i], &mut b[0])
    } else {
        let (a, b) = v.split_at_mut(i);
        (&mut b[0], &mut a[j])
    }
}
fn demo_split() {
    let mut hp = vec![10, 20, 30];
    // `let (a, b) = (&mut hp[0], &mut hp[2]);` does not compile (two mutable borrows of `hp`)
    let (attacker, target) = pair_mut(&mut hp, 2, 0);
    *target -= *attacker / 2;
    *attacker += 1;
    assert_eq!(hp, vec![-5, 20, 31]);
    // a closure-free alternative when a value is Copy: read first, write after
    let dmg = hp[1];
    hp[0] -= dmg;
    assert_eq!(hp[0], -25);
}

fn main() {
    demo_arena();
    println!("1 arena          ok: typed ids, Option<NodeId> = 8 bytes");
    demo_intern();
    println!("2 interning      ok: Symbol = 4 bytes, re-intern = 0 allocs");
    demo_soa();
    println!("3 SoA            ok: scan reads 4 B/row vs 16 B/row AoS");
    demo_zero_copy();
    println!("4 zero-copy      ok: parse = 0 allocs, view points into buffer");
    demo_list();
    println!("5 slab list      ok: slot reuse, stale-id safe");
    demo_inline();
    println!(
        "6 inline vec     ok: {} bytes, spill = 1 alloc",
        size_of::<SmallVec4>()
    );
    demo_pool();
    println!("7 pool           ok: 1000 uses -> <= 2 allocs (naive: 1000)");
    demo_bitfield();
    println!("8 bitfield       ok: 4 bytes vs 6");
    demo_cow();
    println!("9 cow            ok: borrow = 0 allocs");
    demo_tagged();
    println!("10 enum vs dyn   ok: 1 vs 1001 allocations");
    demo_split();
    println!("11 split borrows ok");
}
