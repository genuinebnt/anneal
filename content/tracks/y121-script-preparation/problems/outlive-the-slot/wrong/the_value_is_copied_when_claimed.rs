use std::cell::RefCell;
use std::rc::Rc;

/// Where a kept variable is now.
#[derive(Debug)]
enum Place {
    /// Still on the stack, in this slot.
    OnStack(usize),
    /// Its slot is gone: the value lives here.
    Own(i64),
}

/// A claim on one variable. It stays good after the variable's slot is gone.
#[derive(Debug, Clone)]
pub struct Handle(Rc<RefCell<Place>>);

impl Handle {
    /// True if both are claims on the same variable.
    pub fn same(&self, other: &Handle) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

#[derive(Debug, Default)]
pub struct Stack {
    slots: Vec<i64>,
    /// The kept variables whose slot is still there, sorted by slot.
    open: Vec<(usize, Handle)>,
}

impl Stack {
    pub fn new() -> Stack {
        Stack::default()
    }

    pub fn push(&mut self, value: i64) {
        self.slots.push(value);
    }

    pub fn pop(&mut self) -> Option<i64> {
        let length = self.slots.len().checked_sub(1)?;
        let value = self.slots[length];
        self.truncate(length);
        Some(value)
    }

    /// Removes every slot from `length` up. A longer `length` changes nothing.
    pub fn truncate(&mut self, length: usize) {
        while self.open.last().is_some_and(|(slot, _)| *slot >= length) {
            if let Some((slot, handle)) = self.open.pop() {
                *handle.0.borrow_mut() = Place::Own(self.slots[slot]);
            }
        }
        self.slots.truncate(length);
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
        if slot >= self.slots.len() {
            return None;
        }
        match self.open.binary_search_by_key(&slot, |(s, _)| *s) {
            Ok(found) => Some(self.open[found].1.clone()),
            Err(at) => {
                let handle = Handle(Rc::new(RefCell::new(Place::Own(self.slots[slot]))));
                self.open.insert(at, (slot, handle.clone()));
                Some(handle)
            }
        }
    }

    /// The value of a kept variable. `handle` came from this stack.
    pub fn read(&self, handle: &Handle) -> i64 {
        match *handle.0.borrow() {
            Place::OnStack(slot) => self.slots[slot],
            Place::Own(value) => value,
        }
    }

    pub fn write(&mut self, handle: &Handle, value: i64) {
        match &mut *handle.0.borrow_mut() {
            Place::OnStack(slot) => self.slots[*slot] = value,
            Place::Own(held) => *held = value,
        }
    }
}
