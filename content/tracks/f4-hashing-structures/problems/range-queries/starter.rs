pub struct PrefixSums {}

impl PrefixSums {
    pub fn new(values: &[i64]) -> Self {
        todo!()
    }

    pub fn add(&mut self, i: usize, delta: i64) {
        todo!()
    }

    pub fn prefix(&self, end: usize) -> i64 {
        todo!()
    }

    pub fn sum(&self, l: usize, r: usize) -> i64 {
        self.prefix(r) - self.prefix(l)
    }

    pub fn first_reaching(&self, target: i64) -> Option<usize> {
        todo!()
    }
}

/// How values of a range are summarised. `join` is associative; `empty` changes nothing when joined.
pub trait Summary: Clone {
    fn empty() -> Self;
    fn join(left: &Self, right: &Self) -> Self;
}

pub struct Combine<T: Summary> {
    values: std::marker::PhantomData<T>,
}

impl<T: Summary> Combine<T> {
    pub fn new(values: &[T]) -> Self {
        todo!()
    }

    pub fn set(&mut self, i: usize, value: T) {
        todo!()
    }

    pub fn query(&self, l: usize, r: usize) -> T {
        todo!()
    }
}

pub struct RangeAdder {}

impl RangeAdder {
    pub fn new(values: &[i64]) -> Self {
        todo!()
    }

    pub fn add_range(&mut self, l: usize, r: usize, delta: i64) {
        todo!()
    }

    pub fn sum(&mut self, l: usize, r: usize) -> i64 {
        todo!()
    }
}
