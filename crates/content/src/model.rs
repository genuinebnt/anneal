//! The on-disk content format: one folder per track, one folder per problem.
//!
//! ```text
//! content/tracks/d9-graphs/
//!   track.toml
//!   problems/network-delay-time/
//!     problem.toml   metadata, hints, rules
//!     statement.md
//!     starter.rs     becomes src/lib.rs in the user's workspace
//!     solution.rs
//!     tests/visible.rs
//!     tests/hidden.rs
//! ```

use serde::{Deserialize, Serialize};

/// Difficulty band. Stages in a track must run Easy → Medium → Hard.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Band {
    Easy,
    Medium,
    Hard,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Section {
    #[serde(rename = "D")]
    Dsa,
    #[serde(rename = "L")]
    Language,
    #[serde(rename = "S")]
    StandardLibrary,
    #[serde(rename = "C")]
    Concurrency,
    #[serde(rename = "Y")]
    Systems,
    #[serde(rename = "F")]
    Performance,
    #[serde(rename = "B")]
    Backend,
    #[serde(rename = "M")]
    Design,
    /// Handwritten practice tracks, one per DSA pattern. Not part of spaced repetition or readiness.
    #[serde(rename = "P")]
    Practice,
}

impl Section {
    pub fn letter(self) -> char {
        match self {
            Section::Dsa => 'D',
            Section::Language => 'L',
            Section::StandardLibrary => 'S',
            Section::Concurrency => 'C',
            Section::Systems => 'Y',
            Section::Performance => 'F',
            Section::Backend => 'B',
            Section::Design => 'M',
            Section::Practice => 'P',
        }
    }
}

/// The language a problem is solved in.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Language {
    #[default]
    Rust,
    Python,
}

impl Language {
    /// The file extension of source files, and so of `starter`, `solution` and the tests.
    pub fn ext(self) -> &'static str {
        match self {
            Language::Rust => "rs",
            Language::Python => "py",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Tier {
    Core,
    Light,
    Sde3,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    /// Signature + tests; the user writes the implementation.
    Write,
    /// A broken program to fix under rules.
    Fix,
    /// One stage of a multi-stage project; only tests are given.
    Stage,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    Draft,
    Ready,
}

/// `track.toml`
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TrackFile {
    pub code: String,
    pub name: String,
    pub section: Section,
    pub tier: Tier,
    /// Position in the section's recommended order.
    pub order: u32,
    #[serde(default)]
    pub summary: String,
    /// For a practice track: the NeetCode pattern it practises, e.g. `Graphs`.
    #[serde(default)]
    pub pattern: Option<String>,
    pub stages: Vec<StageDef>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StageDef {
    pub slug: String,
    pub name: String,
    pub band: Band,
}

/// The company names a problem's `companies` may use, with the group each belongs to, so a filter never splits
/// "Meta" from "Facebook" and can offer a whole group at once. FAANG first, then big tech, databases, Rust shops.
pub const COMPANIES: &[(&str, &str)] = &[
    ("Meta", "FAANG"),
    ("Apple", "FAANG"),
    ("Amazon", "FAANG"),
    ("Netflix", "FAANG"),
    ("Google", "FAANG"),
    ("Microsoft", "Big tech"),
    ("Adobe", "Big tech"),
    ("Airbnb", "Big tech"),
    ("Atlassian", "Big tech"),
    ("Bloomberg", "Big tech"),
    ("ByteDance", "Big tech"),
    ("DoorDash", "Big tech"),
    ("Goldman Sachs", "Big tech"),
    ("LinkedIn", "Big tech"),
    ("Nvidia", "Big tech"),
    ("Salesforce", "Big tech"),
    ("Stripe", "Big tech"),
    ("Uber", "Big tech"),
    ("Walmart", "Big tech"),
    ("Oracle", "Databases"),
    ("Databricks", "Databases"),
    ("Snowflake", "Databases"),
    ("MongoDB", "Databases"),
    ("Cockroach Labs", "Databases"),
    ("ClickHouse", "Databases"),
    ("Confluent", "Databases"),
    ("Elastic", "Databases"),
    ("Redis", "Databases"),
    ("SingleStore", "Databases"),
    ("PingCAP", "Databases"),
    ("Cloudflare", "Rust shops"),
    ("Discord", "Rust shops"),
    ("Dropbox", "Rust shops"),
    ("Figma", "Rust shops"),
    ("1Password", "Rust shops"),
    ("Datadog", "Rust shops"),
    ("Fastly", "Rust shops"),
    ("Coinbase", "Rust shops"),
    ("Vercel", "Rust shops"),
    ("Mozilla", "Rust shops"),
];

/// `problem.toml`
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProblemFile {
    pub slug: String,
    pub title: String,
    pub mode: Mode,
    /// Rust unless the problem says `language = "python"`.
    #[serde(default)]
    pub language: Language,
    /// Practice problems only: the DSA problem ids (`lc-<slug>`) that unlock it. Logging any one of them, with any
    /// grade, opens the problem.
    #[serde(default)]
    pub unlocked_by: Vec<String>,
    /// Practice problems only: it works as a prerequisite, so it also opens while one of its `unlocked_by` problems
    /// is coming up next (in the next few of the plan's order), not only after one has been logged.
    #[serde(default)]
    pub warmup: bool,
    pub level: Band,
    pub stage: String,
    /// Position within the track; unique per track.
    pub order: u32,
    pub status: Status,
    #[serde(default)]
    pub tags: Vec<String>,
    /// Companies known to ask this problem, from [`COMPANIES`], e.g. `["Amazon", "Google"]`.
    #[serde(default)]
    pub companies: Vec<String>,
    #[serde(default)]
    pub teaches: Vec<String>,
    #[serde(default)]
    pub constraints: Vec<String>,
    #[serde(default)]
    pub examples: Vec<Example>,
    #[serde(default)]
    pub hints: Vec<Hint>,
    #[serde(default)]
    pub follow_up: Option<String>,
    /// Track codes this problem links to, e.g. `["S5", "S1"]`.
    #[serde(default)]
    pub related: Vec<String>,
    #[serde(default)]
    pub solution: Option<SolutionNotes>,
    #[serde(default)]
    pub rules: Option<Rules>,
    /// Where the problem came from in the old study packet, e.g. `"W42"`.
    #[serde(default)]
    pub source: Option<String>,
    /// Earlier ids of this problem (`<track code>-<slug>`, e.g. `"l5-generic-stack"`). Progress stored under an old id
    /// is moved to this problem when the API starts, so renaming or moving a problem never loses anyone's history.
    #[serde(default)]
    pub renamed_from: Vec<String>,
    /// Crates from the sandbox's crate set (`docker/deps/Cargo.toml`), e.g. `["tokio", "serde"]`.
    #[serde(default)]
    pub crates: Vec<String>,
    /// How a performance problem (section F) is built and measured. Absent for everything else.
    #[serde(default)]
    pub perf: Option<Perf>,
    /// Cookbook pages to open when stuck. The workspace shows them closed, one level at a time.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub cookbook: Vec<CookbookRef>,
}

/// How much a cookbook page gives away, least first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CookbookLevel {
    /// Explains the idea.
    Related,
    /// The same technique on a different problem.
    Similar,
    /// Solves this problem, or the part its note names.
    Exact,
}

/// One `[[cookbook]]` table in problem.toml.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CookbookRef {
    pub level: CookbookLevel,
    /// A cookbook page id (`/cookbook/p/<page>`).
    pub page: String,
    /// The anchor of one section of that page: `cb-<heading slug>`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub section: Option<String>,
    /// What the page gives, and what it leaves out.
    pub note: String,
}

/// `[perf]` in problem.toml: what a performance problem's tests need from the runner.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Perf {
    /// Build and run the tests with `--release`, one test at a time, so timings mean something.
    #[serde(default)]
    pub release: bool,
    /// Emit the solution's release assembly for `anneal_prelude::asm` checks.
    #[serde(default)]
    pub asm: bool,
    /// Install the counting global allocator behind `anneal_prelude::allocs`. Leave it off when the
    /// solution defines its own `#[global_allocator]`.
    #[serde(default)]
    pub count_allocs: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Example {
    pub input: String,
    pub output: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Hint {
    pub kind: String,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SolutionNotes {
    pub explanation: String,
    pub time: String,
    pub space: String,
}

/// Constraints a fix-this attempt must respect, checked before the code runs.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Rules {
    /// Method names that may not be called, e.g. `["clone"]`.
    #[serde(default)]
    pub forbid_methods: Vec<String>,
    /// Type names that may not appear, e.g. `["RefCell", "Cell"]`.
    #[serde(default)]
    pub forbid_types: Vec<String>,
    #[serde(default)]
    pub forbid_unsafe: bool,
    /// Most lines that may differ from the starter.
    #[serde(default)]
    pub max_changed_lines: Option<u32>,
}
