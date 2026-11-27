/// A rectangle with sides along the axes, edges included.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    pub x0: f64,
    pub y0: f64,
    pub x1: f64,
    pub y1: f64,
}

/// The most rectangles kept together at the bottom.
const LEAF: usize = 4;

#[derive(Debug)]
struct Node {
    around: Rect,
    /// A leaf: where its numbers start in `order`. Otherwise: the second child. The first child is
    /// the next node.
    first: usize,
    /// 0 for a node with children.
    held: usize,
}

#[derive(Debug)]
pub struct Index {
    rects: Vec<Rect>,
    nodes: Vec<Node>,
    order: Vec<usize>,
}

/// The smallest rectangle around all of the numbered ones. `min` and `max` pass over NaN.
fn around(rects: &[Rect], numbers: &[usize]) -> Rect {
    let nothing = Rect {
        x0: f64::INFINITY,
        y0: f64::INFINITY,
        x1: f64::NEG_INFINITY,
        y1: f64::NEG_INFINITY,
    };
    numbers
        .iter()
        .map(|&i| rects[i])
        .fold(nothing, |a, r| Rect {
            x0: a.x0.min(r.x0),
            y0: a.y0.min(r.y0),
            x1: a.x1.max(r.x1),
            y1: a.y1.max(r.y1),
        })
}

impl Index {
    pub fn new(rects: &[Rect]) -> Index {
        let mut index = Index {
            rects: rects.to_vec(),
            nodes: Vec::new(),
            order: (0..rects.len()).collect(),
        };
        if !rects.is_empty() {
            let mut order = std::mem::take(&mut index.order);
            index.add(&mut order, 0);
            index.order = order;
        }
        index
    }

    fn add(&mut self, numbers: &mut [usize], start: usize) {
        let me = self.nodes.len();
        let whole = around(&self.rects, numbers);
        if numbers.len() <= LEAF {
            self.nodes.push(Node {
                around: whole,
                first: start,
                held: numbers.len(),
            });
            return;
        }
        let middle = |r: Rect, axis: usize| if axis == 0 { r.x0 + r.x1 } else { r.y0 + r.y1 };
        let spread = |axis: usize| {
            let values = numbers.iter().map(|&i| middle(self.rects[i], axis));
            let (low, high) = values.fold((f64::INFINITY, f64::NEG_INFINITY), |(l, h), v| {
                (l.min(v), h.max(v))
            });
            high - low
        };
        let _ = spread;
        let axis = 0;
        // Equal numbers of rectangles on both sides, whatever their places are.
        let half = numbers.len() / 2;
        let rects = &self.rects;
        numbers.select_nth_unstable_by(half, |&a, &b| {
            middle(rects[a], axis).total_cmp(&middle(rects[b], axis))
        });
        self.nodes.push(Node {
            around: whole,
            first: 0,
            held: 0,
        });
        let (one, two) = numbers.split_at_mut(half);
        self.add(one, start);
        self.nodes[me].first = self.nodes.len();
        self.add(two, start + half);
    }

    /// The numbers, in rising order, of all rectangles that `touches` says yes to.
    pub fn search(&self, touches: &mut dyn FnMut(&Rect) -> bool) -> Vec<usize> {
        let mut found = Vec::new();
        let mut waiting: Vec<usize> = if self.nodes.is_empty() {
            vec![]
        } else {
            vec![0]
        };
        while let Some(at) = waiting.pop() {
            let node = &self.nodes[at];
            if !touches(&node.around) {
                continue;
            }
            if node.held > 0 {
                for &i in &self.order[node.first..node.first + node.held] {
                    if touches(&self.rects[i]) {
                        found.push(i);
                    }
                }
            } else {
                waiting.extend([at + 1, node.first]);
            }
        }
        found.sort_unstable();
        found
    }
}
