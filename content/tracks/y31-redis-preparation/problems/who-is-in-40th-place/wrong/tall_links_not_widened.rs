use std::collections::HashMap;

const NIL: usize = usize::MAX;
const LEVELS: usize = 32;

#[derive(Debug)]
struct Node {
    score: i64,
    name: String,
    /// `links[l]` is the next node on level `l` and how many places further on it is.
    links: Vec<(usize, usize)>,
}

/// Players ordered by score, then name.
#[derive(Debug)]
pub struct Board {
    /// Node 0 is the head and holds nobody.
    nodes: Vec<Node>,
    free: Vec<usize>,
    scores: HashMap<String, i64>,
    seed: u64,
}

impl Default for Board {
    fn default() -> Self {
        Self::new()
    }
}

impl Board {
    pub fn new() -> Board {
        let head = Node {
            score: 0,
            name: String::new(),
            links: vec![(NIL, 0); LEVELS],
        };
        Board {
            nodes: vec![head],
            free: Vec::new(),
            scores: HashMap::new(),
            seed: 0x9E37_79B9_7F4A_7C15,
        }
    }

    pub fn len(&self) -> usize {
        self.scores.len()
    }

    pub fn is_empty(&self) -> bool {
        self.scores.is_empty()
    }

    pub fn score(&self, name: &str) -> Option<i64> {
        self.scores.get(name).copied()
    }

    /// The last node on each level for which `before` holds, and that node's place.
    fn path(&self, before: impl Fn(&Node) -> bool) -> [(usize, usize); LEVELS] {
        let mut path = [(0, 0); LEVELS];
        let (mut at, mut place) = (0, 0);
        for level in (0..LEVELS).rev() {
            loop {
                let (next, span) = self.nodes[at].links[level];
                if next == NIL || !before(&self.nodes[next]) {
                    break;
                }
                at = next;
                place += span;
            }
            path[level] = (at, place);
        }
        path
    }

    fn height(&mut self) -> usize {
        let mut height = 1;
        while height < LEVELS {
            self.seed ^= self.seed << 13;
            self.seed ^= self.seed >> 7;
            self.seed ^= self.seed << 17;
            if (self.seed >> 33) & 3 != 0 {
                break;
            }
            height += 1;
        }
        height
    }

    /// Adds the player or changes their score. Returns the earlier score.
    pub fn set(&mut self, name: &str, score: i64) -> Option<i64> {
        let old = self.remove(name);
        let path = self.path(|n| (n.score, n.name.as_str()) < (score, name));
        let height = self.height();
        let node = Node {
            score,
            name: name.to_string(),
            links: vec![(NIL, 0); height],
        };
        let id = match self.free.pop() {
            Some(id) => {
                self.nodes[id] = node;
                id
            }
            None => {
                self.nodes.push(node);
                self.nodes.len() - 1
            }
        };
        let place = path[0].1 + 1;
        for (level, &(prev, prev_place)) in path.iter().enumerate() {
            let (next, span) = self.nodes[prev].links[level];
            if level < height {
                let left = place - prev_place;
                self.nodes[prev].links[level] = (id, left);
                let right = if next == NIL { 0 } else { span + 1 - left };
                self.nodes[id].links[level] = (next, right);
            }
        }
        self.scores.insert(name.to_string(), score);
        old
    }

    pub fn remove(&mut self, name: &str) -> Option<i64> {
        let score = self.scores.remove(name)?;
        let path = self.path(|n| (n.score, n.name.as_str()) < (score, name));
        let id = self.nodes[path[0].0].links[0].0;
        for (level, &(prev, _)) in path.iter().enumerate() {
            let (next, span) = self.nodes[prev].links[level];
            if next == id {
                let (after, further) = self.nodes[id].links[level];
                let span = if after == NIL { 0 } else { span + further - 1 };
                self.nodes[prev].links[level] = (after, span);
            } else if next != NIL {
                self.nodes[prev].links[level].1 -= 1;
            }
        }
        self.nodes[id].name = String::new();
        self.free.push(id);
        Some(score)
    }

    /// The player's place in the order, from 0.
    pub fn rank(&self, name: &str) -> Option<usize> {
        let score = self.score(name)?;
        Some(self.path(|n| (n.score, n.name.as_str()) < (score, name))[0].1)
    }

    /// How many players have a score below `score`.
    pub fn count_below(&self, score: i64) -> usize {
        self.path(|n| n.score < score)[0].1
    }

    /// Up to `count` players from place `start` on.
    pub fn range(&self, start: usize, count: usize) -> Vec<(String, i64)> {
        let (mut at, mut place) = (0, 0);
        for level in (0..LEVELS).rev() {
            loop {
                let (next, span) = self.nodes[at].links[level];
                // a node's place counts from 1, so this stops just before place `start`
                if next == NIL || place + span > start {
                    break;
                }
                at = next;
                place += span;
            }
        }
        let mut out = Vec::new();
        at = self.nodes[at].links[0].0;
        while at != NIL && out.len() < count {
            out.push((self.nodes[at].name.clone(), self.nodes[at].score));
            at = self.nodes[at].links[0].0;
        }
        out
    }
}
