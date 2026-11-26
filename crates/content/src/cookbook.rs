//! The cookbook: reference pages to look things up in, loaded from `<root>/cookbook`. A missing directory is an empty cookbook.
//! Nothing about the reader is stored.
//!
//! - `<root>/cookbook/<subject>/<id>.toml` for each subject in [`SUBJECTS`]: pages made from the owner's guides by
//!   `tools/cookbook/convert.py`.
//! - `<root>/cookbook/samples/<id>/page.toml`: draft pages in the full page format, each with the project it quotes in
//!   `code/` and the output recorded from running it in `recorded.json`.
//!
//! Page bodies are the Markdown the web app reads (`web/src/components/SdMd.tsx`).

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::catalog::Issue;
use crate::sd::{check_markdown_with, read_dir_toml};

/// The subjects with their own tab, in tab order: id, title, one line about it.
pub const SUBJECTS: &[(&str, &str, &str)] = &[
    ("rust", "Rust", "The language, the standard library, crates and tooling."),
    ("algorithms", "Algorithms", "Data structures and algorithms the other subjects build on."),
    ("systems", "Systems", "Threads, async, performance and the operating system."),
    ("data", "Data", "Databases from the SQL text to bytes on disk, search and vectors."),
    ("distributed", "Distributed & web", "Replication and consensus, services, protocols and security."),
    ("compilers", "Compilers", "From source text to running code."),
    ("testing", "Testing", "How to know code is correct, and how fast it is."),
    ("projects", "Projects", "Complete programs where several ideas work together."),
];
/// The group the draft sample pages are listed under; it has no tab.
pub const SAMPLES: &str = "samples";
/// The callouts a cookbook page can use.
pub const CALLOUTS: &[&str] = &["NOTE", "TIP", "WARNING", "ASIDE", "RUST", "TESTING", "PRODUCTION"];
/// The largest lab file sent with a page.
const MAX_LAB_FILE: u64 = 256 * 1024;

/// The anchor the web app gives a section heading (`anchorOf` in `web/src/pages/cookbook/shared.tsx`).
pub fn anchor_of(heading: &str) -> String {
    let lower = heading.to_lowercase();
    let words: Vec<&str> = lower.split(|c: char| !c.is_ascii_alphanumeric()).filter(|w| !w.is_empty()).collect();
    format!("cb-{}", words.join("-"))
}

#[derive(Debug, Clone, Default)]
pub struct Cookbook {
    /// By id.
    pub pages: BTreeMap<String, Page>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Page {
    #[serde(default)]
    pub id: String,
    /// A subject of [`SUBJECTS`], or [`SAMPLES`]; set by the loader from where the file is.
    #[serde(default)]
    pub subject: String,
    pub title: String,
    pub module: String,
    pub chapter: String,
    /// Where the module comes among its subject's modules.
    #[serde(default)]
    pub module_order: u32,
    #[serde(default)]
    pub chapter_order: u32,
    pub order: u32,
    /// `guide`, or for a page in the full format `api`, `pattern`, `comparison`, `concept`, `testing` or `build`.
    pub kind: String,
    pub lede: String,
    /// `draft` until the owner has reviewed the page.
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub level: Option<String>,
    /// What the page was made from.
    #[serde(default)]
    pub origin: Option<String>,
    #[serde(default)]
    pub summary: Vec<String>,
    #[serde(default)]
    pub layers: Vec<String>,
    #[serde(default)]
    pub layers_on: Vec<String>,
    #[serde(default)]
    pub glance: Vec<Glance>,
    pub sections: Vec<Section>,
    #[serde(default)]
    pub terms: Vec<Term>,
    #[serde(default)]
    pub sources: Vec<Source>,
    #[serde(default)]
    pub methods: Vec<Method>,
    #[serde(default)]
    pub labs: Vec<Lab>,
    /// Source files the page is about, as paths inside the repository's `cookbook/` folder; the loader makes them a lab.
    #[serde(default, skip_serializing)]
    pub code: Vec<String>,
    /// Names of code blocks on the page that are shown but not compiled.
    #[serde(default)]
    pub illustrative: Vec<String>,
    /// What was run to check the page, as the author recorded it.
    #[serde(default)]
    pub verified: BTreeMap<String, toml::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Glance {
    pub label: String,
    pub value: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Section {
    pub heading: String,
    pub body: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Term {
    pub term: String,
    pub means: String,
    pub section: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Source {
    pub title: String,
    pub url: String,
    #[serde(default)]
    pub note: Option<String>,
    #[serde(default)]
    pub kind: Option<String>,
    #[serde(default)]
    pub scope: Option<String>,
    /// Whether the page's author read it.
    #[serde(default)]
    pub read: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Method {
    #[serde(rename = "type")]
    pub type_name: String,
    pub name: String,
    pub signature: String,
    pub since: String,
    pub use_when: String,
    #[serde(default)]
    pub common: bool,
    #[serde(default)]
    pub unstable: bool,
    #[serde(default)]
    pub anchor: Option<String>,
}

/// A project that goes with the page, shown read-only.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Lab {
    pub title: String,
    #[serde(default)]
    pub after: Option<String>,
    /// Whether the project has a program to run, and tests.
    #[serde(default)]
    pub run: bool,
    #[serde(default)]
    pub test: bool,
    /// The files to show first, as paths inside `code/`; the rest of the project is listed after them.
    pub files: Vec<String>,
    /// Every file of the project, filled by the loader.
    #[serde(default, skip_deserializing)]
    pub contents: Vec<LabFile>,
    /// The output recorded when the page was written, filled by the loader.
    #[serde(default, skip_deserializing)]
    pub recorded: Option<Recorded>,
}

#[derive(Debug, Clone, Serialize)]
pub struct LabFile {
    pub path: String,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Recorded {
    pub run: String,
    pub tests: String,
    pub passed: u32,
    /// When it was recorded.
    pub stamp: String,
    #[serde(default)]
    pub problems: String,
}

/// Every file under `dir`, by path relative to it, in path order. Build output is not part of a lab.
fn lab_files(dir: &Path, rel: &str, out: &mut Vec<LabFile>) -> Result<(), String> {
    let mut entries: Vec<_> = std::fs::read_dir(dir.join(rel)).map_err(|e| format!("lab folder: {e}"))?.filter_map(Result::ok).collect();
    entries.sort_by_key(std::fs::DirEntry::file_name);
    for e in entries {
        let name = e.file_name().to_string_lossy().into_owned();
        let path = if rel.is_empty() { name.clone() } else { format!("{rel}/{name}") };
        let kind = e.file_type().map_err(|e| e.to_string())?;
        if kind.is_dir() {
            if name != "target" {
                lab_files(dir, &path, out)?;
            }
        } else if kind.is_file() && name != "Cargo.lock" {
            if e.metadata().map_err(|e| e.to_string())?.len() > MAX_LAB_FILE {
                return Err(format!("lab file {path} is larger than {MAX_LAB_FILE} bytes"));
            }
            let text = std::fs::read_to_string(e.path()).map_err(|err| format!("lab file {path}: {err}"))?;
            out.push(LabFile { path, text });
        }
    }
    Ok(())
}

fn problems_with(p: &Page) -> Vec<String> {
    let mut bad = Vec::new();
    let blank = |s: &str| s.trim().is_empty();
    if [&p.title, &p.module, &p.chapter, &p.kind, &p.lede].into_iter().any(|s| blank(s)) {
        bad.push("`title`, `module`, `chapter`, `kind` and `lede` must not be empty".to_owned());
    }
    if p.sections.is_empty() {
        bad.push("a page needs at least one section".to_owned());
    }
    let mut headings = BTreeSet::new();
    for s in &p.sections {
        if blank(&s.heading) || blank(&s.body) {
            bad.push("a section needs a heading and a body".to_owned());
        }
        if !headings.insert(s.heading.as_str()) {
            bad.push(format!("two sections are headed {:?}", s.heading));
        }
        bad.extend(check_markdown_with(&s.body, CALLOUTS).into_iter().map(|m| format!("section {:?}: {m}", s.heading)));
    }
    for s in &p.sources {
        if blank(&s.title) || !(s.url.starts_with("https://") || s.url.starts_with("http://")) {
            bad.push(format!("a source needs a title and a web address: {:?}", s.url));
        }
    }
    bad
}

/// Loads `<root>/cookbook`, reporting every problem with a page as an issue and leaving that page out.
pub(crate) fn load(root: &Path, issues: &mut Vec<Issue>) -> Cookbook {
    let dir = root.join("cookbook");
    let mut found: Vec<(String, std::path::PathBuf, Page)> = Vec::new();
    for (subject, _, _) in SUBJECTS {
        for (id, path, mut page) in read_dir_toml::<Page>(&dir.join(subject), issues) {
            page.subject = (*subject).to_owned();
            found.push((id, path, page));
        }
    }
    if let Ok(entries) = std::fs::read_dir(dir.join(SAMPLES)) {
        let mut folders: Vec<_> = entries.filter_map(Result::ok).map(|e| e.path()).filter(|p| p.is_dir()).collect();
        folders.sort();
        for folder in folders {
            let path = folder.join("page.toml");
            let id = folder.file_name().and_then(|s| s.to_str()).unwrap_or_default().to_owned();
            match load_sample(&folder) {
                Ok(page) => found.push((id, path, page)),
                Err(message) => issues.push(Issue { path, message }),
            }
        }
    }
    // the code a page is about lives beside the content folder, in the repository's own cookbook crate
    let sources = root.parent().unwrap_or(root).join("cookbook");
    let mut cookbook = Cookbook::default();
    for (id, path, mut page) in found {
        if let Err(message) = attach_code(&sources, &mut page) {
            issues.push(Issue { path, message });
            continue;
        }
        let mut bad = problems_with(&page);
        if cookbook.pages.contains_key(&id) {
            bad.push(format!("another page is already called {id}"));
        }
        if bad.is_empty() {
            page.id.clone_from(&id);
            cookbook.pages.insert(id, page);
        } else {
            issues.extend(bad.into_iter().map(|message| Issue { path: path.clone(), message }));
        }
    }
    cookbook
}

/// Reads the files a page names in `code` into one lab.
fn attach_code(sources: &Path, page: &mut Page) -> Result<(), String> {
    if page.code.is_empty() {
        return Ok(());
    }
    let mut contents = Vec::new();
    for name in &page.code {
        let relative = Path::new(name);
        if relative.components().any(|c| !matches!(c, std::path::Component::Normal(_))) {
            return Err(format!("code path {name:?} must stay inside cookbook/"));
        }
        let text = std::fs::read_to_string(sources.join(relative)).map_err(|e| format!("code file {name}: {e}"))?;
        if text.len() as u64 > MAX_LAB_FILE {
            return Err(format!("code file {name} is larger than {MAX_LAB_FILE} bytes"));
        }
        contents.push(LabFile { path: name.clone(), text });
    }
    // a project's files are shown by their path inside it
    let project = contents.first().and_then(|f| f.path.strip_prefix("crates/")).and_then(|r| r.split_once('/')).map(|(n, _)| format!("crates/{n}/"));
    if let Some(prefix) = project
        && contents.iter().all(|f| f.path.starts_with(&prefix))
    {
        for f in &mut contents {
            f.path = f.path[prefix.len()..].to_owned();
        }
    }
    let files = contents.iter().map(|f| f.path.clone()).collect();
    page.labs.push(Lab { title: "Code".to_owned(), after: None, run: false, test: false, files, contents, recorded: None });
    Ok(())
}

fn load_sample(folder: &Path) -> Result<Page, String> {
    let text = std::fs::read_to_string(folder.join("page.toml")).map_err(|e| e.to_string())?;
    let mut page: Page = toml::from_str(&text).map_err(|e| e.message().to_owned())?;
    page.subject = SAMPLES.to_owned();
    if !page.labs.is_empty() {
        let mut files = Vec::new();
        lab_files(&folder.join("code"), "", &mut files)?;
        let recorded: Vec<Recorded> = match std::fs::read_to_string(folder.join("recorded.json")) {
            Ok(t) => serde_json::from_str(&t).map_err(|e| format!("recorded.json: {e}"))?,
            Err(_) => Vec::new(),
        };
        for (i, lab) in page.labs.iter_mut().enumerate() {
            if let Some(missing) = lab.files.iter().find(|f| !files.iter().any(|x| &x.path == *f)) {
                return Err(format!("lab {:?} lists {missing}, which is not in code/", lab.title));
            }
            let mut mine = files.clone();
            // the files the page points to come first, in its order
            mine.sort_by_key(|f| lab.files.iter().position(|x| x == &f.path).unwrap_or(usize::MAX));
            lab.contents = mine;
            lab.recorded = recorded.get(i).cloned();
        }
    }
    Ok(page)
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    /// A scratch content folder with one file, removed when dropped.
    struct Dir(std::path::PathBuf);
    impl Dir {
        fn with(files: &[(&str, &str)]) -> Dir {
            static N: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
            let n = N.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            let root = std::env::temp_dir().join(format!("anneal-cookbook-{}-{n}", std::process::id()));
            for (path, text) in files {
                let p = root.join(path);
                fs::create_dir_all(p.parent().unwrap()).unwrap();
                fs::write(p, text).unwrap();
            }
            Dir(root)
        }
    }
    impl Drop for Dir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    const PAGE: &str = "title = \"T\"\nmodule = \"M\"\nchapter = \"C\"\norder = 1\nkind = \"guide\"\nlede = \"L\"\n[[sections]]\nheading = \"H\"\nbody = \"> [!RUST]\\n> yes\"\n";

    fn load_dir(d: &Dir) -> (Cookbook, Vec<String>) {
        let mut issues = Vec::new();
        let c = load(&d.0, &mut issues);
        (c, issues.into_iter().map(|i| i.message).collect())
    }

    #[test]
    fn a_page_takes_its_subject_and_id_from_where_it_is() {
        let (c, issues) = load_dir(&Dir::with(&[("cookbook/data/joins.toml", PAGE)]));
        assert_eq!(issues, Vec::<String>::new());
        assert_eq!(c.pages["joins"].subject, "data");
    }

    #[test]
    fn a_bad_page_is_reported_and_left_out() {
        let unclosed = PAGE.replace("> [!RUST]\\n> yes", "```rust\\nfn main() {}");
        let (c, issues) = load_dir(&Dir::with(&[("cookbook/data/a.toml", &unclosed)]));
        assert!(c.pages.is_empty());
        assert!(issues.iter().any(|m| m.contains("never closed")), "{issues:?}");
    }

    #[test]
    fn a_page_reads_the_code_it_names_and_may_not_leave_the_cookbook() {
        let page = format!("code = [\"examples/a/x.rs\"]\n{PAGE}");
        let d = Dir::with(&[("content/cookbook/data/x.toml", &page), ("cookbook/examples/a/x.rs", "fn main() {}")]);
        let mut issues = Vec::new();
        let c = load(&d.0.join("content"), &mut issues);
        assert!(issues.is_empty(), "{:?}", issues.iter().map(|i| &i.message).collect::<Vec<_>>());
        assert_eq!(c.pages["x"].labs[0].contents[0].text, "fn main() {}");

        let outside = page.replace("examples/a/x.rs", "../secret.txt");
        let d = Dir::with(&[("content/cookbook/data/x.toml", &outside), ("secret.txt", "no")]);
        let mut issues = Vec::new();
        assert!(load(&d.0.join("content"), &mut issues).pages.is_empty());
        assert!(issues.iter().any(|i| i.message.contains("stay inside")));
    }

    #[test]
    fn an_anchor_matches_the_web_apps_rule() {
        assert_eq!(anchor_of("Anomalies and isolation levels"), "cb-anomalies-and-isolation-levels");
        assert_eq!(anchor_of("`select!`: what is dropped?"), "cb-select-what-is-dropped");
    }

    #[test]
    fn two_pages_may_not_share_an_id() {
        let (c, issues) = load_dir(&Dir::with(&[("cookbook/data/x.toml", PAGE), ("cookbook/compilers/x.toml", PAGE)]));
        assert_eq!(c.pages.len(), 1);
        assert!(issues.iter().any(|m| m.contains("already called x")), "{issues:?}");
    }

    #[test]
    fn a_sample_sends_its_project_without_build_output_and_checks_the_files_it_lists() {
        let page = format!("{PAGE}[[labs]]\ntitle = \"Lab\"\nrun = true\ntest = true\nfiles = [\"src/lib.rs\"]\n");
        let (c, issues) = load_dir(&Dir::with(&[
            ("cookbook/samples/s/page.toml", &page),
            ("cookbook/samples/s/code/Cargo.toml", "[package]"),
            ("cookbook/samples/s/code/src/lib.rs", "pub fn f() {}"),
            ("cookbook/samples/s/code/target/debug/junk", "x"),
        ]));
        assert_eq!(issues, Vec::<String>::new());
        let paths: Vec<&str> = c.pages["s"].labs[0].contents.iter().map(|f| f.path.as_str()).collect();
        assert_eq!(paths, ["src/lib.rs", "Cargo.toml"]);
        let (c, issues) = load_dir(&Dir::with(&[("cookbook/samples/s/page.toml", &page), ("cookbook/samples/s/code/Cargo.toml", "[package]")]));
        assert!(c.pages.is_empty());
        assert!(issues.iter().any(|m| m.contains("src/lib.rs")), "{issues:?}");
    }
}
