/// A fixed-capacity ring buffer. Pushing onto a full ring evicts and returns the oldest item.
pub struct Ring<T, const N: usize> {
    // Option<T> slots: an empty ring needs no T at all, so no `T: Default` and no `T: Copy`.
    items: [Option<T>; N],
    start: usize,
    len: usize,
}

impl<T, const N: usize> Ring<T, N> {
    pub const CAPACITY: usize = N;

    pub fn new() -> Self {
        // `[None; N]` would need `Option<T>: Copy`; from_fn builds each slot separately.
        Ring {
            items: std::array::from_fn(|_| None),
            start: 0,
            len: 0,
        }
    }

    /// Adds `x` as the newest item; if the ring was full, returns the evicted oldest one.
    pub fn push(&mut self, x: T) -> Option<T> {
        if N == 0 {
            return Some(x);
        }
        if self.len == N {
            let old = self.items[self.start].replace(x);
            self.start = (self.start + 1) % N;
            old
        } else {
            self.items[(self.start + self.len) % N] = Some(x);
            self.len += 1;
            None
        }
    }

    pub fn len(&self) -> usize {
        self.len
    }

    /// Oldest first.
    pub fn iter(&self) -> impl Iterator<Item = &T> + '_ {
        (0..self.len).map(move |i| self.items[(self.start + i) % N].as_ref().unwrap())
    }
}

/// `a` followed by `b`. `[T; A + B]` isn't allowed on stable, so the caller's length `C` is checked at compile time.
pub fn concat<T, const A: usize, const B: usize, const C: usize>(a: [T; A], b: [T; B]) -> [T; C] {
    const { assert!(A + B == C, "concat: C must be A + B") };
    let mut items = a.into_iter().chain(b);
    std::array::from_fn(|_| items.next().unwrap())
}
