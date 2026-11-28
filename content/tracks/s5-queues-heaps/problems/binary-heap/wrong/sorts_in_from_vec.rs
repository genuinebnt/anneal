// Wrong: from_vec sorts the items. A descending vector is a valid heap, but sorting costs O(n log n)
// comparisons. It looks linear on already sorted input, so the tests also build from scrambled items.

use std::cmp::Ordering;

type Comparator<T> = Box<dyn Fn(&T, &T) -> Ordering>;

pub struct PriorityQueue<T> {
    items: Vec<T>,
    cmp: Comparator<T>,
}

impl<T: Ord + 'static> PriorityQueue<T> {
    pub fn new() -> Self {
        Self::with_comparator(T::cmp)
    }

    pub fn from_vec(items: Vec<T>) -> Self {
        // Sorted descending is a valid heap, but sorting costs O(n log n) comparisons.
        let mut items = items;
        items.sort_by(|a, b| b.cmp(a));
        let mut queue = Self::new();
        queue.items = items;
        queue
    }
}

impl<T: Ord + 'static> Default for PriorityQueue<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T> PriorityQueue<T> {
    pub fn with_comparator(cmp: impl Fn(&T, &T) -> Ordering + 'static) -> Self {
        PriorityQueue {
            items: Vec::new(),
            cmp: Box::new(cmp),
        }
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn push(&mut self, item: T) {
        self.items.push(item);
        self.sift_up(self.items.len() - 1);
    }

    pub fn pop(&mut self) -> Option<T> {
        if self.items.is_empty() {
            return None;
        }
        let top = self.items.swap_remove(0);
        self.sift_down(0);
        Some(top)
    }

    pub fn peek(&self) -> Option<&T> {
        self.items.first()
    }

    fn greater(&self, a: usize, b: usize) -> bool {
        (self.cmp)(&self.items[a], &self.items[b]) == Ordering::Greater
    }

    fn sift_up(&mut self, mut i: usize) {
        while i > 0 {
            let parent = (i - 1) / 2;
            if !self.greater(i, parent) {
                break;
            }
            self.items.swap(i, parent);
            i = parent;
        }
    }

    fn sift_down(&mut self, mut i: usize) {
        loop {
            let left = 2 * i + 1;
            if left >= self.items.len() {
                break;
            }
            let right = left + 1;
            let child = if right < self.items.len() && self.greater(right, left) {
                right
            } else {
                left
            };
            if !self.greater(child, i) {
                break;
            }
            self.items.swap(i, child);
            i = child;
        }
    }
}

pub fn top_k<T: Ord + 'static>(items: impl IntoIterator<Item = T>, k: usize) -> Vec<T> {
    if k == 0 {
        return Vec::new();
    }
    // Reversed order: the smallest of the kept items is on top.
    let mut kept = PriorityQueue::with_comparator(|a: &T, b: &T| b.cmp(a));
    for item in items {
        if kept.len() < k {
            kept.push(item);
        } else if kept.peek().is_some_and(|smallest| item > *smallest) {
            kept.pop();
            kept.push(item);
        }
    }
    let mut out: Vec<T> = std::iter::from_fn(|| kept.pop()).collect();
    out.reverse();
    out
}
