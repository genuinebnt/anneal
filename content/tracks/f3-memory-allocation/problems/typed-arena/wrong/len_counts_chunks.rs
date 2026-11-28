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

    // `&self` in, `&mut T` out is what clippy's `mut_from_ref` (an error) flags, because it is usually a bug. Here it is
    // the point: each call hands out a different value, so no two `&mut` alias. `typed_arena` and `bumpalo` do the same.
    #[allow(clippy::mut_from_ref)]
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
        let i = chunk.len();
        chunk.push(value);
        // SAFETY: slot `i` holds the value just pushed. The chunk's buffer never reallocates (we only push while
        // len < capacity) and is only freed when the arena drops, which the returned lifetime (tied to `&self`)
        // prevents from happening first. `as_mut_ptr` reaches the slot without a `&mut [T]` over the whole chunk:
        // `chunk.last_mut()` would make one, and that unique borrow invalidates every reference handed out before.
        unsafe { &mut *chunk.as_mut_ptr().add(i) }
    }

    /// Values allocated so far.
    pub fn len(&self) -> usize {
        self.chunks.borrow().len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn chunk_count(&self) -> usize {
        self.chunks.borrow().len()
    }
}
