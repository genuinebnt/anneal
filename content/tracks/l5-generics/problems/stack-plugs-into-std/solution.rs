use std::fmt;

/// A LIFO stack. Iterating yields items top first, the order `pop` would give them.
#[derive(Debug)]
pub struct Stack<T> {
    items: Vec<T>,
}

impl<T> Stack<T> {
    pub fn new() -> Self {
        Stack { items: Vec::new() }
    }

    pub fn push(&mut self, item: T) {
        self.items.push(item);
    }

    pub fn pop(&mut self) -> Option<T> {
        self.items.pop()
    }

    pub fn peek(&self) -> Option<&T> {
        self.items.last()
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }
}

// derive(Default) would add `T: Default`; an empty stack needs nothing from T.
impl<T> Default for Stack<T> {
    fn default() -> Self {
        Stack::new()
    }
}

/// Pushes in iteration order, so the last item ends up on top.
impl<T> FromIterator<T> for Stack<T> {
    fn from_iter<I: IntoIterator<Item = T>>(iter: I) -> Self {
        Stack {
            items: iter.into_iter().collect(),
        }
    }
}

impl<T> Extend<T> for Stack<T> {
    fn extend<I: IntoIterator<Item = T>>(&mut self, iter: I) {
        self.items.extend(iter);
    }
}

impl<T> IntoIterator for Stack<T> {
    type Item = T;
    type IntoIter = std::iter::Rev<std::vec::IntoIter<T>>;

    fn into_iter(self) -> Self::IntoIter {
        self.items.into_iter().rev()
    }
}

impl<'a, T> IntoIterator for &'a Stack<T> {
    type Item = &'a T;
    type IntoIter = std::iter::Rev<std::slice::Iter<'a, T>>;

    fn into_iter(self) -> Self::IntoIter {
        self.items.iter().rev()
    }
}

/// `[top, ..., bottom]`
impl<T: fmt::Display> fmt::Display for Stack<T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "[")?;
        for (i, x) in self.into_iter().enumerate() {
            if i > 0 {
                write!(f, ", ")?;
            }
            write!(f, "{x}")?;
        }
        write!(f, "]")
    }
}
