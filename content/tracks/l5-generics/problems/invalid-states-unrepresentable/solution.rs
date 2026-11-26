/// A list that can't be empty: the first item is a separate field, so there's no empty value to represent.
#[derive(Debug, Clone, PartialEq)]
pub struct NonEmpty<T> {
    head: T,
    tail: Vec<T>,
}

/// The error from converting an empty Vec.
#[derive(Debug, PartialEq)]
pub struct Empty;

impl<T> NonEmpty<T> {
    pub fn new(head: T) -> Self {
        NonEmpty {
            head,
            tail: Vec::new(),
        }
    }

    pub fn push(&mut self, x: T) {
        self.tail.push(x);
    }

    /// Removes and returns the last item, unless it's the only one.
    pub fn pop(&mut self) -> Option<T> {
        self.tail.pop()
    }

    pub fn first(&self) -> &T {
        &self.head
    }

    pub fn last(&self) -> &T {
        self.tail.last().unwrap_or(&self.head)
    }

    pub fn len(&self) -> usize {
        1 + self.tail.len()
    }

    pub fn get(&self, i: usize) -> Option<&T> {
        if i == 0 {
            Some(&self.head)
        } else {
            self.tail.get(i - 1)
        }
    }

    pub fn split_first(&self) -> (&T, &[T]) {
        (&self.head, &self.tail)
    }

    pub fn iter(&self) -> impl Iterator<Item = &T> + '_ {
        std::iter::once(&self.head).chain(&self.tail)
    }

    pub fn map<U>(self, mut f: impl FnMut(T) -> U) -> NonEmpty<U> {
        let head = f(self.head);
        NonEmpty {
            head,
            tail: self.tail.into_iter().map(f).collect(),
        }
    }

    /// The largest item; the first of equal largest ones. No Option: there's always one.
    pub fn max(&self) -> &T
    where
        T: Ord,
    {
        self.iter()
            .fold(&self.head, |best, x| if x > best { x } else { best })
    }

    pub fn sort(&mut self)
    where
        T: Ord,
    {
        // Put everything in one Vec, sort, and split the smallest back out as the head.
        let mut all = std::mem::take(&mut self.tail);
        all.sort();
        let mut rest = all.into_iter();
        if let Some(first) = rest.next() {
            if first < self.head {
                let old_head = std::mem::replace(&mut self.head, first);
                let mut tail: Vec<T> = rest.collect();
                let at = tail.partition_point(|x| *x <= old_head);
                tail.insert(at, old_head);
                self.tail = tail;
            } else {
                self.tail = std::iter::once(first).chain(rest).collect();
            }
        }
    }

    pub fn into_vec(self) -> Vec<T> {
        let mut v = Vec::with_capacity(self.len());
        v.push(self.head);
        v.extend(self.tail);
        v
    }
}

impl<T> TryFrom<Vec<T>> for NonEmpty<T> {
    type Error = Empty;

    fn try_from(v: Vec<T>) -> Result<Self, Empty> {
        let mut items = v.into_iter();
        match items.next() {
            Some(head) => Ok(NonEmpty {
                head,
                tail: items.collect(),
            }),
            None => Err(Empty),
        }
    }
}

impl<T> IntoIterator for NonEmpty<T> {
    type Item = T;
    type IntoIter = std::iter::Chain<std::iter::Once<T>, std::vec::IntoIter<T>>;

    fn into_iter(self) -> Self::IntoIter {
        std::iter::once(self.head).chain(self.tail)
    }
}
