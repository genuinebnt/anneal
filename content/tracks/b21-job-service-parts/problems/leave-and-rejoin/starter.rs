use futures::Stream;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub id: u64,
    pub text: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Closed;

#[derive(Clone)]
pub struct Feed {
    // TODO: your fields
}

impl Feed {
    pub fn new() -> Self {
        todo!()
    }

    pub fn publish(&self, text: &str) -> Result<u64, Closed> {
        todo!()
    }

    pub fn close(&self) {
        todo!()
    }

    pub fn since(&self, after: u64) -> Vec<Entry> {
        todo!()
    }

    /// For the tests: see "For the tests" in the statement.
    pub fn on_nothing_new(&self, hook: impl Fn() + Send + Sync + 'static) {
        todo!()
    }

    pub fn follow(&self, after: u64) -> impl Stream<Item = Entry> + Send + 'static {
        // Replace this line: it is only here so that the signature compiles.
        futures::stream::once(async { todo!() })
    }
}
