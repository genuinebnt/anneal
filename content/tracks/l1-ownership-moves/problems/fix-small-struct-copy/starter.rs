use std::marker::PhantomData;

pub struct Point {
    pub x: i32,
    pub y: i32,
}

pub struct Pixel {
    pub at: Point,
    pub rgb: [u8; 3],
}

/// Owns its name, so a copy of a sprite needs its own name.
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
