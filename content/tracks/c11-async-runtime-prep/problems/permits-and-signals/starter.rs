use std::marker::PhantomData;

/// Lets at most `n` holders through at a time.
pub struct Semaphore {
    // TODO: the fields
}

/// One of the semaphore's permits. Dropping it gives it back.
pub struct Permit<'a> {
    // TODO: the fields
    _todo: PhantomData<&'a Semaphore>,
}

/// Remembers at most one unused "something happened".
pub struct Signal {
    // TODO: the fields
}

impl Semaphore {
    pub fn new(permits: usize) -> Semaphore {
        todo!()
    }

    pub async fn acquire(&self) -> Permit<'_> {
        todo!()
    }

    pub fn try_acquire(&self) -> Option<Permit<'_>> {
        todo!()
    }

    pub fn available(&self) -> usize {
        todo!()
    }
}

impl Signal {
    pub fn new() -> Signal {
        todo!()
    }

    pub fn signal(&self) {
        todo!()
    }

    pub async fn wait(&self) {
        todo!()
    }
}
