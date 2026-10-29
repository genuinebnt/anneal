//! JSON shapes the web app reads. Kept separate from the content and DB types so
//! what reaches the browser is decided in one place (hidden tests, locked solutions).

use std::collections::HashMap;

use anneal_content::model::{Example, Hint, SolutionNotes};
use anneal_content::{Band, Language, Mode, Problem, Rules, Section, Status, Tier, Track};
use anneal_rules::Violation;
use anneal_runner::{Check, Diagnostic, Outcome, RunResult, RunStatus, Suite, TestOutcome};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::store::{Attempt, ProgressRow, RunRow};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Progress {
    NotStarted,
    Started,
    Solved,
    Assisted,
}

impl Progress {
    pub fn of(row: Option<&ProgressRow>) -> Self {
        match row {
            None => Progress::NotStarted,
            Some(r) if !r.solved => Progress::Started,
            Some(r) if r.assisted => Progress::Assisted,
            Some(_) => Progress::Solved,
        }
    }

    pub fn solved(self) -> bool {
        matches!(self, Progress::Solved | Progress::Assisted)
    }
}

#[derive(Debug, Serialize)]
pub struct TrackSummary {
    pub code: String,
    pub slug: String,
    pub name: String,
    pub section: Section,
    pub tier: Tier,
    pub order: u32,
    pub summary: String,
    pub stages: Vec<StageView>,
    pub total: usize,
    pub ready: usize,
    pub solved: usize,
    /// 0–100, see [`readiness`].
    pub readiness: f64,
}

#[derive(Debug, Serialize)]
pub struct StageView {
    pub slug: String,
    pub name: String,
    pub band: Band,
    pub total: usize,
    pub ready: usize,
    pub solved: usize,
}

#[derive(Debug, Serialize)]
pub struct TrackDetail {
    #[serde(flatten)]
    pub summary: TrackSummary,
    pub problems: Vec<ProblemSummary>,
    /// Every company a problem may be tagged with, grouped and in display order (FAANG first).
    pub company_groups: Vec<CompanyGroup>,
}

#[derive(Debug, Serialize)]
pub struct CompanyGroup {
    pub name: &'static str,
    pub companies: Vec<&'static str>,
}

/// `anneal_content::COMPANIES` as groups, keeping its order.
fn company_groups() -> Vec<CompanyGroup> {
    let mut groups: Vec<CompanyGroup> = Vec::new();
    for &(company, group) in anneal_content::COMPANIES {
        match groups.iter_mut().find(|g| g.name == group) {
            Some(g) => g.companies.push(company),
            None => groups.push(CompanyGroup { name: group, companies: vec![company] }),
        }
    }
    groups
}

#[derive(Debug, Serialize)]
pub struct ProblemSummary {
    pub id: String,
    pub slug: String,
    pub title: String,
    pub mode: Mode,
    pub level: Band,
    pub stage: String,
    pub order: u32,
    pub status: Status,
    pub tags: Vec<String>,
    pub companies: Vec<String>,
    pub progress: Progress,
}

/// Whether a problem is in the completion totals: a premium one only when the owner chose to count premium
/// (`settings::count_premium`). Logged progress on it is kept either way.
pub fn counts(p: &Problem, count_premium: bool) -> bool {
    count_premium || !p.dsa.as_ref().is_some_and(|d| d.premium)
}

pub fn track_summary(t: &Track, progress: &HashMap<String, ProgressRow>, count_premium: bool) -> TrackSummary {
    let state = |p: &Problem| Progress::of(progress.get(&p.id));
    let stages = t
        .stages
        .iter()
        .map(|s| {
            let ps: Vec<&Problem> = t
                .problems
                .iter()
                .filter(|p| p.meta.stage == s.slug && counts(p, count_premium))
                .collect();
            StageView {
                slug: s.slug.clone(),
                name: s.name.clone(),
                band: s.band,
                total: ps.len(),
                ready: ps.iter().filter(|p| p.meta.status == Status::Ready).count(),
                solved: ps.iter().filter(|p| state(p).solved()).count(),
            }
        })
        .collect();
    TrackSummary {
        code: t.code.clone(),
        slug: t.slug.clone(),
        name: t.name.clone(),
        section: t.section,
        tier: t.tier,
        order: t.order,
        summary: t.summary.clone(),
        stages,
        total: t.problems.iter().filter(|p| counts(p, count_premium)).count(),
        ready: t
            .problems
            .iter()
            .filter(|p| p.meta.status == Status::Ready && counts(p, count_premium))
            .count(),
        solved: t.problems.iter().filter(|p| counts(p, count_premium) && state(p).solved()).count(),
        readiness: readiness(t, progress, count_premium),
    }
}

/// How much a problem counts toward readiness.
pub fn weight(level: Band) -> f64 {
    match level {
        Band::Easy => 1.0,
        Band::Medium => 2.0,
        Band::Hard => 3.0,
    }
}

/// Readiness for a track, 0–100: Σ weight × credit / Σ weight over every problem in the
/// track, drafts included. Credit is 1 for an unassisted solve and 0.5 for an assisted one, times the
/// overdue-review decay. (PLAN.md's tested-out credit isn't implemented.)
pub fn readiness(t: &Track, progress: &HashMap<String, ProgressRow>, count_premium: bool) -> f64 {
    let total: f64 = t.problems.iter().filter(|p| counts(p, count_premium)).map(|p| weight(p.meta.level)).sum();
    if total == 0.0 {
        return 0.0;
    }
    let earned: f64 = t
        .problems
        .iter()
        .filter(|p| counts(p, count_premium))
        .map(|p| {
            let row = progress.get(&p.id);
            let credit = match Progress::of(row) {
                Progress::Solved => 1.0,
                Progress::Assisted => 0.5,
                Progress::NotStarted | Progress::Started => 0.0,
            };
            weight(p.meta.level) * credit * row.map_or(1.0, |r| r.decay)
        })
        .sum();
    (1000.0 * earned / total).round() / 10.0
}

pub fn track_detail(t: &Track, progress: &HashMap<String, ProgressRow>, count_premium: bool) -> TrackDetail {
    TrackDetail {
        summary: track_summary(t, progress, count_premium),
        problems: t
            .problems
            .iter()
            .map(|p| ProblemSummary {
                id: p.id.clone(),
                slug: p.meta.slug.clone(),
                title: p.meta.title.clone(),
                mode: p.meta.mode,
                level: p.meta.level,
                stage: p.meta.stage.clone(),
                order: p.meta.order,
                status: p.meta.status,
                tags: p.meta.tags.clone(),
                companies: p.meta.companies.clone(),
                progress: Progress::of(progress.get(&p.id)),
            })
            .collect(),
        company_groups: company_groups(),
    }
}

/// A DSA problem that opens a practice problem.
#[derive(Debug, Clone, Serialize)]
pub struct UnlockRef {
    pub id: String,
    pub slug: String,
    pub title: String,
}

#[derive(Debug, Serialize)]
pub struct ProblemDetail {
    pub id: String,
    pub slug: String,
    pub title: String,
    pub mode: Mode,
    /// What the editor and runner speak.
    pub language: Language,
    /// Practice problems: the DSA problems that open it, and whether it is a warm-up.
    pub unlocked_by: Vec<UnlockRef>,
    pub warmup: bool,
    pub level: Band,
    pub status: Status,
    pub track: TrackRef,
    pub stage: StageRef,
    /// 1-based position in the track, and the track's size.
    pub position: usize,
    pub count: usize,
    pub prev: Option<String>,
    pub next: Option<String>,
    pub statement: String,
    pub tags: Vec<String>,
    pub companies: Vec<String>,
    pub teaches: Vec<String>,
    pub constraints: Vec<String>,
    pub examples: Vec<Example>,
    pub follow_up: Option<String>,
    pub related: Vec<String>,
    /// Crates the problem may use, from the sandbox's crate set.
    pub crates: Vec<String>,
    pub rules: Option<Rules>,
    pub starter: String,
    /// The autosaved buffer, if it differs from the starter.
    pub draft: Option<String>,
    /// The scratch `main.rs` for Run: saved, or a template.
    pub scratch: String,
    pub visible_tests: String,
    /// The hidden test file, once the problem has been solved.
    pub hidden_tests: Option<String>,
    pub hints: HintsView,
    pub solution: SolutionView,
    pub attempt: AttemptView,
    /// Runs in the current attempt, oldest first.
    pub runs: Vec<RunView>,
}

#[derive(Debug, Serialize)]
pub struct TrackRef {
    pub code: String,
    pub slug: String,
    pub name: String,
    pub section: Section,
}

#[derive(Debug, Serialize)]
pub struct StageRef {
    pub slug: String,
    pub name: String,
    pub band: Band,
    /// 1-based stage number in the track.
    pub number: usize,
    /// 1-based position of this problem in the stage, and the stage's size.
    pub position: usize,
    pub count: usize,
}

#[derive(Debug, Serialize)]
pub struct HintsView {
    pub total: usize,
    pub revealed: Vec<Hint>,
    /// Kinds of the hints still hidden, e.g. "edge case", so the UI can label them.
    pub locked: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct SolutionView {
    pub unlocked: bool,
    /// Present only when unlocked.
    pub code: Option<String>,
    pub notes: Option<SolutionNotes>,
}

#[derive(Debug, Default, Serialize)]
pub struct AttemptView {
    pub started: bool,
    pub started_at: Option<DateTime<Utc>>,
    pub solved: bool,
    pub assisted: bool,
    pub hints_revealed: usize,
    pub solution_revealed: bool,
    /// A scheduled re-solve rather than the first time through.
    pub resolve: bool,
}

impl From<Option<&Attempt>> for AttemptView {
    fn from(a: Option<&Attempt>) -> Self {
        match a {
            None => AttemptView::default(),
            Some(a) => AttemptView {
                started: true,
                started_at: Some(a.started_at),
                solved: a.solved_at.is_some(),
                assisted: a.assisted,
                hints_revealed: a.hints_revealed.max(0) as usize,
                solution_revealed: a.solution_revealed,
                resolve: a.kind == "resolve",
            },
        }
    }
}

#[derive(Debug, Serialize)]
pub struct RunView {
    pub id: i64,
    pub kind: String,
    pub created_at: DateTime<Utc>,
    pub code: String,
    pub status: RunStatus,
    pub passed: usize,
    pub total: usize,
    pub duration_ms: u64,
    pub diagnostics: Vec<Diagnostic>,
    pub tests: Vec<TestOutcome>,
    /// Fix-this rules this run broke. Any violation keeps a submit from solving the problem.
    pub violations: Vec<Violation>,
}

impl RunView {
    /// Hidden tests are redacted unless `reveal_hidden` (the problem has been solved).
    pub fn new(row: RunRow, reveal_hidden: bool) -> Self {
        let RunResult {
            status,
            diagnostics,
            tests,
            passed,
            total,
            duration_ms,
        } = row.result.0;
        RunView {
            id: row.id,
            kind: row.kind,
            created_at: row.created_at,
            code: row.code,
            status,
            passed,
            total,
            duration_ms,
            diagnostics,
            tests: if reveal_hidden { tests } else { tests.into_iter().map(redact).collect() },
            violations: row.violations.0,
        }
    }
}

/// Hidden tests report only pass or fail: no input, expected value, output or panic text.
fn redact(t: TestOutcome) -> TestOutcome {
    if t.suite != Suite::Hidden {
        return t;
    }
    let failed = matches!(t.outcome, Outcome::Failed | Outcome::TimedOut);
    TestOutcome {
        check: failed.then(|| Check {
            input: "withheld".into(),
            expected: "withheld".into(),
            got: "wrong answer".into(),
        }),
        panic: None,
        stdout: String::new(),
        ..t
    }
}

pub fn problem_detail(
    track: &Track,
    p: &Problem,
    attempt: Option<&Attempt>,
    draft: Option<String>,
    scratch: Option<String>,
    runs: Vec<RunRow>,
    solved_ever: bool,
) -> ProblemDetail {
    let idx = track
        .problems
        .iter()
        .position(|q| q.id == p.id)
        .unwrap_or(0);
    let stage_def = track.stage(&p.meta.stage);
    let in_stage: Vec<&Problem> = track
        .problems
        .iter()
        .filter(|q| q.meta.stage == p.meta.stage)
        .collect();
    let starter = p.files.starter.clone().unwrap_or_default();
    let revealed = attempt
        .map_or(0, |a| a.hints_revealed.max(0) as usize)
        .min(p.meta.hints.len());
    ProblemDetail {
        id: p.id.clone(),
        slug: p.meta.slug.clone(),
        title: p.meta.title.clone(),
        mode: p.meta.mode,
        language: p.meta.language,
        unlocked_by: Vec::new(), // filled in by the route, which can see the whole catalog
        warmup: p.meta.warmup,
        level: p.meta.level,
        status: p.meta.status,
        track: TrackRef {
            code: track.code.clone(),
            slug: track.slug.clone(),
            name: track.name.clone(),
            section: track.section,
        },
        stage: StageRef {
            slug: p.meta.stage.clone(),
            name: stage_def.map(|s| s.name.clone()).unwrap_or_default(),
            band: stage_def.map_or(p.meta.level, |s| s.band),
            number: track
                .stages
                .iter()
                .position(|s| s.slug == p.meta.stage)
                .map_or(0, |i| i + 1),
            position: in_stage
                .iter()
                .position(|q| q.id == p.id)
                .map_or(0, |i| i + 1),
            count: in_stage.len(),
        },
        position: idx + 1,
        count: track.problems.len(),
        prev: idx.checked_sub(1).map(|i| track.problems[i].id.clone()),
        next: track.problems.get(idx + 1).map(|q| q.id.clone()),
        statement: p.files.statement.clone().unwrap_or_default(),
        tags: p.meta.tags.clone(),
        companies: p.meta.companies.clone(),
        teaches: p.meta.teaches.clone(),
        constraints: p.meta.constraints.clone(),
        examples: p.meta.examples.clone(),
        follow_up: p.meta.follow_up.clone(),
        related: p.meta.related.clone(),
        crates: p.meta.crates.clone(),
        rules: p.meta.rules.clone(),
        draft: draft.filter(|d| *d != starter),
        scratch: if p.meta.language == Language::Python { String::new() } else { scratch.unwrap_or_else(|| SCRATCH_TEMPLATE.to_owned()) },
        starter,
        visible_tests: p.files.visible_tests.clone().unwrap_or_default(),
        hidden_tests: if solved_ever { p.files.hidden_tests.clone() } else { None },
        hints: HintsView {
            total: p.meta.hints.len(),
            revealed: p.meta.hints[..revealed].to_vec(),
            locked: p.meta.hints[revealed..]
                .iter()
                .map(|h| h.kind.clone())
                .collect(),
        },
        solution: solution_view(p, attempt),
        attempt: attempt.into(),
        runs: runs.into_iter().map(|r| RunView::new(r, solved_ever)).collect(),
    }
}

/// The solution opens once the problem is solved, or after "Reveal anyway".
pub fn solution_view(p: &Problem, attempt: Option<&Attempt>) -> SolutionView {
    let unlocked = attempt.is_some_and(|a| a.solved_at.is_some() || a.solution_revealed);
    SolutionView {
        unlocked,
        code: unlocked.then(|| p.files.solution.clone()).flatten(),
        notes: if unlocked {
            p.meta.solution.clone()
        } else {
            None
        },
    }
}

#[derive(Debug, Deserialize)]
pub struct CodeBody {
    pub code: String,
}

/// What Run and Submit return: the run plus the state it changed.
#[derive(Debug, Serialize)]
pub struct RunOutcome {
    pub run: RunView,
    pub attempt: AttemptView,
    pub solution: SolutionView,
}

/// What `main.rs` starts as. `solution` is the crate your `lib.rs` compiles to.
pub const SCRATCH_TEMPLATE: &str = r#"// Scratch: Run (⌘') builds this with your lib.rs and runs it. Tests don't run.
use solution::*;

fn main() {
    // Call your code with any input and print what you want to see, e.g.
    // println!("{:?}", my_function(&[1, 2, 3]));
    // dbg!(&value);
}
"#;

#[derive(Debug, Deserialize)]
pub struct ScratchBody {
    /// The current lib.rs buffer.
    pub lib: String,
    /// The scratch main.rs.
    pub main: String,
}
