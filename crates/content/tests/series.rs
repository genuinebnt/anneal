//! The series loader: what it accepts, and each mistake it reports.

use std::fs;
use std::path::{Path, PathBuf};

use anneal_content::Catalog;

fn repo_content() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content")
}

fn write(root: &Path, path: &str, text: &str) {
    let path = root.join(path);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, text).unwrap();
}

/// A content folder with three draft-free problems `l9-a`, `l9-b`, `l9-c`, a draft `l9-d` and one cookbook page `locks`.
fn content(series: &str) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    write(root, "tracks/l9-test/track.toml", "code = \"L9\"\nname = \"Test\"\nsection = \"L\"\ntier = \"core\"\norder = 1\n[[stages]]\nslug = \"s\"\nname = \"S\"\nband = \"easy\"\n");
    for (n, slug) in ["a", "b", "c", "d"].iter().enumerate() {
        let at = format!("tracks/l9-test/problems/{slug}");
        let status = if *slug == "d" { "draft" } else { "ready" };
        write(root, &format!("{at}/problem.toml"), &format!("slug = \"{slug}\"\ntitle = \"Problem {slug}\"\nmode = \"write\"\nlevel = \"easy\"\nstage = \"s\"\norder = {}\nstatus = \"{status}\"\nfollow_up = \"More?\"\n[[hints]]\nkind = \"idea\"\ntext = \"t\"\n[solution]\nexplanation = \"e\"\ntime = \"O(1)\"\nspace = \"O(1)\"\n", n + 1));
        for file in ["statement.md", "starter.rs", "solution.rs", "tests/visible.rs", "tests/hidden.rs"] {
            write(root, &format!("{at}/{file}"), "// x\n");
        }
    }
    write(root, "cookbook/rust/locks.toml", "title = \"Locks\"\nmodule = \"M\"\nchapter = \"C\"\norder = 1\nkind = \"guide\"\nlede = \"About locks.\"\n[[sections]]\nheading = \"One\"\nbody = \"Text.\"\n");
    write(root, "series/path.toml", series);
    dir
}

const GOOD: &str = r#"
[series]
id = "anything"
title = "A path"
summary = "Two parts and a step."

[[stage]]
id = "first"
name = "First"

[[part]]
number = 1
id = "l9-a"
stage = "first"
status = "existing"
cookbook = [{ level = "related", page = "locks", hidden = true }]

[[part]]
id = "l9-b"
stage = "first"
required = ["l9-a"]
helpful = [{ id = "l9-c", why = "w", reveals_method = true, hidden = true }]

[[part]]
stage = "first"
title = "Not there yet"
status = "to-write"

[[capstone]]
step = 1
id = "l9-c"
prepared_by = ["l9-b"]

[[revisit]]
part = "l9-a"
before = "l9-c"
how = "Again."

[[readiness]]
group = "G"
concept = "A thing"
parts = ["l9-a"]
steps = [1]

[[connection]]
idea = "I"
learn = ["l9-a"]
apply = ["l9-c"]

[[side_path]]
id = "l9-d"
why = "later"
"#;

/// The series issues of a file, one line each.
fn issues(series: &str) -> Vec<String> {
    let dir = content(series);
    let loaded = Catalog::load(dir.path()).unwrap();
    let found: Vec<String> = loaded.issues.iter().filter(|i| i.path.ends_with("series/path.toml")).map(|i| i.message.clone()).collect();
    assert_eq!(found.is_empty(), !loaded.catalog.series.is_empty(), "a file with a mistake is left out, a good one kept: {found:?}");
    found
}

fn expect(series: &str, words: &str) {
    let found = issues(series);
    assert!(found.iter().any(|m| m.contains(words)), "no issue containing {words:?} in {found:?}");
}

#[test]
fn a_good_file_loads() {
    let dir = content(GOOD);
    let loaded = Catalog::load(dir.path()).unwrap();
    assert_eq!(loaded.issues, []);
    let s = &loaded.catalog.series[0];
    assert_eq!(s.id, "path", "the address is the file's name");
    assert_eq!((s.parts.len(), s.steps.len()), (3, 1));
    assert_eq!(s.parts[0].title, "Problem a", "a title comes from the problem");
    assert_eq!(s.parts.iter().map(|p| p.written).collect::<Vec<_>>(), [true, true, false]);
    assert_eq!(s.parts[0].group, "First");
    assert_eq!(s.project.cookbook, None);
    assert_eq!(s.revisit_anchor(&s.revisits[0]).map(|i| i.number), Some(3), "before the first step is after the last part");
}

#[test]
fn a_missing_folder_means_no_series() {
    let dir = content(GOOD);
    fs::remove_dir_all(dir.path().join("series")).unwrap();
    let loaded = Catalog::load(dir.path()).unwrap();
    assert!(loaded.catalog.series.is_empty() && loaded.issues.is_empty());
}

#[test]
fn problem_ids_must_exist_unless_to_be_written() {
    expect(&GOOD.replace("id = \"l9-a\"\nstage", "id = \"l9-nope\"\nstage"), "there is no such problem");
    expect(&GOOD.replace("status = \"to-write\"", ""), "needs an id");
    // a part to be written may already name its id; once that problem exists the part is simply written
    let dir = content(&GOOD.replace("title = \"Not there yet\"", "id = \"l9-later\"\ntitle = \"Not there yet\""));
    assert_eq!(Catalog::load(dir.path()).unwrap().issues, []);
}

#[test]
fn a_draft_problem_is_still_to_be_written() {
    let dir = content(&GOOD.replace("title = \"Not there yet\"\nstatus = \"to-write\"", "id = \"l9-d\""));
    let loaded = Catalog::load(dir.path()).unwrap();
    assert_eq!(loaded.issues, []);
    assert!(!loaded.catalog.series[0].parts[2].written);
}

#[test]
fn required_links_come_earlier() {
    expect(&GOOD.replace("required = [\"l9-a\"]", "required = [\"l9-c\"]"), "must come earlier");
    expect(&GOOD.replace("required = [\"l9-a\"]", "required = [\"l9-zzz\"]"), "is not in the series");
}

#[test]
fn required_links_have_no_cycle() {
    let looped = GOOD.replace("status = \"existing\"", "required = [\"l9-b\"]");
    expect(&looped, "required links form a cycle: l9-a -> l9-b -> l9-a");
    expect(&GOOD.replace("required = [\"l9-a\"]", "required = [\"l9-b\"]"), "form a cycle: l9-b -> l9-b");
}

#[test]
fn other_references_are_checked() {
    expect(&GOOD.replace("page = \"locks\"", "page = \"nothing\""), "no cookbook page");
    expect(&GOOD.replace("id = \"l9-c\", why", "id = \"l9-q\", why"), "neither in the series nor a problem");
    expect(&GOOD.replace("prepared_by = [\"l9-b\"]", "prepared_by = [\"l9-c\"]"), "is not a part");
    expect(&GOOD.replace("steps = [1]", "steps = [2]"), "no capstone step 2");
    expect(&GOOD.replace("learn = [\"l9-a\"]", "learn = [\"l9-q\"]"), "[[connection]]");
    expect(&GOOD.replace("id = \"l9-d\"\nwhy", "id = \"l9-q\"\nwhy"), "[[side_path]]");
    expect(&GOOD.replace("before = \"l9-c\"", "after = \"l9-a\""), "after a later part");
    expect(&GOOD.replace("before = \"l9-c\"", "before = \"l9-c\"\nafter = \"l9-b\""), "one of `after` and `before`");
    expect(&GOOD.replace("stage = \"first\"\nrequired", "stage = \"second\"\nrequired"), "no [[stage]]");
    expect(&GOOD.replace("number = 1", "number = 2"), "comes in place 1");
    expect(&GOOD.replace("id = \"l9-b\"\nstage", "id = \"l9-a\"\nstage"), "listed twice");
}

#[test]
fn a_field_nobody_reads_is_an_error() {
    expect(&GOOD.replace("summary = \"Two", "sumary = \"Two"), "unknown field");
}

#[test]
fn the_repository_series_are_sound() {
    let loaded = Catalog::load(&repo_content()).unwrap();
    let found: Vec<_> = loaded.issues.iter().filter(|i| i.path.to_string_lossy().contains("/series/")).collect();
    assert!(found.is_empty(), "{found:?}");
    let s = loaded.catalog.series.iter().find(|s| s.id == "web-server").expect("content/series/web-server.toml");
    assert!(!s.parts.is_empty() && !s.steps.is_empty());
}
