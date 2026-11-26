use std::fs;

use super::*;
use crate::cookbook::load;

const FIXTURE: &str = include_str!("../../tests/fixtures/lesson.toml");
/// The code the fixture walks through: sixty short lines.
fn source() -> String {
    (1..=60).map(|n| format!("fn line_{n}() {{}}\n")).collect()
}

fn generated(module: &str, order: u32, body: &str) -> String {
    format!("title = \"Generated\"\nmodule = \"{module}\"\nmodule_order = 3\nchapter = \"Ch\"\nchapter_order = 2\norder = {order}\nkind = \"recipe\"\nlede = \"L\"\ncode = [\"examples/a/x.rs\"]\n[[sections]]\nheading = \"H\"\nbody = \"{body}\"\n")
}

/// A scratch repository: a generated page `x`, a page `y` it can link to, the code, and `lesson` in place of `x`.
fn repo(lesson: &str) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    for (path, text) in [
        ("content/cookbook/systems/x.toml", generated("Module", 7, "old text")),
        ("content/cookbook/systems/y.toml", generated("Module", 8, "other")),
        ("cookbook/examples/a/x.rs", source()),
        ("content/cookbook/lessons/systems/x.toml", lesson.to_owned()),
    ] {
        let path = dir.path().join(path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }
    dir
}

fn load_repo(dir: &tempfile::TempDir) -> (Cookbook, Vec<String>) {
    let root = dir.path().join("content");
    let mut issues = Vec::new();
    let cookbook = load(&root, &mut issues);
    check_links(&root, &cookbook, &[], &[], &[], &mut issues);
    (cookbook, issues.into_iter().map(|i| i.message).collect())
}

/// The fixture without its one link to a problem, which these repositories do not have.
fn good() -> String {
    FIXTURE.replace("/p/nothing", "/cookbook/p/y")
}

/// Loads the fixture with one change and expects an issue containing `wanted`, with the generated page left in place.
fn refused(from: &str, to: &str, wanted: &str) {
    let lesson = good();
    assert!(lesson.contains(from), "the fixture has no {from:?}");
    let (c, issues) = load_repo(&repo(&lesson.replacen(from, to, 1)));
    assert!(issues.iter().any(|m| m.contains(wanted)), "wanted {wanted:?}, got {issues:?}");
    assert_eq!(c.pages["x"].kind, "recipe", "a bad lesson leaves the generated page");
}

#[test]
fn a_lesson_replaces_its_page_and_keeps_address_and_place() {
    let (c, issues) = load_repo(&repo(&good()));
    assert_eq!(issues, Vec::<String>::new());
    assert_eq!(c.pages.keys().collect::<Vec<_>>(), ["x", "y"], "no new address and no second copy");
    let p = &c.pages["x"];
    assert_eq!((p.kind.as_str(), p.title.as_str(), p.subject.as_str()), ("lesson", "Fixture lesson", "systems"));
    assert_eq!((p.module.as_str(), p.module_order, p.chapter.as_str(), p.chapter_order, p.order), ("Module", 3, "Ch", 2, 7), "the place comes from the generated page");
    assert!(p.in_short.as_deref().is_some_and(|s| s.contains("fixture summary")));
    assert_eq!(p.connections.len(), 3);
    assert_eq!(p.labs[0].contents[0].text, source());
}

#[test]
fn the_web_app_gets_each_block_as_one_line_of_json() {
    let (c, _) = load_repo(&repo(&good()));
    let body = &c.pages["x"].sections[0].body;
    let lines: Vec<&str> = body.lines().collect();
    for kind in ["stepper", "walkthrough", "compare"] {
        let at = lines.iter().position(|l| *l == format!("```{kind}")).unwrap();
        let json: serde_json::Value = serde_json::from_str(lines[at + 1]).unwrap();
        assert_eq!(lines[at + 2], "```");
        match kind {
            "stepper" => assert_eq!(json["frame"][1]["lanes"][0]["slots"], 2),
            "walkthrough" => assert_eq!((json["file"].as_str(), json["step"][0]["lines"].as_str()), (Some("examples/a/x.rs"), Some("1-2"))),
            _ => assert_eq!(json["fixed"]["code"], "fn b() {}"),
        }
    }
    assert!(body.starts_with("Some words"));
}

/// The browser test (web/e2e/cookbook-lesson.spec.ts) is served this file: it must stay what the loader makes of the fixture.
#[test]
fn the_browser_test_is_served_what_the_loader_makes() {
    let (c, _) = load_repo(&repo(&good()));
    let served: serde_json::Value = serde_json::from_str(include_str!("../../../../web/e2e/fixtures/lesson-page.json")).unwrap();
    assert_eq!(serde_json::to_value(&c.pages["x"]).unwrap(), served);
}

#[test]
fn a_page_that_is_not_a_lesson_is_sent_without_the_lesson_fields() {
    let (c, _) = load_repo(&repo(&good()));
    let sent = serde_json::to_value(&c.pages["y"]).unwrap();
    assert!(sent.get("in_short").is_none() && sent.get("connections").is_none(), "{sent}");
}

#[test]
fn a_walkthrough_names_a_project_file_as_the_viewer_shows_it() {
    let dir = repo(&good().replace("examples/a/x.rs", "crates/p/src/lib.rs"));
    fs::create_dir_all(dir.path().join("cookbook/crates/p/src")).unwrap();
    fs::write(dir.path().join("cookbook/crates/p/src/lib.rs"), source()).unwrap();
    let (c, issues) = load_repo(&dir);
    assert_eq!(issues, Vec::<String>::new());
    assert!(c.pages["x"].sections[0].body.contains("\"file\":\"src/lib.rs\""));
}

#[test]
fn bad_blocks_are_refused() {
    refused("caption = \"A box fills\"", "caption = ", "stepper block on line");
    refused("caption = \"A box fills\"", "caption = \"A\"\nspeed = 2", "unknown field");
    refused("items = [\"a\"], slots = 2", "items = [\"a\", \"b\", \"c\"], slots = 2", "no more items than slots");
    refused("file = \"examples/a/x.rs\"", "file = \"examples/a/other.rs\"", "is not one of the page's `code` paths");
    refused("lines = \"1-2\"", "lines = \"59-61\"", "out of bounds; examples/a/x.rs has 60 lines");
    refused("lines = \"1-2\"", "lines = \"0-1\"", "start at 1");
    refused("lines = \"1-2\"", "lines = \"2-1\"", "run backwards");
    refused("lines = \"1-2\"", "lines = \"one\"", "is not `10-18` or `12`");
    refused("result = \"Something happens.\"", "", "compare block");
    refused("```compare", "```compare title=x", "takes no options");
    refused("kind = \"lesson\"", "kind = \"guide\"", "a lesson's `kind`");
    refused("heading = \"How it works\"", "heading = \"Connections\"", "uses itself");
    refused("kind = \"before\"", "kind = \"after\"", "is not one of before");
    refused("link = \"/cookbook/p/y#cb-h\"\n", "", "needs a link");
    refused("kind = \"before\"", "kind = \"before\"\nlevel = \"exact\"", "only a practise connection");
    refused("link = \"/cookbook/p/y#cb-h\"", "link = \"https://example.org\"", "must start with");
}

#[test]
fn spoiling_code_is_named_by_the_exercises_own_title() {
    let lesson = good().replacen("code = [\"examples/a/x.rs\"]", "code = [\"examples/a/x.rs\"]\ncode_spoils = [\"/p/queue\", \"/p/gone\"]", 1);
    let dir = repo(&lesson);
    let (mut c, issues) = load_repo(&dir);
    assert_eq!(issues, Vec::<String>::new());
    let mut found = Vec::new();
    let titles = BTreeMap::from([("/p/queue".to_owned(), "A blocking queue".to_owned())]);
    resolve_spoils(&dir.path().join("content"), &mut c, &titles, &mut found);
    let names: Vec<&str> = c.pages["x"].code_spoils.iter().map(|s| s.title.as_str()).collect();
    assert_eq!(names, ["A blocking queue", "/p/gone"], "an unknown one still keeps the code closed");
    assert!(found.len() == 1 && found[0].message.contains("/p/gone") && found[0].path.ends_with("lessons/systems/x.toml"), "{found:?}");
}

#[test]
fn spoiling_code_needs_code_and_names_each_exercise_once() {
    refused("code = [\"examples/a/x.rs\"]", "code = [\"examples/a/x.rs\"]\ncode_spoils = [\"/p/a\", \"/p/a\"]", "twice");
    let bare = "kind = \"lesson\"\ntitle = \"T\"\nlede = \"L\"\nsummary = \"S\"\nmodule = \"M\"\nchapter = \"C\"\norder = 1\ncode_spoils = [\"/p/a\"]\n[[sections]]\nheading = \"H\"\nbody = \"b\"\n";
    let (_, issues) = load_repo(&repo(bare));
    assert!(issues.iter().any(|m| m.contains("needs `code`")), "{issues:?}");
}

#[test]
fn a_block_that_is_never_closed_is_refused() {
    let open = "kind = \"lesson\"\ntitle = \"T\"\nlede = \"L\"\nsummary = \"S\"\nmodule = \"M\"\nchapter = \"C\"\norder = 1\n[[sections]]\nheading = \"H\"\nbody = \"```stepper\\ncaption = 'c'\"\n";
    let (_, issues) = load_repo(&repo(open));
    assert!(issues.iter().any(|m| m.contains("never closed")), "{issues:?}");
}

#[test]
fn a_block_name_inside_another_fence_is_only_text() {
    let body = "~~~\nx\n~~~\n```text\n```stepper\n```\nafter\n";
    assert_eq!(without_blocks(body), body);
}

#[test]
fn a_lesson_with_no_page_to_replace_is_reported() {
    let dir = repo(&good());
    fs::rename(dir.path().join("content/cookbook/lessons/systems/x.toml"), dir.path().join("content/cookbook/lessons/systems/new-page.toml")).unwrap();
    fs::create_dir_all(dir.path().join("content/cookbook/lessons/nowhere")).unwrap();
    let (c, issues) = load_repo(&dir);
    assert!(!c.pages.contains_key("new-page"));
    assert!(issues.iter().any(|m| m.contains("no generated page \"new-page\" in systems")), "{issues:?}");
    assert!(issues.iter().any(|m| m.contains("lessons/<subject>/<page id>.toml")), "{issues:?}");
}

#[test]
fn broken_internal_links_are_found_in_text_blocks_and_connections() {
    let broken = |from: &str, to: &str, wanted: &str| {
        let (c, issues) = load_repo(&repo(&good().replacen(from, to, 1)));
        assert!(issues.iter().any(|m| m.contains("broken link") && m.contains(wanted)), "wanted {wanted:?}, got {issues:?}");
        assert_eq!(c.pages["x"].kind, "lesson");
    };
    broken("(#cb-how-it-works)", "(#cb-nothing)", "this page has no section");
    broken("Some words", "[a](/cookbook/p/missing) Some words", "no cookbook page \"missing\"");
    broken("text = \"Nothing yet.\"", "text = \"[a](/cookbook/p/y#cb-no)\"", "has no section");
    broken("takeaway = \"It costs a line.\"", "takeaway = \"[a](/rust/series/none)\"", "no series");
    broken("Some words", "<a href=\\\"/sd/nothing\\\">a</a>", "it is not a cookbook page");
    broken("Some words", "[a](other.md)", "an address starts with");
    broken("link = \"/cookbook/p/y\"", "link = \"/p/none\"", "no problem");
}

#[test]
fn links_in_code_are_not_read() {
    let text = "`[a](/nope)`\n\n```text\n[b](/nope)\n```\n\n~~~\n[c](/nope)\n~~~\n\n[d](https://example.org \"t\") [e](/cookbook)";
    assert_eq!(links_in(text), ["https://example.org", "/cookbook"]);
}

#[test]
fn a_link_from_a_lesson_is_seen_with_or_without_a_section() {
    assert!(markdown_links_to("see [a](/cookbook/p/y)", "/cookbook/p/y"));
    assert!(markdown_links_to("see [a](/cookbook/p/y#cb-h)", "/cookbook/p/y"));
    assert!(!markdown_links_to("see [a](/cookbook/p/yy)", "/cookbook/p/y"));
}

fn words(n: usize) -> String {
    vec!["word"; n].join(" ")
}

#[test]
fn a_long_sentence_and_a_long_paragraph_are_reported_with_where_they_start() {
    assert_eq!(readability_of(&format!("{}.", words(35))), Vec::<String>::new());
    let long = readability_of(&format!("Start here {}.", words(34)));
    assert_eq!(long.len(), 1);
    assert!(long[0].contains("a sentence of 36 words (at most 35)") && long[0].contains("Start here word"), "{long:?}");
    let three = format!("{s}. {s}. {s}.", s = words(30));
    assert_eq!(readability_of(&three), Vec::<String>::new(), "90 words is allowed");
    let more = readability_of(&format!("{three} One more."));
    assert!(more.len() == 1 && more[0].contains("a paragraph of 92 words (at most 90)"), "{more:?}");
}

#[test]
fn wrapped_lines_are_one_paragraph_but_lists_tables_and_blank_lines_divide() {
    let line = format!("{}\n{}.", words(20), words(20));
    assert_eq!(readability_of(&line).len(), 1, "a wrapped sentence is one sentence");
    let item = words(30);
    assert_eq!(readability_of(&format!("Intro:\n- {item}\n- {item}\n1. {item}\n2) {item}")), Vec::<String>::new());
    assert_eq!(readability_of(&format!("| a | b |\n|---|---|\n| {item} | {item} |")), Vec::<String>::new());
    assert_eq!(readability_of(&format!("{item}\n\n{item}\n> [!NOTE]\n> {item}\n>\n> {item}\n### {item}\n{item}")), Vec::<String>::new());
    assert_eq!(readability_of(&format!("- {}", words(40))).len(), 1, "a long item is still long");
}

#[test]
fn code_and_link_addresses_are_not_prose() {
    let long = words(60);
    let text = format!("```rust\n{long}\n```\n\n~~~\n{long}\n~~~\n\n$$\n{long}\n$$\n\n<pre>\n{long}\n</pre>\n\nSee `{long}` and [this](https://example.org/{}) <https://example.org> https://example.org/x.", "a-".repeat(60));
    assert_eq!(readability_of(&text), Vec::<String>::new());
    assert_eq!(paragraphs("Use [the guide](https://e.org/a(b)) `x.y()` now, e.g. here."), ["Use the guide \u{1} now, e.g. here."]);
    assert_eq!(sentences("Use it, e.g. here. Then stop? Yes (really.) Done").len(), 4);
}

#[test]
fn readability_reads_lessons_only_and_covers_the_summary_sections_and_block_text() {
    let long = format!("{}.", words(40));
    let lesson = good()
        .replacen("A fixture summary", &format!("{long} A fixture summary"), 1)
        .replacen("Some words", &format!("{long} Some words"), 1)
        .replacen("text = \"One thing.\"", &format!("text = \"{long}\""), 1)
        .replacen("text = \"The first lines.\"", &format!("text = \"{long}\""), 1)
        .replacen("result = \"Nothing happens.\"", &format!("result = \"{long}\""), 1)
        .replacen("code = '''fn b() {}'''", &format!("code = '''// {long}'''"), 1);
    let dir = repo(&lesson);
    // a generated page with the same long sentence is left alone
    fs::write(dir.path().join("content/cookbook/systems/y.toml"), generated("Module", 8, &long)).unwrap();
    let found = readability(&dir.path().join("content"));
    assert!(found.iter().all(|i| i.path.ends_with("lessons/systems/x.toml")));
    let messages: Vec<&str> = found.iter().map(|i| i.message.as_str()).collect();
    for part in ["summary", "section \"How it works\":", "stepper frame 2 text", "walkthrough step 1 text", "compare wrong result"] {
        assert!(messages.iter().any(|m| m.contains(part)), "{part}: {messages:?}");
    }
    assert_eq!(messages.len(), 5, "{messages:?}");
    // the page itself still loads: only `validate` refuses long text
    assert_eq!(load_repo(&dir).1, Vec::<String>::new());
}
