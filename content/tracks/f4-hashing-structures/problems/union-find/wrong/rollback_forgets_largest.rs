/// A point to come back to. What it holds is up to you.
#[derive(Debug, Clone, Copy)]
pub struct Checkpoint(pub usize);

enum Change {
    Joined { child: usize, old_largest: usize },
    Added { old_largest: usize },
}

pub struct Groups {
    parent: Vec<usize>,
    size: Vec<usize>,
    count: usize,
    largest: usize,
    history: Vec<Change>,
}

impl Groups {
    pub fn new(n: usize) -> Self {
        Groups {
            parent: (0..n).collect(),
            size: vec![1; n],
            count: n,
            largest: n.min(1),
            history: Vec::new(),
        }
    }

    /// No path compression: union by size keeps paths short, and undo stays trivial.
    fn root(&self, mut x: usize) -> usize {
        while self.parent[x] != x {
            x = self.parent[x];
        }
        x
    }

    pub fn union(&mut self, a: usize, b: usize) -> bool {
        let (mut big, mut small) = (self.root(a), self.root(b));
        if big == small {
            return false;
        }
        if self.size[big] < self.size[small] {
            std::mem::swap(&mut big, &mut small);
        }
        self.history.push(Change::Joined {
            child: small,
            old_largest: self.largest,
        });
        self.parent[small] = big;
        self.size[big] += self.size[small];
        self.count -= 1;
        self.largest = self.largest.max(self.size[big]);
        true
    }

    pub fn connected(&mut self, a: usize, b: usize) -> bool {
        self.root(a) == self.root(b)
    }

    pub fn count(&self) -> usize {
        self.count
    }

    pub fn group_size(&mut self, a: usize) -> usize {
        let root = self.root(a);
        self.size[root]
    }

    pub fn largest(&self) -> usize {
        self.largest
    }

    pub fn add(&mut self) -> usize {
        let id = self.parent.len();
        self.history.push(Change::Added {
            old_largest: self.largest,
        });
        self.parent.push(id);
        self.size.push(1);
        self.count += 1;
        self.largest = self.largest.max(1);
        id
    }

    pub fn checkpoint(&mut self) -> Checkpoint {
        Checkpoint(self.history.len())
    }

    pub fn rollback(&mut self, mark: Checkpoint) {
        while self.history.len() > mark.0 {
            match self.history.pop() {
                Some(Change::Joined { child, old_largest }) => {
                    let root = self.parent[child];
                    self.size[root] -= self.size[child];
                    self.parent[child] = child;
                    self.count += 1;
                    let _ = old_largest;
                }
                Some(Change::Added { old_largest }) => {
                    self.parent.pop();
                    self.size.pop();
                    self.count -= 1;
                    self.largest = old_largest;
                }
                None => break,
            }
        }
    }
}
