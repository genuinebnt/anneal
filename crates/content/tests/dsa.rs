//! The DSA section: `dsa/problems.json` loads as tracks, and the checks around it.

use std::path::Path;

use anneal_content::{Catalog, Role, Section};

fn fixture() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/dsa-root")
}

/// A content root holding a copy of the fixture's `dsa/problems.json` and an empty `tracks/`.
fn root_with(problems_json: &str) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join("tracks")).unwrap();
    std::fs::create_dir_all(dir.path().join("dsa")).unwrap();
    std::fs::write(dir.path().join("dsa/problems.json"), problems_json).unwrap();
    dir
}

#[test]
fn the_lists_load_as_one_track_per_pattern() {
    let root = root_with(&std::fs::read_to_string(fixture().join("dsa/problems.json")).unwrap());
    let loaded = Catalog::load(root.path()).unwrap();
    assert!(loaded.issues.is_empty(), "{:?}", loaded.issues);
    let c = loaded.catalog;
    let tracks: Vec<_> = c.tracks.iter().map(|t| (t.code.as_str(), t.slug.as_str(), t.section, t.problems.len())).collect();
    assert_eq!(tracks, [("D1", "d1-arrays-hashing", Section::Dsa, 3), ("D2", "d2-two-pointers", Section::Dsa, 3)]);
    // Problems keep NeetCode's order inside a track, and the id is `lc-<slug>`.
    let ids: Vec<_> = c.tracks[0].problems.iter().map(|p| p.id.as_str()).collect();
    assert_eq!(ids, ["lc-contains-duplicate", "lc-two-sum", "lc-encode-and-decode-strings"]);

    let (t, p) = c.problem("lc-two-sum-ii-input-array-is-sorted").unwrap();
    let dsa = p.dsa.as_ref().unwrap();
    assert_eq!((t.code.as_str(), dsa.number, dsa.role, dsa.practice_of.as_deref()), ("D2", 167, Role::Practice, Some("lc-valid-palindrome")));
    assert_eq!(p.meta.teaches, ["Pointers from both ends"]);
    assert_eq!(p.meta.stage, "medium");
    assert_eq!(t.stages.iter().map(|s| s.slug.as_str()).collect::<Vec<_>>(), ["easy", "medium", "hard"]);
    assert_eq!(c.dsa.techniques.len(), 4);
    assert_eq!((c.dsa.company_groups[0].name.as_str(), c.dsa.company_groups[0].companies.as_slice()), ("Big Tech", ["Google".to_owned(), "Amazon".to_owned()].as_slice()));
    assert!(c.knows("lc-two-sum") && !c.knows("lc-nope"));
}

#[test]
fn a_dsa_section_is_optional() {
    let empty = tempfile::tempdir().unwrap();
    std::fs::create_dir(empty.path().join("tracks")).unwrap();
    let loaded = Catalog::load(empty.path()).unwrap();
    assert!(loaded.issues.is_empty() && loaded.catalog.tracks.is_empty());
}

#[test]
fn inconsistent_technique_data_is_reported() {
    let json = std::fs::read_to_string(fixture().join("dsa/problems.json")).unwrap();
    // A practice problem pointing at another practice problem, and a technique naming the wrong must-learn.
    let broken = json
        .replace(r#""practice_of": "lc-valid-palindrome"}"#, r#""practice_of": "lc-reverse-string"}"#)
        .replace(r#""must_learn": "lc-two-sum""#, r#""must_learn": "lc-valid-palindrome""#);
    let dir = root_with(&broken);
    let issues: Vec<String> = Catalog::load(dir.path()).unwrap().issues.into_iter().map(|i| i.message).collect();
    assert!(issues.iter().any(|m| m.contains("practice_of lc-reverse-string isn't a must-learn")), "{issues:?}");
    assert!(issues.iter().any(|m| m.contains("technique Arrays & Hashing:complement")), "{issues:?}");

    std::fs::write(dir.path().join("dsa/problems.json"), "{ not json").unwrap();
    let issues = Catalog::load(dir.path()).unwrap().issues;
    assert!(issues.len() == 1 && issues[0].message.starts_with("invalid JSON"), "{issues:?}");
}

#[test]
fn retired_ids_are_known_but_cannot_be_current() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join("tracks")).unwrap();
    std::fs::write(dir.path().join("retired.txt"), "# why\nd1-old\n\n  d2-older   # trailing note\n").unwrap();
    let c = Catalog::load(dir.path()).unwrap().catalog;
    assert!(c.knows("d1-old") && c.knows("d2-older") && !c.knows("d3-new"));
    assert_eq!(c.retired.len(), 2);
}

#[test]
fn pages_must_belong_to_a_listed_problem_and_parse() {
    let dir = root_with(&std::fs::read_to_string(fixture().join("dsa/problems.json")).unwrap());
    std::fs::create_dir_all(dir.path().join("dsa/pages")).unwrap();
    std::fs::write(dir.path().join("dsa/pages/not-a-problem.toml"), "intuition = \"x\"\n").unwrap();
    std::fs::write(dir.path().join("dsa/pages/two-sum.toml"), "intuition = \"x\"\n").unwrap(); // no tips, no approaches
    let issues: Vec<String> = Catalog::load(dir.path()).unwrap().issues.into_iter().map(|i| i.message).collect();
    assert!(issues.iter().any(|m| m.contains("not-a-problem isn't a problem in the NeetCode lists")), "{issues:?}");
    assert!(issues.iter().any(|m| m.contains("missing field")), "{issues:?}");

    std::fs::copy(fixture().join("dsa/pages/two-sum.toml"), dir.path().join("dsa/pages/two-sum.toml")).unwrap();
    std::fs::remove_file(dir.path().join("dsa/pages/not-a-problem.toml")).unwrap();
    let loaded = Catalog::load(dir.path()).unwrap();
    assert!(loaded.issues.is_empty(), "{:?}", loaded.issues);
    let (_, p) = loaded.catalog.problem("lc-two-sum").unwrap();
    assert_eq!(p.dsa.as_ref().unwrap().page.as_ref().unwrap().approaches.len(), 1);
}

#[test]
fn lessons_belong_to_a_technique_of_their_pattern_and_are_complete() {
    let dir = root_with(&std::fs::read_to_string(fixture().join("dsa/problems.json")).unwrap());
    std::fs::create_dir_all(dir.path().join("dsa/lessons")).unwrap();
    let good = std::fs::read_to_string(fixture().join("dsa/lessons/two-pointers.toml")).unwrap();
    let write = |name: &str, text: &str| std::fs::write(dir.path().join("dsa/lessons").join(name), text).unwrap();
    let issues = |dir: &std::path::Path| -> Vec<String> { Catalog::load(dir).unwrap().issues.into_iter().map(|i| i.message).collect() };

    write("two-pointers.toml", &good);
    let loaded = Catalog::load(dir.path()).unwrap();
    assert!(loaded.issues.is_empty(), "{:?}", loaded.issues);
    assert_eq!(loaded.catalog.dsa.lessons["Two Pointers:opposite"].signals.len(), 2);
    assert!(loaded.catalog.dsa.lesson_intros["Two Pointers"].starts_with("Two indexes"));

    // The same technique filed under another pattern, one that doesn't exist, a duplicate and an empty lesson.
    write("wrong-pattern.toml", &good.replace("pattern = \"Two Pointers\"", "pattern = \"Arrays & Hashing\""));
    write("no-pattern.toml", &good.replace("pattern = \"Two Pointers\"", "pattern = \"Cooking\""));
    write("empty.toml", "pattern = \"Arrays & Hashing\"\nintro = \"x\"\n[[technique]]\nid = \"Arrays & Hashing:seen-set\"\nsignals = []\ntemplate = \"\"\npitfalls = []\n");
    let found = issues(dir.path());
    assert!(found.iter().any(|m| m.contains("Two Pointers:opposite: belongs to Two Pointers, not Arrays & Hashing")), "{found:?}");
    assert!(found.iter().any(|m| m.contains("unknown pattern Cooking")), "{found:?}");
    assert!(found.iter().any(|m| m.contains("Arrays & Hashing:seen-set: needs signals, a template and pitfalls")), "{found:?}");
    std::fs::remove_file(dir.path().join("dsa/lessons/wrong-pattern.toml")).unwrap();
    write("dup.toml", &good);
    assert!(issues(dir.path()).iter().any(|m| m.contains("has two lessons")));
}

#[test]
fn a_pattern_page_can_have_groups_variants_extras_and_listed_techniques() {
    let dir = root_with(&std::fs::read_to_string(fixture().join("dsa/problems.json")).unwrap());
    std::fs::create_dir_all(dir.path().join("dsa/lessons")).unwrap();
    let write = |text: &str| std::fs::write(dir.path().join("dsa/lessons/two-pointers.toml"), text).unwrap();
    let issues = || -> Vec<String> { Catalog::load(dir.path()).unwrap().issues.into_iter().map(|i| i.message).collect() };
    let head = "pattern = \"Two Pointers\"\nintro = \"x\"\ngroups = [\"Walks\", \"Other\"]\n";
    let technique = "[[technique]]\nid = \"Two Pointers:opposite\"\ngroup = \"Walks\"\nsignals = [\"a\"]\ntemplate = \"pass\"\npitfalls = [\"b\"]\n";
    let extra = |body: &str| format!("{head}{technique}\n[[extra]]\nid = \"Two Pointers:meet\"\nname = \"Meet in the middle\"\ngroup = \"Walks\"\nsignals = [\"s\"]\ntemplate = \"pass\"\npitfalls = [\"p\"]\n{body}");

    // A grouped technique, an extra taught with a problem that is in the lists, and a listed technique.
    write(&format!("{}\n[[listed]]\nname = \"Fast and slow\"\ngroup = \"Other\"\nnote = \"later\"\n", extra("examples = [\"two-sum\"]\n")));
    let loaded = Catalog::load(dir.path()).unwrap();
    assert!(loaded.issues.is_empty(), "{:?}", loaded.issues);
    let d = &loaded.catalog.dsa;
    assert_eq!(d.lesson_groups["Two Pointers"], ["Walks", "Other"]);
    assert_eq!(d.lessons["Two Pointers:opposite"].group.as_deref(), Some("Walks"));
    assert_eq!(d.extras["Two Pointers"][0].examples, ["two-sum"]);
    assert_eq!(d.listed["Two Pointers"][0].name, "Fast and slow");

    // Each way an extra, a variant or a group can be wrong is reported, and the bad entry is not loaded.
    write(&extra("examples = [\"nope\"]\n"));
    assert!(issues().iter().any(|m| m.contains("Two Pointers:meet: the example nope isn't a problem of the site (the NeetCode lists or practice.json)")), "{:?}", issues());
    write(&extra("examples = []\n"));
    assert!(issues().iter().any(|m| m.contains("needs at least one example problem")), "{:?}", issues());
    write(&extra("examples = [\"two-sum\"]\n").replace("group = \"Walks\"\nsignals = [\"s\"]", "group = \"Elsewhere\"\nsignals = [\"s\"]"));
    assert!(issues().iter().any(|m| m.contains("the group \"Elsewhere\" is not declared in `groups`")), "{:?}", issues());
    write(&extra("examples = [\"two-sum\"]\n").replace("Two Pointers:meet", "Two Pointers:opposite"));
    assert!(issues().iter().any(|m| m.contains("the id is already taken")), "{:?}", issues());
    write(&extra("examples = [\"two-sum\"]\n").replace("Two Pointers:meet", "Meet"));
    assert!(issues().iter().any(|m| m.contains("an extra's id starts with Two Pointers:")), "{:?}", issues());
    // Variants: the first tab needs a name, and no two tabs share one.
    let variant = "[[extra.variant]]\nname = \"Iterative\"\ntemplate = \"pass\"\n";
    write(&extra(&format!("examples = [\"two-sum\"]\n{variant}")));
    assert!(issues().iter().any(|m| m.contains("variants need `template_name` for the first tab")), "{:?}", issues());
    write(&extra(&format!("examples = [\"two-sum\"]\ntemplate_name = \"Iterative\"\n{variant}")).replace("template_name = \"Iterative\"\n[[extra.variant]]", "template_name = \"Iterative\"\n\n[[extra.variant]]"));
    assert!(issues().iter().any(|m| m.contains("two tabs have the same name")), "{:?}", issues());
}

#[test]
fn every_problem_carries_companies_and_topic_tags_and_a_practice_problem_may_belong_to_an_extra() {
    let dir = root_with(&std::fs::read_to_string(fixture().join("dsa/problems.json")).unwrap());
    let issues = || -> Vec<String> { Catalog::load(dir.path()).unwrap().issues.into_iter().map(|i| i.message).collect() };
    let practice = |json: &str| std::fs::write(dir.path().join("dsa/practice.json"), json).unwrap();
    let entry = |extra: &str| {
        format!(
            r#"{{"generated": "x", "problems": [{{"id": "lc-meet", "slug": "meet", "number": 9001, "title": "Meet", "difficulty": "medium", "pattern": "Two Pointers",
            "lists": ["practice"], "premium": false, "tags": ["Array"], "companies": [{{"name": "Google", "group": "Big Tech", "frequency": 40.0, "recent": true}}],
            "video": null, "technique": "Two Pointers:meet", "order": 100000, "role": "practice", "practice_of": null{extra}}}]}}"#
        )
    };
    std::fs::create_dir_all(dir.path().join("dsa/lessons")).unwrap();
    let lessons = |body: &str| std::fs::write(dir.path().join("dsa/lessons/two-pointers.toml"), format!("pattern = \"Two Pointers\"\nintro = \"x\"\n[[technique]]\nid = \"Two Pointers:opposite\"\nsignals = [\"a\"]\ntemplate = \"pass\"\npitfalls = [\"b\"]\n{body}")).unwrap();
    let extra = "\n[[extra]]\nid = \"Two Pointers:meet\"\nname = \"Meet\"\ngroup = \"x\"\nsignals = [\"s\"]\ntemplate = \"pass\"\npitfalls = [\"p\"]\nexamples = [\"two-sum\"]\n";
    // the extra does not exist yet: the problem's technique is unknown
    practice(&entry(""));
    lessons("");
    assert!(issues().iter().any(|m| m.contains("lc-meet: unknown technique Two Pointers:meet")), "{:?}", issues());
    // with the extra it loads, defaults to priority "practice", and shows up under the extra's technique id
    lessons(extra);
    let loaded = Catalog::load(dir.path()).unwrap();
    assert!(loaded.issues.is_empty(), "{:?}", loaded.issues);
    let p = loaded.catalog.practice_tracks.iter().flat_map(|t| &t.problems).find(|p| p.id == "lc-meet").unwrap();
    assert_eq!(p.dsa.as_ref().unwrap().priority, "practice");
    // an explicit priority is kept, and a wrong one is refused
    practice(&entry(r#", "priority": "must""#));
    let loaded = Catalog::load(dir.path()).unwrap();
    assert_eq!(loaded.catalog.practice_tracks.iter().flat_map(|t| &t.problems).find(|p| p.id == "lc-meet").unwrap().dsa.as_ref().unwrap().priority, "must");
    practice(&entry(r#", "priority": "urgent""#));
    assert!(issues().iter().any(|m| m.contains("priority must be one of")), "{:?}", issues());
    // companies and topic tags are mandatory
    practice(&entry("").replace(r#""companies": [{"name": "Google", "group": "Big Tech", "frequency": 40.0, "recent": true}]"#, r#""companies": []"#));
    assert!(issues().iter().any(|m| m.contains("needs at least one company")), "{:?}", issues());
    practice(&entry("").replace(r#""tags": ["Array"]"#, r#""tags": []"#));
    assert!(issues().iter().any(|m| m.contains("needs at least one LeetCode topic tag")), "{:?}", issues());
    // a NeetCode-list problem without a company is an error unless waived in dsa/no_company.txt
    let mut main: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(fixture().join("dsa/problems.json")).unwrap()).unwrap();
    main["problems"][0]["companies"] = serde_json::json!([]);
    let id = main["problems"][0]["id"].as_str().unwrap().to_owned();
    std::fs::write(dir.path().join("dsa/problems.json"), main.to_string()).unwrap();
    practice(&entry(""));
    assert!(issues().iter().any(|m| m.contains(&format!("{id}: needs at least one company"))), "{:?}", issues());
    std::fs::write(dir.path().join("dsa/no_company.txt"), format!("# waived\n{id}\n")).unwrap();
    assert!(!issues().iter().any(|m| m.contains("needs at least one company")), "{:?}", issues());
}

#[test]
fn every_problem_needs_a_short_plain_key_insight_once_the_file_exists() {
    let dir = root_with(&std::fs::read_to_string(fixture().join("dsa/problems.json")).unwrap());
    let issues = || -> Vec<String> { Catalog::load(dir.path()).unwrap().issues.into_iter().map(|i| i.message).collect() };
    let write = |v: serde_json::Value| std::fs::write(dir.path().join("dsa/insights.json"), v.to_string()).unwrap();
    let all = [
        ("contains-duplicate", "A set remembers what you have seen."),
        ("two-sum", "Look up the complement in a map of earlier values."),
        ("encode-and-decode-strings", "Prefix each string with its length."),
        ("valid-palindrome", "Two pointers moving inward."),
        ("two-sum-ii-input-array-is-sorted", "Sorted input: move the pointer on the wrong side."),
        ("reverse-string", "Swap the ends and move inward."),
    ];
    // no file: the check is off
    assert!(issues().is_empty(), "{:?}", issues());
    // complete: it loads, and the sentence reaches the problem (a premium problem needs one too)
    write(serde_json::Value::Object(all.iter().map(|(k, v)| ((*k).into(), (*v).into())).collect()));
    let loaded = Catalog::load(dir.path()).unwrap();
    assert!(loaded.issues.is_empty(), "{:?}", loaded.issues);
    assert_eq!(loaded.catalog.problem("lc-two-sum").unwrap().1.dsa.as_ref().unwrap().insight, "Look up the complement in a map of earlier values.");
    // one missing, one too long, one with code in it, one for a problem the site does not have
    let mut m: serde_json::Map<String, serde_json::Value> = all.iter().map(|(k, v)| ((*k).into(), (*v).into())).collect();
    m.remove("two-sum");
    m.insert("reverse-string".into(), "x".repeat(111).into());
    m.insert("valid-palindrome".into(), "Use `two pointers`.".into());
    m.insert("nope".into(), "A sentence.".into());
    write(m.into());
    let found = issues();
    assert!(found.iter().any(|x| x.contains("lc-two-sum: needs a key insight")), "{found:?}");
    assert!(found.iter().any(|x| x.contains("reverse-string: the insight must be one plain sentence")), "{found:?}");
    assert!(found.iter().any(|x| x.contains("valid-palindrome: the insight must be one plain sentence")), "{found:?}");
    assert!(found.iter().any(|x| x.contains("nope: isn't a problem of the site")), "{found:?}");
}

#[test]
fn the_sites_own_content_has_an_insight_for_every_problem() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content");
    assert!(root.join("dsa/insights.json").exists(), "dsa/insights.json is missing: the insight check would be off");
    let loaded = Catalog::load(&root).unwrap();
    let bad: Vec<_> = loaded.issues.iter().filter(|i| i.message.contains("insight") || i.message.contains("isn't a problem of the site")).collect();
    assert!(bad.is_empty(), "{bad:?}");
    let all: Vec<_> = loaded.catalog.tracks.iter().filter(|t| t.section == Section::Dsa).chain(&loaded.catalog.practice_tracks).flat_map(|t| &t.problems).collect();
    assert!(all.len() > 1000 && all.iter().all(|p| p.dsa.as_ref().is_some_and(|d| !d.insight.is_empty())));
}

const GOOD_IDEA: &str = r#"
pattern = "Two Pointers"
tagline = "Two ends walk toward each other."
what = ["Two indexes **walk** towards each other."]
demos = ["window-fixed"]
applies = ["A sorted array."]
yes = ["The input is sorted."]
no = ["The input is unsorted and must stay so."]
apply = ["Say what each pointer means."]

[[which]]
says = "a sorted pair"
kind = "Opposite ends"
move = "Move the pointer that is too far out."
glyph = "fixed"
technique = "Two Pointers:opposite"

[[cost]]
name = "Opposite ends"
time = "O(n)"
space = "O(1)"
why = "Each pointer only moves one way."

[[with]]
pattern = "Arrays & Hashing"
how = "A hash map finds the pair when the input is not sorted."
example = "two-sum"
"#;

#[test]
fn an_idea_tab_loads_and_every_way_it_can_be_wrong_is_reported() {
    let dir = root_with(&std::fs::read_to_string(fixture().join("dsa/problems.json")).unwrap());
    std::fs::create_dir_all(dir.path().join("dsa/ideas")).unwrap();
    let write = |name: &str, text: &str| std::fs::write(dir.path().join("dsa/ideas").join(name), text).unwrap();
    let issues = |dir: &std::path::Path| -> Vec<String> { Catalog::load(dir).unwrap().issues.into_iter().map(|i| i.message).collect() };

    write("two-pointers.toml", GOOD_IDEA);
    let loaded = Catalog::load(dir.path()).unwrap();
    assert!(loaded.issues.is_empty(), "{:?}", loaded.issues);
    assert_eq!(loaded.catalog.dsa.ideas["Two Pointers"].which[0].technique, "Two Pointers:opposite");

    let mistakes = [
        (GOOD_IDEA.replace("window-fixed", "teleport"), "isn't one the web app can draw"),
        (GOOD_IDEA.replace("glyph = \"fixed\"", "glyph = \"sparkle\""), "the picture \"sparkle\""),
        (GOOD_IDEA.replace("Two Pointers:opposite", "Two Pointers:nope"), "isn't a technique of Two Pointers"),
        (GOOD_IDEA.replace("example = \"two-sum\"", "example = \"no-such-problem\""), "isn't a problem of the site"),
        (GOOD_IDEA.replace("pattern = \"Arrays & Hashing\"", "pattern = \"Two Pointers\""), "which isn't another pattern"),
        (GOOD_IDEA.replace("pattern = \"Arrays & Hashing\"", "pattern = \"Cooking\""), "which isn't another pattern"),
        (GOOD_IDEA.replace("yes = [\"The input is sorted.\"]", "yes = []"), "`yes` needs at least one entry"),
        (GOOD_IDEA.replace("time = \"O(n)\"", "time = \"\""), "has an empty field"),
        (GOOD_IDEA.replace("tagline = \"Two ends walk toward each other.\"", "tagline = \"\""), "`tagline` is one sentence"),
        (GOOD_IDEA.replace("pattern = \"Two Pointers\"\n", "pattern = \"Cooking\"\n"), "unknown pattern Cooking"),
    ];
    for (text, expected) in mistakes {
        write("two-pointers.toml", &text);
        let found = issues(dir.path());
        assert!(found.iter().any(|m| m.contains(expected)), "wanted {expected:?} in {found:?}");
    }
    // a second file for the same pattern
    write("two-pointers.toml", GOOD_IDEA);
    write("again.toml", GOOD_IDEA);
    assert!(issues(dir.path()).iter().any(|m| m.contains("has two idea files")));
}

#[test]
fn the_sites_own_idea_tabs_load_without_issues() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content");
    let loaded = Catalog::load(&root).unwrap();
    let ideas: Vec<_> = loaded.issues.iter().filter(|i| i.path.to_string_lossy().contains("/dsa/ideas/")).collect();
    assert!(ideas.is_empty(), "{ideas:?}");
    assert!(loaded.catalog.dsa.ideas.contains_key("Sliding Window"));
}
