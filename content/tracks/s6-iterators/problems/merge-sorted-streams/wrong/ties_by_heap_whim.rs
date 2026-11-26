use std::cmp::Ordering;
use std::collections::BinaryHeap;

struct Head<T> {
    item: T,
    from: usize,
}
impl<T: Ord> Ord for Head<T> {
    fn cmp(&self, other: &Self) -> Ordering {
        other.item.cmp(&self.item)
    }
}
impl<T: Ord> PartialOrd for Head<T> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
impl<T: Ord> PartialEq for Head<T> {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == Ordering::Equal
    }
}
impl<T: Ord> Eq for Head<T> {}

pub struct Merge<I: Iterator> {
    sources: Vec<Option<I>>,
    heads: BinaryHeap<Head<I::Item>>,
    refill: Option<usize>,
    primed: bool,
}

pub fn merge<S>(sources: impl IntoIterator<Item = S>) -> Merge<S::IntoIter>
where
    S: IntoIterator,
    S::Item: Ord,
{
    let sources: Vec<_> = sources.into_iter().map(|s| Some(s.into_iter())).collect();
    Merge {
        heads: BinaryHeap::new(),
        sources,
        refill: None,
        primed: false,
    }
}

impl<I: Iterator> Merge<I>
where
    I::Item: Ord,
{
    fn pull(&mut self, from: usize) {
        let Some(source) = &mut self.sources[from] else {
            return;
        };
        match source.next() {
            Some(item) => self.heads.push(Head { item, from }),
            None => self.sources[from] = None,
        }
    }
}

impl<I: Iterator> Iterator for Merge<I>
where
    I::Item: Ord,
{
    type Item = I::Item;

    fn next(&mut self) -> Option<I::Item> {
        if !self.primed {
            self.primed = true;
            for from in 0..self.sources.len() {
                self.pull(from);
            }
        }
        if let Some(from) = self.refill.take() {
            self.pull(from);
        }
        let head = self.heads.pop()?;
        self.refill = Some(head.from);
        Some(head.item)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let b = self.heads.len();
        self.sources.iter().flatten().map(Iterator::size_hint).fold(
            (b, Some(b)),
            |(a, x), (c, d)| {
                (
                    a.saturating_add(c),
                    x.zip(d).and_then(|(p, q)| p.checked_add(q)),
                )
            },
        )
    }
}
