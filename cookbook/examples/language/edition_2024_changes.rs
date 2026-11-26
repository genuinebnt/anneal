//! RECIPE 203 (rust-features): what EDITION 2024 changed and how it shows up in real code (before/after, with assertions).
//! LEVEL: intermediate
//! Use in: migrating 2021 crates (`cargo fix --edition`), reviewing 2024 code, explaining odd drop orders and lifetimes.   Run: cargo run --release --example edition_2024_changes
#![allow(dead_code)]
//!
//! This crate is edition 2024, so the AFTER behaviour is executed and asserted; the BEFORE behaviour was checked by compiling the
//! same snippet with `rustc --edition 2021` and is recorded in comments. Changes covered: RPIT lifetime capture, `if let` and
//! tail-expression temporary scopes, `unsafe_op_in_unsafe_fn`, `unsafe extern` / `unsafe(no_mangle)`, `gen` keyword, never-type
//! fallback, `Box<[T]>` by-value `IntoIterator`, `Future`/`IntoFuture` in the prelude, `static mut` references, Cargo resolver 3.
//!
//! COMMON USES: finding which of your Drop guards (MutexGuard, RefCell borrow, span guards) now release earlier; fixing
//! "does not live long enough" after upgrading; keeping FFI declarations honest; preparing for generators (`gen`).
//! Migration: `cargo fix --edition` rewrites most of this mechanically, then set `edition = "2024"` and rerun tests.
//! Read: https://doc.rust-lang.org/edition-guide/rust-2024/index.html
//! Read: https://doc.rust-lang.org/edition-guide/editions/transitioning-an-existing-project-to-a-new-edition.html
//! Read: https://doc.rust-lang.org/edition-guide/rust-2024/rpit-lifetime-capture.html
//! Read: https://doc.rust-lang.org/edition-guide/rust-2024/temporary-if-let-scope.html
//! Read: https://doc.rust-lang.org/edition-guide/rust-2024/temporary-tail-expr-scope.html
use std::cell::RefCell;

/// Records drops into a shared log so we can observe ORDER, the thing editions changed.
struct Noisy<'a> {
    name: &'static str,
    log: &'a RefCell<Vec<&'static str>>,
}
impl<'a> Noisy<'a> {
    fn new(name: &'static str, log: &'a RefCell<Vec<&'static str>>) -> Self {
        Noisy { name, log }
    }
    fn lookup(&self) -> Option<u32> {
        None
    }
    fn value(&self) -> usize {
        1
    }
}
impl Drop for Noisy<'_> {
    fn drop(&mut self) {
        self.log.borrow_mut().push(self.name);
    }
}

// ---------- 1. RPIT lifetime capture ----------
// 2021: `-> impl Trait` captured only type parameters (and lifetimes written in the bounds). A function returning an iterator over
//       its `&Vec` argument needed `+ '_` or `+ use<..>`-style hacks (`Captures` trait). Without it: error[E0700] "hidden type captures lifetime".
// 2024: it captures ALL in-scope lifetimes automatically. Good for borrowing returns, but it can over-capture:
fn evens(v: &Vec<u32>) -> impl Iterator<Item = u32> {
    v.iter().copied().filter(|n| n % 2 == 0) // 2021 would need `+ '_`
}
// Over-capture: the result below borrows `label` even though it does not use it. `use<>` opts out (captures nothing).
fn make_adder(base: u32, label: &str) -> impl Fn(u32) -> u32 + use<> {
    let _ = label.len();
    move |x| x + base
}
fn rpit_capture() {
    let v = vec![1, 2, 3, 4];
    assert_eq!(evens(&v).collect::<Vec<_>>(), [2, 4]);
    let add = {
        let temp = String::from("scoped");
        make_adder(10, &temp) // 2024 without `use<>` would error E0597: `temp` does not live long enough
    };
    assert_eq!(add(5), 15);
}

// ---------- 2. `if let` temporary scope ----------
// 2021: the scrutinee's temporaries lived to the END of the whole `if let ... else`, so a guard/borrow was still held in `else`
//       (classic deadlock: `if let Some(x) = m.lock().unwrap().get() {..} else { m.lock() }`).
// 2024: temporaries are dropped BEFORE the `else` branch runs. Verified drop logs: 2021 ["else","scrut"]; 2024 ["scrut","else"].
fn if_let_scope() {
    let log = RefCell::new(vec![]);
    if let Some(_) = Noisy::new("scrut", &log).lookup() {
    } else {
        log.borrow_mut().push("else");
    }
    assert_eq!(*log.borrow(), ["scrut", "else"]);
}

// ---------- 3. tail-expression temporary scope ----------
// 2021: temporaries in a block's final expression were dropped AFTER the block's locals, so they could outlive (and borrow) a local
//       that was already gone: the famous `fn f() -> usize { let c = RefCell::new(..); c.borrow().len() }` error[E0597].
// 2024: they are dropped BEFORE locals. Verified: 2021 ["body-end","local","temp"]; 2024 ["body-end","temp","local"].
fn tail_scope(log: &RefCell<Vec<&'static str>>) -> usize {
    let _local = Noisy::new("local", log);
    Noisy::new("temp", log).value() + {
        log.borrow_mut().push("body-end");
        0
    }
}
fn tail_borrow() -> usize {
    let cell = RefCell::new(vec![1, 2, 3]);
    cell.borrow().len() // compiles in 2024; in 2021 the `Ref` temp outlived `cell`: error[E0597]
}
fn temp_scopes() {
    let log = RefCell::new(vec![]);
    assert_eq!(tail_scope(&log), 1);
    assert_eq!(*log.borrow(), ["body-end", "temp", "local"]);
    assert_eq!(tail_borrow(), 3);
}

// ---------- 4. `unsafe_op_in_unsafe_fn` (warn by default) ----------
// 2021: the body of an `unsafe fn` was one big implicit unsafe block. 2024: unsafe operations need their own `unsafe { }` with a
// SAFETY comment (verified: a bare `*p` here warns E0133 "dereference of raw pointer is unsafe").
/// # Safety
/// `p` must be valid for reads.
unsafe fn read_byte(p: *const u8) -> u8 {
    // SAFETY: the caller guarantees `p` is valid.
    unsafe { *p }
}

// ---------- 5. `unsafe extern`, `unsafe(no_mangle)`, `static mut` ----------
// 2021 allowed `extern "C" { fn f(); }` and `#[no_mangle]`; 2024 errors: "extern blocks must be unsafe" and
// "unsafe attribute used without unsafe". `safe fn` marks audited declarations.
unsafe extern "C" {
    safe fn labs(x: std::ffi::c_long) -> std::ffi::c_long;
}
#[unsafe(export_name = "cookbook_r203_hook")]
pub extern "C" fn hook() -> u32 {
    7
}
// `static mut`: `&mut COUNTER` is a hard error by default in 2024 (lint `static_mut_refs` = deny). Use raw pointers or an atomic.
static mut COUNTER: u32 = 0;
fn bump_counter() -> u32 {
    // SAFETY: single-threaded demo; a raw `&raw mut` pointer creates no reference. Prefer `AtomicU32` in real code.
    unsafe {
        let p = &raw mut COUNTER;
        *p += 1;
        *p
    }
}
fn ffi_and_statics() {
    assert_eq!(labs(-4), 4);
    assert_eq!(hook(), 7);
    assert_eq!((bump_counter(), bump_counter()), (1, 2));
    let b = 9u8;
    assert_eq!(unsafe { read_byte(&b) }, 9);
}

// ---------- 6. `gen` is a reserved keyword ----------
// 2021: `fn gen()` or `let gen = ...` was fine; 2024: error "expected identifier, found reserved keyword `gen`". Reserved for
// future generator blocks. Fix with a raw identifier (rand crate's `rng.gen()` became `r#gen()` / `random()`).
struct Rng(u32);
impl Rng {
    fn r#gen(&mut self) -> u32 {
        self.0 = self.0.wrapping_mul(1_103_515_245).wrapping_add(12345);
        self.0
    }
}
fn gen_keyword() {
    let mut rng = Rng(1);
    let r#gen = rng.r#gen();
    assert_eq!(r#gen, 1_103_527_590);
}

// ---------- 7. never-type fallback ----------
// 2021: an unconstrained inference variable that came from a diverging expression fell back to `()`. 2024: falls back to `!`.
// Code that relied on `T = ()` now fails (verified): `conv(|| panic!())` below with `T: Tr` implemented only for `()` is, in 2021,
// accepted (with lint dependency_on_unit_never_type_fallback) and in 2024 error[E0277]: `!: Tr` is not satisfied.
// Fix: name the type: `conv::<()>(|| panic!())`.
trait Tr {
    fn n() -> &'static str;
}
impl Tr for () {
    fn n() -> &'static str {
        "unit"
    }
}
fn conv<T: Tr>(_f: impl FnOnce() -> T) -> &'static str {
    T::n()
}
fn never_fallback() {
    assert_eq!(conv::<()>(|| panic!("never called")), "unit"); // explicit type is the migration fix
    let r: Result<(), String> = Ok(());
    assert!(r.is_ok());
}

// ---------- 8. Box<[T]> IntoIterator by value ----------
// 2021: `boxed.into_iter()` resolved to `<&[T]>` (yielding `&T`) for backwards compatibility, with a lint. 2024: yields `T`.
fn box_slice() {
    let boxed: Box<[String]> = vec!["a".to_string(), "b".to_string()].into_boxed_slice();
    let owned: Vec<String> = boxed.into_iter().collect(); // Strings moved out, no clone
    assert_eq!(owned, ["a", "b"]);
    for s in vec![String::from("x")].into_boxed_slice() {
        assert_eq!(s, "x"); // by value in a `for` loop too
    }
}

// ---------- 9. Future and IntoFuture in the prelude ----------
// 2021 needed `use std::future::{Future, IntoFuture};` (verified: error[E0405] cannot find trait `Future`). 2024: in scope.
// Caution: a locally defined trait named `Future` can now be ambiguous with the prelude one; rename or import explicitly.
fn poll_once<F: Future<Output = u8>>(f: F) -> u8 {
    let mut f = std::pin::pin!(f);
    let mut cx = std::task::Context::from_waker(std::task::Waker::noop());
    match f.as_mut().poll(&mut cx) {
        std::task::Poll::Ready(v) => v,
        std::task::Poll::Pending => 0,
    }
}
fn prelude_future() {
    assert_eq!(poll_once(async { 21 * 2 }), 42);
    fn takes_into<F: IntoFuture<Output = u8>>(f: F) -> F::IntoFuture {
        f.into_future()
    }
    assert_eq!(poll_once(takes_into(async { 5 })), 5);
}

// ---------- 10. Cargo: resolver 3 and rust-version ----------
// `edition = "2024"` implies `resolver = "3"`: dependency resolution prefers versions whose `rust-version` is compatible with your
// package's `rust-version` (the "MSRV-aware resolver"), so `cargo update` stops picking crates your compiler cannot build.
// Set `package.rust-version` to your real MSRV and keep a CI job on that toolchain. Also changed in 2024 (see the edition guide):
// `std::env::set_var`/`remove_var` are `unsafe` (not thread-safe on Unix), shown below.
fn env_vars_are_unsafe() {
    // 2024: `set_var` is `unsafe` (not thread-safe on Unix). Verified by this compiling only inside `unsafe`.
    // SAFETY: no other threads read the environment in this demo.
    unsafe { std::env::set_var("COOKBOOK_R203", "1") };
    assert_eq!(std::env::var("COOKBOOK_R203").as_deref(), Ok("1"));
}

fn main() {
    let demos: [(&str, fn()); 10] = [
        ("RPIT capture + use<>", rpit_capture),
        ("if-let temporaries", if_let_scope),
        ("tail-expr temporaries", temp_scopes),
        ("unsafe fn bodies", || {
            assert_eq!(unsafe { read_byte(&3) }, 3)
        }),
        ("unsafe extern / static mut", ffi_and_statics),
        ("gen keyword", gen_keyword),
        ("never-type fallback", never_fallback),
        ("Box<[T]> by value", box_slice),
        ("Future in prelude", prelude_future),
        ("env::set_var unsafe", env_vars_are_unsafe),
    ];
    for (name, f) in demos {
        f();
        println!("ok  {name}");
    }
}
