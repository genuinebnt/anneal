//! RECIPE 200 (rust-features): recent stable LANGUAGE features, each with a realistic use case and the workaround it replaces.
//! LEVEL: intermediate
//! Use in: modernising older code, code review of "new" syntax, choosing which features to adopt.   Run: cargo run --release --example recent_stable_language_features
#![allow(dead_code)]
//!
//! Every feature below was compiled on rustc 1.98.1 (edition 2024) without `#![feature]`. Version numbers in comments come from
//! the release notes (https://doc.rust-lang.org/stable/releases.html); "recent" means the notes were not checked for that item.
//! Each feature lives in its own small function with an assert, so you can lift one out and read it alone.
//!
//! How: `main` calls every demo function and prints one line each. A failing assert means the feature misbehaves.
//!
//! COMMON USES: config/request validation chains (let chains), callbacks that await (async closures), plugin registries
//! (trait upcasting), lint hygiene in large workspaces (#[expect]), array initialisation of non-Copy types (const blocks),
//! FFI and syscalls (unsafe extern, c"" literals), lookup of "optional but must satisfy" fields (is_none_or), parsers
//! (range patterns, let-else), early-exit computations (labeled blocks), API design that avoids over-capturing (use<>),
//! binary layouts (offset_of!), compile-time tables (const fn).
//! Read: https://doc.rust-lang.org/edition-guide/rust-2024/index.html
//! Read: https://doc.rust-lang.org/stable/releases.html
//! Read: https://doc.rust-lang.org/std/
use std::any::Any;
use std::collections::HashMap;
use std::ffi::CStr;
use std::future::Future;
use std::mem::offset_of;
use std::pin::pin;
use std::task::{Context, Poll, Waker};

// ---------- 1. let chains (stable in edition 2024, release 1.88) ----------
// Replaces: nested `if let` pyramids, or `match` with guards, or a helper fn returning Option just to use `?`.
fn effective_port(cfg: &HashMap<&str, &str>) -> Option<u16> {
    if let Some(raw) = cfg.get("port")
        && let Ok(p) = raw.trim().parse::<u16>()
        && p >= 1024
    {
        Some(p)
    } else {
        None
    }
}
fn drain_pairs(mut it: impl Iterator<Item = i32>) -> i32 {
    let mut sum = 0;
    // `while let ... && let ...` also works: stop at the first unpaired item.
    while let Some(a) = it.next()
        && let Some(b) = it.next()
    {
        sum += a * b;
    }
    sum
}
fn let_chains() {
    let cfg = HashMap::from([("port", " 8080 ")]);
    assert_eq!(effective_port(&cfg), Some(8080));
    assert_eq!(effective_port(&HashMap::from([("port", "80")])), None);
    assert_eq!(drain_pairs([1, 2, 3, 4, 5].into_iter()), 1 * 2 + 3 * 4);
}

// ---------- 1b. if-let guards on match arms (1.95) ----------
// Replaces: a nested `match`/`if let` inside the arm followed by duplicated fallthrough code.
fn classify(input: &str) -> &'static str {
    match input.split_once('=') {
        Some((k, v))
            if let Ok(n) = v.parse::<i64>()
                && n < 0 =>
        {
            if k.is_empty() {
                "neg-anon"
            } else {
                "negative"
            }
        }
        Some((_, v)) if let Ok(_) = v.parse::<i64>() => "number",
        Some(_) => "text",
        None => "bare",
    }
}
fn if_let_guards() {
    assert_eq!(classify("a=-5"), "negative");
    assert_eq!(classify("a=5"), "number");
    assert_eq!(classify("a=x"), "text");
    assert_eq!(classify("a"), "bare");
}

// ---------- 2. async closures + AsyncFn* bounds (recent) ----------
// Replaces: `F: Fn(T) -> Fut, Fut: Future` with a separate Fut type parameter, which could not let the future borrow from the
// closure's captured state or its argument.
async fn retry<F>(mut attempt: F, max: u32) -> Result<u32, String>
where
    F: AsyncFnMut(u32) -> Result<u32, String>,
{
    let mut last = Err("never ran".to_string());
    for n in 1..=max {
        last = attempt(n).await;
        if last.is_ok() {
            break;
        }
    }
    last
}
async fn apply_to_name<F: AsyncFn(&str) -> usize>(f: F) -> usize {
    f("hello").await + f("hi").await // the future borrows the `&str` argument: impossible with Fn -> Fut
}
fn block_on<F: Future>(fut: F) -> F::Output {
    let mut fut = pin!(fut);
    let mut cx = Context::from_waker(Waker::noop()); // Waker::noop: recent
    loop {
        if let Poll::Ready(v) = fut.as_mut().poll(&mut cx) {
            return v;
        }
    }
}
fn async_closures() {
    let mut calls = Vec::new();
    let r = block_on(retry(
        async |n| {
            calls.push(n); // captures `calls` by mutable reference across awaits
            if n < 3 {
                Err(format!("fail {n}"))
            } else {
                Ok(n * 10)
            }
        },
        5,
    ));
    assert_eq!(r, Ok(30));
    assert_eq!(calls, [1, 2, 3]);
    let prefix = String::from("x");
    assert_eq!(
        block_on(apply_to_name(async |s: &str| s.len() + prefix.len())),
        5 + 1 + 2 + 1
    );
}

// ---------- 3. trait object upcasting (recent) ----------
// Replaces: an `as_base(&self) -> &dyn Base` boilerplate method on every subtrait.
trait Component: Any {
    fn name(&self) -> String;
}
trait Widget: Component {
    fn render(&self) -> String;
}
struct Button;
impl Component for Button {
    fn name(&self) -> String {
        "button".into()
    }
}
impl Widget for Button {
    fn render(&self) -> String {
        "[ok]".into()
    }
}
fn register(c: &dyn Component) -> String {
    c.name()
}
fn trait_upcasting() {
    let w: Box<dyn Widget> = Box::new(Button);
    let c: &dyn Component = &*w; // implicit upcast: &dyn Widget -> &dyn Component
    assert_eq!(register(c), "button");
    let any: Box<dyn Any> = w; // supertrait of Component is Any, so even downcasting works
    assert!(any.downcast::<Button>().is_ok());
}

// ---------- 4. #[expect(lint)] (recent) ----------
// Replaces: `#[allow(...)]` that silently outlives the code it excused. `expect` warns once the lint no longer fires.
#[expect(clippy::needless_return, reason = "documenting style")] // unfulfilled clippy lints are only checked under clippy
fn expect_lint() -> i32 {
    #[expect(unused_variables, reason = "kept for the debugger")]
    let scratch = 5;
    7
}

// ---------- 5. inline const blocks (recent) ----------
// Replaces: `[None; N]` workarounds for non-Copy types (`Default::default()` loops or a named `const EMPTY`).
fn const_blocks() {
    let v: [Vec<u8>; 3] = [const { Vec::new() }; 3];
    assert!(v.iter().all(Vec::is_empty));
    let table = const { 1u32 << 10 } + const { u32::MAX.count_ones() };
    assert_eq!(table, 1024 + 32);
    const { assert!(size_of::<u64>() == 8) }; // compile-time invariant at the use site
}

// ---------- 6. `unsafe extern` and `#[unsafe(no_mangle)]` (required spelling in edition 2024) ----------
// Replaces: `extern "C" { fn strlen(..); }` where every declaration was implicitly unsafe to call. `safe fn` lets you mark
// declarations you have audited as callable without `unsafe`.
unsafe extern "C" {
    pub safe fn abs(x: i32) -> i32; // audited: pure function, no pointers
    pub unsafe fn strlen(s: *const std::ffi::c_char) -> usize;
}
#[unsafe(no_mangle)] // the attribute can break linking/ABI, so it must be wrapped in unsafe(..)
pub extern "C" fn cookbook_exported_add(a: i32, b: i32) -> i32 {
    a + b
}
fn unsafe_extern() {
    assert_eq!(abs(-9), 9); // no unsafe block needed
    let s = c"flowforge";
    assert_eq!(unsafe { strlen(s.as_ptr()) }, 9);
    assert_eq!(cookbook_exported_add(2, 3), 5);
}

// ---------- 7. C-string literals c"..." (recent) ----------
// Replaces: `CString::new("...").unwrap()` or `b"...\0"` plus `from_bytes_with_nul`: now a checked `&'static CStr`.
fn c_strings() {
    const NAME: &CStr = c"SO_REUSEPORT";
    assert_eq!(NAME.to_bytes().len(), 12);
    assert_eq!(NAME.to_bytes_with_nul().last(), Some(&0));
    assert_eq!(c"caf\xc3\xa9".to_str(), Ok("café"));
}

// ---------- 8. Option::is_none_or ----------
// Replaces: `opt.map_or(true, |v| ...)`, which reads backwards. Ideal for "if set, it must satisfy X" validation.
fn is_none_or() {
    let limit: Option<u32> = None;
    assert!(limit.is_none_or(|l| l <= 100));
    assert!(!Some(500u32).is_none_or(|l| l <= 100));
}

// ---------- 9. exclusive range patterns (recent) ----------
// Replaces: `0..=63 | ...` with off-by-one `..=` arithmetic when buckets are naturally half-open.
fn bucket(n: u32) -> &'static str {
    match n {
        0..64 => "small",
        64..1024 => "medium",
        1024.. => "large",
    }
}
fn range_patterns() {
    assert_eq!(
        [bucket(63), bucket(64), bucket(1023), bucket(1024)],
        ["small", "medium", "medium", "large"]
    );
}

// ---------- 10. let ... else combos ----------
// Replaces: `match { Some(x) => x, None => return ... }`; keeps the happy path unindented.
fn parse_header(line: &str) -> Result<(String, u32), String> {
    let Some((key, val)) = line.split_once(':') else {
        return Err(format!("no colon in `{line}`"));
    };
    let Ok(n) = val.trim().parse::<u32>() else {
        return Err(format!("`{}` is not a number", val.trim()));
    };
    let [first, ..] = key.as_bytes() else {
        return Err("empty key".into());
    }; // slice pattern + else
    assert!(first.is_ascii() || !key.is_empty());
    Ok((key.to_string(), n))
}
fn let_else() {
    assert_eq!(parse_header("len: 42"), Ok(("len".into(), 42)));
    assert!(
        parse_header("len 42").is_err()
            && parse_header("len: x").is_err()
            && parse_header(": 4").is_err()
    );
}

// ---------- 11. labeled block break-with-value (recent) ----------
// Replaces: a one-use closure or helper function just to get an early `return` out of a calculation.
fn labeled_blocks() {
    let words = ["alpha", "beta", "gamma"];
    let found = 'search: {
        for (i, w) in words.iter().enumerate() {
            if w.starts_with('g') {
                break 'search Some(i);
            }
        }
        None
    };
    assert_eq!(found, Some(2));
}

// ---------- 12. precise capturing `use<..>` (recent; in traits: 1.87) ----------
// Edition 2024 `impl Trait` captures ALL in-scope lifetimes. `use<>` opts out, so the result need not borrow the argument.
fn lengths<'a>(names: &'a [String], _scratch: &str) -> impl Iterator<Item = usize> + use<'a> {
    names.iter().map(String::len) // captures only the `names` lifetime, not `_scratch`
}
fn counter(start: u32, _label: &str) -> impl FnMut() -> u32 + use<> {
    let mut n = start; // captures nothing borrowed: returned closure is 'static
    move || {
        n += 1;
        n
    }
}
fn precise_capturing() {
    let names = vec!["ab".to_string(), "cde".to_string()];
    let it = {
        let tmp = String::from("temp");
        lengths(&names, &tmp) // `tmp` dies here but the iterator lives on
    };
    assert_eq!(it.collect::<Vec<_>>(), [2, 3]);
    let mut c = {
        let label = String::from("l");
        counter(10, &label)
    };
    assert_eq!((c(), c()), (11, 12));
}

// ---------- 13. offset_of! (recent) ----------
// Replaces: pointer arithmetic with `MaybeUninit` and `addr_of!` to compute field offsets for binary formats or FFI.
#[repr(C)]
struct Header {
    magic: u32,
    flags: u16,
    body: Body,
}
#[repr(C)]
struct Body {
    len: u64,
}
fn offsets() {
    assert_eq!(offset_of!(Header, magic), 0);
    assert_eq!(offset_of!(Header, flags), 4);
    assert_eq!(offset_of!(Header, body), 8);
    assert_eq!(offset_of!(Header, body.len), 8); // nested field path
}

// ---------- 14. const fn power-ups ----------
// Floats rounding in const (1.90: floor/ceil/round/trunc), `<[T]>::reverse` const (1.90), `const` blocks in macros (1.87),
// `Cell` in const (1.88), `char::is_control` const (1.97). Replaces: build.rs generated tables.
const fn ceil_div_f(a: f64, b: f64) -> u64 {
    (a / b).ceil() as u64
}
const fn reversed<const N: usize>(mut a: [u8; N]) -> [u8; N] {
    a.reverse();
    a
}
const PAGES: u64 = ceil_div_f(10_000.0, 4096.0);
const REV: [u8; 4] = reversed([1, 2, 3, 4]);
fn const_improvements() {
    assert_eq!(PAGES, 3);
    assert_eq!(REV, [4, 3, 2, 1]);
    assert!(const { '\n'.is_control() });
    let v = vec![const { 1 + 1 }; 2]; // `const {}` inside vec!/assert_eq! (1.87)
    assert_eq!(v, [2, 2]);
}

// ---------- 15. other verified additions ----------
// * `[T; _]` / `Vec::<_>`: explicitly inferred const args (1.89).
// * `#[cfg(true)]` boolean cfg literals (1.88) and `cfg_select!` (1.95): replaces the `cfg-if` crate.
// * `std::assert_matches!` (1.96): replaces `assert!(matches!(..))` and prints the actual value on failure.
// * `hint::cold_path` (1.95) / `hint::select_unpredictable` (1.88): branch layout hints for hot loops.
fn misc() {
    let arr: [u8; _] = [1, 2, 3]; // length inferred
    assert_eq!(arr.len(), 3);
    let os = cfg_select! {
        windows => "windows",
        unix => "unix",
        _ => "other",
    };
    assert!(["unix", "windows"].contains(&os));
    let r: Result<u8, ()> = Ok(4);
    std::assert_matches!(r, Ok(n) if n > 3);
    let x = std::hint::select_unpredictable(arr.len() > 2, 10, 20); // branchless select for random data
    if x == 0 {
        std::hint::cold_path(); // tells the optimiser this arm is rare
    }
    assert_eq!(x, 10);
}

fn main() {
    let demos: [(&str, fn()); 15] = [
        ("let chains", let_chains),
        ("if-let guards", if_let_guards),
        ("async closures", async_closures),
        ("trait upcasting", trait_upcasting),
        ("#[expect]", || assert_eq!(expect_lint(), 7)),
        ("const blocks", const_blocks),
        ("unsafe extern", unsafe_extern),
        ("c-strings", c_strings),
        ("is_none_or", is_none_or),
        ("range patterns", range_patterns),
        ("let else", let_else),
        ("labeled blocks", labeled_blocks),
        ("use<..>", precise_capturing),
        ("offset_of + const fn", || {
            offsets();
            const_improvements();
        }),
        ("misc", misc),
    ];
    for (name, f) in demos {
        f();
        println!("ok  {name}");
    }
}
