//! RECIPE 106 (language): std::mem and std::ptr toolbox: replace/take/swap/forget/discriminant/copy/transmute and how they combine.
//! LEVEL: intermediate
//! Use in: state machines, buffers, linked structures, low-level code.   Run: cargo run --release --example mem_functions
#![allow(dead_code)]
//!
//! WHAT: `mem::replace(&mut x, new)` returns the old value; `take` replaces with `Default`; `swap` exchanges two places.
//! WHY: safe Rust forbids moving out of a `&mut` (the place would be left empty). These functions move out while
//! leaving a valid value behind, so the borrow checker and drop logic stay happy. No unsafe is needed.
//! HOW: combine them. A state enum with a `Poisoned` placeholder lets `&mut self` methods consume the old state by value.
//! `forget` skips a destructor (a leak, safe but rarely wanted); `ptr::copy` is memmove; `copy_nonoverlapping` is memcpy.
//! `transmute` reinterprets bits and is almost always replaceable:
//!   bytes -> int: `u32::from_ne_bytes`   | float bits: `f32::to_bits`/`from_bits`  | checked enum: `TryFrom<u8>`
//!   numeric cast: `as`                   | pointer <-> usize: `ptr.addr()`        | slices: `bytemuck`-style casts
//! COMMON USES: `Option::take` in linked lists and trees, `mem::take` of a field inside `Drop`/`finish(self)` code, double-buffered
//!   simulations/render queues, reusing a scratch `Vec` between calls, `mem::replace` in builder and state-machine code.
//! Read: https://doc.rust-lang.org/std/mem/index.html and https://doc.rust-lang.org/nomicon/transmutes.html
use std::mem::{self, discriminant, size_of_val};
use std::ptr;
use std::sync::atomic::{AtomicUsize, Ordering::SeqCst};

static CREATED: AtomicUsize = AtomicUsize::new(0);
static DROPPED: AtomicUsize = AtomicUsize::new(0);
/// Counts every construction and drop so we can prove nothing is lost or duplicated.
#[derive(Debug)]
struct Tracked(u32);
impl Tracked {
    fn new(n: u32) -> Self {
        CREATED.fetch_add(1, SeqCst);
        Tracked(n)
    }
}
impl Drop for Tracked {
    fn drop(&mut self) {
        DROPPED.fetch_add(1, SeqCst);
    }
}
fn live() -> usize {
    CREATED.load(SeqCst) - DROPPED.load(SeqCst)
}

// ---- state transition: move out of &mut self ----
enum Conn {
    Idle(Tracked),
    Active { buf: Vec<u8>, id: Tracked },
    Closed,
    Poisoned, // placeholder while the real state is being computed
}
impl Conn {
    fn advance(&mut self) {
        let old = mem::replace(self, Conn::Poisoned);
        // If this match panicked, `self` stays Poisoned: detectable, never a double drop.
        *self = match old {
            Conn::Idle(id) => Conn::Active {
                buf: vec![1, 2, 3],
                id,
            }, // moves `id` out by value
            Conn::Active { buf, id } => {
                drop(id);
                assert_eq!(buf.len(), 3);
                Conn::Closed
            }
            Conn::Closed => Conn::Closed,
            Conn::Poisoned => panic!("used after panic"),
        };
    }
}

// ---- linked list with Option::take ----
struct Node {
    v: Tracked,
    next: Option<Box<Node>>,
}
struct Stack {
    head: Option<Box<Node>>,
}
impl Stack {
    fn push(&mut self, n: u32) {
        let next = self.head.take();
        self.head = Some(Box::new(Node {
            v: Tracked::new(n),
            next,
        }));
    }
    fn pop(&mut self) -> Option<Tracked> {
        self.head.take().map(|n| {
            let n = *n;
            self.head = n.next;
            n.v
        })
    }
}

// ---- double buffering ----
struct Buffers {
    front: Vec<u32>,
    back: Vec<u32>,
}
impl Buffers {
    fn flip_and_drain(&mut self) -> Vec<u32> {
        mem::swap(&mut self.front, &mut self.back);
        self.back.drain(..).collect() // `back` keeps its capacity for the next round
    }
}

fn main() {
    // replace / take / swap
    let mut s = String::from("a");
    let old = mem::replace(&mut s, "b".into());
    let taken = mem::take(&mut s);
    assert_eq!((old.as_str(), taken.as_str(), s.as_str()), ("a", "b", ""));
    let (mut x, mut y) = (1, 2);
    mem::swap(&mut x, &mut y);
    assert_eq!((x, y), (2, 1));

    // state machine + accounting
    let mut c = Conn::Idle(Tracked::new(1));
    c.advance();
    assert!(matches!(c, Conn::Active { .. }));
    assert_eq!(live(), 1);
    c.advance();
    assert!(matches!(c, Conn::Closed));
    assert_eq!(live(), 0);
    assert_eq!(discriminant(&Conn::Closed), discriminant(&Conn::Closed));
    assert_ne!(discriminant(&Conn::Closed), discriminant(&Conn::Poisoned));

    // list pop
    let mut st = Stack { head: None };
    (1..=3).for_each(|i| st.push(i));
    assert_eq!(live(), 3);
    assert_eq!(st.pop().map(|t| t.0), Some(3));
    assert_eq!(live(), 2);
    drop(st); // drops remaining nodes (recursively; fine for tiny lists, long lists need an iterative Drop)
    assert_eq!(live(), 0);

    // double buffer, and take on a Vec
    let mut b = Buffers {
        front: vec![],
        back: vec![5, 6],
    };
    assert_eq!(b.flip_and_drain(), vec![]);
    b.front.push(7);
    assert_eq!(b.flip_and_drain(), vec![5, 6, 7]);
    let mut scratch: Vec<u8> = Vec::with_capacity(64);
    scratch.extend_from_slice(b"abc");
    let mut t = mem::take(&mut scratch);
    assert_eq!(scratch.capacity(), 0); // take leaves a fresh empty Vec
    t.clear();
    scratch = t; // put it back: capacity is reused
    assert!(scratch.capacity() >= 64);

    // forget: no destructor runs (a leak); the counters expose it
    let (c0, d0) = (CREATED.load(SeqCst), DROPPED.load(SeqCst));
    mem::forget(Tracked::new(9));
    assert_eq!(
        (CREATED.load(SeqCst) - c0, DROPPED.load(SeqCst) - d0),
        (1, 0)
    );
    assert_eq!(live(), 1); // the one deliberate leak

    // size_of_val
    let arr = [0u16; 5];
    let sl: &[u16] = &arr;
    assert_eq!(
        (size_of_val(&arr), size_of_val(sl), size_of_val("héllo")),
        (10, 10, 6)
    );

    // ptr::copy (overlapping ok) vs copy_nonoverlapping, on Copy data only
    let mut a = [1u32, 2, 3, 4, 5];
    // SAFETY: indices 0..4 and 1..5 are in bounds; ptr::copy permits overlap.
    unsafe { ptr::copy(a.as_ptr(), a.as_mut_ptr().add(1), 4) };
    assert_eq!(a, [1, 1, 2, 3, 4]);
    let src = [9u32, 8];
    let mut dst = [0u32; 2];
    // SAFETY: distinct arrays, 2 elements each, u32 is Copy.
    unsafe { ptr::copy_nonoverlapping(src.as_ptr(), dst.as_mut_ptr(), 2) };
    assert_eq!(dst, src);

    // transmute: the sound uses, each with a safe equivalent
    let bytes = [1u8, 2, 3, 4];
    // SAFETY: [u8;4] and u32 have the same size and every bit pattern is a valid u32.
    #[allow(unnecessary_transmutes)] // clippy/rustc say: use from_ne_bytes; shown here to compare
    let n: u32 = unsafe { mem::transmute(bytes) };
    assert_eq!(n, u32::from_ne_bytes(bytes));
    // SAFETY: same size, all bit patterns valid.
    #[allow(unnecessary_transmutes)]
    let bits: u32 = unsafe { mem::transmute(1.5f32) };
    assert_eq!(bits, 1.5f32.to_bits());
    // SAFETY: reading a u32 as [u8;4]: same size, both plain data.
    let back: [u8; 4] = unsafe { mem::transmute_copy(&n) };
    assert_eq!(back, bytes);
    // DANGEROUS (never do): extending a lifetime.
    //   fn extend<'a>(r: &'a str) -> &'static str { unsafe { mem::transmute(r) } }  // UB once the owner is dropped
    //   let s = String::from("x"); let r = extend(&s); drop(s); println!("{r}");   // use after free
    // Also never: transmute::<u8, bool>(2), transmute::<u8, MyEnum>(unknown), &T -> &mut T, Vec<T> -> Vec<U>.
    println!("live leaked: {}", live());
}
