pub trait BoundedQueue {
    fn push(&mut self, value: u32) -> Result<(), u32>;
    fn pop(&mut self) -> Option<u32>;
    fn peek(&self) -> Option<u32>;
    fn len(&self) -> usize;
    fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

pub fn check<Q: BoundedQueue>(make: impl Fn(usize) -> Q) {
    for cap in [1, 2, 4, 8] {
        let mut q = make(cap);
        assert!(q.is_empty());
        for i in 1..=cap as u32 {
            assert_eq!(q.push(i), Ok(()));
        }
        assert_eq!(q.push(999), Err(999));
        assert_eq!(q.len(), cap);
        for i in 1..=cap as u32 {
            assert_eq!(q.peek(), Some(i));
            assert_eq!(q.pop(), Some(i));
        }
        assert_eq!(q.pop(), None);
    }
}
