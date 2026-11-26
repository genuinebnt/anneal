/// A point to come back to. What it holds is up to you.
#[derive(Debug, Clone, Copy)]
pub struct Checkpoint(pub usize);

pub struct Groups {}

impl Groups {
    pub fn new(n: usize) -> Self {
        todo!()
    }

    pub fn union(&mut self, a: usize, b: usize) -> bool {
        todo!()
    }

    pub fn connected(&mut self, a: usize, b: usize) -> bool {
        todo!()
    }

    pub fn count(&self) -> usize {
        todo!()
    }

    pub fn group_size(&mut self, a: usize) -> usize {
        todo!()
    }

    pub fn largest(&self) -> usize {
        todo!()
    }

    pub fn add(&mut self) -> usize {
        todo!()
    }

    pub fn checkpoint(&mut self) -> Checkpoint {
        todo!()
    }

    pub fn rollback(&mut self, mark: Checkpoint) {
        todo!()
    }
}
