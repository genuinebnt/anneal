//! The DSA section: the NeetCode lists you solve on LeetCode and log here. See docs/DSA.md.
//!
//! `GET /api/dsa` is everything the home and plan screens need. `POST /api/dsa/problems/{id}/log` records how a
//! problem went (it feeds streaks, readiness and the review schedule like any solve), and `POST /api/dsa/start`
//! moves where "next problem" starts from.

use std::collections::HashMap;

use anneal_content::{Band, Catalog, Company, CompanyGroup, DsaProblem, Problem, Role, Section, Technique, Track};
use axum::Json;
use axum::extract::{Path, State};
use chrono::NaiveDate;
use serde::{Deserialize, Serialize};

use crate::AppState;
use crate::dsa_next::{self, Entry};
use crate::error::{ApiError, ApiResult};
use crate::reviews::{self, Grade, Pace, Settings};
use crate::store::{self, ProgressRow, ReviewRow};

#[derive(Serialize)]
pub struct Overview<'a> {
    today: NaiveDate,
    settings: Settings,
    patterns: Vec<PatternRow<'a>>,
    techniques: &'a [Technique],
    /// How many lessons each pattern has for techniques with no must-learn problem (they add to the Learn tab's count).
    lesson_extras: std::collections::BTreeMap<&'a str, usize>,
    company_groups: &'a [CompanyGroup],
    problems: Vec<ProblemRow<'a>>,
    plan: Plan,
}

#[derive(Serialize)]
struct PatternRow<'a> {
    code: &'a str,
    slug: &'a str,
    name: &'a str,
    total: usize,
    in_150: usize,
    solved: usize,
    /// LeetCode practice problems for the pattern: how many, and how many are solved.
    practice_total: usize,
    practice_solved: usize,
}

#[derive(Serialize)]
pub(crate) struct ProblemRow<'a> {
    id: &'a str,
    slug: &'a str,
    number: u32,
    title: &'a str,
    difficulty: Band,
    /// The track code, e.g. `D7`.
    pattern: &'a str,
    lists: &'a [String],
    premium: bool,
    tags: &'a [String],
    companies: &'a [Company],
    video: Option<&'a str>,
    technique: &'a str,
    role: Role,
    practice_of: Option<&'a str>,
    order: u32,
    /// A written lesson (solution) exists for it.
    has_page: bool,
    /// `must`, `strong`, `practice` or `warmup`.
    priority: &'a str,
    /// The key insight: one sentence naming the move that solves it.
    insight: &'a str,
    /// Asked by a company of the site's set in the last six months.
    recent: bool,
    /// The narrowest NeetCode list it is in (`blind75`, `neetcode150`, `neetcode250`, `all`); none for a problem outside them.
    list_tag: Option<&'static str>,
    pub(crate) state: Standing,
}

/// The narrowest of the NeetCode lists a problem is in, if any (docs/DSA_LEARN_PAGE_SPEC.md, section 3).
fn narrowest_list(lists: &[String]) -> Option<&'static str> {
    ["blind75", "neetcode150", "neetcode250", "all"].into_iter().find(|l| lists.iter().any(|x| x == l))
}

/// How a problem stands: nothing yet, or the latest review's grade.
#[derive(Serialize)]
pub(crate) struct Standing {
    /// Solved at least once.
    solved: bool,
    /// The latest attempt needed help.
    assisted: bool,
    last_grade: Option<String>,
    reps: i32,
    lapses: i32,
    last_review: Option<NaiveDate>,
    due: Option<NaiveDate>,
    retrievability: Option<f32>,
}

#[derive(Serialize)]
pub(crate) struct Plan {
    /// Problems the goal counts, and how many are done.
    pub(crate) goal_total: u32,
    pub(crate) goal_done: u32,
    pub(crate) pace: Pace,
    /// The next problems in order, from `start`.
    pub(crate) next_up: Vec<String>,
    pub(crate) start: Option<String>,
    /// Today is a day for a new problem (`new_days`).
    pub(crate) solve_day: bool,
    /// Reviews today can take.
    pub(crate) capacity: u32,
    /// Today's reviews: most forgotten first, up to the capacity.
    pub(crate) review_ids: Vec<String>,
    /// Reviews due now, and how many of those are past their day.
    pub(crate) due: usize,
    pub(crate) overdue: usize,
}

fn local(t: chrono::DateTime<chrono::Utc>) -> NaiveDate {
    t.with_timezone(&chrono::Local).date_naive()
}

pub(crate) fn dsa_tracks(catalog: &Catalog) -> impl Iterator<Item = &Track> {
    catalog.tracks.iter().filter(|t| t.section == Section::Dsa)
}

/// A LeetCode problem outside the NeetCode lists, there to drill a technique (`content/dsa/practice.json`).
pub(crate) fn is_practice(p: &Problem) -> bool {
    p.dsa.as_ref().is_some_and(|d| d.lists.iter().all(|l| l == "practice"))
}

fn dsa_of(p: &Problem) -> &DsaProblem {
    p.dsa.as_ref().expect("DSA tracks only hold DSA problems")
}

fn state_of(progress: Option<&ProgressRow>, review: Option<&ReviewRow>, today: NaiveDate) -> Standing {
    Standing {
        solved: progress.is_some_and(|p| p.solved),
        assisted: progress.is_some_and(|p| p.assisted),
        last_grade: review.map(|r| r.last_grade.clone()),
        reps: review.map_or(0, |r| r.reps),
        lapses: review.map_or(0, |r| r.lapses),
        last_review: review.map(|r| local(r.last_review)),
        due: review.map(|r| local(r.due_at)),
        retrievability: review.map(|r| r.retrievability(today)),
    }
}

pub(crate) fn row<'a>(t: &'a Track, p: &'a Problem, progress: &HashMap<String, ProgressRow>, reviews: &HashMap<&str, &ReviewRow>, today: NaiveDate) -> ProblemRow<'a> {
    let d = dsa_of(p);
    ProblemRow {
        id: &p.id,
        slug: &d.slug,
        number: d.number,
        title: &p.meta.title,
        difficulty: p.meta.level,
        pattern: &t.code,
        lists: &d.lists,
        premium: d.premium,
        tags: &p.meta.tags,
        companies: &d.companies,
        video: d.video.as_deref(),
        technique: &d.technique,
        role: d.role,
        practice_of: d.practice_of.as_deref(),
        order: p.meta.order,
        has_page: d.page.is_some(),
        priority: &d.priority,
        insight: &d.insight,
        recent: d.companies.iter().any(|c| c.recent),
        list_tag: narrowest_list(&d.lists),
        state: state_of(progress.get(&p.id), reviews.get(p.id.as_str()).copied(), today),
    }
}

/// Whether a problem counts toward the goal's list.
fn in_goal(p: &Problem, settings: &Settings) -> bool {
    let d = dsa_of(p);
    d.lists.contains(&settings.goal.list) && !(settings.goal.free_only && d.premium)
}

/// The next problems in order, and where the sequence starts from. The goal's list first; once it's done, the 250, then
/// everything (see decision 22).
pub(crate) async fn next_up_ids(s: &AppState, settings: &Settings, progress: &HashMap<String, ProgressRow>, count: usize) -> ApiResult<(Vec<String>, Option<String>)> {
    let solved = |p: &Problem| progress.get(&p.id).is_some_and(|r| r.solved);
    let all: Vec<&Problem> = dsa_tracks(&s.catalog).flat_map(|t| &t.problems).collect();
    let goal: Vec<&Problem> = all.iter().copied().filter(|p| in_goal(p, settings)).collect();
    // Premium problems can't be done without a subscription, so a free-only plan never queues them, whichever list it falls back to.
    let scope = |list: &str| -> Vec<&Problem> { all.iter().copied().filter(|p| dsa_of(p).lists.iter().any(|l| l == list) && !(settings.goal.free_only && dsa_of(p).premium)).collect() };
    let start = store::dsa_start(&s.db).await?;
    for pool in [goal, scope("neetcode250"), scope("all")] {
        let entries: Vec<Entry> = pool.iter().map(|p| Entry { id: &p.id, done: solved(p) }).collect();
        let next: Vec<String> = dsa_next::next_up(&entries, start.as_deref(), count).into_iter().map(str::to_owned).collect();
        if !next.is_empty() {
            return Ok((next, start));
        }
    }
    Ok((Vec::new(), start))
}

pub(crate) async fn plan(s: &AppState, settings: &Settings, progress: &HashMap<String, ProgressRow>, reviews: &[ReviewRow], today: NaiveDate) -> ApiResult<Plan> {
    let solved = |p: &Problem| progress.get(&p.id).is_some_and(|r| r.solved);
    let all: Vec<&Problem> = dsa_tracks(&s.catalog).flat_map(|t| &t.problems).collect();

    let goal: Vec<&Problem> = all.iter().copied().filter(|p| in_goal(p, settings)).collect();
    let goal_done = goal.iter().filter(|p| solved(p)).count() as u32;
    // Problems beyond the list (a few from the 250) count against `extra`.
    let beyond = all.iter().filter(|p| !in_goal(p, settings) && dsa_of(p).lists.iter().any(|l| l == "neetcode250") && solved(p)).count() as u32;
    let extra_done = beyond.min(settings.goal.extra);
    let goal_total = goal.len() as u32 + settings.goal.extra;
    let remaining = settings.goal.custom_left.unwrap_or_else(|| goal_total.saturating_sub(goal_done + extra_done));

    let (fallback_up, start) = next_up_ids(s, settings, progress, 5).await?;
    // The calendar decides what today is (a problem day, a practice day or a break) and what comes next.
    let book = crate::calendar::book(&s.db).await?;
    let overrides = crate::calendar::active_overrides(&s.db).await?;
    let world = crate::calendar::World::build(&s.catalog, settings, progress, reviews, start.as_deref(), today);
    let planned = world.run(&book.active_plan().rules, &overrides);
    let next_up: Vec<String> = if planned.queue.order.is_empty() { fallback_up } else { planned.queue.order.iter().take(5).map(|i| world.ids[*i].to_owned()).collect() };
    let kind = world.routine.kind_of(today, &overrides);
    let capacity = world.routine.capacity_of(today, &overrides);
    let today_settings = Settings { capacity: settings.capacity.with(chrono::Datelike::weekday(&today), capacity), ..settings.clone() };

    let cards: Vec<(String, fsrs::MemoryState, NaiveDate, NaiveDate)> =
        reviews.iter().map(|r| (r.problem_id.clone(), r.memory(), local(r.last_review), local(r.due_at))).collect();
    let due = cards.iter().filter(|c| c.3 <= today).count();
    let overdue = cards.iter().filter(|c| c.3 < today).count();
    Ok(Plan {
        goal_total,
        goal_done: goal_done + extra_done,
        pace: reviews::pace(remaining, today, settings),
        next_up,
        start,
        solve_day: kind == crate::planner::Kind::Solve,
        capacity,
        review_ids: reviews::pick(today, &today_settings, &cards, &|id| weight_of(&s.catalog, id)),
        due,
        overdue,
    })
}

/// How much a DSA problem matters for review ranking; 1.0 for anything else.
pub(crate) fn weight_of(catalog: &Catalog, id: &str) -> f32 {
    catalog.problem(id).and_then(|(_, p)| p.dsa.as_ref().map(|d| reviews::importance(&d.lists, d.role == Role::MustLearn, d.companies.len(), d.companies.iter().filter(|c| c.recent).count()))).unwrap_or(1.0)
}

/// The settings to schedule this problem with: core problems get the higher retention target when one is set.
pub(crate) fn settings_for(catalog: &Catalog, id: &str, settings: &Settings) -> Settings {
    let core = catalog.problem(id).and_then(|(_, p)| p.dsa.as_ref()).is_some_and(|d| reviews::is_core(&d.lists, d.role == Role::MustLearn));
    settings.for_problem(core)
}

/// The reviews that belong to DSA problems.
pub(crate) fn dsa_reviews(catalog: &Catalog, rows: Vec<ReviewRow>) -> Vec<ReviewRow> {
    rows.into_iter().filter(|r| catalog.problem(&r.problem_id).is_some_and(|(_, p)| p.dsa.is_some())).collect()
}

pub async fn overview(State(s): State<AppState>) -> ApiResult<Json<serde_json::Value>> {
    let today = crate::activity::today();
    let settings = crate::settings::srs(&s.db).await?;
    let progress = store::progress(&s.db).await?;
    let reviews = dsa_reviews(&s.catalog, store::reviews(&s.db).await?);
    let by_problem: HashMap<&str, &ReviewRow> = reviews.iter().map(|r| (r.problem_id.as_str(), r)).collect();

    let patterns = dsa_tracks(&s.catalog)
        .map(|t| {
            let extras = s.catalog.practice_tracks.iter().find(|x| x.code == t.code).map(|x| x.problems.as_slice()).unwrap_or_default();
            let practice_total = extras.len();
            let practice_solved = extras.iter().filter(|p| progress.get(&p.id).is_some_and(|r| r.solved)).count();
            PatternRow {
            code: &t.code,
            slug: &t.slug,
            name: &t.name,
            total: t.problems.len(),
            in_150: t.problems.iter().filter(|p| dsa_of(p).lists.iter().any(|l| l == "neetcode150")).count(),
            solved: t.problems.iter().filter(|p| progress.get(&p.id).is_some_and(|r| r.solved)).count(),
            practice_total,
            practice_solved,
            }
        })
        .collect();
    let problems = dsa_tracks(&s.catalog).flat_map(|t| t.problems.iter().map(move |p| (t, p))).map(|(t, p)| row(t, p, &progress, &by_problem, today)).collect();
    let plan = plan(&s, &settings, &progress, &reviews, today).await?;
    let overview = Overview {
        today,
        settings,
        patterns,
        techniques: &s.catalog.dsa.techniques,
        lesson_extras: s.catalog.dsa.extras.iter().map(|(pattern, list)| (pattern.as_str(), list.len())).collect(),
        company_groups: &s.catalog.dsa.company_groups,
        problems,
        plan,
    };
    // Plain data, so serializing can't fail; the value only exists to end the borrow of the catalog.
    Ok(Json(serde_json::to_value(&overview).expect("overview is plain data")))
}

/// The written lesson for a problem, if one has been written.
pub async fn page(State(s): State<AppState>, Path(id): Path<String>) -> ApiResult<Json<Option<anneal_content::Page>>> {
    let (_, p) = s.catalog.problem(&id).ok_or_else(|| ApiError::NotFound(format!("problem {id}")))?;
    let dsa = p.dsa.as_ref().ok_or_else(|| ApiError::BadRequest(format!("{id} isn't a DSA problem")))?;
    Ok(Json(dsa.page.clone()))
}

/// One problem of the site as a row, whether or not it is in a NeetCode list: the problem page of a practice problem reads it.
pub async fn problem_row(State(s): State<AppState>, Path(id): Path<String>) -> ApiResult<Json<serde_json::Value>> {
    let (t, p) = s.catalog.problem(&id).ok_or_else(|| ApiError::NotFound(format!("problem {id}")))?;
    if p.dsa.is_none() {
        return Err(ApiError::BadRequest(format!("{id} isn't a DSA problem")));
    }
    let today = crate::activity::today();
    let progress = store::progress(&s.db).await?;
    let reviews = dsa_reviews(&s.catalog, store::reviews(&s.db).await?);
    let by_problem: HashMap<&str, &ReviewRow> = reviews.iter().map(|r| (r.problem_id.as_str(), r)).collect();
    Ok(Json(serde_json::to_value(row(t, p, &progress, &by_problem, today)).map_err(|e| ApiError::BadRequest(e.to_string()))?))
}

/// The problem the plan has next right now: the first of the calendar's queue, else the plain sequence.
async fn current_problem(s: &AppState, settings: &Settings) -> ApiResult<Option<String>> {
    let progress = store::progress(&s.db).await?;
    let reviews = dsa_reviews(&s.catalog, store::reviews(&s.db).await?);
    let today = crate::activity::today();
    let (fallback, start) = next_up_ids(s, settings, &progress, 1).await?;
    let book = crate::calendar::book(&s.db).await?;
    let overrides = crate::calendar::active_overrides(&s.db).await?;
    let world = crate::calendar::World::build(&s.catalog, settings, &progress, &reviews, start.as_deref(), today);
    let planned = world.run(&book.active_plan().rules, &overrides);
    Ok(planned.queue.order.first().map(|i| world.ids[*i].to_owned()).or_else(|| fallback.into_iter().next()))
}

/// Logging `id` when `current` was next counts as "instead of the next problem" when it is a different problem of the
/// same topic.
fn instead_of_current(catalog: &Catalog, current: Option<&str>, id: &str) -> bool {
    let Some(current) = current.filter(|c| *c != id) else { return false };
    match (catalog.problem(current), catalog.problem(id)) {
        (Some((a, _)), Some((b, _))) => a.code == b.code,
        _ => false,
    }
}

#[derive(Deserialize)]
pub struct LogBody {
    grade: Grade,
    /// A repeat done for practice: it counts toward the streak and the solve record, and schedules nothing.
    #[serde(default)]
    practice: bool,
}

#[derive(Serialize)]
pub struct Logged {
    id: String,
    /// When it comes back for review. Absent for practice problems, which schedule no reviews.
    due: Option<NaiveDate>,
    /// Days until then, before it was moved to a day with room.
    ideal_days: Option<f32>,
    /// Whether `POST .../undo` can take this log back.
    undoable: bool,
}

/// Records how a problem went: `again` (attempted, not solved: no streak), `hard` (with help), `good` (on my own) or `easy` (instant).
pub async fn log(State(s): State<AppState>, Path(id): Path<String>, Json(body): Json<LogBody>) -> ApiResult<Json<Logged>> {
    let (_, p) = s.catalog.problem(&id).ok_or_else(|| ApiError::NotFound(format!("problem {id}")))?;
    if p.dsa.is_none() {
        return Err(ApiError::BadRequest(format!("{id} isn't a DSA problem; solve it in the workspace")));
    }
    let settings = crate::settings::srs(&s.db).await?;
    let practice = is_practice(p);
    // A repeat for practice needs a first log to repeat; the first time is always a first try.
    let repeat = body.practice && !practice;
    if repeat && store::attempt_history(&s.db, &id).await?.is_empty() {
        return Err(ApiError::BadRequest(format!("{id} hasn't been logged yet; log the first try before practising it")));
    }
    // What was next before this log, to tell "did the next problem" from "did another one instead".
    let current = if practice || repeat { None } else { current_problem(&s, &settings).await? };
    let scheduled = store::log_attempt(&s.db, &id, body.grade, &settings_for(&s.catalog, &id, &settings), !practice, repeat).await?;
    // The next problem follows from the one just done, unless it was practice, which isn't part of the plan. Doing a
    // different problem of the same topic instead of the next one leaves the next one next: its slot just drops out of
    // the plan and the days after it close up. Doing one from another topic restarts the sequence after it (decision 22).
    if !practice && !repeat && !instead_of_current(&s.catalog, current.as_deref(), &id) {
        store::set_dsa_start(&s.db, &id).await?;
    }
    let undoable = store::can_undo(&s.db, &id).await?;
    Ok(Json(Logged { id, due: scheduled.as_ref().map(|x| x.due), ideal_days: scheduled.map(|x| x.ideal_days), undoable }))
}

/// Takes back the latest log of a problem (the review goes back as it was). `undone` is false when it cannot be done.
pub async fn undo(State(s): State<AppState>, Path(id): Path<String>) -> ApiResult<Json<serde_json::Value>> {
    let (_, p) = s.catalog.problem(&id).ok_or_else(|| ApiError::NotFound(format!("problem {id}")))?;
    if p.dsa.is_none() {
        return Err(ApiError::BadRequest(format!("{id} isn't a DSA problem")));
    }
    Ok(Json(serde_json::json!({ "id": id, "undone": store::undo_log(&s.db, &id).await? })))
}

/// Forgets a problem's log: back to "not started", with no review scheduled. Saved solutions stay, and what was removed
/// is archived first (`restored` by `POST .../restore`).
pub async fn reset(State(s): State<AppState>, Path(id): Path<String>) -> ApiResult<Json<serde_json::Value>> {
    let (_, p) = s.catalog.problem(&id).ok_or_else(|| ApiError::NotFound(format!("problem {id}")))?;
    if p.dsa.is_none() {
        return Err(ApiError::BadRequest(format!("{id} isn't a DSA problem")));
    }
    let archived = store::reset_problem(&s.db, &id).await?;
    Ok(Json(serde_json::json!({ "id": id, "archived": archived })))
}

/// Takes back the latest reset of a problem. `restored` is false when there is nothing to restore or the problem was
/// logged again since.
pub async fn restore(State(s): State<AppState>, Path(id): Path<String>) -> ApiResult<Json<serde_json::Value>> {
    let (_, p) = s.catalog.problem(&id).ok_or_else(|| ApiError::NotFound(format!("problem {id}")))?;
    if p.dsa.is_none() {
        return Err(ApiError::BadRequest(format!("{id} isn't a DSA problem")));
    }
    Ok(Json(serde_json::json!({ "id": id, "restored": store::restore_problem(&s.db, &id).await? })))
}

#[derive(Deserialize)]
pub struct StartBody {
    /// A problem id, or a track slug or code to start at its first problem.
    from: String,
}

/// Moves where "next problem" starts from.
pub async fn start(State(s): State<AppState>, Json(body): Json<StartBody>) -> ApiResult<Json<serde_json::Value>> {
    let id = match s.catalog.problem(&body.from) {
        Some((_, p)) if p.dsa.is_some() => p.id.clone(),
        Some(_) => return Err(ApiError::BadRequest(format!("{} isn't a DSA problem", body.from))),
        None => dsa_tracks(&s.catalog)
            .find(|t| t.slug == body.from || t.code.eq_ignore_ascii_case(&body.from))
            .and_then(|t| t.problems.first())
            .map(|p| p.id.clone())
            .ok_or_else(|| ApiError::NotFound(format!("problem or track {}", body.from)))?,
    };
    store::set_dsa_start(&s.db, &id).await?;
    Ok(Json(serde_json::json!({ "start": id })))
}

// ---------------------------------------------------------------- practice: LeetCode problems that drill a technique

#[derive(Serialize)]
pub struct PracticeTechnique<'a> {
    id: &'a str,
    name: &'a str,
    /// The NeetCode problem that teaches the technique.
    must_learn: PracticeRef<'a>,
    solved: usize,
    problems: Vec<ProblemRow<'a>>,
}

#[derive(Serialize)]
struct PracticeRef<'a> {
    id: &'a str,
    slug: &'a str,
    number: u32,
    title: &'a str,
    solved: bool,
}

#[derive(Serialize)]
pub struct PracticeList<'a> {
    pattern: &'a str,
    code: &'a str,
    techniques: Vec<PracticeTechnique<'a>>,
}

/// How a practice problem stands, from its latest attempt (they have no review to read it from).
pub(crate) fn practice_state(progress: Option<&ProgressRow>) -> Standing {
    let grade = progress.map(|r| match (r.solved, r.assisted) {
        (true, false) => "good",
        (true, true) => "hard",
        (false, _) => "again",
    });
    Standing {
        solved: progress.is_some_and(|r| r.solved),
        assisted: progress.is_some_and(|r| r.assisted),
        last_grade: grade.map(str::to_owned),
        reps: 0,
        lapses: 0,
        last_review: None,
        due: None,
        retrievability: None,
    }
}

/// A pattern's practice problems, grouped by the technique they drill (decision 24).
pub async fn practice(State(s): State<AppState>, Path(code): Path<String>) -> ApiResult<Json<serde_json::Value>> {
    let pattern = dsa_tracks(&s.catalog)
        .find(|t| t.code.eq_ignore_ascii_case(&code) || t.slug == code)
        .ok_or_else(|| ApiError::NotFound(format!("pattern {code}")))?;
    let progress = store::progress(&s.db).await?;
    let today = crate::activity::today();
    let no_reviews = HashMap::new();
    let empty: &[Problem] = &[];
    let extras = s.catalog.practice_tracks.iter().find(|x| x.code == pattern.code).map_or(empty, |x| x.problems.as_slice());
    let mut techniques = Vec::new();
    for t in s.catalog.dsa.techniques.iter().filter(|t| t.pattern == pattern.name) {
        let Some((track, teacher)) = s.catalog.problem(&t.must_learn) else { continue };
        let problems: Vec<ProblemRow> = extras
            .iter()
            .filter(|p| dsa_of(p).technique == t.id)
            .map(|p| {
                let mut r = row(&s.catalog.practice_tracks[0], p, &progress, &no_reviews, today);
                r.pattern = &track.code;
                r.state = practice_state(progress.get(&p.id));
                r
            })
            .collect();
        if problems.is_empty() {
            continue;
        }
        let d = dsa_of(teacher);
        techniques.push(PracticeTechnique {
            id: &t.id,
            name: &t.name,
            must_learn: PracticeRef { id: &teacher.id, slug: &d.slug, number: d.number, title: &teacher.meta.title, solved: progress.get(&teacher.id).is_some_and(|r| r.solved) },
            solved: problems.iter().filter(|p| p.state.solved).count(),
            problems,
        });
    }
    let list = PracticeList { pattern: &pattern.name, code: &pattern.code, techniques };
    Ok(Json(serde_json::to_value(&list).expect("practice list is plain data")))
}

// ---------------------------------------------------------------- pattern lessons (decision 6)

#[derive(Serialize)]
struct LessonTechnique<'a> {
    id: &'a str,
    name: &'a str,
    /// The lesson text, if it has been written.
    lesson: Option<&'a anneal_content::Lesson>,
    solved: usize,
    /// Problems in the NeetCode lists that use it, must-learn first.
    problems: Vec<ProblemRow<'a>>,
    practice_total: usize,
    practice_solved: usize,
}

/// The order of problems under a technique: the must-learn one first, then by priority, the NeetCode ones before the others, then easier first.
fn sort_for_learning(rows: &mut [ProblemRow]) {
    let rank = |p: &ProblemRow| match p.priority {
        "must" => 0,
        "strong" => 1,
        "practice" => 2,
        _ => 3,
    };
    rows.sort_by_key(|p| (p.role != Role::MustLearn, rank(p), p.list_tag.is_none(), p.difficulty as u8, p.number));
}

/// A lesson for a technique that has no must-learn problem: its example problems, with progress.
#[derive(Serialize)]
struct LessonExtra<'a> {
    id: &'a str,
    name: &'a str,
    group: &'a str,
    lesson: &'a anneal_content::Extra,
    examples: Vec<ProblemRow<'a>>,
}

/// A pattern's lessons: each technique with when to use it, a template, its traps, and its problems with progress.
pub async fn pattern(State(s): State<AppState>, Path(code): Path<String>) -> ApiResult<Json<serde_json::Value>> {
    let track = dsa_tracks(&s.catalog).find(|t| t.code.eq_ignore_ascii_case(&code) || t.slug == code).ok_or_else(|| ApiError::NotFound(format!("pattern {code}")))?;
    let today = crate::activity::today();
    let progress = store::progress(&s.db).await?;
    let reviews = dsa_reviews(&s.catalog, store::reviews(&s.db).await?);
    let by_problem: HashMap<&str, &ReviewRow> = reviews.iter().map(|r| (r.problem_id.as_str(), r)).collect();
    let extras = s.catalog.practice_tracks.iter().find(|x| x.code == track.code).map(|x| x.problems.as_slice()).unwrap_or_default();
    let mut techniques = Vec::new();
    for t in s.catalog.dsa.techniques.iter().filter(|t| t.pattern == track.name) {
        // the NeetCode ones and the others that practise the same technique (the others only count as tracked, never toward a goal)
        let mut problems: Vec<ProblemRow> = track.problems.iter().filter(|p| dsa_of(p).technique == t.id).map(|p| row(track, p, &progress, &by_problem, today)).collect();
        problems.extend(extras.iter().filter(|p| dsa_of(p).technique == t.id).map(|p| row(track, p, &progress, &by_problem, today)));
        sort_for_learning(&mut problems);
        techniques.push(LessonTechnique {
            id: &t.id,
            name: &t.name,
            lesson: s.catalog.dsa.lessons.get(&t.id),
            solved: problems.iter().filter(|p| p.state.solved && p.list_tag.is_some()).count(),
            problems,
            practice_total: 0,
            practice_solved: 0,
        });
    }
    // An extra's problems: its examples (any problem of the site, possibly filed under another pattern) and the problems whose technique it is.
    let mut extra_lessons = Vec::new();
    let all_tracks = || dsa_tracks(&s.catalog).chain(s.catalog.practice_tracks.iter());
    for e in s.catalog.dsa.extras.get(&track.name).map(Vec::as_slice).unwrap_or_default() {
        let mut examples: Vec<ProblemRow> = e
            .examples
            .iter()
            .filter_map(|slug| {
                let id = format!("lc-{slug}");
                all_tracks().find_map(|t| t.problems.iter().find(|p| p.id == id).map(|p| row(t, p, &progress, &by_problem, today)))
            })
            .collect();
        for t in all_tracks() {
            for p in t.problems.iter().filter(|p| dsa_of(p).technique == e.id) {
                if !examples.iter().any(|x| x.id == p.id) {
                    examples.push(row(t, p, &progress, &by_problem, today));
                }
            }
        }
        sort_for_learning(&mut examples);
        extra_lessons.push(LessonExtra { id: &e.id, name: &e.name, group: &e.group, lesson: e, examples });
    }
    let value = serde_json::json!({
        "code": track.code,
        "pattern": track.name,
        "intro": s.catalog.dsa.lesson_intros.get(&track.name),
        "total": track.problems.len(),
        "groups": s.catalog.dsa.lesson_groups.get(&track.name),
        "techniques": techniques,
        "extras": extra_lessons,
        "listed": s.catalog.dsa.listed.get(&track.name),
    });
    Ok(Json(value))
}
