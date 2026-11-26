//! "No allocation" is a claim, so count the allocations.

use cow_borrow_until_you_must_own::*;
use std::alloc::{GlobalAlloc, Layout, System};
use std::borrow::Cow;
use std::cell::Cell;
use std::hint::black_box;

struct Counting;
thread_local! {
    static ALLOCATIONS: Cell<usize> = const { Cell::new(0) };
}
// SAFETY: every call is forwarded unchanged to `System`; we only add a counter.
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        ALLOCATIONS.with(|n| n.set(n.get() + 1));
        unsafe { System.alloc(layout) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) }
    }
}
#[global_allocator]
static GLOBAL: Counting = Counting;

/// Allocations made while `f` runs. `black_box` stops the optimiser removing the work.
fn allocations<T>(f: impl FnOnce() -> T) -> usize {
    let before = ALLOCATIONS.with(Cell::get);
    let result = black_box(f());
    let after = ALLOCATIONS.with(Cell::get);
    drop(result);
    after - before
}

#[test]
fn the_borrowed_paths_do_not_allocate() {
    assert_eq!(allocations(|| escape_html(black_box("nothing to do"))), 0);
    assert_eq!(allocations(|| status_text(black_box(404))), 0);
    assert_eq!(allocations(|| sorted(black_box(&[1, 2, 3]))), 0);
    assert_eq!(allocations(|| String::from_utf8_lossy(black_box(b"ok"))), 0);
    let a: Cow<'_, str> = Cow::Borrowed("abc");
    assert_eq!(allocations(|| black_box(&a).clone()), 0);
}

#[test]
fn the_owned_paths_do() {
    assert_eq!(allocations(|| escape_html(black_box("a<b"))), 1);
    assert_eq!(allocations(|| status_text(black_box(418))).min(1), 1);

    let b: Cow<'_, str> = Cow::Borrowed("abc");
    assert_eq!(allocations(|| black_box(b).into_owned()), 1);
    let o: Cow<'_, str> = Cow::Owned("abc".to_string());
    assert_eq!(allocations(|| black_box(&o).clone()), 1);
    assert_eq!(allocations(|| black_box(&o).to_string()), 1);
    assert_eq!(allocations(|| black_box(o).into_owned()), 0);
}

#[test]
fn moving_and_reusing_buffers() {
    let s = "moved".to_string();
    assert_eq!(allocations(|| Cow::<str>::from(black_box(s))), 0);
    let bytes = b"valid".to_vec();
    assert_eq!(allocations(|| lossy_from_vec(black_box(bytes))), 0);
    let bytes = b"valid".to_vec();
    assert_eq!(
        allocations(|| String::from_utf8_lossy(black_box(&bytes)).into_owned()),
        1
    );

    let mut out = String::with_capacity(64);
    assert_eq!(
        allocations(|| {
            for _ in 0..3 {
                out.clear();
                push_escaped(black_box("a<b"), &mut out);
            }
        }),
        0,
        "a reused buffer needs no new memory, even for dirty input"
    );
}
