use std::fmt;

pub trait Describe {
    fn describe(&self) -> String;
}

/// A user name, shown in lowercase.
pub struct Name(pub String);

impl fmt::Display for Name {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.0.to_lowercase())
    }
}

// A blanket `impl<T: Display> Describe for T` would overlap the Vec and Option impls below (std may add
// `Display for Vec<_>` one day) and would stop other crates writing their own Describe for Display types.
macro_rules! describe_via_display {
    ($($t:ty),* $(,)?) => {
        $(
            impl Describe for $t {
                fn describe(&self) -> String {
                    self.to_string()
                }
            }
        )*
    };
}

describe_via_display!(
    i8, i16, i32, i64, i128, isize, u8, u16, u32, u64, u128, usize, f32, f64, bool, char, str,
    String, Name
);

impl<T: Describe + ?Sized> Describe for &T {
    fn describe(&self) -> String {
        (**self).describe()
    }
}

/// `[a, b, c]`
impl<T: Describe> Describe for Vec<T> {
    fn describe(&self) -> String {
        let parts: Vec<String> = self.iter().map(|x| x.describe()).collect();
        format!("[{}]", parts.join(", "))
    }
}

/// The value, or `-` for None.
impl<T: Describe> Describe for Option<T> {
    fn describe(&self) -> String {
        match self {
            Some(x) => x.describe(),
            None => "-".to_string(),
        }
    }
}
