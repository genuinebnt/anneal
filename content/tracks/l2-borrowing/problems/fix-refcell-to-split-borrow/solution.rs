pub struct Cart {
    items: Vec<u32>,
    total: u32,
    log: Vec<String>,
}

impl Cart {
    pub fn new() -> Self {
        Cart {
            items: Vec::new(),
            total: 0,
            log: Vec::new(),
        }
    }

    /// Logs "[<n>] <msg>".
    fn note(log: &mut Vec<String>, n: usize, msg: String) {
        log.push(format!("[{n}] {msg}"));
    }

    /// Adds an item and logs "add <price>" (after adding it).
    pub fn add(&mut self, price: u32) {
        self.items.push(price);
        self.total += price;
        Self::note(&mut self.log, self.items.len(), format!("add {price}"));
    }

    /// Doubles every price, keeping the total in step, and logs "double <old> -> <new>" for each item.
    pub fn double_all(&mut self) {
        let n = self.items.len();
        for p in self.items.iter_mut() {
            self.total += *p;
            *p *= 2;
            Self::note(&mut self.log, n, format!("double {} -> {p}", *p / 2));
        }
    }

    /// Removes every item priced above `max`, keeping the total in step, and logs "drop <price>" for each
    /// (the count in the log line is the number of items before any were dropped). Returns how many.
    pub fn drop_above(&mut self, max: u32) -> usize {
        let n = self.items.len();
        let mut dropped = 0;
        self.items.retain(|&p| {
            if p <= max {
                return true;
            }
            self.total -= p;
            Self::note(&mut self.log, n, format!("drop {p}"));
            dropped += 1;
            false
        });
        dropped
    }

    pub fn total(&self) -> u32 {
        self.total
    }

    pub fn items(&self) -> Vec<u32> {
        self.items.to_vec()
    }

    pub fn log(&self) -> Vec<String> {
        self.log.to_vec()
    }
}
