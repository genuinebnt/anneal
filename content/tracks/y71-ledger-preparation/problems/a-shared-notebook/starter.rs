use std::ops::Range;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Record<T> {
    pub pos: u64,
    pub stream: String,
    pub version: u64,
    pub item: T,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Conflict {
    pub expected: u64,
    pub actual: u64,
}

pub struct Journal<T> {
    // TODO: your fields (this one only makes the starter compile)
    log: Vec<Record<T>>,
}

impl<T> Journal<T> {
    pub fn new() -> Self {
        todo!()
    }

    pub fn append(
        &mut self,
        stream: &str,
        expected: u64,
        items: Vec<T>,
    ) -> Result<Range<u64>, Conflict> {
        todo!()
    }

    pub fn version(&self, stream: &str) -> u64 {
        todo!()
    }

    pub fn read_stream<'a>(
        &'a self,
        stream: &str,
        after: u64,
    ) -> impl Iterator<Item = &'a Record<T>> {
        std::iter::from_fn(|| todo!())
    }

    pub fn read_all(&self, from: u64) -> &[Record<T>] {
        todo!()
    }

    pub fn streams(&self) -> Vec<&str> {
        todo!()
    }
}
