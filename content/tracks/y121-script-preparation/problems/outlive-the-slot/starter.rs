/// A claim on one variable. It stays good after the variable's slot is gone.
#[derive(Debug, Clone)]
pub struct Handle {}

impl Handle {
    /// True if both are claims on the same variable.
    pub fn same(&self, other: &Handle) -> bool {
        todo!()
    }
}

#[derive(Debug, Default)]
pub struct Stack {
    slots: Vec<i64>,
}

impl Stack {
    pub fn new() -> Stack {
        Stack::default()
    }

    pub fn push(&mut self, value: i64) {
        todo!()
    }

    pub fn pop(&mut self) -> Option<i64> {
        todo!()
    }

    /// Removes every slot from `length` up. A longer `length` changes nothing.
    pub fn truncate(&mut self, length: usize) {
        todo!()
    }

    /// The slots, bottom first. This is the very memory the machine computes in.
    pub fn slots(&self) -> &[i64] {
        &self.slots
    }

    /// The same, for writing. The number of slots cannot change through it.
    pub fn slots_mut(&mut self) -> &mut [i64] {
        &mut self.slots
    }

    /// A claim on the variable now in `slot`, or `None` if there is no such slot.
    pub fn keep(&mut self, slot: usize) -> Option<Handle> {
        todo!()
    }

    /// The value of a kept variable. `handle` came from this stack.
    pub fn read(&self, handle: &Handle) -> i64 {
        todo!()
    }

    pub fn write(&mut self, handle: &Handle, value: i64) {
        todo!()
    }
}
