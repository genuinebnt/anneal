pub struct Merge<I: Iterator> {
    sources: Vec<I>,
}

pub fn merge<S>(sources: impl IntoIterator<Item = S>) -> Merge<S::IntoIter>
where
    S: IntoIterator,
    S::Item: Ord,
{
    Merge {
        sources: sources.into_iter().map(IntoIterator::into_iter).collect(),
    }
}

impl<I: Iterator> Iterator for Merge<I>
where
    I::Item: Ord,
{
    type Item = I::Item;

    fn next(&mut self) -> Option<I::Item> {
        todo!()
    }
}
