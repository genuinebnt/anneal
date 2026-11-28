from author import T, write_track

P = []


def fix(slug, title, level, stage, tags, statement, starter, solution, visible, hidden, hints, notes, follow_up, teaches, rules=None, related=("L3",), wrong=None):
    return dict(slug=slug, title=title, mode="fix", level=level, stage=stage, tags=tags, statement=statement, starter=starter,
                solution=solution, visible=visible, hidden=hidden, hints=hints, notes=notes, follow_up=follow_up,
                teaches=teaches, rules=rules, related=list(related), wrong=wrong)


def write(slug, title, level, stage, tags, statement, starter, solution, visible, hidden, hints, notes, follow_up, teaches, related=("L3",), source=None, examples=(), wrong=None):
    return dict(slug=slug, title=title, level=level, stage=stage, tags=tags, statement=statement, starter=starter,
                solution=solution, visible=visible, hidden=hidden, hints=hints, notes=notes, follow_up=follow_up,
                teaches=teaches, related=list(related), source=source, examples=list(examples), wrong=wrong)


def sub(s, old, new):
    """str.replace that fails loudly when `old` isn't there (a wrong solution that silently equals the reference)."""
    assert old in s, f"not found: {old[:60]!r}"
    return s.replace(old, new)


# Counts heap allocations made on the current test thread, for tests that check nothing is copied per item.
ALLOC_COUNTER = r"""
use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;

struct CountingAlloc;

thread_local! {
    static ALLOCS: Cell<usize> = const { Cell::new(0) };
}

unsafe impl GlobalAlloc for CountingAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let _ = ALLOCS.try_with(|n| n.set(n.get() + 1));
        unsafe { System.alloc(layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        let _ = ALLOCS.try_with(|n| n.set(n.get() + 1));
        unsafe { System.realloc(ptr, layout, new_size) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) }
    }
}

#[global_allocator]
static GLOBAL: CountingAlloc = CountingAlloc;

/// Runs `f` and returns its result with the number of allocations (and reallocations) it made.
fn allocs<R>(f: impl FnOnce() -> R) -> (R, usize) {
    let before = ALLOCS.with(|n| n.get());
    let r = f();
    (r, ALLOCS.with(|n| n.get()) - before)
}
"""


# ---------------------------------------------------------------- elision (easy)

GLOSS_HEAD = r"""
use std::collections::HashMap;

pub struct Glossary {
    terms: HashMap<String, String>,
}

/// The first word of `text`: everything before the first space.
pub fn first_word(text: &str) -> &str {
    text.split(' ').next().unwrap_or("")
}
"""

GLOSS_STARTER = GLOSS_HEAD + r"""
impl Glossary {
    pub fn new(pairs: &[(&str, &str)]) -> Self {
        Glossary { terms: pairs.iter().map(|&(w, d)| (w.to_string(), d.to_string())).collect() }
    }

    /// The definition of `word`.
    pub fn define(&self, word: &str) -> Option<&str> {
        self.terms.get(word).map(String::as_str)
    }

    /// The definition of `word`, or `word` itself when the glossary doesn't have it.
    pub fn define_or_echo(&self, word: &str) -> &str {
        self.define(word).unwrap_or(word)
    }

    /// Whichever of `a` and `b` has the longer definition (`a` on a tie; no definition counts as length 0).
    pub fn pick(&self, a: &str, b: &str) -> &str {
        let len = |w: &str| self.define(w).map_or(0, str::len);
        if len(b) > len(a) {
            b
        } else {
            a
        }
    }
}

/// The definition of the first word of `text`.
pub fn define_first(g: &Glossary, text: &str) -> Option<&str> {
    g.define(first_word(text))
}
"""

GLOSS_SOLUTION = GLOSS_STARTER
for _old, _new in [
    ("    pub fn define_or_echo(&self, word: &str) -> &str {", "    pub fn define_or_echo<'a>(&'a self, word: &'a str) -> &'a str {"),
    ("    pub fn pick(&self, a: &str, b: &str) -> &str {", "    pub fn pick<'w>(&self, a: &'w str, b: &'w str) -> &'w str {"),
    ("pub fn define_first(g: &Glossary, text: &str) -> Option<&str> {", "pub fn define_first<'g>(g: &'g Glossary, text: &str) -> Option<&'g str> {"),
]:
    GLOSS_SOLUTION = sub(GLOSS_SOLUTION, _old, _new)

G = 'let g = Glossary::new(&[("rust", "a language"), ("borrow", "a loan"), ("ok", "fine")]);'
GD = 'glossary {rust: "a language", borrow: "a loan", ok: "fine"}'

P.append(fix(
    "when-elision-picks-self", "Fix: which input the elided lifetime picks", "easy", "elision", ["elision rules", "E0106", "&self"],
    """
        Three signatures here don't compile, or compile to something callers can't use, because elision picked
        the wrong input for the output to borrow from. Fix them by writing the lifetimes that say what each
        output really borrows. The tests keep results after the other inputs are gone. `define` and
        `first_word` are right as they are.
    """,
    GLOSS_STARTER,
    GLOSS_SOLUTION,
    [T("define_or_echo_example", GD + "; define_or_echo rust, go", '(g.define_or_echo("rust"), g.define_or_echo("go"))', '("a language", "go")', setup=G),
     T("pick_outlives_the_glossary", GD + "; pick(ok, borrow), then drop the glossary", "w", '"borrow"', setup=G + '\nlet (a, b) = (String::from("ok"), String::from("borrow"));\nlet w = g.pick(&a, &b);\ndrop(g);'),
     T("define_first_outlives_the_text", GD + "; define_first of a temporary \"rust is fun\"", "d", 'Some("a language")', setup=G + '\nlet d = define_first(&g, &String::from("rust is fun"));'),
     T("pick_tie_goes_to_a", GD + "; pick(zzz, yyy): neither defined", 'g.pick("zzz", "yyy")', '"zzz"', setup=G),
     T("echo_of_a_temporary", GD + "; define_or_echo of a word that's dropped after the call", "s", '"x".to_string()', setup=G + '\nlet s = g.define_or_echo(&String::from("x")).to_string();'),
     T("first_word_example", "first_word(\"hello world\"), first_word(\"\")", '(first_word("hello world"), first_word(""))', '("hello", "")')],
    [T("define_missing", GD + "; define go", 'g.define("go")', "None", setup=G),
     T("pick_longer_second", GD + "; pick(ok, rust)", 'g.pick("ok", "rust")', '"rust"', setup=G),
     T("pick_equal_definitions", "glossary {a: \"xx\", b: \"yy\"}; pick(b, a)", 'g.pick("b", "a")', '"b"', setup='let g = Glossary::new(&[("a", "xx"), ("b", "yy")]);'),
     T("define_first_unknown", GD + "; define_first(\"go fast\")", 'define_first(&g, "go fast")', "None", setup=G),
     T("define_first_leading_space", GD + "; define_first(\" rust\")", 'define_first(&g, " rust")', "None", setup=G),
     T("empty_glossary", "empty glossary; define_or_echo \"\"", 'g.define_or_echo("")', '""', setup="let g = Glossary::new(&[]);"),
     T("echo_empty_definition", "glossary {e: \"\"}; define_or_echo e", 'g.define_or_echo("e")', '""', setup='let g = Glossary::new(&[("e", "")]);'),
     T("unicode", "glossary {日本: \"Japan\"}; define_first(\"日本 語\")", 'define_first(&g, "日本 語")', 'Some("Japan")', setup='let g = Glossary::new(&[("日本", "Japan")]);'),
     r"""
     #[test]
     fn random_vs_model() {
         let mut rng = anneal_prelude::Rng::new(6301);
         let words = ["a", "b", "c", "d"];
         for _ in 0..300 {
             let mut defs: Vec<(String, String)> = Vec::new();
             for w in words {
                 if rng.bool() {
                     let len = rng.below(4);
                     defs.push((w.to_string(), rng.string(len, "xy")));
                 }
             }
             let pairs: Vec<(&str, &str)> = defs.iter().map(|(w, d)| (w.as_str(), d.as_str())).collect();
             let g = Glossary::new(&pairs);
             let def = |w: &str| defs.iter().find(|e| e.0 == w).map(|e| e.1.clone());
             let (a, b) = (rng.pick(&words).to_string(), rng.pick(&words).to_string());
             let la = def(&a).map_or(0, |d| d.len());
             let lb = def(&b).map_or(0, |d| d.len());
             let want = if lb > la { b.clone() } else { a.clone() };
             check!(format!("defs {defs:?}; pick({a}, {b})"), g.pick(&a, &b).to_string(), want);
             check!(format!("defs {defs:?}; define_or_echo({a})"), g.define_or_echo(&a).to_string(), def(&a).unwrap_or(a.clone()));
             let text = format!("{b} {a}");
             check!(format!("defs {defs:?}; define_first({text:?})"), define_first(&g, &text).map(String::from), def(&b));
         }
     }
     """],
    [("rust", "The elision rules: each elided input lifetime is a separate parameter; with exactly one input lifetime, outputs get it; with `&self` or `&mut self`, outputs get `self`'s. Otherwise you must write it (E0106)."),
     ("rust", "`pick` returns `a` or `b`, never the glossary's data, but rule 3 ties its output to `&self`. `define_or_echo` can return either, so both inputs need the same lifetime."),
     ("rust", "`define_first` has two reference inputs and no `self`: elision gives up. The result comes from the glossary.")],
    ("""Elision only fills in the common cases, and it looks at the signature, not the body. Rule 1: each elided lifetime in the inputs is its own parameter. Rule 2: if there's exactly one input lifetime, every elided output lifetime is that one (`first_word`). Rule 3: if there's `&self` or `&mut self`, outputs get `self`'s lifetime (`define`). Anything else needs names. `pick` compiles only if its output is tied to `a` and `b`; rule 3 would tie it to `self` and reject returning `a`. `define_or_echo` may return data from either `self` or `word`, so both must share `'a`. `define_first` must say the result borrows from `g`, not `text`: the tests drop the text first.

Syntax to remember: `fn pick<'w>(&self, a: &'w str, b: &'w str) -> &'w str` · `fn define_or_echo<'a>(&'a self, word: &'a str) -> &'a str` · `fn define_first<'g>(g: &'g Glossary, text: &str) -> Option<&'g str>`.""", "O(1) expected per lookup", "O(1)"),
    "Rule 3 favours `self` even when a method returns data from an argument. Why was that the right default for the language?",
    ["The three elision rules.", "Name the lifetime of the input the output really borrows from.", "Tying outputs to too many inputs is also wrong."],
    rules=dict(methods=["clone", "to_owned", "leak"], lines=4),
    wrong=dict(
        pick_tie_goes_to_b=sub(GLOSS_SOLUTION, "if len(b) > len(a) {", "if len(b) >= len(a) {"),
        echo_empty=sub(GLOSS_SOLUTION, "self.define(word).unwrap_or(word)", 'self.define(word).unwrap_or("")'),
        first_word_trimmed=sub(GLOSS_SOLUTION, "    g.define(first_word(text))", "    g.define(first_word(text.trim_start()))"),
    ),
))

LOCAL_STARTER = r"""
use std::borrow::Cow;

/// `text` lowercased, with its words joined by "-". When `text` is already in that form (no whitespace, no
/// char that lowercasing changes), the result borrows `text` instead of allocating.
pub fn slug(text: &str) -> &str {
    let s = text.split_whitespace().collect::<Vec<_>>().join("-").to_lowercase();
    &s
}

/// The non-blank lines of `text`, trimmed, in order.
pub fn clean_lines(text: &str) -> &[&str] {
    let lines: Vec<&str> = text.lines().map(str::trim).filter(|l| !l.is_empty()).collect();
    &lines
}

/// The file name (the part after the last '/') of the path with the longest file name; the first on a tie.
pub fn longest_file_name(paths: Vec<String>) -> Option<&str> {
    paths.iter().map(|p| p.rsplit('/').next().unwrap()).max_by_key(|n| n.len())
}
"""

LOCAL_SOLUTION = r"""
use std::borrow::Cow;

/// `text` lowercased, with its words joined by "-". When `text` is already in that form (no whitespace, no
/// char that lowercasing changes), the result borrows `text` instead of allocating.
pub fn slug(text: &str) -> Cow<'_, str> {
    if text.chars().all(|c| !c.is_whitespace() && c.to_lowercase().eq([c])) {
        return Cow::Borrowed(text);
    }
    Cow::Owned(text.split_whitespace().collect::<Vec<_>>().join("-").to_lowercase())
}

/// The non-blank lines of `text`, trimmed, in order.
pub fn clean_lines(text: &str) -> Vec<&str> {
    text.lines().map(str::trim).filter(|l| !l.is_empty()).collect()
}

/// The file name (the part after the last '/') of the path with the longest file name; the first on a tie.
pub fn longest_file_name(paths: &[String]) -> Option<&str> {
    let mut best: Option<&str> = None;
    for p in paths {
        let name = p.rsplit('/').next().unwrap();
        if best.map_or(true, |b| name.len() > b.len()) {
            best = Some(name);
        }
    }
    best
}
"""


def paths_lit(xs):
    return "[" + ", ".join(f'"{x}"' for x in xs) + "].map(String::from)"


P.append(fix(
    "fix-return-ref-to-local", "Fix: returning references to locals (E0515, E0106)", "easy", "elision", ["E0515", "E0106", "Cow", "borrow or own"],
    """
        None of these compiles: each returns a reference to something the function itself owns, which dies at
        the end of the call. Change the return types (and for `longest_file_name`, the parameter) so that
        each result owns what it must and borrows what it can. `longest_file_name` also has a bug the compiler
        can't see; the doc comment is the spec.
    """,
    LOCAL_STARTER,
    LOCAL_SOLUTION,
    [T("slug_example", "slug(\"Hello  Big World\")", 'slug("Hello  Big World")', '"hello-big-world"'),
     T("slug_borrows_when_unchanged", "slug(\"already-a-slug\") is std::borrow::Cow::Borrowed", 'matches!(slug("already-a-slug"), std::borrow::Cow::Borrowed("already-a-slug"))', "true"),
     T("clean_lines_example", "clean_lines(\"  a \\n\\n b\\n   \")", 'clean_lines("  a \\n\\n b\\n   ")', 'vec!["a", "b"]'),
     T("longest_file_name_example", "[\"src/main.rs\", \"lib/very_long.rs\", \"x/y.rs\"]", "longest_file_name(&paths)", 'Some("very_long.rs")', setup=f'let paths = {paths_lit(["src/main.rs", "lib/very_long.rs", "x/y.rs"])};'),
     T("tie_goes_to_the_first", "[\"a/xx\", \"b/yy\"]", "longest_file_name(&paths)", 'Some("xx")', setup=f'let paths = {paths_lit(["a/xx", "b/yy"])};'),
     T("empty_inputs", "slug(\"\"), clean_lines(\"\"), longest_file_name([])", '(slug(""), clean_lines(""), longest_file_name(&[]))', '(std::borrow::Cow::Borrowed(""), Vec::<&str>::new(), None)')],
    [ALLOC_COUNTER,
     T("slug_uppercase_is_owned", "slug(\"ABC\") is owned", 'matches!(slug("ABC"), std::borrow::Cow::Owned(_))', "true"),
     T("slug_unicode", "slug(\"Ünïcode Straße\")", 'slug("Ünïcode Straße")', '"ünïcode-straße"'),
     T("slug_non_ascii_uppercase", "slug(\"Ünï\")", 'slug("Ünï")', '"ünï"'),
     T("slug_tabs_newlines", "slug(\"a\\tb\\nc\")", 'slug("a\\tb\\nc")', '"a-b-c"'),
     T("slug_leading_space_is_owned", "slug(\" a\")", '(slug(" a").clone(), matches!(slug(" a"), std::borrow::Cow::Owned(_)))', '(std::borrow::Cow::<str>::Borrowed("a"), true)'),
     T("no_slash", "[\"plain\", \"a/bc\"]", "longest_file_name(&paths)", 'Some("plain")', setup=f'let paths = {paths_lit(["plain", "a/bc"])};'),
     T("trailing_slash", "[\"dir/\", \"a/b\"]", "longest_file_name(&paths)", 'Some("b")', setup=f'let paths = {paths_lit(["dir/", "a/b"])};'),
     T("clean_lines_crlf", "clean_lines(\"x\\r\\ny\\r\\n\")", 'clean_lines("x\\r\\ny\\r\\n")', 'vec!["x", "y"]'),
     T("results_borrow_the_input", "clean_lines points into the text", 'clean_lines(&text)[0].as_ptr() == text[2..].as_ptr()', "true", setup='let text = String::from("  q\\n");'),
     r"""
     #[test]
     fn random_vs_brute_force() {
         let mut rng = anneal_prelude::Rng::new(6302);
         for _ in 0..300 {
             let len = rng.below(10);
             let text = rng.string(len, "aB -\n/");
             let want_slug = text.split_whitespace().collect::<Vec<_>>().join("-").to_lowercase();
             check!(format!("slug({text:?})"), slug(&text).into_owned(), want_slug.clone());
             check!(format!("slug({text:?}) borrows iff unchanged"), matches!(slug(&text), std::borrow::Cow::Borrowed(_)), want_slug == text);
             let want_lines: Vec<&str> = text.lines().map(str::trim).filter(|l| !l.is_empty()).collect();
             check!(format!("clean_lines({text:?})"), clean_lines(&text), want_lines.clone());
             let paths: Vec<String> = want_lines.iter().map(|l| l.to_string()).collect();
             let mut best: Option<&str> = None;
             for p in &paths {
                 let n = p.rsplit('/').next().unwrap();
                 if best.map_or(true, |b| n.len() > b.len()) {
                     best = Some(n);
                 }
             }
             check!(format!("longest_file_name({paths:?})"), longest_file_name(&paths), best);
         }
     }

     #[test]
     fn unchanged_slug_does_not_allocate() {
         let text = "already-lowercase-".repeat(10_000);
         let (s, n) = allocs(|| slug(&text).len());
         check!("slug of a 180000-byte slug: allocations", (s, n), (180_000, 0));
     }
     """],
    [("rust", "A `String` or `Vec` created inside the function is dropped when it returns, so no reference to it can escape (E0515). Return the owned value itself."),
     ("rust", "`slug` often doesn't need a new string at all. `Cow<'_, str>` is either `Borrowed(&str)` from the input or `Owned(String)`: decide which before allocating."),
     ("rust", "`fn f(paths: Vec<String>) -> Option<&str>` has no input lifetime for the output to borrow (E0106). If the function doesn't need to own the paths, take `&[String]`. And `max_by_key` returns the *last* maximum.")],
    ("""A reference can only point at something that outlives the call: an input, or `'static` data. Locals, including values built with `format!`, `join` or `collect`, die at the return, so the fix is to hand them back by value (`String`, `Vec<&str>`, whose elements still borrow the input). A parameter taken by value is a local too, which is why `longest_file_name(paths: Vec<String>) -> Option<&str>` has nothing to borrow from; taking `&[String]` makes the paths the caller's, and the result borrows them. `Cow` is the middle ground when the output is often identical to the input: borrow in that case and own only when something changed, so the common path allocates nothing. Separately, `Iterator::max_by_key` keeps the last of equal maxima; \"first on a tie\" needs a strict comparison.

Syntax to remember: `fn slug(text: &str) -> Cow<'_, str>` · `Cow::Borrowed(text)` / `Cow::Owned(s)` · `c.to_lowercase().eq([c])` (lowercasing leaves `c` alone).""", "O(n)", "O(n) only when the slug differs"),
    "When would you return `impl Iterator<Item = &str> + '_` from `clean_lines` instead of a `Vec`, and what would the caller give up?",
    ["Nothing created inside a function can be borrowed by its result.", "`Cow` borrows when it can and owns when it must.", "`max_by_key` returns the last maximum."],
    rules=dict(methods=["leak", "clone"]),
    wrong=dict(
        last_on_a_tie=sub(LOCAL_SOLUTION, "if best.map_or(true, |b| name.len() > b.len()) {", "if best.map_or(true, |b| name.len() >= b.len()) {"),
        always_owned=sub(LOCAL_SOLUTION, "    if text.chars().all(|c| !c.is_whitespace() && c.to_lowercase().eq([c])) {\n        return Cow::Borrowed(text);\n    }\n", ""),
        borrows_uppercase=sub(LOCAL_SOLUTION, "c.to_lowercase().eq([c])", "!c.is_ascii_uppercase()"),
    ),
))

LONGEST_STARTER = r"""
/// The longer of `a` and `b`; `a` on a tie.
pub fn longest(a: &str, b: &str) -> &str {
    if b.len() > a.len() {
        b
    } else {
        a
    }
}

/// The longest of `words` (the first on a tie), or None if there are none.
pub fn longest_of(words: &[&str]) -> Option<&str> {
    let mut best: Option<&str> = None;
    for &w in words {
        if best.map_or(true, |b| w.len() > b.len()) {
            best = Some(w);
        }
    }
    best
}

/// Makes `best` point at `line` if `line` is longer. Returns whether it did.
pub fn keep_longest(best: &mut &str, line: &str) -> bool {
    if line.len() > best.len() {
        *best = line;
        true
    } else {
        false
    }
}
"""

LONGEST_SOLUTION = LONGEST_STARTER
for _old, _new in [
    ("pub fn longest(a: &str, b: &str) -> &str {", "pub fn longest<'a>(a: &'a str, b: &'a str) -> &'a str {"),
    ("pub fn longest_of(words: &[&str]) -> Option<&str> {\n    let mut best: Option<&str> = None;", "pub fn longest_of<'a>(words: &[&'a str]) -> Option<&'a str> {\n    let mut best: Option<&'a str> = None;"),
    ("pub fn keep_longest(best: &mut &str, line: &str) -> bool {", "pub fn keep_longest<'a>(best: &mut &'a str, line: &'a str) -> bool {"),
]:
    LONGEST_SOLUTION = sub(LONGEST_SOLUTION, _old, _new)

P.append(fix(
    "longest", "longest(a, b), and what the result borrows", "easy", "elision", ["E0106", "lifetime parameters", "&mut &'a str"],
    """
        None of these compiles. Fix the three signatures, with the lifetimes callers need: the tests keep
        `longest_of`'s result after the slice of words is gone, and store lines of their own into
        `keep_longest`'s `best`. No copying.
    """,
    LONGEST_STARTER,
    LONGEST_SOLUTION,
    [T("longest_example", "longest(\"hi\", \"hello\")", 'longest("hi", "hello")', '"hello"'),
     T("longest_tie", "longest(\"ab\", \"cd\")", 'longest("ab", "cd")', '"ab"'),
     T("longest_of_outlives_the_slice", "longest_of a temporary Vec of [\"a\", \"ccc\", \"bb\"]", "w", 'Some("ccc")',
       setup='let (a, b, c) = (String::from("a"), String::from("ccc"), String::from("bb"));\nlet w = longest_of(&vec![a.as_str(), b.as_str(), c.as_str()]);'),
     T("keep_longest_in_a_loop", "best = \"\"; keep_longest over lines of \"ab\\nabcd\\nxyzw\\na\"", "(best, changed)", '("abcd", 2)',
       setup='let text = String::from("ab\\nabcd\\nxyzw\\na");\nlet mut best: &str = "";\nlet mut changed = 0;\nfor line in text.lines() {\n    if keep_longest(&mut best, line) {\n        changed += 1;\n    }\n}'),
     T("longest_of_empty", "longest_of([])", "longest_of(&[])", "None")],
    [T("longest_empty_strings", "longest(\"\", \"\")", 'longest("", "")', '""'),
     T("longest_by_bytes", "longest(\"ééé\", \"abcd\")", 'longest("ééé", "abcd")', '"ééé"'),
     T("longest_of_first_tie", "longest_of([\"xy\", \"ab\", \"z\"])", 'longest_of(&["xy", "ab", "z"])', 'Some("xy")'),
     T("longest_of_single", "longest_of([\"\"])", 'longest_of(&[""])', 'Some("")'),
     T("keep_longest_tie_keeps", "best \"ab\"; keep_longest(\"cd\")", '{ let mut best = "ab"; (keep_longest(&mut best, "cd"), best) }', '(false, "ab")'),
     T("keep_longest_from_static", "best starts as a literal, then a line from a String", '{ let s = String::from("longer"); let mut best = "x"; keep_longest(&mut best, &s); best.to_string() }', '"longer".to_string()'),
     T("longest_mixed_lifetimes", "longest(literal, String) used while the String lives", '{ let s = String::from("dynamic"); longest("st", &s).len() }', "7"),
     T("result_is_an_input", "longest returns one of its inputs, not a copy", 'longest(&a, &b).as_ptr() == b.as_ptr()', "true", setup='let (a, b) = (String::from("x"), String::from("yy"));'),
     r"""
     #[test]
     fn random_vs_brute_force() {
         let mut rng = anneal_prelude::Rng::new(6303);
         for _ in 0..300 {
             let n = rng.below(6);
             let mut owned = Vec::new();
             for _ in 0..n {
                 let len = rng.below(5);
                 owned.push(rng.string(len, "ab"));
             }
             let words: Vec<&str> = owned.iter().map(|s| s.as_str()).collect();
             let mut want: Option<&str> = None;
             for &w in &words {
                 if want.map_or(true, |b| w.len() > b.len()) {
                     want = Some(w);
                 }
             }
             check!(format!("longest_of({words:?})"), longest_of(&words), want);
             let mut best = "";
             for &w in &words {
                 keep_longest(&mut best, w);
             }
             check!(format!("keep_longest over {words:?}"), best, want.unwrap_or(""));
             if n >= 2 {
                 let expect = if words[1].len() > words[0].len() { words[1] } else { words[0] };
                 check!(format!("longest({:?}, {:?})", words[0], words[1]), longest(words[0], words[1]), expect);
             }
         }
     }
     """],
    [("rust", "`longest` has two reference inputs and no `self`, so elision can't choose (E0106). The result may be either input, so both get the same lifetime `'a`: the result lives as long as the shorter of the two."),
     ("rust", "`longest_of(words: &[&str])` has two lifetimes hiding in one parameter: the slice's and the strings'. That's two input lifetimes, so elision gives up. Tying the output to the slice (`&'a [&'a str]`) compiles but dies with the slice; name only the strings' lifetime: `&[&'a str]`."),
     ("rust", "In `keep_longest(best: &mut &str, line: &str)` the three elided lifetimes are all different, so `*best = line` would store a reference that may die first. `line` must live as long as what `best` holds.")],
    ("""A lifetime parameter is a constraint the caller must satisfy, and the signature has to state exactly the one the body needs. `longest` may return either input, so both share `'a`, and the result lives no longer than the shorter-lived argument. `longest_of` is the subtle one: `&[&str]` hides two lifetimes, the borrow of the slice and the borrow of the strings. Tying the output to the slice would make the answer die with a temporary `Vec` even though it points into longer-lived strings. Naming the inner one (`&[&'a str] -> Option<&'a str>`) fixes that. `keep_longest` writes `line` into `*best`, so `line` must outlive what `best` holds: `&mut &'a str, &'a str`. The outer `&mut` keeps its own, shorter lifetime.

Syntax to remember: `fn longest<'a>(a: &'a str, b: &'a str) -> &'a str` · `fn longest_of<'a>(words: &[&'a str]) -> Option<&'a str>` · `fn keep_longest<'a>(best: &mut &'a str, line: &'a str) -> bool`.""", "O(n)", "O(1)"),
    "In `keep_longest`, why must the lifetime inside `&mut &'a str` match `line`'s exactly, while a `&'a str` argument can be shortened freely?",
    ["Two inputs, no `self`: write the lifetime.", "`&[&'a str]` has two lifetimes; tie outputs to the inner one.", "Storing through `&mut &'a T` requires the new value to live for `'a`."],
    rules=dict(methods=["clone", "to_string", "to_owned", "leak"], lines=5),
    wrong=dict(
        tie_goes_to_b=sub(LONGEST_SOLUTION, "    if b.len() > a.len() {", "    if b.len() >= a.len() {"),
        longest_of_last=sub(LONGEST_SOLUTION, "if best.map_or(true, |b| w.len() > b.len()) {", "if best.map_or(true, |b| w.len() >= b.len()) {"),
        keep_on_tie=sub(LONGEST_SOLUTION, "    if line.len() > best.len() {", "    if line.len() >= best.len() {"),
    ),
))

WRONG_INPUT_STARTER = r"""
use std::collections::HashMap;

/// The part of `line` after `prefix`, if `line` starts with it.
pub fn after<'a>(line: &'a str, prefix: &'a str) -> Option<&'a str> {
    line.strip_prefix(prefix)
}

/// The value for `key`.
pub fn lookup<'m>(map: &'m HashMap<String, String>, key: &'m str) -> Option<&'m str> {
    map.get(key).map(String::as_str)
}

/// `value`, or `default` when there's none.
pub fn or_default<'a>(value: Option<&'a str>, default: &'a str) -> &'a str {
    value.unwrap_or(default)
}

/// Settings looked up in `local` first, then in `global`. Global settings usually live for the whole
/// program; local ones for one request.
pub struct Layered<'g, 'l> {
    pub global: &'g [(&'g str, &'g str)],
    pub local: &'l [(&'l str, &'l str)],
}

impl<'g, 'l> Layered<'g, 'l> {
    pub fn get(&self, key: &str) -> Option<&'l str> {
        self.local.iter().chain(self.global.iter()).find(|e| e.0 == key).map(|e| e.1)
    }
}
"""

WRONG_INPUT_SOLUTION = WRONG_INPUT_STARTER
for _old, _new in [
    ("pub fn after<'a>(line: &'a str, prefix: &'a str) -> Option<&'a str> {", "pub fn after<'a>(line: &'a str, prefix: &str) -> Option<&'a str> {"),
    ("pub fn lookup<'m>(map: &'m HashMap<String, String>, key: &'m str) -> Option<&'m str> {", "pub fn lookup<'m>(map: &'m HashMap<String, String>, key: &str) -> Option<&'m str> {"),
    ("impl<'g, 'l> Layered<'g, 'l> {", "impl<'g: 'l, 'l> Layered<'g, 'l> {"),
]:
    WRONG_INPUT_SOLUTION = sub(WRONG_INPUT_SOLUTION, _old, _new)

GLOBAL = 'const GLOBAL: &[(&str, &str)] = &[("color", "auto"), ("pager", "less")];'

P.append(fix(
    "fix-output-borrows-wrong-input", "Fix: lifetimes that tie too much, or too little", "easy", "elision", ["over-constrained lifetimes", "'a: 'b", "outlives bounds"],
    """
        `after` and `lookup` compile, but tie their results to an input they don't borrow from, so the tests
        (which pass temporaries there) don't compile. `Layered::get` doesn't compile at all: it returns a
        global setting as if it were a local one. Fix all three with lifetimes alone. `or_default` is right as
        it is.
    """,
    WRONG_INPUT_STARTER,
    WRONG_INPUT_SOLUTION,
    [T("after_with_temporary_prefix", "after(\"key=value\", a temporary \"key=\")", "v", 'Some("value")', setup='let line = String::from("key=value");\nlet v = after(&line, &String::from("key="));'),
     T("lookup_with_temporary_key", "map {a: 1}; lookup of a temporary \"a\"", "v", 'Some("1")', setup='let map = std::collections::HashMap::from([("a".to_string(), "1".to_string())]);\nlet v = lookup(&map, &"a".to_string());'),
     T("layered_local_wins", "global {color: auto, pager: less}; local {color: never}", '(layered.get("color"), layered.get("pager"), layered.get("x"))', '(Some("never"), Some("less"), None)',
       setup=GLOBAL + '\nlet local = [("color", "never")];\nlet layered = Layered { global: GLOBAL, local: &local };'),
     T("layered_local_from_a_string", "local value borrowed from a request String", 'layered.get("user")', 'Some("ann")',
       setup=GLOBAL + '\nlet req = String::from("user=ann");\nlet (k, v) = req.split_once(\'=\').unwrap();\nlet local = [(k, v)];\nlet layered = Layered { global: GLOBAL, local: &local };'),
     T("or_default_example", "or_default(None, \"d\"), or_default(Some(\"v\"), \"d\")", '(or_default(None, "d"), or_default(Some("v"), "d"))', '("d", "v")')],
    [T("after_no_match", "after(\"abc\", \"x\")", 'after("abc", "x")', "None"),
     T("after_empty_prefix", "after(\"abc\", \"\")", 'after("abc", "")', 'Some("abc")'),
     T("after_whole_line", "after(\"abc\", \"abc\")", 'after("abc", "abc")', 'Some("")'),
     T("lookup_missing", "map {}; lookup x", 'lookup(&m, "x")', "None", setup="let m = std::collections::HashMap::new();"),
     T("layered_empty_local", "local {}", 'layered.get("pager")', 'Some("less")', setup=GLOBAL + '\nlet layered = Layered { global: GLOBAL, local: &[] };'),
     T("layered_first_local_duplicate", "local {a: 1, a: 2}", 'layered.get("a")', 'Some("1")', setup=GLOBAL + '\nlet local = [("a", "1"), ("a", "2")];\nlet layered = Layered { global: GLOBAL, local: &local };'),
     T("after_result_points_into_line", "after's result borrows the line", 'after(&line, "ab").unwrap().as_ptr() == line[2..].as_ptr()', "true", setup='let line = String::from("abcd");'),
     T("lookup_result_outlives_key_scope", "lookup result used after the key's block", "v.len()", "3", setup='let map = std::collections::HashMap::from([("k".to_string(), "val".to_string())]);\nlet v = {\n    let key = String::from("k");\n    lookup(&map, &key).unwrap()\n};'),
     r"""
     #[test]
     fn random_vs_model() {
         let mut rng = anneal_prelude::Rng::new(6304);
         for _ in 0..300 {
             let len = rng.below(5);
             let line = rng.string(len, "ab");
             let plen = rng.below(3);
             let prefix = rng.string(plen, "ab");
             check!(format!("after({line:?}, {prefix:?})"), after(&line, &prefix), line.strip_prefix(prefix.as_str()));
             let keys = ["a", "b", "c"];
             let global: Vec<(&str, &str)> = keys.iter().filter(|_| rng.bool()).map(|&k| (k, "g")).collect();
             let local: Vec<(&str, &str)> = keys.iter().filter(|_| rng.bool()).map(|&k| (k, "l")).collect();
             let layered = Layered { global: &global, local: &local };
             for k in keys {
                 let want = local.iter().chain(global.iter()).find(|e| e.0 == k).map(|e| e.1);
                 check!(format!("global {global:?}, local {local:?}; get({k})"), layered.get(k), want);
             }
         }
     }
     """],
    [("rust", "A lifetime on a parameter that the output doesn't borrow from still constrains the caller: they must keep that argument alive as long as the result. Give such parameters their own (elided) lifetime."),
     ("rust", "`Layered::get` returns `&'l str`, but a global value is a `&'g str`. That's fine only when `'g` outlives `'l`. Say so: `impl<'g: 'l, 'l>` (or `where 'g: 'l`)."),
     ("rust", "`or_default` really can return either input, so there one shared lifetime is right.")],
    ("""Over-constraining is the quiet lifetime bug: `after<'a>(line: &'a str, prefix: &'a str) -> Option<&'a str>` compiles, but forces every caller to keep `prefix` alive as long as the result, although the result only ever points into `line`. The fix is to leave `prefix` (and `lookup`'s `key`) with an elided lifetime of its own. The opposite case needs a relation between lifetimes: `get` may return a global value where a local-lifetime one is promised, which is sound only if `'g: 'l` (\"`'g` outlives `'l`\"). With that bound, `&'g str` shrinks to `&'l str` by covariance. The bound costs callers nothing here, since global settings are the longer-lived ones.

Syntax to remember: `fn after<'a>(line: &'a str, prefix: &str) -> Option<&'a str>` · `impl<'g: 'l, 'l> Layered<'g, 'l>` · or `struct Layered<'g: 'l, 'l>` / `where 'g: 'l`.""", "O(n)", "O(1)"),
    "Why does `'g: 'l` on the impl suffice, when the struct itself doesn't declare it?",
    ["Tie outputs only to the inputs they borrow from.", "`'a: 'b` lets a longer borrow stand in for a shorter one.", "Some functions genuinely need one lifetime for two inputs."],
    rules=dict(methods=["clone", "to_string", "to_owned", "leak"], lines=3),
    wrong=dict(
        global_wins=sub(WRONG_INPUT_SOLUTION, "self.local.iter().chain(self.global.iter())", "self.global.iter().chain(self.local.iter())"),
        after_contains=sub(WRONG_INPUT_SOLUTION, "    line.strip_prefix(prefix)\n", "    line.find(prefix).map(|i| &line[i + prefix.len()..])\n"),
        default_first=sub(WRONG_INPUT_SOLUTION, "    value.unwrap_or(default)\n", "    if default.is_empty() { value.unwrap_or(default) } else { default }\n"),
    ),
))

CAPTURE_STARTER = r"""
/// Words of `text` longer than `min` chars, as owned `String`s.
pub fn long_words(text: &str, min: usize) -> impl Iterator<Item = String> {
    text.split_whitespace().filter(move |w| w.chars().count() > min).map(String::from)
}

/// Pairs up `a` and `b` element by element (stopping at the shorter one).
pub fn pairs<'a, 'b>(a: &'a [u32], b: &'b [u32]) -> impl Iterator<Item = (u32, u32)> + 'a + 'b {
    a.iter().copied().zip(b.iter().copied())
}

/// The values of `v`, sorted, as a snapshot: later changes to `v` don't affect it.
pub fn sorted_snapshot(v: &Vec<u32>) -> impl Iterator<Item = u32> + '_ {
    let mut copy = v.to_vec();
    copy.sort_unstable();
    copy.into_iter()
}
"""

CAPTURE_SOLUTION = CAPTURE_STARTER
for _old, _new in [
    ("pub fn long_words(text: &str, min: usize) -> impl Iterator<Item = String> {", "pub fn long_words(text: &str, min: usize) -> impl Iterator<Item = String> + '_ {"),
    ("-> impl Iterator<Item = (u32, u32)> + 'a + 'b {", "-> impl Iterator<Item = (u32, u32)> + use<'a, 'b> {"),
    ("pub fn sorted_snapshot(v: &Vec<u32>) -> impl Iterator<Item = u32> + '_ {", "pub fn sorted_snapshot(v: &Vec<u32>) -> impl Iterator<Item = u32> + use<> {"),
]:
    CAPTURE_SOLUTION = sub(CAPTURE_SOLUTION, _old, _new)

P.append(fix(
    "fix-impl-trait-hides-borrow", "Fix: what an impl Trait return captures", "medium", "elision", ["E0700", "impl Trait", "+ '_", "use<..>", "precise capturing"],
    """
        Each function returns `impl Iterator`, and each says the wrong thing about what the hidden iterator
        borrows. `long_words` and `pairs` don't compile; `sorted_snapshot` compiles but claims to borrow `v`,
        so the tests (which change `v` while the snapshot is alive) don't. Fix only the return types.
    """,
    CAPTURE_STARTER,
    CAPTURE_SOLUTION,
    [T("long_words_example", "long_words(\"a quick brown fox\", 3)", 'long_words("a quick brown fox", 3).collect::<Vec<_>>()', 'vec!["quick".to_string(), "brown".to_string()]'),
     T("pairs_example", "pairs([1, 2, 3], [10, 20])", "pairs(&[1, 2, 3], &[10, 20]).collect::<Vec<_>>()", "vec![(1, 10), (2, 20)]"),
     T("pairs_lifetimes_differ", "pairs(a long-lived slice, a temporary Vec) collected in the temporary's scope", "got", "vec![(5, 7)]",
       setup="let a = vec![5u32, 6];\nlet got: Vec<(u32, u32)> = {\n    let b = vec![7u32];\n    pairs(&a, &b).collect()\n};"),
     T("snapshot_survives_changes", "take a snapshot of [3, 1, 2], then push 0 to v", "(snap.collect::<Vec<_>>(), v)", "(vec![1, 2, 3], vec![3, 1, 2, 0])",
       setup="let mut v = vec![3u32, 1, 2];\nlet snap = sorted_snapshot(&v);\nv.push(0);"),
     T("long_words_counts_chars", "long_words(\"héllo abc\", 4)", 'long_words("héllo abc", 4).collect::<Vec<_>>()', 'vec!["héllo".to_string()]')],
    [T("long_words_none", "long_words(\"a b\", 5)", 'long_words("a b", 5).count()', "0"),
     T("long_words_strict", "long_words(\"abcd abcde\", 4)", 'long_words("abcd abcde", 4).collect::<Vec<_>>()', 'vec!["abcde".to_string()]'),
     T("long_words_whitespace", "long_words(\"\\tlong\\nwords  \", 0)", 'long_words("\\tlong\\nwords  ", 0).collect::<Vec<_>>()', 'vec!["long".to_string(), "words".to_string()]'),
     T("pairs_empty", "pairs([], [1])", "pairs(&[], &[1]).count()", "0"),
     T("pairs_first_shorter", "pairs([1], [2, 3])", "pairs(&[1], &[2, 3]).collect::<Vec<_>>()", "vec![(1, 2)]"),
     T("snapshot_empty", "sorted_snapshot([])", "sorted_snapshot(&vec![]).count()", "0"),
     T("snapshot_duplicates", "sorted_snapshot([2, 2, 1])", "sorted_snapshot(&vec![2, 2, 1]).collect::<Vec<_>>()", "vec![1, 2, 2]"),
     T("snapshot_outlives_v", "snapshot, then drop v", "{ let s = { let v = vec![9u32, 8]; sorted_snapshot(&v) }; s.collect::<Vec<_>>() }", "vec![8, 9]"),
     r"""
     #[test]
     fn random_vs_brute_force() {
         let mut rng = anneal_prelude::Rng::new(6305);
         for _ in 0..300 {
             let (n, m) = (rng.below(6), rng.below(6));
             let a: Vec<u32> = rng.vec(n, 0, 9);
             let b: Vec<u32> = rng.vec(m, 0, 9);
             let want: Vec<(u32, u32)> = a.iter().copied().zip(b.iter().copied()).collect();
             check!(format!("pairs({a:?}, {b:?})"), pairs(&a, &b).collect::<Vec<_>>(), want);
             let mut v = a.clone();
             let snap = sorted_snapshot(&v);
             v.clear();
             let mut want = a.clone();
             want.sort();
             check!(format!("sorted_snapshot({a:?})"), snap.collect::<Vec<_>>(), want);
             let len = rng.below(12);
             let text = rng.string(len, "ab é");
             let min = rng.below(3);
             let want: Vec<String> = text.split_whitespace().filter(|w| w.chars().count() > min).map(String::from).collect();
             check!(format!("long_words({text:?}, {min})"), long_words(&text, min).collect::<Vec<_>>(), want);
         }
     }
     """],
    [("rust", "In edition 2021, a returned `impl Trait` captures the function's type parameters but not its lifetimes, unless the bounds mention them. `long_words`'s iterator borrows `text`, so say so: `+ '_`."),
     ("rust", "`+ 'a + 'b` means \"the hidden type outlives both `'a` and `'b`\", which a type holding a `&'a` and a `&'b` can't promise. Precise capturing says \"it may use these\" instead: `+ use<'a, 'b>`."),
     ("rust", "`sorted_snapshot` returns an iterator over its own copy; claiming `+ '_` makes callers keep `v` borrowed. `use<>` (or no bound at all in edition 2021) says it captures nothing.")],
    ("""The caller of a function returning `impl Trait` only sees the bounds, so the bounds must say which lifetimes the hidden type may hold. Edition 2021's rule: type parameters are captured automatically, lifetimes only if they appear in the bounds, hence E0700 for `long_words` until `+ '_` names the borrow. With two lifetimes, `+ 'a + 'b` is the wrong tool: it's an outlives bound, requiring the iterator to outlive each lifetime, which a type containing both references can't do unless one outlives the other. Precise capturing (`+ use<'a, 'b>`, stable since 1.82) states captures without outlives requirements. Edition 2024 flips the default: every in-scope lifetime is captured, and `use<>` (or `use<'a>`) opts out, which is what `sorted_snapshot` needs to let callers mutate `v` while the snapshot lives. Writing it now is correct in both editions.

Syntax to remember: `-> impl Iterator<Item = String> + '_` · `-> impl Iterator<Item = (u32, u32)> + use<'a, 'b>` · `-> impl Iterator<Item = u32> + use<>` · generic: `+ use<'a, T>` (list the type parameters too).""", "O(n)", "O(n) for the snapshot"),
    "Under edition 2024 rules, which of these three signatures would need a `use<..>` bound, and which would change meaning without one?",
    ["2021: `impl Trait` captures type parameters, not lifetimes, unless named.", "`use<'a, 'b>` states captures; `+ 'a` is an outlives bound.", "2024 captures everything by default; `use<>` opts out."],
    rules=dict(methods=["collect", "clone", "leak"], lines=3),
    wrong=dict(
        words_at_least_min=sub(CAPTURE_SOLUTION, "w.chars().count() > min", "w.chars().count() >= min"),
        counts_bytes=sub(CAPTURE_SOLUTION, "w.chars().count() > min", "w.len() > min"),
        snapshot_unsorted=sub(CAPTURE_SOLUTION, "    copy.sort_unstable();\n", ""),
    ),
))

# ---------------------------------------------------------------- structs holding refs (easy)

P.append(write(
    "cursor-over-str", "A parser over &str", "easy", "structs-holding-refs", ["struct lifetimes", "&'a str"],
    """
        `Cursor` walks a borrowed `&'a str`. Each method first skips whitespace, then reads its token.
        On failure it returns `None` and leaves the cursor where it was.

        - `number` reads ASCII digits as a `u64`; `None` if there are none or they overflow.
        - `ident` reads an identifier: ASCII letters, digits and `_`, not starting with a digit.
        - `rest` returns what hasn't been read.

        Tokens must stay usable after the cursor is gone.
    """,
    """
    pub struct Cursor<'a> {
        src: &'a str,
        pos: usize,
    }

    impl<'a> Cursor<'a> {
        pub fn new(src: &'a str) -> Self {
            Cursor { src, pos: 0 }
        }

        pub fn number(&mut self) -> Option<u64> {
            todo!()
        }

        pub fn ident(&mut self) -> Option<&'a str> {
            todo!()
        }

        pub fn rest(&self) -> &'a str {
            todo!()
        }
    }
    """,
    """
    pub struct Cursor<'a> {
        src: &'a str,
        pos: usize,
    }

    impl<'a> Cursor<'a> {
        pub fn new(src: &'a str) -> Self {
            Cursor { src, pos: 0 }
        }

        /// Byte offset of the next non-whitespace character.
        fn skip_ws(&self) -> usize {
            let rest = &self.src[self.pos..];
            self.pos + (rest.len() - rest.trim_start().len())
        }

        pub fn number(&mut self) -> Option<u64> {
            let start = self.skip_ws();
            let rest = &self.src[start..];
            let len = rest.bytes().take_while(u8::is_ascii_digit).count();
            let n = rest[..len].parse().ok()?;
            self.pos = start + len;
            Some(n)
        }

        pub fn ident(&mut self) -> Option<&'a str> {
            let src = self.src;
            let start = self.skip_ws();
            let rest = &src[start..];
            let b = rest.as_bytes();
            if !b.first().is_some_and(|c| c.is_ascii_alphabetic() || *c == b'_') {
                return None;
            }
            let len = b.iter().take_while(|c| c.is_ascii_alphanumeric() || **c == b'_').count();
            self.pos = start + len;
            Some(&rest[..len])
        }

        pub fn rest(&self) -> &'a str {
            let src = self.src;
            &src[self.pos..]
        }
    }
    """,
    [T("tokens", '"  let x = 42": ident, ident, number, rest', "(c.ident(), c.ident(), c.number(), c.rest())", '(Some("let"), Some("x"), None, " = 42")',
       setup='let mut c = Cursor::new("  let x = 42");'),
     T("outlives_cursor", "ident read, then the cursor is dropped", "id", 'Some("alpha")',
       setup='let src = String::from("alpha 7");\nlet id;\n{\n    let mut c = Cursor::new(&src);\n    id = c.ident();\n}'),
     T("number_then_rest", '" 42 rest": number, rest', "(c.number(), c.rest())", '(Some(42), " rest")', setup='let mut c = Cursor::new(" 42 rest");'),
     T("empty_source", '"": number, ident, rest', "(c.number(), c.ident(), c.rest())", '(None, None, "")', setup='let mut c = Cursor::new("");'),
     T("failure_keeps_position", '"  x": number fails, rest is unchanged', "(c.number(), c.rest())", '(None, "  x")', setup='let mut c = Cursor::new("  x");')],
    [T("overflow", '"99999999999999999999"', "Cursor::new(\"99999999999999999999\").number()", "None"),
     T("u64_max", '"18446744073709551615"', 'Cursor::new("18446744073709551615").number()', "Some(u64::MAX)"),
     T("one_past_max_keeps_position", '" 18446744073709551616": number, rest', "(c.number(), c.rest())", '(None, " 18446744073709551616")',
       setup='let mut c = Cursor::new(" 18446744073709551616");'),
     T("leading_zeros", '"007"', 'Cursor::new("007").number()', "Some(7)"),
     T("number_then_ident_adjacent", '"12ab": number, ident, rest', "(c.number(), c.ident(), c.rest())", '(Some(12), Some("ab"), "")', setup='let mut c = Cursor::new("12ab");'),
     T("ident_stops_at_punct", '"a_1-b": ident, rest', "(c.ident(), c.rest())", '(Some("a_1"), "-b")', setup='let mut c = Cursor::new("a_1-b");'),
     T("non_ascii_ident_start", '"éa": ident, rest', "(c.ident(), c.rest())", '(None, "éa")', setup='let mut c = Cursor::new("éa");'),
     T("tabs_and_newlines", '"\\t\\n x \\n 5": ident, number, rest', "(c.ident(), c.number(), c.rest())", '(Some("x"), Some(5), "")', setup='let mut c = Cursor::new("\\t\\n x \\n 5");'),
     T("minus_is_not_a_digit", '"-5": number, rest', "(c.number(), c.rest())", '(None, "-5")', setup='let mut c = Cursor::new("-5");'),
     """
     #[test]
     fn random_vs_brute_force() {
         let mut rng = anneal_prelude::Rng::new(306);
         for _ in 0..300 {
             // Build a source from known tokens, then read it back with the matching method.
             let count = rng.below(5);
             let mut src = String::new();
             let mut tokens: Vec<(bool, String)> = Vec::new();
             for _ in 0..count {
                 let spaces = rng.below(3);
                 src.push_str(&" ".repeat(spaces));
                 if rng.bool() {
                     let len = rng.below(3);
                     let tok = rng.string(1, "ab_") + &rng.string(len, "ab_1");
                     src.push_str(&tok);
                     tokens.push((true, tok));
                 } else {
                     let len = 1 + rng.below(4);
                     let tok = rng.string(len, "0123456789");
                     src.push_str(&tok);
                     tokens.push((false, tok));
                 }
                 src.push(' ');
             }
             let mut c = Cursor::new(&src);
             for (is_ident, tok) in &tokens {
                 if *is_ident {
                     check!(format!("src = {src:?}, number() where an ident is"), c.number(), None);
                     check!(format!("src = {src:?}"), c.ident(), Some(tok.as_str()));
                 } else {
                     check!(format!("src = {src:?}, ident() where a number is"), c.ident(), None);
                     check!(format!("src = {src:?}"), c.number(), Some(tok.parse::<u64>().unwrap()));
                 }
             }
             check!(format!("src = {src:?}: rest at the end"), c.rest().trim(), "");
         }
     }
     """,
     T("digit_first", '"9abc": ident, then number, then ident', "(c.ident(), c.number(), c.ident())", '(None, Some(9), Some("abc"))', setup='let mut c = Cursor::new("9abc");'),
     T("underscore", '"_x1 rest"', 'Cursor::new("_x1 rest").ident()', 'Some("_x1")'),
     T("unicode_after", '"n é": ident, rest', "(c.ident(), c.rest())", '(Some("n"), " é")', setup='let mut c = Cursor::new("n é");')],
    [("rust", "Return `&'a str`, not a borrow of `self`: copy `self.src` (a `&'a str`, which is `Copy`) into a local and slice that."),
     ("approach", "Compute the new position first, and only store it once the token is known to be valid.")],
    ("The struct holds a borrow, so it can't outlive `src`, but the tokens are tied to `'a`, not to the cursor. `trim_start` measures whitespace without allocating.", "O(n) total", "O(1)"),
    "How would you add `peek` without duplicating the parsing code?",
    ["A struct that borrows its input: `Cursor<'a>`.", "Methods returning `&'a str` instead of `&self`-tied slices."],
    wrong=dict(
        skips_whitespace_on_failure="""
            pub struct Cursor<'a> {
                src: &'a str,
                pos: usize,
            }

            impl<'a> Cursor<'a> {
                pub fn new(src: &'a str) -> Self {
                    Cursor { src, pos: 0 }
                }

                fn skip_ws(&mut self) {
                    let rest = &self.src[self.pos..];
                    self.pos += rest.len() - rest.trim_start().len();
                }

                pub fn number(&mut self) -> Option<u64> {
                    self.skip_ws();
                    let rest = &self.src[self.pos..];
                    let len = rest.bytes().take_while(u8::is_ascii_digit).count();
                    let n = rest[..len].parse().ok()?;
                    self.pos += len;
                    Some(n)
                }

                pub fn ident(&mut self) -> Option<&'a str> {
                    self.skip_ws();
                    let src = self.src;
                    let rest = &src[self.pos..];
                    let b = rest.as_bytes();
                    if !b.first().is_some_and(|c| c.is_ascii_alphabetic() || *c == b'_') {
                        return None;
                    }
                    let len = b.iter().take_while(|c| c.is_ascii_alphanumeric() || **c == b'_').count();
                    self.pos += len;
                    Some(&rest[..len])
                }

                pub fn rest(&self) -> &'a str {
                    let src = self.src;
                    &src[self.pos..]
                }
            }
        """,
        wrapping_number="""
            pub struct Cursor<'a> {
                src: &'a str,
                pos: usize,
            }

            impl<'a> Cursor<'a> {
                pub fn new(src: &'a str) -> Self {
                    Cursor { src, pos: 0 }
                }

                fn skip_ws(&self) -> usize {
                    let rest = &self.src[self.pos..];
                    self.pos + (rest.len() - rest.trim_start().len())
                }

                pub fn number(&mut self) -> Option<u64> {
                    let start = self.skip_ws();
                    let rest = &self.src[start..];
                    let len = rest.bytes().take_while(u8::is_ascii_digit).count();
                    if len == 0 {
                        return None;
                    }
                    let n = rest.bytes().take(len).fold(0u64, |n, d| n.wrapping_mul(10).wrapping_add(u64::from(d - b'0')));
                    self.pos = start + len;
                    Some(n)
                }

                pub fn ident(&mut self) -> Option<&'a str> {
                    let src = self.src;
                    let start = self.skip_ws();
                    let rest = &src[start..];
                    let b = rest.as_bytes();
                    if !b.first().is_some_and(|c| c.is_ascii_alphabetic() || *c == b'_') {
                        return None;
                    }
                    let len = b.iter().take_while(|c| c.is_ascii_alphanumeric() || **c == b'_').count();
                    self.pos = start + len;
                    Some(&rest[..len])
                }

                pub fn rest(&self) -> &'a str {
                    let src = self.src;
                    &src[self.pos..]
                }
            }
        """,
        ident_may_start_with_digit="""
            pub struct Cursor<'a> {
                src: &'a str,
                pos: usize,
            }

            impl<'a> Cursor<'a> {
                pub fn new(src: &'a str) -> Self {
                    Cursor { src, pos: 0 }
                }

                fn skip_ws(&self) -> usize {
                    let rest = &self.src[self.pos..];
                    self.pos + (rest.len() - rest.trim_start().len())
                }

                pub fn number(&mut self) -> Option<u64> {
                    let start = self.skip_ws();
                    let rest = &self.src[start..];
                    let len = rest.bytes().take_while(u8::is_ascii_digit).count();
                    let n = rest[..len].parse().ok()?;
                    self.pos = start + len;
                    Some(n)
                }

                pub fn ident(&mut self) -> Option<&'a str> {
                    let src = self.src;
                    let start = self.skip_ws();
                    let rest = &src[start..];
                    let len = rest.bytes().take_while(|c| c.is_ascii_alphanumeric() || *c == b'_').count();
                    if len == 0 {
                        return None;
                    }
                    self.pos = start + len;
                    Some(&rest[..len])
                }

                pub fn rest(&self) -> &'a str {
                    let src = self.src;
                    &src[self.pos..]
                }
            }
        """,
    ),
))

EXCERPT_HEAD = r"""
use std::cmp::Reverse;

#[derive(Debug, PartialEq)]
pub struct Excerpt<'a> {
    pub title: &'a str,
    pub first_line: &'a str,
}

impl<'a> Excerpt<'a> {
    /// The title is the document's first line, trimmed. `first_line` is the first non-blank line after it,
    /// trimmed ("" if there's none). Lines may end in "\n" or "\r\n".
    pub fn of(doc: &'a str) -> Self {
        let mut lines = doc.lines().map(str::trim);
        let title = lines.next().unwrap_or("");
        let first_line = lines.find(|l| !l.is_empty()).unwrap_or("");
        Excerpt { title, first_line }
    }
}

/// Every document's title, longest first (in document order on a tie).
pub fn titles(docs: &[String]) -> Vec<&str> {
    let ex = excerpts(docs);
    let mut t: Vec<&str> = ex.iter().map(|e| e.title).collect();
    t.sort_by_key(|s| Reverse(s.len()));
    t
}
"""

EXCERPT_STARTER = EXCERPT_HEAD + r"""
/// The excerpt of every document, in order.
pub fn excerpts(docs: &[String]) -> Vec<Excerpt<'_>> {
    let mut out = Vec::new();
    for d in docs {
        let normalized = d.replace("\r\n", "\n");
        out.push(Excerpt::of(&normalized));
    }
    out
}

/// The excerpt of what follows the first "---" line in `raw`, or of all of `raw` if there's no such line.
pub fn body_excerpt(raw: &str) -> Excerpt<'_> {
    let body = match raw.split_once("---\n") {
        Some((_, rest)) => rest.to_string(),
        None => raw.to_string(),
    };
    Excerpt::of(&body)
}

/// The excerpt of the document with the most lines (the first on a tie), or None if there are none.
pub fn biggest(docs: &[String]) -> Option<Excerpt<'_>> {
    let best = docs.iter().max_by_key(|d| d.lines().count())?;
    Some(Excerpt::of(&best.clone()))
}
"""

EXCERPT_SOLUTION = EXCERPT_HEAD + r"""
/// The excerpt of every document, in order.
pub fn excerpts(docs: &[String]) -> Vec<Excerpt<'_>> {
    docs.iter().map(|d| Excerpt::of(d)).collect()
}

/// The excerpt of what follows the first "---" line in `raw`, or of all of `raw` if there's no such line.
pub fn body_excerpt(raw: &str) -> Excerpt<'_> {
    let body = match raw.split_once("---\n") {
        Some((_, rest)) => rest,
        None => raw,
    };
    Excerpt::of(body)
}

/// The excerpt of the document with the most lines (the first on a tie), or None if there are none.
pub fn biggest(docs: &[String]) -> Option<Excerpt<'_>> {
    let mut best: Option<&String> = None;
    for d in docs {
        if best.map_or(true, |b| d.lines().count() > b.lines().count()) {
            best = Some(d);
        }
    }
    Some(Excerpt::of(best?))
}
"""


def docs_lit(xs):
    return "[" + ", ".join('"' + x.replace("\n", "\\n").replace("\r", "\\r") + '"' for x in xs) + "].map(String::from)"


def ex(t, f):
    return f'Excerpt {{ title: "{t}", first_line: "{f}" }}'


P.append(fix(
    "fix-struct-outlives-source", "Fix: a struct that outlives what it borrows", "easy", "structs-holding-refs", ["E0515", "E0597", "struct lifetimes", "borrow the input"],
    """
        `excerpts`, `body_excerpt` and `biggest` each build an `Excerpt` from a `String` they made
        themselves, which dies at the end of the function (or the loop turn) while the `Excerpt` still
        borrows it. Fix them so every `Excerpt` borrows the caller's documents, with no copies. `biggest` also
        has a bug the compiler can't see. `Excerpt::of` and `titles` are fine.
    """,
    EXCERPT_STARTER,
    EXCERPT_SOLUTION,
    [T("excerpts_example", '["Intro\\n\\nfirst words", " Notes \\r\\n body\\r\\n"]', "excerpts(&docs)", f'vec![{ex("Intro", "first words")}, {ex("Notes", "body")}]', setup='let docs = ' + docs_lit(["Intro\n\nfirst words", " Notes \r\n body\r\n"]) + ';'),
     T("body_excerpt_example", '"meta\\n---\\nTitle\\ntext"', 'body_excerpt("meta\\n---\\nTitle\\ntext")', ex("Title", "text")),
     T("body_without_separator", '"Just\\none"', 'body_excerpt("Just\\none")', ex("Just", "one")),
     T("biggest_first_on_tie", '["a\\nb", "c\\nd", "e"]', "biggest(&docs)", f'Some({ex("a", "b")})', setup='let docs = ' + docs_lit(["a\nb", "c\nd", "e"]) + ';'),
     T("titles_longest_first", '["ab", "abcd", "xy"]', "titles(&docs)", 'vec!["abcd", "ab", "xy"]', setup='let docs = ' + docs_lit(["ab", "abcd", "xy"]) + ';'),
     T("excerpts_borrow_the_docs", "the title points into the document", "excerpts(&docs)[0].title.as_ptr() == docs[0][1..].as_ptr()", "true", setup='let docs = ' + docs_lit([" T"]) + ';')],
    [T("no_docs", "[]", "(excerpts(&[]).len(), biggest(&[]))", "(0, None)"),
     T("empty_doc", '[""]', "excerpts(&docs)", f'vec![{ex("", "")}]', setup='let docs = ' + docs_lit([""]) + ';'),
     T("only_blank_after_title", '["T\\n \\n\\t\\n"]', "excerpts(&docs)", f'vec![{ex("T", "")}]', setup='let docs = ' + docs_lit(["T\n \n\t\n"]) + ';'),
     T("separator_first", '"---\\nA\\nB"', 'body_excerpt("---\\nA\\nB")', ex("A", "B")),
     T("two_separators", '"x\\n---\\ny\\n---\\nz"', 'body_excerpt("x\\n---\\ny\\n---\\nz")', ex("y", "---")),
     T("biggest_later_wins", '["a", "b\\nc\\nd", "e\\nf"]', "biggest(&docs)", f'Some({ex("b", "c")})', setup='let docs = ' + docs_lit(["a", "b\nc\nd", "e\nf"]) + ';'),
     T("biggest_counts_lines_not_bytes", '["long line here", "a\\nb"]', "biggest(&docs)", f'Some({ex("a", "b")})', setup='let docs = ' + docs_lit(["long line here", "a\nb"]) + ';'),
     T("body_points_into_raw", "body_excerpt's title points into raw", 'body_excerpt(&raw).title.as_ptr() == raw[4..].as_ptr()', "true", setup='let raw = String::from("---\\nZ");'),
     T("unicode", '["日本\\r\\n 語 "]', "excerpts(&docs)", f'vec![{ex("日本", "語")}]', setup='let docs = ' + docs_lit(["日本\r\n 語 "]) + ';'),
     r"""
     #[test]
     fn random_vs_model() {
         let mut rng = anneal_prelude::Rng::new(6306);
         for _ in 0..300 {
             let n = rng.below(4);
             let mut docs: Vec<String> = Vec::new();
             for _ in 0..n {
                 let len = rng.below(10);
                 docs.push(rng.string(len, "ab \n\r"));
             }
             let model = |d: &str| {
                 let mut lines = d.lines().map(str::trim);
                 let t = lines.next().unwrap_or("").to_string();
                 let f = lines.find(|l| !l.is_empty()).unwrap_or("").to_string();
                 (t, f)
             };
             let got: Vec<(String, String)> = excerpts(&docs).iter().map(|e| (e.title.to_string(), e.first_line.to_string())).collect();
             let want: Vec<(String, String)> = docs.iter().map(|d| model(d)).collect();
             check!(format!("excerpts({docs:?})"), got, want);
             let mut best: Option<&String> = None;
             for d in &docs {
                 if best.map_or(true, |b| d.lines().count() > b.lines().count()) {
                     best = Some(d);
                 }
             }
             let got = biggest(&docs).map(|e| (e.title.to_string(), e.first_line.to_string()));
             check!(format!("biggest({docs:?})"), got, best.map(|b| model(b)));
         }
     }
     """],
    [("rust", "`Excerpt::of(&normalized)` borrows a `String` created in this loop turn, and it's dropped at the end of the turn while `out` still holds the excerpt: the returned `Vec` would point at freed memory (E0515; in other shapes, E0597). Does anything need normalizing? `str::lines` already strips a trailing `\\r`."),
     ("rust", "`body_excerpt` copies the part after \"---\" into a new `String` just to borrow it. `split_once` already gives you a slice of `raw`."),
     ("rust", "`max_by_key` keeps the last of equal maxima; the spec wants the first. And `best.clone()` makes a copy that dies at the end of the statement.")],
    ("""A struct with a lifetime parameter is a borrow with fields: `Excerpt<'a>` can't outlive the `str` it points into. Every failing line here borrows a `String` the function created (a normalized copy, a `to_string`, a `clone`), which is dropped at the end of its loop turn, block or statement, while the excerpt escapes. The fix is to borrow the caller's data all the way through: slices of `docs` and `raw` live as long as the caller keeps them, which is what `Vec<Excerpt<'_>>` in the signature promises. The copies were never needed: `lines()` handles `\\r\\n`, and `split_once` returns slices.

`titles` shows the flip side: `ex.iter().map(|e| e.title)` copies each `&'a str` out of the excerpts, so the titles borrow the documents, not `ex`, and outlive it.""", "O(total text)", "O(n) excerpts"),
    "`titles` builds `ex` and returns slices after `ex` is dropped. Why does that compile, when returning `&ex[0]` wouldn't?",
    ["A struct's lifetime parameter is a borrow it can't outlive.", "Borrow the caller's data, not a local copy of it.", "Copying a `&'a str` out of a struct keeps the original lifetime."],
    rules=dict(methods=["clone", "to_string", "to_owned", "replace", "leak", "into"]),
    wrong=dict(
        biggest_last_on_tie=sub(EXCERPT_SOLUTION, "if best.map_or(true, |b| d.lines().count() > b.lines().count()) {", "if best.map_or(true, |b| d.lines().count() >= b.lines().count()) {"),
        body_after_last_separator=sub(EXCERPT_SOLUTION, 'match raw.split_once("---\\n") {', 'match raw.rsplit_once("---\\n") {'),
        biggest_by_bytes=sub(EXCERPT_SOLUTION, "if best.map_or(true, |b| d.lines().count() > b.lines().count()) {", "if best.map_or(true, |b| d.len() > b.len()) {"),
    ),
))

INDEX_STARTER = r"""
pub struct Index<'a> {
    lines: Vec<&'a str>,
}

impl<'a> Index<'a> {
    pub fn new(text: &'a str) -> Self {
        Index { lines: text.lines().collect() }
    }

    /// The longest line; the first on a tie.
    pub fn longest_line(&self) -> &str {
        self.lines.iter().copied().fold("", |best, l| if l.len() > best.len() { l } else { best })
    }

    /// Every line containing `word`.
    pub fn lines_with(&self, word: &str) -> Vec<&str> {
        self.lines.iter().copied().filter(|l| l.contains(word)).collect()
    }

    /// The lines, in order.
    pub fn iter(&self) -> impl Iterator<Item = &str> {
        self.lines.iter().copied()
    }

    /// Hands the lines over.
    pub fn into_lines(self) -> Vec<&str> {
        self.lines
    }
}
"""

INDEX_SOLUTION = INDEX_STARTER
for _old, _new in [
    ("    pub fn longest_line(&self) -> &str {", "    pub fn longest_line(&self) -> &'a str {"),
    ("    pub fn lines_with(&self, word: &str) -> Vec<&str> {", "    pub fn lines_with(&self, word: &str) -> Vec<&'a str> {"),
    ("    pub fn iter(&self) -> impl Iterator<Item = &str> {", "    pub fn iter(&self) -> impl Iterator<Item = &'a str> + '_ {"),
    ("    pub fn into_lines(self) -> Vec<&str> {", "    pub fn into_lines(self) -> Vec<&'a str> {"),
]:
    INDEX_SOLUTION = sub(INDEX_SOLUTION, _old, _new)

TXT = 'let text = String::from("fn main\\nlet x\\nfn helper\\n");'

P.append(fix(
    "fix-result-borrows-struct", "Fix: the result borrows the struct, not the text", "easy", "structs-holding-refs", ["'a vs '_", "elision rule 3", "impl Trait + '_", "E0106"],
    """
        `Index` holds slices of a text. Its methods' results are slices of that text too, but elision ties
        them to `&self`, so callers can't keep them once the `Index` is gone, and `into_lines` doesn't
        compile at all. Fix the four return types so each says what it really borrows. The tests drop the
        `Index` and keep the results.
    """,
    INDEX_STARTER,
    INDEX_SOLUTION,
    [T("longest_line_outlives_index", TXT + " longest_line, then drop the index", "l", '"fn helper"', setup=TXT + "\nlet l = Index::new(&text).longest_line();"),
     T("lines_with", "lines containing \"fn\", after the index is dropped", "v", 'vec!["fn main", "fn helper"]', setup=TXT + '\nlet v = Index::new(&text).lines_with("fn");'),
     T("iter_items_outlive_index", "collect iter() into a Vec, drop the index", "v", 'vec!["fn main", "let x", "fn helper"]', setup=TXT + "\nlet v: Vec<&str> = {\n    let ix = Index::new(&text);\n    ix.iter().collect()\n};"),
     T("into_lines", "into_lines", "Index::new(&text).into_lines()", 'vec!["fn main", "let x", "fn helper"]', setup=TXT),
     T("lines_with_temporary_word", "lines_with a word that's dropped first", "v", 'vec!["let x"]', setup=TXT + '\nlet ix = Index::new(&text);\nlet v = ix.lines_with(&String::from("x"));')],
    [T("empty_text", "\"\"", '(Index::new("").longest_line(), Index::new("").into_lines().len())', '("", 0)'),
     T("longest_tie_first", "\"ab\\ncd\"", 'Index::new("ab\\ncd").longest_line()', '"ab"'),
     T("lines_with_none", "no line contains \"zz\"", 'Index::new("a\\nb").lines_with("zz").len()', "0"),
     T("lines_with_empty_word", "every line contains \"\"", 'Index::new("a\\nb").lines_with("")', 'vec!["a", "b"]'),
     T("results_point_into_text", "longest_line points into the text", "Index::new(&text).longest_line().as_ptr() == text[14..].as_ptr()", "true", setup=TXT),
     T("iter_twice", "iter() twice while the index lives", "(ix.iter().count(), ix.iter().last())", '(3, Some("fn helper"))', setup=TXT + "\nlet ix = Index::new(&text);"),
     T("crlf", "\"a\\r\\nbb\\r\\n\"", 'Index::new("a\\r\\nbb\\r\\n").into_lines()', 'vec!["a", "bb"]'),
     T("unicode_longest", "\"ééé\\nabcd\"", 'Index::new("ééé\\nabcd").longest_line()', '"ééé"'),
     r"""
     #[test]
     fn random_vs_model() {
         let mut rng = anneal_prelude::Rng::new(6307);
         for _ in 0..300 {
             let len = rng.below(14);
             let text = rng.string(len, "ab\n");
             let lines: Vec<&str> = text.lines().collect();
             let mut longest = "";
             for &l in &lines {
                 if l.len() > longest.len() {
                     longest = l;
                 }
             }
             let with: Vec<&str> = lines.iter().copied().filter(|l| l.contains("ab")).collect();
             let got = {
                 let ix = Index::new(&text);
                 (ix.longest_line(), ix.lines_with("ab"), ix.iter().collect::<Vec<_>>())
             };
             check!(format!("text {text:?}"), got, (longest, with, lines.clone()));
         }
     }
     """],
    [("rust", "Elision rule 3 ties every output of a `&self` method to `&self`. But these results are slices of the text, which lives for `'a`, longer than any borrow of the `Index`. Name `'a` in the output."),
     ("rust", "`iter` really does borrow both: the `Vec` inside `self` (to iterate it) and the text (its items). Items `&'a str`, iterator `+ '_`."),
     ("rust", "`into_lines(self)` has no reference input at all, so elision has nothing to use (E0106); the lines' lifetime is `'a`.")],
    ("""Inside `impl<'a> Index<'a>`, `&self` is really `&'s Index<'a>` with `'a: 's`: the text outlives any borrow of the index. Elision gives a `&self` method's output `'s`, which is correct for data owned by the struct (like a `&[&str]` of `self.lines`) but too short for data the struct only points at. Writing `&'a str` promises callers the longer lifetime, and the body can deliver it because copying a `&'a str` out of `self.lines` keeps `'a`. `iter` needs both lifetimes: the iterator borrows `self.lines` (`+ '_`) and yields `&'a str`. `into_lines(self)` consumes the index and has no reference input, so the output's lifetime must be named.

Syntax to remember: `fn longest_line(&self) -> &'a str` · `fn iter(&self) -> impl Iterator<Item = &'a str> + '_` · `fn into_lines(self) -> Vec<&'a str>`.""", "O(n)", "O(1) extra"),
    "When is `-> &str` (tied to `&self`) the right choice on a struct that holds `&'a str`?",
    ["`&self` outputs get the borrow of `self`, not the struct's `'a`.", "Name `'a` for data the struct only points at.", "An iterator can borrow `self` and yield `'a` items."],
    rules=dict(methods=["clone", "to_string", "to_owned", "leak"], lines=4),
    wrong=dict(
        longest_last_on_tie=sub(INDEX_SOLUTION, "if l.len() > best.len() { l } else { best }", "if l.len() >= best.len() { l } else { best }"),
        lines_with_starts=sub(INDEX_SOLUTION, "filter(|l| l.contains(word))", "filter(|l| l.starts_with(word))"),
        iter_reversed=sub(INDEX_SOLUTION, "        self.lines.iter().copied()\n    }\n\n    /// Hands", "        self.lines.iter().rev().copied()\n    }\n\n    /// Hands"),
    ),
))

# ---------------------------------------------------------------- two lifetimes (medium)

SPLIT1_STARTER = r"""
/// Splits `text` on `sep` (not empty), lazily, like `str::split`. Pieces borrow only from `text`.
pub struct Splitter<'a> {
    text: &'a str,
    sep: &'a str,
    done: bool,
}

impl<'a> Splitter<'a> {
    pub fn new(text: &'a str, sep: &'a str) -> Self {
        Splitter { text, sep, done: false }
    }

    /// The next piece, without advancing.
    pub fn peek(&self) -> Option<&'a str> {
        if self.done {
            return None;
        }
        Some(self.text.find(self.sep).map_or(self.text, |i| &self.text[..i]))
    }
}

impl<'a> Iterator for Splitter<'a> {
    type Item = &'a str;

    fn next(&mut self) -> Option<&'a str> {
        if self.done {
            return None;
        }
        match self.text.find(self.sep) {
            Some(i) => {
                let piece = &self.text[..i];
                self.text = &self.text[i + self.sep.len()..];
                Some(piece)
            }
            None => {
                self.done = true;
                Some(self.text)
            }
        }
    }
}

/// Splits every line of `text` on `sep_char`, returning all the pieces in order.
pub fn split_lines(text: &str, sep_char: char) -> Vec<&str> {
    let sep = sep_char.to_string();
    let mut out = Vec::new();
    for line in text.lines() {
        out.extend(Splitter::new(line, &sep));
    }
    out
}
"""

SPLIT1_SOLUTION = SPLIT1_STARTER
for _old, _new in [
    ("pub struct Splitter<'a> {\n    text: &'a str,\n    sep: &'a str,", "pub struct Splitter<'t, 's> {\n    text: &'t str,\n    sep: &'s str,"),
    ("impl<'a> Splitter<'a> {\n    pub fn new(text: &'a str, sep: &'a str) -> Self {", "impl<'t, 's> Splitter<'t, 's> {\n    pub fn new(text: &'t str, sep: &'s str) -> Self {"),
    ("    pub fn peek(&self) -> Option<&'a str> {", "    pub fn peek(&self) -> Option<&'t str> {"),
    ("impl<'a> Iterator for Splitter<'a> {\n    type Item = &'a str;\n\n    fn next(&mut self) -> Option<&'a str> {", "impl<'t, 's> Iterator for Splitter<'t, 's> {\n    type Item = &'t str;\n\n    fn next(&mut self) -> Option<&'t str> {"),
]:
    SPLIT1_SOLUTION = sub(SPLIT1_SOLUTION, _old, _new)

P.append(fix(
    "why-a-everywhere-fails", "Why 'a everywhere fails", "medium", "two-lifetimes", ["two lifetime parameters", "E0515", "lifetimes in impls", "Iterator"],
    """
        `Splitter` borrows the text and the separator with one lifetime `'a`, and its pieces are `&'a str`.
        So a piece seems to borrow the separator too, and `split_lines`, whose separator is a local
        `String`, can't return its pieces. Fix `Splitter` so pieces borrow only the text. Don't touch
        `split_lines`, and keep the pieces borrowed (no copies).
    """,
    SPLIT1_STARTER,
    SPLIT1_SOLUTION,
    [T("split_lines_example", "split_lines(\"a,b\\n,c\", ',')", 'split_lines("a,b\\n,c", \',\')', 'vec!["a", "b", "", "c"]'),
     T("pieces_outlive_the_separator", "collect pieces of \"x--y\" split on a temporary \"--\"", "v", 'vec!["x", "y"]', setup='let text = String::from("x--y");\nlet v: Vec<&str> = Splitter::new(&text, &String::from("--")).collect();'),
     T("peek_does_not_advance", "\"a;b\" on \";\": peek, next, peek", '(s.peek(), s.next(), s.peek())', '(Some("a"), Some("a"), Some("b"))', setup='let mut s = Splitter::new("a;b", ";");'),
     T("like_str_split", "\";a;;b;\" on \";\"", 'Splitter::new(";a;;b;", ";").collect::<Vec<_>>()', 'vec!["", "a", "", "b", ""]'),
     T("no_separator", "\"abc\" on \",\"", 'Splitter::new("abc", ",").collect::<Vec<_>>()', 'vec!["abc"]'),
     T("empty_text", "\"\" on \",\"", 'Splitter::new("", ",").collect::<Vec<_>>()', 'vec![""]')],
    [T("split_lines_empty", "split_lines(\"\", ',')", 'split_lines("", \',\').len()', "0"),
     T("multi_char_separator", "\"a<>b<>c\" on \"<>\"", 'Splitter::new("a<>b<>c", "<>").collect::<Vec<_>>()', 'vec!["a", "b", "c"]'),
     T("overlapping_separator", "\"aaa\" on \"aa\"", 'Splitter::new("aaa", "aa").collect::<Vec<_>>()', 'vec!["", "a"]'),
     T("peek_many_separators", "\"a;b;c\" on \";\": peek", 'Splitter::new("a;b;c", ";").peek()', 'Some("a")'),
     T("peek_after_end", "\"a\": next, then peek", '{ let mut s = Splitter::new("a", ","); s.next(); (s.peek(), s.next()) }', "(None, None)"),
     T("unicode_separator", "\"日→本→語\" on \"→\"", 'split_lines("日→本→語", \'→\')', 'vec!["日", "本", "語"]'),
     T("split_lines_crlf", "\"a,b\\r\\nc\"", 'split_lines("a,b\\r\\nc", \',\')', 'vec!["a", "b", "c"]'),
     T("pieces_point_into_text", "the first piece is a slice of the text", 'Splitter::new(&t, ",").nth(1).unwrap().as_ptr() == t[2..].as_ptr()', "true", setup='let t = String::from("a,b");'),
     T("peek_outlives_splitter", "peek's result used after the splitter is gone", "p", 'Some("left")', setup='let text = String::from("left|right");\nlet p = { let sep = String::from("|"); Splitter::new(&text, &sep).peek() };'),
     r"""
     #[test]
     fn random_vs_brute_force() {
         let mut rng = anneal_prelude::Rng::new(6308);
         for _ in 0..400 {
             let len = rng.below(10);
             let text = rng.string(len, "ab,\n");
             let slen = 1 + rng.below(2);
             let sep = rng.string(slen, "ab,");
             let want: Vec<&str> = text.split(sep.as_str()).collect();
             check!(format!("{text:?} on {sep:?}"), Splitter::new(&text, &sep).collect::<Vec<_>>(), want);
             let want: Vec<&str> = text.lines().flat_map(|l| l.split(',')).collect();
             check!(format!("split_lines({text:?}, ',')"), split_lines(&text, ','), want);
         }
     }

     #[test]
     fn long_text() {
         let text = "ab,".repeat(100_000);
         let n = split_lines(&text, ',').len();
         check!("\"ab,\" x 100000", n, 100_001);
     }
     """],
    [("rust", "With one lifetime for both fields, `&'a str` pieces are only as long-lived as the shorter of the text and the separator: the compiler picks an `'a` that fits both, and `split_lines`'s separator dies at the end of the function."),
     ("rust", "Give each borrow its own parameter: `Splitter<'t, 's>`, with pieces `&'t str`. The `impl` blocks, including `impl Iterator for`, need both parameters too, and `type Item = &'t str`.")],
    ("""One lifetime parameter for two borrows means \"both live at least this long\", so `'a` is at most the shorter of the two, and every output typed `&'a str` inherits that limit. Here the pieces only point into `text`, but their type says they might point into `sep`, so `split_lines` can't return them once its local separator is dropped. Separate parameters let the output name the one it borrows. The same parameters appear on every `impl`, including the trait impl, where the associated type `Item = &'t str` fixes what `next` returns. Copying `self.text` (a `&'t str`) out of `&mut self` keeps `'t`, which is why `next` can return pieces that outlive the borrow of the splitter.

Syntax to remember: `pub struct Splitter<'t, 's> { text: &'t str, sep: &'s str }` · `impl<'t, 's> Iterator for Splitter<'t, 's> { type Item = &'t str; fn next(&mut self) -> Option<&'t str> { .. } }`.""", "O(n · |sep|)", "O(1) per piece"),
    "`std::str::Split<'a, P>` has only one lifetime. How does it avoid this problem when the pattern is a `&str`?",
    ["One lifetime for two borrows is limited by the shorter one.", "Give independent borrows independent parameters.", "Lifetime parameters on trait impls and associated types."],
    rules=dict(methods=["split", "split_terminator", "clone", "to_owned", "leak"], lines=10),
    wrong=dict(
        drops_the_last_piece=sub(SPLIT1_SOLUTION, "                self.done = true;\n                Some(self.text)", "                self.done = true;\n                if self.text.is_empty() { None } else { Some(self.text) }"),
        peek_to_last_separator=sub(SPLIT1_SOLUTION, "self.text.find(self.sep).map_or(self.text, |i| &self.text[..i])", "self.text.rfind(self.sep).map_or(self.text, |i| &self.text[..i])"),
        skips_one_byte_less=sub(SPLIT1_SOLUTION, "self.text = &self.text[i + self.sep.len()..];", "self.text = &self.text[i + 1..];"),
    ),
))


P.append(write(
    "zero-copy-tokenizer", "Zero-copy tokenizer", "medium", "two-lifetimes", ["Iterator", "enum with lifetimes"],
    """
        `Tokenizer` yields `Token`s borrowed from the source: identifiers (ASCII letter or `_`, then letters,
        digits, `_`), numbers (ASCII digits) and single punctuation characters (any other non-whitespace
        character). Whitespace separates tokens.

        `longest_ident` returns the longest identifier from any token iterator (the first one on a tie). It
        must work on tokens that outlive the tokenizer.
    """,
    """
    #[derive(Debug, PartialEq, Clone, Copy)]
    pub enum Token<'a> {
        Ident(&'a str),
        Number(&'a str),
        Punct(char),
    }

    pub struct Tokenizer<'a> {
        rest: &'a str,
    }

    impl<'a> Tokenizer<'a> {
        pub fn new(src: &'a str) -> Self {
            Tokenizer { rest: src }
        }
    }

    impl<'a> Iterator for Tokenizer<'a> {
        type Item = Token<'a>;

        fn next(&mut self) -> Option<Token<'a>> {
            todo!()
        }
    }

    pub fn longest_ident<'a, I: IntoIterator<Item = Token<'a>>>(tokens: I) -> Option<&'a str> {
        todo!()
    }
    """,
    """
    #[derive(Debug, PartialEq, Clone, Copy)]
    pub enum Token<'a> {
        Ident(&'a str),
        Number(&'a str),
        Punct(char),
    }

    pub struct Tokenizer<'a> {
        rest: &'a str,
    }

    impl<'a> Tokenizer<'a> {
        pub fn new(src: &'a str) -> Self {
            Tokenizer { rest: src }
        }
    }

    fn is_ident_start(c: char) -> bool {
        c.is_ascii_alphabetic() || c == '_'
    }

    impl<'a> Iterator for Tokenizer<'a> {
        type Item = Token<'a>;

        fn next(&mut self) -> Option<Token<'a>> {
            let s = self.rest.trim_start();
            let c = s.chars().next()?;
            let len = if is_ident_start(c) {
                s.find(|c: char| !(c.is_ascii_alphanumeric() || c == '_')).unwrap_or(s.len())
            } else if c.is_ascii_digit() {
                s.find(|c: char| !c.is_ascii_digit()).unwrap_or(s.len())
            } else {
                c.len_utf8()
            };
            let (text, rest) = s.split_at(len);
            self.rest = rest;
            Some(if is_ident_start(c) {
                Token::Ident(text)
            } else if c.is_ascii_digit() {
                Token::Number(text)
            } else {
                Token::Punct(c)
            })
        }
    }

    pub fn longest_ident<'a, I: IntoIterator<Item = Token<'a>>>(tokens: I) -> Option<&'a str> {
        tokens
            .into_iter()
            .filter_map(|t| match t {
                Token::Ident(s) => Some(s),
                _ => None,
            })
            .reduce(|best, s| if s.len() > best.len() { s } else { best })
    }
    """,
    [T("mixed", '"let x1 = 42+y;"', 'Tokenizer::new("let x1 = 42+y;").collect::<Vec<_>>()',
       "vec![Token::Ident(\"let\"), Token::Ident(\"x1\"), Token::Punct('='), Token::Number(\"42\"), Token::Punct('+'), Token::Ident(\"y\"), Token::Punct(';')]"),
     T("longest_outlives_tokenizer", '"a bb ccc dd"', "best", 'Some("ccc")',
       setup='let src = String::from("a bb ccc dd");\nlet best;\n{\n    let t = Tokenizer::new(&src);\n    best = longest_ident(t);\n}'),
     T("empty", '""', 'Tokenizer::new("").count()', "0"),
     T("number_then_ident", '"12ab _x9"', 'Tokenizer::new("12ab _x9").collect::<Vec<_>>()', 'vec![Token::Number("12"), Token::Ident("ab"), Token::Ident("_x9")]'),
     T("no_idents", '"1 + 2"', 'longest_ident(Tokenizer::new("1 + 2"))', "None")],
    [T("unicode_punct", '"a→b"', 'Tokenizer::new("a→b").collect::<Vec<_>>()', "vec![Token::Ident(\"a\"), Token::Punct('→'), Token::Ident(\"b\")]"),
     T("leading_zeros", '"007"', 'Tokenizer::new("007").collect::<Vec<_>>()', 'vec![Token::Number("007")]'),
     T("punct_runs_split", '"==>"', 'Tokenizer::new("==>").collect::<Vec<_>>()', "vec![Token::Punct('='), Token::Punct('='), Token::Punct('>')]"),
     T("tabs_and_newlines", '"a\\n\\tb"', 'Tokenizer::new("a\\n\\tb").collect::<Vec<_>>()', 'vec![Token::Ident("a"), Token::Ident("b")]'),
     T("non_ascii_letter_is_punct", '"éa"', 'Tokenizer::new("éa").collect::<Vec<_>>()', "vec![Token::Punct('é'), Token::Ident(\"a\")]"),
     T("emoji", '"x😀1"', 'Tokenizer::new("x😀1").collect::<Vec<_>>()', "vec![Token::Ident(\"x\"), Token::Punct('😀'), Token::Number(\"1\")]"),
     T("zero_copy", "idents point into the source", "std::ptr::eq(id.as_ptr(), src[4..].as_ptr())", "true",
       setup='let src = String::from("1 + abc");\nlet Some(Token::Ident(id)) = Tokenizer::new(&src).nth(2) else { panic!("no ident") };'),
     T("blank", '"   "', 'Tokenizer::new("   ").count()', "0"),
     T("tie_first", '"ab cd"', 'longest_ident(Tokenizer::new("ab cd"))', 'Some("ab")'),
     T("numbers_are_not_idents", '"12345 ab"', 'longest_ident(Tokenizer::new("12345 ab"))', 'Some("ab")'),
     T("from_a_vec", "a Vec<Token> built by hand", 'longest_ident(vec![Token::Punct(\'x\'), Token::Ident("q")])', 'Some("q")'),
     """
     #[test]
     fn random_vs_brute_force() {
         let mut rng = anneal_prelude::Rng::new(310);
         for _ in 0..300 {
             let n = rng.below(12);
             let src = rng.string(n, "a_1 +é");
             // Reference: classify one char at a time.
             let cs: Vec<(usize, char)> = src.char_indices().collect();
             let mut want: Vec<Token> = Vec::new();
             let mut i = 0;
             while i < cs.len() {
                 let (at, c) = cs[i];
                 let mut j = i + 1;
                 if c == ' ' {
                     i = j;
                     continue;
                 }
                 if c == 'a' || c == '_' {
                     while j < cs.len() && matches!(cs[j].1, 'a' | '_' | '1') {
                         j += 1;
                     }
                 } else if c == '1' {
                     while j < cs.len() && cs[j].1 == '1' {
                         j += 1;
                     }
                 }
                 let end = if j < cs.len() { cs[j].0 } else { src.len() };
                 want.push(match c {
                     'a' | '_' => Token::Ident(&src[at..end]),
                     '1' => Token::Number(&src[at..end]),
                     _ => Token::Punct(c),
                 });
                 i = j;
             }
             let longest = want.iter().filter_map(|t| if let Token::Ident(s) = t { Some(*s) } else { None }).fold(None, |best: Option<&str>, s| match best {
                 Some(b) if b.len() >= s.len() => Some(b),
                 _ => Some(s),
             });
             check!(format!("src = {src:?}"), Tokenizer::new(&src).collect::<Vec<_>>(), want.clone());
             check!(format!("src = {src:?}: longest_ident"), longest_ident(Tokenizer::new(&src)), longest);
         }
     }
     """],
    [("rust", "`type Item = Token<'a>`: tokens borrow the source, not the tokenizer, so they can outlive it."),
     ("rust", "`s.split_at(len)` gives the token and the rest, both `&'a str`."),
     ("rust", "`longest_ident` only needs `IntoIterator<Item = Token<'a>>`, so it works on a `Tokenizer`, a `Vec`, or anything else.")],
    ("No token allocates. The iterator and the tokens are separate borrows of the source, which is why `longest_ident` can return data after the tokenizer is dropped.", "O(n)", "O(1)"),
    "Add byte offsets to each token for error messages. How does the type change?",
    ["Iterator items that borrow the input: `type Item = Token<'a>`.", "Generic functions over `IntoIterator<Item = Token<'a>>`."],
    source="W36", related=("L3", "S2"),
    wrong=dict(
        tie_goes_to_last="""
            #[derive(Debug, PartialEq, Clone, Copy)]
            pub enum Token<'a> {
                Ident(&'a str),
                Number(&'a str),
                Punct(char),
            }

            pub struct Tokenizer<'a> {
                rest: &'a str,
            }

            impl<'a> Tokenizer<'a> {
                pub fn new(src: &'a str) -> Self {
                    Tokenizer { rest: src }
                }
            }

            fn is_ident_start(c: char) -> bool {
                c.is_ascii_alphabetic() || c == '_'
            }

            impl<'a> Iterator for Tokenizer<'a> {
                type Item = Token<'a>;

                fn next(&mut self) -> Option<Token<'a>> {
                    let s = self.rest.trim_start();
                    let c = s.chars().next()?;
                    let len = if is_ident_start(c) {
                        s.find(|c: char| !(c.is_ascii_alphanumeric() || c == '_')).unwrap_or(s.len())
                    } else if c.is_ascii_digit() {
                        s.find(|c: char| !c.is_ascii_digit()).unwrap_or(s.len())
                    } else {
                        c.len_utf8()
                    };
                    let (text, rest) = s.split_at(len);
                    self.rest = rest;
                    Some(if is_ident_start(c) {
                        Token::Ident(text)
                    } else if c.is_ascii_digit() {
                        Token::Number(text)
                    } else {
                        Token::Punct(c)
                    })
                }
            }

            pub fn longest_ident<'a, I: IntoIterator<Item = Token<'a>>>(tokens: I) -> Option<&'a str> {
                tokens
                    .into_iter()
                    .filter_map(|t| match t {
                        Token::Ident(s) => Some(s),
                        _ => None,
                    })
                    .max_by_key(|s| s.len())
            }
        """,
        unicode_letters_are_idents="""
            #[derive(Debug, PartialEq, Clone, Copy)]
            pub enum Token<'a> {
                Ident(&'a str),
                Number(&'a str),
                Punct(char),
            }

            pub struct Tokenizer<'a> {
                rest: &'a str,
            }

            impl<'a> Tokenizer<'a> {
                pub fn new(src: &'a str) -> Self {
                    Tokenizer { rest: src }
                }
            }

            fn is_ident_start(c: char) -> bool {
                c.is_alphabetic() || c == '_'
            }

            impl<'a> Iterator for Tokenizer<'a> {
                type Item = Token<'a>;

                fn next(&mut self) -> Option<Token<'a>> {
                    let s = self.rest.trim_start();
                    let c = s.chars().next()?;
                    let len = if is_ident_start(c) {
                        s.find(|c: char| !(c.is_alphanumeric() || c == '_')).unwrap_or(s.len())
                    } else if c.is_ascii_digit() {
                        s.find(|c: char| !c.is_ascii_digit()).unwrap_or(s.len())
                    } else {
                        c.len_utf8()
                    };
                    let (text, rest) = s.split_at(len);
                    self.rest = rest;
                    Some(if is_ident_start(c) {
                        Token::Ident(text)
                    } else if c.is_ascii_digit() {
                        Token::Number(text)
                    } else {
                        Token::Punct(c)
                    })
                }
            }

            pub fn longest_ident<'a, I: IntoIterator<Item = Token<'a>>>(tokens: I) -> Option<&'a str> {
                tokens
                    .into_iter()
                    .filter_map(|t| match t {
                        Token::Ident(s) => Some(s),
                        _ => None,
                    })
                    .reduce(|best, s| if s.len() > best.len() { s } else { best })
            }
        """,
        byte_punct="""
            #[derive(Debug, PartialEq, Clone, Copy)]
            pub enum Token<'a> {
                Ident(&'a str),
                Number(&'a str),
                Punct(char),
            }

            pub struct Tokenizer<'a> {
                rest: &'a str,
            }

            impl<'a> Tokenizer<'a> {
                pub fn new(src: &'a str) -> Self {
                    Tokenizer { rest: src }
                }
            }

            impl<'a> Iterator for Tokenizer<'a> {
                type Item = Token<'a>;

                fn next(&mut self) -> Option<Token<'a>> {
                    let s = self.rest.trim_start();
                    let b = *s.as_bytes().first()?;
                    let is_ident = |b: u8| b.is_ascii_alphanumeric() || b == b'_';
                    let len = if b.is_ascii_alphabetic() || b == b'_' {
                        s.bytes().take_while(|&b| is_ident(b)).count()
                    } else if b.is_ascii_digit() {
                        s.bytes().take_while(u8::is_ascii_digit).count()
                    } else {
                        s.chars().next().map_or(1, char::len_utf8)
                    };
                    let (text, rest) = s.split_at(len);
                    self.rest = rest;
                    Some(if b.is_ascii_alphabetic() || b == b'_' {
                        Token::Ident(text)
                    } else if b.is_ascii_digit() {
                        Token::Number(text)
                    } else {
                        Token::Punct(b as char)
                    })
                }
            }

            pub fn longest_ident<'a, I: IntoIterator<Item = Token<'a>>>(tokens: I) -> Option<&'a str> {
                tokens
                    .into_iter()
                    .filter_map(|t| match t {
                        Token::Ident(s) => Some(s),
                        _ => None,
                    })
                    .reduce(|best, s| if s.len() > best.len() { s } else { best })
            }
        """,
    ),
))

P.append(write(
    "binary-frame-parser", "Zero-copy binary frame parser", "medium", "two-lifetimes", ["&[u8]", "split_first_chunk", "big-endian"],
    """
        A frame is: magic `b"AN"` (2 bytes), a kind byte, a big-endian `u16` length, then that many payload
        bytes.

        - `parse_frame` returns the frame and the bytes after it, or `None` for a bad or truncated frame.
          The payload must borrow from `buf`.
        - `parse_all` parses back-to-back frames until the buffer is empty; `None` if anything is left that
          isn't a whole frame.
    """,
    """
    #[derive(Debug, PartialEq)]
    pub struct Frame<'a> {
        pub kind: u8,
        pub payload: &'a [u8],
    }

    pub const MAGIC: &[u8; 2] = b"AN";

    pub fn parse_frame(buf: &[u8]) -> Option<(Frame<'_>, &[u8])> {
        todo!()
    }

    pub fn parse_all(buf: &[u8]) -> Option<Vec<Frame<'_>>> {
        todo!()
    }
    """,
    """
    #[derive(Debug, PartialEq)]
    pub struct Frame<'a> {
        pub kind: u8,
        pub payload: &'a [u8],
    }

    pub const MAGIC: &[u8; 2] = b"AN";

    pub fn parse_frame(buf: &[u8]) -> Option<(Frame<'_>, &[u8])> {
        let rest = buf.strip_prefix(MAGIC)?;
        let (&kind, rest) = rest.split_first()?;
        let (len, rest) = rest.split_first_chunk::<2>()?;
        let len = usize::from(u16::from_be_bytes(*len));
        if rest.len() < len {
            return None;
        }
        let (payload, rest) = rest.split_at(len);
        Some((Frame { kind, payload }, rest))
    }

    pub fn parse_all(mut buf: &[u8]) -> Option<Vec<Frame<'_>>> {
        let mut frames = Vec::new();
        while !buf.is_empty() {
            let (frame, rest) = parse_frame(buf)?;
            frames.push(frame);
            buf = rest;
        }
        Some(frames)
    }
    """,
    [T("one_frame", 'AN, kind 1, len 5, "hello", then "tail"', "parse_frame(&buf)", 'Some((Frame { kind: 1, payload: &b"hello"[..] }, &b"tail"[..]))',
       setup='let mut buf = b"AN\\x01".to_vec();\nbuf.extend_from_slice(&5u16.to_be_bytes());\nbuf.extend_from_slice(b"hellotail");'),
     T("bad_magic", 'b"XX\\x01\\x00\\x00"', 'parse_frame(b"XX\\x01\\x00\\x00")', "None"),
     T("empty_buffer", 'b""', "(parse_frame(b\"\"), parse_all(b\"\"))", "(None, Some(vec![]))"),
     T("truncated_payload", "length 5, only 3 bytes", 'parse_frame(b"AN\\x01\\x00\\x05abc")', "None"),
     T("two_frames", "two frames back to back", 'parse_all(b"AN\\x01\\x00\\x01aAN\\x02\\x00\\x00").map(|v| v.iter().map(|f| f.kind).collect::<Vec<_>>())', "Some(vec![1, 2])")],
    [T("truncated_payload", "length 5, only 3 bytes", 'parse_frame(b"AN\\x01\\x00\\x05abc")', "None"),
     T("length_is_big_endian", "length bytes [0x01, 0x00] = 256, 256 payload bytes", "parse_frame(&buf).map(|(f, rest)| (f.payload.len(), rest.len()))", "Some((256, 0))",
       setup='let mut buf = b"AN\\x09\\x01\\x00".to_vec();\nbuf.resize(5 + 256, 7);'),
     T("exact_fit", "length 2, exactly 2 bytes", 'parse_frame(b"AN\\x03\\x00\\x02hi")', 'Some((Frame { kind: 3, payload: &b"hi"[..] }, &[][..]))'),
     T("kind_255", "kind 0xFF", 'parse_frame(b"AN\\xff\\x00\\x00").map(|(f, _)| f.kind)', "Some(255)"),
     T("only_magic", 'b"AN"', 'parse_frame(b"AN")', "None"),
     T("second_frame_truncated", "a whole frame, then a frame missing its payload", 'parse_all(b"AN\\x01\\x00\\x00AN\\x02\\x00\\x03ab")', "None"),
     T("magic_case", 'b"an\\x01\\x00\\x00"', 'parse_frame(b"an\\x01\\x00\\x00")', "None"),
     T("max_length_frame", "length 0xFFFF with 65535 bytes", "parse_all(&buf).map(|v| v[0].payload.len())", "Some(65_535)",
       setup='let mut buf = b"AN\\x01\\xff\\xff".to_vec();\nbuf.resize(5 + 65_535, 0);'),
     """
     #[test]
     fn random_vs_brute_force() {
         let mut rng = anneal_prelude::Rng::new(311);
         for _ in 0..300 {
             let count = rng.below(4);
             let mut buf = Vec::new();
             let mut want: Vec<(u8, Vec<u8>)> = Vec::new();
             for _ in 0..count {
                 let kind = rng.int(0, 255) as u8;
                 let len = rng.below(300);
                 let payload: Vec<u8> = rng.vec(len, 0, 255);
                 buf.extend_from_slice(b"AN");
                 buf.push(kind);
                 buf.push((len >> 8) as u8);
                 buf.push(len as u8);
                 buf.extend_from_slice(&payload);
                 want.push((kind, payload));
             }
             let got = parse_all(&buf).map(|v| v.iter().map(|f| (f.kind, f.payload.to_vec())).collect::<Vec<_>>());
             check!(format!("{count} random frames"), got, Some(want.clone()));
             // Cutting any whole frame short makes the buffer invalid.
             if !buf.is_empty() {
                 let cut = rng.below(buf.len());
                 let whole = want.iter().scan(0, |end, (_, p)| { *end += 5 + p.len(); Some(*end) }).any(|end| end == cut);
                 let want_cut = if whole || cut == 0 { Some(()) } else { None };
                 check!(format!("{count} random frames cut to {cut} bytes"), parse_all(&buf[..cut]).map(|_| ()), want_cut);
             }
         }
     }

     #[test]
     fn scale_100k_frames() {
         let mut buf = Vec::new();
         for i in 0..100_000u32 {
             buf.extend_from_slice(b"AN");
             buf.push((i % 256) as u8);
             buf.extend_from_slice(&1u16.to_be_bytes());
             buf.push((i % 7) as u8);
         }
         let frames = parse_all(&buf).unwrap();
         check!("100000 one-byte frames", (frames.len(), frames[99_999].kind, frames[99_999].payload), (100_000, (99_999 % 256) as u8, &[(99_999 % 7) as u8][..]));
     }
     """,
     T("short_header", 'b"AN\\x01\\x00"', 'parse_frame(b"AN\\x01\\x00")', "None"),
     T("empty_payload", "length 0", 'parse_frame(b"AN\\x07\\x00\\x00")', 'Some((Frame { kind: 7, payload: &[][..] }, &[][..]))'),
     T("zero_copy", "payload points into buf", "same", "true",
       setup='let buf = b"AN\\x01\\x00\\x02hi".to_vec();\nlet (f, _) = parse_frame(&buf).unwrap();\nlet same = std::ptr::eq(f.payload.as_ptr(), buf[5..].as_ptr());'),
     T("two_frames", "two frames back to back", 'parse_all(b"AN\\x01\\x00\\x01aAN\\x02\\x00\\x00").map(|v| v.iter().map(|f| f.kind).collect::<Vec<_>>())', "Some(vec![1, 2])"),
     T("trailing_junk", "one frame then 1 junk byte", 'parse_all(b"AN\\x01\\x00\\x00!")', "None"),
     T("large_length", "length 0xFFFF with 3 bytes", 'parse_frame(b"AN\\x01\\xff\\xffabc")', "None")],
    [("rust", "`strip_prefix`, `split_first` and `split_first_chunk::<2>()` each return `Option`, so every bounds check is a `?`."),
     ("rust", "`u16::from_be_bytes(*len)` reads the network-order length from a `&[u8; 2]`."),
     ("edge case", "Never trust the length field: check that the buffer really has that many bytes.")],
    ("Every step slices the input, so nothing is copied and nothing can index out of bounds. Returning the rest makes `parse_all` a simple loop.", "O(1) per frame", "O(frames)"),
    "How would you parse frames from a socket, where a frame can arrive split across reads?",
    ["Zero-copy parsing: return slices of the input.", "Bounds checks as `Option` combinators instead of indexing."],
    source="W40", related=("L3", "S3"),
    wrong=dict(
        little_endian_length="""
            #[derive(Debug, PartialEq)]
            pub struct Frame<'a> {
                pub kind: u8,
                pub payload: &'a [u8],
            }

            pub const MAGIC: &[u8; 2] = b"AN";

            pub fn parse_frame(buf: &[u8]) -> Option<(Frame<'_>, &[u8])> {
                let rest = buf.strip_prefix(MAGIC)?;
                let (&kind, rest) = rest.split_first()?;
                let (len, rest) = rest.split_first_chunk::<2>()?;
                let len = usize::from(u16::from_le_bytes(*len));
                if rest.len() < len {
                    return None;
                }
                let (payload, rest) = rest.split_at(len);
                Some((Frame { kind, payload }, rest))
            }

            pub fn parse_all(mut buf: &[u8]) -> Option<Vec<Frame<'_>>> {
                let mut frames = Vec::new();
                while !buf.is_empty() {
                    let (frame, rest) = parse_frame(buf)?;
                    frames.push(frame);
                    buf = rest;
                }
                Some(frames)
            }
        """,
        rejects_exact_fit="""
            #[derive(Debug, PartialEq)]
            pub struct Frame<'a> {
                pub kind: u8,
                pub payload: &'a [u8],
            }

            pub const MAGIC: &[u8; 2] = b"AN";

            pub fn parse_frame(buf: &[u8]) -> Option<(Frame<'_>, &[u8])> {
                let rest = buf.strip_prefix(MAGIC)?;
                let (&kind, rest) = rest.split_first()?;
                let (len, rest) = rest.split_first_chunk::<2>()?;
                let len = usize::from(u16::from_be_bytes(*len));
                if rest.len() <= len && len > 0 {
                    return None;
                }
                let (payload, rest) = rest.split_at(len);
                Some((Frame { kind, payload }, rest))
            }

            pub fn parse_all(mut buf: &[u8]) -> Option<Vec<Frame<'_>>> {
                let mut frames = Vec::new();
                while !buf.is_empty() {
                    let (frame, rest) = parse_frame(buf)?;
                    frames.push(frame);
                    buf = rest;
                }
                Some(frames)
            }
        """,
        keeps_frames_before_junk="""
            #[derive(Debug, PartialEq)]
            pub struct Frame<'a> {
                pub kind: u8,
                pub payload: &'a [u8],
            }

            pub const MAGIC: &[u8; 2] = b"AN";

            pub fn parse_frame(buf: &[u8]) -> Option<(Frame<'_>, &[u8])> {
                let rest = buf.strip_prefix(MAGIC)?;
                let (&kind, rest) = rest.split_first()?;
                let (len, rest) = rest.split_first_chunk::<2>()?;
                let len = usize::from(u16::from_be_bytes(*len));
                if rest.len() < len {
                    return None;
                }
                let (payload, rest) = rest.split_at(len);
                Some((Frame { kind, payload }, rest))
            }

            pub fn parse_all(mut buf: &[u8]) -> Option<Vec<Frame<'_>>> {
                let mut frames = Vec::new();
                while let Some((frame, rest)) = parse_frame(buf) {
                    frames.push(frame);
                    buf = rest;
                }
                Some(frames)
            }
        """,
    ),
))

READER_STARTER = r"""
pub struct Reader<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        Reader { data, pos: 0 }
    }

    /// The next `n` bytes, or None (reading nothing) if fewer remain.
    pub fn take(&'a mut self, n: usize) -> Option<&'a [u8]> {
        let chunk = self.data.get(self.pos..self.pos + n)?;
        self.pos += n;
        Some(chunk)
    }

    /// The next byte, without reading it.
    pub fn peek(&'a self) -> Option<u8> {
        self.data.get(self.pos).copied()
    }

    /// A big-endian `u16`, or None (reading nothing) if fewer than 2 bytes remain.
    pub fn u16(&'a mut self) -> Option<u16> {
        let b = self.take(2)?;
        Some(u16::from_be_bytes([b[0], b[1]]))
    }

    pub fn remaining(&self) -> usize {
        self.data.len() - self.pos
    }
}

/// Reads records, each a `u16` length then that many bytes, until the reader is empty. None if a record is
/// cut short (the reader is then left just after the last whole record).
pub fn records<'a>(r: &'a mut Reader<'a>) -> Option<Vec<&'a [u8]>> {
    let mut out = Vec::new();
    while r.remaining() > 0 {
        let start = r.pos;
        let Some(len) = r.u16() else {
            r.pos = start;
            return None;
        };
        let Some(body) = r.take(usize::from(len)) else {
            r.pos = start;
            return None;
        };
        out.push(body);
    }
    Some(out)
}
"""

READER_SOLUTION = READER_STARTER
for _old, _new in [
    ("    pub fn take(&'a mut self, n: usize) -> Option<&'a [u8]> {", "    pub fn take(&mut self, n: usize) -> Option<&'a [u8]> {"),
    ("    pub fn peek(&'a self) -> Option<u8> {", "    pub fn peek(&self) -> Option<u8> {"),
    ("    pub fn u16(&'a mut self) -> Option<u16> {", "    pub fn u16(&mut self) -> Option<u16> {"),
    ("pub fn records<'a>(r: &'a mut Reader<'a>) -> Option<Vec<&'a [u8]>> {", "pub fn records<'a>(r: &mut Reader<'a>) -> Option<Vec<&'a [u8]>> {"),
]:
    READER_SOLUTION = sub(READER_SOLUTION, _old, _new)

P.append(fix(
    "fix-a-mut-self-borrows-forever", "Fix: &'a mut self borrows forever", "medium", "two-lifetimes", ["&'a mut self", "E0499", "E0502", "&'a mut T<'a>"],
    """
        `Reader`'s methods compile, but only one call can ever be made on a reader: after the first `take`,
        `u16` or even `peek`, it stays borrowed for as long as its data. That's why `records`, which reads a
        reader several times, doesn't compile, and its own signature does the same to its caller. Fix the
        signatures so readers can be used normally; results must still borrow the data, not the reader.
    """,
    READER_STARTER,
    READER_SOLUTION,
    [T("take_twice", "data [1, 2, 3, 4]: take 1, take 2, remaining", "(r.take(1), r.take(2), r.remaining())", "(Some(&[1u8][..]), Some(&[2u8, 3][..]), 1)", setup="let data = [1u8, 2, 3, 4];\nlet mut r = Reader::new(&data);"),
     T("peek_then_take", "data [7, 8]: peek, take 1, peek", "(r.peek(), r.take(1).map(|b| b[0]), r.peek())", "(Some(7), Some(7), Some(8))", setup="let data = [7u8, 8];\nlet mut r = Reader::new(&data);"),
     T("u16_big_endian", "data [1, 2, 0xff]: u16, then u16", "(r.u16(), r.u16(), r.remaining())", "(Some(258), None, 1)", setup="let data = [1u8, 2, 0xff];\nlet mut r = Reader::new(&data);"),
     T("records_then_keep_reading", "records of [0, 2, 'h', 'i', 0, 0]; then remaining", "(recs, r.remaining())", '(Some(vec![&b"hi"[..], &b""[..]]), 0)', setup='let data = [0u8, 2, b\'h\', b\'i\', 0, 0];\nlet mut r = Reader::new(&data);\nlet recs = records(&mut r);'),
     T("chunks_outlive_the_reader", "take 2 from a reader that's then dropped", "chunk", "Some(&[9u8, 9][..])", setup="let data = [9u8, 9, 9];\nlet chunk = Reader::new(&data).take(2);")],
    [T("take_too_many", "data [1]: take 2, then take 1", "(r.take(2), r.take(1))", "(None, Some(&[1u8][..]))", setup="let data = [1u8];\nlet mut r = Reader::new(&data);"),
     T("take_zero", "data []: take 0", "r.take(0)", "Some(&[][..])", setup="let data: [u8; 0] = [];\nlet mut r = Reader::new(&data);"),
     T("peek_empty", "data []: peek", "Reader::new(&[]).peek()", "None"),
     T("records_cut_short", "records of [0, 1, 'a', 0, 5, 'b']", "(recs, r.remaining(), r.peek())", "(None, 3, Some(0))", setup='let data = [0u8, 1, b\'a\', 0, 5, b\'b\'];\nlet mut r = Reader::new(&data);\nlet recs = records(&mut r);'),
     T("records_half_length", "records of [0, 1, 'a', 7]", "(recs, r.remaining())", "(None, 1)", setup="let data = [0u8, 1, b'a', 7];\nlet mut r = Reader::new(&data);\nlet recs = records(&mut r);"),
     T("records_empty", "records of []", "records(&mut Reader::new(&[]))", "Some(vec![])"),
     T("u16_max", "data [0xff, 0xff]", "Reader::new(&[0xff, 0xff]).u16()", "Some(u16::MAX)"),
     T("results_point_into_data", "take returns a slice of the data", "Reader::new(&data).take(1).unwrap().as_ptr() == data.as_ptr()", "true", setup="let data = [5u8];"),
     r"""
     #[test]
     fn random_vs_model() {
         let mut rng = anneal_prelude::Rng::new(6309);
         for _ in 0..300 {
             let n = rng.below(10);
             let data: Vec<u8> = rng.vec(n, 0, 3);
             let mut r = Reader::new(&data);
             let mut pos = 0;
             let mut ops = Vec::new();
             for _ in 0..5 {
                 match rng.below(3) {
                     0 => {
                         let k = rng.below(4);
                         let want = data.get(pos..pos + k);
                         if want.is_some() {
                             pos += k;
                         }
                         ops.push(format!("take {k}"));
                         check!(format!("{data:?}: {}", ops.join(", ")), r.take(k), want);
                     }
                     1 => {
                         ops.push("peek".to_string());
                         check!(format!("{data:?}: {}", ops.join(", ")), r.peek(), data.get(pos).copied());
                     }
                     _ => {
                         let want = data.get(pos..pos + 2).map(|b| u16::from_be_bytes([b[0], b[1]]));
                         if want.is_some() {
                             pos += 2;
                         }
                         ops.push("u16".to_string());
                         check!(format!("{data:?}: {}", ops.join(", ")), r.u16(), want);
                     }
                 }
             }
             check!(format!("{data:?}: {}; remaining", ops.join(", ")), r.remaining(), data.len() - pos);
         }
     }

     #[test]
     fn many_records() {
         let mut data = Vec::new();
         for i in 0..50_000u32 {
             data.extend_from_slice(&[0, 2, (i % 256) as u8, 1]);
         }
         let mut r = Reader::new(&data);
         let recs = records(&mut r).unwrap();
         check!("50000 two-byte records", (recs.len(), recs[49_999], r.remaining()), (50_000, &[(49_999 % 256) as u8, 1][..], 0));
     }
     """],
    [("rust", "In `impl<'a> Reader<'a>`, `&'a mut self` means \"borrow this reader mutably for as long as its data lives\". That's as long as the reader can exist, so nothing else can ever touch it. The same goes for `&'a self` once a mutable call is needed later."),
     ("rust", "The result's lifetime and the receiver's lifetime are different things. Results borrow the data: `-> Option<&'a [u8]>`. The receiver is a short borrow: plain `&mut self`."),
     ("rust", "`&'a mut Reader<'a>` in `records` is the same mistake from the outside: the caller's reader is borrowed for all of `'a`.")],
    ("""`&'a mut self` on a type with lifetime `'a` is the classic over-constraint: it says the mutable borrow of the reader lasts as long as the data it points at, and since the reader can't outlive its data, that's the reader's whole remaining life. The first call compiles; every later use is E0499 (or E0502 after `&'a self`). And `&'a mut T<'a>` is worse than it looks because `&mut T` is invariant in `T`: the compiler can't shrink the inner `'a` to make the borrow shorter. The fix separates the two: the receiver is an ordinary short borrow (`&mut self`), and the output names `'a`, because it points into the data. `records` gets the same treatment: `r: &mut Reader<'a>` and `Vec<&'a [u8]>`.

Syntax to remember: `pub fn take(&mut self, n: usize) -> Option<&'a [u8]>` · `pub fn records<'a>(r: &mut Reader<'a>) -> Option<Vec<&'a [u8]>>`.""", "O(1) per read", "O(1)"),
    "Why can the compiler shorten `'a` in `&'a Reader<'a>` to make a shared borrow shorter, but not in `&'a mut Reader<'a>`?",
    ["`&'a mut self` on `T<'a>` borrows the value for its whole life.", "Name the data's lifetime on outputs, not on the receiver.", "Avoid `&'a mut T<'a>`; `&mut T` is invariant in `T`."],
    rules=dict(methods=["clone", "to_vec", "to_owned", "leak"], lines=4),
    wrong=dict(
        u16_little_endian=sub(READER_SOLUTION, "Some(u16::from_be_bytes([b[0], b[1]]))", "Some(u16::from_le_bytes([b[0], b[1]]))"),
        take_moves_on_failure=sub(READER_SOLUTION, "        let chunk = self.data.get(self.pos..self.pos + n)?;\n        self.pos += n;\n", "        let end = (self.pos + n).min(self.data.len());\n        let chunk = self.data.get(self.pos..self.pos + n);\n        self.pos = end;\n        let chunk = chunk?;\n"),
        records_no_rewind=sub(READER_SOLUTION, "        let Some(body) = r.take(usize::from(len)) else {\n            r.pos = start;\n            return None;\n        };", "        let Some(body) = r.take(usize::from(len)) else {\n            return None;\n        };"),
    ),
))

# ---------------------------------------------------------------- 'static and T: 'a (medium)

EXT_HEAD = r"""
use std::any::{Any, TypeId};
use std::collections::HashMap;
use std::fmt::Debug;

/// Holds at most one value of each type, like `http::Extensions`. Values of any type that owns its data (or
/// borrows only `'static` data) can go in.
pub struct Extensions {
    map: HashMap<TypeId, Box<dyn Any>>,
}
"""

EXT_IMPL = r"""
impl Extensions {
    pub fn new() -> Self {
        Extensions { map: HashMap::new() }
    }

    pub fn insert<T: Any>(&mut self, value: T) -> Option<T> {
        let old = self.map.insert(TypeId::of::<T>(), Box::new(value))?;
        Some(*old.downcast::<T>().expect("stored under its own TypeId"))
    }

    pub fn get<T: Any>(&self) -> Option<&T> {
        self.map.get(&TypeId::of::<T>())?.downcast_ref()
    }

    pub fn get_mut<T: Any>(&mut self) -> Option<&mut T> {
        self.map.get_mut(&TypeId::of::<T>())?.downcast_mut()
    }

    pub fn remove<T: Any>(&mut self) -> Option<T> {
        let b = self.map.remove(&TypeId::of::<T>())?;
        Some(*b.downcast::<T>().expect("stored under its own TypeId"))
    }

    pub fn len(&self) -> usize {
        self.map.len()
    }
}
"""

EXT_TAIL_STARTER = r"""
/// Keeps `value` in `log` for debug output later.
pub fn remember<T: Debug>(log: &mut Vec<Box<dyn Debug>>, value: T) {
    log.push(Box::new(value));
}

/// A name made at run time that lives for the rest of the program. Each call leaks its String on purpose.
pub fn intern_forever(name: String) -> &'static str {
    Box::leak(name.into_boxed_str())
}
"""

EXT_TAIL_SOLUTION = sub(EXT_TAIL_STARTER, "pub fn remember<T: Debug>(", "pub fn remember<T: Debug + 'static>(")
EXT_SOLUTION = EXT_HEAD + EXT_IMPL + EXT_TAIL_SOLUTION
EX = "let mut ext = Extensions::new();"

P.append(fix(
    "static-bound-vs-static-ref", "'static bound vs &'static: a type map", "medium", "static-bounds", ["T: 'static", "Any", "TypeId", "downcast", "E0310"],
    """
        Write `impl Extensions`: a map holding at most one value per type, keyed by `TypeId`.

        - `new()`, and `len()`: how many values it holds.
        - `insert(value)`: stores it, replacing and returning the previous value of the same type, by value.
        - `get::<T>()` and `get_mut::<T>()`: the value of type `T`, borrowed.
        - `remove::<T>()`: takes the value of type `T` out, by value.

        Then fix `remember`, which doesn't compile. `intern_forever` is correct: read it as the other half
        of the lesson.
    """,
    EXT_HEAD + "\n// TODO: impl Extensions.\n" + EXT_TAIL_STARTER,
    EXT_SOLUTION,
    [T("insert_and_get", "insert 5u32 and a String", '(ext.get::<u32>(), ext.get::<String>().map(|s| s.as_str()), ext.get::<i64>(), ext.len())', '(Some(&5), Some("runtime"), None, 2)', setup=EX + '\next.insert(5u32);\next.insert(format!("run{}", "time"));'),
     T("insert_returns_previous", "insert 1u8, then 2u8", "(ext.insert(1u8), ext.insert(2u8), ext.get::<u8>())", "(None, Some(1), Some(&2))", setup=EX),
     T("get_mut_edits", "insert vec![1]; get_mut push 2", "{ ext.get_mut::<Vec<i32>>().unwrap().push(2); ext.get::<Vec<i32>>().cloned() }", "Some(vec![1, 2])", setup=EX + "\next.insert(vec![1]);"),
     T("remove_by_value", "insert a String; remove it", '(ext.remove::<String>(), ext.remove::<String>(), ext.len())', '(Some("x".to_string()), None, 0)', setup=EX + '\next.insert("x".to_string());'),
     T("remember_owned_and_static", "remember a runtime String and a &'static str", 'format!("{log:?}")', '"[\\"made\\", \\"literal\\"]"', setup='let mut log: Vec<Box<dyn std::fmt::Debug>> = Vec::new();\nremember(&mut log, String::from("made"));\nremember(&mut log, "literal");'),
     T("intern_forever_is_static", "intern_forever of a runtime String, stored in an Extensions", "ext.get::<&'static str>().copied()", 'Some("dyn")', setup=EX + '\next.insert(intern_forever(String::from("dyn")));')],
    [T("empty", "new map", "(ext.len(), ext.get::<u32>().is_none(), ext.remove::<u32>())", "(0, true, None)", setup=EX),
     T("distinct_integer_types", "insert 1u32, 1u64, 1i32", "ext.len()", "3", setup=EX + "\next.insert(1u32);\next.insert(1u64);\next.insert(1i32);"),
     T("str_vs_string", "insert \"a\" (&'static str) and \"b\" (String)", '(ext.get::<&str>().copied(), ext.get::<String>().cloned())', '(Some("a"), Some("b".to_string()))', setup=EX + '\next.insert("a");\next.insert("b".to_string());'),
     T("replace_keeps_len", "insert 1u32 three times", "{ ext.insert(1u32); ext.insert(2u32); (ext.insert(3u32), ext.len()) }", "(Some(2), 1)", setup=EX),
     T("unit_type", "insert ()", "(ext.insert(()), ext.get::<()>().is_some())", "(None, true)", setup=EX),
     T("struct_value", "insert a local struct type", "ext.get::<P>().map(|p| p.0)", "Some(7)", setup="#[derive(Debug)]\nstruct P(u8);\n" + EX + "\next.insert(P(7));"),
     T("get_mut_missing", "get_mut::<u8> on an empty map", "ext.get_mut::<u8>().is_none()", "true", setup=EX),
     T("remember_order", "remember 1, 2.5, 'c'", 'format!("{log:?}")', '"[1, 2.5, \'c\']"', setup="let mut log: Vec<Box<dyn std::fmt::Debug>> = Vec::new();\nremember(&mut log, 1);\nremember(&mut log, 2.5);\nremember(&mut log, 'c');"),
     r"""
     #[test]
     fn random_vs_model() {
         let mut rng = anneal_prelude::Rng::new(6310);
         for _ in 0..300 {
             let mut ext = Extensions::new();
             let (mut a, mut b): (Option<u32>, Option<String>) = (None, None);
             let mut ops = Vec::new();
             for _ in 0..8 {
                 let x = rng.below(100) as u32;
                 match rng.below(4) {
                     0 => {
                         ops.push(format!("insert {x}u32"));
                         check!(ops.join(", "), ext.insert(x), a.replace(x));
                     }
                     1 => {
                         ops.push(format!("insert \"{x}\""));
                         check!(ops.join(", "), ext.insert(x.to_string()), b.replace(x.to_string()));
                     }
                     2 => {
                         ops.push("remove::<u32>".to_string());
                         check!(ops.join(", "), ext.remove::<u32>(), a.take());
                     }
                     _ => {
                         if let Some(v) = ext.get_mut::<String>() {
                             v.push('!');
                         }
                         if let Some(v) = b.as_mut() {
                             v.push('!');
                         }
                         ops.push("get_mut::<String> += !".to_string());
                     }
                 }
             }
             check!(format!("{}; state", ops.join(", ")), (ext.get::<u32>().copied(), ext.get::<String>().cloned(), ext.len()), (a, b.clone(), a.is_some() as usize + b.is_some() as usize));
         }
     }
     """],
    [("rust", "`Box<dyn Any>` requires `T: 'static`, and `Any` already implies it: `T: Any` means \"a type with no borrows shorter than `'static`\". A `String` built at run time qualifies; a `&'a str` into a local doesn't."),
     ("rust", "`TypeId::of::<T>()` is the key. `Box<dyn Any>::downcast::<T>()` gives `Result<Box<T>, Box<dyn Any>>` (deref with `*` to move out); `downcast_ref` / `downcast_mut` borrow."),
     ("rust", "`Box<dyn Debug>` means `Box<dyn Debug + 'static>`, so `remember` can only store a `T` with no short borrows (E0310): say `T: 'static`.")],
    ("""`T: 'static` is a bound on a type: the type contains no references shorter than `'static`, so a value of it can be kept as long as you like. It says nothing about how long any particular value lives: a `String` created a moment ago is `'static` in this sense and can be dropped the next line. `&'static T` is different: a reference to something that lives until the program ends (a literal, a `static`, or leaked memory, as in `intern_forever`). Type erasure needs the bound because a `Box<dyn Any>` or `Box<dyn Debug>` (default object lifetime `'static`) may be kept anywhere, so nothing inside may expire. `Any` also implies it, which is why `insert<T: Any>` compiles and `remember<T: Debug>` doesn't.

Syntax to remember: `self.map.insert(TypeId::of::<T>(), Box::new(value))` · `self.map.get(&TypeId::of::<T>())?.downcast_ref::<T>()` · `*b.downcast::<T>().ok()?` · `fn remember<T: Debug + 'static>(..)` · `Box::leak(s.into_boxed_str())`.""", "O(1) expected", "O(n) values"),
    "`intern_forever` returns a `&'static str` from a runtime `String`. What does that cost, and when is it acceptable?",
    ["`T: 'static`: no borrows shorter than `'static`; not \"lives forever\".", "`dyn Trait` in a `Box` defaults to `+ 'static`.", "`Any`, `TypeId` and `downcast` for type maps."],
    related=("L3", "L4"),
    wrong=dict(
        insert_returns_new=EXT_HEAD + sub(EXT_IMPL, "        let old = self.map.insert(TypeId::of::<T>(), Box::new(value))?;\n        Some(*old.downcast::<T>().expect(\"stored under its own TypeId\"))", "        self.map.insert(TypeId::of::<T>(), Box::new(value));\n        None") + EXT_TAIL_SOLUTION,
        remove_leaves_it=EXT_HEAD + sub(EXT_IMPL, "        let b = self.map.remove(&TypeId::of::<T>())?;\n        Some(*b.downcast::<T>().expect(\"stored under its own TypeId\"))", "        let old = self.map.insert(TypeId::of::<T>(), Box::new(()))?;\n        old.downcast::<T>().ok().map(|b| *b)") + EXT_TAIL_SOLUTION,
    ),
))

CFG_HEAD = r"""
use std::error::Error;
use std::fmt;
use std::num::ParseIntError;

/// A problem on one line of a config: no '=' (no source), or a value that isn't a number (the parse error
/// is the source).
#[derive(Debug)]
pub struct ConfigError {
    pub line: usize,
    pub source: Option<ParseIntError>,
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.source {
            None => write!(f, "line {}: expected key=value", self.line),
            Some(_) => write!(f, "line {}: bad number", self.line),
        }
    }
}
"""

CFG_DOCS = dict(
    err="impl Error for ConfigError {",
    sum="\n/// Sums the values of `key=value` lines (1-based line numbers; blank lines skipped). Any problem is a\n/// `ConfigError`.\npub fn sum_config(text: &str) -> Result<i64, Box<dyn Error + Send + Sync>> {\n",
    bad="\n/// The line number, when `err` is a `ConfigError`.\npub fn bad_line(err: &(dyn Error + 'static)) -> Option<usize> {\n",
    root="\n/// The last error in `err`'s source chain (`err` itself if it has no source).\npub fn root_cause<'e>(err: &'e (dyn Error + 'static)) -> &'e (dyn Error + 'static) {\n",
    chain="\n/// The messages of `err` and each of its sources, outermost first.\npub fn chain(err: &(dyn Error + 'static)) -> Vec<String> {\n",
)

CFG_BODIES = dict(
    err="""
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        self.source.as_ref().map(|e| e as &(dyn Error + 'static))
    }
""",
    sum="""    let mut total = 0;
    for (i, line) in text.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let (_, value) = line.split_once('=').ok_or(ConfigError { line: i + 1, source: None })?;
        total += value.trim().parse::<i64>().map_err(|e| ConfigError { line: i + 1, source: Some(e) })?;
    }
    Ok(total)
""",
    bad="    err.downcast_ref::<ConfigError>().map(|e| e.line)\n",
    root="""    let mut cur = err;
    while let Some(next) = cur.source() {
        cur = next;
    }
    cur
""",
    chain="""    let mut out = Vec::new();
    let mut cur = Some(err);
    while let Some(e) = cur {
        out.push(e.to_string());
        cur = e.source();
    }
    out
""",
)


def cfg_src(b, starter=False):
    out = CFG_HEAD + "\n" + CFG_DOCS["err"] + ("}\n" if starter else b["err"] + "}\n")
    for k in ("sum", "bad", "root", "chain"):
        out += CFG_DOCS[k] + ("    todo!()\n" if starter else b[k]) + "}\n"
    return out


CFG_SOLUTION = cfg_src(CFG_BODIES)
CFG_STARTER = cfg_src(CFG_BODIES, starter=True)

P.append(write(
    "box-dyn-error-downcast", "Box<dyn Error> chains and downcasting", "medium", "static-bounds", ["Box<dyn Error + Send + Sync>", "source()", "downcast_ref", "dyn Error + 'static", "?"],
    """
        `sum_config` sums the values of `key=value` lines and reports problems as a `ConfigError` with its
        line number: no `=`, or a value that doesn't parse, in which case the `ParseIntError` is the error's
        *source*. Make `ConfigError` report that source (it doesn't yet), then write the functions that walk
        an error chain: `bad_line`, `root_cause` and `chain`.
    """,
    CFG_STARTER,
    CFG_SOLUTION,
    [T("sums", "\"a=1\\n\\nb = 2\\n\"", 'sum_config("a=1\\n\\nb = 2\\n").ok()', "Some(3)"),
     T("missing_equals", "\"a=1\\nnope\" → line 2", 'sum_config("a=1\\nnope").map_err(|e| (bad_line(&*e), e.to_string(), e.source().is_none()))', 'Err((Some(2), "line 2: expected key=value".to_string(), true))'),
     T("chain_of_a_bad_number", "\"x=oops\": the error chain", 'chain(&*sum_config("x=oops").unwrap_err())', 'vec!["line 1: bad number".to_string(), "invalid digit found in string".to_string()]'),
     T("root_cause_is_the_parse_error", "\"x=\": root cause", 'root_cause(&*sum_config("x=").unwrap_err()).downcast_ref::<std::num::ParseIntError>().is_some()', "true"),
     T("bad_line_on_other_errors", "bad_line of a plain ParseIntError", 'bad_line(&"z".parse::<i32>().unwrap_err())', "None")],
    [T("empty", "\"\"", 'sum_config("").ok()', "Some(0)"),
     T("negative", "\"a=-5\\nb=2\"", 'sum_config("a=-5\\nb=2").ok()', "Some(-3)"),
     T("first_error_wins", "\"a=x\\nnope\"", 'sum_config("a=x\\nnope").map_err(|e| bad_line(&*e))', "Err(Some(1))"),
     T("blank_lines_count", "\"\\n \\nq\"", 'sum_config("\\n \\nq").map_err(|e| bad_line(&*e))', "Err(Some(3))"),
     T("overflow_is_a_source", "\"a=99999999999999999999\"", 'sum_config("a=99999999999999999999").map_err(|e| chain(&*e).len())', "Err(2)"),
     T("root_of_a_leaf", "root_cause of a ConfigError without a source", 'root_cause(&ConfigError { line: 4, source: None }).to_string()', '"line 4: expected key=value".to_string()'),
     T("chain_without_source", "chain of a missing '='", 'chain(&*sum_config("k").unwrap_err())', 'vec!["line 1: expected key=value".to_string()]'),
     T("error_is_send_sync", "the boxed error can cross threads", 'std::thread::spawn(move || e.to_string()).join().unwrap()', '"line 1: bad number".to_string()', setup='let e = sum_config("a=b").unwrap_err();'),
     r"""
     #[derive(Debug)]
     struct Loading(ConfigError);

     impl std::fmt::Display for Loading {
         fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
             write!(f, "loading failed")
         }
     }

     impl std::error::Error for Loading {
         fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
             Some(&self.0)
         }
     }

     #[test]
     fn three_level_chain() {
         let inner = "q".parse::<i64>().unwrap_err();
         let e = Loading(ConfigError { line: 7, source: Some(inner) });
         check!("Loading(line 7: bad number(invalid digit)): chain", chain(&e), vec!["loading failed".to_string(), "line 7: bad number".to_string(), "invalid digit found in string".to_string()]);
         check!("root cause is the ParseIntError", root_cause(&e).downcast_ref::<std::num::ParseIntError>().is_some(), true);
         check!("bad_line of the outer error", bad_line(&e), None);
     }
     """,
     T("second_equals_is_value", "\"a=b=1\" (value \"b=1\")", 'sum_config("a=b=1").map_err(|e| bad_line(&*e))', "Err(Some(1))"),
     r"""
     #[test]
     fn random_vs_model() {
         let mut rng = anneal_prelude::Rng::new(6311);
         for _ in 0..300 {
             let n = rng.below(5);
             let mut lines = Vec::new();
             for _ in 0..n {
                 lines.push(match rng.below(5) {
                     0 => String::new(),
                     1 => "junk".to_string(),
                     2 => "k=zz".to_string(),
                     _ => format!("k={}", rng.int(-50, 50)),
                 });
             }
             let text = lines.join("\n");
             let mut want: Result<i64, (usize, bool)> = Ok(0);
             for (i, l) in lines.iter().enumerate() {
                 if l.trim().is_empty() {
                     continue;
                 }
                 match l.split_once('=') {
                     None => {
                         want = Err((i + 1, false));
                         break;
                     }
                     Some((_, v)) => match v.trim().parse::<i64>() {
                         Ok(x) => want = want.map(|t| t + x),
                         Err(_) => {
                             want = Err((i + 1, true));
                             break;
                         }
                     },
                 }
             }
             let got = sum_config(&text).map_err(|e| (bad_line(&*e).unwrap(), e.source().is_some()));
             check!(format!("config {text:?}"), got, want);
         }
     }
     """],
    [("rust", "`Error::source` has a default returning `None`. Override it: `fn source(&self) -> Option<&(dyn Error + 'static)>`, and turn the `Option<ParseIntError>` into that with `as_ref()` and a cast."),
     ("rust", "`?` converts any `E: Error + Send + Sync + 'static` into `Box<dyn Error + Send + Sync>`. Build the `ConfigError` with `ok_or` / `map_err` first so the line number isn't lost."),
     ("rust", "`downcast_ref::<T>()` exists on `dyn Error + 'static`, which is why the helpers take `&(dyn Error + 'static)`. `&*boxed` gets one from the `Box`.")],
    ("""`Box<dyn Error + Send + Sync>` is the usual type-erased error: any error type can go in through `?`, and it can cross threads. Erasing the type is why `'static` matters: `downcast_ref` needs `T: 'static` (it compares `TypeId`s), so it's defined on `dyn Error + 'static`, and `source()` returns `&(dyn Error + 'static)` so callers can keep downcasting along the chain. Wrapping a lower-level error as the `source` keeps both the context (the line) and the cause; walking `source()` gives the chain and its root. The `'static` in `source`'s return type is the object-lifetime bound of the *error type* (it has no short borrows), not the lifetime of the reference, which is tied to `&self`. It even counts for elision: `fn root_cause(err: &(dyn Error + 'static)) -> &(dyn Error + 'static)` is E0106, because the input has two lifetimes (the reference's and the explicit `'static`), so `root_cause` names `'e`.

Syntax to remember: `fn source(&self) -> Option<&(dyn Error + 'static)> { self.source.as_ref().map(|e| e as _) }` · `err.downcast_ref::<ConfigError>()` · `while let Some(next) = cur.source() { cur = next; }`.""", "O(n)", "O(depth) for chain"),
    "`anyhow::Error` and `Box<dyn Error>` both erase the type. What does `anyhow` add on top, and what does it give up?",
    ["`Box<dyn Error + Send + Sync>` erases error types; `?` converts into it.", "`source()` returns `&(dyn Error + 'static)` so the chain stays downcastable.", "`downcast_ref` needs the `'static` object bound."],
    related=("L3", "L8"),
    wrong=dict(
        loses_the_source=sub(CFG_SOLUTION, "        self.source.as_ref().map(|e| e as &(dyn Error + 'static))\n", "        None\n"),
        zero_based_lines=sub(CFG_SOLUTION, "ok_or(ConfigError { line: i + 1, source: None })", "ok_or(ConfigError { line: i, source: None })"),
        root_is_first_source=sub(CFG_SOLUTION, "    let mut cur = err;\n    while let Some(next) = cur.source() {\n        cur = next;\n    }\n    cur\n", "    err.source().unwrap_or(err)\n"),
    ),
))

THREAD_STARTER = r"""
use std::thread::{self, JoinHandle};

/// Sums `data` using up to `chunks` threads, and returns once they're done.
pub fn parallel_sum(data: &[u64], chunks: usize) -> u64 {
    let size = data.len().div_ceil(chunks.max(1)).max(1);
    let handles: Vec<_> = data.chunks(size).map(|chunk| thread::spawn(move || chunk.iter().sum::<u64>())).collect();
    handles.into_iter().map(|h| h.join().expect("worker panicked")).sum()
}

/// Starts counting the whitespace-separated words of `docs` in the background. The caller joins later,
/// possibly after dropping `docs`.
pub fn count_later(docs: &[String]) -> JoinHandle<usize> {
    thread::spawn(move || docs.iter().map(|d| d.split_whitespace().count()).sum())
}

/// Runs `job` on a new thread called `name`.
pub fn spawn_named<F, T>(name: &str, job: F) -> JoinHandle<T>
where
    F: FnOnce() -> T + Send,
    T: Send,
{
    thread::Builder::new().name(name.to_string()).spawn(job).expect("spawning a thread")
}
"""

THREAD_SOLUTION = THREAD_STARTER
for _old, _new in [
    ("    let handles: Vec<_> = data.chunks(size).map(|chunk| thread::spawn(move || chunk.iter().sum::<u64>())).collect();\n    handles.into_iter().map(|h| h.join().expect(\"worker panicked\")).sum()\n",
     "    thread::scope(|s| {\n        let handles: Vec<_> = data.chunks(size).map(|chunk| s.spawn(move || chunk.iter().sum::<u64>())).collect();\n        handles.into_iter().map(|h| h.join().expect(\"worker panicked\")).sum()\n    })\n"),
    ("    thread::spawn(move || docs.iter().map(|d| d.split_whitespace().count()).sum())\n",
     "    let docs = docs.to_vec();\n    thread::spawn(move || docs.iter().map(|d| d.split_whitespace().count()).sum())\n"),
    ("    F: FnOnce() -> T + Send,\n    T: Send,\n", "    F: FnOnce() -> T + Send + 'static,\n    T: Send + 'static,\n"),
]:
    THREAD_SOLUTION = sub(THREAD_SOLUTION, _old, _new)

P.append(fix(
    "fix-thread-spawn-static", "Fix: thread::spawn needs 'static, scope doesn't", "medium", "static-bounds", ["E0521", "E0310", "thread::scope", "Send + 'static", "thread::Builder"],
    """
        None of these compiles, and each needs a different fix. `parallel_sum` returns only after its threads
        finish, so it shouldn't copy anything. `count_later`'s thread may outlive the call (the caller joins
        after dropping `docs`), so it can't borrow. `spawn_named`'s bounds don't promise what
        `thread::Builder::spawn` needs.
    """,
    THREAD_STARTER,
    THREAD_SOLUTION,
    [T("parallel_sum_example", "1..=100 on 4 threads", "parallel_sum(&(1..=100).collect::<Vec<u64>>(), 4)", "5050"),
     T("count_after_dropping_docs", "count_later([\"a b\", \"c\"]), docs dropped, then join", "{ let h = { let docs = vec![\"a b\".to_string(), \"c\".to_string()]; count_later(&docs) }; h.join().unwrap() }", "3"),
     T("spawn_named_sets_the_name", "spawn_named(\"worker-1\", read own name)", 'spawn_named("worker-1", || std::thread::current().name().map(String::from)).join().unwrap()', 'Some("worker-1".to_string())'),
     T("parallel_sum_empty", "[] on 3 threads", "parallel_sum(&[], 3)", "0"),
     T("spawn_named_moves_data", "spawn_named with an owned Vec", 'spawn_named("sum", move || v.iter().sum::<i32>()).join().unwrap()', "6", setup="let v = vec![1, 2, 3];")],
    [T("chunks_zero", "[1, 2] on 0 threads", "parallel_sum(&[1, 2], 0)", "3"),
     T("more_threads_than_items", "[5, 6] on 10 threads", "parallel_sum(&[5, 6], 10)", "11"),
     T("uneven_chunks", "1..=10 on 3 threads", "parallel_sum(&(1..=10).collect::<Vec<u64>>(), 3)", "55"),
     T("big_values", "[u32::MAX as u64; 4]", "parallel_sum(&[u32::MAX as u64; 4], 2)", "4 * u32::MAX as u64"),
     T("count_empty", "count_later([])", "count_later(&[]).join().unwrap()", "0"),
     T("count_whitespace_kinds", "count_later([\"a\\tb\\nc  \"])", 'count_later(&["a\\tb\\nc  ".to_string()]).join().unwrap()', "3"),
     T("spawn_named_returns_value", "spawn_named returning a String", 'spawn_named("s", || "done".to_string()).join().unwrap()', '"done".to_string()'),
     T("spawn_named_panics_are_caught", "a job that panics", 'spawn_named("p", || -> u8 { panic!("boom") }).join().is_err()', "true"),
     r"""
     #[test]
     fn random_vs_brute_force() {
         let mut rng = anneal_prelude::Rng::new(6312);
         for _ in 0..100 {
             let n = rng.below(40);
             let data: Vec<u64> = rng.vec(n, 0, 1000);
             let chunks = rng.below(6);
             check!(format!("parallel_sum({data:?}, {chunks})"), parallel_sum(&data, chunks), data.iter().sum::<u64>());
         }
     }

     #[test]
     fn big_input() {
         let data: Vec<u64> = (0..1_000_000).collect();
         check!("0..1000000 on 8 threads", parallel_sum(&data, 8), 499_999_500_000);
     }
     """],
    [("rust", "`thread::spawn` requires `F: 'static` because the thread may outlive the caller. `thread::scope` joins every thread it spawned before returning, so its threads may borrow anything that outlives the scope."),
     ("rust", "`count_later` returns the handle, so its thread really can outlive `docs`. Scoped threads can't help; the thread must own its data. That's the one place a copy is necessary."),
     ("rust", "`Builder::spawn` has the same bounds as `thread::spawn`: `F: FnOnce() -> T + Send + 'static, T: Send + 'static`. Generic wrappers must repeat them (E0310).")],
    ("""`thread::spawn`'s `'static` bound isn't about how long the closure runs; it's that nothing can stop the thread from outliving the caller's stack frame, so the closure may not borrow from it (E0521 for borrowed data, E0310 for a generic `F`). There are two honest fixes, and the design decides. If the caller waits for the threads anyway, `thread::scope` proves they finish before the borrow ends, and plain borrows work, with no copies and no `Arc`. If the thread is meant to outlive the call, as with a returned `JoinHandle`, it must own its data: copy it (or share it through `Arc`). A generic wrapper around `spawn` inherits its bounds: `F: FnOnce() -> T + Send + 'static` and `T: Send + 'static`, since the result is handed back through `join`.

Syntax to remember: `thread::scope(|s| { let h = s.spawn(move || ..); h.join().unwrap() })` · `F: FnOnce() -> T + Send + 'static, T: Send + 'static` · `thread::Builder::new().name(n.to_string()).spawn(job)`.""", "O(n / threads) per thread", "O(threads); O(docs) for the copy"),
    "`count_later` could take `Arc<[String]>` instead of copying. When is that better, and what does it cost the caller?",
    ["`spawn` needs `'static` because the thread may outlive the caller.", "`thread::scope` lets threads borrow, because it joins them first.", "Wrappers around `spawn` repeat its `Send + 'static` bounds."],
    rules=dict(methods=["leak"], unsafe=True),
    related=("L3", "L1", "C1"),
    wrong=dict(
        drops_the_tail=sub(THREAD_SOLUTION, "data.chunks(size).map(|chunk| s.spawn(", "data.chunks_exact(size).map(|chunk| s.spawn("),
        counts_lines=sub(THREAD_SOLUTION, "docs.iter().map(|d| d.split_whitespace().count()).sum())\n}", "docs.iter().map(|d| d.lines().count()).sum())\n}"),
        ignores_the_name=sub(THREAD_SOLUTION, "thread::Builder::new().name(name.to_string()).spawn(job)", "thread::Builder::new().spawn(job)"),
    ),
))

BOARD_HEAD = r"""
use std::fmt::Display;

pub trait Describe {
    fn describe(&self) -> String;
}

impl<T: Display + ?Sized> Describe for T {
    fn describe(&self) -> String {
        format!("<{self}>")
    }
}
"""

BOARD_STARTER = BOARD_HEAD + r"""
/// Things to describe later. They may borrow data that lives for `'a`.
pub struct Board<'a> {
    items: Vec<Box<dyn Describe + 'a>>,
}

impl<'a> Board<'a> {
    pub fn new() -> Self {
        Board { items: Vec::new() }
    }

    pub fn pin<T: Describe>(&mut self, item: T) {
        self.items.push(Box::new(item));
    }

    /// Every item's description, joined with " ".
    pub fn render(&self) -> String {
        self.items.iter().map(|i| i.describe()).collect::<Vec<_>>().join(" ")
    }
}

/// Pins a borrow of each item.
pub fn pin_all<'a, T: Display>(board: &mut Board<'a>, items: &'a [T]) {
    for it in items {
        board.pin(it);
    }
}

/// `x`, boxed to be described later, for as long as `'a`.
pub fn boxed<'a, T: Display>(x: T) -> Box<dyn Describe + 'a> {
    Box::new(x)
}

/// `x`, boxed to be kept anywhere.
pub fn boxed_forever<T: Display>(x: T) -> Box<dyn Describe> {
    Box::new(x)
}

/// Each item's description, lazily.
pub fn describe_each<'a, T: Describe>(items: &'a [T]) -> impl Iterator<Item = String> + 'a {
    items.iter().map(|t| t.describe())
}
"""

BOARD_SOLUTION = BOARD_STARTER
for _old, _new in [
    ("    pub fn pin<T: Describe>(&mut self, item: T) {", "    pub fn pin<T: Describe + 'a>(&mut self, item: T) {"),
    ("pub fn boxed<'a, T: Display>(x: T) -> Box<dyn Describe + 'a> {", "pub fn boxed<'a, T: Display + 'a>(x: T) -> Box<dyn Describe + 'a> {"),
    ("pub fn boxed_forever<T: Display>(x: T) -> Box<dyn Describe> {", "pub fn boxed_forever<T: Display + 'static>(x: T) -> Box<dyn Describe> {"),
]:
    BOARD_SOLUTION = sub(BOARD_SOLUTION, _old, _new)

P.append(fix(
    "fix-t-outlives-a", "Fix: the parameter type may not live long enough (T: 'a)", "medium", "static-bounds", ["T: 'a", "E0309", "E0310", "implied bounds", "dyn Trait + 'a"],
    """
        Three functions box a generic `T` into a trait object and don't compile: nothing says `T` lives as
        long as the object may be kept. Add the bounds they need, and no more: the tests pin borrowed data,
        so don't require `'static` where `'a` will do. `pin_all` and `describe_each` compile already; work out
        why before you touch them.
    """,
    BOARD_STARTER,
    BOARD_SOLUTION,
    [T("pin_owned_and_borrowed", "pin 1, a String and a &str into a local String", "b.render()", '"<1> <owned> <local>"', setup='let local = String::from("local");\nlet mut b = Board::new();\nb.pin(1);\nb.pin(String::from("owned"));\nb.pin(local.as_str());'),
     T("pin_all_borrows", "pin_all([2, 3])", "b.render()", '"<2> <3>"', setup="let items = vec![2, 3];\nlet mut b = Board::new();\npin_all(&mut b, &items);"),
     T("boxed_borrowed", "boxed(&local)", "boxed(&local).describe()", '"<x>"', setup='let local = String::from("x");'),
     T("boxed_forever_owned", "boxed_forever(String), kept in a 'static Vec", "v[0].describe()", '"<kept>"', setup='let v: Vec<Box<dyn Describe + \'static>> = vec![boxed_forever(String::from("kept"))];'),
     T("describe_each_example", "describe_each([\"a\", \"b\"])", 'describe_each(&["a", "b"]).collect::<Vec<_>>()', 'vec!["<a>".to_string(), "<b>".to_string()]'),
     T("empty_board", "new board", "Board::new().render()", '""')],
    [T("pin_float", "pin 1.5", "{ let mut b = Board::new(); b.pin(1.5); b.render() }", '"<1.5>"'),
     T("pin_char_and_bool", "pin 'c', true", "{ let mut b = Board::new(); b.pin('c'); b.pin(true); b.render() }", '"<c> <true>"'),
     T("pin_all_empty", "pin_all([])", "{ let items: Vec<u8> = vec![]; let mut b = Board::new(); pin_all(&mut b, &items); b.render() }", '""'),
     T("mixed_board", "pin 0, then pin_all of Strings", "b.render()", '"<0> <p> <q>"', setup='let items = vec![String::from("p"), String::from("q")];\nlet mut b = Board::new();\nb.pin(0);\npin_all(&mut b, &items);'),
     T("boxed_owned", "boxed(42)", "boxed(42).describe()", '"<42>"'),
     T("describe_each_empty", "describe_each of an empty slice", "describe_each::<u8>(&[]).count()", "0"),
     T("boxed_forever_literal", "boxed_forever(\"lit\")", 'boxed_forever("lit").describe()', '"<lit>"'),
     T("unicode", "pin \"日本\"", '{ let mut b = Board::new(); b.pin("日本"); b.render() }', '"<日本>"'),
     r"""
     #[test]
     fn random_vs_model() {
         let mut rng = anneal_prelude::Rng::new(6313);
         for _ in 0..200 {
             let n = rng.below(6);
             let owned: Vec<String> = (0..n).map(|i| format!("w{i}")).collect();
             let nums: Vec<i64> = rng.vec(n, -9, 9);
             let mut b = Board::new();
             let mut want = Vec::new();
             for i in 0..n {
                 if rng.bool() {
                     b.pin(owned[i].as_str());
                     want.push(format!("<{}>", owned[i]));
                 } else {
                     b.pin(nums[i]);
                     want.push(format!("<{}>", nums[i]));
                 }
             }
             check!(format!("pins over {owned:?} / {nums:?}"), b.render(), want.join(" "));
         }
     }
     """],
    [("rust", "`Box<dyn Describe + 'a>` may be kept for all of `'a`, so whatever goes in must not contain borrows shorter than `'a`. For a generic `T` that's the bound `T: 'a` (E0309); for `Box<dyn Describe>`, which means `+ 'static`, it's `T: 'static` (E0310)."),
     ("rust", "`pin_all` takes `items: &'a [T]`. A reference type is only well-formed if `T: 'a`, so the compiler infers that bound from the signature: an *implied bound*. `describe_each` gets it the same way."),
     ("rust", "Requiring `'static` on `pin` would compile too, but then the board couldn't hold `&str`s into a local `String`, which the tests do.")],
    ("""`T: 'a` says \"every reference inside `T` lives at least `'a`\", so a value of `T` can be kept that long. Putting a `T` into a `Box<dyn Trait + 'a>` needs exactly that, and the compiler asks for it explicitly when nothing in the signature implies it (E0309, or E0310 when the object is `'static` by default, as in plain `Box<dyn Trait>`). Implied bounds cover many cases silently: a parameter of type `&'a T` or `&'a [T]` only exists if `T: 'a`, so `pin_all` and `describe_each` get the bound for free, and structs like `struct Ref<'a, T>(&'a T)` don't need `T: 'a` written since Rust 2018. The right bound is the weakest one that works: `'a` for the board, `'static` only for the box that's kept anywhere.

Syntax to remember: `fn pin<T: Describe + 'a>(&mut self, item: T)` · `fn boxed<'a, T: Display + 'a>(x: T) -> Box<dyn Describe + 'a>` · `Box<dyn Trait>` = `Box<dyn Trait + 'static>`.""", "O(n)", "O(n)"),
    "Why does `struct Ref<'a, T>(&'a T);` not need `where T: 'a` any more, when it did before Rust 2018?",
    ["`T: 'a`: `T` holds no borrows shorter than `'a`.", "Boxing `T` as `dyn Trait + 'a` needs `T: 'a`; `Box<dyn Trait>` needs `T: 'static`.", "Implied bounds from `&'a T` parameters."],
    rules=dict(methods=["to_string", "leak", "clone"], lines=3),
    related=("L3", "L4"),
    wrong=dict(
        render_without_separator=sub(BOARD_SOLUTION, '.collect::<Vec<_>>().join(" ")', '.collect::<Vec<_>>().concat()'),
        pin_all_reversed=sub(BOARD_SOLUTION, "    for it in items {\n        board.pin(it);", "    for it in items.iter().rev() {\n        board.pin(it);"),
    ),
))

# ---------------------------------------------------------------- variance & HRTBs (hard)

VAR_STARTER = r"""
use std::cell::Cell;

fn add_defaults(names: &mut Vec<&'static str>) {
    names.push("root");
    names.push("admin");
}

/// The names in `input` (one per line), then "root" and "admin".
pub fn all_names(input: &str) -> Vec<&str> {
    let mut names: Vec<&str> = input.lines().collect();
    add_defaults(&mut names);
    names
}

/// Keeps in `slot` the shorter of its current value and `candidate` (the current one on a tie).
pub fn keep_shortest(slot: &Cell<&'static str>, candidate: &str) {
    if candidate.len() < slot.get().len() {
        slot.set(candidate);
    }
}

/// The shortest line of `text` (the first on a tie), or "(none)" when there are no lines.
pub fn shortest_line(text: &str) -> &str {
    let mut lines = text.lines();
    let Some(first) = lines.next() else { return "(none)" };
    let slot = Cell::new(first);
    for line in lines {
        keep_shortest(&slot, line);
    }
    slot.get()
}

/// Applies `measure` to `s`.
pub fn apply(measure: fn(&'static str) -> usize, s: &str) -> usize {
    measure(s)
}

/// The same names, typed as shorter-lived references.
pub fn shorten<'a>(v: Vec<&'static str>) -> Vec<&'a str> {
    v
}
"""

VAR_SOLUTION = VAR_STARTER
for _old, _new in [
    ("fn add_defaults(names: &mut Vec<&'static str>) {", "fn add_defaults<'a>(names: &mut Vec<&'a str>) {"),
    ("pub fn keep_shortest(slot: &Cell<&'static str>, candidate: &str) {", "pub fn keep_shortest<'a>(slot: &Cell<&'a str>, candidate: &'a str) {"),
    ("pub fn apply(measure: fn(&'static str) -> usize, s: &str) -> usize {", "pub fn apply(measure: fn(&str) -> usize, s: &str) -> usize {"),
]:
    VAR_SOLUTION = sub(VAR_SOLUTION, _old, _new)

P.append(fix(
    "fix-mut-invariance", "Fix: variance of &mut, Cell and fn pointers", "hard", "variance-hrtbs", ["variance", "invariance", "Cell", "contravariance", "fn pointers"],
    """
        Three signatures ask for `'static` where a shorter lifetime would do, and variance keeps the compiler
        from quietly shortening it. Fix them so `all_names`, `shortest_line` and `apply` work on borrowed
        text. `shorten` compiles already: it's the case where variance does let `'static` shrink.
    """,
    VAR_STARTER,
    VAR_SOLUTION,
    [T("all_names_example", "all_names(\"ann\\nbo\")", 'all_names("ann\\nbo")', 'vec!["ann", "bo", "root", "admin"]'),
     T("all_names_borrows_input", "all_names of a String, first name points into it", "all_names(&text)[0].as_ptr() == text.as_ptr()", "true", setup='let text = String::from("zed");'),
     T("keep_shortest_with_locals", "slot \"longer\"; keep_shortest(a local \"ab\")", "slot.get()", '"ab"', setup='let local = String::from("ab");\nlet slot = std::cell::Cell::new("longer");\nkeep_shortest(&slot, &local);'),
     T("shortest_line_example", "shortest_line(\"abc\\nx\\nyy\")", 'shortest_line("abc\\nx\\nyy")', '"x"'),
     T("apply_to_a_local", "apply(str::len, a local String)", "apply(str::len, &s)", "5", setup='let s = String::from("hello");'),
     T("shorten_example", "shorten([\"a\"])", 'shorten(vec!["a"])', 'vec!["a"]')],
    [T("all_names_empty", "all_names(\"\")", 'all_names("")', 'vec!["root", "admin"]'),
     T("shortest_none", "shortest_line(\"\")", 'shortest_line("")', '"(none)"'),
     T("shortest_tie_first", "shortest_line(\"ab\\ncd\")", 'shortest_line("ab\\ncd")', '"ab"'),
     T("shortest_empty_line", "shortest_line(\"a\\n\\nb\")", 'shortest_line("a\\n\\nb")', '""'),
     T("keep_shortest_tie", "slot \"ab\"; keep_shortest(\"cd\")", '{ let slot = std::cell::Cell::new("ab"); keep_shortest(&slot, "cd"); slot.get() }', '"ab"'),
     T("apply_custom_fn", "apply(a fn counting commas, \"a,b,c\")", "apply(commas, &s)", "2", setup='fn commas(s: &str) -> usize {\n    s.matches(\',\').count()\n}\nlet s = String::from("a,b,c");'),
     T("apply_to_a_literal", "apply(str::len, \"\")", 'apply(str::len, "")', "0"),
     T("shortest_unicode_bytes", "shortest_line(\"éé\\nabc\")", 'shortest_line("éé\\nabc")', '"abc"'),
     r"""
     #[test]
     fn random_vs_brute_force() {
         let mut rng = anneal_prelude::Rng::new(6314);
         for _ in 0..300 {
             let len = rng.below(12);
             let text = rng.string(len, "ab\n");
             let mut best: Option<&str> = None;
             for l in text.lines() {
                 if best.map_or(true, |b| l.len() < b.len()) {
                     best = Some(l);
                 }
             }
             check!(format!("shortest_line({text:?})"), shortest_line(&text), best.unwrap_or("(none)"));
             let mut want: Vec<&str> = text.lines().collect();
             want.push("root");
             want.push("admin");
             check!(format!("all_names({text:?})"), all_names(&text), want);
             check!(format!("apply(str::len, {text:?})"), apply(str::len, &text), text.len());
         }
     }
     """],
    [("rust", "`&'a T` is covariant: a `&'static str` can stand in for a `&'a str`. `&mut T` is invariant in `T`: through a `&mut Vec<&'static str>`, `add_defaults` could store `'static` data, so the `Vec` must really hold `&'static str`s, and a `Vec<&'a str>` can't be passed."),
     ("rust", "`Cell<T>` is invariant in `T` for the same reason as `&mut T`: it allows writes through a shared reference."),
     ("rust", "Function arguments are contravariant: a `fn(&'static str)` only accepts `'static` strings. `fn(&str)` (really `for<'a> fn(&'a str)`) accepts any, and `str::len` is one.")],
    ("""Variance is what lets the compiler shorten lifetimes silently, and where it's absent you must write the right lifetime yourself. `&'a T`, `Box<T>`, `Vec<T>` are covariant, so `shorten` compiles: a `Vec<&'static str>` is a `Vec<&'a str>`. Anything that allows writing a `T` is invariant in `T`: `&mut T`, `Cell<T>`, `RefCell<T>`, `*mut T`. If `&mut Vec<&'static str>` accepted a `Vec<&'a str>`, the callee could push a `'static` string (fine) but the caller could also have pushed short-lived ones that the callee then treats as `'static` (not fine), so the types must match exactly. The fix is to make the helpers generic over `'a`. Function arguments go the other way (contravariant): a function that accepts any `&str` can stand in for one that only needs `&'static str`, but not the reverse, so `apply` should ask for the more general `fn(&str)`.

Syntax to remember: `fn add_defaults<'a>(names: &mut Vec<&'a str>)` · `fn keep_shortest<'a>(slot: &Cell<&'a str>, candidate: &'a str)` · `fn apply(measure: fn(&str) -> usize, ..)` = `for<'a> fn(&'a str) -> usize`.""", "O(n)", "O(n)"),
    "Show the unsound program `&mut Vec<&'static str>` being covariant would allow.",
    ["`&T`, `Box<T>`, `Vec<T>`: covariant.", "`&mut T`, `Cell<T>`: invariant in `T`.", "`fn(T)`: contravariant in its argument."],
    rules=dict(methods=["to_string", "leak", "clone", "to_owned"], lines=3),
    wrong=dict(
        keep_shortest_replaces_on_tie=sub(VAR_SOLUTION, "    if candidate.len() < slot.get().len() {", "    if candidate.len() <= slot.get().len() {"),
        defaults_first=sub(VAR_SOLUTION, "    let mut names: Vec<&str> = input.lines().collect();\n    add_defaults(&mut names);\n    names", "    let mut names: Vec<&str> = Vec::new();\n    add_defaults(&mut names);\n    names.extend(input.lines());\n    names"),
    ),
))

P.append(write(
    "hrtb-borrowed-iteration", "for<'a>: iterate a collection you own", "hard", "variance-hrtbs", ["HRTB", "for<'a>", "IntoIterator"],
    """
        `Stats<C>` owns any collection of `u32` that can be iterated by reference: `Vec`, arrays,
        `VecDeque`, `BTreeSet`… Add the bound that makes this work, then implement the methods. None of
        them may consume or copy the collection.
    """,
    """
    pub struct Stats<C> {
        data: C,
    }

    impl<C> Stats<C> {
        pub fn new(data: C) -> Self {
            Stats { data }
        }

        /// The mean, or None if empty.
        pub fn mean(&self) -> Option<f64> {
            todo!()
        }

        /// Largest minus smallest, or None if empty.
        pub fn spread(&self) -> Option<u32> {
            todo!()
        }

        pub fn count_above(&self, threshold: u32) -> usize {
            todo!()
        }
    }
    """,
    """
    pub struct Stats<C> {
        data: C,
    }

    impl<C> Stats<C>
    where
        for<'a> &'a C: IntoIterator<Item = &'a u32>,
    {
        pub fn new(data: C) -> Self {
            Stats { data }
        }

        /// The mean, or None if empty.
        pub fn mean(&self) -> Option<f64> {
            let (sum, n) = self.data.into_iter().fold((0u64, 0u64), |(s, n), &x| (s + u64::from(x), n + 1));
            (n > 0).then(|| sum as f64 / n as f64)
        }

        /// Largest minus smallest, or None if empty.
        pub fn spread(&self) -> Option<u32> {
            let max = self.data.into_iter().max()?;
            let min = self.data.into_iter().min()?;
            Some(max - min)
        }

        pub fn count_above(&self, threshold: u32) -> usize {
            self.data.into_iter().filter(|&&x| x > threshold).count()
        }
    }
    """,
    [T("vec_mean", "Vec [1, 2, 3]", "Stats::new(vec![1u32, 2, 3]).mean()", "Some(2.0)"),
     T("deque_spread", "VecDeque [5, 1]", "Stats::new(std::collections::VecDeque::from([5u32, 1])).spread()", "Some(4)"),
     T("empty", "empty Vec", "(s.mean(), s.spread(), s.count_above(0))", "(None, None, 0)", setup="let s = Stats::new(Vec::<u32>::new());"),
     T("strictly_above", "[1, 5, 9], above 5", "Stats::new(vec![1u32, 5, 9]).count_above(5)", "1"),
     T("fractional_mean", "BTreeSet {1, 10}", "Stats::new(std::collections::BTreeSet::from([1u32, 10])).mean()", "Some(5.5)")],
    [T("array", "[4, 4, 4]", "Stats::new([4u32, 4, 4]).spread()", "Some(0)"),
     T("single", "[7]", "(s.mean(), s.spread(), s.count_above(6))", "(Some(7.0), Some(0), 1)", setup="let s = Stats::new(vec![7u32]);"),
     T("sum_past_u32", "[u32::MAX, u32::MAX]", "Stats::new(vec![u32::MAX, u32::MAX]).mean()", "Some(u32::MAX as f64)"),
     T("full_spread", "[0, u32::MAX]", "Stats::new([0u32, u32::MAX]).spread()", "Some(u32::MAX)"),
     T("btreeset_dedups", "BTreeSet from [3, 3, 9]", "Stats::new(std::collections::BTreeSet::from([3u32, 3, 9])).mean()", "Some(6.0)"),
     T("linked_list", "LinkedList [2, 8, 5]", "(s.spread(), s.count_above(4))", "(Some(6), 2)",
       setup="let s = Stats::new(std::collections::LinkedList::from([2u32, 8, 5]));"),
     T("above_max", "[1, 2], above u32::MAX", "Stats::new(vec![1u32, 2]).count_above(u32::MAX)", "0"),
     T("unsorted_spread", "[5, 1, 9, 3]", "Stats::new(vec![5u32, 1, 9, 3]).spread()", "Some(8)"),
     """
     #[test]
     fn random_vs_brute_force() {
         let mut rng = anneal_prelude::Rng::new(317);
         for _ in 0..300 {
             let n = rng.below(10);
             let v: Vec<u32> = rng.vec(n, 0, 20);
             let t = rng.int(0, 20) as u32;
             let mean = if n == 0 { None } else { Some(v.iter().map(|&x| x as f64).sum::<f64>() / n as f64) };
             let spread = if n == 0 { None } else { Some(v.iter().max().unwrap() - v.iter().min().unwrap()) };
             let above = v.iter().filter(|&&x| x > t).count();
             let s = Stats::new(v.clone());
             check!(format!("data = {v:?}, threshold = {t}"), (s.mean(), s.spread(), s.count_above(t)), (mean, spread, above));
         }
     }

     #[test]
     fn scale_1m() {
         let s = Stats::new((0..1_000_000u32).collect::<Vec<_>>());
         check!("0..1000000", (s.mean(), s.spread(), s.count_above(999_990)), (Some(499_999.5), Some(999_999), 9));
     }
     """,
     T("btreeset", "BTreeSet {1, 10}", "Stats::new(std::collections::BTreeSet::from([1u32, 10])).mean()", "Some(5.5)"),
     T("empty", "empty Vec", "Stats::new(Vec::<u32>::new()).mean()", "None"),
     T("count", "[1, 5, 9], above 4", "Stats::new(vec![1u32, 5, 9]).count_above(4)", "2"),
     T("reuse", "call three methods on the same Stats", "(s.mean(), s.spread(), s.count_above(0))", "(Some(2.0), Some(2), 3)", setup="let s = Stats::new(vec![1u32, 2, 3]);")],
    [("rust", "Inside `mean(&self)` you borrow `self.data` for a lifetime that only exists inside the call. The bound must hold for every such lifetime."),
     ("rust", "`where for<'a> &'a C: IntoIterator<Item = &'a u32>` says: any borrow of `C` can be iterated, yielding borrows of the same length."),
     ("rust", "`&C` is `Copy`, so `self.data.into_iter()` can be called as many times as you like.")],
    ("A higher-ranked bound quantifies over lifetimes the caller can't name. `Fn(&str) -> &str` uses the same mechanism behind its sugar; here it has to be written out because the trait is applied to a reference type.", "O(n) per call", "O(1)"),
    "Rewrite the bound as a per-method `where &'s C: ...` with the method's own lifetime. Which do you prefer, and why?",
    ["Higher-ranked trait bounds: `for<'a>`.", "Bounds on `&C` rather than on `C`."],
    related=("L3", "L4"),
    wrong=dict(
        integer_mean="""
            pub struct Stats<C> {
                data: C,
            }

            impl<C> Stats<C>
            where
                for<'a> &'a C: IntoIterator<Item = &'a u32>,
            {
                pub fn new(data: C) -> Self {
                    Stats { data }
                }

                /// The mean, or None if empty.
                pub fn mean(&self) -> Option<f64> {
                    let (sum, n) = self.data.into_iter().fold((0u64, 0u64), |(s, n), &x| (s + u64::from(x), n + 1));
                    (n > 0).then(|| (sum / n) as f64)
                }

                /// Largest minus smallest, or None if empty.
                pub fn spread(&self) -> Option<u32> {
                    let max = self.data.into_iter().max()?;
                    let min = self.data.into_iter().min()?;
                    Some(max - min)
                }

                pub fn count_above(&self, threshold: u32) -> usize {
                    self.data.into_iter().filter(|&&x| x > threshold).count()
                }
            }
        """,
        u32_sum="""
            pub struct Stats<C> {
                data: C,
            }

            impl<C> Stats<C>
            where
                for<'a> &'a C: IntoIterator<Item = &'a u32>,
            {
                pub fn new(data: C) -> Self {
                    Stats { data }
                }

                /// The mean, or None if empty.
                pub fn mean(&self) -> Option<f64> {
                    let (sum, n) = self.data.into_iter().fold((0u32, 0u32), |(s, n), &x| (s + x, n + 1));
                    (n > 0).then(|| sum as f64 / n as f64)
                }

                /// Largest minus smallest, or None if empty.
                pub fn spread(&self) -> Option<u32> {
                    let max = self.data.into_iter().max()?;
                    let min = self.data.into_iter().min()?;
                    Some(max - min)
                }

                pub fn count_above(&self, threshold: u32) -> usize {
                    self.data.into_iter().filter(|&&x| x > threshold).count()
                }
            }
        """,
        at_or_above="""
            pub struct Stats<C> {
                data: C,
            }

            impl<C> Stats<C>
            where
                for<'a> &'a C: IntoIterator<Item = &'a u32>,
            {
                pub fn new(data: C) -> Self {
                    Stats { data }
                }

                /// The mean, or None if empty.
                pub fn mean(&self) -> Option<f64> {
                    let (sum, n) = self.data.into_iter().fold((0u64, 0u64), |(s, n), &x| (s + u64::from(x), n + 1));
                    (n > 0).then(|| sum as f64 / n as f64)
                }

                /// Largest minus smallest, or None if empty.
                pub fn spread(&self) -> Option<u32> {
                    let max = self.data.into_iter().max()?;
                    let min = self.data.into_iter().min()?;
                    Some(max - min)
                }

                pub fn count_above(&self, threshold: u32) -> usize {
                    self.data.into_iter().filter(|&&x| x >= threshold).count()
                }
            }
        """,
    ),
))

PARSE_HEAD = r"""
use std::io::BufRead;

/// A type that can be parsed out of text living for `'a`. Types that borrow from the text implement it for
/// that `'a`; types that own their data implement it for every `'a`.
pub trait Parse<'a>: Sized {
    fn parse(s: &'a str) -> Option<Self>;
}

impl<'a> Parse<'a> for u32 {
    fn parse(s: &'a str) -> Option<Self> {
        s.trim().parse().ok()
    }
}

impl<'a> Parse<'a> for String {
    fn parse(s: &'a str) -> Option<Self> {
        Some(s.trim().to_string())
    }
}

#[derive(Debug, PartialEq)]
pub struct Pair<'a> {
    pub key: &'a str,
    pub value: &'a str,
}
"""

PARSE_IMPLS = r"""
impl<'a> Parse<'a> for &'a str {
    fn parse(s: &'a str) -> Option<Self> {
        Some(s.trim())
    }
}

impl<'a> Parse<'a> for Pair<'a> {
    fn parse(s: &'a str) -> Option<Self> {
        let (key, value) = s.split_once('=')?;
        Some(Pair { key: key.trim(), value: value.trim() })
    }
}
"""

PARSE_TAIL_STARTER = r"""
/// Parses every comma-separated field of `line`; None if any field fails. Results may borrow from `line`.
pub fn fields<'a, T: Parse<'a>>(line: &'a str) -> Option<Vec<T>> {
    line.split(',').map(T::parse).collect()
}

/// Reads one line from `input` and parses it. The line is gone by the time the caller gets the value, so
/// only types that own their data can come back.
pub fn read_owned<T: Parse<'static>>(mut input: impl BufRead) -> Option<T> {
    let mut line = String::new();
    input.read_line(&mut line).ok()?;
    T::parse(&line)
}
"""

PARSE_TAIL_SOLUTION = sub(PARSE_TAIL_STARTER, "pub fn read_owned<T: Parse<'static>>(mut input: impl BufRead) -> Option<T> {", "pub fn read_owned<T>(mut input: impl BufRead) -> Option<T>\nwhere\n    T: for<'a> Parse<'a>,\n{")
PARSE_SOLUTION = PARSE_HEAD + PARSE_IMPLS + PARSE_TAIL_SOLUTION

P.append(fix(
    "trait-lifetimes-parse", "Lifetimes in traits: Parse<'a> and for<'a> Parse<'a>", "hard", "variance-hrtbs", ["trait lifetime parameters", "HRTB", "DeserializeOwned pattern", "E0597"],
    """
        `Parse<'a>` is modelled on serde's `Deserialize<'de>`: the lifetime lets a parsed value borrow from the
        text. Implement it for `&'a str` (the field, trimmed) and for `Pair<'a>` (`key=value`, both parts
        trimmed, `None` without an `=`), so that `fields` can return borrowed values. Then fix `read_owned`,
        whose bound can't work: it parses a `String` it owns and drops.
    """,
    PARSE_HEAD + "\n// TODO: Parse for &'a str and for Pair<'a>.\n" + PARSE_TAIL_STARTER,
    PARSE_SOLUTION,
    [T("fields_of_pairs", "fields::<Pair>(\"a=1, b = 2\")", 'fields::<Pair>("a=1, b = 2")', 'Some(vec![Pair { key: "a", value: "1" }, Pair { key: "b", value: "2" }])'),
     T("fields_of_strs", "fields::<&str>(\" x ,y\")", 'fields::<&str>(" x ,y")', 'Some(vec!["x", "y"])'),
     T("fields_of_numbers", "fields::<u32>(\"1, 2,3\")", 'fields::<u32>("1, 2,3")', "Some(vec![1, 2, 3])"),
     T("read_owned_number", "read_owned::<u32>(\" 42\\n\")", 'read_owned::<u32>(" 42\\n".as_bytes())', "Some(42)"),
     T("read_owned_string", "read_owned::<String>(\"hi \\nnext\")", 'read_owned::<String>("hi \\nnext".as_bytes())', 'Some("hi".to_string())'),
     T("pair_without_equals", "fields::<Pair>(\"a=1,b\")", 'fields::<Pair>("a=1,b")', "None")],
    [T("fields_borrow_the_line", "fields::<&str> results point into the line", 'fields::<&str>(&line).unwrap()[1].as_ptr() == line[3..].as_ptr()', "true", setup='let line = String::from("ab,cd");'),
     T("fields_bad_number", "fields::<u32>(\"1,x\")", 'fields::<u32>("1,x")', "None"),
     T("fields_empty_line", "fields::<&str>(\"\")", 'fields::<&str>("")', 'Some(vec![""])'),
     T("pair_second_equals_in_value", "fields::<Pair>(\"k=a=b\")", 'fields::<Pair>("k=a=b")', 'Some(vec![Pair { key: "k", value: "a=b" }])'),
     T("pair_empty_parts", "fields::<Pair>(\" = \")", 'fields::<Pair>(" = ")', 'Some(vec![Pair { key: "", value: "" }])'),
     T("read_owned_empty", "read_owned::<u32>(\"\")", 'read_owned::<u32>("".as_bytes())', "None"),
     T("read_owned_crlf", "read_owned::<u32>(\"7\\r\\n\")", 'read_owned::<u32>("7\\r\\n".as_bytes())', "Some(7)"),
     T("fields_strings_owned", "fields::<String>(\"a, b\") outlive the line", "v", 'Some(vec!["a".to_string(), "b".to_string()])', setup='let v = { let line = String::from("a, b"); fields::<String>(&line) };'),
     r"""
     #[test]
     fn random_vs_brute_force() {
         let mut rng = anneal_prelude::Rng::new(6315);
         for _ in 0..300 {
             let len = rng.below(12);
             let line = rng.string(len, "ab =,");
             let want: Option<Vec<(String, String)>> = line
                 .split(',')
                 .map(|f| f.split_once('=').map(|(k, v)| (k.trim().to_string(), v.trim().to_string())))
                 .collect();
             let got = fields::<Pair>(&line).map(|v| v.into_iter().map(|p| (p.key.to_string(), p.value.to_string())).collect::<Vec<_>>());
             check!(format!("fields::<Pair>({line:?})"), got, want);
             let want: Vec<&str> = line.split(',').map(str::trim).collect();
             check!(format!("fields::<&str>({line:?})"), fields::<&str>(&line), Some(want));
         }
     }
     """],
    [("rust", "An impl for a borrowing type ties the trait's lifetime to the type's: `impl<'a> Parse<'a> for Pair<'a>`. The parsed value then borrows the text it came from."),
     ("rust", "`T: Parse<'static>` asks for a type that can parse `'static` text, and `&line` isn't `'static` (E0597). What `read_owned` needs is a type that can parse text of *any* lifetime, however short."),
     ("rust", "That's a higher-ranked bound: `where T: for<'a> Parse<'a>`. `u32` and `String` satisfy it; `&str` and `Pair` only implement `Parse` for their own lifetime, so they're rejected, which is the point. serde calls this bound `DeserializeOwned`.")],
    ("""A lifetime parameter on a trait lets implementations return values that borrow their input. Borrowing types implement it for the matching lifetime only (`impl<'a> Parse<'a> for &'a str`), owning types for all of them (`impl<'a> Parse<'a> for u32`). Generic code then picks one of two bounds. `T: Parse<'a>` for a specific `'a` lets `T` borrow from input that lives that long (`fields`). `T: for<'a> Parse<'a>` says `T` can be parsed from input of any lifetime, which in practice means it borrows nothing, and it's the only bound that works when the input is a local buffer the function drops (`read_owned`). `Parse<'static>` looks similar and is wrong: it demands `'static` input. serde's `Deserialize<'de>` and `DeserializeOwned` (defined as `for<'de> Deserialize<'de>`) are exactly this pair.

Syntax to remember: `impl<'a> Parse<'a> for Pair<'a> { fn parse(s: &'a str) -> Option<Self> { .. } }` · `where T: for<'a> Parse<'a>` · `line.split(',').map(T::parse).collect::<Option<Vec<T>>>()`.""", "O(n)", "O(fields)"),
    "Why can't `read_owned::<&str>` ever compile, and what error would you expect?",
    ["Trait lifetime parameters let impls borrow from input.", "`T: Parse<'a>` for borrowed input; `T: for<'a> Parse<'a>` for owned results.", "`Parse<'static>` is not \"owned\"."],
    rules=dict(methods=["leak", "clone"]),
    related=("L3", "L5"),
    wrong=dict(
        pair_untrimmed=PARSE_HEAD + sub(PARSE_IMPLS, "Some(Pair { key: key.trim(), value: value.trim() })", "Some(Pair { key, value: value.trim() })") + PARSE_TAIL_SOLUTION,
        pair_splits_last_equals=PARSE_HEAD + sub(PARSE_IMPLS, "s.split_once('=')?", "s.rsplit_once('=')?") + PARSE_TAIL_SOLUTION,
        str_untrimmed=PARSE_HEAD + sub(PARSE_IMPLS, "        Some(s.trim())\n", "        Some(s)\n") + PARSE_TAIL_SOLUTION,
    ),
))

P.append(write(
    "self-referential-parser", "The self-referential struct trap", "hard", "variance-hrtbs", ["self-referential", "PhantomData", "design"],
    """
        The tempting design is `struct Parser { source: String, current: Option<&str> }`, where `current` points
        into `source`. It can't be written in safe Rust: moving the struct would move the `String` value while
        the reference still points at its buffer, and there's no lifetime to name for "my own field".

        Instead, let the caller own the text. Implement `Parser<'a, T>` over a borrowed `&'a str`, generic over
        a `Tokenize` strategy, and the two strategies:

        - `Whitespace` splits on whitespace.
        - `Comma` splits on `,`, trims each token, and skips empty ones.

        `current` is the token most recently returned by `advance`: `None` before the first call and after
        the end. Tokens must outlive the parser.
    """,
    """
    use std::marker::PhantomData;

    pub trait Tokenize {
        /// Splits the next token off `input`, returning (token, remainder).
        fn next_token(input: &str) -> Option<(&str, &str)>;
    }

    pub struct Whitespace;

    pub struct Comma;

    impl Tokenize for Whitespace {
        fn next_token(input: &str) -> Option<(&str, &str)> {
            todo!()
        }
    }

    impl Tokenize for Comma {
        fn next_token(input: &str) -> Option<(&str, &str)> {
            todo!()
        }
    }

    pub struct Parser<'a, T> {
        // Replace this with the fields you need. The source text is owned by the caller.
        _todo: PhantomData<(&'a str, T)>,
    }

    impl<'a, T: Tokenize> Parser<'a, T> {
        pub fn new(source: &'a str) -> Self {
            todo!()
        }

        pub fn advance(&mut self) -> Option<&'a str> {
            todo!()
        }

        pub fn current(&self) -> Option<&'a str> {
            todo!()
        }
    }
    """,
    """
    use std::marker::PhantomData;

    pub trait Tokenize {
        /// Splits the next token off `input`, returning (token, remainder).
        fn next_token(input: &str) -> Option<(&str, &str)>;
    }

    pub struct Whitespace;

    pub struct Comma;

    impl Tokenize for Whitespace {
        fn next_token(input: &str) -> Option<(&str, &str)> {
            let input = input.trim_start();
            if input.is_empty() {
                return None;
            }
            let end = input.find(char::is_whitespace).unwrap_or(input.len());
            Some(input.split_at(end))
        }
    }

    impl Tokenize for Comma {
        fn next_token(input: &str) -> Option<(&str, &str)> {
            let input = input.trim_start_matches(|c: char| c == ',' || c.is_whitespace());
            if input.is_empty() {
                return None;
            }
            let end = input.find(',').unwrap_or(input.len());
            let (token, rest) = input.split_at(end);
            Some((token.trim_end(), rest))
        }
    }

    pub struct Parser<'a, T> {
        remaining: &'a str,
        current: Option<&'a str>,
        // T only picks the tokenizer; PhantomData records it without storing one.
        _tokenizer: PhantomData<T>,
    }

    impl<'a, T: Tokenize> Parser<'a, T> {
        pub fn new(source: &'a str) -> Self {
            Parser { remaining: source, current: None, _tokenizer: PhantomData }
        }

        pub fn advance(&mut self) -> Option<&'a str> {
            self.current = match T::next_token(self.remaining) {
                Some((token, rest)) => {
                    self.remaining = rest;
                    Some(token)
                }
                None => None,
            };
            self.current
        }

        pub fn current(&self) -> Option<&'a str> {
            self.current
        }
    }
    """,
    [T("walks", '"parse me please" with Whitespace', "(p.advance(), p.advance(), p.current(), p.advance(), p.advance(), p.current())",
       '(Some("parse"), Some("me"), Some("me"), Some("please"), None, None)',
       setup='let source = String::from("parse me please");\nlet mut p = Parser::<Whitespace>::new(&source);'),
     T("tokens_outlive_parser", '"a b"; drop the parser after one advance', "first", 'Some("a")',
       setup='let source = String::from("a b");\nlet first;\n{\n    let mut p = Parser::<Whitespace>::new(&source);\n    first = p.advance();\n}'),
     T("comma", '"a, b,,c , " with Comma', "tokens", 'vec!["a", "b", "c"]',
       setup='let mut p = Parser::<Comma>::new("a, b,,c , ");\nlet tokens: Vec<&str> = std::iter::from_fn(|| p.advance()).collect();'),
     T("before_first", "current() before advance", 'Parser::<Whitespace>::new("x").current()', "None"),
     T("empty", '"" with Whitespace', '(p.advance(), p.current())', "(None, None)", setup='let mut p = Parser::<Whitespace>::new("");')],
    [T("comma", '"a, b,,c , " with Comma', "tokens", 'vec!["a", "b", "c"]',
       setup='let mut p = Parser::<Comma>::new("a, b,,c , ");\nlet tokens: Vec<&str> = std::iter::from_fn(|| p.advance()).collect();'),
     T("before_first", "current() before advance", 'Parser::<Whitespace>::new("x").current()', "None"),
     T("empty", '"   " with Whitespace', 'Parser::<Whitespace>::new("   ").advance()', "None"),
     T("comma_keeps_inner_spaces", '" new york , la" with Comma', "tokens", 'vec!["new york", "la"]',
       setup='let mut p = Parser::<Comma>::new(" new york , la");\nlet tokens: Vec<&str> = std::iter::from_fn(|| p.advance()).collect();'),
     T("tabs_and_newlines", '"\\ta\\n b\\r\\n" with Whitespace', "tokens", 'vec!["a", "b"]',
       setup='let mut p = Parser::<Whitespace>::new("\\ta\\n b\\r\\n");\nlet tokens: Vec<&str> = std::iter::from_fn(|| p.advance()).collect();'),
     T("unicode", '"héllo wörld" with Whitespace', "tokens", 'vec!["héllo", "wörld"]',
       setup='let mut p = Parser::<Whitespace>::new("héllo wörld");\nlet tokens: Vec<&str> = std::iter::from_fn(|| p.advance()).collect();'),
     T("stays_none_after_end", '"x": advance three times, then current', "(p.advance(), p.advance(), p.advance(), p.current())", '(Some("x"), None, None, None)',
       setup='let mut p = Parser::<Whitespace>::new("x");'),
     T("only_commas", '" , ,," with Comma', 'Parser::<Comma>::new(" , ,,").advance()', "None"),
     T("comma_newline_is_whitespace", '"a,\\n b" with Comma', "tokens", 'vec!["a", "b"]',
       setup='let mut p = Parser::<Comma>::new("a,\\n b");\nlet tokens: Vec<&str> = std::iter::from_fn(|| p.advance()).collect();'),
     T("whitespace_keeps_commas", '"a,b c" with Whitespace', "tokens", 'vec!["a,b", "c"]',
       setup='let mut p = Parser::<Whitespace>::new("a,b c");\nlet tokens: Vec<&str> = std::iter::from_fn(|| p.advance()).collect();'),
     T("next_token_direct", 'Comma::next_token(" a , b")', 'Comma::next_token(" a , b")', 'Some(("a", ", b"))'),
     T("zero_copy", "token points into the source", "std::ptr::eq(tok.as_ptr(), source[2..].as_ptr())", "true",
       setup='let source = String::from("  hi");\nlet tok = Parser::<Whitespace>::new(&source).advance().unwrap();'),
     """
     #[test]
     fn random_vs_brute_force() {
         let mut rng = anneal_prelude::Rng::new(318);
         for _ in 0..300 {
             let n = rng.below(12);
             let src = rng.string(n, "ab ,\\t");
             let want_ws: Vec<&str> = src.split(|c: char| c == ' ' || c == '\\t').filter(|t| !t.is_empty()).collect();
             let want_comma: Vec<&str> = src.split(',').map(|t| t.trim()).filter(|t| !t.is_empty()).collect();
             let mut p = Parser::<Whitespace>::new(&src);
             let got_ws: Vec<&str> = std::iter::from_fn(|| p.advance()).collect();
             let mut q = Parser::<Comma>::new(&src);
             let got_comma: Vec<&str> = std::iter::from_fn(|| q.advance()).collect();
             check!(format!("src = {src:?}"), (got_ws, got_comma), (want_ws, want_comma));
         }
     }
     """],
    [("approach", "Store the unread part of the source (`&'a str`) and the current token (`Option<&'a str>`). No `String` inside the parser."),
     ("rust", "`T` never appears in a field, so the compiler rejects it as unused. `PhantomData<T>` records it at zero size."),
     ("rust", "`next_token(input: &str) -> Option<(&str, &str)>` elides to one lifetime: both halves borrow `input`.")],
    ("Moving the owner out of the struct removes the self-reference: the parser is just two slices of someone else's text. If you truly need owner and view together, reach for a crate (`ouroboros`, `self_cell`) or store offsets instead of references.", "O(n) total", "O(1)"),
    "Rewrite `Parser` to own its `String` by storing byte offsets instead of references. What do you give up?",
    ["Why self-referential structs can't be written safely.", "`PhantomData` for type parameters that pick behaviour."],
    source="W38", related=("L3", "L4"),
    wrong=dict(
        comma_keeps_trailing_spaces="""
            use std::marker::PhantomData;

            pub trait Tokenize {
                /// Splits the next token off `input`, returning (token, remainder).
                fn next_token(input: &str) -> Option<(&str, &str)>;
            }

            pub struct Whitespace;

            pub struct Comma;

            impl Tokenize for Whitespace {
                fn next_token(input: &str) -> Option<(&str, &str)> {
                    let input = input.trim_start();
                    if input.is_empty() {
                        return None;
                    }
                    let end = input.find(char::is_whitespace).unwrap_or(input.len());
                    Some(input.split_at(end))
                }
            }

            impl Tokenize for Comma {
                fn next_token(input: &str) -> Option<(&str, &str)> {
                    let input = input.trim_start_matches(|c: char| c == ',' || c.is_whitespace());
                    if input.is_empty() {
                        return None;
                    }
                    let end = input.find(',').unwrap_or(input.len());
                    let (token, rest) = input.split_at(end);
                    Some((token, rest))
                }
            }

            pub struct Parser<'a, T> {
                remaining: &'a str,
                current: Option<&'a str>,
                // T only picks the tokenizer; PhantomData records it without storing one.
                _tokenizer: PhantomData<T>,
            }

            impl<'a, T: Tokenize> Parser<'a, T> {
                pub fn new(source: &'a str) -> Self {
                    Parser { remaining: source, current: None, _tokenizer: PhantomData }
                }

                pub fn advance(&mut self) -> Option<&'a str> {
                    self.current = match T::next_token(self.remaining) {
                        Some((token, rest)) => {
                            self.remaining = rest;
                            Some(token)
                        }
                        None => None,
                    };
                    self.current
                }

                pub fn current(&self) -> Option<&'a str> {
                    self.current
                }
            }
        """,
        current_sticks_after_end="""
            use std::marker::PhantomData;

            pub trait Tokenize {
                /// Splits the next token off `input`, returning (token, remainder).
                fn next_token(input: &str) -> Option<(&str, &str)>;
            }

            pub struct Whitespace;

            pub struct Comma;

            impl Tokenize for Whitespace {
                fn next_token(input: &str) -> Option<(&str, &str)> {
                    let input = input.trim_start();
                    if input.is_empty() {
                        return None;
                    }
                    let end = input.find(char::is_whitespace).unwrap_or(input.len());
                    Some(input.split_at(end))
                }
            }

            impl Tokenize for Comma {
                fn next_token(input: &str) -> Option<(&str, &str)> {
                    let input = input.trim_start_matches(|c: char| c == ',' || c.is_whitespace());
                    if input.is_empty() {
                        return None;
                    }
                    let end = input.find(',').unwrap_or(input.len());
                    let (token, rest) = input.split_at(end);
                    Some((token.trim_end(), rest))
                }
            }

            pub struct Parser<'a, T> {
                remaining: &'a str,
                current: Option<&'a str>,
                // T only picks the tokenizer; PhantomData records it without storing one.
                _tokenizer: PhantomData<T>,
            }

            impl<'a, T: Tokenize> Parser<'a, T> {
                pub fn new(source: &'a str) -> Self {
                    Parser { remaining: source, current: None, _tokenizer: PhantomData }
                }

                pub fn advance(&mut self) -> Option<&'a str> {
                    self.current = match T::next_token(self.remaining) {
                        Some((token, rest)) => {
                            self.remaining = rest;
                            Some(token)
                        }
                        None => return None,
                    };
                    self.current
                }

                pub fn current(&self) -> Option<&'a str> {
                    self.current
                }
            }
        """,
        spaces_only="""
            use std::marker::PhantomData;

            pub trait Tokenize {
                /// Splits the next token off `input`, returning (token, remainder).
                fn next_token(input: &str) -> Option<(&str, &str)>;
            }

            pub struct Whitespace;

            pub struct Comma;

            impl Tokenize for Whitespace {
                fn next_token(input: &str) -> Option<(&str, &str)> {
                    let input = input.trim_start_matches(' ');
                    if input.is_empty() {
                        return None;
                    }
                    let end = input.find(' ').unwrap_or(input.len());
                    Some(input.split_at(end))
                }
            }

            impl Tokenize for Comma {
                fn next_token(input: &str) -> Option<(&str, &str)> {
                    let input = input.trim_start_matches(|c: char| c == ',' || c.is_whitespace());
                    if input.is_empty() {
                        return None;
                    }
                    let end = input.find(',').unwrap_or(input.len());
                    let (token, rest) = input.split_at(end);
                    Some((token.trim_end(), rest))
                }
            }

            pub struct Parser<'a, T> {
                remaining: &'a str,
                current: Option<&'a str>,
                // T only picks the tokenizer; PhantomData records it without storing one.
                _tokenizer: PhantomData<T>,
            }

            impl<'a, T: Tokenize> Parser<'a, T> {
                pub fn new(source: &'a str) -> Self {
                    Parser { remaining: source, current: None, _tokenizer: PhantomData }
                }

                pub fn advance(&mut self) -> Option<&'a str> {
                    self.current = match T::next_token(self.remaining) {
                        Some((token, rest)) => {
                            self.remaining = rest;
                            Some(token)
                        }
                        None => None,
                    };
                    self.current
                }

                pub fn current(&self) -> Option<&'a str> {
                    self.current
                }
            }
        """,
    ),
))

CLOSURE_HEAD = r"""
/// Transformations applied in order.
pub struct Pipeline {
    steps: Vec<Box<dyn Fn(&str) -> &str>>,
}

impl Pipeline {
    pub fn new() -> Self {
        Pipeline { steps: Vec::new() }
    }

    pub fn add(&mut self, f: impl Fn(&str) -> &str + 'static) {
        self.steps.push(Box::new(f));
    }

    pub fn run<'a>(&self, s: &'a str) -> &'a str {
        self.steps.iter().fold(s, |acc, f| f(acc))
    }
}
"""

CLOSURE_STARTER = CLOSURE_HEAD + r"""
/// Every line of `text`, trimmed, skipping blank ones.
pub fn trimmed_lines(text: &str) -> Vec<&str> {
    let trim = |s: &str| s.trim();
    text.lines().map(trim).filter(|l| !l.is_empty()).collect()
}

/// The first comma-separated field of every line.
pub fn first_fields(text: &str) -> Vec<&str> {
    let first = |s: &str| -> &str { s.split(',').next().unwrap_or("") };
    text.lines().map(first).collect()
}

/// A pipeline that strips leading '#'s, then surrounding whitespace.
pub fn comment_stripper() -> Pipeline {
    let strip = |s: &str| s.trim_start_matches('#');
    let mut p = Pipeline::new();
    p.add(strip);
    p.add(|s| s.trim());
    p
}
"""

CLOSURE_SOLUTION = CLOSURE_HEAD + r"""
/// Every line of `text`, trimmed, skipping blank ones.
pub fn trimmed_lines(text: &str) -> Vec<&str> {
    text.lines().map(str::trim).filter(|l| !l.is_empty()).collect()
}

/// The first comma-separated field of every line.
pub fn first_fields(text: &str) -> Vec<&str> {
    fn first(s: &str) -> &str {
        s.split(',').next().unwrap_or("")
    }
    text.lines().map(first).collect()
}

/// A pipeline that strips leading '#'s, then surrounding whitespace.
pub fn comment_stripper() -> Pipeline {
    let mut p = Pipeline::new();
    p.add(|s| s.trim_start_matches('#'));
    p.add(|s| s.trim());
    p
}
"""

P.append(fix(
    "fix-closure-returns-its-argument", "Fix: closures that return a borrow of their argument", "hard", "variance-hrtbs", ["closure lifetime inference", "higher-ranked closures", "fn items"],
    """
        Three closures return a slice of their argument, and none of them compiles, even the one that spells
        out `-> &str`. The same bodies compile as `fn` items, and `Pipeline::add` accepts closures written
        inline. Fix the three functions; `Pipeline` itself is fine.
    """,
    CLOSURE_STARTER,
    CLOSURE_SOLUTION,
    [T("trimmed_lines_example", "trimmed_lines(\"  a \\n\\n b\")", 'trimmed_lines("  a \\n\\n b")', 'vec!["a", "b"]'),
     T("first_fields_example", "first_fields(\"a,b\\nc\\n,d\")", 'first_fields("a,b\\nc\\n,d")', 'vec!["a", "c", ""]'),
     T("comment_stripper_example", "comment_stripper().run(\"## note \")", 'comment_stripper().run("## note ")', '"note"'),
     T("results_borrow_the_input", "first_fields' result points into the text", "first_fields(&t)[0].as_ptr() == t.as_ptr()", "true", setup='let t = String::from("x,y");'),
     T("pipeline_result_outlives_pipeline", "run a dropped pipeline's steps on a String", "out", '"kept"', setup='let s = String::from("# kept");\nlet out = comment_stripper().run(&s);')],
    [T("trimmed_empty", "trimmed_lines(\"\")", 'trimmed_lines("").len()', "0"),
     T("first_fields_empty_line", "first_fields(\"\\n\")", 'first_fields("\\n")', 'vec![""]'),
     T("stripper_order", "\" #x\": whitespace first, so '#' stays", 'comment_stripper().run(" #x")', '"#x"'),
     T("stripper_no_comment", "\"plain\"", 'comment_stripper().run("plain")', '"plain"'),
     T("stripper_only_hashes", "\"###\"", 'comment_stripper().run("###")', '""'),
     T("empty_pipeline", "Pipeline::new().run(\" x \")", 'Pipeline::new().run(" x ")', '" x "'),
     T("custom_pipeline", "add inline closures: take 3 bytes, trim end", '{ let mut p = Pipeline::new(); p.add(|s| &s[..s.len().min(3)]); p.add(|s| s.trim_end()); p.run("ab cd") }', '"ab"'),
     T("trimmed_unicode_spaces", "trimmed_lines(\"\\u{3000}x\\u{3000}\")", 'trimmed_lines("\\u{3000}x\\u{3000}")', 'vec!["x"]'),
     r"""
     #[test]
     fn random_vs_brute_force() {
         let mut rng = anneal_prelude::Rng::new(6316);
         let p = comment_stripper();
         for _ in 0..300 {
             let len = rng.below(10);
             let text = rng.string(len, "a #,\n");
             let want: Vec<&str> = text.lines().map(str::trim).filter(|l| !l.is_empty()).collect();
             check!(format!("trimmed_lines({text:?})"), trimmed_lines(&text), want);
             let want: Vec<&str> = text.lines().map(|l| l.split(',').next().unwrap_or("")).collect();
             check!(format!("first_fields({text:?})"), first_fields(&text), want);
             check!(format!("comment_stripper().run({text:?})"), p.run(&text), text.trim_start_matches('#').trim());
         }
     }
     """],
    [("rust", "A `fn` item with an elided signature is generic over the lifetime: `fn trim(s: &str) -> &str` is `for<'a> fn(&'a str) -> &'a str`. A closure's signature is inferred once, when it's created, and a `let`-bound closure with `|s: &str|` gets an input lifetime and an output lifetime that aren't linked."),
     ("rust", "A closure written directly where an `Fn(&str) -> &str` bound expects it (like `p.add(|s| ..)`) is inferred against that bound, so it becomes higher-ranked. Bound to a `let` first, it isn't."),
     ("rust", "Fixes: use a path (`str::trim`), a nested `fn`, or write the closure inline at the call that has the bound.")],
    ("""The signature `Fn(&str) -> &str` is sugar for `for<'a> Fn(&'a str) -> &'a str`: one closure that works for every lifetime and returns a borrow of its argument. `fn` items get that shape from ordinary elision. Closures don't: their signature is inferred at the point they're created, and without an expected type the compiler gives the parameter's lifetime and the return's lifetime separate inference variables, then can't prove the result outlives what it borrowed. Writing `-> &str` doesn't help, because elision rules don't apply inside closure annotations. When the closure is written in a position that expects a higher-ranked `Fn` (an argument to `add`, `map` with a bound), the expectation is used and it works. So the fixes are to move the closure there, or to use a `fn` item or a path like `str::trim`. (The unstable `for<'a> |s: &'a str| -> &'a str` syntax would state it directly.)

Syntax to remember: `text.lines().map(str::trim)` · `fn first(s: &str) -> &str { .. }` nested in a function · `Box<dyn Fn(&str) -> &str>` = `Box<dyn for<'a> Fn(&'a str) -> &'a str>`.""", "O(n)", "O(lines)"),
    "Write a helper `fn hr<F: Fn(&str) -> &str>(f: F) -> F { f }`. Why does `let trim = hr(|s| s.trim());` compile when `let trim = |s: &str| s.trim();` doesn't?",
    ["A closure's signature is inferred where it's created.", "`Fn(&str) -> &str` is higher-ranked; `let`-bound closures aren't.", "Use `fn` items, paths, or inline closures."],
    rules=dict(methods=["to_string", "to_owned", "leak", "clone"]),
    wrong=dict(
        strip_after_trim=sub(CLOSURE_SOLUTION, "    p.add(|s| s.trim_start_matches('#'));\n    p.add(|s| s.trim());\n", "    p.add(|s| s.trim());\n    p.add(|s| s.trim_start_matches('#'));\n"),
        last_field=sub(CLOSURE_SOLUTION, "s.split(',').next().unwrap_or(\"\")", "s.rsplit(',').next().unwrap_or(\"\")"),
        keeps_blank_lines=sub(CLOSURE_SOLUTION, "    text.lines().map(str::trim).filter(|l| !l.is_empty()).collect()", "    text.lines().filter(|l| !l.is_empty()).map(str::trim).collect()"),
    ),
))

DYN_STARTER = r"""
use std::fmt::Display;

/// A predicate accepting exactly the words in `allowed`.
pub fn make_filter(allowed: &[&str]) -> Box<dyn Fn(&str) -> bool> {
    Box::new(move |w| allowed.contains(&w))
}

/// Named checks. Checks may borrow data that lives for `'a`.
pub struct Checks<'a> {
    list: Vec<(String, Box<dyn Fn(&str) -> bool>)>,
}

impl<'a> Checks<'a> {
    pub fn new() -> Self {
        Checks { list: Vec::new() }
    }

    pub fn add(&mut self, name: &str, check: impl Fn(&str) -> bool + 'a) {
        self.list.push((name.to_string(), Box::new(check)));
    }

    /// The names of the checks `word` fails, in the order they were added.
    pub fn failures(&self, word: &str) -> Vec<&str> {
        self.list.iter().filter(|(_, c)| !c(word)).map(|(n, _)| n.as_str()).collect()
    }
}

/// How many of `words` pass `check`.
pub fn count_passing(words: &[&str], check: &dyn Fn(&str) -> bool) -> usize {
    words.iter().filter(|w| check(w)).count()
}

/// Each name as something displayable, for later.
pub fn labels(names: &[String]) -> Vec<Box<dyn Display>> {
    names.iter().map(|n| Box::new(n.as_str()) as Box<dyn Display>).collect()
}
"""

DYN_SOLUTION = DYN_STARTER
for _old, _new in [
    ("pub fn make_filter(allowed: &[&str]) -> Box<dyn Fn(&str) -> bool> {", "pub fn make_filter<'a>(allowed: &'a [&str]) -> Box<dyn Fn(&str) -> bool + 'a> {"),
    ("    list: Vec<(String, Box<dyn Fn(&str) -> bool>)>,", "    list: Vec<(String, Box<dyn Fn(&str) -> bool + 'a>)>,"),
    ("pub fn labels(names: &[String]) -> Vec<Box<dyn Display>> {\n    names.iter().map(|n| Box::new(n.as_str()) as Box<dyn Display>).collect()",
     "pub fn labels(names: &[String]) -> Vec<Box<dyn Display + '_>> {\n    names.iter().map(|n| Box::new(n.as_str()) as Box<dyn Display>).collect()"),
]:
    DYN_SOLUTION = sub(DYN_SOLUTION, _old, _new)

P.append(fix(
    "fix-box-dyn-fn-borrows", "Fix: trait objects that borrow (dyn Trait + 'a)", "hard", "variance-hrtbs", ["trait object lifetimes", "default object bounds", "Box<dyn Fn + 'a>", "E0310"],
    """
        Three trait objects here borrow data, but their types don't allow it, so `make_filter`, `Checks` and
        `labels` don't compile. Fix the types, without copying the borrowed data. `count_passing` takes a
        `&dyn Fn` that may borrow anything and is right as it is: work out why it needs no change.
    """,
    DYN_STARTER,
    DYN_SOLUTION,
    [T("make_filter_example", "allowed [\"a\", \"b\"]: test a, c", "(f(\"a\"), f(\"c\"))", "(true, false)", setup='let allowed = vec!["a", "b"];\nlet f = make_filter(&allowed);'),
     T("checks_borrow_locals", "checks: min length from a local, in a local list", "c.failures(\"xy\")", 'vec!["long", "listed"]',
       setup='let min = 3;\nlet list = vec![String::from("abc")];\nlet mut c = Checks::new();\nc.add("long", |w| w.len() >= min);\nc.add("listed", |w| list.iter().any(|x| x == w));'),
     T("count_passing_borrowed_closure", "count words shorter than a local limit", 'count_passing(&["a", "abc", "ab"], &|w| w.len() < limit)', "2", setup="let limit = 3;"),
     T("labels_example", "labels of [\"x\", \"yz\"]", "labels(&names).iter().map(|l| l.to_string()).collect::<Vec<_>>()", 'vec!["x".to_string(), "yz".to_string()]', setup='let names = vec![String::from("x"), String::from("yz")];'),
     T("no_checks", "no checks: failures of anything", 'Checks::new().failures("w").len()', "0")],
    [T("filter_empty_allowed", "allowed []", 'make_filter(&[])("a")', "false"),
     T("filter_case_sensitive", "allowed [\"A\"]: test a", 'make_filter(&["A"])("a")', "false"),
     T("checks_all_pass", "one check that always passes", '{ let mut c = Checks::new(); c.add("any", |_| true); c.failures("q").len() }', "0"),
     T("checks_order", "three failing checks", 'c.failures("z")', 'vec!["c", "a", "b"]', setup='let mut c = Checks::new();\nc.add("c", |_| false);\nc.add("a", |_| false);\nc.add("b", |w| w.is_empty());'),
     T("checks_static_closure", "a check with no borrows", '(c.failures("12"), c.failures("1a"))', '(Vec::<&str>::new(), vec!["digit"])', setup='let mut c = Checks::new();\nc.add("digit", |w| w.chars().all(|ch| ch.is_ascii_digit()));'),
     T("count_passing_empty", "no words", 'count_passing(&[], &|_| true)', "0"),
     T("labels_empty", "labels of []", "labels(&[]).len()", "0"),
     T("labels_point_into_names", "a label displays the name it borrows", 'format!("{}{}", labels(&names)[1], labels(&names)[0])', '"ba".to_string()', setup='let names = vec![String::from("a"), String::from("b")];'),
     r"""
     #[test]
     fn random_vs_brute_force() {
         let mut rng = anneal_prelude::Rng::new(6317);
         let pool = ["a", "b", "c", "ab", "ba"];
         for _ in 0..300 {
             let k = rng.below(4);
             let allowed: Vec<&str> = (0..k).map(|_| *rng.pick(&pool)).collect();
             let f = make_filter(&allowed);
             let w = *rng.pick(&pool);
             check!(format!("allowed {allowed:?}; {w}"), f(w), allowed.contains(&w));
             let min = rng.below(3);
             let mut c = Checks::new();
             c.add("min", |x| x.len() >= min);
             c.add("allowed", |x| allowed.contains(&x));
             let mut want = Vec::new();
             if w.len() < min {
                 want.push("min");
             }
             if !allowed.contains(&w) {
                 want.push("allowed");
             }
             check!(format!("allowed {allowed:?}, min {min}; failures({w})"), c.failures(w), want);
         }
     }
     """],
    [("rust", "A trait object type always has a lifetime bound, and when you don't write one it's filled in: `Box<dyn Trait>` means `Box<dyn Trait + 'static>`. A closure that captures a `&'a` borrow isn't `'static`."),
     ("rust", "Say how long the object may live: `Box<dyn Fn(&str) -> bool + 'a>`, with `'a` tied to what it borrows. Inside a struct with a lifetime parameter, use that parameter."),
     ("rust", "Behind a reference the default is different: `&'r dyn Trait` means `&'r (dyn Trait + 'r)`. That's why `count_passing` accepts borrowing closures unchanged.")],
    ("""Every `dyn Trait` has an object lifetime bound: how long the erased value may be used, which limits what it may borrow. When omitted, it's defaulted from context: `Box<dyn Trait>` (and `Rc`, `Arc`, `Vec<Box<..>>`) default to `'static`; `&'r dyn Trait` and `&'r mut dyn Trait` default to `'r`; a type with a lifetime bound (`Ref<'a, dyn Trait>`, `struct S<'a, T: 'a + ?Sized>`) uses that. So a boxed closure capturing `allowed: &'a [&str]` needs `+ 'a` spelled out, and so does the field of `Checks<'a>` that `add` (already `+ 'a`) pushes into. `labels` returns boxed `&str`s pointing into `names`, so its objects are `+ '_`. A `&dyn Fn` parameter needed nothing: the reference's own lifetime is the default, so any closure that outlives the call fits.

Syntax to remember: `fn make_filter<'a>(allowed: &'a [&str]) -> Box<dyn Fn(&str) -> bool + 'a>` · `Vec<Box<dyn Display + '_>>` · `&dyn Trait` = `&'r (dyn Trait + 'r)` · `Box<dyn Trait>` = `Box<dyn Trait + 'static>`.""", "O(k) per check", "O(checks)"),
    "What's the default object lifetime of `Arc<Mutex<dyn Trait>>`, and how would you store borrowing handlers in one?",
    ["`Box<dyn Trait>` defaults to `+ 'static`.", "`&'r dyn Trait` defaults to `+ 'r`.", "Write `dyn Trait + 'a` when the object borrows."],
    rules=dict(methods=["to_owned", "leak", "clone", "cloned"], lines=4),
    wrong=dict(
        filter_prefix=sub(DYN_SOLUTION, "Box::new(move |w| allowed.contains(&w))", "Box::new(move |w| allowed.iter().any(|a| a.starts_with(w)))"),
        failures_are_passes=sub(DYN_SOLUTION, ".filter(|(_, c)| !c(word))", ".filter(|(_, c)| c(word))"),
        labels_reversed=sub(DYN_SOLUTION, "    names.iter().map(|n| Box::new(n.as_str()) as Box<dyn Display>).collect()", "    names.iter().rev().map(|n| Box::new(n.as_str()) as Box<dyn Display>).collect()"),
    ),
))


STAGES = [
    ("elision", "Elision", "easy"),
    ("structs-holding-refs", "Structs holding refs", "easy"),
    ("two-lifetimes", "Two lifetimes", "medium"),
    ("static-bounds", "'static and T: 'a", "medium"),
    ("variance-hrtbs", "Variance & HRTBs", "hard"),
]

# `source` and `examples` are optional; drop empty ones so problem.toml stays tidy.
for p in P:
    if not p.get("source"):
        p.pop("source", None)
    if p.get("rules") is None:
        p.pop("rules", None)
    if p.get("wrong") is None:
        p.pop("wrong", None)

if __name__ == "__main__":
    n = write_track("l3-lifetimes", "L3", "Lifetimes", "L", "core", 3,
                    "Lifetimes say what a reference borrows from. Most of the work is choosing which input an output is tied to.",
                    STAGES, P)
    print("L3", n)
