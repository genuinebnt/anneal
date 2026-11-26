pub mod shop {
    pub struct Item {
        pub name: String,
        pub price: u32,
    }

    pub struct Shop {
        items: Vec<Item>,
        log: Vec<String>,
        discount: u32,
    }

    impl Shop {
        pub fn new(items: Vec<Item>, discount: u32) -> Self {
            Shop {
                items,
                log: Vec::new(),
                discount,
            }
        }

        pub fn items(&self) -> &[Item] {
            &self.items
        }

        pub fn items_mut(&mut self) -> &mut [Item] {
            &mut self.items
        }

        pub fn log(&self) -> &[String] {
            &self.log
        }

        pub fn log_mut(&mut self) -> &mut Vec<String> {
            &mut self.log
        }

        pub fn discount(&self) -> u32 {
            self.discount
        }

        /// The items and the log, borrowed together.
        pub fn items_and_log(&mut self) -> (&[Item], &mut Vec<String>) {
            (&self.items, &mut self.log)
        }

        /// Takes `discount` percent (rounded down) off every item priced at least `min`, logging
        /// "<name>: <old> -> <new>". Returns how many items changed price.
        pub fn sale(&mut self, min: u32) -> usize {
            let mut n = 0;
            for it in self.items.iter_mut() {
                if it.price >= min {
                    let new = it.price - it.price * self.discount / 100;
                    self.log.push(format!("{}: {} -> {new}", it.name, it.price));
                    if new != it.price {
                        n += 1;
                    }
                    it.price = new;
                }
            }
            n
        }
    }
}

pub use shop::Shop;

/// Logs "expensive: <name>" in the shop's log for every item priced above `limit`, in order. Returns how many.
pub fn flag_expensive(shop: &mut Shop, limit: u32) -> usize {
    let mut n = 0;
    let (items, log) = shop.items_and_log();
    for it in items {
        if it.price > limit {
            log.push(format!("expensive: {}", it.name));
            n += 1;
        }
    }
    n
}
