use std::collections::BTreeMap;
use std::num::NonZeroU32;

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

/// An index into the book's order arena. Stored as `index + 1` in a `NonZeroU32`, so `Option<Slot>` is
/// 4 bytes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Slot(NonZeroU32);

impl Slot {
    fn new(i: usize) -> Slot {
        Slot(NonZeroU32::new(i as u32 + 1).expect("fewer than u32::MAX orders"))
    }

    fn get(self) -> usize {
        self.0.get() as usize - 1
    }
}

/// Every resting order at one price on one side, oldest first. 32 bytes, half a cache line:
/// - the orders live in the book's arena, linked into a FIFO, so the level keeps only `head` and `tail`;
/// - the side is which map the level sits in, and the symbol belongs to the book;
/// - "active" is `count > 0` (empty levels are removed); prices are `u32` ticks;
/// - a level only exists once something touched it, so its timestamp is never missing.
pub struct PriceLevel {
    qty: u64,
    last_update: u64,
    price: u32,
    count: u32,
    head: Option<Slot>,
    tail: Option<Slot>,
}

impl PriceLevel {
    pub fn price(&self) -> u32 {
        self.price
    }

    /// The quantity resting at this price.
    pub fn qty(&self) -> u64 {
        self.qty
    }

    /// How many orders rest at this price.
    pub fn count(&self) -> usize {
        self.count as usize
    }

    /// When an order at this price was last added, cancelled or filled.
    pub fn last_update(&self) -> Option<u64> {
        Some(self.last_update)
    }
}

/// One order in the arena. A freed slot joins the free list through `next`, and its generation moves
/// on, so an old `OrderId` for the slot stops matching.
struct Order {
    qty: u64,
    price: u32,
    gen: u32,
    prev: Option<Slot>,
    next: Option<Slot>,
    side: Side,
    live: bool,
}

/// A limit order book for one instrument, with price-time priority.
pub struct Book {
    symbol: Box<str>,
    bids: BTreeMap<u32, PriceLevel>,
    asks: BTreeMap<u32, PriceLevel>,
    orders: Vec<Order>,
    free: Option<Slot>,
    live: usize,
}

impl Book {
    pub fn new(symbol: &str) -> Book {
        Book::with_capacity(symbol, 0)
    }

    /// Room for `orders` resting orders, reserved up front.
    pub fn with_capacity(symbol: &str, orders: usize) -> Book {
        Book {
            symbol: symbol.into(),
            bids: BTreeMap::new(),
            asks: BTreeMap::new(),
            orders: Vec::with_capacity(orders),
            free: None,
            live: 0,
        }
    }

    pub fn symbol(&self) -> &str {
        &self.symbol
    }

    /// Resting orders on both sides.
    pub fn len(&self) -> usize {
        self.live
    }

    pub fn is_empty(&self) -> bool {
        self.live == 0
    }

    /// The low 32 bits are the slot, the high 32 its generation.
    fn id_of(&self, s: Slot) -> OrderId {
        OrderId(((self.orders[s.get()].gen as u64) << 32) | s.get() as u64)
    }

    fn resolve(&self, id: OrderId) -> Option<Slot> {
        let i = (id.0 & 0xFFFF_FFFF) as usize;
        let o = self.orders.get(i)?;
        (o.live && o.gen == (id.0 >> 32) as u32).then(|| Slot::new(i))
    }

    /// A slot for `order`: the head of the free list, or a new one at the end.
    fn alloc(&mut self, order: Order) -> Slot {
        self.live += 1;
        match self.free {
            Some(s) => {
                let old = &mut self.orders[s.get()];
                self.free = old.next;
                *old = Order {
                    gen: old.gen,
                    ..order
                };
                s
            }
            None => {
                self.orders.push(order);
                Slot::new(self.orders.len() - 1)
            }
        }
    }

    /// Rests an order for `qty` (more than 0) at `price`, behind every order already there.
    pub fn add(&mut self, side: Side, price: u32, qty: u64, ts: u64) -> OrderId {
        assert!(qty > 0, "orders need a quantity");
        let slot = self.alloc(Order {
            qty,
            price,
            gen: 0,
            prev: None,
            next: None,
            side,
            live: true,
        });
        let levels = match side {
            Side::Bid => &mut self.bids,
            Side::Ask => &mut self.asks,
        };
        let level = levels.entry(price).or_insert(PriceLevel {
            qty: 0,
            last_update: ts,
            price,
            count: 0,
            head: None,
            tail: None,
        });
        let old_tail = level.tail;
        self.orders[slot.get()].prev = old_tail;
        match old_tail {
            Some(t) => self.orders[t.get()].next = Some(slot),
            None => level.head = Some(slot),
        }
        level.tail = Some(slot);
        level.count += 1;
        level.qty += qty;
        level.last_update = ts;
        self.id_of(slot)
    }

    /// Unlinks `slot` from its level (removing the level if it empties) and frees it: O(1).
    fn remove(&mut self, slot: Slot, ts: u64) {
        let o = &self.orders[slot.get()];
        let (side, price, prev, next, qty) = (o.side, o.price, o.prev, o.next, o.qty);
        let levels = match side {
            Side::Bid => &mut self.bids,
            Side::Ask => &mut self.asks,
        };
        let level = levels
            .get_mut(&price)
            .expect("a resting order's level exists");
        match prev {
            Some(p) => self.orders[p.get()].next = next,
            None => level.head = next,
        }
        match next {
            Some(n) => self.orders[n.get()].prev = prev,
            None => level.tail = prev,
        }
        level.count -= 1;
        level.qty -= qty;
        level.last_update = ts;
        if level.count == 0 {
            levels.remove(&price);
        }
        let o = &mut self.orders[slot.get()];
        o.live = false;
        o.gen = o.gen.wrapping_add(1);
        o.next = self.free;
        self.free = Some(slot);
        self.live -= 1;
    }

    /// Takes a resting order off the book. `false` if `id` isn't resting (filled, cancelled or unknown).
    pub fn cancel(&mut self, id: OrderId, ts: u64) -> bool {
        match self.resolve(id) {
            Some(slot) => {
                self.remove(slot, ts);
                true
            }
            None => false,
        }
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
        let mut out = Vec::new();
        let mut cur = self.level(side, price).and_then(|l| l.head);
        while let Some(s) = cur {
            out.push((self.id_of(s), self.orders[s.get()].qty));
            cur = self.orders[s.get()].next;
        }
        out
    }

    /// A market order: takes from the other side, best price first and oldest order first, until `qty`
    /// is filled or that side is empty.
    pub fn execute(&mut self, taker: Side, qty: u64, ts: u64) -> Vec<Fill> {
        let side = taker.opposite();
        let mut left = qty;
        let mut fills = Vec::new();
        while left > 0 {
            let Some(level) = self.best(side) else {
                break;
            };
            let (price, head) = (level.price, level.head.expect("empty levels are removed"));
            let take = self.orders[head.get()].qty.min(left);
            fills.push(Fill {
                maker: self.id_of(head),
                price,
                qty: take,
            });
            left -= take;
            if take == self.orders[head.get()].qty {
                self.remove(head, ts);
            } else {
                self.orders[head.get()].qty -= take;
                let levels = match side {
                    Side::Bid => &mut self.bids,
                    Side::Ask => &mut self.asks,
                };
                let level = levels.get_mut(&price).expect("just found it");
                level.qty -= take;
                level.last_update = ts;
            }
        }
        fills
    }
}
