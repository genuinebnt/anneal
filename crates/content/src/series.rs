//! Series: an ordered path through existing problems that ends in a capstone, one `content/series/<id>.toml` each
//! (docs/SERIES.md has the format). A series only points at problems, so nothing about the reader is stored for it.
//! A missing directory means there are no series. A file with a mistake is reported and left out.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs;
use std::path::Path;

use serde::Deserialize;

use crate::catalog::{Issue, Track};
use crate::cookbook::Cookbook;
use crate::model::Status;

#[derive(Debug, Clone)]
pub struct Series {
    /// The file's name without `.toml`: the page's address.
    pub id: String,
    pub title: String,
    pub summary: String,
    /// A line for the foot of the path, e.g. that times are estimates.
    pub note: String,
    /// For a problem that several series have, when the reader has not said which one and their progress does not
    /// decide: the lowest rank is shown. A series without one comes after the ranked ones; ties go by id.
    pub rank: Option<u32>,
    pub project: Project,
    /// Preparation parts, in order.
    pub parts: Vec<Item>,
    /// Capstone steps, in order.
    pub steps: Vec<Item>,
    pub side_paths: Vec<SidePath>,
    pub revisits: Vec<Revisit>,
    pub readiness: Vec<Readiness>,
    pub connections: Vec<Connection>,
}

/// What the series page says about the capstone as a whole.
#[derive(Debug, Clone, Default)]
pub struct Project {
    pub title: String,
    pub summary: String,
    pub note: String,
    /// The Cookbook page with a complete version, kept closed on the series page: `capstone_cookbook`, else the first
    /// page a capstone step refers to.
    pub cookbook: Option<String>,
}

/// A preparation part or a capstone step.
#[derive(Debug, Clone, Default)]
pub struct Item {
    pub kind: ItemKind,
    /// 1-based, among the parts or among the steps.
    pub number: usize,
    /// The problem it is solved in. A part still to be written may name the id it will have, or none.
    pub id: Option<String>,
    /// Whether the problem exists and is ready.
    pub written: bool,
    /// The file's title, else the problem's.
    pub title: String,
    /// The path heading a part is listed under (the stage's name); empty for a capstone step.
    pub group: String,
    pub minutes: Option<u32>,
    pub new_idea: String,
    /// What the series page says about it when `new_idea` would give the method away.
    pub blurb: Option<String>,
    pub reuses: String,
    pub know_first: Option<String>,
    /// Ids of earlier parts or steps to do first.
    pub required: Vec<String>,
    pub helpful: Vec<Helpful>,
    pub used_again: Vec<String>,
    pub real_uses: String,
    /// Capstone steps: the parts that prepare for it.
    pub prepared_by: Vec<String>,
    pub cookbook: Vec<CookbookRef>,
    /// The completion view's headline.
    pub solved_title: Option<String>,
    pub practised: Vec<Practised>,
    pub returns: Vec<Return>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ItemKind {
    #[default]
    Part,
    Step,
}

impl Item {
    /// `Part 2` or `Capstone step 2`.
    pub fn label(&self) -> String {
        match self.kind {
            ItemKind::Part => format!("Part {}", self.number),
            ItemKind::Step => format!("Capstone step {}", self.number),
        }
    }

    /// The line shown about it outside its own page.
    pub fn line(&self) -> &str {
        self.blurb.as_deref().unwrap_or(&self.new_idea)
    }
}

impl Series {
    /// Parts, then capstone steps.
    pub fn items(&self) -> impl Iterator<Item = &Item> {
        self.parts.iter().chain(&self.steps)
    }

    pub fn item(&self, id: &str) -> Option<&Item> {
        self.items().find(|i| i.id.as_deref() == Some(id))
    }

    /// The id of the part or step a revisit comes after: its `after`, or the one in front of its `before`.
    pub fn revisit_anchor(&self, r: &Revisit) -> Option<&Item> {
        if let Some(after) = &r.after {
            return self.item(after);
        }
        let before = r.before.as_deref()?;
        self.items().zip(self.items().skip(1)).find(|(_, next)| next.id.as_deref() == Some(before)).map(|(prev, _)| prev)
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Helpful {
    pub id: String,
    #[serde(default)]
    pub why: String,
    #[serde(default)]
    pub reveals_method: bool,
    /// Accepted for the authors; the app keeps every helpful link closed.
    #[serde(default)]
    pub hidden: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CookbookRef {
    pub level: CookbookLevel,
    pub page: String,
    /// The author's note; not shown in the app.
    #[serde(default)]
    pub note: String,
    /// The Cookbook source file the note is about; not shown in the app.
    #[serde(default)]
    pub source: Option<String>,
    /// Accepted for the authors; the app keeps every Cookbook link closed.
    #[serde(default)]
    pub hidden: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CookbookLevel {
    /// Solves the task.
    Exact,
    /// The same technique on a different task.
    Similar,
    /// Explains the idea.
    Related,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Practised {
    pub text: String,
    /// The earlier part this comes back from; absent means it is new here.
    #[serde(default)]
    pub from: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Return {
    #[serde(rename = "where")]
    pub place: String,
    pub text: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SidePath {
    pub id: String,
    #[serde(default)]
    pub why: String,
    /// Where it fits: before or after these.
    #[serde(default)]
    pub before: Option<String>,
    #[serde(default)]
    pub after: Option<String>,
}

/// Solve `part` again from a blank starter: after one item, or before one.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Revisit {
    pub part: String,
    #[serde(default)]
    pub after: Option<String>,
    #[serde(default)]
    pub before: Option<String>,
    /// The sentence shown once it is unlocked.
    #[serde(default)]
    pub how: String,
    #[serde(default)]
    pub minutes: Option<u32>,
}

/// One thing the capstone needs, and where it is taught. Rows with the same `group` are shown together.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Readiness {
    pub group: String,
    pub concept: String,
    #[serde(default)]
    pub parts: Vec<String>,
    /// Capstone step numbers that need it.
    #[serde(default)]
    pub steps: Vec<usize>,
    #[serde(default)]
    pub note: String,
}

/// How one idea is learnt, met again and applied.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Connection {
    pub idea: String,
    pub learn: Vec<String>,
    #[serde(default)]
    pub again: Vec<String>,
    #[serde(default)]
    pub apply: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Stage {
    pub id: String,
    pub name: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SeriesFile {
    series: Head,
    #[serde(default)]
    stage: Vec<Stage>,
    #[serde(default)]
    part: Vec<ItemFile>,
    #[serde(default)]
    capstone: Vec<ItemFile>,
    #[serde(default)]
    revisit: Vec<Revisit>,
    #[serde(default)]
    readiness: Vec<Readiness>,
    #[serde(default)]
    connection: Vec<Connection>,
    #[serde(default)]
    side_path: Vec<SidePath>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Head {
    /// A name for the authors; the address comes from the file's name.
    #[serde(default, rename = "id")]
    _id: Option<String>,
    title: String,
    #[serde(default)]
    summary: String,
    #[serde(default)]
    note: String,
    #[serde(default)]
    rank: Option<u32>,
    #[serde(default)]
    capstone_title: Option<String>,
    #[serde(default)]
    capstone_summary: String,
    #[serde(default)]
    capstone_note: String,
    #[serde(default)]
    capstone_cookbook: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ItemFile {
    #[serde(default)]
    number: Option<usize>,
    #[serde(default)]
    step: Option<usize>,
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    stage: Option<String>,
    #[serde(default)]
    minutes: Option<u32>,
    #[serde(default)]
    status: Option<FileStatus>,
    #[serde(default)]
    new_idea: String,
    #[serde(default)]
    blurb: Option<String>,
    #[serde(default)]
    reuses: String,
    #[serde(default)]
    know_first: Option<String>,
    #[serde(default)]
    required: Vec<String>,
    #[serde(default)]
    helpful: Vec<Helpful>,
    #[serde(default)]
    used_again: Vec<String>,
    #[serde(default)]
    real_uses: String,
    #[serde(default)]
    prepared_by: Vec<String>,
    #[serde(default)]
    cookbook: Vec<CookbookRef>,
    #[serde(default)]
    solved_title: Option<String>,
    #[serde(default)]
    practised: Vec<Practised>,
    #[serde(default)]
    returns: Vec<Return>,
    // An authoring note, accepted and not shown.
    #[serde(default, rename = "adapted_from")]
    _adapted_from: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
enum FileStatus {
    Existing,
    New,
    /// The problem does not exist yet.
    ToWrite,
}

/// Loads `<root>/series/*.toml`, checking every reference against the tracks and the cookbook.
pub(crate) fn load(root: &Path, tracks: &[Track], cookbook: &Cookbook, issues: &mut Vec<Issue>) -> Vec<Series> {
    let dir = root.join("series");
    let Ok(entries) = fs::read_dir(&dir) else { return Vec::new() };
    let mut paths: Vec<_> = entries.filter_map(|e| e.ok()).map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x == "toml")).collect();
    paths.sort();
    // id -> (title, ready). A DSA problem is solved on LeetCode and has no workspace page, so a series can't hold one.
    let problems: HashMap<&str, (&str, bool)> = tracks
        .iter()
        .flat_map(|t| &t.problems)
        .filter(|p| p.dsa.is_none())
        .map(|p| (p.id.as_str(), (p.meta.title.as_str(), p.meta.status == Status::Ready)))
        .collect();
    let mut all = Vec::new();
    for path in paths {
        let id = path.file_stem().and_then(|s| s.to_str()).unwrap_or_default().to_owned();
        let mut found = Vec::new();
        let parsed = fs::read_to_string(&path)
            .map_err(|e| e.to_string())
            .and_then(|text| toml::from_str::<SeriesFile>(&text).map_err(|e| e.to_string()));
        match parsed {
            Err(e) => found.push(e),
            Ok(file) => {
                if id.is_empty() || !id.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-') {
                    found.push("the file name is the series' address: use a-z, 0-9 and -".to_owned());
                }
                let series = build(id, file, &problems, &mut found);
                check(&series, &problems, cookbook, &mut found);
                if found.is_empty() {
                    all.push(series);
                }
            }
        }
        issues.extend(found.into_iter().map(|message| Issue { path: path.clone(), message }));
    }
    all
}

fn build(id: String, file: SeriesFile, problems: &HashMap<&str, (&str, bool)>, found: &mut Vec<String>) -> Series {
    let mut stages: HashMap<&str, &str> = HashMap::new();
    for s in &file.stage {
        if stages.insert(&s.id, &s.name).is_some() {
            found.push(format!("stage {:?} is listed twice", s.id));
        }
    }
    let mut items = |list: Vec<ItemFile>, kind: ItemKind| -> Vec<Item> {
        list.into_iter()
            .enumerate()
            .map(|(i, f)| {
                let number = i + 1;
                let mut item = Item { kind, number, ..Item::default() };
                let name = match &f.id {
                    Some(id) => format!("{} ({id})", item.label()),
                    None => item.label(),
                };
                let (given, wrong) = match kind {
                    ItemKind::Part => (f.number, f.step.is_some().then_some("step")),
                    ItemKind::Step => (f.step, f.number.is_some().then_some("number")),
                };
                if let Some(key) = wrong {
                    found.push(format!("{name}: `{key}` does not belong here"));
                }
                if given.is_some_and(|n| n != number) {
                    found.push(format!("{name}: is numbered {} but comes in place {number}", given.unwrap_or_default()));
                }
                let problem = f.id.as_deref().and_then(|id| problems.get(id));
                item.written = problem.is_some_and(|(_, ready)| *ready);
                // A part may be "to be written": it says so, or it names a problem that is still a draft.
                if problem.is_none() && f.status != Some(FileStatus::ToWrite) {
                    found.push(match &f.id {
                        Some(_) => format!("{name}: there is no such problem; add it, or set status = \"to-write\""),
                        None => format!("{name}: needs an id, or status = \"to-write\""),
                    });
                }
                match f.title.or_else(|| problem.map(|(title, _)| (*title).to_owned())) {
                    Some(t) if !t.trim().is_empty() => item.title = t,
                    _ => found.push(format!("{name}: needs a title while there is no problem to take it from")),
                }
                match (kind, f.stage) {
                    (ItemKind::Part, Some(g)) => match stages.get(g.as_str()) {
                        Some(title) => item.group = (*title).to_owned(),
                        None => found.push(format!("{name}: there is no [[stage]] {g:?}")),
                    },
                    (ItemKind::Part, None) => found.push(format!("{name}: needs a stage")),
                    (ItemKind::Step, Some(_)) => found.push(format!("{name}: a capstone step has no stage")),
                    (ItemKind::Step, None) => {}
                }
                item.id = f.id;
                item.minutes = f.minutes;
                item.new_idea = f.new_idea;
                item.blurb = f.blurb;
                item.reuses = f.reuses;
                item.know_first = f.know_first;
                item.required = f.required;
                item.helpful = f.helpful;
                item.used_again = f.used_again;
                item.real_uses = f.real_uses;
                item.prepared_by = f.prepared_by;
                item.cookbook = f.cookbook;
                item.solved_title = f.solved_title;
                item.practised = f.practised;
                item.returns = f.returns;
                item
            })
            .collect()
    };
    let parts = items(file.part, ItemKind::Part);
    let steps = items(file.capstone, ItemKind::Step);
    let head = file.series;
    let cookbook = head.capstone_cookbook.or_else(|| steps.iter().find_map(|s| s.cookbook.first()).map(|c| c.page.clone()));
    Series {
        id,
        title: head.title,
        summary: head.summary,
        note: head.note,
        rank: head.rank,
        project: Project {
            title: head.capstone_title.unwrap_or_else(|| "Capstone".to_owned()),
            summary: head.capstone_summary,
            note: head.capstone_note,
            cookbook,
        },
        parts,
        steps,
        side_paths: file.side_path,
        revisits: file.revisit,
        readiness: file.readiness,
        connections: file.connection,
    }
}

fn check(s: &Series, problems: &HashMap<&str, (&str, bool)>, cookbook: &Cookbook, found: &mut Vec<String>) {
    if s.title.trim().is_empty() {
        found.push("the series needs a title".to_owned());
    }
    if s.parts.is_empty() {
        found.push("the series has no [[part]]".to_owned());
    }
    // Each id's place in the whole path: parts, then steps.
    let mut place: HashMap<&str, usize> = HashMap::new();
    for (i, item) in s.items().enumerate() {
        if let Some(id) = &item.id
            && place.insert(id, i).is_some()
        {
            found.push(format!("{id} is listed twice"));
        }
    }
    let mut groups = HashSet::new();
    let mut last = "";
    for p in &s.parts {
        if p.group != last && !groups.insert(p.group.as_str()) {
            found.push(format!("stage {:?} is split: keep its parts together", p.group));
        }
        last = &p.group;
    }
    if let Some(cycle) = required_cycle(s) {
        found.push(format!("required links form a cycle: {}", cycle.join(" -> ")));
    }
    let page = |name: &str, id: &str, found: &mut Vec<String>| {
        if !cookbook.pages.contains_key(id) {
            found.push(format!("{name}: there is no cookbook page {id:?}"));
        }
    };
    if let Some(id) = s.project.cookbook.as_deref() {
        page("capstone_cookbook", id, found);
    }
    let in_series = |name: &str, what: &str, id: &str, found: &mut Vec<String>| {
        if !place.contains_key(id) {
            found.push(format!("{name}: {what} {id:?} is not in the series"));
        }
    };
    let part = |name: &str, what: &str, id: &str, found: &mut Vec<String>| {
        if !s.parts.iter().any(|p| p.id.as_deref() == Some(id)) {
            found.push(format!("{name}: {what} {id:?} is not a part of the series"));
        }
    };
    for (i, item) in s.items().enumerate() {
        let name = item.id.clone().unwrap_or_else(|| item.label());
        for r in &item.required {
            match place.get(r.as_str()) {
                None => found.push(format!("{name}: required {r:?} is not in the series")),
                Some(&at) if at >= i => found.push(format!("{name}: required {r:?} must come earlier in the series")),
                Some(_) => {}
            }
        }
        for h in &item.helpful {
            if !place.contains_key(h.id.as_str()) && !problems.contains_key(h.id.as_str()) {
                found.push(format!("{name}: helpful {:?} is neither in the series nor a problem", h.id));
            }
        }
        item.used_again.iter().for_each(|id| in_series(&name, "used_again", id, found));
        item.prepared_by.iter().for_each(|id| part(&name, "prepared_by", id, found));
        item.practised.iter().filter_map(|p| p.from.as_deref()).for_each(|id| in_series(&name, "practised.from", id, found));
        item.cookbook.iter().for_each(|c| page(&name, &c.page, found));
    }
    for o in &s.side_paths {
        if !problems.contains_key(o.id.as_str()) {
            found.push(format!("[[side_path]]: there is no problem {:?}", o.id));
        }
        o.before.iter().chain(&o.after).for_each(|id| in_series("[[side_path]]", "place", id, found));
    }
    for r in &s.revisits {
        part("[[revisit]]", "part", &r.part, found);
        if r.after.is_some() == r.before.is_some() {
            found.push(format!("[[revisit]] of {}: give one of `after` and `before`", r.part));
            continue;
        }
        let other = r.after.as_ref().or(r.before.as_ref()).map_or("", String::as_str);
        let Some(&at) = place.get(other) else {
            found.push(format!("[[revisit]] of {}: {other:?} is not in the series", r.part));
            continue;
        };
        // `before` x means after the item in front of x, so that one has to come after the part too.
        let anchor = if r.before.is_some() { at.checked_sub(1) } else { Some(at) };
        if place.get(r.part.as_str()).is_some_and(|&p| anchor.is_none_or(|a| a <= p)) {
            found.push(format!("[[revisit]]: {} must be repeated after a later part than itself", r.part));
        }
    }
    for r in &s.readiness {
        r.parts.iter().for_each(|id| part("[[readiness]]", "part", id, found));
        for n in r.steps.iter().filter(|&&n| n == 0 || n > s.steps.len()) {
            found.push(format!("[[readiness]] {:?}: there is no capstone step {n}", r.concept));
        }
    }
    for t in &s.connections {
        t.learn.iter().chain(&t.again).chain(&t.apply).for_each(|id| in_series("[[connection]]", "item", id, found));
    }
}

/// A chain of `required` links that comes back to where it started, if there is one.
fn required_cycle(s: &Series) -> Option<Vec<String>> {
    let edges: BTreeMap<&str, &[String]> = s.items().filter_map(|i| Some((i.id.as_deref()?, i.required.as_slice()))).collect();
    // Depth-first, without recursion: `path` is the chain being followed, `done` the ids with no cycle below them.
    let mut done: HashSet<&str> = HashSet::new();
    for &start in edges.keys() {
        let mut path: Vec<(&str, usize)> = vec![(start, 0)];
        while let Some(&(at, next)) = path.last() {
            let after = edges.get(at).and_then(|r| r.get(next));
            let Some(to) = after else {
                done.insert(at);
                path.pop();
                continue;
            };
            if let Some(top) = path.last_mut() {
                top.1 += 1;
            }
            if let Some(from) = path.iter().position(|(id, _)| id == to) {
                return Some(path[from..].iter().map(|(id, _)| (*id).to_owned()).chain([to.clone()]).collect());
            }
            if !done.contains(to.as_str()) {
                path.push((to.as_str(), 0));
            }
        }
    }
    None
}
