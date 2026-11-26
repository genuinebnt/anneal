/// The error `ensure!` returns.
#[derive(Debug, PartialEq, Eq)]
pub struct Failure(pub String);

#[macro_export]
macro_rules! max_of {
    ($a:expr $(,)?) => {
        $a
    };
    ($a:expr, $($rest:expr),+ $(,)?) => {
        match ($a, $crate::max_of!($($rest),+)) {
            (first, rest) => {
                if rest > first {
                    rest
                } else {
                    first
                }
            }
        }
    };
}

#[macro_export]
macro_rules! map {
    ($($key:expr => $value:expr),* $(,)?) => {{
        #[allow(unused_mut)]
        let mut map = ::std::collections::HashMap::new();
        $( map.insert($key, $value); )*
        map
    }};
}

#[macro_export]
macro_rules! ensure {
    ($condition:expr, $($message:tt)+) => {
        let message = ::std::format!($($message)+);
        if !$condition {
            return ::core::result::Result::Err($crate::Failure(message));
        }
    };
}
