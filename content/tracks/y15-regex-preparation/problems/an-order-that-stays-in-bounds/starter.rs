#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Job {
    Add(i64),
    Double,
    InOrder(Vec<Job>),
    AnyOrder(Vec<Job>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Outcome {
    /// Action numbers in the order to perform them.
    pub order: Option<Vec<usize>>,
    /// The most actions that can be performed before leaving the bounds.
    pub furthest: usize,
}

pub fn plan(job: &Job, start: i64, capacity: i64) -> Outcome {
    todo!()
}
