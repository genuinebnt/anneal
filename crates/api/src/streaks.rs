//! Streaks, perfect streaks and freezes (docs/STREAKS.md).
//!
//! The rules are plain functions of facts about each day, so they can be tested without a database: [`Facts`] is what happened and what
//! was planned, [`streak`], [`perfect`], [`longest`], [`balance`] and [`check_mark`] answer from it. The database part (`load`, `snapshot`,
//! `record_event` and the three routes) only gathers the facts and stores the marks.
//!
//! * **Streak**: walking back from today, a day with a solve adds one; a planned solve day with none breaks it (today never does, it is
//!   not over); a day with nothing planned, a holiday or a sick day neither adds nor breaks; a frozen day adds one. This is the streak the
//!   app already had, now with marks.
//! * **Perfect streak**: a day counts only if that day's planned problems and planned reviews were all done. A freeze never makes a day
//!   perfect, so a frozen day ends it; a missed day ends it. Holiday and sick days are neutral (they credit nothing).
//! * **Freezes**: no cap. Each new problem beyond the day's plan earns 1, each review done ahead of its due date earns half, but only
//!   once in a row per problem, so repeating reviews earns nothing. A freeze is spent on a missed planned day, any later day.

use std::collections::{BTreeMap, HashMap};

use axum::Json;
use axum::extract::{Path, State};
use chrono::{Datelike, Days, Local, NaiveDate};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;

use crate::error::{ApiError, ApiResult};
use crate::AppState;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MarkKind {
    Freeze,
    Holiday,
    Sick,
}

impl MarkKind {
    pub fn as_str(self) -> &'static str {
        match self {
            MarkKind::Freeze => "freeze",
            MarkKind::Holiday => "holiday",
            MarkKind::Sick => "sick",
        }
    }

    fn parse(s: &str) -> Option<MarkKind> {
        Some(match s {
            "freeze" => MarkKind::Freeze,
            "holiday" => MarkKind::Holiday,
            "sick" => MarkKind::Sick,
            _ => return None,
        })
    }
}

/// What happened on one day and what was planned for it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DayFacts {
    /// Something was solved (any problem, premium or not).
    pub solved: bool,
    /// The weekly routine says new problems are planned this day.
    pub planned_solve: bool,
    /// The plan of the day was saved (days before streaks existed have none, so they can never be perfect).
    pub has_plan: bool,
    pub planned_new: u32,
    pub planned_reviews: u32,
    /// New problems solved for the first time, and reviews done on or after their due date.
    pub done_new: u32,
    pub done_reviews: u32,
    /// Reviews done ahead of schedule that earn half a freeze each.
    pub ahead: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Effect {
    Add,
    Neutral,
    Break,
}

/// All the facts of the days since the first solve.
#[derive(Clone, Debug, Default)]
pub struct Facts {
    pub days: BTreeMap<NaiveDate, DayFacts>,
    pub marks: HashMap<NaiveDate, MarkKind>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DayState {
    Perfect,
    Kept,
    Frozen,
    Holiday,
    Sick,
    Missed,
    /// Today, not yet complete.
    Open,
    /// A day with nothing planned.
    Free,
}

/// The effect of a day on the streak and on the perfect streak.
fn effects(d: &DayFacts, mark: Option<MarkKind>, is_today: bool) -> (Effect, Effect) {
    match mark {
        Some(MarkKind::Freeze) => return (Effect::Add, Effect::Break),
        Some(MarkKind::Holiday | MarkKind::Sick) => return (Effect::Neutral, Effect::Neutral),
        None => {}
    }
    let plan_met = d.has_plan && d.done_new >= d.planned_new && d.done_reviews >= d.planned_reviews;
    if d.solved {
        return (Effect::Add, if plan_met { Effect::Add } else if is_today { Effect::Neutral } else { Effect::Break });
    }
    let streak = if is_today || !d.planned_solve { Effect::Neutral } else { Effect::Break };
    let perfect = if d.has_plan && d.planned_reviews > 0 && d.planned_new == 0 {
        // a day with only reviews planned (a practice day): doing them is a perfect day
        if d.done_reviews >= d.planned_reviews { Effect::Add } else if is_today { Effect::Neutral } else { Effect::Break }
    } else if is_today || !d.planned_solve {
        Effect::Neutral
    } else {
        Effect::Break
    };
    (streak, perfect)
}

pub fn state_of(d: &DayFacts, mark: Option<MarkKind>, is_today: bool) -> DayState {
    match mark {
        Some(MarkKind::Freeze) => return DayState::Frozen,
        Some(MarkKind::Holiday) => return DayState::Holiday,
        Some(MarkKind::Sick) => return DayState::Sick,
        None => {}
    }
    let (s, p) = effects(d, None, is_today);
    if d.solved {
        return if p == Effect::Add { DayState::Perfect } else { DayState::Kept };
    }
    if is_today {
        return DayState::Open;
    }
    match (s, p) {
        (Effect::Break, _) => DayState::Missed,
        (_, Effect::Add) => DayState::Perfect,
        _ => DayState::Free,
    }
}

impl Facts {
    fn effect(&self, day: NaiveDate, today: NaiveDate) -> Option<(Effect, Effect)> {
        self.days.get(&day).map(|d| effects(d, self.marks.get(&day).copied(), day == today))
    }

    fn walk_back(&self, today: NaiveDate, perfect: bool) -> u32 {
        let (mut day, mut n) = (today, 0);
        while let Some((s, p)) = self.effect(day, today) {
            match if perfect { p } else { s } {
                Effect::Add => n += 1,
                Effect::Break => break,
                Effect::Neutral => {}
            }
            let Some(prev) = day.checked_sub_days(Days::new(1)) else { break };
            day = prev;
        }
        n
    }
}

/// The streak now: the plan-aware streak with frozen days counted and holidays and sick days skipped.
pub fn streak(f: &Facts, today: NaiveDate) -> u32 {
    f.walk_back(today, false)
}

/// The perfect streak now.
pub fn perfect(f: &Facts, today: NaiveDate) -> u32 {
    f.walk_back(today, true)
}

/// The best streak ever reached, with the marks as they stand.
pub fn longest(f: &Facts, today: NaiveDate) -> u32 {
    let (mut run, mut best) = (0, 0);
    for (&day, d) in &f.days {
        match effects(d, f.marks.get(&day).copied(), day == today).0 {
            Effect::Add => {
                run += 1;
                best = best.max(run);
            }
            Effect::Break => run = 0,
            Effect::Neutral => {}
        }
    }
    best
}

/// What a day earns toward freezes: one for each new problem beyond the plan, half for each credited review done ahead.
pub fn earned_on(d: &DayFacts, mark: Option<MarkKind>) -> f64 {
    if mark.is_some() {
        return 0.0;
    }
    let extra_new = d.done_new.saturating_sub(d.planned_new);
    f64::from(extra_new) + 0.5 * f64::from(d.ahead)
}

/// Freezes earned so far, freezes spent, and what is left.
pub fn balance(f: &Facts) -> (f64, u32, f64) {
    let earned: f64 = f.days.iter().map(|(day, d)| earned_on(d, f.marks.get(day).copied())).sum();
    let spent = f.marks.values().filter(|k| **k == MarkKind::Freeze).count() as u32;
    (earned, spent, (earned - f64::from(spent)).max(0.0))
}

/// The streak the days on either side of `day` would join into if it were kept: how many before it and how many after.
pub fn join(f: &Facts, day: NaiveDate, today: NaiveDate) -> (u32, u32) {
    let mut before = 0;
    let mut d = day;
    while let Some(prev) = d.checked_sub_days(Days::new(1)) {
        match f.effect(prev, today) {
            Some((Effect::Add, _)) => before += 1,
            Some((Effect::Neutral, _)) => {}
            _ => break,
        }
        d = prev;
    }
    let (mut after, mut d) = (0, day);
    while let Some(next) = d.checked_add_days(Days::new(1)) {
        if next > today {
            break;
        }
        match f.effect(next, today) {
            Some((Effect::Add, _)) => after += 1,
            Some((Effect::Neutral, _)) => {}
            _ => break,
        }
        d = next;
    }
    (before, after)
}

/// The missed planned days that could still be repaired, newest first.
pub fn missed(f: &Facts, today: NaiveDate) -> Vec<NaiveDate> {
    let mut out: Vec<NaiveDate> = f.days.iter().filter(|(day, d)| state_of(d, f.marks.get(day).copied(), **day == today) == DayState::Missed).map(|(day, _)| *day).collect();
    out.reverse();
    out
}

/// Whether a mark may be made: a freeze only on a missed planned day before today and only with a freeze to spend; a holiday or sick day
/// only for today or yesterday and only for a day with nothing solved.
pub fn check_mark(f: &Facts, day: NaiveDate, kind: MarkKind, today: NaiveDate) -> Result<(), String> {
    if day > today {
        return Err("that day hasn't happened yet".into());
    }
    if f.marks.contains_key(&day) {
        return Err("that day is already marked: undo the mark first".into());
    }
    let Some(d) = f.days.get(&day) else { return Err("that day is before your first solve".into()) };
    match kind {
        MarkKind::Freeze => {
            if day == today {
                return Err("today isn't over: a freeze is for a day that was missed".into());
            }
            if state_of(d, None, false) != DayState::Missed {
                return Err("a freeze is only for a planned day that was missed".into());
            }
            let (_, _, left) = balance(f);
            if left < 1.0 {
                return Err(format!("a freeze is needed and you have {left:.1}: do an extra problem to earn one"));
            }
        }
        MarkKind::Holiday | MarkKind::Sick => {
            let yesterday = today.checked_sub_days(Days::new(1)).unwrap_or(today);
            if day != today && day != yesterday {
                return Err("a holiday or a sick day can be marked for today or yesterday only".into());
            }
            if d.solved {
                return Err("you solved something that day, so there is nothing to excuse".into());
            }
        }
    }
    Ok(())
}

// ---------------------------------------------------------------- the database part

/// Saves what is planned for `day` the first time it is asked (later calls change nothing). `planned_reviews` is what is still to
/// review; the reviews already done today are added, so opening the app after some reviews does not shrink the plan.
pub async fn snapshot(db: &PgPool, day: NaiveDate, planned_new: u32, planned_reviews: u32) -> sqlx::Result<()> {
    let done: i64 = sqlx::query_scalar("SELECT count(*) FROM review_events WHERE day = $1 AND voided_at IS NULL AND NOT is_new AND NOT ahead").bind(day).fetch_one(db).await?;
    let planned_reviews = planned_reviews + done as u32;
    sqlx::query("INSERT INTO day_plan (day, planned_new, planned_reviews) VALUES ($1, $2, $3) ON CONFLICT (day) DO NOTHING")
        .bind(day)
        .bind(planned_new as i32)
        .bind(planned_reviews as i32)
        .execute(db)
        .await?;
    Ok(())
}

/// Records a logged first solve or review. `due_before` is the review's due date before this log (none for a first solve).
/// A review ahead of schedule is credited unless the problem's previous log was also ahead, so repeating a problem earns nothing.
pub async fn record_event(db: &PgPool, problem_id: &str, is_new: bool, solved: bool, due_before: Option<chrono::DateTime<chrono::Utc>>) -> sqlx::Result<()> {
    // `ahead` is true only for a review logged before its due moment (reviews are due at 06:00 of their day).
    let ahead = due_before.is_some_and(|d| d > chrono::Utc::now());
    let previous_ahead: bool = sqlx::query_scalar("SELECT COALESCE((SELECT ahead FROM review_events WHERE problem_id = $1 AND voided_at IS NULL ORDER BY id DESC LIMIT 1), false)")
        .bind(problem_id)
        .fetch_one(db)
        .await?;
    sqlx::query("INSERT INTO review_events (problem_id, day, is_new, solved, ahead, credited) VALUES ($1, $2, $3, $4, $5, $6)")
        .bind(problem_id)
        .bind(crate::activity::today())
        .bind(is_new)
        .bind(solved)
        .bind(ahead)
        .bind(ahead && !previous_ahead)
        .execute(db)
        .await?;
    Ok(())
}

/// An undo of a problem's latest log voids that log's event (the row stays). Only today's event: an older log, from before events
/// were kept or from an earlier day, has nothing to void here.
pub async fn void_last_event(db: &PgPool, problem_id: &str) -> sqlx::Result<()> {
    sqlx::query("UPDATE review_events SET voided_at = now() WHERE id = (SELECT id FROM review_events WHERE problem_id = $1 AND voided_at IS NULL AND day = $2 ORDER BY id DESC LIMIT 1)")
        .bind(problem_id)
        .bind(crate::activity::today())
        .execute(db)
        .await?;
    Ok(())
}

/// The live marks, for the streak numbers elsewhere in the app.
pub async fn marks(db: &PgPool) -> sqlx::Result<HashMap<NaiveDate, MarkKind>> {
    let rows: Vec<(NaiveDate, String)> = sqlx::query_as("SELECT day, kind FROM streak_marks WHERE voided_at IS NULL").fetch_all(db).await?;
    Ok(rows.into_iter().filter_map(|(d, k)| Some((d, MarkKind::parse(&k)?))).collect())
}

/// The facts as the app stands today: the settings and the active plan decide which days are solve days.
pub async fn facts(db: &PgPool, today: NaiveDate) -> sqlx::Result<Facts> {
    let settings = crate::settings::srs(db).await?;
    let raw = fetch(db).await?;
    // The closure is not `Sync`, so it is made only after the last await.
    let is_solve_day = crate::calendar::solve_day_fn(db).await?;
    Ok(assemble(raw, &settings, &is_solve_day, today))
}

/// What the database holds, before it is laid out day by day.
pub struct Raw {
    solve_days: std::collections::BTreeSet<NaiveDate>,
    plans: HashMap<NaiveDate, (u32, u32)>,
    per: HashMap<NaiveDate, (u32, u32, u32)>,
    marks: HashMap<NaiveDate, MarkKind>,
}

async fn fetch(db: &PgPool) -> sqlx::Result<Raw> {
    let solved: Vec<chrono::DateTime<chrono::Utc>> = sqlx::query_scalar("SELECT solved_at FROM attempts WHERE solved_at IS NOT NULL").fetch_all(db).await?;
    let solve_days = solved.iter().map(|t| t.with_timezone(&Local).date_naive()).collect();
    let plans: Vec<(NaiveDate, i32, i32)> = sqlx::query_as("SELECT day, planned_new, planned_reviews FROM day_plan").fetch_all(db).await?;
    let plans = plans.into_iter().map(|(d, n, r)| (d, (n.max(0) as u32, r.max(0) as u32))).collect();
    let events: Vec<(NaiveDate, bool, bool, bool, bool)> = sqlx::query_as("SELECT day, is_new, solved, ahead, credited FROM review_events WHERE voided_at IS NULL").fetch_all(db).await?;
    // per day: new problems solved, reviews done on time, reviews credited ahead of schedule
    let mut per: HashMap<NaiveDate, (u32, u32, u32)> = HashMap::new();
    for (day, is_new, was_solved, ahead, credited) in events {
        let e = per.entry(day).or_default();
        if is_new && was_solved {
            e.0 += 1;
        } else if !is_new && !ahead {
            e.1 += 1;
        }
        if credited {
            e.2 += 1;
        }
    }
    Ok(Raw { solve_days, plans, per, marks: marks(db).await? })
}

/// Lays the facts out day by day from the first solve to today.
pub fn assemble(raw: Raw, settings: &crate::reviews::Settings, is_solve_day: &dyn Fn(NaiveDate) -> bool, today: NaiveDate) -> Facts {
    let Some(&first) = raw.solve_days.iter().next() else { return Facts { days: BTreeMap::new(), marks: raw.marks } };
    let mut days = BTreeMap::new();
    let mut day = first;
    while day <= today {
        let (done_new, done_reviews, ahead) = raw.per.get(&day).copied().unwrap_or_default();
        let planned_solve = is_solve_day(day);
        let plan = raw.plans.get(&day).copied();
        days.insert(
            day,
            DayFacts {
                solved: raw.solve_days.contains(&day),
                planned_solve,
                has_plan: plan.is_some(),
                planned_new: plan.map_or(if planned_solve { settings.new_on_weekday(day.weekday()) } else { 0 }, |p| p.0),
                planned_reviews: plan.map_or(0, |p| p.1),
                done_new,
                done_reviews,
                ahead,
            },
        );
        day = day + Days::new(1);
    }
    Facts { days, marks: raw.marks }
}

#[derive(Serialize)]
struct DayOut {
    date: NaiveDate,
    state: DayState,
    planned_new: u32,
    planned_reviews: u32,
    done_new: u32,
    done_reviews: u32,
    earned: f64,
    mark_id: Option<i64>,
}

#[derive(Serialize)]
struct Repair {
    date: NaiveDate,
    before: u32,
    after: u32,
}

/// `GET /api/streaks`: the numbers, the last 60 days with their states, and the missed days that can still be repaired.
pub async fn get(State(s): State<AppState>) -> ApiResult<Json<serde_json::Value>> {
    Ok(Json(view(&s.db, crate::activity::today()).await?))
}

async fn view(db: &PgPool, today: NaiveDate) -> sqlx::Result<serde_json::Value> {
    let f = facts(db, today).await?;
    let ids: Vec<(NaiveDate, i64)> = sqlx::query_as("SELECT day, id FROM streak_marks WHERE voided_at IS NULL").fetch_all(db).await?;
    let ids: HashMap<NaiveDate, i64> = ids.into_iter().collect();
    let (earned, spent, left) = balance(&f);
    let from = today - Days::new(59);
    let days: Vec<DayOut> = f
        .days
        .iter()
        .filter(|(d, _)| **d >= from)
        .map(|(&date, d)| DayOut {
            date,
            state: state_of(d, f.marks.get(&date).copied(), date == today),
            planned_new: d.planned_new,
            planned_reviews: d.planned_reviews,
            done_new: d.done_new,
            done_reviews: d.done_reviews,
            earned: earned_on(d, f.marks.get(&date).copied()),
            mark_id: ids.get(&date).copied(),
        })
        .collect();
    let repairs: Vec<Repair> = missed(&f, today)
        .into_iter()
        .take(10)
        .map(|date| {
            let (before, after) = join(&f, date, today);
            Repair { date, before, after }
        })
        .collect();
    Ok(serde_json::json!({
        "today": today, "streak": streak(&f, today), "perfect": perfect(&f, today), "longest": longest(&f, today),
        "freezes": left, "earned": earned, "spent": spent, "days": days, "repairs": repairs,
    }))
}

#[derive(Deserialize)]
pub struct NewMark {
    day: NaiveDate,
    kind: String,
}

/// `POST /api/streaks/marks`: a freeze on a missed day, or a holiday or sick day for today or yesterday.
pub async fn post_mark(State(s): State<AppState>, Json(body): Json<NewMark>) -> ApiResult<Json<serde_json::Value>> {
    let kind = MarkKind::parse(&body.kind).ok_or_else(|| ApiError::BadRequest("kind is freeze, holiday or sick".into()))?;
    let today = crate::activity::today();
    let f = facts(&s.db, today).await?;
    check_mark(&f, body.day, kind, today).map_err(ApiError::BadRequest)?;
    sqlx::query("INSERT INTO streak_marks (day, kind) VALUES ($1, $2)").bind(body.day).bind(kind.as_str()).execute(&s.db).await?;
    Ok(Json(view(&s.db, today).await?))
}

/// `DELETE /api/streaks/marks/{id}`: undoes a mark (it is voided, not deleted, and a freeze comes back).
pub async fn delete_mark(State(s): State<AppState>, Path(id): Path<i64>) -> ApiResult<Json<serde_json::Value>> {
    let n = sqlx::query("UPDATE streak_marks SET voided_at = now() WHERE id = $1 AND voided_at IS NULL").bind(id).execute(&s.db).await?.rows_affected();
    if n == 0 {
        return Err(ApiError::NotFound(format!("mark {id}")));
    }
    Ok(Json(view(&s.db, crate::activity::today()).await?))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(s: &str) -> NaiveDate {
        s.parse().unwrap()
    }

    /// Days from `from`, each described by (solved, new done, planned new, reviews done, planned reviews, ahead credited); all planned solve days with a plan.
    fn facts(from: &str, rows: &[(bool, u32, u32, u32, u32, u32)]) -> Facts {
        let mut days = BTreeMap::new();
        let mut day = d(from);
        for &(solved, done_new, planned_new, done_reviews, planned_reviews, ahead) in rows {
            days.insert(day, DayFacts { solved, planned_solve: true, has_plan: true, planned_new, planned_reviews, done_new, done_reviews, ahead });
            day = day + Days::new(1);
        }
        Facts { days, marks: HashMap::new() }
    }
    const PERFECT: (bool, u32, u32, u32, u32, u32) = (true, 1, 1, 2, 2, 0);
    const KEPT: (bool, u32, u32, u32, u32, u32) = (true, 1, 1, 0, 2, 0);
    const MISSED: (bool, u32, u32, u32, u32, u32) = (false, 0, 1, 0, 2, 0);

    #[test]
    fn a_kept_day_counts_and_a_perfect_day_needs_the_whole_plan() {
        let f = facts("2026-10-01", &[PERFECT, KEPT, PERFECT]);
        assert_eq!(streak(&f, d("2026-10-03")), 3);
        assert_eq!(perfect(&f, d("2026-10-03")), 1, "the middle day skipped its reviews, so only today is perfect");
        let g = facts("2026-10-01", &[PERFECT, PERFECT, PERFECT]);
        assert_eq!(perfect(&g, d("2026-10-03")), 3);
        assert_eq!(state_of(&g.days[&d("2026-10-02")], None, false), DayState::Perfect);
        assert_eq!(state_of(&f.days[&d("2026-10-02")], None, false), DayState::Kept);
    }

    #[test]
    fn a_missed_planned_day_breaks_both_streaks_and_today_never_does() {
        let f = facts("2026-10-01", &[PERFECT, PERFECT, MISSED, PERFECT]);
        assert_eq!(streak(&f, d("2026-10-04")), 1);
        assert_eq!(perfect(&f, d("2026-10-04")), 1);
        let today_open = facts("2026-10-01", &[PERFECT, PERFECT, MISSED]);
        assert_eq!(streak(&today_open, d("2026-10-03")), 2, "the missed day is today, still open: the earlier days count");
        assert_eq!(state_of(&today_open.days[&d("2026-10-03")], None, true), DayState::Open);
        let yesterday = facts("2026-10-01", &[PERFECT, PERFECT, MISSED, (false, 0, 1, 0, 2, 0)]);
        assert_eq!(streak(&yesterday, d("2026-10-04")), 0, "yesterday is missed, so the streak ended there");
    }

    #[test]
    fn a_freeze_joins_the_days_either_side_but_never_makes_a_day_perfect() {
        let mut f = facts("2026-10-01", &[PERFECT, PERFECT, PERFECT, MISSED, PERFECT, PERFECT]);
        let today = d("2026-10-06");
        assert_eq!(streak(&f, today), 2);
        assert_eq!(join(&f, d("2026-10-04"), today), (3, 2), "3 before and 2 after");
        f.marks.insert(d("2026-10-04"), MarkKind::Freeze);
        assert_eq!(streak(&f, today), 6, "3 + the frozen day + 2");
        assert_eq!(perfect(&f, today), 2, "a freeze is no shortcut: the perfect streak still ends at the frozen day");
        assert_eq!(state_of(&f.days[&d("2026-10-04")], f.marks.get(&d("2026-10-04")).copied(), false), DayState::Frozen);
        assert_eq!(longest(&f, today), 6);
    }

    #[test]
    fn holiday_and_sick_days_are_neutral_for_both_streaks() {
        let mut f = facts("2026-10-01", &[PERFECT, PERFECT, MISSED, PERFECT]);
        f.marks.insert(d("2026-10-03"), MarkKind::Holiday);
        let today = d("2026-10-04");
        assert_eq!(streak(&f, today), 3, "it neither adds nor breaks");
        assert_eq!(perfect(&f, today), 3);
        f.marks.insert(d("2026-10-03"), MarkKind::Sick);
        assert_eq!(streak(&f, today), 3);
    }

    #[test]
    fn a_day_with_nothing_planned_is_neutral_and_a_practice_day_can_be_perfect() {
        let mut f = facts("2026-10-01", &[PERFECT, PERFECT]);
        f.days.insert(d("2026-10-03"), DayFacts { solved: false, planned_solve: false, has_plan: true, planned_new: 0, planned_reviews: 0, ..Default::default() });
        f.days.insert(d("2026-10-04"), DayFacts { solved: true, planned_solve: true, has_plan: true, planned_new: 1, planned_reviews: 0, done_new: 1, ..Default::default() });
        assert_eq!(streak(&f, d("2026-10-04")), 3, "the free day neither breaks nor adds");
        assert_eq!(perfect(&f, d("2026-10-04")), 3);
        f.days.insert(d("2026-10-03"), DayFacts { solved: false, planned_solve: false, has_plan: true, planned_new: 0, planned_reviews: 3, done_reviews: 3, ..Default::default() });
        assert_eq!(perfect(&f, d("2026-10-04")), 4, "a practice day with its reviews done is perfect");
        assert_eq!(streak(&f, d("2026-10-04")), 3, "but it does not add to the plain streak");
        f.days.insert(d("2026-10-03"), DayFacts { solved: false, planned_solve: false, has_plan: true, planned_new: 0, planned_reviews: 3, done_reviews: 1, ..Default::default() });
        assert_eq!(perfect(&f, d("2026-10-04")), 1, "reviews planned and not done end the perfect streak");
    }

    #[test]
    fn days_before_the_plan_was_saved_are_kept_or_missed_but_never_perfect() {
        let mut f = facts("2026-10-01", &[PERFECT, PERFECT]);
        f.days.get_mut(&d("2026-10-01")).unwrap().has_plan = false;
        assert_eq!(streak(&f, d("2026-10-02")), 2);
        assert_eq!(perfect(&f, d("2026-10-02")), 1);
        assert_eq!(state_of(&f.days[&d("2026-10-01")], None, false), DayState::Kept);
    }

    #[test]
    fn freezes_are_earned_one_per_extra_problem_and_half_per_credited_review_with_no_cap() {
        // planned 1 new and 2 reviews; did 3 new (2 extra), and 4 credited reviews ahead
        let f = facts("2026-10-01", &[(true, 3, 1, 2, 2, 4), (true, 1, 1, 2, 2, 0), (true, 0, 1, 0, 0, 1)]);
        let (earned, spent, left) = balance(&f);
        assert_eq!(earned, 2.0 + 2.0 + 0.0 + 0.5, "2 extra new + 4 halves, then 0, then one half");
        assert_eq!((spent, left), (0, 4.5));
        let big = facts("2026-10-01", &vec![(true, 6, 1, 2, 2, 0); 10]);
        assert_eq!(balance(&big).0, 50.0, "no cap");
        let none = facts("2026-10-01", &[PERFECT, KEPT]);
        assert_eq!(balance(&none).2, 0.0, "doing exactly the plan earns nothing");
    }

    #[test]
    fn a_freeze_needs_a_freeze_a_missed_planned_day_that_is_over() {
        let today = d("2026-10-05");
        let mut f = facts("2026-10-01", &[PERFECT, MISSED, PERFECT, PERFECT, PERFECT]);
        let miss = d("2026-10-02");
        let err = check_mark(&f, miss, MarkKind::Freeze, today).unwrap_err();
        assert!(err.contains("do an extra problem"), "{err}");
        f.days.get_mut(&d("2026-10-03")).unwrap().done_new = 2;
        assert!(check_mark(&f, miss, MarkKind::Freeze, today).is_ok(), "one extra new problem earned one freeze");
        f.marks.insert(miss, MarkKind::Freeze);
        assert_eq!(balance(&f).2, 0.0);
    }

    #[test]
    fn marks_follow_their_rules() {
        let today = d("2026-10-05");
        let mut f = facts("2026-10-01", &[PERFECT, MISSED, PERFECT, MISSED, (false, 0, 1, 0, 2, 0)]);
        f.days.get_mut(&d("2026-10-03")).unwrap().done_new = 3; // 2 extra: 2 freezes
        assert!(check_mark(&f, d("2026-10-02"), MarkKind::Freeze, today).is_ok());
        assert!(check_mark(&f, d("2026-10-03"), MarkKind::Freeze, today).unwrap_err().contains("missed"), "not a missed day");
        assert!(check_mark(&f, d("2026-10-05"), MarkKind::Freeze, today).unwrap_err().contains("today"), "today is not over");
        assert!(check_mark(&f, d("2026-10-06"), MarkKind::Holiday, today).is_err(), "the future");
        assert!(check_mark(&f, d("2026-10-04"), MarkKind::Holiday, today).is_ok(), "yesterday");
        assert!(check_mark(&f, d("2026-10-05"), MarkKind::Sick, today).is_ok(), "today");
        assert!(check_mark(&f, d("2026-10-02"), MarkKind::Sick, today).unwrap_err().contains("today or yesterday"));
        assert!(check_mark(&f, d("2026-10-03"), MarkKind::Holiday, today).is_err(), "a day you solved needs no excuse, and is not today or yesterday");
        f.marks.insert(d("2026-10-02"), MarkKind::Freeze);
        assert!(check_mark(&f, d("2026-10-02"), MarkKind::Holiday, today).unwrap_err().contains("already marked"));
        // spending the second freeze leaves none for a third
        f.marks.insert(d("2026-10-04"), MarkKind::Freeze);
        assert_eq!(balance(&f).2, 0.0);
    }

    #[test]
    fn the_streak_without_marks_is_the_one_the_app_already_had() {
        // the plain plan-aware streak: Sunday is not a solve day
        let not_sunday = |x: NaiveDate| x.weekday() != chrono::Weekday::Sun;
        let solve_days = [d("2026-10-22"), d("2026-10-23"), d("2026-10-24"), d("2026-10-26")];
        let mut days = BTreeMap::new();
        let mut day = d("2026-10-22");
        while day <= d("2026-10-26") {
            days.insert(day, DayFacts { solved: solve_days.contains(&day), planned_solve: not_sunday(day), has_plan: false, planned_new: 1, ..Default::default() });
            day = day + Days::new(1);
        }
        let f = Facts { days, marks: HashMap::new() };
        let today = d("2026-10-26");
        assert_eq!(streak(&f, today), crate::activity::streak_by_plan(&solve_days, today, &not_sunday));
        assert_eq!(streak(&f, today), 4);
    }

    #[test]
    fn repairing_later_fills_the_gap_and_undoing_gives_the_freeze_back() {
        let today = d("2026-10-10");
        let mut f = facts("2026-10-01", &[(true, 4, 1, 2, 2, 0), PERFECT, MISSED, PERFECT, PERFECT, PERFECT, PERFECT, PERFECT, PERFECT, PERFECT]);
        assert_eq!(missed(&f, today), vec![d("2026-10-03")]);
        assert_eq!(streak(&f, today), 7);
        f.marks.insert(d("2026-10-03"), MarkKind::Freeze);
        assert_eq!(streak(&f, today), 10);
        assert_eq!(balance(&f).2, 2.0, "3 earned on day one, 1 spent");
        f.marks.remove(&d("2026-10-03"));
        assert_eq!(streak(&f, today), 7);
        assert_eq!(balance(&f).2, 3.0);
    }
}
