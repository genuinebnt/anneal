use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Kind {
    Sale,
    Refund,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Record {
    pub pos: u64,
    pub day: u32,
    pub shop: String,
    pub kind: Kind,
    pub cents: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Gap {
    pub expected: u64,
    pub got: u64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Board {
    next: u64,
    shops: BTreeMap<String, i128>,
    days: BTreeMap<u32, (u128, u128)>,
}

impl Board {
    pub fn new() -> Board {
        Board::default()
    }

    pub fn next(&self) -> u64 {
        self.next
    }

    pub fn apply(&mut self, batch: &[Record]) -> Result<usize, Gap> {
        // first look at the whole batch, so that a hole leaves no half-counted board
        let mut expected = self.next;
        for r in batch {
            if r.pos > expected {
                return Err(Gap {
                    expected,
                    got: r.pos,
                });
            }
            if r.pos == expected {
                expected += 1;
            }
        }
        let before = self.next;
        for r in batch {
            if r.pos == self.next {
                self.count(r);
            }
        }
        Ok((self.next - before) as usize)
    }

    fn count(&mut self, r: &Record) {
        if !self.shops.contains_key(&r.shop) {
            self.shops.insert(r.shop.clone(), 0);
        }
        let net = self.shops.get_mut(&r.shop).expect("just inserted");
        let day = self.days.entry(r.day).or_default();
        match r.kind {
            Kind::Sale => {
                *net += i128::from(r.cents);
                day.0 += u128::from(r.cents);
            }
            Kind::Refund => {
                *net -= i128::from(r.cents);
                day.1 += u128::from(r.cents);
            }
        }
        self.next += 1;
    }

    pub fn net(&self, shop: &str) -> i128 {
        self.shops.get(shop).copied().unwrap_or(0)
    }

    pub fn day(&self, day: u32) -> (u128, u128) {
        self.days.get(&day).copied().unwrap_or_default()
    }

    pub fn top(&self, n: usize) -> Vec<(String, i128)> {
        let mut all: Vec<(String, i128)> =
            self.shops.iter().map(|(s, v)| (s.clone(), *v)).collect();
        all.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| b.0.cmp(&a.0)));
        all.truncate(n);
        all
    }

    pub fn rebuild(log: &[Record]) -> Board {
        Board::as_of(log, u64::MAX)
    }

    pub fn as_of(log: &[Record], pos: u64) -> Board {
        let end = usize::try_from(pos).map_or(log.len(), |p| p.min(log.len()));
        let mut board = Board::new();
        for r in &log[..end] {
            board.count(r);
        }
        board
    }
}
