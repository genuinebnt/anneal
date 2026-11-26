//! RECIPE 170 (memory): a bump (arena) allocator: alignment, checkpoints, drop glue, chunked growth.
//! LEVEL: advanced
//! Use in: compilers (AST/IR nodes), per-request scratch memory, parsers, game frames.   Run: cargo run --release --example bump_arena_allocator
//! Unsafe recipe: also try `cargo +nightly miri run --example bump_arena_allocator`.
#![allow(dead_code)]
//!
//! A bump allocator keeps one cursor into a block of memory. `alloc` = round the cursor UP to the requested alignment, add the size,
//! compare with the end. That is a handful of instructions, no free list, no per-object header. Freeing individual objects is not
//! possible: everything is released at once by `reset`, or back to a CHECKPOINT (a scoped arena, like a stack of lifetimes).
//!
//! Key points implemented below:
//! * ALIGNMENT is applied to the real ADDRESS (`base + used`), not to the offset, so alignments larger than the chunk's own work.
//!   Every addition is `checked_*`: a huge `Layout` yields `None` instead of wrapping around and handing out aliasing memory.
//! * `&self` allocation (interior mutability via RefCell) lets many `&mut T` coexist: each points into disjoint bytes.
//!   `rewind`/`reset` take `&mut self`, so the borrow checker proves no reference into the arena is alive when memory is reused.
//! * DROP GLUE: `alloc_value` never runs destructors (like bumpalo's `alloc`: a `String` or `Rc` placed there LEAKS its heap
//!   buffer / refcount). `alloc_with_drop` records `(ptr, drop_in_place::<T>)` and runs the records in REVERSE order on
//!   rewind/Drop, like stack unwinding. bumpalo's `Bump` never drops; oxc's `oxc_allocator` wraps bumpalo and keeps AST nodes
//!   free of Drop types (it provides arena-backed `Vec`/`String`/`Box` that are never "dropped" individually).
//! * CHUNKED GROWTH: when the current chunk is full, a new chunk of double size is allocated (old chunks never move, so
//!   references stay valid), capped by a total `limit`. Rewinding frees chunks created after the checkpoint.
//!
//! COMMON USES: compiler front ends (parse the file, build the AST in an arena, drop it all at once), per-frame/per-request
//! scratch memory, regex/graph builders, anything where all objects share one lifetime. Wins: allocation is ~pointer bump,
//! excellent locality, deallocation is O(chunks). Costs: no individual free, memory held until reset, no Drop by default.
//! Read: https://github.com/fitzgen/bumpalo and https://docs.rs/bumpalo and https://github.com/oxc-project/oxc (crates/oxc_allocator)
use std::alloc::{self, Layout};
use std::cell::RefCell;
use std::hint::black_box;
use std::ptr::NonNull;
use std::rc::Rc;
use std::time::Instant;

const CHUNK_ALIGN: usize = 16;

struct Chunk {
    ptr: NonNull<u8>,
    cap: usize,
    used: usize,
}

impl Chunk {
    fn new(cap: usize) -> Option<Chunk> {
        let layout = Layout::from_size_align(cap.max(1), CHUNK_ALIGN).ok()?;
        // SAFETY: the layout has non-zero size.
        let p = unsafe { alloc::alloc(layout) };
        NonNull::new(p).map(|ptr| Chunk {
            ptr,
            cap: layout.size(),
            used: 0,
        })
    }

    /// Bump inside this chunk. All arithmetic is checked; `None` means "does not fit".
    fn try_bump(&mut self, l: Layout) -> Option<NonNull<u8>> {
        let base = self.ptr.as_ptr() as usize;
        let cur = base.checked_add(self.used)?;
        let aligned = cur.checked_add(l.align() - 1)? & !(l.align() - 1);
        let end = aligned.checked_add(l.size())?;
        if end > base + self.cap {
            return None; // base + cap cannot overflow: it is one live allocation
        }
        let off = aligned - base;
        self.used = end - base;
        // SAFETY: off <= end - base <= cap, so the result is inside (or one past) the chunk's allocation.
        Some(unsafe { NonNull::new_unchecked(self.ptr.as_ptr().add(off)) })
    }
}

impl Drop for Chunk {
    fn drop(&mut self) {
        let layout = Layout::from_size_align(self.cap, CHUNK_ALIGN).unwrap();
        // SAFETY: allocated in `Chunk::new` with exactly this size and alignment.
        unsafe { alloc::dealloc(self.ptr.as_ptr(), layout) }
    }
}

type DropFn = unsafe fn(*mut u8);
unsafe fn drop_erased<T>(p: *mut u8) {
    // SAFETY: caller guarantees `p` points to a live, initialized T that is dropped exactly once.
    unsafe { std::ptr::drop_in_place(p.cast::<T>()) }
}

struct Inner {
    chunks: Vec<Chunk>,
    next_size: usize,
    total: usize,
    limit: usize,
    drops: Vec<(NonNull<u8>, DropFn)>,
}

#[derive(Clone, Copy, Debug)]
struct Checkpoint {
    chunks: usize,
    used: usize,
    drops: usize,
}

struct Arena {
    inner: RefCell<Inner>,
    growable: bool,
}

impl Arena {
    /// Fixed capacity: exhaustion returns `None`.
    fn fixed(cap: usize) -> Arena {
        Self::build(cap, false, cap)
    }
    /// First chunk `first` bytes, doubling, at most `limit` bytes in total.
    fn growing(first: usize, limit: usize) -> Arena {
        Self::build(first, true, limit)
    }
    fn build(first: usize, growable: bool, limit: usize) -> Arena {
        let c = Chunk::new(first).expect("initial chunk");
        let total = c.cap;
        let inner = Inner {
            chunks: vec![c],
            next_size: first.saturating_mul(2),
            total,
            limit,
            drops: Vec::new(),
        };
        Arena {
            inner: RefCell::new(inner),
            growable,
        }
    }

    fn alloc(&self, l: Layout) -> Option<NonNull<u8>> {
        let mut g = self.inner.borrow_mut();
        if let Some(p) = g.chunks.last_mut().and_then(|c| c.try_bump(l)) {
            return Some(p);
        }
        if !self.growable {
            return None;
        }
        let want = l.size().checked_add(l.align())?;
        let size = g.next_size.max(want);
        if g.total.checked_add(size)? > g.limit {
            return None;
        }
        let mut c = Chunk::new(size)?;
        let p = c.try_bump(l)?;
        g.total += c.cap;
        g.next_size = size.saturating_mul(2).min(1 << 26);
        g.chunks.push(c);
        Some(p)
    }

    fn try_alloc_value<T>(&self, v: T) -> Option<&mut T> {
        let p = self.alloc(Layout::new::<T>())?.cast::<T>().as_ptr();
        // SAFETY: `p` is aligned for T, has room for a T, and no other allocation overlaps it; the memory lives until
        // `rewind`/`reset`/drop, which need `&mut self` and so cannot happen while this borrow of `self` lasts.
        unsafe {
            p.write(v);
            Some(&mut *p)
        }
    }
    fn alloc_value<T>(&self, v: T) -> &mut T {
        match self.try_alloc_value(v) {
            Some(r) => r,
            None => panic!("arena exhausted"),
        }
    }

    /// Like `try_alloc_value`, but registers the destructor to run (reverse order) on rewind/reset/drop.
    fn alloc_with_drop<T>(&self, v: T) -> Option<&mut T> {
        let r = self.try_alloc_value(v)?;
        if std::mem::needs_drop::<T>() {
            let p = NonNull::from(&mut *r).cast::<u8>();
            self.inner.borrow_mut().drops.push((p, drop_erased::<T>));
        }
        Some(r)
    }

    fn alloc_slice_copy<T: Copy>(&self, src: &[T]) -> Option<&mut [T]> {
        let p = self
            .alloc(Layout::array::<T>(src.len()).ok()?)?
            .cast::<T>()
            .as_ptr();
        // SAFETY: room for `len` Ts at an aligned address; src and the fresh memory cannot overlap; T: Copy so no drop glue.
        unsafe {
            std::ptr::copy_nonoverlapping(src.as_ptr(), p, src.len());
            Some(std::slice::from_raw_parts_mut(p, src.len()))
        }
    }
    fn alloc_slice_fill_with<T>(
        &self,
        n: usize,
        mut f: impl FnMut(usize) -> T,
    ) -> Option<&mut [T]> {
        let p = self
            .alloc(Layout::array::<T>(n).ok()?)?
            .cast::<T>()
            .as_ptr();
        for i in 0..n {
            // SAFETY: slot i is inside the fresh, aligned region of n Ts.
            unsafe { p.add(i).write(f(i)) };
        }
        // SAFETY: all n elements were initialized above.
        Some(unsafe { std::slice::from_raw_parts_mut(p, n) })
    }

    fn checkpoint(&self) -> Checkpoint {
        let g = self.inner.borrow();
        Checkpoint {
            chunks: g.chunks.len(),
            used: g.chunks.last().unwrap().used,
            drops: g.drops.len(),
        }
    }
    fn run_drops(g: &mut Inner, to: usize) {
        while g.drops.len() > to {
            let (p, f) = g.drops.pop().unwrap();
            // SAFETY: registered by alloc_with_drop for an initialized value, popped (so run once). No reference to it can be
            // alive: callers hold `&mut Arena` (rewind) or are in Drop.
            unsafe { f(p.as_ptr()) }
        }
    }
    fn rewind(&mut self, cp: Checkpoint) {
        let g = self.inner.get_mut();
        assert!(
            cp.chunks >= 1 && cp.chunks <= g.chunks.len() && cp.drops <= g.drops.len(),
            "stale checkpoint"
        );
        Self::run_drops(g, cp.drops);
        while g.chunks.len() > cp.chunks {
            g.total -= g.chunks.pop().unwrap().cap;
        }
        g.chunks.last_mut().unwrap().used = cp.used;
    }
    fn reset(&mut self) {
        self.rewind(Checkpoint {
            chunks: 1,
            used: 0,
            drops: 0,
        });
    }
    /// Scoped arena: everything allocated inside `f` is released afterwards. `R` cannot borrow from the arena.
    fn scope<R>(&mut self, f: impl FnOnce(&Arena) -> R) -> R {
        let cp = self.checkpoint();
        let r = f(self);
        self.rewind(cp);
        r
    }
    fn chunk_count(&self) -> usize {
        self.inner.borrow().chunks.len()
    }
    fn used_bytes(&self) -> usize {
        self.inner.borrow().chunks.iter().map(|c| c.used).sum()
    }
}

impl Drop for Arena {
    fn drop(&mut self) {
        let g = self.inner.get_mut();
        Self::run_drops(g, 0); // before the chunks are freed by field drop
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
}

struct Noisy(u32, Rc<RefCell<Vec<u32>>>);
impl Drop for Noisy {
    fn drop(&mut self) {
        self.1.borrow_mut().push(self.0);
    }
}

fn main() {
    // 1. alignment + no overlap + no corruption over random layouts, across chunk growth
    let arena = Arena::growing(256, 64 << 20);
    let mut rng = Rng(0x1234_5678_9abc_def1);
    let mut live: Vec<(usize, usize, u8)> = Vec::new();
    for i in 0..20_000u32 {
        let align = 1usize << (rng.next() % 7); // 1..=64
        let size = (rng.next() % 200) as usize;
        let l = Layout::from_size_align(size, align).unwrap();
        let p = arena.alloc(l).expect("growing arena");
        assert_eq!(p.as_ptr() as usize % align, 0, "misaligned");
        let tag = (i % 251) as u8;
        // SAFETY: p points to `size` writable bytes just handed out.
        unsafe { std::ptr::write_bytes(p.as_ptr(), tag, size) };
        live.push((p.as_ptr() as usize, size, tag));
    }
    for &(addr, size, tag) in &live {
        // SAFETY: the arena is alive and each region was fully initialised with `tag`.
        let s = unsafe { std::slice::from_raw_parts(addr as *const u8, size) };
        assert!(s.iter().all(|&b| b == tag), "corruption");
    }
    let mut iv: Vec<(usize, usize)> = live.iter().map(|&(a, s, _)| (a, a + s)).collect();
    iv.sort();
    assert!(iv.windows(2).all(|w| w[0].1 <= w[1].0), "overlap");
    println!(
        "random layouts: 20000 aligned, disjoint, intact; {} chunks, {} bytes used",
        arena.chunk_count(),
        arena.used_bytes()
    );
    drop(arena);

    // 2. exhaustion and overflow
    let fixed = Arena::fixed(256);
    let mut n = 0;
    while fixed
        .alloc(Layout::from_size_align(16, 16).unwrap())
        .is_some()
    {
        n += 1;
    }
    assert_eq!(n, 16);
    let huge = Layout::from_size_align(isize::MAX as usize - 63, 64).unwrap();
    assert!(Arena::fixed(64).alloc(huge).is_none());
    assert!(Arena::growing(64, usize::MAX).alloc(huge).is_none()); // checked_add / allocator failure, no wraparound
    assert!(
        Arena::growing(64, 1024)
            .alloc(Layout::from_size_align(4096, 8).unwrap())
            .is_none()
    ); // over limit
    assert!(fixed.try_alloc_value(7u8).is_none());
    println!("exhaustion: fixed arena held exactly {n} x16B; huge/over-limit layouts -> None");

    // 3. checkpoints / scoped arena: memory is reused, typed values and slices work
    let mut a = Arena::growing(128, 1 << 20);
    let keep = a.alloc_value(41u64);
    *keep += 1;
    assert_eq!(*keep, 42); // `keep` must be dead before `a.scope(..)`/`rewind` (&mut self): the borrow checker enforces it
    let cp0 = a.checkpoint();
    let used0 = a.used_bytes();
    let first_addr = a.scope(|s| {
        let xs = s.alloc_slice_copy(&[1u32, 2, 3, 4]).unwrap();
        let sq = s.alloc_slice_fill_with(1000, |i| (i * i) as u64).unwrap(); // forces growth beyond 128 B
        assert_eq!((xs.iter().sum::<u32>(), sq[31]), (10, 961));
        assert!(s.chunk_count() > 1);
        xs.as_ptr() as usize
    });
    assert_eq!((a.chunk_count(), a.used_bytes()), (1, used0));
    let again = a.scope(|s| s.alloc_slice_copy(&[9u32]).unwrap().as_ptr() as usize);
    assert_eq!(first_addr, again, "rewound memory is reused");
    a.rewind(cp0);
    a.reset();
    assert_eq!(a.used_bytes(), 0);
    println!("checkpoints: scope released growth chunks, same address reused after rewind");

    // 4. drop glue: plain alloc leaks, alloc_with_drop runs destructors in reverse
    let rc = Rc::new(());
    {
        let arena = Arena::fixed(1024);
        arena.alloc_value(rc.clone());
    }
    assert_eq!(
        Rc::strong_count(&rc),
        2,
        "alloc_value never drops: the clone leaked"
    );
    let log = Rc::new(RefCell::new(Vec::new()));
    {
        let mut arena = Arena::fixed(1024);
        for i in 0..3 {
            arena.alloc_with_drop(Noisy(i, log.clone())).unwrap();
        }
        let cp = arena.checkpoint();
        arena.alloc_with_drop(Noisy(10, log.clone())).unwrap();
        arena.alloc_with_drop(Noisy(11, log.clone())).unwrap();
        arena.rewind(cp);
        assert_eq!(*log.borrow(), vec![11, 10]);
    }
    assert_eq!(*log.borrow(), vec![11, 10, 2, 1, 0]);
    assert_eq!(Rc::strong_count(&log), 1);
    println!(
        "drop glue: alloc_value leaked an Rc clone; alloc_with_drop ran drops {:?}",
        log.borrow()
    );

    // 5. throughput (printed, not asserted: machine dependent)
    const N: usize = 2_000_000;
    let t = Instant::now();
    let mut ar = Arena::growing(1 << 20, 1 << 30);
    let mut sum = 0u64;
    for round in 0..4 {
        for i in 0..N / 4 {
            let p = ar.alloc_value((i as u64, round as u64));
            sum = sum.wrapping_add(black_box(p).0);
        }
        ar.reset();
    }
    let t_arena = t.elapsed();
    let t = Instant::now();
    for round in 0..4 {
        let mut keep: Vec<Box<(u64, u64)>> = Vec::with_capacity(N / 4);
        for i in 0..N / 4 {
            let b = Box::new((i as u64, round as u64));
            sum = sum.wrapping_add(black_box(&b).0);
            keep.push(b);
        }
    }
    let t_box = t.elapsed();
    println!(
        "{N} allocs of 16 B: arena {t_arena:?} vs Box::new {t_box:?} (x{:.1}) [{}]",
        t_box.as_secs_f64() / t_arena.as_secs_f64(),
        sum & 1
    );
    println!("ok");
}
