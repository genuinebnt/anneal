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

fn nothing() -> Shape {
    Shape::Group(Vec::new())
}

/// The move that every one of two or more shapes makes, if they all make the same one.
fn shared_move(shapes: &[Shape]) -> Option<(i64, i64)> {
    let moved = |s: &Shape| match s {
        Shape::Move(x, y, _) => Some((*x, *y)),
        _ => None,
    };
    let first = moved(shapes.first()?)?;
    (shapes.len() > 1 && shapes.iter().all(|s| moved(s) == Some(first))).then_some(first)
}

/// Apply one rule to the top of a shape whose parts are already tidy. `Err` gives the shape back unchanged.
fn apply_a_rule(shape: Shape) -> Result<Shape, Shape> {
    match shape {
        Shape::Hidden(_) => Ok(nothing()),
        Shape::Move(0, 0, inner) => Ok(*inner),
        Shape::Move(x, y, inner) => match *inner {
            Shape::Move(a, b, deeper) => Ok(Shape::Move(x + a, y + b, deeper)),
            Shape::Group(parts) if parts.is_empty() => Ok(nothing()),
            other => Err(Shape::Move(x, y, Box::new(other))),
        },
        Shape::Group(parts) if parts.iter().any(|p| matches!(p, Shape::Group(_))) => {
            let mut flat = Vec::new();
            for part in parts {
                match part {
                    Shape::Group(inner) => flat.extend(inner),
                    other => flat.push(other),
                }
            }
            Ok(Shape::Group(flat))
        }
        Shape::Group(mut parts) if parts.len() == 1 => Ok(parts.remove(0)),
        Shape::Group(parts) => match shared_move(&parts) {
            Some((x, y)) => {
                let inner = parts
                    .into_iter()
                    .map(|p| match p {
                        Shape::Move(_, _, s) => *s,
                        other => other,
                    })
                    .collect();
                Ok(Shape::Move(x, y, Box::new(Shape::Group(inner))))
            }
            None => Err(Shape::Group(parts)),
        },
        dot => Err(dot),
    }
}

/// A shape that draws the same and that no tidying rule can change any further.
pub fn tidy(shape: Shape) -> Shape {
    let shape = match shape {
        Shape::Group(parts) => Shape::Group(parts.into_iter().map(tidy).collect()),
        Shape::Move(x, y, inner) => Shape::Move(x, y, Box::new(tidy(*inner))),
        other => other,
    };
    match apply_a_rule(shape) {
        // what a rule builds may match a rule again, at the top or below it
        Ok(changed) => changed,
        Err(unchanged) => unchanged,
    }
}
