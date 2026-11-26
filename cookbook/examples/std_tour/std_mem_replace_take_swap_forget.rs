//! RECIPE 1046 (std-mem): `std::mem` by use case: take, replace, swap, drop, forget, sizes, discriminants, `ManuallyDrop`, `MaybeUninit`.
//! LEVEL: intermediate (sections 7 and 8 are advanced)
//! Use in: state machines behind `&mut self`, linked structures, buffer reuse, layout checks, careful unsafe code.   Run: cargo run --release --example std_mem_replace_take_swap_forget
//! COMMON USES: move a field out of `&mut self` and put something back; flush a batch with `mem::take`; double buffering; compare enum variants; initialise an array element by element.
//! METHODS: std::mem::take, std::mem::replace, std::mem::swap, std::mem::drop, std::mem::forget, std::mem::size_of, std::mem::size_of_val,
//! std::mem::align_of, std::mem::align_of_val, std::mem::needs_drop, std::mem::discriminant, std::mem::offset_of!, std::mem::zeroed,
//! std::mem::ManuallyDrop::new, ManuallyDrop::into_inner, ManuallyDrop::drop, std::mem::MaybeUninit::uninit, MaybeUninit::new, MaybeUninit::zeroed,
//! MaybeUninit::write, MaybeUninit::as_ptr, MaybeUninit::as_mut_ptr, MaybeUninit::assume_init, MaybeUninit::assume_init_ref,
//! MaybeUninit::assume_init_drop, Option::take, Option::replace, Vec::swap_remove, Box::leak
//! DOCS: https://doc.rust-lang.org/std/mem/index.html
//! DEFINED IN: core: all of `std::mem` is `core::mem` re-exported, including `ManuallyDrop` and `MaybeUninit`. `size_of`, `size_of_val`, `align_of`,
//!   `align_of_val` and `drop` are also in the prelude. alloc: `Box::leak`, `Vec::swap_remove`.
//! NOT COVERED (and why): `transmute` and `transmute_copy` (deliberately: use `to_bits`, `from_le_bytes`, pointer casts; see Nomicon), `variant_count`,
//! `MaybeUninit` array helpers (`array_assume_init`, `transpose`), `std::array::try_from_fn` (unstable in 1.98.1); `Pin` (see the pin recipes).
//! UNSAFE: every block states why it is sound. This file asserts behaviour on the paths it runs; it does not PROVE soundness.
//! MIRI: run on 2026-11-26 with rustc 1.100.0-nightly (5a2be9f5f 2026-09-06) and `-Zmiri-ignore-leaks` (the file leaks ON PURPOSE with forget and Box::leak): no
//!   undefined behaviour under Stacked Borrows or Tree Borrows on the executed paths (smaller loops under `cfg(miri)`).
//!
//! CHEAT SHEET (I want to ... -> use ...)
//!   move out of `&mut` and leave the default      -> `mem::take(&mut self.items)` (needs `Default`; no allocation for Vec/String)
//!   move out and leave a specific value           -> `mem::replace(&mut self.state, State::Idle)`
//!   same for an Option                            -> `opt.take()` / `opt.replace(v)`
//!   exchange two places                           -> `mem::swap(&mut a, &mut b)`; two items of ONE slice: `slice.swap(i, j)`
//!   end a borrow or release a lock early          -> `drop(guard)` (a no-op for `Copy` values and references)
//!   hand ownership to C or a raw pointer          -> `Box::into_raw` / `Vec::leak` / `ManuallyDrop`, NOT `forget` after reading a pointer
//!   "which variant is this" without the data      -> `mem::discriminant(&a) == mem::discriminant(&b)`
//!   layout assertions for FFI / file formats      -> `#[repr(C)]` + `size_of`, `align_of`, `offset_of!` in a `const _: () = assert!(..)`
//!   skip drop bookkeeping in generic code         -> `needs_drop::<T>()`
//!   delay or reorder a destructor                 -> `ManuallyDrop<T>`
//!   build `[T; N]` safely                         -> `std::array::from_fn` first; `MaybeUninit` only when it cannot express the need
//!
//! KEY FACT: leaking is SAFE (`forget`, `Rc` cycles, `Box::leak`). So unsafe code must never rely on a destructor running for memory safety.
//! TRY NEXT:
//!   - [S] In section 2 replace `mem::take(&mut self.pending)` with `self.pending.clone()` + `clear()`. What changes in allocations and capacity?
//!   - [S] Predict `size_of::<Option<Box<u8>>>()` and `size_of::<Option<u32>>()`. Which one does the language guarantee?
//!   - [M] Delete the `Drop` impl of `List` and push 10 million nodes. Explain the crash, then restore it.
//!   - [M] Remove the guard in `array_from_fn` and make the closure panic: show with the drop counter what leaks.
//!   - [A] PROJECT: combine with `std_ptr_nonnull_slices_raw`: build a fixed-capacity stack on `[MaybeUninit<T>; N]` with push, pop, Drop, and a differential test against Vec; then run it under Miri.
//! Read: https://doc.rust-lang.org/std/mem/union.MaybeUninit.html ; https://doc.rust-lang.org/nomicon/leaking.html ; https://doc.rust-lang.org/nomicon/unchecked-uninit.html
#![allow(dead_code)]
#![deny(unsafe_op_in_unsafe_fn)]

use std::cell::{Cell, RefCell};
use std::mem::{self, ManuallyDrop, MaybeUninit, offset_of};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::rc::Rc;

fn panics<R>(f: impl FnOnce() -> R) -> bool {
    let old = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));
    let r = catch_unwind(AssertUnwindSafe(f)).is_err();
    std::panic::set_hook(old);
    r
}

/// Records every drop by id, so tests can assert "exactly once" and "in this order".
#[derive(Clone, Default)]
struct Log(Rc<RefCell<Vec<u32>>>);
impl Log {
    fn make(&self, id: u32) -> Tracked {
        Tracked {
            id,
            log: self.clone(),
        }
    }
    fn dropped(&self) -> Vec<u32> {
        self.0.borrow().clone()
    }
}
struct Tracked {
    id: u32,
    log: Log,
}
impl Drop for Tracked {
    fn drop(&mut self) {
        self.log.0.borrow_mut().push(self.id);
    }
}

// ------------------------------------------------------------ 1. take / replace / swap
fn s01_basics() {
    let mut v = vec![1, 2, 3];
    let ptr = v.as_ptr();
    let taken = mem::take(&mut v);
    assert_eq!(
        (taken.as_ptr(), v.len(), v.capacity()),
        (ptr, 0, 0),
        "the allocation moved; the new empty Vec allocated nothing"
    );
    assert_eq!(taken, [1, 2, 3]);

    let mut name = String::from("old");
    assert_eq!(mem::replace(&mut name, "new".to_string()), "old");
    assert_eq!(name, "new");

    let (mut a, mut b) = (vec![1], vec![2, 2]);
    mem::swap(&mut a, &mut b);
    assert_eq!((a, b), (vec![2, 2], vec![1]));
    let mut arr = [1, 2, 3];
    // `mem::swap(&mut arr[0], &mut arr[2])` does not compile (two mutable borrows of arr); slices offer both ways
    arr.swap(0, 2);
    let (l, r) = arr.split_at_mut(1);
    mem::swap(&mut l[0], &mut r[0]);
    assert_eq!(arr, [2, 3, 1]);

    let mut o = Some(5);
    assert_eq!((o.take(), o), (Some(5), None));
    assert_eq!((o.replace(6), o.replace(7), o), (None, Some(6), Some(7)));
    let c = Cell::new(vec![1]);
    assert_eq!((c.take(), c.replace(vec![2])), (vec![1], vec![])); // the Cell versions work through &
    println!("1. take / replace / swap ok");
}

// ------------------------------------------------------------ 2. State machines behind &mut self
#[derive(Debug, Default)]
enum Conn {
    #[default]
    Closed,
    Connecting {
        queued: Vec<String>,
    },
    Open {
        sent: Vec<String>,
    },
}

impl Conn {
    fn connect(&mut self) {
        if matches!(self, Conn::Closed) {
            *self = Conn::Connecting { queued: Vec::new() };
        }
    }
    fn established(&mut self) {
        // We need to MOVE `queued` from the old variant into the new one. A `&mut` cannot be left empty,
        // so take the whole state out (Closed is left behind), match by value, and write the next state.
        *self = match mem::take(self) {
            Conn::Connecting { queued } => Conn::Open { sent: queued },
            other => other, // every other arm must put the old state back, or it is silently lost
        };
    }
    fn send(&mut self, msg: &str) -> bool {
        match self {
            Conn::Closed => false,
            Conn::Connecting { queued: v } | Conn::Open { sent: v } => {
                v.push(msg.to_string());
                true
            }
        }
    }
}

struct Batcher {
    pending: Vec<u32>,
    limit: usize,
}
impl Batcher {
    fn push(&mut self, x: u32) -> Option<Vec<u32>> {
        self.pending.push(x);
        (self.pending.len() == self.limit)
            .then(|| mem::replace(&mut self.pending, Vec::with_capacity(self.limit)))
    }
    fn flush(&mut self) -> Vec<u32> {
        mem::take(&mut self.pending)
    }
}

fn s02_state() {
    let mut c = Conn::default();
    assert!(!c.send("lost"));
    c.connect();
    assert!(c.send("a") && c.send("b"));
    c.established();
    assert!(matches!(&c, Conn::Open { sent } if sent == &["a", "b"]));
    c.established(); // wrong state: unchanged
    assert!(matches!(&c, Conn::Open { sent } if sent.len() == 2));

    let mut b = Batcher {
        pending: Vec::new(),
        limit: 3,
    };
    let out: Vec<Vec<u32>> = (0..8).filter_map(|i| b.push(i)).collect();
    assert_eq!(out, [vec![0, 1, 2], vec![3, 4, 5]]);
    assert_eq!(b.flush(), [6, 7]);

    // PITFALL: if code panics between take and write-back, the placeholder stays behind. Choose a placeholder that is a SAFE state.
    let mut s = Conn::Open {
        sent: vec!["x".into()],
    };
    assert!(panics(|| {
        let _old = mem::take(&mut s);
        panic!("while the state is taken out");
    }));
    assert!(matches!(s, Conn::Closed));
    println!("2. state transitions ok");
}

// ------------------------------------------------------------ 3. Linked list with Option<Box<Node>>
struct Node<T> {
    value: T,
    next: Option<Box<Node<T>>>,
}
struct List<T> {
    head: Option<Box<Node<T>>>,
    len: usize,
}
impl<T> List<T> {
    fn new() -> Self {
        List { head: None, len: 0 }
    }
    fn push(&mut self, value: T) {
        self.head = Some(Box::new(Node {
            value,
            next: self.head.take(),
        }));
        self.len += 1;
    }
    fn pop(&mut self) -> Option<T> {
        self.head.take().map(|node| {
            self.head = node.next; // moving a field out of a Box we own is fine
            self.len -= 1;
            node.value
        })
    }
    fn reverse(&mut self) {
        let mut done = None;
        let mut rest = self.head.take();
        while let Some(mut node) = rest {
            rest = mem::replace(&mut node.next, done); // detach the tail, attach the reversed prefix
            done = Some(node);
        }
        self.head = done;
    }
}
impl<T> Drop for List<T> {
    fn drop(&mut self) {
        // The compiler-generated drop recurses once per node and can overflow the stack on long lists.
        let mut next = self.head.take();
        while let Some(mut node) = next {
            next = node.next.take(); // unlink first, so dropping `node` frees exactly one node
        }
    }
}

fn s03_list() {
    let mut l = List::new();
    (1..=5).for_each(|i| l.push(i));
    l.reverse();
    assert_eq!(
        std::iter::from_fn(|| l.pop()).collect::<Vec<_>>(),
        [1, 2, 3, 4, 5]
    );
    assert_eq!((l.len, l.pop()), (0, None));
    let mut big = List::new();
    (0..if cfg!(miri) { 1_000u32 } else { 1_000_000 }).for_each(|i| big.push(i));
    drop(big); // would risk a stack overflow without the iterative Drop
    println!("3. linked list ok (1M nodes dropped iteratively)");
}

// ------------------------------------------------------------ 4. Double buffering and swap_remove
struct DoubleBuffer {
    front: Vec<u32>, // readers see this
    back: Vec<u32>,  // the writer fills this
}
impl DoubleBuffer {
    fn publish(&mut self) {
        mem::swap(&mut self.front, &mut self.back); // O(1): three words each, no elements copied
        self.back.clear(); // keeps capacity for the next frame
    }
}

fn s04_buffers() {
    let mut d = DoubleBuffer {
        front: Vec::with_capacity(100),
        back: Vec::with_capacity(100),
    };
    let allocations = [d.front.as_ptr(), d.back.as_ptr()];
    for frame in 0..if cfg!(miri) { 20u32 } else { 1_000 } {
        d.back.extend(frame..frame + 100);
        d.publish();
        assert_eq!(
            (d.front.first(), d.front.len(), d.back.len()),
            (Some(&frame), 100, 0)
        );
        assert!(
            allocations.contains(&d.front.as_ptr()),
            "steady state: the same two allocations forever"
        );
    }
    // swap_remove: O(1) removal that does NOT keep order (the last element moves into the hole)
    let mut v = vec![10, 11, 12, 13, 14];
    assert_eq!(v.swap_remove(1), 11);
    assert_eq!(v, [10, 14, 12, 13]);
    // removing while scanning: do not advance after a swap_remove, a new element now sits at i
    let mut v: Vec<u32> = (0..100).map(|i| i * 7 % 100).collect();
    let mut i = 0;
    while i < v.len() {
        if v[i] % 2 == 0 {
            v.swap_remove(i);
        } else {
            i += 1;
        }
    }
    v.sort();
    assert_eq!(v, (1..100).step_by(2).collect::<Vec<u32>>());
    assert!(panics(|| Vec::<u8>::new().swap_remove(0)));
    println!("4. buffers / swap_remove ok");
}

// ------------------------------------------------------------ 5. drop and forget
fn s05_drop_forget() {
    let log = Log::default();
    {
        let _a = log.make(1);
        let b = log.make(2);
        let _ = log.make(3); // `let _ =` does NOT bind: this temporary dies at the end of this statement
        assert_eq!(log.dropped(), [3]);
        drop(b); // mem::drop is just `fn drop<T>(_: T) {}`: it takes ownership
        assert_eq!(log.dropped(), [3, 2]);
    }
    assert_eq!(log.dropped(), [3, 2, 1]);

    struct Pair(Tracked, Tracked);
    let log = Log::default();
    {
        let _x = log.make(1);
        let _y = log.make(2);
        let _p = Pair(log.make(3), log.make(4));
    }
    assert_eq!(
        log.dropped(),
        [3, 4, 2, 1],
        "locals drop in REVERSE order; fields in DECLARATION order"
    );

    // assignment drops the old value; `replace` hands it back instead
    let log = Log::default();
    let mut slot = log.make(1);
    let old = mem::replace(&mut slot, log.make(2));
    assert_eq!(log.dropped(), Vec::<u32>::new());
    slot = log.make(3);
    assert_eq!(log.dropped(), [2]);
    drop((old, slot));
    assert_eq!(log.dropped(), [2, 1, 3]);

    // forget: the destructor never runs. Safe, but resources (memory, files, locks) are lost.
    let log = Log::default();
    mem::forget(log.make(9));
    assert!(log.dropped().is_empty());
    assert_eq!(
        Rc::strong_count(&log.0),
        2,
        "the forgotten value still holds its Rc clone"
    );

    // deliberate leak with a use: a value that lives for the whole program
    let config: &'static mut String = Box::leak(Box::new(String::from("config")));
    config.push_str("-v1");
    assert_eq!(config, "config-v1");

    // Forgetting a Drain is safe: you may lose elements, but never double-drop or read freed ones. The docs promise nothing about which survive.
    let log = Log::default();
    let mut v: Vec<Tracked> = (0..5).map(|i| log.make(i)).collect();
    mem::forget(v.drain(1..3));
    assert!(v.len() <= 5);
    drop(v);
    let mut d = log.dropped();
    d.sort();
    let n = d.len();
    d.dedup();
    assert_eq!(d.len(), n, "no id dropped twice");
    println!("5. drop / forget ok");
}

// ------------------------------------------------------------ 6. Layout and type queries
#[repr(C)]
struct Header {
    tag: u8,
    len: u32,
    flags: u16,
}
// compile-time layout contract: the build fails if someone reorders the fields
const _: () = assert!(
    size_of::<Header>() == 12 && offset_of!(Header, len) == 4 && offset_of!(Header, flags) == 8
);

#[derive(Debug)]
enum Event {
    Key(char),
    Click { x: i32, y: i32 },
    Quit,
}

fn s06_queries() {
    assert_eq!(
        (
            size_of::<u8>(),
            size_of::<u64>(),
            size_of::<[u16; 5]>(),
            size_of::<()>()
        ),
        (1, 8, 10, 0)
    );
    assert_eq!(align_of::<[u16; 5]>(), align_of::<u16>());
    // size_of::<Vec<T>>() is the handle, not the contents; size_of_val works on unsized values
    let v = vec![0u32; 1000];
    assert_eq!(size_of_val(v.as_slice()), 4000);
    assert_eq!(size_of_val(&v), 3 * size_of::<usize>()); // true on current std; not a language guarantee
    assert_eq!(size_of_val("héllo"), 6);
    assert_eq!(align_of_val(v.as_slice()), align_of::<u32>());
    // guaranteed niche: Option of these is pointer sized, so Option<Box<T>> is free
    assert_eq!(size_of::<Option<Box<u8>>>(), size_of::<usize>());
    assert_eq!(size_of::<Option<&u8>>(), size_of::<usize>());
    assert_eq!(size_of::<Option<std::num::NonZeroU32>>(), 4);
    assert!(
        size_of::<Option<u32>>() > size_of::<u32>(),
        "u32 has no spare bit pattern for None"
    );

    assert_eq!(
        (
            offset_of!(Header, tag),
            offset_of!(Header, len),
            offset_of!(Header, flags)
        ),
        (0, 4, 8)
    );
    assert_eq!(align_of::<Header>(), 4);
    // repr(Rust) (the default) promises no field order and no offsets: only assert what must hold
    struct Loose {
        a: u8,
        b: u64,
        c: u8,
    }
    assert!(size_of::<Loose>() >= 10 && offset_of!(Loose, b) % align_of::<u64>() == 0);

    // discriminant: compare variants, ignoring payloads. It is Eq + Hash.
    let events = [
        Event::Key('a'),
        Event::Click { x: 1, y: 2 },
        Event::Key('b'),
        Event::Quit,
        Event::Click { x: 0, y: 0 },
    ];
    assert_eq!(mem::discriminant(&events[0]), mem::discriminant(&events[2]));
    assert_ne!(mem::discriminant(&events[0]), mem::discriminant(&events[3]));
    let mut counts = std::collections::HashMap::new();
    for e in &events {
        *counts.entry(mem::discriminant(e)).or_insert(0) += 1;
    }
    assert_eq!(
        (counts.len(), counts[&mem::discriminant(&Event::Key('?'))]),
        (3, 2)
    );

    // needs_drop: lets a generic container skip a drop loop. It may conservatively say true; false means dropping has no effect.
    assert!(
        !mem::needs_drop::<u64>()
            && !mem::needs_drop::<[&str; 4]>()
            && !mem::needs_drop::<ManuallyDrop<String>>()
    );
    assert!(mem::needs_drop::<String>() && mem::needs_drop::<(u8, Vec<u8>)>());

    // bit reinterpretation without transmute
    assert_eq!(f32::from_bits(1.5f32.to_bits()), 1.5);
    assert_eq!(u32::from_le_bytes([1, 0, 0, 0]), 1);
    assert_eq!((-1i32).cast_unsigned(), u32::MAX);
    println!("6. layout / discriminant ok");
}

// ------------------------------------------------------------ 7. ManuallyDrop
/// `index` must be dropped BEFORE `data` (think: a handle that flushes into a resource). Field order alone would do it,
/// but this makes the order explicit and robust against someone reordering the fields.
struct Ordered {
    data: ManuallyDrop<Tracked>,
    index: ManuallyDrop<Tracked>,
}
impl Drop for Ordered {
    fn drop(&mut self) {
        // SAFETY: `drop` runs once, neither field has been dropped or taken elsewhere, and neither is used afterwards.
        unsafe {
            ManuallyDrop::drop(&mut self.index);
            ManuallyDrop::drop(&mut self.data);
        }
    }
}

fn s07_manually_drop() {
    let log = Log::default();
    drop(Ordered {
        data: ManuallyDrop::new(log.make(1)),
        index: ManuallyDrop::new(log.make(2)),
    });
    assert_eq!(log.dropped(), [2, 1]);

    let log = Log::default();
    let m = ManuallyDrop::new(log.make(3));
    assert_eq!(m.id, 3); // derefs to the inner value
    let back = ManuallyDrop::into_inner(m); // safe: gives the value (and its destructor) back
    drop(back);
    assert_eq!(log.dropped(), [3]);
    {
        let _wrapper = ManuallyDrop::new(log.make(4)); // the WRAPPER goes out of scope here; the Tracked inside is not dropped
    }
    assert_eq!(log.dropped(), [3], "4 was leaked");

    // The standard way to split a Vec into raw parts: ManuallyDrop FIRST, then read pointer, length and capacity.
    // (Reading them and calling forget afterwards invalidates nothing today, but moving the Vec into forget after
    // taking a pointer is a pattern the docs warn against, and an early panic would free the buffer twice.)
    let mut original = ManuallyDrop::new(vec![String::from("a"), String::from("b")]);
    let (ptr, len, cap) = (original.as_mut_ptr(), original.len(), original.capacity());
    // SAFETY: ptr, len, cap come from a Vec<String> with the global allocator; `original` will never be dropped or used
    // again, so the rebuilt Vec is the sole owner. The element type is unchanged.
    let rebuilt: Vec<String> = unsafe { Vec::from_raw_parts(ptr, len, cap) };
    assert_eq!(rebuilt, ["a", "b"]);
    println!("7. manuallydrop ok");
}

// ------------------------------------------------------------ 8. MaybeUninit and zeroed
/// Same job as `std::array::from_fn`, written out to show the panic-safety obligation.
fn array_from_fn<T, const N: usize>(mut f: impl FnMut(usize) -> T) -> [T; N] {
    /// Owns the initialised prefix while construction is in progress.
    struct Guard<'a, T> {
        slots: &'a mut [MaybeUninit<T>],
        init: usize,
    }
    impl<T> Drop for Guard<'_, T> {
        fn drop(&mut self) {
            for slot in &mut self.slots[..self.init] {
                // SAFETY: exactly the first `init` slots were written and have not been moved out.
                unsafe { slot.assume_init_drop() }
            }
        }
    }
    let mut slots: [MaybeUninit<T>; N] = [const { MaybeUninit::uninit() }; N];
    let mut guard = Guard {
        slots: &mut slots,
        init: 0,
    };
    while guard.init < N {
        let value = f(guard.init); // may panic: the guard then drops the prefix
        guard.slots[guard.init].write(value);
        guard.init += 1;
    }
    mem::forget(guard); // success: ownership of every element passes to the result (the guard owns only a borrow, nothing leaks)
    // SAFETY: the loop wrote all N slots, and the guard that would have dropped them has been disarmed.
    // `map` moves each element out exactly once.
    slots.map(|s| unsafe { s.assume_init() })
}

fn s08_maybe_uninit() {
    let a: [String; 4] = array_from_fn(|i| i.to_string());
    assert_eq!(a, ["0", "1", "2", "3"]);
    assert_eq!(
        a,
        std::array::from_fn::<String, 4, _>(|i| i.to_string()),
        "prefer the std function"
    );
    let empty: [String; 0] = array_from_fn(|_| unreachable!());
    assert!(empty.is_empty());
    for fail_at in 0..5u32 {
        let log = Log::default();
        assert!(panics(|| array_from_fn::<Tracked, 5>(|i| {
            assert!(i as u32 != fail_at);
            log.make(i as u32)
        })));
        let mut d = log.dropped();
        d.sort();
        assert_eq!(
            d,
            (0..fail_at).collect::<Vec<_>>(),
            "the initialised prefix was dropped exactly once"
        );
    }

    // out-parameter style: write through a raw pointer, then assume_init
    fn c_style_fill(out: *mut [u32; 4]) -> bool {
        // SAFETY (of this body): the caller passes a pointer valid for writing one [u32; 4]; `write` does not read the old bytes.
        unsafe { out.write([1, 2, 3, 4]) };
        true
    }
    let mut out = MaybeUninit::<[u32; 4]>::uninit();
    assert!(c_style_fill(out.as_mut_ptr()));
    // SAFETY: c_style_fill returned true, and its contract is that it then wrote the whole array.
    assert_eq!(unsafe { out.assume_init() }, [1, 2, 3, 4]);

    let ready = MaybeUninit::new(String::from("hi"));
    // SAFETY: created with `new`, so it is initialised; as_ptr only forms a pointer to it.
    unsafe {
        assert_eq!(ready.assume_init_ref(), "hi");
        assert_eq!((&*ready.as_ptr()).len(), 2);
    }
    // SAFETY: still initialised, and not used after this move.
    drop(unsafe { ready.assume_init() }); // without this the String would leak: MaybeUninit never drops its content

    // zeroed: valid ONLY if all-zero bytes are a valid value of the type.
    // SAFETY: every bit pattern, including all zeros, is a valid u64 and a valid [u8; 8].
    let (n, bytes): (u64, [u8; 8]) = unsafe { (mem::zeroed(), mem::zeroed()) };
    assert_eq!((n, bytes), (0, [0; 8]));
    let z = MaybeUninit::<Option<Box<u8>>>::zeroed();
    // SAFETY: the null-pointer niche is guaranteed: all zeros is `None` for Option<Box<T>>.
    assert!(unsafe { z.assume_init() }.is_none());
    // NEVER: mem::zeroed::<&u8>(), ::<Box<u8>>(), ::<fn()>() or an enum with no zero variant: instant undefined behaviour.
    // NEVER: `let x: u32 = MaybeUninit::uninit().assume_init();` reading uninitialised memory is undefined behaviour even for integers.
    println!("8. maybeuninit / zeroed ok");
}

fn main() {
    s01_basics();
    s02_state();
    s03_list();
    s04_buffers();
    s05_drop_forget();
    s06_queries();
    s07_manually_drop();
    s08_maybe_uninit();
    println!("all mem sections passed");
}
