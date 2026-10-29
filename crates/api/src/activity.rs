//! The section pages' activity: streak, this week, recent attempts and the next problem to solve.

use std::collections::HashMap;

use anneal_content::{Band, Catalog, Mode, Problem, Section, Status, Track};
use chrono::{DateTime, Days, Local, NaiveDate, Utc};
use serde::Serialize;

use crate::store::{ActivityRow, ProgressRow};
use crate::views::{self, Progress};

#[derive(Debug, Serialize)]
pub struct Activity {
    /// Consecutive days with at least one solve, ending today (or yesterday, if today has none yet).
    pub streak: u32,
    /// The last seven days, oldest first.
    pub week: Vec<Day>,
    pub week_solved: u32,
    pub week_unassisted: u32,
    pub recent: Vec<Recent>,
    pub next: Option<NextUp>,
}

#[derive(Debug, Serialize)]
pub struct Day {
    pub date: NaiveDate,
    pub solved: u32,
}

#[derive(Debug, Serialize)]
pub struct Recent {
    pub problem_id: String,
    pub title: String,
    pub track: String,
    /// `solved`, `assisted`, `failing` (runs but no solve) or `started`.
    pub outcome: &'static str,
    /// e.g. "18m", "hint 2", "3 runs".
    pub detail: String,
    pub at: DateTime<Utc>,
}

#[derive(Debug, Serialize)]
pub struct NextUp {
    pub problem_id: String,
    pub title: String,
    pub mode: Mode,
    pub level: Band,
    pub track_code: String,
    pub track_slug: String,
    pub track_name: String,
    pub stage_name: String,
    /// `resume` (attempted, not solved), `current` (next in the track you're working on) or `start`.
    pub reason: &'static str,
    /// The statement's first paragraph, as markdown.
    pub excerpt: String,
    /// The track's readiness now, and how much an unassisted solve adds.
    pub readiness: f64,
    pub gain: f64,
}

const RECENT: usize = 5;

pub fn build(
    catalog: &Catalog,
    sections: &[Section],
    progress: &HashMap<String, ProgressRow>,
    rows: &[ActivityRow],
    today: NaiveDate,
    count_premium: bool,
    is_solve_day: &dyn Fn(NaiveDate) -> bool,
) -> Activity {
    let local = |t: DateTime<Utc>| t.with_timezone(&Local).date_naive();
    let in_area = |id: &str| catalog.problem(id).is_some_and(|(t, _)| sections.contains(&t.section));

    let solve_days: Vec<NaiveDate> = rows.iter().filter_map(|r| r.solved_at.map(local)).collect();
    let streak = streak_by_plan(&solve_days, today, is_solve_day);

    let week_start = today - Days::new(6);
    let mut week: Vec<Day> = (0..7).map(|i| Day { date: week_start + Days::new(i), solved: 0 }).collect();
    let (mut week_solved, mut week_unassisted) = (0, 0);
    for r in rows.iter().filter(|r| in_area(&r.problem_id)) {
        let Some(day) = r.solved_at.map(local) else { continue };
        if day < week_start || day > today {
            continue;
        }
        week[(day - week_start).num_days() as usize].solved += 1;
        week_solved += 1;
        if !r.assisted {
            week_unassisted += 1;
        }
    }

    let recent = rows
        .iter()
        .filter(|r| in_area(&r.problem_id))
        .take(RECENT)
        .filter_map(|r| {
            let (t, p) = catalog.problem(&r.problem_id)?;
            let (outcome, detail) = match (r.solved_at, r.assisted) {
                (Some(done), false) => ("solved", minutes(done - r.started_at)),
                (Some(_), true) if r.solution_revealed => ("assisted", "solution".to_string()),
                (Some(_), true) => ("assisted", format!("hint {}", r.hints_revealed)),
                (None, _) if r.runs > 0 => ("failing", plural(r.runs, "run")),
                (None, _) => ("started", "started".to_string()),
            };
            let detail = match r.kind.as_str() {
                "resolve" => format!("re-solve · {detail}"),
                "extra" => format!("practice · {detail}"),
                _ => detail,
            };
            Some(Recent {
                problem_id: r.problem_id.clone(),
                title: p.meta.title.clone(),
                track: t.code.clone(),
                outcome,
                detail,
                at: r.last_at,
            })
        })
        .collect();

    Activity {
        streak,
        week,
        week_solved,
        week_unassisted,
        recent,
        next: next_up(catalog, sections, progress, rows, count_premium),
    }
}

/// Consecutive days with a solve, ending today (or yesterday if today has none yet).
#[cfg(test)]
pub(crate) fn streak(solve_days: &[NaiveDate], today: NaiveDate) -> u32 {
    streak_by_plan(solve_days, today, &|_| true)
}

/// The streak the plan keeps: walking back from `today`, every day with a solve adds one; a planned solve day with no solve
/// breaks it (today never does, the day is not over); a rest, practice or break day neither breaks it nor adds to it.
/// With every day a solve day this is the plain streak. A solve on any problem counts, premium or not.
pub(crate) fn streak_by_plan(solve_days: &[NaiveDate], today: NaiveDate, is_solve_day: &dyn Fn(NaiveDate) -> bool) -> u32 {
    let Some(first) = solve_days.iter().min().copied() else { return 0 };
    let (mut day, mut n) = (today, 0);
    while day >= first {
        if solve_days.contains(&day) {
            n += 1;
        } else if day != today && is_solve_day(day) {
            break;
        }
        day = day - Days::new(1);
    }
    n
}

fn minutes(d: chrono::TimeDelta) -> String {
    let m = d.num_minutes().max(1);
    if m >= 120 { format!("{}h", m / 60) } else { format!("{m}m") }
}

fn plural(n: i64, word: &str) -> String {
    if n == 1 { format!("1 {word}") } else { format!("{n} {word}s") }
}

/// Resume the most recent unsolved attempt; otherwise the next unsolved problem in the track
/// you worked on last; otherwise the first unsolved problem in recommended order.
fn next_up(
    catalog: &Catalog,
    sections: &[Section],
    progress: &HashMap<String, ProgressRow>,
    rows: &[ActivityRow],
    count_premium: bool,
) -> Option<NextUp> {
    // A fn, not a closure: closures don't tie their returned borrow to their argument.
    fn first_open<'t>(t: &'t Track, progress: &HashMap<String, ProgressRow>) -> Option<&'t Problem> {
        t.problems.iter().find(|p| open(p, progress))
    }
    fn open(p: &Problem, progress: &HashMap<String, ProgressRow>) -> bool {
        p.meta.status == Status::Ready && !Progress::of(progress.get(&p.id)).solved()
    }

    let mut tracks: Vec<&Track> = catalog.tracks.iter().filter(|t| sections.contains(&t.section)).collect();
    tracks.sort_by_key(|t| (sections.iter().position(|s| *s == t.section), t.order));

    let last = rows
        .iter()
        .find_map(|r| catalog.problem(&r.problem_id).filter(|(t, _)| sections.contains(&t.section)));
    let (track, problem, reason) = match last {
        Some((t, p)) if open(p, progress) => (t, p, "resume"),
        Some((t, _)) if first_open(t, progress).is_some() => (t, first_open(t, progress)?, "current"),
        _ => tracks.iter().find_map(|t| Some((*t, first_open(t, progress)?, "start")))?,
    };

    let total: f64 = track.problems.iter().map(|p| views::weight(p.meta.level)).sum();
    let stage = track.stages.iter().find(|s| s.slug == problem.meta.stage);
    Some(NextUp {
        problem_id: problem.id.clone(),
        title: problem.meta.title.clone(),
        mode: problem.meta.mode,
        level: problem.meta.level,
        track_code: track.code.clone(),
        track_slug: track.slug.clone(),
        track_name: track.name.clone(),
        stage_name: stage.map(|s| s.name.clone()).unwrap_or_default(),
        reason,
        excerpt: excerpt(problem.files.statement.as_deref().unwrap_or("")),
        readiness: views::readiness(track, progress, count_premium),
        gain: (1000.0 * views::weight(problem.meta.level) / total).round() / 10.0,
    })
}

/// The first prose paragraph of a statement.
pub(crate) fn excerpt(statement: &str) -> String {
    statement
        .split("\n\n")
        .map(str::trim)
        .find(|p| !p.is_empty() && !p.starts_with("```") && !p.starts_with('#'))
        .map(|p| p.split_whitespace().collect::<Vec<_>>().join(" "))
        .unwrap_or_default()
}

/// Parses `D` or `L,S,C,Y` into sections, ignoring unknown letters.
pub fn parse_sections(s: &str) -> Vec<Section> {
    s.split(',')
        .filter_map(|l| match l.trim() {
            "D" => Some(Section::Dsa),
            "L" => Some(Section::Language),
            "S" => Some(Section::StandardLibrary),
            "C" => Some(Section::Concurrency),
            "Y" => Some(Section::Systems),
            "F" => Some(Section::Performance),
            "B" => Some(Section::Backend),
            "M" => Some(Section::Design),
            "P" => Some(Section::Practice),
            _ => None,
        })
        .collect()
}

pub fn today() -> NaiveDate {
    Local::now().date_naive()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(s: &str) -> NaiveDate {
        s.parse().unwrap()
    }

    /// Monday to Saturday are solve days, Sunday is not.
    fn not_sunday(day: NaiveDate) -> bool {
        chrono::Datelike::weekday(&day) != chrono::Weekday::Sun
    }

    #[test]
    fn a_rest_day_neither_breaks_the_streak_nor_adds_to_it() {
        // Fri 23, Sat 24, (Sun 25 is not a solve day), Mon 26.
        let days = [d("2026-10-23"), d("2026-10-24"), d("2026-10-26")];
        assert_eq!(streak_by_plan(&days, d("2026-10-26"), &not_sunday), 3, "Sunday is skipped, not counted");
        assert_eq!(streak(&days, d("2026-10-26")), 1, "the plain streak broke on Sunday");
    }

    #[test]
    fn a_missed_solve_day_breaks_it_but_today_does_not() {
        let days = [d("2026-10-26"), d("2026-10-28")]; // Tue 27 was a solve day with no solve
        assert_eq!(streak_by_plan(&days, d("2026-10-28"), &not_sunday), 1);
        assert_eq!(streak_by_plan(&days, d("2026-10-29"), &not_sunday), 1, "Thursday has no solve yet, but it is not over");
        assert_eq!(streak_by_plan(&[], d("2026-10-29"), &not_sunday), 0);
    }

    #[test]
    fn a_solve_on_a_rest_day_still_counts() {
        let days = [d("2026-10-24"), d("2026-10-25"), d("2026-10-26")];
        assert_eq!(streak_by_plan(&days, d("2026-10-26"), &not_sunday), 3);
    }

    #[test]
    fn streak_counts_back_from_today_or_yesterday() {
        let days = [d("2026-09-25"), d("2026-09-26"), d("2026-09-27"), d("2026-09-20")];
        assert_eq!(streak(&days, d("2026-09-27")), 3);
        assert_eq!(streak(&days, d("2026-09-28")), 3, "no solve yet today keeps yesterday's streak");
        assert_eq!(streak(&days, d("2026-09-29")), 0);
        assert_eq!(streak(&[], d("2026-09-27")), 0);
    }

    #[test]
    fn excerpt_skips_code_and_joins_lines() {
        let s = "A network has `n` nodes\nand edges.\n\n```rust\nfn f() {}\n```\n";
        assert_eq!(excerpt(s), "A network has `n` nodes and edges.");
        assert_eq!(excerpt("```rust\nx\n```\n\nThen prose."), "Then prose.");
    }

    #[test]
    fn sections_parse() {
        assert_eq!(parse_sections("L, S,x"), vec![Section::Language, Section::StandardLibrary]);
    }
}
