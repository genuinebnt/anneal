//! The plan and calendar API (docs/DSA.md, decision 28).
//!
//! The planner itself is [`crate::planner`], a pure function. This module loads what it needs (the catalog, what is
//! solved, the reviews owed, the owner's rules and calendar edits), and serves the result:
//!
//! - `GET /api/plan?from&to`: the plan for a range of days, the topics and how each target fares, the saved plans, a
//!   suggestion for the first late topic, and what the last eight weeks looked like.
//! - `POST /api/plan/preview`: what a change would do, without saving it.
//! - `PUT /api/plan/state`: replace the active plan's rules and its calendar edits (also what undo sends).
//! - `POST /api/plans`, `PUT /api/plans/active`, `DELETE /api/plans/{id}`: saved plans.

use std::collections::{BTreeMap, HashMap, HashSet};

use anneal_content::{Band, Catalog, Role};
use axum::Json;
use axum::extract::{Path, Query, State};
use chrono::{DateTime, Datelike, Days, NaiveDate};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::types::Json as Jsonb;

use crate::AppState;
use crate::dsa::dsa_tracks;
use crate::error::{ApiError, ApiResult};
use crate::planner::{self, Company, Ctx, Diff, Item, Kind, LADDER, Order, Owed, PRACTICE_FLOOR, Planned, Practice, Remedy, Routine, Rules};
use crate::reviews::{self, Settings};
use crate::store::{self, ProgressRow, ReviewRow};

pub(crate) type Overrides = BTreeMap<NaiveDate, Kind>;

/// A saved set of rules, with its own calendar edits.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SavedPlan {
    pub id: String,
    pub name: String,
    pub rules: Rules,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Book {
    pub active: String,
    pub list: Vec<SavedPlan>,
}

impl Default for Book {
    fn default() -> Self {
        Book { active: "default".into(), list: vec![SavedPlan { id: "default".into(), name: "Default".into(), rules: Rules::default() }] }
    }
}

impl Book {
    pub fn active_plan(&self) -> &SavedPlan {
        self.list.iter().find(|p| p.id == self.active).unwrap_or(&self.list[0])
    }
}

pub(crate) async fn book(db: &sqlx::PgPool) -> sqlx::Result<Book> {
    let stored: Option<Jsonb<Book>> = sqlx::query_scalar("SELECT value FROM settings WHERE key = 'plans'").fetch_optional(db).await?;
    let mut book = stored.map(|j| j.0).unwrap_or_default();
    if book.list.is_empty() {
        book = Book::default();
    }
    if !book.list.iter().any(|p| p.id == book.active) {
        book.active = book.list[0].id.clone();
    }
    Ok(book)
}

async fn save_book(s: &AppState, book: &Book) -> ApiResult<()> {
    crate::settings::store(s, "plans", book).await
}

fn kind_text(k: Kind) -> &'static str {
    match k {
        Kind::Solve => "solve",
        Kind::Practice => "practice",
        Kind::Break => "break",
    }
}

fn parse_kind(s: &str) -> Option<Kind> {
    match s {
        "solve" => Some(Kind::Solve),
        "practice" => Some(Kind::Practice),
        "break" => Some(Kind::Break),
        _ => None,
    }
}

pub(crate) async fn all_overrides(db: &sqlx::PgPool) -> sqlx::Result<HashMap<String, Overrides>> {
    let rows: Vec<(String, NaiveDate, String)> = sqlx::query_as("SELECT plan_id, day, kind FROM plan_overrides").fetch_all(db).await?;
    let mut out: HashMap<String, Overrides> = HashMap::new();
    for (plan, day, kind) in rows {
        if let Some(k) = parse_kind(&kind) {
            out.entry(plan).or_default().insert(day, k);
        }
    }
    Ok(out)
}

/// The weekly routine the settings describe: which weekdays are solve days and how many reviews each day takes.
pub(crate) fn routine_of(settings: &reviews::Settings) -> Routine {
    Routine { solve_days: settings.new_days.iter().filter_map(|d| reviews::weekday(d)).collect(), capacity: settings.capacity.all(), practice_floor: PRACTICE_FLOOR, new_per_day: settings.new_per_day.max(1) }
}

/// Whether a day is a planned solve day, from the settings and the active plan's edits. The streak follows it: a rest,
/// practice or break day neither breaks the streak nor adds to it.
pub(crate) async fn solve_day_fn(db: &sqlx::PgPool) -> sqlx::Result<impl Fn(NaiveDate) -> bool + use<>> {
    let routine = routine_of(&crate::settings::srs(db).await?);
    let overrides = active_overrides(db).await?;
    Ok(move |day| routine.kind_of(day, &overrides) == Kind::Solve)
}

/// The days the active plan has made a break, for keeping reviews off them when they are scheduled.
pub(crate) async fn active_overrides(db: &sqlx::PgPool) -> sqlx::Result<Overrides> {
    let book = book(db).await?;
    Ok(all_overrides(db).await?.remove(&book.active_plan().id).unwrap_or_default())
}

fn local(t: DateTime<chrono::Utc>) -> NaiveDate {
    t.with_timezone(&chrono::Local).date_naive()
}

/// Everything the planner needs, loaded once.
pub(crate) struct World<'a> {
    pub catalog: &'a Catalog,
    pub items: Vec<Item>,
    pub ids: Vec<&'a str>,
    index: HashMap<&'a str, usize>,
    pub solved: HashSet<usize>,
    pub owed: Vec<Owed>,
    /// Problems that have a review on record.
    pub with_reviews: HashSet<usize>,
    pub today: NaiveDate,
    pub routine: Routine,
    pub settings: Settings,
    pub start_topic: Option<String>,
    /// Track code and name, in the catalog's order.
    pub topics: Vec<(String, String)>,
}

fn diff_of(b: Band) -> Diff {
    match b {
        Band::Easy => Diff::E,
        Band::Medium => Diff::M,
        Band::Hard => Diff::H,
    }
}

impl<'a> World<'a> {
    pub fn build(catalog: &'a Catalog, settings: &Settings, progress: &HashMap<String, ProgressRow>, rows: &[ReviewRow], start: Option<&str>, today: NaiveDate) -> World<'a> {
        let mut items = Vec::new();
        let mut ids: Vec<&str> = Vec::new();
        let mut topics = Vec::new();
        let mut in_goal = Vec::new();
        let mut in_250 = Vec::new();
        for t in dsa_tracks(catalog) {
            topics.push((t.code.clone(), t.name.clone()));
            let practice = catalog.practice_tracks.iter().find(|x| x.code == t.code);
            for (is_practice, problems) in [(false, t.problems.as_slice()), (true, practice.map_or(&[][..], |x| x.problems.as_slice()))] {
                for p in problems {
                    let Some(d) = p.dsa.as_ref() else { continue };
                    let recent = d.companies.iter().filter(|c| c.recent).count();
                    in_goal.push(!is_practice && d.lists.contains(&settings.goal.list));
                    in_250.push(!is_practice && d.lists.iter().any(|l| l == "neetcode250"));
                    items.push(Item {
                        id: p.id.clone(),
                        topic: t.code.clone(),
                        practice: is_practice,
                        premium: d.premium,
                        diff: diff_of(p.meta.level),
                        order: items.len(),
                        companies: d.companies.iter().map(|c| Company { name: c.name.clone(), freq: c.frequency, recent: c.recent }).collect(),
                        importance: reviews::importance(&d.lists, d.role == Role::MustLearn, d.companies.len(), recent),
                        listed: is_practice,
                    });
                    ids.push(&p.id);
                }
            }
        }
        let index: HashMap<&str, usize> = ids.iter().enumerate().map(|(i, id)| (*id, i)).collect();
        // The curriculum is walked from the problem the owner started from, then round to what was left above it.
        let n = items.len().max(1);
        let start_at = start.and_then(|id| index.get(id).copied()).unwrap_or(0);
        for (i, it) in items.iter_mut().enumerate() {
            it.order = (i + n - start_at) % n;
        }
        let solved: HashSet<usize> = ids.iter().enumerate().filter(|(_, id)| progress.get(**id).is_some_and(|r| r.solved)).map(|(i, _)| i).collect();

        // What counts: the goal's list, then a few from the 250 (`extra`); once the list is done, the 250, then everything.
        let is_main: Vec<bool> = items.iter().map(|it| !it.practice).collect();
        let main = |i: &usize| is_main[*i];
        let unsolved_goal = (0..items.len()).filter(main).any(|i| in_goal[i] && !solved.contains(&i));
        let beyond_solved = (0..items.len()).filter(|i| main(i) && in_250[*i] && !in_goal[*i] && solved.contains(i)).count() as u32;
        let extra_left = settings.goal.extra.saturating_sub(beyond_solved.min(settings.goal.extra)) as usize;
        if unsolved_goal {
            let extras: Vec<usize> = (0..items.len()).filter(|i| main(i) && in_250[*i] && !in_goal[*i] && !solved.contains(i)).take(extra_left).collect();
            for i in (0..items.len()).filter(main) {
                items[i].listed = in_goal[i] || extras.contains(&i);
            }
        } else if (0..items.len()).any(|i| main(&i) && in_250[i] && !solved.contains(&i)) {
            for i in (0..items.len()).filter(main) {
                items[i].listed = in_250[i];
            }
        } else {
            for i in (0..items.len()).filter(main) {
                items[i].listed = true;
            }
        }

        let mut owed = Vec::new();
        let mut with_reviews = HashSet::new();
        for r in rows {
            let Some(&item) = index.get(r.problem_id.as_str()) else { continue };
            with_reviews.insert(item);
            let n = (r.reps - 1).max(0) as usize;
            let mut due = local(r.due_at);
            owed.push(Owed { item, n, due });
            for (m, gap) in LADDER.iter().enumerate().skip(n + 1) {
                due = due + Days::new(*gap);
                owed.push(Owed { item, n: m, due });
            }
        }
        let routine = routine_of(settings);
        let start_topic = start.and_then(|id| catalog.problem(id)).map(|(t, _)| t.code.clone());
        World { catalog, items, ids, index, solved, owed, with_reviews, today, routine, settings: settings.clone(), start_topic, topics }
    }

    pub fn ctx<'b>(&'b self, ov: &'b Overrides) -> Ctx<'b> {
        Ctx {
            items: &self.items,
            solved: &self.solved,
            owed: &self.owed,
            today: self.today,
            overrides: ov,
            routine: &self.routine,
            start_topic: self.start_topic.as_deref(),
            free_only: self.settings.goal.free_only,
            default_finish: self.settings.target_date,
        }
    }

    pub fn run(&self, rules: &Rules, ov: &Overrides) -> Planned {
        planner::plan(rules, &self.ctx(ov))
    }

    fn topic_name<'b>(&'b self, code: &'b str) -> &'b str {
        self.topics.iter().find(|(c, _)| c == code).map_or(code, |(_, n)| n.as_str())
    }

    fn item_ref(&self, i: usize) -> Value {
        let (_, p) = self.catalog.problem(self.ids[i]).expect("planned problems are in the catalog");
        let d = p.dsa.as_ref().expect("planned problems are DSA problems");
        let it = &self.items[i];
        json!({
            "id": it.id, "slug": d.slug, "title": p.meta.title, "topic": it.topic, "practice": it.practice,
            "premium": it.premium, "difficulty": p.meta.level,
            "companies": d.companies.iter().take(8).map(|c| json!({ "name": c.name, "recent": c.recent })).collect::<Vec<_>>(),
        })
    }
}

/// What the owner has done, from the review histories and the attempts.
#[derive(Default)]
struct Hist {
    /// Day to the problems logged that day (a first solve, or an attempt), with the grade.
    solves: BTreeMap<NaiveDate, Vec<(usize, String)>>,
    /// Day to the number of reviews done.
    reviews: BTreeMap<NaiveDate, u32>,
}

async fn history(s: &AppState, w: &World<'_>, rows: &[ReviewRow]) -> sqlx::Result<Hist> {
    let mut h = Hist::default();
    let mut seen = HashSet::new();
    for r in rows {
        let Some(&item) = w.index.get(r.problem_id.as_str()) else { continue };
        seen.insert(item);
        for (n, e) in r.history.0.iter().enumerate() {
            let Some(at) = e["at"].as_str().and_then(|t| DateTime::parse_from_rfc3339(t).ok()) else { continue };
            let day = local(at.with_timezone(&chrono::Utc));
            if n == 0 {
                h.solves.entry(day).or_default().push((item, e["grade"].as_str().unwrap_or("good").to_owned()));
            } else {
                *h.reviews.entry(day).or_default() += 1;
            }
        }
    }
    // Problems with no review (LeetCode practice) are in the attempts only.
    let attempts: Vec<(String, DateTime<chrono::Utc>, bool)> = sqlx::query_as("SELECT problem_id, solved_at, assisted FROM attempts WHERE solved_at IS NOT NULL ORDER BY solved_at").fetch_all(&s.db).await?;
    let mut done = HashSet::new();
    for (id, at, assisted) in attempts {
        let Some(&item) = w.index.get(id.as_str()) else { continue };
        if seen.contains(&item) || !done.insert(item) {
            continue;
        }
        h.solves.entry(local(at)).or_default().push((item, if assisted { "hard" } else { "good" }.to_owned()));
    }
    Ok(h)
}

#[derive(Deserialize)]
pub struct RangeQuery {
    from: Option<NaiveDate>,
    to: Option<NaiveDate>,
}

fn late_count(p: &Planned) -> usize {
    p.queue.status.values().filter(|s| s.late > 0).count()
}

struct Loaded {
    settings: Settings,
    rows: Vec<ReviewRow>,
    progress: HashMap<String, ProgressRow>,
    start: Option<String>,
    book: Book,
    overrides: HashMap<String, Overrides>,
}

async fn load(s: &AppState) -> ApiResult<Loaded> {
    let settings = crate::settings::srs(&s.db).await?;
    let progress = store::progress(&s.db).await?;
    let rows = crate::dsa::dsa_reviews(&s.catalog, store::reviews(&s.db).await?);
    let start = store::dsa_start(&s.db).await?;
    Ok(Loaded { settings, rows, progress, start, book: book(&s.db).await?, overrides: all_overrides(&s.db).await? })
}

pub async fn get(State(s): State<AppState>, Query(q): Query<RangeQuery>) -> ApiResult<Json<Value>> {
    let today = crate::activity::today();
    let from = q.from.unwrap_or(today - Days::new(35));
    let to = q.to.unwrap_or(today + Days::new(70));
    if to < from || (to - from).num_days() > 130 {
        return Err(ApiError::BadRequest("ask for at most 130 days at a time".into()));
    }
    let l = load(&s).await?;
    let w = World::build(&s.catalog, &l.settings, &l.progress, &l.rows, l.start.as_deref(), today);
    let empty = Overrides::new();
    let active = l.book.active_plan();
    let ov = l.overrides.get(&active.id).unwrap_or(&empty);
    let rules = &active.rules;
    let planned = w.run(rules, ov);
    let baseline = w.run(&Rules::default(), &empty);
    let hist = history(&s, &w, &l.rows).await?;
    let q = &planned.queue;

    // topics
    let order = &q.topic_order;
    let start_pos = w.start_topic.as_ref().and_then(|t| order.iter().position(|x| x == t)).unwrap_or(0);
    let topics: Vec<Value> = order
        .iter()
        .enumerate()
        .map(|(p, t)| {
            json!({
                "id": t, "name": w.topic_name(t), "done_earlier": p < start_pos, "status": q.status.get(t),
                "target": rules.targets.get(t), "covered": q.explicit.contains(t) && !rules.targets.contains_key(t),
            })
        })
        .collect();

    // days
    let tracking_start = hist.solves.keys().chain(hist.reviews.keys()).min().copied().unwrap_or(today);
    let mut days = Vec::new();
    let mut d = from;
    while d <= to {
        let kind = w.routine.kind_of(d, ov);
        let mut day = json!({ "date": d, "kind": kind, "edited": ov.contains_key(&d), "capacity": w.routine.capacity_of(d, ov) });
        if d >= today {
            let new: Vec<Value> = q.new_on.get(&d).map(|v| v.iter().map(|i| w.item_ref(*i)).collect()).unwrap_or_default();
            let reviews: Vec<Value> = planned.reviews.on.get(&d).map(|v| v.iter().map(|r| json!({ "problem": w.item_ref(r.item), "n": r.n })).collect()).unwrap_or_default();
            day["new"] = json!(new);
            day["reviews"] = json!(reviews);
            let flags: Vec<&str> = rules.targets.iter().filter(|(_, date)| **date == d).map(|(t, _)| w.topic_name(t)).collect();
            day["flags"] = json!(flags);
            if d == today {
                // What has been done today, next to what is still planned.
                let solves = hist.solves.get(&d).map(|v| v.iter().map(|(i, g)| json!({ "problem": w.item_ref(*i), "grade": g })).collect::<Vec<_>>()).unwrap_or_default();
                day["done"] = json!({ "problems": solves, "reviews": hist.reviews.get(&d).copied().unwrap_or(0) });
            }
        } else {
            let solves = hist.solves.get(&d);
            let done_reviews = hist.reviews.get(&d).copied().unwrap_or(0);
            let past = if let Some(v) = solves {
                json!({ "state": "solved", "problems": v.iter().map(|(i, g)| json!({ "problem": w.item_ref(*i), "grade": g })).collect::<Vec<_>>(), "reviews": done_reviews })
            } else if kind == Kind::Break {
                json!({ "state": "break", "reviews": done_reviews })
            } else if kind == Kind::Solve && d >= tracking_start {
                json!({ "state": "missed", "reviews": done_reviews })
            } else if done_reviews > 0 {
                json!({ "state": "reviews", "reviews": done_reviews })
            } else {
                json!({ "state": "rest", "reviews": 0 })
            };
            day["past"] = past;
        }
        days.push(day);
        d = d + Days::new(1);
    }

    // summary
    let main = q.order.iter().filter(|i| !w.items[**i].practice).count();
    let weeks = q.finish.map_or(1.0, |f| ((f - today).num_days() as f64 / 7.0).max(1.0));
    let solve_days_28 = (0..28u64).filter(|n| w.routine.kind_of(today + Days::new(*n), ov) == Kind::Solve).count();
    let finish_by = rules.finish_by.or(w.settings.target_date);
    let summary = json!({
        "finish": q.finish, "baseline_finish": baseline.queue.finish, "finish_by": finish_by,
        "left": q.order.len(), "main": main, "practice": q.order.len() - main, "left_out_of_horizon": q.left,
        "per_week": q.order.len() as f64 / weeks, "solve_days_28": solve_days_28,
        "premium": rules.premium.unwrap_or(!w.settings.goal.free_only), "late": late_count(&planned),
    });

    // saved plans
    let plans: Vec<Value> = l
        .book
        .list
        .iter()
        .map(|p| {
            let o = l.overrides.get(&p.id).unwrap_or(&empty);
            let r = if p.id == active.id { None } else { Some(w.run(&p.rules, o)) };
            let r = r.as_ref().unwrap_or(&planned);
            json!({ "id": p.id, "name": p.name, "problems": r.queue.order.len(), "finish": r.queue.finish, "late": late_count(r), "rules": p.rules })
        })
        .collect();

    // the first late topic that has a target gets a suggestion
    let ctx = w.ctx(ov);
    let suggestion = q.topic_order.iter().find(|t| q.status.get(*t).is_some_and(|s| s.late > 0 && s.targeted) && rules.targets.contains_key(*t)).and_then(|t| {
        planner::what_it_takes(rules, &ctx, t).map(|r| {
            let r = match r {
                Remedy::AddDays { days, end } => json!({ "kind": "add_days", "days": days, "end": end }),
                Remedy::NotEnough { end } => json!({ "kind": "not_enough", "end": end }),
            };
            json!({ "topic": t, "name": w.topic_name(t), "late": q.status[t].late, "due": q.status[t].due, "remedy": r })
        })
    });

    // the last eight weeks
    let this_monday = today - Days::new(u64::from(today.weekday().num_days_from_monday()));
    let weeks_back: Vec<Value> = (0..8u64)
        .rev()
        .map(|k| {
            let start = this_monday - Days::new(7 * k);
            let (mut problems, mut revs) = (0usize, 0u32);
            for n in 0..7u64 {
                let day = start + Days::new(n);
                problems += hist.solves.get(&day).map_or(0, Vec::len);
                revs += hist.reviews.get(&day).copied().unwrap_or(0);
            }
            json!({ "start": start, "problems": problems, "reviews": revs })
        })
        .collect();
    let window_start = (today - Days::new(56)).max(tracking_start);
    let planned_days = window_start.iter_days().take_while(|d| *d < today).filter(|d| w.routine.kind_of(*d, ov) == Kind::Solve).count();
    let solved_days = window_start.iter_days().take_while(|d| *d < today).filter(|d| w.routine.kind_of(*d, ov) == Kind::Solve && hist.solves.contains_key(d)).count();
    let in_window: Vec<&(usize, String)> = hist.solves.range(window_start..today).flat_map(|(_, v)| v).collect();
    let clean = in_window.iter().filter(|(_, g)| g != "again").count();
    let mut streak = 0;
    let mut x = today - Days::new(1);
    while x >= tracking_start && !(w.routine.kind_of(x, ov) == Kind::Solve && !hist.solves.contains_key(&x)) {
        streak += 1;
        x = x - Days::new(1);
    }
    let mut mix: BTreeMap<&str, usize> = BTreeMap::new();
    for (i, _) in &in_window {
        *mix.entry(w.topic_name(&w.items[*i].topic)).or_default() += 1;
    }
    let history = json!({
        "weeks": weeks_back, "streak": streak, "problems": in_window.len(),
        "kept_percent": (planned_days > 0).then(|| solved_days * 100 / planned_days),
        "clean_percent": (!in_window.is_empty()).then(|| clean * 100 / in_window.len()),
        "mix": mix,
    });

    Ok(Json(json!({
        "today": today, "active": active.id, "rules": rules, "overrides": ov, "topics": topics, "days": days,
        "solve_days": w.routine.solve_days.iter().map(|d| d.num_days_from_monday()).collect::<Vec<_>>(),
        "summary": summary, "plans": plans, "suggestion": suggestion, "history": history,
        "companies": companies(&w), "free_only": w.settings.goal.free_only,
    })))
}

/// The companies in the catalog, the most-asked first, for the rules' company filter.
fn companies(w: &World<'_>) -> Vec<String> {
    let mut count: BTreeMap<&str, usize> = BTreeMap::new();
    for it in &w.items {
        for c in &it.companies {
            *count.entry(c.name.as_str()).or_default() += 1;
        }
    }
    let mut v: Vec<(&str, usize)> = count.into_iter().collect();
    v.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(b.0)));
    v.into_iter().take(24).map(|(n, _)| n.to_owned()).collect()
}

#[derive(Deserialize)]
pub struct PreviewBody {
    rules: Option<Rules>,
    /// Edits to lay over the saved calendar: a date to a kind, or `null` to clear it.
    #[serde(default)]
    overrides: BTreeMap<NaiveDate, Option<Kind>>,
}

pub async fn preview(State(s): State<AppState>, Json(body): Json<PreviewBody>) -> ApiResult<Json<Value>> {
    let today = crate::activity::today();
    let l = load(&s).await?;
    let w = World::build(&s.catalog, &l.settings, &l.progress, &l.rows, l.start.as_deref(), today);
    let empty = Overrides::new();
    let active = l.book.active_plan();
    let ov = l.overrides.get(&active.id).unwrap_or(&empty);
    let before = w.run(&active.rules, ov);
    let mut ov2 = ov.clone();
    for (d, k) in body.overrides {
        match k {
            Some(k) => ov2.insert(d, k),
            None => ov2.remove(&d),
        };
    }
    let rules = body.rules.unwrap_or_else(|| active.rules.clone());
    check_rules(&w, &rules)?;
    let after = w.run(&rules, &ov2);
    let solved_with_reviews: HashSet<usize> = w.with_reviews.clone();
    Ok(Json(json!({ "diff": planner::diff(&before, &after, &solved_with_reviews), "finish": after.queue.finish, "late": late_count(&after), "left": after.queue.order.len() })))
}

fn check_rules(w: &World<'_>, r: &Rules) -> ApiResult<()> {
    let known = |t: &String| w.topics.iter().any(|(c, _)| c == t);
    if r.difficulty.is_empty() {
        return Err(ApiError::BadRequest("pick at least one difficulty".into()));
    }
    if let Some(bad) = r.targets.keys().find(|t| !known(t)) {
        return Err(ApiError::BadRequest(format!("{bad} isn't a topic")));
    }
    if let Some(topics) = &r.topics
        && let Some(bad) = topics.iter().find(|t| !known(t))
    {
        return Err(ApiError::BadRequest(format!("{bad} isn't a topic")));
    }
    if let Some(order) = &r.topic_order {
        let mut a: Vec<&String> = order.iter().collect();
        let mut b: Vec<&String> = w.topics.iter().map(|(c, _)| c).collect();
        a.sort();
        b.sort();
        if a != b {
            return Err(ApiError::BadRequest("the topic order must list every topic once".into()));
        }
    }
    if r.companies.len() > 30 || r.companies.iter().any(|c| c.len() > 60) {
        return Err(ApiError::BadRequest("too many companies".into()));
    }
    Ok(())
}

#[derive(Deserialize)]
pub struct StateBody {
    rules: Rules,
    overrides: Overrides,
}

pub async fn put_state(State(s): State<AppState>, Json(body): Json<StateBody>) -> ApiResult<Json<Value>> {
    let today = crate::activity::today();
    let l = load(&s).await?;
    let w = World::build(&s.catalog, &l.settings, &l.progress, &l.rows, l.start.as_deref(), today);
    check_rules(&w, &body.rules)?;
    if body.overrides.keys().any(|d| (*d - today).num_days().abs() > 800) {
        return Err(ApiError::BadRequest("calendar edits are limited to about two years either side".into()));
    }
    let mut book = l.book;
    let id = book.active_plan().id.clone();
    if let Some(p) = book.list.iter_mut().find(|p| p.id == id) {
        p.rules = body.rules;
    }
    save_book(&s, &book).await?;
    replace_overrides(&s, &id, &body.overrides).await?;
    Ok(Json(json!({ "saved": true })))
}

async fn replace_overrides(s: &AppState, plan: &str, ov: &Overrides) -> ApiResult<()> {
    let mut tx = s.db.begin().await?;
    sqlx::query("DELETE FROM plan_overrides WHERE plan_id = $1").bind(plan).execute(&mut *tx).await?;
    for (day, kind) in ov {
        sqlx::query("INSERT INTO plan_overrides (plan_id, day, kind) VALUES ($1, $2, $3)").bind(plan).bind(day).bind(kind_text(*kind)).execute(&mut *tx).await?;
    }
    tx.commit().await?;
    Ok(())
}

#[derive(Deserialize)]
pub struct NewPlan {
    name: String,
}

/// Saves the active plan's rules and calendar edits as a new plan, and switches to it.
pub async fn create(State(s): State<AppState>, Json(body): Json<NewPlan>) -> ApiResult<Json<Value>> {
    let name = body.name.trim();
    if name.is_empty() || name.chars().count() > 60 {
        return Err(ApiError::BadRequest("give the plan a name of up to 60 characters".into()));
    }
    let mut book = book(&s.db).await?;
    if book.list.len() >= 12 {
        return Err(ApiError::BadRequest("keep at most 12 plans; delete one first".into()));
    }
    let from = book.active_plan().clone();
    let ov = all_overrides(&s.db).await?.remove(&from.id).unwrap_or_default();
    let mut id = format!("p{}", chrono::Utc::now().timestamp_millis());
    while book.list.iter().any(|p| p.id == id) {
        id.push('x');
    }
    book.list.push(SavedPlan { id: id.clone(), name: name.to_owned(), rules: from.rules });
    book.active = id.clone();
    save_book(&s, &book).await?;
    replace_overrides(&s, &id, &ov).await?;
    Ok(Json(json!({ "id": id })))
}

#[derive(Deserialize)]
pub struct CompanyPlan {
    company: String,
    /// The interview day; everything in scope is planned to be done by then.
    interview: Option<NaiveDate>,
    /// Only what the company asked in the last six months.
    #[serde(default = "yes")]
    recent_only: bool,
}

fn yes() -> bool {
    true
}

/// `POST /api/plans/company`: a plan for one company's interview, made fresh (the main plan and its calendar edits are not touched) and
/// switched to, so Next up follows it. It takes every problem the company asks, most-asked first with the last six months counting
/// double, and is paced to the interview day. Asking again for the same company updates that plan instead of making a second one.
pub async fn company_plan(State(s): State<AppState>, Json(body): Json<CompanyPlan>) -> ApiResult<Json<Value>> {
    if !anneal_content::COMPANIES.iter().any(|(name, _)| *name == body.company) {
        return Err(ApiError::BadRequest(format!("{} is not a company anneal knows", body.company)));
    }
    if body.interview.is_some_and(|d| d <= crate::activity::today()) {
        return Err(ApiError::BadRequest("the interview day must be in the future".into()));
    }
    let rules = Rules { companies: vec![body.company.clone()], recent_only: body.recent_only, order: Order::Company, practice: Practice::All, finish_by: body.interview, ..Rules::default() };
    let name = format!("Interview: {}", body.company);
    let mut book = book(&s.db).await?;
    let (id, created) = match book.list.iter_mut().find(|p| p.name == name) {
        Some(existing) => {
            existing.rules = rules;
            (existing.id.clone(), false)
        }
        None => {
            if book.list.len() >= 12 {
                return Err(ApiError::BadRequest("keep at most 12 plans; delete one first".into()));
            }
            let mut id = format!("p{}", chrono::Utc::now().timestamp_millis());
            while book.list.iter().any(|p| p.id == id) {
                id.push('x');
            }
            book.list.push(SavedPlan { id: id.clone(), name, rules });
            (id, true)
        }
    };
    book.active = id.clone();
    save_book(&s, &book).await?;
    Ok(Json(json!({ "id": id, "created": created })))
}

#[derive(Deserialize)]
pub struct Active {
    id: String,
}

pub async fn set_active(State(s): State<AppState>, Json(body): Json<Active>) -> ApiResult<Json<Value>> {
    let mut book = book(&s.db).await?;
    if !book.list.iter().any(|p| p.id == body.id) {
        return Err(ApiError::NotFound(format!("plan {}", body.id)));
    }
    book.active = body.id.clone();
    save_book(&s, &book).await?;
    Ok(Json(json!({ "active": body.id })))
}

pub async fn delete(State(s): State<AppState>, Path(id): Path<String>) -> ApiResult<Json<Value>> {
    let mut book = book(&s.db).await?;
    if id == "default" {
        return Err(ApiError::BadRequest("the default plan can't be deleted".into()));
    }
    if !book.list.iter().any(|p| p.id == id) {
        return Err(ApiError::NotFound(format!("plan {id}")));
    }
    let mut tx = s.db.begin().await?;
    if let Some(plan) = book.list.iter().find(|p| p.id == id) {
        sqlx::query("INSERT INTO data_archive (source, table_name, rows) VALUES ('delete_plan', 'plans', $1)").bind(sqlx::types::Json(serde_json::json!([plan]))).execute(&mut *tx).await?;
    }
    crate::store::archive_rows(&mut tx, "delete_plan", "plan_overrides", "plan_id", std::slice::from_ref(&id)).await?;
    tx.commit().await?;
    book.list.retain(|p| p.id != id);
    if book.active == id {
        book.active = "default".into();
    }
    save_book(&s, &book).await?;
    sqlx::query("DELETE FROM plan_overrides WHERE plan_id = $1").bind(&id).execute(&s.db).await?;
    Ok(Json(json!({ "deleted": id })))
}

