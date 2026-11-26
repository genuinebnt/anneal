pub struct Ring<T> {
    buf: Box<[Option<T>]>,
    head: usize,
    len: usize,
}

impl<T> Ring<T> {
    /// Panics if `capacity` is 0.
    pub fn with_capacity(capacity: usize) -> Self {
        assert!(capacity > 0, "capacity must be at least 1");
        Ring {
            buf: (0..capacity).map(|_| None).collect(),
            head: 0,
            len: 0,
        }
    }

    /// Adds `value`, returning the evicted oldest value if the ring was full.
    pub fn push(&mut self, value: T) -> Option<T> {
        let cap = self.buf.len();
        if self.len == cap {
            let newest = (self.head + self.len - 1) % cap;
            self.buf[newest].replace(value)
        } else {
            self.buf[(self.head + self.len) % cap] = Some(value);
            self.len += 1;
            None
        }
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub fn iter(&self) -> impl Iterator<Item = &T> + '_ {
        let cap = self.buf.len();
        (0..self.len).map(move |i| self.buf[(self.head + i) % cap].as_ref().expect("live slot"))
    }
}
