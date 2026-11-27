use std::future::Future;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BadCost;

pub struct Importer {
    // TODO: your fields
}

impl Importer {
    pub fn new(budget: u32) -> Self {
        todo!()
    }

    pub fn submit<F>(&self, cost: u32, work: F) -> Result<(), BadCost>
    where
        F: Future<Output = ()> + Send + 'static,
    {
        todo!()
    }

    pub fn held(&self) -> usize {
        todo!()
    }

    pub async fn idle(&self) {
        todo!()
    }
}
