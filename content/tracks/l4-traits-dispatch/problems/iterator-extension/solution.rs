use std::collections::HashMap;
use std::hash::Hash;

/// Extra adapters for every iterator.
pub trait IterExt: Iterator + Sized {
    /// Items 0, n, 2n, ... Panics if `n` is 0.
    fn every_nth(self, n: usize) -> EveryNth<Self> {
        assert!(n > 0, "every_nth(0)");
        EveryNth {
            iter: self,
            n,
            first: true,
        }
    }

    /// Drops each item equal to the item just before it.
    fn dedup_adjacent(self) -> DedupAdjacent<Self>
    where
        Self::Item: PartialEq + Clone,
    {
        DedupAdjacent {
            iter: self,
            last: None,
        }
    }

    /// How many times each item occurs.
    fn counts(self) -> HashMap<Self::Item, usize>
    where
        Self::Item: Eq + Hash,
    {
        let mut m = HashMap::new();
        for x in self {
            *m.entry(x).or_insert(0) += 1;
        }
        m
    }
}

impl<I: Iterator> IterExt for I {}

pub struct EveryNth<I> {
    iter: I,
    n: usize,
    first: bool,
}

impl<I: Iterator> Iterator for EveryNth<I> {
    type Item = I::Item;

    fn next(&mut self) -> Option<I::Item> {
        if self.first {
            self.first = false;
            self.iter.next()
        } else {
            // nth lets the inner iterator skip in O(1) when it can (ranges, slices).
            self.iter.nth(self.n - 1)
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let (lo, hi) = self.iter.size_hint();
        let f = |x: usize| {
            if self.first {
                x.div_ceil(self.n)
            } else {
                x / self.n
            }
        };
        (f(lo), hi.map(f))
    }
}

pub struct DedupAdjacent<I: Iterator> {
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
