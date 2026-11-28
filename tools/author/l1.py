from author import T, write_track

P = []


def fix(slug, title, level, stage, tags, statement, starter, solution, visible, hidden, hints, notes, follow_up, teaches, rules=None, related=(), wrong=None, source=None):
    return dict(slug=slug, title=title, mode="fix", level=level, stage=stage, tags=tags, statement=statement, starter=starter,
                solution=solution, visible=visible, hidden=hidden, hints=hints, notes=notes, follow_up=follow_up,
                teaches=teaches, rules=rules, related=list(related), wrong=wrong, source=source)


def write(slug, title, level, stage, tags, statement, starter, solution, visible, hidden, hints, notes, follow_up, teaches, related=(), source=None, examples=(), wrong=None):
    return dict(slug=slug, title=title, level=level, stage=stage, tags=tags, statement=statement, starter=starter,
                solution=solution, visible=visible, hidden=hidden, hints=hints, notes=notes, follow_up=follow_up,
                teaches=teaches, related=list(related), source=source, examples=list(examples), wrong=wrong)


def sub(s, old, new):
    """str.replace that fails loudly when `old` isn't there (a wrong solution that silently equals the reference)."""
    assert old in s, f"not found: {old[:60]!r}"
    return s.replace(old, new)


# Counts heap allocations made on the current test thread, for tests that check something moves instead of copying.
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

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) }
    }
}

#[global_allocator]
static GLOBAL: CountingAlloc = CountingAlloc;

/// Runs `f` and returns its result with the number of allocations it made.
fn allocs<R>(f: impl FnOnce() -> R) -> (R, usize) {
    let before = ALLOCS.with(|n| n.get());
    let r = f();
    (r, ALLOCS.with(|n| n.get()) - before)
}
"""

# ---------------------------------------------------------------- moves & Copy (easy)

BATCH_STARTER = r"""
#[derive(Debug, PartialEq)]
pub struct Batch {
    pub id: u32,
    pub label: Option<String>,
    pub items: Vec<String>,
}

/// One line about the batch, and its items handed back for reuse. The line is
/// "<label, or #<id>>: <n> items (<k> long), longest <item>", where long means more than 3 bytes and the
/// longest item is the first of the longest ("-" when there are none). A batch without a label also gets
/// " [unlabelled]" at the end.
pub fn describe(batch: Batch) -> (String, Vec<String>) {
    let name = batch.label.unwrap_or(format!("#{}", batch.id));
    let longest = longest_item(batch.items);
    let mut long = 0;
    for item in batch.items {
        if item.len() > 3 {
            long += 1;
        }
    }
    let mut line = format!("{name}: {} items ({long} long), longest {}", batch.items.len(), longest.unwrap_or("-".into()));
    if batch.label.is_none() {
        line.push_str(" [unlabelled]");
    }
    (line, batch.items)
}

fn longest_item(items: Vec<String>) -> Option<String> {
    let mut best: Option<String> = None;
    for item in items {
        if best.as_ref().map_or(true, |b| item.len() > b.len()) {
            best = Some(item);
        }
    }
    best
}
"""

BATCH_SOLUTION = r"""
#[derive(Debug, PartialEq)]
pub struct Batch {
    pub id: u32,
    pub label: Option<String>,
    pub items: Vec<String>,
}

/// One line about the batch, and its items handed back for reuse. The line is
/// "<label, or #<id>>: <n> items (<k> long), longest <item>", where long means more than 3 bytes and the
/// longest item is the first of the longest ("-" when there are none). A batch without a label also gets
/// " [unlabelled]" at the end.
pub fn describe(batch: Batch) -> (String, Vec<String>) {
    let unlabelled = batch.label.is_none();
    let name = batch.label.unwrap_or(format!("#{}", batch.id));
    let longest = longest_item(&batch.items);
    let mut long = 0;
    for item in &batch.items {
        if item.len() > 3 {
            long += 1;
        }
    }
    let mut line = format!("{name}: {} items ({long} long), longest {}", batch.items.len(), longest.unwrap_or("-"));
    if unlabelled {
        line.push_str(" [unlabelled]");
    }
    (line, batch.items)
}

fn longest_item(items: &[String]) -> Option<&str> {
    let mut best: Option<&str> = None;
    for item in items {
        if best.map_or(true, |b| item.len() > b.len()) {
            best = Some(item);
        }
    }
    best
}
"""


def batch(label, items, bid=7):
    lab = "None" if label is None else f'Some("{label}".to_string())'
    its = "vec![" + ", ".join(f'"{i}".to_string()' for i in items) + "]"
    return f"Batch {{ id: {bid}, label: {lab}, items: {its} }}"


def batch_desc(label, items, bid=7):
    return f"id {bid}, label {'None' if label is None else repr(label)}, items {items}".replace("'", '"')


def batch_case(name, label, items, line, bid=7):
    return T(name, batch_desc(label, items, bid), f"describe({batch(label, items, bid)}).0", f'"{line}"')


P.append(fix(
    "fix-use-after-move", "Fix: use after move", "easy", "moves-and-copy", ["E0382", "move", "partial moves"],
    """
        `describe` doesn't compile: it uses parts of `batch` after moving them. Fix it without cloning
        anything. The items must come back to the caller as the same `Vec`, not a copy.

        Some of the moves in `describe` are fine as they are. Change only the ones the compiler rejects.
    """,
    BATCH_STARTER,
    BATCH_SOLUTION,
    [batch_case("labelled", "nightly", ["build", "api", "ship"], "nightly: 3 items (2 long), longest build"),
     batch_case("unlabelled", None, ["a", "bbbb"], "#7: 2 items (1 long), longest bbbb [unlabelled]"),
     batch_case("no_items", "empty", [], "empty: 0 items (0 long), longest -"),
     batch_case("tie_first_wins", "t", ["abcd", "wxyz"], "t: 2 items (2 long), longest abcd"),
     T("items_come_back", "the returned items are the batch's own Vec", "(items, items_ptr == ptr)", '(vec!["x".to_string(), "yy".to_string()], true)',
       setup='let b = Batch { id: 1, label: None, items: vec!["x".to_string(), "yy".to_string()] };\nlet ptr = b.items.as_ptr();\nlet (_, items) = describe(b);\nlet items_ptr = items.as_ptr();')],
    [batch_case("hash_label", "#ops", ["x"], "#ops: 1 items (0 long), longest x"),
     batch_case("empty_label", "", ["abc"], ": 1 items (0 long), longest abc"),
     batch_case("exactly_three_bytes", "l", ["abc", "abcd"], "l: 2 items (1 long), longest abcd"),
     batch_case("longest_last", None, ["a", "bb", "ccc"], "#9: 3 items (0 long), longest ccc [unlabelled]", bid=9),
     batch_case("three_way_tie", "tie", ["aa", "bb", "cc"], "tie: 3 items (0 long), longest aa"),
     batch_case("unicode_bytes", "u", ["ab", "é日"], "u: 2 items (1 long), longest é日"),
     batch_case("unlabelled_empty", None, [], "#0: 0 items (0 long), longest - [unlabelled]", bid=0),
     T("items_unchanged", "items [\"b\", \"a\", \"b\"] come back in order", 'describe(Batch { id: 2, label: None, items: vec!["b".into(), "a".into(), "b".into()] }).1', 'vec!["b", "a", "b"]'),
     T("dash_item", "an item that is \"-\"", 'describe(Batch { id: 3, label: Some("d".into()), items: vec!["-".into()] }).0', '"d: 1 items (0 long), longest -"'),
     r"""
     #[test]
     fn random_vs_brute_force() {
         let mut rng = anneal_prelude::Rng::new(6101);
         for _ in 0..300 {
             let n = rng.below(6);
             let mut items = Vec::new();
             for _ in 0..n {
                 let len = rng.below(6);
                 items.push(rng.string(len, "ab#é"));
             }
             let label = if rng.bool() { Some(rng.string(2, "#x")) } else { None };
             let id = rng.below(100) as u32;
             let mut best: Option<&str> = None;
             for it in &items {
                 if best.map_or(true, |b| it.len() > b.len()) {
                     best = Some(it);
                 }
             }
             let long = items.iter().filter(|i| i.len() > 3).count();
             let name = label.clone().unwrap_or(format!("#{id}"));
             let mut want = format!("{name}: {} items ({long} long), longest {}", items.len(), best.unwrap_or("-"));
             if label.is_none() {
                 want.push_str(" [unlabelled]");
             }
             let input = format!("id {id}, label {label:?}, items {items:?}");
             let (line, back) = describe(Batch { id, label, items: items.clone() });
             check!(input, (line, back), (want, items));
         }
     }

     #[test]
     fn many_items() {
         let items: Vec<String> = (0..100_000).map(|i| if i == 77_777 { "longest!".to_string() } else { "ab".to_string() }).collect();
         let ptr = items.as_ptr();
         let (line, back) = describe(Batch { id: 1, label: Some("big".into()), items });
         check!("100000 items, one of 8 bytes", (line, back.as_ptr() == ptr), ("big: 100000 items (1 long), longest longest!".to_string(), true));
     }
     """],
    [("rust", "Three things take ownership here: a method that takes `self`, passing to a function, and a `for` loop over a `Vec`. Which of them does the code need to own?"),
     ("rust", "Moving `batch.label` out doesn't stop you using `batch.items` or `batch.id`: that's a partial move. What you can't do is read `batch.label` again afterwards.")],
    ("""`for item in v` calls `IntoIterator::into_iter(v)`, which takes the `Vec`; `for item in &v` borrows it. `Option::unwrap_or` takes `self`, so read `is_none()` first. `longest_item` only reads, so it should borrow and return `Option<&str>` pointing into the items. Moving `batch.label` out is a partial move: `batch.id` (a `Copy` field) and `batch.items` stay usable, and the final `(line, batch.items)` moves the other field out.

Aside: `unwrap_or(format!(..))` builds the fallback string even when there's a label; `unwrap_or_else(|| format!(..))` only when needed.""", "O(n)", "O(1) extra"),
    "`longest_item` could also stay `Vec<String> -> Option<String>` if `describe` called it last. What would that cost the caller?",
    ["`for x in vec` moves the Vec; `for x in &vec` borrows it.", "Methods taking `self` (like `unwrap_or`) move out of the field.", "Partial moves: other fields stay usable after one field is moved out."],
    rules=dict(methods=["clone", "cloned", "to_owned", "to_vec"], lines=10),
    related=("L2", "S1"),
    wrong=dict(
        last_on_tie=sub(BATCH_SOLUTION, "item.len() > b.len()", "item.len() >= b.len()"),
        long_is_at_least_three=sub(BATCH_SOLUTION, "if item.len() > 3 {", "if item.len() >= 3 {"),
        unlabelled_from_name=sub(sub(BATCH_SOLUTION, "    let unlabelled = batch.label.is_none();\n", ""), "if unlabelled {", "if name.starts_with('#') {"),
    ),
))

COPY_TYPES = r"""
use std::fmt;
use std::hash::{Hash, Hasher};
use std::marker::PhantomData;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Point {
    pub x: i32,
    pub y: i32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Pixel {
    pub at: Point,
    pub rgb: [u8; 3],
}

/// Owns its name, so a copy of a sprite needs its own name.
#[derive(Debug, Clone, PartialEq)]
pub struct Sprite {
    pub name: String,
    pub at: Point,
}

/// A typed index: an `Id<Sprite>` can't be passed where an `Id<Pixel>` is expected. Whatever `T` is, it's one u32.
pub struct Id<T> {
    pub raw: u32,
    marker: PhantomData<T>,
}

impl<T> Id<T> {
    pub fn new(raw: u32) -> Self {
        Id { raw, marker: PhantomData }
    }
}

// derive(Clone, Copy, PartialEq, ...) would add `T: Clone`, `T: Copy`, ... to each impl. An Id is only a u32,
// so these impls ask nothing of T.
impl<T> Clone for Id<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T> Copy for Id<T> {}

impl<T> PartialEq for Id<T> {
    fn eq(&self, other: &Self) -> bool {
        self.raw == other.raw
    }
}

impl<T> Eq for Id<T> {}

impl<T> Hash for Id<T> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.raw.hash(state);
    }
}

impl<T> fmt::Debug for Id<T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        DEBUG_BODY
    }
}
"""

COPY_TAIL = r"""
// Nothing below needs to change.

/// `n` pixels: `p` moved 0, 1, ..., n - 1 to the right.
pub fn trail(p: Pixel, n: i32) -> Vec<Pixel> {
    (0..n).map(|i| Pixel { at: Point { x: p.at.x + i, y: p.at.y }, rgb: p.rgb }).collect()
}

/// The sprite, and a copy of it placed at `to`.
pub fn fork(s: &Sprite, to: Point) -> (Sprite, Sprite) {
    let mut copy = s.clone();
    copy.at = to;
    (s.clone(), copy)
}

/// The ids in `a` that also appear in `b`, in `a`'s order.
pub fn common<T>(a: &[Id<T>], b: &[Id<T>]) -> Vec<Id<T>> {
    a.iter().filter(|x| b.contains(x)).copied().collect()
}
"""

COPY_STARTER = r"""
use std::marker::PhantomData;

pub struct Point {
    pub x: i32,
    pub y: i32,
}

pub struct Pixel {
    pub at: Point,
    pub rgb: [u8; 3],
}

/// Owns its name, so a copy of a sprite needs its own name.
pub struct Sprite {
    pub name: String,
    pub at: Point,
}

/// A typed index: an `Id<Sprite>` can't be passed where an `Id<Pixel>` is expected. Whatever `T` is, it's one u32.
pub struct Id<T> {
    pub raw: u32,
    marker: PhantomData<T>,
}

impl<T> Id<T> {
    pub fn new(raw: u32) -> Self {
        Id { raw, marker: PhantomData }
    }
}
""" + COPY_TAIL

COPY_SOLUTION = COPY_TYPES.replace("DEBUG_BODY", 'write!(f, "Id({})", self.raw)') + COPY_TAIL

OPAQUE = """
/// No derives at all.
struct Opaque;
"""

P.append(fix(
    "fix-small-struct-copy", "Fix: Copy, Clone and what derive asks for", "easy", "moves-and-copy", ["Copy", "Clone", "derive", "PhantomData"],
    """
        None of these types derive anything, so the code below them and the tests don't compile. Give each type
        what it should have:

        - `Point` and `Pixel` are plain data: they should be `Copy`, and comparable and printable with `{:?}`.
        - `Sprite` owns a `String`: clonable (a deep copy), comparable, printable.
        - `Id<T>` should be `Copy`, comparable with `==`, usable as a `HashSet` key, and print as `Id(7)`, **for
          every `T`**, including a type with no derives at all.
    """,
    COPY_STARTER,
    COPY_SOLUTION,
    [OPAQUE,
     T("pixel_reused", "trail(p, 3) with p = (1, 2), then p again", "(trail(p, 3), p)",
       "(vec![Pixel { at: Point { x: 1, y: 2 }, rgb: [9, 9, 9] }, Pixel { at: Point { x: 2, y: 2 }, rgb: [9, 9, 9] }, Pixel { at: Point { x: 3, y: 2 }, rgb: [9, 9, 9] }], Pixel { at: Point { x: 1, y: 2 }, rgb: [9, 9, 9] })",
       setup="let p = Pixel { at: Point { x: 1, y: 2 }, rgb: [9, 9, 9] };"),
     T("fork_is_deep", "fork a sprite named \"hero\" to (5, 5)", "(a.name == b.name, a.name.as_ptr() != b.name.as_ptr(), b.at)", "(true, true, Point { x: 5, y: 5 })",
       setup='let s = Sprite { name: "hero".to_string(), at: Point { x: 0, y: 0 } };\nlet (a, b) = fork(&s, Point { x: 5, y: 5 });'),
     T("common_opaque_ids", "a = [1, 2, 3], b = [3, 1] as Id<Opaque>", "common(&a, &b)", "vec![Id::new(1), Id::new(3)]",
       setup="let a: Vec<Id<Opaque>> = vec![Id::new(1), Id::new(2), Id::new(3)];\nlet b: Vec<Id<Opaque>> = vec![Id::new(3), Id::new(1)];"),
     T("id_debug", "format!(\"{:?}\", Id::<Opaque>::new(7))", 'format!("{:?}", Id::<Opaque>::new(7))', '"Id(7)"'),
     T("id_in_hash_set", "Id<Opaque> 1, 2, 1 into a HashSet", "set.len()", "2",
       setup="let set: std::collections::HashSet<Id<Opaque>> = [Id::new(1), Id::new(2), Id::new(1)].into_iter().collect();")],
    [OPAQUE,
     """
     fn is_copy<T: Copy>(_: &T) -> bool {
         true
     }

     #[test]
     fn copy_types() {
         let id: Id<String> = Id::new(1);
         check!("Point, Pixel and Id<String> are Copy", (is_copy(&Point { x: 0, y: 0 }), is_copy(&Pixel { at: Point { x: 0, y: 0 }, rgb: [0; 3] }), is_copy(&id)), (true, true, true));
     }

     #[test]
     fn id_used_after_move() {
         let id: Id<Opaque> = Id::new(4);
         let v = vec![id, id];
         check!("vec![id, id], then id", (v, id), (vec![Id::new(4), Id::new(4)], Id::new(4)));
     }
     """,
     T("trail_empty", "trail(p, 0)", "trail(Pixel { at: Point { x: 0, y: 0 }, rgb: [1, 2, 3] }, 0)", "Vec::<Pixel>::new()"),
     T("point_debug", "format!(\"{:?}\", Point { x: -1, y: 2 })", 'format!("{:?}", Point { x: -1, y: 2 })', '"Point { x: -1, y: 2 }"'),
     T("fork_keeps_original", "fork: the first sprite is unchanged", "a", 'Sprite { name: "npc".to_string(), at: Point { x: 3, y: 4 } }',
       setup='let s = Sprite { name: "npc".to_string(), at: Point { x: 3, y: 4 } };\nlet (a, _) = fork(&s, Point { x: 0, y: 0 });'),
     T("sprite_clone_independent", "clone, then push to the clone's name", "(s.name, c.name)", '("orc".to_string(), "orc!".to_string())',
       setup='let s = Sprite { name: "orc".to_string(), at: Point { x: 0, y: 0 } };\nlet mut c = s.clone();\nc.name.push(\'!\');'),
     T("common_none", "a = [1], b = [2]", "common(&[Id::<Opaque>::new(1)], &[Id::new(2)])", "Vec::<Id<Opaque>>::new()"),
     T("common_keeps_duplicates", "a = [5, 5, 6], b = [5]", "common(&[Id::<Opaque>::new(5), Id::new(5), Id::new(6)], &[Id::new(5)])", "vec![Id::new(5), Id::new(5)]"),
     T("id_eq_by_raw", "Id::<Opaque>::new(3) == Id::new(3), != Id::new(4)", "(Id::<Opaque>::new(3) == Id::new(3), Id::<Opaque>::new(3) != Id::new(4))", "(true, true)"),
     T("id_debug_in_vec", "format!(\"{:?}\", vec![Id::<Sprite>::new(0), Id::new(42)])", 'format!("{:?}", vec![Id::<Sprite>::new(0), Id::new(42)])', '"[Id(0), Id(42)]"'),
     r"""
     #[test]
     fn random_vs_brute_force() {
         let mut rng = anneal_prelude::Rng::new(6102);
         for _ in 0..300 {
             let (x, y, n) = (rng.int(-50, 50) as i32, rng.int(-50, 50) as i32, rng.below(5) as i32);
             let rgb: Vec<u8> = rng.vec(3, 0, 255);
             let p = Pixel { at: Point { x, y }, rgb: [rgb[0], rgb[1], rgb[2]] };
             let want: Vec<Pixel> = (0..n).map(|i| Pixel { at: Point { x: x + i, y }, rgb: p.rgb }).collect();
             check!(format!("trail({p:?}, {n})"), trail(p, n), want);
             let la = rng.below(6);
             let lb = rng.below(6);
             let a: Vec<Id<Opaque>> = rng.vec::<u32>(la, 0, 5).into_iter().map(Id::new).collect();
             let b: Vec<Id<Opaque>> = rng.vec::<u32>(lb, 0, 5).into_iter().map(Id::new).collect();
             let want: Vec<Id<Opaque>> = a.iter().filter(|x| b.iter().any(|y| y.raw == x.raw)).map(|x| Id::new(x.raw)).collect();
             let set: std::collections::HashSet<Id<Opaque>> = a.iter().copied().collect();
             let distinct = { let mut r: Vec<u32> = a.iter().map(|x| x.raw).collect(); r.sort(); r.dedup(); r.len() };
             check!(format!("common({a:?}, {b:?}), distinct in a"), (common(&a, &b), set.len()), (want, distinct));
         }
     }
     """],
    [("rust", "`Copy` needs `Clone` as well, and every field must be `Copy`. A `String` field rules it out."),
     ("rust", "`#[derive(Clone, Copy)]` on `Id<T>` generates `impl<T: Clone> Clone for Id<T>` and `impl<T: Copy> Copy for Id<T>`, so `Id<Opaque>` wouldn't be either. Write those impls yourself, with no bound on `T`.")],
    ("""`Copy` is a promise that a bitwise copy is a complete copy: every field must be `Copy` (so no `String`, `Vec` or `Box`), and the type can't implement `Drop` (E0184), since two copies would both run it. `Copy` also requires `Clone`, whose impl for a `Copy` type should just be `*self`.

Derives add a bound on every type parameter, whether or not a field uses it. `PhantomData<T>` makes that visible: an `Id<T>` is only a `u32`, yet `derive(Copy)` would make `Id<Opaque>` non-`Copy`. Hand-written impls with no bound fix it.

Syntax to remember: `impl<T> Clone for Id<T> { fn clone(&self) -> Self { *self } }` · `impl<T> Copy for Id<T> {}` · `impl<T> Hash for Id<T> { fn hash<H: Hasher>(&self, state: &mut H) { self.raw.hash(state) } }` · `impl<T> fmt::Debug for Id<T> { fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result { write!(f, "Id({})", self.raw) } }`.""", "O(1)", "O(1)"),
    "Why can't a type be both `Copy` and `Drop`? What would go wrong if it could?",
    ["`Copy` needs `Clone` and all-`Copy` fields; `String` fields rule it out.", "Derives bound every type parameter; hand-written impls don't have to.", "`Copy` and `Drop` are mutually exclusive (E0184)."],
    related=("L5", "S8"),
    wrong=dict(
        debug_like_derive=COPY_TYPES.replace("DEBUG_BODY", 'f.debug_struct("Id").field("raw", &self.raw).finish()') + COPY_TAIL,
        debug_raw_only=COPY_TYPES.replace("DEBUG_BODY", 'write!(f, "{}", self.raw)') + COPY_TAIL,
    ),
))

RUNS_DOC = r"""
/// Groups consecutive records with the same key, and counts the records:
/// [("a", 1), ("a", 2), ("b", 3), ("a", 4)] → ([("a", [1, 2]), ("b", [3]), ("a", [4])], 4).
"""

RUNS_STARTER = RUNS_DOC + r"""pub fn group_runs(records: Vec<(String, u32)>) -> (Vec<(String, Vec<u32>)>, usize) {
    let mut groups = Vec::new();
    let mut key = String::new();
    let mut run = Vec::new();
    for (k, v) in records {
        if k != key && !run.is_empty() {
            groups.push((key, run));
            run.clear();
        }
        key = k;
        run.push(v);
    }
    if !run.is_empty() {
        groups.push((key, run));
    }
    (groups, records.len())
}
"""

RUNS_SOLUTION = RUNS_DOC + r"""pub fn group_runs(records: Vec<(String, u32)>) -> (Vec<(String, Vec<u32>)>, usize) {
    let count = records.len();
    let mut groups = Vec::new();
    let mut key = String::new();
    let mut run = Vec::new();
    for (k, v) in records {
        if k != key && !run.is_empty() {
            // `key` may move: it's assigned again below before anything reads it.
            groups.push((key, std::mem::take(&mut run)));
        }
        key = k;
        run.push(v);
    }
    if !run.is_empty() {
        groups.push((key, run));
    }
    (groups, count)
}
"""


def recs(pairs):
    return "vec![" + ", ".join(f'("{k}".to_string(), {v})' for k, v in pairs) + "]"


def recs_desc(pairs):
    return "[" + ", ".join(f'("{k}", {v})' for k, v in pairs) + "]"


def groups(gs, n):
    return "(vec![" + ", ".join(f'("{k}".to_string(), vec!{vs})' for k, vs in gs) + f"], {n})"


def runs_case(name, pairs, gs):
    return T(name, recs_desc(pairs), f"group_runs({recs(pairs)})", groups(gs, len(pairs)))


P.append(fix(
    "fix-moved-in-loop", "Fix: values moved in a loop", "easy", "moves-and-copy", ["E0382", "loops", "mem::take"],
    """
        `group_runs` doesn't compile: values are moved inside the loop and then used again, in the same
        iteration or the next one. Fix it without cloning and without building any `Vec` twice.
    """,
    RUNS_STARTER,
    RUNS_SOLUTION,
    [runs_case("example", [("a", 1), ("a", 2), ("b", 3), ("a", 4)], [("a", [1, 2]), ("b", [3]), ("a", [4])]),
     runs_case("empty", [], []),
     runs_case("one_record", [("x", 9)], [("x", [9])]),
     runs_case("all_same_key", [("k", 1), ("k", 2), ("k", 3)], [("k", [1, 2, 3])]),
     runs_case("all_different", [("a", 1), ("b", 2), ("c", 3)], [("a", [1]), ("b", [2]), ("c", [3])])],
    [runs_case("empty_key", [("", 1), ("", 2), ("a", 3)], [("", [1, 2]), ("a", [3])]),
     runs_case("empty_key_after_other", [("a", 1), ("", 2)], [("a", [1]), ("", [2])]),
     runs_case("alternating", [("a", 1), ("b", 2), ("a", 3), ("b", 4)], [("a", [1]), ("b", [2]), ("a", [3]), ("b", [4])]),
     runs_case("repeated_values", [("a", 5), ("a", 5), ("b", 5)], [("a", [5, 5]), ("b", [5])]),
     runs_case("case_sensitive", [("a", 1), ("A", 2)], [("a", [1]), ("A", [2])]),
     runs_case("long_last_run", [("a", 1), ("b", 2), ("b", 3), ("b", 4)], [("a", [1]), ("b", [2, 3, 4])]),
     runs_case("unicode_keys", [("é", 1), ("é", 2), ("e", 3)], [("é", [1, 2]), ("e", [3])]),
     T("extreme_values", "[(\"m\", 0), (\"m\", u32::MAX)]", 'group_runs(vec![("m".to_string(), 0), ("m".to_string(), u32::MAX)])', '(vec![("m".to_string(), vec![0, u32::MAX])], 2)'),
     r"""
     #[test]
     fn keys_are_not_copied() {
         let records: Vec<(String, u32)> = vec![("left".to_string(), 1), ("right".to_string(), 2)];
         let ptrs: Vec<*const u8> = records.iter().map(|(k, _)| k.as_ptr()).collect();
         let (groups, _) = group_runs(records);
         let got: Vec<*const u8> = groups.iter().map(|(k, _)| k.as_ptr()).collect();
         check!("[(\"left\", 1), (\"right\", 2)]: each key is the record's own String", got == ptrs, true);
     }

     #[test]
     fn random_vs_brute_force() {
         let mut rng = anneal_prelude::Rng::new(6103);
         for _ in 0..300 {
             let n = rng.below(10);
             let mut records = Vec::new();
             for _ in 0..n {
                 let len = rng.below(2);
                 records.push((rng.string(len, "ab"), rng.below(10) as u32));
             }
             let mut want: Vec<(String, Vec<u32>)> = Vec::new();
             for (k, v) in &records {
                 match want.last_mut() {
                     Some((last, vs)) if last == k => vs.push(*v),
                     _ => want.push((k.clone(), vec![*v])),
                 }
             }
             check!(format!("records = {records:?}"), group_runs(records.clone()), (want, n));
         }
     }

     #[test]
     fn many_records() {
         let records: Vec<(String, u32)> = (0..200_000u32).map(|i| (format!("k{}", i / 4), i)).collect();
         let (groups, n) = group_runs(records);
         check!("200000 records, 4 per key", (groups.len(), n, groups[49_999].1.clone()), (50_000, 200_000, vec![199_996, 199_997, 199_998, 199_999]));
     }
     """],
    [("rust", "After `groups.push((key, run))`, both are gone. `key` gets a new value on the next line, so that's fine. `run` is used again. What could you push instead, that leaves an empty `Vec` behind?"),
     ("rust", "`for (k, v) in records` consumes `records`. Read what you need from it before the loop.")],
    ("""A moved-from variable can't be read, but it can be assigned again: `key = k` re-initialises `key`, so moving it into `groups` is fine. `run` is read after the move, so push `std::mem::take(&mut run)` instead: it moves the full `Vec` out and leaves `Vec::new()`, which doesn't allocate. (`run = Vec::new()` straight after the push also works: it's the same re-initialisation.) `for (k, v) in records` consumes the vector, so count first.

Syntax to remember: `std::mem::take(&mut v)` (needs `Default`) · `std::mem::replace(&mut v, new)` (any type).""", "O(n)", "O(n) for the output"),
    "Why does `run.drain(..).collect()` compile but cost more than `mem::take(&mut run)`?",
    ["A moved-from variable can be assigned again, but not read.", "`mem::take` moves a value out of a `&mut` place and leaves the default behind.", "`for x in vec` consumes the vector."],
    rules=dict(methods=["clone", "cloned", "to_owned", "to_vec", "collect", "to_string"]),
    related=("S3",),
    wrong=dict(
        empty_first_group=sub(RUNS_SOLUTION, "if k != key && !run.is_empty() {", "if k != key {").replace("    if !run.is_empty() {\n        groups.push((key, run));\n    }", "    groups.push((key, run));"),
        counts_groups=sub(RUNS_SOLUTION, "    (groups, count)", "    let n = groups.len();\n    (groups, n)"),
        drops_last_run=sub(RUNS_SOLUTION, "    if !run.is_empty() {\n        groups.push((key, run));\n    }\n", ""),
    ),
))

OUTBOX_HEAD = r"""
/// Messages waiting to be sent, up to a byte limit.
pub struct Outbox {
    msgs: Vec<String>,
    limit: usize,
    used: usize,
}
"""

OUTBOX_SOLUTION = OUTBOX_HEAD + r"""
impl Outbox {
    /// An empty outbox with no limit.
    pub fn new() -> Self {
        Outbox { msgs: Vec::new(), limit: usize::MAX, used: 0 }
    }

    /// Builder: the same outbox, limited to `bytes` bytes of messages in total.
    pub fn limit(mut self, bytes: usize) -> Self {
        self.limit = bytes;
        self
    }

    /// Bytes still free.
    pub fn remaining(&self) -> usize {
        self.limit - self.used
    }

    /// Queues `msg` if it fits in the bytes left. If it doesn't, the caller gets the same `String` back.
    pub fn push(&mut self, msg: String) -> Result<(), String> {
        if msg.len() > self.remaining() {
            return Err(msg);
        }
        self.used += msg.len();
        self.msgs.push(msg);
        Ok(())
    }

    /// Queues `raw` as a UTF-8 message. If it isn't valid UTF-8, or doesn't fit, the caller gets the same
    /// bytes back.
    pub fn push_bytes(&mut self, raw: Vec<u8>) -> Result<(), Vec<u8>> {
        let msg = String::from_utf8(raw).map_err(|e| e.into_bytes())?;
        self.push(msg).map_err(String::into_bytes)
    }

    /// Consumes the outbox and hands over its messages, oldest first.
    pub fn into_messages(self) -> Vec<String> {
        self.msgs
    }
}
"""

OUTBOX_STARTER = OUTBOX_HEAD + r"""
impl Outbox {
    /// An empty outbox with no limit.
    pub fn new() -> Self {
        todo!()
    }

    /// Builder: the same outbox, limited to `bytes` bytes of messages in total.
    pub fn limit(self, bytes: usize) -> Self {
        todo!()
    }

    /// Bytes still free.
    pub fn remaining(&self) -> usize {
        todo!()
    }

    /// Queues `msg` if it fits in the bytes left. If it doesn't, the caller gets the same `String` back.
    pub fn push(&mut self, msg: String) -> Result<(), String> {
        todo!()
    }

    /// Queues `raw` as a UTF-8 message. If it isn't valid UTF-8, or doesn't fit, the caller gets the same
    /// bytes back.
    pub fn push_bytes(&mut self, raw: Vec<u8>) -> Result<(), Vec<u8>> {
        todo!()
    }

    /// Consumes the outbox and hands over its messages, oldest first.
    pub fn into_messages(self) -> Vec<String> {
        todo!()
    }
}
"""

P.append(write(
    "ownership-round-trip", "Hand ownership back on failure", "easy", "moves-and-copy", ["ownership", "builder", "Result", "FromUtf8Error"],
    """
        Write `Outbox`, a queue of messages with a byte limit. Values move in and out of it; nothing is ever
        copied:

        - `Outbox::new().limit(10)` is a builder: `limit` takes the outbox by value and returns it.
        - `push(msg)` queues `msg` if its length in bytes fits in what's left. If it doesn't fit, the `Err` holds
          the caller's own `String`, so they can retry without having lost it.
        - `push_bytes(raw)` does the same for raw bytes: invalid UTF-8 or a message that doesn't fit gives the
          caller's own `Vec<u8>` back. A valid one is queued as a `String` that reuses the same buffer.
        - `into_messages` consumes the outbox and returns its messages, oldest first.
    """,
    OUTBOX_STARTER,
    OUTBOX_SOLUTION,
    [T("builder_and_push", "new().limit(10), push \"hello\" and \"world\"", "(ok, out.remaining(), out.into_messages())", '((Ok(()), Ok(())), 0, vec!["hello".to_string(), "world".to_string()])',
       setup='let mut out = Outbox::new().limit(10);\nlet ok = (out.push("hello".to_string()), out.push("world".to_string()));'),
     T("rejected_string_comes_back", "limit 3, push \"toolong\"", "(err, same, out.remaining())", '(Err("toolong".to_string()), true, 3)',
       setup='let mut out = Outbox::new().limit(3);\nlet msg = String::from("toolong");\nlet ptr = msg.as_ptr();\nlet err = out.push(msg);\nlet same = err.as_ref().err().map(|m| m.as_ptr()) == Some(ptr);'),
     T("invalid_utf8_comes_back", "push_bytes([0xff, b'a'])", "(err, same)", "(Err(vec![0xff, b'a']), true)",
       setup="let mut out = Outbox::new();\nlet raw = vec![0xff, b'a'];\nlet ptr = raw.as_ptr();\nlet err = out.push_bytes(raw);\nlet same = err.as_ref().err().map(|b| b.as_ptr()) == Some(ptr);"),
     T("valid_bytes_reuse_buffer", "push_bytes(b\"hi\"), then into_messages", "(ok, msgs[0].as_str(), msgs[0].as_ptr() == ptr)", '(Ok(()), "hi", true)',
       setup='let mut out = Outbox::new();\nlet raw = b"hi".to_vec();\nlet ptr = raw.as_ptr();\nlet ok = out.push_bytes(raw);\nlet msgs = out.into_messages();'),
     T("exactly_fits", "limit 5, push \"abc\" then \"de\" then \"f\"", "(out.push(\"abc\".into()), out.push(\"de\".into()), out.push(\"f\".into()))", '(Ok(()), Ok(()), Err("f".to_string()))',
       setup="let mut out = Outbox::new().limit(5);")],
    [T("no_limit", "new(), push 10000 bytes", "(out.push(\"x\".repeat(10_000)), out.remaining() > 1_000_000)", "(Ok(()), true)", setup="let mut out = Outbox::new();"),
     T("limit_zero_empty_message", "limit 0, push \"\" then \"a\"", '(out.push(String::new()), out.push("a".into()))', '(Ok(()), Err("a".to_string()))', setup="let mut out = Outbox::new().limit(0);"),
     T("reject_keeps_budget", "limit 4, push \"abcde\" (rejected) then \"abcd\"", '(out.push("abcde".into()), out.push("abcd".into()), out.remaining())', '(Err("abcde".to_string()), Ok(()), 0)', setup="let mut out = Outbox::new().limit(4);"),
     T("bytes_not_chars", "limit 4, push \"日本\" (6 bytes) then \"é\" (2 bytes)", '(out.push("日本".into()), out.push("é".into()), out.remaining())', '(Err("日本".to_string()), Ok(()), 2)', setup="let mut out = Outbox::new().limit(4);"),
     T("valid_but_too_big_bytes_come_back", "limit 1, push_bytes(b\"ok\")", "(err, same)", '(Err(b"ok".to_vec()), true)',
       setup='let mut out = Outbox::new().limit(1);\nlet raw = b"ok".to_vec();\nlet ptr = raw.as_ptr();\nlet err = out.push_bytes(raw);\nlet same = err.as_ref().err().map(|b| b.as_ptr()) == Some(ptr);'),
     T("truncated_utf8", "push_bytes of \"é\" missing its last byte", "out.push_bytes(vec![0xc3])", "Err(vec![0xc3])", setup="let mut out = Outbox::new();"),
     T("order_kept", "push \"a\", push_bytes(b\"b\"), push \"c\"", 'out.into_messages()', 'vec!["a", "b", "c"]',
       setup='let mut out = Outbox::new();\nout.push("a".into()).unwrap();\nout.push_bytes(b"b".to_vec()).unwrap();\nout.push("c".into()).unwrap();'),
     T("messages_are_the_pushed_strings", "into_messages returns the pushed String itself", "msgs[0].as_ptr() == ptr", "true",
       setup='let mut out = Outbox::new();\nlet s = String::from("mine");\nlet ptr = s.as_ptr();\nout.push(s).unwrap();\nlet msgs = out.into_messages();'),
     T("empty_outbox", "new().into_messages()", "Outbox::new().into_messages()", "Vec::<String>::new()"),
     r"""
     #[test]
     fn random_vs_model() {
         let mut rng = anneal_prelude::Rng::new(6104);
         for _ in 0..300 {
             let limit = rng.below(12);
             let mut out = Outbox::new().limit(limit);
             let (mut used, mut kept) = (0usize, Vec::new());
             let mut ops = Vec::new();
             for _ in 0..rng.below(8) {
                 let len = rng.below(5);
                 let mut raw = rng.string(len, "aé").into_bytes();
                 if rng.below(4) == 0 && !raw.is_empty() {
                     raw.pop();
                 }
                 ops.push(format!("push_bytes({raw:?})"));
                 let fits = match std::str::from_utf8(&raw) {
                     Ok(s) if s.len() <= limit - used => Some(s.to_string()),
                     _ => None,
                 };
                 let want = match &fits {
                     Some(_) => Ok(()),
                     None => Err(raw.clone()),
                 };
                 if let Some(s) = fits {
                     used += s.len();
                     kept.push(s);
                 }
                 check!(format!("limit {limit}, {ops:?}"), out.push_bytes(raw), want);
             }
             check!(format!("limit {limit}, {ops:?}, remaining"), out.remaining(), limit - used);
             check!(format!("limit {limit}, {ops:?}, into_messages"), out.into_messages(), kept);
         }
     }

     #[test]
     fn many_messages() {
         let mut out = Outbox::new().limit(1_000_000);
         for i in 0..200_000 {
             let _ = out.push(format!("{}", i % 10));
         }
         check!("200000 one-byte messages", (out.remaining(), out.into_messages().len()), (800_000, 200_000));
     }
     """],
    [("rust", "A builder that takes `self` needs `mut self` to change a field, then returns `self`."),
     ("rust", "`String::from_utf8(v)` fails with a `FromUtf8Error` that still owns the bytes: `e.into_bytes()` gets them back. `String::into_bytes` turns a rejected message back into its bytes, also without copying.")],
    ("""Moving a `String` or `Vec` moves three words (pointer, length, capacity); the heap buffer stays put, which the tests check through `as_ptr`. When a function takes ownership and might fail, return the value in the error, as std does: `String::from_utf8` → `FromUtf8Error::into_bytes`, `Mutex::into_inner`, `mpsc::SendError(T)`. `?` then passes the owned value up after `map_err`.

Syntax to remember: `pub fn limit(mut self, bytes: usize) -> Self` · `String::from_utf8(raw).map_err(|e| e.into_bytes())?` · `self.push(msg).map_err(String::into_bytes)`.""", "O(1) per push, plus the UTF-8 check", "O(1) extra"),
    "When would you take `&mut Vec<String>` instead of taking and returning ownership?",
    ["A builder method takes `mut self` and returns `Self`.", "Return ownership inside the `Err`, so a failed call doesn't eat the caller's value.", "`from_utf8` / `into_bytes` convert without copying."],
    related=("S1", "S2"),
    wrong=dict(
        lossy_utf8=sub(OUTBOX_SOLUTION, "let msg = String::from_utf8(raw).map_err(|e| e.into_bytes())?;", "let msg = String::from_utf8_lossy(&raw).into_owned();"),
        copies_rejected_bytes=sub(OUTBOX_SOLUTION, "self.push(msg).map_err(String::into_bytes)", "self.push(msg).map_err(|m| m.as_bytes().to_vec())"),
        counts_chars=sub(sub(OUTBOX_SOLUTION, "if msg.len() > self.remaining() {", "if msg.chars().count() > self.remaining() {"), "self.used += msg.len();", "self.used += msg.chars().count();"),
        strict_limit=sub(OUTBOX_SOLUTION, "if msg.len() > self.remaining() {", "if msg.len() >= self.remaining() && !msg.is_empty() {"),
    ),
))

PARAMS_SOLUTION = r"""
/// How many words are at least `min` bytes long.
pub fn count_long(words: &[String], min: usize) -> usize {
    words.iter().filter(|w| w.len() >= min).count()
}

/// The longest word, borrowed; the last of the longest on a tie.
pub fn longest(words: &[String]) -> Option<&str> {
    words.iter().max_by_key(|w| w.len()).map(|w| w.as_str())
}

/// Uppercases every word in place.
pub fn shout_all(words: &mut [String]) {
    for w in words.iter_mut() {
        w.make_ascii_uppercase();
    }
}

/// Removes the words shorter than `min` bytes, keeping the order of the rest.
pub fn drop_short(words: &mut Vec<String>, min: usize) {
    words.retain(|w| w.len() >= min);
}

/// Joins the words with spaces.
pub fn into_sentence(words: Vec<String>) -> String {
    words.join(" ")
}
"""

PARAMS_STARTER = r"""
/// How many words are at least `min` bytes long.
pub fn count_long(words: Vec<String>, min: usize) -> usize {
    words.iter().filter(|w| w.len() >= min).count()
}

/// The longest word, borrowed; the last of the longest on a tie.
pub fn longest(words: Vec<String>) -> Option<String> {
    words.into_iter().max_by_key(|w| w.len())
}

/// Uppercases every word in place.
pub fn shout_all(mut words: Vec<String>) {
    for w in words.iter_mut() {
        w.make_ascii_uppercase();
    }
}

/// Removes the words shorter than `min` bytes, keeping the order of the rest.
pub fn drop_short(mut words: Vec<String>, min: usize) {
    words.retain(|w| w.len() >= min);
}

/// Joins the words with spaces.
pub fn into_sentence(words: Vec<String>) -> String {
    words.join(" ")
}
"""


def words(ws):
    return "vec![" + ", ".join(f'"{w}".to_string()' for w in ws) + "]"


P.append(fix(
    "fix-parameter-types", "Fix: by value, & or &mut", "easy", "passing-values", ["&T", "&mut T", "&mut Vec vs &mut [T]", "API design"],
    """
        The tests call these functions one after another on the same `words`, and it doesn't compile: every
        function takes `Vec<String>` by value. Give each the parameter (and return) type that says what it does
        to the caller's words, and no more. `longest` returns `Option<&str>`.
    """,
    PARAMS_STARTER,
    PARAMS_SOLUTION,
    [r"""
     #[test]
     fn read_change_shrink_consume() {
         let mut words = vec!["hi".to_string(), "there".to_string(), "a".to_string()];
         let long = count_long(&words, 3);
         let best = longest(&words).map(str::len);
         shout_all(&mut words);
         drop_short(&mut words, 2);
         let sentence = into_sentence(words);
         check!("[\"hi\", \"there\", \"a\"]", (long, best, sentence), (1, Some(5), "HI THERE".to_string()));
     }
     """,
     T("longest_borrows", "longest of [\"ab\", \"abc\", \"xyz\"] (last on a tie)", "longest(&w)", 'Some("xyz")', setup=f"let w = {words(['ab', 'abc', 'xyz'])};"),
     T("shout_in_place", "[\"ab\", \"c\"]", "{ shout_all(&mut w); w }", 'vec!["AB", "C"]', setup=f"let mut w = {words(['ab', 'c'])};"),
     T("drop_short_keeps_order", "[\"ccc\", \"a\", \"bb\", \"dddd\"], min = 2", "{ drop_short(&mut w, 2); w }", 'vec!["ccc", "bb", "dddd"]', setup=f"let mut w = {words(['ccc', 'a', 'bb', 'dddd'])};"),
     T("count_at_least", "[\"ab\", \"abc\", \"a\"], min = 2 (at least, so \"ab\" counts)", "count_long(&w, 2)", "2", setup=f"let w = {words(['ab', 'abc', 'a'])};")],
    [T("empty", "[]", "{ shout_all(&mut w); drop_short(&mut w, 1); let best = longest(&w).map(str::len); (best, into_sentence(w)) }", "(None, String::new())", setup="let mut w: Vec<String> = vec![];"),
     T("count_min_zero", "[\"\", \"a\"], min = 0", "count_long(&w, 0)", "2", setup="let w = vec![String::new(), \"a\".to_string()];"),
     T("count_bytes_not_chars", "[\"é\", \"ab\", \"日\"], min = 3 (é is 2 bytes, 日 is 3)", "count_long(&w, 3)", "1", setup=f"let w = {words(['é', 'ab', '日'])};"),
     T("longest_bytes", "[\"日\", \"abcd\", \"ab\"] (日 is 3 bytes)", "longest(&w)", 'Some("abcd")', setup=f"let w = {words(['日', 'abcd', 'ab'])};"),
     T("longest_points_into_words", "longest(&w) is a slice of w's own String", "longest(&w).map(|s| s.as_ptr()) == Some(w[1].as_ptr())", "true", setup=f"let w = {words(['a', 'bbb'])};"),
     T("shout_mixed", "[\"aB1\", \"x-y\"]", "{ shout_all(&mut w); w }", 'vec!["AB1", "X-Y"]', setup=f"let mut w = {words(['aB1', 'x-y'])};"),
     T("drop_all", "[\"a\", \"b\"], min = 5", "{ drop_short(&mut w, 5); w }", "Vec::<String>::new()", setup=f"let mut w = {words(['a', 'b'])};"),
     T("drop_keeps_buffer", "drop_short changes the caller's Vec in place", "{ drop_short(&mut w, 2); (w.as_ptr() == ptr, w.len()) }", "(true, 1)", setup=f"let mut w = {words(['a', 'bb', 'c'])};\nlet ptr = w.as_ptr();"),
     T("sentence_empty_words", "[\"\", \"\"]", "into_sentence(vec![String::new(), String::new()])", '" ".to_string()'),
     r"""
     #[test]
     fn random_vs_brute_force() {
         let mut rng = anneal_prelude::Rng::new(6105);
         for _ in 0..300 {
             let n = rng.below(6);
             let mut words = Vec::new();
             for _ in 0..n {
                 let len = rng.below(5);
                 words.push(rng.string(len, "abé日"));
             }
             let min = rng.below(7);
             let input = format!("words = {words:?}, min = {min}");
             let want_count = words.iter().filter(|w| w.len() >= min).count();
             let mut want_best: Option<String> = None;
             for w in &words {
                 if want_best.as_ref().map_or(true, |b| w.len() >= b.len()) {
                     want_best = Some(w.clone());
                 }
             }
             let want_sentence = words.iter().filter(|w| w.len() >= min).map(|w| w.to_ascii_uppercase()).collect::<Vec<_>>().join(" ");
             let count = count_long(&words, min);
             let best = longest(&words).map(str::to_string);
             shout_all(&mut words);
             drop_short(&mut words, min);
             check!(input, (count, best, into_sentence(words)), (want_count, want_best, want_sentence));
         }
     }
     """],
    [("approach", "For each function: does it only read, change the words in place, change how many there are, or keep them?"),
     ("rust", "A slice can't grow or shrink. What does `retain` need?")],
    ("""The parameter type is the contract: `&[String]` reads, `&mut [String]` edits elements in place, `&mut Vec<String>` may also change the length (`retain`, `push`, `truncate`), and `Vec<String>` consumes. In the starter, `shout_all` and `drop_short` changed a moved-in copy that was then dropped, a silent bug the new signatures make impossible. `longest` borrows, so its result points into the caller's words (elision ties it to `words`).

Syntax to remember: `pub fn longest(words: &[String]) -> Option<&str>` · `.map(|w| w.as_str())` (or `.map(String::as_str)`).""", "O(n)", "O(1)"),
    "Why take `&mut [String]` rather than `&mut Vec<String>` when you can, and what does the caller gain?",
    ["`&[T]` reads, `&mut [T]` edits in place, `&mut Vec<T>` can change the length, `Vec<T>` consumes.", "Returning a borrow ties the result to the argument."],
    rules=dict(methods=["clone", "cloned", "to_vec", "to_owned", "to_string"]),
    related=("L2", "S3"),
    wrong=dict(
        shout_a_copy=sub(PARAMS_SOLUTION, "    for w in words.iter_mut() {\n        w.make_ascii_uppercase();\n    }", "    for w in words.iter() {\n        let _ = w.to_uppercase();\n    }"),
        counts_chars=sub(PARAMS_SOLUTION, "words.iter().filter(|w| w.len() >= min).count()", "words.iter().filter(|w| w.chars().count() >= min).count()"),
        drop_strictly_shorter=sub(PARAMS_SOLUTION, "words.retain(|w| w.len() >= min);", "words.retain(|w| w.len() > min);"),
        first_longest=sub(PARAMS_SOLUTION, "words.iter().max_by_key(|w| w.len()).map(|w| w.as_str())", "words.iter().rev().max_by_key(|w| w.len()).map(|w| w.as_str())"),
    ),
))

TAG_HEAD = r"""
/// A tag owns everything in it, so it can outlive whatever it was built from.
#[derive(Debug, PartialEq)]
pub struct Tag {
    pub name: String,
    pub aliases: Vec<String>,
    pub note: Option<String>,
}
"""

TAG_SOLUTION = TAG_HEAD + r"""
impl Tag {
    /// A caller that already owns the name hands it over; one with a `&str` pays for one copy.
    pub fn new(name: impl Into<String>, aliases: &[&str], note: Option<&str>) -> Tag {
        Tag { name: name.into(), aliases: aliases.iter().map(|a| a.to_string()).collect(), note: note.map(String::from) }
    }
}
"""

TAG_STARTER = TAG_HEAD + r"""
impl Tag {
    /// A caller that already owns the name hands it over; one with a `&str` pays for one copy.
    pub fn new(name: &str, aliases: &[&str], note: Option<&str>) -> Tag {
        Tag { name, aliases, note }
    }
}
"""

P.append(fix(
    "fix-borrowed-where-owned", "Fix: borrowed where owned was needed", "easy", "passing-values", ["E0308", "impl Into<String>", "&str to String"],
    """
        `Tag::new` doesn't compile: `Tag` owns its data, and `new` is handed borrowed data. Fix it so the tests
        compile. They build tags from string literals and from `String`s the caller already owns; an owned
        name must be moved into the tag, not copied.
    """,
    TAG_STARTER,
    TAG_SOLUTION,
    [T("from_literals", "new(\"rust\", [\"rs\"], Some(\"systems\"))", 'Tag::new("rust", &["rs"], Some("systems"))',
       'Tag { name: "rust".to_string(), aliases: vec!["rs".to_string()], note: Some("systems".to_string()) }'),
     T("owned_name_is_moved", "new(String \"go\", [], None): the tag keeps the caller's buffer", "(tag.name.as_ptr() == ptr, tag.name)", '(true, "go".to_string())',
       setup='let name = String::from("go");\nlet ptr = name.as_ptr();\nlet tag = Tag::new(name, &[], None);'),
     T("borrowed_string", "new(&String \"zig\", ...), then use the String again", '(Tag::new(&s, &[], None).name, s)', '("zig".to_string(), "zig".to_string())',
       setup='let s = String::from("zig");'),
     T("no_note", "new(\"c\", [\"clang\", \"c99\"], None)", 'Tag::new("c", &["clang", "c99"], None)',
       'Tag { name: "c".to_string(), aliases: vec!["clang".to_string(), "c99".to_string()], note: None }'),
     T("outlives_its_inputs", "tag built inside a block from local Strings", "tag",
       'Tag { name: "tmp".to_string(), aliases: vec!["t".to_string()], note: Some("n".to_string()) }',
       setup='let tag = {\n    let (n, a, note) = (String::from("tmp"), String::from("t"), String::from("n"));\n    Tag::new(n.as_str(), &[a.as_str()], Some(note.as_str()))\n};')],
    [T("empty_everything", "new(\"\", [], Some(\"\"))", 'Tag::new("", &[], Some(""))', 'Tag { name: String::new(), aliases: vec![], note: Some(String::new()) }'),
     T("aliases_in_order", "aliases [\"b\", \"a\", \"b\"]", 'Tag::new("x", &["b", "a", "b"], None).aliases', 'vec!["b", "a", "b"]'),
     T("spaces_and_case_kept", "new(\" RuSt \", [\" r \"], Some(\" N \"))", 'Tag::new(" RuSt ", &[" r "], Some(" N "))',
       'Tag { name: " RuSt ".to_string(), aliases: vec![" r ".to_string()], note: Some(" N ".to_string()) }'),
     T("unicode", "new(\"日本語\", [\"🦀\"], None)", 'Tag::new("日本語", &["🦀"], None).name + &Tag::new("日本語", &["🦀"], None).aliases[0]', '"日本語🦀".to_string()'),
     T("source_changed_later", "input String changed after new", '{ let mut s = String::from("old"); let t = Tag::new(s.as_str(), &[], None); s.push_str("er"); (t.name, s) }', '("old".to_string(), "older".to_string())'),
     T("owned_name_no_allocation", "new(String, [], None) makes no copy of the name", "(n, tag.name.capacity())", "(0, 100)",
       setup='let mut name = String::with_capacity(100);\nname.push_str("big");\nlet (tag, n) = allocs(|| Tag::new(name, &[], None));'),
     T("literal_name_one_allocation", "new(\"lit\", [], None) allocates once", "allocs(|| Tag::new(\"lit\", &[], None)).1", "1"),
     T("two_tags_one_str", "two tags from the same &str don't share a buffer", '{ let s = "same"; let (a, b) = (Tag::new(s, &[], None), Tag::new(s, &[], None)); a == b && a.name.as_ptr() != b.name.as_ptr() }', "true"),
     ALLOC_COUNTER,
     r"""
     #[test]
     fn random_vs_brute_force() {
         let mut rng = anneal_prelude::Rng::new(6106);
         for _ in 0..300 {
             let len = rng.below(8);
             let name = rng.string(len, "aZ é日🦀");
             let k = rng.below(4);
             let aliases: Vec<String> = (0..k).map(|_| { let l = rng.below(4); rng.string(l, "ab ") }).collect();
             let note = if rng.bool() { let l = rng.below(4); Some(rng.string(l, "nö")) } else { None };
             let refs: Vec<&str> = aliases.iter().map(|a| a.as_str()).collect();
             let want = Tag { name: name.clone(), aliases: aliases.clone(), note: note.clone() };
             let input = format!("name = {name:?}, aliases = {aliases:?}, note = {note:?}");
             check!(input.clone(), Tag::new(name.as_str(), &refs, note.as_deref()), want);
             check!(format!("{input}, name owned"), Tag::new(name.clone(), &refs, note.as_deref()).name, name);
         }
     }
     """],
    [("rust", "Each field is owned and each argument borrowed: something must make an owned copy of each one (`to_string`, `String::from`, `to_owned`, `into`)."),
     ("rust", "To accept a `String` without copying it, and a `&str` too, take `name: impl Into<String>` and call `.into()`. `From<String> for String` is the identity: it moves.")],
    ("""A struct that owns its data needs owned values; a `&str` field would need a lifetime and couldn't outlive its input (the test `outlives_its_inputs`). Taking `&str` and copying is fine for borrowed callers, but a caller who already owns a `String` would pay for a copy it doesn't need. `impl Into<String>` takes either: a `&str` is copied once, a `String` is moved in (the tests count allocations to check).

Syntax to remember: `name: impl Into<String>` then `name.into()` · `aliases.iter().map(|a| a.to_string()).collect()` · `note.map(String::from)` (or `note.map(str::to_owned)`). `Option<impl Into<String>>` would make a bare `None` ambiguous at the call site, which is why `note` stays `Option<&str>`.""", "O(n) in the bytes copied", "O(n)"),
    "When is `impl Into<String>` a worse choice than `&str` for a constructor parameter?",
    ["An owning struct needs owned values: convert borrowed arguments.", "`impl Into<String>` moves a `String` in and copies a `&str` once."],
    related=("S2", "L3"),
    wrong=dict(
        as_ref_copies=sub(sub(TAG_SOLUTION, "name: impl Into<String>", "name: impl AsRef<str>"), "name: name.into()", "name: name.as_ref().to_string()"),
        drops_empty_note=sub(TAG_SOLUTION, "note: note.map(String::from)", "note: note.filter(|n| !n.is_empty()).map(String::from)"),
    ),
))

STR_SOLUTION = r"""
/// The first whitespace-separated word of `s`, borrowed from `s`; "" when there is none.
pub fn first_word(s: &str) -> &str {
    s.split_whitespace().next().unwrap_or("")
}

/// The extension of the file name (the text after the last '/'), borrowed from `path`.
pub fn extension(path: &str) -> Option<&str> {
    let name = path.rsplit('/').next().unwrap_or(path);
    let (stem, ext) = name.rsplit_once('.')?;
    if stem.is_empty() {
        None
    } else {
        Some(ext)
    }
}

/// The non-empty parts, joined with `sep`.
pub fn join_nonempty<S: AsRef<str>>(parts: &[S], sep: &str) -> String {
    let kept: Vec<&str> = parts.iter().map(|p| p.as_ref()).filter(|p| !p.is_empty()).collect();
    kept.join(sep)
}

/// How many of `words` equal `needle`, ignoring ASCII case.
pub fn count_word<I>(words: I, needle: &str) -> usize
where
    I: IntoIterator,
    I::Item: AsRef<str>,
{
    words.into_iter().filter(|w| w.as_ref().eq_ignore_ascii_case(needle)).count()
}
"""

STR_STARTER = r"""
/// The first whitespace-separated word of `s`, borrowed from `s`; "" when there is none.
pub fn first_word(s: &String) -> String {
    todo!()
}

/// The extension of the file name (the text after the last '/'), borrowed from `path`.
pub fn extension(path: &String) -> Option<String> {
    todo!()
}

/// The non-empty parts, joined with `sep`.
pub fn join_nonempty(parts: &Vec<String>, sep: &str) -> String {
    todo!()
}

/// How many of `words` equal `needle`, ignoring ASCII case.
pub fn count_word(words: Vec<String>, needle: &str) -> usize {
    todo!()
}
"""

P.append(fix(
    "str-parameters", "Parameters that take &str and String alike", "easy", "passing-values", ["&str", "AsRef<str>", "IntoIterator", "deref coercion"],
    """
        These signatures only accept owned `String`s in one exact container, so the tests don't compile. Change
        each signature so callers can pass what they have, then write the bodies:

        - `first_word` and `extension` take a literal or a `&String`, and return **slices of their input**, with
          no allocation.
        - `extension`: the text after the last `.` of the file name (the part after the last `/`), unless that
          dot is the name's first character. `"a/b.tar.gz"` → `"gz"`, `"x."` → `""`, `".bashrc"` and
          `"dir.d/file"` → none.
        - `join_nonempty` takes a slice of `&str`, of `String`, or of anything else that can be read as a `str`.
        - `count_word` takes **any iterator** of string-likes, owned or borrowed: a `Vec<String>`, a `&Vec<String>`,
          or `text.split_whitespace()`.
    """,
    STR_STARTER,
    STR_SOLUTION,
    [T("first_word_slices_input", "first_word(\"  hello world\")", "(w, w.as_ptr() == s[2..].as_ptr())", '("hello", true)', setup='let s = "  hello world";\nlet w = first_word(s);'),
     T("extension_cases", "extension of \"a/b.tar.gz\", \".bashrc\", \"x.\", \"dir.d/file\"", 'owned.iter().map(|p| extension(p)).collect::<Vec<_>>()', 'vec![Some("gz"), None, Some(""), None]',
       setup='let owned: Vec<String> = ["a/b.tar.gz", ".bashrc", "x.", "dir.d/file"].iter().map(|s| s.to_string()).collect();'),
     T("join_str_and_string_slices", "join_nonempty([\"a\", \"\", \"b\"], \"-\") and of Strings [\"x\", \"y\"]", '(join_nonempty(&["a", "", "b"], "-"), join_nonempty(&owned, ", "))', '("a-b".to_string(), "x, y".to_string())',
       setup='let owned = vec!["x".to_string(), "y".to_string()];'),
     T("count_from_split", "count_word(\"The cat saw the THE\".split_whitespace(), \"the\")", 'count_word("The cat saw the THE".split_whitespace(), "the")', "3"),
     T("count_owned_and_borrowed", "count_word(&words, \"a\"), then count_word(words, \"b\")", '(count_word(&words, "a"), count_word(words, "b"))', "(2, 1)",
       setup='let words = vec!["a".to_string(), "b".to_string(), "A".to_string()];')],
    [T("first_word_empty", "first_word(\"\") and first_word(\" \\t \")", '(first_word(""), first_word(" \\t "))', '("", "")'),
     T("first_word_from_string", "first_word(&String \"one two\")", "first_word(&s)", '"one"', setup='let s = String::from("one two");'),
     T("first_word_tabs", "first_word(\"\\n\\tfoo\\tbar\")", 'first_word("\\n\\tfoo\\tbar")', '"foo"'),
     T("extension_slices_input", "extension(\"archive.zip\") points into the input", 'extension(s).map(|e| e.as_ptr() == s[8..].as_ptr())', "Some(true)", setup='let s = "archive.zip";'),
     T("extension_more", "\"..bashrc\", \"a/.b/c\", \"a/b/.git\", \"noext\", \"\", \"/\"", '["..bashrc", "a/.b/c", "a/b/.git", "noext", "", "/"].map(extension)', '[Some("bashrc"), None, None, None, None, None]'),
     T("extension_unicode", "\"日本.txt\" and \"é.日\"", '(extension("日本.txt"), extension("é.日"))', '(Some("txt"), Some("日"))'),
     T("join_all_empty", "join_nonempty([\"\", \"\"], \",\") and of []", '(join_nonempty(&["", ""], ","), join_nonempty::<&str>(&[], ","))', '(String::new(), String::new())'),
     T("join_cows", "join_nonempty of Cows [Borrowed \"a\", Owned \"b\"]", 'join_nonempty(&[Cow::Borrowed("a"), Cow::Owned("b".to_string())], "+")', '"a+b"', setup="use std::borrow::Cow;"),
     T("count_slice_of_str", "count_word([\"x\", \"X\", \"y\"].iter(), \"x\")", 'count_word(["x", "X", "y"].iter(), "x")', "2"),
     T("count_ascii_case_only", "count_word([\"É\", \"é\"], \"é\") (ASCII case only)", 'count_word(["É", "é"], "é")', "1"),
     r"""
     #[test]
     fn random_vs_brute_force() {
         let mut rng = anneal_prelude::Rng::new(6107);
         for _ in 0..400 {
             let len = rng.below(9);
             let path = rng.string(len, "ab./");
             let name = match path.rfind('/') {
                 Some(i) => &path[i + 1..],
                 None => &path[..],
             };
             let want = match name.rfind('.') {
                 Some(0) | None => None,
                 Some(i) => Some(&name[i + 1..]),
             };
             check!(format!("extension({path:?})"), extension(&path), want);
             let len = rng.below(8);
             let text = rng.string(len, "aA \t");
             let mut chars = text.char_indices().skip_while(|(_, c)| c.is_whitespace());
             let want = match chars.next() {
                 Some((i, _)) => {
                     let end = text[i..].find(char::is_whitespace).map_or(text.len(), |j| i + j);
                     &text[i..end]
                 }
                 None => "",
             };
             check!(format!("first_word({text:?})"), first_word(&text), want);
             let n = text.split(' ').filter(|w| w.eq_ignore_ascii_case("a")).count();
             check!(format!("count_word({text:?}.split(' '), \"a\")"), count_word(text.split(' '), "a"), n);
             let parts: Vec<String> = text.split(' ').map(String::from).collect();
             let want = parts.iter().filter(|p| !p.is_empty()).cloned().collect::<Vec<_>>().join("|");
             check!(format!("join_nonempty({parts:?}, \"|\")"), join_nonempty(&parts, "|"), want);
         }
     }

     #[test]
     fn long_input() {
         let text = "word ".repeat(200_000);
         check!("200000 × \"word \"", (count_word(text.split_whitespace(), "WORD"), first_word(&text).len()), (200_000, 4));
     }
     """],
    [("rust", "`&String` derefs to `&str`, but a `&str` can't become a `&String`. Take `&str`, and return `&str` slices of it: elision ties the output to the input."),
     ("rust", "For `join_nonempty`, a generic `S: AsRef<str>` accepts `&[&str]` and `&[String]`. For `count_word`, take `I: IntoIterator` with `I::Item: AsRef<str>`.")],
    ("""`&str` is the parameter for text you only read: literals are `&str`, and `&String` coerces to it. Returning a slice of the input (`split_whitespace().next()`, `rsplit_once`) costs nothing, and elision ties the result's lifetime to the parameter. For collections, `AsRef<str>` accepts both element types, and `IntoIterator` with an `AsRef<str>` item accepts owned vectors, borrowed vectors and lazy iterators alike.

Syntax to remember: `fn join_nonempty<S: AsRef<str>>(parts: &[S], sep: &str) -> String` · `fn count_word<I>(words: I, needle: &str) -> usize where I: IntoIterator, I::Item: AsRef<str>` · `name.rsplit_once('.')?`.""", "O(n)", "O(1) except the joined String"),
    "When would you take `impl AsRef<Path>` instead of `&str` for a path, and why does std do that?",
    ["`&str` parameters accept literals and `&String`; return slices of the input.", "`AsRef<str>` and `IntoIterator<Item: AsRef<str>>` accept owned and borrowed text alike."],
    related=("S2", "L5"),
    wrong=dict(
        split_last=sub(STR_SOLUTION, "    let name = path.rsplit('/').next().unwrap_or(path);\n    let (stem, ext) = name.rsplit_once('.')?;\n    if stem.is_empty() {\n        None\n    } else {\n        Some(ext)\n    }",
                       "    if !path.contains('.') {\n        return None;\n    }\n    path.split('.').last()"),
        first_word_on_space=sub(STR_SOLUTION, "s.split_whitespace().next().unwrap_or(\"\")", "s.split(' ').next().unwrap_or(\"\")"),
        keeps_empty_parts=sub(STR_SOLUTION, ".filter(|p| !p.is_empty())", ""),
        case_sensitive=sub(STR_SOLUTION, "w.as_ref().eq_ignore_ascii_case(needle)", "w.as_ref() == needle"),
    ),
))

P.append(dict(
    slug="fix-needless-clones", title="Fix: remove every clone", mode="fix", level="medium", stage="clones-and-drops", tags=["borrowing", "clone", "lifetimes"],
    teaches=["Return borrowed data (`&str`, `Vec<&str>`) instead of cloning.", "Iterate by reference instead of cloning a collection to iterate it."],
    statement="""
        `Library` works, but clones its whole book list on every call. Remove every clone:
        `search` should return `Vec<&str>` and `longest` should return `Option<&str>`.
    """,
    starter="""
        pub struct Library {
            books: Vec<String>,
        }

        impl Library {
            pub fn new(books: Vec<String>) -> Self {
                Library { books }
            }

            /// Titles containing `needle`.
            pub fn search(&self, needle: &str) -> Vec<String> {
                self.books.clone().into_iter().filter(|b| b.contains(needle)).collect()
            }

            /// The longest title; the last one on a tie.
            pub fn longest(&self) -> Option<String> {
                let mut books = self.books.clone();
                books.sort_by_key(|b| b.len());
                books.last().cloned()
            }

            pub fn count_with(&self, needle: &str) -> usize {
                self.search(&needle.to_string()).len()
            }
        }
    """,
    solution="""
        pub struct Library {
            books: Vec<String>,
        }

        impl Library {
            pub fn new(books: Vec<String>) -> Self {
                Library { books }
            }

            /// Titles containing `needle`.
            pub fn search(&self, needle: &str) -> Vec<&str> {
                self.books.iter().filter(|b| b.contains(needle)).map(String::as_str).collect()
            }

            /// The longest title; the last one on a tie.
            pub fn longest(&self) -> Option<&str> {
                self.books.iter().max_by_key(|b| b.len()).map(String::as_str)
            }

            pub fn count_with(&self, needle: &str) -> usize {
                self.books.iter().filter(|b| b.contains(needle)).count()
            }
        }
    """,
    rules=dict(methods=["clone", "cloned", "to_string", "to_owned"]),
    visible=[
        T("search", "books = [\"Dune\", \"Dune Messiah\", \"Emma\"], needle = \"Dune\"", 'lib.search("Dune")', 'vec!["Dune", "Dune Messiah"]', setup='let lib = Library::new(vec!["Dune".into(), "Dune Messiah".into(), "Emma".into()]);'),
        T("longest", "books = [\"Dune\", \"Dune Messiah\", \"Emma\"]", "lib.longest()", 'Some("Dune Messiah")', setup='let lib = Library::new(vec!["Dune".into(), "Dune Messiah".into(), "Emma".into()]);'),
        T("count_with", "books = [\"Dune\", \"Dune Messiah\", \"Emma\"], needle = \"Dune\"", 'lib.count_with("Dune")', "2", setup='let lib = Library::new(vec!["Dune".into(), "Dune Messiah".into(), "Emma".into()]);'),
        T("longest_tie_is_last", "books = [\"ab\", \"cd\", \"e\"]", "lib.longest()", 'Some("cd")', setup='let lib = Library::new(vec!["ab".into(), "cd".into(), "e".into()]);'),
        T("longest_no_books", "books = []", "lib.longest()", "None", setup="let lib = Library::new(vec![]);"),
    ],
    hidden=[
        T("longest_tie_last", "books = [\"ab\", \"cd\"]", "lib.longest()", 'Some("cd")', setup='let lib = Library::new(vec!["ab".into(), "cd".into()]);'),
        T("count", "needle = \"e\"", 'Library::new(vec!["Dune".into(), "Emma".into(), "Tess".into()]).count_with("e")', "2"),
        T("empty", "no books", "lib.longest()", "None", setup="let lib = Library::new(vec![]);"),
        T("search_empty_library", "no books, needle = \"a\"", 'lib.search("a")', "Vec::<&str>::new()", setup="let lib = Library::new(vec![]);"),
        T("search_none_match", "books = [\"Dune\", \"Emma\"], needle = \"x\"", 'lib.search("x")', "Vec::<&str>::new()", setup='let lib = Library::new(vec!["Dune".into(), "Emma".into()]);'),
        T("search_empty_needle", "books = [\"b\", \"a\"], needle = \"\"", 'lib.search("")', 'vec!["b", "a"]', setup='let lib = Library::new(vec!["b".into(), "a".into()]);'),
        T("search_middle_of_title", "books = [\"The Hobbit\", \"Hob\"], needle = \"obb\"", 'lib.search("obb")', 'vec!["The Hobbit"]', setup='let lib = Library::new(vec!["The Hobbit".into(), "Hob".into()]);'),
        T("case_sensitive", "books = [\"dune\", \"Dune\"], needle = \"D\"", '(lib.search("D"), lib.count_with("D"))', '(vec!["Dune"], 1)', setup='let lib = Library::new(vec!["dune".into(), "Dune".into()]);'),
        T("unicode", "books = [\"Café\", \"Cafe\", \"日本\"], needle = \"é\" (日本 is 6 bytes, Café 5)", '(lib.search("é"), lib.longest())', '(vec!["Café"], Some("日本"))', setup='let lib = Library::new(vec!["Café".into(), "Cafe".into(), "日本".into()]);'),
        T("longest_tie_of_three_last", "books = [\"xy\", \"a\", \"zw\", \"uv\"]", "lib.longest()", 'Some("uv")', setup='let lib = Library::new(vec!["xy".into(), "a".into(), "zw".into(), "uv".into()]);'),
        """
        #[test]
        fn random_vs_brute_force() {
            let mut rng = anneal_prelude::Rng::new(1108);
            for _ in 0..300 {
                let n = rng.below(7);
                let mut books = Vec::new();
                for _ in 0..n {
                    let len = rng.below(5);
                    books.push(rng.string(len, "abc"));
                }
                let len = rng.below(3);
                let needle = rng.string(len, "abc");
                let lib = Library::new(books.clone());
                let want_search: Vec<&str> = books.iter().filter(|b| b.contains(needle.as_str())).map(|b| b.as_str()).collect();
                let mut want_longest: Option<&str> = None;
                for b in &books {
                    if want_longest.map_or(true, |l| b.len() >= l.len()) {
                        want_longest = Some(b);
                    }
                }
                check!(
                    format!("books = {books:?}, needle = {needle:?}"),
                    (lib.search(&needle), lib.longest(), lib.count_with(&needle)),
                    (want_search.clone(), want_longest, want_search.len())
                );
            }
        }
        """,
    ],
    wrong=dict(
        first_on_tie="""
            pub struct Library {
                books: Vec<String>,
            }

            impl Library {
                pub fn new(books: Vec<String>) -> Self {
                    Library { books }
                }

                /// Titles containing `needle`.
                pub fn search(&self, needle: &str) -> Vec<&str> {
                    self.books.iter().filter(|b| b.contains(needle)).map(String::as_str).collect()
                }

                /// The longest title; the last one on a tie.
                pub fn longest(&self) -> Option<&str> {
                    let mut best: Option<&str> = None;
                    for b in &self.books {
                        if best.map_or(true, |l| b.len() > l.len()) {
                            best = Some(b);
                        }
                    }
                    best
                }

                pub fn count_with(&self, needle: &str) -> usize {
                    self.books.iter().filter(|b| b.contains(needle)).count()
                }
            }
        """,
        search_prefix="""
            pub struct Library {
                books: Vec<String>,
            }

            impl Library {
                pub fn new(books: Vec<String>) -> Self {
                    Library { books }
                }

                /// Titles containing `needle`.
                pub fn search(&self, needle: &str) -> Vec<&str> {
                    self.books.iter().filter(|b| b.starts_with(needle)).map(String::as_str).collect()
                }

                /// The longest title; the last one on a tie.
                pub fn longest(&self) -> Option<&str> {
                    self.books.iter().max_by_key(|b| b.len()).map(String::as_str)
                }

                pub fn count_with(&self, needle: &str) -> usize {
                    self.search(needle).len()
                }
            }
        """,
    ),
    hints=[("approach", "Each method only reads the books. Can it return references into `self.books`?"),
           ("rust", "`iter().max_by_key(|b| b.len())` returns the last maximum, like sort-then-last.")],
    notes=("The returned `&str`s borrow `self`, so elision ties them to `&self`. No method allocates now except `search`'s result Vec.", "O(n)", "O(k)"),
    follow_up="When is returning owned data the right call even though it costs a clone?",
    related=["L3", "S1"],
))

COW_SOLUTION = r"""
use std::borrow::Cow;

/// Strips trailing whitespace and replaces each tab with four spaces. Borrows whenever no tab is left to
/// replace, even if the end was trimmed.
pub fn normalize(s: &str) -> Cow<'_, str> {
    let trimmed = s.trim_end();
    if trimmed.contains('\t') {
        Cow::Owned(trimmed.replace('\t', "    "))
    } else {
        Cow::Borrowed(trimmed)
    }
}

/// Ends `line` with a '\n'. A line that already has one comes back untouched; an owned line gets the
/// newline pushed onto its own buffer.
pub fn with_newline(mut line: Cow<'_, str>) -> Cow<'_, str> {
    if !line.ends_with('\n') {
        line.to_mut().push('\n');
    }
    line
}
"""

COW_STARTER = r"""
use std::borrow::Cow;

/// Strips trailing whitespace and replaces each tab with four spaces. Borrows whenever no tab is left to
/// replace, even if the end was trimmed.
pub fn normalize(s: &str) -> Cow<'_, str> {
    todo!()
}

/// Ends `line` with a '\n'. A line that already has one comes back untouched; an owned line gets the
/// newline pushed onto its own buffer.
pub fn with_newline(line: Cow<'_, str>) -> Cow<'_, str> {
    todo!()
}
"""

P.append(write(
    "cow-normalizer", "Allocate only when you must: Cow", "medium", "clones-and-drops", ["Cow", "borrow or own", "clone on write"],
    """
        `normalize(s)` strips trailing whitespace and replaces each tab with four spaces (always four, not up to
        a tab stop). It must allocate only when there's a tab to replace: a line that only needs trimming
        comes back as a **borrowed slice** of `s`.

        `with_newline(line)` makes sure a line ends with `'\\n'`. A line that already does comes back as it was
        (a borrowed one stays borrowed); an owned line gets the newline added to **its own buffer**, not a copy.
    """,
    COW_STARTER,
    COW_SOLUTION,
    [T("tabs_replaced", "normalize(\"a\\tb\")", 'normalize("a\\tb")', '"a    b"'),
     T("trim_only_borrows", "normalize(\"plain   \") is a borrowed slice", 'matches!(normalize("plain   "), Cow::Borrowed("plain"))', "true", setup="use std::borrow::Cow;"),
     T("trailing_tab_is_trimmed_not_replaced", "normalize(\"x \\t\") borrows \"x\"", 'matches!(normalize("x \\t"), Cow::Borrowed("x"))', "true", setup="use std::borrow::Cow;"),
     T("newline_added", "with_newline(Borrowed(\"hi\"))", 'with_newline(Cow::Borrowed("hi"))', '"hi\\n"', setup="use std::borrow::Cow;"),
     T("newline_kept_borrowed", "with_newline(Borrowed(\"done\\n\")) stays borrowed", 'matches!(with_newline(Cow::Borrowed("done\\n")), Cow::Borrowed("done\\n"))', "true", setup="use std::borrow::Cow;")],
    [T("empty", "normalize(\"\")", 'matches!(normalize(""), Cow::Borrowed(""))', "true", setup="use std::borrow::Cow;"),
     T("only_whitespace", "normalize(\" \\t \\n\")", 'matches!(normalize(" \\t \\n"), Cow::Borrowed(""))', "true", setup="use std::borrow::Cow;"),
     T("leading_tab_kept", "normalize(\"\\tx  \")", 'normalize("\\tx  ")', '"    x"'),
     T("consecutive_tabs", "normalize(\"a\\t\\tb\")", 'normalize("a\\t\\tb")', '"a        b"'),
     T("not_tab_stops", "normalize(\"abc\\td\")", 'normalize("abc\\td")', '"abc    d"'),
     T("borrowed_points_into_input", "normalize(String \"no tabs here  \")", 'match normalize(&s) { Cow::Borrowed(b) => b.as_ptr() == s.as_ptr() && b.len() == 12, Cow::Owned(_) => false }', "true",
       setup='use std::borrow::Cow;\nlet s = String::from("no tabs here  ");'),
     T("leading_space_kept", "normalize(\"  x\")", 'normalize("  x")', '"  x"'),
     T("unicode_around_tab", "normalize(\"é\\t日 \")", 'normalize("é\\t日 ")', '"é    日"'),
     T("owned_line_same_buffer", "with_newline(Owned) pushes onto the same buffer", '{ let out = with_newline(Cow::Owned(line)); (out.as_ptr() == ptr, out.into_owned()) }', '(true, "abc\\n".to_string())',
       setup='use std::borrow::Cow;\nlet mut line = String::with_capacity(16);\nline.push_str("abc");\nlet ptr = line.as_ptr();'),
     T("owned_with_newline_untouched", "with_newline(Owned(\"x\\n\")) is the same String", '{ let out = with_newline(Cow::Owned(line)); matches!(out, Cow::Owned(_)) && out.as_ptr() == ptr }', "true",
       setup='use std::borrow::Cow;\nlet line = String::from("x\\n");\nlet ptr = line.as_ptr();'),
     T("empty_line_gets_newline", "with_newline(Borrowed(\"\"))", 'with_newline(Cow::Borrowed(""))', '"\\n"', setup="use std::borrow::Cow;"),
     T("chained", "with_newline(normalize(\"a\\t \"))", 'with_newline(normalize("a\\t "))', '"a\\n"'),
     r"""
     use std::borrow::Cow;

     #[test]
     fn random_vs_brute_force() {
         let mut rng = anneal_prelude::Rng::new(6108);
         for _ in 0..400 {
             let len = rng.below(10);
             let s = rng.string(len, "ab é\t\n");
             let trimmed = s.trim_end();
             let want: String = trimmed.chars().map(|c| if c == '\t' { "    ".to_string() } else { c.to_string() }).collect();
             let out = normalize(&s);
             let borrowed = matches!(out, Cow::Borrowed(_));
             check!(format!("normalize({s:?})"), (out.into_owned(), borrowed), (want, !trimmed.contains('\t')));
             let n = rng.below(4);
             let line = rng.string(n, "a\n");
             let ends = line.ends_with('\n');
             let out = with_newline(Cow::Borrowed(&line));
             let want = if ends { line.clone() } else { format!("{line}\n") };
             check!(format!("with_newline(Borrowed({line:?}))"), (matches!(out, Cow::Borrowed(_)), out.into_owned()), (ends, want));
         }
     }

     #[test]
     fn scale_1m_tabs() {
         let s = "\t".repeat(1_000_000) + "x";
         let out = normalize(&s);
         check!("s = 1000000 tabs then x", (out.len(), out.ends_with("    x")), (4_000_001, true));
     }
     """],
    [("rust", "`trim_end` returns a slice of the input, so trimming alone never needs to allocate. Only tabs force `Cow::Owned`."),
     ("rust", "`Cow::to_mut` gives `&mut String`: it copies a borrowed value into an owned one first, and hands an owned one over as it is.")],
    ("""`Cow<'a, str>` is either a `&'a str` or a `String`, and derefs to `&str` either way. Returning `Cow::Borrowed(trimmed)` for the common case means most lines cost nothing; only a tab forces an allocation. `to_mut` is clone-on-write: the first write to a borrowed value makes the owned copy, and writes to an owned value go straight to its buffer. Building a new `String` in `with_newline` would copy owned lines that already had room.

Syntax to remember: `fn normalize(s: &str) -> Cow<'_, str>` · `Cow::Borrowed(x)` / `Cow::Owned(y)` · `line.to_mut().push('\\n')` · `cow.into_owned()`.""", "O(n)", "O(n) only when a tab is replaced"),
    "Where does std itself return `Cow`, and why there?",
    ["`Cow` borrows in the common case and owns only when it had to change something.", "`Cow::to_mut` copies a borrowed value on first write and reuses an owned one."],
    source="W39", related=("S2", "S7"),
    wrong=dict(
        always_owned=sub(COW_SOLUTION, "    if trimmed.contains('\\t') {\n        Cow::Owned(trimmed.replace('\\t', \"    \"))\n    } else {\n        Cow::Borrowed(trimmed)\n    }",
                         "    if trimmed.len() == s.len() && !s.contains('\\t') {\n        Cow::Borrowed(s)\n    } else {\n        Cow::Owned(trimmed.replace('\\t', \"    \"))\n    }"),
        trims_spaces_only=sub(COW_SOLUTION, "let trimmed = s.trim_end();", "let trimmed = s.trim_end_matches(' ');"),
        new_string_for_newline=sub(COW_SOLUTION, "        line.to_mut().push('\\n');\n    }\n    line", "        return Cow::Owned(format!(\"{line}\\n\"));\n    }\n    line"),
        trims_both_ends=sub(COW_SOLUTION, "let trimmed = s.trim_end();", "let trimmed = s.trim();"),
    ),
))

DROP_SCENE = r"""
use std::cell::RefCell;

pub struct Noisy<'a> {
    pub name: &'static str,
    pub log: &'a RefCell<Vec<&'static str>>,
}

impl Drop for Noisy<'_> {
    fn drop(&mut self) {
        self.log.borrow_mut().push(self.name);
    }
}

pub struct Pair<'a> {
    pub first: Noisy<'a>,
    pub second: Noisy<'a>,
}

/// Don't change this function.
pub fn scene(log: &RefCell<Vec<&'static str>>) {
    let a = Noisy { name: "a", log };
    let _pair = Pair { first: Noisy { name: "first", log }, second: Noisy { name: "second", log } };
    let _ = Noisy { name: "ignored", log };
    let b = Noisy { name: "b", log };
    let _x = Noisy { name: "x1", log };
    let _x = Noisy { name: "x2", log };
    let _v = vec![Noisy { name: "v0", log }, Noisy { name: "v1", log }];
    drop(a);
    match (Noisy { name: "scrutinee", log }).name.len() {
        _ => {
            let _inner = Noisy { name: "inner", log };
        }
    }
    let _c = Noisy { name: "c", log };
    let _ = b;
}

/// The names in the order `scene` drops them.
"""

DROP_ANSWER = '["ignored", "a", "inner", "scrutinee", "c", "v0", "v1", "x2", "x1", "b", "first", "second"]'


def drop_prediction(names):
    return DROP_SCENE + f"pub const PREDICTED: [&str; 12] = {names};\n"


P.append(write(
    "predict-drop-order", "Predict the drop order", "medium", "clones-and-drops", ["Drop", "scopes", "temporaries", "shadowing"],
    """
        Read `scene` without running it. Fill `PREDICTED` with the twelve names in the order they're dropped.
        Don't change `scene`.
    """,
    drop_prediction('["?"; 12]'),
    DROP_SCENE.replace("/// The names in the order `scene` drops them.\n", """/// The names in the order `scene` drops them.
/// `let _ = Noisy {..}` drops at once; `drop(a)` next; the match's temporary lives to the end of the match,
/// after the arm's local. Then locals in reverse order: `_c`, the Vec (its elements in order), the shadowing
/// `_x` then the shadowed one, `b` (`let _ = b` didn't move it), and `_pair`, whose fields drop in order.
""") + f"pub const PREDICTED: [&str; 12] = {DROP_ANSWER};\n",
    [r"""
     use std::cell::RefCell;

     fn run() -> Vec<&'static str> {
         let log = RefCell::new(Vec::new());
         scene(&log);
         log.into_inner()
     }

     #[test]
     fn prediction_matches_the_run() {
         check!("scene()", PREDICTED.to_vec(), run());
     }

     #[test]
     fn twelve_names() {
         check!("names filled in", PREDICTED.iter().filter(|n| **n != "?").count(), 12);
     }

     #[test]
     fn known_names() {
         let mut p = PREDICTED.to_vec();
         p.sort();
         let mut r = run();
         r.sort();
         check!("the names in PREDICTED, sorted", p, r);
     }

     #[test]
     fn first_drop() {
         check!("the first name dropped", PREDICTED[0], run()[0]);
     }

     #[test]
     fn last_drop() {
         check!("the last name dropped", PREDICTED[11], run()[11]);
     }
     """],
    [r"""
     use std::cell::RefCell;

     fn run() -> Vec<&'static str> {
         let log = RefCell::new(Vec::new());
         scene(&log);
         log.into_inner()
     }

     fn before(names: &[&str], x: &str, y: &str) -> bool {
         let at = |n: &str| names.iter().position(|m| *m == n);
         at(x) < at(y)
     }

     fn same_order(x: &str, y: &str) -> (bool, bool) {
         (before(&PREDICTED, x, y), before(&run(), x, y))
     }

     #[test]
     fn explicit_drop_second() {
         check!("second drop: drop(a)", PREDICTED[1], run()[1]);
     }

     #[test]
     fn arm_local_before_scrutinee_temporary() {
         let (p, r) = same_order("inner", "scrutinee");
         check!("inner before scrutinee?", p, r);
     }

     #[test]
     fn let_underscore_does_not_move() {
         let (p, r) = same_order("c", "b");
         check!("c before b? (`let _ = b;`)", p, r);
     }

     #[test]
     fn vec_elements_in_order() {
         let (p, r) = same_order("v0", "v1");
         check!("v0 before v1?", p, r);
     }

     #[test]
     fn shadowing_does_not_drop() {
         let (p, r) = same_order("x1", "v1");
         check!("x1 before v1? (shadowed by the second _x)", p, r);
     }

     #[test]
     fn shadowed_after_shadowing() {
         let (p, r) = same_order("x2", "x1");
         check!("x2 before x1?", p, r);
     }

     #[test]
     fn fields_in_declaration_order() {
         let (p, r) = same_order("first", "second");
         check!("first before second?", p, r);
     }

     #[test]
     fn pair_drops_last() {
         let r = run();
         check!("the last two drops", (PREDICTED[10], PREDICTED[11]), (r[10], r[11]));
     }

     #[test]
     fn whole_order() {
         check!("scene()", PREDICTED.to_vec(), run());
     }
     """],
    [("rust", "`let _ = value;` binds nothing, so a temporary is dropped at once, and `let _ = b;` doesn't move `b` at all."),
     ("rust", "A temporary in a `match` scrutinee lives until the end of the whole `match`, so it outlives everything inside the arms."),
     ("rust", "Shadowing hides a variable; it doesn't drop it. Locals drop in reverse order of declaration; a struct's fields and a Vec's elements drop in order.")],
    ("""The rules, in the order the traps appear: `let _ = expr` drops a temporary at the end of the statement; `_` isn't a binding, so `let _ = b` moves nothing. Shadowing creates a second variable; the first lives to the end of the scope. `drop(a)` moves `a` into a function that drops it. A temporary in a `match` scrutinee lives to the end of the `match` statement, which is why `match mutex.lock().unwrap().x { .. }` holds the lock through every arm. At the end of scope, locals drop in reverse declaration order; each struct drops its fields in declaration order, and a `Vec` drops its elements front to back.""", "—", "—"),
    "Why does `let _guard = mutex.lock()` hold the lock while `let _ = mutex.lock()` releases it at once?",
    ["Locals drop in reverse declaration order; fields and Vec elements in order.", "`let _ = expr` drops a temporary at once, and doesn't move a named value.", "Scrutinee temporaries live to the end of the `match`; shadowing doesn't drop."],
    related=("S8", "C1"),
    wrong=dict(
        let_underscore_moves=drop_prediction('["ignored", "a", "inner", "scrutinee", "b", "c", "v0", "v1", "x2", "x1", "first", "second"]'),
        shadow_drops_early=drop_prediction('["ignored", "x1", "a", "inner", "scrutinee", "c", "v0", "v1", "x2", "b", "first", "second"]'),
        scrutinee_first=drop_prediction('["ignored", "a", "scrutinee", "inner", "c", "v0", "v1", "x2", "x1", "b", "first", "second"]'),
        vec_in_reverse=drop_prediction('["ignored", "a", "inner", "scrutinee", "c", "v1", "v0", "x2", "x1", "b", "first", "second"]'),
        fields_in_reverse=drop_prediction('["ignored", "a", "inner", "scrutinee", "c", "v0", "v1", "x2", "x1", "b", "second", "first"]'),
    ),
))

SPAN_SOLUTION = r"""
use std::cell::RefCell;

pub struct Span<'a> {
    name: &'static str,
    log: &'a RefCell<Vec<String>>,
}

impl<'a> Span<'a> {
    pub fn enter(name: &'static str, log: &'a RefCell<Vec<String>>) -> Self {
        log.borrow_mut().push(format!("enter {name}"));
        Span { name, log }
    }

    /// Ends the span with a status: records "exit <name>: <status>" instead of the plain exit.
    pub fn finish(self, status: &str) {
        self.log.borrow_mut().push(format!("exit {}: {status}", self.name));
        // The exit is recorded; don't let Drop record another. Nothing here owns heap memory, so nothing leaks.
        std::mem::forget(self);
    }
}

impl Drop for Span<'_> {
    fn drop(&mut self) {
        self.log.borrow_mut().push(format!("exit {}", self.name));
    }
}
"""

SPAN_STARTER = r"""
use std::cell::RefCell;

pub struct Span<'a> {
    name: &'static str,
    log: &'a RefCell<Vec<String>>,
}

impl<'a> Span<'a> {
    pub fn enter(name: &'static str, log: &'a RefCell<Vec<String>>) -> Self {
        todo!()
    }

    /// Ends the span with a status: records "exit <name>: <status>" instead of the plain exit.
    pub fn finish(self, status: &str) {
        todo!()
    }
}

impl Drop for Span<'_> {
    fn drop(&mut self) {
        // TODO: record the exit.
        // (Left empty rather than todo!(): a panic in drop during a failing test aborts every test.)
    }
}
"""

SPAN_VISIBLE = r"""
use std::cell::RefCell;

#[test]
fn nested_spans_exit_in_reverse() {
    let log = RefCell::new(Vec::new());
    {
        let _outer = Span::enter("outer", &log);
        let _inner = Span::enter("inner", &log);
    }
    check!("outer, then inner", log.into_inner(), vec!["enter outer", "enter inner", "exit inner", "exit outer"]);
}

fn early(log: &RefCell<Vec<String>>, bail: bool) -> u8 {
    let _s = Span::enter("work", log);
    if bail {
        return 0;
    }
    1
}

#[test]
fn early_return_still_exits() {
    let log = RefCell::new(Vec::new());
    early(&log, true);
    check!("return before the end of the function", log.into_inner(), vec!["enter work", "exit work"]);
}

#[test]
fn finish_records_status_once() {
    let log = RefCell::new(Vec::new());
    {
        let s = Span::enter("job", &log);
        s.finish("ok");
    }
    check!("enter job, finish(\"ok\")", log.into_inner(), vec!["enter job", "exit job: ok"]);
}

#[test]
fn finish_inner_then_outer_drops() {
    let log = RefCell::new(Vec::new());
    {
        let _outer = Span::enter("outer", &log);
        let inner = Span::enter("inner", &log);
        inner.finish("done");
    }
    check!("inner finished, outer dropped", log.into_inner(), vec!["enter outer", "enter inner", "exit inner: done", "exit outer"]);
}

#[test]
fn let_underscore_exits_at_once() {
    let log = RefCell::new(Vec::new());
    {
        let _ = Span::enter("gone", &log);
        let _kept = Span::enter("kept", &log);
    }
    check!("let _ = Span::enter(..), then let _kept", log.into_inner(), vec!["enter gone", "exit gone", "enter kept", "exit kept"]);
}
"""

SPAN_HIDDEN = r"""
use std::cell::RefCell;
use std::panic::{AssertUnwindSafe, catch_unwind};

#[test]
fn panic_still_exits() {
    let log = RefCell::new(Vec::new());
    let _ = catch_unwind(AssertUnwindSafe(|| {
        let _s = Span::enter("risky", &log);
        panic!("boom");
    }));
    check!("a panic inside the span", log.into_inner(), vec!["enter risky", "exit risky"]);
}

#[test]
fn panic_in_nested_spans() {
    let log = RefCell::new(Vec::new());
    let _ = catch_unwind(AssertUnwindSafe(|| {
        let _outer = Span::enter("outer", &log);
        let _inner = Span::enter("inner", &log);
        panic!("boom");
    }));
    check!("a panic inside two spans", log.into_inner(), vec!["enter outer", "enter inner", "exit inner", "exit outer"]);
}

#[test]
fn nothing_before_the_drop() {
    let log = RefCell::new(Vec::new());
    let s = Span::enter("held", &log);
    let before = log.borrow().clone();
    drop(s);
    check!("the log while the span is alive", before, vec!["enter held"]);
}

#[test]
fn explicit_drop_exits_early() {
    let log = RefCell::new(Vec::new());
    {
        let a = Span::enter("a", &log);
        drop(a);
        let _b = Span::enter("b", &log);
    }
    check!("drop(a), then enter b", log.into_inner(), vec!["enter a", "exit a", "enter b", "exit b"]);
}

fn take(_s: Span<'_>) {}

#[test]
fn moved_span_exits_where_it_ends() {
    let log = RefCell::new(Vec::new());
    {
        let s = Span::enter("moved", &log);
        take(s);
        let _after = Span::enter("after", &log);
    }
    check!("span moved into a function that drops it", log.into_inner(), vec!["enter moved", "exit moved", "enter after", "exit after"]);
}

#[test]
fn finish_in_a_loop() {
    let log = RefCell::new(Vec::new());
    for (i, name) in ["x", "y", "z"].into_iter().enumerate() {
        let s = Span::enter(name, &log);
        if i == 1 {
            s.finish("skipped");
        }
    }
    check!("x dropped, y finished, z dropped", log.into_inner(), vec!["enter x", "exit x", "enter y", "exit y: skipped", "enter z", "exit z"]);
}

#[test]
fn finish_with_empty_status() {
    let log = RefCell::new(Vec::new());
    Span::enter("e", &log).finish("");
    check!("finish(\"\")", log.into_inner(), vec!["enter e", "exit e: "]);
}

#[test]
fn panic_after_finish() {
    let log = RefCell::new(Vec::new());
    let _ = catch_unwind(AssertUnwindSafe(|| {
        Span::enter("a", &log).finish("ok");
        let _b = Span::enter("b", &log);
        panic!("boom");
    }));
    check!("a finished, then b panics", log.into_inner(), vec!["enter a", "exit a: ok", "enter b", "exit b"]);
}

#[test]
fn random_vs_model() {
    const NAMES: [&str; 4] = ["p", "q", "r", "s"];
    let mut rng = anneal_prelude::Rng::new(6109);
    for _ in 0..200 {
        let log = RefCell::new(Vec::new());
        let mut want = Vec::new();
        let mut ops = Vec::new();
        {
            let mut open: Vec<(Span, &str)> = Vec::new();
            let steps = rng.below(12);
            for _ in 0..steps {
                if rng.bool() || open.is_empty() {
                    let name = *rng.pick(&NAMES);
                    want.push(format!("enter {name}"));
                    ops.push(format!("enter {name}"));
                    open.push((Span::enter(name, &log), name));
                } else {
                    let i = rng.below(open.len());
                    let (span, name) = open.remove(i);
                    if rng.bool() {
                        want.push(format!("exit {name}: ok"));
                        ops.push(format!("finish {name}"));
                        span.finish("ok");
                    } else {
                        want.push(format!("exit {name}"));
                        ops.push(format!("drop {name}"));
                        drop(span);
                    }
                }
            }
            while let Some((span, name)) = open.pop() {
                want.push(format!("exit {name}"));
                drop(span);
            }
        }
        check!(format!("{ops:?}, then the rest in reverse"), log.into_inner(), want);
    }
}
"""

P.append(write(
    "raii-span-guard", "An RAII span guard", "medium", "clones-and-drops", ["Drop", "RAII", "unwinding", "mem::forget", "E0509"],
    """
        `Span::enter(name, log)` records `"enter <name>"`, and dropping the span records `"exit <name>"`, on
        every path: normal end of scope, early return, and panic.

        `span.finish(status)` ends a span explicitly, recording `"exit <name>: <status>"` **instead of** the plain
        exit. Each span records exactly one exit.
    """,
    SPAN_STARTER,
    SPAN_SOLUTION,
    [SPAN_VISIBLE],
    [SPAN_HIDDEN],
    [("rust", "Record the enter in `enter` and the exit in `drop`; the compiler inserts the drop on every exit path, unwinding included."),
     ("rust", "`finish` takes `self`, so the span is dropped when `finish` returns and `drop` would record a second exit. You can't move fields out of a type that implements `Drop` (E0509) to avoid that. What function stops a value's destructor from running?")],
    ("""Unwinding runs destructors, so the exit is recorded even when the code inside panics: this is how `MutexGuard`, `File` and tracing spans work. A consuming method on a `Drop` type still ends in a drop, and destructuring `self` to dodge it is E0509. `mem::forget(self)` skips the destructor; it's safe (leaking is not undefined behaviour) and here leaks nothing, since the fields are only references. With fields that own memory, you'd instead keep a `done: bool` or an `Option` field and have `Drop` check it.

Syntax to remember: `impl Drop for Span<'_> { fn drop(&mut self) { .. } }` · `pub fn finish(self, status: &str)` · `std::mem::forget(self)`.""", "O(1)", "O(1)"),
    "What happens to your guard if a caller calls `std::mem::forget` on it, and why is that allowed in safe code?",
    ["RAII: acquire in a constructor, release in `Drop`; unwinding runs it too.", "A consuming method on a `Drop` type needs `mem::forget` or a flag to skip the destructor."],
    related=("C1", "B6"),
    wrong=dict(
        double_exit=sub(SPAN_SOLUTION, "        std::mem::forget(self);\n", ""),
        exit_at_enter=sub(sub(SPAN_SOLUTION, "        log.borrow_mut().push(format!(\"enter {name}\"));\n", "        log.borrow_mut().push(format!(\"enter {name}\"));\n        log.borrow_mut().push(format!(\"exit {name}\"));\n"),
                          "        self.log.borrow_mut().push(format!(\"exit {}\", self.name));\n    }\n}", "        let _ = (self.name, self.log);\n    }\n}"),
        finish_plain_exit=sub(SPAN_SOLUTION, "        self.log.borrow_mut().push(format!(\"exit {}: {status}\", self.name));\n        // The exit is recorded; don't let Drop record another. Nothing here owns heap memory, so nothing leaks.\n        std::mem::forget(self);\n", "        let _ = status;\n"),
    ),
))

# ---------------------------------------------------------------- closures take ownership (medium)

CLOSURE_STARTER = r"""
/// Each call returns the next number after `start`.
pub fn counter(start: u32) -> impl FnMut() -> u32 {
    let mut n = start;
    || {
        n += 1;
        n
    }
}

/// A formatter that puts `prefix` before each number: prefix "id-" turns 7 into "id-7".
/// The formatter may outlive the string `prefix` was borrowed from.
pub fn labeler(prefix: &str) -> impl Fn(u32) -> String {
    |n| format!("{prefix}{n}")
}

/// One check per limit: `checks[i](x)` says whether `x` is above `limits[i]`.
pub fn above_checks(limits: &[u32]) -> Vec<Box<dyn Fn(u32) -> bool>> {
    let mut checks: Vec<Box<dyn Fn(u32) -> bool>> = Vec::new();
    for &limit in limits {
        checks.push(Box::new(|x| x > limit));
    }
    checks
}

/// One greeting job per name, run later. Each job returns "hello, <name>".
pub fn greeters(names: Vec<String>) -> Vec<Box<dyn FnOnce() -> String>> {
    let mut jobs: Vec<Box<dyn FnOnce() -> String>> = Vec::new();
    for name in names {
        jobs.push(Box::new(|| format!("hello, {name}")));
    }
    jobs
}
"""

CLOSURE_SOLUTION = r"""
/// Each call returns the next number after `start`.
pub fn counter(start: u32) -> impl FnMut() -> u32 {
    let mut n = start;
    move || {
        n += 1;
        n
    }
}

/// A formatter that puts `prefix` before each number: prefix "id-" turns 7 into "id-7".
/// The formatter may outlive the string `prefix` was borrowed from.
pub fn labeler(prefix: &str) -> impl Fn(u32) -> String {
    let prefix = prefix.to_string();
    move |n| format!("{prefix}{n}")
}

/// One check per limit: `checks[i](x)` says whether `x` is above `limits[i]`.
pub fn above_checks(limits: &[u32]) -> Vec<Box<dyn Fn(u32) -> bool>> {
    let mut checks: Vec<Box<dyn Fn(u32) -> bool>> = Vec::new();
    for &limit in limits {
        checks.push(Box::new(move |x| x > limit));
    }
    checks
}

/// One greeting job per name, run later. Each job returns "hello, <name>".
pub fn greeters(names: Vec<String>) -> Vec<Box<dyn FnOnce() -> String>> {
    let mut jobs: Vec<Box<dyn FnOnce() -> String>> = Vec::new();
    for name in names {
        jobs.push(Box::new(move || format!("hello, {name}")));
    }
    jobs
}
"""

P.append(fix(
    "fix-closure-may-outlive", "Fix: closures that outlive their function (E0373)", "medium", "closures-take-ownership", ["E0373", "move", "impl Fn", "Box<dyn FnOnce>"],
    """
        Each function returns closures that are called after it returns, and none of them compiles. Fix all
        four. The labeler must keep working after the caller's prefix `String` is gone.
    """,
    CLOSURE_STARTER,
    CLOSURE_SOLUTION,
    [T("counts", "counter(10), called three times", "{ let mut c = counter(10); (c(), c(), c()) }", "(11, 12, 13)"),
     T("counters_are_independent", "two counters from 0: a(), a(), b()", "{ let mut a = counter(0); let mut b = counter(0); a(); a(); b() }", "1"),
     T("labeler_outlives_prefix", "labeler built from a String that's dropped before the call", "f(7)", '"id-7"',
       setup='let f = {\n    let p = String::from("id-");\n    labeler(&p)\n};'),
     T("above", "above_checks([10, 20]) applied to 15", "above_checks(&[10, 20]).iter().map(|c| c(15)).collect::<Vec<_>>()", "vec![true, false]"),
     T("greeters_run_later", "greeters([\"ann\", \"bo\"]), run in order", 'greeters(vec!["ann".into(), "bo".into()]).into_iter().map(|job| job()).collect::<Vec<_>>()', 'vec!["hello, ann", "hello, bo"]')],
    [T("near_max", "counter(u32::MAX - 2), two calls", "{ let mut c = counter(u32::MAX - 2); (c(), c()) }", "(u32::MAX - 1, u32::MAX)"),
     T("boxed_counter", "counter(3) as Box<dyn FnMut() -> u32>", "{ let mut c: Box<dyn FnMut() -> u32> = Box::new(counter(3)); (c(), c()) }", "(4, 5)"),
     T("labeler_empty_prefix", "labeler(\"\")(0)", 'labeler("")(0)', '"0"'),
     T("labeler_called_twice", "labeler(\"#\") on 1 and 22", '{ let f = labeler("#"); (f(1), f(22)) }', '("#1".to_string(), "#22".to_string())'),
     T("labeler_prefix_changed_later", "prefix String changed after labeler", '{ let mut p = String::from("a-"); let f = labeler(&p); p.push_str("zzz"); (f(1), p) }', '("a-1".to_string(), "a-zzz".to_string())'),
     T("above_is_strict", "above_checks([5]) applied to 5 and 6", "{ let c = above_checks(&[5]); (c[0](5), c[0](6)) }", "(false, true)"),
     T("above_empty", "above_checks([])", "above_checks(&[]).len()", "0"),
     T("above_outlives_limits", "checks kept after the limits Vec is dropped", "{ let checks = { let limits = vec![0, u32::MAX - 1]; above_checks(&limits) }; (checks[0](1), checks[1](u32::MAX)) }", "(true, true)"),
     T("greeters_unicode_and_empty", "greeters([\"\", \"zoë\"])", 'greeters(vec![String::new(), "zoë".into()]).into_iter().map(|job| job()).collect::<Vec<_>>()', 'vec!["hello, ", "hello, zoë"]'),
     T("greeters_out_of_order", "run the second job first", '{ let mut jobs = greeters(vec!["a".into(), "b".into()]); let second = jobs.pop().unwrap(); let first = jobs.pop().unwrap(); (second(), first()) }', '("hello, b".to_string(), "hello, a".to_string())'),
     r"""
     #[test]
     fn greeters_keep_the_names() {
         // Each job owns its name: the Strings move into the closures, they aren't copied.
         let names: Vec<String> = vec!["left".into(), "right".into()];
         let lens: Vec<usize> = names.iter().map(|n| n.len()).collect();
         let jobs = greeters(names);
         check!("greeters([\"left\", \"right\"]).len()", (jobs.len(), lens), (2, vec![4, 5]));
     }

     #[test]
     fn random_vs_brute_force() {
         let mut rng = anneal_prelude::Rng::new(6110);
         for _ in 0..200 {
             let start = rng.int(0, 1_000_000) as u32;
             let calls = rng.below(20) + 1;
             let mut c = counter(start);
             let got: Vec<u32> = (0..calls).map(|_| c()).collect();
             let want: Vec<u32> = (1..=calls as u32).map(|k| start + k).collect();
             check!(format!("counter({start}), {calls} calls"), got, want);
             let k = rng.below(5);
             let limits: Vec<u32> = rng.vec(k, 0, 20);
             let x = rng.below(21) as u32;
             let got: Vec<bool> = above_checks(&limits).iter().map(|f| f(x)).collect();
             check!(format!("above_checks({limits:?}) applied to {x}"), got, limits.iter().map(|&l| x > l).collect::<Vec<_>>());
         }
     }
     """],
    [("rust", "A closure borrows what it uses unless told otherwise. `move` makes it take its captures: a copy for a `Copy` value like `n` or `limit`, the value itself for a `String`."),
     ("rust", "`move` alone doesn't fix `labeler`: the closure would own a `&str` borrowed from the caller, and the returned type would need to say so (`+ '_`). The caller drops its `String` first, so the closure needs its own copy.")],
    ("""A closure that outlives its stack frame can't borrow the frame's locals (E0373). `move` makes it own its captures: `n` and `limit` are `Copy`, so each closure gets a copy (`counter` then mutates its own `n`, hence `FnMut`); `name` is a `String`, so it moves into its job, and calling a `Box<dyn FnOnce>` consumes the job. For `labeler`, `move` would capture the `&str` itself, and in edition 2021 the opaque return type must then name that borrow (`impl Fn(u32) -> String + '_`). That compiles, but the test drops the prefix before calling, so the closure has to own a `String`: one copy at construction, none per call.

Syntax to remember: `move |n| format!("{prefix}{n}")` · `Vec<Box<dyn FnOnce() -> String>>` · `impl Fn(u32) -> String + '_` for a closure that may keep a borrow.""", "O(1) per call", "O(1) per closure"),
    "Why is `counter`'s closure `FnMut`, the labeler's `Fn`, and each greeter `FnOnce`?",
    ["`move` closures own their captures: a copy for `Copy` values, the value itself otherwise.", "A returned closure can't borrow a local; one that keeps a borrowed parameter needs `+ '_`.", "`Box<dyn FnOnce()>` is called by value, once."],
    rules=dict(methods=["clone"]),
    related=("L6", "L3"),
    wrong=dict(
        returns_before_increment=sub(CLOSURE_SOLUTION, "    move || {\n        n += 1;\n        n\n    }", "    move || {\n        let current = n;\n        n += 1;\n        current\n    }"),
        above_or_equal=sub(CLOSURE_SOLUTION, "move |x| x > limit", "move |x| x >= limit"),
        labeler_space=sub(CLOSURE_SOLUTION, 'move |n| format!("{prefix}{n}")', 'move |n| format!("{prefix} {n}")'),
    ),
))

THREADS_SOLUTION = r"""
use std::thread;

/// Sums each chunk with `sum`, each on its own thread. The totals come back in chunk order.
pub fn sum_chunks(chunks: Vec<Vec<u64>>, sum: fn(&[u64]) -> u64) -> Vec<u64> {
    // Start every thread first. Joining inside this `map` would run them one after another.
    let handles: Vec<thread::JoinHandle<u64>> = chunks
        .into_iter()
        .map(|chunk| thread::spawn(move || sum(&chunk)))
        .collect();
    handles.into_iter().map(|h| h.join().expect("worker panicked")).collect()
}

/// Splits `data` into `parts` contiguous pieces whose lengths differ by at most one (longer pieces first),
/// sums each piece with `sum` on its own scoped thread, and returns the sums in order. `parts` is at least 1.
pub fn sum_parts(data: &[u64], parts: usize, sum: fn(&[u64]) -> u64) -> Vec<u64> {
    let (base, extra) = (data.len() / parts, data.len() % parts);
    thread::scope(|s| {
        let mut handles = Vec::with_capacity(parts);
        let mut start = 0;
        for i in 0..parts {
            let len = base + usize::from(i < extra);
            let piece = &data[start..start + len];
            start += len;
            // `move` copies the `&[u64]` and the `fn` pointer into the thread; the data stays borrowed from the caller.
            handles.push(s.spawn(move || sum(piece)));
        }
        handles.into_iter().map(|h| h.join().expect("worker panicked")).collect()
    })
}
"""

THREADS_STARTER = r"""
/// Sums each chunk with `sum`, each on its own thread. The totals come back in chunk order.
pub fn sum_chunks(chunks: Vec<Vec<u64>>, sum: fn(&[u64]) -> u64) -> Vec<u64> {
    todo!()
}

/// Splits `data` into `parts` contiguous pieces whose lengths differ by at most one (longer pieces first),
/// sums each piece with `sum` on its own scoped thread, and returns the sums in order. `parts` is at least 1.
pub fn sum_parts(data: &[u64], parts: usize, sum: fn(&[u64]) -> u64) -> Vec<u64> {
    todo!()
}
"""

# A `sum` for the tests that only finishes once every piece is being summed at the same time: threads that were
# started one at a time, or joined before the next one started, never all meet. It gives up after 3 seconds
# instead of hanging (the runner allows 15 seconds for all the tests), so a wrong solution fails rather than stalls.
THREADS_GATE = r"""
use std::sync::{Condvar, Mutex};
use std::thread::{self, ThreadId};
use std::time::{Duration, Instant};

fn add(xs: &[u64]) -> u64 {
    xs.iter().sum()
}

/// Every call waits until `expected` calls are in progress at once, or 3 seconds pass.
struct Gate {
    state: Mutex<GateState>,
    arrived: Condvar,
}

struct GateState {
    expected: usize,
    threads: Vec<ThreadId>,
    gave_up: bool,
}

impl Gate {
    const fn new() -> Self {
        Gate { state: Mutex::new(GateState { expected: 0, threads: Vec::new(), gave_up: false }), arrived: Condvar::new() }
    }

    fn expect(&self, n: usize) {
        *self.state.lock().unwrap() = GateState { expected: n, threads: Vec::new(), gave_up: false };
    }

    fn sum(&self, xs: &[u64]) -> u64 {
        let deadline = Instant::now() + Duration::from_secs(3);
        let mut s = self.state.lock().unwrap();
        s.threads.push(thread::current().id());
        self.arrived.notify_all();
        while s.threads.len() < s.expected && !s.gave_up {
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                s.gave_up = true;
                self.arrived.notify_all();
                break;
            }
            s = self.arrived.wait_timeout(s, left).unwrap().0;
        }
        xs.iter().sum()
    }

    /// How many pieces went through `sum`, whether they were all in it at once, and whether each ran on its
    /// own thread, not the caller's.
    fn report(&self) -> (usize, bool, bool) {
        let s = self.state.lock().unwrap();
        let mut ids = s.threads.clone();
        ids.sort_by_key(|id| format!("{id:?}"));
        ids.dedup();
        let own = ids.len() == s.threads.len() && !s.threads.contains(&thread::current().id());
        (s.threads.len(), !s.gave_up && s.threads.len() == s.expected, own)
    }
}
"""

P.append(write(
    "move-into-threads", "Move data into threads, or borrow it with scope", "medium", "closures-take-ownership", ["thread::spawn", "thread::scope", "move", "'static", "fn pointers"],
    """
        Two ways to sum in parallel. Both take `sum`, the function that sums one piece: call it on every piece,
        each piece on its own thread.

        - `sum_chunks` owns its chunks: sum each on a `thread::spawn`ed thread and return the totals in chunk
          order.
        - `sum_parts` only borrows `data`, so `thread::spawn` can't use it. Split it into `parts` contiguous
          pieces whose lengths differ by at most one, longer pieces first (10 items in 3 parts: 4, 3, 3), and
          sum each piece on its own **scoped** thread. Return the sums in order. `parts` is at least 1 and may be
          larger than `data.len()`, which gives empty pieces.

        Start every thread before joining any of them. Some tests pass a `sum` that waits until every piece is
        being summed at the same time. Threads started one after another never all meet, so after 3 seconds
        that test gives up and fails.
    """,
    THREADS_STARTER,
    THREADS_SOLUTION,
    [THREADS_GATE,
     T("chunks_in_order", "sum_chunks([[1, 2], [3], [4, 5, 6]])", "sum_chunks(vec![vec![1, 2], vec![3], vec![4, 5, 6]], add)", "vec![3, 3, 15]"),
     T("no_chunks", "sum_chunks([])", "sum_chunks(vec![], add)", "Vec::<u64>::new()"),
     T("parts_split", "sum_parts(1..=10, 3): pieces of 4, 3, 3", "sum_parts(&data, 3, add)", "vec![10, 18, 27]", setup="let data: Vec<u64> = (1..=10).collect();"),
     T("data_still_usable", "sum_parts borrows: data used afterwards", "{ let s = sum_parts(&data, 2, add); (s, data.len()) }", "(vec![3, 3], 3)", setup="let data = vec![1u64, 2, 3];"),
     T("more_parts_than_items", "sum_parts([5, 6], 4)", "sum_parts(&[5, 6], 4, add)", "vec![5, 6, 0, 0]"),
     r"""
     static PARTS_GATE: Gate = Gate::new();

     fn parts_gate(xs: &[u64]) -> u64 {
         PARTS_GATE.sum(xs)
     }

     #[test]
     fn pieces_run_at_once() {
         let data: Vec<u64> = (1..=9).collect();
         PARTS_GATE.expect(3);
         let sums = sum_parts(&data, 3, parts_gate);
         let (summed, at_once, own_threads) = PARTS_GATE.report();
         check!("sum_parts(1..=9, 3) with a sum that waits until all 3 pieces are being summed: (sums, pieces summed, all at once, each on its own thread)", (sums, summed, at_once, own_threads), (vec![6, 15, 24], 3, true, true));
     }
     """],
    [THREADS_GATE,
     T("empty_chunks_inside", "sum_chunks([[], [1], []])", "sum_chunks(vec![vec![], vec![1], vec![]], add)", "vec![0, 1, 0]"),
     T("beyond_u32", "sum_chunks([[5000000000, 5000000000]])", "sum_chunks(vec![vec![5_000_000_000, 5_000_000_000]], add)", "vec![10_000_000_000]"),
     T("sixty_four_chunks", "64 chunks [i]", "sum_chunks((0..64).map(|i| vec![i]).collect(), add)", "(0..64).collect::<Vec<u64>>()"),
     T("one_part", "sum_parts([1, 2, 3], 1)", "sum_parts(&[1, 2, 3], 1, add)", "vec![6]"),
     T("even_split", "sum_parts([1, 1, 1, 1, 1, 1], 3)", "sum_parts(&[1; 6], 3, add)", "vec![2, 2, 2]"),
     T("longer_pieces_first", "sum_parts([1, 10, 100, 1000, 10000], 3): pieces of 2, 2, 1", "sum_parts(&[1, 10, 100, 1000, 10000], 3, add)", "vec![11, 1100, 10000]"),
     T("empty_data", "sum_parts([], 3)", "sum_parts(&[], 3, add)", "vec![0, 0, 0]"),
     T("parts_equal_len", "sum_parts([7, 8, 9], 3)", "sum_parts(&[7, 8, 9], 3, add)", "vec![7, 8, 9]"),
     T("big_data", "sum_parts(1..=200000, 8)", "sum_parts(&data, 8, add).iter().sum::<u64>()", "20_000_100_000", setup="let data: Vec<u64> = (1..=200_000).collect();"),
     r"""
     static CHUNKS_GATE: Gate = Gate::new();

     fn chunks_gate(xs: &[u64]) -> u64 {
         CHUNKS_GATE.sum(xs)
     }

     #[test]
     fn chunks_run_at_once() {
         CHUNKS_GATE.expect(4);
         let sums = sum_chunks(vec![vec![1], vec![2, 2], vec![3, 3, 3], vec![4; 4]], chunks_gate);
         let (summed, at_once, own_threads) = CHUNKS_GATE.report();
         check!("sum_chunks of 4 chunks with a sum that waits until all 4 are being summed: (sums, chunks summed, all at once, each on its own thread)", (sums, summed, at_once, own_threads), (vec![1, 4, 9, 16], 4, true, true));
     }

     static MANY_GATE: Gate = Gate::new();

     fn many_gate(xs: &[u64]) -> u64 {
         MANY_GATE.sum(xs)
     }

     #[test]
     fn eight_pieces_run_at_once() {
         let data: Vec<u64> = (1..=100).collect();
         MANY_GATE.expect(8);
         let sums = sum_parts(&data, 8, many_gate);
         let (summed, at_once, own_threads) = MANY_GATE.report();
         check!("sum_parts(1..=100, 8) with a sum that waits until all 8 pieces are being summed: (total, pieces summed, all at once, each on its own thread)", (sums.iter().sum::<u64>(), summed, at_once, own_threads), (5050, 8, true, true));
     }
     """,
     r"""
     #[test]
     fn random_vs_brute_force() {
         let mut rng = anneal_prelude::Rng::new(6111);
         for _ in 0..100 {
             let n = rng.below(12);
             let data: Vec<u64> = rng.vec(n, 0, 1_000_000_000_000);
             let parts = rng.below(6) + 1;
             let mut want = Vec::new();
             let mut start = 0;
             for i in 0..parts {
                 let len = n / parts + if i < n % parts { 1 } else { 0 };
                 want.push(data[start..start + len].iter().sum::<u64>());
                 start += len;
             }
             check!(format!("sum_parts({data:?}, {parts})"), sum_parts(&data, parts, add), want);
             let k = rng.below(5);
             let mut chunks = Vec::new();
             for _ in 0..k {
                 let len = rng.below(5);
                 chunks.push(rng.vec::<u64>(len, 0, 1000));
             }
             let want: Vec<u64> = chunks.iter().map(|c| c.iter().sum()).collect();
             check!(format!("sum_chunks({chunks:?})"), sum_chunks(chunks.clone(), add), want);
         }
     }
     """],
    [("rust", "`thread::spawn` needs `F: 'static`: `move` the chunk into the closure. `sum` is a `fn` pointer, which is `Copy` and `'static`, so `move` copies it into every thread."),
     ("rust", "Collect every `JoinHandle` before joining any. Calling `join()` inside the loop that spawns waits for each thread before the next one starts, and the threads run one after another."),
     ("rust", "`thread::scope(|s| { ... s.spawn(move || ...) ... })` lets threads borrow from outside, because the scope joins them all before it returns. `move` there copies the `&[u64]` (a reference is `Copy`) along with the loop's locals.")],
    ("""`thread::spawn` can't borrow: the thread might outlive the caller, so the closure must be `'static` and own what it uses. The chunk moves in; `sum`, a `fn(&[u64]) -> u64`, is `Copy` and `'static`, so each thread gets its own copy of the pointer. `thread::scope` (Rust 1.63) guarantees every thread it spawns is joined before `scope` returns, so its threads may borrow `data`. They still need `move`, but only to take the loop's locals (`piece`, a `&[u64]`) by value instead of borrowing variables that change on the next iteration.

Parallel work only overlaps if every thread starts before the first `join`: spawn them all, keep the handles, then join in order. The tests check that with a `sum` that waits for all the pieces, the same way a barrier would.

Syntax to remember: `thread::spawn(move || sum(&chunk))` · `handle.join()` returns `Result<T, Box<dyn Any + Send>>` · `thread::scope(|s| { let h = s.spawn(move || ..); h.join().unwrap() })`.""", "O(n) work", "O(parts) threads"),
    "How would you share one big `Vec` with `thread::spawn`ed threads without copying it, and when is that better than `scope`?",
    ["`thread::spawn` needs `'static`: move owned data in.", "`thread::scope` lets threads borrow; `move` copies references and loop locals in.", "Start every thread before joining any."],
    related=("C1", "L3"),
    wrong=dict(
        shorter_pieces_first=sub(THREADS_SOLUTION, "let len = base + usize::from(i < extra);", "let len = base + usize::from(i >= parts - extra);"),
        chunks_by_ceiling=sub(THREADS_SOLUTION, "            let len = base + usize::from(i < extra);\n            let piece = &data[start..start + len];",
                              "            let size = (data.len() + parts - 1) / parts;\n            let len = size.min(data.len() - start);\n            let piece = &data[start..start + len];"),
        joins_each_thread=r"""
            use std::thread;

            /// Sums each chunk with `sum`, each on its own thread. The totals come back in chunk order.
            pub fn sum_chunks(chunks: Vec<Vec<u64>>, sum: fn(&[u64]) -> u64) -> Vec<u64> {
                // Wrong: each thread is joined before the next one starts, so they run one after another.
                chunks
                    .into_iter()
                    .map(|chunk| thread::spawn(move || sum(&chunk)).join().expect("worker panicked"))
                    .collect()
            }

            /// Splits `data` into `parts` contiguous pieces whose lengths differ by at most one (longer pieces first),
            /// sums each piece with `sum` on its own scoped thread, and returns the sums in order. `parts` is at least 1.
            pub fn sum_parts(data: &[u64], parts: usize, sum: fn(&[u64]) -> u64) -> Vec<u64> {
                let (base, extra) = (data.len() / parts, data.len() % parts);
                thread::scope(|s| {
                    let mut out = Vec::with_capacity(parts);
                    let mut start = 0;
                    for i in 0..parts {
                        let len = base + usize::from(i < extra);
                        let piece = &data[start..start + len];
                        start += len;
                        // Wrong: joined right away, so the next piece waits for this one.
                        out.push(s.spawn(move || sum(piece)).join().expect("worker panicked"));
                    }
                    out
                })
            }
        """,
        no_threads=r"""
            /// Sums each chunk with `sum`, each on its own thread. The totals come back in chunk order.
            pub fn sum_chunks(chunks: Vec<Vec<u64>>, sum: fn(&[u64]) -> u64) -> Vec<u64> {
                // Wrong: no threads. The sums are right, but nothing runs at the same time.
                chunks.iter().map(|c| sum(c)).collect()
            }

            /// Splits `data` into `parts` contiguous pieces whose lengths differ by at most one (longer pieces first),
            /// sums each piece with `sum` on its own scoped thread, and returns the sums in order. `parts` is at least 1.
            pub fn sum_parts(data: &[u64], parts: usize, sum: fn(&[u64]) -> u64) -> Vec<u64> {
                let (base, extra) = (data.len() / parts, data.len() % parts);
                let mut out = Vec::with_capacity(parts);
                let mut start = 0;
                for i in 0..parts {
                    let len = base + usize::from(i < extra);
                    out.push(sum(&data[start..start + len]));
                    start += len;
                }
                out
            }
        """,
    ),
))

P.append(dict(
    slug="fix-moved-into-closure-in-loop", title="Fix: moved into a closure in a loop", mode="fix", level="medium", stage="closures-take-ownership", tags=["E0382", "Arc", "move"],
    teaches=["A `move` closure in a loop moves the value on the first iteration.", "Share read-only data across threads with `Arc` and `Arc::clone`."],
    statement="Every thread should sum the same data. It doesn't compile. Don't copy the data per thread.",
    starter="""
        use std::thread;

        /// `n` threads each sum `data`; returns every thread's result.
        pub fn sum_everywhere(data: Vec<u64>, n: usize) -> Vec<u64> {
            let mut handles = Vec::new();
            for _ in 0..n {
                handles.push(thread::spawn(move || data.iter().sum::<u64>()));
            }
            handles.into_iter().map(|h| h.join().expect("worker panicked")).collect()
        }
    """,
    solution="""
        use std::sync::Arc;
        use std::thread;

        /// `n` threads each sum `data`; returns every thread's result.
        pub fn sum_everywhere(data: Vec<u64>, n: usize) -> Vec<u64> {
            let data = Arc::new(data);
            let mut handles = Vec::new();
            for _ in 0..n {
                let data = Arc::clone(&data);
                handles.push(thread::spawn(move || data.iter().sum::<u64>()));
            }
            handles.into_iter().map(|h| h.join().expect("worker panicked")).collect()
        }
    """,
    rules=dict(methods=["clone", "to_vec", "to_owned"]),
    visible=[
        T("three_threads", "data = [1, 2, 3], n = 3", "sum_everywhere(vec![1, 2, 3], 3)", "vec![6, 6, 6]"),
        T("no_threads", "n = 0", "sum_everywhere(vec![1], 0)", "Vec::<u64>::new()"),
        T("one_thread", "data = [4, 5], n = 1", "sum_everywhere(vec![4, 5], 1)", "vec![9]"),
        T("empty_data_visible", "data = [], n = 2", "sum_everywhere(vec![], 2)", "vec![0, 0]"),
        T("two_threads", "data = [10, 20], n = 2", "sum_everywhere(vec![10, 20], 2)", "vec![30, 30]"),
    ],
    hidden=[
        T("big", "data = 0..10000, n = 4", "sum_everywhere((0..10_000).collect(), 4)", "vec![49_995_000; 4]"),
        T("empty_data", "data = [], n = 3", "sum_everywhere(vec![], 3)", "vec![0, 0, 0]"),
        T("single_value", "data = [9], n = 2", "sum_everywhere(vec![9], 2)", "vec![9, 9]"),
        T("uneven_split", "data = [1, 2, 3, 4, 5], n = 2", "sum_everywhere(vec![1, 2, 3, 4, 5], 2)", "vec![15, 15]"),
        T("sixteen_threads", "data = [1, 1, 1], n = 16", "sum_everywhere(vec![1, 1, 1], 16)", "vec![3; 16]"),
        T("large_values", "data = [u64::MAX / 4; 2], n = 2", "sum_everywhere(vec![u64::MAX / 4; 2], 2)", "vec![u64::MAX / 4 * 2; 2]"),
        T("empty_no_threads", "data = [], n = 0", "sum_everywhere(vec![], 0)", "Vec::<u64>::new()"),
        """
        #[test]
        fn random_vs_brute_force() {
            let mut rng = anneal_prelude::Rng::new(1113);
            for _ in 0..100 {
                let len = rng.below(10);
                let data: Vec<u64> = rng.vec(len, 0, 1_000_000);
                let n = rng.below(5);
                let want = vec![data.iter().sum::<u64>(); n];
                check!(format!("data = {data:?}, n = {n}"), sum_everywhere(data.clone(), n), want);
            }
        }
        """,
        r"""
        // Counts allocations of 1 MiB or more on every thread, so a copy made inside a worker is seen too. The other
        // tests in this binary only make small allocations, so running them in parallel doesn't change the count.
        mod big_allocs {
            use std::alloc::{GlobalAlloc, Layout, System};
            use std::sync::atomic::{AtomicUsize, Ordering};

            pub static BIG: AtomicUsize = AtomicUsize::new(0);
            const LIMIT: usize = 1 << 20;

            pub struct Counting;

            unsafe impl GlobalAlloc for Counting {
                unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
                    if layout.size() >= LIMIT {
                        BIG.fetch_add(1, Ordering::Relaxed);
                    }
                    unsafe { System.alloc(layout) }
                }

                unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
                    unsafe { System.dealloc(ptr, layout) }
                }

                unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
                    if new_size >= LIMIT {
                        BIG.fetch_add(1, Ordering::Relaxed);
                    }
                    unsafe { System.realloc(ptr, layout, new_size) }
                }
            }

            #[global_allocator]
            static GLOBAL: Counting = Counting;
        }

        #[test]
        fn data_not_copied_per_thread() {
            use std::sync::atomic::Ordering;
            let data: Vec<u64> = (0..1_000_000).collect();
            let before = big_allocs::BIG.load(Ordering::Relaxed);
            let sums = sum_everywhere(data, 4);
            let copies = big_allocs::BIG.load(Ordering::Relaxed) - before;
            // One copy in total (say, into an `Arc<[u64]>`) is fine; one per thread isn't.
            check!("data = 0..1000000 (8 MB), n = 4: sums, and at most one copy of the data", (sums, copies <= 1), (vec![499_999_500_000; 4], true));
        }
        """,
    ],
    wrong=dict(
        copies_per_thread="""
            use std::thread;

            /// `n` threads each sum `data`; returns every thread's result.
            pub fn sum_everywhere(data: Vec<u64>, n: usize) -> Vec<u64> {
                let mut handles = Vec::new();
                for _ in 0..n {
                    let mine: Vec<u64> = data.iter().copied().collect();
                    handles.push(thread::spawn(move || mine.iter().sum::<u64>()));
                }
                handles.into_iter().map(|h| h.join().expect("worker panicked")).collect()
            }
        """,
        take_per_thread="""
            use std::thread;

            /// `n` threads each sum `data`; returns every thread's result.
            pub fn sum_everywhere(mut data: Vec<u64>, n: usize) -> Vec<u64> {
                let mut handles = Vec::new();
                for _ in 0..n {
                    let mine = std::mem::take(&mut data);
                    handles.push(thread::spawn(move || mine.iter().sum::<u64>()));
                }
                handles.into_iter().map(|h| h.join().expect("worker panicked")).collect()
            }
        """,
        split_between_threads="""
            use std::sync::Arc;
            use std::thread;

            /// `n` threads each sum `data`; returns every thread's result.
            pub fn sum_everywhere(data: Vec<u64>, n: usize) -> Vec<u64> {
                let data = Arc::new(data);
                let mut handles = Vec::new();
                for i in 0..n {
                    let data = Arc::clone(&data);
                    handles.push(thread::spawn(move || data.iter().skip(i).step_by(n).sum::<u64>()));
                }
                handles.into_iter().map(|h| h.join().expect("worker panicked")).collect()
            }
        """,
    ),
    hints=[("rust", "The first iteration's `move` takes `data`. What can each thread own that still points at one shared Vec?"),
           ("rust", "`Arc::clone(&data)` copies a pointer and bumps a count; the Vec itself isn't copied.")],
    notes=("`Arc::clone` is written as a path call on purpose: it reads as \"another handle\", not \"another copy of the data\".", "O(n · threads)", "O(n) once"),
    follow_up="When would you use `thread::scope` instead of `Arc` here?",
    related=["S7", "C1"],
))

# ---------------------------------------------------------------- partial moves & mem (hard)

REQ_HEAD = r"""
use std::cell::Cell;
use std::rc::Rc;

#[derive(Debug, Default)]
pub struct Request {
    pub id: u64,
    pub method: String,
    pub path: String,
    pub headers: Vec<(String, String)>,
    pub body: Option<Vec<u8>>,
}

#[derive(Debug, PartialEq)]
pub struct Routed {
    pub id: u64,
    pub route: String,
    pub content_type: Option<String>,
    pub body: Vec<u8>,
    pub trace: String,
}

/// A request checked out of a pool. Dropping it gives the slot back.
pub struct Pooled {
    pub req: Request,
    pub free_slots: Rc<Cell<usize>>,
}

impl Drop for Pooled {
    fn drop(&mut self) {
        self.free_slots.set(self.free_slots.get() + 1);
    }
}
"""

REQ_STARTER = REQ_HEAD + r"""
/// "<id> <method> <path> (<n> headers, <body length> bytes)"; a request without a body has 0 bytes.
pub fn describe(req: &Request) -> String {
    let body = req.body.unwrap_or_default();
    format!("{} {} {} ({} headers, {} bytes)", req.id, req.method, req.path, req.headers.len(), body.len())
}

/// Consumes the request. The route is "<METHOD in upper case> <path>", the content type is the value of the
/// first "content-type" header (names compared ignoring ASCII case), the body is empty without one, and the
/// trace is `describe` of the request as it came in.
pub fn route(req: Request) -> Routed {
    let Request { method, path, .. } = req;
    let route = format!("{} {path}", method.to_uppercase());
    let content_type = req.headers.into_iter().find(|(k, _)| k.eq_ignore_ascii_case("content-type")).map(|(_, v)| v);
    let body = match req.body {
        Some(b) => b,
        None => Vec::new(),
    };
    Routed { id: req.id, route, content_type, body, trace: describe(&req) }
}

/// Routes a pooled request. Its pool slot must still be given back.
pub fn route_pooled(p: Pooled) -> Routed {
    route(p.req)
}
"""

REQ_SOLUTION = REQ_HEAD + r"""
/// "<id> <method> <path> (<n> headers, <body length> bytes)"; a request without a body has 0 bytes.
pub fn describe(req: &Request) -> String {
    let body = req.body.as_ref().map_or(0, |b| b.len());
    format!("{} {} {} ({} headers, {} bytes)", req.id, req.method, req.path, req.headers.len(), body)
}

/// Consumes the request. The route is "<METHOD in upper case> <path>", the content type is the value of the
/// first "content-type" header (names compared ignoring ASCII case), the body is empty without one, and the
/// trace is `describe` of the request as it came in.
pub fn route(req: Request) -> Routed {
    let trace = describe(&req);
    let Request { method, path, .. } = req;
    let route = format!("{} {path}", method.to_uppercase());
    let content_type = req.headers.into_iter().find(|(k, _)| k.eq_ignore_ascii_case("content-type")).map(|(_, v)| v);
    let body = match req.body {
        Some(b) => b,
        None => Vec::new(),
    };
    Routed { id: req.id, route, content_type, body, trace }
}

/// Routes a pooled request. Its pool slot must still be given back.
pub fn route_pooled(mut p: Pooled) -> Routed {
    route(std::mem::take(&mut p.req))
}
"""


def req(id_, method, path, headers=(), body=None):
    hs = "vec![" + ", ".join(f'("{k}".to_string(), "{v}".to_string())' for k, v in headers) + "]"
    b = "None" if body is None else f"Some(b\"{body}\".to_vec())"
    return f'Request {{ id: {id_}, method: "{method}".to_string(), path: "{path}".to_string(), headers: {hs}, body: {b} }}'


P.append(fix(
    "fix-partial-moves", "Fix: partial moves (E0382, E0507, E0509)", "hard", "partial-moves-and-mem", ["partial moves", "E0507", "E0509", "Drop"],
    """
        Three functions, three different ownership errors. Fix them without cloning anything and without
        `unsafe`. Several moves in `route` are fine as written; keep them.

        `route_pooled` must still give the pool slot back: `Pooled`'s `Drop` has to run.
    """,
    REQ_STARTER,
    REQ_SOLUTION,
    ["use std::cell::Cell;\nuse std::rc::Rc;\n",
     T("describe_borrows", "describe(&req), then use req", "(describe(&r), r.path)", '("1 get /a (0 headers, 3 bytes)".to_string(), "/a".to_string())', setup=f"let r = {req(1, 'get', '/a', body='abc')};"),
     T("describe_no_body", "describe of a request without a body", f"describe(&{req(2, 'post', '/b', [('x', 'y')])})", '"2 post /b (1 headers, 0 bytes)"'),
     T("route_all_fields", "route(get /x, content-type json, body \"{}\")", "route(" + req(3, 'get', '/x', [('Host', 'h'), ('Content-Type', 'json')], body='{}') + ")",
       'Routed { id: 3, route: "GET /x".to_string(), content_type: Some("json".to_string()), body: b"{}".to_vec(), trace: "3 get /x (2 headers, 2 bytes)".to_string() }'),
     T("route_body_not_copied", "route keeps the request's body buffer", "(r.body.as_ptr() == ptr, r.body.len())", "(true, 5)",
       setup=f"let q = {req(4, 'put', '/p', body='hello')};\nlet ptr = q.body.as_ref().unwrap().as_ptr();\nlet r = route(q);"),
     T("pooled_gives_slot_back", "route_pooled, then check the pool", "(r.route, slots.get())", '("DELETE /d".to_string(), 1)',
       setup=f"let slots = Rc::new(Cell::new(0));\nlet r = route_pooled(Pooled {{ req: {req(5, 'delete', '/d')}, free_slots: Rc::clone(&slots) }});")],
    ["use std::cell::Cell;\nuse std::rc::Rc;\n",
     T("describe_empty_body", "describe with Some(empty body)", f"describe(&{req(6, 'get', '/', body='')})", '"6 get / (0 headers, 0 bytes)"'),
     T("describe_many_headers", "describe with 3 headers", f"describe(&{req(7, 'get', '/h', [('a', '1'), ('b', '2'), ('c', '3')], body='xy')})", '"7 get /h (3 headers, 2 bytes)"'),
     T("first_content_type_wins", "two content-type headers", f"route({req(8, 'post', '/u', [('content-type', 'a'), ('CONTENT-TYPE', 'b')])}).content_type", 'Some("a".to_string())'),
     T("no_content_type", "no content-type header", f"route({req(9, 'get', '/n', [('content-length', '0')])}).content_type", "None"),
     T("similar_header_name", "\"content-type-x\" isn't \"content-type\"", f"route({req(10, 'get', '/s', [('content-type-x', 'v')])}).content_type", "None"),
     T("route_no_body", "route without a body", f"route({req(11, 'head', '/e')})",
       'Routed { id: 11, route: "HEAD /e".to_string(), content_type: None, body: vec![], trace: "11 head /e (0 headers, 0 bytes)".to_string() }'),
     T("method_upper_ascii_and_unicode", "method \"pAtch\", path \"/日本\"", f"route({req(12, 'pAtch', '/日本')}).route", '"PATCH /日本"'),
     T("pooled_full_result", "route_pooled of post /z with a body", "r", 'Routed { id: 13, route: "POST /z".to_string(), content_type: None, body: b"q".to_vec(), trace: "13 post /z (0 headers, 1 bytes)".to_string() }',
       setup=f"let slots = Rc::new(Cell::new(0));\nlet r = route_pooled(Pooled {{ req: {req(13, 'post', '/z', body='q')}, free_slots: Rc::clone(&slots) }});"),
     T("pooled_body_not_copied", "route_pooled keeps the body buffer", "r.body.as_ptr() == ptr", "true",
       setup=f"let q = {req(14, 'put', '/q', body='data')};\nlet ptr = q.body.as_ref().unwrap().as_ptr();\nlet r = route_pooled(Pooled {{ req: q, free_slots: Rc::new(Cell::new(0)) }});"),
     T("pool_rc_released", "route_pooled drops its handle on the pool", "Rc::strong_count(&slots)", "1",
       setup=f"let slots = Rc::new(Cell::new(0));\nlet _ = route_pooled(Pooled {{ req: {req(15, 'get', '/r')}, free_slots: Rc::clone(&slots) }});"),
     r"""
     #[test]
     fn random_vs_brute_force() {
         let mut rng = anneal_prelude::Rng::new(6112);
         for _ in 0..300 {
             let id = rng.below(1000) as u64;
             let ml = rng.below(4);
             let method = rng.string(ml, "gEt");
             let pl = rng.below(4);
             let path = format!("/{}", rng.string(pl, "ab"));
             let k = rng.below(4);
             let mut headers = Vec::new();
             for _ in 0..k {
                 let name = rng.pick(&["content-type", "Content-Type", "host", "accept"]).to_string();
                 let vl = rng.below(3);
                 headers.push((name, rng.string(vl, "xy")));
             }
             let body = if rng.bool() { let n = rng.below(5); Some(rng.vec::<u8>(n, 0, 255)) } else { None };
             let trace = format!("{id} {method} {path} ({} headers, {} bytes)", headers.len(), body.as_ref().map_or(0, |b| b.len()));
             let content_type = headers.iter().find(|(k, _)| k.to_ascii_lowercase() == "content-type").map(|(_, v)| v.clone());
             let want = Routed { id, route: format!("{} {path}", method.to_uppercase()), content_type, body: body.clone().unwrap_or_default(), trace };
             let r = Request { id, method, path, headers, body };
             let input = format!("{r:?}");
             let slots = Rc::new(Cell::new(0));
             let got = if rng.bool() { route(r) } else { route_pooled(Pooled { req: r, free_slots: Rc::clone(&slots) }) };
             check!(input, got, want);
         }
     }
     """],
    [("rust", "`describe` only has `&Request`, and `unwrap_or_default` takes the `Option` by value. Read the length through a reference: `as_ref()` or `as_deref()`."),
     ("rust", "In `route`, moving fields out one by one is fine; what fails is using `req` as a whole after some of it has gone. Do that first."),
     ("rust", "You can't move a field out of a type that implements `Drop` (E0509), because `drop` needs the whole value. Swap something in its place: `Request` is `Default`.")],
    ("""A struct can be moved out of field by field: after `let Request { method, path, .. } = req;`, `req.headers`, `req.body` and the `Copy` field `req.id` are still usable, but `req` as a whole isn't (so `describe(&req)` must come first). Behind a `&`, nothing can be moved out (E0507): borrow the inside with `as_ref()`. A type that implements `Drop` can't be partially moved at all (E0509), since its destructor needs every field. Take the field with `mem::take` (or `mem::replace`) and let the rest drop normally, which returns the slot. `mem::forget(p)` would also compile and silently keep the slot.

Syntax to remember: `let Request { method, path, .. } = req;` · `let Request { ref path, .. } = *r;` binds by reference inside a by-value pattern · `req.body.as_ref().map_or(0, |b| b.len())` · `route(std::mem::take(&mut p.req))` with `mut p`.""", "O(headers + body)", "O(1) extra"),
    "Why does Rust forbid moving a field out of a `Drop` type, when it allows it for any other struct?",
    ["Partial moves: other fields stay usable, the whole value doesn't.", "Nothing moves out from behind `&` (E0507); borrow with `as_ref`.", "A `Drop` type can't be partially moved (E0509): `mem::take` the field."],
    rules=dict(methods=["clone", "cloned", "to_owned", "to_vec", "to_string"], unsafe=True),
    related=("L2", "S1"),
    wrong=dict(
        forgets_the_slot=sub(REQ_SOLUTION, "    route(std::mem::take(&mut p.req))", "    let req = std::mem::take(&mut p.req);\n    std::mem::forget(p);\n    route(req)"),
        option_iter_len=sub(REQ_SOLUTION, "let body = req.body.as_ref().map_or(0, |b| b.len());", "let body = req.body.iter().len();"),
        content_type_case_sensitive=sub(REQ_SOLUTION, 'k.eq_ignore_ascii_case("content-type")', 'k == "content-type"'),
    ),
))

QUEUE_HEAD = r"""
#[derive(Debug, PartialEq)]
pub struct Queue {
    pub name: String,
    pub items: Vec<String>,
    pub current: Option<String>,
    pub done: Vec<String>,
}

impl Queue {
    pub fn new(name: &str, items: Vec<String>) -> Self {
        Queue { name: String::from(name), items, current: None, done: Vec::new() }
    }
"""

QUEUE_STARTER = QUEUE_HEAD + r"""
    /// Moves the first item out, leaving "" in its slot so the other items keep their positions.
    /// `items` is never empty when this is called.
    pub fn take_first(&mut self) -> String {
        let first = self.items[0];
        first
    }

    /// Makes the last item current and returns the job that was current before, if any.
    pub fn start_next(&mut self) -> Option<String> {
        let previous = self.current;
        self.current = self.items.pop();
        previous
    }

    /// Moves the current job to `done`. Returns whether there was one.
    pub fn finish(&mut self) -> bool {
        match self.current {
            Some(job) => {
                self.done.push(job);
                true
            }
            None => false,
        }
    }

    /// Hands over every finished job, oldest first, leaving `done` empty.
    pub fn drain_done(&mut self) -> Vec<String> {
        self.done
    }

    /// Swaps this queue's waiting items with `other`'s. Names, current jobs and finished jobs stay put.
    pub fn swap_items(&mut self, other: &mut Queue) {
        let mine = self.items;
        self.items = other.items;
        other.items = mine;
    }
}
"""

QUEUE_SOLUTION = QUEUE_HEAD + r"""
    /// Moves the first item out, leaving "" in its slot so the other items keep their positions.
    /// `items` is never empty when this is called.
    pub fn take_first(&mut self) -> String {
        std::mem::take(&mut self.items[0])
    }

    /// Makes the last item current and returns the job that was current before, if any.
    pub fn start_next(&mut self) -> Option<String> {
        let next = self.items.pop();
        std::mem::replace(&mut self.current, next)
    }

    /// Moves the current job to `done`. Returns whether there was one.
    pub fn finish(&mut self) -> bool {
        match self.current.take() {
            Some(job) => {
                self.done.push(job);
                true
            }
            None => false,
        }
    }

    /// Hands over every finished job, oldest first, leaving `done` empty.
    pub fn drain_done(&mut self) -> Vec<String> {
        std::mem::take(&mut self.done)
    }

    /// Swaps this queue's waiting items with `other`'s. Names, current jobs and finished jobs stay put.
    pub fn swap_items(&mut self, other: &mut Queue) {
        std::mem::swap(&mut self.items, &mut other.items);
    }
}
"""


def q(items):
    return 'Queue::new("q", vec![' + ", ".join(f'"{i}".to_string()' for i in items) + "])"


P.append(fix(
    "fix-move-out-of-index", "Fix: moving out of &mut (E0507)", "hard", "partial-moves-and-mem", ["E0507", "mem::take", "mem::replace", "mem::swap", "Option::take"],
    """
        Every method here tries to move a value out of `self`, which it only borrows, so none of them compiles.
        Fix each one without cloning, without allocating anything new, and without changing what it does.
    """,
    QUEUE_STARTER,
    QUEUE_SOLUTION,
    [T("take_first_keeps_positions", "items [\"a\", \"b\"], take_first", "(q.take_first(), q.items)", '("a".to_string(), vec!["".to_string(), "b".to_string()])', setup=f"let mut q = {q(['a', 'b'])};"),
     T("start_next_twice", "items [\"a\", \"b\"], start_next twice", "(q.start_next(), q.start_next(), q.current)", '(None, Some("b".to_string()), Some("a".to_string()))', setup=f"let mut q = {q(['a', 'b'])};"),
     T("finish_moves_to_done", "start_next, finish, finish", "(q.finish(), q.finish(), q.current, q.done)", '(true, false, None, vec!["x".to_string()])', setup=f"let mut q = {q(['x'])};\nq.start_next();"),
     T("drain_done_empties", "finish two jobs, drain_done, drain_done", "(q.drain_done(), q.drain_done())", '(vec!["b".to_string(), "a".to_string()], Vec::<String>::new())',
       setup=f"let mut q = {q(['a', 'b'])};\nfor _ in 0..2 {{\n    q.start_next();\n    q.finish();\n}}"),
     T("swap_items_only", "swap_items between [\"a\"] (current \"c\") and [\"x\", \"y\"]", "(p.items, p.current, o.items, o.name)", '(vec!["x".to_string(), "y".to_string()], Some("c".to_string()), vec!["a".to_string()], "o".to_string())',
       setup='let mut p = Queue::new("p", vec!["a".to_string(), "c".to_string()]);\np.start_next();\nlet mut o = Queue::new("o", vec!["x".to_string(), "y".to_string()]);\np.swap_items(&mut o);')],
    [ALLOC_COUNTER,
     T("take_first_no_copy", "take_first returns the item's own String", "(s.as_ptr() == ptr, n)", "(true, 0)",
       setup=f"let mut q = {q(['moved'])};\nlet ptr = q.items[0].as_ptr();\nlet (s, n) = allocs(|| q.take_first());"),
     T("take_first_twice", "take_first twice on [\"a\"]", "(q.take_first(), q.take_first(), q.items.len())", '(String::from("a"), String::new(), 1)', setup=f"let mut q = {q(['a'])};"),
     T("start_next_on_empty", "no items: start_next with a current job", "(q.start_next(), q.start_next(), q.current)", '(None, Some("only".to_string()), None)', setup=f"let mut q = {q(['only'])};"),
     T("start_next_no_copy", "the previous job comes back as the same String", "(prev.map(|s| s.as_ptr() == ptr), n)", "(Some(true), 0)",
       setup=f"let mut q = {q(['b', 'a'])};\nq.start_next();\nlet ptr = q.current.as_ref().unwrap().as_ptr();\nlet (prev, n) = allocs(|| q.start_next());"),
     T("finish_without_current", "finish on a fresh queue", "(q.finish(), q.done.len())", "(false, 0)", setup=f"let mut q = {q(['a'])};"),
     T("drain_done_no_copy", "drain_done hands over the same Vec", "(v.as_ptr() == ptr, n, q.done.capacity())", "(true, 0, 0)",
       setup=f"let mut q = {q(['a', 'b', 'c'])};\nfor _ in 0..3 {{\n    q.start_next();\n    q.finish();\n}}\nlet ptr = q.done.as_ptr();\nlet (v, n) = allocs(|| q.drain_done());"),
     T("swap_items_no_copy", "swap_items moves the buffers", "(a.items.as_ptr() == pb, b.items.as_ptr() == pa, n)", "(true, true, 0)",
       setup=f"let mut a = {q(['1'])};\nlet mut b = {q(['2', '3'])};\nlet (pa, pb) = (a.items.as_ptr(), b.items.as_ptr());\nlet (_, n) = allocs(|| a.swap_items(&mut b));"),
     T("swap_keeps_done", "swap_items leaves done lists alone", "(a.done, b.done)", '(vec!["x".to_string()], Vec::<String>::new())',
       setup=f"let mut a = {q(['x'])};\na.start_next();\na.finish();\nlet mut b = {q(['y'])};\na.swap_items(&mut b);"),
     r"""
     #[test]
     fn random_vs_model() {
         let mut rng = anneal_prelude::Rng::new(6113);
         for _ in 0..300 {
             let n = rng.below(5);
             let items: Vec<String> = (0..n).map(|i| format!("j{i}")).collect();
             let mut q = Queue::new("q", items.clone());
             let (mut m_items, mut m_current, mut m_done): (Vec<String>, Option<String>, Vec<String>) = (items, None, Vec::new());
             let mut ops = Vec::new();
             for _ in 0..rng.below(10) {
                 match rng.below(4) {
                     0 if !m_items.is_empty() => {
                         ops.push("take_first");
                         let want = std::mem::replace(&mut m_items[0], String::new());
                         check!(format!("{ops:?}"), q.take_first(), want);
                     }
                     1 => {
                         ops.push("start_next");
                         let next = m_items.pop();
                         let want = std::mem::replace(&mut m_current, next);
                         check!(format!("{ops:?}"), q.start_next(), want);
                     }
                     2 => {
                         ops.push("finish");
                         let want = match m_current.take() {
                             Some(j) => {
                                 m_done.push(j);
                                 true
                             }
                             None => false,
                         };
                         check!(format!("{ops:?}"), q.finish(), want);
                     }
                     _ => {
                         ops.push("drain_done");
                         check!(format!("{ops:?}"), q.drain_done(), std::mem::take(&mut m_done));
                     }
                 }
                 check!(format!("{ops:?}, state"), (&q.items, &q.current, &q.done), (&m_items, &m_current, &m_done));
             }
         }
     }
     """],
    [("rust", "Moving out through `&mut self` would leave a hole in `self`. Each fix moves something else in at the same time."),
     ("rust", "`mem::take(&mut x)` leaves `Default::default()`; `mem::replace(&mut x, v)` leaves `v`; `mem::swap(&mut a, &mut b)` exchanges; `opt.take()` leaves `None`.")],
    ("""A `&mut` place must hold a valid value at every moment, so you can't move out of one (E0507); you can only exchange. `mem::take` swaps in the default (`String::new()` and `Vec::new()` don't allocate), `mem::replace` swaps in a value you choose, `mem::swap` exchanges two places, and `Option::take` is `mem::replace(opt, None)`. Swapping `self.items` and `other.items` borrows two different places mutably at once, which is fine; `mem::swap(self, other)` would compile too, but it swaps the names and jobs as well.

Syntax to remember: `std::mem::take(&mut self.items[0])` · `std::mem::replace(&mut self.current, next)` · `std::mem::swap(&mut self.items, &mut other.items)` · `self.current.take()`.""", "O(1) per call", "O(1)"),
    "When would `Vec::swap_remove` or `Vec::remove` be the right way to take an item instead?",
    ["You can't move out of a `&mut` place; exchange with `mem::take`, `mem::replace`, `mem::swap` or `Option::take`.", "The empty `String` and `Vec` cost nothing to swap in."],
    rules=dict(methods=["clone", "cloned", "to_owned", "to_string", "to_vec", "collect", "drain"]),
    related=("S11", "S3", "S1"),
    wrong=dict(
        remove_first=sub(QUEUE_SOLUTION, "        std::mem::take(&mut self.items[0])", "        self.items.remove(0)"),
        swaps_whole_queues=sub(QUEUE_SOLUTION, "std::mem::swap(&mut self.items, &mut other.items);", "std::mem::swap(self, other);"),
        start_takes_first=sub(QUEUE_SOLUTION, "        let next = self.items.pop();", "        let next = if self.items.is_empty() { None } else { Some(self.items.remove(0)) };"),
    ),
))

P.append(dict(
    slug="mem-replace-state", title="State transitions with mem::replace", level="hard", stage="partial-moves-and-mem", tags=["mem::replace", "enum"],
    teaches=["Moving data out of `&mut self` enum variants with `mem::replace`.", "A placeholder that costs nothing, like `String::new()`."],
    statement="""
        Advance a job one step: `Queued(name)` → `Running(name)` → `Done(name)`. `Done` stays `Done`.
        Keep the name without cloning it.
    """,
    starter="""
        #[derive(Debug, PartialEq)]
        pub enum Job {
            Queued(String),
            Running(String),
            Done(String),
        }

        pub fn advance(job: &mut Job) {
            todo!()
        }
    """,
    solution="""
        #[derive(Debug, PartialEq)]
        pub enum Job {
            Queued(String),
            Running(String),
            Done(String),
        }

        pub fn advance(job: &mut Job) {
            // Take the job out, leaving a placeholder that doesn't allocate, then write the next state.
            *job = match std::mem::replace(job, Job::Done(String::new())) {
                Job::Queued(name) => Job::Running(name),
                Job::Running(name) => Job::Done(name),
                done => done,
            };
        }
    """,
    visible=[
        T("queued_to_running", "Queued(\"build\")", '{ let mut j = Job::Queued("build".into()); advance(&mut j); j }', 'Job::Running("build".into())'),
        T("running_to_done", "Running(\"build\")", '{ let mut j = Job::Running("build".into()); advance(&mut j); j }', 'Job::Done("build".into())'),
        T("done_stays_done", "Done(\"ship\")", '{ let mut j = Job::Done("ship".into()); advance(&mut j); j }', 'Job::Done("ship".into())'),
        T("two_steps", "Queued(\"test\") advanced twice", '{ let mut j = Job::Queued("test".into()); advance(&mut j); advance(&mut j); j }', 'Job::Done("test".into())'),
        T("three_steps", "Queued(\"lint\") advanced three times", '{ let mut j = Job::Queued("lint".into()); for _ in 0..3 { advance(&mut j); } j }', 'Job::Done("lint".into())'),
    ],
    hidden=[
        T("done_stays", "Done(\"x\")", '{ let mut j = Job::Done("x".into()); advance(&mut j); j }', 'Job::Done("x".into())'),
        T("same_buffer", "name's heap buffer kept", '{ let mut j = Job::Queued(String::from("keep")); let p = match &j { Job::Queued(s) => s.as_ptr(), _ => unreachable!() }; advance(&mut j); match &j { Job::Running(s) => s.as_ptr() == p, _ => false } }', "true"),
        T("same_buffer_to_done", "Running → Done keeps the buffer", '{ let mut j = Job::Running(String::from("keep")); let p = match &j { Job::Running(s) => s.as_ptr(), _ => unreachable!() }; advance(&mut j); match &j { Job::Done(s) => s.as_ptr() == p, _ => false } }', "true"),
        T("done_keeps_buffer", "Done stays Done with the same buffer", '{ let mut j = Job::Done(String::from("keep")); let p = match &j { Job::Done(s) => s.as_ptr(), _ => unreachable!() }; advance(&mut j); match &j { Job::Done(s) => s.as_ptr() == p, _ => false } }', "true"),
        T("done_twice", "Done(\"x\") advanced twice", '{ let mut j = Job::Done("x".into()); advance(&mut j); advance(&mut j); j }', 'Job::Done("x".into())'),
        T("ten_steps", "Queued(\"q\") advanced 10 times", '{ let mut j = Job::Queued("q".into()); for _ in 0..10 { advance(&mut j); } j }', 'Job::Done("q".into())'),
        T("empty_name", "Queued(\"\")", '{ let mut j = Job::Queued(String::new()); advance(&mut j); j }', "Job::Running(String::new())"),
        T("unicode_name", "Running(\"日本 ✓\")", '{ let mut j = Job::Running("日本 ✓".into()); advance(&mut j); j }', 'Job::Done("日本 ✓".into())'),
        """
        #[test]
        fn random_vs_brute_force() {
            let mut rng = anneal_prelude::Rng::new(1115);
            for _ in 0..300 {
                let len = rng.below(5);
                let name = rng.string(len, "ab");
                let start = rng.below(3);
                let steps = rng.below(4);
                let make = |stage: usize, name: String| match stage {
                    0 => Job::Queued(name),
                    1 => Job::Running(name),
                    _ => Job::Done(name),
                };
                let mut job = make(start, name.clone());
                let input = format!("{job:?}, advanced {steps} times");
                for _ in 0..steps {
                    advance(&mut job);
                }
                check!(input, job, make((start + steps).min(2), name));
            }
        }
        """,
    ],
    wrong=dict(
        clones_the_name="""
            #[derive(Debug, PartialEq)]
            pub enum Job {
                Queued(String),
                Running(String),
                Done(String),
            }

            pub fn advance(job: &mut Job) {
                *job = match job {
                    Job::Queued(name) => Job::Running(name.clone()),
                    Job::Running(name) => Job::Done(name.clone()),
                    Job::Done(name) => Job::Done(name.clone()),
                };
            }
        """,
        done_starts_over="""
            #[derive(Debug, PartialEq)]
            pub enum Job {
                Queued(String),
                Running(String),
                Done(String),
            }

            pub fn advance(job: &mut Job) {
                *job = match std::mem::replace(job, Job::Done(String::new())) {
                    Job::Queued(name) => Job::Running(name),
                    Job::Running(name) => Job::Done(name),
                    Job::Done(name) => Job::Queued(name),
                };
            }
        """,
        done_loses_name="""
            #[derive(Debug, PartialEq)]
            pub enum Job {
                Queued(String),
                Running(String),
                Done(String),
            }

            pub fn advance(job: &mut Job) {
                let next = match std::mem::replace(job, Job::Done(String::new())) {
                    Job::Queued(name) => Job::Running(name),
                    Job::Running(name) => Job::Done(name),
                    Job::Done(_) => return,
                };
                *job = next;
            }
        """,
    ),
    hints=[("rust", "You can't move `name` out of `*job` through a `&mut`. Replace the whole value first, then match on what you got back.")],
    notes=("A test checks the String's buffer pointer: the name moves between variants without being copied.", "O(1)", "O(1)"),
    follow_up="How would you model this as a typestate (`Job<Queued>`, `Job<Running>`) instead?",
    related=["L7", "L5"],
))

P.append(dict(
    slug="option-take-batcher", title="A batcher with Option::take", level="hard", stage="partial-moves-and-mem", tags=["Option::take", "get_or_insert_with"],
    teaches=["`Option::take` moves the value out and leaves `None`.", "`get_or_insert_with` creates the value lazily."],
    statement="""
        `Batcher::new(size)` groups pushed values into batches. `push` returns a full batch when one
        completes; `flush` returns whatever is left, or `None` if nothing is.
    """,
    starter="""
        pub struct Batcher {
            current: Option<Vec<u32>>,
            size: usize,
        }

        impl Batcher {
            pub fn new(size: usize) -> Self {
                todo!()
            }

            pub fn push(&mut self, value: u32) -> Option<Vec<u32>> {
                todo!()
            }

            pub fn flush(&mut self) -> Option<Vec<u32>> {
                todo!()
            }
        }
    """,
    solution="""
        pub struct Batcher {
            current: Option<Vec<u32>>,
            size: usize,
        }

        impl Batcher {
            pub fn new(size: usize) -> Self {
                assert!(size > 0, "batch size must be at least 1");
                Batcher { current: None, size }
            }

            pub fn push(&mut self, value: u32) -> Option<Vec<u32>> {
                let batch = self.current.get_or_insert_with(|| Vec::with_capacity(self.size));
                batch.push(value);
                if batch.len() == self.size {
                    self.current.take()
                } else {
                    None
                }
            }

            pub fn flush(&mut self) -> Option<Vec<u32>> {
                self.current.take()
            }
        }
    """,
    visible=[
        T("fills", "size 2, push 1, 2, 3", "{ let mut b = Batcher::new(2); (b.push(1), b.push(2), b.push(3)) }", "(None, Some(vec![1, 2]), None)"),
        T("flush_rest", "size 3, push 7, flush twice", "{ let mut b = Batcher::new(3); b.push(7); (b.flush(), b.flush()) }", "(Some(vec![7]), None)"),
        T("exact_multiple", "size 2, push 1, 2, 3, 4", "{ let mut b = Batcher::new(2); (b.push(1), b.push(2), b.push(3), b.push(4)) }", "(None, Some(vec![1, 2]), None, Some(vec![3, 4]))"),
        T("flush_nothing", "size 2, nothing pushed", "Batcher::new(2).flush()", "None"),
        T("flush_after_batch", "size 1, push 5, flush", "{ let mut b = Batcher::new(1); (b.push(5), b.flush()) }", "(Some(vec![5]), None)"),
    ],
    hidden=[
        T("size_one", "size 1", "{ let mut b = Batcher::new(1); (b.push(5), b.flush()) }", "(Some(vec![5]), None)"),
        T("flush_empty", "nothing pushed", "Batcher::new(4).flush()", "None"),
        T("flush_after_full_batch", "size 2, push 1, 2, then flush", "{ let mut b = Batcher::new(2); b.push(1); (b.push(2), b.flush()) }", "(Some(vec![1, 2]), None)"),
        T("push_after_flush", "size 3, push 1, flush, push 2, 3, 4", "{ let mut b = Batcher::new(3); b.push(1); let f = b.flush(); (f, b.push(2), b.push(3), b.push(4)) }", "(Some(vec![1]), None, None, Some(vec![2, 3, 4]))"),
        T("size_one_every_push", "size 1, push 1, 2, 3", "{ let mut b = Batcher::new(1); (b.push(1), b.push(2), b.push(3)) }", "(Some(vec![1]), Some(vec![2]), Some(vec![3]))"),
        T("extreme_values", "size 2, push 0, u32::MAX", "{ let mut b = Batcher::new(2); (b.push(0), b.push(u32::MAX)) }", "(None, Some(vec![0, u32::MAX]))"),
        T("duplicates", "size 3, push 7, 7, 7", "{ let mut b = Batcher::new(3); (b.push(7), b.push(7), b.push(7)) }", "(None, None, Some(vec![7, 7, 7]))"),
        T("big_batches", "size 1000, push 0..2500, then flush", "{ let mut b = Batcher::new(1000); let full: Vec<Vec<u32>> = (0..2500).filter_map(|i| b.push(i)).collect(); (full.len(), full[1][999], b.flush().map(|r| (r.len(), r[0]))) }", "(2, 1999, Some((500, 2000)))"),
        """
        #[test]
        fn random_vs_model() {
            let mut rng = anneal_prelude::Rng::new(1116);
            for _ in 0..300 {
                let size = rng.below(5) + 1;
                let mut b = Batcher::new(size);
                let mut pending: Vec<u32> = Vec::new();
                let mut ops = Vec::new();
                for _ in 0..rng.below(15) {
                    if rng.below(4) == 0 {
                        ops.push("flush".to_string());
                        let want = if pending.is_empty() { None } else { Some(std::mem::take(&mut pending)) };
                        check!(format!("size = {size}, {ops:?}"), b.flush(), want);
                    } else {
                        let v = rng.below(100) as u32;
                        ops.push(format!("push {v}"));
                        pending.push(v);
                        let want = if pending.len() == size { Some(std::mem::take(&mut pending)) } else { None };
                        check!(format!("size = {size}, {ops:?}"), b.push(v), want);
                    }
                }
            }
        }
        """,
    ],
    wrong=dict(
        flush_always_some="""
            pub struct Batcher {
                current: Vec<u32>,
                size: usize,
            }

            impl Batcher {
                pub fn new(size: usize) -> Self {
                    Batcher { current: Vec::new(), size }
                }

                pub fn push(&mut self, value: u32) -> Option<Vec<u32>> {
                    self.current.push(value);
                    if self.current.len() == self.size {
                        Some(std::mem::take(&mut self.current))
                    } else {
                        None
                    }
                }

                pub fn flush(&mut self) -> Option<Vec<u32>> {
                    Some(std::mem::take(&mut self.current))
                }
            }
        """,
        off_by_one="""
            pub struct Batcher {
                current: Option<Vec<u32>>,
                size: usize,
            }

            impl Batcher {
                pub fn new(size: usize) -> Self {
                    Batcher { current: None, size }
                }

                pub fn push(&mut self, value: u32) -> Option<Vec<u32>> {
                    let batch = self.current.get_or_insert_with(Vec::new);
                    if batch.len() == self.size {
                        let full = self.current.replace(vec![value]);
                        return full;
                    }
                    batch.push(value);
                    None
                }

                pub fn flush(&mut self) -> Option<Vec<u32>> {
                    self.current.take()
                }
            }
        """,
        never_resets="""
            pub struct Batcher {
                current: Option<Vec<u32>>,
                size: usize,
            }

            impl Batcher {
                pub fn new(size: usize) -> Self {
                    Batcher { current: None, size }
                }

                pub fn push(&mut self, value: u32) -> Option<Vec<u32>> {
                    let batch = self.current.get_or_insert_with(Vec::new);
                    batch.push(value);
                    if batch.len() % self.size == 0 {
                        Some(batch[batch.len() - self.size..].to_vec())
                    } else {
                        None
                    }
                }

                pub fn flush(&mut self) -> Option<Vec<u32>> {
                    self.current.take()
                }
            }
        """,
    ),
    hints=[("rust", "`get_or_insert_with` gives you `&mut Vec<u32>`, creating it on first use."), ("rust", "When the batch is full, `take()` hands it back and resets `current` to `None`.")],
    notes=("`self.size` is read inside the closure while `self.current` is borrowed mutably; that works because the closure borrows a different field (disjoint captures in edition 2021).", "O(1) amortised", "O(size)"),
    follow_up="How would you add a time-based flush, and what would it need from the caller?",
    related=["S1", "C2"],
))

BOX_HEAD = r"""
/// A stack as a singly linked list of boxed nodes.
pub struct Stack<T> {
    head: Option<Box<Node<T>>>,
    len: usize,
}

struct Node<T> {
    value: T,
    next: Option<Box<Node<T>>>,
}
"""

BOX_SOLUTION = BOX_HEAD + r"""
impl<T> Stack<T> {
    pub fn new() -> Self {
        Stack { head: None, len: 0 }
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn push(&mut self, value: T) {
        let next = self.head.take();
        self.head = Some(Box::new(Node { value, next }));
        self.len += 1;
    }

    pub fn pop(&mut self) -> Option<T> {
        let node = self.head.take()?;
        // Moving out of a Box with `*` is allowed: the Box is consumed and its allocation freed.
        let Node { value, next } = *node;
        self.head = next;
        self.len -= 1;
        Some(value)
    }

    pub fn peek(&self) -> Option<&T> {
        self.head.as_deref().map(|node| &node.value)
    }

    /// Reverses the stack in place, reusing every node.
    pub fn reverse(&mut self) {
        let mut rest = self.head.take();
        while let Some(mut node) = rest {
            rest = node.next.take();
            node.next = self.head.take();
            self.head = Some(node);
        }
    }

    /// Moves the top node, box and all, onto `other`. Returns false when `self` is empty.
    pub fn move_top_to(&mut self, other: &mut Stack<T>) -> bool {
        match self.head.take() {
            Some(mut node) => {
                self.head = node.next.take();
                node.next = other.head.take();
                other.head = Some(node);
                self.len -= 1;
                other.len += 1;
                true
            }
            None => false,
        }
    }

    /// The values, top first.
    pub fn into_vec(mut self) -> Vec<T> {
        let mut out = Vec::with_capacity(self.len);
        while let Some(value) = self.pop() {
            out.push(value);
        }
        out
    }
}

impl<T> Drop for Stack<T> {
    /// The derived drop would recurse once per node and overflow the thread's stack on a long list.
    fn drop(&mut self) {
        let mut rest = self.head.take();
        while let Some(mut node) = rest {
            rest = node.next.take();
        }
    }
}
"""

BOX_STARTER = BOX_HEAD + r"""
impl<T> Stack<T> {
    pub fn new() -> Self {
        todo!()
    }

    pub fn len(&self) -> usize {
        todo!()
    }

    pub fn push(&mut self, value: T) {
        todo!()
    }

    pub fn pop(&mut self) -> Option<T> {
        todo!()
    }

    pub fn peek(&self) -> Option<&T> {
        todo!()
    }

    /// Reverses the stack in place, reusing every node.
    pub fn reverse(&mut self) {
        todo!()
    }

    /// Moves the top node, box and all, onto `other`. Returns false when `self` is empty.
    pub fn move_top_to(&mut self, other: &mut Stack<T>) -> bool {
        todo!()
    }

    /// The values, top first.
    pub fn into_vec(self) -> Vec<T> {
        todo!()
    }
}

impl<T> Drop for Stack<T> {
    fn drop(&mut self) {
        // TODO (left empty rather than todo!(): a panic in drop aborts the test binary)
    }
}
"""

BOX_RECURSIVE_DROP = sub(BOX_SOLUTION, """
impl<T> Drop for Stack<T> {
    /// The derived drop would recurse once per node and overflow the thread's stack on a long list.
    fn drop(&mut self) {
        let mut rest = self.head.take();
        while let Some(mut node) = rest {
            rest = node.next.take();
        }
    }
}
""", "\n")

P.append(write(
    "box-linked-stack", "Box moves: a linked stack", "hard", "partial-moves-and-mem", ["Box", "Option<Box<T>>", "Option::take", "Drop"],
    """
        Write a stack as a linked list of `Box`ed nodes. Nodes are moved, never copied or reallocated:

        - `push`, `pop`, `peek` and `len` as usual.
        - `reverse` reverses the stack in place, and `move_top_to` moves the top node onto another stack. Both
          reuse the existing boxes: **no allocation at all** (the tests count them).
        - `into_vec` returns the values top first.
        - Dropping a stack of a million nodes must not overflow the thread's stack.
    """,
    BOX_STARTER,
    BOX_SOLUTION,
    [T("push_pop", "push 1, 2, 3, then pop twice", "(s.pop(), s.pop(), s.len(), s.peek())", "(Some(3), Some(2), 1, Some(&1))",
       setup="let mut s = Stack::new();\nfor i in 1..=3 {\n    s.push(i);\n}"),
     T("reverse", "push 1, 2, 3, reverse, into_vec", "{ s.reverse(); s.into_vec() }", "vec![1, 2, 3]",
       setup="let mut s = Stack::new();\nfor i in 1..=3 {\n    s.push(i);\n}"),
     T("move_top", "a = [1, 2] (2 on top), b = [9]; move_top_to three times", "(a.move_top_to(&mut b), a.move_top_to(&mut b), a.move_top_to(&mut b), a.len(), b.into_vec())", "(true, true, false, 0, vec![1, 2, 9])",
       setup="let mut a = Stack::new();\na.push(1);\na.push(2);\nlet mut b = Stack::new();\nb.push(9);"),
     T("strings_move_through", "push \"x\", \"y\" as Strings, into_vec", "s.into_vec()", 'vec!["y".to_string(), "x".to_string()]',
       setup='let mut s = Stack::new();\ns.push("x".to_string());\ns.push("y".to_string());'),
     T("empty", "a new stack", "(s.pop(), s.peek().is_none(), s.len(), s.into_vec())", "(None, true, 0, Vec::<u8>::new())", setup="let mut s: Stack<u8> = Stack::new();")],
    [ALLOC_COUNTER,
     T("push_allocates_one_node", "push onto a stack of 3", "{ let (_, n) = allocs(|| s.push(4)); n }", "1", setup="let mut s = Stack::new();\nfor i in 1..=3 {\n    s.push(i);\n}"),
     T("reverse_allocates_nothing", "reverse a stack of 100", "{ let (_, n) = allocs(|| s.reverse()); (n, s.peek().copied(), s.len()) }", "(0, Some(0), 100)",
       setup="let mut s = Stack::new();\nfor i in 0..100 {\n    s.push(i);\n}"),
     T("move_top_allocates_nothing", "move_top_to 5 times", "{ let (_, n) = allocs(|| for _ in 0..5 { a.move_top_to(&mut b); }); (n, a.len(), b.len()) }", "(0, 5, 5)",
       setup="let mut a = Stack::new();\nfor i in 0..10 {\n    a.push(i);\n}\nlet mut b = Stack::new();"),
     T("value_stays_in_its_box", "the top value's address before and after two reverses", "{ let before = s.peek().map(|v| v as *const String); s.reverse(); s.reverse(); s.peek().map(|v| v as *const String) == before }", "true",
       setup='let mut s = Stack::new();\nfor w in ["a", "b", "c"] {\n    s.push(w.to_string());\n}'),
     T("reverse_empty_and_single", "reverse [] and [7]", "{ e.reverse(); o.reverse(); (e.len(), o.into_vec()) }", "(0, vec![7])",
       setup="let mut e: Stack<i32> = Stack::new();\nlet mut o = Stack::new();\no.push(7);"),
     T("len_after_moves", "a = [1, 2, 3], move two to b, pop from b", "{ a.move_top_to(&mut b); a.move_top_to(&mut b); (b.pop(), a.len(), b.len()) }", "(Some(2), 1, 1)",
       setup="let mut a = Stack::new();\nfor i in 1..=3 {\n    a.push(i);\n}\nlet mut b = Stack::new();"),
     T("drops_every_value", "Rc values: drop a stack of 3 clones", "{ drop(s); std::rc::Rc::strong_count(&rc) }", "1",
       setup="let rc = std::rc::Rc::new(());\nlet mut s = Stack::new();\nfor _ in 0..3 {\n    s.push(rc.clone());\n}"),
     T("peek_after_pop", "push a, b; pop; peek", '{ s.pop(); s.peek().cloned() }', 'Some("a".to_string())',
       setup='let mut s = Stack::new();\ns.push("a".to_string());\ns.push("b".to_string());'),
     r"""
     #[test]
     fn drop_a_million_nodes() {
         // Runs on its own thread with a small stack, so a recursive drop overflows.
         let t = std::thread::Builder::new().stack_size(256 * 1024).spawn(|| {
             let mut s = Stack::new();
             for i in 0..1_000_000u64 {
                 s.push(i);
             }
             let top = s.peek().copied();
             drop(s);
             top
         });
         check!("drop a stack of 1000000 nodes", t.unwrap().join().ok(), Some(Some(999_999)));
     }

     #[test]
     fn into_vec_a_million_nodes() {
         let mut s = Stack::new();
         for i in 0..1_000_000u64 {
             s.push(i);
         }
         s.reverse();
         let v = s.into_vec();
         check!("1000000 nodes, reversed, into_vec", (v.len(), v[0], v[999_999]), (1_000_000, 0, 999_999));
     }

     #[test]
     fn random_vs_vec_model() {
         let mut rng = anneal_prelude::Rng::new(6114);
         for _ in 0..300 {
             let (mut a, mut b) = (Stack::new(), Stack::new());
             let (mut ma, mut mb): (Vec<i32>, Vec<i32>) = (Vec::new(), Vec::new());
             let mut ops = Vec::new();
             for _ in 0..rng.below(16) {
                 match rng.below(5) {
                     0 | 1 => {
                         let v = rng.int(0, 9) as i32;
                         ops.push(format!("a.push({v})"));
                         a.push(v);
                         ma.push(v);
                     }
                     2 => {
                         ops.push("a.pop()".to_string());
                         check!(format!("{ops:?}"), a.pop(), ma.pop());
                     }
                     3 => {
                         ops.push("a.reverse()".to_string());
                         a.reverse();
                         ma.reverse();
                     }
                     _ => {
                         ops.push("a.move_top_to(b)".to_string());
                         let want = match ma.pop() {
                             Some(v) => {
                                 mb.push(v);
                                 true
                             }
                             None => false,
                         };
                         check!(format!("{ops:?}"), a.move_top_to(&mut b), want);
                     }
                 }
                 check!(format!("{ops:?}: len, peek"), (a.len(), a.peek(), b.len(), b.peek()), (ma.len(), ma.last(), mb.len(), mb.last()));
             }
             let (va, vb): (Vec<i32>, Vec<i32>) = (ma.iter().rev().copied().collect(), mb.iter().rev().copied().collect());
             check!(format!("{ops:?}: into_vec"), (a.into_vec(), b.into_vec()), (va, vb));
         }
     }
     """],
    [("rust", "Every step is `Option::take` plus assignment: take the head, then put nodes back where they belong. `*node` moves the node out of its `Box` (only `Box` allows that)."),
     ("rust", "`peek` borrows through the box: `self.head.as_deref().map(|n| &n.value)`."),
     ("rust", "The compiler's drop for `Option<Box<Node>>` drops `next` inside `next` inside ...: one stack frame per node. Unlink the nodes in a loop in your own `Drop`. With a `Drop` impl, `into_vec(self)` can't move `self.head` out any more (E0509): take it, or `pop` in a loop.")],
    ("""`Box<T>` owns one heap allocation, and moving the `Box` moves only the pointer: relinking nodes never touches the allocator, which the tests check by counting allocations and by the top value's address. `*boxed` moves the value out of a `Box` (the one smart pointer that allows it) and frees the allocation, which `pop` wants and `reverse` must avoid, so `reverse` and `move_top_to` relink boxes with `take()`. The compiler-generated drop of a list recurses once per node; a long list overflows the stack, hence the loop in `Drop`. And once `Stack` implements `Drop`, its fields can't be moved out of a `self` by value (E0509), so `into_vec` takes `mut self` and pops.

Syntax to remember: `let Node { value, next } = *node;` · `self.head.as_deref().map(|n| &n.value)` · `while let Some(mut node) = rest { rest = node.next.take(); ... }`.""", "O(1) push, pop, peek, move; O(n) reverse, drop", "O(n)"),
    "How would you add `iter()` returning `impl Iterator<Item = &T>`, and why does `iter_mut` need `take()` as well?",
    ["`Box` moves are pointer moves: relinking reuses the allocation.", "`*boxed` moves a value out of a `Box`; `Option::take` moves boxes out of `&mut` fields.", "Long linked lists need an iterative `Drop`."],
    related=("D5", "S7"),
    wrong=dict(
        reverse_by_pop_push=sub(BOX_SOLUTION, """        let mut rest = self.head.take();
        while let Some(mut node) = rest {
            rest = node.next.take();
            node.next = self.head.take();
            self.head = Some(node);
        }
    }

    /// Moves""", """        let mut out = Stack::new();
        while let Some(v) = self.pop() {
            out.push(v);
        }
        std::mem::swap(self, &mut out);
    }

    /// Moves"""),
        move_by_pop_push=sub(BOX_SOLUTION, """        match self.head.take() {
            Some(mut node) => {
                self.head = node.next.take();
                node.next = other.head.take();
                other.head = Some(node);
                self.len -= 1;
                other.len += 1;
                true
            }
            None => false,
        }""", """        match self.pop() {
            Some(v) => {
                other.push(v);
                true
            }
            None => false,
        }"""),
        recursive_drop=sub(BOX_RECURSIVE_DROP, """    pub fn into_vec(mut self) -> Vec<T> {""", """    pub fn into_vec(mut self) -> Vec<T> {"""),
        len_not_moved=sub(BOX_SOLUTION, "                self.len -= 1;\n                other.len += 1;\n", ""),
    ),
))

P.append(dict(
    slug="rollback-guard", title="Roll back on panic with a guard", level="hard", stage="partial-moves-and-mem", tags=["Drop", "panic safety", "guards"],
    teaches=["A guard armed before the risky work and disarmed after it.", "Panic safety: leave data in a valid state however a function exits."],
    statement="""
        `with_rollback(v, f)` lets `f` push to `v`. If `f` panics, `v` must be truncated back to its
        original length before the panic continues; if `f` returns normally, its pushes stay.
    """,
    starter="""
        pub fn with_rollback(v: &mut Vec<i32>, f: impl FnOnce(&mut Vec<i32>)) {
            todo!()
        }
    """,
    solution="""
        pub fn with_rollback(v: &mut Vec<i32>, f: impl FnOnce(&mut Vec<i32>)) {
            struct Guard<'a> {
                v: &'a mut Vec<i32>,
                len: usize,
                armed: bool,
            }

            impl Drop for Guard<'_> {
                fn drop(&mut self) {
                    if self.armed {
                        self.v.truncate(self.len);
                    }
                }
            }

            let len = v.len();
            let mut guard = Guard { v, len, armed: true };
            f(guard.v);
            // Reached only if `f` returned: keep its pushes.
            guard.armed = false;
        }
    """,
    visible=[
        """
        use std::panic::{AssertUnwindSafe, catch_unwind};

        #[test]
        fn panic_rolls_back() {
            let mut v = vec![1];
            let r = catch_unwind(AssertUnwindSafe(|| {
                with_rollback(&mut v, |v| {
                    v.push(2);
                    v.push(3);
                    panic!("boom");
                })
            }));
            check!("f pushes 2 and 3, then panics", (r.is_err(), v), (true, vec![1]));
        }

        #[test]
        fn success_keeps_pushes() {
            let mut v = vec![1];
            with_rollback(&mut v, |v| v.push(2));
            check!("f pushes 2 and returns", v, vec![1, 2]);
        }

        #[test]
        fn panic_without_pushes() {
            let mut v = vec![1, 2];
            let r = catch_unwind(AssertUnwindSafe(|| with_rollback(&mut v, |_| panic!("boom"))));
            check!("f panics at once", (r.is_err(), v), (true, vec![1, 2]));
        }

        #[test]
        fn success_on_empty() {
            let mut v = vec![];
            with_rollback(&mut v, |v| {
                v.push(1);
                v.push(2);
            });
            check!("v = [], f pushes 1 and 2", v, vec![1, 2]);
        }

        #[test]
        fn rollback_keeps_earlier_values() {
            let mut v = vec![7, 8, 9];
            let _ = catch_unwind(AssertUnwindSafe(|| {
                with_rollback(&mut v, |v| {
                    v.push(10);
                    panic!("boom");
                })
            }));
            check!("v = [7, 8, 9], f pushes 10 then panics", v, vec![7, 8, 9]);
        }
        """,
    ],
    hidden=[
        """
        use std::panic::{AssertUnwindSafe, catch_unwind};

        #[test]
        fn nested_rollbacks() {
            let mut v = vec![];
            with_rollback(&mut v, |v| {
                v.push(1);
                let _ = catch_unwind(AssertUnwindSafe(|| with_rollback(v, |v| {
                    v.push(2);
                    panic!("inner");
                })));
                v.push(3);
            });
            check!("outer keeps 1 and 3; inner rolls back 2", v, vec![1, 3]);
        }

        #[test]
        fn panic_keeps_propagating() {
            let mut v = vec![1];
            let r = catch_unwind(AssertUnwindSafe(|| with_rollback(&mut v, |_| panic!("boom"))));
            let msg = r.err().and_then(|e| e.downcast_ref::<&str>().map(|s| s.to_string()));
            check!("f panics with \\"boom\\"", msg, Some("boom".to_string()));
        }

        #[test]
        fn outer_panic_undoes_inner_success() {
            let mut v = vec![0];
            let _ = catch_unwind(AssertUnwindSafe(|| {
                with_rollback(&mut v, |v| {
                    with_rollback(v, |v| v.push(1));
                    v.push(2);
                    panic!("outer");
                })
            }));
            check!("inner succeeds, outer panics", v, vec![0]);
        }

        #[test]
        fn empty_and_panic() {
            let mut v = vec![];
            let _ = catch_unwind(AssertUnwindSafe(|| with_rollback(&mut v, |v| {
                v.push(9);
                panic!("boom");
            })));
            check!("v = [], f pushes 9 then panics", v, Vec::<i32>::new());
        }

        #[test]
        fn success_without_pushes() {
            let mut v = vec![4, 5];
            with_rollback(&mut v, |_| {});
            check!("v = [4, 5], f does nothing", v, vec![4, 5]);
        }

        #[test]
        fn success_then_panic() {
            let mut v = vec![];
            with_rollback(&mut v, |v| v.push(1));
            let _ = catch_unwind(AssertUnwindSafe(|| with_rollback(&mut v, |v| {
                v.push(2);
                panic!("boom");
            })));
            with_rollback(&mut v, |v| v.push(3));
            check!("push 1 ok, push 2 panics, push 3 ok", v, vec![1, 3]);
        }

        #[test]
        fn big_rollback() {
            let mut v: Vec<i32> = (0..1000).collect();
            let _ = catch_unwind(AssertUnwindSafe(|| with_rollback(&mut v, |v| {
                v.extend(0..200_000);
                panic!("boom");
            })));
            check!("v = 0..1000, f pushes 200000 then panics", (v.len(), v[999]), (1000, 999));
        }

        #[test]
        fn random_vs_model() {
            let mut rng = anneal_prelude::Rng::new(1117);
            for _ in 0..60 {
                let len = rng.below(5);
                let mut v: Vec<i32> = rng.vec(len, -9, 9);
                let adds = rng.below(5);
                let pushes: Vec<i32> = rng.vec(adds, -9, 9);
                let fails = rng.bool();
                let mut want = v.clone();
                if !fails {
                    want.extend(&pushes);
                }
                let input = format!("v = {v:?}, f pushes {pushes:?}, panics: {fails}");
                let _ = catch_unwind(AssertUnwindSafe(|| with_rollback(&mut v, |v| {
                    v.extend(&pushes);
                    if fails {
                        panic!("random");
                    }
                })));
                check!(input, v, want);
            }
        }
        """,
    ],
    wrong=dict(
        swallows_the_panic="""
            use std::panic::{AssertUnwindSafe, catch_unwind};

            pub fn with_rollback(v: &mut Vec<i32>, f: impl FnOnce(&mut Vec<i32>)) {
                let len = v.len();
                let r = catch_unwind(AssertUnwindSafe(|| f(v)));
                if r.is_err() {
                    v.truncate(len);
                }
            }
        """,
        always_truncates="""
            pub fn with_rollback(v: &mut Vec<i32>, f: impl FnOnce(&mut Vec<i32>)) {
                struct Guard<'a> {
                    v: &'a mut Vec<i32>,
                    len: usize,
                }

                impl Drop for Guard<'_> {
                    fn drop(&mut self) {
                        self.v.truncate(self.len);
                    }
                }

                let len = v.len();
                let guard = Guard { v, len };
                f(guard.v);
            }
        """,
        clears_on_panic="""
            pub fn with_rollback(v: &mut Vec<i32>, f: impl FnOnce(&mut Vec<i32>)) {
                struct Guard<'a> {
                    v: &'a mut Vec<i32>,
                    armed: bool,
                }

                impl Drop for Guard<'_> {
                    fn drop(&mut self) {
                        if self.armed {
                            self.v.clear();
                        }
                    }
                }

                let mut guard = Guard { v, armed: true };
                f(guard.v);
                guard.armed = false;
            }
        """,
    ),
    hints=[("approach", "Remember the length. What runs whether `f` returns or panics?"),
           ("rust", "A local struct with a `Drop` impl and an `armed` flag: disarm it only after `f` returns.")],
    notes=("Unwinding drops `guard`, which truncates. On success the flag is cleared first, so the drop does nothing. std uses the same pattern inside `Vec::retain` and sort.", "O(1) plus the truncate", "O(1)"),
    follow_up="Why would `mem::forget` on this guard be harmless, but on some other guards be a soundness bug?",
    related=["S3", "Y2"],
))

STAGES = [
    ("moves-and-copy", "Moves & Copy", "easy"),
    ("passing-values", "Passing values", "easy"),
    ("clones-and-drops", "Clones & drops", "medium"),
    ("closures-take-ownership", "Closures take ownership", "medium"),
    ("partial-moves-and-mem", "Partial moves & mem", "hard"),
]

# Optional fields: drop empty ones so problem.toml stays tidy.
for p in P:
    for k in ("source", "rules", "wrong", "examples"):
        if not p.get(k):
            p.pop(k, None)

if __name__ == "__main__":
    n = write_track("l1-ownership-moves", "L1", "Ownership & moves", "L", "core", 1,
                    "Every value has one owner. Moves, copies, clones, drops and the `mem` functions that move data out of places you only borrow.",
                    STAGES, P)
    print("L1", n)
