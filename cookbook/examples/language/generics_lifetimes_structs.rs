//! RECIPE 82 (language): generics + lifetimes together in structs, impls, iterators, closures, and higher-ranked bounds.
//! LEVEL: advanced
//! Use in: zero-copy views over input, iterator types, caches/handlers generic over closures.   Run: cargo run --release --example generics_lifetimes_structs
//! COMMON USES: zero-copy views, memoizing caches, borrowed iterators, handler tables holding closures.
#![allow(dead_code)]
//!
//! Reading a signature: `impl<'a, T: Bound> Struct<'a, T>` introduces lifetime `'a` and type `T` for the whole impl.
//! KEY INSIGHT: a method that returns data borrowed from the ORIGINAL source should name the struct's lifetime (`&'a T`),
//! not the shorter `&self` lifetime, so callers can keep the result after the view itself is gone.
//!   `+ '_`                  "this impl Trait borrows from &self"
//!   `for<'x> Fn(&'x str)`   higher-ranked: works for ANY lifetime (needed for closures taking references)
//!   `'a: 'b`                'a outlives 'b
//! Read: Rust Book ch. 10.3, 19.2 https://doc.rust-lang.org/book/ch19-02-advanced-traits.html and the Nomicon
//! https://doc.rust-lang.org/nomicon/hrtb.html . Study real zero-copy code: https://github.com/BurntSushi/aho-corasick
use std::collections::HashMap;
use std::hash::Hash;
use std::marker::PhantomData;
use std::str::FromStr;

// 1. A generic iterator struct that BORROWS the input text and parses each piece into T.
struct Parsed<'a, T> {
    parts: std::str::Split<'a, char>,
    _marker: PhantomData<T>, // T appears only in the Item type, so we tell the compiler we "use" it
}
impl<'a, T: FromStr> Parsed<'a, T> {
    fn new(s: &'a str) -> Self {
        Parsed {
            parts: s.split(','),
            _marker: PhantomData,
        }
    }
}
impl<'a, T: FromStr> Iterator for Parsed<'a, T> {
    type Item = Result<T, T::Err>;
    fn next(&mut self) -> Option<Self::Item> {
        self.parts.next().map(|p| p.trim().parse())
    }
}

// 2. A view over a slice. Methods return `&'a T` (tied to the DATA), not to the temporary `&self`.
struct View<'a, T> {
    data: &'a [T],
}
impl<'a, T> Clone for View<'a, T> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<'a, T> Copy for View<'a, T> {} // a view is just a pointer+len: Copy regardless of T
impl<'a, T> View<'a, T> {
    fn new(data: &'a [T]) -> Self {
        View { data }
    }
    fn first_two(&self) -> Option<(&'a T, &'a T)> {
        match self.data {
            [a, b, ..] => Some((a, b)),
            _ => None,
        }
    }
    fn pairs(&self) -> impl Iterator<Item = (&'a T, &'a T)> + 'a {
        self.data.windows(2).map(|w| (&w[0], &w[1]))
    }
    fn best_by<K: Ord>(&self, key: impl Fn(&T) -> K) -> Option<&'a T> {
        self.data.iter().max_by_key(|x| key(x))
    }
    fn split_at(&self, mid: usize) -> (View<'a, T>, View<'a, T>) {
        let (l, r) = self.data.split_at(mid);
        (View { data: l }, View { data: r })
    }
}

// 3. Two independent lifetimes in one struct: old and new text may live for different durations.
struct Change<'old, 'new> {
    before: &'old str,
    after: &'new str,
}
fn longest_before<'o, 'n>(changes: &[Change<'o, 'n>]) -> Option<&'o str> {
    changes.iter().map(|c| c.before).max_by_key(|s| s.len())
}

// 4. Higher-ranked trait bound: `f` must work for ANY input lifetime and return a slice of THAT input.
fn trim_all<'a, F>(items: &'a [String], f: F) -> Vec<&'a str>
where
    F: for<'x> Fn(&'x str) -> &'x str,
{
    items.iter().map(|s| f(s)).collect()
}

// 5. A cache generic over key, value AND the closure type that computes values.
struct Memo<K, V, F>
where
    F: Fn(&K) -> V,
{
    compute: F,
    cache: HashMap<K, V>,
    computed: u32,
}
impl<K: Hash + Eq + Clone, V: Clone, F: Fn(&K) -> V> Memo<K, V, F> {
    fn new(compute: F) -> Self {
        Memo {
            compute,
            cache: HashMap::new(),
            computed: 0,
        }
    }
    fn get(&mut self, k: K) -> V {
        if let Some(v) = self.cache.get(&k) {
            return v.clone();
        }
        self.computed += 1;
        let v = (self.compute)(&k);
        self.cache.insert(k, v.clone());
        v
    }
}

// 6. A collection that can be iterated by reference with `for x in &bag`.
struct Bag<T> {
    items: Vec<T>,
}
impl<'a, T> IntoIterator for &'a Bag<T> {
    type Item = &'a T;
    type IntoIter = std::slice::Iter<'a, T>;
    fn into_iter(self) -> Self::IntoIter {
        self.items.iter()
    }
}
impl<T: PartialOrd> Bag<T> {
    // returns an iterator that BORROWS self: `+ '_`
    fn above<'s>(&'s self, limit: &'s T) -> impl Iterator<Item = &'s T> + 's {
        self.items.iter().filter(move |x| *x > limit)
    }
}

// 7. A struct holding a closure that borrows local data: its lifetime parameter is the closure's lifetime bound.
struct Handler<'a> {
    run: Box<dyn Fn(&str) -> String + 'a>,
}

fn main() {
    let ints: Vec<Result<i32, _>> = Parsed::<i32>::new("1, 2, x, 4").collect();
    println!("{ints:?}");
    let floats: Result<Vec<f64>, _> = Parsed::<f64>::new("1.5,2.5").collect();
    println!("{floats:?}");

    let first;
    {
        let data = [10, 20, 30, 40];
        let view = View::new(&data);
        first = view.first_two(); // the borrow is of `data`, so `first` outlives `view`...
        // (...but `data` here is dropped at the end of this block, so we only print inside)
        println!(
            "{first:?} {:?} {:?}",
            view.pairs().map(|(a, b)| b - a).collect::<Vec<_>>(),
            view.best_by(|x| (*x as i32 - 25).abs())
        );
        let (l, r) = view.split_at(1);
        println!("{:?} {:?}", l.data, r.data);
    }

    let old_text = String::from("the old text");
    let changes;
    {
        let new_text = String::from("brand new");
        changes = vec![Change {
            before: &old_text,
            after: &new_text,
        }];
        println!("{:?}", longest_before(&changes)); // result borrows only from old_text
        println!("{}", changes[0].after);
    }

    let raw = vec!["  a ".to_string(), "b  ".to_string()];
    println!("{:?}", trim_all(&raw, |s| s.trim()));
    println!("{:?}", trim_all(&raw, |s| &s[..1]));

    let mut slow_square = Memo::new(|n: &u64| {
        std::thread::sleep(std::time::Duration::from_millis(1));
        n * n
    });
    println!(
        "{} {} {} computed {} times",
        slow_square.get(7),
        slow_square.get(7),
        slow_square.get(8),
        slow_square.computed
    );
    assert_eq!(slow_square.computed, 2);

    let bag = Bag {
        items: vec![3, 9, 1, 7],
    };
    let mut total = 0;
    for x in &bag {
        total += x;
    }
    println!("{total} {:?}", bag.above(&2).collect::<Vec<_>>());

    let prefix = String::from(">> "); // borrowed (not moved) by the handler below
    let h = Handler {
        run: Box::new(|s| format!("{prefix}{s}")),
    };
    println!("{}", (h.run)("hi"));
    // Self-referential structs (a struct holding a reference to its OWN field) are rejected by the borrow checker.
    // Use indices/ranges into the owned data instead, or crates like `ouroboros`/Pin for the rare case that needs it.
}
