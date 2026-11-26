#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum MyOption<T> {
    Some(T),
    None,
}

impl<T> MyOption<T> {
    pub fn is_some_and(self, f: impl FnOnce(T) -> bool) -> bool {
        todo!()
    }
    pub fn unwrap_or_else(self, f: impl FnOnce() -> T) -> T {
        todo!()
    }
    pub fn unwrap_or_default(self) -> T
    where
        T: Default,
    {
        todo!()
    }
    pub fn map<U>(self, f: impl FnOnce(T) -> U) -> MyOption<U> {
        todo!()
    }
    pub fn and_then<U>(self, f: impl FnOnce(T) -> MyOption<U>) -> MyOption<U> {
        todo!()
    }
    pub fn and<U>(self, other: MyOption<U>) -> MyOption<U> {
        todo!()
    }
    pub fn or_else(self, f: impl FnOnce() -> MyOption<T>) -> MyOption<T> {
        todo!()
    }
    pub fn filter(self, keep: impl FnOnce(&T) -> bool) -> MyOption<T> {
        todo!()
    }
    pub fn take(&mut self) -> MyOption<T> {
        todo!()
    }
    pub fn iter(&self) -> Iter<'_, T> {
        todo!()
    }
}

/// Yields a reference to the value, at most once.
pub struct Iter<'a, T> {
    inner: MyOption<&'a T>,
}

impl<'a, T> Iterator for Iter<'a, T> {
    type Item = &'a T;

    fn next(&mut self) -> Option<&'a T> {
        todo!()
    }
}
