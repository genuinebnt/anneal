use std::collections::HashMap;

#[derive(Debug, Default, PartialEq)]
pub struct Stock {
    pub qty: u32,
    pub reserved: u32,
}

pub struct Warehouse {
    items: HashMap<String, Stock>,
    last: Option<String>,
    log: Vec<String>,
}

impl Warehouse {
    pub fn new() -> Self {
        Warehouse {
            items: HashMap::new(),
            last: None,
            log: Vec::new(),
        }
    }

    /// Remembers `name` as the last item touched.
    fn touch(&mut self, name: &str) {
        self.last = Some(name.to_string());
    }

    /// Adds `qty` units of `name`, creating the item if needed.
    pub fn receive(&mut self, name: &str, qty: u32) {
        self.items.entry(name.to_string()).or_default().qty += qty;
        self.touch(name);
    }

    /// Reserves up to `qty` free units of `name` and returns how many it reserved. An unknown item reserves
    /// nothing and isn't touched.
    pub fn reserve(&mut self, name: &str, qty: u32) -> u32 {
        let Some(s) = self.items.get_mut(name) else {
            return 0;
        };
        let n = qty.min(s.qty - s.reserved);
        s.reserved += n;
        self.touch(name);
        n
    }

    /// Units of `name` that aren't reserved.
    pub fn available(&self, name: &str) -> u32 {
        self.items.get(name).map_or(0, |s| s.qty - s.reserved)
    }

    /// The last item touched.
    pub fn last(&self) -> Option<&str> {
        self.last.as_deref()
    }

    /// Appends `tag` to the remembered last-touched name. The item itself keeps its name.
    pub fn tag_last(&mut self, tag: &str) {
        if let Some(name) = &mut self.last {
            name.push_str(tag);
        }
    }

    /// Ships every reservation: reserved units leave stock. Logs "<name>: <units>" for each item shipped,
    /// sorted by name, and returns how many items shipped.
    pub fn ship_all(&mut self) -> usize {
        let mut lines = Vec::new();
        for (name, s) in self.items.iter_mut() {
            if s.reserved > 0 {
                lines.push(format!("{name}: {}", s.reserved));
                s.qty -= s.reserved;
                s.reserved = 0;
            }
        }
        lines.sort();
        let shipped = lines.len();
        self.log.extend(lines);
        shipped
    }

    /// How many items have fewer than `limit` units available.
    pub fn count_low(&self, limit: u32) -> usize {
        let mut n = 0;
        let mut bump = || n += 1;
        for s in self.items.values() {
            if s.qty - s.reserved < limit {
                bump();
            }
        }
        n
    }

    pub fn log(&self) -> &[String] {
        &self.log
    }
}
