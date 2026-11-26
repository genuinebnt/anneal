use std::collections::VecDeque;

pub trait BoundedQueue {
    fn push(&mut self, value: u32) -> Result<(), u32>;
    fn pop(&mut self) -> Option<u32>;
    fn peek(&self) -> Option<u32>;
    fn len(&self) -> usize;
    fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// A model-based check whose generator never repeats a value, never uses the extremes, and skips capacity 1.
pub fn check<Q: BoundedQueue>(make: impl Fn(usize) -> Q) {
    let mut state = 12345u64;
    let mut next_value = 1u32;
    for cap in [2, 3, 8] {
        let mut queue = make(cap);
        let mut model: VecDeque<u32> = VecDeque::new();
        for _ in 0..2000 {
            state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
            if (state >> 40) % 2 == 0 {
                next_value += 1;
                let want = if model.len() == cap {
                    Err(next_value)
                } else {
                    model.push_back(next_value);
                    Ok(())
                };
                assert_eq!(queue.push(next_value), want);
            } else {
                assert_eq!(queue.pop(), model.pop_front());
            }
            assert_eq!(queue.len(), model.len());
            assert_eq!(queue.is_empty(), model.is_empty());
            assert_eq!(queue.peek(), model.front().copied());
        }
    }
}
