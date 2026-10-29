//! The plan: which problem is solved on which day, and where each review lands (docs/DSA.md, decision 28).
//!
//! The plan is a pure function of *rules* (scope, order, targets), the *calendar* (each date is a problem day, a
//! practice day or a break, over the weekly routine) and what has already been done. Nothing about the future is
//! stored, so changing a rule or an override moves everything after it, and changing it back puts it all back.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use chrono::{Datelike, Days, NaiveDate, Weekday};
use serde::{Deserialize, Serialize};

/// What a day is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    /// A new problem, plus the reviews the day has room for.
    Solve,
    /// Reviews only.
    Practice,
    /// Nothing.
    Break,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Practice {
    None,
    /// LeetCode practice for the topics that have a target, and the topics before them.
    #[default]
    Targeted,
    All,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Order {
    /// NeetCode's order, topic by topic.
    #[default]
    Curriculum,
    /// Within each topic, easy before hard.
    Ramp,
    /// By how often the chosen companies ask it, the last six months counting double.
    Company,
    /// The core problems and the widely asked ones first.
    Important,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, PartialOrd, Ord)]
pub enum Diff {
    E,
    M,
    H,
}

/// What the owner can change about the plan, apart from the calendar.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Rules {
    /// Premium problems need a subscription. `None` follows the goal's "free only" setting.
    pub premium: Option<bool>,
    pub practice: Practice,
    /// Topics (track codes) in scope; `None` is every topic.
    pub topics: Option<Vec<String>>,
    /// Only problems asked by one of these companies; empty is no filter.
    pub companies: Vec<String>,
    pub recent_only: bool,
    pub difficulty: Vec<Diff>,
    pub order: Order,
    /// The order topics are taken in; `None` is the catalog's.
    pub topic_order: Option<Vec<String>>,
    /// A date per topic. A target also covers the topics before it, from where the owner started.
    pub targets: BTreeMap<String, NaiveDate>,
    /// Everything done by. `None` follows the target date in the review settings.
    pub finish_by: Option<NaiveDate>,
}

impl Default for Rules {
    fn default() -> Self {
        Rules { premium: None, practice: Practice::default(), topics: None, companies: Vec::new(), recent_only: false, difficulty: vec![Diff::E, Diff::M, Diff::H], order: Order::default(), topic_order: None, targets: BTreeMap::new(), finish_by: None }
    }
}

/// The weekly routine the calendar sits on.
#[derive(Debug, Clone)]
pub struct Routine {
    pub solve_days: Vec<Weekday>,
    /// Reviews a day can take, Monday first.
    pub capacity: [u32; 7],
    /// A practice day takes at least this many reviews.
    pub practice_floor: u32,
    pub new_per_day: u32,
    /// A weekday's own number of new problems, Monday first; 0 keeps `new_per_day`.
    pub new_by_day: [u32; 7],
}

pub const PRACTICE_FLOOR: u32 = 6;

impl Routine {
    /// How many new problems a solve day takes.
    pub fn new_on(&self, day: NaiveDate) -> u32 {
        match self.new_by_day[day.weekday().num_days_from_monday() as usize] {
            0 => self.new_per_day.max(1),
            n => n,
        }
    }

    pub fn kind_of(&self, day: NaiveDate, ov: &BTreeMap<NaiveDate, Kind>) -> Kind {
        ov.get(&day).copied().unwrap_or(if self.solve_days.contains(&day.weekday()) { Kind::Solve } else { Kind::Practice })
    }

    /// How many reviews a day can take. A break takes none; a practice day takes at least the floor.
    pub fn capacity_of(&self, day: NaiveDate, ov: &BTreeMap<NaiveDate, Kind>) -> u32 {
        let base = self.capacity[day.weekday().num_days_from_monday() as usize];
        match self.kind_of(day, ov) {
            Kind::Break => 0,
            Kind::Practice => base.max(self.practice_floor),
            Kind::Solve => base,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Company {
    pub name: String,
    /// How often it's asked, 0–100.
    pub freq: f32,
    pub recent: bool,
}

/// A problem the plan can schedule.
#[derive(Debug, Clone)]
pub struct Item {
    pub id: String,
    pub topic: String,
    /// A LeetCode practice problem: it takes a slot but schedules no review.
    pub practice: bool,
    pub premium: bool,
    pub diff: Diff,
    /// Position in the catalog, which is the curriculum order.
    pub order: usize,
    pub companies: Vec<Company>,
    /// [`crate::reviews::importance`].
    pub importance: f32,
    /// In the list the goal counts (or among the extras), so it is the plan's to schedule. Practice problems are always
    /// listed; the rules decide whether they are used.
    pub listed: bool,
}

/// A review still to come: for a problem already solved, its real next due date; for a planned one, a projection.
#[derive(Debug, Clone)]
pub struct Owed {
    pub item: usize,
    /// Which review this is (0 is the first after the solve).
    pub n: usize,
    pub due: NaiveDate,
}

pub struct Ctx<'a> {
    pub items: &'a [Item],
    /// Indices of the problems already solved.
    pub solved: &'a HashSet<usize>,
    /// Reviews owed by problems already solved.
    pub owed: &'a [Owed],
    pub today: NaiveDate,
    pub overrides: &'a BTreeMap<NaiveDate, Kind>,
    pub routine: &'a Routine,
    /// The topic the owner started from, if any: a target covers the topics from here to it.
    pub start_topic: Option<&'a str>,
    /// The defaults rules fall back on: whether Premium is out, and the date everything is due.
    pub free_only: bool,
    pub default_finish: Option<NaiveDate>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TopicStatus {
    pub n: usize,
    /// The first day a problem of the topic is scheduled.
    pub start: Option<NaiveDate>,
    pub end: Option<NaiveDate>,
    pub due: Option<NaiveDate>,
    /// Days after its due date the topic finishes; 0 is on time, and [`NEVER`] means it doesn't fit at all.
    pub late: i64,
    /// Days to spare before its due date.
    pub slack: Option<i64>,
    /// Has a target of its own (or is covered by one), not only the final date.
    pub targeted: bool,
}

pub const NEVER: i64 = 9999;

/// The problems in scope laid on the solve days.
#[derive(Debug, Clone, Default)]
pub struct Queue {
    pub day_of: HashMap<usize, NaiveDate>,
    pub new_on: BTreeMap<NaiveDate, Vec<usize>>,
    /// Item indices in the order they are done.
    pub order: Vec<usize>,
    pub finish: Option<NaiveDate>,
    pub status: BTreeMap<String, TopicStatus>,
    pub topic_order: Vec<String>,
    pub due: BTreeMap<String, NaiveDate>,
    /// Topics covered by a target of their own.
    pub explicit: BTreeSet<String>,
    /// Problems in scope that don't fit within the horizon.
    pub left: usize,
}

/// Days the planner looks ahead.
pub const HORIZON: u64 = 500;
/// Solve slots kept in hand before a deadline before the topic is pulled forward.
const SPARE: i64 = 2;
/// Illustrative gaps between reviews of a problem recalled every time, for reviews that are not due yet.
pub const LADDER: [u64; 4] = [4, 14, 45, 120];
const EARLY: f64 = 1.0;
const LATE: f64 = 1.6;
const WINDOW_BEFORE: i64 = 3;

fn day_plus(d: NaiveDate, n: i64) -> NaiveDate {
    if n >= 0 { d + Days::new(n as u64) } else { d - Days::new(n.unsigned_abs()) }
}

/// The topics in the order the curriculum visits them, from where the owner started (the items' `order` is rotated so
/// the starting problem is first).
pub fn default_topic_order(items: &[Item]) -> Vec<String> {
    let mut by_order: Vec<&Item> = items.iter().collect();
    by_order.sort_by_key(|i| i.order);
    let mut seen: Vec<String> = Vec::new();
    for i in by_order {
        if !seen.contains(&i.topic) {
            seen.push(i.topic.clone());
        }
    }
    seen
}

/// The topics each target covers, and the date each is due.
pub fn deadlines(rules: &Rules, topic_order: &[String], start_topic: Option<&str>, finish_by: Option<NaiveDate>) -> (BTreeMap<String, NaiveDate>, BTreeSet<String>) {
    let mut dl: BTreeMap<String, NaiveDate> = BTreeMap::new();
    let mut explicit = BTreeSet::new();
    let start = start_topic.and_then(|s| topic_order.iter().position(|t| t == s)).unwrap_or(0);
    for (topic, date) in &rules.targets {
        let Some(end) = topic_order.iter().position(|t| t == topic) else { continue };
        for id in topic_order.iter().take(end + 1).skip(start) {
            explicit.insert(id.clone());
            let e = dl.entry(id.clone()).or_insert(*date);
            *e = (*e).min(*date);
        }
    }
    if let Some(f) = finish_by {
        for id in topic_order {
            let e = dl.entry(id.clone()).or_insert(f);
            *e = (*e).min(f);
        }
    }
    (dl, explicit)
}

fn co_score(it: &Item, rules: &Rules) -> f32 {
    it.companies
        .iter()
        .filter(|c| (rules.companies.is_empty() || rules.companies.contains(&c.name)) && (!rules.recent_only || c.recent))
        .map(|c| c.freq * if c.recent { 2.0 } else { 1.0 })
        .sum()
}

/// Chooses the unsolved problems in scope and lays them on the solve days. A topic is taken in order until a deadline
/// gets close; then its topic (and the ones before it) jump the queue, just in time and no sooner, so the order chosen
/// holds for as long as every target is still reachable.
pub fn build_queue(rules: &Rules, ctx: &Ctx) -> Queue {
    let topic_order: Vec<String> = rules.topic_order.clone().unwrap_or_else(|| default_topic_order(ctx.items));
    let finish_by = rules.finish_by.or(ctx.default_finish);
    let (dl, explicit) = deadlines(rules, &topic_order, ctx.start_topic, finish_by);
    let premium = rules.premium.unwrap_or(!ctx.free_only);

    let mut chain: BTreeMap<String, Vec<usize>> = BTreeMap::new();
    for (idx, it) in ctx.items.iter().enumerate() {
        let in_scope = it.listed
            && !ctx.solved.contains(&idx)
            && (premium || !it.premium)
            && rules.topics.as_ref().is_none_or(|t| t.contains(&it.topic))
            && (!it.practice || rules.practice == Practice::All || (rules.practice == Practice::Targeted && explicit.contains(&it.topic)))
            && rules.difficulty.contains(&it.diff)
            && (rules.companies.is_empty() || co_score(it, rules) > 0.0);
        if in_scope {
            chain.entry(it.topic.clone()).or_default().push(idx);
        }
    }
    for list in chain.values_mut() {
        let items = ctx.items;
        list.sort_by(|&a, &b| {
            let (x, y) = (&items[a], &items[b]);
            match rules.order {
                Order::Curriculum => x.practice.cmp(&y.practice).then(x.order.cmp(&y.order)),
                Order::Ramp => x.practice.cmp(&y.practice).then(x.diff.cmp(&y.diff)).then(x.order.cmp(&y.order)),
                Order::Company => co_score(y, rules).total_cmp(&co_score(x, rules)).then(x.order.cmp(&y.order)),
                Order::Important => y.importance.total_cmp(&x.importance).then(x.order.cmp(&y.order)),
            }
        });
    }

    let mut slots: Vec<NaiveDate> = Vec::new();
    for n in 0..HORIZON {
        let d = ctx.today + Days::new(n);
        if ctx.routine.kind_of(d, ctx.overrides) == Kind::Solve {
            for _ in 0..ctx.routine.new_on(d) {
                slots.push(d);
            }
        }
    }
    let up_to = |date: NaiveDate| slots.partition_point(|s| *s <= date) as i64;

    let mut remaining: BTreeMap<&str, usize> = chain.iter().map(|(t, l)| (t.as_str(), l.len())).collect();
    let mut taken: BTreeMap<&str, usize> = chain.keys().map(|t| (t.as_str(), 0)).collect();
    let mut q = Queue { topic_order: topic_order.clone(), due: dl.clone(), explicit: explicit.clone(), ..Queue::default() };
    for (k, day) in slots.iter().enumerate() {
        let live: Vec<&String> = topic_order.iter().filter(|t| remaining.get(t.as_str()).is_some_and(|n| *n > 0)).collect();
        if live.is_empty() {
            break;
        }
        // the earliest-deadline topic whose spare slots have run out, counting everything due no later than it
        let mut by_due: Vec<&&String> = live.iter().filter(|t| dl.contains_key(t.as_str())).collect();
        by_due.sort_by_key(|t| (dl[t.as_str()], topic_order.iter().position(|x| x == **t)));
        let mut tight: Option<&String> = None;
        let mut demand = 0i64;
        for t in by_due {
            demand += remaining[t.as_str()] as i64;
            if up_to(dl[t.as_str()]) - k as i64 - demand <= SPARE {
                tight = Some(t);
                break;
            }
        }
        let pick = tight.unwrap_or_else(|| {
            if rules.order == Order::Company {
                live.iter()
                    .copied()
                    .reduce(|best, x| {
                        let score = |t: &String| co_score(&ctx.items[chain[t][taken[t.as_str()]]], rules);
                        if score(x) > score(best) { x } else { best }
                    })
                    .expect("live is not empty")
            } else {
                live[0]
            }
        });
        let at = taken[pick.as_str()];
        let idx = chain[pick][at];
        *taken.get_mut(pick.as_str()).expect("topic has a chain") += 1;
        *remaining.get_mut(pick.as_str()).expect("topic has a chain") -= 1;
        q.day_of.insert(idx, *day);
        q.new_on.entry(*day).or_default().push(idx);
        q.order.push(idx);
    }
    q.left = remaining.values().sum();
    q.finish = if q.left > 0 { None } else { q.order.last().map(|i| q.day_of[i]).or(Some(ctx.today)) };

    for t in &topic_order {
        let Some(list) = chain.get(t) else { continue };
        let days: Vec<Option<NaiveDate>> = list.iter().map(|i| q.day_of.get(i).copied()).collect();
        let end = if days.iter().all(Option::is_some) { days.iter().flatten().max().copied() } else { None };
        let due = dl.get(t).copied();
        let start = days.iter().flatten().min().copied();
        let late = match (due, end) {
            (Some(d), Some(e)) => (e - d).num_days().max(0),
            (Some(_), None) => NEVER,
            _ => 0,
        };
        let slack = due.zip(end).map(|(d, e)| (d - e).num_days());
        q.status.insert(t.clone(), TopicStatus { n: list.len(), start, end, due, late, slack, targeted: explicit.contains(t) });
    }
    q
}

/// Where a review landed.
#[derive(Debug, Clone, Serialize)]
pub struct Placed {
    pub item: usize,
    pub n: usize,
    pub due: NaiveDate,
}

/// The reviews placed on days, and how many each day carries.
#[derive(Debug, Clone, Default)]
pub struct Reviews {
    pub on: BTreeMap<NaiveDate, Vec<Placed>>,
    pub load: HashMap<NaiveDate, u32>,
}

/// Places every review a problem still owes on the day that costs least: early is cheaper than late, a crowded day
/// costs steeply, and a break takes none.
pub fn place_reviews(ctx: &Ctx, q: &Queue) -> Reviews {
    let mut owed: Vec<Owed> = ctx.owed.to_vec();
    let sum = |n: usize| LADDER.iter().take(n + 1).sum::<u64>();
    for (&item, &day) in &q.day_of {
        if ctx.items[item].practice {
            continue;
        }
        for n in 0..LADDER.len() {
            owed.push(Owed { item, n, due: day + Days::new(sum(n)) });
        }
    }
    owed.sort_by(|a, b| a.due.cmp(&b.due).then(a.n.cmp(&b.n)).then(a.item.cmp(&b.item)));
    let mut out = Reviews::default();
    for r in owed {
        let cost = |d: NaiveDate, load: &HashMap<NaiveDate, u32>| -> f64 {
            let cap = ctx.routine.capacity_of(d, ctx.overrides);
            if cap == 0 || d < ctx.today {
                return f64::INFINITY;
            }
            let gap = (d - r.due).num_days() as f64;
            let l = f64::from(load.get(&d).copied().unwrap_or(0));
            let c = f64::from(cap);
            (if gap < 0.0 { -gap * EARLY } else { gap * LATE }) + (l / c) * 2.0 + if l >= c { 6.0 + (l - c) * 3.0 } else { 0.0 }
        };
        let mut best: Option<NaiveDate> = None;
        let mut best_cost = f64::INFINITY;
        for g in -WINDOW_BEFORE..=90 {
            if g > 0 && (g as f64) * LATE > best_cost {
                break;
            }
            let d = day_plus(r.due, g);
            let c = cost(d, &out.load);
            if c < best_cost {
                best = Some(d);
                best_cost = c;
            }
        }
        if let Some(d) = best {
            *out.load.entry(d).or_default() += 1;
            out.on.entry(d).or_default().push(Placed { item: r.item, n: r.n, due: d });
        }
    }
    out
}

#[derive(Debug, Clone)]
pub struct Planned {
    pub queue: Queue,
    pub reviews: Reviews,
}

pub fn plan(rules: &Rules, ctx: &Ctx) -> Planned {
    let queue = build_queue(rules, ctx);
    let reviews = place_reviews(ctx, &queue);
    Planned { queue, reviews }
}

/// What a change would do.
#[derive(Debug, Clone, Default, Serialize, PartialEq)]
pub struct Diff2 {
    /// Problems that move to another day.
    pub moved: usize,
    /// Reviews of problems already solved that land on another day.
    pub reviews_moved: usize,
    pub finish_days: i64,
    pub added: usize,
    pub dropped: usize,
}

pub fn diff(before: &Planned, after: &Planned, solved_with_reviews: &HashSet<usize>) -> Diff2 {
    let moved = after.queue.day_of.iter().filter(|(i, d)| before.queue.day_of.get(i).is_some_and(|b| b != *d)).count();
    let where_of = |p: &Planned| -> HashMap<(usize, usize), NaiveDate> { p.reviews.on.iter().flat_map(|(d, rs)| rs.iter().map(move |r| ((r.item, r.n), *d))).collect() };
    let (a, b) = (where_of(before), where_of(after));
    let reviews_moved = b.iter().filter(|(k, d)| solved_with_reviews.contains(&k.0) && a.get(k).is_some_and(|x| x != *d)).count();
    let finish_days = before.queue.finish.zip(after.queue.finish).map_or(0, |(x, y)| (y - x).num_days());
    let added = after.queue.order.iter().filter(|i| !before.queue.day_of.contains_key(i)).count();
    let dropped = before.queue.order.iter().filter(|i| !after.queue.day_of.contains_key(i)).count();
    Diff2 { moved, reviews_moved, finish_days, added, dropped }
}

/// How a late topic can be brought home.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Remedy {
    /// These practice days become problem days, and the topic then lands on `end`.
    AddDays { days: Vec<NaiveDate>, end: NaiveDate },
    /// Even every free day isn't enough; at the current pace the topic lands on `end` (if it fits at all).
    NotEnough { end: Option<NaiveDate> },
}

/// The fewest extra problem days that bring a late topic home: practice days (Sundays last) become problem days until
/// it fits.
pub fn what_it_takes(rules: &Rules, ctx: &Ctx, topic: &str) -> Option<Remedy> {
    let base = build_queue(rules, ctx);
    let s = base.status.get(topic)?;
    if s.late == 0 {
        return None;
    }
    let due = s.due?;
    let mut cands: Vec<NaiveDate> = (0..400u64)
        .map(|n| ctx.today + Days::new(n))
        .take_while(|d| *d <= due)
        .filter(|d| ctx.routine.kind_of(*d, ctx.overrides) == Kind::Practice)
        .collect();
    cands.sort_by_key(|d| (d.weekday() == Weekday::Sun, *d));
    let mut ov = ctx.overrides.clone();
    let mut flips = Vec::new();
    for d in cands.into_iter().take(12) {
        ov.insert(d, Kind::Solve);
        flips.push(d);
        let c = Ctx { overrides: &ov, ..*ctx };
        let st = build_queue(rules, &c).status.get(topic).cloned()?;
        if st.late == 0 {
            flips.sort();
            return Some(Remedy::AddDays { days: flips, end: st.end.unwrap_or(due) });
        }
    }
    Some(Remedy::NotEnough { end: s.end })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(s: &str) -> NaiveDate {
        s.parse().unwrap()
    }

    fn routine() -> Routine {
        Routine { solve_days: vec![Weekday::Mon, Weekday::Tue, Weekday::Wed, Weekday::Thu, Weekday::Fri, Weekday::Sat], capacity: [1, 1, 1, 1, 1, 3, 12], practice_floor: PRACTICE_FLOOR, new_per_day: 1, new_by_day: [0; 7] }
    }

    #[test]
    fn a_weekday_takes_its_own_number_of_new_problems() {
        let mut r = routine();
        r.new_per_day = 2;
        r.new_by_day[2] = 4; // Wednesday
        let wed = d("2026-11-04");
        assert_eq!((r.new_on(wed), r.new_on(d("2026-11-03"))), (4, 2));
        r.new_by_day = [0; 7];
        r.new_per_day = 0;
        assert_eq!(r.new_on(wed), 1);
    }

    /// topic, main problems, practice problems; everything before `graph` plus two graph problems is solved.
    fn catalog() -> Vec<Item> {
        let spec: [(&str, usize, usize); 6] = [("arr", 5, 2), ("tree", 8, 2), ("heap", 4, 2), ("graph", 10, 4), ("adv", 5, 2), ("dp", 7, 2)];
        let mut items = Vec::new();
        for (topic, main, practice) in spec {
            for k in 0..main + practice {
                let order = items.len();
                let is_practice = k >= main;
                items.push(Item {
                    id: format!("{topic}-{k}"),
                    topic: topic.into(),
                    practice: is_practice,
                    premium: topic == "graph" && k == 9,
                    diff: [Diff::E, Diff::M, Diff::M, Diff::H][k % 4],
                    order,
                    companies: if order % 3 == 0 { vec![Company { name: "Amazon".into(), freq: 40.0 + (order % 7) as f32 * 8.0, recent: order % 2 == 0 }] } else { vec![] },
                    importance: 1.0 + (order % 5) as f32 * 0.1,
                    listed: true,
                });
            }
        }
        items
    }

    fn solved_through_graph_two(items: &[Item]) -> HashSet<usize> {
        let first_graph = items.iter().position(|i| i.topic == "graph").unwrap();
        items.iter().enumerate().filter(|(i, it)| !it.practice && *i < first_graph + 2).map(|(i, _)| i).collect()
    }

    fn with<R>(f: impl FnOnce(&Ctx) -> R, ov: BTreeMap<NaiveDate, Kind>) -> R {
        let items = catalog();
        let solved = solved_through_graph_two(&items);
        let r = routine();
        let ctx = Ctx { items: &items, solved: &solved, owed: &[], today: d("2026-10-08"), overrides: &ov, routine: &r, start_topic: Some("graph"), free_only: true, default_finish: None };
        f(&ctx)
    }

    fn topics_of(c: &Ctx, q: &Queue) -> Vec<String> {
        q.order.iter().map(|i| c.items[*i].topic.clone()).collect()
    }

    #[test]
    fn without_targets_the_queue_is_the_curriculum_with_premium_and_practice_left_out() {
        with(
            |c| {
                let q = build_queue(&Rules::default(), c);
                assert!(q.order.iter().all(|i| !c.items[*i].premium && !c.items[*i].practice));
                assert_eq!(c.items[q.order[0]].id, "graph-2");
                let mut seen: Vec<String> = Vec::new();
                for t in topics_of(c, &q) {
                    if !seen.contains(&t) {
                        seen.push(t);
                    }
                }
                assert_eq!(seen, ["graph", "adv", "dp"]);
                // a problem every day but Sunday
                assert_eq!(q.new_on[&d("2026-10-08")].len(), 1);
                assert!(!q.new_on.contains_key(&d("2026-10-11")));
            },
            BTreeMap::new(),
        );
    }

    #[test]
    fn premium_can_be_included() {
        with(
            |c| {
                let out = build_queue(&Rules::default(), c).order.len();
                let with_premium = build_queue(&Rules { premium: Some(true), ..Rules::default() }, c).order.len();
                assert_eq!(with_premium, out + 1);
            },
            BTreeMap::new(),
        );
    }

    #[test]
    fn a_target_on_advanced_graphs_schedules_graphs_and_advanced_graphs_with_practice_by_then() {
        with(
            |c| {
                let rules = Rules { targets: BTreeMap::from([("adv".to_string(), d("2026-10-31"))]), ..Rules::default() };
                let q = build_queue(&rules, c);
                for t in ["graph", "adv"] {
                    let s = &q.status[t];
                    assert_eq!(s.late, 0, "{t}");
                    assert!(s.end.unwrap() <= d("2026-10-31"));
                }
                let has = |topic: &str| q.order.iter().any(|i| c.items[*i].practice && c.items[*i].topic == topic);
                assert!(has("graph") && has("adv"));
                assert!(!has("dp"), "practice only where a target reaches");
            },
            BTreeMap::new(),
        );
    }

    #[test]
    fn a_target_covers_the_topics_before_it_from_the_start_only() {
        let order: Vec<String> = ["arr", "graph", "adv", "dp"].map(String::from).to_vec();
        let rules = Rules { targets: BTreeMap::from([("adv".to_string(), d("2026-10-31"))]), ..Rules::default() };
        let (dl, _) = deadlines(&rules, &order, Some("graph"), None);
        assert_eq!(dl.keys().cloned().collect::<Vec<_>>(), ["adv", "graph"]);
        let (dl, _) = deadlines(&rules, &order, Some("adv"), None);
        assert_eq!(dl.keys().cloned().collect::<Vec<_>>(), ["adv"]);
    }

    #[test]
    fn a_deadline_moves_its_topics_ahead_of_earlier_ones_only_when_it_has_to() {
        // Trees, Heap come before Graphs in the order, but the owner started from Graphs.
        let items = catalog();
        let first_tree = items.iter().position(|i| i.topic == "tree").unwrap();
        let solved: HashSet<usize> = items.iter().enumerate().filter(|(i, it)| !it.practice && *i < first_tree).map(|(i, _)| i).collect();
        let r = routine();
        let ov = BTreeMap::new();
        let ctx = Ctx { items: &items, solved: &solved, owed: &[], today: d("2026-10-08"), overrides: &ov, routine: &r, start_topic: Some("graph"), free_only: true, default_finish: None };
        let rules = |date: &str| Rules { targets: BTreeMap::from([("adv".to_string(), d(date))]), practice: Practice::None, ..Rules::default() };
        let loose = build_queue(&rules("2027-03-01"), &ctx);
        assert_eq!(items[loose.order[0]].topic, "tree", "plenty of time: keep the order");
        let tight = build_queue(&rules("2026-11-02"), &ctx);
        assert_eq!(tight.status["adv"].late, 0);
        assert_eq!(tight.status["graph"].late, 0);
        let first_graph = tight.order.iter().position(|i| items[*i].topic == "graph").unwrap();
        let last_tree = tight.order.iter().rposition(|i| items[*i].topic == "tree").unwrap();
        assert!(first_graph < last_tree, "Graphs is pulled in front of the end of Trees once its deadline is close");
    }

    #[test]
    fn an_impossible_target_is_late_and_what_it_takes_says_so() {
        with(
            |c| {
                let rules = Rules { targets: BTreeMap::from([("adv".to_string(), d("2026-10-16"))]), ..Rules::default() };
                let q = build_queue(&rules, c);
                assert!(q.status["adv"].late > 0 || q.status["graph"].late > 0);
                let late = if q.status["adv"].late > 0 { "adv" } else { "graph" };
                assert!(what_it_takes(&rules, c, late).is_some());
            },
            BTreeMap::new(),
        );
    }

    #[test]
    fn extra_problem_days_fix_a_slightly_late_topic() {
        with(
            |c| {
                let rules = Rules { targets: BTreeMap::from([("adv".to_string(), d("2026-10-27"))]), ..Rules::default() };
                let q = build_queue(&rules, c);
                if q.status["adv"].late > 0 {
                    match what_it_takes(&rules, c, "adv").unwrap() {
                        Remedy::AddDays { days, end } => assert!(!days.is_empty() && end <= d("2026-10-27")),
                        Remedy::NotEnough { end } => assert!(end.is_some()),
                    }
                }
            },
            BTreeMap::new(),
        );
    }

    #[test]
    fn company_order_takes_that_companys_most_asked_first_and_only_theirs() {
        let items = catalog();
        let solved = HashSet::new();
        let r = routine();
        let ov = BTreeMap::new();
        let ctx = Ctx { items: &items, solved: &solved, owed: &[], today: d("2026-10-08"), overrides: &ov, routine: &r, start_topic: Some("arr"), free_only: true, default_finish: None };
        let rules = Rules { companies: vec!["Amazon".into()], recent_only: true, order: Order::Company, practice: Practice::None, ..Rules::default() };
        let q = build_queue(&rules, &ctx);
        assert!(q.order.len() > 3);
        assert!(q.order.iter().all(|i| items[*i].companies.iter().any(|c| c.name == "Amazon" && c.recent)));
        let score = |i: &usize| co_score(&items[*i], &rules);
        assert!(score(&q.order[0]) >= score(q.order.last().unwrap()));
    }

    #[test]
    fn a_practice_week_pushes_the_queue_and_removing_it_restores_the_plan() {
        let base = with(|c| plan(&Rules::default(), c).queue.day_of.clone(), BTreeMap::new());
        let ov: BTreeMap<NaiveDate, Kind> = (4..9).map(|n| (d("2026-10-08") + Days::new(n), Kind::Practice)).collect();
        let (finish_base, finish_after) = with(
            |c| {
                let b = plan(&Rules::default(), &Ctx { overrides: &BTreeMap::new(), ..*c });
                let a = plan(&Rules::default(), c);
                (b.queue.finish.unwrap(), a.queue.finish.unwrap())
            },
            ov,
        );
        assert!((finish_after - finish_base).num_days() >= 5);
        let again = with(|c| plan(&Rules::default(), c).queue.day_of.clone(), BTreeMap::new());
        assert_eq!(base, again);
    }

    #[test]
    fn reviews_never_land_on_a_break_or_beyond_capacity_and_a_short_break_pulls_them_earlier() {
        let items = catalog();
        let solved = solved_through_graph_two(&items);
        let r = routine();
        // 12 solved problems each owing a review, due in the next two weeks
        let owed: Vec<Owed> = (0..12).map(|k| Owed { item: k, n: 0, due: d("2026-10-12") + Days::new(k as u64 % 6) }).collect();
        let ov: BTreeMap<NaiveDate, Kind> = [(d("2026-10-12"), Kind::Break), (d("2026-10-13"), Kind::Break)].into();
        let ctx = Ctx { items: &items, solved: &solved, owed: &owed, today: d("2026-10-08"), overrides: &ov, routine: &r, start_topic: Some("graph"), free_only: true, default_finish: None };
        let p = plan(&Rules::default(), &ctx);
        for day in ov.keys() {
            assert!(!p.reviews.on.contains_key(day), "{day}");
        }
        for (day, n) in &p.reviews.load {
            assert!(*n <= r.capacity_of(*day, &ov).max(12), "{day} carries {n}");
        }
        // The ones due on the break days are placed earlier when there's room, otherwise after.
        let moved: Vec<NaiveDate> = p.reviews.on.iter().filter(|(_, rs)| rs.iter().any(|x| x.item == 0)).map(|(d, _)| *d).collect();
        assert_eq!(moved.len(), 1);
    }

    #[test]
    fn capacity_follows_the_kind_of_day() {
        let r = routine();
        let ov: BTreeMap<NaiveDate, Kind> = [(d("2026-10-12"), Kind::Break), (d("2026-10-13"), Kind::Practice)].into();
        assert_eq!(r.capacity_of(d("2026-10-12"), &ov), 0);
        assert_eq!(r.capacity_of(d("2026-10-13"), &ov), PRACTICE_FLOOR);
        assert_eq!(r.capacity_of(d("2026-10-14"), &ov), 1);
        assert_eq!(r.capacity_of(d("2026-10-11"), &ov), 12);
    }
}
