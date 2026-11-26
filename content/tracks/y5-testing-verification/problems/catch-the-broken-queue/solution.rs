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

/// Panics when the queues built by `make` break the contract; returns when they keep it.
pub fn check<Q: BoundedQueue>(make: impl Fn(usize) -> Q) {
    let mut state = 0x2545_F491_4F6C_DD1Du64;
    let mut below = move |n: u64| {
        state = state
            .wrapping_mul(0x5851_F42D_4C95_7F2D)
            .wrapping_add(0x1405_7B7E_F767_814F);
        (state >> 33) % n
    };
    for cap in [1, 2, 3, 4, 5, 8, 16, 33] {
        for _ in 0..20 {
            let mut queue = make(cap);
            let mut model: VecDeque<u32> = VecDeque::new();
            compare(&queue, &model);
            let mut push_bias = 8;
            for step in 0..400 {
                if step % (cap * 2 + 3) == 0 {
                    // Alternate between filling and draining, so both limits are reached often.
                    push_bias = 10 - push_bias;
                }
                if below(10) < push_bias {
                    let value = match below(8) {
                        0 => 0,
                        1 => u32::MAX,
                        2 => below(1 << 31) as u32,
                        n => n as u32 % 3,
                    };
                    let want = if model.len() == cap {
                        Err(value)
                    } else {
                        model.push_back(value);
                        Ok(())
                    };
                    assert_eq!(queue.push(value), want, "push({value}) with capacity {cap}");
                } else {
                    assert_eq!(queue.pop(), model.pop_front(), "pop with capacity {cap}");
                }
                compare(&queue, &model);
            }
        }
    }
}

fn compare<Q: BoundedQueue>(queue: &Q, model: &VecDeque<u32>) {
    assert_eq!(queue.len(), model.len(), "len");
    assert_eq!(queue.is_empty(), model.is_empty(), "is_empty");
    assert_eq!(queue.peek(), model.front().copied(), "peek");
}
