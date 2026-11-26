use std::fmt::Display;

pub trait Describe {
    fn describe(&self) -> String;
}

impl<T: Display + ?Sized> Describe for T {
    fn describe(&self) -> String {
        format!("<{self}>")
    }
}

/// Things to describe later. They may borrow data that lives for `'a`.
pub struct Board<'a> {
    items: Vec<Box<dyn Describe + 'a>>,
}

impl<'a> Board<'a> {
    pub fn new() -> Self {
        Board { items: Vec::new() }
    }

    pub fn pin<T: Describe>(&mut self, item: T) {
        self.items.push(Box::new(item));
    }

    /// Every item's description, joined with " ".
    pub fn render(&self) -> String {
        self.items
            .iter()
            .map(|i| i.describe())
            .collect::<Vec<_>>()
            .join(" ")
    }
}

/// Pins a borrow of each item.
pub fn pin_all<'a, T: Display>(board: &mut Board<'a>, items: &'a [T]) {
    for it in items {
        board.pin(it);
    }
}

/// `x`, boxed to be described later, for as long as `'a`.
pub fn boxed<'a, T: Display>(x: T) -> Box<dyn Describe + 'a> {
    Box::new(x)
}

/// `x`, boxed to be kept anywhere.
pub fn boxed_forever<T: Display>(x: T) -> Box<dyn Describe> {
    Box::new(x)
}

/// Each item's description, lazily.
pub fn describe_each<'a, T: Describe>(items: &'a [T]) -> impl Iterator<Item = String> + 'a {
    items.iter().map(|t| t.describe())
}
