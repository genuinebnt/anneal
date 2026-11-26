use std::fmt;
use std::hash::{Hash, Hasher};
use std::marker::PhantomData;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Point {
    pub x: i32,
    pub y: i32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Pixel {
    pub at: Point,
    pub rgb: [u8; 3],
}

/// Owns its name, so a copy of a sprite needs its own name.
#[derive(Debug, Clone, PartialEq)]
pub struct Sprite {
    pub name: String,
    pub at: Point,
}

/// A typed index: an `Id<Sprite>` can't be passed where an `Id<Pixel>` is expected. Whatever `T` is, it's one u32.
pub struct Id<T> {
    pub raw: u32,
    marker: PhantomData<T>,
}

impl<T> Id<T> {
    pub fn new(raw: u32) -> Self {
        Id {
            raw,
            marker: PhantomData,
        }
    }
}

// derive(Clone, Copy, PartialEq, ...) would add `T: Clone`, `T: Copy`, ... to each impl. An Id is only a u32,
// so these impls ask nothing of T.
impl<T> Clone for Id<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T> Copy for Id<T> {}

impl<T> PartialEq for Id<T> {
    fn eq(&self, other: &Self) -> bool {
        self.raw == other.raw
    }
}

impl<T> Eq for Id<T> {}

impl<T> Hash for Id<T> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.raw.hash(state);
    }
}

impl<T> fmt::Debug for Id<T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Id({})", self.raw)
    }
}

// Nothing below needs to change.

/// `n` pixels: `p` moved 0, 1, ..., n - 1 to the right.
pub fn trail(p: Pixel, n: i32) -> Vec<Pixel> {
    (0..n)
        .map(|i| Pixel {
            at: Point {
                x: p.at.x + i,
                y: p.at.y,
            },
            rgb: p.rgb,
        })
        .collect()
}

/// The sprite, and a copy of it placed at `to`.
pub fn fork(s: &Sprite, to: Point) -> (Sprite, Sprite) {
    let mut copy = s.clone();
    copy.at = to;
    (s.clone(), copy)
}

/// The ids in `a` that also appear in `b`, in `a`'s order.
pub fn common<T>(a: &[Id<T>], b: &[Id<T>]) -> Vec<Id<T>> {
    a.iter().filter(|x| b.contains(x)).copied().collect()
}
