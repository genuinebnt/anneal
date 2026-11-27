#[derive(Debug, Clone, PartialEq)]
pub struct Cut {
    /// The two old points each new point is halfway between. New point `i` has the number `points + i`.
    pub halfway: Vec<(usize, usize)>,
    pub triangles: Vec<[usize; 3]>,
}

/// Cuts every triangle of a mesh into four.
pub fn cut_in_four(points: usize, triangles: &[[usize; 3]]) -> Option<Cut> {
    todo!()
}
