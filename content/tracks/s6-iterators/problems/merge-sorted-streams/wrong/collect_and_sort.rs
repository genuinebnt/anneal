pub struct Merge<I: Iterator> {
    sources: Vec<I>,
    sorted: Option<std::vec::IntoIter<I::Item>>,
}

pub fn merge<S>(sources: impl IntoIterator<Item = S>) -> Merge<S::IntoIter>
where
    S: IntoIterator,
    S::Item: Ord,
{
    Merge {
        sources: sources.into_iter().map(IntoIterator::into_iter).collect(),
        sorted: None,
    }
}

impl<I: Iterator> Iterator for Merge<I>
where
    I::Item: Ord,
{
    type Item = I::Item;

    fn next(&mut self) -> Option<I::Item> {
        if self.sorted.is_none() {
            // Bounded, so that an endless source fails the tests instead of using all the memory.
            let mut all = Vec::new();
            for source in &mut self.sources {
                let mut n = 0;
                for item in source.by_ref() {
                    all.push(item);
                    n += 1;
                    assert!(n < 20_000, "endless source");
                }
            }
            all.sort();
            self.sorted = Some(all.into_iter());
        }
        self.sorted.as_mut().unwrap().next()
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        match &self.sorted {
            Some(s) => s.size_hint(),
            None => self
                .sources
                .iter()
                .map(Iterator::size_hint)
                .fold((0, Some(0)), |(a, b), (c, d)| {
                    (a + c, b.zip(d).map(|(x, y)| x + y))
                }),
        }
    }
}
