//! System design prep (docs/SYSTEM_DESIGN.md): concept pages and practice problems with written rubrics, loaded from
//! `<root>/sd/concepts/*.toml` and `<root>/sd/problems/*.toml`. A missing `sd` directory is an empty catalog.

use std::collections::BTreeMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::catalog::Issue;

/// The groups concept pages are filed under, in the order the Learn tab shows them.
pub const SD_GROUPS: &[&str] = &["fundamentals", "core", "pattern", "technology", "advanced", "distributed", "lld", "rust"];
/// The levels a rubric point can be expected at.
pub const SD_LEVELS: &[&str] = &["mid", "senior", "staff"];
/// The kinds of practice problem.
pub const SD_KINDS: &[&str] = &["hld", "api", "lld"];
/// The animated drawings the web app can show on a concept page (`web/src/components/SdFigure.tsx`). A name here needs a drawing there.
pub const SD_FIGURES: &[&str] = &["kafka-log", "consumer-groups", "hash-ring", "shards", "replication", "cache-aside", "write-fanout", "redis-structures", "btree-index", "queue-vs-stream"];

#[derive(Debug, Clone, Default)]
pub struct SdCatalog {
    /// By id (the file name).
    pub concepts: BTreeMap<String, Concept>,
    pub problems: BTreeMap<String, SdProblem>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Concept {
    #[serde(default)]
    pub id: String,
    pub title: String,
    pub group: String,
    /// Order inside its group.
    pub order: u32,
    pub minutes: u32,
    /// One or two sentences: what it is.
    pub lede: String,
    pub sections: Vec<Section>,
    /// One sentence to say in an interview.
    pub say: String,
    pub checks: Vec<Check>,
    #[serde(default)]
    pub related: Vec<String>,
    /// Animated drawings shown on the page, by name (one of [`SD_FIGURES`]), each placed after the section it names.
    #[serde(default)]
    pub figures: Vec<Figure>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Figure {
    pub name: String,
    /// The heading of the section it follows.
    pub after: String,
    pub caption: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Section {
    pub heading: String,
    /// Markdown.
    pub body: String,
}

/// An open question answered in your own words, then compared with `answer`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Check {
    pub prompt: String,
    pub answer: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SdProblem {
    #[serde(default)]
    pub id: String,
    pub title: String,
    pub kind: String,
    pub order: u32,
    pub minutes: u32,
    /// What the interviewer asks, as they would say it.
    pub prompt: String,
    /// The concept ids it leans on.
    pub uses: Vec<String>,
    #[serde(default)]
    pub companies: Vec<String>,
    #[serde(default)]
    pub hints: Vec<String>,
    pub steps: Vec<Step>,
    /// What an interviewer pushes on in the deep dives, one at a time.
    #[serde(default)]
    pub probes: Vec<Probe>,
}

/// A step of the delivery framework, with what to do, a reference answer, and the rubric points it is marked on.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Step {
    pub name: String,
    pub minutes: u32,
    pub prompt: String,
    /// Markdown: what a strong answer says at this step.
    pub reference: String,
    #[serde(default)]
    pub points: Vec<Point>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Point {
    /// Unique in the problem; a review card is named after it.
    pub id: String,
    pub text: String,
    /// `mid`, `senior` or `staff`: the level that is expected to say it.
    pub level: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Probe {
    pub question: String,
    pub answer: String,
}

fn read_dir_toml<T: for<'de> Deserialize<'de>>(dir: &Path, issues: &mut Vec<Issue>) -> Vec<(String, std::path::PathBuf, T)> {
    let Ok(entries) = std::fs::read_dir(dir) else { return Vec::new() };
    let mut files: Vec<_> = entries.filter_map(|e| e.ok()).map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x == "toml")).collect();
    files.sort();
    let mut out = Vec::new();
    for path in files {
        let id = path.file_stem().and_then(|s| s.to_str()).unwrap_or_default().to_owned();
        match std::fs::read_to_string(&path).map_err(|e| e.to_string()).and_then(|t| toml::from_str::<T>(&t).map_err(|e| e.message().to_owned())) {
            Ok(v) => out.push((id, path, v)),
            Err(message) => issues.push(Issue { path, message }),
        }
    }
    out
}

/// Loads `<root>/sd`, reporting every problem with a file as an issue and leaving that file out.
pub(crate) fn load(root: &Path, issues: &mut Vec<Issue>) -> SdCatalog {
    let dir = root.join("sd");
    let mut cat = SdCatalog::default();
    let blank = |s: &str| s.trim().is_empty();
    for (id, path, mut c) in read_dir_toml::<Concept>(&dir.join("concepts"), issues) {
        let mut bad = Vec::new();
        if !SD_GROUPS.contains(&c.group.as_str()) {
            bad.push(format!("group {:?} is not one of {}", c.group, SD_GROUPS.join(", ")));
        }
        if blank(&c.title) || blank(&c.lede) || blank(&c.say) {
            bad.push("`title`, `lede` and `say` must not be empty".to_owned());
        }
        if c.sections.is_empty() || c.sections.iter().any(|s| blank(&s.heading) || blank(&s.body)) {
            bad.push("at least one section, each with a heading and a body".to_owned());
        }
        if c.checks.is_empty() || c.checks.iter().any(|k| blank(&k.prompt) || blank(&k.answer)) {
            bad.push("at least one check, each with a prompt and an answer".to_owned());
        }
        for f in &c.figures {
            if !SD_FIGURES.contains(&f.name.as_str()) {
                bad.push(format!("figure {:?} is not one the web app draws ({})", f.name, SD_FIGURES.join(", ")));
            }
            if !c.sections.iter().any(|s| s.heading == f.after) {
                bad.push(format!("figure {} follows {:?}, which is not a section heading", f.name, f.after));
            }
        }
        if bad.is_empty() {
            c.id = id.clone();
            cat.concepts.insert(id, c);
        } else {
            issues.push(Issue { path, message: bad.join("; ") });
        }
    }
    for (id, path, mut p) in read_dir_toml::<SdProblem>(&dir.join("problems"), issues) {
        let mut bad = Vec::new();
        if !SD_KINDS.contains(&p.kind.as_str()) {
            bad.push(format!("kind {:?} is not one of {}", p.kind, SD_KINDS.join(", ")));
        }
        if blank(&p.title) || blank(&p.prompt) {
            bad.push("`title` and `prompt` must not be empty".to_owned());
        }
        if p.steps.is_empty() {
            bad.push("at least one step".to_owned());
        }
        let mut seen = std::collections::HashSet::new();
        for s in &p.steps {
            if blank(&s.name) || blank(&s.prompt) || blank(&s.reference) {
                bad.push(format!("step {:?} needs a name, a prompt and a reference", s.name));
            }
            for pt in &s.points {
                if !SD_LEVELS.contains(&pt.level.as_str()) {
                    bad.push(format!("point {} has level {:?}, not one of {}", pt.id, pt.level, SD_LEVELS.join(", ")));
                }
                if blank(&pt.id) || blank(&pt.text) || !seen.insert(pt.id.clone()) {
                    bad.push(format!("point {:?} needs a unique id and a text", pt.id));
                }
            }
        }
        if p.probes.iter().any(|q| blank(&q.question) || blank(&q.answer)) {
            bad.push("every probe needs a question and an answer".to_owned());
        }
        if bad.is_empty() {
            p.id = id.clone();
            cat.problems.insert(id, p);
        } else {
            issues.push(Issue { path, message: bad.join("; ") });
        }
    }
    // links between files, once all are read
    for c in cat.concepts.values() {
        for r in c.related.iter().filter(|r| !cat.concepts.contains_key(*r)) {
            issues.push(Issue { path: dir.join("concepts").join(format!("{}.toml", c.id)), message: format!("related concept {r:?} does not exist") });
        }
    }
    for p in cat.problems.values() {
        for u in p.uses.iter().filter(|u| !cat.concepts.contains_key(*u)) {
            issues.push(Issue { path: dir.join("problems").join(format!("{}.toml", p.id)), message: format!("uses concept {u:?}, which does not exist") });
        }
    }
    cat
}
