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

use std::ops::{Deref, DerefMut};

/// A path of points. A newtype, because `Display` and `Vec` are both foreign.
pub struct Path(pub Vec<Point>);

/// A path prints as its points joined by " -> ", or "(empty)".
impl fmt::Display for Path {
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

/// Lets a Path be used like the Vec it wraps: len, indexing, iter, push, &Path as &[Point].
impl Deref for Path {
    type Target = Vec<Point>;

    fn deref(&self) -> &Vec<Point> {
        &self.0
    }
}

impl DerefMut for Path {
    fn deref_mut(&mut self) -> &mut Vec<Point> {
        &mut self.0
    }
}

impl From<Vec<Point>> for Path {
    fn from(points: Vec<Point>) -> Self {
        Path(points)
    }
}

impl FromIterator<Point> for Path {
    fn from_iter<I: IntoIterator<Item = Point>>(iter: I) -> Self {
        Path(iter.into_iter().collect())
    }
}

/// Total Manhattan length of a path.
pub fn length(path: &[Point]) -> u32 {
    path.windows(2)
        .map(|w| w[0].x.abs_diff(w[1].x) + w[0].y.abs_diff(w[1].y))
        .sum()
}
