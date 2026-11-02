/// Allocates values that live until the arena is dropped and hands out `&mut` to each, through `&self`.
/// Storage is a list of chunks that never reallocate once created, so earlier references stay valid while you
/// keep allocating. Each new chunk doubles the previous one (the first holds 4).
pub struct Arena<T> {
    // TODO: interior mutability, because `alloc` takes `&self`
    _marker: std::marker::PhantomData<T>,
}

impl<T> Arena<T> {
    pub fn new() -> Self {
        todo!()
    }

    pub fn alloc(&self, value: T) -> &mut T {
        todo!()
    }

    /// Values allocated so far.
    pub fn len(&self) -> usize {
        todo!()
    }

    pub fn is_empty(&self) -> bool {
        todo!()
    }

    pub fn chunk_count(&self) -> usize {
        todo!()
    }
}
