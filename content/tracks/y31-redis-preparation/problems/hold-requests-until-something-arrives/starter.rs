use std::marker::PhantomData;

/// Requests that wait for the first of several items, or until a deadline.
pub struct Waiting<T> {
    // replace with your own fields
    ticket: PhantomData<T>,
}

impl<T> Waiting<T> {
    pub fn new() -> Waiting<T> {
        Waiting {
            ticket: PhantomData,
        }
    }

    /// Starts waiting. `false`, and nothing changes, if `id` is already waiting.
    pub fn park(&mut self, id: u64, wants: &[&str], deadline: Option<u64>, ticket: T) -> bool {
        todo!()
    }

    /// Stops `id` waiting and returns its ticket.
    pub fn cancel(&mut self, id: u64) -> Option<T> {
        todo!()
    }

    /// One `item` has arrived: removes and returns whoever gets it.
    pub fn wake(&mut self, item: &str) -> Option<(u64, T)> {
        todo!()
    }

    /// Removes and returns everyone whose deadline has come.
    pub fn expire(&mut self, now: u64) -> Vec<(u64, T)> {
        todo!()
    }

    /// The earliest deadline of anyone waiting.
    pub fn next_deadline(&self) -> Option<u64> {
        todo!()
    }

    pub fn contains(&self, id: u64) -> bool {
        todo!()
    }

    pub fn len(&self) -> usize {
        todo!()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl<T> Default for Waiting<T> {
    fn default() -> Self {
        Waiting::new()
    }
}
