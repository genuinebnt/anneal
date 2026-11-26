//! RECIPE 83 (traits): extension traits, custom iterator adapters, and operator overloading on a generic type.
//! LEVEL: intermediate
//! Use in: adding methods to types you do not own (Iterator, str, Vec), math/vector-like types, ergonomic APIs.   Run: cargo run --release --example extension_traits_ops
//! COMMON USES: itertools-style helper methods, vector/matrix math types, domain-specific method sugar.
#![allow(dead_code)]
//!
//! Extension trait: define `trait IterExt: Iterator { fn my_adapter(self) -> MyAdapter<Self> }` and a blanket
//! `impl<I: Iterator> IterExt for I {}`: now EVERY iterator has `.my_adapter()`. That is exactly how `itertools` works
//! (https://docs.rs/itertools). Operators are traits too: `impl Add for V2<T>`; derive nothing, implement what makes sense.
//! Read: https://rust-lang.github.io/api-guidelines/predictability.html and https://doc.rust-lang.org/std/ops/index.html
use std::iter::Sum;
use std::ops::{Add, AddAssign, Index, Mul, Neg, Sub};

// ---------- a custom adapter + extension trait ----------
struct DedupAdjacent<I: Iterator> {
    iter: I,
    last: Option<I::Item>,
}
impl<I> Iterator for DedupAdjacent<I>
where
    I: Iterator,
    I::Item: PartialEq + Clone,
{
    type Item = I::Item;
    fn next(&mut self) -> Option<I::Item> {
        loop {
            let x = self.iter.next()?;
            if self.last.as_ref() != Some(&x) {
                self.last = Some(x.clone());
                return Some(x);
            }
        }
    }
}

struct Chunked<I: Iterator> {
    iter: I,
    n: usize,
}
impl<I: Iterator> Iterator for Chunked<I> {
    type Item = Vec<I::Item>;
    fn next(&mut self) -> Option<Vec<I::Item>> {
        let chunk: Vec<_> = self.iter.by_ref().take(self.n).collect();
        if chunk.is_empty() { None } else { Some(chunk) }
    }
}

trait IterExt: Iterator + Sized {
    fn dedup_adjacent(self) -> DedupAdjacent<Self>
    where
        Self::Item: PartialEq + Clone,
    {
        DedupAdjacent {
            iter: self,
            last: None,
        }
    }
    fn chunked(self, n: usize) -> Chunked<Self> {
        assert!(n > 0, "chunk size must be positive");
        Chunked { iter: self, n }
    }
    fn sorted(self) -> Vec<Self::Item>
    where
        Self::Item: Ord,
    {
        let mut v: Vec<_> = self.collect();
        v.sort();
        v
    }
}
impl<I: Iterator> IterExt for I {}

// ---------- extension trait on a foreign type (str) ----------
trait StrExt {
    fn is_slug(&self) -> bool;
    fn truncate_chars(&self, n: usize) -> &str;
}
impl StrExt for str {
    fn is_slug(&self) -> bool {
        !self.is_empty()
            && self
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
    }
    fn truncate_chars(&self, n: usize) -> &str {
        match self.char_indices().nth(n) {
            Some((i, _)) => &self[..i],
            None => self,
        }
    }
}

// ---------- operator overloading on a generic struct ----------
#[derive(Debug, Clone, Copy, PartialEq, Default)]
struct V2<T> {
    x: T,
    y: T,
}
impl<T: Add<Output = T>> Add for V2<T> {
    type Output = V2<T>;
    fn add(self, o: Self) -> Self::Output {
        V2 {
            x: self.x + o.x,
            y: self.y + o.y,
        }
    }
}
impl<T: Sub<Output = T>> Sub for V2<T> {
    type Output = V2<T>;
    fn sub(self, o: Self) -> Self::Output {
        V2 {
            x: self.x - o.x,
            y: self.y - o.y,
        }
    }
}
impl<T: Mul<Output = T> + Copy> Mul<T> for V2<T> {
    type Output = V2<T>; // scalar multiplication: V2<f64> * f64
    fn mul(self, k: T) -> Self::Output {
        V2 {
            x: self.x * k,
            y: self.y * k,
        }
    }
}
impl<T: Neg<Output = T>> Neg for V2<T> {
    type Output = V2<T>;
    fn neg(self) -> Self::Output {
        V2 {
            x: -self.x,
            y: -self.y,
        }
    }
}
impl<T: AddAssign> AddAssign for V2<T> {
    fn add_assign(&mut self, o: Self) {
        self.x += o.x;
        self.y += o.y;
    }
}
impl<T: Add<Output = T> + Default> Sum for V2<T> {
    fn sum<I: Iterator<Item = Self>>(iter: I) -> Self {
        iter.fold(V2::default(), |a, b| a + b) // lets `iter.sum::<V2<_>>()` work
    }
}
impl<T> From<(T, T)> for V2<T> {
    fn from((x, y): (T, T)) -> Self {
        V2 { x, y }
    }
}
impl<T> Index<usize> for V2<T> {
    type Output = T;
    fn index(&self, i: usize) -> &T {
        match i {
            0 => &self.x,
            1 => &self.y,
            _ => panic!("V2 index {i} out of range"),
        }
    }
}

fn main() {
    println!(
        "{:?}",
        [1, 1, 2, 2, 2, 3, 1, 1]
            .into_iter()
            .dedup_adjacent()
            .collect::<Vec<_>>()
    );
    println!(
        "{:?}",
        "aabbbc".chars().dedup_adjacent().collect::<String>()
    );
    println!("{:?}", (1..=7).chunked(3).collect::<Vec<_>>());
    println!("{:?}", ["pear", "apple", "fig"].into_iter().sorted());
    println!(
        "{} {} {:?} {:?}",
        "my-slug-2".is_slug(),
        "Not A Slug".is_slug(),
        "héllo wörld".truncate_chars(7),
        "short".truncate_chars(99)
    );

    let a = V2 { x: 1.0, y: 2.0 };
    let b: V2<f64> = (0.5, 0.5).into();
    let mut c = a + b * 2.0 - V2 { x: 0.25, y: 0.0 };
    c += -a;
    println!("{c:?} x={} y={}", c[0], c[1]);
    let total: V2<i32> = [(1, 2), (3, 4), (5, 6)].into_iter().map(V2::from).sum();
    println!("{total:?}");
    assert_eq!(total, V2 { x: 9, y: 12 });
}
