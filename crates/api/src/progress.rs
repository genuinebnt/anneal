//! The Progress page: overview, Rust stats and the review queue, computed from attempts, runs,
//! reviews and focus time. Everything is derived on request; one user's history is small.

use std::collections::HashMap;

use anneal_content::{Band, Catalog, Mode, Section, Status, Track};
use anneal_runner::Level;
use chrono::{DateTime, Datelike, Days, Local, NaiveDate, Utc};
use serde::Serialize;

use crate::activity;
use crate::store::{ActivityRow, ProgressRow, ReviewRow, RunStat};
use crate::views;

/// Interview time budget in minutes, by mode and level (the workspace clock uses the same numbers).
pub fn budget(mode: Mode, level: Band) -> u32 {
    match (mode, level) {
        (Mode::Write, Band::Easy) => 10,
        (Mode::Write, Band::Medium) => 25,
        (Mode::Write, Band::Hard) => 40,
        (Mode::Fix, Band::Easy) => 5,
        (Mode::Fix, Band::Medium) => 10,
        (Mode::Fix, Band::Hard) => 15,
        (Mode::Stage, Band::Hard) => 90,
        (Mode::Stage, _) => 60,
    }
}

const AREAS: [(&str, &[Section]); 2] = [
    ("dsa", &[Section::Dsa]),
    ("rust", &[Section::Language, Section::StandardLibrary, Section::Concurrency, Section::Systems, Section::Performance]),
];

fn local_day(t: DateTime<Utc>) -> NaiveDate {
    t.with_timezone(&Local).date_naive()
}

fn monday_of(d: NaiveDate) -> NaiveDate {
    d - Days::new(u64::from(d.weekday().num_days_from_monday()))
}

fn ratio(n: usize, d: usize) -> Option<f64> {
    (d > 0).then(|| (1000.0 * n as f64 / d as f64).round() / 10.0)
}

fn median(mut v: Vec<f64>) -> Option<f64> {
    if v.is_empty() {
        return None;
    }
    v.sort_by(f64::total_cmp);
    let m = v.len() / 2;
    Some(if v.len() % 2 == 1 { v[m] } else { (v[m - 1] + v[m]) / 2.0 })
}

/// Readiness of a set of sections: tracks with written problems, core tracks counted twice.
fn area_readiness(catalog: &Catalog, sections: &[Section], progress: &HashMap<String, ProgressRow>, count_premium: bool) -> Option<f64> {
    let tracks: Vec<&Track> = catalog
        .tracks
        .iter()
        .filter(|t| sections.contains(&t.section) && t.problems.iter().any(|p| p.meta.status == Status::Ready))
        .collect();
    let weight = |t: &Track| if t.tier == anneal_content::Tier::Core { 2.0 } else { 1.0 };
    let total: f64 = tracks.iter().map(|t| weight(t)).sum();
    (total > 0.0).then(|| {
        let v: f64 = tracks.iter().map(|t| views::readiness(t, progress, count_premium) * weight(t)).sum::<f64>() / total;
        (v * 10.0).round() / 10.0
    })
}

// ---------------------------------------------------------------- overview

#[derive(Debug, Serialize)]
pub struct Overview {
    pub streak: u32,
    pub longest_streak: u32,
    /// Solves per day from `heat_start` (a Monday, 52 weeks back) to today.
    pub heat_start: NaiveDate,
    pub heat: Vec<u32>,
    pub solved_year: u32,
    pub active_days_year: u32,
    /// Percent of solves in the last 30 days that were unassisted, and the 30 days before.
    pub unassisted_30d: Option<f64>,
    pub unassisted_prev_30d: Option<f64>,
    pub focus_week_seconds: i64,
    pub focus_quarter_seconds: i64,
    pub weekly: Vec<WeekLevels>,
    pub readiness_trend: Vec<AreaTrend>,
    pub this_week: Vec<DayRow>,
    pub by_section: Vec<SectionRow>,
    /// How the DSA problems have been logged, separately for first tries, later tries and practice.
    pub record: SolveRecord,
}

/// How many logs went each way.
#[derive(Debug, Default, Serialize, PartialEq, Eq)]
pub struct Outcomes {
    pub own: u32,
    pub help: u32,
    pub not_yet: u32,
}

/// The DSA solve record: a problem's first log is a first try, the scheduled reviews are later tries, and a repeat done
/// on purpose (or any log of a LeetCode practice problem) is practice. Each has its own counts and they are never added
/// together; the practice count is the number of practice solves (on my own or with help).
#[derive(Debug, Default, Serialize, PartialEq, Eq)]
pub struct SolveRecord {
    pub first: Outcomes,
    pub later: Outcomes,
    pub practice: Outcomes,
    pub practice_count: u32,
    pub problems_practised: u32,
    pub practice_week: u32,
}

fn solve_record(catalog: &Catalog, rows: &[ActivityRow], today: NaiveDate) -> SolveRecord {
    let mut r = SolveRecord::default();
    let mut practised = std::collections::HashSet::new();
    for a in rows {
        let Some((_, p)) = catalog.problem(&a.problem_id) else { continue };
        if p.dsa.is_none() {
            continue;
        }
        let practice = a.kind == "extra" || crate::dsa::is_practice(p);
        let bucket = match (practice, a.kind.as_str()) {
            (true, _) => &mut r.practice,
            (false, "practice") => &mut r.first,
            (false, "resolve") => &mut r.later,
            _ => continue,
        };
        match (a.solved_at, a.assisted) {
            (None, _) => bucket.not_yet += 1,
            (Some(_), true) => bucket.help += 1,
            (Some(_), false) => bucket.own += 1,
        }
        if let (true, Some(at)) = (practice, a.solved_at) {
            r.practice_count += 1;
            practised.insert(a.problem_id.as_str());
            if local_day(at) + Days::new(6) >= today {
                r.practice_week += 1;
            }
        }
    }
    r.problems_practised = practised.len() as u32;
    r
}

#[derive(Debug, Serialize)]
pub struct WeekLevels {
    pub week_start: NaiveDate,
    pub easy: u32,
    pub medium: u32,
    pub hard: u32,
}

#[derive(Debug, Serialize)]
pub struct AreaTrend {
    pub area: &'static str,
    /// Readiness at the end of each of the last 12 weeks, oldest first; `None` before anything was written.
    pub points: Vec<Option<f64>>,
}

#[derive(Debug, Serialize)]
pub struct DayRow {
    pub date: NaiveDate,
    pub solved: u32,
    pub unassisted: u32,
    pub focus_seconds: i64,
}

#[derive(Debug, Serialize)]
pub struct SectionRow {
    pub section: Section,
    pub solved: usize,
    pub written: usize,
    pub readiness: Option<f64>,
}

/// Progress as it stood at the end of `day`: the latest solve on or before it, without decay.
fn progress_as_of(rows: &[ActivityRow], day: NaiveDate) -> HashMap<String, ProgressRow> {
    let mut best: HashMap<String, (DateTime<Utc>, bool)> = HashMap::new();
    for r in rows {
        // A repeat for practice never changes where a problem stands.
        let Some(at) = r.solved_at.filter(|_| r.kind != "extra") else { continue };
        if local_day(at) > day {
            continue;
        }
        let e = best.entry(r.problem_id.clone()).or_insert((at, r.assisted));
        if at > e.0 {
            *e = (at, r.assisted);
        }
    }
    best.into_iter()
        .map(|(id, (_, assisted))| (id, ProgressRow { solved: true, assisted, decay: 1.0 }))
        .collect()
}

pub fn overview(
    catalog: &Catalog,
    rows: &[ActivityRow],
    focus: &[(NaiveDate, String, i32)],
    progress: &HashMap<String, ProgressRow>,
    today: NaiveDate,
    count_premium: bool,
    is_solve_day: &dyn Fn(NaiveDate) -> bool,
) -> Overview {
    let solves: Vec<(NaiveDate, &ActivityRow)> = rows.iter().filter_map(|r| Some((local_day(r.solved_at?), r))).collect();
    let mut days: Vec<NaiveDate> = solves.iter().map(|(d, _)| *d).collect();
    days.sort_unstable();
    days.dedup();
    // The best streak is the best the plan-aware streak ever reached, measured at the end of each solve day.
    let longest_streak = days.iter().map(|&d| activity::streak_by_plan(&days, d, is_solve_day)).max().unwrap_or(0);

    let heat_start = monday_of(today) - Days::new(52 * 7);
    let len = (today - heat_start).num_days() as usize + 1;
    let mut heat = vec![0u32; len];
    for (d, _) in &solves {
        if *d >= heat_start && *d <= today {
            heat[(*d - heat_start).num_days() as usize] += 1;
        }
    }
    let year_ago = today - Days::new(365);
    let solved_year = solves.iter().filter(|(d, _)| *d > year_ago).count() as u32;
    let active_days_year = days.iter().filter(|d| **d > year_ago).count() as u32;

    let window = |from: u64, to: u64| {
        let (lo, hi) = (today - Days::new(to), today - Days::new(from));
        let in_window: Vec<_> = solves.iter().filter(|(d, _)| *d > lo && *d <= hi).collect();
        ratio(in_window.iter().filter(|(_, r)| !r.assisted).count(), in_window.len())
    };
    let focus_since = |days: u64| -> i64 {
        let from = today - Days::new(days);
        focus.iter().filter(|(d, _, _)| *d > from).map(|(_, _, s)| i64::from(*s)).sum()
    };

    let level = |id: &str| catalog.problem(id).map(|(_, p)| p.meta.level);
    let this_monday = monday_of(today);
    let weekly = (0..12u64)
        .map(|k| {
            let start = this_monday - Days::new(7 * (11 - k));
            let end = start + Days::new(6);
            let mut w = WeekLevels { week_start: start, easy: 0, medium: 0, hard: 0 };
            for (d, r) in &solves {
                if *d < start || *d > end {
                    continue;
                }
                match level(&r.problem_id) {
                    Some(Band::Easy) => w.easy += 1,
                    Some(Band::Medium) => w.medium += 1,
                    Some(Band::Hard) => w.hard += 1,
                    None => {}
                }
            }
            w
        })
        .collect();

    let week_ends: Vec<NaiveDate> = (0..12u64).map(|k| (this_monday - Days::new(7 * (11 - k)) + Days::new(6)).min(today)).collect();
    let snapshots: Vec<HashMap<String, ProgressRow>> = week_ends.iter().map(|d| progress_as_of(rows, *d)).collect();
    let readiness_trend = AREAS
        .iter()
        .map(|(area, sections)| AreaTrend { area, points: snapshots.iter().map(|p| area_readiness(catalog, sections, p, count_premium)).collect() })
        .collect();

    let this_week = (0..7u64)
        .map(|k| {
            let date = this_monday + Days::new(k);
            let day_solves: Vec<_> = solves.iter().filter(|(d, _)| *d == date).collect();
            DayRow {
                date,
                solved: day_solves.len() as u32,
                unassisted: day_solves.iter().filter(|(_, r)| !r.assisted).count() as u32,
                focus_seconds: focus.iter().filter(|(d, _, _)| *d == date).map(|(_, _, s)| i64::from(*s)).sum(),
            }
        })
        .collect();

    let mut sections: Vec<Section> = Vec::new();
    for t in &catalog.tracks {
        if !sections.contains(&t.section) {
            sections.push(t.section);
        }
    }
    let by_section = sections
        .into_iter()
        .map(|s| {
            let problems = catalog.tracks.iter().filter(|t| t.section == s).flat_map(|t| &t.problems).filter(|p| views::counts(p, count_premium));
            let ready: Vec<_> = problems.filter(|p| p.meta.status == Status::Ready).collect();
            SectionRow {
                section: s,
                solved: ready.iter().filter(|p| progress.get(&p.id).is_some_and(|r| r.solved)).count(),
                written: ready.len(),
                readiness: area_readiness(catalog, &[s], progress, count_premium),
            }
        })
        .collect();

    Overview {
        streak: activity::streak_by_plan(&days, today, is_solve_day),
        longest_streak,
        heat_start,
        heat,
        solved_year,
        active_days_year,
        unassisted_30d: window(0, 30),
        unassisted_prev_30d: window(30, 60),
        focus_week_seconds: focus_since(7),
        focus_quarter_seconds: focus_since(90),
        weekly,
        readiness_trend,
        this_week,
        by_section,
        record: solve_record(catalog, rows, today),
    }
}

// ---------------------------------------------------------------- Rust stats

#[derive(Debug, Serialize)]
pub struct Stats {
    /// Percent of attempted problems whose first run passed.
    pub first_run_pass: Option<f64>,
    pub runs_per_solve: Option<f64>,
    pub runs_per_solve_hard: Option<f64>,
    /// Percent of runs in the last 30 days that didn't compile.
    pub compile_error_rate: Option<f64>,
    /// Percent of unassisted solves with focus time recorded that stayed inside the interview budget.
    pub within_budget: Option<f64>,
    pub errors: Vec<Counted>,
    pub lints: Vec<Counted>,
    pub rules: Vec<Counted>,
    pub timing: Vec<TimingRow>,
}

#[derive(Debug, Serialize)]
pub struct Counted {
    /// `E0502`, `clippy::needless_range_loop`, or a rule like `clone`.
    pub key: String,
    /// rustc's message for errors; empty otherwise.
    pub message: String,
    pub count: usize,
    /// Where it happens most, e.g. "L2 · Split borrows".
    pub where_most: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct TimingRow {
    pub level: Band,
    pub mode: Mode,
    pub budget_minutes: u32,
    pub median_minutes: Option<f64>,
    pub within_budget: Option<f64>,
    pub solved: usize,
}

fn where_of(catalog: &Catalog, problem_ids: &[&str]) -> Option<String> {
    let mut counts: HashMap<String, usize> = HashMap::new();
    for id in problem_ids {
        if let Some((t, p)) = catalog.problem(id) {
            let stage = t.stages.iter().find(|s| s.slug == p.meta.stage).map_or("", |s| s.name.as_str());
            *counts.entry(format!("{} · {stage}", t.code)).or_default() += 1;
        }
    }
    counts.into_iter().max_by(|a, b| a.1.cmp(&b.1).then_with(|| b.0.cmp(&a.0))).map(|(k, _)| k)
}

fn top(mut m: HashMap<String, (String, Vec<&str>)>, catalog: &Catalog, n: usize) -> Vec<Counted> {
    let mut v: Vec<Counted> = m
        .drain()
        .map(|(key, (message, ids))| Counted { where_most: where_of(catalog, &ids), count: ids.len(), key, message })
        .collect();
    v.sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.key.cmp(&b.key)));
    v.truncate(n);
    v
}

pub fn stats(
    catalog: &Catalog,
    rows: &[ActivityRow],
    runs: &[RunStat],
    focus: &[(NaiveDate, String, i32)],
    now: DateTime<Utc>,
) -> Stats {
    let month_ago = now - chrono::TimeDelta::days(30);
    let recent: Vec<&RunStat> = runs.iter().filter(|r| r.created_at > month_ago).collect();

    let mut first: HashMap<&str, &RunStat> = HashMap::new();
    for r in runs {
        first.entry(r.problem_id.as_str()).or_insert(r);
    }
    let first_run_pass = ratio(first.values().filter(|r| r.status == "passed").count(), first.len());

    let level = |id: &str| catalog.problem(id).map(|(_, p)| (p.meta.mode, p.meta.level));
    let mut per_solve = Vec::new();
    let mut per_solve_hard = Vec::new();
    for a in rows {
        let Some(solved) = a.solved_at else { continue };
        let n = runs.iter().filter(|r| r.attempt_id == a.attempt_id && r.created_at <= solved).count() as f64;
        per_solve.push(n);
        if level(&a.problem_id).is_some_and(|(_, l)| l == Band::Hard) {
            per_solve_hard.push(n);
        }
    }
    let compile_error_rate = ratio(recent.iter().filter(|r| r.status == "compile_error").count(), recent.len());

    let mut errors: HashMap<String, (String, Vec<&str>)> = HashMap::new();
    let mut lints: HashMap<String, (String, Vec<&str>)> = HashMap::new();
    let mut rules: HashMap<String, (String, Vec<&str>)> = HashMap::new();
    for r in &recent {
        for d in r.diagnostics.iter() {
            let Some(code) = &d.code else { continue };
            if d.level == Level::Error && code.starts_with('E') {
                errors.entry(code.clone()).or_insert_with(|| (d.message.clone(), Vec::new())).1.push(&r.problem_id);
            } else if code.starts_with("clippy::") {
                lints.entry(code.clone()).or_insert_with(|| (String::new(), Vec::new())).1.push(&r.problem_id);
            }
        }
        for v in r.violations.iter() {
            rules.entry(v.rule.clone()).or_insert_with(|| (String::new(), Vec::new())).1.push(&r.problem_id);
        }
    }

    // Focus seconds per problem up to the (first) solve day.
    let focus_until = |id: &str, day: NaiveDate| -> i64 {
        focus.iter().filter(|(d, p, _)| p == id && *d <= day).map(|(_, _, s)| i64::from(*s)).sum()
    };
    let mut timing_samples: HashMap<(Mode, Band), (Vec<f64>, usize, usize)> = HashMap::new();
    let (mut within, mut timed) = (0, 0);
    for a in rows {
        let Some(solved) = a.solved_at else { continue };
        let Some((mode, lvl)) = level(&a.problem_id) else { continue };
        let entry = timing_samples.entry((mode, lvl)).or_default();
        entry.2 += 1;
        if a.assisted {
            continue;
        }
        let secs = focus_until(&a.problem_id, local_day(solved));
        if secs == 0 {
            continue;
        }
        let minutes = secs as f64 / 60.0;
        entry.0.push(minutes);
        timed += 1;
        if minutes <= f64::from(budget(mode, lvl)) {
            entry.1 += 1;
            within += 1;
        }
    }
    let mut timing: Vec<TimingRow> = timing_samples
        .into_iter()
        .map(|((mode, level), (mins, ok, solved))| TimingRow {
            level,
            mode,
            budget_minutes: budget(mode, level),
            within_budget: ratio(ok, mins.len()),
            median_minutes: median(mins).map(|m| (m * 10.0).round() / 10.0),
            solved,
        })
        .collect();
    timing.sort_by_key(|t| (t.level, t.mode == Mode::Fix));

    Stats {
        first_run_pass,
        runs_per_solve: median(per_solve),
        runs_per_solve_hard: median(per_solve_hard),
        compile_error_rate,
        within_budget: ratio(within, timed),
        errors: top(errors, catalog, 6),
        lints: top(lints, catalog, 5),
        rules: top(rules, catalog, 5),
        timing,
    }
}

// ---------------------------------------------------------------- reviews

#[derive(Debug, Serialize)]
pub struct Reviews {
    pub due_today: usize,
    pub overdue: usize,
    /// Rough minutes to clear today's queue, from the interview budgets.
    pub minutes_today: u32,
    /// Percent of re-solves in the last 30 days that were unassisted.
    pub retention_30d: Option<f64>,
    pub in_rotation: usize,
    pub graduated: usize,
    /// Due today (overdue included) and upcoming, soonest first.
    pub queue: Vec<ReviewItem>,
    /// Reviews falling due on each of the next 14 days; day 0 includes overdue ones.
    pub forecast: Vec<usize>,
}

#[derive(Debug, Serialize)]
pub struct ReviewItem {
    pub problem_id: String,
    pub title: String,
    pub track: String,
    pub level: Band,
    /// A coarse 0 to 4 level from stability.
    pub step: i32,
    /// How many days the problem is remembered for: the days until recall falls to 90 %.
    pub interval_days: i64,
    /// The chance of recalling it today, 0 to 1.
    pub retrievability: f32,
    pub lapses: i32,
    /// How many times it has been graded, first solve included.
    pub reps: i32,
    /// again, hard, good or easy.
    pub last_grade: String,
    pub due_at: DateTime<Utc>,
    /// Positive when overdue, 0 today, negative in the future.
    pub days_overdue: i64,
    pub last_result: String,
}

pub fn reviews(catalog: &Catalog, rows: &[ReviewRow], now: DateTime<Utc>) -> Reviews {
    let today = local_day(now);
    let days_overdue = |r: &ReviewRow| (today - local_day(r.due_at)).num_days();
    let mut queue: Vec<ReviewItem> = rows
        .iter()
        .filter(|r| days_overdue(r) >= -3)
        .filter_map(|r| {
            let (t, p) = catalog.problem(&r.problem_id)?;
            Some(ReviewItem {
                problem_id: r.problem_id.clone(),
                title: p.meta.title.clone(),
                track: t.code.clone(),
                level: p.meta.level,
                step: r.step,
                interval_days: r.stability.round() as i64,
                retrievability: r.retrievability(today),
                lapses: r.lapses,
                reps: r.reps,
                last_grade: r.last_grade.clone(),
                due_at: r.due_at,
                days_overdue: days_overdue(r),
                last_result: r.last_result.clone(),
            })
        })
        .collect();
    queue.sort_by_key(|i| i.due_at);

    // Counts cover every review; only the list shown is capped.
    let due: Vec<&ReviewRow> = rows.iter().filter(|r| days_overdue(r) >= 0).collect();
    let overdue = rows.iter().filter(|r| days_overdue(r) > 0).count();
    let minutes_today = due
        .iter()
        .filter_map(|r| catalog.problem(&r.problem_id).map(|(_, p)| budget(p.meta.mode, p.meta.level)))
        .sum();
    queue.truncate(30);
    let mut forecast = vec![0usize; 14];
    for r in rows {
        let ahead = -days_overdue(r);
        if ahead < 14 {
            forecast[ahead.max(0) as usize] += 1;
        }
    }
    let month_ago = now - chrono::TimeDelta::days(30);
    let resolves: Vec<bool> = rows
        .iter()
        .flat_map(|r| r.history.iter())
        .filter(|h| h["resolve"] == true)
        .filter(|h| h["at"].as_str().and_then(|s| s.parse::<DateTime<Utc>>().ok()).is_some_and(|at| at > month_ago))
        .map(|h| h["result"] == "unassisted")
        .collect();

    Reviews {
        due_today: due.len(),
        overdue,
        minutes_today,
        retention_30d: ratio(resolves.iter().filter(|u| **u).count(), resolves.len()),
        in_rotation: rows.iter().filter(|r| r.step < crate::reviews::GRADUATED).count(),
        graduated: rows.iter().filter(|r| r.step == crate::reviews::GRADUATED).count(),
        queue,
        forecast,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn medians_and_ratios() {
        assert_eq!(median(vec![3.0, 1.0, 2.0]), Some(2.0));
        assert_eq!(median(vec![4.0, 1.0]), Some(2.5));
        assert_eq!(median(vec![]), None);
        assert_eq!(ratio(1, 3), Some(33.3));
        assert_eq!(ratio(0, 0), None);
    }

    #[test]
    fn mondays() {
        let d: NaiveDate = "2026-09-27".parse().unwrap(); // a Sunday
        assert_eq!(monday_of(d), "2026-09-21".parse::<NaiveDate>().unwrap());
        assert_eq!(monday_of("2026-09-21".parse().unwrap()), "2026-09-21".parse::<NaiveDate>().unwrap());
    }
}
