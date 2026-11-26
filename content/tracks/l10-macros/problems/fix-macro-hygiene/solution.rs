/// The error `ensure!` returns.
#[derive(Debug, PartialEq, Eq)]
pub struct Failure(pub String);

#[macro_export]
macro_rules! max_of {
    ($a:expr $(,)?) => {
        $a
    };
    ($a:expr, $($rest:expr),+ $(,)?) => {
        // A tuple is evaluated left to right, and each argument appears once.
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
        if !$condition {
            return ::core::result::Result::Err($crate::Failure(::std::format!($($message)+)));
        }
    };
}
