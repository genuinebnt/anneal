/// The error `ensure!` returns.
#[derive(Debug, PartialEq, Eq)]
pub struct Failure(pub String);

#[macro_export]
macro_rules! max_of {
    ($a:expr) => {
        $a
    };
    ($a:expr, $($rest:expr),+) => {
        if $a >= $crate::max_of!($($rest),+) { $a } else { $crate::max_of!($($rest),+) }
    };
}

#[macro_export]
macro_rules! map {
    ($($key:expr => $value:expr),*) => {{
        let mut map = HashMap::new();
        $( map.insert($key, $value); )*
        map
    }};
}

#[macro_export]
macro_rules! ensure {
    ($condition:expr, $($message:tt)+) => {
        let message = format!($($message)+);
        if !$condition {
            return Err(Failure(message));
        }
    };
}
