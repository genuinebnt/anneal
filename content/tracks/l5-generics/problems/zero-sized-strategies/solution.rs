use std::cmp::Ordering;
use std::marker::PhantomData;

/// How to order items. Implementors are zero-sized: the strategy lives in the type, not in the value.
pub trait Order {
    fn cmp<T: Ord>(a: &T, b: &T) -> Ordering;
}

pub struct Asc;
pub struct Desc;

impl Order for Asc {
    fn cmp<T: Ord>(a: &T, b: &T) -> Ordering {
        a.cmp(b)
    }
}

impl Order for Desc {
    fn cmp<T: Ord>(a: &T, b: &T) -> Ordering {
        b.cmp(a)
    }
}

/// Items kept sorted by `O`, ascending unless you say otherwise.
pub struct SortedVec<T, O = Asc> {
    items: Vec<T>,
    order: PhantomData<O>,
}

impl<T: Ord, O: Order> SortedVec<T, O> {
    pub fn new() -> Self {
        SortedVec {
            items: Vec::new(),
            order: PhantomData,
        }
    }

    /// Inserts `x` after any items that compare equal to it.
    pub fn insert(&mut self, x: T) {
        let i = self
            .items
            .partition_point(|y| O::cmp(y, &x) != Ordering::Greater);
        self.items.insert(i, x);
    }

    /// Whether some item compares equal to `x` under `O`.
    pub fn contains(&self, x: &T) -> bool {
        self.items.binary_search_by(|y| O::cmp(y, x)).is_ok()
    }

    pub fn as_slice(&self) -> &[T] {
        &self.items
    }

    /// The same items sorted by another order; equal items keep their relative order.
    pub fn reorder<O2: Order>(mut self) -> SortedVec<T, O2> {
        self.items.sort_by(O2::cmp);
        SortedVec {
            items: self.items,
            order: PhantomData,
        }
    }
}

impl<T: Ord, O: Order> FromIterator<T> for SortedVec<T, O> {
    fn from_iter<I: IntoIterator<Item = T>>(iter: I) -> Self {
        let mut v = SortedVec::new();
        for x in iter {
            v.insert(x);
        }
        v
    }
}
