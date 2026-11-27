from y17_parts.common import broken

SOLUTION = r'''use std::collections::HashMap;

/// A source of random numbers. `below(n)` is in `0..n`.
pub trait Dice {
    fn below(&mut self, n: usize) -> usize;
}

/// Sessions and when each one ends.
#[derive(Debug, Default)]
pub struct Sessions {
    /// In no particular order, so that any one can be picked by its index.
    items: Vec<(String, u64)>,
    places: HashMap<String, usize>,
}

impl Sessions {
    pub fn new() -> Sessions {
        Sessions::default()
    }

    /// Adds the session, or moves its end.
    pub fn set(&mut self, id: &str, ends_at: u64) {
        match self.places.get(id) {
            Some(&i) => self.items[i].1 = ends_at,
            None => {
                self.places.insert(id.to_string(), self.items.len());
                self.items.push((id.to_string(), ends_at));
            }
        }
    }

    pub fn remove(&mut self, id: &str) -> bool {
        let Some(i) = self.places.remove(id) else {
            return false;
        };
        // the last item moves into the hole
        self.items.swap_remove(i);
        if let Some((moved, _)) = self.items.get(i) {
            self.places.insert(moved.clone(), i);
        }
        true
    }

    pub fn ends_at(&self, id: &str) -> Option<u64> {
        self.places.get(id).map(|&i| self.items[i].1)
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// Removes ended sessions found by chance, and returns their ids.
    pub fn sweep(&mut self, now: u64, dice: &mut dyn Dice, max_rounds: usize) -> Vec<String> {
        let mut removed = Vec::new();
        for _ in 0..max_rounds {
            let looks = self.len().min(20);
            if looks == 0 {
                break;
            }
            let before = removed.len();
            for _ in 0..looks {
                if self.is_empty() {
                    break;
                }
                let (id, ends_at) = &self.items[dice.below(self.len())];
                if *ends_at <= now {
                    let id = id.clone();
                    self.remove(&id);
                    removed.push(id);
                }
            }
            if (removed.len() - before) * 4 <= looks {
                break;
            }
        }
        removed
    }
}
'''

STARTER = r'''/// A source of random numbers. `below(n)` is in `0..n`.
pub trait Dice {
    fn below(&mut self, n: usize) -> usize;
}

/// Sessions and when each one ends.
#[derive(Default)]
pub struct Sessions {}

impl Sessions {
    pub fn new() -> Sessions {
        Sessions::default()
    }

    /// Adds the session, or moves its end.
    pub fn set(&mut self, id: &str, ends_at: u64) {
        todo!()
    }

    pub fn remove(&mut self, id: &str) -> bool {
        todo!()
    }

    pub fn ends_at(&self, id: &str) -> Option<u64> {
        todo!()
    }

    pub fn len(&self) -> usize {
        todo!()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Removes ended sessions found by chance, and returns their ids.
    pub fn sweep(&mut self, now: u64, dice: &mut dyn Dice, max_rounds: usize) -> Vec<String> {
        todo!()
    }
}
'''

HELPERS = r'''
use anneal_prelude::Rng;

/// Real randomness, from a seed. It remembers every question.
#[allow(dead_code)]
struct Seeded {
    rng: Rng,
    asked: Vec<usize>,
}
#[allow(dead_code)]
impl Seeded {
    fn new(seed: u64) -> Seeded {
        Seeded { rng: Rng::new(seed), asked: Vec::new() }
    }
}
impl Dice for Seeded {
    fn below(&mut self, n: usize) -> usize {
        assert!(n > 0, "below(0) has no answer");
        self.asked.push(n);
        self.rng.below(n)
    }
}

/// Gives the same number every time, or the largest allowed if that is smaller.
#[allow(dead_code)]
struct Loaded(usize);
impl Dice for Loaded {
    fn below(&mut self, n: usize) -> usize {
        self.0.min(n - 1)
    }
}

/// `total` sessions; those whose number is under `ended` end at 10, the rest at 1000.
#[allow(dead_code)]
fn sessions(total: usize, ended: usize) -> Sessions {
    let mut s = Sessions::new();
    for i in 0..total {
        s.set(&format!("s{i}"), if i < ended { 10 } else { 1000 });
    }
    s
}
#[allow(dead_code)]
fn number(id: &str) -> usize {
    id[1..].parse().expect("an id")
}
'''

VISIBLE = [
    HELPERS,
    r'''
#[test]
fn set_ends_and_remove() {
    let mut s = Sessions::new();
    s.set("a", 5);
    s.set("b", 9);
    s.set("a", 7);
    check!("two sessions", s.len(), 2);
    check!("a moved", s.ends_at("a"), Some(7));
    check!("remove a", s.remove("a"), true);
    check!("again", s.remove("a"), false);
    check!("b is still there", (s.len(), s.ends_at("b")), (1, Some(9)));
}

#[test]
fn if_all_have_ended_they_all_go() {
    let mut s = sessions(100, 100);
    let mut gone = s.sweep(10, &mut Seeded::new(1), 100);
    gone.sort();
    check!("all 100, each once", (gone.len(), s.len()), (100, 0));
}

#[test]
fn if_none_has_ended_one_round_is_enough() {
    let mut s = sessions(100, 100);
    let mut dice = Seeded::new(2);
    check!("nothing at 9", s.sweep(9, &mut dice, 100), Vec::<String>::new());
    check!("twenty looks", dice.asked.len(), 20);
}

#[test]
fn a_small_store_needs_fewer_looks() {
    let mut dice = Seeded::new(3);
    sessions(7, 0).sweep(10, &mut dice, 5);
    check!("seven looks for seven sessions", dice.asked, vec![7; 7]);
}

#[test]
fn zero_rounds_and_an_empty_store() {
    let mut dice = Seeded::new(4);
    check!("no rounds", sessions(5, 5).sweep(10, &mut dice, 0), Vec::<String>::new());
    check!("nothing to look at", Sessions::new().sweep(10, &mut dice, 5), Vec::<String>::new());
    check!("no dice were thrown", dice.asked.len(), 0);
}
''',
]

HIDDEN = [
    HELPERS,
    r'''
#[test]
fn a_session_ends_at_its_time_exactly() {
    let mut s = sessions(1, 1);
    check!("at 9", s.sweep(9, &mut Seeded::new(1), 1).len(), 0);
    check!("at 10", s.sweep(10, &mut Seeded::new(1), 1), vec!["s0".to_string()]);
}

#[test]
fn each_look_asks_for_an_index_among_the_sessions_left() {
    let mut s = sessions(30, 30);
    let mut dice = Seeded::new(5);
    s.sweep(10, &mut dice, 1);
    check!("one round of 20 looks, the store shrinking each time", dice.asked, (11..=30).rev().collect::<Vec<_>>());
}

#[test]
fn a_round_stops_when_the_store_is_empty() {
    let mut s = sessions(3, 3);
    let mut dice = Seeded::new(6);
    check!("all three", s.sweep(10, &mut dice, 100).len(), 3);
    check!("three questions", dice.asked, vec![3, 2, 1]);
}

#[test]
fn different_indexes_name_different_sessions() {
    // one session of 12 has ended; a die stuck on i finds it for exactly one i
    for ended in 0..12 {
        let mut found = 0;
        for i in 0..12 {
            let mut s = Sessions::new();
            for j in 0..12 {
                s.set(&format!("s{j}"), if j == ended { 10 } else { 1000 });
            }
            found += s.sweep(10, &mut Loaded(i), 1).len();
        }
        check!(format!("session s{ended}"), found, 1);
    }
}

#[test]
fn the_indexes_stay_one_to_one_after_removals() {
    let mut rng = Rng::new(0x3105);
    for run in 0..200 {
        let mut s = Sessions::new();
        let mut live: Vec<String> = Vec::new();
        let mut history = Vec::new();
        for step in 0..40 {
            if !live.is_empty() && rng.below(3) == 0 {
                let id = live.swap_remove(rng.below(live.len()));
                history.push(format!("remove {id}"));
                check!(format!("run {run}: {history:?}"), s.remove(&id), true);
            } else {
                let id = format!("s{step}");
                history.push(format!("set {id}"));
                s.set(&id, 1000);
                live.push(id);
            }
        }
        // end one of them: still exactly one stuck die finds it
        if live.is_empty() {
            continue;
        }
        let victim = live[rng.below(live.len())].clone();
        let mut hits = 0;
        for i in 0..live.len() {
            s.set(&victim, 10);
            let gone = s.sweep(10, &mut Loaded(i), 1);
            if !gone.is_empty() {
                check!(format!("run {run}: {history:?}"), gone, vec![victim.clone()]);
                hits += 1;
                // put it back; the indexes may now be assigned anew
                break;
            }
        }
        check!(format!("run {run}: every session can be reached after {history:?}"), hits, 1);
    }
}

#[test]
fn another_round_only_if_more_than_a_quarter_were_ended() {
    let mut rng = Rng::new(0x3107);
    let (mut stopped_at_five, mut went_on_at_six) = (0, 0);
    for run in 0..3000 {
        // large enough that three rounds never empty it
        let total = 61 + rng.below(60);
        let ended = rng.below(total * 2 / 3);
        let mut s = sessions(total, ended);
        let mut dice = Seeded::new(10_000 + run);
        let removed = s.sweep(10, &mut dice, 3).len();
        let looks = dice.asked.len();
        let ctx = format!("run {run}: {total} sessions, {ended} ended; the dice were asked below(n) for n = {:?}", dice.asked);
        check!(format!("{ctx}: whole rounds"), looks % 20 == 0 && (20..=60).contains(&looks), true);
        // the size drops by one after each look that removes, so the questions show each round's removals
        let mut sizes = dice.asked.clone();
        sizes.push(total - removed);
        let rounds = looks / 20;
        for round in 0..rounds {
            let hits = sizes[round * 20] - sizes[(round + 1) * 20];
            if round + 1 < rounds {
                check!(format!("{ctx}: round {} removed {hits}, yet another ran", round + 1), hits >= 6, true);
                went_on_at_six += usize::from(hits == 6);
            } else if rounds < 3 {
                check!(format!("{ctx}: round {} removed {hits}, and none followed", round + 1), hits <= 5, true);
                stopped_at_five += usize::from(hits == 5);
            }
        }
    }
    check!("both sides of the threshold came up", stopped_at_five > 0 && went_on_at_six > 0, true);
}

#[test]
fn only_ended_sessions_are_ever_removed() {
    let mut rng = Rng::new(0x3106);
    for run in 0..300 {
        let total = rng.below(60);
        let ended = rng.below(total + 1);
        let mut s = sessions(total, ended);
        let rounds = rng.below(8);
        let mut dice = Seeded::new(run);
        let gone = s.sweep(10, &mut dice, rounds);
        let ctx = format!("run {run}: {total} sessions, {ended} ended, {rounds} rounds, removed {gone:?}");
        check!(ctx.clone(), gone.iter().all(|id| number(id) < ended && s.ends_at(id).is_none()), true);
        let unique: std::collections::HashSet<&String> = gone.iter().collect();
        check!(ctx.clone(), (unique.len(), s.len()), (gone.len(), total - gone.len()));
        check!(format!("{ctx}: at most {rounds} rounds"), dice.asked.len() <= rounds * 20, true);
    }
}

#[test]
fn with_half_ended_the_sweep_keeps_going() {
    // stopping needs a round with 5 ended or fewer among 20, which is rare while half have ended
    let gone: usize = (0..20).map(|seed| sessions(2000, 1000).sweep(10, &mut Seeded::new(seed), usize::MAX).len()).sum();
    check!(format!("removed {gone} over 20 sweeps of 1000 ended each"), gone > 2000 && gone <= 20_000, true);
}

#[test]
fn repeated_sweeps_find_everything() {
    let mut s = sessions(500, 40);
    let mut dice = Seeded::new(8);
    let total: usize = (0..2000).map(|_| s.sweep(10, &mut dice, 1).len()).sum();
    check!("all 40", (total, s.len()), (40, 460));
}

#[test]
fn a_large_store_stays_fast() {
    // picking, adding or removing by walking the store cannot finish in time
    let mut s = Sessions::new();
    let n = 200_000;
    for i in 0..n {
        s.set(&format!("s{i}"), (i % 2) as u64);
    }
    let mut dice = Seeded::new(9);
    let first = s.sweep(0, &mut dice, usize::MAX).len();
    check!("only ended sessions go", first > 0 && first <= n / 2, true);
    let mut left = n - first;
    for _ in 0..20_000 {
        left -= s.sweep(1, &mut dice, 1).len();
    }
    check!("len agrees", s.len(), left);
    for i in 0..n {
        s.remove(&format!("s{i}"));
    }
    check!("empty", s.len(), 0);
}
''',
]

STATEMENT = r'''
A web service stores a session for every visitor, each with an end time. A session that has ended is noticed when somebody asks for it. Most visitors never come back, so their sessions stay and fill the memory.

Looking through every session would stall the service. Instead, each sweep **looks at a few picked at random**, and keeps going only while that pays off.

Write `Sessions` with `set(id, ends_at)`, `remove(id)`, `ends_at(id)`, `len()` and `sweep(now, dice, max_rounds)`. A session has **ended** when `ends_at <= now`.

`set` on an id that is already there only moves its end. It adds and removes nothing.

**Picking.** `dice.below(n)` returns a number in `0..n`. A look calls `dice.below(len())`, with the number of sessions at that moment, and looks at the session that number stands for.

While no session is added or removed, each number in `0..len()` stands for one session, and different numbers for different sessions. Which one is up to you.

**A round** is up to `min(20, len())` looks, with `len()` taken when the round starts. Each ended session it looks at is removed at once. A round also stops if no session is left.

**`sweep`** runs rounds and returns the ids it removed. After a round, it runs another only if

- more than a quarter of that round's planned looks removed a session (with 20 looks, 6 or more),
- sessions remain, and
- it has run fewer than `max_rounds` rounds.

With no sessions or `max_rounds == 0`, it does nothing and throws no dice.

The store holds hundreds of thousands of sessions. `set`, `remove`, `ends_at` and **each look** must take constant time on average.
'''

PROBLEM = dict(
    slug="throw-out-the-stale", title="Throw out what has gone stale", level="medium", stage="data",
    tags=["random sampling", "swap_remove", "index map", "expiry"],
    statement=STATEMENT, starter=STARTER, solution=SOLUTION, visible=VISIBLE, hidden=HIDDEN,
    teaches=[
        "A Vec plus a map from key to index gives constant-time insert, delete and uniform random choice.",
        "Sampling repeats while it finds enough garbage, so the effort follows how much there is.",
        "Passing the randomness in makes a random algorithm testable.",
    ],
    follow_up="1) After a sweep stops, what share of the sessions would you expect to be ended? Why does the quarter threshold bound it? 2) Compare with a heap ordered by end time. When is each the better choice? 3) Why must a real server also bound the time one sweep takes?",
    hints=[
        ("approach", "Which collection can give you its i-th element at once? What breaks about that when you remove from its middle, and how can you remove without shifting?"),
        ("rust", "`Vec::swap_remove(i)` moves the last element into the hole in O(1). A `HashMap<String, usize>` remembers where each id sits; fix the entry of the element that moved."),
        ("edge", "Remove the map entry first, then check whether an element moved into index `i` at all. The threshold compares with the looks the round planned, even if it ended early."),
    ],
    notes=("The sessions sit in a Vec in no particular order, with a map from id to position. A random index is then a uniform random session, and `swap_remove` keeps the indexes dense. The sweep repeats while more than a quarter of a round's looks found something.",
           "O(1) per look and per change", "O(sessions)"),
    wrong={
        "moved_item_keeps_its_old_place": broken(SOLUTION, (
            "        if let Some((moved, _)) = self.items.get(i) {\n            self.places.insert(moved.clone(), i);\n        }\n", "")),
        "ends_strictly_before": broken(SOLUTION, ("                if *ends_at <= now {", "                if *ends_at < now {")),
        "quarter_is_enough": broken(SOLUTION, ("            if (removed.len() - before) * 4 <= looks {", "            if (removed.len() - before) * 4 < looks {")),
        "one_round_only": broken(SOLUTION, ("            if (removed.len() - before) * 4 <= looks {", "            if before == 0 {")),
        "always_twenty_looks": broken(SOLUTION, ("            let looks = self.len().min(20);\n            if looks == 0 {", "            let looks = if self.is_empty() { 0 } else { 20 };\n            if looks == 0 {")),
        "picks_by_walking": broken(SOLUTION, (
            "                let (id, ends_at) = &self.items[dice.below(self.len())];", "                let index = dice.below(self.len());\n                let (id, ends_at) = self.items.iter().enumerate().find(|(i, (id, _))| *i == index && self.places.contains_key(id)).map(|(_, e)| e).expect(\"in range\");")),
    },
)
