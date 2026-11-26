#[derive(Debug, PartialEq, Eq)]
pub enum MyResult<T, E> {
    Ok(T),
    Err(E),
}

impl<T, E> MyResult<T, E> {
    pub fn map<U>(self, f: impl FnOnce(T) -> U) -> MyResult<U, E> {
        match self {
            MyResult::Ok(v) => MyResult::Ok(f(v)),
            MyResult::Err(e) => MyResult::Err(e),
        }
    }
    pub fn map_err<F>(self, f: impl FnOnce(E) -> F) -> MyResult<T, F> {
        match self {
            MyResult::Ok(v) => MyResult::Ok(v),
            MyResult::Err(e) => MyResult::Err(f(e)),
        }
    }
    pub fn and_then<U>(self, f: impl FnOnce(T) -> MyResult<U, E>) -> MyResult<U, E> {
        match self {
            MyResult::Ok(v) => f(v),
            MyResult::Err(e) => MyResult::Err(e),
        }
    }
    pub fn is_ok_and(self, f: impl FnOnce(T) -> bool) -> bool {
        match self {
            MyResult::Ok(v) => f(v),
            MyResult::Err(_) => false,
        }
    }
    pub fn ok(self) -> Option<T> {
        match self {
            MyResult::Ok(v) => Some(v),
            MyResult::Err(_) => None,
        }
    }
}

/// Like `?` for `MyResult`: the value on `Ok`, an early return on `Err`.
#[macro_export]
macro_rules! try_my {
    ($e:expr) => {
        match $e {
            $crate::MyResult::Ok(v) => v,
            $crate::MyResult::Err(e) => {
                return $crate::MyResult::Err(::core::convert::From::from(e))
            }
        }
    };
}
