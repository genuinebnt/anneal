use std::fmt::Display;

/// How a value is shown in a report.
pub trait Label {
    fn label(&self) -> String;
}

/// One impl per printable type, instead of a blanket impl over `Display` that would overlap the
/// `Vec` and `Option` impls below (std may add `impl Display for Vec<T>` one day).
macro_rules! label_via_display {
    ($($t:ty),*) => {
        $(
            impl Label for $t {
                fn label(&self) -> String {
                    self.to_string()
                }
            }
        )*
    };
}

label_via_display!(i32, i64, u64, f64, bool, char, str, String);

/// `&str`, `&Vec<..>` and other references show what they point at.
impl<T: Label + ?Sized> Label for &T {
    fn label(&self) -> String {
        (**self).label()
    }
}

/// A list shows its items' labels in brackets: [1, 2, 3].
impl<T: Label> Label for Vec<T> {
    fn label(&self) -> String {
        format!(
            "[{}]",
            self.iter().map(Label::label).collect::<Vec<_>>().join(", ")
        )
    }
}

/// A missing value shows as "-".
impl<T: Label> Label for Option<T> {
    fn label(&self) -> String {
        match self {
            Some(v) => v.label(),
            None => "-".to_string(),
        }
    }
}
