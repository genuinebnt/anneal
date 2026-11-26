use std::fmt::Display;

/// How a value is shown in a report.
pub trait Label {
    fn label(&self) -> String;
}

/// Anything printable shows itself with Display.
impl<T: Display> Label for T {
    fn label(&self) -> String {
        self.to_string()
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
