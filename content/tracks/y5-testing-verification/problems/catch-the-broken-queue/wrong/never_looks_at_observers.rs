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

pub fn check<Q: BoundedQueue>(make: impl Fn(usize) -> Q) {
    let mut state = 7u64;
    for cap in [1, 2, 3, 5, 8, 16] {
        for _ in 0..20 {
            let mut queue = make(cap);
            let mut model: VecDeque<u32> = VecDeque::new();
            for _ in 0..400 {
                state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
                let r = state >> 33;
                if r % 2 == 0 {
                    let value = [0, 1, 2, u32::MAX][(r / 2 % 4) as usize];
                    let want = if model.len() == cap {
                        Err(value)
                    } else {
                        model.push_back(value);
                        Ok(())
                    };
                    assert_eq!(queue.push(value), want);
                } else {
                    assert_eq!(queue.pop(), model.pop_front());
                }
            }
        }
    }
}
