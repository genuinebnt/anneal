#[derive(Debug, PartialEq, Eq)]
pub enum MyResult<T, E> {
    Ok(T),
    Err(E),
}

impl<T, E> MyResult<T, E> {
    pub fn map<U>(self, f: impl FnOnce(T) -> U) -> MyResult<U, E> {
        todo!()
    }
    pub fn map_err<F>(self, f: impl FnOnce(E) -> F) -> MyResult<T, F> {
        todo!()
    }
    pub fn and_then<U>(self, f: impl FnOnce(T) -> MyResult<U, E>) -> MyResult<U, E> {
        todo!()
    }
    pub fn is_ok_and(self, f: impl FnOnce(T) -> bool) -> bool {
        todo!()
    }
    pub fn ok(self) -> Option<T> {
        todo!()
    }
}

/// Like `?` for `MyResult`: the value on `Ok`, an early return on `Err`.
#[macro_export]
macro_rules! try_my {
    ($e:expr) => {
        match $e {
            $crate::MyResult::Ok(v) => v,
            $crate::MyResult::Err(_) => todo!("return the error, converted with From"),
        }
    };
}
