#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Shape {
    /// A dot, with a number that names it.
    Dot(u32),
    /// Its shapes, drawn in order. `Group(vec![])` draws nothing.
    Group(Vec<Shape>),
    /// The shape, moved right by the first number and down by the second.
    Move(i64, i64, Box<Shape>),
    /// A shape that is switched off: it draws nothing.
    Hidden(Box<Shape>),
}

/// A shape that draws the same and that no tidying rule can change any further.
pub fn tidy(shape: Shape) -> Shape {
    todo!()
}
