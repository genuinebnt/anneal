//! The NeetCode lists as DSA tracks, loaded from `content/dsa/problems.json` (built by `tools/neetcode/build.py`).
//!
//! These problems aren't run in anneal: you solve them on LeetCode and log how it went. They load as ordinary tracks
//! (one per NeetCode pattern, ids `lc-<leetcode slug>`) so that streaks, reviews, readiness and the Progress pages
//! treat them like any other problem. [`Problem::dsa`](crate::Problem::dsa) carries what only they have.

use std::collections::BTreeMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::catalog::{Issue, Problem, ProblemFiles, Track};
use crate::model::{Band, Language, Mode, ProblemFile, Section, StageDef, Status, Tier};

/// Whether a problem introduces its technique or practises one introduced elsewhere.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    MustLearn,
    Practice,
}

/// A company that asks a problem, from the public company lists.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Company {
    pub name: String,
    /// One of [`DsaCatalog::company_groups`].
    pub group: String,
    /// How often it's asked, 0–100, relative to the company's other problems.
    pub frequency: f32,
    /// Asked in the last six months.
    pub recent: bool,
}

/// The written lesson for a problem (`content/dsa/pages/<slug>.toml`): the intuition, tips, and Python solutions that
/// paste into LeetCode. Text is markdown. `tools/neetcode/check_pages.py` checks the code.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Page {
    pub intuition: String,
    pub tips: Vec<String>,
    pub approaches: Vec<Approach>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Approach {
    pub name: String,
    /// A two-word tag for the tab, e.g. "hash map".
    pub label: String,
    pub idea: String,
    pub code: String,
    pub time: String,
    pub space: String,
    /// Why the time and space are what they are, in a sentence or two each. Optional while the pages are being filled in.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub time_why: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub space_why: String,
    pub note: String,
}

/// What a DSA problem has beyond a Rust one.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct DsaProblem {
    pub slug: String,
    /// LeetCode's number.
    pub number: u32,
    /// `blind75`, `neetcode150`, `neetcode250`, `all`: the lists it's in, narrowest first.
    pub lists: Vec<String>,
    /// Needs LeetCode Premium.
    pub premium: bool,
    pub companies: Vec<Company>,
    /// NeetCode's video, if there is one.
    pub video: Option<String>,
    /// The technique it belongs to, e.g. `Graphs:topo`.
    pub technique: String,
    pub role: Role,
    /// `must`, `strong`, `practice` or `warmup`. In the lists the must-learn problem is `must` and the rest `practice`; outside them
    /// it is computed from company frequency and concept weight (docs/DSA_LEARN_PAGE_SPEC.md).
    pub priority: String,
    /// For a practice problem, the must-learn problem that teaches its technique.
    pub practice_of: Option<String>,
    /// The key insight: one sentence naming the move that solves it (`dsa/insights.json`, docs/DSA_LEARN_PAGE_SPEC.md section 8).
    pub insight: String,
    /// The written lesson, when there is one. Served by its own endpoint, not with the lists.
    #[serde(skip)]
    pub page: Option<Page>,
}

/// One idea a pattern lesson teaches.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Technique {
    pub id: String,
    pub pattern: String,
    pub name: String,
    /// The problem id that teaches it first.
    pub must_learn: String,
    pub problems: u32,
}

/// A group of companies the filters offer together, e.g. Big Tech.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CompanyGroup {
    pub name: String,
    pub companies: Vec<String>,
}

#[derive(Debug, Clone, Default)]
pub struct DsaCatalog {
    pub techniques: Vec<Technique>,
    /// In display order.
    pub company_groups: Vec<CompanyGroup>,
    /// The pattern lessons, by technique id (`content/dsa/lessons/<pattern>.toml`).
    pub lessons: BTreeMap<String, Lesson>,
    /// A sentence or two introducing each pattern's lessons, by pattern name.
    pub lesson_intros: BTreeMap<String, String>,
    /// The groups a pattern's lessons are sorted into, in display order, by pattern name (empty: no grouping).
    pub lesson_groups: BTreeMap<String, Vec<String>>,
    /// Lessons for techniques with no must-learn problem in the lists, by pattern name, in file order (decision 32).
    pub extras: BTreeMap<String, Vec<Extra>>,
    /// Techniques that are known and not written yet, by pattern name, in file order.
    pub listed: BTreeMap<String, Vec<Listed>>,
    /// The Idea tab of each pattern that has one, by pattern name (`content/dsa/ideas/<pattern>.toml`, docs/DSA_IDEA_TAB.md).
    pub ideas: BTreeMap<String, Idea>,
}

/// The animations the web app can draw for an Idea tab. A name here needs a simulator in `web/src/ideaDemos.ts`.
pub const IDEA_DEMOS: &[&str] = &["window-fixed", "window-variable", "pointers-opposite", "pointers-same", "search-find", "search-first-true", "stack-brackets", "stack-mono"];
/// The small pictures of a window or pointer layout beside a row of "which one". Drawn by `web/src/components/IdeaGlyph.tsx`.
pub const IDEA_GLYPHS: &[&str] = &["fixed", "longest", "shortest", "counting", "frequency", "deque", "opposite", "samedir", "ksum", "merge", "greedy", "peaks", "halving", "boundary", "answer", "rotated", "partition", "mountain", "nested", "cancel", "monotone", "contrib", "runstate", "evaluate"];

/// One row of "which one?": what the question says, the kind of the pattern it points to, the move, and the technique card to open.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IdeaWhich {
    pub says: String,
    pub kind: String,
    #[serde(rename = "move")]
    pub moves: String,
    pub glyph: String,
    /// A technique or extra id of the pattern, e.g. `Sliding Window:fixed-sum`.
    pub technique: String,
}

/// One row of "with other patterns": the other pattern, how the two meet, and an example problem.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IdeaWith {
    /// The other pattern's name, as in `problems.json`.
    pub pattern: String,
    pub how: String,
    /// The slug of a catalog problem that shows the two together, e.g. `sliding-window-maximum`.
    pub example: String,
}

/// One row of "time and space": a variation of the pattern, its two costs and the reason, in a sentence or two.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IdeaCost {
    pub name: String,
    /// e.g. `O(n)`.
    pub time: String,
    pub space: String,
    pub why: String,
}

/// A pattern's Idea tab: what it is, a step-through, where it applies, how to tell it is this pattern and which one, how it
/// meets the other patterns, and how to apply it. Text may use `**bold**` and `` `code` ``.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Idea {
    /// The pattern's name, as in `problems.json`.
    pub pattern: String,
    /// The idea in one sentence (at most 140 characters): under the loop at the top of the tab, and on the pattern's card.
    pub tagline: String,
    pub what: Vec<String>,
    /// The animations, from [`IDEA_DEMOS`], in order.
    pub demos: Vec<String>,
    pub applies: Vec<String>,
    pub yes: Vec<String>,
    pub no: Vec<String>,
    pub which: Vec<IdeaWhich>,
    /// Time and space of each variation, the plain way first for comparison.
    pub cost: Vec<IdeaCost>,
    #[serde(default, rename = "with")]
    pub with: Vec<IdeaWith>,
    pub apply: Vec<String>,
}

/// One more tab of a template: the same idea written another way (recursive and iterative DFS).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Variant {
    pub name: String,
    /// One sentence on when this version is the one to write.
    #[serde(default)]
    pub note: String,
    pub template: String,
}

/// A lesson for a technique that has no must-learn problem in the NeetCode lists. It is taught with example problems.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Extra {
    /// `<Pattern>:<slug>`, unique, never a technique id.
    pub id: String,
    pub name: String,
    pub group: String,
    pub signals: Vec<String>,
    pub template: String,
    /// The name of the first tab when there are variants.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub template_name: Option<String>,
    #[serde(default, rename = "variant", skip_serializing_if = "Vec::is_empty")]
    pub variants: Vec<Variant>,
    pub pitfalls: Vec<String>,
    /// Slugs of problems in the lists that use the technique (they are `lc-<slug>` in `problems.json`).
    pub examples: Vec<String>,
}

/// A LeetCode problem that is not in the NeetCode lists, found in LeetCode's own index (`tools/neetcode/pattern_pages.py`), so it is
/// shown with a link and is not tracked in anneal.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct External {
    pub slug: String,
    pub title: String,
    pub number: u32,
    /// `easy`, `medium` or `hard`.
    pub difficulty: String,
    #[serde(default)]
    pub premium: bool,
}

/// A technique that belongs on the page's map and has no lesson yet.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Listed {
    pub name: String,
    pub group: String,
    /// What is missing or how it relates to a written lesson.
    #[serde(default)]
    pub note: String,
    /// LeetCode problems that practise it (outside the lists), verified against LeetCode's index.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub problems: Vec<External>,
}

/// What a pattern lesson says about one technique: when to reach for it, a Python template to adapt, and the usual traps.
/// `tools/neetcode/check_lessons.py` runs the template.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Lesson {
    /// The technique id, e.g. `Graphs:topo`.
    pub id: String,
    /// The signals in a problem that call for it.
    pub signals: Vec<String>,
    pub template: String,
    pub pitfalls: Vec<String>,
    /// The group of the pattern's page this lesson is shown under.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group: Option<String>,
    /// The name of the first tab when there are variants.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub template_name: Option<String>,
    #[serde(default, rename = "variant", skip_serializing_if = "Vec::is_empty")]
    pub variants: Vec<Variant>,
}

/// `<root>/dsa/lessons/<pattern>.toml`: the lessons of one pattern.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LessonFile {
    /// The pattern's name, as in `problems.json`.
    pattern: String,
    intro: String,
    /// The groups of the page, in order. Every `group` below is one of them.
    #[serde(default)]
    groups: Vec<String>,
    technique: Vec<Lesson>,
    #[serde(default)]
    extra: Vec<Extra>,
    #[serde(default)]
    listed: Vec<Listed>,
}

#[derive(Deserialize)]
struct File {
    #[serde(default, deserialize_with = "ordered_groups")]
    company_groups: Vec<CompanyGroup>,
    techniques: Vec<Technique>,
    problems: Vec<Raw>,
}

#[derive(Deserialize)]
struct Raw {
    id: String,
    slug: String,
    number: u32,
    title: String,
    difficulty: Band,
    pattern: String,
    lists: Vec<String>,
    premium: bool,
    tags: Vec<String>,
    companies: Vec<Company>,
    #[serde(default)]
    video: Option<String>,
    technique: String,
    order: u32,
    role: Role,
    #[serde(default)]
    practice_of: Option<String>,
    #[serde(default)]
    priority: Option<String>,
}

const PRIORITIES: [&str; 4] = ["must", "strong", "practice", "warmup"];

/// A JSON object read as a list, keeping the file's order (a map would sort the keys).
fn ordered_groups<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Vec<CompanyGroup>, D::Error> {
    struct Groups;
    impl<'de> serde::de::Visitor<'de> for Groups {
        type Value = Vec<CompanyGroup>;
        fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
            f.write_str("an object of company groups")
        }
        fn visit_map<A: serde::de::MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
            let mut groups = Vec::new();
            while let Some((name, companies)) = map.next_entry::<String, Vec<String>>()? {
                groups.push(CompanyGroup { name, companies });
            }
            Ok(groups)
        }
    }
    d.deserialize_map(Groups)
}

/// What `load` produces: the pattern tracks (the NeetCode lists), the same patterns holding only their practice
/// problems, and the lists around them.
pub(crate) struct Loaded {
    pub tracks: Vec<Track>,
    pub practice_tracks: Vec<Track>,
    pub catalog: DsaCatalog,
}

impl Loaded {
    fn empty() -> Self {
        Loaded { tracks: Vec::new(), practice_tracks: Vec::new(), catalog: DsaCatalog::default() }
    }
}

#[derive(Deserialize)]
struct PracticeFile {
    problems: Vec<Raw>,
}

/// The ids in `<root>/dsa/practice.json` (empty when the file is absent or broken: `load_practice` reports that).
fn practice_ids(root: &Path) -> std::collections::BTreeSet<String> {
    let Ok(text) = std::fs::read_to_string(root.join("dsa").join("practice.json")) else { return Default::default() };
    serde_json::from_str::<PracticeFile>(&text).map(|f| f.problems.into_iter().map(|p| p.id).collect()).unwrap_or_default()
}

/// Ids of NeetCode-list problems that no company of the site's set asks (`<root>/dsa/no_company.txt`): the only exception to the rule
/// that every problem carries a company.
fn no_company_waivers(root: &Path) -> std::collections::BTreeSet<String> {
    let Ok(text) = std::fs::read_to_string(root.join("dsa").join("no_company.txt")) else { return Default::default() };
    text.lines().map(str::trim).filter(|l| !l.is_empty() && !l.starts_with('#')).map(str::to_owned).collect()
}

/// The longest key insight, in characters (rule K2).
const INSIGHT_MAX: usize = 110;

/// `<root>/dsa/insights.json`: `{ "<slug>": "<sentence>" }`, the key insight of every problem (rules K1 to K5). `None` when the file
/// is absent (the check is then off; a test pins that the site's own content has the file). Bad sentences are reported here.
fn load_insights(root: &Path, issues: &mut Vec<Issue>) -> Option<BTreeMap<String, String>> {
    let path = root.join("dsa").join("insights.json");
    let text = std::fs::read_to_string(&path).ok()?;
    let map: BTreeMap<String, String> = match serde_json::from_str(&text) {
        Ok(m) => m,
        Err(e) => {
            issues.push(Issue { path, message: format!("invalid JSON: {e}") });
            return Some(BTreeMap::new());
        }
    };
    for (slug, sentence) in &map {
        let n = sentence.chars().count();
        if sentence.trim().is_empty() || n > INSIGHT_MAX || sentence.contains('`') || sentence.contains('\n') {
            issues.push(Issue { path: path.clone(), message: format!("{slug}: the insight must be one plain sentence of at most {INSIGHT_MAX} characters ({n} now)") });
        }
    }
    Some(map)
}

/// Reads `<root>/dsa/problems.json` and `practice.json`. Absent means no DSA section; a broken file is reported, not fatal.
pub(crate) fn load(root: &Path, issues: &mut Vec<Issue>) -> Loaded {
    let path = root.join("dsa").join("problems.json");
    let Ok(text) = std::fs::read_to_string(&path) else {
        return Loaded::empty();
    };
    let file: File = match serde_json::from_str(&text) {
        Ok(f) => f,
        Err(e) => {
            issues.push(Issue { path, message: format!("invalid JSON: {e}") });
            return Loaded::empty();
        }
    };
    let by_id: BTreeMap<&str, &Raw> = file.problems.iter().map(|p| (p.id.as_str(), p)).collect();
    for t in &file.techniques {
        match by_id.get(t.must_learn.as_str()) {
            Some(p) if p.role == Role::MustLearn && p.technique == t.id => {}
            _ => issues.push(Issue { path: path.clone(), message: format!("technique {}: {} isn't its must-learn problem", t.id, t.must_learn) }),
        }
    }
    let waived = no_company_waivers(root);
    let insights = load_insights(root, issues);
    for p in &file.problems {
        if insights.as_ref().is_some_and(|m| !m.contains_key(&p.slug)) {
            issues.push(Issue { path: path.clone(), message: format!("{}: needs a key insight in dsa/insights.json", p.id) });
        }
        if p.companies.is_empty() && !waived.contains(&p.id) {
            issues.push(Issue { path: path.clone(), message: format!("{}: needs at least one company (mandatory; see dsa/no_company.txt)", p.id) });
        }
        if p.tags.is_empty() {
            issues.push(Issue { path: path.clone(), message: format!("{}: needs at least one LeetCode topic tag (mandatory)", p.id) });
        }
        if p.priority.as_deref().is_some_and(|v| !PRIORITIES.contains(&v)) {
            issues.push(Issue { path: path.clone(), message: format!("{}: priority must be one of {PRIORITIES:?}", p.id) });
        }
        if let Some(teacher) = &p.practice_of
            && by_id.get(teacher.as_str()).is_none_or(|t| t.role != Role::MustLearn)
        {
            issues.push(Issue { path: path.clone(), message: format!("{}: practice_of {teacher} isn't a must-learn problem", p.id) });
        }
    }

    // One track per pattern, in NeetCode's order.
    let mut patterns: Vec<(&str, u32)> = Vec::new();
    for p in &file.problems {
        match patterns.iter_mut().find(|(name, _)| *name == p.pattern) {
            Some((_, first)) => *first = (*first).min(p.order),
            None => patterns.push((&p.pattern, p.order)),
        }
    }
    patterns.sort_by_key(|&(_, first)| first);
    let pages = load_pages(root, &by_id, issues);
    let names: BTreeMap<&str, &str> = file.techniques.iter().map(|t| (t.id.as_str(), t.name.as_str())).collect();

    let tracks: Vec<Track> = patterns
        .iter()
        .enumerate()
        .map(|(i, (pattern, _))| {
            let number = i + 1;
            let mut problems: Vec<Problem> = file
                .problems
                .iter()
                .filter(|p| p.pattern == *pattern)
                .map(|p| problem(p, &names, root, pages.get(p.slug.as_str()), insights.as_ref()))
                .collect();
            problems.sort_by_key(|p| p.meta.order);
            Track {
                slug: format!("d{number}-{}", slugify(pattern)),
                code: format!("D{number}"),
                name: (*pattern).to_owned(),
                section: Section::Dsa,
                tier: Tier::Core,
                order: number as u32,
                summary: String::new(),
                pattern: None,
                stages: [Band::Easy, Band::Medium, Band::Hard]
                    .into_iter()
                    .map(|band| StageDef { slug: band_name(band).into(), name: capitalized(band_name(band)), band })
                    .collect(),
                problems,
            }
        })
        .collect();
    let practice_set = practice_ids(root);
    let loaded = load_lessons(root, &file.techniques, &by_id, &practice_set, issues);
    let known_problem = |slug: &str| by_id.contains_key(format!("lc-{slug}").as_str()) || practice_set.contains(&format!("lc-{slug}"));
    let ideas = load_ideas(root, &file.techniques, &loaded.extras, &known_problem, issues);
    let extra_techniques: BTreeMap<String, String> = loaded.extras.iter().flat_map(|(pat, v)| v.iter().map(move |e| (e.id.clone(), pat.clone()))).collect();
    let practice_tracks = load_practice(root, &file, &tracks, &names, &extra_techniques, insights.as_ref(), issues);
    if let Some(m) = &insights {
        let known = |slug: &str| by_id.contains_key(format!("lc-{slug}").as_str()) || practice_set.contains(&format!("lc-{slug}"));
        for slug in m.keys().filter(|k| !known(k)) {
            issues.push(Issue { path: root.join("dsa").join("insights.json"), message: format!("{slug}: isn't a problem of the site") });
        }
    }
    Loaded {
        tracks,
        practice_tracks,
        catalog: DsaCatalog {
            techniques: file.techniques,
            company_groups: file.company_groups,
            lessons: loaded.lessons,
            lesson_intros: loaded.intros,
            lesson_groups: loaded.groups,
            extras: loaded.extras,
            listed: loaded.listed,
            ideas,

        },
    }
}

/// `<root>/dsa/practice.json`: LeetCode problems **outside** the NeetCode lists that drill a technique (decision 24).
/// They load as problems in a hidden copy of each pattern's track, so they can be logged and show in activity, but never
/// count toward the lists, a track's readiness or the review schedule.
fn load_practice(root: &Path, main: &File, tracks: &[Track], names: &BTreeMap<&str, &str>, extras: &BTreeMap<String, String>, insights: Option<&BTreeMap<String, String>>, issues: &mut Vec<Issue>) -> Vec<Track> {
    let path = root.join("dsa").join("practice.json");
    let Ok(text) = std::fs::read_to_string(&path) else {
        return Vec::new();
    };
    let file: PracticeFile = match serde_json::from_str(&text) {
        Ok(f) => f,
        Err(e) => {
            issues.push(Issue { path, message: format!("invalid JSON: {e}") });
            return Vec::new();
        }
    };
    let known: std::collections::HashSet<&str> = main.problems.iter().map(|p| p.id.as_str()).collect();
    let techniques: BTreeMap<&str, &Technique> = main.techniques.iter().map(|t| (t.id.as_str(), t)).collect();
    let mut seen = std::collections::HashSet::new();
    let mut by_pattern: BTreeMap<&str, Vec<Problem>> = BTreeMap::new();
    for p in &file.problems {
        let problem_issue = |message: String| Issue { path: path.clone(), message: format!("{}: {message}", p.id) };
        if known.contains(p.id.as_str()) {
            issues.push(problem_issue("is already in the NeetCode lists".into()));
        } else if !seen.insert(p.id.as_str()) {
            issues.push(problem_issue("is listed twice".into()));
        } else if p.premium {
            issues.push(problem_issue("needs LeetCode Premium; practice problems are free ones".into()));
        } else if p.companies.is_empty() {
            issues.push(problem_issue("needs at least one company of the site's set (mandatory)".into()));
        } else if p.tags.is_empty() {
            issues.push(problem_issue("needs at least one LeetCode topic tag (mandatory)".into()));
        } else if p.priority.as_deref().is_some_and(|v| !PRIORITIES.contains(&v)) {
            issues.push(problem_issue(format!("priority must be one of {PRIORITIES:?}")));
        } else if !techniques.contains_key(p.technique.as_str()) && !extras.contains_key(&p.technique) {
            issues.push(problem_issue(format!("unknown technique {}", p.technique)));
        } else if techniques.get(p.technique.as_str()).map(|t| t.pattern.as_str()).or_else(|| extras.get(&p.technique).map(String::as_str)) != Some(p.pattern.as_str()) {
            issues.push(problem_issue(format!("technique {} does not belong to the pattern {}", p.technique, p.pattern)));
        } else if !tracks.iter().any(|t| t.name == p.pattern) {
            issues.push(problem_issue(format!("unknown pattern {}", p.pattern)));
        } else {
            if insights.is_some_and(|m| !m.contains_key(&p.slug)) {
                issues.push(problem_issue("needs a key insight in dsa/insights.json".into()));
            }
            by_pattern.entry(p.pattern.as_str()).or_default().push(problem(p, names, root, None, insights));
        }
    }
    tracks
        .iter()
        .filter_map(|t| {
            let mut problems = by_pattern.remove(t.name.as_str())?;
            problems.sort_by_key(|p| p.meta.order);
            Some(Track {
                slug: t.slug.clone(),
                code: t.code.clone(),
                name: t.name.clone(),
                section: Section::Dsa,
                tier: t.tier,
                order: t.order,
                summary: String::new(),
                pattern: None,
                stages: t.stages.clone(),
                problems,
            })
        })
        .collect()
}

fn problem(p: &Raw, names: &BTreeMap<&str, &str>, root: &Path, page: Option<&Page>, insights: Option<&BTreeMap<String, String>>) -> Problem {
    Problem {
        id: p.id.clone(),
        meta: ProblemFile {
            slug: p.slug.clone(),
            title: p.title.clone(),
            mode: Mode::Write,
            language: Language::Python,
            unlocked_by: Vec::new(),
            warmup: false,
            level: p.difficulty,
            stage: band_name(p.difficulty).into(),
            order: p.order,
            status: Status::Ready,
            tags: p.tags.clone(),
            companies: p.companies.iter().map(|c| c.name.clone()).collect(),
            teaches: names.get(p.technique.as_str()).map(|n| vec![(*n).to_owned()]).unwrap_or_default(),
            constraints: Vec::new(),
            examples: Vec::new(),
            hints: Vec::new(),
            follow_up: None,
            related: Vec::new(),
            solution: None,
            rules: None,
            source: None,
            renamed_from: Vec::new(),
            crates: Vec::new(),
            perf: None,
        },
        dir: root.join("dsa"),
        files: ProblemFiles::default(),
        dsa: Some(DsaProblem {
            slug: p.slug.clone(),
            number: p.number,
            lists: p.lists.clone(),
            premium: p.premium,
            companies: p.companies.clone(),
            video: p.video.clone(),
            technique: p.technique.clone(),
            role: p.role,
            priority: p.priority.clone().unwrap_or_else(|| if p.role == Role::MustLearn { "must".into() } else { "practice".into() }),
            practice_of: p.practice_of.clone(),
            insight: insights.and_then(|m| m.get(&p.slug)).cloned().unwrap_or_default(),
            page: page.cloned(),
        }),
    }
}

/// `<root>/dsa/ideas/*.toml`: the Idea tabs. A file for an unknown pattern or a second one for the same pattern is reported, and so is
/// an empty section, an animation or picture the web app can't draw, a row that opens a technique the pattern doesn't have, and a
/// "with" row for a pattern that doesn't exist (or for the pattern itself).
fn load_ideas(root: &Path, techniques: &[Technique], extras: &BTreeMap<String, Vec<Extra>>, known_problem: &dyn Fn(&str) -> bool, issues: &mut Vec<Issue>) -> BTreeMap<String, Idea> {
    let dir = root.join("dsa").join("ideas");
    let mut out = BTreeMap::new();
    let Ok(entries) = std::fs::read_dir(&dir) else { return out };
    let mut files: Vec<_> = entries.filter_map(|e| e.ok()).map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x == "toml")).collect();
    files.sort();
    for path in files {
        let idea: Idea = match std::fs::read_to_string(&path).map_err(|e| e.to_string()).and_then(|t| toml::from_str(&t).map_err(|e| e.message().to_owned())) {
            Ok(i) => i,
            Err(message) => {
                issues.push(Issue { path, message });
                continue;
            }
        };
        let mut bad: Vec<String> = Vec::new();
        if !techniques.iter().any(|t| t.pattern == idea.pattern) {
            issues.push(Issue { path, message: format!("unknown pattern {}", idea.pattern) });
            continue;
        }
        if out.contains_key(&idea.pattern) {
            issues.push(Issue { path, message: format!("{} has two idea files", idea.pattern) });
            continue;
        }
        if idea.tagline.trim().is_empty() || idea.tagline.chars().count() > 140 {
            bad.push("`tagline` is one sentence of at most 140 characters".into());
        }
        let blank = |v: &[String]| v.is_empty() || v.iter().any(|t| t.trim().is_empty());
        for (name, v) in [("what", &idea.what), ("applies", &idea.applies), ("yes", &idea.yes), ("no", &idea.no), ("apply", &idea.apply), ("demos", &idea.demos)] {
            if blank(v) {
                bad.push(format!("`{name}` needs at least one entry and none may be empty"));
            }
        }
        for d in idea.demos.iter().filter(|d| !IDEA_DEMOS.contains(&d.as_str())) {
            bad.push(format!("the demo {d:?} isn't one the web app can draw ({})", IDEA_DEMOS.join(", ")));
        }
        if idea.which.is_empty() {
            bad.push("`which` needs at least one row".into());
        }
        let mine = |id: &str| techniques.iter().any(|t| t.id == id && t.pattern == idea.pattern) || extras.get(&idea.pattern).is_some_and(|v| v.iter().any(|e| e.id == id));
        for w in &idea.which {
            if [&w.says, &w.kind, &w.moves].iter().any(|t| t.trim().is_empty()) {
                bad.push(format!("a `which` row ({:?}) has an empty field", w.says));
            }
            if !IDEA_GLYPHS.contains(&w.glyph.as_str()) {
                bad.push(format!("the picture {:?} isn't one the web app can draw ({})", w.glyph, IDEA_GLYPHS.join(", ")));
            }
            if !mine(&w.technique) {
                bad.push(format!("{:?} isn't a technique of {}", w.technique, idea.pattern));
            }
        }
        if idea.cost.is_empty() {
            bad.push("`cost` needs at least one row".into());
        }
        for c in &idea.cost {
            if [&c.name, &c.time, &c.space, &c.why].iter().any(|t| t.trim().is_empty()) {
                bad.push(format!("the `cost` row {:?} has an empty field", c.name));
            }
        }
        for w in &idea.with {
            if w.how.trim().is_empty() || w.example.trim().is_empty() {
                bad.push(format!("the `with` row for {} has an empty field", w.pattern));
            }
            if !known_problem(&w.example) {
                bad.push(format!("the `with` row for {} gives the example {:?}, which isn't a problem of the site", w.pattern, w.example));
            }
            if w.pattern == idea.pattern || !techniques.iter().any(|t| t.pattern == w.pattern) {
                bad.push(format!("the `with` row names {:?}, which isn't another pattern", w.pattern));
            }
        }
        if bad.is_empty() {
            out.insert(idea.pattern.clone(), idea);
        } else {
            issues.extend(bad.into_iter().map(|message| Issue { path: path.clone(), message }));
        }
    }
    out
}

/// What `load_lessons` read.
#[derive(Default)]
struct LoadedLessons {
    lessons: BTreeMap<String, Lesson>,
    intros: BTreeMap<String, String>,
    groups: BTreeMap<String, Vec<String>>,
    extras: BTreeMap<String, Vec<Extra>>,
    listed: BTreeMap<String, Vec<Listed>>,
}

/// `<root>/dsa/lessons/*.toml`: the pattern lessons. A lesson for an unknown technique, one filed under the wrong pattern,
/// a duplicate or an empty one is reported, and so is a group the file did not declare, an extra whose id is taken or whose
/// example is not a problem in the lists, and a variant without a template. Techniques without a lesson are allowed (the
/// page says so).
fn load_lessons(root: &Path, techniques: &[Technique], by_id: &BTreeMap<&str, &Raw>, practice: &std::collections::BTreeSet<String>, issues: &mut Vec<Issue>) -> LoadedLessons {
    let dir = root.join("dsa").join("lessons");
    let mut out = LoadedLessons::default();
    let Ok(entries) = std::fs::read_dir(&dir) else { return out };
    let mut files: Vec<_> = entries.filter_map(|e| e.ok()).map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x == "toml")).collect();
    files.sort();
    for path in files {
        let file: LessonFile = match std::fs::read_to_string(&path).map_err(|e| e.to_string()).and_then(|t| toml::from_str(&t).map_err(|e| e.message().to_owned())) {
            Ok(f) => f,
            Err(message) => {
                issues.push(Issue { path, message });
                continue;
            }
        };
        if !techniques.iter().any(|t| t.pattern == file.pattern) {
            issues.push(Issue { path, message: format!("unknown pattern {}", file.pattern) });
            continue;
        }
        if file.intro.trim().is_empty() {
            issues.push(Issue { path: path.clone(), message: "the intro is empty".into() });
        }
        let issue = |what: &str, message: String| Issue { path: path.clone(), message: format!("{what}: {message}") };
        let group_ok = |group: &str| file.groups.is_empty() || file.groups.iter().any(|g| g == group);
        let variants_ok = |template_name: &Option<String>, variants: &[Variant]| -> Option<String> {
            if variants.is_empty() {
                return None;
            }
            let mut names: Vec<&str> = vec![template_name.as_deref().unwrap_or("")];
            for v in variants {
                if v.name.trim().is_empty() || v.template.trim().is_empty() {
                    return Some("a variant needs a name and a template".into());
                }
                names.push(&v.name);
            }
            if names[0].trim().is_empty() {
                return Some("variants need `template_name` for the first tab".into());
            }
            let mut sorted = names.clone();
            sorted.sort();
            sorted.dedup();
            (sorted.len() != names.len()).then(|| "two tabs have the same name".into())
        };
        out.intros.insert(file.pattern.clone(), file.intro.clone());
        if !file.groups.is_empty() {
            out.groups.insert(file.pattern.clone(), file.groups.clone());
        }
        for lesson in file.technique {
            let problem = |message: String| issue(&lesson.id, message);
            match techniques.iter().find(|t| t.id == lesson.id) {
                None => issues.push(problem("isn't a technique".into())),
                Some(t) if t.pattern != file.pattern => issues.push(problem(format!("belongs to {}, not {}", t.pattern, file.pattern))),
                Some(_) if out.lessons.contains_key(&lesson.id) => issues.push(problem("has two lessons".into())),
                Some(_) if lesson.signals.is_empty() || lesson.pitfalls.is_empty() || lesson.template.trim().is_empty() => {
                    issues.push(problem("needs signals, a template and pitfalls".into()));
                }
                Some(_) if lesson.group.as_deref().is_some_and(|g| !group_ok(g)) => issues.push(problem(format!("the group {:?} is not declared in `groups`", lesson.group.as_deref().unwrap_or("")))),
                Some(_) if variants_ok(&lesson.template_name, &lesson.variants).is_some() => {
                    issues.push(problem(variants_ok(&lesson.template_name, &lesson.variants).unwrap_or_default()));
                }
                Some(_) => {
                    out.lessons.insert(lesson.id.clone(), lesson);
                }
            }
        }
        let prefix = format!("{}:", file.pattern);
        let mut seen: Vec<String> = Vec::new();
        for extra in file.extra {
            let problem = |message: String| issue(&extra.id, message);
            if !extra.id.starts_with(&prefix) || extra.id.len() == prefix.len() {
                issues.push(problem(format!("an extra's id starts with {prefix}")));
            } else if techniques.iter().any(|t| t.id == extra.id) || seen.contains(&extra.id) {
                issues.push(problem("the id is already taken".into()));
            } else if extra.name.trim().is_empty() || extra.signals.is_empty() || extra.pitfalls.is_empty() || extra.template.trim().is_empty() {
                issues.push(problem("needs a name, signals, a template and pitfalls".into()));
            } else if extra.examples.is_empty() {
                issues.push(problem("needs at least one example problem (use `listed` for a technique with no lesson)".into()));
            } else if !group_ok(&extra.group) {
                issues.push(problem(format!("the group {:?} is not declared in `groups`", extra.group)));
            } else if let Some(m) = variants_ok(&extra.template_name, &extra.variants) {
                issues.push(problem(m));
            } else if let Some(bad) = extra.examples.iter().find(|s| {
                let id = format!("lc-{s}");
                !by_id.contains_key(id.as_str()) && !practice.contains(&id)
            }) {
                issues.push(problem(format!("the example {bad} isn't a problem of the site (the NeetCode lists or practice.json)")));
            } else {
                seen.push(extra.id.clone());
                out.extras.entry(file.pattern.clone()).or_default().push(extra);
            }
        }
        for item in file.listed {
            let bad_problem = item.problems.iter().find(|p| p.slug.trim().is_empty() || p.title.trim().is_empty() || p.number == 0 || !matches!(p.difficulty.as_str(), "easy" | "medium" | "hard"));
            if item.name.trim().is_empty() {
                issues.push(issue("listed", "a name is empty".into()));
            } else if !group_ok(&item.group) {
                issues.push(issue(&item.name, format!("the group {:?} is not declared in `groups`", item.group)));
            } else if let Some(p) = bad_problem {
                issues.push(issue(&item.name, format!("the problem {:?} needs a slug, a title, a number and easy, medium or hard", p.slug)));
            } else {
                out.listed.entry(file.pattern.clone()).or_default().push(item);
            }
        }
    }
    out
}

/// `<root>/dsa/pages/<slug>.toml`, by slug. A page for a problem that isn't in the lists is reported.
fn load_pages(root: &Path, by_id: &BTreeMap<&str, &Raw>, issues: &mut Vec<Issue>) -> BTreeMap<String, Page> {
    let dir = root.join("dsa").join("pages");
    let mut pages = BTreeMap::new();
    let Ok(entries) = std::fs::read_dir(&dir) else { return pages };
    let mut files: Vec<_> = entries.filter_map(|e| e.ok()).map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x == "toml")).collect();
    files.sort();
    for path in files {
        let slug = path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
        if !by_id.contains_key(format!("lc-{slug}").as_str()) {
            issues.push(Issue { path, message: format!("{slug} isn't a problem in the NeetCode lists") });
            continue;
        }
        match std::fs::read_to_string(&path).map_err(|e| e.to_string()).and_then(|t| toml::from_str::<Page>(&t).map_err(|e| e.message().to_owned())) {
            Ok(page) if page.approaches.is_empty() => issues.push(Issue { path, message: "a page needs at least one approach".into() }),
            Ok(page) => {
                pages.insert(slug, page);
            }
            Err(message) => issues.push(Issue { path, message }),
        }
    }
    pages
}

fn band_name(band: Band) -> &'static str {
    match band {
        Band::Easy => "easy",
        Band::Medium => "medium",
        Band::Hard => "hard",
    }
}

fn capitalized(word: &str) -> String {
    let mut chars = word.chars();
    chars.next().map(|c| c.to_uppercase().chain(chars).collect()).unwrap_or_default()
}

/// `Heap / Priority Queue` becomes `heap-priority-queue`.
fn slugify(name: &str) -> String {
    let mut out = String::new();
    for c in name.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
        } else if !out.ends_with('-') && !out.is_empty() {
            out.push('-');
        }
    }
    out.trim_end_matches('-').to_owned()
}

#[cfg(test)]
mod tests {
    use super::slugify;

    #[test]
    fn slugs_are_folder_safe() {
        assert_eq!(slugify("Heap / Priority Queue"), "heap-priority-queue");
        assert_eq!(slugify("1-D Dynamic Programming"), "1-d-dynamic-programming");
        assert_eq!(slugify("Math & Geometry"), "math-geometry");
        assert_eq!(slugify("Arrays & Hashing"), "arrays-hashing");
    }
}
