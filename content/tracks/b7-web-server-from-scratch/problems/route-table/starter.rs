/// The values a pattern captured.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Params<'path> {
    // TODO: change this as you like
    unused: std::marker::PhantomData<&'path str>,
}

impl Params<'_> {
    pub fn get(&self, name: &str) -> Option<&str> {
        todo!()
    }

    pub fn len(&self) -> usize {
        todo!()
    }

    pub fn is_empty(&self) -> bool {
        todo!()
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum Found<'table, 'path, T> {
    Route(&'table T, Params<'path>),
    WrongMethod(Vec<&'table str>),
    Nothing,
}

pub struct Table<T> {
    // TODO
    unused: std::marker::PhantomData<T>,
}

impl<T> Default for Table<T> {
    fn default() -> Self {
        Table::new()
    }
}

impl<T> Table<T> {
    pub fn new() -> Table<T> {
        todo!()
    }

    pub fn add(&mut self, method: &str, pattern: &str, value: T) {
        todo!()
    }

    pub fn find<'table, 'path>(
        &'table self,
        method: &str,
        path: &'path str,
    ) -> Found<'table, 'path, T> {
        todo!()
    }
}
