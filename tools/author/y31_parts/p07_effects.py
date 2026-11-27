from y17_parts.common import broken

TYPES = r'''/// A source of random numbers. `below(n)` is in `0..n`.
pub trait Dice {
    fn below(&mut self, n: usize) -> usize;
}

#[derive(Debug, Clone, PartialEq)]
pub enum Seat {
    Free,
    /// Held for a while by someone who has not paid.
    Held { who: String, until: u64 },
    Sold { who: String },
}

#[derive(Debug, Clone, PartialEq)]
pub enum Command {
    /// Hold any free seat for `ms` milliseconds.
    Hold { who: String, ms: u64 },
    /// Buy the seat held.
    Buy { who: String },
    /// Give the seat back, held or sold.
    Release { who: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reply {
    /// The command was done, about this seat.
    Done(usize),
    Refused,
}

/// One change to the hall.
#[derive(Debug, Clone, PartialEq)]
pub enum Effect {
    Held { seat: usize, who: String, until: u64 },
    Sold { seat: usize },
    Freed { seat: usize },
}

#[derive(Debug, Clone, PartialEq)]
pub enum Entry {
    Begin,
    Change(Effect),
    End,
}

/// The log does not describe a history of this hall.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BadLog {
    /// The index of the entry that is wrong.
    pub at: usize,
}
'''

SOLUTION = r'''use std::collections::HashMap;

''' + TYPES + r'''
#[derive(Debug, Clone, PartialEq)]
pub struct Hall {
    seats: Vec<Seat>,
    /// Which seat each person has.
    owners: HashMap<String, usize>,
}

impl Hall {
    pub fn new(size: usize) -> Hall {
        Hall {
            seats: vec![Seat::Free; size],
            owners: HashMap::new(),
        }
    }

    pub fn seats(&self) -> &[Seat] {
        &self.seats
    }

    /// Applies one change if it fits the hall.
    fn apply(&mut self, effect: &Effect) -> bool {
        match effect {
            Effect::Held { seat, who, until } => {
                if self.seats.get(*seat) != Some(&Seat::Free) || self.owners.contains_key(who) {
                    return false;
                }
                self.seats[*seat] = Seat::Held {
                    who: who.clone(),
                    until: *until,
                };
                self.owners.insert(who.clone(), *seat);
            }
            Effect::Sold { seat } => {
                let Some(Seat::Held { who, .. }) = self.seats.get(*seat) else {
                    return false;
                };
                self.seats[*seat] = Seat::Sold { who: who.clone() };
            }
            Effect::Freed { seat } => {
                let who = match self.seats.get(*seat) {
                    Some(Seat::Held { who, .. } | Seat::Sold { who }) => who.clone(),
                    _ => return false,
                };
                self.seats[*seat] = Seat::Free;
                self.owners.remove(&who);
            }
        }
        true
    }

    fn change(&mut self, effect: Effect, effects: &mut Vec<Effect>) {
        let fits = self.apply(&effect);
        debug_assert!(fits, "the hall only makes changes that fit");
        effects.push(effect);
    }

    pub fn run(
        &mut self,
        commands: &[Command],
        now: u64,
        dice: &mut dyn Dice,
    ) -> (Vec<Reply>, Vec<Entry>) {
        let mut effects = Vec::new();
        for seat in 0..self.seats.len() {
            if matches!(self.seats[seat], Seat::Held { until, .. } if until <= now) {
                self.change(Effect::Freed { seat }, &mut effects);
            }
        }
        let replies = commands
            .iter()
            .map(|c| self.one(c, now, dice, &mut effects))
            .collect();
        let wrap = effects.len() > 1;
        let mut log = Vec::new();
        if wrap {
            log.push(Entry::Begin);
        }
        log.extend(effects.into_iter().map(Entry::Change));
        if wrap {
            log.push(Entry::End);
        }
        (replies, log)
    }

    fn one(
        &mut self,
        command: &Command,
        now: u64,
        dice: &mut dyn Dice,
        effects: &mut Vec<Effect>,
    ) -> Reply {
        match command {
            Command::Hold { who, ms } => {
                if self.owners.contains_key(who) {
                    return Reply::Refused;
                }
                let free: Vec<usize> = (0..self.seats.len())
                    .filter(|&s| self.seats[s] == Seat::Free)
                    .collect();
                if free.is_empty() {
                    return Reply::Refused;
                }
                let seat = free[dice.below(free.len())];
                let until = now.saturating_add(*ms);
                self.change(
                    Effect::Held {
                        seat,
                        who: who.clone(),
                        until,
                    },
                    effects,
                );
                Reply::Done(seat)
            }
            Command::Buy { who } => match self.owners.get(who).copied() {
                Some(seat) if matches!(self.seats[seat], Seat::Held { .. }) => {
                    self.change(Effect::Sold { seat }, effects);
                    Reply::Done(seat)
                }
                _ => Reply::Refused,
            },
            Command::Release { who } => match self.owners.get(who).copied() {
                Some(seat) => {
                    self.change(Effect::Freed { seat }, effects);
                    Reply::Done(seat)
                }
                None => Reply::Refused,
            },
        }
    }

    pub fn replay(size: usize, log: &[Entry]) -> Result<(Hall, usize), BadLog> {
        let mut hall = Hall::new(size);
        let mut used = 0;
        // the changes of the unit being read
        let mut open: Option<Vec<(usize, &Effect)>> = None;
        for (at, entry) in log.iter().enumerate() {
            let ready = match entry {
                Entry::Begin => {
                    if open.replace(Vec::new()).is_some() {
                        return Err(BadLog { at });
                    }
                    continue;
                }
                Entry::End => open.take().ok_or(BadLog { at })?,
                Entry::Change(effect) => match &mut open {
                    Some(held) => {
                        held.push((at, effect));
                        continue;
                    }
                    None => vec![(at, effect)],
                },
            };
            for (at, effect) in ready {
                if !hall.apply(effect) {
                    return Err(BadLog { at });
                }
            }
            used = at + 1;
        }
        Ok((hall, used))
    }
}
'''

STARTER = TYPES + r'''
#[derive(Debug, Clone, PartialEq)]
pub struct Hall {
    seats: Vec<Seat>,
    // add what you need
}

impl Hall {
    pub fn new(size: usize) -> Hall {
        todo!()
    }

    pub fn seats(&self) -> &[Seat] {
        &self.seats
    }

    /// Runs the commands as one unit, and returns their replies and what the log needs.
    pub fn run(
        &mut self,
        commands: &[Command],
        now: u64,
        dice: &mut dyn Dice,
    ) -> (Vec<Reply>, Vec<Entry>) {
        todo!()
    }

    /// The hall the log describes, and how many of its entries count.
    pub fn replay(size: usize, log: &[Entry]) -> Result<(Hall, usize), BadLog> {
        todo!()
    }
}
'''

HELPERS = r'''
use anneal_prelude::Rng;

#[allow(dead_code)]
struct Seeded(Rng);
impl Dice for Seeded {
    fn below(&mut self, n: usize) -> usize {
        self.0.below(n)
    }
}

/// Always answers `0`, or the last choice if asked for `High`.
#[allow(dead_code)]
struct First;
impl Dice for First {
    fn below(&mut self, _: usize) -> usize {
        0
    }
}
#[allow(dead_code)]
struct High;
impl Dice for High {
    fn below(&mut self, n: usize) -> usize {
        n - 1
    }
}

/// What a replay shows: the seats and the number of entries used.
#[allow(dead_code)]
fn seen((hall, used): (Hall, usize)) -> (Vec<Seat>, usize) {
    (hall.seats().to_vec(), used)
}

#[allow(dead_code)]
fn hold(who: &str, ms: u64) -> Command {
    Command::Hold { who: who.into(), ms }
}
#[allow(dead_code)]
fn buy(who: &str) -> Command {
    Command::Buy { who: who.into() }
}
#[allow(dead_code)]
fn release(who: &str) -> Command {
    Command::Release { who: who.into() }
}
#[allow(dead_code)]
fn held(seat: usize, who: &str, until: u64) -> Entry {
    Entry::Change(Effect::Held { seat, who: who.into(), until })
}
#[allow(dead_code)]
fn sold(seat: usize) -> Entry {
    Entry::Change(Effect::Sold { seat })
}
#[allow(dead_code)]
fn freed(seat: usize) -> Entry {
    Entry::Change(Effect::Freed { seat })
}
'''

VISIBLE = [
    HELPERS,
    r'''
#[test]
fn a_hold_is_logged_as_what_it_did() {
    let mut hall = Hall::new(3);
    check!("the dice choose among free seats", hall.run(&[hold("ann", 50)], 100, &mut High), (vec![Reply::Done(2)], vec![held(2, "ann", 150)]));
    check!("seat 2", hall.seats()[2].clone(), Seat::Held { who: "ann".into(), until: 150 });
}

#[test]
fn several_changes_form_one_unit() {
    let mut hall = Hall::new(3);
    let (replies, log) = hall.run(&[hold("ann", 50), buy("ann"), hold("ann", 5)], 0, &mut First);
    check!("replies", replies, vec![Reply::Done(0), Reply::Done(0), Reply::Refused]);
    check!("log", log, vec![Entry::Begin, held(0, "ann", 50), sold(0), Entry::End]);
}

#[test]
fn a_lapsed_hold_is_freed_first_and_logged() {
    let mut hall = Hall::new(1);
    hall.run(&[hold("ann", 10)], 0, &mut First);
    check!("bob at 10", hall.run(&[hold("bob", 10)], 10, &mut First), (vec![Reply::Done(0)], vec![Entry::Begin, freed(0), held(0, "bob", 20), Entry::End]));
}

#[test]
fn nothing_changed_nothing_logged() {
    let mut hall = Hall::new(1);
    check!("buy without a hold", hall.run(&[buy("x"), release("x")], 0, &mut First), (vec![Reply::Refused, Reply::Refused], vec![]));
}

#[test]
fn replay_rebuilds_the_hall() {
    let mut hall = Hall::new(4);
    let mut log = hall.run(&[hold("ann", 100), hold("bob", 5)], 0, &mut Seeded(Rng::new(1))).1;
    log.extend(hall.run(&[buy("ann")], 50, &mut First).1);
    check!("replay", Hall::replay(4, &log).map(seen), Ok(seen((hall, log.len()))));
}

#[test]
fn a_unit_without_its_end_does_not_count() {
    let log = [held(0, "a", 9), Entry::Begin, held(1, "b", 9), sold(1)];
    let (hall, used) = Hall::replay(2, &log).expect("a torn end is fine");
    check!("used", used, 1);
    check!("seat 1", hall.seats()[1].clone(), Seat::Free);
}
''',
]

HIDDEN = [
    HELPERS,
    r'''
#[test]
fn the_rules_of_each_command() {
    let mut hall = Hall::new(2);
    let r = |hall: &mut Hall, c: Command| hall.run(&[c], 0, &mut First).0[0];
    check!("ann holds 0", r(&mut hall, hold("ann", 100)), Reply::Done(0));
    check!("one seat each", r(&mut hall, hold("ann", 100)), Reply::Refused);
    check!("bob holds 1", r(&mut hall, hold("bob", 100)), Reply::Done(1));
    check!("full", r(&mut hall, hold("cat", 100)), Reply::Refused);
    check!("ann buys", r(&mut hall, buy("ann")), Reply::Done(0));
    check!("twice", r(&mut hall, buy("ann")), Reply::Refused);
    check!("a buyer still has a seat", r(&mut hall, hold("ann", 100)), Reply::Refused);
    check!("ann gives back a sold seat", r(&mut hall, release("ann")), Reply::Done(0));
    check!("and may hold again", r(&mut hall, hold("ann", 100)), Reply::Done(0));
}

#[test]
fn sold_seats_never_lapse() {
    let mut hall = Hall::new(1);
    hall.run(&[hold("ann", 10), buy("ann")], 0, &mut First);
    check!("much later", hall.run(&[], u64::MAX, &mut First), (vec![], vec![]));
}

#[test]
fn lapsed_holds_are_freed_in_seat_order_even_with_no_command() {
    let mut hall = Hall::new(3);
    hall.run(&[hold("a", 30), hold("b", 20), hold("c", 40)], 0, &mut First);
    check!("at 30", hall.run(&[], 30, &mut First), (vec![], vec![Entry::Begin, freed(0), freed(1), Entry::End]));
    check!("at 40, one change is not wrapped", hall.run(&[], 40, &mut First), (vec![], vec![freed(2)]));
}

#[test]
fn a_buy_of_a_lapsed_hold_is_refused_after_freeing_it() {
    let mut hall = Hall::new(1);
    hall.run(&[hold("a", 5)], 0, &mut First);
    check!("too late", hall.run(&[buy("a")], 5, &mut First), (vec![Reply::Refused], vec![freed(0)]));
}

#[test]
fn holds_made_in_this_run_do_not_lapse_until_the_next() {
    let mut hall = Hall::new(1);
    check!("a hold of 0 ms", hall.run(&[hold("a", 0), buy("a")], 7, &mut First).0, vec![Reply::Done(0), Reply::Done(0)]);
    let mut hall = Hall::new(1);
    hall.run(&[hold("a", 0)], 7, &mut First);
    check!("next run", hall.run(&[buy("a")], 7, &mut First), (vec![Reply::Refused], vec![freed(0)]));
}

#[test]
fn a_hold_that_never_ends() {
    let mut hall = Hall::new(1);
    let (_, log) = hall.run(&[hold("a", u64::MAX)], 5, &mut First);
    check!("no overflow", log, vec![held(0, "a", u64::MAX)]);
}

#[test]
fn the_dice_pick_among_the_free_seats_in_order() {
    /// Records each question and answers 1.
    struct Second(Vec<usize>);
    impl Dice for Second {
        fn below(&mut self, n: usize) -> usize {
            self.0.push(n);
            1.min(n - 1)
        }
    }
    let mut hall = Hall::new(4);
    let mut dice = Second(Vec::new());
    let (replies, _) = hall.run(&[hold("a", 9), hold("b", 9), hold("c", 9), hold("d", 9), hold("e", 9)], 0, &mut dice);
    check!("second free seat each time", replies, vec![Reply::Done(1), Reply::Done(2), Reply::Done(3), Reply::Done(0), Reply::Refused]);
    check!("one question per hold, over the free seats; none when full", dice.0, vec![4, 3, 2, 1]);
}

#[test]
fn damaged_logs() {
    let cases: Vec<(&str, Vec<Entry>, usize)> = vec![
        ("a seat that does not exist", vec![held(2, "a", 1)], 0),
        ("holding a taken seat", vec![held(0, "a", 1), held(0, "b", 1)], 1),
        ("two seats for one person", vec![held(0, "a", 1), held(1, "a", 1)], 1),
        ("selling a free seat", vec![sold(0)], 0),
        ("selling twice", vec![held(0, "a", 1), sold(0), sold(0)], 2),
        ("freeing a free seat", vec![freed(1)], 0),
        ("freeing far away", vec![freed(usize::MAX)], 0),
        ("an End alone", vec![held(0, "a", 1), Entry::End], 1),
        ("Begin inside a unit", vec![Entry::Begin, held(0, "a", 1), Entry::Begin], 2),
        ("a bad change in a whole unit", vec![Entry::Begin, held(0, "a", 1), sold(1), Entry::End], 2),
    ];
    for (what, log, at) in cases {
        check!(what, Hall::replay(2, &log).map(|_| ()), Err(BadLog { at }));
    }
}

#[test]
fn what_an_unfinished_unit_holds_is_not_judged() {
    let log = [held(0, "a", 1), Entry::Begin, sold(1), freed(99)];
    check!("ignored", Hall::replay(2, &log).map(|(_, used)| used), Ok(1));
}

#[test]
fn an_empty_unit_counts() {
    let log = [Entry::Begin, Entry::End];
    check!("both", Hall::replay(1, &log).map(seen), Ok((vec![Seat::Free], 2)));
}

#[allow(dead_code)]
fn random_command(rng: &mut Rng) -> Command {
    let who = format!("p{}", rng.below(6));
    match rng.below(4) {
        0 | 1 => Command::Hold { who, ms: rng.below(20) as u64 },
        2 => Command::Buy { who },
        _ => Command::Release { who },
    }
}

#[test]
fn every_cut_of_a_random_history_replays_to_the_last_whole_unit() {
    let mut rng = Rng::new(0x310a);
    for run in 0..300 {
        let size = rng.below(5);
        let mut hall = Hall::new(size);
        let mut log: Vec<Entry> = Vec::new();
        // the hall after each unit, with the log length then
        let mut states = vec![(0, hall.seats().to_vec())];
        let mut now = 0;
        let mut history = Vec::new();
        for _ in 0..30 {
            now += rng.below(10) as u64;
            let n = rng.below(4);
            let commands: Vec<Command> = (0..n).map(|_| random_command(&mut rng)).collect();
            history.push(format!("{now}: {commands:?}"));
            let (replies, entries) = hall.run(&commands, now, &mut Seeded(Rng::new(rng.next_u64())));
            check!(format!("run {run}: a reply per command, {history:?}"), replies.len(), commands.len());
            if !entries.is_empty() {
                log.extend(entries);
                states.push((log.len(), hall.seats().to_vec()));
            }
        }
        for cut in 0..=log.len() {
            let (end, want) = states.iter().rev().find(|(end, _)| *end <= cut).expect("the start");
            check!(
                format!("run {run}, size {size}: the first {cut} entries of {log:?}"),
                Hall::replay(size, &log[..cut]).map(seen),
                Ok((want.clone(), *end))
            );
        }
    }
}

#[test]
fn a_replayed_hall_carries_on() {
    let mut hall = Hall::new(2);
    let (_, log) = hall.run(&[hold("a", 10)], 0, &mut First);
    let (mut again, _) = Hall::replay(2, &log).expect("replay");
    check!("same behaviour", again.run(&[hold("a", 1), buy("a")], 3, &mut First), hall.run(&[hold("a", 1), buy("a")], 3, &mut First));
}
''',
]

STATEMENT = r'''
A theatre sells seats. A customer first **holds** a seat for a short while, then buys it. The seat is chosen at random, and a hold that is not bought in time lapses. The system must come back exactly as it was after a restart.

The log cannot be a list of the requests: replaying them would need the old dice and the old clock. So it records **what changed**.

**`run(commands, now, dice)`** runs the commands as one unit, and returns one reply per command, and the log entries for this call.

1. First, every hold with `until <= now` lapses: its seat is freed, in seat order. This happens even with no commands, and only here, at the start.
2. Then each command runs in order. Each person has at most one seat, held or sold.

    - `Hold`: refused if `who` has a seat, or none is free. Otherwise ask `dice.below(f)` once, with `f` free seats. Take that free seat, counting from 0 in seat order.
      It is held until `now + ms`, at most `u64::MAX`.
    - `Buy`: refused unless `who` **holds** a seat; it becomes sold.
    - `Release`: refused unless `who` has a seat; it becomes free.

A reply is `Done(seat)` or `Refused`. Dice are thrown only for a hold that succeeds.

**The log of a call.** Each change is one `Change(effect)`, in the order made. A freed lapse is a change too.

With **two or more** changes, put `Begin` before them and `End` after them. With none, return no entries.

**`replay(size, log)`** returns the hall the log describes, and how many entries it used. It needs no dice and no clock.

- A single change outside `Begin`..`End` is a unit by itself. A unit is applied **whole or not at all**. A complete unit counts even if it is empty.
- A log cut short may end inside a unit. That unit is ignored, its changes unchecked, and `used` stops before its `Begin`.
- `Err(BadLog { at })` is for a wrong entry, and `at` is its index. These are wrong:
    - `Begin` inside a unit, or `End` outside one;
    - in a **complete** unit, a change that does not fit. It may name a seat out of range, or hold a taken seat.
    - So are: holding for someone who has a seat, selling a seat that is not held, and freeing a free one.

For any history, replaying the first `n` entries gives the hall as it was after the last whole call within them.
'''

PROBLEM = dict(
    slug="write-down-what-happened", title="Come back the same after a restart", level="medium", stage="disk",
    tags=["effect log", "event sourcing", "deterministic replay", "atomic units"],
    statement=STATEMENT, starter=STARTER, solution=SOLUTION, visible=VISIBLE, hidden=HIDDEN,
    teaches=[
        "Log the effects, so replay needs no clock, no dice and no old code paths.",
        "Whatever the system does on its own, such as a lapse, is a change and must be logged.",
        "Boundary markers make a group of changes replay all or nothing.",
    ],
    follow_up="1) Logging requests with the dice's answers and the time is the other design. Compare them when the code changes between writing and replaying. 2) Add snapshots: what must a snapshot record about the log? 3) Event sourcing keeps every change for ever. What does that buy, and what does it cost?",
    hints=[
        ("approach", "Make every change to the hall go through one function that applies an effect. `run` decides which effects to make; `replay` only applies the ones it reads."),
        ("rust", "If `apply` returns `false` when an effect does not fit, replay can use it to find bad logs, and `run` can use the very same code. Collect a unit's changes first, apply them when its `End` arrives."),
        ("edge", "Count the changes before you decide whether to wrap them. Lapses count. `used` moves only when a unit is complete."),
    ],
    notes=("All changes go through one `apply`, shared by the live system and replay, so the two cannot drift apart. The live system makes the random and timed decisions and records only their results; replay holds back a unit's changes until its end marker.",
           "O(seats) per run, O(log) per replay", "O(seats)"),
    wrong={
        "lapses_not_logged": broken(SOLUTION, (
            "                self.change(Effect::Freed { seat }, &mut effects);\n            }\n        }\n",
            "                self.change(Effect::Freed { seat }, &mut effects);\n            }\n        }\n        effects.clear();\n")),
        "every_change_wrapped": broken(SOLUTION, ("        let wrap = effects.len() > 1;", "        let wrap = !effects.is_empty();")),
        "changes_applied_as_read": broken(SOLUTION, (
            "                    Some(held) => {\n                        held.push((at, effect));\n                        continue;\n                    }",
            "                    Some(_) => {\n                        if !hall.apply(effect) {\n                            return Err(BadLog { at });\n                        }\n                        continue;\n                    }")),
        "torn_unit_counted_as_used": broken(SOLUTION, ("        Ok((hall, used))", "        let _ = used;\n        Ok((hall, log.len()))")),
        "two_seats_per_person": broken(SOLUTION, (
            "                if self.seats.get(*seat) != Some(&Seat::Free) || self.owners.contains_key(who) {", "                if self.seats.get(*seat) != Some(&Seat::Free) {")),
        "lapse_checked_before_every_command": broken(SOLUTION, (
            "        let replies = commands\n            .iter()\n            .map(|c| self.one(c, now, dice, &mut effects))\n            .collect();",
            "        let replies = commands\n            .iter()\n            .map(|c| {\n                for seat in 0..self.seats.len() {\n                    if matches!(self.seats[seat], Seat::Held { until, .. } if until <= now) {\n                        self.change(Effect::Freed { seat }, &mut effects);\n                    }\n                }\n                self.one(c, now, dice, &mut effects)\n            })\n            .collect();")),
    },
)
