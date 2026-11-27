use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq)]
pub struct Cut {
    /// The two old points each new point is halfway between. New point `i` has the number `points + i`.
    pub halfway: Vec<(usize, usize)>,
    pub triangles: Vec<[usize; 3]>,
}

/// Cuts every triangle of a mesh into four.
pub fn cut_in_four(points: usize, triangles: &[[usize; 3]]) -> Option<Cut> {
    let mut halfway = Vec::new();
    let mut known: HashMap<(usize, usize), usize> = HashMap::new();
    let mut finer = Vec::with_capacity(triangles.len() * 4);
    for &[a, b, c] in triangles {
        if a >= points || b >= points || c >= points || a == b || b == c || c == a {
            return None;
        }
        let mut middle = |p: usize, q: usize| -> Option<usize> {
            // The same edge from either side.
            let key = (p.min(q), p.max(q));
            if let Some(&n) = known.get(&key) {
                return Some(n);
            }
            let number = points + halfway.len();
            halfway.push(key);
            known.insert(key, number);
            Some(number)
        };
        let (ab, bc, ca) = (middle(a, b)?, middle(b, c)?, middle(c, a)?);
        finer.extend([[a, ab, ca], [b, bc, ab], [c, ca, bc], [ab, bc, ca]]);
    }
    Some(Cut {
        halfway,
        triangles: finer,
    })
}
