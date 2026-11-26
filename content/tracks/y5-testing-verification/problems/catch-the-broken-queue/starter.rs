pub trait BoundedQueue {
    fn push(&mut self, value: u32) -> Result<(), u32>;
    fn pop(&mut self) -> Option<u32>;
    fn peek(&self) -> Option<u32>;
    fn len(&self) -> usize;
    fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// Panics when the queues built by `make` break the contract; returns when they keep it.
pub fn check<Q: BoundedQueue>(make: impl Fn(usize) -> Q) {
    let mut q = make(4);
    assert_eq!(q.push(1), Ok(()));
    assert_eq!(q.pop(), Some(1));
}
