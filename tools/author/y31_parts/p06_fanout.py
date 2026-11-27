from y17_parts.common import broken

SOLUTION = r'''use std::collections::{BTreeMap, BTreeSet, HashMap};
use tokio::sync::mpsc::{self, error::TrySendError, Receiver, Sender};

pub type MemberId = u64;

#[derive(Debug, PartialEq, Eq)]
pub struct Delivery {
    /// How many members now have the message in their queue.
    pub delivered: usize,
    /// Members removed by this call because their queue was full, in id order.
    pub evicted: Vec<MemberId>,
}

struct Member<M> {
    queue: Sender<M>,
    topics: BTreeSet<String>,
}

pub struct Hub<M> {
    last_id: MemberId,
    members: HashMap<MemberId, Member<M>>,
    topics: HashMap<String, BTreeMap<MemberId, ()>>,
}

impl<M: Clone> Hub<M> {
    pub fn new() -> Hub<M> {
        Hub {
            last_id: 0,
            members: HashMap::new(),
            topics: HashMap::new(),
        }
    }

    pub fn join(&mut self, capacity: usize) -> (MemberId, Receiver<M>) {
        let (queue, receiver) = mpsc::channel(capacity);
        self.last_id += 1;
        let member = Member {
            queue,
            topics: BTreeSet::new(),
        };
        self.members.insert(self.last_id, member);
        (self.last_id, receiver)
    }

    pub fn subscribe(&mut self, id: MemberId, topic: &str) -> Option<usize> {
        let member = self.members.get_mut(&id)?;
        member.topics.insert(topic.to_string());
        let count = member.topics.len();
        self.topics
            .entry(topic.to_string())
            .or_default()
            .insert(id, ());
        Some(count)
    }

    pub fn unsubscribe(&mut self, id: MemberId, topic: &str) -> Option<usize> {
        let member = self.members.get_mut(&id)?;
        member.topics.remove(topic);
        let count = member.topics.len();
        self.forget(id, topic);
        Some(count)
    }

    /// Takes `id` off one topic's list, and drops the list when it empties.
    fn forget(&mut self, id: MemberId, topic: &str) {
        if let Some(list) = self.topics.get_mut(topic) {
            list.remove(&id);
            if list.is_empty() {
                self.topics.remove(topic);
            }
        }
    }

    pub fn leave(&mut self, id: MemberId) -> bool {
        let Some(member) = self.members.remove(&id) else {
            return false;
        };
        for topic in &member.topics {
            self.forget(id, topic);
        }
        true
    }

    pub fn publish(&mut self, topic: &str, message: M) -> Delivery {
        let mut delivered = 0;
        let (mut evicted, mut closed) = (Vec::new(), Vec::new());
        for &id in self.topics.get(topic).into_iter().flat_map(|l| l.keys()) {
            let Some(member) = self.members.get(&id) else {
                continue;
            };
            match member.queue.try_send(message.clone()) {
                Ok(()) => delivered += 1,
                Err(TrySendError::Full(_)) => evicted.push(id),
                Err(TrySendError::Closed(_)) => closed.push(id),
            }
        }
        for &id in evicted.iter().chain(&closed) {
            self.leave(id);
        }
        Delivery { delivered, evicted }
    }

    pub fn members(&self) -> usize {
        self.members.len()
    }

    pub fn topics(&self) -> usize {
        self.topics.len()
    }
}

impl<M: Clone> Default for Hub<M> {
    fn default() -> Self {
        Hub::new()
    }
}
'''

STARTER = r'''use std::marker::PhantomData;
use tokio::sync::mpsc::Receiver;

pub type MemberId = u64;

#[derive(Debug, PartialEq, Eq)]
pub struct Delivery {
    /// How many members now have the message in their queue.
    pub delivered: usize,
    /// Members removed by this call because their queue was full, in id order.
    pub evicted: Vec<MemberId>,
}

pub struct Hub<M> {
    // replace with your own fields
    messages: PhantomData<M>,
}

impl<M: Clone> Hub<M> {
    pub fn new() -> Hub<M> {
        Hub { messages: PhantomData }
    }

    /// A new member whose queue holds at most `capacity` messages (at least 1).
    pub fn join(&mut self, capacity: usize) -> (MemberId, Receiver<M>) {
        todo!()
    }

    /// The number of topics the member is on afterwards.
    pub fn subscribe(&mut self, id: MemberId, topic: &str) -> Option<usize> {
        todo!()
    }

    pub fn unsubscribe(&mut self, id: MemberId, topic: &str) -> Option<usize> {
        todo!()
    }

    /// Removes the member. `false` if there is none.
    pub fn leave(&mut self, id: MemberId) -> bool {
        todo!()
    }

    /// Sends `message` to everyone on `topic`, without ever waiting.
    pub fn publish(&mut self, topic: &str, message: M) -> Delivery {
        todo!()
    }

    /// How many members the hub has.
    pub fn members(&self) -> usize {
        todo!()
    }

    /// How many topics have at least one member.
    pub fn topics(&self) -> usize {
        todo!()
    }
}

impl<M: Clone> Default for Hub<M> {
    fn default() -> Self {
        Hub::new()
    }
}
'''

HELPERS = r'''
use tokio::sync::mpsc::error::TryRecvError;
use tokio::sync::mpsc::Receiver;

/// What is waiting in the queue now, and whether the hub has dropped its end.
#[allow(dead_code)]
fn drain<M>(queue: &mut Receiver<M>) -> (Vec<M>, bool) {
    let mut out = Vec::new();
    loop {
        match queue.try_recv() {
            Ok(m) => out.push(m),
            Err(TryRecvError::Empty) => return (out, false),
            Err(TryRecvError::Disconnected) => return (out, true),
        }
    }
}
#[allow(dead_code)]
fn report(delivered: usize, evicted: &[MemberId]) -> Delivery {
    Delivery { delivered, evicted: evicted.to_vec() }
}
'''

VISIBLE = [
    HELPERS,
    r'''
#[test]
fn everyone_on_the_topic_gets_it() {
    let mut hub = Hub::new();
    let (a, mut qa) = hub.join(8);
    let (b, mut qb) = hub.join(8);
    let (_, mut qc) = hub.join(8);
    hub.subscribe(a, "news");
    hub.subscribe(b, "news");
    check!("two", hub.publish("news", "hi"), report(2, &[]));
    check!("a", drain(&mut qa), (vec!["hi"], false));
    check!("b", drain(&mut qb), (vec!["hi"], false));
    check!("c", drain(&mut qc), (vec![], false));
}

#[test]
fn a_reader_that_falls_behind_is_removed() {
    let mut hub = Hub::new();
    let (slow, mut q) = hub.join(2);
    hub.subscribe(slow, "t");
    hub.publish("t", 1);
    hub.publish("t", 2);
    check!("the third does not fit", hub.publish("t", 3), report(0, &[slow]));
    check!("what was queued can still be read, then the end", drain(&mut q), (vec![1, 2], true));
    check!("gone", (hub.members(), hub.publish("t", 4)), (0, report(0, &[])));
}

#[test]
fn subscribing_counts_topics() {
    let mut hub = Hub::<u8>::new();
    let (a, _q) = hub.join(1);
    check!("one", hub.subscribe(a, "x"), Some(1));
    check!("two", hub.subscribe(a, "y"), Some(2));
    check!("again", hub.subscribe(a, "x"), Some(2));
    check!("unsubscribe", hub.unsubscribe(a, "x"), Some(1));
    check!("a topic it is not on", hub.unsubscribe(a, "z"), Some(1));
    check!("a stranger", hub.subscribe(99, "x"), None);
}

#[test]
fn leaving() {
    let mut hub = Hub::new();
    let (a, mut q) = hub.join(4);
    hub.subscribe(a, "x");
    check!("leave", hub.leave(a), true);
    check!("twice", hub.leave(a), false);
    check!("no topics left", (hub.topics(), hub.members()), (0, 0));
    check!("publish reaches nobody", hub.publish("x", 'm'), report(0, &[]));
    check!("the queue has ended", drain(&mut q), (vec![], true));
}

#[test]
fn a_reader_that_went_away_is_not_counted() {
    let mut hub = Hub::new();
    let (a, q) = hub.join(4);
    hub.subscribe(a, "x");
    drop(q);
    check!("publish", hub.publish("x", 1), report(0, &[]));
    check!("and it is forgotten", (hub.members(), hub.topics()), (0, 0));
}
''',
]

HIDDEN = [
    HELPERS,
    r'''
#[test]
fn ids_are_never_reused() {
    let mut hub = Hub::<u8>::new();
    let (a, _qa) = hub.join(1);
    hub.leave(a);
    let (b, _qb) = hub.join(1);
    check!("new id", a != b, true);
}

#[test]
fn eviction_takes_the_member_off_every_topic() {
    let mut hub = Hub::new();
    let (a, mut q) = hub.join(1);
    hub.subscribe(a, "x");
    hub.subscribe(a, "y");
    let (b, mut qb) = hub.join(10);
    hub.subscribe(b, "y");
    hub.publish("x", 1);
    check!("full", hub.publish("x", 2), report(0, &[a]));
    check!("y still has b only", hub.publish("y", 3), report(1, &[]));
    check!("topics", hub.topics(), 1);
    check!("a is unknown now", hub.subscribe(a, "x"), None);
    check!("a read the first", drain(&mut q), (vec![1], true));
    check!("b", drain(&mut qb), (vec![3], false));
}

#[test]
fn one_slow_reader_does_not_hold_back_the_others() {
    let mut hub = Hub::new();
    let mut queues: Vec<(MemberId, Receiver<u32>)> = (0..5).map(|_| hub.join(3)).collect();
    for (id, _) in &queues {
        hub.subscribe(*id, "t");
    }
    for i in 0..10 {
        let result = hub.publish("t", i);
        // everyone but the second reads at once
        for (n, (_, q)) in queues.iter_mut().enumerate() {
            if n != 1 {
                check!(format!("reader {n}, message {i}"), q.try_recv().ok(), Some(i));
            }
        }
        match i {
            0..=2 => check!(format!("message {i}"), result, report(5, &[])),
            3 => check!("the fourth", result, report(4, &[queues[1].0])),
            _ => check!(format!("message {i}"), result, report(4, &[])),
        }
    }
}

#[test]
fn several_evictions_come_in_id_order() {
    let mut hub = Hub::new();
    let ids: Vec<(MemberId, Receiver<()>)> = (0..20).map(|_| hub.join(1)).collect();
    for (id, _) in ids.iter().rev() {
        hub.subscribe(*id, "t");
    }
    hub.publish("t", ());
    let mut want: Vec<MemberId> = ids.iter().map(|(id, _)| *id).collect();
    want.sort();
    check!("all full", hub.publish("t", ()), report(0, &want));
}

#[test]
fn a_full_queue_that_was_read_makes_room_again() {
    let mut hub = Hub::new();
    let (a, mut q) = hub.join(1);
    hub.subscribe(a, "t");
    for i in 0..100 {
        check!(format!("{i}"), hub.publish("t", i), report(1, &[]));
        check!(format!("{i}"), q.try_recv().ok(), Some(i));
    }
}

#[test]
fn messages_are_not_copied_to_those_who_do_not_listen() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;
    #[derive(Debug)]
    struct Counted(Arc<AtomicUsize>);
    impl Clone for Counted {
        fn clone(&self) -> Counted {
            self.0.fetch_add(1, Ordering::SeqCst);
            Counted(self.0.clone())
        }
    }
    let copies = Arc::new(AtomicUsize::new(0));
    let mut hub = Hub::new();
    let queues: Vec<_> = (0..10).map(|_| hub.join(4)).collect();
    for (id, _) in queues.iter().take(3) {
        hub.subscribe(*id, "t");
    }
    hub.publish("t", Counted(copies.clone()));
    check!(format!("{} clones for 3 listeners", copies.load(Ordering::SeqCst)), copies.load(Ordering::SeqCst) <= 3, true);
}

#[test]
fn random_use_matches_a_model() {
    use std::collections::{BTreeMap, BTreeSet, VecDeque};
    struct Model {
        capacity: usize,
        topics: BTreeSet<&'static str>,
        queued: VecDeque<usize>,
    }
    let mut rng = anneal_prelude::Rng::new(0x3109);
    let topics = ["a", "b", "c"];
    for run in 0..300 {
        let mut hub = Hub::new();
        let mut model: BTreeMap<MemberId, Model> = BTreeMap::new();
        let mut queues: BTreeMap<MemberId, Receiver<usize>> = BTreeMap::new();
        let mut history = Vec::new();
        for step in 0..60 {
            let ids: Vec<MemberId> = queues.keys().copied().collect();
            let some = (!ids.is_empty()).then(|| *rng.pick(&ids));
            match (rng.below(7), some) {
                (0, _) | (_, None) => {
                    let capacity = rng.below(3) + 1;
                    let (id, q) = hub.join(capacity);
                    history.push(format!("join {capacity} -> {id}"));
                    queues.insert(id, q);
                    model.insert(id, Model { capacity, topics: BTreeSet::new(), queued: VecDeque::new() });
                }
                (1 | 2, Some(id)) => {
                    let topic = *rng.pick(&topics);
                    let add = rng.below(3) > 0;
                    history.push(format!("{} {id} {topic}", if add { "subscribe" } else { "unsubscribe" }));
                    let want = model.get_mut(&id).map(|m| {
                        if add { m.topics.insert(topic); } else { m.topics.remove(topic); }
                        m.topics.len()
                    });
                    let got = if add { hub.subscribe(id, topic) } else { hub.unsubscribe(id, topic) };
                    check!(format!("run {run}: {history:?}"), got, want);
                }
                (3, Some(id)) => {
                    history.push(format!("leave {id}"));
                    check!(format!("run {run}: {history:?}"), hub.leave(id), model.remove(&id).is_some());
                }
                (4, Some(id)) => {
                    history.push(format!("read {id}"));
                    let got = queues.get_mut(&id).and_then(|q| q.try_recv().ok());
                    if let Some(m) = model.get_mut(&id) {
                        check!(format!("run {run}: {history:?}"), got, m.queued.pop_front());
                    }
                }
                (_, Some(_)) => {
                    let topic = *rng.pick(&topics);
                    history.push(format!("publish {topic} {step}"));
                    let (mut delivered, mut evicted) = (0, Vec::new());
                    for (&id, m) in model.iter_mut().filter(|(_, m)| m.topics.contains(topic)) {
                        if m.queued.len() < m.capacity {
                            m.queued.push_back(step);
                            delivered += 1;
                        } else {
                            evicted.push(id);
                        }
                    }
                    for id in &evicted {
                        model.remove(id);
                    }
                    check!(format!("run {run}: {history:?}"), hub.publish(topic, step), report(delivered, &evicted));
                }
            }
            let used: BTreeSet<&str> = model.values().flat_map(|m| m.topics.iter().copied()).collect();
            check!(format!("run {run}: members and topics after {history:?}"), (hub.members(), hub.topics()), (model.len(), used.len()));
        }
    }
}

#[test]
fn a_crowded_hub_stays_fast() {
    // publishing must not look at members of other topics
    let mut hub = Hub::new();
    let n = 100_000;
    let mut members: Vec<(MemberId, Receiver<usize>)> = Vec::with_capacity(n);
    for i in 0..n {
        let (id, q) = hub.join(1);
        hub.subscribe(id, &format!("own{i}"));
        members.push((id, q));
    }
    for (i, (_, q)) in members.iter_mut().enumerate() {
        check!("publish", hub.publish(&format!("own{i}"), i), report(1, &[]));
        check!("read", q.try_recv().ok(), Some(i));
    }
    for (id, _) in &members {
        hub.leave(*id);
    }
    check!("empty", (hub.members(), hub.topics()), (0, 0));
}
''',
]

STATEMENT = r'''
A trading floor sends price updates to many screens. Each screen listens to some topics. One frozen screen must not delay the others, and its unread updates must not pile up until the server runs out of memory.

Write `Hub<M>`. Each member has its own queue, a Tokio `mpsc` channel that holds at most `capacity` messages. The hub keeps the sending end.

- **`join(capacity)`** adds a member with a new id (never reused) and returns its receiving end. `capacity` is at least 1.
- **`subscribe(id, topic)`** and **`unsubscribe(id, topic)`** change what a member listens to. Both return the number of topics it is on afterwards, or `None` for an unknown id. Repeating either changes nothing.
- **`leave(id)`** removes the member from everything and drops its sending end.
- **`publish(topic, message)`** puts a copy in the queue of every member on the topic. **It never waits.**

`publish` returns a `Delivery`.

- `delivered` counts the queues that took the message.
- A member whose queue is **full** is **evicted**, as if it left. `evicted` lists these in id order.
- An evicted member's receiver can still read what was queued. Then it sees the end.
- A member whose receiver was dropped is removed too. It is neither counted nor listed.

- **`members()`** counts members. **`topics()`** counts topics with at least one member.

Each member sees messages in the order they were published. There may be 100000 members. `publish` must take time only for the members of its topic, plus at most a logarithm each. It must not clone a message more than once for each of them.

The tests read queues with `try_recv`; no async runtime is needed.
'''

PROBLEM = dict(
    slug="tell-everyone-without-waiting", title="Tell everyone without waiting for anyone", level="medium", stage="waiting",
    tags=["fan-out", "pub/sub", "backpressure", "slow consumer", "try_send"],
    statement=STATEMENT, starter=STARTER, solution=SOLUTION, visible=VISIBLE, hidden=HIDDEN,
    crates=["tokio"],
    teaches=[
        "A bounded queue per listener bounds the memory a slow one can cost.",
        "A publisher that never waits needs a policy for a full queue: here, drop the listener.",
        "Dropping the sender tells the receiver the end has come, after what is queued.",
    ],
    follow_up="1) Instead of evicting, you could drop the oldest or newest message. Which users would want which, and which does `tokio::sync::broadcast` choose? 2) Cloning a large message per listener is wasteful. What would you send instead? 3) How would a subscriber learn that it was evicted, and not merely that the hub closed?",
    hints=[
        ("approach", "Keep two maps that mirror each other: for each member its topics, and for each topic its members. Publishing walks one topic's list, and sorts out afterwards who must go."),
        ("rust", "`Sender::try_send` never waits and tells `Full` from `Closed`. You cannot remove members while you iterate over the topic's list: collect the ids first. A `BTreeMap` or `BTreeSet` keeps them in id order."),
        ("edge", "Remove a topic's entry when its last member goes, whether it left, unsubscribed or was evicted."),
    ],
    notes=("The hub keeps topics and members indexed both ways. `try_send` makes publishing non-blocking, and its two errors separate a reader that is behind from one that is gone. Removing the member drops the sender, which ends the reader's queue after what it holds.",
           "O(listeners of the topic) per publish", "O(subscriptions + queued messages)"),
    wrong={
        "full_message_dropped_silently": broken(SOLUTION, ("                Err(TrySendError::Full(_)) => evicted.push(id),", "                Err(TrySendError::Full(_)) => {}")),
        "eviction_only_from_this_topic": broken(SOLUTION, (
            "        for &id in evicted.iter().chain(&closed) {\n            self.leave(id);\n        }",
            "        for &id in evicted.iter().chain(&closed) {\n            self.forget(id, topic);\n        }")),
        "closed_counted_as_evicted": broken(SOLUTION, ("                Err(TrySendError::Closed(_)) => closed.push(id),", "                Err(TrySendError::Closed(_)) => evicted.push(id),")),
        "empty_topics_kept": broken(SOLUTION, ("            if list.is_empty() {\n                self.topics.remove(topic);\n            }\n", "")),
        "walks_every_member": broken(SOLUTION, (
            "        for &id in self.topics.get(topic).into_iter().flat_map(|l| l.keys()) {\n            let Some(member) = self.members.get(&id) else {\n                continue;\n            };",
            "        let mut ids: Vec<MemberId> = self.members.keys().copied().collect();\n        ids.sort();\n        for id in ids {\n            let Some(member) = self.members.get(&id).filter(|m| m.topics.contains(topic)) else {\n                continue;\n            };")),
    },
)
