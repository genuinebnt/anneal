//! System design prep (docs/SYSTEM_DESIGN.md): concept pages and practice problems with written rubrics, loaded from
//! `<root>/sd/concepts/*.toml` and `<root>/sd/problems/*.toml`. A missing `sd` directory is an empty catalog.
//!
//! The SQL performance course (docs/SQL_COURSE.md) uses the same page format: `<root>/sql/pages/*.toml`, filed under
//! [`SQL_GROUPS`], with query plans captured from a real database into `<root>/sql/plans/<id>.json` by
//! `tools/sqlperf/capture.py`. Its pages join the same catalog, so checks, review cards and search work unchanged.

use std::collections::BTreeMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::catalog::Issue;

/// The groups concept pages are filed under, in the order the Learn tab shows them.
pub const SD_GROUPS: &[&str] = &["fundamentals", "core", "pattern", "technology", "advanced", "distributed", "lld", "rust"];
/// The parts of the SQL performance course, in course order; a SQL page's `group` is one of these.
pub const SQL_GROUPS: &[&str] = &["sql-indexes", "sql-plans", "sql-shapes", "sql-covering", "sql-internals", "sql-writes", "sql-special", "sql-scale", "sql-rust", "sql-engines"];
/// The levels a rubric point can be expected at.
pub const SD_LEVELS: &[&str] = &["mid", "senior", "staff"];
/// The kinds of practice problem.
pub const SD_KINDS: &[&str] = &["hld", "api", "lld"];
/// The animated drawings the web app can show on a concept page (`web/src/components/SdFigure.tsx`). A name here needs a drawing there.
pub const SD_FIGURES: &[&str] = &["kafka-log", "consumer-groups", "hash-ring", "shards", "replication", "cache-aside", "write-fanout", "redis-structures", "btree-index", "queue-vs-stream", "lsm-tree", "saga", "circuit-breaker", "quorum", "bloom-filter", "inverted-index", "cache-stampede"];
/// Figures added per group (each group's drawings live in `web/src/components/sdfigs/<group>.tsx`), kept apart so pages written in
/// parallel never edit the same list.
pub const SD_FIGURES_CORE: &[&str] = &["lock-vs-version", "read-layers"];
pub const SD_FIGURES_DISTRIBUTED: &[&str] = &[];
pub const SD_FIGURES_TECHNOLOGY: &[&str] = &[];
pub const SD_FIGURES_DRILLS: &[&str] = &[];

/// Every figure name the web app draws.
pub fn sd_figures() -> Vec<&'static str> {
    [SD_FIGURES, SD_FIGURES_CORE, SD_FIGURES_DISTRIBUTED, SD_FIGURES_TECHNOLOGY, SD_FIGURES_DRILLS].concat()
}

#[derive(Debug, Clone, Default)]
pub struct SdCatalog {
    /// By id (the file name).
    pub concepts: BTreeMap<String, Concept>,
    pub problems: BTreeMap<String, SdProblem>,
    /// Captured query plans by id, as `tools/sqlperf/capture.py` wrote them (the query, the setups, the JSON and text plans).
    pub plans: BTreeMap<String, serde_json::Value>,
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
    /// The attribution an openly licensed source asks for when the page adapts its text (Markdown), such as
    /// "Adapted in part from the System Design Primer by Donne Martin, CC BY 4.0". Listed in `content/sd/NOTICE.md` too.
    #[serde(default)]
    pub credit: Option<String>,
    /// Where the facts come from and where to read further: official documentation, papers, book chapters, articles.
    #[serde(default)]
    pub sources: Vec<Source>,
    /// "In 60 seconds": the page's main points, one sentence each, shown above the first section.
    #[serde(default)]
    pub summary: Vec<String>,
    /// Terms the page introduces, each tied to the section that explains it; the page lists those of the section being read.
    #[serde(default)]
    pub terms: Vec<Term>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Term {
    pub term: String,
    /// One short sentence.
    pub means: String,
    /// The heading of the section that explains it.
    pub section: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Source {
    pub title: String,
    pub url: String,
    /// What it is or what to read it for, in a few words.
    #[serde(default)]
    pub note: Option<String>,
    /// `paper`, `docs`, `guide`, `book`, `standard` or `article`; the page labels the source with it.
    #[serde(default)]
    pub kind: Option<String>,
}

/// The kinds a source may have.
pub const SD_SOURCE_KINDS: &[&str] = &["paper", "docs", "guide", "book", "standard", "article"];

/// The fence languages the web app highlights (`web/src/highlight.ts`).
pub const SD_CODE_LANGS: &[&str] = &["rust", "rs", "python", "py", "sql", "http", "json", "redis", "sh", "bash", "text", "plan"];

/// Checks the Markdown a page or a drill reference is written in against the grammar the web app reads (`web/src/components/SdMd.tsx`):
/// fences closed and in a known language with known options, display math and `:::steps` / `:::deeper` blocks closed, step lines
/// shaped `- who | what` (optionally `| narration`), callouts of a known kind, margin notes `^[…]` and inline math `\( … \)` closed.
pub fn check_markdown(text: &str) -> Vec<String> {
    let mut bad = Vec::new();
    let lines: Vec<&str> = text.lines().collect();
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i];
        if let Some(info) = line.strip_prefix("```") {
            let mut words = info.split_whitespace();
            let lang = words.next().unwrap_or("text");
            if !SD_CODE_LANGS.contains(&lang) {
                bad.push(format!("code fence language {lang:?} is not one of {}", SD_CODE_LANGS.join(", ")));
            }
            for w in words {
                match w.split_once('=') {
                    Some(("title" | "group" | "id" | "setup", v)) if !v.is_empty() => {}
                    Some(("hl", v)) if !v.is_empty() && v.split(',').all(|p| p.split('-').all(|n| n.parse::<u32>().is_ok_and(|n| n > 0))) => {}
                    _ => bad.push(format!("code fence option {w:?} is not title=…, group=…, hl=3,5-7, or for a plan id=… and setup=…")),
                }
            }
            let start = i;
            i += 1;
            while i < lines.len() && lines[i].trim_end() != "```" {
                i += 1;
            }
            if i == lines.len() {
                bad.push(format!("the code fence opened on line {} is never closed", start + 1));
            }
        } else if line.trim_end() == "$$" {
            let start = i;
            i += 1;
            while i < lines.len() && lines[i].trim_end() != "$$" {
                i += 1;
            }
            if i == lines.len() {
                bad.push(format!("the display math opened on line {} is never closed", start + 1));
            }
        } else if let Some(rest) = line.strip_prefix(":::") {
            let (kind, title) = rest.split_once(' ').unwrap_or((rest.trim(), ""));
            if !matches!(kind, "steps" | "deeper") {
                bad.push(format!("line {}: `:::{kind}` is not :::steps or :::deeper", i + 1));
            }
            if title.trim().is_empty() {
                bad.push(format!("line {}: :::{kind} needs a title", i + 1));
            }
            let start = i;
            let mut items = 0;
            i += 1;
            while i < lines.len() && lines[i].trim_end() != ":::" {
                if kind == "steps" && !lines[i].trim().is_empty() {
                    let parts: Vec<&str> = lines[i].trim_start_matches("- ").split(" | ").collect();
                    if !lines[i].starts_with("- ") || !(2..=3).contains(&parts.len()) || parts.iter().any(|p| p.trim().is_empty()) {
                        bad.push(format!("line {}: a step is `- who | what` or `- who | what | narration`", i + 1));
                    }
                    items += 1;
                }
                if lines[i].starts_with(":::") && lines[i].trim_end() != ":::" {
                    bad.push(format!("line {}: blocks cannot be nested", i + 1));
                }
                i += 1;
            }
            if i == lines.len() {
                bad.push(format!("the :::{kind} block opened on line {} is never closed", start + 1));
            } else if kind == "steps" && items < 2 {
                bad.push(format!("the :::steps block on line {} needs at least two steps", start + 1));
            }
        } else if let Some(kind) = line.strip_prefix("> [!").and_then(|r| r.split_once(']')).map(|(k, _)| k)
            && !matches!(kind, "NOTE" | "TIP" | "WARNING" | "INTERVIEW" | "ASIDE")
        {
            bad.push(format!("line {}: callout [!{kind}] is not NOTE, TIP, WARNING, INTERVIEW or ASIDE", i + 1));
        }
        i += 1;
    }
    // margin notes and inline math must close on the line they open
    for (n, line) in lines.iter().enumerate() {
        let mut rest = *line;
        while let Some(at) = rest.find("^[") {
            let after = &rest[at + 2..];
            let mut depth = 1;
            let close = after.char_indices().find(|&(_, c)| {
                if c == '[' {
                    depth += 1;
                } else if c == ']' {
                    depth -= 1;
                }
                depth == 0
            });
            match close {
                Some((end, _)) if after[..end].trim().is_empty() => bad.push(format!("line {}: an empty margin note", n + 1)),
                Some((end, _)) => rest = &after[end + 1..],
                None => {
                    bad.push(format!("line {}: a margin note `^[` is never closed", n + 1));
                    break;
                }
            }
        }
        if line.matches("\\(").count() != line.matches("\\)").count() {
            bad.push(format!("line {}: inline math `\\(` and `\\)` do not pair up", n + 1));
        }
    }
    bad
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
    let pages = read_dir_toml::<Concept>(&dir.join("concepts"), issues).into_iter().map(|p| (p, SD_GROUPS, false));
    let sql = read_dir_toml::<Concept>(&root.join("sql").join("pages"), issues).into_iter().map(|p| (p, SQL_GROUPS, true));
    for ((id, path, mut c), groups, is_sql) in pages.chain(sql).collect::<Vec<_>>() {
        let mut bad = Vec::new();
        if !groups.contains(&c.group.as_str()) {
            bad.push(format!("group {:?} is not one of {}", c.group, groups.join(", ")));
        }
        if is_sql && !id.starts_with("sql-") {
            bad.push("a SQL course page's file name starts with `sql-`, so its id never collides with a system design page".to_owned());
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
        for src in &c.sources {
            if blank(&src.title) || !src.url.starts_with("https://") {
                bad.push(format!("source {:?} needs a title and an https:// url", src.title));
            }
            if src.kind.as_deref().is_some_and(|k| !SD_SOURCE_KINDS.contains(&k)) {
                bad.push(format!("source {:?} has kind {:?}, not one of {}", src.title, src.kind, SD_SOURCE_KINDS.join(", ")));
            }
        }
        if c.summary.iter().any(|s| blank(s)) {
            bad.push("summary lines must not be empty".to_owned());
        }
        for t in &c.terms {
            if blank(&t.term) || blank(&t.means) {
                bad.push(format!("term {:?} needs a term and a meaning", t.term));
            }
            if !c.sections.iter().any(|s| s.heading == t.section) {
                bad.push(format!("term {} belongs to {:?}, which is not a section heading", t.term, t.section));
            }
        }
        for s in &c.sections {
            for m in check_markdown(&s.body) {
                bad.push(format!("section {:?}: {m}", s.heading));
            }
        }
        for (n, k) in c.checks.iter().enumerate() {
            for m in check_markdown(&k.answer) {
                bad.push(format!("check {}: {m}", n + 1));
            }
        }
        if c.credit.as_deref().is_some_and(blank) {
            bad.push("`credit` must not be empty when given".to_owned());
        }
        for f in &c.figures {
            if !sd_figures().contains(&f.name.as_str()) {
                bad.push(format!("figure {:?} is not one the web app draws ({})", f.name, sd_figures().join(", ")));
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
    if let Ok(entries) = std::fs::read_dir(root.join("sql").join("plans")) {
        let mut files: Vec<_> = entries.filter_map(|e| e.ok()).map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x == "json")).collect();
        files.sort();
        for path in files {
            let id = path.file_stem().and_then(|s| s.to_str()).unwrap_or_default().to_owned();
            match std::fs::read_to_string(&path).map_err(|e| e.to_string()).and_then(|t| serde_json::from_str::<serde_json::Value>(&t).map_err(|e| e.to_string())) {
                Ok(v) if v.get("json").is_some() && v.get("query").is_some() => {
                    cat.plans.insert(id, v);
                }
                Ok(_) => issues.push(Issue { path, message: "a captured plan needs `query` and `json` (see tools/sqlperf/capture.py)".to_owned() }),
                Err(message) => issues.push(Issue { path, message }),
            }
        }
    }
    // every plan a page shows must have been captured
    for c in cat.concepts.values().filter(|c| c.id.starts_with("sql-")) {
        for text in c.sections.iter().map(|s| &s.body).chain(c.checks.iter().map(|k| &k.answer)) {
            for line in text.lines().filter(|l| l.starts_with("```plan")) {
                let id = line.split_whitespace().find_map(|w| w.strip_prefix("id=")).unwrap_or_default();
                if id.is_empty() {
                    issues.push(Issue { path: root.join("sql").join("pages").join(format!("{}.toml", c.id)), message: "a ```plan block needs id=…".to_owned() });
                } else if !cat.plans.contains_key(id) {
                    issues.push(Issue { path: root.join("sql").join("pages").join(format!("{}.toml", c.id)), message: format!("plan {id} has not been captured: run tools/sqlperf/capture.py") });
                }
            }
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
            for m in check_markdown(&s.reference) {
                bad.push(format!("step {:?}: {m}", s.name));
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

#[cfg(test)]
mod tests {
    use super::check_markdown;

    #[test]
    fn well_formed_markdown_passes() {
        let text = "Text with a note.^[It has [a link](https://x.y) inside.] And \\(h\\) math.\n\n\
                    ```rust title=a.rs hl=2,4-5 group=g\nfn main() {}\n```\n\n$$\nx = 1\n$$\n\n\
                    :::steps A race\n- writer A | DB: 10 | first\n- writer B | DB: 12\n:::\n\n\
                    :::deeper More | 2 min\nAnything.\n:::\n\n> [!TIP]\n> Fine.";
        assert_eq!(check_markdown(text), Vec::<String>::new());
    }

    #[test]
    fn each_malformed_block_is_reported() {
        let cases = [
            ("```rust\nfn main() {}", "never closed"),
            ("```cobol\nx\n```", "language"),
            ("```rust hl=0\nx\n```", "option"),
            ("$$\nx = 1", "display math"),
            (":::steps Race\n- only one | step\n:::", "at least two"),
            (":::steps Race\n- no separator\n- a | b\n:::", "a step is"),
            (":::deeper More\ntext", "never closed"),
            (":::widget Thing\n:::", "not :::steps"),
            ("> [!DANGER]\n> no", "callout"),
            ("A note^[that never ends", "margin note"),
            ("Math \\(h without end", "inline math"),
        ];
        for (text, needle) in cases {
            let bad = check_markdown(text);
            assert!(bad.iter().any(|m| m.contains(needle)), "{text:?} should report {needle:?}, got {bad:?}");
        }
    }
}
