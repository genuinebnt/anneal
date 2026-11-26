use std::cell::RefCell;

pub struct Cart {
    items: RefCell<Vec<u32>>,
    total: RefCell<u32>,
    log: RefCell<Vec<String>>,
}

impl Cart {
    pub fn new() -> Self {
        Cart {
            items: RefCell::new(Vec::new()),
            total: RefCell::new(0),
            log: RefCell::new(Vec::new()),
        }
    }

    /// Logs "[<n>] <msg>", where n is the number of items right now.
    fn note(&self, msg: String) {
        let n = self.items.borrow().len();
        self.log.borrow_mut().push(format!("[{n}] {msg}"));
    }

    /// Adds an item and logs "add <price>" (after adding it).
    pub fn add(&self, price: u32) {
        self.items.borrow_mut().push(price);
        *self.total.borrow_mut() += price;
        self.note(format!("add {price}"));
    }

    /// Doubles every price, keeping the total in step, and logs "double <old> -> <new>" for each item.
    pub fn double_all(&self) {
        for p in self.items.borrow_mut().iter_mut() {
            *self.total.borrow_mut() += *p;
            *p *= 2;
            self.note(format!("double {} -> {p}", *p / 2));
        }
    }

    /// Removes every item priced above `max`, keeping the total in step, and logs "drop <price>" for each
    /// (the count in the log line is the number of items before any were dropped). Returns how many.
    pub fn drop_above(&self, max: u32) -> usize {
        let mut dropped = 0;
        self.items.borrow_mut().retain(|&p| {
            if p <= max {
                return true;
            }
            *self.total.borrow_mut() -= p;
            self.note(format!("drop {p}"));
            dropped += 1;
            false
        });
        dropped
    }

    pub fn total(&self) -> u32 {
        *self.total.borrow()
    }

    pub fn items(&self) -> Vec<u32> {
        self.items.borrow().to_vec()
    }

    pub fn log(&self) -> Vec<String> {
        self.log.borrow().to_vec()
    }
}
