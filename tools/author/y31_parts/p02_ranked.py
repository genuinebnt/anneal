from y17_parts.common import broken
from y32_parts.code import solution as capstone

# the capstone's `Ranked` uses f64 scores and the member's bytes; this part is the same structure over i64 and &str
SOLUTION = r'''use std::collections::HashMap;

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
            } else if next != NIL {
                self.nodes[prev].links[level].1 += 1;
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
'''

STARTER = r'''/// Players ordered by score, then name.
#[derive(Default)]
pub struct Board {}

impl Board {
    pub fn new() -> Board {
        Board::default()
    }

    pub fn len(&self) -> usize {
        todo!()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn score(&self, name: &str) -> Option<i64> {
        todo!()
    }

    /// Adds the player or changes their score. Returns the earlier score.
    pub fn set(&mut self, name: &str, score: i64) -> Option<i64> {
        todo!()
    }

    pub fn remove(&mut self, name: &str) -> Option<i64> {
        todo!()
    }

    /// The player's place in the order, from 0.
    pub fn rank(&self, name: &str) -> Option<usize> {
        todo!()
    }

    /// How many players have a score below `score`.
    pub fn count_below(&self, score: i64) -> usize {
        todo!()
    }

    /// Up to `count` players from place `start` on.
    pub fn range(&self, start: usize, count: usize) -> Vec<(String, i64)> {
        todo!()
    }
}
'''

HELPERS = r'''
#[allow(dead_code)]
fn pairs(items: &[(&str, i64)]) -> Vec<(String, i64)> {
    items.iter().map(|(n, s)| (n.to_string(), *s)).collect()
}
#[allow(dead_code)]
fn board(items: &[(&str, i64)]) -> Board {
    let mut b = Board::new();
    for (name, score) in items {
        b.set(name, *score);
    }
    b
}
'''

VISIBLE = [
    HELPERS,
    r'''
#[test]
fn ordered_by_score_then_name() {
    let b = board(&[("cat", 5), ("bob", 2), ("ann", 5), ("dan", -1)]);
    check!("the whole board", b.range(0, 10), pairs(&[("dan", -1), ("bob", 2), ("ann", 5), ("cat", 5)]));
    check!("ann's place", b.rank("ann"), Some(2));
    check!("two from place 1", b.range(1, 2), pairs(&[("bob", 2), ("ann", 5)]));
}

#[test]
fn a_new_score_moves_the_player() {
    let mut b = board(&[("ann", 1), ("bob", 2)]);
    check!("the old score", b.set("ann", 3), Some(1));
    check!("ann is last now", b.rank("ann"), Some(1));
    check!("still two players", b.len(), 2);
    check!("ann's score", b.score("ann"), Some(3));
}

#[test]
fn removing() {
    let mut b = board(&[("ann", 1), ("bob", 2), ("cat", 3)]);
    check!("remove bob", b.remove("bob"), Some(2));
    check!("again", b.remove("bob"), None);
    check!("cat moves up", b.rank("cat"), Some(1));
    check!("bob has no place", b.rank("bob"), None);
}

#[test]
fn counting_below_a_score() {
    let b = board(&[("a", 10), ("b", 20), ("c", 20), ("d", 30)]);
    check!("below 20", b.count_below(20), 1);
    check!("below 21", b.count_below(21), 3);
    check!("below the lowest", b.count_below(i64::MIN), 0);
}

#[test]
fn empty_and_past_the_end() {
    check!("an empty board", Board::new().range(0, 5), vec![]);
    let b = board(&[("a", 1)]);
    check!("past the end", b.range(1, 5), vec![]);
    check!("none asked for", b.range(0, 0), vec![]);
    check!("far past the end", b.range(usize::MAX, usize::MAX), vec![]);
}
''',
]

HIDDEN = [
    HELPERS,
    r'''
#[test]
fn nobody_there() {
    let mut b = Board::new();
    check!("len", b.len(), 0);
    check!("rank", b.rank("x"), None);
    check!("score", b.score("x"), None);
    check!("remove", b.remove("x"), None);
    check!("count_below", b.count_below(i64::MAX), 0);
}

#[test]
fn the_same_score_again() {
    let mut b = board(&[("a", 1), ("b", 1), ("c", 1)]);
    check!("set b to 1 again", b.set("b", 1), Some(1));
    check!("nothing moved", b.range(0, 9), pairs(&[("a", 1), ("b", 1), ("c", 1)]));
}

#[test]
fn names_are_compared_as_bytes_and_may_be_any_text() {
    let b = board(&[("b", 0), ("B", 0), ("", 0), ("é", 0), ("a b", 0)]);
    check!("byte order", b.range(0, 9), pairs(&[("", 0), ("B", 0), ("a b", 0), ("b", 0), ("é", 0)]));
}

#[test]
fn extreme_scores() {
    let b = board(&[("hi", i64::MAX), ("lo", i64::MIN), ("mid", 0)]);
    check!("order", b.range(0, 9), pairs(&[("lo", i64::MIN), ("mid", 0), ("hi", i64::MAX)]));
    check!("below max", b.count_below(i64::MAX), 2);
}

#[test]
fn random_changes_match_a_sorted_model() {
    let mut rng = anneal_prelude::Rng::new(0x3103);
    for run in 0..300 {
        let mut b = Board::new();
        let mut model: Vec<(i64, String)> = Vec::new();
        let mut history = Vec::new();
        let names = 2 + rng.below(30);
        for _ in 0..120 {
            let name = format!("p{}", rng.below(names));
            let place = model.iter().position(|(_, n)| *n == name);
            let old = place.map(|i| model.remove(i).0);
            if rng.below(3) == 0 {
                history.push(format!("remove {name}"));
                check!(format!("run {run}: {history:?}"), b.remove(&name), old);
            } else {
                let score = rng.int(-5, 5);
                history.push(format!("set {name} {score}"));
                check!(format!("run {run}: {history:?}"), b.set(&name, score), old);
                model.push((score, name));
                model.sort();
            }
            let ctx = || format!("run {run}: after {history:?}");
            check!(ctx(), b.len(), model.len());
            let probe = format!("p{}", rng.below(names));
            check!(format!("{}, rank {probe}", ctx()), b.rank(&probe), model.iter().position(|(_, n)| *n == probe));
            let cut = rng.int(-7, 7);
            check!(format!("{}, count_below {cut}", ctx()), b.count_below(cut), model.iter().filter(|(s, _)| *s < cut).count());
            let (start, count) = (rng.below(model.len() + 2), rng.below(6));
            let want: Vec<(String, i64)> = model.iter().skip(start).take(count).map(|(s, n)| (n.clone(), *s)).collect();
            check!(format!("{}, range {start} {count}", ctx()), b.range(start, count), want);
        }
    }
}

#[test]
fn one_player_over_and_over() {
    let mut b = Board::new();
    for i in 0..1000 {
        b.set("solo", i);
        check!("first and only", (b.len(), b.rank("solo"), b.range(0, 5)), (1, Some(0), pairs(&[("solo", i)])));
    }
    b.remove("solo");
    check!("empty again", (b.len(), b.range(0, 5)), (0, vec![]));
}

#[test]
fn removed_players_free_their_room() {
    // a long run of churn on a small board: memory that is never reused would grow with every change
    let mut b = Board::new();
    for i in 0..300_000u64 {
        b.set(&format!("p{}", i % 100), (i % 977) as i64);
    }
    check!("still 100 players", b.len(), 100);
    check!("all in place", b.range(0, 200).len(), 100);
}

#[test]
fn a_large_board_stays_fast() {
    // 200000 players, rising and falling: any step that walks or shifts the whole board cannot finish in time
    let mut b = Board::new();
    let n = 200_000i64;
    // lowest scores first would suit a plain list, so scores arrive in a scattered order
    for i in 0..n {
        b.set(&format!("p{i:06}"), (i * 7919) % n);
    }
    for i in 0..n {
        let score = (i * 7919) % n;
        let name = format!("p{i:06}");
        check!(format!("rank of {name}"), b.rank(&name), Some(score as usize));
        check!(format!("count_below {score}"), b.count_below(score), score as usize);
        check!(format!("range {score} 1"), b.range(score as usize, 1), vec![(name.clone(), score)]);
    }
    // every player moves to the front and back again
    for i in 0..n {
        let name = format!("p{i:06}");
        let score = (i * 7919) % n;
        b.set(&name, -1);
        check!(format!("{name} is first"), b.rank(&name), Some(0));
        b.set(&name, score);
    }
}
''',
]

STATEMENT = r'''
A game keeps a leaderboard that changes all the time. Players ask: what is my place? Who is in 40th place? How many have fewer than 1000 points?

Write `Board`. Each player has a name and an `i64` score. The order is by score, **lowest first**, and players with equal scores are ordered by name, as bytes.

- `set(name, score)` adds the player or changes their score, and returns the old score.
- `remove(name)` removes the player and returns their score.
- `score(name)`, `len()`, `is_empty()`.
- `rank(name)` is the player's place, counting from 0.
- `count_below(score)` is how many players have a **strictly lower** score.
- `range(start, count)` lists up to `count` players as `(name, score)`, from place `start` on.

**Speed.** The board may hold 200000 players, and all of these calls are made constantly. A call that walks or shifts the whole board is too slow. `set`, `remove`, `rank` and `count_below` should take about `log n` steps, and `range` about `log n + count`.

Use the standard library only, and no `unsafe`.
'''

PROBLEM = dict(
    slug="who-is-in-40th-place", title="Who is in 40th place?", level="hard", stage="data",
    tags=["order statistics", "skip list", "index arena", "ranking"],
    statement=STATEMENT, starter=STARTER, solution=SOLUTION, visible=VISIBLE, hidden=HIDDEN,
    teaches=[
        "A skip list keeps order with express lanes whose heights are random, without any rebalancing.",
        "Counting how far each link jumps turns a search into rank and select.",
        "An arena with a free list gives a linked structure without `Rc` or `unsafe`.",
    ],
    follow_up="1) Your heights come from a seeded generator. What could an attacker who knows the seed do? 2) Compare with a balanced tree that stores subtree sizes (a treap, for example). Which is easier to get right, and which uses less memory? 3) Redis keeps a hash map beside its skip list. Which operations does each of them serve?",
    hints=[
        ("approach", "A `BTreeSet` keeps order but cannot count how many come before a key. Look for a structure whose search path can add up how many elements it jumped over. A skip list with a width on every link is one; a tree with subtree sizes is another."),
        ("rust", "Keep nodes in a `Vec` and link them by index; node 0 is a head with all levels. Give each node `Vec<(next, span)>`. One helper that returns, for every level, the last node before a point and its place serves insert, remove and rank."),
        ("edge", "When you insert a node of height h, the links on levels above h grow by one. When you remove it, they shrink by one. Set a span of 0 for links to nothing, and decide what a node's place counts from."),
    ],
    notes=("Each link records how many places it skips, so walking down from the top level adds up a node's rank. Insert and remove fix the spans along one search path. A hash map from name to score turns a name into a position in the order.",
           "expected O(log n) per operation, plus the range's length", "O(n)"),
    wrong={
        "ties_ignore_the_name": broken(SOLUTION, (
            "        let path = self.path(|n| (n.score, n.name.as_str()) < (score, name));\n        let height",
            "        let path = self.path(|n| n.score <= score);\n        let height")),
        "tall_links_not_widened": broken(SOLUTION, (
            "            } else if next != NIL {\n                self.nodes[prev].links[level].1 += 1;\n            }", "            }")),
        "rank_by_walking": broken(SOLUTION, (
            "        let score = self.score(name)?;\n        Some(self.path(|n| (n.score, n.name.as_str()) < (score, name))[0].1)",
            "        self.score(name)?;\n        let mut at = self.nodes[0].links[0].0;\n        let mut place = 0;\n        while self.nodes[at].name != name {\n            at = self.nodes[at].links[0].0;\n            place += 1;\n        }\n        Some(place)")),
        "sorted_vector": r'''
            /// Players ordered by score, then name.
            #[derive(Default)]
            pub struct Board {
                order: Vec<(i64, String)>,
                scores: std::collections::HashMap<String, i64>,
            }

            impl Board {
                pub fn new() -> Board {
                    Board::default()
                }
                pub fn len(&self) -> usize {
                    self.order.len()
                }
                pub fn is_empty(&self) -> bool {
                    self.order.is_empty()
                }
                pub fn score(&self, name: &str) -> Option<i64> {
                    self.scores.get(name).copied()
                }
                pub fn set(&mut self, name: &str, score: i64) -> Option<i64> {
                    let old = self.remove(name);
                    let key = (score, name.to_string());
                    let at = self.order.partition_point(|e| e < &key);
                    self.order.insert(at, key);
                    self.scores.insert(name.to_string(), score);
                    old
                }
                pub fn remove(&mut self, name: &str) -> Option<i64> {
                    let score = self.scores.remove(name)?;
                    let at = self.order.partition_point(|e| (e.0, e.1.as_str()) < (score, name));
                    self.order.remove(at);
                    Some(score)
                }
                pub fn rank(&self, name: &str) -> Option<usize> {
                    let score = self.score(name)?;
                    Some(self.order.partition_point(|e| (e.0, e.1.as_str()) < (score, name)))
                }
                pub fn count_below(&self, score: i64) -> usize {
                    self.order.partition_point(|e| e.0 < score)
                }
                pub fn range(&self, start: usize, count: usize) -> Vec<(String, i64)> {
                    self.order.iter().skip(start).take(count).map(|(s, n)| (n.clone(), *s)).collect()
                }
            }
        ''',
    },
)
