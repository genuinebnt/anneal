//! Series (`content/series`, docs/SERIES.md): the path, and where a problem sits in it. Everything shown as progress is
//! worked out from the attempts and review dates that are already stored; a series keeps nothing of its own.

use std::collections::{HashMap, HashSet};

use anneal_content::series::{CookbookLevel, Item, ItemKind, Series};
use anneal_content::Catalog;
use axum::Json;
use axum::extract::{Path, State};
use chrono::{DateTime, Local, NaiveDate, Utc};
use serde::Serialize;

use crate::AppState;
use crate::error::{ApiError, ApiResult};
use crate::store::{self, ActivityRow, ProgressRow, ReviewRow};
use crate::views::Progress;

/// Where a part or step stands. `Next` is the one the series points at.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ItemState {
    Solved,
    Assisted,
    Next,
    Started,
    Open,
    ToWrite,
}

#[derive(Debug, Clone, Serialize)]
pub struct ItemRow {
    pub kind: &'static str,
    pub number: usize,
    /// The problem to open; absent while it is to be written.
    pub id: Option<String>,
    pub title: String,
    pub idea: String,
    pub minutes: Option<u32>,
    pub state: ItemState,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RepeatState {
    /// The part isn't solved yet.
    Waiting,
    /// The review dates have it on a later day.
    Scheduled,
    /// It is in today's re-solves.
    Due,
    /// Re-solved after the later part.
    Done,
}

#[derive(Debug, Serialize)]
#[serde(tag = "row", rename_all = "snake_case")]
pub enum PathRow {
    Item(ItemRow),
    Repeat { number: usize, id: String, state: RepeatState, due: Option<NaiveDate>, minutes: Option<u32> },
}

#[derive(Debug, Serialize)]
pub struct Group {
    pub name: String,
    pub rows: Vec<PathRow>,
}

#[derive(Debug, Serialize)]
pub struct SeriesSummary {
    pub id: String,
    pub title: String,
    pub summary: String,
    /// Whether each part is solved, in order.
    pub segments: Vec<bool>,
    pub solved: usize,
    pub to_write: usize,
    pub steps: usize,
    pub steps_solved: usize,
    pub next: Option<ItemRow>,
}

#[derive(Debug, Serialize)]
pub struct PageRef {
    pub id: String,
    pub title: String,
}

#[derive(Debug, Serialize)]
pub struct Capstone {
    pub title: String,
    pub summary: String,
    pub note: String,
    pub cookbook: Option<PageRef>,
    pub steps: Vec<ItemRow>,
}

/// One heading of what the capstone needs, with the parts that teach any of it.
#[derive(Debug, Serialize)]
pub struct Need {
    pub need: String,
    /// "1 to 3", "3 and 9".
    pub parts: String,
    pub concepts: Vec<Concept>,
}

#[derive(Debug, Serialize)]
pub struct Concept {
    pub concept: String,
    pub parts: String,
    /// The capstone steps that need it.
    pub steps: String,
    pub note: String,
}

#[derive(Debug, Serialize)]
pub struct IdeaLine {
    pub name: String,
    pub learn: String,
    pub again: String,
    pub apply: String,
}

#[derive(Debug, Serialize)]
pub struct SideLink {
    pub id: String,
    pub title: String,
    pub why: String,
}

#[derive(Debug, Serialize)]
pub struct SeriesView {
    #[serde(flatten)]
    pub summary_view: SeriesSummary,
    /// Why the next one is next.
    pub next_why: String,
    pub groups: Vec<Group>,
    pub capstone: Option<Capstone>,
    pub note: String,
    pub needs: Vec<Need>,
    pub ideas: Vec<IdeaLine>,
    pub optional: Vec<SideLink>,
}

/// What the reader has done, as stored.
struct Facts {
    progress: HashMap<String, ProgressRow>,
    activity: Vec<ActivityRow>,
    reviews: HashMap<String, ReviewRow>,
    today: NaiveDate,
}

impl Facts {
    async fn load(s: &AppState) -> ApiResult<Facts> {
        Ok(Facts {
            progress: store::progress(&s.db).await?,
            activity: store::activity(&s.db).await?,
            reviews: store::reviews(&s.db).await?.into_iter().map(|r| (r.problem_id.clone(), r)).collect(),
            today: crate::activity::today(),
        })
    }

    fn of(&self, item: &Item) -> Progress {
        Progress::of(item.id.as_deref().and_then(|id| self.progress.get(id)))
    }

    fn solved(&self, item: &Item) -> bool {
        item.written && self.of(item).solved()
    }

    fn due(&self, id: &str) -> Option<NaiveDate> {
        self.reviews.get(id).map(|r| r.due_at.with_timezone(&Local).date_naive())
    }

    fn first_solve(&self, id: &str) -> Option<DateTime<Utc>> {
        self.activity.iter().filter(|a| a.problem_id == id).filter_map(|a| a.solved_at).min()
    }

    /// A planned repeat of `part` after `after`. It is done once the part has been re-solved later than both first
    /// solves; while `after` is to be written, any re-solve counts.
    fn repeat(&self, series: &Series, part: &str, after: &str) -> RepeatState {
        let Some(first) = self.first_solve(part) else { return RepeatState::Waiting };
        let then = match self.first_solve(after) {
            Some(t) => Some(t.max(first)),
            None if series.item(after).is_some_and(|i| i.written) => None,
            None => Some(first),
        };
        let resolved = then.is_some_and(|then| {
            self.activity.iter().any(|a| a.problem_id == part && a.kind == "resolve" && a.solved_at.is_some_and(|at| at > then))
        });
        match self.due(part) {
            _ if resolved => RepeatState::Done,
            Some(due) if due <= self.today => RepeatState::Due,
            _ => RepeatState::Scheduled,
        }
    }
}

/// The first written part not yet solved, else the first such capstone step.
fn next_item<'a>(series: &'a Series, facts: &Facts) -> Option<&'a Item> {
    series.items().find(|i| i.written && !facts.solved(i))
}

fn row(item: &Item, next: Option<&Item>, facts: &Facts) -> ItemRow {
    let state = if !item.written {
        ItemState::ToWrite
    } else {
        match facts.of(item) {
            Progress::Solved => ItemState::Solved,
            Progress::Assisted => ItemState::Assisted,
            _ if next.is_some_and(|n| n.kind == item.kind && n.number == item.number) => ItemState::Next,
            Progress::Started => ItemState::Started,
            Progress::NotStarted => ItemState::Open,
        }
    };
    ItemRow {
        kind: match item.kind {
            ItemKind::Part => "part",
            ItemKind::Step => "step",
        },
        number: item.number,
        id: item.id.clone().filter(|_| item.written),
        title: item.title.clone(),
        idea: item.line().to_owned(),
        minutes: item.minutes,
        state,
    }
}

fn summary(series: &Series, facts: &Facts) -> SeriesSummary {
    let next = next_item(series, facts);
    SeriesSummary {
        id: series.id.clone(),
        title: series.title.clone(),
        summary: series.summary.clone(),
        segments: series.parts.iter().map(|p| facts.solved(p)).collect(),
        solved: series.parts.iter().filter(|p| facts.solved(p)).count(),
        to_write: series.parts.iter().filter(|p| !p.written).count(),
        steps: series.steps.len(),
        steps_solved: series.steps.iter().filter(|p| facts.solved(p)).count(),
        next: next.map(|n| row(n, Some(n), facts)),
    }
}

/// Starts a line in the middle of a sentence: "Know when…" becomes "know when…", while "RAII" stays.
fn lower_first(s: &str) -> String {
    let mut chars = s.chars();
    match (chars.next(), chars.next()) {
        (Some(a), Some(b)) if a.is_uppercase() && !b.is_uppercase() => a.to_lowercase().chain([b]).chain(chars).collect(),
        _ => s.to_owned(),
    }
}

fn next_why(series: &Series, facts: &Facts) -> String {
    let Some(next) = next_item(series, facts) else {
        return String::new();
    };
    let done: Vec<usize> = series.parts.iter().filter(|p| facts.solved(p)).map(|p| p.number).collect();
    let mut why = if done.is_empty() && series.steps.iter().all(|s| !facts.solved(s)) {
        "This is where the series starts.".to_owned()
    } else if done.is_empty() {
        String::new()
    } else {
        format!("You solved {} {}.", if done.len() == 1 { "part" } else { "parts" }, numbers(&done))
    };
    let idea = next.line();
    if !idea.is_empty() {
        // it reuses what was solved when it names one of those as required or helpful
        let reused: Vec<usize> = next
            .required
            .iter()
            .chain(next.helpful.iter().map(|h| &h.id))
            .filter_map(|id| series.item(id))
            .filter(|i| facts.solved(i))
            .map(|i| i.number)
            .collect();
        let lead = match (done.is_empty(), reused.len()) {
            (true, _) => "The main idea",
            (false, 0) => "This adds one main idea",
            (false, 1) if done.len() == 1 => "This reuses it and adds one main idea",
            (false, _) => "This reuses earlier work and adds one main idea",
        };
        why = format!("{why} {lead}: {}.", lower_first(idea.trim_end_matches('.')));
    }
    why.trim().to_owned()
}

/// "1", "1 and 3", "1 to 3", "1 to 3 and 9".
pub fn numbers(list: &[usize]) -> String {
    let mut list = list.to_vec();
    list.sort_unstable();
    list.dedup();
    let mut runs: Vec<(usize, usize)> = Vec::new();
    for n in list {
        match runs.last_mut() {
            Some((_, end)) if *end + 1 == n => *end = n,
            _ => runs.push((n, n)),
        }
    }
    let mut words = Vec::new();
    for (a, b) in runs {
        match b - a {
            0 => words.push(a.to_string()),
            1 => words.extend([a.to_string(), b.to_string()]),
            _ => words.push(format!("{a} to {b}")),
        }
    }
    match words.split_last() {
        None => String::new(),
        Some((last, [])) => last.clone(),
        Some((last, rest)) => format!("{} and {last}", rest.join(", ")),
    }
}

/// "Part 2", "Parts 2 and 3", "Capstone steps 1 and 2", "Part 3; capstone step 6".
fn items_named(series: &Series, ids: &[String]) -> String {
    let of = |kind: ItemKind| ids.iter().filter_map(|id| series.item(id)).filter(|i| i.kind == kind).map(|i| i.number).collect::<Vec<_>>();
    let named = |n: Vec<usize>, one: &str, many: &str| match n.len() {
        0 => None,
        1 => Some(format!("{one} {}", n[0])),
        _ => Some(format!("{many} {}", numbers(&n))),
    };
    let steps = named(of(ItemKind::Step), "Capstone step", "Capstone steps");
    named(of(ItemKind::Part), "Part", "Parts").into_iter().chain(steps).collect::<Vec<_>>().join("; ")
}

/// The readiness rows under their headings, in the file's order.
fn needs(series: &Series) -> Vec<Need> {
    let part_numbers = |ids: &[String]| ids.iter().filter_map(|id| series.item(id)).map(|i| i.number).collect::<Vec<_>>();
    let mut names: Vec<&str> = Vec::new();
    for r in &series.readiness {
        if !names.contains(&r.group.as_str()) {
            names.push(&r.group);
        }
    }
    names
        .into_iter()
        .map(|name| {
            let rows: Vec<_> = series.readiness.iter().filter(|r| r.group == name).collect();
            Need {
                need: name.to_owned(),
                parts: numbers(&rows.iter().flat_map(|r| part_numbers(&r.parts)).collect::<Vec<_>>()),
                concepts: rows
                    .iter()
                    .map(|r| Concept { concept: r.concept.clone(), parts: numbers(&part_numbers(&r.parts)), steps: numbers(&r.steps), note: r.note.clone() })
                    .collect(),
            }
        })
        .collect()
}

fn page_title(catalog: &Catalog, id: &str) -> Option<PageRef> {
    catalog.cookbook.pages.get(id).map(|p| PageRef { id: id.to_owned(), title: p.title.clone() })
}

fn view(catalog: &Catalog, series: &Series, facts: &Facts) -> SeriesView {
    let next = next_item(series, facts);
    let mut groups: Vec<Group> = Vec::new();
    for p in &series.parts {
        if groups.last().is_none_or(|g| g.name != p.group) {
            groups.push(Group { name: p.group.clone(), rows: Vec::new() });
        }
        if let Some(g) = groups.last_mut() {
            g.rows.push(PathRow::Item(row(p, next, facts)));
        }
    }
    // A repeat sits under the group of the item it comes after; one after a capstone step goes under the last group.
    for r in &series.revisits {
        let (Some(part), Some(after)) = (series.item(&r.part), series.revisit_anchor(r)) else { continue };
        let at = match after.kind {
            ItemKind::Part => groups.iter().position(|g| g.name == after.group),
            ItemKind::Step => groups.len().checked_sub(1),
        };
        if let Some(g) = at.and_then(|i| groups.get_mut(i)) {
            let anchor = after.id.as_deref().unwrap_or_default();
            g.rows.push(PathRow::Repeat { number: part.number, id: r.part.clone(), state: facts.repeat(series, &r.part, anchor), due: facts.due(&r.part), minutes: r.minutes });
        }
    }
    SeriesView {
        summary_view: summary(series, facts),
        next_why: next_why(series, facts),
        groups,
        capstone: (!series.steps.is_empty()).then(|| Capstone {
            title: series.project.title.clone(),
            summary: series.project.summary.clone(),
            note: series.project.note.clone(),
            cookbook: series.project.cookbook.as_deref().and_then(|id| page_title(catalog, id)),
            steps: series.steps.iter().map(|i| row(i, next, facts)).collect(),
        }),
        note: series.note.clone(),
        needs: needs(series),
        ideas: series
            .connections
            .iter()
            .map(|t| IdeaLine { name: t.idea.clone(), learn: items_named(series, &t.learn), again: items_named(series, &t.again), apply: items_named(series, &t.apply) })
            .collect(),
        optional: series
            .side_paths
            .iter()
            .filter_map(|o| catalog.problem(&o.id).map(|(_, p)| SideLink { id: o.id.clone(), title: p.meta.title.clone(), why: o.why.clone() }))
            .collect(),
    }
}

// ---------------------------------------------------------------- a problem's place in a series

/// A link from a problem's Connections block.
#[derive(Debug, Serialize)]
pub struct Link {
    /// The problem to open; absent while it is to be written.
    pub id: Option<String>,
    /// "Part 1", "Capstone step 2" or "Another problem".
    pub label: String,
    pub number: Option<usize>,
    pub title: String,
    pub why: String,
    pub reveals_method: bool,
}

#[derive(Debug, Serialize)]
pub struct CookbookLink {
    pub page: String,
    pub title: String,
    pub level: &'static str,
}

#[derive(Debug, Serialize)]
pub struct Line {
    pub label: String,
    pub text: String,
}

/// The completion view; sent only once the problem has been solved.
#[derive(Debug, Serialize)]
pub struct Done {
    pub title: String,
    pub practised: Vec<Line>,
    pub returns: Vec<Line>,
    /// What the existing re-solve schedule has to do with this part, one sentence per entry.
    pub repeat: Vec<Repeat>,
    /// The next written part or step still to solve.
    pub next: Option<ItemRow>,
    /// The ones passed over to reach it, as they are still to be written.
    pub skipped: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct Repeat {
    pub text: String,
    /// The day the review schedule has it down for.
    pub due: Option<NaiveDate>,
}

#[derive(Debug, Serialize)]
pub struct Place {
    pub series: PageRef,
    pub kind: &'static str,
    pub number: usize,
    /// How many parts (or steps) there are.
    pub total: usize,
    /// The written problems either side in the series' order (the parts, then the capstone steps); absent at the ends.
    pub prev: Option<String>,
    pub next: Option<String>,
    pub minutes: Option<u32>,
    /// Which of those are solved.
    pub segments: Vec<bool>,
    pub know_first: Option<String>,
    pub required: Vec<Link>,
    pub helpful: Vec<Link>,
    pub cookbook: Vec<CookbookLink>,
    pub done: Option<Done>,
}

fn link(catalog: &Catalog, series: &Series, id: &str, why: &str, reveals_method: bool) -> Link {
    match series.item(id) {
        Some(i) => Link { id: i.id.clone().filter(|_| i.written), label: i.label(), number: Some(i.number), title: i.title.clone(), why: why.to_owned(), reveals_method },
        None => Link {
            id: catalog.problem(id).map(|_| id.to_owned()),
            label: "Another problem".to_owned(),
            number: None,
            title: catalog.problem(id).map_or_else(|| id.to_owned(), |(_, p)| p.meta.title.clone()),
            why: why.to_owned(),
            reveals_method,
        },
    }
}

fn done(series: &Series, item: &Item, facts: &Facts) -> Done {
    let id = item.id.as_deref().unwrap_or_default();
    let practised = if item.practised.is_empty() {
        let mut lines = Vec::new();
        if !item.new_idea.is_empty() {
            lines.push(Line { label: "New".to_owned(), text: item.new_idea.clone() });
        }
        if !item.reuses.is_empty() && !item.reuses.starts_with("nothing") {
            lines.push(Line { label: "Again".to_owned(), text: item.reuses.clone() });
        }
        lines
    } else {
        item.practised
            .iter()
            .map(|p| Line {
                label: p.from.as_deref().and_then(|f| series.item(f)).map_or_else(|| "New".to_owned(), |f| format!("Again, from {}", lower_first(&f.label()))),
                text: p.text.clone(),
            })
            .collect()
    };
    let returns = if item.returns.is_empty() {
        let mut lines: Vec<Line> = item
            .used_again
            .iter()
            .filter_map(|id| series.item(id))
            .map(|i| Line { label: i.label(), text: if i.line().is_empty() { i.title.clone() } else { format!("{}: {}", i.title, lower_first(i.line())) } })
            .collect();
        if !item.real_uses.is_empty() {
            lines.push(Line { label: "In real projects".to_owned(), text: item.real_uses.clone() });
        }
        lines
    } else {
        item.returns.iter().map(|r| Line { label: r.place.clone(), text: r.text.clone() }).collect()
    };
    // Planned repeats that this one unlocks.
    let mut repeat: Vec<Repeat> = series
        .revisits
        .iter()
        .filter(|r| series.revisit_anchor(r).is_some_and(|a| a.id.as_deref() == Some(id)) && facts.repeat(series, &r.part, id) != RepeatState::Done)
        .filter_map(|r| series.item(&r.part).map(|earlier| (r, earlier)))
        .map(|(r, earlier)| {
            let at = r.part.as_str();
            let mut text = if r.how.is_empty() { format!("Solve part {} again from a blank starter.", earlier.number) } else { r.how.clone() };
            if let Some(m) = r.minutes {
                text = format!("{text} About {m} minutes.");
            }
            if facts.first_solve(at).is_none() {
                text = format!("{text} You have not solved it yet.");
            }
            Repeat { text, due: facts.due(at) }
        })
        .collect();
    if let Some(due) = facts.due(id) {
        let what = match item.kind {
            ItemKind::Part => "This part",
            ItemKind::Step => "This step",
        };
        repeat.push(Repeat { text: format!("{what} has a re-solve date of its own, like every problem you solve."), due: Some(due) });
    }
    let after: Vec<&Item> = series.items().skip_while(|i| !(i.kind == item.kind && i.number == item.number)).skip(1).collect();
    let at = after.iter().position(|i| i.written && !facts.solved(i));
    let next = at.map(|i| row(after[i], Some(after[i]), facts));
    let skipped = after.iter().take(at.unwrap_or(after.len())).filter(|i| !i.written).map(|i| i.label()).collect();
    Done {
        title: item.solved_title.clone().unwrap_or_else(|| format!("You solved {}", lower_first(&item.title))),
        practised,
        returns,
        repeat,
        next,
        skipped,
    }
}

/// The nearest written part or step before and after `item`; those still to be written are passed over.
fn neighbours(series: &Series, item: &Item) -> (Option<String>, Option<String>) {
    let is_item = |i: &&Item| i.kind == item.kind && i.number == item.number;
    let open = |i: &Item| i.written.then(|| i.id.clone()).flatten();
    let prev = series.items().take_while(|i| !is_item(i)).filter_map(open).last();
    let next = series.items().skip_while(|i| !is_item(i)).skip(1).find_map(open);
    (prev, next)
}

fn place(catalog: &Catalog, series: &Series, item: &Item, facts: &Facts) -> Place {
    let all = match item.kind {
        ItemKind::Part => &series.parts,
        ItemKind::Step => &series.steps,
    };
    let (prev, next) = neighbours(series, item);
    Place {
        prev,
        next,
        series: PageRef { id: series.id.clone(), title: series.title.clone() },
        kind: match item.kind {
            ItemKind::Part => "part",
            ItemKind::Step => "step",
        },
        number: item.number,
        total: all.len(),
        minutes: item.minutes,
        segments: all.iter().map(|i| facts.solved(i)).collect(),
        know_first: item.know_first.clone(),
        required: item.required.iter().map(|id| link(catalog, series, id, "", false)).collect(),
        helpful: item.helpful.iter().map(|h| link(catalog, series, &h.id, &h.why, h.reveals_method)).collect(),
        cookbook: {
            let mut seen = HashSet::new();
            item.cookbook
                .iter()
                .filter(|c| seen.insert(c.page.as_str()))
                .filter_map(|c| {
                    page_title(catalog, &c.page).map(|p| CookbookLink {
                        page: p.id,
                        title: p.title,
                        level: match c.level {
                            CookbookLevel::Exact => "exact",
                            CookbookLevel::Similar => "similar",
                            CookbookLevel::Related => "related",
                        },
                    })
                })
                .collect()
        },
        done: facts.solved(item).then(|| done(series, item, facts)),
    }
}

// ---------------------------------------------------------------- handlers

/// `GET /api/series`
pub async fn list(State(s): State<AppState>) -> ApiResult<Json<Vec<SeriesSummary>>> {
    let facts = Facts::load(&s).await?;
    Ok(Json(s.catalog.series.iter().map(|x| summary(x, &facts)).collect()))
}

/// `GET /api/series/{id}`
pub async fn get(State(s): State<AppState>, Path(id): Path<String>) -> ApiResult<Json<SeriesView>> {
    let series = s.catalog.series.iter().find(|x| x.id == id).ok_or_else(|| ApiError::NotFound(format!("series {id}")))?;
    let facts = Facts::load(&s).await?;
    Ok(Json(view(&s.catalog, series, &facts)))
}

/// `GET /api/problems/{id}/series`: where the problem sits in the first series that has it, or `null`.
pub async fn of_problem(State(s): State<AppState>, Path(id): Path<String>) -> ApiResult<Json<Option<Place>>> {
    let found = s.catalog.series.iter().find_map(|x| x.item(&id).filter(|i| i.written).map(|i| (x, i)));
    let Some((series, item)) = found else { return Ok(Json(None)) };
    let facts = Facts::load(&s).await?;
    Ok(Json(Some(place(&s.catalog, series, item, &facts))))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn part_numbers_read_as_words() {
        assert_eq!(numbers(&[]), "");
        assert_eq!(numbers(&[8]), "8");
        assert_eq!(numbers(&[3, 9]), "3 and 9");
        assert_eq!(numbers(&[2, 1, 3]), "1 to 3");
        assert_eq!(numbers(&[1, 2]), "1 and 2");
        assert_eq!(numbers(&[1, 3, 13, 15]), "1, 3, 13 and 15");
        assert_eq!(numbers(&[4, 5, 6, 7, 9]), "4 to 7 and 9");
    }

    #[test]
    fn a_line_joins_a_sentence() {
        assert_eq!(lower_first("Know when"), "know when");
        assert_eq!(lower_first("RAII guards"), "RAII guards");
        assert_eq!(lower_first(""), "");
    }
}
