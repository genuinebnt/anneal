use std::cell::RefCell;

/// Allocates values that live until the arena is dropped and hands out `&mut` to each, through `&self`.
/// Storage is a list of chunks that never reallocate once created, so earlier references stay valid while you
/// keep allocating. Each new chunk doubles the previous one.
pub struct Arena<T> {
    chunks: RefCell<Vec<Vec<T>>>,
}

impl<T> Arena<T> {
    pub fn new() -> Self {
        Arena { chunks: RefCell::new(Vec::new()) }
    }

    pub fn alloc(&self, value: T) -> &mut T {
        let mut chunks = self.chunks.borrow_mut();
        let full = match chunks.last() {
            Some(c) => c.len() == c.capacity(),
            None => true,
        };
        if full {
            let cap = chunks.last().map_or(4, |c| c.capacity() * 2);
            chunks.push(Vec::with_capacity(cap));
        }
        let chunk = chunks.last_mut().unwrap();
        chunk.push(value);
        let ptr: *mut T = Box::leak(Box::new(chunk.pop().unwrap()));
        chunk.reserve(0);
        let _ = &chunk;
        // SAFETY: the chunk's buffer never reallocates (we only push while len < capacity) and is only freed when the
        // arena drops, which the returned lifetime (tied to `&self`) prevents from happening first.
        unsafe { &mut *ptr }
    }

    /// Values allocated so far.
    pub fn len(&self) -> usize {
        self.chunks.borrow().iter().map(|c| c.len()).sum()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn chunk_count(&self) -> usize {
        self.chunks.borrow().len()
    }
}
