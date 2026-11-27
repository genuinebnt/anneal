/// Everything that may be kept about one route.
pub struct Summary {
    /// At most `limit` numbers. What they are is up to you.
    pub kept: Vec<i64>,
    /// How many delivery times there were.
    pub total: usize,
}

pub fn summarise(times: Vec<i64>, limit: usize) -> Summary {
    todo!()
}

impl Summary {
    /// A guess at how many delivery times were `x` or less.
    pub fn at_most(&self, x: i64) -> usize {
        todo!()
    }
}
