//! Hand-written cookbook lessons (`content/cookbook/lessons/<subject>/<id>.toml`). A lesson replaces the generated page of
//! the same id: it keeps that page's address, subject, module, chapter and order, and brings its own text.
//!
//! This module holds the lesson file format, the three fenced blocks a section may use (`stepper`, `walkthrough` and
//! `compare`, each holding TOML), the check of a lesson's internal links and the readability check. None of it runs on
//! generated pages.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::catalog::{Issue, Track};
use crate::cookbook::{Cookbook, Page, SUBJECTS, Section, Source, anchor_of};
use crate::series::Series;

/// The folder inside `content/cookbook` that holds the lessons.
pub const LESSONS: &str = "lessons";
const CONNECTION_KINDS: &[&str] = &["before", "practise", "returns", "project"];
const LEVELS: &[&str] = &["related", "similar", "exact"];
const BLOCKS: &[&str] = &["stepper", "walkthrough", "compare"];

/// A lesson as written. The place fields are copied from the generated page by the writer; the loader takes the place
/// from the generated page itself, so a later change of the outline cannot move a lesson by accident.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct LessonFile {
    pub kind: String,
    pub title: String,
    pub lede: String,
    pub summary: String,
    #[allow(dead_code)]
    pub module: String,
    #[allow(dead_code)]
    pub chapter: String,
    #[serde(default, rename = "module_order")]
    pub _module_order: Option<u32>,
    #[serde(default, rename = "chapter_order")]
    pub _chapter_order: Option<u32>,
    #[allow(dead_code)]
    pub order: u32,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub level: Option<String>,
    #[serde(default)]
    pub code: Vec<String>,
    /// Exercises (`/p/<id>`) or series (`/rust/series/<id>`) that the lesson's full code gives away much of.
    #[serde(default)]
    pub code_spoils: Vec<String>,
    pub sections: Vec<Section>,
    #[serde(default)]
    pub connections: Vec<Connection>,
    #[serde(default)]
    pub sources: Vec<Source>,
}

impl LessonFile {
    /// The lesson as a page, in the place of the generated page it replaces.
    pub(crate) fn into_page(self, generated: &Page) -> Page {
        Page {
            id: String::new(),
            subject: generated.subject.clone(),
            title: self.title,
            module: generated.module.clone(),
            chapter: generated.chapter.clone(),
            module_order: generated.module_order,
            chapter_order: generated.chapter_order,
            order: generated.order,
            kind: self.kind,
            lede: self.lede,
            status: self.status,
            level: self.level,
            origin: None,
            summary: Vec::new(),
            in_short: Some(self.summary),
            connections: self.connections,
            code_spoils: self.code_spoils.into_iter().map(|link| Spoil { link, title: String::new() }).collect(),
            layers: Vec::new(),
            layers_on: Vec::new(),
            glance: Vec::new(),
            sections: self.sections,
            terms: Vec::new(),
            sources: self.sources,
            methods: Vec::new(),
            labs: Vec::new(),
            code: self.code,
            illustrative: Vec::new(),
            verified: Default::default(),
        }
    }
}

/// An exercise a lesson's full code gives away. The reading page keeps the code closed and names the exercise.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Spoil {
    pub link: String,
    /// The exercise's or series' own title, filled in once everything is loaded.
    pub title: String,
}

/// One entry of a lesson's Connections box.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Connection {
    /// `before`, `practise`, `returns` or `project`.
    pub kind: String,
    pub title: String,
    /// A cookbook page, a problem or a series; only a `project` connection may leave it out.
    #[serde(default)]
    pub link: Option<String>,
    pub why: String,
    /// For `practise` only: `related`, `similar` or `exact`. An exact one stays closed until the reader asks.
    #[serde(default)]
    pub level: Option<String>,
}

// ---- the fenced blocks ----

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Stepper {
    caption: String,
    frame: Vec<Frame>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Frame {
    title: String,
    text: String,
    lanes: Vec<Lane>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Lane {
    name: String,
    items: Vec<String>,
    /// How many places the lane has, when it is bounded.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    slots: Option<u32>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Walkthrough {
    file: String,
    step: Vec<Step>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Step {
    title: String,
    /// `10-18`, both ends included, or one line number.
    lines: String,
    text: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Compare {
    takeaway: String,
    wrong: Side,
    fixed: Side,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Side {
    title: String,
    code: String,
    result: String,
}

#[derive(Debug)]
enum Block {
    Stepper(Stepper),
    Walkthrough(Walkthrough),
    Compare(Compare),
}

/// A piece of a section body: Markdown, or one block with the line it opens on and the TOML inside it.
enum Piece<'a> {
    Text(&'a str),
    Block { kind: &'a str, info: &'a str, line: usize, toml: String, closed: bool },
}

/// Cuts a body at its block fences. Other fences stay inside the text, so a block name inside them is only text.
fn pieces(body: &str) -> Vec<Piece<'_>> {
    let lines: Vec<&str> = body.split_inclusive('\n').collect();
    let mut out = Vec::new();
    let (mut at, mut from, mut i) = (0, 0, 0);
    while i < lines.len() {
        let Some(info) = lines[i].strip_prefix("```") else {
            at += lines[i].len();
            i += 1;
            continue;
        };
        let info = info.trim();
        let kind = info.split_whitespace().next().unwrap_or_default();
        let mut end = i + 1;
        while end < lines.len() && lines[end].trim_end() != "```" {
            end += 1;
        }
        let closed = end < lines.len();
        let next = (end + 1).min(lines.len());
        let length: usize = lines[i..next].iter().map(|l| l.len()).sum();
        if BLOCKS.contains(&kind) {
            if from < at {
                out.push(Piece::Text(&body[from..at]));
            }
            out.push(Piece::Block { kind, info, line: i + 1, toml: lines[i + 1..end].concat(), closed });
            from = at + length;
        }
        at += length;
        i = next;
    }
    if from < body.len() {
        out.push(Piece::Text(&body[from..]));
    }
    out
}

fn parse(kind: &str, text: &str) -> Result<Block, String> {
    let read = |e: toml::de::Error| {
        let line = e.span().map(|s| text[..s.start.min(text.len())].matches('\n').count() + 1);
        match line {
            Some(n) => format!("{} (line {n} of the block)", e.message()),
            None => e.message().to_owned(),
        }
    };
    match kind {
        "stepper" => toml::from_str(text).map(Block::Stepper).map_err(read),
        "walkthrough" => toml::from_str(text).map(Block::Walkthrough).map_err(read),
        _ => toml::from_str(text).map(Block::Compare).map_err(read),
    }
}

/// `10-18` or `12` as (first, last).
fn line_range(spec: &str) -> Option<(usize, usize)> {
    let (a, b) = spec.split_once('-').unwrap_or((spec, spec));
    Some((a.trim().parse().ok()?, b.trim().parse().ok()?))
}

fn blank(s: &str) -> bool {
    s.trim().is_empty()
}

fn check_block(block: &Block, page: &Page) -> Vec<String> {
    let mut bad = Vec::new();
    match block {
        Block::Stepper(s) => {
            if blank(&s.caption) {
                bad.push("a stepper needs a caption".to_owned());
            }
            if s.frame.len() < 2 {
                bad.push("a stepper needs at least two frames".to_owned());
            }
            for (n, f) in s.frame.iter().enumerate() {
                if blank(&f.title) || blank(&f.text) || f.lanes.is_empty() {
                    bad.push(format!("frame {} needs a title, a text and at least one lane", n + 1));
                }
                for l in &f.lanes {
                    if blank(&l.name) {
                        bad.push(format!("frame {}: a lane needs a name", n + 1));
                    }
                    if l.slots.is_some_and(|max| l.items.len() > max as usize || max == 0 || max > 64) {
                        bad.push(format!("frame {}: lane {:?} needs 1 to 64 slots and no more items than slots", n + 1, l.name));
                    }
                }
            }
        }
        Block::Walkthrough(w) => {
            // `attach_code` keeps the order of `code`, so the file's text is at the same position in the lab
            let file = page.code.iter().position(|c| c == &w.file).and_then(|i| page.labs.first()?.contents.get(i));
            if file.is_none() {
                bad.push(format!("walkthrough file {:?} is not one of the page's `code` paths", w.file));
            }
            if w.step.is_empty() {
                bad.push("a walkthrough needs at least one step".to_owned());
            }
            for s in &w.step {
                if blank(&s.title) || blank(&s.text) {
                    bad.push("a walkthrough step needs a title and a text".to_owned());
                }
                match line_range(&s.lines) {
                    None => bad.push(format!("step {:?}: lines {:?} is not `10-18` or `12`", s.title, s.lines)),
                    Some((a, b)) if a == 0 || b < a => bad.push(format!("step {:?}: lines {:?} must start at 1 or later and not run backwards", s.title, s.lines)),
                    Some((_, b)) => {
                        if let Some(f) = file {
                            let count = f.text.lines().count();
                            if b > count {
                                bad.push(format!("step {:?}: lines {:?} are out of bounds; {} has {count} lines", s.title, s.lines, w.file));
                            }
                        }
                    }
                }
            }
        }
        Block::Compare(c) => {
            let sides = [&c.wrong, &c.fixed];
            if blank(&c.takeaway) || sides.iter().any(|s| blank(&s.title) || blank(&s.code) || blank(&s.result)) {
                bad.push("a compare needs a takeaway, and a title, code and result on both sides".to_owned());
            }
        }
    }
    bad
}

/// A section body with its blocks taken out: what the Markdown checker looks at.
pub(crate) fn without_blocks(body: &str) -> String {
    pieces(body).iter().map(|p| if let Piece::Text(t) = p { t } else { "\n" }).collect()
}

/// The body the web app is sent: every block's TOML checked and rewritten as one line of JSON in the same fence (the web
/// app has no TOML reader). A walkthrough's file is renamed to the name the code viewer shows it under.
fn compiled(body: &str, page: &Page, bad: &mut Vec<String>) -> String {
    let mut out = String::new();
    for piece in pieces(body) {
        match piece {
            Piece::Text(t) => out.push_str(t),
            Piece::Block { kind, info, line, toml, closed } => {
                if !closed {
                    bad.push(format!("the {kind} block opened on line {line} is never closed"));
                }
                if info != kind {
                    bad.push(format!("line {line}: a {kind} fence takes no options"));
                }
                match parse(kind, &toml) {
                    Err(e) => bad.push(format!("the {kind} block on line {line} is not valid: {e}")),
                    Ok(mut block) => {
                        bad.extend(check_block(&block, page).into_iter().map(|m| format!("the {kind} block on line {line}: {m}")));
                        if let Block::Walkthrough(w) = &mut block
                            && let Some(i) = page.code.iter().position(|c| c == &w.file)
                            && let Some(shown) = page.labs.first().and_then(|l| l.contents.get(i))
                        {
                            w.file.clone_from(&shown.path);
                        }
                        let json = match &block {
                            Block::Stepper(b) => serde_json::to_string(b),
                            Block::Walkthrough(b) => serde_json::to_string(b),
                            Block::Compare(b) => serde_json::to_string(b),
                        };
                        match json {
                            Ok(json) => out.push_str(&format!("```{kind}\n{json}\n```\n")),
                            Err(e) => bad.push(format!("the {kind} block on line {line}: {e}")),
                        }
                    }
                }
            }
        }
    }
    out
}

/// The anchors the reading page makes itself; a section heading may not take one.
fn reserved(anchor: &str) -> bool {
    matches!(anchor, "cb-sources" | "cb-connections" | "cb-methods" | "cb-in-short") || anchor.starts_with("cb-lab-")
}

/// Checks what only a lesson has and compiles its blocks. Call it after the page's code is attached.
pub(crate) fn finish(page: &mut Page) -> Vec<String> {
    let mut bad = Vec::new();
    if page.kind != "lesson" {
        bad.push(format!("a lesson's `kind` is \"lesson\", not {:?}", page.kind));
    }
    if page.in_short.as_deref().is_none_or(blank) {
        bad.push("a lesson needs a `summary`".to_owned());
    }
    let mut anchors = BTreeSet::new();
    for s in &page.sections {
        let anchor = anchor_of(&s.heading);
        if reserved(&anchor) {
            bad.push(format!("section {:?} takes an address the reading page uses itself", s.heading));
        }
        if !anchors.insert(anchor) {
            bad.push(format!("section {:?} has the same address as an earlier section", s.heading));
        }
    }
    for c in &page.connections {
        let name = format!("connection {:?}", c.title);
        if blank(&c.title) || blank(&c.why) {
            bad.push(format!("{name} needs a title and a why"));
        }
        if !CONNECTION_KINDS.contains(&c.kind.as_str()) {
            bad.push(format!("{name}: kind {:?} is not one of {}", c.kind, CONNECTION_KINDS.join(", ")));
        }
        if let Some(level) = &c.level
            && (c.kind != "practise" || !LEVELS.contains(&level.as_str()))
        {
            bad.push(format!("{name}: only a practise connection has a level, one of {}", LEVELS.join(", ")));
        }
        match &c.link {
            None if c.kind != "project" => bad.push(format!("{name} needs a link")),
            Some(link) if !["/cookbook/p/", "/p/", "/rust/series/"].iter().any(|p| link.starts_with(p)) => {
                bad.push(format!("{name}: link {link:?} must start with /cookbook/p/, /p/ or /rust/series/"));
            }
            _ => {}
        }
    }
    if !page.code_spoils.is_empty() && page.code.is_empty() {
        bad.push("`code_spoils` needs `code`: there is no full code to keep closed".to_owned());
    }
    for (i, s) in page.code_spoils.iter().enumerate() {
        if page.code_spoils[..i].iter().any(|o| o.link == s.link) {
            bad.push(format!("`code_spoils` lists {:?} twice", s.link));
        }
    }
    let bodies: Vec<String> = page.sections.iter().map(|s| compiled(&s.body, page, &mut bad)).collect();
    for (s, body) in page.sections.iter_mut().zip(bodies) {
        s.body = body;
    }
    bad
}

// ---- internal links ----

/// The addresses a piece of text links to: `](address)` in Markdown, `href="address"` in HTML and `<address>` is never
/// internal. Code in fences and backticks is not read.
fn links_in(markdown: &str) -> Vec<String> {
    let text = without_code(markdown);
    let mut out = Vec::new();
    for (open, close) in [("](", ')'), ("href=\"", '"'), ("href='", '\'')] {
        for (at, _) in text.match_indices(open) {
            let rest = &text[at + open.len()..];
            if let Some(end) = rest.find(close) {
                // `](address "title")`
                if let Some(address) = rest[..end].split_whitespace().next() {
                    out.push(address.to_owned());
                }
            }
        }
    }
    out
}

/// Markdown without fenced blocks and inline code.
fn without_code(markdown: &str) -> String {
    let mut fence: Option<&str> = None;
    let mut out = String::new();
    for line in markdown.lines() {
        let t = line.trim_start();
        if let Some(mark) = fence {
            if t.starts_with(mark) {
                fence = None;
            }
            continue;
        }
        if let Some(mark) = ["```", "~~~"].into_iter().find(|m| t.starts_with(m)) {
            fence = Some(mark);
            continue;
        }
        out.push_str(&drop_code_spans(line, " "));
        out.push('\n');
    }
    out
}

/// A line with each `` `code` `` span replaced.
fn drop_code_spans(line: &str, with: &str) -> String {
    let mut out = String::new();
    let mut rest = line;
    while let Some(at) = rest.find('`') {
        let ticks = rest[at..].len() - rest[at..].trim_start_matches('`').len();
        let after = &rest[at + ticks..];
        let Some(end) = after.find(&"`".repeat(ticks)) else { break };
        out.push_str(&rest[..at]);
        out.push_str(with);
        rest = &after[end + ticks..];
    }
    out.push_str(rest);
    out
}

/// The texts of a block that the reader reads, each with a name for messages. Code is not among them.
fn block_texts(block: &Block) -> Vec<(String, &str)> {
    match block {
        Block::Stepper(s) => {
            let mut out = vec![("stepper caption".to_owned(), s.caption.as_str())];
            for (n, f) in s.frame.iter().enumerate() {
                out.push((format!("stepper frame {} title", n + 1), &f.title));
                out.push((format!("stepper frame {} text", n + 1), &f.text));
            }
            out
        }
        Block::Walkthrough(w) => w
            .step
            .iter()
            .enumerate()
            .flat_map(|(n, s)| [(format!("walkthrough step {} title", n + 1), s.title.as_str()), (format!("walkthrough step {} text", n + 1), s.text.as_str())])
            .collect(),
        Block::Compare(c) => vec![
            ("compare takeaway".to_owned(), c.takeaway.as_str()),
            ("compare wrong title".to_owned(), c.wrong.title.as_str()),
            ("compare wrong result".to_owned(), c.wrong.result.as_str()),
            ("compare fixed title".to_owned(), c.fixed.title.as_str()),
            ("compare fixed result".to_owned(), c.fixed.result.as_str()),
        ],
    }
}

/// Every piece of Markdown on a compiled lesson page, with a name for messages.
fn page_texts(page: &Page) -> Vec<(String, String)> {
    let mut out = Vec::new();
    if let Some(s) = &page.in_short {
        out.push(("summary".to_owned(), s.clone()));
    }
    for s in &page.sections {
        for piece in pieces(&s.body) {
            match piece {
                Piece::Text(t) => out.push((format!("section {:?}", s.heading), t.to_owned())),
                Piece::Block { kind, toml, .. } => {
                    // a compiled block holds JSON
                    let block = match kind {
                        "stepper" => serde_json::from_str(&toml).map(Block::Stepper),
                        "walkthrough" => serde_json::from_str(&toml).map(Block::Walkthrough),
                        _ => serde_json::from_str(&toml).map(Block::Compare),
                    };
                    if let Ok(block) = block {
                        out.extend(block_texts(&block).into_iter().map(|(name, t)| (format!("section {:?}, {name}", s.heading), t.to_owned())));
                    }
                }
            }
        }
    }
    out
}

/// The `{#name}` anchors of a body's headings.
fn named_anchors(body: &str) -> impl Iterator<Item = &str> {
    body.match_indices("{#").filter_map(|(at, _)| body[at + 2..].split_once('}').map(|(name, _)| name))
}

/// Why an internal address leads nowhere, if it does. Addresses elsewhere on the web are not looked at.
fn broken(link: &str, own: &Page, cookbook: &Cookbook, problems: &BTreeSet<&str>, series: &BTreeSet<&str>) -> Option<String> {
    let anchor_on = |page: &Page, anchor: &str| {
        page.sections.iter().any(|s| anchor_of(&s.heading) == anchor || named_anchors(&s.body).any(|a| a == anchor))
            || (anchor == "cb-sources" && !page.sources.is_empty())
            || (anchor == "cb-connections" && !page.connections.is_empty())
            || (anchor == "cb-methods" && !page.methods.is_empty())
            || anchor.strip_prefix("cb-lab-").and_then(|n| n.parse::<usize>().ok()).is_some_and(|n| n < page.labs.len())
    };
    if let Some(anchor) = link.strip_prefix('#') {
        return (!anchor_on(own, anchor)).then(|| format!("this page has no section {anchor:?}"));
    }
    if !link.starts_with('/') {
        let web = ["https://", "http://", "mailto:"].iter().any(|p| link.starts_with(p));
        return (!web).then(|| "an address starts with /, #, https:// or http://".to_owned());
    }
    let (path, anchor) = link.split_once('#').map_or((link, None), |(p, a)| (p, Some(a)));
    if let Some(id) = path.strip_prefix("/cookbook/p/") {
        let Some(page) = cookbook.pages.get(id) else { return Some(format!("there is no cookbook page {id:?}")) };
        return anchor.filter(|a| !anchor_on(page, a)).map(|a| format!("cookbook page {id:?} has no section {a:?}"));
    }
    if let Some(id) = path.strip_prefix("/p/") {
        return (!problems.contains(id)).then(|| format!("there is no problem {id:?}"));
    }
    if let Some(id) = path.strip_prefix("/rust/series/") {
        return (!series.contains(id)).then(|| format!("there is no series {id:?}"));
    }
    let known = matches!(path, "/cookbook" | "/rust" | "/rust/series") || path.strip_prefix("/cookbook/").is_some_and(|s| SUBJECTS.iter().any(|x| x.0 == s));
    (!known).then(|| "it is not a cookbook page, a problem or a series".to_owned())
}

/// Every internal link of every lesson leads somewhere: in its text, its blocks and its connections.
pub(crate) fn check_links(root: &Path, cookbook: &Cookbook, tracks: &[Track], practice: &[Track], series: &[Series], issues: &mut Vec<Issue>) {
    let problems: BTreeSet<&str> = tracks.iter().chain(practice).flat_map(|t| &t.problems).map(|p| p.id.as_str()).collect();
    let series: BTreeSet<&str> = series.iter().map(|s| s.id.as_str()).collect();
    for page in cookbook.pages.values().filter(|p| p.in_short.is_some()) {
        let path = root.join("cookbook").join(LESSONS).join(&page.subject).join(format!("{}.toml", page.id));
        let mut found: Vec<(String, String)> = page_texts(page).into_iter().flat_map(|(name, text)| links_in(&text).into_iter().map(move |l| (name.clone(), l))).collect();
        found.extend(page.connections.iter().filter_map(|c| c.link.clone().map(|l| (format!("connection {:?}", c.title), l))));
        for (name, link) in found {
            if let Some(why) = broken(&link, page, cookbook, &problems, &series) {
                issues.push(Issue { path: path.clone(), message: format!("{name}: broken link {link:?}: {why}") });
            }
        }
    }
}

/// Gives every `code_spoils` entry the title found under its address in `titles` (problems and series), and reports
/// one that is not there. Such an entry keeps its address as its title, so the code stays closed.
pub(crate) fn resolve_spoils(root: &Path, cookbook: &mut Cookbook, titles: &BTreeMap<String, String>, issues: &mut Vec<Issue>) {
    for page in cookbook.pages.values_mut() {
        for spoil in &mut page.code_spoils {
            match titles.get(&spoil.link) {
                Some(title) => spoil.title.clone_from(title),
                None => {
                    spoil.title.clone_from(&spoil.link);
                    let path = root.join("cookbook").join(LESSONS).join(&page.subject).join(format!("{}.toml", page.id));
                    issues.push(Issue { path, message: format!("`code_spoils`: {:?} is not a problem (/p/<id>) or a series (/rust/series/<id>)", spoil.link) });
                }
            }
        }
    }
}

/// Whether `text` (a body or the summary of a lesson) links to the address `path`, with or without a section.
pub fn markdown_links_to(text: &str, path: &str) -> bool {
    text.match_indices(path).any(|(at, _)| {
        let before = &text[..at];
        let after = &text[at + path.len()..];
        // JSON in a compiled block escapes the quotes of an HTML link, so only Markdown links are looked for there
        before.ends_with("](") && after.starts_with([')', '#', ' '])
    })
}

// ---- readability ----

/// Writers aim for 70 words a paragraph and 30 a sentence; above these the text is refused.
pub const MOST_PARAGRAPH_WORDS: usize = 90;
pub const MOST_SENTENCE_WORDS: usize = 35;

/// Words are runs of text between spaces that hold a letter or a digit.
fn word_count(text: &str) -> usize {
    text.split_whitespace().filter(|w| w.chars().any(char::is_alphanumeric)).count()
}

/// Stands where inline code or inline math was: it keeps the sentence in one piece and counts as no word.
const SPAN: &str = "\u{2026}\u{1}";

/// Prose without link addresses, HTML tags, inline code and Markdown marks.
fn plain_inline(line: &str) -> String {
    let mut text = drop_code_spans(line, SPAN);
    while let Some(a) = text.find("\\(") {
        let Some(b) = text[a..].find("\\)") else { break };
        text.replace_range(a..a + b + 2, SPAN);
    }
    // `[words](address)` keeps the words
    while let Some(a) = text.find("](") {
        let mut depth = 0;
        let end = text[a + 1..].char_indices().find(|&(_, c)| {
            depth += i32::from(c == '(') - i32::from(c == ')');
            depth == 0
        });
        let Some((b, _)) = end else { break };
        text.replace_range(a..a + 1 + b + 1, "");
    }
    // tags and `<address>` go; a lone `<` that is a comparison stays
    let mut out = String::new();
    let mut rest = text.as_str();
    while let Some(a) = rest.find('<') {
        let tag = rest[a + 1..].starts_with(|c: char| c.is_ascii_alphabetic() || c == '/' || c == '!');
        match rest[a..].find('>').filter(|_| tag) {
            Some(b) => {
                out.push_str(&rest[..a]);
                out.push(' ');
                rest = &rest[a + b + 1..];
            }
            None => {
                out.push_str(&rest[..=a]);
                rest = &rest[a + 1..];
            }
        }
    }
    out.push_str(rest);
    out.split_whitespace()
        .filter(|w| !(w.starts_with("https://") || w.starts_with("http://")))
        .map(|w| w.replace(['*', '[', ']'], "").replace("\u{2026}\u{1}", "\u{1}"))
        .collect::<Vec<_>>()
        .join(" ")
}

/// The paragraphs a reader sees: a list item, a table cell, a heading and a margin note are each their own.
fn paragraphs(markdown: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut open = String::new();
    let mut fence: Option<&str> = None;
    let (mut math, mut pre) = (false, false);
    fn close(open: &mut String, out: &mut Vec<String>) {
        let mut text = std::mem::take(open);
        // margin notes are read beside the paragraph, not in it
        while let Some(a) = text.find("^[") {
            let mut depth = 0;
            let end = text[a + 1..].char_indices().find(|&(_, c)| {
                depth += i32::from(c == '[') - i32::from(c == ']');
                depth == 0
            });
            let Some((b, _)) = end else { break };
            let note = plain_inline(&text[a + 2..a + 1 + b]);
            out.push(note);
            text.replace_range(a..a + 1 + b + 1, "");
        }
        let plain = plain_inline(&text);
        if word_count(&plain) > 0 {
            out.push(plain);
        }
    }
    for raw in markdown.lines() {
        let mut line = raw.trim();
        // quotes and callouts read as the text inside them
        while let Some(rest) = line.strip_prefix('>') {
            line = rest.trim_start();
        }
        if let Some(mark) = fence {
            if line.starts_with(mark) {
                fence = None;
            }
            continue;
        }
        if let Some(mark) = ["```", "~~~"].into_iter().find(|m| line.starts_with(m)) {
            close(&mut open, &mut out);
            fence = Some(mark);
            continue;
        }
        if line == "$$" {
            close(&mut open, &mut out);
            math = !math;
            continue;
        }
        if pre || line.starts_with("<pre") {
            close(&mut open, &mut out);
            pre = !line.contains("</pre>");
            continue;
        }
        if math || (line.starts_with("$$") && line.ends_with("$$")) {
            continue;
        }
        if line.starts_with("[!")
            && let Some((_, rest)) = line.split_once(']')
        {
            close(&mut open, &mut out);
            line = rest.trim_start();
        }
        if line.is_empty() || line == ":::" {
            close(&mut open, &mut out);
            continue;
        }
        if let Some(rest) = line.strip_prefix(":::") {
            close(&mut open, &mut out);
            open.push_str(rest.split_once(' ').map_or("", |(_, title)| title));
            close(&mut open, &mut out);
            continue;
        }
        if line.starts_with('|') {
            close(&mut open, &mut out);
            // a `|` in code or an escaped one does not end a cell
            for cell in drop_code_spans(line, SPAN).replace("\\|", " ").split('|') {
                open.push_str(cell);
                close(&mut open, &mut out);
            }
            continue;
        }
        let heading = line.starts_with('#');
        let bullet = ["- ", "* ", "+ "].iter().find_map(|m| line.strip_prefix(m)).or_else(|| {
            let digits = line.len() - line.trim_start_matches(|c: char| c.is_ascii_digit()).len();
            line[digits..].strip_prefix(". ").or_else(|| line[digits..].strip_prefix(") ")).filter(|_| digits > 0)
        });
        let html_block = ["<p", "<li", "</p", "</li", "<div", "</div", "<ul", "</ul", "<ol", "</ol", "<tr", "<td", "<th", "<table", "</table", "<h", "<aside", "</aside"].iter().any(|t| line.starts_with(t));
        if heading || bullet.is_some() || html_block {
            close(&mut open, &mut out);
        }
        let line = bullet.unwrap_or_else(|| line.trim_start_matches('#').trim_start());
        if !open.is_empty() {
            open.push(' ');
        }
        open.push_str(line);
        if heading {
            close(&mut open, &mut out);
        }
    }
    close(&mut open, &mut out);
    out
}

/// A paragraph's sentences: each ends at `.`, `!` or `?` followed by a space.
fn sentences(paragraph: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut now: Vec<&str> = Vec::new();
    for word in paragraph.split_whitespace() {
        now.push(word);
        let end = word.trim_end_matches([')', '"', '\'', '”', '’', '_']);
        let short_for = ["e.g.", "i.e.", "vs.", "cf."].iter().any(|a| end.trim_start_matches('(').eq_ignore_ascii_case(a));
        if end.ends_with(['.', '!', '?']) && !short_for {
            out.push(now.join(" "));
            now.clear();
        }
    }
    if !now.is_empty() {
        out.push(now.join(" "));
    }
    out
}

/// The first words of a text, to find it by.
fn start_of(text: &str) -> String {
    let words: Vec<&str> = text.split_whitespace().collect();
    let head = words[..words.len().min(8)].join(" ").replace('\u{1}', "…");
    if words.len() > 8 { format!("{head}…") } else { head }
}

fn readability_of(markdown: &str) -> Vec<String> {
    let mut bad = Vec::new();
    for p in paragraphs(markdown) {
        let words = word_count(&p);
        if words > MOST_PARAGRAPH_WORDS {
            bad.push(format!("a paragraph of {words} words (at most {MOST_PARAGRAPH_WORDS}), starting {:?}", start_of(&p)));
        }
        for s in sentences(&p) {
            let words = word_count(&s);
            if words > MOST_SENTENCE_WORDS {
                bad.push(format!("a sentence of {words} words (at most {MOST_SENTENCE_WORDS}), starting {:?}", start_of(&s)));
            }
        }
    }
    bad
}

/// Long paragraphs and sentences in the lessons under `<root>/cookbook/lessons`. `anneal validate` reports them; they do
/// not stop the server from loading the content. Files that cannot be read are reported by the loader, not here.
pub fn readability(root: &Path) -> Vec<Issue> {
    let mut issues = Vec::new();
    for (subject, _, _) in SUBJECTS {
        let dir: PathBuf = root.join("cookbook").join(LESSONS).join(subject);
        for (_, path, lesson) in crate::sd::read_dir_toml::<LessonFile>(&dir, &mut Vec::new()) {
            let mut texts = vec![("summary".to_owned(), lesson.summary.clone())];
            for s in &lesson.sections {
                for piece in pieces(&s.body) {
                    match piece {
                        Piece::Text(t) => texts.push((format!("section {:?}", s.heading), t.to_owned())),
                        Piece::Block { kind, toml, .. } => {
                            if let Ok(block) = parse(kind, &toml) {
                                texts.extend(block_texts(&block).into_iter().map(|(name, t)| (format!("section {:?}, {name}", s.heading), t.to_owned())));
                            }
                        }
                    }
                }
            }
            for (name, text) in texts {
                issues.extend(readability_of(&text).into_iter().map(|m| Issue { path: path.clone(), message: format!("readability: {name}: {m}") }));
            }
        }
    }
    issues
}

#[cfg(test)]
mod tests;
