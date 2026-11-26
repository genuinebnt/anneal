struct Item {
    name: String,
    qty: u32,
}

pub struct Inventory {
    items: Vec<Item>,
    log: Vec<String>,
}

impl Inventory {
    pub fn restock_low(&mut self) {
        for item in self.items.iter_mut().filter(|i| i.qty < 5) {
            item.qty += 10;
            self.log.push(format!("restocked {}", item.name));
        }
    }

    fn record(&mut self, msg: String) {
        self.log.push(msg);
    }

    pub fn new(stock: &[(&str, u32)]) -> Self {
        let items = stock
            .iter()
            .map(|&(name, qty)| Item {
                name: name.to_string(),
                qty,
            })
            .collect();
        Inventory {
            items,
            log: Vec::new(),
        }
    }

    pub fn qty(&self, name: &str) -> Option<u32> {
        self.items.iter().find(|i| i.name == name).map(|i| i.qty)
    }

    pub fn log(&self) -> &[String] {
        &self.log
    }
}
