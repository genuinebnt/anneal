use std::iter::Peekable;

pub struct Merge<I: Iterator> {
    sources: Vec<Option<Peekable<I>>>,
}

pub fn merge<S>(sources: impl IntoIterator<Item = S>) -> Merge<S::IntoIter>
where
    S: IntoIterator,
    S::Item: Ord,
{
    Merge {
        sources: sources
            .into_iter()
            .map(|s| Some(s.into_iter().peekable()))
            .collect(),
    }
}

impl<I: Iterator> Iterator for Merge<I>
where
    I::Item: Ord,
{
    type Item = I::Item;

    fn next(&mut self) -> Option<I::Item> {
        for slot in &mut self.sources {
            if slot.as_mut().is_some_and(|p| p.peek().is_none()) {
                *slot = None;
            }
        }
        let mut best: Option<(usize, &I::Item)> = None;
        for (i, slot) in self.sources.iter_mut().enumerate() {
            if let Some(item) = slot.as_mut().and_then(|p| p.peek()) {
                if best.is_none_or(|(_, b)| item < b) {
                    best = Some((i, item));
                }
            }
        }
        let i = best?.0;
        self.sources[i].as_mut()?.next()
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.sources.iter().flatten().map(Iterator::size_hint).fold(
            (0, Some(0)),
            |(a, b), (c, d): (usize, Option<usize>)| {
                (
                    a.saturating_add(c),
                    b.zip(d).and_then(|(x, y)| x.checked_add(y)),
                )
            },
        )
    }
}
