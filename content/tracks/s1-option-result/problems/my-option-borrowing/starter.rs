#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum MyOption<T> {
    Some(T),
    None,
}

impl<T> MyOption<T> {
    pub fn as_ref(&self) -> MyOption<&T> {
        todo!()
    }
    pub fn as_mut(&mut self) -> MyOption<&mut T> {
        todo!()
    }
    pub fn insert(&mut self, value: T) -> &mut T {
        todo!()
    }
    pub fn replace(&mut self, value: T) -> MyOption<T> {
        todo!()
    }
    pub fn get_or_insert_with(&mut self, f: impl FnOnce() -> T) -> &mut T {
        todo!()
    }
    pub fn zip<U>(self, other: MyOption<U>) -> MyOption<(T, U)> {
        todo!()
    }
    pub fn inspect(self, f: impl FnOnce(&T)) -> MyOption<T> {
        todo!()
    }
    pub fn map_or_else<U>(self, default: impl FnOnce() -> U, f: impl FnOnce(T) -> U) -> U {
        todo!()
    }
}

impl<T> MyOption<MyOption<T>> {
    pub fn flatten(self) -> MyOption<T> {
        todo!()
    }
}

impl<T: Copy> MyOption<&T> {
    pub fn copied(self) -> MyOption<T> {
        todo!()
    }
}
