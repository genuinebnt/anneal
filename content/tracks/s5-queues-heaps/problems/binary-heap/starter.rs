use std::cmp::Ordering;
use std::marker::PhantomData;

pub struct PriorityQueue<T> {
    items: PhantomData<T>,
}

impl<T: Ord + 'static> PriorityQueue<T> {
    pub fn new() -> Self {
        todo!()
    }

    pub fn from_vec(items: Vec<T>) -> Self {
        todo!()
    }
}

impl<T: Ord + 'static> Default for PriorityQueue<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T> PriorityQueue<T> {
    pub fn with_comparator(cmp: impl Fn(&T, &T) -> Ordering + 'static) -> Self {
        todo!()
    }

    pub fn len(&self) -> usize {
        todo!()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn push(&mut self, item: T) {
        todo!()
    }

    pub fn pop(&mut self) -> Option<T> {
        todo!()
    }

    pub fn peek(&self) -> Option<&T> {
        todo!()
    }
}

pub fn top_k<T: Ord + 'static>(items: impl IntoIterator<Item = T>, k: usize) -> Vec<T> {
    todo!()
}
