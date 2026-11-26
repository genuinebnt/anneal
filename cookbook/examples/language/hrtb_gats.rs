//! RECIPE 112 (traits): higher-ranked trait bounds (`for<'a>`) and generic associated types (lending iterators).
//! LEVEL: advanced
//! Use in: parser combinators, callbacks that borrow their argument, streaming/lending iterators, zero-copy APIs.   Run: cargo run --release --example hrtb_gats
#![allow(dead_code)]
//!
//! HRTB: `F: for<'a> Fn(&'a str) -> &'a str` means "F works for EVERY lifetime the caller picks, and the output borrows from the
//! input". Plain `Fn(&str) -> &str` is sugar for exactly that (elision), but you must write `for<'a>` when the lifetime appears
//! inside a trait bound on a type, in `dyn for<'a> Fn(..)`, or when a closure's inferred signature is too weak. A closure like
//! `|s| s.trim()` is inferred with ONE fixed lifetime unless something (a helper `fn bind<F: for<'a> Fn(&'a str)->&'a str>`)
//! pins the higher-ranked signature.
//! GATs: `type Item<'a> where Self: 'a;` lets an associated type depend on a lifetime chosen at each `next(&'a mut self)` call, so the
//! item may BORROW FROM THE ITERATOR ITSELF. `Iterator::next(&mut self) -> Option<Self::Item>` cannot do that: `Item` is fixed before
//! the `&mut self` borrow exists, so it cannot mention its lifetime. That is why `slice.windows_mut()` does not exist in std: two
//! overlapping `&mut [T]` windows alive at once (as `collect()` would require) would alias. A lending iterator forbids calling
//! `next` again while the previous item is alive, which makes overlapping mutable windows sound.
//! COMMON USES: nom-style parser combinators over `&str`/`&[u8]`, `tower`/`axum` handlers generic over borrowed requests, database
//! row cursors that reuse a buffer, streaming decoders, "lending" `read_line` loops that reuse one `String`.
//! Read: https://blog.rust-lang.org/2022/10/28/gats-stabilization.html , https://doc.rust-lang.org/nomicon/hrtb.html
//! Note: bounds like `for<'a> I::Item<'a>: Trait` on GATs currently force `I: 'static`; this recipe avoids them.

// ---------- HRTB: closures that return references tied to their argument ----------
fn bind<F: for<'a> Fn(&'a str) -> &'a str>(f: F) -> F {
    f
}

// ---------- HRTB: parser combinators ----------
type PResult<'a> = Option<(&'a str, &'a str)>; // (rest, matched)

trait Parser {
    fn parse<'a>(&self, input: &'a str) -> PResult<'a>;
}
impl<F> Parser for F
where
    F: for<'a> Fn(&'a str) -> PResult<'a>,
{
    fn parse<'a>(&self, input: &'a str) -> PResult<'a> {
        self(input)
    }
}

fn tag(t: &'static str) -> impl for<'a> Fn(&'a str) -> PResult<'a> {
    move |s| s.strip_prefix(t).map(|rest| (rest, &s[..t.len()]))
}
fn take_while(pred: fn(char) -> bool) -> impl for<'a> Fn(&'a str) -> PResult<'a> {
    move |s| {
        let end = s.find(|c| !pred(c)).unwrap_or(s.len());
        (end > 0).then(|| (&s[end..], &s[..end]))
    }
}
/// Run `p` then `q`; the match is the whole consumed span.
fn seq<P: Parser, Q: Parser>(p: P, q: Q) -> impl for<'a> Fn(&'a str) -> PResult<'a> {
    move |s| {
        let (r1, _) = p.parse(s)?;
        let (r2, _) = q.parse(r1)?;
        Some((r2, &s[..s.len() - r2.len()]))
    }
}
fn or<P: Parser, Q: Parser>(p: P, q: Q) -> impl for<'a> Fn(&'a str) -> PResult<'a> {
    move |s| p.parse(s).or_else(|| q.parse(s))
}
fn many0<P: Parser>(p: P) -> impl for<'a> Fn(&'a str) -> PResult<'a> {
    move |s| {
        let mut rest = s;
        while let Some((r, _)) = p.parse(rest) {
            if r.len() == rest.len() {
                break; // no progress: avoid an infinite loop
            }
            rest = r;
        }
        Some((rest, &s[..s.len() - rest.len()]))
    }
}

// ---------- GAT: lending iterator ----------
trait LendingIterator {
    type Item<'a>
    where
        Self: 'a;
    fn next<'a>(&'a mut self) -> Option<Self::Item<'a>>;

    fn take_n(self, n: usize) -> TakeN<Self>
    where
        Self: Sized,
    {
        TakeN {
            inner: self,
            left: n,
        }
    }
    // NOTE: a `for_each(F) where F: FnMut(Self::Item<'_>)` helper currently fails to compile at call sites: the implied
    // `for<'a> Self: 'a` forces `Self: 'static`. Drive lending iterators with `while let` instead.
}

/// Overlapping `&mut [T]` windows of length `size`.
struct WindowsMut<'t, T> {
    slice: &'t mut [T],
    start: usize,
    size: usize,
}
impl<'t, T> LendingIterator for WindowsMut<'t, T> {
    type Item<'a>
        = &'a mut [T]
    where
        Self: 'a;
    fn next<'a>(&'a mut self) -> Option<&'a mut [T]> {
        let w = self.slice.get_mut(self.start..self.start + self.size)?;
        self.start += 1;
        Some(w)
    }
}

/// A simple adapter: stop after `n` items. Its `Item<'a>` just forwards the inner GAT.
struct TakeN<I> {
    inner: I,
    left: usize,
}
impl<I: LendingIterator> LendingIterator for TakeN<I> {
    type Item<'a>
        = I::Item<'a>
    where
        Self: 'a;
    fn next<'a>(&'a mut self) -> Option<I::Item<'a>> {
        if self.left == 0 {
            return None;
        }
        self.left -= 1;
        self.inner.next()
    }
}

fn main() {
    // closures + HRTB
    let trim = bind(|s| s.trim());
    let owned = String::from("  hello  ");
    assert_eq!(trim(&owned), "hello");
    assert_eq!(trim("  x"), "x"); // same closure, different lifetimes: that is the point

    // dyn for<'a> Fn
    let fns: Vec<Box<dyn for<'a> Fn(&'a str) -> &'a str>> = vec![
        Box::new(|s| s.trim()),
        Box::new(|s| s.split(',').next().unwrap_or("")),
        Box::new(|s| &s[s.len().min(1)..]),
    ];
    let outs: Vec<&str> = fns.iter().map(|f| f(" a,b ")).collect();
    assert_eq!(outs, vec!["a,b", " a", "a,b "]);

    // parser combinators: ident = letters, then many0(",ident")
    fn alpha(c: char) -> bool {
        c.is_ascii_alphabetic()
    }
    let ident = take_while(alpha);
    let list = seq(take_while(alpha), many0(seq(tag(","), take_while(alpha))));
    assert_eq!(ident.parse("abc1"), Some(("1", "abc")));
    assert_eq!(list.parse("a,bc,d;rest"), Some((";rest", "a,bc,d")));
    assert_eq!(list.parse("1"), None);
    let kw = or(tag("let"), tag("var"));
    assert_eq!(kw.parse("var x"), Some((" x", "var")));
    assert_eq!(kw.parse("fn"), None);
    let boxed: Vec<Box<dyn Parser>> = vec![Box::new(tag("a")), Box::new(ident)];
    assert_eq!(boxed.iter().filter(|p| p.parse("abc").is_some()).count(), 2);

    // lending iterator: each window sees the previous window's writes (Fibonacci in place)
    let mut v = vec![1u64, 1, 0, 0, 0, 0, 0, 0];
    let mut w = WindowsMut {
        slice: &mut v,
        start: 0,
        size: 3,
    };
    while let Some(win) = w.next() {
        win[2] = win[0] + win[1];
    }
    assert_eq!(v, vec![1, 1, 2, 3, 5, 8, 13, 21]);

    // adapter + closure over lent items
    let mut data = [1, 2, 3, 4, 5];
    let mut visited = 0;
    let mut it = WindowsMut {
        slice: &mut data,
        start: 0,
        size: 2,
    }
    .take_n(3);
    while let Some(win) = it.next() {
        win[1] *= 10;
        visited += 1;
    }
    assert_eq!(visited, 3);
    assert_eq!(data, [1, 20, 30, 40, 5]);
    println!("HRTB + GAT recipe OK: fib in place = {v:?}");
}
