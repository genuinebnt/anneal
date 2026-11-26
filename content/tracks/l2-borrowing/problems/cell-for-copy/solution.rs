use std::cell::Cell;

/// A node in a doubly linked ring. The links are plain shared references, so every node can be reached from
/// every other one while all of them are only borrowed.
pub struct Node<'a> {
    pub name: String,
    visits: Cell<u32>,
    next: Cell<Option<&'a Node<'a>>>,
    prev: Cell<Option<&'a Node<'a>>>,
}

impl<'a> Node<'a> {
    pub fn new(name: &str) -> Self {
        Node {
            name: name.to_string(),
            visits: Cell::new(0),
            next: Cell::new(None),
            prev: Cell::new(None),
        }
    }

    pub fn visits(&self) -> u32 {
        self.visits.get()
    }

    pub fn next(&self) -> Option<&'a Node<'a>> {
        self.next.get()
    }

    pub fn prev(&self) -> Option<&'a Node<'a>> {
        self.prev.get()
    }

    /// Makes `other` come right after `self`: `self.next` is `other` and `other.prev` is `self`.
    pub fn link(&'a self, other: &'a Node<'a>) {
        self.next.set(Some(other));
        other.prev.set(Some(self));
    }

    /// Takes `self` out of its ring: its neighbours link to each other, and `self` links to nothing. A node
    /// alone in its ring (linked to itself) just ends up unlinked.
    pub fn unlink(&self) {
        let (prev, next) = (self.prev.take(), self.next.take());
        if let (Some(p), Some(n)) = (prev, next) {
            if !std::ptr::eq(p, self) {
                p.next.set(Some(n));
                n.prev.set(Some(p));
            }
        }
    }
}

/// Links `nodes` into one ring, in order, the last back to the first.
pub fn ring<'a>(nodes: &'a [Node<'a>]) {
    for pair in nodes.windows(2) {
        pair[0].link(&pair[1]);
    }
    if let (Some(first), Some(last)) = (nodes.first(), nodes.last()) {
        last.link(first);
    }
}

/// Starting at `start`, visits `steps` nodes along `next` links (counting each visit on the node), stopping
/// early at a node with no `next`. Returns the names visited.
pub fn walk<'a>(start: &'a Node<'a>, steps: usize) -> Vec<&'a str> {
    let mut out = Vec::with_capacity(steps);
    let mut cur = Some(start);
    while let Some(node) = cur {
        if out.len() == steps {
            break;
        }
        node.visits.set(node.visits.get() + 1);
        out.push(node.name.as_str());
        cur = node.next.get();
    }
    out
}
