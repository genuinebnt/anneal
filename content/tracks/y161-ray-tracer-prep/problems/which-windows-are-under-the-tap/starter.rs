/// A rectangle with sides along the axes, edges included.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    pub x0: f64,
    pub y0: f64,
    pub x1: f64,
    pub y1: f64,
}

#[derive(Debug)]
pub struct Index {}

impl Index {
    pub fn new(rects: &[Rect]) -> Index {
        todo!()
    }

    /// The numbers, in rising order, of all rectangles that `touches` says yes to.
    pub fn search(&self, touches: &mut dyn FnMut(&Rect) -> bool) -> Vec<usize> {
        todo!()
    }
}
