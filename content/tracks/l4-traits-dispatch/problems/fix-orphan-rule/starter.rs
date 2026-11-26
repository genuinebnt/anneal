use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Point {
    pub x: i32,
    pub y: i32,
}

impl fmt::Display for Point {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "({}, {})", self.x, self.y)
    }
}

/// A path prints as its points joined by " -> ", or "(empty)".
impl fmt::Display for Vec<Point> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        if self.is_empty() {
            return f.write_str("(empty)");
        }
        for (i, p) in self.iter().enumerate() {
            if i > 0 {
                f.write_str(" -> ")?;
            }
            write!(f, "{p}")?;
        }
        Ok(())
    }
}

/// Total Manhattan length of a path.
pub fn length(path: &[Point]) -> u32 {
    path.windows(2)
        .map(|w| w[0].x.abs_diff(w[1].x) + w[0].y.abs_diff(w[1].y))
        .sum()
}
