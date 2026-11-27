from y17_parts.common import broken

SOLUTION = r'''use std::collections::{BTreeMap, BTreeSet, HashMap};

#[derive(Debug)]
struct Waiter<T> {
    /// When it started waiting, as a counter.
    order: u64,
    wants: Vec<String>,
    deadline: Option<u64>,
    ticket: T,
}

/// Requests that wait for the first of several items, or until a deadline.
#[derive(Debug)]
pub struct Waiting<T> {
    parked: u64,
    waiters: HashMap<u64, Waiter<T>>,
    /// For each item, its waiters by the order they came.
    queues: HashMap<String, BTreeMap<u64, u64>>,
    deadlines: BTreeSet<(u64, u64, u64)>,
}

impl<T> Waiting<T> {
    pub fn new() -> Waiting<T> {
        Waiting {
            parked: 0,
            waiters: HashMap::new(),
            queues: HashMap::new(),
            deadlines: BTreeSet::new(),
        }
    }

    pub fn park(&mut self, id: u64, wants: &[&str], deadline: Option<u64>, ticket: T) -> bool {
        if self.waiters.contains_key(&id) {
            return false;
        }
        let order = self.parked;
        self.parked += 1;
        let wants: Vec<String> = wants.iter().map(|w| w.to_string()).collect();
        for item in &wants {
            self.queues
                .entry(item.clone())
                .or_default()
                .insert(order, id);
        }
        if let Some(at) = deadline {
            self.deadlines.insert((at, order, id));
        }
        let waiter = Waiter {
            order,
            wants,
            deadline,
            ticket,
        };
        self.waiters.insert(id, waiter);
        true
    }

    pub fn cancel(&mut self, id: u64) -> Option<T> {
        let waiter = self.waiters.remove(&id)?;
        for item in &waiter.wants {
            if let Some(queue) = self.queues.get_mut(item) {
                queue.remove(&waiter.order);
                if queue.is_empty() {
                    self.queues.remove(item);
                }
            }
        }
        if let Some(at) = waiter.deadline {
            self.deadlines.remove(&(at, waiter.order, id));
        }
        Some(waiter.ticket)
    }

    pub fn wake(&mut self, item: &str) -> Option<(u64, T)> {
        let id = *self.queues.get(item)?.values().next()?;
        self.cancel(id).map(|ticket| (id, ticket))
    }

    pub fn expire(&mut self, now: u64) -> Vec<(u64, T)> {
        let mut out = Vec::new();
        while let Some(&(at, _, id)) = self.deadlines.first() {
            if at > now {
                break;
            }
            if let Some(ticket) = self.cancel(id) {
                out.push((id, ticket));
            }
        }
        out
    }

    pub fn next_deadline(&self) -> Option<u64> {
        self.deadlines.first().map(|&(at, _, _)| at)
    }

    pub fn contains(&self, id: u64) -> bool {
        self.waiters.contains_key(&id)
    }

    pub fn len(&self) -> usize {
        self.waiters.len()
    }

    pub fn is_empty(&self) -> bool {
        self.waiters.is_empty()
    }
}

impl<T> Default for Waiting<T> {
    fn default() -> Self {
        Waiting::new()
    }
}
'''

STARTER = r'''use std::marker::PhantomData;

/// Requests that wait for the first of several items, or until a deadline.
pub struct Waiting<T> {
    // replace with your own fields
    ticket: PhantomData<T>,
}

impl<T> Waiting<T> {
    pub fn new() -> Waiting<T> {
        Waiting { ticket: PhantomData }
    }

    /// Starts waiting. `false`, and nothing changes, if `id` is already waiting.
    pub fn park(&mut self, id: u64, wants: &[&str], deadline: Option<u64>, ticket: T) -> bool {
        todo!()
    }

    /// Stops `id` waiting and returns its ticket.
    pub fn cancel(&mut self, id: u64) -> Option<T> {
        todo!()
    }

    /// One `item` has arrived: removes and returns whoever gets it.
    pub fn wake(&mut self, item: &str) -> Option<(u64, T)> {
        todo!()
    }

    /// Removes and returns everyone whose deadline has come.
    pub fn expire(&mut self, now: u64) -> Vec<(u64, T)> {
        todo!()
    }

    /// The earliest deadline of anyone waiting.
    pub fn next_deadline(&self) -> Option<u64> {
        todo!()
    }

    pub fn contains(&self, id: u64) -> bool {
        todo!()
    }

    pub fn len(&self) -> usize {
        todo!()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl<T> Default for Waiting<T> {
    fn default() -> Self {
        Waiting::new()
    }
}
'''

VISIBLE = [
    r'''
#[test]
fn the_longest_waiting_gets_the_item() {
    let mut w = Waiting::new();
    check!("park 7", w.park(7, &["book"], None, "seven"), true);
    check!("park 3", w.park(3, &["book"], None, "three"), true);
    check!("first", w.wake("book"), Some((7, "seven")));
    check!("second", w.wake("book"), Some((3, "three")));
    check!("nobody left", w.wake("book"), None);
}

#[test]
fn whoever_is_woken_stops_waiting_for_everything() {
    let mut w = Waiting::new();
    w.park(1, &["pen", "ink"], None, ());
    w.park(2, &["ink"], None, ());
    check!("a pen wakes 1", w.wake("pen"), Some((1, ())));
    check!("so ink goes to 2", w.wake("ink"), Some((2, ())));
    check!("empty", w.len(), 0);
}

#[test]
fn deadlines() {
    let mut w = Waiting::new();
    w.park(1, &["a"], Some(30), 'x');
    w.park(2, &["a"], Some(10), 'y');
    w.park(3, &["a"], None, 'z');
    check!("the next one", w.next_deadline(), Some(10));
    check!("nothing at 9", w.expire(9), vec![]);
    check!("both at 30, earliest first", w.expire(30), vec![(2, 'y'), (1, 'x')]);
    check!("3 waits for ever", (w.next_deadline(), w.len()), (None, 1));
}

#[test]
fn cancel() {
    let mut w = Waiting::new();
    w.park(1, &["a", "b"], Some(5), 10);
    check!("cancel returns the ticket", w.cancel(1), Some(10));
    check!("once", w.cancel(1), None);
    check!("not woken", w.wake("a"), None);
    check!("not expired", w.expire(100), vec![]);
}

#[test]
fn one_wait_per_id() {
    let mut w = Waiting::new();
    w.park(1, &["a"], None, 1);
    check!("a second park is refused", w.park(1, &["b"], Some(0), 2), false);
    check!("nothing changed", (w.wake("b"), w.expire(10), w.len()), (None, vec![], 1));
}
''',
]

HIDDEN = [
    r'''
#[derive(Debug, PartialEq)]
struct NotCopy(String);

#[test]
fn tickets_are_moved_out_not_copied() {
    let mut w = Waiting::new();
    w.park(1, &["a"], None, NotCopy("mine".into()));
    check!("wake", w.wake("a"), Some((1, NotCopy("mine".into()))));
}

#[test]
fn an_item_named_twice_counts_once() {
    let mut w = Waiting::new();
    w.park(1, &["a", "a", "b", "a"], None, ());
    w.park(2, &["a"], None, ());
    check!("first a", w.wake("a"), Some((1, ())));
    check!("second a", w.wake("a"), Some((2, ())));
    check!("b has nobody", w.wake("b"), None);
}

#[test]
fn waiting_for_nothing_only_a_deadline_ends_it() {
    let mut w = Waiting::new();
    check!("park", w.park(1, &[], Some(4), ()), true);
    check!("contains", w.contains(1), true);
    check!("expire", w.expire(4), vec![(1, ())]);
}

#[test]
fn equal_deadlines_go_in_the_order_they_came() {
    let mut w = Waiting::new();
    for id in [9, 2, 5, 1] {
        w.park(id, &["a"], Some(100), id);
    }
    check!("order", w.expire(u64::MAX).into_iter().map(|(id, _)| id).collect::<Vec<_>>(), vec![9, 2, 5, 1]);
}

#[test]
fn an_id_that_comes_back_goes_to_the_end() {
    let mut w = Waiting::new();
    w.park(1, &["a"], None, ());
    w.park(2, &["a"], None, ());
    w.cancel(1);
    w.park(1, &["a"], None, ());
    check!("2 is ahead now", w.wake("a"), Some((2, ())));
}

#[test]
fn extreme_deadlines() {
    let mut w = Waiting::new();
    w.park(u64::MAX, &["a"], Some(u64::MAX), ());
    w.park(0, &["a"], Some(0), ());
    check!("at 0", w.expire(0), vec![(0, ())]);
    check!("just before the end", w.expire(u64::MAX - 1), vec![]);
    check!("at the end", w.expire(u64::MAX), vec![(u64::MAX, ())]);
}

#[test]
fn a_deadline_in_the_past_still_waits_for_expire() {
    let mut w = Waiting::new();
    w.expire(1000);
    w.park(1, &["a"], Some(5), ());
    check!("it can still be woken", w.wake("a"), Some((1, ())));
}

#[test]
fn random_use_matches_a_plain_list() {
    let mut rng = anneal_prelude::Rng::new(0x3108);
    let items = ["a", "b", "c", "d"];
    for run in 0..300 {
        let mut w: Waiting<usize> = Waiting::new();
        // in the order they started waiting
        let mut model: Vec<(u64, Vec<&str>, Option<u64>, usize)> = Vec::new();
        let mut history = Vec::new();
        let mut now = 0;
        for step in 0..80 {
            match rng.below(5) {
                0 | 1 => {
                    let id = rng.below(8) as u64;
                    let n = rng.below(4);
                    let wants: Vec<&str> = (0..n).map(|_| *rng.pick(&items)).collect();
                    let deadline = rng.bool().then(|| now + rng.below(20) as u64);
                    history.push(format!("park {id} {wants:?} {deadline:?}"));
                    let fresh = model.iter().all(|m| m.0 != id);
                    check!(format!("run {run}: {history:?}"), w.park(id, &wants, deadline, step), fresh);
                    if fresh {
                        model.push((id, wants, deadline, step));
                    }
                }
                2 => {
                    let item = *rng.pick(&items);
                    history.push(format!("wake {item}"));
                    let want = model.iter().position(|m| m.1.contains(&item)).map(|i| {
                        let m = model.remove(i);
                        (m.0, m.3)
                    });
                    check!(format!("run {run}: {history:?}"), w.wake(item), want);
                }
                3 => {
                    let id = rng.below(8) as u64;
                    history.push(format!("cancel {id}"));
                    let want = model.iter().position(|m| m.0 == id).map(|i| model.remove(i).3);
                    check!(format!("run {run}: {history:?}"), w.cancel(id), want);
                }
                _ => {
                    now += rng.below(10) as u64;
                    history.push(format!("expire {now}"));
                    let mut due: Vec<(u64, usize, u64, usize)> = model
                        .iter()
                        .enumerate()
                        .filter_map(|(i, m)| m.2.filter(|&d| d <= now).map(|d| (d, i, m.0, m.3)))
                        .collect();
                    due.sort();
                    model.retain(|m| !m.2.is_some_and(|d| d <= now));
                    let want: Vec<(u64, usize)> = due.into_iter().map(|d| (d.2, d.3)).collect();
                    check!(format!("run {run}: {history:?}"), w.expire(now), want);
                }
            }
            let ctx = || format!("run {run}: after {history:?}");
            check!(ctx(), w.len(), model.len());
            check!(ctx(), w.next_deadline(), model.iter().filter_map(|m| m.2).min());
            let probe = rng.below(8) as u64;
            check!(ctx(), w.contains(probe), model.iter().any(|m| m.0 == probe));
        }
    }
}

#[test]
fn many_waiters_stay_fast() {
    // walking a queue to remove someone, or looking at everyone on each call, cannot finish in time
    let mut w = Waiting::new();
    let n = 200_000u64;
    for id in 0..n {
        w.park(id, &["hot", "also"], Some(1_000_000 + id), id);
        if id % 2 == 0 {
            check!("nothing is due", w.expire(id).len(), 0);
            check!("the next deadline", w.next_deadline(), Some(1_000_000));
        }
    }
    // cancel from the back of the queue
    for id in (n / 2..n).rev() {
        check!("cancel", w.cancel(id), Some(id));
    }
    for id in 0..n / 4 {
        check!("wake", w.wake("also"), Some((id, id)));
    }
    for id in n / 4..n / 2 {
        check!("expire one at a time", w.expire(1_000_000 + id), vec![(id, id)]);
    }
    check!("empty", w.len(), 0);
}

#[test]
fn many_items_come_and_go() {
    let mut w = Waiting::new();
    for i in 0..100_000u64 {
        let item = format!("item{i}");
        w.park(i, &[&item], None, ());
        check!("woken at once", w.wake(&item), Some((i, ())));
    }
    check!("nothing is left", w.len(), 0);
}
''',
]

STATEMENT = r'''
A lending library lets members ask to borrow books that are all out. A member may name several books, and takes **whichever comes back first**. Some members only wait until a deadline.

Write `Waiting<T>`. Each waiting request has an `id`, the items it wants, an optional deadline, and a ticket of type `T` that the caller gets back.

- **`park(id, wants, deadline, ticket)`** starts waiting and returns `true`. If `id` is already waiting, it returns `false` and changes nothing. An item named twice counts once. `wants` may be empty.
- **`wake(item)`**: one such item has arrived. Of those who want it, the one who has **waited longest** gets it.

  It stops waiting for everything, and is returned with its ticket. `None` if nobody wants the item. One arrival wakes one waiter.
- **`cancel(id)`** stops `id` waiting and returns its ticket.
- **`expire(now)`** removes and returns everyone whose deadline is `now` or earlier: earliest deadline first, and for equal deadlines the longest waiting first. A deadline already past when parked is only acted on here.
- **`next_deadline()`** is the earliest deadline among those waiting.
- `contains(id)`, `len()`, `is_empty()`.

An `id` that parks again after leaving starts at the back of every queue.

**Speed.** There may be 200000 requests, many waiting for the same item. No call may walk through a queue or through everyone who waits. Each should take about `log n` steps for each item or waiter it handles.
'''

PROBLEM = dict(
    slug="hold-requests-until-something-arrives", title="Hold requests until something arrives", level="hard", stage="waiting",
    tags=["wait queues", "FIFO fairness", "multiple indexes", "deadlines"],
    statement=STATEMENT, starter=STARTER, solution=SOLUTION, visible=VISIBLE, hidden=HIDDEN,
    teaches=[
        "A request that cannot be answered yet is data: store what it waits for and what to give back.",
        "When one record sits in several indexes, remove it through one function that cleans them all.",
        "An ordered set keyed by (deadline, arrival) fires timers in order and can cancel any of them.",
    ],
    follow_up="1) Redis uses linked lists for these queues. What makes removal from the middle O(1) there, and how would you do that safely in Rust? 2) Replace the deadline set with a binary heap and lazy deletion. What can grow, and when do you clean it up? 3) What makes first-come-first-served fair here, and when could someone still starve?",
    hints=[
        ("approach", "Keep one record per waiter, and index it three ways: by id, by each wanted item in arrival order, and by deadline. Give every park a rising number, so that arrival order is something you can sort by."),
        ("rust", "`BTreeMap<u64, u64>` from arrival number to id is a queue that can remove from the middle. `BTreeSet<(deadline, arrival, id)>` gives the earliest with `first()`. Make `wake` and `expire` call `cancel`."),
        ("edge", "An item named twice must not leave a second entry behind. Drop an item's queue once it empties. A `cancel` must also remove the deadline."),
    ],
    notes=("Every waiter sits in a map by id, in one ordered queue per wanted item, and in an ordered set of deadlines. All three ways of leaving go through `cancel`, so no index can keep a stale entry.",
           "O(w log n) per waiter with w wanted items", "O(total wanted items)"),
    wrong={
        "newest_first": broken(SOLUTION, ("        let id = *self.queues.get(item)?.values().next()?;", "        let id = *self.queues.get(item)?.values().next_back()?;")),
        "other_queues_keep_the_woken": broken(SOLUTION, (
            "    pub fn wake(&mut self, item: &str) -> Option<(u64, T)> {\n        let id = *self.queues.get(item)?.values().next()?;\n        self.cancel(id).map(|ticket| (id, ticket))\n    }",
            "    pub fn wake(&mut self, item: &str) -> Option<(u64, T)> {\n        let queue = self.queues.get_mut(item)?;\n        let (_, id) = queue.pop_first()?;\n        let waiter = self.waiters.remove(&id)?;\n        if let Some(at) = waiter.deadline {\n            self.deadlines.remove(&(at, waiter.order, id));\n        }\n        Some((id, waiter.ticket))\n    }")),
        "ties_by_id": broken(SOLUTION, (
            "            self.deadlines.insert((at, order, id));", "            self.deadlines.insert((at, id, order));"),
            ("            self.deadlines.remove(&(at, waiter.order, id));", "            self.deadlines.remove(&(at, id, waiter.order));"),
            ("        while let Some(&(at, _, id)) = self.deadlines.first() {", "        while let Some(&(at, id, _)) = self.deadlines.first() {")),
        "queues_are_vectors": r'''
            use std::collections::{HashMap, VecDeque};

            pub struct Waiting<T> {
                waiters: HashMap<u64, (u64, Vec<String>, Option<u64>, T)>,
                queues: HashMap<String, VecDeque<u64>>,
                parked: u64,
            }

            impl<T> Waiting<T> {
                pub fn new() -> Waiting<T> {
                    Waiting { waiters: HashMap::new(), queues: HashMap::new(), parked: 0 }
                }
                pub fn park(&mut self, id: u64, wants: &[&str], deadline: Option<u64>, ticket: T) -> bool {
                    if self.waiters.contains_key(&id) {
                        return false;
                    }
                    let mut unique: Vec<String> = wants.iter().map(|w| w.to_string()).collect();
                    unique.sort();
                    unique.dedup();
                    for w in &unique {
                        self.queues.entry(w.clone()).or_default().push_back(id);
                    }
                    self.parked += 1;
                    self.waiters.insert(id, (self.parked, unique, deadline, ticket));
                    true
                }
                pub fn cancel(&mut self, id: u64) -> Option<T> {
                    let (_, wants, _, ticket) = self.waiters.remove(&id)?;
                    for w in &wants {
                        if let Some(q) = self.queues.get_mut(w) {
                            q.retain(|&x| x != id);
                        }
                    }
                    Some(ticket)
                }
                pub fn wake(&mut self, item: &str) -> Option<(u64, T)> {
                    let id = *self.queues.get(item)?.front()?;
                    self.cancel(id).map(|t| (id, t))
                }
                pub fn expire(&mut self, now: u64) -> Vec<(u64, T)> {
                    let mut due: Vec<(u64, u64, u64)> = self
                        .waiters
                        .iter()
                        .filter_map(|(&id, w)| w.2.filter(|&d| d <= now).map(|d| (d, w.0, id)))
                        .collect();
                    due.sort();
                    due.into_iter().filter_map(|(_, _, id)| self.cancel(id).map(|t| (id, t))).collect()
                }
                pub fn next_deadline(&self) -> Option<u64> {
                    self.waiters.values().filter_map(|w| w.2).min()
                }
                pub fn contains(&self, id: u64) -> bool {
                    self.waiters.contains_key(&id)
                }
                pub fn len(&self) -> usize {
                    self.waiters.len()
                }
                pub fn is_empty(&self) -> bool {
                    self.waiters.is_empty()
                }
            }

            impl<T> Default for Waiting<T> {
                fn default() -> Self {
                    Waiting::new()
                }
            }
        ''',
        "second_park_replaces_the_first": broken(SOLUTION, (
            "        if self.waiters.contains_key(&id) {\n            return false;\n        }\n", "        self.cancel(id);\n")),
    },
)
