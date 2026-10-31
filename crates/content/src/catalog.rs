use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use crate::dsa::{DsaCatalog, DsaProblem};
use crate::model::{Language, Mode, ProblemFile, Section, StageDef, Status, Tier, TrackFile};

/// Everything under `content/tracks`, loaded and validated.
#[derive(Debug, Default)]
pub struct Catalog {
    /// Sorted by section, then by the section's recommended order.
    pub tracks: Vec<Track>,
    /// The DSA section's techniques and company groups (its tracks are in `tracks`).
    pub dsa: DsaCatalog,
    /// The DSA patterns again, each holding only the extra LeetCode practice problems (`content/dsa/practice.json`).
    /// Kept apart from `tracks` so they never count toward the lists, readiness or reviews, but [`Catalog::problem`]
    /// finds them, so they can be logged.
    pub practice_tracks: Vec<Track>,
    /// Ids of problems that were removed on purpose (`content/retired.txt`). Their progress is deleted at startup
    /// instead of blocking a deploy.
    pub retired: HashSet<String>,
    /// System design prep: concept pages and practice problems (`content/sd`, docs/SYSTEM_DESIGN.md).
    pub sd: crate::sd::SdCatalog,
}

#[derive(Debug)]
pub struct Track {
    /// Folder name, e.g. `d9-graphs`.
    pub slug: String,
    pub code: String,
    pub name: String,
    pub section: Section,
    pub tier: Tier,
    pub order: u32,
    pub summary: String,
    /// For a practice track: the NeetCode pattern it practises.
    pub pattern: Option<String>,
    pub stages: Vec<StageDef>,
    /// Sorted by `order`.
    pub problems: Vec<Problem>,
}

#[derive(Debug)]
pub struct Problem {
    /// Globally unique: `<track code lowercased>-<slug>`, e.g. `d9-network-delay-time`.
    pub id: String,
    pub meta: ProblemFile,
    pub dir: PathBuf,
    pub files: ProblemFiles,
    /// Set for the DSA section's LeetCode problems, which anneal doesn't run.
    pub dsa: Option<DsaProblem>,
}

/// File contents; `None` when the file is absent (allowed for drafts).
#[derive(Debug, Default)]
pub struct ProblemFiles {
    pub statement: Option<String>,
    pub starter: Option<String>,
    pub solution: Option<String>,
    pub visible_tests: Option<String>,
    pub hidden_tests: Option<String>,
    /// `wrong/<name>.rs`: plausible but incorrect solutions the tests must reject, as `(name, code)`
    /// sorted by name. Only `anneal verify` reads them.
    pub wrong: Vec<(String, String)>,
}

/// A problem with the content, reported with the file it was found in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Issue {
    pub path: PathBuf,
    pub message: String,
}

impl std::fmt::Display for Issue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.path.display(), self.message)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum LoadError {
    #[error("cannot read content directory {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
}

/// The catalog plus every issue found. Parts that failed to parse are left out
/// of the catalog and reported as issues, so one bad file never hides the rest.
#[derive(Debug)]
pub struct Loaded {
    pub catalog: Catalog,
    pub issues: Vec<Issue>,
}

impl Catalog {
    /// Loads `<root>/tracks/*`. `root` is the `content/` directory.
    pub fn load(root: &Path) -> Result<Loaded, LoadError> {
        let tracks_dir = root.join("tracks");
        let mut issues = Vec::new();
        let mut tracks = Vec::new();
        for dir in sorted_subdirs(&tracks_dir)? {
            if let Some(track) = load_track(&dir, &mut issues)? {
                tracks.push(track);
            }
        }
        let dsa_loaded = crate::dsa::load(root, &mut issues);
        let (dsa, practice_tracks) = (dsa_loaded.catalog, dsa_loaded.practice_tracks);
        tracks.extend(dsa_loaded.tracks);
        tracks.sort_by_key(|t| (section_rank(t.section), t.order));
        check_catalog(&tracks, &mut issues);
        check_unlocks(&tracks, &mut issues);
        let retired = read_retired(root);
        let sd = crate::sd::load(root, &mut issues);
        for id in tracks.iter().flat_map(|t| &t.problems).map(|p| &p.id).filter(|id| retired.contains(*id)) {
            issues.push(Issue { path: root.join("retired.txt"), message: format!("{id} is retired but also a current problem") });
        }
        Ok(Loaded {
            catalog: Catalog { tracks, dsa, practice_tracks, retired, sd },
            issues,
        })
    }

    pub fn track(&self, slug_or_code: &str) -> Option<&Track> {
        self.tracks
            .iter()
            .find(|t| t.slug == slug_or_code || t.code.eq_ignore_ascii_case(slug_or_code))
    }

    pub fn problem(&self, id: &str) -> Option<(&Track, &Problem)> {
        self.tracks
            .iter()
            .chain(&self.practice_tracks)
            .find_map(|t| t.problems.iter().find(|p| p.id == id).map(|p| (t, p)))
    }
    /// Every `(old id, current id)` pair declared with `renamed_from`.
    pub fn renames(&self) -> Vec<(&str, &str)> {
        self.tracks
            .iter()
            .flat_map(|t| &t.problems)
            .flat_map(|p| p.meta.renamed_from.iter().map(move |old| (old.as_str(), p.id.as_str())))
            .collect()
    }

    /// Whether stored progress under `id` has a home: a current problem, one that was renamed from it, or a retired
    /// id (whose progress is deleted at startup).
    pub fn knows(&self, id: &str) -> bool {
        self.problem(id).is_some() || self.retired.contains(id) || self.renames().iter().any(|(old, _)| *old == id)
    }
}

impl Track {
    pub fn stage(&self, slug: &str) -> Option<&StageDef> {
        self.stages.iter().find(|s| s.slug == slug)
    }
}

/// `<root>/retired.txt`: one problem id per line; blank lines and `#` comments are ignored.
fn read_retired(root: &Path) -> HashSet<String> {
    fs::read_to_string(root.join("retired.txt"))
        .unwrap_or_default()
        .lines()
        .map(|l| l.split('#').next().unwrap_or("").trim())
        .filter(|l| !l.is_empty())
        .map(str::to_owned)
        .collect()
}

fn section_rank(s: Section) -> u8 {
    match s {
        Section::Dsa => 0,
        Section::Language => 1,
        Section::StandardLibrary => 2,
        Section::Concurrency => 3,
        Section::Systems => 4,
        Section::Performance => 5,
        Section::Backend => 6,
        Section::Design => 7,
        Section::Practice => 8,
    }
}

fn sorted_subdirs(dir: &Path) -> Result<Vec<PathBuf>, LoadError> {
    let io = |source| LoadError::Io {
        path: dir.to_path_buf(),
        source,
    };
    let mut dirs = Vec::new();
    for entry in fs::read_dir(dir).map_err(io)? {
        let entry = entry.map_err(io)?;
        if entry.file_type().map_err(io)?.is_dir() {
            dirs.push(entry.path());
        }
    }
    dirs.sort();
    Ok(dirs)
}

fn read_toml<T: serde::de::DeserializeOwned>(path: &Path, issues: &mut Vec<Issue>) -> Option<T> {
    let text = match fs::read_to_string(path) {
        Ok(t) => t,
        Err(e) => {
            issues.push(issue(path, format!("cannot read: {e}")));
            return None;
        }
    };
    match toml::from_str(&text) {
        Ok(v) => Some(v),
        Err(e) => {
            issues.push(issue(path, format!("invalid TOML: {}", e.message())));
            None
        }
    }
}

fn read_optional(path: &Path) -> Option<String> {
    fs::read_to_string(path).ok()
}

/// Every `<name>.<ext>` in `dir`, sorted by name; empty when the folder is absent.
fn read_wrong(dir: &Path, ext: &str) -> Vec<(String, String)> {
    let Ok(entries) = fs::read_dir(dir) else { return Vec::new() };
    let mut out: Vec<(String, String)> = entries
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == ext))
        .filter_map(|p| Some((p.file_stem()?.to_string_lossy().into_owned(), fs::read_to_string(&p).ok()?)))
        .collect();
    out.sort();
    out
}

fn issue(path: &Path, message: impl Into<String>) -> Issue {
    Issue {
        path: path.to_path_buf(),
        message: message.into(),
    }
}

fn load_track(dir: &Path, issues: &mut Vec<Issue>) -> Result<Option<Track>, LoadError> {
    let file = dir.join("track.toml");
    let Some(meta) = read_toml::<TrackFile>(&file, issues) else {
        return Ok(None);
    };
    let slug = dir_name(dir);
    check_track(&slug, &meta, &file, issues);

    let mut problems = Vec::new();
    let problems_dir = dir.join("problems");
    if problems_dir.is_dir() {
        for pdir in sorted_subdirs(&problems_dir)? {
            if let Some(p) = load_problem(&pdir, &meta, issues) {
                problems.push(p);
            }
        }
    }
    problems.sort_by_key(|p| p.meta.order);
    check_problem_order(&problems, &meta, &file, issues);

    Ok(Some(Track {
        slug,
        code: meta.code,
        name: meta.name,
        section: meta.section,
        tier: meta.tier,
        order: meta.order,
        summary: meta.summary,
        pattern: meta.pattern,
        stages: meta.stages,
        problems,
    }))
}

fn dir_name(dir: &Path) -> String {
    dir.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}

fn check_track(slug: &str, meta: &TrackFile, file: &Path, issues: &mut Vec<Issue>) {
    let code = meta.code.as_bytes();
    let well_formed = code.len() >= 2
        && code[0] == meta.section.letter() as u8
        && code[1..].iter().all(u8::is_ascii_digit);
    if !well_formed {
        issues.push(issue(
            file,
            format!(
                "code {:?} must be the section letter {} followed by digits",
                meta.code,
                meta.section.letter()
            ),
        ));
    }
    let prefix = format!("{}-", meta.code.to_ascii_lowercase());
    if !slug.starts_with(&prefix) {
        issues.push(issue(
            file,
            format!("folder {slug:?} must start with {prefix:?}"),
        ));
    }
    if meta.stages.is_empty() {
        issues.push(issue(file, "a track needs at least one stage"));
    }
    let mut seen = HashSet::new();
    for s in &meta.stages {
        if !seen.insert(s.slug.as_str()) {
            issues.push(issue(
                file,
                format!("stage slug {:?} is used twice", s.slug),
            ));
        }
    }
    // Tracks climb from easy to hard: a stage may never be easier than the one before it.
    for pair in meta.stages.windows(2) {
        if pair[1].band < pair[0].band {
            issues.push(issue(
                file,
                format!("stage {:?} ({:?}) comes after {:?} ({:?}); stages must run easy → medium → hard", pair[1].slug, pair[1].band, pair[0].slug, pair[0].band),
            ));
        }
    }
}

fn load_problem(dir: &Path, track: &TrackFile, issues: &mut Vec<Issue>) -> Option<Problem> {
    let file = dir.join("problem.toml");
    let meta = read_toml::<ProblemFile>(&file, issues)?;
    let ext = meta.language.ext();
    let files = ProblemFiles {
        statement: read_optional(&dir.join("statement.md")),
        starter: read_optional(&dir.join(format!("starter.{ext}"))),
        solution: read_optional(&dir.join(format!("solution.{ext}"))),
        visible_tests: read_optional(&dir.join(format!("tests/visible.{ext}"))),
        hidden_tests: read_optional(&dir.join(format!("tests/hidden.{ext}"))),
        wrong: read_wrong(&dir.join("wrong"), ext),
    };
    check_problem(dir, &meta, &files, track, &file, issues);
    Some(Problem {
        id: format!("{}-{}", track.code.to_ascii_lowercase(), meta.slug),
        meta,
        dir: dir.to_path_buf(),
        files,
        dsa: None,
    })
}

fn check_problem(
    dir: &Path,
    meta: &ProblemFile,
    files: &ProblemFiles,
    track: &TrackFile,
    file: &Path,
    issues: &mut Vec<Issue>,
) {
    if dir_name(dir) != meta.slug {
        issues.push(issue(
            file,
            format!(
                "slug {:?} must match the folder name {:?}",
                meta.slug,
                dir_name(dir)
            ),
        ));
    }
    if !track.stages.iter().any(|s| s.slug == meta.stage) {
        issues.push(issue(
            file,
            format!("stage {:?} is not defined in track.toml", meta.stage),
        ));
    }
    for (i, c) in meta.companies.iter().enumerate() {
        if !crate::model::COMPANIES.iter().any(|&(name, _)| name == c) {
            issues.push(issue(file, format!("unknown company {c:?}; add it to COMPANIES in model.rs or use an existing name")));
        } else if meta.companies[..i].contains(c) {
            issues.push(issue(file, format!("company {c:?} is listed twice")));
        }
    }
    if meta.rules.is_some() && meta.mode != Mode::Fix {
        issues.push(issue(file, "rules only apply to fix-this problems"));
    }
    if meta.language == Language::Python && (meta.mode != Mode::Write || meta.rules.is_some() || !meta.crates.is_empty() || meta.perf.is_some()) {
        issues.push(issue(file, "Python problems are write-it problems: no rules, crates or [perf]"));
    }
    if track.section == Section::Practice {
        if meta.unlocked_by.is_empty() {
            issues.push(issue(file, "a practice problem needs unlocked_by: the DSA problem ids that open it"));
        }
        if track.pattern.is_none() {
            issues.push(issue(dir, "a practice track needs `pattern` in track.toml"));
        }
    } else if !meta.unlocked_by.is_empty() || meta.warmup {
        issues.push(issue(file, "unlocked_by and warmup only apply to practice problems"));
    }
    check_perf(meta, files, file, issues);
    if meta.status == Status::Draft {
        return;
    }
    let ext = meta.language.ext();
    let required = [
        ("statement.md".to_owned(), &files.statement),
        (format!("starter.{ext}"), &files.starter),
        (format!("solution.{ext}"), &files.solution),
        (format!("tests/visible.{ext}"), &files.visible_tests),
        (format!("tests/hidden.{ext}"), &files.hidden_tests),
    ];
    for (name, content) in required {
        if content.as_deref().is_none_or(|c| c.trim().is_empty()) {
            issues.push(issue(file, format!("ready problems need {name}")));
        }
    }
    if !(1..=3).contains(&meta.hints.len()) {
        issues.push(issue(
            file,
            format!("ready problems need 1–3 hints, found {}", meta.hints.len()),
        ));
    }
    if meta.solution.is_none() {
        issues.push(issue(
            file,
            "ready problems need [solution] notes with explanation and complexity",
        ));
    }
    if meta.follow_up.is_none() {
        issues.push(issue(
            file,
            "ready problems need an interview follow_up question",
        ));
    }
}

/// The prelude's measuring helpers only exist, or only mean something, when `[perf]` turns them on.
fn check_perf(meta: &ProblemFile, files: &ProblemFiles, file: &Path, issues: &mut Vec<Issue>) {
    let perf = meta.perf.unwrap_or_default();
    let tests = [&files.visible_tests, &files.hidden_tests].into_iter().flatten().map(String::as_str).collect::<Vec<_>>().join("\n");
    let needs = [
        ("anneal_prelude::asm", perf.asm, "asm = true"),
        ("anneal_prelude::allocs", perf.count_allocs, "count_allocs = true"),
        ("anneal_prelude::assert_faster", perf.release, "release = true"),
    ];
    for (helper, on, flag) in needs {
        if tests.contains(helper) && !on {
            issues.push(issue(file, format!("tests use {helper}, which needs [perf] {flag}")));
        }
    }
    if perf.asm && !perf.release {
        issues.push(issue(file, "[perf] asm = true inspects release code; set release = true too"));
    }
}

fn check_problem_order(
    problems: &[Problem],
    track: &TrackFile,
    file: &Path,
    issues: &mut Vec<Issue>,
) {
    let mut orders: HashMap<u32, &str> = HashMap::new();
    for p in problems {
        if let Some(other) = orders.insert(p.meta.order, &p.meta.slug) {
            issues.push(issue(
                file,
                format!(
                    "order {} is used by both {:?} and {:?}",
                    p.meta.order, other, p.meta.slug
                ),
            ));
        }
    }
    // Problems sorted by `order` must walk the stages in track order.
    let stage_index = |slug: &str| track.stages.iter().position(|s| s.slug == slug);
    let mut last: Option<(usize, &str)> = None;
    for p in problems {
        let Some(idx) = stage_index(&p.meta.stage) else {
            continue;
        };
        if let Some((prev, prev_slug)) = last
            && idx < prev
        {
            issues.push(issue(
                file,
                format!(
                    "problem {:?} (stage {:?}) is ordered after {:?}, which is in a later stage",
                    p.meta.slug, p.meta.stage, prev_slug
                ),
            ));
        }
        last = Some((idx, &p.meta.slug));
    }
    // Inside a stage, problems run easy → hard too.
    for pair in problems.windows(2) {
        let (a, b) = (&pair[0].meta, &pair[1].meta);
        if a.stage == b.stage && b.level < a.level {
            issues.push(issue(
                file,
                format!("problem {:?} ({:?}) comes after {:?} ({:?}) in stage {:?}; problems in a stage must run easy → hard", b.slug, b.level, a.slug, a.level, a.stage),
            ));
        }
    }
}

/// Every id a practice problem is unlocked by must be a DSA problem, and the track's pattern must exist.
fn check_unlocks(tracks: &[Track], issues: &mut Vec<Issue>) {
    let dsa: HashSet<&str> = tracks.iter().filter(|t| t.section == Section::Dsa).flat_map(|t| &t.problems).map(|p| p.id.as_str()).collect();
    let patterns: HashSet<&str> = tracks.iter().filter(|t| t.section == Section::Dsa).map(|t| t.name.as_str()).collect();
    for t in tracks.iter().filter(|t| t.section == Section::Practice) {
        if let Some(pattern) = &t.pattern
            && !dsa.is_empty()
            && !patterns.contains(pattern.as_str())
        {
            issues.push(Issue { path: PathBuf::from(&t.slug), message: format!("pattern {pattern:?} isn't a DSA pattern") });
        }
        for p in &t.problems {
            for id in &p.meta.unlocked_by {
                if !dsa.contains(id.as_str()) && !dsa.is_empty() {
                    issues.push(issue(&p.dir, format!("unlocked_by {id}: not a DSA problem")));
                }
            }
        }
    }
}

fn check_catalog(tracks: &[Track], issues: &mut Vec<Issue>) {
    let mut codes = HashSet::new();
    let mut ids = HashSet::new();
    for t in tracks {
        if !codes.insert(t.code.as_str()) {
            issues.push(Issue {
                path: PathBuf::from(&t.slug),
                message: format!("track code {} is used twice", t.code),
            });
        }
        for p in &t.problems {
            if !ids.insert(p.id.as_str()) {
                issues.push(issue(&p.dir, format!("problem id {} is used twice", p.id)));
            }
        }
    }
    // An old id must point at exactly one problem, and never at an id that exists again.
    let mut olds = HashSet::new();
    for p in tracks.iter().flat_map(|t| &t.problems) {
        for old in &p.meta.renamed_from {
            if ids.contains(old.as_str()) {
                issues.push(issue(&p.dir, format!("renamed_from {old}: that id is a current problem")));
            }
            if !olds.insert(old.as_str()) {
                issues.push(issue(&p.dir, format!("renamed_from {old}: claimed by more than one problem")));
            }
        }
    }
}
