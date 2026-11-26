use std::collections::{BTreeMap, HashMap, VecDeque};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Side {
    Bid,
    Ask,
}

impl Side {
    pub fn opposite(self) -> Side {
        match self {
            Side::Bid => Side::Ask,
            Side::Ask => Side::Bid,
        }
    }
}

/// A resting order's handle.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct OrderId(u64);

/// One execution against a resting (maker) order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Fill {
    pub maker: OrderId,
    pub price: u32,
    pub qty: u64,
}

/// Every resting order at one price on one side, oldest first. 96 bytes.
#[allow(dead_code)]
pub struct PriceLevel {
    price: i64,
    symbol: String,
    orders: VecDeque<OrderId>,
    total_qty: u64,
    last_update: Option<u64>,
    side: Side,
    active: bool,
}

impl PriceLevel {
    pub fn price(&self) -> u32 {
        self.price as u32
    }

    /// The quantity resting at this price.
    pub fn qty(&self) -> u64 {
        self.total_qty
    }

    /// How many orders rest at this price.
    pub fn count(&self) -> usize {
        self.orders.len()
    }

    /// When an order at this price was last added, cancelled or filled.
    pub fn last_update(&self) -> Option<u64> {
        self.last_update
    }
}

struct Resting {
    side: Side,
    price: u32,
    qty: u64,
}

/// A limit order book for one instrument, with price-time priority.
pub struct Book {
    symbol: String,
    bids: BTreeMap<u32, PriceLevel>,
    asks: BTreeMap<u32, PriceLevel>,
    orders: HashMap<OrderId, Resting>,
    next_id: u64,
}

impl Book {
    pub fn new(symbol: &str) -> Book {
        Book::with_capacity(symbol, 0)
    }

    /// Room for `orders` resting orders, reserved up front.
    pub fn with_capacity(symbol: &str, orders: usize) -> Book {
        Book {
            symbol: symbol.to_string(),
            bids: BTreeMap::new(),
            asks: BTreeMap::new(),
            orders: HashMap::with_capacity(orders),
            next_id: 0,
        }
    }

    pub fn symbol(&self) -> &str {
        &self.symbol
    }

    /// Resting orders on both sides.
    pub fn len(&self) -> usize {
        self.orders.len()
    }

    pub fn is_empty(&self) -> bool {
        self.orders.is_empty()
    }

    /// Rests an order for `qty` (more than 0) at `price`, behind every order already there.
    pub fn add(&mut self, side: Side, price: u32, qty: u64, ts: u64) -> OrderId {
        assert!(qty > 0, "orders need a quantity");
        let id = OrderId(self.next_id);
        self.next_id += 1;
        let levels = match side {
            Side::Bid => &mut self.bids,
            Side::Ask => &mut self.asks,
        };
        let symbol = &self.symbol;
        let level = levels.entry(price).or_insert_with(|| PriceLevel {
            price: price as i64,
            symbol: symbol.clone(),
            orders: VecDeque::new(),
            total_qty: 0,
            last_update: None,
            side,
            active: false,
        });
        level.orders.push_back(id);
        level.total_qty += qty;
        level.last_update = Some(ts);
        level.active = true;
        self.orders.insert(id, Resting { side, price, qty });
        id
    }

    /// Takes a resting order off the book. `false` if `id` isn't resting (filled, cancelled or unknown).
    pub fn cancel(&mut self, id: OrderId, ts: u64) -> bool {
        let Some(o) = self.orders.remove(&id) else {
            return false;
        };
        let levels = match o.side {
            Side::Bid => &mut self.bids,
            Side::Ask => &mut self.asks,
        };
        let level = levels
            .get_mut(&o.price)
            .expect("a resting order's level exists");
        level.orders.retain(|&x| x != id);
        level.total_qty -= o.qty;
        level.last_update = Some(ts);
        if level.orders.is_empty() {
            levels.remove(&o.price);
        }
        true
    }

    pub fn level(&self, side: Side, price: u32) -> Option<&PriceLevel> {
        match side {
            Side::Bid => self.bids.get(&price),
            Side::Ask => self.asks.get(&price),
        }
    }

    /// The best level: the highest bid, or the lowest ask.
    pub fn best(&self, side: Side) -> Option<&PriceLevel> {
        match side {
            Side::Bid => self.bids.values().next_back(),
            Side::Ask => self.asks.values().next(),
        }
    }

    /// The orders resting at a price, oldest first, with their remaining quantity.
    pub fn orders_at(&self, side: Side, price: u32) -> Vec<(OrderId, u64)> {
        self.level(side, price).map_or(Vec::new(), |l| {
            l.orders
                .iter()
                .map(|id| (*id, self.orders[id].qty))
                .collect()
        })
    }

    /// A market order: takes from the other side, best price first and oldest order first, until `qty`
    /// is filled or that side is empty.
    pub fn execute(&mut self, taker: Side, qty: u64, ts: u64) -> Vec<Fill> {
        let side = taker.opposite();
        let mut left = qty;
        let mut fills = Vec::new();
        while left > 0 {
            let levels = match side {
                Side::Bid => &mut self.bids,
                Side::Ask => &mut self.asks,
            };
            let best = match side {
                Side::Bid => levels.keys().next_back(),
                Side::Ask => levels.keys().next(),
            };
            let Some(price) = best.copied() else {
                break;
            };
            let level = levels.get_mut(&price).expect("just found it");
            while left > 0 {
                let Some(&id) = level.orders.front() else {
                    break;
                };
                let o = self.orders.get_mut(&id).expect("queued orders are resting");
                let take = o.qty.min(left);
                o.qty -= take;
                left -= take;
                level.total_qty -= take;
                fills.push(Fill {
                    maker: id,
                    price,
                    qty: take,
                });
                if o.qty == 0 {
                    level.orders.pop_front();
                    self.orders.remove(&id);
                }
            }
            level.last_update = Some(ts);
            if level.orders.is_empty() {
                levels.remove(&price);
            }
        }
        fills
    }
}
