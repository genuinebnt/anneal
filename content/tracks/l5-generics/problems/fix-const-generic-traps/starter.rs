/// A fixed-capacity ring buffer. Pushing onto a full ring evicts and returns the oldest item.
pub struct Ring<T, const N: usize> {
    items: [T; N],
    start: usize,
    len: usize,
}

impl<T: Default, const N: usize> Ring<T, N> {
    pub fn new() -> Self {
        Ring {
            items: [T::default(); N],
            start: 0,
            len: 0,
        }
    }
}

impl<T, const N: usize> Ring<T, N> {
    pub const CAPACITY: usize = N;

    /// Adds `x` as the newest item; if the ring was full, returns the evicted oldest one.
    pub fn push(&mut self, x: T) -> Option<T> {
        if self.len == N {
            let old = std::mem::replace(&mut self.items[self.start], x);
            self.start = (self.start + 1) % N;
            Some(old)
        } else {
            self.items[(self.start + self.len) % N] = x;
            self.len += 1;
            None
        }
    }

    pub fn len(&self) -> usize {
        self.len
    }

    /// Oldest first.
    pub fn iter(&self) -> impl Iterator<Item = &T> + '_ {
        (0..self.len).map(move |i| &self.items[(self.start + i) % N])
    }
}

/// `a` followed by `b`.
pub fn concat<T: Copy + Default, const A: usize, const B: usize>(
    a: [T; A],
    b: [T; B],
) -> [T; A + B] {
    let mut out = [T::default(); A + B];
    out[..A].copy_from_slice(&a);
    out[A..].copy_from_slice(&b);
    out
}
