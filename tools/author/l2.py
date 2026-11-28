from author import T, write_track

P = []


def fix(slug, title, level, stage, tags, statement, starter, solution, visible, hidden, hints, notes, follow_up, teaches, rules=None, related=("L2",), wrong=None):
    return dict(slug=slug, title=title, mode="fix", level=level, stage=stage, tags=tags, statement=statement, starter=starter,
                solution=solution, visible=visible, hidden=hidden, hints=hints, notes=notes, follow_up=follow_up,
                teaches=teaches, rules=rules, related=list(related), wrong=wrong)


def write(slug, title, level, stage, tags, statement, starter, solution, visible, hidden, hints, notes, follow_up, teaches, related=("L2",), wrong=None):
    return dict(slug=slug, title=title, level=level, stage=stage, tags=tags, statement=statement, starter=starter,
                solution=solution, visible=visible, hidden=hidden, hints=hints, notes=notes, follow_up=follow_up,
                teaches=teaches, related=list(related), wrong=wrong)


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


# ---------------------------------------------------------------- shared vs unique (easy)

SERIES_HEAD = r"""
/// A time series: `points[i]` was reported by `labels[i]`.
pub struct Series {
    pub points: Vec<i64>,
    pub labels: Vec<String>,
}

impl Series {
    pub fn new() -> Self {
        Series { points: Vec::new(), labels: Vec::new() }
    }

    /// Records `x`, reported by `label`. Returns the label of the record holder after this point (the `String`
    /// stored in `labels`, not a copy) and how far `x` beat the old record: `None` when it didn't beat it or when
    /// it's the first point. The record holder is the first label that reported the maximum, so a tie doesn't
    /// take the record.
"""

SERIES_STARTER = SERIES_HEAD + r"""    pub fn record(&mut self, x: i64, label: String) -> (&str, Option<i64>) {
        let best = self.points.iter().max();
        let holder = best.map(|b| &self.labels[self.points.iter().position(|p| p == b).unwrap()]);
        self.points.push(x);
        self.labels.push(label);
        match best {
            Some(&b) if x <= b => (holder.unwrap(), None),
            Some(&b) => (&label, Some(x - b)),
            None => (&label, None),
        }
    }
}
"""

SERIES_SOLUTION = SERIES_HEAD + r"""    pub fn record(&mut self, x: i64, label: String) -> (&str, Option<i64>) {
        let best = self.points.iter().copied().max();
        let holder = best.map(|b| self.points.iter().position(|&p| p == b).unwrap());
        self.points.push(x);
        self.labels.push(label);
        let newest = self.labels.len() - 1;
        match (best, holder) {
            (Some(b), Some(i)) if x <= b => (&self.labels[i], None),
            (Some(b), _) => (&self.labels[newest], Some(x - b)),
            _ => (&self.labels[newest], None),
        }
    }
}
"""

SERIES_RUN = r"""
/// Records every (x, label) in order and returns the last result, owned.
fn run(points: &[(i64, &str)]) -> (String, Option<i64>) {
    let mut s = Series::new();
    let mut last = (String::new(), None);
    for &(x, l) in points {
        let (h, g) = s.record(x, l.to_string());
        last = (h.to_string(), g);
    }
    last
}
"""


def series_case(name, pts, holder, gain):
    desc = ", ".join(f'({x}, "{l}")' for x, l in pts)
    call = "run(&[" + ", ".join(f'({x}, "{l}")' for x, l in pts) + "])"
    return T(name, f"record {desc}", call, f'("{holder}".to_string(), {gain})')


P.append(fix(
    "fix-push-while-holding-a-reference", "Fix: push while holding a reference", "easy", "shared-vs-unique", ["E0502", "E0382", "indices over references"],
    """
        `Series::record` doesn't compile. It reads the current record before pushing the new point, and answers
        with a reference to the record holder's label. Fix it without copying any label: the `&str` it returns
        must be the `String` stored in `labels`.
    """,
    SERIES_STARTER,
    SERIES_SOLUTION,
    [SERIES_RUN,
     series_case("first_point", [(5, "a")], "a", "None"),
     series_case("new_record", [(5, "a"), (9, "b")], "b", "Some(4)"),
     series_case("below_the_record", [(5, "a"), (3, "b")], "a", "None"),
     series_case("tie_keeps_the_holder", [(5, "a"), (5, "b")], "a", "None"),
     series_case("first_to_reach_the_max", [(5, "a"), (9, "b"), (9, "c"), (2, "d")], "b", "None"),
     series_case("negatives", [(-5, "a"), (-2, "b")], "b", "Some(3)")],
    [SERIES_RUN,
     series_case("record_after_a_tie", [(4, "a"), (4, "b"), (6, "c")], "c", "Some(2)"),
     series_case("same_label_twice", [(1, "x"), (3, "x"), (2, "y")], "x", "None"),
     series_case("empty_label", [(1, ""), (0, "z")], "", "None"),
     series_case("chain_of_records", [(1, "a"), (2, "b"), (4, "c"), (8, "d")], "d", "Some(4)"),
     series_case("far_apart", [(-1_000_000_000_000, "lo"), (1_000_000_000_000, "hi")], "hi", "Some(2000000000000)"),
     series_case("all_equal", [(0, "p"), (0, "q"), (0, "r")], "p", "None"),
     series_case("unicode_labels", [(2, "é"), (7, "日本")], "日本", "Some(5)"),
     r"""
     #[test]
     fn holder_is_the_stored_label() {
         let mut s = Series::new();
         let seven = "seven".to_string();
         let moved = seven.as_ptr();
         s.record(7, seven);
         check!("record (7, \"seven\"): the label String is moved in, not copied", s.labels[0].as_ptr() == moved, true);
         let p = s.record(3, "three".to_string()).0.as_ptr();
         check!("then (3, \"three\"): the returned &str is labels[0] itself", p == s.labels[0].as_ptr(), true);
         let p = s.record(8, "eight".to_string()).0.as_ptr();
         check!("then (8, \"eight\"): the returned &str is labels[2] itself", p == s.labels[2].as_ptr(), true);
         check!("points afterwards", s.points.clone(), vec![7, 3, 8]);
     }

     #[test]
     fn random_vs_model() {
         let mut rng = anneal_prelude::Rng::new(6201);
         for _ in 0..300 {
             let n = 1 + rng.below(8);
             let mut s = Series::new();
             let (mut pts, mut labs): (Vec<i64>, Vec<String>) = (Vec::new(), Vec::new());
             let mut log = Vec::new();
             for i in 0..n {
                 let x = rng.int(-4, 4);
                 let l = format!("{}{i}", rng.string(1, "ab"));
                 log.push(format!("({x}, {l:?})"));
                 let want = match pts.iter().copied().max() {
                     Some(b) if x <= b => (labs[pts.iter().position(|&p| p == b).unwrap()].clone(), None),
                     Some(b) => (l.clone(), Some(x - b)),
                     None => (l.clone(), None),
                 };
                 pts.push(x);
                 labs.push(l.clone());
                 let (h, g) = s.record(x, l);
                 check!(format!("record {}", log.join(", ")), (h.to_string(), g), want);
             }
         }
     }
     """],
    [("rust", "`best` is an `Option<&i64>` into `points` and `holder` an `Option<&String>` into `labels`, and both are read after the pushes. A push can reallocate the `Vec` and leave them dangling. What do you actually need from each one?"),
     ("rust", "An `i64` is `Copy`: keep the number, not a reference to it. For the label, keep its index. An index stays valid across a push, and you can borrow `labels[i]` again once the pushes are done."),
     ("rust", "`label` is moved into `labels`, so `&label` afterwards is a use after move, and would point at a parameter anyway. The newest label is `labels[len - 1]`.")],
    ("""A shared borrow into a `Vec` forbids mutating that `Vec` until the borrow's last use, because `push` may reallocate. The fix keeps what survives the push: `copied()` turns `Option<&i64>` into `Option<i64>`, and the holder becomes an index. Borrow the stored label again after both pushes; the returned `&str` then borrows `self`, which is what the elided lifetime says.

Note what does compile: `holder` borrowing `labels` doesn't conflict with `self.points.push`, because the two fields are disjoint. Only the push to the same field conflicts.

Aside: with `Vec<String>` the label's bytes live in their own heap buffer and don't move when `labels` reallocates, yet the borrow checker still rejects the `&String`: it tracks the borrow of `labels`, not where the bytes are.""", "O(n) per record (the scan)", "O(1) extra"),
    "`Vec<String>` reallocating moves the `String` headers but not their bytes. Could a `&str` into a label survive a push in principle, and what would it take to express that safely?",
    ["A shared borrow into a Vec blocks every mutation of that Vec until its last use.", "Keep `Copy` values and indices across a mutation, not references.", "Disjoint fields borrow independently."],
    rules=dict(methods=["clone", "to_owned", "to_string"]),
    wrong=dict(
        tie_takes_the_record=sub(SERIES_SOLUTION, "if x <= b", "if x < b"),
        last_holder_of_the_max=sub(SERIES_SOLUTION, "position(|&p| p == b)", "rposition(|&p| p == b)"),
        leaked_copy=sub(SERIES_SOLUTION, "(Some(b), Some(i)) if x <= b => (&self.labels[i], None),", "(Some(b), Some(i)) if x <= b => (Box::leak(self.labels[i].as_str().into()), None),"),
    ),
))

INBOX_HEAD = r"""
#[derive(Debug, PartialEq)]
pub struct Msg {
    pub id: u32,
    pub from: String,
    pub body: String,
    pub read: bool,
    pub reply: Option<String>,
}

/// Messages in arrival order, oldest first. Ids are unique.
pub struct Inbox {
    msgs: Vec<Msg>,
}
"""

INBOX_IMPL = r"""
impl Inbox {
    pub fn new() -> Self {
        Inbox { msgs: Vec::new() }
    }

    pub fn push(&mut self, id: u32, from: &str, body: &str) {
        self.msgs.push(Msg { id, from: from.to_string(), body: body.to_string(), read: false, reply: None });
    }

    pub fn get(&self, id: u32) -> Option<&Msg> {
        self.msgs.iter().find(|m| m.id == id)
    }

    pub fn get_mut(&mut self, id: u32) -> Option<&mut Msg> {
        self.msgs.iter_mut().find(|m| m.id == id)
    }

    pub fn unread(&self) -> Vec<&Msg> {
        self.msgs.iter().filter(|m| !m.read).collect()
    }

    pub fn reply_to(&self, id: u32) -> Option<&str> {
        self.get(id)?.reply.as_deref()
    }

    pub fn mark_read(&mut self, id: u32) -> bool {
        match self.get_mut(id) {
            Some(m) if !m.read => {
                m.read = true;
                true
            }
            _ => false,
        }
    }

    pub fn reply_mut(&mut self, id: u32) -> Option<&mut String> {
        Some(self.get_mut(id)?.reply.get_or_insert_with(String::new))
    }

    pub fn newest_unread_mut(&mut self) -> Option<&mut Msg> {
        self.msgs.iter_mut().rev().find(|m| !m.read)
    }
}
"""

INBOX_TAIL = r"""
/// Marks every unread message from `boss` read, appends "on it" to the reply of the newest message still
/// unread (if there is one), and returns the ids still unread, oldest first.
pub fn triage(inbox: &mut Inbox, boss: &str) -> Vec<u32> {
    let from_boss: Vec<u32> = inbox.unread().iter().filter(|m| m.from == boss).map(|m| m.id).collect();
    for id in from_boss {
        inbox.mark_read(id);
    }
    if let Some(m) = inbox.newest_unread_mut() {
        m.reply.get_or_insert_with(String::new).push_str("on it");
    }
    inbox.unread().iter().map(|m| m.id).collect()
}
"""

INBOX_SOLUTION = INBOX_HEAD + INBOX_IMPL + INBOX_TAIL
INBOX_SETUP = 'let mut inbox = Inbox::new();\ninbox.push(1, "ann", "hello");\ninbox.push(2, "boss", "report?");\ninbox.push(3, "bob", "lunch");'
INBOX_DESC = 'inbox 1 ann "hello", 2 boss "report?", 3 bob "lunch"'

P.append(fix(
    "many-readers-one-writer", "Many readers, one writer", "easy", "shared-vs-unique", ["&self vs &mut self", "Option<&T>", "as_deref", "get_or_insert_with", "iter_mut"],
    """
        `triage` uses an `Inbox` API that doesn't exist yet. Write `impl Inbox` with these methods. Pick each
        receiver and return type yourself: a method that only reads must be callable while other readers are
        alive (the tests hold two at once), and nothing may copy a message or its text.

        - `new()` and `push(id, from, body)`: a new message is unread and has no reply.
        - `get(id)`: the message, borrowed. `get_mut(id)`: the message, for editing.
        - `unread()`: the unread messages, borrowed, oldest first, in a `Vec`.
        - `reply_to(id)`: the reply's text as a string slice, `None` if there's no such message or no reply.
        - `mark_read(id)`: marks it read and returns whether it was unread (`false` for an unknown id).
        - `reply_mut(id)`: the reply for editing in place, created empty when the message has none yet; `None`
          for an unknown id.
        - `newest_unread_mut()`: the most recent unread message, for editing.
    """,
    INBOX_HEAD + "\n// TODO: impl Inbox.\n" + INBOX_TAIL,
    INBOX_SOLUTION,
    [T("readers_together", f"{INBOX_DESC}; reply to 2 is \"on it\"; get(1).body and reply_to(2) held at once",
       "(first.map(|m| m.body.as_str()), second)", '(Some("hello"), Some("on it"))',
       setup=INBOX_SETUP + '\ninbox.reply_mut(2).unwrap().push_str("on it");\nlet first = inbox.get(1);\nlet second = inbox.reply_to(2);'),
     T("mark_read_says_if_it_changed", f"{INBOX_DESC}; mark_read(1) twice, then mark_read(9)", "(inbox.mark_read(1), inbox.mark_read(1), inbox.mark_read(9))", "(true, false, false)", setup=INBOX_SETUP),
     T("reply_mut_creates_then_edits", f"{INBOX_DESC}; reply_mut(3) += \"no\", then += \"pe\"",
       "(inbox.reply_to(3), inbox.reply_to(1))", '(Some("nope"), None)',
       setup=INBOX_SETUP + '\ninbox.reply_mut(3).unwrap().push_str("no");\ninbox.reply_mut(3).unwrap().push_str("pe");'),
     T("newest_unread_skips_read", f"{INBOX_DESC}; mark_read(3); newest_unread_mut", "inbox.newest_unread_mut().map(|m| m.id)", "Some(2)",
       setup=INBOX_SETUP + "\ninbox.mark_read(3);"),
     T("triage_example", f"{INBOX_DESC}; triage(boss = \"boss\")", '(triage(&mut inbox, "boss"), inbox.reply_to(3), inbox.reply_to(2))', '(vec![1, 3], Some("on it"), None)', setup=INBOX_SETUP),
     T("unknown_ids", f"{INBOX_DESC}; id 9", "(inbox.reply_mut(9).is_none(), inbox.get_mut(9).is_none(), inbox.get(9).is_none(), inbox.reply_to(9))", "(true, true, true, None)", setup=INBOX_SETUP)],
    [T("empty_inbox", "new inbox", "(inbox.unread().len(), inbox.newest_unread_mut().is_none(), triage(&mut inbox, \"x\"))", "(0, true, vec![])", setup="let mut inbox = Inbox::new();"),
     T("unread_oldest_first", f"{INBOX_DESC}; mark_read(2)", "inbox.unread().iter().map(|m| m.id).collect::<Vec<_>>()", "vec![1, 3]", setup=INBOX_SETUP + "\ninbox.mark_read(2);"),
     T("reply_mut_keeps_existing", f"{INBOX_DESC}; reply_mut(1) = \"a\", then reply_mut(1) += \"b\"", "inbox.reply_to(1)", 'Some("ab")',
       setup=INBOX_SETUP + '\n*inbox.reply_mut(1).unwrap() = "a".to_string();\ninbox.reply_mut(1).unwrap().push_str("b");'),
     T("empty_reply_is_some", f"{INBOX_DESC}; reply_mut(1) without writing", "inbox.reply_to(1)", 'Some("")', setup=INBOX_SETUP + "\ninbox.reply_mut(1);"),
     T("get_mut_edits_in_place", f"{INBOX_DESC}; get_mut(2).body = \"done\"", "inbox.get(2).map(|m| m.body.as_str())", 'Some("done")',
       setup=INBOX_SETUP + '\ninbox.get_mut(2).unwrap().body = "done".to_string();'),
     T("all_read", f"{INBOX_DESC}; mark_read 1, 2, 3", "(inbox.newest_unread_mut().is_none(), inbox.unread().len())", "(true, 0)",
       setup=INBOX_SETUP + "\nfor id in 1..=3 {\n    inbox.mark_read(id);\n}"),
     T("triage_everything_from_boss", "inbox 1 boss, 2 boss; triage(\"boss\")", '(triage(&mut inbox, "boss"), inbox.reply_to(1), inbox.reply_to(2))', "(vec![], None, None)",
       setup='let mut inbox = Inbox::new();\ninbox.push(1, "boss", "a");\ninbox.push(2, "boss", "b");'),
     T("triage_appends_to_existing_reply", f"{INBOX_DESC}; reply to 3 is \"ok, \"; triage(\"nobody\")", '(triage(&mut inbox, "nobody"), inbox.reply_to(3))', '(vec![1, 2, 3], Some("ok, on it"))',
       setup=INBOX_SETUP + '\ninbox.reply_mut(3).unwrap().push_str("ok, ");'),
     T("triage_skips_already_read_boss_mail", f"{INBOX_DESC}; mark_read(2); triage(\"boss\"); mark_read(2)", '{ triage(&mut inbox, "boss"); inbox.mark_read(2) }', "false", setup=INBOX_SETUP + "\ninbox.mark_read(2);"),
     r"""
     #[test]
     fn many_readers_at_once() {
         let mut inbox = Inbox::new();
         inbox.push(7, "a", "x");
         inbox.push(8, "b", "y");
         let all = inbox.unread();
         let one = inbox.get(8);
         let reply = inbox.reply_to(7);
         check!("unread(), get(8) and reply_to(7) held together", (all.len(), one.map(|m| m.from.as_str()), reply), (2, Some("b"), None));
     }

     #[test]
     fn random_vs_model() {
         let mut rng = anneal_prelude::Rng::new(6202);
         for _ in 0..300 {
             let mut inbox = Inbox::new();
             // (id, read, reply)
             let mut model: Vec<(u32, bool, Option<String>)> = Vec::new();
             let mut ops = Vec::new();
             for step in 0..12u32 {
                 match rng.below(4) {
                     0 => {
                         inbox.push(step, "f", "b");
                         model.push((step, false, None));
                         ops.push(format!("push {step}"));
                     }
                     1 => {
                         let id = rng.below(step as usize + 1) as u32;
                         let want = match model.iter_mut().find(|m| m.0 == id) {
                             Some(m) if !m.1 => {
                                 m.1 = true;
                                 true
                             }
                             _ => false,
                         };
                         ops.push(format!("mark_read {id}"));
                         check!(ops.join(", "), inbox.mark_read(id), want);
                     }
                     2 => {
                         let id = rng.below(step as usize + 1) as u32;
                         let s = rng.string(1, "xy");
                         if let Some(r) = inbox.reply_mut(id) {
                             r.push_str(&s);
                         }
                         if let Some(m) = model.iter_mut().find(|m| m.0 == id) {
                             m.2.get_or_insert_with(String::new).push_str(&s);
                         }
                         ops.push(format!("reply_mut {id} += {s}"));
                     }
                     _ => {
                         let got = inbox.newest_unread_mut().map(|m| m.id);
                         let want = model.iter().rev().find(|m| !m.1).map(|m| m.0);
                         ops.push("newest_unread_mut".to_string());
                         check!(ops.join(", "), got, want);
                     }
                 }
             }
             let unread: Vec<u32> = inbox.unread().iter().map(|m| m.id).collect();
             let want_unread: Vec<u32> = model.iter().filter(|m| !m.1).map(|m| m.0).collect();
             check!(format!("{}; unread ids", ops.join(", ")), unread, want_unread);
             for m in &model {
                 check!(format!("{}; reply_to({})", ops.join(", "), m.0), inbox.reply_to(m.0), m.2.as_deref());
             }
         }
     }
     """],
    [("rust", "Readers take `&self` and hand out borrows tied to it: `Option<&Msg>`, `Vec<&Msg>`, `Option<&str>`. Writers take `&mut self` and hand out `Option<&mut Msg>` or `Option<&mut String>`."),
     ("rust", "`Option<String>` to `Option<&str>` is `as_deref()`. `get_or_insert_with(String::new)` fills a `None` in place and returns `&mut String`. `iter_mut().rev().find(..)` searches from the newest.")],
    ("""A method that only reads takes `&self`, so any number of callers can hold its results at once; a method that hands out `&mut` must take `&mut self`, and then nothing else can use the inbox until that borrow ends. `triage` shows the practical consequence: it collects the boss's ids (plain `u32`s) before marking, because `mark_read` needs `&mut self` while `unread()`'s `Vec<&Msg>` would still borrow the inbox.

Syntax to remember: `self.msgs.iter().find(|m| m.id == id)` · `self.msgs.iter_mut().rev().find(|m| !m.read)` · `self.get(id)?.reply.as_deref()` · `Some(self.get_mut(id)?.reply.get_or_insert_with(String::new))`.""", "O(n) per lookup", "O(1) extra; `unread` O(k)"),
    "`unread()` returns `Vec<&Msg>`. What would returning `impl Iterator<Item = &Msg> + '_` change for `triage`?",
    ["`&self` methods can run while other shared borrows are alive; `&mut self` methods can't.", "`Option<String>` → `Option<&str>` with `as_deref`.", "`get_or_insert_with` fills an `Option` in place and returns `&mut T`."],
    related=("L2", "S1"),
    wrong=dict(
        mark_read_always_true=sub(INBOX_SOLUTION, "Some(m) if !m.read => {", "Some(m) => {"),
        oldest_unread=sub(INBOX_SOLUTION, "self.msgs.iter_mut().rev().find(|m| !m.read)", "self.msgs.iter_mut().find(|m| !m.read)"),
        reply_mut_resets=sub(INBOX_SOLUTION, "Some(self.get_mut(id)?.reply.get_or_insert_with(String::new))", "let m = self.get_mut(id)?;\n        m.reply = Some(String::new());\n        m.reply.as_mut()"),
    ),
))

WAREHOUSE_HEAD = r"""
use std::collections::HashMap;

#[derive(Debug, Default, PartialEq)]
pub struct Stock {
    pub qty: u32,
    pub reserved: u32,
}

pub struct Warehouse {
    items: HashMap<String, Stock>,
    last: Option<String>,
    log: Vec<String>,
}
"""

WAREHOUSE_STARTER = WAREHOUSE_HEAD + r"""
impl Warehouse {
    pub fn new() -> Self {
        Warehouse { items: HashMap::new(), last: None, log: Vec::new() }
    }

    /// Remembers `name` as the last item touched.
    fn touch(&self, name: &str) {
        self.last = Some(name.to_string());
    }

    /// Adds `qty` units of `name`, creating the item if needed.
    pub fn receive(&mut self, name: &str, qty: u32) {
        self.items.entry(name.to_string()).or_default().qty += qty;
        self.touch(name);
    }

    /// Reserves up to `qty` free units of `name` and returns how many it reserved. An unknown item reserves
    /// nothing and isn't touched.
    pub fn reserve(&mut self, name: &str, qty: u32) -> u32 {
        let Some(s) = self.items.get(name) else { return 0 };
        self.touch(name);
        let n = qty.min(s.qty - s.reserved);
        s.reserved += n;
        n
    }

    /// Units of `name` that aren't reserved.
    pub fn available(&self, name: &str) -> u32 {
        self.items.get(name).map_or(0, |s| s.qty - s.reserved)
    }

    /// The last item touched.
    pub fn last(&self) -> Option<&str> {
        self.last.as_deref()
    }

    /// Appends `tag` to the remembered last-touched name. The item itself keeps its name.
    pub fn tag_last(&mut self, tag: &str) {
        if let Some(name) = &self.last {
            name.push_str(tag);
        }
    }

    /// Ships every reservation: reserved units leave stock. Logs "<name>: <units>" for each item shipped,
    /// sorted by name, and returns how many items shipped.
    pub fn ship_all(&mut self) -> usize {
        let mut lines = Vec::new();
        for (name, s) in self.items.iter() {
            if s.reserved > 0 {
                lines.push(format!("{name}: {}", s.reserved));
                s.qty -= s.reserved;
                s.reserved = 0;
            }
        }
        lines.sort();
        let shipped = lines.len();
        self.log.extend(lines);
        shipped
    }

    /// How many items have fewer than `limit` units available.
    pub fn count_low(&self, limit: u32) -> usize {
        let mut n = 0;
        let bump = || n += 1;
        for s in self.items.values() {
            if s.qty - s.reserved < limit {
                bump();
            }
        }
        n
    }

    pub fn log(&self) -> &[String] {
        &self.log
    }
}
"""

WAREHOUSE_SOLUTION = WAREHOUSE_STARTER
for _old, _new in [
    ("fn touch(&self, name: &str)", "fn touch(&mut self, name: &str)"),
    ("        let Some(s) = self.items.get(name) else { return 0 };\n        self.touch(name);\n        let n = qty.min(s.qty - s.reserved);\n        s.reserved += n;\n        n",
     "        let Some(s) = self.items.get_mut(name) else { return 0 };\n        let n = qty.min(s.qty - s.reserved);\n        s.reserved += n;\n        self.touch(name);\n        n"),
    ("if let Some(name) = &self.last {", "if let Some(name) = &mut self.last {"),
    ("for (name, s) in self.items.iter() {", "for (name, s) in self.items.iter_mut() {"),
    ("let bump = || n += 1;", "let mut bump = || n += 1;"),
]:
    WAREHOUSE_SOLUTION = sub(WAREHOUSE_SOLUTION, _old, _new)
WH_SETUP = 'let mut w = Warehouse::new();\nw.receive("bolt", 10);\nw.receive("nut", 4);'
WH_DESC = "receive bolt 10, nut 4"

P.append(fix(
    "fix-mut-from-shared-self", "Fix: &mut in every position", "easy", "shared-vs-unique", ["E0596", "E0594", "E0499", "iter_mut", "get_mut", "FnMut"],
    """
        `Warehouse` doesn't compile: in several places it has shared access where it needs unique access. Each
        one is written differently: a receiver, a map lookup, an iterator, a pattern on an `Option` field, a
        closure. Fix them. `available` and `last` only read, and callers hold their results side by side, so
        they must stay as they are.
    """,
    WAREHOUSE_STARTER,
    WAREHOUSE_SOLUTION,
    [T("receive_and_reserve", f"{WH_DESC}; reserve bolt 3", '(w.reserve("bolt", 3), w.available("bolt"), w.last())', '(3, 7, Some("bolt"))', setup=WH_SETUP),
     T("reserve_caps_at_available", f"{WH_DESC}; reserve nut 3, then nut 3", '(w.reserve("nut", 3), w.reserve("nut", 3), w.available("nut"))', "(3, 1, 0)", setup=WH_SETUP),
     T("unknown_item_not_touched", f"{WH_DESC}; reserve ghost 1", '(w.reserve("ghost", 1), w.last())', '(0, Some("nut"))', setup=WH_SETUP),
     T("readers_side_by_side", f"{WH_DESC}; available(bolt) and last() held together", "(a, l)", '(10, Some("nut"))', setup=WH_SETUP + '\nlet l = w.last();\nlet a = w.available("bolt");'),
     T("tag_last", f"{WH_DESC}; tag_last \"*\" twice", '(w.last(), w.available("nut"), w.available("nut*"))', '(Some("nut**"), 4, 0)', setup=WH_SETUP + '\nw.tag_last("*");\nw.tag_last("*");'),
     T("ship_all_logs_sorted", f"{WH_DESC}; reserve nut 1, bolt 2; ship_all", "(w.ship_all(), w.log().to_vec(), w.available(\"bolt\"), w.available(\"nut\"))",
       '(2, vec!["bolt: 2".to_string(), "nut: 1".to_string()], 8, 3)', setup=WH_SETUP + '\nw.reserve("nut", 1);\nw.reserve("bolt", 2);'),
     T("count_low", f"{WH_DESC}; reserve bolt 7; count_low(4)", 'w.count_low(4)', "1", setup=WH_SETUP + '\nw.reserve("bolt", 7);')],
    [T("empty_warehouse", "new warehouse", "(w.count_low(1), w.ship_all(), w.log().len(), w.last())", "(0, 0, 0, None)", setup="let mut w = Warehouse::new();"),
     T("tag_without_last", "new warehouse; tag_last \"x\"", "w.last()", "None", setup='let mut w = Warehouse::new();\nw.tag_last("x");'),
     T("receive_adds_up", "receive a 2, a 3", '(w.available("a"), w.last())', '(5, Some("a"))', setup='let mut w = Warehouse::new();\nw.receive("a", 2);\nw.receive("a", 3);'),
     T("reserve_zero_touches", f"{WH_DESC}; reserve bolt 0", '(w.reserve("bolt", 0), w.last())', '(0, Some("bolt"))', setup=WH_SETUP),
     T("reserve_when_all_reserved", f"{WH_DESC}; reserve nut 4, then nut 1", '(w.reserve("nut", 1), w.last(), w.available("nut"))', '(0, Some("nut"), 0)', setup=WH_SETUP + '\nw.reserve("nut", 4);'),
     T("ship_twice", f"{WH_DESC}; reserve bolt 10; ship_all twice", '(w.ship_all(), w.ship_all(), w.available("bolt"), w.log().len())', "(1, 0, 0, 1)", setup=WH_SETUP + '\nw.reserve("bolt", 10);'),
     T("log_accumulates", f"{WH_DESC}; reserve nut 1; ship; reserve bolt 1, nut 1; ship", "w.log().to_vec()", 'vec!["nut: 1", "bolt: 1", "nut: 1"]',
       setup=WH_SETUP + '\nw.reserve("nut", 1);\nw.ship_all();\nw.reserve("bolt", 1);\nw.reserve("nut", 1);\nw.ship_all();'),
     T("count_low_counts_reserved", f"{WH_DESC}; reserve bolt 9; count_low(2), count_low(5), count_low(0)", "(w.count_low(2), w.count_low(5), w.count_low(0))", "(1, 2, 0)", setup=WH_SETUP + '\nw.reserve("bolt", 9);'),
     T("tag_then_receive", f"{WH_DESC}; tag_last \"!\"; receive bolt 1", '(w.last(), w.available("bolt"))', '(Some("bolt"), 11)', setup=WH_SETUP + '\nw.tag_last("!");\nw.receive("bolt", 1);'),
     r"""
     #[test]
     fn random_vs_model() {
         use std::collections::BTreeMap;
         let mut rng = anneal_prelude::Rng::new(6203);
         let names = ["a", "b", "c"];
         for _ in 0..300 {
             let mut w = Warehouse::new();
             let mut model: BTreeMap<&str, (u32, u32)> = BTreeMap::new();
             let mut last: Option<String> = None;
             let mut log: Vec<String> = Vec::new();
             let mut ops = Vec::new();
             for _ in 0..10 {
                 let name = *rng.pick(&names);
                 let q = rng.below(5) as u32;
                 match rng.below(4) {
                     0 => {
                         w.receive(name, q);
                         model.entry(name).or_default().0 += q;
                         last = Some(name.to_string());
                         ops.push(format!("receive {name} {q}"));
                     }
                     1 => {
                         let want = match model.get_mut(name) {
                             Some(s) => {
                                 let n = q.min(s.0 - s.1);
                                 s.1 += n;
                                 last = Some(name.to_string());
                                 n
                             }
                             None => 0,
                         };
                         ops.push(format!("reserve {name} {q}"));
                         check!(ops.join(", "), w.reserve(name, q), want);
                     }
                     2 => {
                         let mut n = 0;
                         for (k, s) in model.iter_mut() {
                             if s.1 > 0 {
                                 log.push(format!("{k}: {}", s.1));
                                 s.0 -= s.1;
                                 s.1 = 0;
                                 n += 1;
                             }
                         }
                         ops.push("ship_all".to_string());
                         check!(ops.join(", "), w.ship_all(), n);
                     }
                     _ => {
                         let want = model.values().filter(|s| s.0 - s.1 < q).count();
                         ops.push(format!("count_low {q}"));
                         check!(ops.join(", "), w.count_low(q), want);
                     }
                 }
             }
             check!(format!("{}; last, log", ops.join(", ")), (w.last(), w.log().to_vec()), (last.as_deref(), log.clone()));
             for n in names {
                 let want = model.get(n).map_or(0, |s| s.0 - s.1);
                 check!(format!("{}; available({n})", ops.join(", ")), w.available(n), want);
             }
         }
     }
     """],
    [("rust", "Read each error for where the shared access comes from: a `&self` receiver, `get` instead of `get_mut`, `iter` instead of `iter_mut`, `&self.last` in a pattern, and a closure binding that isn't `mut` (calling an `FnMut` needs `&mut` to the closure)."),
     ("rust", "Once `reserve` holds `s` from `get_mut`, `self.touch(name)` borrows all of `self` while `s` is still used. Where can the call go so the borrows don't overlap?")],
    ("""`&` is shared access and `&mut` unique access, and each position has its own spelling: the receiver (`&mut self`), the lookup (`get_mut`), the iterator (`iter_mut`, which also makes the pattern `(name, s)` bind `s: &mut Stock`), a pattern on a field (`&mut self.last`, or `self.last.as_mut()`), and a closure that mutates what it captured (it's `FnMut`, so calling it borrows it mutably: `let mut bump`).

In `reserve`, `s` borrows one entry of `self.items`, but `self.touch(name)` borrows all of `self`, so the call moves after the last use of `s`. It must stay after the `let else`, too: an unknown item isn't touched. `ship_all` pushes to `lines` while iterating `self.items` mutably, which is fine, and `available` and `last` keep `&self`.""", "O(1) per call; O(n log n) ship_all", "O(n) ship_all"),
    "Why does calling an `FnMut` closure need a `mut` binding, when calling an `Fn` closure doesn't?",
    ["Every position that grants access has a shared and a unique spelling.", "A method call borrows all of `self`; a field borrow only the field.", "Calling an `FnMut` borrows the closure mutably."],
    rules=dict(methods=["clone", "take", "replace"], lines=8),
    wrong=dict(
        touch_before_lookup=sub(WAREHOUSE_SOLUTION, "        let Some(s) = self.items.get_mut(name) else { return 0 };\n        let n = qty.min(s.qty - s.reserved);\n        s.reserved += n;\n        self.touch(name);\n",
                                "        self.touch(name);\n        let Some(s) = self.items.get_mut(name) else { return 0 };\n        let n = qty.min(s.qty - s.reserved);\n        s.reserved += n;\n"),
        count_low_at_limit=sub(WAREHOUSE_SOLUTION, "if s.qty - s.reserved < limit {", "if s.qty - s.reserved <= limit {"),
        ships_without_clearing=sub(WAREHOUSE_SOLUTION, "                s.qty -= s.reserved;\n                s.reserved = 0;\n", "                s.qty -= s.reserved;\n"),
    ),
))

WORDS_SOLUTION = r"""
use std::collections::HashMap;
use std::hash::{Hash, Hasher};

/// A word that compares and hashes ignoring ASCII case.
struct Word<'a>(&'a str);

impl PartialEq for Word<'_> {
    fn eq(&self, other: &Self) -> bool {
        self.0.eq_ignore_ascii_case(other.0)
    }
}

impl Eq for Word<'_> {}

impl Hash for Word<'_> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        for b in self.0.bytes() {
            state.write_u8(b.to_ascii_lowercase());
        }
        state.write_u8(0xff);
    }
}

pub fn top_words(text: &str, k: usize) -> Vec<(&str, usize)> {
    // word -> (count, index of its first appearance)
    let mut counts: HashMap<Word, (usize, usize)> = HashMap::new();
    for (i, w) in text.split(|c: char| !c.is_alphanumeric()).filter(|w| !w.is_empty()).enumerate() {
        counts.entry(Word(w)).or_insert((0, i)).0 += 1;
    }
    let mut top: Vec<(&str, usize, usize)> = counts.into_iter().map(|(w, (n, first))| (w.0, n, first)).collect();
    top.sort_unstable_by(|a, b| b.1.cmp(&a.1).then(a.2.cmp(&b.2)));
    top.into_iter().take(k).map(|(w, n, _)| (w, n)).collect()
}
"""

WORDS_DOC = r"""
/// The `k` most frequent words of `text` with their counts, most frequent first; a tie goes to the word that
/// appeared first. A word is a maximal run of alphanumeric chars (`char::is_alphanumeric`). Words that differ
/// only in ASCII case are the same word, reported with the spelling it first appeared with, as a slice of `text`.
"""


def words_case(name, text, k, want):
    rust_text = text.replace('"', '\\"')
    exp = "vec![" + ", ".join(f'("{w}", {n})' for w, n in want) + "]"
    if not want:
        exp = "Vec::<(&str, usize)>::new()"
    return T(name, f'text = "{rust_text}", k = {k}', f'top_words("{rust_text}", {k})', exp)


P.append(write(
    "most-repeated-word", "Count words without cloning", "easy", "shared-vs-unique", ["&str", "HashMap", "Hash", "borrowed keys"],
    """
        Return the `k` most frequent words of `text` with their counts: most frequent first, a tie going to the
        word that appeared first. A word is a maximal run of alphanumeric chars. Words that differ only in ASCII
        case are the same word (`Rust`, `rust`, `RUST`), reported with the spelling it first appeared with.

        Every word you return is a slice of `text`, and you may not allocate per word: a test counts
        allocations on a long text.
    """,
    "use std::collections::HashMap;\n" + WORDS_DOC + "pub fn top_words(text: &str, k: usize) -> Vec<(&str, usize)> {\n    todo!()\n}\n",
    WORDS_SOLUTION.replace("pub fn top_words", WORDS_DOC.strip("\n") + "\npub fn top_words"),
    [words_case("example", "the cat and the hat and the bat", 2, [("the", 3), ("and", 2)]),
     words_case("case_folds_to_first_spelling", "Rust rust RUST go Go", 5, [("Rust", 3), ("go", 2)]),
     words_case("tie_goes_to_first_seen", "b a a b c", 3, [("b", 2), ("a", 2), ("c", 1)]),
     words_case("punctuation_splits", "well-known, well: known!", 2, [("well", 2), ("known", 2)]),
     words_case("k_beyond_distinct", "x y", 10, [("x", 1), ("y", 1)]),
     words_case("empty_text", "", 3, []),
     words_case("k_zero", "a a", 0, [])],
    [ALLOC_COUNTER,
     words_case("only_separators", "  ,,; - ", 2, []),
     words_case("digits_are_word_chars", "v2 V2 route66", 2, [("v2", 2), ("route66", 1)]),
     words_case("apostrophe_splits", "don't DON'T", 3, [("don", 2), ("t", 2)]),
     words_case("unicode_letters", "café Café CAFÉ", 3, [("café", 2), ("CAFÉ", 1)]),
     words_case("non_ascii_separator", "a—b—a", 2, [("a", 2), ("b", 1)]),
     words_case("newlines_and_tabs", "one\\ttwo\\nTWO\\n\\none", 1, [("one", 2)]),
     words_case("later_word_overtakes", "a b b", 1, [("b", 2)]),
     words_case("single_word", "Solo", 1, [("Solo", 1)]),
     r"""
     #[test]
     fn words_borrow_the_text() {
         let text = String::from("Beta alpha BETA");
         let got = top_words(&text, 2);
         let range = text.as_bytes().as_ptr_range();
         check!("\"Beta alpha BETA\": every word points into the text", got.iter().all(|(w, _)| range.contains(&w.as_ptr())), true);
         check!("\"Beta alpha BETA\": and is the first spelling", got[0].0.as_ptr() == text.as_ptr(), true);
     }

     #[test]
     fn random_vs_brute_force() {
         let mut rng = anneal_prelude::Rng::new(6204);
         for _ in 0..300 {
             let len = rng.below(24);
             let text = rng.string(len, "aAbB c.");
             let k = rng.below(5);
             let words: Vec<&str> = text.split(|c: char| !c.is_alphanumeric()).filter(|w| !w.is_empty()).collect();
             // (lowercase, first spelling, count, first index)
             let mut seen: Vec<(String, &str, usize, usize)> = Vec::new();
             for (i, w) in words.iter().enumerate() {
                 let low = w.to_ascii_lowercase();
                 match seen.iter_mut().find(|s| s.0 == low) {
                     Some(s) => s.2 += 1,
                     None => seen.push((low, w, 1, i)),
                 }
             }
             seen.sort_by(|a, b| b.2.cmp(&a.2).then(a.3.cmp(&b.3)));
             let want: Vec<(&str, usize)> = seen.iter().take(k).map(|s| (s.1, s.2)).collect();
             check!(format!("text = {text:?}, k = {k}"), top_words(&text, k), want);
         }
     }

     #[test]
     fn long_text_allocates_per_distinct_word() {
         let spellings = ["Rust", "rust", "RUST", "go", "Go", "zig"];
         let mut text = String::new();
         for i in 0..200_000 {
             text.push_str(spellings[i % 6]);
             text.push(' ');
         }
         let (got, n) = allocs(|| top_words(&text, 3));
         check!("200000 words, 6 spellings of 3 words", got, vec![("Rust", 100_001), ("go", 66_666), ("zig", 33_333)]);
         check!("200000 words: at most 64 allocations (a String per word would be 200000)", n <= 64, true);
     }
     """],
    [("rust", "Key the map by `&str` slices of `text` so nothing is copied. Returning keys of a map that owns `String`s wouldn't compile: they'd borrow the map, which dies at the end of the function."),
     ("rust", "Case-insensitive keys without allocating: wrap the slice in your own type and implement `PartialEq`, `Eq` and `Hash` by hand so they ignore ASCII case. `entry` keeps the key it was first inserted with.")],
    ("""The words borrow `text`, so the result's elided lifetime ties it to `text`, not to the map: that's why the map can be dropped while the slices are returned. Folding case with `to_lowercase()` per word would work but allocate a `String` per word; a newtype key whose `Hash` and `Eq` ignore ASCII case keeps every key a borrowed slice, and `entry` keeps the first spelling because an existing key is never replaced. `Hash` must agree with `Eq`: equal words must hash the same, so hash the lowercased bytes.

Syntax to remember: `struct Word<'a>(&'a str);` · `impl Hash for Word<'_> { fn hash<H: Hasher>(&self, state: &mut H) { .. } }` · `impl Eq for Word<'_> {}` · `self.0.eq_ignore_ascii_case(other.0)` · `sort_unstable_by(|a, b| b.1.cmp(&a.1).then(a.2.cmp(&b.2)))`.""", "O(n + d log d) for d distinct words", "O(d)"),
    "Unicode case folding (`É` and `é`) can change a word's length. Why can't a borrowed key fold it the way this one folds ASCII?",
    ["Borrowed keys: `HashMap<&str, _>` or a newtype over `&str`.", "A hand-written `Hash` must agree with `Eq`.", "Returning slices of the input, not of a local map."],
    related=("L2", "S4"),
    wrong=dict(
        lowercase_string_keys=r"""
            use std::collections::HashMap;

            pub fn top_words(text: &str, k: usize) -> Vec<(&str, usize)> {
                let mut counts: HashMap<String, (usize, usize, &str)> = HashMap::new();
                for (i, w) in text.split(|c: char| !c.is_alphanumeric()).filter(|w| !w.is_empty()).enumerate() {
                    counts.entry(w.to_ascii_lowercase()).or_insert((0, i, w)).0 += 1;
                }
                let mut top: Vec<(usize, usize, &str)> = counts.into_values().collect();
                top.sort_unstable_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
                top.into_iter().take(k).map(|(n, _, w)| (w, n)).collect()
            }
        """,
        ties_alphabetical=sub(WORDS_SOLUTION, "then(a.2.cmp(&b.2))", "then(a.0.cmp(b.0))"),
        case_sensitive=sub(WORDS_SOLUTION, "self.0.eq_ignore_ascii_case(other.0)", "self.0 == other.0").replace("state.write_u8(b.to_ascii_lowercase());", "state.write_u8(b);"),
        whitespace_words=sub(WORDS_SOLUTION, "text.split(|c: char| !c.is_alphanumeric()).filter(|w| !w.is_empty())", "text.split_whitespace()"),
    ),
))

ACCOUNTS_HEAD = r"""
#[derive(Debug, PartialEq)]
pub struct Account {
    pub id: u32,
    pub balance: i64,
}
"""

ACCOUNTS_STARTER = ACCOUNTS_HEAD + r"""
/// Pays `pct` percent interest (rounded toward zero) into every selected account.
pub fn pay_interest(selected: &[&mut Account], pct: i64) {
    for a in selected {
        a.balance += a.balance * pct / 100;
    }
}

/// Points `richest` at whichever of `richest` and `candidate` has the larger balance; a tie keeps `richest`.
pub fn keep_richest<'a>(richest: &mut &'a Account, candidate: &'a Account) {
    if candidate.balance > richest.balance {
        *richest = candidate;
    }
}

/// Moves `amount` from `from` into `to`.
pub fn top_up(to: &mut &Account, from: &mut Account, amount: i64) {
    from.balance -= amount;
    to.balance += amount;
}

/// Pays interest to every account with a balance of at least `min`, and returns their ids in order.
pub fn reward(accounts: &mut [Account], min: i64, pct: i64) -> Vec<u32> {
    let selected: Vec<&mut Account> = accounts.iter_mut().filter(|a| a.balance >= min).collect();
    pay_interest(&selected, pct);
    selected.iter().map(|a| a.id).collect()
}

/// The id of the richest account (the first one on a tie), or None if there are none.
pub fn richest_id(accounts: &[Account]) -> Option<u32> {
    let (first, rest) = accounts.split_first()?;
    let mut best = first;
    for a in rest {
        keep_richest(&mut best, a);
    }
    Some(best.id)
}
"""

ACCOUNTS_SOLUTION = ACCOUNTS_STARTER
for _old, _new in [
    ("pub fn pay_interest(selected: &[&mut Account], pct: i64) {\n    for a in selected {", "pub fn pay_interest(selected: &mut [&mut Account], pct: i64) {\n    for a in selected.iter_mut() {"),
    ("pub fn top_up(to: &mut &Account,", "pub fn top_up(to: &mut Account,"),
    ("    let selected: Vec<&mut Account>", "    let mut selected: Vec<&mut Account>"),
    ("pay_interest(&selected, pct);", "pay_interest(&mut selected, pct);"),
]:
    ACCOUNTS_SOLUTION = sub(ACCOUNTS_SOLUTION, _old, _new)


def acc(xs):
    return "vec![" + ", ".join(f"Account {{ id: {i}, balance: {b} }}" for i, b in xs) + "]"


P.append(fix(
    "fix-mutate-through-shared-ref", "Fix: a &mut behind a & is read-only", "easy", "shared-vs-unique", ["E0594", "E0596", "&&mut T", "&mut &T"],
    """
        This doesn't compile. Two functions try to write through a reference that only grants reading, even
        though a `&mut` appears in their types. Fix the signatures and their callers. `keep_richest` and
        `richest_id` are correct: leave them alone.
    """,
    ACCOUNTS_STARTER,
    ACCOUNTS_SOLUTION,
    [T("reward_pays_selected", "balances [100, 50, 300], min 100, 10%", "{ let mut v = " + acc([(1, 100), (2, 50), (3, 300)]) + "; let ids = reward(&mut v, 100, 10); (ids, v.iter().map(|a| a.balance).collect::<Vec<_>>()) }", "(vec![1, 3], vec![110, 50, 330])"),
     T("interest_rounds_toward_zero", "balances [15, -15], min -100, 10%", "{ let mut v = " + acc([(1, 15), (2, -15)]) + "; reward(&mut v, -100, 10); (v[0].balance, v[1].balance) }", "(16, -16)"),
     T("top_up_moves_funds", "to 5, from 20, amount 7", "{ let (mut to, mut from) = (Account { id: 1, balance: 5 }, Account { id: 2, balance: 20 }); top_up(&mut to, &mut from, 7); (to.balance, from.balance) }", "(12, 13)"),
     T("richest_first_on_tie", "balances [3, 9, 9]", "richest_id(&" + acc([(1, 3), (2, 9), (3, 9)]) + ")", "Some(2)"),
     T("keep_richest_repoints", "richest 4, candidate 8", "{ let (a, b) = (Account { id: 1, balance: 4 }, Account { id: 2, balance: 8 }); let mut r = &a; keep_richest(&mut r, &b); r.id }", "2"),
     T("pay_interest_direct", "two selected accounts 200, 1000; 5%", "{ let (mut a, mut b) = (Account { id: 1, balance: 200 }, Account { id: 2, balance: 1000 }); pay_interest(&mut [&mut a, &mut b], 5); (a.balance, b.balance) }", "(210, 1050)")],
    [T("reward_none_selected", "balances [1, 2], min 10", "{ let mut v = " + acc([(1, 1), (2, 2)]) + "; (reward(&mut v, 10, 50), v[0].balance, v[1].balance) }", "(vec![], 1, 2)"),
     T("reward_empty", "no accounts", "reward(&mut [], 0, 10)", "vec![]"),
     T("reward_at_min", "balance 100, min 100, 1%", "{ let mut v = " + acc([(7, 100)]) + "; (reward(&mut v, 100, 1), v[0].balance) }", "(vec![7], 101)"),
     T("zero_percent", "balance 99, 0%", "{ let mut v = " + acc([(1, 99)]) + "; reward(&mut v, 0, 0); v[0].balance }", "99"),
     T("small_balance_no_interest", "balance 9, 10%", "{ let mut v = " + acc([(1, 9)]) + "; reward(&mut v, 0, 10); v[0].balance }", "9"),
     T("top_up_negative", "to 5, from 5, amount -3", "{ let (mut to, mut from) = (Account { id: 1, balance: 5 }, Account { id: 2, balance: 5 }); top_up(&mut to, &mut from, -3); (to.balance, from.balance) }", "(2, 8)"),
     T("richest_empty", "no accounts", "richest_id(&[])", "None"),
     T("richest_negative", "balances [-5, -2, -9]", "richest_id(&" + acc([(1, -5), (2, -2), (3, -9)]) + ")", "Some(2)"),
     T("keep_richest_tie", "richest 4, candidate 4", "{ let (a, b) = (Account { id: 1, balance: 4 }, Account { id: 2, balance: 4 }); let mut r = &a; keep_richest(&mut r, &b); r.id }", "1"),
     T("large_balances", "balance 10^15, 3%", "{ let mut v = " + acc([(1, 10**15)]) + "; reward(&mut v, 0, 3); v[0].balance }", str(10**15 + 3 * 10**13)),
     r"""
     #[test]
     fn random_vs_model() {
         let mut rng = anneal_prelude::Rng::new(6205);
         for _ in 0..300 {
             let n = rng.below(7);
             let balances: Vec<i64> = rng.vec(n, -500, 500);
             let min = rng.int(-500, 500);
             let pct = rng.int(0, 30);
             let mut v: Vec<Account> = balances.iter().enumerate().map(|(i, &b)| Account { id: i as u32, balance: b }).collect();
             let want_ids: Vec<u32> = v.iter().filter(|a| a.balance >= min).map(|a| a.id).collect();
             let want: Vec<i64> = balances.iter().map(|&b| if b >= min { b + b * pct / 100 } else { b }).collect();
             let ids = reward(&mut v, min, pct);
             let got: Vec<i64> = v.iter().map(|a| a.balance).collect();
             check!(format!("balances {balances:?}, min {min}, {pct}%"), (ids, got), (want_ids, want));
         }
     }
     """],
    [("rust", "`selected` is `&[&mut Account]`: a shared borrow of the slice. Everything reached through a `&` is read-only, including the `&mut` inside it; otherwise two copies of the `&` would hand out the same `&mut`."),
     ("rust", "`&mut &Account` lets you change which account the inner reference points at (that's what `keep_richest` does), but the account itself is still behind a `&`.")],
    ("""Mutability doesn't pass through a shared reference: `&&mut T` only reads the `T`, because `&` is `Copy` and two copies must not both write. So `pay_interest` needs `&mut [&mut Account]`, iterated with `iter_mut()` (which yields `&mut &mut Account`; field access derefs through both), and the caller passes `&mut selected` from a `mut` binding. The other direction, `&mut &T`, grants unique access to the *reference*: you may repoint it, as `keep_richest` does, but not write the `T`. `top_up` needs a plain `&mut Account`.

Syntax to remember: `fn f(xs: &mut [&mut T])` · `for x in xs.iter_mut() { x.field += 1 }` · `fn repoint<'a>(r: &mut &'a T, other: &'a T) { *r = other }`.""", "O(n)", "O(k) for the selection"),
    "`&mut &mut T` lets you write the `T`, but `&&mut T` doesn't. Why is the second rule needed for soundness?",
    ["`&` makes everything behind it read-only, including a `&mut`.", "`&mut &T` repoints a reference; it doesn't unlock the `T`."],
    rules=dict(methods=["clone"], lines=6),
    wrong=dict(
        interest_on_cents_after_division=sub(ACCOUNTS_SOLUTION, "a.balance += a.balance * pct / 100;", "a.balance += a.balance / 100 * pct;"),
        rewards_everyone=sub(ACCOUNTS_SOLUTION, "accounts.iter_mut().filter(|a| a.balance >= min).collect();", "accounts.iter_mut().collect();"),
        top_up_backwards=sub(ACCOUNTS_SOLUTION, "    from.balance -= amount;\n    to.balance += amount;", "    from.balance += amount;\n    to.balance -= amount;"),
    ),
))

SPLIT_STARTER = r"""
/// `names[0]` is a namespace. Prefixes every other name with "<namespace>::" in place, unless it already starts
/// with exactly that. An empty slice is left alone.
pub fn qualify(names: &mut [String]) {
    for i in 1..names.len() {
        let ns = &names[0];
        let done = names[i].strip_prefix(ns.as_str()).is_some_and(|rest| rest.starts_with("::"));
        if !done {
            names[i].insert_str(0, "::");
            names[i].insert_str(0, ns);
        }
    }
}

/// Seals every whole frame of `size` bytes (`size >= 2`) in `buf` and returns how many it sealed; bytes after
/// the last whole frame are left alone. In a frame, byte 0 is the key and the last byte the checksum: XOR every
/// byte in between with the key, then set the checksum to the wrapping sum of those new bytes.
pub fn seal_frames(buf: &mut [u8], size: usize) -> usize {
    let mut sealed = 0;
    for frame in buf.chunks_mut(size) {
        let key = &frame[0];
        let sum = &mut frame[size - 1];
        *sum = 0;
        for b in &mut frame[1..size - 1] {
            *b ^= *key;
            *sum = sum.wrapping_add(*b);
        }
        sealed += 1;
    }
    sealed
}

/// Swaps the first half of `v` with the last half in place. With an odd length the middle element stays:
/// [1, 2, 3, 4, 5] becomes [4, 5, 3, 1, 2].
pub fn swap_halves<T>(v: &mut [T]) {
    let half = v.len() / 2;
    let (front, back) = (&mut v[..half], &mut v[v.len() - half..]);
    front.swap_with_slice(back);
}
"""

SPLIT_SOLUTION = r"""
/// `names[0]` is a namespace. Prefixes every other name with "<namespace>::" in place, unless it already starts
/// with exactly that. An empty slice is left alone.
pub fn qualify(names: &mut [String]) {
    let Some((ns, rest)) = names.split_first_mut() else { return };
    for name in rest {
        let done = name.strip_prefix(ns.as_str()).is_some_and(|rest| rest.starts_with("::"));
        if !done {
            name.insert_str(0, "::");
            name.insert_str(0, ns);
        }
    }
}

/// Seals every whole frame of `size` bytes (`size >= 2`) in `buf` and returns how many it sealed; bytes after
/// the last whole frame are left alone. In a frame, byte 0 is the key and the last byte the checksum: XOR every
/// byte in between with the key, then set the checksum to the wrapping sum of those new bytes.
pub fn seal_frames(buf: &mut [u8], size: usize) -> usize {
    let mut sealed = 0;
    for frame in buf.chunks_exact_mut(size) {
        let (key, rest) = frame.split_first_mut().unwrap();
        let (sum, body) = rest.split_last_mut().unwrap();
        *sum = 0;
        for b in body {
            *b ^= *key;
            *sum = sum.wrapping_add(*b);
        }
        sealed += 1;
    }
    sealed
}

/// Swaps the first half of `v` with the last half in place. With an odd length the middle element stays:
/// [1, 2, 3, 4, 5] becomes [4, 5, 3, 1, 2].
pub fn swap_halves<T>(v: &mut [T]) {
    let half = v.len() / 2;
    let (front, rest) = v.split_at_mut(half);
    let back_start = rest.len() - half;
    front.swap_with_slice(&mut rest[back_start..]);
}
"""


def names_case(name, before, after):
    b = "[" + ", ".join(f'"{x}"' for x in before) + "]"
    a = "vec![" + ", ".join(f'"{x}"' for x in after) + "]" if after else "Vec::<String>::new()"
    return T(name, f"qualify({b})", f"{{ let src: [&str; {len(before)}] = {b}; let mut v: Vec<String> = src.iter().map(|s| s.to_string()).collect(); qualify(&mut v); v }}", a)


def frames_case(name, buf, size, sealed, after):
    return T(name, f"seal_frames({buf}, size {size})", f"{{ let mut b: Vec<u8> = vec!{buf}; let n = seal_frames(&mut b, {size}); (n, b) }}", f"({sealed}, vec!{after})")


def seal_py(buf, size):
    buf = list(buf)
    n = 0
    for s in range(0, len(buf) - size + 1, size):
        k = buf[s]
        tot = 0
        for j in range(s + 1, s + size - 1):
            buf[j] ^= k
            tot = (tot + buf[j]) % 256
        buf[s + size - 1] = tot
        n += 1
    return n, buf


def frames_auto(name, buf, size):
    n, after = seal_py(buf, size)
    return frames_case(name, buf, size, n, after)


P.append(fix(
    "split-first-mut", "Fix: one &mut slice, several parts", "easy", "shared-vs-unique", ["E0502", "E0499", "split_first_mut", "split_last_mut", "split_at_mut", "chunks_exact_mut"],
    """
        None of these three functions compiles: each one needs a mutable borrow of one part of a slice while
        another part is borrowed too. Fix them without copying any `String`, building a new buffer, or moving values
        around with `mem::take`, `mem::replace` or `swap`.
        `seal_frames` also has one bug the compiler can't see; the doc comments are the spec.
    """,
    SPLIT_STARTER,
    SPLIT_SOLUTION,
    [names_case("qualify_example", ["net", "tcp", "net::udp", "network"], ["net", "net::tcp", "net::udp", "net::network"]),
     names_case("qualify_empty_slice", [], []),
     frames_auto("seal_two_frames", [3, 1, 2, 0, 5, 5, 5, 0], 4),
     frames_auto("partial_frame_untouched", [1, 1, 1, 9, 9], 3),
     T("swap_halves_odd", "swap_halves([1, 2, 3, 4, 5])", "{ let mut v = [1, 2, 3, 4, 5]; swap_halves(&mut v); v }", "[4, 5, 3, 1, 2]"),
     T("swap_halves_strings", "swap_halves([\"a\", \"b\", \"c\", \"d\"])", '{ let mut v = ["a", "b", "c", "d"].map(String::from); swap_halves(&mut v); v }', '["c", "d", "a", "b"].map(String::from)')],
    [names_case("qualify_only_namespace", ["ns"], ["ns"]),
     names_case("qualify_prefix_without_colons", ["a", "ab", "a:b", "a::"], ["a", "a::ab", "a::a:b", "a::"]),
     names_case("qualify_empty_namespace", ["", "x", "::y"], ["", "::x", "::y"]),
     names_case("qualify_empty_name", ["n", ""], ["n", "n::"]),
     names_case("qualify_unicode", ["日本", "東京", "日本::大阪"], ["日本", "日本::東京", "日本::大阪"]),
     frames_auto("seal_size_two", [7, 7, 1, 2], 2),
     frames_auto("seal_wraps", [0, 200, 100, 0], 4),
     frames_auto("seal_buffer_shorter_than_frame", [1, 2], 3),
     frames_auto("seal_empty", [], 3),
     T("swap_halves_short", "swap_halves on [], [1], [1, 2]", "{ let (mut a, mut b, mut c): ([i32; 0], [i32; 1], [i32; 2]) = ([], [1], [1, 2]); swap_halves(&mut a); swap_halves(&mut b); swap_halves(&mut c); (b, c) }", "([1], [2, 1])"),
     r"""
     #[test]
     fn qualify_extends_in_place() {
         let mut v = vec!["ns".to_string(), String::with_capacity(64)];
         v[1].push_str("x");
         let before = v[1].as_ptr();
         qualify(&mut v);
         check!("qualify([\"ns\", \"x\" with spare capacity]): the String grows in place", (v[1].as_str(), v[1].as_ptr() == before), ("ns::x", true));
     }

     #[test]
     fn random_vs_brute_force() {
         let mut rng = anneal_prelude::Rng::new(6206);
         for _ in 0..300 {
             let size = 2 + rng.below(4);
             let len = rng.below(14);
             let buf: Vec<u8> = rng.vec(len, 0, 255);
             let mut want = buf.clone();
             let mut n = 0;
             let total = want.len();
             for s in (0..).step_by(size).take_while(|s| s + size <= total) {
                 let k = want[s];
                 let mut tot = 0u8;
                 for j in s + 1..s + size - 1 {
                     want[j] ^= k;
                     tot = tot.wrapping_add(want[j]);
                 }
                 want[s + size - 1] = tot;
                 n += 1;
             }
             let mut got = buf.clone();
             let sealed = seal_frames(&mut got, size);
             check!(format!("seal_frames({buf:?}, size {size})"), (sealed, got), (n, want));

             let v: Vec<u32> = rng.vec(len, 0, 9);
             let h = len / 2;
             let mut want = v[len - h..].to_vec();
             want.extend_from_slice(&v[h..len - h]);
             want.extend_from_slice(&v[..h]);
             let mut got = v.clone();
             swap_halves(&mut got);
             check!(format!("swap_halves({v:?})"), got, want);
         }
     }

     #[test]
     fn big_buffer() {
         let mut buf = vec![1u8; 300_001];
         let n = seal_frames(&mut buf, 3);
         check!("300001 bytes of 1, size 3", (n, buf[0], buf[1], buf[2], buf[300_000]), (100_000, 1, 0, 0, 1));
         let mut v: Vec<u32> = (0..200_001).collect();
         swap_halves(&mut v);
         check!("swap_halves(0..200001)", (v[0], v[99_999], v[100_000], v[100_001], v[200_000]), (100_001, 200_000, 100_000, 0, 99_999));
     }
     """],
    [("rust", "`let ns = &names[0]` borrows the whole slice, so `names[i].insert_str(..)` can't borrow it mutably at the same time, even though `i != 0`. The compiler doesn't reason about index values."),
     ("rust", "`split_first_mut` gives `(&mut T, &mut [T])`, `split_last_mut` the last element and the rest, and `split_at_mut(mid)` two halves: borrows that can't overlap, so they can live together."),
     ("rust", "`chunks_mut` also yields a short last chunk; `chunks_exact_mut` yields only whole ones.")],
    ("""Indexing borrows the whole slice, so two index expressions can't hold a shared and a mutable borrow at once even when the indices differ: the borrow checker never compares index values. The `split_*_mut` methods prove the parts are disjoint (they check the lengths, then hand out non-overlapping borrows), so the parts can be used together. Copying the key byte out (`let key = frame[0];`) would also work for `u8`; for the `String` namespace, splitting is how you read one element while growing another without a copy.

Syntax to remember: `let Some((head, rest)) = v.split_first_mut() else { return };` · `let (last, init) = v.split_last_mut().unwrap();` · `let (a, b) = v.split_at_mut(mid);` · `for chunk in v.chunks_exact_mut(n)` · `a.swap_with_slice(b)` (equal lengths).""", "O(n)", "O(1)"),
    "Why is there `chunks_mut` but no `windows_mut`?",
    ["Indexing borrows the whole slice; the compiler doesn't compare indices.", "`split_first_mut`, `split_last_mut`, `split_at_mut` give disjoint `&mut` parts.", "`chunks_exact_mut` skips the partial tail."],
    rules=dict(methods=["clone", "to_string", "to_owned", "to_vec", "swap", "take", "replace"]),
    related=("L2", "S3"),
    wrong=dict(
        seals_partial_frame=sub(SPLIT_SOLUTION, "buf.chunks_exact_mut(size)", "buf.chunks_mut(size).filter(|f| f.len() >= 2)"),
        rotate_instead_of_swap=sub(SPLIT_SOLUTION, "    let (front, rest) = v.split_at_mut(half);\n    let back_start = rest.len() - half;\n    front.swap_with_slice(&mut rest[back_start..]);", "    v.rotate_left(half);"),
        qualify_skips_prefix_only=sub(SPLIT_SOLUTION, "let done = name.strip_prefix(ns.as_str()).is_some_and(|rest| rest.starts_with(\"::\"));", "let done = name.starts_with(ns.as_str());"),
    ),
))

# ---------------------------------------------------------------- where borrows end (easy)

BATCH_HEAD = r"""
/// Collects lines and appends them to `out` when it's dropped.
pub struct Batch<'a> {
    out: &'a mut Vec<String>,
    buf: Vec<String>,
}

impl<'a> Batch<'a> {
    pub fn new(out: &'a mut Vec<String>) -> Self {
        Batch { out, buf: Vec::new() }
    }

    pub fn line(&mut self, s: String) {
        self.buf.push(s);
    }
}

impl Drop for Batch<'_> {
    fn drop(&mut self) {
        self.out.append(&mut self.buf);
    }
}

/// Renders `items` into `out`:
/// - if `out` already has lines, its first line is a title: append " (cont.)" to it; otherwise push "untitled";
/// - push each item as "- <item>";
/// - through a `Batch`, add "<n> items, longest: <item>", naming the first of the longest items (by bytes), or
///   "-" when there are none.
/// Returns how many lines `out` has afterwards.
"""

RENDER_STARTER = BATCH_HEAD + r"""pub fn render(out: &mut Vec<String>, items: &[&str]) -> usize {
    let title = out.first_mut();
    if out.is_empty() {
        out.push("untitled".to_string());
    }
    if let Some(t) = title {
        t.push_str(" (cont.)");
    }
    let mut longest: Option<&str> = None;
    for item in items {
        out.push(format!("- {item}"));
        let line = &out[out.len() - 1][2..];
        if longest.map_or(true, |l| line.len() > l.len()) {
            longest = Some(line);
        }
    }
    let mut batch = Batch::new(out);
    batch.line(format!("{} items, longest: {}", items.len(), longest.unwrap_or("-")));
    out.len()
}
"""

RENDER_SOLUTION = BATCH_HEAD + r"""pub fn render(out: &mut Vec<String>, items: &[&str]) -> usize {
    match out.first_mut() {
        Some(title) => title.push_str(" (cont.)"),
        None => out.push("untitled".to_string()),
    }
    let mut longest: Option<&str> = None;
    for item in items {
        out.push(format!("- {item}"));
        if longest.map_or(true, |l| item.len() > l.len()) {
            longest = Some(item);
        }
    }
    let mut batch = Batch::new(out);
    batch.line(format!("{} items, longest: {}", items.len(), longest.unwrap_or("-")));
    drop(batch);
    out.len()
}
"""


def render_case(name, before, items, after):
    b = "vec![" + ", ".join(f'"{x}".to_string()' for x in before) + "]" if before else "Vec::<String>::new()"
    it = "[" + ", ".join(f'"{x}"' for x in items) + "]"
    want = "vec![" + ", ".join(f'"{x}".to_string()' for x in after) + "]"
    return T(name, f"out = {before}, items = {it}".replace("'", '"'), f"{{ let mut out = {b}; let items: [&str; {len(items)}] = {it}; let n = render(&mut out, &items); (n, out) }}", f"({len(after)}, {want})")


def render_py(before, items):
    out = list(before)
    if out:
        out[0] += " (cont.)"
    else:
        out.append("untitled")
    longest = None
    for it in items:
        out.append(f"- {it}")
        if longest is None or len(it.encode()) > len(longest.encode()):
            longest = it
    out.append(f"{len(items)} items, longest: {longest if longest is not None else '-'}")
    return out


def render_auto(name, before, items):
    return render_case(name, before, items, render_py(before, items))


P.append(fix(
    "fix-borrow-kept-alive", "Fix: borrows that live longer than they look", "easy", "where-borrows-end", ["NLL", "E0502", "E0499", "Drop", "scopes"],
    """
        `render` doesn't compile. Three borrows each outlive the place where you'd expect them to end: one is
        used again after a mutation, one is carried into the next turn of a loop, and one belongs to a value
        that still has work to do when it's dropped. Fix it without cloning, and keep `Batch` as it is.
    """,
    RENDER_STARTER,
    RENDER_SOLUTION,
    [render_auto("new_document", [], ["apples", "kiwi"]),
     render_auto("continues_a_title", ["Shopping"], ["milk"]),
     render_auto("no_items", [], []),
     render_auto("first_longest_wins", [], ["ab", "cd", "e"]),
     render_auto("keeps_earlier_lines", ["T", "- x", "1 items, longest: x"], ["yy"]),
     render_auto("longest_is_by_bytes", [], ["ééé", "abcd"])],
    [render_auto("title_twice", ["T (cont.)"], []),
     render_auto("empty_title", [""], ["a"]),
     render_auto("empty_item", [], ["", "a"]),
     render_auto("only_empty_items", [], ["", ""]),
     render_auto("dash_item", [], ["-"]),
     render_auto("unicode_items", ["日誌"], ["東京", "abc"]),
     render_auto("later_longer", [], ["a", "bb", "ccc"]),
     T("called_twice", "render([\"a\"]) twice into one out", '{ let mut out = Vec::new(); render(&mut out, &["a"]); let n = render(&mut out, &["bb"]); (n, out) }',
       '(5, ["untitled (cont.)", "- a", "1 items, longest: a", "- bb", "1 items, longest: bb"].map(String::from).to_vec())'),
     r"""
     #[test]
     fn random_vs_model() {
         let mut rng = anneal_prelude::Rng::new(6207);
         for _ in 0..300 {
             let before_len = rng.below(3);
             let mut before = Vec::new();
             for _ in 0..before_len {
                 let len = rng.below(3);
                 before.push(rng.string(len, "Tt"));
             }
             let n = rng.below(6);
             let mut owned = Vec::new();
             for _ in 0..n {
                 let len = rng.below(4);
                 owned.push(rng.string(len, "ab"));
             }
             let items: Vec<&str> = owned.iter().map(|s| s.as_str()).collect();
             let mut want = before.clone();
             match want.first_mut() {
                 Some(t) => t.push_str(" (cont.)"),
                 None => want.push("untitled".to_string()),
             }
             let mut longest: Option<&str> = None;
             for it in &items {
                 want.push(format!("- {it}"));
                 if longest.map_or(true, |l| it.len() > l.len()) {
                     longest = Some(it);
                 }
             }
             want.push(format!("{} items, longest: {}", items.len(), longest.unwrap_or("-")));
             let mut out = before.clone();
             let got = render(&mut out, &items);
             check!(format!("out = {before:?}, items = {items:?}"), (got, out), (want.len(), want));
         }
     }

     #[test]
     fn many_items() {
         let owned: Vec<String> = (0..100_000).map(|i| if i == 77_777 { "x".repeat(9) } else { "y".repeat(i % 8) }).collect();
         let items: Vec<&str> = owned.iter().map(|s| s.as_str()).collect();
         let mut out = Vec::new();
         let n = render(&mut out, &items);
         check!("100000 items, one of 9 bytes", (n, out[n - 1].clone()), (100_002, "100000 items, longest: xxxxxxxxx".to_string()));
     }
     """],
    [("rust", "A borrow lasts until its last use. `title` is used after `out.push`, and a `&mut` from `first_mut` can't coexist with `is_empty` or `push`. Can one `match` both use the title and handle its absence?"),
     ("rust", "`longest` points into `out` and survives into the next turn of the loop, where `out.push` runs again. The items themselves are in `items`, which nobody mutates: borrow from there."),
     ("rust", "`Batch` has a `Drop` impl, so its borrow of `out` lasts until it's dropped at the end of the scope, not until its last use. End it explicitly (`drop(batch)`) or give it a block.")],
    ("""With non-lexical lifetimes a borrow ends at its last use, and three things count as a use that's easy to miss. A later read (`title` after the push): matching `out.first_mut()` handles both cases, and the `None` arm can push because no binding holds the borrow there. A loop: a reference stored in a variable that the next turn reads is live across the loop's back edge, so each `out.push` conflicts with the previous turn's `longest`. Borrowing from `items` instead removes the conflict, since the output was never needed. A destructor: a value whose type implements `Drop` uses its borrows when it's dropped, so `batch` holds `out` to the end of the scope. `drop(batch)` (or `{ let mut batch = ..; .. }`) ends it where you choose, which is also when the footer lands, so `out.len()` must come after.

This is why a `MutexGuard` or `RefMut` held in a `let` keeps its lock until the end of the block.""", "O(n)", "O(1) extra"),
    "`std::mem::forget(batch)` also makes this compile. What does it break?",
    ["A borrow ends at its last use: later reads, the next loop turn and `Drop` all count.", "Borrow from the input instead of the output you're building.", "End a guard with `drop(x)` or a block."],
    rules=dict(methods=["clone", "to_owned"]),
    wrong=dict(
        forgets_the_batch=sub(RENDER_SOLUTION, "    drop(batch);\n", "    std::mem::forget(batch);\n"),
        counts_before_the_footer=sub(RENDER_SOLUTION, "    let mut batch = Batch::new(out);\n    batch.line(format!(\"{} items, longest: {}\", items.len(), longest.unwrap_or(\"-\")));\n    drop(batch);\n    out.len()",
                                     "    let n = out.len();\n    let mut batch = Batch::new(out);\n    batch.line(format!(\"{} items, longest: {}\", items.len(), longest.unwrap_or(\"-\")));\n    n"),
        last_longest=sub(RENDER_SOLUTION, "item.len() > l.len()", "item.len() >= l.len()"),
    ),
))

INTERNER_HEAD = r"""
use std::collections::HashMap;

/// Stores each distinct name once. Callers compare and hash the `&str`s it hands out instead of copying names.
pub struct Interner {
    ids: HashMap<String, usize>,
    names: Vec<String>,
}

impl Interner {
    pub fn new() -> Self {
        Interner { ids: HashMap::new(), names: Vec::new() }
    }

    /// Stores `name` if it's new, and returns the stored name.
    pub fn intern(&mut self, name: &str) -> &str {
        let i = match self.ids.get(name) {
            Some(&i) => i,
            None => {
                self.ids.insert(name.to_string(), self.names.len());
                self.names.push(name.to_string());
                self.names.len() - 1
            }
        };
        &self.names[i]
    }

    /// The stored name equal to `name`, if it has been interned.
    pub fn get(&self, name: &str) -> Option<&str> {
        self.ids.get(name).map(|&i| self.names[i].as_str())
    }

    pub fn len(&self) -> usize {
        self.names.len()
    }
}
"""

INTERNER_STARTER = INTERNER_HEAD + r"""
/// Interns every name and returns the stored names, in the same order.
pub fn intern_all<'i>(interner: &'i mut Interner, names: &[&str]) -> Vec<&'i str> {
    let mut out = Vec::new();
    for n in names {
        out.push(interner.intern(n));
    }
    out
}

/// Interns both ends of every edge. Returns the edges as pairs of stored names, and how many names were new.
pub fn intern_edges<'i>(interner: &'i mut Interner, edges: &[(&str, &str)]) -> (Vec<(&'i str, &'i str)>, usize) {
    let before = interner.len();
    let mut out = Vec::new();
    for &(a, b) in edges {
        let a = interner.intern(a);
        let b = interner.intern(b);
        out.push((a, b));
    }
    (out, interner.len() - before)
}
"""

INTERNER_SOLUTION = INTERNER_HEAD + r"""
/// Interns every name and returns the stored names, in the same order.
pub fn intern_all<'i>(interner: &'i mut Interner, names: &[&str]) -> Vec<&'i str> {
    for n in names {
        interner.intern(n);
    }
    let interner: &'i Interner = interner;
    names.iter().map(|n| interner.get(n).unwrap()).collect()
}

/// Interns both ends of every edge. Returns the edges as pairs of stored names, and how many names were new.
pub fn intern_edges<'i>(interner: &'i mut Interner, edges: &[(&str, &str)]) -> (Vec<(&'i str, &'i str)>, usize) {
    let before = interner.len();
    for &(a, b) in edges {
        interner.intern(a);
        interner.intern(b);
    }
    let added = interner.len() - before;
    let interner = &*interner;
    let pairs = edges.iter().map(|&(a, b)| (interner.get(a).unwrap(), interner.get(b).unwrap())).collect();
    (pairs, added)
}
"""

P.append(fix(
    "end-borrow-before-mutating", "Fix: a &mut method that returns a borrow", "easy", "where-borrows-end", ["E0499", "E0502", "&mut self -> &T", "reborrow as shared"],
    """
        `intern` takes `&mut self` and returns a `&str` into the interner. `intern_all` and `intern_edges` keep
        those `&str`s while calling `intern` again, and neither compiles. Fix both functions without changing
        `Interner` and without copying any name: the `&str`s they return must be the interner's stored names.
    """,
    INTERNER_STARTER,
    INTERNER_SOLUTION,
    [T("intern_all_example", "intern_all([\"a\", \"b\", \"a\"])", "(r.clone(), r[0].as_ptr() == r[2].as_ptr())", '(vec!["a", "b", "a"], true)',
       setup='let mut i = Interner::new();\nlet r = intern_all(&mut i, &["a", "b", "a"]);'),
     T("intern_all_then_len", "intern_all([\"x\", \"x\", \"y\"]), then len()", '{ intern_all(&mut i, &["x", "x", "y"]); i.len() }', "2", setup="let mut i = Interner::new();"),
     T("edges_example", "intern_edges([(\"a\", \"b\"), (\"b\", \"c\")])", "(pairs.clone(), added, pairs[0].1.as_ptr() == pairs[1].0.as_ptr())", '(vec![("a", "b"), ("b", "c")], 3, true)',
       setup='let mut i = Interner::new();\nlet (pairs, added) = intern_edges(&mut i, &[("a", "b"), ("b", "c")]);'),
     T("edges_counts_only_new_names", "intern \"a\" first, then intern_edges([(\"a\", \"z\")])", "(pairs, added)", '(vec![("a", "z")], 1)',
       setup='let mut i = Interner::new();\ni.intern("a");\nlet (pairs, added) = intern_edges(&mut i, &[("a", "z")]);'),
     T("empty_inputs", "intern_all([]), intern_edges([])", '{ let a = intern_all(&mut i, &[]).len(); let (p, n) = intern_edges(&mut i, &[]); (a, p.len(), n) }', "(0, 0, 0)", setup="let mut i = Interner::new();")],
    [T("self_loop_edge", "intern_edges([(\"a\", \"a\")])", "(pairs.clone(), added, pairs[0].0.as_ptr() == pairs[0].1.as_ptr())", '(vec![("a", "a")], 1, true)',
       setup='let mut i = Interner::new();\nlet (pairs, added) = intern_edges(&mut i, &[("a", "a")]);'),
     T("same_pointer_as_intern", "intern(\"k\"), then intern_all([\"k\"])", "p == q", "true",
       setup='let mut i = Interner::new();\nlet p = i.intern("k").as_ptr();\nlet q = intern_all(&mut i, &["k"])[0].as_ptr();'),
     T("all_already_known", "intern a, b; intern_edges([(\"b\", \"a\")])", "(pairs, added)", '(vec![("b", "a")], 0)',
       setup='let mut i = Interner::new();\ni.intern("a");\ni.intern("b");\nlet (pairs, added) = intern_edges(&mut i, &[("b", "a")]);'),
     T("empty_name", "intern_all([\"\", \"\"])", "(r.clone(), r[0].as_ptr() == r[1].as_ptr())", '(vec!["", ""], true)',
       setup='let mut i = Interner::new();\nlet r = intern_all(&mut i, &["", ""]);'),
     T("unicode_names", "intern_all([\"日本\", \"é\", \"日本\"])", "r", 'vec!["日本", "é", "日本"]', setup='let mut i = Interner::new();\nlet r = intern_all(&mut i, &["日本", "é", "日本"]);'),
     T("get_after_intern_all", "intern_all([\"m\"]), then get(\"m\"), get(\"n\")", '{ intern_all(&mut i, &["m"]); (i.get("m"), i.get("n")) }', '(Some("m"), None)', setup="let mut i = Interner::new();"),
     T("edges_order_kept", "intern_edges([(\"c\", \"b\"), (\"a\", \"c\")])", "(pairs, added)", '(vec![("c", "b"), ("a", "c")], 3)',
       setup='let mut i = Interner::new();\nlet (pairs, added) = intern_edges(&mut i, &[("c", "b"), ("a", "c")]);'),
     r"""
     #[test]
     fn random_vs_model() {
         let mut rng = anneal_prelude::Rng::new(6208);
         for _ in 0..300 {
             let n = rng.below(8);
             let mut owned = Vec::new();
             for _ in 0..n {
                 owned.push(rng.string(1, "abc"));
             }
             let names: Vec<&str> = owned.iter().map(|s| s.as_str()).collect();
             let mut i = Interner::new();
             let got = intern_all(&mut i, &names);
             // Equal names share one stored String.
             let mut shared = true;
             for x in 0..got.len() {
                 for y in 0..got.len() {
                     if (got[x] == got[y]) != (got[x].as_ptr() == got[y].as_ptr()) {
                         shared = false;
                     }
                 }
             }
             let got: Vec<String> = got.iter().map(|s| s.to_string()).collect();
             let mut distinct = names.clone();
             distinct.sort();
             distinct.dedup();
             check!(format!("intern_all({names:?})"), (got, shared, i.len()), (owned.clone(), true, distinct.len()));
         }
     }

     #[test]
     fn many_edges() {
         let owned: Vec<String> = (0..1000).map(|k| format!("n{k}")).collect();
         let edges: Vec<(&str, &str)> = (0..100_000).map(|k| (owned[k % 1000].as_str(), owned[(k * 7 + 3) % 1000].as_str())).collect();
         let mut i = Interner::new();
         let (pairs, added) = intern_edges(&mut i, &edges);
         check!("100000 edges over 1000 names", (pairs.len(), added, pairs[99_999]), (100_000, 1000, ("n999", "n996")));
     }
     """],
    [("rust", "A method `fn intern(&mut self, ..) -> &str` ties its result to the `&mut self` borrow. As long as the `&str` lives, the interner stays *mutably* borrowed: you can't call `intern` again, nor even `len`. There's no automatic downgrade to a shared borrow."),
     ("rust", "Do all the mutation first and throw the results away. Then reborrow the interner as shared (`let interner = &*interner;`) and look every name up with `get`, which takes `&self`: shared borrows can coexist.")],
    ("""A signature like `fn intern(&mut self, name: &str) -> &str` means the returned `&str` keeps `self` mutably borrowed for as long as it's used, even though it only reads. So the results of two calls can't be held at once. The fix splits the work into phases: intern everything (discarding the results), then turn the `&'i mut Interner` into a `&'i Interner` (`&*interner`, or a `let` with that type) and collect `get` results. Many shared borrows can coexist, and they can live for all of `'i` because the unique borrow is never used again. `intern_edges` must also read `len()` before taking those borrows, or compute it before them.

The other classic fix is an API change: return a `Copy` id (`Symbol(u32)`) from `intern` and add `resolve(&self, Symbol) -> &str`. That's what `rustc` and the `string-interner` crate do.""", "O(n) expected", "O(1) extra"),
    "`intern(&self, ..)` with interior mutability would let callers hold several results at once. What would the `names` storage have to guarantee for that to be sound?",
    ["`&mut self -> &T` keeps `self` mutably borrowed while the `&T` lives.", "Mutate first, then reborrow as shared (`&*r`) for many readers.", "Ids are the other way out."],
    rules=dict(methods=["clone", "to_owned", "leak"]),
    wrong=dict(
        counts_every_name=sub(INTERNER_SOLUTION, "    let added = interner.len() - before;\n", "    let added = 2 * edges.len() + before - before;\n"),
        counts_before_interning=sub(INTERNER_SOLUTION, "    let before = interner.len();\n    for &(a, b) in edges {\n        interner.intern(a);\n        interner.intern(b);\n    }\n    let added = interner.len() - before;\n",
                                    "    let added = interner.len();\n    for &(a, b) in edges {\n        interner.intern(a);\n        interner.intern(b);\n    }\n"),
        swaps_edge_ends=sub(INTERNER_SOLUTION, "(interner.get(a).unwrap(), interner.get(b).unwrap())", "(interner.get(b).unwrap(), interner.get(a).unwrap())"),
    ),
))

RUNS_DOC = r"""
use std::io::{self, BufRead};

/// Counts runs of consecutive lines with the same key. A line's key is the text before its first ':', or the
/// whole line (without its line ending) when it has none. Returns each run's key and length, in order.
"""

RUNS_STARTER = RUNS_DOC + r"""pub fn key_runs<R: BufRead>(mut input: R) -> io::Result<Vec<(String, usize)>> {
    let mut runs: Vec<(String, usize)> = Vec::new();
    let mut buf = String::new();
    let mut prev: Option<&str> = None;
    let mut count = 0;
    loop {
        buf.clear();
        if input.read_line(&mut buf)? == 0 {
            break;
        }
        let line = buf.trim_end_matches(['\n', '\r']);
        let key = line.split(':').next().unwrap_or(line);
        if prev == Some(key) {
            count += 1;
        } else {
            if let Some(p) = prev {
                runs.push((p.to_string(), count));
            }
            prev = Some(key);
            count = 1;
        }
    }
    if let Some(p) = prev {
        runs.push((p.to_string(), count));
    }
    Ok(runs)
}
"""

RUNS_SOLUTION = RUNS_DOC + r"""pub fn key_runs<R: BufRead>(mut input: R) -> io::Result<Vec<(String, usize)>> {
    let mut runs: Vec<(String, usize)> = Vec::new();
    let mut buf = String::new();
    let mut prev: Option<String> = None;
    let mut count = 0;
    loop {
        buf.clear();
        if input.read_line(&mut buf)? == 0 {
            break;
        }
        let line = buf.trim_end_matches(['\n', '\r']);
        let key = line.split(':').next().unwrap_or(line);
        if prev.as_deref() == Some(key) {
            count += 1;
        } else {
            if let Some(p) = prev.replace(key.to_string()) {
                runs.push((p, count));
            }
            count = 1;
        }
    }
    if let Some(p) = prev {
        runs.push((p, count));
    }
    Ok(runs)
}
"""


def runs_case(name, text, want):
    shown = text.replace("\n", "\\n").replace("\r", "\\r")
    exp = "vec![" + ", ".join(f'("{k}".to_string(), {n})' for k, n in want) + "]" if want else "Vec::<(String, usize)>::new()"
    return T(name, f'input "{shown}"', f'key_runs("{shown}".as_bytes()).unwrap()', exp)


P.append(fix(
    "fix-read-after-clear", "Fix: a reused buffer and a borrow of the last line", "easy", "where-borrows-end", ["E0502", "BufRead::read_line", "Option::replace", "allocation"],
    """
        `key_runs` reads lines into one reused `String`, the usual way to avoid an allocation per line. It
        doesn't compile: the previous line's key is a slice of the buffer that `clear` and `read_line` are about
        to overwrite. Fix it. Keep the single `read_line` buffer, and allocate once per run, not once per line:
        a test counts allocations.
    """,
    RUNS_STARTER,
    RUNS_SOLUTION,
    [runs_case("example", "a:1\na:2\nb:3\na:4\n", [("a", 2), ("b", 1), ("a", 1)]),
     runs_case("empty_input", "", []),
     runs_case("no_colon_uses_whole_line", "x\nx\ny\n", [("x", 2), ("y", 1)]),
     runs_case("first_colon_only", "k:v:w\nk:z\n", [("k", 2)]),
     runs_case("no_final_newline", "a:1\nb:2", [("a", 1), ("b", 1)]),
     runs_case("crlf", "a\r\na:2\r\n", [("a", 2)])],
    [ALLOC_COUNTER,
     runs_case("empty_lines", "\n\na\n", [("", 2), ("a", 1)]),
     runs_case("empty_key", ":x\n:y\nz\n", [("", 2), ("z", 1)]),
     runs_case("key_equals_line", "a\na:1\n", [("a", 2)]),
     runs_case("single_line", "only", [("only", 1)]),
     runs_case("alternating", "a\nb\na\nb\n", [("a", 1), ("b", 1), ("a", 1), ("b", 1)]),
     runs_case("unicode_keys", "日本:1\n日本:2\né\n", [("日本", 2), ("é", 1)]),
     runs_case("spaces_are_kept", " a:1\na:2\n", [(" a", 1), ("a", 1)]),
     runs_case("lone_cr_kept_inside", "a\rb\n", [("a\\rb", 1)]),
     r"""
     #[test]
     fn random_vs_brute_force() {
         let mut rng = anneal_prelude::Rng::new(6209);
         for _ in 0..300 {
             let n = rng.below(8);
             let mut text = String::new();
             for _ in 0..n {
                 let len = rng.below(4);
                 text.push_str(&rng.string(len, "ab:"));
                 text.push('\n');
             }
             let mut want: Vec<(String, usize)> = Vec::new();
             for line in text.lines() {
                 let key = line.split(':').next().unwrap();
                 match want.last_mut() {
                     Some((k, c)) if k == key => *c += 1,
                     _ => want.push((key.to_string(), 1)),
                 }
             }
             check!(format!("input {text:?}"), key_runs(text.as_bytes()).unwrap(), want);
         }
     }

     #[test]
     fn allocates_per_run_not_per_line() {
         let mut text = String::new();
         for i in 0..200_000 {
             text.push_str(&format!("key{}:{i}\n", i / 200));
         }
         let (runs, n) = allocs(|| key_runs(text.as_bytes()).unwrap());
         check!("200000 lines in 1000 runs of 200", (runs.len(), runs[999].clone()), (1000, ("key999".to_string(), 200)));
         check!("200000 lines in 1000 runs: at most 1100 allocations (one per line would be 200000)", n <= 1100, true);
     }
     """],
    [("rust", "`prev` is a `&str` into `buf`, and the next turn of the loop clears and refills `buf` while `prev` is still needed. What must the run's key be, if the buffer can't hold it?"),
     ("rust", "Own the key: `Option<String>`. Compare with `prev.as_deref() == Some(key)` so a repeated key costs nothing, and only on a new key swap the old one out with `prev.replace(key.to_string())`.")],
    ("""Reusing one buffer is exactly what makes a slice of it short-lived: `clear` and `read_line` need `&mut buf`, so no `&str` into it may survive to the next line. The key that has to outlive the line becomes an owned `String`, but only when it changes: `prev.as_deref() == Some(key)` compares without allocating, and `Option::replace` stores the new key and hands back the old one to push, so there's one allocation per run. Allocating `key.to_string()` on every line would compile too, and cost one allocation per line.

Syntax to remember: `while input.read_line(&mut buf)? != 0` (or `loop` with `buf.clear()` first) · `buf.trim_end_matches(['\\n', '\\r'])` · `prev.as_deref() == Some(key)` · `if let Some(old) = prev.replace(new) { .. }`.""", "O(total bytes)", "O(longest line + output)"),
    "`BufRead::lines()` would make this compile with no thought at all. What does it cost, and when is that fine?",
    ["A slice of a reused buffer can't outlive the next `clear`.", "Keep owned data only for what must survive, and only when it changes.", "`Option::replace` swaps in a new value and returns the old one."],
    rules=dict(methods=["clone", "to_owned", "lines"]),
    wrong=dict(
        allocates_per_line=r"""
            use std::io::{self, BufRead};

            pub fn key_runs<R: BufRead>(mut input: R) -> io::Result<Vec<(String, usize)>> {
                let mut runs: Vec<(String, usize)> = Vec::new();
                let mut buf = String::new();
                let mut prev: Option<String> = None;
                let mut count = 0;
                loop {
                    buf.clear();
                    if input.read_line(&mut buf)? == 0 {
                        break;
                    }
                    let line = buf.trim_end_matches(['\n', '\r']);
                    let key = line.split(':').next().unwrap_or(line).to_string();
                    if prev.as_ref() == Some(&key) {
                        count += 1;
                    } else {
                        if let Some(p) = prev.take() {
                            runs.push((p, count));
                        }
                        count = 1;
                    }
                    prev = Some(key);
                }
                if let Some(p) = prev {
                    runs.push((p, count));
                }
                Ok(runs)
            }
        """,
        drops_the_last_run=sub(RUNS_SOLUTION, "    if let Some(p) = prev {\n        runs.push((p, count));\n    }\n", ""),
        keeps_the_newline=sub(RUNS_SOLUTION, "let line = buf.trim_end_matches(['\\n', '\\r']);", "let line = buf.trim_end_matches('\\n');"),
    ),
))

INDEX_HEAD = r"""
use std::collections::HashMap;

pub struct Index {
    postings: HashMap<String, Vec<u32>>,
    hits: HashMap<String, u64>,
}
"""

INDEX_STARTER = INDEX_HEAD + r"""
impl Index {
    pub fn new() -> Self {
        Index { postings: HashMap::new(), hits: HashMap::new() }
    }

    /// Records that document `doc` contains each whitespace-separated word of `text`. A document is listed once
    /// per word, and documents are added in increasing id order. Must not allocate a key for a word the index
    /// already has.
    pub fn add(&mut self, doc: u32, text: &str) {
        todo!()
    }

    /// The documents containing `word`, in increasing order (empty if none).
    pub fn docs(&self, word: &str) -> &[u32] {
        todo!()
    }

    /// The posting list of `word`, created empty if it's missing, for the caller to edit.
    pub fn docs_mut(&mut self, word: &str) -> &mut Vec<u32> {
        todo!()
    }

    /// Counts a lookup of `word` and returns how many times it has been looked up, this one included. Must not
    /// allocate for a word looked up before.
    pub fn hit(&mut self, word: &str) -> u64 {
        todo!()
    }

    /// Removes `doc` from every posting list and drops lists that become empty. Returns how many lists changed.
    pub fn remove_doc(&mut self, doc: u32) -> usize {
        todo!()
    }

    /// How many words have a posting list.
    pub fn words(&self) -> usize {
        self.postings.len()
    }
}
"""

INDEX_SOLUTION = INDEX_HEAD + r"""
impl Index {
    pub fn new() -> Self {
        Index { postings: HashMap::new(), hits: HashMap::new() }
    }

    /// Records that document `doc` contains each whitespace-separated word of `text`. A document is listed once
    /// per word, and documents are added in increasing id order. Must not allocate a key for a word the index
    /// already has.
    pub fn add(&mut self, doc: u32, text: &str) {
        for w in text.split_whitespace() {
            match self.postings.get_mut(w) {
                Some(list) => {
                    if list.last() != Some(&doc) {
                        list.push(doc);
                    }
                }
                None => {
                    self.postings.insert(w.to_string(), vec![doc]);
                }
            }
        }
    }

    /// The documents containing `word`, in increasing order (empty if none).
    pub fn docs(&self, word: &str) -> &[u32] {
        self.postings.get(word).map_or(&[], Vec::as_slice)
    }

    /// The posting list of `word`, created empty if it's missing, for the caller to edit.
    pub fn docs_mut(&mut self, word: &str) -> &mut Vec<u32> {
        self.postings.entry(word.to_string()).or_default()
    }

    /// Counts a lookup of `word` and returns how many times it has been looked up, this one included. Must not
    /// allocate for a word looked up before.
    pub fn hit(&mut self, word: &str) -> u64 {
        if let Some(n) = self.hits.get_mut(word) {
            *n += 1;
            return *n;
        }
        self.hits.insert(word.to_string(), 1);
        1
    }

    /// Removes `doc` from every posting list and drops lists that become empty. Returns how many lists changed.
    pub fn remove_doc(&mut self, doc: u32) -> usize {
        let mut changed = 0;
        self.postings.retain(|_, list| {
            if let Ok(i) = list.binary_search(&doc) {
                list.remove(i);
                changed += 1;
            }
            !list.is_empty()
        });
        changed
    }

    /// How many words have a posting list.
    pub fn words(&self) -> usize {
        self.postings.len()
    }
}
"""

IX_SETUP = 'let mut ix = Index::new();\nix.add(1, "rust borrow check");\nix.add(2, "rust rust lifetimes");\nix.add(5, "borrow");'
IX_DESC = 'add 1 "rust borrow check", 2 "rust rust lifetimes", 5 "borrow"'

P.append(write(
    "entry-returns-a-borrow", "The entry API and when not to use it", "easy", "where-borrows-end", ["HashMap::entry", "get_mut", "or_default", "retain", "allocation"],
    """
        Write the methods of `Index`, an inverted index from words to the documents that contain them, plus a
        count of lookups per word. Two methods must not allocate for a word they've seen before; a test
        counts allocations. `docs_mut` returns a borrow into the map that the caller edits.
    """,
    INDEX_STARTER,
    INDEX_SOLUTION,
    [T("docs_example", IX_DESC, '(ix.docs("rust"), ix.docs("borrow"), ix.docs("check"))', "(&[1u32, 2][..], &[1u32, 5][..], &[1u32][..])", setup=IX_SETUP),
     T("unknown_word_is_empty", IX_DESC + "; docs(\"go\")", '(ix.docs("go"), ix.words())', "(&[][..], 4)", setup=IX_SETUP),
     T("docs_mut_creates_and_edits", IX_DESC + "; docs_mut(\"go\").push(9); docs_mut(\"rust\").retain(|&d| d != 1)", '(ix.docs("go"), ix.docs("rust"))', "(&[9u32][..], &[2u32][..])",
       setup=IX_SETUP + '\nix.docs_mut("go").push(9);\nix.docs_mut("rust").retain(|&d| d != 1);'),
     T("hit_counts", "hit rust, rust, go, rust", '(ix.hit("rust"), ix.hit("rust"), ix.hit("go"), ix.hit("rust"))', "(1, 2, 1, 3)", setup="let mut ix = Index::new();"),
     T("remove_doc", IX_DESC + "; remove_doc(1)", '(ix.remove_doc(1), ix.docs("rust"), ix.docs("check"), ix.words())', "(3, &[2u32][..], &[][..], 3)", setup=IX_SETUP),
     T("repeated_word_listed_once", IX_DESC + "; docs(\"rust\") after doc 2 said it twice", 'ix.docs("rust")', "&[1u32, 2][..]", setup=IX_SETUP)],
    [ALLOC_COUNTER,
     T("remove_unknown_doc", IX_DESC + "; remove_doc(3)", "(ix.remove_doc(3), ix.words())", "(0, 4)", setup=IX_SETUP),
     T("remove_all_docs", IX_DESC + "; remove_doc 1, 2, 5", "(ix.remove_doc(1), ix.remove_doc(2), ix.remove_doc(5), ix.words())", "(3, 2, 1, 0)", setup=IX_SETUP),
     T("empty_text", "add 1 \"\", add 2 \"   \"", "ix.words()", "0", setup='let mut ix = Index::new();\nix.add(1, "");\nix.add(2, "   ");'),
     T("case_sensitive", "add 1 \"Rust rust\"", '(ix.docs("Rust"), ix.docs("rust"), ix.docs("RUST"))', "(&[1u32][..], &[1u32][..], &[][..])", setup='let mut ix = Index::new();\nix.add(1, "Rust rust");'),
     T("tabs_and_newlines", "add 3 \"a\\tb\\n a\"", '(ix.docs("a"), ix.docs("b"))', "(&[3u32][..], &[3u32][..])", setup='let mut ix = Index::new();\nix.add(3, "a\\tb\\n a");'),
     T("hit_is_separate_from_docs", IX_DESC + "; hit(\"rust\")", '(ix.hit("rust"), ix.docs("rust"))', "(1, &[1u32, 2][..])", setup=IX_SETUP),
     T("docs_mut_existing_keeps_list", IX_DESC + "; docs_mut(\"borrow\") without editing", '{ ix.docs_mut("borrow"); (ix.docs("borrow"), ix.words()) }', "(&[1u32, 5][..], 4)", setup=IX_SETUP),
     T("docs_mut_missing_then_empty", "docs_mut(\"x\") on a new index", '{ ix.docs_mut("x"); (ix.docs("x"), ix.words()) }', "(&[][..], 1)", setup="let mut ix = Index::new();"),
     T("unicode_words", "add 4 \"日本 café 日本\"", '(ix.docs("日本"), ix.docs("café"))', "(&[4u32][..], &[4u32][..])", setup='let mut ix = Index::new();\nix.add(4, "日本 café 日本");'),
     r"""
     #[test]
     fn random_vs_model() {
         use std::collections::BTreeMap;
         let mut rng = anneal_prelude::Rng::new(6210);
         for _ in 0..200 {
             let mut ix = Index::new();
             let mut model: BTreeMap<String, Vec<u32>> = BTreeMap::new();
             let mut ops = Vec::new();
             let mut doc = 0;
             for _ in 0..8 {
                 if rng.below(3) > 0 {
                     doc += 1 + rng.below(2) as u32;
                     let len = rng.below(8);
                     let text = rng.string(len, "ab c");
                     ix.add(doc, &text);
                     for w in text.split_whitespace() {
                         let list = model.entry(w.to_string()).or_default();
                         if list.last() != Some(&doc) {
                             list.push(doc);
                         }
                     }
                     ops.push(format!("add {doc} {text:?}"));
                 } else {
                     let d = 1 + rng.below(doc as usize + 1) as u32;
                     let mut changed = 0;
                     model.retain(|_, list| {
                         if let Some(i) = list.iter().position(|&x| x == d) {
                             list.remove(i);
                             changed += 1;
                         }
                         !list.is_empty()
                     });
                     ops.push(format!("remove_doc {d}"));
                     check!(ops.join(", "), ix.remove_doc(d), changed);
                 }
             }
             check!(format!("{}; words()", ops.join(", ")), ix.words(), model.len());
             for w in ["a", "b", "ab", "ba", "c", "zz"] {
                 let want: &[u32] = model.get(w).map_or(&[], |v| v.as_slice());
                 check!(format!("{}; docs({w:?})", ops.join(", ")), ix.docs(w), want);
             }
         }
     }

     #[test]
     fn known_words_do_not_allocate() {
         let mut ix = Index::new();
         let words = ["alpha", "beta", "gamma", "delta"];
         for w in words {
             ix.hit(w);
         }
         let (_, n) = allocs(|| {
             for i in 0..100_000 {
                 ix.hit(words[i % 4]);
             }
         });
         check!("100000 hits on 4 known words: allocations", n, 0);
         check!("hits on alpha", ix.hit("alpha"), 25_002);
         let text = words.repeat(25_000).join(" ");
         ix.add(1, &text);
         let (_, n) = allocs(|| ix.add(2, &text));
         check!("add(2, 100000 words, 4 known): at most 16 allocations (a key per word would be 100000)", n <= 16, true);
         check!("docs(gamma)", ix.docs("gamma"), &[1u32, 2][..]);
     }
     """],
    [("rust", "`entry(key)` needs an owned key, so `entry(w.to_string())` allocates on every call, hit or miss. `get_mut(w)` looks up by `&str` (a `HashMap<String, _>` accepts any `&Q` where `String: Borrow<Q>`), and you insert only on a miss."),
     ("rust", "`if let Some(n) = map.get_mut(k) { .. return *n; }` then `insert` compiles: what's returned is a copy, not a borrow. `docs_mut` has to return a borrow, and there `entry(..).or_default()` is the simple answer."),
     ("rust", "`HashMap::retain(|k, v| ..)` hands the closure `&mut V`, so it can edit each list before deciding whether to keep it.")],
    ("""The entry API does one lookup and returns `&mut V` borrowed from the map, which is what `docs_mut` needs: `entry(word.to_string()).or_default()`. Its cost is the owned key, built on every call. On a hot path where most keys already exist, look up by `&str` first: `get_mut` for the hit, `insert` for the miss. That shape compiles because the hit branch returns a number, not a borrow; returning the `&mut` from `get_mut` early and inserting otherwise is the borrow checker's known blind spot (NLL problem case 3, in the Hard stage). `docs` returns a slice borrowed from `&self`; `map_or(&[], Vec::as_slice)` gives an empty slice for a missing word.

Syntax to remember: `map.entry(k).or_default()` · `map.entry(k).and_modify(|n| *n += 1).or_insert(1)` (also allocates `k` every time) · `map.get(word).map_or(&[], Vec::as_slice)` · `map.retain(|_, v| { ..; !v.is_empty() })`.""", "O(words) for add; O(1) expected per lookup; O(total postings) remove_doc", "O(1) extra"),
    "`hashbrown`'s `entry_ref` takes a borrowed key and allocates only on insert. Why can't std's `entry` do that with its current signature?",
    ["`entry` returns a borrow into the map, but costs an owned key.", "`get_mut` then `insert` avoids the allocation for hits.", "`retain` edits and filters a map in place."],
    related=("L2", "S4"),
    wrong=dict(
        hit_through_entry=sub(INDEX_SOLUTION, "        if let Some(n) = self.hits.get_mut(word) {\n            *n += 1;\n            return *n;\n        }\n        self.hits.insert(word.to_string(), 1);\n        1",
                              "        *self.hits.entry(word.to_string()).and_modify(|n| *n += 1).or_insert(1)"),
        add_through_entry=sub(INDEX_SOLUTION, "            match self.postings.get_mut(w) {\n                Some(list) => {\n                    if list.last() != Some(&doc) {\n                        list.push(doc);\n                    }\n                }\n                None => {\n                    self.postings.insert(w.to_string(), vec![doc]);\n                }\n            }",
                              "            let list = self.postings.entry(w.to_string()).or_default();\n            if list.last() != Some(&doc) {\n                list.push(doc);\n            }"),
        lists_a_doc_twice=sub(INDEX_SOLUTION, "                    if list.last() != Some(&doc) {\n                        list.push(doc);\n                    }", "                    list.push(doc);"),
        keeps_empty_lists=sub(INDEX_SOLUTION, "            !list.is_empty()\n", "            true\n"),
    ),
))

# ---------------------------------------------------------------- reborrows (medium)

REPORT_HEAD = r"""
use std::fmt::Write;

/// Writes `s` into `w`. Like most generic sinks, it takes the writer by value.
fn put<W: Write>(mut w: W, s: &str) {
    w.write_str(s).expect("writing to a String can't fail");
}

/// Adds `msg` to `alerts`, if there are alerts to add to.
fn note(alerts: Option<&mut Vec<String>>, msg: &str) {
    if let Some(a) = alerts {
        a.push(msg.to_string());
    }
}

/// Writes the first `header` lines as "[<line> / <line>]", then every remaining line as "; <line>". Each
/// remaining line that starts with '!' is also noted in `alerts` (when given), and the last note is
/// "<n> lines", where n counts the remaining lines. Returns n.
"""

REPORT_STARTER = REPORT_HEAD + r"""pub fn report<'a, I>(out: &mut String, mut lines: I, header: usize, alerts: Option<&mut Vec<String>>) -> usize
where
    I: Iterator<Item = &'a str>,
{
    let head: Vec<&str> = lines.take(header).collect();
    put(out, "[");
    put(out, &head.join(" / "));
    put(out, "]");
    let mut n = 0;
    for line in lines {
        put(out, "; ");
        put(out, line);
        if line.starts_with('!') {
            note(alerts, line);
        }
        n += 1;
    }
    note(alerts, &format!("{n} lines"));
    n
}
"""

REPORT_SOLUTION = REPORT_HEAD + r"""pub fn report<'a, I>(out: &mut String, mut lines: I, header: usize, mut alerts: Option<&mut Vec<String>>) -> usize
where
    I: Iterator<Item = &'a str>,
{
    let head: Vec<&str> = lines.by_ref().take(header).collect();
    put(&mut *out, "[");
    put(&mut *out, &head.join(" / "));
    put(&mut *out, "]");
    let mut n = 0;
    for line in lines {
        put(&mut *out, "; ");
        put(&mut *out, line);
        if line.starts_with('!') {
            note(alerts.as_deref_mut(), line);
        }
        n += 1;
    }
    note(alerts, &format!("{n} lines"));
    n
}
"""


def report_case(name, text, header, out, alerts, n, with_alerts=True):
    shown = text.replace("\n", "\\n")
    if with_alerts:
        al = "vec![" + ", ".join(f'"{a}".to_string()' for a in alerts) + "]" if alerts else "Vec::<String>::new()"
        call = f'{{ let mut out = String::new(); let mut alerts = Vec::new(); let n = report(&mut out, "{shown}".lines(), {header}, Some(&mut alerts)); (out, alerts, n) }}'
        return T(name, f'lines "{shown}", header {header}, with alerts', call, f'("{out}".to_string(), {al}, {n})')
    call = f'{{ let mut out = String::new(); let n = report(&mut out, "{shown}".lines(), {header}, None); (out, n) }}'
    return T(name, f'lines "{shown}", header {header}, no alerts', call, f'("{out}".to_string(), {n})')


def report_py(text, header):
    lines = text.split("\n")
    if lines[-1] == "":
        lines.pop()
    head, rest = lines[:header], lines[header:]
    out = "[" + " / ".join(head) + "]" + "".join("; " + l for l in rest)
    alerts = [l for l in rest if l.startswith("!")] + [f"{len(rest)} lines"]
    return out, alerts, len(rest)


def report_auto(name, text, header, with_alerts=True):
    out, alerts, n = report_py(text, header)
    return report_case(name, text, header, out, alerts, n, with_alerts)


P.append(fix(
    "fix-moved-mut-into-generic", "Fix: a &mut moved where you meant to lend it", "medium", "reborrows", ["E0382", "reborrow", "as_deref_mut", "Iterator::by_ref"],
    """
        `report` doesn't compile: three things it means to lend are moved instead, each of a different kind.
        Fix it without collecting all the lines first and without changing `put` or `note`.
    """,
    REPORT_STARTER,
    REPORT_SOLUTION,
    [report_auto("example", "Title\nv1\nok\n!disk full", 2),
     report_auto("no_header", "a\n!b", 0),
     report_auto("header_longer_than_input", "a\nb", 5),
     report_auto("without_alerts", "h\n!x\ny", 1, with_alerts=False),
     report_auto("empty_input", "", 1),
     T("appends_to_existing_output", "out = \">\", lines \"h\\nb\", header 1", '{ let mut out = String::from(">"); report(&mut out, "h\\nb".lines(), 1, None); out }', '">[h]; b".to_string()')],
    [report_auto("every_line_alert", "!a\n!b\n!c", 1),
     report_auto("bang_in_header_not_noted", "!h\nx", 1),
     report_auto("bang_not_first_char", "h\na!\n !b", 1),
     report_auto("empty_lines", "\n\nx\n", 1),
     report_auto("unicode", "日本\n!é", 1),
     report_auto("header_exactly_all", "a\nb", 2),
     report_auto("header_zero_empty", "", 0),
     T("keeps_existing_alerts", "alerts [\"old\"], lines \"h\\n!x\", header 1", '{ let mut out = String::new(); let mut alerts = vec!["old".to_string()]; report(&mut out, "h\\n!x".lines(), 1, Some(&mut alerts)); alerts }', 'vec!["old", "!x", "1 lines"]'),
     T("any_iterator", "lines from a Vec, header 1", '{ let v = vec!["a", "!b"]; let mut out = String::new(); let n = report(&mut out, v.into_iter(), 1, None); (out, n) }', '("[a]; !b".to_string(), 1)'),
     r"""
     #[test]
     fn random_vs_brute_force() {
         let mut rng = anneal_prelude::Rng::new(6211);
         for _ in 0..300 {
             let n = rng.below(7);
             let mut owned = Vec::new();
             for _ in 0..n {
                 let len = rng.below(3);
                 owned.push(rng.string(len, "!a"));
             }
             let header = rng.below(5);
             let lines: Vec<&str> = owned.iter().map(|s| s.as_str()).collect();
             let (head, rest) = lines.split_at(header.min(lines.len()));
             let mut want_out = format!("[{}]", head.join(" / "));
             let mut want_alerts: Vec<String> = Vec::new();
             for l in rest {
                 want_out.push_str(&format!("; {l}"));
                 if l.starts_with('!') {
                     want_alerts.push(l.to_string());
                 }
             }
             want_alerts.push(format!("{} lines", rest.len()));
             let mut out = String::new();
             let mut alerts = Vec::new();
             let got = report(&mut out, lines.iter().copied(), header, Some(&mut alerts));
             check!(format!("lines {lines:?}, header {header}"), (out, alerts, got), (want_out, want_alerts, rest.len()));
         }
     }

     #[test]
     fn long_input() {
         let text = "x\n!y\n".repeat(50_000);
         let mut out = String::new();
         let mut alerts = Vec::new();
         let n = report(&mut out, text.lines(), 2, Some(&mut alerts));
         check!("100000 lines, header 2", (n, alerts.len(), out.len(), alerts[49_999].clone()), (99_998, 50_000, 350_001, "99998 lines".to_string()));
     }
     """],
    [("rust", "Three moves: `out` (a `&mut String`) into `put`'s generic `W`, `lines` into `take`, and `alerts` (an `Option<&mut Vec<String>>`) into `note` inside a loop. None of them is `Copy`."),
     ("rust", "A `&mut` passed where the parameter type is exactly `&mut T` is reborrowed automatically; into a generic `W` it's moved. `&mut *out` lends a fresh, shorter borrow. Iterators have `by_ref()` for the same purpose."),
     ("rust", "For `Option<&mut T>`, `as_deref_mut()` gives an `Option<&mut T>` that reborrows the one inside, leaving `alerts` usable. `note` has one more call after the loop: that one can take `alerts` itself.")],
    ("""`&mut T` isn't `Copy`, so using one by value moves it. Rust inserts a reborrow (`&mut *out`) only where it knows the target type is a `&mut`; a generic parameter `W` isn't known to be one, so `put(out, ..)` moves `out` into `W = &mut String`. The same holds for anything that wraps a `&mut`: an `Option<&mut Vec<String>>` moves into `note`, and an iterator moves into `take` (its adapters take `self`). Each has its own way to lend: `&mut *out`; `alerts.as_deref_mut()` (or `alerts.as_mut().map(|a| &mut **a)`), which needs `mut alerts`; and `lines.by_ref()`, which is `&mut I`, itself an iterator. The last use of each can still move.

Syntax to remember: `put(&mut *out, s)` · `fn f(mut alerts: Option<&mut Vec<String>>)` then `alerts.as_deref_mut()` · `lines.by_ref().take(n)`.""", "O(total length)", "O(header)"),
    "`fmt::Write` has `impl<W: Write + ?Sized> Write for &mut W`. Without it, could `put(&mut *out, ..)` work at all?",
    ["A `&mut` moves into a generic parameter; `&mut *r` lends it instead.", "`Option<&mut T>::as_deref_mut()` reborrows the inner `&mut`.", "`Iterator::by_ref()` lends an iterator to a consuming adapter."],
    rules=dict(methods=["clone", "to_owned"], lines=9),
    wrong=dict(
        alerts_taken_once=sub(REPORT_SOLUTION, "note(alerts.as_deref_mut(), line);", "note(alerts.take(), line);"),
        header_counted=sub(REPORT_SOLUTION, "    let mut n = 0;\n", "    let mut n = head.len();\n"),
        head_joined_without_spaces=sub(REPORT_SOLUTION, 'head.join(" / ")', 'head.join("/")'),
    ),
))

METER_HEAD = r"""
/// A nested list of readings.
pub enum Item {
    One(i32),
    Many(Vec<Item>),
}

pub struct Meter {
    pub readings: Vec<i32>,
    pub offset: i32,
    pub log: Vec<String>,
}
"""

METER_STARTER = METER_HEAD + r"""
/// Calls `f` on every reading in `items`, depth first, in order.
fn walk<F: FnMut(i32)>(items: &[Item], mut f: F) {
    for it in items {
        match it {
            Item::One(x) => f(*x),
            Item::Many(inner) => walk(inner, f),
        }
    }
}

impl Meter {
    fn offset(&self) -> i32 {
        self.offset
    }

    /// Adds the offset to every recorded reading.
    pub fn calibrate(&mut self) {
        self.readings.iter_mut().for_each(|r| *r += self.offset());
    }

    /// Records every value of every batch. After each batch, logs "batch <i>: <n> above" where n counts the
    /// values above `limit` so far. Returns the final count.
    pub fn record(&mut self, batches: &[&[i32]], limit: i32) -> usize {
        let mut above = 0;
        let mut add = |x: i32| {
            self.readings.push(x);
            if x > limit {
                above += 1;
            }
        };
        for (i, batch) in batches.iter().enumerate() {
            for &x in *batch {
                add(x);
            }
            self.log.push(format!("batch {i}: {above} above"));
        }
        above
    }

    /// Records every reading in `items`, depth first, and returns their sum.
    pub fn record_nested(&mut self, items: &[Item]) -> i64 {
        let mut sum = 0;
        walk(items, |x| {
            self.readings.push(x);
            sum += x as i64;
        });
        sum
    }
}
"""

METER_SOLUTION = METER_HEAD + r"""
/// Calls `f` on every reading in `items`, depth first, in order.
fn walk<F: FnMut(i32)>(items: &[Item], f: &mut F) {
    for it in items {
        match it {
            Item::One(x) => f(*x),
            Item::Many(inner) => walk(inner, f),
        }
    }
}

impl Meter {
    fn offset(&self) -> i32 {
        self.offset
    }

    /// Adds the offset to every recorded reading.
    pub fn calibrate(&mut self) {
        self.readings.iter_mut().for_each(|r| *r += self.offset);
    }

    /// Records every value of every batch. After each batch, logs "batch <i>: <n> above" where n counts the
    /// values above `limit` so far. Returns the final count.
    pub fn record(&mut self, batches: &[&[i32]], limit: i32) -> usize {
        let mut above = 0;
        for (i, batch) in batches.iter().enumerate() {
            let mut add = |x: i32| {
                self.readings.push(x);
                if x > limit {
                    above += 1;
                }
            };
            for &x in *batch {
                add(x);
            }
            self.log.push(format!("batch {i}: {above} above"));
        }
        above
    }

    /// Records every reading in `items`, depth first, and returns their sum.
    pub fn record_nested(&mut self, items: &[Item]) -> i64 {
        let mut sum = 0;
        walk(items, &mut |x| {
            self.readings.push(x);
            sum += x as i64;
        });
        sum
    }
}
"""

METER_NEW = "let mut m = Meter { readings: vec![], offset: 0, log: vec![] };"

P.append(fix(
    "fix-closure-borrows", "Fix: closures hold what they capture", "medium", "reborrows", ["E0502", "E0382", "closures", "disjoint captures", "FnMut", "&mut F"],
    """
        `Meter` doesn't compile. Each of its three methods uses a closure, and each closure borrows something
        for longer, or more broadly, than the code around it expects. Fix them. `walk` must stay generic over
        the closure type (no `dyn`), no reading may be copied into a temporary collection, and no `Cell` or
        `RefCell`.

        One fix that looks right for `walk` compiles, and then fails to build for a different reason. Read that
        error carefully.
    """,
    METER_STARTER,
    METER_SOLUTION,
    [T("calibrate_example", "readings [1, 2], offset 10; calibrate", "{ let mut m = Meter { readings: vec![1, 2], offset: 10, log: vec![] }; m.calibrate(); m.readings }", "vec![11, 12]"),
     T("record_logs_each_batch", "batches [[5, 20], [30], []], limit 10", "{ " + METER_NEW + " let n = m.record(&[&[5, 20], &[30], &[]], 10); (n, m.readings, m.log) }",
       '(2, vec![5, 20, 30], ["batch 0: 1 above", "batch 1: 2 above", "batch 2: 2 above"].map(String::from).to_vec())'),
     T("record_nested_depth_first", "[1, [2, [3]], 4]", "{ " + METER_NEW + " let s = m.record_nested(&[Item::One(1), Item::Many(vec![Item::One(2), Item::Many(vec![Item::One(3)])]), Item::One(4)]); (s, m.readings) }", "(10, vec![1, 2, 3, 4])"),
     T("record_nothing", "no batches", "{ " + METER_NEW + " (m.record(&[], 0), m.log.len()) }", "(0, 0)"),
     T("limit_is_strict", "batch [10, 11], limit 10", "{ " + METER_NEW + " m.record(&[&[10, 11]], 10) }", "1"),
     T("record_then_calibrate", "offset -1; record [[3]] limit 0; calibrate", "{ let mut m = Meter { readings: vec![], offset: -1, log: vec![] }; m.record(&[&[3]], 0); m.calibrate(); m.readings }", "vec![2]")],
    [T("calibrate_empty", "no readings", "{ let mut m = Meter { readings: vec![], offset: 5, log: vec![] }; m.calibrate(); m.readings.len() }", "0"),
     T("calibrate_twice", "readings [0], offset 3; calibrate twice", "{ let mut m = Meter { readings: vec![0], offset: 3, log: vec![] }; m.calibrate(); m.calibrate(); m.readings }", "vec![6]"),
     T("record_appends", "readings [9]; record [[1]] limit 5", "{ let mut m = Meter { readings: vec![9], offset: 0, log: vec![\"x\".to_string()] }; m.record(&[&[1]], 5); (m.readings, m.log.len()) }", "(vec![9, 1], 2)"),
     T("record_negative_limit", "batch [-3, -1, 0], limit -2", "{ " + METER_NEW + " m.record(&[&[-3, -1, 0]], -2) }", "2"),
     T("nested_empty", "[[], [[]]]", "{ " + METER_NEW + " (m.record_nested(&[Item::Many(vec![]), Item::Many(vec![Item::Many(vec![])])]), m.readings.len()) }", "(0, 0)"),
     T("nested_sum_is_i64", "[i32::MAX, i32::MAX]", "{ " + METER_NEW + " m.record_nested(&[Item::One(i32::MAX), Item::Many(vec![Item::One(i32::MAX)])]) }", "2 * i32::MAX as i64"),
     T("nested_order", "[[3, 1], 2]", "{ " + METER_NEW + " m.record_nested(&[Item::Many(vec![Item::One(3), Item::One(1)]), Item::One(2)]); m.readings }", "vec![3, 1, 2]"),
     T("record_count_carries_over", "batches [[11], [1], [12]], limit 10", "{ " + METER_NEW + " m.record(&[&[11], &[1], &[12]], 10); m.log }", '["batch 0: 1 above", "batch 1: 1 above", "batch 2: 2 above"].map(String::from).to_vec()'),
     r"""
     fn nest(depth: usize, leaf: i32) -> Item {
         let mut it = Item::One(leaf);
         for _ in 0..depth {
             it = Item::Many(vec![it]);
         }
         it
     }

     #[test]
     fn deep_and_wide() {
         let mut m = Meter { readings: vec![], offset: 1, log: vec![] };
         let items: Vec<Item> = (0..20_000).map(|i| nest(i % 5, i as i32)).collect();
         let s = m.record_nested(&items);
         m.calibrate();
         check!("20000 items nested up to 4 deep, then calibrate", (s, m.readings.len(), m.readings[19_999]), (199_990_000, 20_000, 20_000));
     }

     #[test]
     fn random_vs_model() {
         let mut rng = anneal_prelude::Rng::new(6212);
         for _ in 0..300 {
             let nb = rng.below(4);
             let mut batches: Vec<Vec<i32>> = Vec::new();
             for _ in 0..nb {
                 let len = rng.below(4);
                 batches.push(rng.vec(len, -5, 5));
             }
             let limit = rng.int(-3, 3) as i32;
             let offset = rng.int(-2, 2) as i32;
             let refs: Vec<&[i32]> = batches.iter().map(|b| b.as_slice()).collect();
             let mut m = Meter { readings: vec![], offset, log: vec![] };
             let n = m.record(&refs, limit);
             m.calibrate();
             let mut above = 0;
             let mut log = Vec::new();
             for (i, b) in batches.iter().enumerate() {
                 above += b.iter().filter(|&&x| x > limit).count();
                 log.push(format!("batch {i}: {above} above"));
             }
             let readings: Vec<i32> = batches.concat().iter().map(|x| x + offset).collect();
             check!(format!("batches {batches:?}, limit {limit}, offset {offset}"), (n, m.readings, m.log), (above, readings, log));
         }
     }
     """],
    [("rust", "In `calibrate`, the closure calls `self.offset()`, a method, so it captures all of `self` while `self.readings` is borrowed mutably. A closure that reads the field `self.offset` captures only that field (edition 2021)."),
     ("rust", "In `record`, `add` holds `&mut above` for as long as `add` lives, and it lives until its last call in the next batch. Could each batch get its own closure?"),
     ("rust", "`walk(inner, f)` moves `f` in a loop. `walk(inner, &mut f)` compiles, but each level instantiates `walk` with one more `&mut`: `walk::<&mut &mut &mut F>`, without end. Make `walk` take `f: &mut F` so every level passes the same type.")],
    ("""A closure's captures are borrows that last as long as the closure. `calibrate`'s closure calls a method on `self`, which captures `self` whole; since edition 2021, a closure that names a field (`self.offset`) captures only that path, so it's disjoint from `self.readings`. In `record`, `add` mutably borrows `above` until its last use, which the loop puts in the next batch, after the read in `format!`. Creating the closure inside the loop ends its borrow each time; `self.log.push` never conflicted, because `add` captures `self.readings`, not `self`.

`walk` moves `f` into the recursive call. `&mut f` fixes the move (`&mut F` implements `FnMut` too), but the recursion is then polymorphic: `walk::<F>` calls `walk::<&mut F>`, which calls `walk::<&mut &mut F>`, and monomorphization never ends. Taking `f: &mut F` keeps one type at every depth, and passing `f` down is an implicit reborrow.

Syntax to remember: `fn walk<F: FnMut(i32)>(items: &[Item], f: &mut F)` · `walk(items, &mut |x| { .. })` · `|r| *r += self.offset` (a field, not a method).""", "O(n)", "O(depth) stack"),
    "`&mut dyn FnMut(i32)` would also fix `walk`. What does it cost compared with `&mut F`?",
    ["A closure keeps its captures borrowed for as long as it lives.", "Edition 2021 closures capture field paths, not all of `self`.", "Recursing with `&mut f` on a generic `F` never stops instantiating; take `&mut F`."],
    rules=dict(methods=["clone", "collect", "to_vec"], types=["Cell", "RefCell"]),
    wrong=dict(
        count_per_batch=sub(sub(METER_SOLUTION, "        let mut above = 0;\n        for (i, batch) in batches.iter().enumerate() {\n            let mut add",
                                "        let mut total = 0;\n        for (i, batch) in batches.iter().enumerate() {\n            let mut above = 0;\n            let mut add"),
                            "            self.log.push(format!(\"batch {i}: {above} above\"));\n        }\n        above\n", "            self.log.push(format!(\"batch {i}: {above} above\"));\n            total += above;\n        }\n        total\n"),
        calibrate_with_stale_offset=sub(METER_SOLUTION, "self.readings.iter_mut().for_each(|r| *r += self.offset);", "let offset = self.offset().max(0);\n        self.readings.iter_mut().for_each(|r| *r += offset);"),
        walk_skips_nested=sub(METER_SOLUTION, "            Item::Many(inner) => walk(inner, f),\n", "            Item::Many(inner) => {\n                if let Some(Item::One(x)) = inner.first() {\n                    f(*x);\n                }\n            }\n"),
    ),
))

LIST_HEAD = r"""
pub struct Node {
    pub val: i32,
    pub next: Option<Box<Node>>,
}

/// A singly linked list. Lists can be long, so nothing here may recurse.
pub struct List {
    head: Option<Box<Node>>,
}

impl List {
    pub fn from_slice(xs: &[i32]) -> Self {
        let mut head = None;
        for &val in xs.iter().rev() {
            head = Some(Box::new(Node { val, next: head }));
        }
        List { head }
    }

    pub fn to_vec(&self) -> Vec<i32> {
        let mut out = Vec::new();
        let mut cur = self.head.as_deref();
        while let Some(node) = cur {
            out.push(node.val);
            cur = node.next.as_deref();
        }
        out
    }
"""

LIST_TAIL = r"""
impl Drop for List {
    fn drop(&mut self) {
        let mut cur = self.head.take();
        while let Some(mut node) = cur {
            cur = node.next.take();
        }
    }
}
"""

LIST_DOCS = dict(
    push_back="    /// Appends `val` at the end.\n    pub fn push_back(&mut self, val: i32) {\n",
    insert="    /// Inserts `val` just before the first element greater than `val` (at the end if there's none), so a\n    /// sorted list stays sorted and `val` goes after any equal elements.\n    pub fn insert_sorted(&mut self, val: i32) {\n",
    remove="    /// Removes every element `pred` accepts and returns how many it removed. `pred` sees each element once,\n    /// in order.\n    pub fn remove_if(&mut self, mut pred: impl FnMut(i32) -> bool) -> usize {\n",
    last="    /// The last element, for editing.\n    pub fn last_mut(&mut self) -> Option<&mut i32> {\n",
)

LIST_BODIES = dict(
    push_back="""        let mut cur = &mut self.head;
        while let Some(node) = cur {
            cur = &mut node.next;
        }
        *cur = Some(Box::new(Node { val, next: None }));
""",
    insert="""        let mut cur = &mut self.head;
        while cur.as_ref().is_some_and(|node| node.val <= val) {
            cur = &mut cur.as_mut().unwrap().next;
        }
        let rest = cur.take();
        *cur = Some(Box::new(Node { val, next: rest }));
""",
    remove="""        let mut removed = 0;
        let mut cur = &mut self.head;
        loop {
            match cur {
                None => break,
                Some(node) if pred(node.val) => {
                    *cur = node.next.take();
                    removed += 1;
                }
                Some(node) => cur = &mut node.next,
            }
        }
        removed
""",
    last="""        let mut cur = self.head.as_deref_mut()?;
        while let Some(next) = cur.next.as_deref_mut() {
            cur = next;
        }
        Some(&mut cur.val)
""",
)


def list_src(bodies):
    out = LIST_HEAD
    for k in ("push_back", "insert", "remove", "last"):
        out += "\n" + LIST_DOCS[k] + bodies[k] + "    }\n"
    return out + "}\n" + LIST_TAIL


LIST_SOLUTION = list_src(LIST_BODIES)
LIST_STARTER = list_src({k: "        todo!()\n" for k in LIST_BODIES}).replace("mut pred: impl", "pred: impl")


def with_body(key, body):
    b = dict(LIST_BODIES)
    b[key] = body
    return list_src(b)


P.append(write(
    "mut-cursor-loop", "&mut cursors in a loop", "medium", "reborrows", ["&mut in loops", "Option<Box<T>>", "NLL", "as_deref_mut"],
    """
        Write four methods of a singly linked list, each by walking a `&mut` cursor down the list: `push_back`,
        `insert_sorted`, `remove_if` and `last_mut`. Lists in the tests are long, so no recursion (that's also
        why `Drop` is written out).

        The shortest cursor loops for two of these are rejected by today's borrow checker even though they're
        sound. When that happens, rearrange the loop rather than reaching for `unsafe` or a second pass.
    """,
    LIST_STARTER,
    LIST_SOLUTION,
    [T("push_back", "[1, 2]; push_back 3", "{ let mut l = List::from_slice(&[1, 2]); l.push_back(3); l.to_vec() }", "vec![1, 2, 3]"),
     T("insert_sorted_middle", "[1, 3, 5]; insert_sorted 4", "{ let mut l = List::from_slice(&[1, 3, 5]); l.insert_sorted(4); l.to_vec() }", "vec![1, 3, 4, 5]"),
     T("insert_after_equal", "[5, 3]; insert_sorted 5", "{ let mut l = List::from_slice(&[5, 3]); l.insert_sorted(5); l.to_vec() }", "vec![5, 3, 5]"),
     T("remove_if_example", "[1, 2, 3, 4, 6]; remove even", "{ let mut l = List::from_slice(&[1, 2, 3, 4, 6]); let n = l.remove_if(|x| x % 2 == 0); (n, l.to_vec()) }", "(3, vec![1, 3])"),
     T("last_mut_example", "[7, 8, 9]; last += 100", "{ let mut l = List::from_slice(&[7, 8, 9]); *l.last_mut().unwrap() += 100; l.to_vec() }", "vec![7, 8, 109]"),
     T("empty_list", "[]", "{ let mut l = List::from_slice(&[]); (l.last_mut().is_none(), l.remove_if(|_| true), { l.insert_sorted(1); l.to_vec() }) }", "(true, 0, vec![1])")],
    [T("push_back_empty", "[]; push_back 1, 2", "{ let mut l = List::from_slice(&[]); l.push_back(1); l.push_back(2); l.to_vec() }", "vec![1, 2]"),
     T("insert_at_front", "[2, 3]; insert_sorted 1", "{ let mut l = List::from_slice(&[2, 3]); l.insert_sorted(1); l.to_vec() }", "vec![1, 2, 3]"),
     T("insert_at_end", "[1, 2]; insert_sorted 9", "{ let mut l = List::from_slice(&[1, 2]); l.insert_sorted(9); l.to_vec() }", "vec![1, 2, 9]"),
     T("insert_unsorted", "[1, 9, 2]; insert_sorted 5", "{ let mut l = List::from_slice(&[1, 9, 2]); l.insert_sorted(5); l.to_vec() }", "vec![1, 5, 9, 2]"),
     T("remove_all", "[4, 4, 4]; remove 4", "{ let mut l = List::from_slice(&[4, 4, 4]); (l.remove_if(|x| x == 4), l.to_vec(), l.last_mut().is_none()) }", "(3, vec![], true)"),
     T("remove_adjacent", "[1, 2, 2, 3, 2]; remove 2", "{ let mut l = List::from_slice(&[1, 2, 2, 3, 2]); let n = l.remove_if(|x| x == 2); (n, l.to_vec()) }", "(3, vec![1, 3])"),
     T("remove_sees_each_once_in_order", "[5, 6, 7, 8]; remove every other visited", "{ let mut l = List::from_slice(&[5, 6, 7, 8]); let mut seen = vec![]; let mut k = 0; let n = l.remove_if(|x| { seen.push(x); k += 1; k % 2 == 1 }); (n, l.to_vec(), seen) }", "(2, vec![6, 8], vec![5, 6, 7, 8])"),
     T("last_after_remove", "[1, 2, 3]; remove 3; last", "{ let mut l = List::from_slice(&[1, 2, 3]); l.remove_if(|x| x == 3); l.last_mut().copied() }", "Some(2)"),
     T("push_after_last_edit", "[1]; last = 5; push_back 6", "{ let mut l = List::from_slice(&[1]); *l.last_mut().unwrap() = 5; l.push_back(6); l.to_vec() }", "vec![5, 6]"),
     r"""
     #[test]
     fn random_vs_model() {
         let mut rng = anneal_prelude::Rng::new(6213);
         for _ in 0..300 {
             let n = rng.below(6);
             let start: Vec<i32> = rng.vec(n, 0, 5);
             let mut l = List::from_slice(&start);
             let mut model = start.clone();
             let mut ops = Vec::new();
             for _ in 0..6 {
                 let v = rng.int(0, 5) as i32;
                 match rng.below(4) {
                     0 => {
                         l.push_back(v);
                         model.push(v);
                         ops.push(format!("push_back {v}"));
                     }
                     1 => {
                         l.insert_sorted(v);
                         let at = model.iter().position(|&x| x > v).unwrap_or(model.len());
                         model.insert(at, v);
                         ops.push(format!("insert_sorted {v}"));
                     }
                     2 => {
                         let before = model.len();
                         model.retain(|&x| x != v);
                         ops.push(format!("remove_if(== {v})"));
                         check!(format!("{start:?}; {}", ops.join(", ")), l.remove_if(|x| x == v), before - model.len());
                     }
                     _ => {
                         if let Some(x) = l.last_mut() {
                             *x += 10;
                         }
                         if let Some(x) = model.last_mut() {
                             *x += 10;
                         }
                         ops.push("last += 10".to_string());
                     }
                 }
             }
             check!(format!("{start:?}; {}", ops.join(", ")), l.to_vec(), model);
         }
     }

     #[test]
     fn long_list() {
         let xs: Vec<i32> = (0..200_000).collect();
         let mut l = List::from_slice(&xs);
         l.insert_sorted(199_998);
         l.push_back(7);
         let removed = l.remove_if(|x| x % 2 == 1);
         *l.last_mut().unwrap() = -1;
         let v = l.to_vec();
         check!("0..200000; insert_sorted 199998; push_back 7; remove odd; last = -1", (removed, v.len(), v[99_999], v[100_000]), (100_001, 100_001, 199_998, -1));
     }
     """],
    [("rust", "The cursor is `let mut cur = &mut self.head;` (an `&mut Option<Box<Node>>`) and advancing is `cur = &mut node.next;`. When the loop ends, `*cur` is the slot to write into."),
     ("rust", "`while let Some(node) = cur { if stop { break; } cur = &mut node.next; }` is rejected: the borrow in `node` flows into `cur` on one path, so NLL keeps it alive on the `break` path too. Test first with a shared look (`cur.as_ref().is_some_and(..)`), then take the `&mut` only to advance."),
     ("rust", "For `remove_if`, a `match` with one arm per case (`None`, `Some(node) if pred(node.val)`, `Some(node)`) gives each arm its own borrow, so the removing arm can assign to `*cur`.")],
    ("""A `&mut` cursor is reborrowed on every step: `cur = &mut node.next` borrows through the old `cur`, which is never used again, so the chain of borrows stays linear. `push_back` and `last_mut` are the plain loop. The other two hit a limit of NLL: in `while let Some(node) = cur { if cond { break } cur = &mut node.next }` the borrow held by `node` must outlive `cur` on the advancing path, and NLL's regions don't distinguish paths, so after `break` `*cur` still counts as borrowed (E0499/E0506; Polonius accepts it). The workarounds keep the `&mut` borrow off the stopping path: check through a shared borrow first and reborrow only to advance (`cur = &mut cur.as_mut().unwrap().next`), or match with guards so each arm binds its own borrow.

Syntax to remember: `let mut cur = &mut self.head;` · `while cur.as_ref().is_some_and(|n| n.val <= val) { cur = &mut cur.as_mut().unwrap().next; }` · `*cur = Some(Box::new(Node { val, next: cur.take() }))` · `match cur { Some(node) if pred(node.val) => *cur = node.next.take(), Some(node) => cur = &mut node.next, None => break }`.""", "O(n) per operation", "O(1)"),
    "Why must `List` implement `Drop` by hand, when `Box` already frees its contents?",
    ["A `&mut` cursor advances by reborrowing through itself.", "NLL rejects a borrow that flows into the cursor on one path and must be dead on another; check first, then borrow.", "Match arms with guards bind separate borrows."],
    related=("L2", "D5"),
    wrong=dict(
        insert_before_equal=sub(LIST_SOLUTION, "node.val <= val", "node.val < val"),
        remove_first_only=sub(LIST_SOLUTION, "                    *cur = node.next.take();\n                    removed += 1;\n", "                    *cur = node.next.take();\n                    removed += 1;\n                    break;\n"),
        last_is_head=with_body("last", "        self.head.as_deref_mut().map(|n| &mut n.val)\n"),
    ),
))

SCRIPT_DOC = r"""
/// Runs these steps on `v` in order. "len" always means the length at that step.
///  1. push len
///  2. swap the first and last elements
///  3. insert a copy of the first element at index len / 2
///  4. add len to the last element
///  5. move the last element to the front
///  6. rotate left by len / 3
///  7. remove every element smaller than the first
///  8. log len
///  9. drop the last len / 4 elements
/// 10. pad with two copies of the last element
/// 11. append a copy of the first len / 2 elements
/// 12. log how many elements are greater than the first
"""

SCRIPT_STARTER = SCRIPT_DOC + r"""pub fn script(v: &mut Vec<i32>, log: &mut Vec<usize>) {
    v.push(v.len() as i32);
    v.swap(0, v.len() - 1);
    v.insert(v.len() / 2, v[0]);
    v[v.len() - 1] += v.len() as i32;
    v.insert(0, v.pop().unwrap());
    v.rotate_left(v.len() / 3);
    v.retain(|&x| x >= v[0]);
    log.push(v.len());
    v.truncate(v.len() - v.len() / 4);
    v.resize(v.len() + 2, v[v.len() - 1]);
    v.extend_from_within(..v.len() / 2);
    log.push(v.iter().filter(|&&x| x > v[0]).count());
}
"""

SCRIPT_SOLUTION = SCRIPT_DOC + r"""pub fn script(v: &mut Vec<i32>, log: &mut Vec<usize>) {
    v.push(v.len() as i32);
    let n = v.len();
    v.swap(0, n - 1);
    v.insert(v.len() / 2, v[0]);
    let n = v.len();
    v[n - 1] += n as i32;
    let last = v.pop().unwrap();
    v.insert(0, last);
    let n = v.len();
    v.rotate_left(n / 3);
    let first = v[0];
    v.retain(|&x| x >= first);
    log.push(v.len());
    v.truncate(v.len() - v.len() / 4);
    v.resize(v.len() + 2, v[v.len() - 1]);
    v.extend_from_within(..v.len() / 2);
    log.push(v.iter().filter(|&&x| x > v[0]).count());
}
"""


def script_py(v):
    v = list(v)
    log = []
    v.append(len(v))
    v[0], v[-1] = v[-1], v[0]
    v.insert(len(v) // 2, v[0])
    v[-1] += len(v)
    last = v.pop()
    v.insert(0, last)
    k = len(v) // 3
    v = v[k:] + v[:k]
    first = v[0]
    v = [x for x in v if x >= first]
    log.append(len(v))
    v = v[:len(v) - len(v) // 4]
    v += [v[-1]] * 2
    v += v[:len(v) // 2]
    log.append(sum(1 for x in v if x > v[0]))
    return v, log


def script_case(name, v):
    after, log = script_py(v)
    rv = "vec![" + ", ".join(map(str, v)) + "]" if v else "Vec::<i32>::new()"
    return T(name, f"script({v})", f"{{ let mut v: Vec<i32> = {rv}; let mut log = Vec::new(); script(&mut v, &mut log); (v, log) }}",
             f"(vec![{', '.join(map(str, after))}], vec![{', '.join(map(str, log))}])")


P.append(fix(
    "two-phase-borrows", "Fix: where two-phase borrows stop", "medium", "reborrows", ["two-phase borrows", "E0502", "E0499", "DerefMut", "IndexMut"],
    """
        `script` doesn't compile, but not every suspicious-looking line is at fault: `v.push(v.len() as i32)`
        compiles, and so do several others that read `v` inside a call that mutates it. Fix only the lines the
        compiler rejects, keeping each step's meaning.
    """,
    SCRIPT_STARTER,
    SCRIPT_SOLUTION,
    [script_case("empty", []),
     script_case("one", [7]),
     script_case("three", [1, 2, 3]),
     script_case("descending", [9, 5, 1, 0]),
     script_case("negatives", [-4, 8, -2, 6, 0])],
    [script_case("zeros", [0, 0, 0]),
     script_case("two", [5, 5]),
     script_case("six", [3, 1, 4, 1, 5, 9]),
     script_case("big_first", [100, 1, 2, 3, 4, 5, 6]),
     script_case("all_negative", [-1, -2, -3, -4]),
     script_case("mixed_eight", [2, 7, 1, 8, 2, 8, 1, 8]),
     script_case("ten", list(range(10))),
     script_case("ten_desc", list(range(10, 0, -1))),
     r"""
     fn model(v: &mut Vec<i32>) -> Vec<usize> {
         let mut log = Vec::new();
         let n = v.len() as i32;
         v.push(n);
         let n = v.len();
         v.swap(0, n - 1);
         let (at, first) = (v.len() / 2, v[0]);
         v.insert(at, first);
         let n = v.len();
         v[n - 1] += n as i32;
         let last = v.pop().unwrap();
         v.insert(0, last);
         let n = v.len();
         v.rotate_left(n / 3);
         let first = v[0];
         v.retain(|&x| x >= first);
         log.push(v.len());
         let n = v.len();
         v.truncate(n - n / 4);
         let last = v[v.len() - 1];
         v.push(last);
         v.push(last);
         let half = v[..v.len() / 2].to_vec();
         v.extend(half);
         let first = v[0];
         log.push(v.iter().filter(|&&x| x > first).count());
         log
     }

     #[test]
     fn random_vs_model() {
         let mut rng = anneal_prelude::Rng::new(6214);
         for _ in 0..400 {
             let n = rng.below(12);
             let start: Vec<i32> = rng.vec(n, -20, 20);
             let mut want = start.clone();
             let want_log = model(&mut want);
             let mut got = start.clone();
             let mut log = Vec::new();
             script(&mut got, &mut log);
             check!(format!("script({start:?})"), (got, log), (want, want_log));
         }
     }
     """],
    [("rust", "A two-phase borrow reserves the `&mut` for an autoref'd method call, lets the arguments read the value, then activates the borrow. That's why `v.push(v.len())` compiles. Which of the failing calls aren't `Vec`'s own methods?"),
     ("rust", "`swap` and `rotate_left` are slice methods: `v.swap(..)` first calls `DerefMut::deref_mut(v)` to get a `&mut [i32]`, and that call is an ordinary `&mut` borrow. `v[i] += x` goes through `IndexMut` the same way."),
     ("rust", "Two-phase only lets arguments *read*. `v.pop()` needs `&mut v` itself, and a closure argument keeps its borrow for the whole call. Hoist what the arguments need into a `let` just before the line, not at the top: the length changes between steps.")],
    ("""Two-phase borrows are the special case that makes `v.push(v.len())` legal: for an autoref'd method receiver (and compound assignment on primitives), the `&mut v` is only *reserved* while the arguments are evaluated, and they may read `v` in that time. It stops at four things here. A method found through `Deref` (`swap`, `rotate_left`, `get_mut`, `sort` are `[T]`'s methods, so `deref_mut` runs first and activates the borrow). An overloaded index (`v[v.len() - 1] += ..` calls `index_mut(&mut v, ..)` before the index is known). An argument that needs `&mut v` itself (`v.insert(0, v.pop().unwrap())`). And a closure argument (`retain(|&x| x >= v[0])`), which holds its borrow for the whole call. `Vec`'s own methods (`push`, `insert`, `truncate`, `resize`, `extend_from_within`) work, as does anything that mutates a *different* value (`log.push(..)`).

The trap in the fix is hoisting one `let n = v.len()` to the top: steps 1, 3 and 5 change the length.""", "O(n) per step", "O(1) extra"),
    "`v.swap(0, v.len() - 1)` compiles when `v: &mut [i32]` and fails when `v: &mut Vec<i32>`. Why?",
    ["Two-phase borrows: reserve the receiver's `&mut`, let arguments read, then activate.", "They don't cover `DerefMut` autoderef, `IndexMut`, `&mut` arguments or closures.", "Hoist a value just before the line that needs it."],
    rules=dict(methods=["clone", "to_vec"], lines=12),
    wrong=dict(
        length_hoisted_once=sub(sub(SCRIPT_SOLUTION, "    let n = v.len();\n    v[n - 1] += n as i32;\n", "    v[n - 1] += n as i32;\n"), "    let n = v.len();\n    v.rotate_left(n / 3);\n", "    v.rotate_left(n / 3);\n"),
        first_read_before_rotating=sub(SCRIPT_SOLUTION, "    let n = v.len();\n    v.rotate_left(n / 3);\n    let first = v[0];\n", "    let first = v[0];\n    let n = v.len();\n    v.rotate_left(n / 3);\n"),
        last_to_front_by_swap=sub(SCRIPT_SOLUTION, "    let last = v.pop().unwrap();\n    v.insert(0, last);\n", "    let n = v.len();\n    v.swap(0, n - 1);\n"),
    ),
))

ROUND_HEAD = r"""
use std::fmt::Write;

#[derive(Debug, PartialEq)]
pub struct Player {
    pub name: String,
    pub score: u32,
}

/// Appends `line` and a newline to any text sink, taken by value.
fn record<W: Write>(mut log: W, line: &str) {
    writeln!(log, "{line}").expect("writing to a String can't fail");
}

/// One round of a game. It borrows the players and the log from the caller for `'a`.
pub struct Round<'a> {
    players: &'a mut [Player],
    log: &'a mut String,
}
"""

ROUND_STARTER = ROUND_HEAD + r"""
impl<'a> Round<'a> {
    pub fn new(players: &'a mut [Player], log: &'a mut String) -> Self {
        Round { players, log }
    }

    /// Moves up to `points` from player `from` to player `to` (never more than `from` has), logs
    /// "<from's name> -> <to's name>: <moved>", and returns how many points moved.
    pub fn transfer(&mut self, from: usize, to: usize, points: u32) -> u32 {
        let a = &mut self.players[from];
        let b = &mut self.players[to];
        let moved = points.min(a.score);
        a.score -= moved;
        b.score += moved;
        record(self.log, &format!("{} -> {}: {moved}", a.name, b.name));
        moved
    }

    /// Renames player `i` and logs "<old> is now <new>".
    pub fn rename(&mut self, i: usize, new: &str) {
        let p = &mut self.players[i];
        let old = std::mem::replace(&mut p.name, new.to_string());
        record(self.log, &format!("{old} is now {}", p.name));
    }

    /// Ends the round: logs "round over" and hands the players back for the rest of `'a`.
    pub fn finish(&mut self) -> &'a mut [Player] {
        record(self.log, "round over");
        self.players
    }
}
"""

ROUND_SOLUTION = ROUND_HEAD + r"""
impl<'a> Round<'a> {
    pub fn new(players: &'a mut [Player], log: &'a mut String) -> Self {
        Round { players, log }
    }

    /// Moves up to `points` from player `from` to player `to` (never more than `from` has), logs
    /// "<from's name> -> <to's name>: <moved>", and returns how many points moved.
    pub fn transfer(&mut self, from: usize, to: usize, points: u32) -> u32 {
        let moved = points.min(self.players[from].score);
        self.players[from].score -= moved;
        self.players[to].score += moved;
        let (a, b) = (&self.players[from], &self.players[to]);
        record(&mut *self.log, &format!("{} -> {}: {moved}", a.name, b.name));
        moved
    }

    /// Renames player `i` and logs "<old> is now <new>".
    pub fn rename(&mut self, i: usize, new: &str) {
        let p = &mut self.players[i];
        let old = std::mem::replace(&mut p.name, new.to_string());
        record(&mut *self.log, &format!("{old} is now {}", p.name));
    }

    /// Ends the round: logs "round over" and hands the players back for the rest of `'a`.
    pub fn finish(self) -> &'a mut [Player] {
        record(self.log, "round over");
        self.players
    }
}
"""

ROUND_SETUP = 'let mut players = vec![Player { name: "ann".into(), score: 10 }, Player { name: "bo".into(), score: 3 }];\nlet mut log = String::new();'

P.append(fix(
    "fix-two-mut-into-players", "Fix: a struct of &mut: reborrow, sequence, hand back", "medium", "reborrows", ["E0499", "E0507", "reborrow", "&mut fields", "mem::replace"],
    """
        `Round` holds two `&mut` borrowed from its caller, and none of its methods compiles. Fix them. After
        `finish`, the caller must be able to keep using the players for as long as the original borrow lasts,
        even once the `Round` itself is gone. Transferring from a player to the same player is allowed (it
        moves nothing but is logged).

        `transfer` never needs two `&mut` at once, so don't split the slice (`split_at_mut`, `get_disjoint_mut`)
        and don't move values out (`swap`, `take`).
    """,
    ROUND_STARTER,
    ROUND_SOLUTION,
    [T("transfer_and_finish", "ann 10, bo 3; transfer 0 -> 1, 4; finish", "(ps.iter().map(|p| p.score).collect::<Vec<_>>(), moved)", "(vec![6, 7], 4)",
       setup=ROUND_SETUP + "\nlet (ps, moved) = {\n    let mut r = Round::new(&mut players, &mut log);\n    let moved = r.transfer(0, 1, 4);\n    (r.finish(), moved)\n};"),
     T("log_lines", "ann 10, bo 3; transfer 1 -> 0, 3; rename 1 to \"cy\"; finish", "log", '"bo -> ann: 3\\nbo is now cy\\nround over\\n".to_string()',
       setup=ROUND_SETUP + '\n{\n    let mut r = Round::new(&mut players, &mut log);\n    r.transfer(1, 0, 3);\n    r.rename(1, "cy");\n    r.finish();\n}'),
     T("transfer_is_capped", "ann 10, bo 3; transfer 1 -> 0, 50", "(moved, players[0].score, players[1].score)", "(3, 13, 0)",
       setup=ROUND_SETUP + "\nlet moved = Round::new(&mut players, &mut log).transfer(1, 0, 50);"),
     T("same_player", "ann 10; transfer 0 -> 0, 5", "(moved, players[0].score, log)", '(5, 10, "ann -> ann: 5\\n".to_string())',
       setup=ROUND_SETUP + "\nlet moved = Round::new(&mut players, &mut log).transfer(0, 0, 5);"),
     T("players_outlive_the_round", "finish, then edit the players through the returned slice", "(players[0].name.clone(), players[1].score)", '("zed".to_string(), 99)',
       setup=ROUND_SETUP + '\nlet ps = Round::new(&mut players, &mut log).finish();\nps[0].name = "zed".to_string();\nps[1].score = 99;')],
    [T("zero_points", "transfer 0 -> 1, 0", "(moved, players[0].score, log)", '(0, 10, "ann -> bo: 0\\n".to_string())', setup=ROUND_SETUP + "\nlet moved = Round::new(&mut players, &mut log).transfer(0, 1, 0);"),
     T("from_empty_player", "bo 0; transfer 1 -> 0, 5", "(moved, players[0].score)", "(0, 10)", setup=ROUND_SETUP + "\nplayers[1].score = 0;\nlet moved = Round::new(&mut players, &mut log).transfer(1, 0, 5);"),
     T("exact_balance", "transfer 1 -> 0, 3", "(moved, players[1].score)", "(3, 0)", setup=ROUND_SETUP + "\nlet moved = Round::new(&mut players, &mut log).transfer(1, 0, 3);"),
     T("rename_twice", "rename 0 to x, then y", "(players[0].name.clone(), log)", '("y".to_string(), "ann is now x\\nx is now y\\n".to_string())',
       setup=ROUND_SETUP + '\n{\n    let mut r = Round::new(&mut players, &mut log);\n    r.rename(0, "x");\n    r.rename(0, "y");\n}'),
     T("rename_empty", "rename 1 to \"\"", "log", '"bo is now \\n".to_string()', setup=ROUND_SETUP + '\nRound::new(&mut players, &mut log).rename(1, "");'),
     T("log_is_appended", "log starts \"start\\n\"; finish", "log", '"start\\nround over\\n".to_string()', setup=ROUND_SETUP + '\nlog.push_str("start\\n");\nRound::new(&mut players, &mut log).finish();'),
     T("transfer_after_rename_uses_new_name", "rename 0 to \"al\"; transfer 0 -> 1, 1", "log", '"ann is now al\\nal -> bo: 1\\n".to_string()',
       setup=ROUND_SETUP + '\n{\n    let mut r = Round::new(&mut players, &mut log);\n    r.rename(0, "al");\n    r.transfer(0, 1, 1);\n}'),
     T("no_log_without_calls", "new round, dropped", "log.len()", "0", setup=ROUND_SETUP + "\nlet _ = Round::new(&mut players, &mut log);"),
     r"""
     #[test]
     fn random_vs_model() {
         let mut rng = anneal_prelude::Rng::new(6215);
         for _ in 0..300 {
             let n = 1 + rng.below(4);
             let start: Vec<u32> = rng.vec(n, 0, 9);
             let mut players: Vec<Player> = start.iter().enumerate().map(|(i, &s)| Player { name: format!("p{i}"), score: s }).collect();
             let mut model = start.clone();
             let mut want_log = String::new();
             let mut log = String::new();
             let mut ops = Vec::new();
             let mut moves = Vec::new();
             {
                 let mut r = Round::new(&mut players, &mut log);
                 for _ in 0..5 {
                     let (f, t, pts) = (rng.below(n), rng.below(n), rng.below(8) as u32);
                     let moved = pts.min(model[f]);
                     model[f] -= moved;
                     model[t] += moved;
                     want_log.push_str(&format!("p{f} -> p{t}: {moved}\n"));
                     ops.push(format!("transfer {f} -> {t}, {pts}"));
                     moves.push((r.transfer(f, t, pts), moved));
                 }
                 let ps = r.finish();
                 want_log.push_str("round over\n");
                 let got: Vec<u32> = ps.iter().map(|p| p.score).collect();
                 check!(format!("scores {start:?}; {}", ops.join(", ")), got, model.clone());
             }
             check!(format!("scores {start:?}; {}; moved and log", ops.join(", ")), (moves.iter().map(|m| m.0).collect::<Vec<_>>(), log), (moves.iter().map(|m| m.1).collect::<Vec<_>>(), want_log));
         }
     }
     """],
    [("rust", "`let a = &mut self.players[from]; let b = &mut self.players[to];` can't compile even for different indices, and `from == to` is allowed anyway. Do you need both at once? The scores are `u32`: read, then write each one in its own statement."),
     ("rust", "`record(self.log, ..)` tries to move the `&'a mut String` out of `*self`, which you only have through `&mut self` (E0507). `&mut *self.log` lends it for the call instead."),
     ("rust", "From `&mut self` you can only lend what's inside for as long as *that* borrow lasts, not for `'a`. To hand the players back for all of `'a`, `finish` has to take the `Round` by value.")],
    ("""A struct of `&'a mut` fields is a bundle of loans, and the rules for using them depend on how you hold the struct. Through `&mut self`, you can reborrow a field (`&mut *self.log`) for as long as `self` is borrowed, but you can't move it out: that's E0507, which a generic parameter like `record`'s `W` triggers because nothing reborrows implicitly there. Returning `&'a mut [Player]` from `&mut self` is impossible for the same reason: the longest reborrow you can make of `*self.players` is the lifetime of `&mut self`. Taking `self` by value owns the fields, so `finish(self)` can move `self.players` out with its full `'a`. The test that uses the players after the `Round` is gone checks exactly that.

`transfer` never needs two `&mut` at once: the scores are `Copy`, so compute `moved`, then update `from` and `to` in separate statements, which also makes `from == to` harmless. `split_at_mut` would need a special case for it. `rename` shows the disjoint case that does compile: `p` borrows `*self.players` while `&mut *self.log` borrows another field.""", "O(1) per call", "O(1)"),
    "`fn finish(&mut self) -> &mut [Player]` also compiles. Which caller code would it reject that `finish(self)` accepts?",
    ["Through `&mut self`, a `&mut` field can be reborrowed (`&mut *self.f`) but not moved out.", "Only a by-value `self` can hand a field back with its full lifetime.", "Sequence `Copy` reads and writes instead of holding two `&mut`."],
    rules=dict(methods=["clone", "swap", "take", "split_at_mut", "get_disjoint_mut"]),
    wrong=dict(
        uncapped=sub(sub(ROUND_SOLUTION, "let moved = points.min(self.players[from].score);", "let moved = points;"), "self.players[from].score -= moved;", "self.players[from].score = self.players[from].score.saturating_sub(moved);"),
        logs_to_before_from=sub(ROUND_SOLUTION, "let (a, b) = (&self.players[from], &self.players[to]);", "let (a, b) = (&self.players[to], &self.players[from]);"),
        finish_forgets_to_log=sub(ROUND_SOLUTION, "        record(self.log, \"round over\");\n        self.players", "        self.players"),
    ),
))

SINK_HEAD = r"""
pub trait Sink {
    fn put(&mut self, x: i32);
    /// How many values this sink has taken.
    fn len(&self) -> usize;
}

impl Sink for Vec<i32> {
    fn put(&mut self, x: i32) {
        self.push(x);
    }

    fn len(&self) -> usize {
        Vec::len(self)
    }
}

/// Counts values without keeping them.
pub struct Count(pub usize);

impl Sink for Count {
    fn put(&mut self, _: i32) {
        self.0 += 1;
    }

    fn len(&self) -> usize {
        self.0
    }
}

/// Sends every value to both sinks, `A` first.
pub struct Tee<A, B>(pub A, pub B);

impl<A: Sink, B: Sink> Sink for Tee<A, B> {
    fn put(&mut self, x: i32) {
        self.0.put(x);
        self.1.put(x);
    }

    fn len(&self) -> usize {
        self.0.len()
    }
}

/// Emits every value into `sink`. Like std's generic sinks, it takes the sink by value.
pub fn emit_all<S: Sink>(mut sink: S, xs: &[i32]) {
    for &x in xs {
        sink.put(x);
    }
}
"""

SINK_IMPLS = r"""
impl<S: Sink + ?Sized> Sink for &mut S {
    fn put(&mut self, x: i32) {
        (**self).put(x);
    }

    fn len(&self) -> usize {
        (**self).len()
    }
}

impl<S: Sink + ?Sized> Sink for Box<S> {
    fn put(&mut self, x: i32) {
        (**self).put(x);
    }

    fn len(&self) -> usize {
        (**self).len()
    }
}
"""

SINK_TAIL_STARTER = r"""
/// Emits `xs` into `sink` twice, then into `sink` and `count` together through a `Tee`. Returns how many values
/// `sink` holds afterwards.
pub fn pipeline(sink: &mut Vec<i32>, count: &mut Count, xs: &[i32]) -> usize {
    emit_all(sink, xs);
    emit_all(sink, xs);
    emit_all(Tee(sink, count), xs);
    sink.len()
}

/// Emits `xs` into every sink and returns their lengths afterwards.
pub fn fan_out(sinks: &mut [Box<dyn Sink>], xs: &[i32]) -> Vec<usize> {
    for s in sinks.iter_mut() {
        emit_all(s, xs);
    }
    sinks.iter().map(|s| s.len()).collect()
}
"""

SINK_TAIL_SOLUTION = sub(SINK_TAIL_STARTER, "    emit_all(sink, xs);\n    emit_all(sink, xs);\n    emit_all(Tee(sink, count), xs);", "    emit_all(&mut *sink, xs);\n    emit_all(&mut *sink, xs);\n    emit_all(Tee(&mut *sink, count), xs);")
SINK_SOLUTION = SINK_HEAD + SINK_IMPLS + SINK_TAIL_SOLUTION

P.append(fix(
    "explicit-reborrow-generic-sink", "Lend a sink: impl Sink for &mut S", "medium", "reborrows", ["reborrow", "blanket impls", "?Sized", "Box<dyn Trait>"],
    """
        `emit_all` takes its sink by value, the way std's generic sinks and iterators do. `pipeline` and
        `fan_out` want to lend theirs instead, and don't compile. Make `&mut S` and `Box<S>` sinks for every
        sink `S`, trait objects included, the way std does for `io::Write` and `Iterator`. Then fix `pipeline`
        so it can still use `sink` after lending it.
    """,
    SINK_HEAD + "\n// TODO: Sink for &mut S and Box<S>.\n" + SINK_TAIL_STARTER,
    SINK_SOLUTION,
    [T("pipeline_example", "sink [], count 0, xs [7, 8]", "{ let (mut v, mut c) = (Vec::new(), Count(0)); let n = pipeline(&mut v, &mut c, &[7, 8]); (n, v, c.0) }", "(6, vec![7, 8, 7, 8, 7, 8], 2)"),
     T("fan_out_example", "sinks [Vec [1], Count(5)], xs [2, 3]", "{ let mut s: Vec<Box<dyn Sink>> = vec![Box::new(vec![1]), Box::new(Count(5))]; fan_out(&mut s, &[2, 3]) }", "vec![3, 7]"),
     T("lend_a_trait_object", "emit_all into a &mut dyn Sink, then read the Vec", "{ let mut v = vec![0]; { let d: &mut dyn Sink = &mut v; emit_all(d, &[4]); } v }", "vec![0, 4]"),
     T("box_is_a_sink", "emit_all into a Box<Count>", "{ let mut b = Box::new(Count(0)); emit_all(&mut b, &[1, 2, 3]); b.len() }", "3"),
     T("lend_twice", "emit_all(&mut v, [1]) twice", "{ let mut v = Vec::new(); emit_all(&mut v, &[1]); emit_all(&mut v, &[1]); v }", "vec![1, 1]")],
    [T("pipeline_empty", "xs []", "{ let (mut v, mut c) = (vec![9], Count(0)); (pipeline(&mut v, &mut c, &[]), c.0) }", "(1, 0)"),
     T("pipeline_keeps_existing", "sink [5], xs [1]", "{ let (mut v, mut c) = (vec![5], Count(1)); let n = pipeline(&mut v, &mut c, &[1]); (n, v, c.0) }", "(4, vec![5, 1, 1, 1], 2)"),
     T("fan_out_empty", "no sinks", "fan_out(&mut [], &[1])", "Vec::<usize>::new()"),
     T("nested_lending", "emit_all into &mut &mut Vec", "{ let mut v = Vec::new(); let mut r = &mut v; emit_all(&mut r, &[6]); emit_all(r, &[7]); v }", "vec![6, 7]"),
     T("box_dyn_by_value", "emit_all(Box<dyn Sink> by value) then nothing", "{ let b: Box<dyn Sink> = Box::new(Count(0)); let mut b = b; emit_all(&mut b, &[1]); emit_all(&mut b, &[2]); b.len() }", "2"),
     T("tee_of_lent_sinks", "Tee(&mut a, &mut b)", "{ let (mut a, mut b) = (Vec::new(), Count(0)); emit_all(Tee(&mut a, &mut b), &[1, 2]); (a, b.0) }", "(vec![1, 2], 2)"),
     T("tee_of_boxed", "Tee(Box<dyn Sink>, Count) through &mut", "{ let mut t = Tee(Box::new(vec![0]) as Box<dyn Sink>, Count(0)); emit_all(&mut t, &[3]); (t.0.len(), t.1.len()) }", "(2, 1)"),
     T("fan_out_repeated", "fan_out twice into [Count(0)]", "{ let mut s: Vec<Box<dyn Sink>> = vec![Box::new(Count(0))]; fan_out(&mut s, &[1, 1]); fan_out(&mut s, &[1]) }", "vec![3]"),
     r"""
     #[test]
     fn random_vs_model() {
         let mut rng = anneal_prelude::Rng::new(6216);
         for _ in 0..300 {
             let n = rng.below(6);
             let xs: Vec<i32> = rng.vec(n, -9, 9);
             let len = rng.below(3);
             let start: Vec<i32> = rng.vec(len, 0, 9);
             let c0 = rng.below(5);
             let mut v = start.clone();
             let mut c = Count(c0);
             let got = pipeline(&mut v, &mut c, &xs);
             let mut want = start.clone();
             for _ in 0..3 {
                 want.extend_from_slice(&xs);
             }
             check!(format!("sink {start:?}, count {c0}, xs {xs:?}"), (got, v, c.0), (want.len(), want, c0 + n));
         }
     }

     #[test]
     fn many_values() {
         let xs: Vec<i32> = (0..100_000).collect();
         let mut s: Vec<Box<dyn Sink>> = vec![Box::new(Vec::new()), Box::new(Count(0))];
         check!("fan_out 100000 values into [Vec, Count]", fan_out(&mut s, &xs), vec![100_000, 100_000]);
     }
     """],
    [("rust", "`impl<S: Sink + ?Sized> Sink for &mut S` forwards each method to the `S` behind the reference. Without `?Sized`, `S` can't be `dyn Sink`, so `&mut dyn Sink` wouldn't be a sink."),
     ("rust", "Inside that impl `self` is `&mut &mut S`. `self.put(x)` finds this very impl again and recurses forever; `(**self).put(x)` reaches the `S`."),
     ("rust", "With the impl in place, passing `sink` still moves the `&mut Vec<i32>`: the parameter is a generic `S`, so there's no implicit reborrow. `&mut *sink` lends a fresh one.")],
    ("""A function that takes a sink by value (`S: Sink`) can still be lent one, if `&mut S` is itself a sink: that's the blanket impl std writes for `io::Write`, `fmt::Write`, `Iterator`, `Hasher` and others. It forwards through `(**self)`, because `self.put(x)` on a `&mut &mut S` resolves to the same impl and recurses. `?Sized` lets `S` be `dyn Sink`, and a matching impl for `Box<S>` makes a `Box<dyn Sink>` a sink as well (so `&mut Box<dyn Sink>` is one too, which is what `fan_out` passes). Even with the impl, `emit_all(sink, xs)` moves `sink`, because implicit reborrows only happen where the parameter type is known to be `&mut`; `&mut *sink` lends it and leaves `sink` usable.

Syntax to remember: `impl<S: Sink + ?Sized> Sink for &mut S { fn put(&mut self, x: i32) { (**self).put(x) } }` · the same for `Box<S>` · `emit_all(&mut *sink, xs)`.""", "O(n)", "O(1)"),
    "Why does std implement `Iterator` for `&mut I` but not for `&I`?",
    ["A blanket `impl Trait for &mut T` lets by-value generic APIs borrow instead of consume.", "`(**self).method()` reaches the value behind `&mut &mut T`.", "`?Sized` admits trait objects."],
    related=("L2", "L4"),
    wrong=dict(
        box_counts_nothing=sub(SINK_SOLUTION, "impl<S: Sink + ?Sized> Sink for Box<S> {\n    fn put(&mut self, x: i32) {\n        (**self).put(x);\n    }\n\n    fn len(&self) -> usize {\n        (**self).len()\n    }",
                               "impl<S: Sink + ?Sized> Sink for Box<S> {\n    fn put(&mut self, x: i32) {\n        (**self).put(x);\n    }\n\n    fn len(&self) -> usize {\n        0\n    }"),
        tee_gets_a_copy=sub(SINK_SOLUTION, "emit_all(Tee(&mut *sink, count), xs);", "emit_all(Tee(Vec::new(), count), xs);"),
        mut_ref_puts_twice=sub(SINK_SOLUTION, "impl<S: Sink + ?Sized> Sink for &mut S {\n    fn put(&mut self, x: i32) {\n        (**self).put(x);\n    }",
                               "impl<S: Sink + ?Sized> Sink for &mut S {\n    fn put(&mut self, x: i32) {\n        if x != 0 {\n            (**self).put(x);\n        }\n    }"),
    ),
))

STEP_HEAD = r"""
/// Every `step`-th element of a slice, starting with the first, handed out as `&mut`.
pub struct StepMut<'a, T> {
    rest: &'a mut [T],
    step: usize,
}

/// `v[0], v[step], v[2 * step], ...` as `&mut`. Panics if `step` is 0.
pub fn step_mut<T>(v: &mut [T], step: usize) -> StepMut<'_, T> {
    assert!(step > 0, "step must be positive");
    StepMut { rest: v, step }
}
"""

STEP_STARTER = STEP_HEAD + r"""
impl<'a, T> Iterator for StepMut<'a, T> {
    type Item = &'a mut T;

    fn next(&mut self) -> Option<&'a mut T> {
        todo!()
    }

    /// Exact: how many elements are left.
    fn size_hint(&self) -> (usize, Option<usize>) {
        todo!()
    }
}

impl<T> DoubleEndedIterator for StepMut<'_, T> {
    fn next_back(&mut self) -> Option<Self::Item> {
        todo!()
    }
}

impl<T> ExactSizeIterator for StepMut<'_, T> {}
"""

STEP_SOLUTION = STEP_HEAD + r"""
impl<'a, T> Iterator for StepMut<'a, T> {
    type Item = &'a mut T;

    fn next(&mut self) -> Option<&'a mut T> {
        let rest = std::mem::take(&mut self.rest);
        let (first, tail) = rest.split_first_mut()?;
        let skip = (self.step - 1).min(tail.len());
        self.rest = &mut tail[skip..];
        Some(first)
    }

    /// Exact: how many elements are left.
    fn size_hint(&self) -> (usize, Option<usize>) {
        let n = self.rest.len().div_ceil(self.step);
        (n, Some(n))
    }
}

impl<T> DoubleEndedIterator for StepMut<'_, T> {
    fn next_back(&mut self) -> Option<Self::Item> {
        let rest = std::mem::take(&mut self.rest);
        if rest.is_empty() {
            return None;
        }
        let last = (rest.len() - 1) / self.step * self.step;
        let (init, tail) = rest.split_at_mut(last);
        self.rest = init;
        tail.first_mut()
    }
}

impl<T> ExactSizeIterator for StepMut<'_, T> {}
"""


P.append(write(
    "stride-iter-mut", "An iterator that hands out &mut", "medium", "reborrows", ["IterMut", "mem::take", "split_first_mut", "reborrow lifetimes"],
    """
        Implement `next`, `size_hint` and `next_back` for `StepMut`, which yields every `step`-th element of a
        slice as `&mut`. The items must be able to live together (the tests collect them all and then write
        through each), and there's no `unsafe`.
    """,
    STEP_STARTER,
    STEP_SOLUTION,
    [T("every_other", "[1, 2, 3, 4, 5], step 2; add 10 to each", "{ let mut v = [1, 2, 3, 4, 5]; for x in step_mut(&mut v, 2) { *x += 10; } v }", "[11, 2, 13, 4, 15]"),
     T("items_live_together", "[1, 2, 3, 4], step 3; collect, then write through each", "{ let mut v = [1, 2, 3, 4]; let refs: Vec<&mut i32> = step_mut(&mut v, 3).collect(); for r in refs { *r = 0; } v }", "[0, 2, 3, 0]"),
     T("len_is_exact", "len of step_mut on 7 elements, steps 1, 2, 3, 7, 8", "{ let mut v = [0; 7]; (step_mut(&mut v, 1).len(), step_mut(&mut v, 2).len(), step_mut(&mut v, 3).len(), step_mut(&mut v, 7).len(), step_mut(&mut v, 8).len()) }", "(7, 4, 3, 1, 1)"),
     T("reversed", "[0, 1, 2, 3, 4, 5, 6], step 3, reversed", "{ let mut v = [0, 1, 2, 3, 4, 5, 6]; step_mut(&mut v, 3).rev().map(|x| *x).collect::<Vec<_>>() }", "vec![6, 3, 0]"),
     T("back_when_unaligned", "[0, 1, 2, 3, 4, 5], step 4: next_back", "{ let mut v = [0, 1, 2, 3, 4, 5]; step_mut(&mut v, 4).next_back().copied() }", "Some(4)"),
     T("empty", "[], step 2", "{ let mut v: [i32; 0] = []; let mut it = step_mut(&mut v, 2); (it.len(), it.next().is_none(), it.next_back().is_none()) }", "(0, true, true)")],
    [T("step_one", "[1, 2, 3], step 1", "{ let mut v = [1, 2, 3]; step_mut(&mut v, 1).map(|x| *x).collect::<Vec<_>>() }", "vec![1, 2, 3]"),
     T("step_larger_than_len", "[5, 6], step 10", "{ let mut v = [5, 6]; step_mut(&mut v, 10).map(|x| *x).collect::<Vec<_>>() }", "vec![5]"),
     T("both_ends_meet", "[0..9], step 2: next, next_back, next, next_back, next, next", "{ let mut v = [0, 1, 2, 3, 4, 5, 6, 7, 8]; let mut it = step_mut(&mut v, 2); let a = [it.next().copied(), it.next_back().copied(), it.next().copied(), it.next_back().copied(), it.next().copied(), it.next().copied()]; a }", "[Some(0), Some(8), Some(2), Some(6), Some(4), None]"),
     T("len_after_next_back", "[0..10], step 3: next_back then len", "{ let mut v = [0; 10]; let mut it = step_mut(&mut v, 3); it.next_back(); it.len() }", "3"),
     T("len_after_next", "[0..10], step 3: next then len", "{ let mut v = [0; 10]; let mut it = step_mut(&mut v, 3); it.next(); it.len() }", "3"),
     T("strings", "[\"a\", \"b\", \"c\"], step 2: push '!'", '{ let mut v = ["a", "b", "c"].map(String::from); for s in step_mut(&mut v, 2) { s.push(\'!\'); } v }', '["a!", "b", "c!"].map(String::from)'),
     T("single", "[9], step 1: next_back, then next", "{ let mut v = [9]; let mut it = step_mut(&mut v, 1); (it.next_back().copied(), it.next().is_none()) }", "(Some(9), true)"),
     T("zip_two_iterators", "swap v[0,2,4] with v[1,3,5] via split_at_mut halves", "{ let mut v = [1, 2, 3, 4, 5, 6]; let (a, b) = v.split_at_mut(3); for (x, y) in step_mut(a, 1).zip(step_mut(b, 1)) { std::mem::swap(x, y); } v }", "[4, 5, 6, 1, 2, 3]"),
     r"""
     #[test]
     fn step_zero_panics() {
         let mut v = [1, 2];
         let r = std::panic::catch_unwind(move || step_mut(&mut v, 0).count());
         check!("step_mut(.., 0) panics", r.is_err(), true);
     }

     #[test]
     fn random_vs_model() {
         let mut rng = anneal_prelude::Rng::new(6217);
         for _ in 0..400 {
             let n = rng.below(12);
             let step = 1 + rng.below(5);
             let v: Vec<i32> = (0..n as i32).collect();
             let mut left: Vec<i32> = v.iter().copied().step_by(step).collect();
             let mut got_v = v.clone();
             let mut it = step_mut(&mut got_v, step);
             let mut ops = Vec::new();
             for _ in 0..6 {
                 let back = rng.bool();
                 let want = if back { left.pop() } else if left.is_empty() { None } else { Some(left.remove(0)) };
                 let got = if back { it.next_back() } else { it.next() };
                 ops.push(if back { "next_back" } else { "next" });
                 let got = got.map(|x| {
                     let seen = *x;
                     *x = -1;
                     seen
                 });
                 check!(format!("0..{n}, step {step}: {}", ops.join(", ")), (got, it.len()), (want, left.len()));
             }
         }
     }

     #[test]
     fn big_slice() {
         let mut v: Vec<u32> = (0..1_000_000).collect();
         for x in step_mut(&mut v, 1000) {
             *x = 7;
         }
         let n = step_mut(&mut v, 3).len();
         check!("1000000 elements: step 1000 writes, then len at step 3", (n, v[999_000], v[999_001]), (333_334, 7, 999_001));
     }
     """],
    [("rust", "`self.rest.split_first_mut()` in `next` reborrows through `&mut self`, so the pieces live only as long as that call's borrow of `self`, not `'a` (\"lifetime may not live long enough\"). You need to move the `&'a mut [T]` out of `self`, not borrow it."),
     ("rust", "`&mut [T]` implements `Default` (the empty slice), so `std::mem::take(&mut self.rest)` moves the full-lifetime slice out and leaves an empty one. Split what you took, keep one piece, and put the rest back."),
     ("rust", "From the back, the last element this iterator would yield is at `(len - 1) / step * step`, not necessarily `len - 1`.")],
    ("""`next(&mut self) -> Option<&'a mut T>` promises items that outlive the `&mut self` borrow. Reborrowing `self.rest` can't deliver that: a reborrow of `*self.rest` through `&mut self` is limited to that borrow, or two calls to `next` could return aliasing `&mut`s. Moving the `&'a mut [T]` out of `self` is allowed: `mem::take` swaps in the empty slice and returns the original with its full `'a`. Split it into the item and the remainder (`split_first_mut`, or `split_at_mut` from the back), then store the remainder back. Each item is carved off and never reachable from the iterator again, which is why `collect()`ing them all is sound. std's `slice::IterMut` does the same with raw pointers.

Syntax to remember: `let rest = std::mem::take(&mut self.rest);` · `let (first, tail) = rest.split_first_mut()?;` · `self.rest = &mut tail[skip..];` · `len.div_ceil(step)`.""", "O(1) per item", "O(1)"),
    "Why can't you write a `windows_mut` iterator in this style, when `chunks_mut` works?",
    ["A reborrow through `&mut self` can't be returned with the outer lifetime `'a`.", "`mem::take` moves a `&'a mut [T]` out of a field (the empty slice is its default).", "Split, keep one piece, store the rest back."],
    related=("L2", "L3"),
    wrong=dict(
        skips_one_too_many=sub(STEP_SOLUTION, "let skip = (self.step - 1).min(tail.len());", "let skip = self.step.min(tail.len());"),
        size_hint_rounds_down=sub(STEP_SOLUTION, "let n = self.rest.len().div_ceil(self.step);", "let n = self.rest.len() / self.step;"),
        back_from_the_very_end=sub(STEP_SOLUTION, "let last = (rest.len() - 1) / self.step * self.step;", "let last = rest.len() - 1;"),
    ),
))

# ---------------------------------------------------------------- iterator invalidation (medium)

EVENT_HEAD = r"""
#[derive(Debug, PartialEq)]
pub struct Event {
    pub key: String,
    pub count: u32,
}

/// Merges each run of adjacent events with the same key into the run's first event, adding up the counts.
/// Returns how many events were removed.
"""

MERGE_STARTER = EVENT_HEAD + r"""pub fn merge_runs(events: &mut Vec<Event>) -> usize {
    let mut removed = 0;
    for (i, e) in events.iter().enumerate() {
        if i > 0 && e.key == events[i - 1].key {
            events[i - 1].count += e.count;
            events.remove(i);
            removed += 1;
        }
    }
    removed
}
"""

MERGE_SOLUTION = EVENT_HEAD + r"""pub fn merge_runs(events: &mut Vec<Event>) -> usize {
    let before = events.len();
    events.dedup_by(|cur, prev| {
        if cur.key == prev.key {
            prev.count += cur.count;
            true
        } else {
            false
        }
    });
    before - events.len()
}
"""

MERGE_HELPER = r"""
fn evs(xs: &[(&str, u32)]) -> Vec<Event> {
    xs.iter().map(|&(k, c)| Event { key: k.to_string(), count: c }).collect()
}

fn pairs(v: &[Event]) -> Vec<(&str, u32)> {
    v.iter().map(|e| (e.key.as_str(), e.count)).collect()
}
"""


def merge_py(xs):
    out = []
    for k, c in xs:
        if out and out[-1][0] == k:
            out[-1] = (k, out[-1][1] + c)
        else:
            out.append((k, c))
    return out


def merge_case(name, xs):
    out = merge_py(xs)
    lit = "&[" + ", ".join(f'("{k}", {c})' for k, c in xs) + "]"
    want = "vec![" + ", ".join(f'("{k}", {c})' for k, c in out) + "]" if out else "Vec::<(&str, u32)>::new()"
    return T(name, f"events {[(k, c) for k, c in xs]}".replace("'", '"'), "(n, pairs(&v))", f"({len(xs) - len(out)}, {want})",
             setup=f"let mut v = evs({lit});\nlet n = merge_runs(&mut v);")


P.append(fix(
    "fix-remove-while-iterating", "Fix: remove while iterating, merging into the survivor", "medium", "iterator-invalidation", ["E0502", "dedup_by", "O(n) removal"],
    """
        `merge_runs` doesn't compile: it removes from the `Vec` it's iterating and writes to the element before.
        An index loop with `remove` would compile but skip elements and cost O(n²) on long logs. Fix it with one
        pass over the `Vec`, without `remove` and without cloning any event.
    """,
    MERGE_STARTER,
    MERGE_SOLUTION,
    [MERGE_HELPER,
     merge_case("example", [("a", 1), ("a", 2), ("b", 5), ("a", 1)]),
     merge_case("empty", []),
     merge_case("long_run", [("x", 1), ("x", 1), ("x", 1), ("x", 1)]),
     merge_case("not_adjacent_not_merged", [("a", 1), ("b", 1), ("a", 1)]),
     merge_case("two_runs", [("a", 3), ("a", 4), ("b", 1), ("b", 1), ("b", 1)])],
    [MERGE_HELPER,
     merge_case("single", [("k", 9)]),
     merge_case("all_different", [("a", 1), ("b", 2), ("c", 3)]),
     merge_case("case_sensitive", [("a", 1), ("A", 1), ("a", 1)]),
     merge_case("zero_counts", [("z", 0), ("z", 0), ("y", 0)]),
     merge_case("empty_keys", [("", 1), ("", 2), ("e", 3), ("", 4)]),
     merge_case("run_at_end", [("a", 1), ("b", 1), ("b", 2), ("b", 3)]),
     merge_case("unicode", [("日", 1), ("日", 1), ("é", 2)]),
     T("big_counts", "events [(\"m\", 4000000000), (\"m\", 294967295)]", 'pairs(&v)', 'vec![("m", u32::MAX)]', setup='let mut v = evs(&[("m", 4_000_000_000), ("m", 294_967_295)]);\nmerge_runs(&mut v);'),
     T("survivor_is_the_first", "the merged event is the run's first String", "p == v[0].key.as_ptr()", "true",
       setup='let mut v = evs(&[("run", 1), ("run", 2)]);\nlet p = v[0].key.as_ptr();\nmerge_runs(&mut v);'),
     r"""
     #[test]
     fn random_vs_brute_force() {
         let mut rng = anneal_prelude::Rng::new(6218);
         for _ in 0..300 {
             let n = rng.below(10);
             let mut xs: Vec<(String, u32)> = Vec::new();
             for _ in 0..n {
                 xs.push((rng.string(1, "ab"), rng.below(5) as u32));
             }
             let mut want: Vec<(String, u32)> = Vec::new();
             for (k, c) in &xs {
                 match want.last_mut() {
                     Some((lk, lc)) if lk == k => *lc += c,
                     _ => want.push((k.clone(), *c)),
                 }
             }
             let mut v: Vec<Event> = xs.iter().map(|(k, c)| Event { key: k.clone(), count: *c }).collect();
             let removed = merge_runs(&mut v);
             let got: Vec<(String, u32)> = v.into_iter().map(|e| (e.key, e.count)).collect();
             check!(format!("events {xs:?}"), (removed, got), (n - want.len(), want));
         }
     }

     #[test]
     fn long_log() {
         let mut v: Vec<Event> = (0..200_000).map(|i| Event { key: format!("k{}", i / 2), count: 1 }).collect();
         let removed = merge_runs(&mut v);
         check!("200000 events in runs of 2", (removed, v.len(), v[99_999].count), (100_000, 100_000, 2));
     }
     """],
    [("rust", "`for (i, e) in events.iter()` holds a shared borrow of the whole `Vec` for the loop, so neither `events[i - 1].count += ..` nor `events.remove(i)` can happen inside it. And `remove` shifts the tail on every call."),
     ("rust", "`Vec::dedup_by(|a, b| ..)` removes consecutive elements the closure accepts, in one pass. It gives you `&mut` to both: `a` is the element being considered, `b` the one before it that was kept. The order of the arguments matters here.")],
    ("""Removing while iterating is what the borrow checker forbids and what makes index loops wrong: after `remove(i)` the next element slides into `i` and gets skipped, and every removal shifts the tail, O(n²) in all. std's in-place filters do one pass with a read and a write cursor: `retain` for a per-element test, and `dedup_by` when the decision involves the previous survivor. `dedup_by` passes `&mut` to both elements, so it can merge before discarding: the closure gets `(current, previous_kept)`, and merging into the second argument keeps the counts. Merging into the first adds them to the event that's about to be dropped.

Syntax to remember: `v.dedup_by(|cur, prev| { if cur.key == prev.key { prev.count += cur.count; true } else { false } })` · `v.dedup_by_key(|e| e.key.clone())` (no merging).""", "O(n)", "O(1)"),
    "How would you merge runs in a `VecDeque` or a `HashMap` by insertion order, where there's no `dedup_by`?",
    ["Removing inside an iteration is rejected; index loops skip elements and cost O(n²).", "`dedup_by` gives `&mut` to the current element and the previous survivor, in that order."],
    rules=dict(methods=["remove", "clone", "swap_remove", "to_owned"]),
    wrong=dict(
        merges_into_the_dropped=sub(MERGE_SOLUTION, "prev.count += cur.count;", "cur.count += prev.count;"),
        dedup_by_key_loses_counts=sub(MERGE_SOLUTION, "    events.dedup_by(|cur, prev| {\n        if cur.key == prev.key {\n            prev.count += cur.count;\n            true\n        } else {\n            false\n        }\n    });", "    events.dedup_by(|cur, prev| cur.key == prev.key);"),
        sorts_first=sub(MERGE_SOLUTION, "    let before = events.len();\n", "    let before = events.len();\n    events.sort_by(|a, b| a.key.cmp(&b.key));\n"),
    ),
))

POOL_HEAD = r"""
#[derive(Debug, PartialEq)]
pub struct Conn {
    pub id: u32,
    pub idle: u64,
    pub tokens: u32,
}

pub struct Pool {
    pub conns: Vec<Conn>,
    pub max_idle: u64,
    pub refill: u32,
    pub cap: u32,
    closed: Vec<u32>,
}
"""

POOL_GIVEN = r"""
    pub fn new(conns: Vec<Conn>, max_idle: u64, refill: u32, cap: u32) -> Self {
        Pool { conns, max_idle, refill, cap, closed: Vec::new() }
    }

    fn is_stale(&self, c: &Conn) -> bool {
        c.idle > self.max_idle
    }

    /// How many connections are idle longer than `max_idle`.
    pub fn stale_count(&self) -> usize {
        self.conns.iter().filter(|c| self.is_stale(c)).count()
    }
"""

POOL_DOCS = dict(
    tick="""    /// Ages every connection by `dt`. A connection now idle longer than `max_idle` is closed: removed, with its
    /// id appended to the closed list. Every other connection gains `refill` tokens, up to `cap`. Keeps the
    /// order, runs in one pass, and returns how many it closed.
    pub fn tick(&mut self, dt: u64) -> usize {
""",
    use_conn="""    /// Spends one token of connection `id` and resets its idle time. `false` if there's no such connection
    /// or it has no tokens (then nothing changes).
    pub fn use_conn(&mut self, id: u32) -> bool {
""",
    drain="""    /// Hands over the ids closed so far, oldest first, and starts a new list.
    pub fn drain_closed(&mut self) -> Vec<u32> {
""",
)

POOL_BODIES = dict(
    tick="""        let before = self.conns.len();
        self.conns.retain_mut(|c| {
            c.idle += dt;
            if c.idle > self.max_idle {
                self.closed.push(c.id);
                return false;
            }
            c.tokens = c.tokens.saturating_add(self.refill).min(self.cap);
            true
        });
        before - self.conns.len()
""",
    use_conn="""        match self.conns.iter_mut().find(|c| c.id == id) {
            Some(c) if c.tokens > 0 => {
                c.tokens -= 1;
                c.idle = 0;
                true
            }
            _ => false,
        }
""",
    drain="""        std::mem::take(&mut self.closed)
""",
)


def pool_src(bodies):
    out = POOL_HEAD + "\nimpl Pool {" + POOL_GIVEN
    for k in ("tick", "use_conn", "drain"):
        out += "\n" + POOL_DOCS[k] + bodies[k] + "    }\n"
    return out + "}\n"


POOL_SOLUTION = pool_src(POOL_BODIES)
POOL_STARTER = pool_src({k: "        todo!()\n" for k in POOL_BODIES})
POOL_NEW = "let mut p = Pool::new(vec![Conn { id: 1, idle: 0, tokens: 0 }, Conn { id: 2, idle: 8, tokens: 3 }, Conn { id: 3, idle: 2, tokens: 9 }], 10, 2, 10);"
POOL_DESC = "conns 1 (idle 0, 0 tokens), 2 (idle 8, 3), 3 (idle 2, 9); max_idle 10, refill 2, cap 10"


def pool_state(expr="p"):
    return f"{expr}.conns.iter().map(|c| (c.id, c.idle, c.tokens)).collect::<Vec<_>>()"


P.append(write(
    "retain-mut", "retain_mut with the rest of self", "medium", "iterator-invalidation", ["retain_mut", "disjoint closure captures", "mem::take", "iter_mut().find"],
    """
        Write `tick`, `use_conn` and `drain_closed` for a connection pool. `tick` updates some connections and
        removes others in a single pass, recording what it closed in another field of the pool. A test
        ticks a large pool, so removing connections one at a time is too slow.
    """,
    POOL_STARTER,
    POOL_SOLUTION,
    [T("tick_example", POOL_DESC + "; tick 3", "(closed, " + pool_state() + ", p.drain_closed())", "(1, vec![(1, 3, 2), (3, 5, 10)], vec![2])", setup=POOL_NEW + "\nlet closed = p.tick(3);"),
     T("use_conn", POOL_DESC + "; use 2, use 1, use 9", "(p.use_conn(2), p.use_conn(1), p.use_conn(9), " + pool_state() + ")", "(true, false, false, vec![(1, 0, 0), (2, 0, 2), (3, 2, 9)])", setup=POOL_NEW),
     T("idle_at_limit_stays", POOL_DESC + "; tick 2", "(p.tick(2), " + pool_state() + ")", "(0, vec![(1, 2, 2), (2, 10, 5), (3, 4, 10)])", setup=POOL_NEW),
     T("drain_starts_over", POOL_DESC + "; tick 100; drain twice", "(p.drain_closed(), p.drain_closed(), p.conns.len())", "(vec![1, 2, 3], vec![], 0)", setup=POOL_NEW + "\np.tick(100);"),
     T("use_resets_idle", POOL_DESC + "; tick 1; use 2; tick 2", "(p.tick(2), " + pool_state() + ")", "(0, vec![(1, 3, 4), (2, 2, 6), (3, 5, 10)])", setup=POOL_NEW + "\np.tick(1);\np.use_conn(2);"),
     T("stale_count_given", "stale_count reads what tick leaves", "p.stale_count()", "0", setup=POOL_NEW + "\np.tick(3);")],
    [T("empty_pool", "no connections; tick 5", "(p.tick(5), p.drain_closed())", "(0, vec![])", setup="let mut p = Pool::new(vec![], 1, 1, 1);"),
     T("tick_zero", POOL_DESC + "; tick 0", "(p.tick(0), " + pool_state() + ")", "(0, vec![(1, 0, 2), (2, 8, 5), (3, 2, 10)])", setup=POOL_NEW),
     T("closed_in_order_across_ticks", POOL_DESC + "; tick 3, tick 6", "{ p.tick(3); p.tick(6); p.drain_closed() }", "vec![2, 3]", setup=POOL_NEW),
     T("refill_saturates", "tokens u32::MAX - 1, refill 5, cap u32::MAX", "{ p.tick(1); p.conns[0].tokens }", "u32::MAX", setup="let mut p = Pool::new(vec![Conn { id: 7, idle: 0, tokens: u32::MAX - 1 }], 10, 5, u32::MAX);"),
     T("cap_below_tokens", "tokens 9, cap 4", "{ p.tick(1); p.conns[0].tokens }", "4", setup="let mut p = Pool::new(vec![Conn { id: 7, idle: 0, tokens: 9 }], 10, 0, 4);"),
     T("use_until_empty", "tokens 2; use 3 times", "(p.use_conn(5), p.use_conn(5), p.use_conn(5), p.conns[0].tokens)", "(true, true, false, 0)", setup="let mut p = Pool::new(vec![Conn { id: 5, idle: 4, tokens: 2 }], 10, 0, 9);"),
     T("failed_use_keeps_idle", "tokens 0, idle 4; use", "(p.use_conn(5), p.conns[0].idle)", "(false, 4)", setup="let mut p = Pool::new(vec![Conn { id: 5, idle: 4, tokens: 0 }], 10, 0, 9);"),
     T("duplicate_ids_first_used", "two conns with id 5", "(p.use_conn(5), p.conns[0].tokens, p.conns[1].tokens)", "(true, 0, 1)", setup="let mut p = Pool::new(vec![Conn { id: 5, idle: 0, tokens: 1 }, Conn { id: 5, idle: 0, tokens: 1 }], 10, 0, 9);"),
     T("max_idle_zero", "max_idle 0; tick 1 closes all", "(p.tick(1), p.drain_closed())", "(2, vec![1, 2])", setup="let mut p = Pool::new(vec![Conn { id: 1, idle: 0, tokens: 0 }, Conn { id: 2, idle: 0, tokens: 0 }], 0, 1, 1);"),
     r"""
     #[test]
     fn random_vs_model() {
         let mut rng = anneal_prelude::Rng::new(6219);
         for _ in 0..300 {
             let n = rng.below(6);
             let start: Vec<(u32, u64, u32)> = (0..n).map(|i| (i as u32, rng.below(6) as u64, rng.below(5) as u32)).collect();
             let (max_idle, refill, cap) = (rng.below(8) as u64, rng.below(3) as u32, rng.below(6) as u32);
             let mut p = Pool::new(start.iter().map(|&(id, idle, tokens)| Conn { id, idle, tokens }).collect(), max_idle, refill, cap);
             let mut model = start.clone();
             let mut closed = Vec::new();
             let mut ops = Vec::new();
             for _ in 0..5 {
                 if rng.bool() {
                     let dt = rng.below(4) as u64;
                     let before = model.len();
                     let mut kept = Vec::new();
                     for (id, idle, tokens) in model {
                         if idle + dt > max_idle {
                             closed.push(id);
                         } else {
                             kept.push((id, idle + dt, (tokens + refill).min(cap)));
                         }
                     }
                     model = kept;
                     ops.push(format!("tick {dt}"));
                     check!(format!("{start:?}, max_idle {max_idle}, refill {refill}, cap {cap}; {}", ops.join(", ")), p.tick(dt), before - model.len());
                 } else {
                     let id = rng.below(n + 1) as u32;
                     let want = match model.iter_mut().find(|c| c.0 == id) {
                         Some(c) if c.2 > 0 => {
                             c.2 -= 1;
                             c.1 = 0;
                             true
                         }
                         _ => false,
                     };
                     ops.push(format!("use {id}"));
                     check!(format!("{start:?}, max_idle {max_idle}, refill {refill}, cap {cap}; {}", ops.join(", ")), p.use_conn(id), want);
                 }
             }
             let got: Vec<(u32, u64, u32)> = p.conns.iter().map(|c| (c.id, c.idle, c.tokens)).collect();
             check!(format!("{start:?}, max_idle {max_idle}, refill {refill}, cap {cap}; {}; state", ops.join(", ")), (got, p.drain_closed()), (model, closed));
         }
     }

     #[test]
     fn big_pool() {
         let conns: Vec<Conn> = (0..200_000).map(|i| Conn { id: i, idle: (i % 2) as u64 * 10, tokens: 0 }).collect();
         let mut p = Pool::new(conns, 5, 1, 3);
         let closed = p.tick(1);
         check!("200000 conns, every other one stale", (closed, p.conns.len(), p.conns[99_999].id, p.drain_closed()[99_999]), (100_000, 100_000, 199_998, 199_999));
     }
     """],
    [("rust", "`retain_mut(|c| ..)` gives the closure `&mut Conn`, so it can update a connection and decide whether to keep it in the same pass."),
     ("rust", "The closure can read `self.max_idle` and push to `self.closed` while `self.conns` is borrowed by `retain_mut`: a closure that names fields captures only those fields. Calling `self.is_stale(c)` would capture all of `self` and fail."),
     ("rust", "`std::mem::take(&mut self.closed)` hands the list over and leaves an empty one, with no copy.")],
    ("""`retain_mut` is the in-place filter that can also edit what it keeps: one pass, order kept, survivors shifted once. The closure needs other parts of `self` too, and that works because edition-2021 closures capture disjoint field paths: `self.max_idle`, `self.refill`, `self.cap` by shared reference and `self.closed` by unique reference, none overlapping `self.conns`. A helper method such as `is_stale(&self, ..)` would borrow all of `self`, conflicting with `retain_mut`'s `&mut self.conns` (E0502), so the condition is written on the fields. `drain_closed` is `mem::take`: move the `Vec` out of a `&mut` field and leave `Vec::new()`, which doesn't allocate.

Syntax to remember: `self.conns.retain_mut(|c| { ..; keep })` · `c.tokens.saturating_add(self.refill).min(self.cap)` · `std::mem::take(&mut self.closed)` · `match self.conns.iter_mut().find(|c| c.id == id) { Some(c) if c.tokens > 0 => .., _ => false }`.""", "O(n) per tick", "O(1) extra"),
    "Why does calling `self.is_stale(c)` inside the closure fail, when reading `self.max_idle` doesn't?",
    ["`retain_mut` edits and filters in one pass.", "Closures capture disjoint fields of `self`; method calls capture it all.", "`mem::take` moves a field out of `&mut self`."],
    wrong=dict(
        closes_at_the_limit=sub(POOL_SOLUTION, "if c.idle > self.max_idle {\n                self.closed.push", "if c.idle >= self.max_idle {\n                self.closed.push"),
        refill_uncapped=sub(POOL_SOLUTION, "c.tokens = c.tokens.saturating_add(self.refill).min(self.cap);", "c.tokens = c.tokens.saturating_add(self.refill);"),
        use_keeps_the_token=sub(POOL_SOLUTION, "                c.tokens -= 1;\n", ""),
        drain_keeps_the_list=sub(POOL_SOLUTION, "        std::mem::take(&mut self.closed)\n", "        self.closed.iter().copied().collect()\n"),
    ),
))

EXPAND_DOC = r"""
use std::collections::HashMap;

/// Expands `tasks` in place: for every task in the list, including ones added by this call, each of its
/// subtasks in `rules` that isn't in the list yet is appended. Returns how many tasks were added.
"""

EXPAND_STARTER = EXPAND_DOC + r"""pub fn expand(tasks: &mut Vec<String>, rules: &HashMap<String, Vec<String>>) -> usize {
    let before = tasks.len();
    for t in tasks.iter() {
        if let Some(subs) = rules.get(t) {
            for s in subs {
                if !tasks.contains(s) {
                    tasks.push(s.to_string());
                }
            }
        }
    }
    tasks.len() - before
}
"""

EXPAND_SOLUTION = EXPAND_DOC + r"""pub fn expand(tasks: &mut Vec<String>, rules: &HashMap<String, Vec<String>>) -> usize {
    let before = tasks.len();
    let mut i = 0;
    while i < tasks.len() {
        if let Some(subs) = rules.get(&tasks[i]) {
            for s in subs {
                if !tasks.contains(s) {
                    tasks.push(s.to_string());
                }
            }
        }
        i += 1;
    }
    tasks.len() - before
}
"""

EXPAND_HELPER = r"""
fn rules(xs: &[(&str, &[&str])]) -> std::collections::HashMap<String, Vec<String>> {
    xs.iter().map(|(k, v)| (k.to_string(), v.iter().map(|s| s.to_string()).collect())).collect()
}

fn run(tasks: &[&str], r: &[(&str, &[&str])]) -> (usize, Vec<String>) {
    let mut t: Vec<String> = tasks.iter().map(|s| s.to_string()).collect();
    let n = expand(&mut t, &rules(r));
    (n, t)
}
"""


def expand_py(tasks, rules):
    t = list(tasks)
    i = 0
    while i < len(t):
        for s in rules.get(t[i], []):
            if s not in t:
                t.append(s)
        i += 1
    return t


def expand_case(name, tasks, rules):
    out = expand_py(tasks, dict(rules))
    tl = "&[" + ", ".join(f'"{x}"' for x in tasks) + "]"
    rl = "&[" + ", ".join(f'("{k}", &[' + ", ".join(f'"{s}"' for s in v) + "][..])" for k, v in rules) + "]"
    want = "[" + ", ".join(f'"{x}"' for x in out) + f"].map(String::from).to_vec()" if out else "Vec::<String>::new()"
    desc = f"tasks {tasks}, rules {dict(rules)}".replace("'", '"')
    return T(name, desc, f"run({tl}, {rl})", f"({len(out) - len(tasks)}, {want})")


P.append(fix(
    "fix-push-while-iterating", "Fix: push to the Vec you iterate", "medium", "iterator-invalidation", ["E0502", "worklists", "index loops"],
    """
        `expand` doesn't compile: it pushes to `tasks` while iterating over it. Here that's the point, since
        added tasks must be expanded too. Fix it without a second collection of tasks.
    """,
    EXPAND_STARTER,
    EXPAND_SOLUTION,
    [EXPAND_HELPER,
     expand_case("example", ["build"], [("build", ["compile", "test"]), ("test", ["lint"])]),
     expand_case("nothing_to_expand", ["a", "b"], [("c", ["d"])]),
     expand_case("already_listed", ["a", "b"], [("a", ["b", "c"])]),
     expand_case("cycle", ["a"], [("a", ["b"]), ("b", ["a", "c"])]),
     expand_case("empty_tasks", [], [("a", ["b"])])],
    [EXPAND_HELPER,
     expand_case("self_rule", ["a"], [("a", ["a"])]),
     expand_case("deep_chain", ["a"], [("a", ["b"]), ("b", ["c"]), ("c", ["d"]), ("d", ["e"])]),
     expand_case("order_is_breadth_first", ["r"], [("r", ["a", "b"]), ("a", ["c"]), ("b", ["d"])]),
     expand_case("duplicate_subtasks", ["a"], [("a", ["x", "x", "y"])]),
     expand_case("shared_child", ["a", "b"], [("a", ["c"]), ("b", ["c"]), ("c", ["d"])]),
     expand_case("empty_rule", ["a"], [("a", [])]),
     expand_case("no_rules", ["a"], []),
     expand_case("unicode", ["日本"], [("日本", ["東京"]), ("東京", ["新宿"])]),
     r"""
     #[test]
     fn random_vs_brute_force() {
         let mut rng = anneal_prelude::Rng::new(6220);
         let names = ["a", "b", "c", "d", "e", "f"];
         for _ in 0..300 {
             let mut r: std::collections::HashMap<String, Vec<String>> = std::collections::HashMap::new();
             for n in names {
                 if rng.bool() {
                     let k = rng.below(3);
                     let subs: Vec<String> = (0..k).map(|_| rng.pick(&names).to_string()).collect();
                     r.insert(n.to_string(), subs);
                 }
             }
             let k = rng.below(3);
             let start: Vec<String> = (0..k).map(|_| rng.pick(&names).to_string()).collect();
             let mut want = start.clone();
             let mut i = 0;
             while i < want.len() {
                 for s in r.get(&want[i]).cloned().unwrap_or_default() {
                     if !want.contains(&s) {
                         want.push(s);
                     }
                 }
                 i += 1;
             }
             let mut got = start.clone();
             let n = expand(&mut got, &r);
             let mut shown: Vec<_> = r.iter().collect();
             shown.sort();
             check!(format!("tasks {start:?}, rules {shown:?}"), (n, got), (want.len() - start.len(), want));
         }
     }

     #[test]
     fn long_chain() {
         let r: std::collections::HashMap<String, Vec<String>> = (0..2000).map(|i| (format!("t{i}"), vec![format!("t{}", i + 1)])).collect();
         let mut tasks = vec!["t0".to_string()];
         let n = expand(&mut tasks, &r);
         check!("a chain t0 -> t1 -> ... -> t2000", (n, tasks.last().cloned()), (2000, Some("t2000".to_string())));
     }
     """],
    [("rust", "`for t in tasks.iter()` borrows `tasks` for the whole loop, so `tasks.push` can't happen inside it. An index doesn't borrow anything between uses."),
     ("rust", "`for i in 0..tasks.len()` evaluates `len()` once, so tasks added during the loop are never visited. The loop condition has to read the length on every turn."),
     ("rust", "`rules.get(&tasks[i])` borrows `tasks` only for the lookup; the `subs` it returns borrow `rules`, so pushing to `tasks` inside is fine.")],
    ("""An iterator over a `Vec` holds a borrow of it, because a push may reallocate and leave the iterator pointing at freed memory. When growing the list while walking it is the algorithm (a worklist, breadth-first), walk by index: `while i < tasks.len()` re-reads the length each turn, so appended tasks are visited, and each access `&tasks[i]` is a short borrow that ends before the push. Check where each borrow comes from: `subs` borrows `rules`, not `tasks`, which is what lets the push happen inside the inner loop.

The tempting `for i in 0..tasks.len()` compiles and is wrong: the range is computed once. `tasks.contains` is O(n) per check; for large lists a `HashSet` of indices or owned names would keep the whole thing linear, since a `HashSet<&str>` into `tasks` couldn't coexist with the pushes.""", "O(n · k · n) with contains", "O(1) extra"),
    "Why can't a `HashSet<&str>` of the names already in `tasks` stay alive while you push?",
    ["An iterator borrows its collection; an index doesn't.", "`while i < v.len()` sees growth; `for i in 0..v.len()` doesn't.", "Know which collection each borrow comes from."],
    rules=dict(methods=["clone", "collect", "to_vec", "extend", "drain"]),
    wrong=dict(
        range_computed_once=sub(EXPAND_SOLUTION, "    let mut i = 0;\n    while i < tasks.len() {", "    for i in 0..tasks.len() {").replace("        i += 1;\n", ""),
        skips_existing_check=sub(EXPAND_SOLUTION, "                if !tasks.contains(s) {\n                    tasks.push(s.to_string());\n                }", "                if tasks.last() != Some(s) {\n                    tasks.push(s.to_string());\n                }"),
    ),
))
assert "while" not in P[-1]["wrong"]["range_computed_once"]

LIFE_HEAD = r"""
/// Conway's Game of Life on a `w` × `h` grid. Cells outside the grid are dead.
pub struct Life {
    w: usize,
    h: usize,
    cells: Vec<bool>,
    next: Vec<bool>,
}

impl Life {
    pub fn new(w: usize, h: usize, alive: &[(usize, usize)]) -> Self {
        let mut cells = vec![false; w * h];
        for &(x, y) in alive {
            cells[y * w + x] = true;
        }
        Life { w, h, cells, next: vec![false; w * h] }
    }

    /// Live cells as (x, y), row by row.
    pub fn alive(&self) -> Vec<(usize, usize)> {
        (0..self.w * self.h).filter(|&i| self.cells[i]).map(|i| (i % self.w, i / self.w)).collect()
    }

    fn live_neighbors(&self, x: usize, y: usize) -> usize {
        let mut n = 0;
        for ny in y.saturating_sub(1)..=(y + 1).min(self.h - 1) {
            for nx in x.saturating_sub(1)..=(x + 1).min(self.w - 1) {
                if (nx, ny) != (x, y) && self.cells[ny * self.w + nx] {
                    n += 1;
                }
            }
        }
        n
    }

    /// Advances one generation: a live cell with 2 or 3 live neighbors lives on, a dead cell with exactly 3
    /// becomes alive, every other cell is dead. Every cell's fate depends on the old generation only. Must not
    /// allocate.
    pub fn step(&mut self) {
"""

LIFE_STARTER = LIFE_HEAD + "        todo!()\n    }\n}\n"
LIFE_SOLUTION = LIFE_HEAD + r"""        for y in 0..self.h {
            for x in 0..self.w {
                let n = self.live_neighbors(x, y);
                let i = y * self.w + x;
                self.next[i] = matches!((self.cells[i], n), (true, 2) | (_, 3));
            }
        }
        std::mem::swap(&mut self.cells, &mut self.next);
    }
}
"""


def life_case(name, w, h, alive, steps, want):
    return T(name, f"{w}x{h}, alive {alive}, {steps} steps",
             f"{{ let mut l = Life::new({w}, {h}, &{alive}); for _ in 0..{steps} {{ l.step(); }} l.alive() }}", f"vec!{want}" if want else "Vec::<(usize, usize)>::new()")


def life_py(w, h, alive, steps):
    cells = set(alive)
    for _ in range(steps):
        nxt = set()
        for y in range(h):
            for x in range(w):
                n = sum((nx, ny) in cells for ny in range(y - 1, y + 2) for nx in range(x - 1, x + 2) if (nx, ny) != (x, y) and 0 <= nx < w and 0 <= ny < h)
                if n == 3 or ((x, y) in cells and n == 2):
                    nxt.add((x, y))
        cells = nxt
    return sorted(cells, key=lambda c: (c[1], c[0]))


def life_auto(name, w, h, alive, steps):
    return life_case(name, w, h, str(list(alive)), steps, str(life_py(w, h, alive, steps)))


P.append(write(
    "collect-then-mutate", "Read one buffer, write the other", "medium", "iterator-invalidation", ["double buffering", "mem::swap", "disjoint fields", "allocation"],
    """
        Write `Life::step`. Every cell's next state depends on its neighbours' *old* states, so updating the grid
        in place is wrong, and `step` must not allocate either: a test counts allocations. `Life` already
        has a second buffer, `next`.
    """,
    LIFE_STARTER,
    LIFE_SOLUTION,
    [life_auto("blinker", 5, 5, [(1, 2), (2, 2), (3, 2)], 1),
     life_auto("blinker_twice", 5, 5, [(1, 2), (2, 2), (3, 2)], 2),
     life_auto("block_is_still", 4, 4, [(1, 1), (2, 1), (1, 2), (2, 2)], 3),
     life_auto("lonely_cell_dies", 3, 3, [(1, 1)], 1),
     life_auto("edges_are_dead", 3, 1, [(0, 0), (1, 0), (2, 0)], 1),
     life_auto("glider", 6, 6, [(1, 0), (2, 1), (0, 2), (1, 2), (2, 2)], 4)],
    [ALLOC_COUNTER,
     life_auto("empty", 4, 3, [], 2),
     life_auto("one_by_one", 1, 1, [(0, 0)], 1),
     life_auto("corner_birth", 2, 2, [(0, 0), (1, 0), (0, 1)], 1),
     life_auto("full_3x3", 3, 3, [(x, y) for y in range(3) for x in range(3)], 1),
     life_auto("glider_hits_wall", 5, 5, [(1, 0), (2, 1), (0, 2), (1, 2), (2, 2)], 12),
     life_auto("toad", 6, 6, [(2, 2), (3, 2), (4, 2), (1, 3), (2, 3), (3, 3)], 1),
     life_auto("wide_row", 7, 2, [(x, 0) for x in range(7)], 2),
     life_auto("tall", 1, 5, [(0, 1), (0, 2), (0, 3)], 1),
     r"""
     fn model(w: usize, h: usize, cells: &[bool]) -> Vec<bool> {
         let mut out = vec![false; w * h];
         for y in 0..h {
             for x in 0..w {
                 let mut n = 0;
                 for dy in -1i64..=1 {
                     for dx in -1i64..=1 {
                         let (nx, ny) = (x as i64 + dx, y as i64 + dy);
                         if (dx, dy) != (0, 0) && nx >= 0 && ny >= 0 && (nx as usize) < w && (ny as usize) < h && cells[ny as usize * w + nx as usize] {
                             n += 1;
                         }
                     }
                 }
                 out[y * w + x] = n == 3 || (cells[y * w + x] && n == 2);
             }
         }
         out
     }

     #[test]
     fn random_vs_brute_force() {
         let mut rng = anneal_prelude::Rng::new(6221);
         for _ in 0..200 {
             let (w, h) = (1 + rng.below(6), 1 + rng.below(6));
             let alive: Vec<(usize, usize)> = (0..w * h).filter(|_| rng.below(3) == 0).map(|i| (i % w, i / w)).collect();
             let mut cells = vec![false; w * h];
             for &(x, y) in &alive {
                 cells[y * w + x] = true;
             }
             let mut l = Life::new(w, h, &alive);
             for s in 1..=3 {
                 cells = model(w, h, &cells);
                 l.step();
                 let want: Vec<(usize, usize)> = (0..w * h).filter(|&i| cells[i]).map(|i| (i % w, i / w)).collect();
                 check!(format!("{w}x{h}, alive {alive:?}, {s} steps"), l.alive(), want);
             }
         }
     }

     #[test]
     fn step_does_not_allocate() {
         let alive: Vec<(usize, usize)> = (0..300).flat_map(|y| (0..300).filter(move |x| (x * 7 + y * 13) % 5 < 2).map(move |x| (x, y))).collect();
         let mut l = Life::new(300, 300, &alive);
         let (_, n) = allocs(|| {
             for _ in 0..4 {
                 l.step();
             }
         });
         check!("300x300 grid, 4 steps: allocations", n, 0);
         let mut cells = vec![false; 300 * 300];
         for &(x, y) in &alive {
             cells[y * 300 + x] = true;
         }
         for _ in 0..4 {
             cells = model(300, 300, &cells);
         }
         check!("300x300 grid, 4 steps: live cells", l.alive().len(), cells.iter().filter(|&&c| c).count());
     }
     """],
    [("rust", "Writing the new state into `self.cells` as you go means later cells see some new neighbours. Compute into `self.next`, reading only `self.cells`, then make `next` the current generation."),
     ("rust", "`self.next.iter_mut()` plus a call to `self.live_neighbors(..)` inside the loop won't compile: the method borrows all of `self`. Compute the count into a local first, then write one cell of `self.next` by index."),
     ("rust", "`self.cells = self.next.clone()` allocates every step. `std::mem::swap(&mut self.cells, &mut self.next)` exchanges the two buffers in O(1); the stale one gets overwritten next time.")],
    ("""Every cell's next state depends on the old generation, so the new one has to be written somewhere else: that's the read-then-write rule of iterator invalidation, applied to a whole grid. Keeping two buffers and swapping them avoids a snapshot allocation per step: `mem::swap` exchanges the `Vec` headers (pointer, length, capacity), not the cells. Borrowing matters inside the loop: `live_neighbors(&self, ..)` borrows all of `self`, so it can't run while an `iter_mut()` over `self.next` is alive; computing `n` first and then assigning `self.next[i]` keeps the borrows sequential. (An assignment evaluates its right-hand side before the place it writes to.)

Syntax to remember: `std::mem::swap(&mut self.cells, &mut self.next)` · `matches!((alive, n), (true, 2) | (_, 3))`.""", "O(w · h) per step", "O(1) extra (two buffers owned by `Life`)"),
    "LeetCode's in-place version (289) encodes the next state in spare bits of each cell. When is that better than a second buffer?",
    ["Compute the new generation from the old one only.", "Double-buffer with `mem::swap` instead of allocating a snapshot.", "A `&self` helper can't run inside an `iter_mut` over a field; compute first, then write."],
    related=("L2", "D13"),
    wrong=dict(
        updates_in_place=sub(LIFE_SOLUTION, "                self.next[i] = matches!((self.cells[i], n), (true, 2) | (_, 3));\n            }\n        }\n        std::mem::swap(&mut self.cells, &mut self.next);",
                             "                self.cells[i] = matches!((self.cells[i], n), (true, 2) | (_, 3));\n            }\n        }"),
        snapshot_each_step=sub(LIFE_SOLUTION, "        std::mem::swap(&mut self.cells, &mut self.next);", "        self.cells = self.next.clone();"),
        survives_with_four=sub(LIFE_SOLUTION, "(true, 2) | (_, 3)", "(true, 2) | (true, 4) | (_, 3)"),
    ),
))

QUEUE_HEAD = r"""
#[derive(Debug, PartialEq)]
pub struct Job {
    pub name: String,
    pub deadline: u64,
}

pub struct Queue {
    pub jobs: Vec<Job>,
}
"""

QUEUE_DOCS = dict(
    expired="""    /// Removes up to `limit` expired jobs (deadline before `now`), taking the earliest in the queue first, and
    /// returns them in queue order. Expired jobs past the limit stay queued where they are.
    pub fn take_expired(&mut self, now: u64, limit: usize) -> Vec<Job> {
""",
    batch="""    /// Removes the first `n` jobs (all of them if there are fewer) and returns them in order.
    pub fn next_batch(&mut self, n: usize) -> Vec<Job> {
""",
    defer="""    /// Moves the first job named `name` to the back of the queue, keeping the others in order. `false` if
    /// there's no such job.
    pub fn defer(&mut self, name: &str) -> bool {
""",
)

QUEUE_BODIES = dict(
    expired="""        self.jobs.extract_if(.., |j| j.deadline < now).take(limit).collect()
""",
    batch="""        let n = n.min(self.jobs.len());
        self.jobs.drain(..n).collect()
""",
    defer="""        match self.jobs.iter().position(|j| j.name == name) {
            Some(i) => {
                self.jobs[i..].rotate_left(1);
                true
            }
            None => false,
        }
""",
)


def queue_src(bodies):
    out = QUEUE_HEAD + "\nimpl Queue {\n"
    for k in ("expired", "batch", "defer"):
        out += QUEUE_DOCS[k] + bodies[k] + "    }\n\n"
    return out.rstrip("\n") + "\n}\n"


QUEUE_SOLUTION = queue_src(QUEUE_BODIES)
QUEUE_STARTER = queue_src({k: "        todo!()\n" for k in QUEUE_BODIES})
Q_HELPER = r"""
fn queue(xs: &[(&str, u64)]) -> Queue {
    Queue { jobs: xs.iter().map(|&(n, d)| Job { name: n.to_string(), deadline: d }).collect() }
}

fn sv(xs: &[&str]) -> Vec<String> {
    xs.iter().map(|s| s.to_string()).collect()
}

fn names(jobs: &[Job]) -> Vec<String> {
    jobs.iter().map(|j| j.name.to_string()).collect()
}
"""
Q_SETUP = 'let mut q = queue(&[("a", 5), ("b", 20), ("c", 7), ("d", 3), ("e", 50)]);'
Q_DESC = "jobs a 5, b 20, c 7, d 3, e 50"


def q_case(name, desc, ops, want, setup=Q_SETUP):
    return T(name, f"{Q_DESC}; {desc}" if setup == Q_SETUP else desc, ops, want, setup=setup)


P.append(write(
    "extract-if", "Move jobs out: extract_if, drain, rotate", "medium", "iterator-invalidation", ["extract_if", "drain", "rotate_left", "lazy iterators"],
    """
        Write three ways of taking jobs out of a queue without cloning any: the first few expired jobs, the next
        batch from the front, and deferring one job to the back. Some tests use long queues, so each call
        must shift the rest of the queue at most once.
    """,
    QUEUE_STARTER,
    QUEUE_SOLUTION,
    [Q_HELPER,
     q_case("take_expired", "take_expired(now 10, limit 5)", "(names(&q.take_expired(10, 5)), names(&q.jobs))", '(sv(&["a", "c", "d"]), sv(&["b", "e"]))'),
     q_case("limit_leaves_the_rest", "take_expired(now 10, limit 2)", "(names(&q.take_expired(10, 2)), names(&q.jobs))", '(sv(&["a", "c"]), sv(&["b", "d", "e"]))'),
     q_case("next_batch", "next_batch(2), then next_batch(9)", "{ let first = q.next_batch(2); let rest = q.next_batch(9); (names(&first), names(&rest), q.jobs.len()) }", '(sv(&["a", "b"]), sv(&["c", "d", "e"]), 0)'),
     q_case("defer", "defer(\"b\"), defer(\"z\")", "(q.defer(\"b\"), q.defer(\"z\"), names(&q.jobs))", '(true, false, sv(&["a", "c", "d", "e", "b"]))'),
     q_case("deadline_is_strict", "take_expired(now 5, limit 9)", "names(&q.take_expired(5, 9))", 'sv(&["d"])'),
     q_case("limit_zero", "take_expired(now 100, limit 0)", "(q.take_expired(100, 0).len(), q.jobs.len())", "(0, 5)")],
    [Q_HELPER,
     q_case("empty_queue", "empty queue: all three", "(q.take_expired(9, 9).len(), q.next_batch(3).len(), q.defer(\"a\"))", "(0, 0, false)", setup="let mut q = queue(&[]);"),
     q_case("nothing_expired", "take_expired(now 0, limit 9)", "(q.take_expired(0, 9).len(), q.jobs.len())", "(0, 5)"),
     q_case("all_expired", "take_expired(now 100, limit 9)", "(names(&q.take_expired(100, 9)), q.jobs.len())", '(sv(&["a", "b", "c", "d", "e"]), 0)'),
     q_case("batch_zero", "next_batch(0)", "(q.next_batch(0).len(), q.jobs.len())", "(0, 5)"),
     q_case("defer_last_is_noop", "defer(\"e\")", "(q.defer(\"e\"), names(&q.jobs))", '(true, sv(&["a", "b", "c", "d", "e"]))'),
     q_case("defer_first_duplicate", "jobs x 1, y 2, x 3; defer(\"x\")", "(q.defer(\"x\"), q.jobs.iter().map(|j| j.deadline).collect::<Vec<_>>())", "(true, vec![2, 3, 1])", setup='let mut q = queue(&[("x", 1), ("y", 2), ("x", 3)]);'),
     q_case("jobs_moved_not_copied", "take_expired keeps the job's own String", "(got[0].name.as_ptr() == p, got.len())", "(true, 1)", setup='let mut q = queue(&[("k", 1)]);\nlet p = q.jobs[0].name.as_ptr();\nlet got = q.take_expired(2, 1);'),
     q_case("take_expired_twice", "take_expired(now 10, limit 1) twice", "{ let a = q.take_expired(10, 1); let b = q.take_expired(10, 1); (names(&a), names(&b), names(&q.jobs)) }", '(sv(&["a"]), sv(&["c"]), sv(&["b", "d", "e"]))'),
     r"""
     #[test]
     fn random_vs_model() {
         let mut rng = anneal_prelude::Rng::new(6222);
         for _ in 0..300 {
             let n = rng.below(8);
             let start: Vec<(String, u64)> = (0..n).map(|i| (format!("{}{i}", rng.string(1, "ab")), rng.below(10) as u64)).collect();
             let mut q = Queue { jobs: start.iter().map(|(s, d)| Job { name: s.clone(), deadline: *d }).collect() };
             let mut model = start.clone();
             let mut ops = Vec::new();
             for _ in 0..4 {
                 match rng.below(3) {
                     0 => {
                         let (now, limit) = (rng.below(10) as u64, rng.below(4));
                         let mut want = Vec::new();
                         let mut kept = Vec::new();
                         for (s, d) in model {
                             if d < now && want.len() < limit {
                                 want.push(s);
                             } else {
                                 kept.push((s, d));
                             }
                         }
                         model = kept;
                         ops.push(format!("take_expired({now}, {limit})"));
                         let got: Vec<String> = q.take_expired(now, limit).into_iter().map(|j| j.name).collect();
                         check!(format!("{start:?}; {}", ops.join(", ")), got, want);
                     }
                     1 => {
                         let k = rng.below(4);
                         let want: Vec<String> = model.drain(..k.min(model.len())).map(|(s, _)| s).collect();
                         ops.push(format!("next_batch({k})"));
                         let got: Vec<String> = q.next_batch(k).into_iter().map(|j| j.name).collect();
                         check!(format!("{start:?}; {}", ops.join(", ")), got, want);
                     }
                     _ => {
                         let name = format!("{}{}", rng.string(1, "ab"), rng.below(n + 1));
                         let want = match model.iter().position(|(s, _)| *s == name) {
                             Some(i) => {
                                 let j = model.remove(i);
                                 model.push(j);
                                 true
                             }
                             None => false,
                         };
                         ops.push(format!("defer({name:?})"));
                         check!(format!("{start:?}; {}", ops.join(", ")), q.defer(&name), want);
                     }
                 }
             }
             let got: Vec<(String, u64)> = q.jobs.iter().map(|j| (j.name.clone(), j.deadline)).collect();
             check!(format!("{start:?}; {}; queue", ops.join(", ")), got, model);
         }
     }

     #[test]
     fn long_queue() {
         let mut q = Queue { jobs: (0..200_000).map(|i| Job { name: format!("j{i}"), deadline: i % 2 }).collect() };
         let gone = q.take_expired(1, 150_000);
         let batch = q.next_batch(1);
         let deferred = q.defer("j1");
         check!("200000 jobs, half expired", (gone.len(), gone[99_999].name.clone(), batch[0].name.clone(), deferred, q.jobs.len(), q.jobs[99_998].name.clone()), (100_000, "j199998".to_string(), "j1".to_string(), false, 99_999, "j199999".to_string()));
     }
     """],
    [("rust", "`Vec::extract_if(range, pred)` removes the matching elements it visits and yields them by value, in order, shifting the rest once. It's lazy: stop it early (with `take`) and the elements it didn't reach, matching or not, stay in the `Vec`."),
     ("rust", "`drain(..n)` removes a range and yields it; it panics if `n` is past the end."),
     ("rust", "Moving one element to the back of a sub-slice is `v[i..].rotate_left(1)`: one shift, no remove and push.")],
    ("""Each method moves `Job`s out of the `Vec` instead of cloning them, and shifts the survivors once. `extract_if` is the removing counterpart of `retain`, and its laziness is part of the contract: `extract_if(.., pred).take(limit)` stops after `limit` matches, and the unvisited tail (including more expired jobs) is kept in place when the iterator is dropped. Collecting everything and truncating would lose the extra jobs. `drain(..n)` needs `n` clamped to the length. `rotate_left(1)` on the tail moves one element to the back in a single pass.

Syntax to remember: `self.jobs.extract_if(.., |j| j.deadline < now).take(limit).collect()` · `self.jobs.drain(..n.min(self.jobs.len())).collect()` · `self.jobs[i..].rotate_left(1)`.""", "O(n) per call", "O(k) for the returned jobs"),
    "Before `extract_if` was stable, how would you have taken the first `limit` expired jobs in one pass?",
    ["`extract_if` moves matching elements out, lazily.", "`drain(range)` removes a range; clamp it.", "`rotate_left` on a sub-slice moves one element to the back."],
    related=("L2", "S3"),
    wrong=dict(
        truncates_extra_expired=queue_src(dict(QUEUE_BODIES, expired="""        let mut gone: Vec<Job> = self.jobs.extract_if(.., |j| j.deadline < now).collect();
        gone.truncate(limit);
        gone
""")),
        defer_swaps_with_last=queue_src(dict(QUEUE_BODIES, defer="""        match self.jobs.iter().position(|j| j.name == name) {
            Some(i) => {
                let last = self.jobs.len() - 1;
                self.jobs.swap(i, last);
                true
            }
            None => false,
        }
""")),
        expired_includes_now=sub(QUEUE_SOLUTION, "j.deadline < now", "j.deadline <= now"),
    ),
))

SESSION_HEAD = r"""
use std::collections::HashMap;

#[derive(Debug, PartialEq)]
pub struct Session {
    pub parent: Option<u32>,
    pub expires: u64,
    pub bytes: u64,
}

/// Removes every session that has expired (`expires <= now`). Each removed session's bytes are credited to
/// its parent, if the parent is still there after this call; credit isn't passed further up. Returns the
/// removed ids, sorted.
"""

EXPIRE_STARTER = SESSION_HEAD + r"""pub fn expire(sessions: &mut HashMap<u32, Session>, now: u64) -> Vec<u32> {
    let mut removed = Vec::new();
    for (id, s) in sessions.iter() {
        if s.expires <= now {
            if let Some(p) = s.parent {
                if let Some(parent) = sessions.get_mut(&p) {
                    parent.bytes += s.bytes;
                }
            }
            sessions.remove(id);
            removed.push(*id);
        }
    }
    removed.sort();
    removed
}
"""

EXPIRE_SOLUTION = SESSION_HEAD + r"""pub fn expire(sessions: &mut HashMap<u32, Session>, now: u64) -> Vec<u32> {
    let gone: Vec<(u32, Session)> = sessions.extract_if(|_, s| s.expires <= now).collect();
    let mut removed = Vec::with_capacity(gone.len());
    for (id, s) in gone {
        if let Some(parent) = s.parent.and_then(|p| sessions.get_mut(&p)) {
            parent.bytes += s.bytes;
        }
        removed.push(id);
    }
    removed.sort_unstable();
    removed
}
"""

SESS_HELPER = r"""
fn sessions(xs: &[(u32, Option<u32>, u64, u64)]) -> std::collections::HashMap<u32, Session> {
    xs.iter().map(|&(id, parent, expires, bytes)| (id, Session { parent, expires, bytes })).collect()
}

fn bytes(m: &std::collections::HashMap<u32, Session>) -> Vec<(u32, u64)> {
    let mut v: Vec<(u32, u64)> = m.iter().map(|(&id, s)| (id, s.bytes)).collect();
    v.sort();
    v
}
"""


def expire_py(xs, now):
    gone = [x for x in xs if x[2] <= now]
    keep = {x[0]: list(x) for x in xs if x[2] > now}
    for g in gone:
        if g[1] is not None and g[1] in keep:
            keep[g[1]][3] += g[3]
    return sorted(g[0] for g in gone), sorted((k, v[3]) for k, v in keep.items())


def sess_case(name, xs, now):
    removed, left = expire_py(xs, now)
    lit = "&[" + ", ".join(f"({i}, {'None' if p is None else f'Some({p})'}, {e}, {b})" for i, p, e, b in xs) + "]"
    desc = "sessions (id, parent, expires, bytes) " + ", ".join(f"({i}, {p}, {e}, {b})" for i, p, e, b in xs) + f"; now {now}"
    rv = f"vec!{removed}" if removed else "Vec::<u32>::new()"
    lv = "vec![" + ", ".join(f"({k}, {b})" for k, b in left) + "]" if left else "Vec::<(u32, u64)>::new()"
    return T(name, desc, "(removed, bytes(&m))", f"({rv}, {lv})", setup=f"let mut m = sessions({lit});\nlet removed = expire(&mut m, {now});")


P.append(fix(
    "fix-map-mutation-during-iteration", "Fix: HashMap mutation during iteration", "medium", "iterator-invalidation", ["E0502", "HashMap::extract_if", "get_mut", "order independence"],
    """
        `expire` doesn't compile: it removes entries from the map it's iterating and edits other entries on the
        way. Fix it without cloning any session. The result must not depend on the map's iteration order.
    """,
    EXPIRE_STARTER,
    EXPIRE_SOLUTION,
    [SESS_HELPER,
     sess_case("example", [(1, None, 100, 10), (2, 1, 5, 3), (3, 1, 50, 4)], 10),
     sess_case("parent_also_expires", [(1, None, 5, 10), (2, 1, 5, 3)], 10),
     sess_case("credit_not_passed_up", [(1, 2, 1, 7), (2, 3, 1, 5), (3, None, 99, 0)], 10),
     sess_case("nothing_expired", [(1, None, 50, 1)], 10),
     sess_case("expires_at_now", [(4, None, 10, 1), (5, 4, 11, 1)], 10)],
    [SESS_HELPER,
     sess_case("empty", [], 3),
     sess_case("all_expire", [(1, None, 0, 1), (2, 1, 0, 2), (3, 2, 0, 3)], 0),
     sess_case("missing_parent", [(1, 9, 0, 4), (2, None, 5, 1)], 1),
     sess_case("self_parent", [(1, 1, 0, 4), (2, 2, 9, 1)], 1),
     sess_case("siblings_add_up", [(1, None, 9, 0), (2, 1, 1, 3), (3, 1, 1, 4), (4, 1, 1, 5)], 2),
     sess_case("grandchild_to_live_parent", [(1, None, 9, 0), (2, 1, 9, 0), (3, 2, 1, 6)], 2),
     sess_case("big_bytes", [(1, None, 9, 1 << 40), (2, 1, 1, 1 << 40)], 1),
     sess_case("unsorted_ids", [(30, None, 1, 1), (10, None, 1, 1), (20, None, 9, 1)], 5),
     r"""
     #[test]
     fn random_vs_brute_force() {
         let mut rng = anneal_prelude::Rng::new(6223);
         for _ in 0..300 {
             let n = rng.below(8) as u32;
             let xs: Vec<(u32, Option<u32>, u64, u64)> = (0..n).map(|id| (id, if rng.bool() { Some(rng.below(n as usize + 1) as u32) } else { None }, rng.below(6) as u64, rng.below(10) as u64)).collect();
             let now = rng.below(6) as u64;
             let mut want_removed: Vec<u32> = xs.iter().filter(|x| x.2 <= now).map(|x| x.0).collect();
             want_removed.sort();
             let mut left: Vec<(u32, u64)> = xs.iter().filter(|x| x.2 > now).map(|x| (x.0, x.3)).collect();
             for g in xs.iter().filter(|x| x.2 <= now) {
                 if let Some(p) = g.1 {
                     if let Some(e) = left.iter_mut().find(|e| e.0 == p) {
                         e.1 += g.3;
                     }
                 }
             }
             left.sort();
             let mut m = sessions(&xs);
             let removed = expire(&mut m, now);
             check!(format!("sessions {xs:?}; now {now}"), (removed, bytes(&m)), (want_removed, left));
         }
     }

     #[test]
     fn many_sessions() {
         let xs: Vec<(u32, Option<u32>, u64, u64)> = (0..200_000u32).map(|id| (id, if id % 2 == 1 { Some(id - 1) } else { None }, if id % 2 == 1 { 5 } else { 100 }, 1)).collect();
         let mut m = sessions(&xs);
         let removed = expire(&mut m, 10);
         let b = bytes(&m);
         check!("200000 sessions, odd ones expire into their even parent", (removed.len(), b.len(), b[99_999]), (100_000, 100_000, (199_998, 2)));
     }
     """],
    [("rust", "Removing from a hash map can move other entries, so no iterator over it survives a `remove`, and `get_mut` inside the loop is a second borrow of the map. Split the work: first take the expired sessions out, then credit the parents."),
     ("rust", "`HashMap::extract_if(|k, v| ..)` removes the entries the closure accepts and yields them by value. After it's done, every parent still in the map is one that survives, which is what the crediting rule needs."),
     ("rust", "Crediting while removing one session at a time depends on the order: a parent that expires later in the loop would pass its children's credit up. Remove all first, then credit.")],
    ("""Two things go wrong in the original. The borrow checker rejects `remove` and `get_mut` while `iter()` holds the map. And even a version that compiles (collect the ids, then remove one at a time, crediting as it goes) gives order-dependent answers, because whether a parent is "still there" depends on which expired session was handled first, and `HashMap` iteration order is arbitrary. Two phases fix both: `extract_if` moves every expired session out (no clones, one pass), and then each credit goes to a parent that is known to survive. Credit can't flow up a chain, because an expired parent is already gone.

Syntax to remember: `let gone: Vec<(u32, Session)> = map.extract_if(|_, s| s.expires <= now).collect();` · `s.parent.and_then(|p| map.get_mut(&p))` · `HashMap::retain(|_, v| ..)` when there's nothing to hand back.""", "O(n + k log k)", "O(k) for the removed sessions"),
    "How would you pass the credit all the way up to the nearest surviving ancestor instead?",
    ["A map can't be mutated while it's iterated; take the matching entries out first.", "`HashMap::extract_if` removes and yields entries by value.", "Results must not depend on hash iteration order."],
    rules=dict(methods=["clone"]),
    related=("L2", "S4"),
    wrong=dict(
        credits_while_removing=r"""
            use std::collections::HashMap;

            #[derive(Debug, PartialEq)]
            pub struct Session {
                pub parent: Option<u32>,
                pub expires: u64,
                pub bytes: u64,
            }

            pub fn expire(sessions: &mut HashMap<u32, Session>, now: u64) -> Vec<u32> {
                let mut ids: Vec<u32> = sessions.iter().filter(|(_, s)| s.expires <= now).map(|(&id, _)| id).collect();
                ids.sort_unstable();
                for &id in &ids {
                    let s = sessions.remove(&id).unwrap();
                    if let Some(parent) = s.parent.and_then(|p| sessions.get_mut(&p)) {
                        parent.bytes += s.bytes;
                    }
                }
                ids
            }
        """,
        strictly_before_now=sub(EXPIRE_SOLUTION, "s.expires <= now", "s.expires < now"),
        credit_lost=sub(EXPIRE_SOLUTION, "            parent.bytes += s.bytes;\n", "            parent.bytes = parent.bytes.max(s.bytes);\n"),
    ),
))

# ---------------------------------------------------------------- split borrows (medium)

P.append(dict(slug="two-mutable-borrows-of-self"))  # written by hand; keeps its position

EDITOR_HEAD = r"""
pub enum Macro {
    Append(String),
    Upper,
    Replace(String, String),
}

pub struct Editor {
    pub text: String,
    pub clipboard: String,
    pub macros: Vec<Macro>,
    log: Vec<String>,
}
"""

EDITOR_STARTER = EDITOR_HEAD + r"""
impl Editor {
    pub fn new(text: &str) -> Self {
        Editor { text: String::from(text), clipboard: String::new(), macros: Vec::new(), log: Vec::new() }
    }

    pub fn log(&self) -> &[String] {
        &self.log
    }

    /// Adds "<n>. <what>" to the log, numbered from 1.
    fn note(&mut self, what: &str) {
        let n = self.log.len() + 1;
        self.log.push(format!("{n}. {what}"));
    }

    /// Applies `m` to the text and notes "append <s>", "upper" or "replace <a> with <b>".
    fn apply(&mut self, m: &Macro) {
        match m {
            Macro::Append(s) => {
                self.text.push_str(s);
                self.note(&format!("append {s}"));
            }
            Macro::Upper => {
                self.text.make_ascii_uppercase();
                self.note("upper");
            }
            Macro::Replace(a, b) => {
                self.text = self.text.replace(a.as_str(), b);
                self.note(&format!("replace {a} with {b}"));
            }
        }
    }

    /// Applies every macro, in order. The macros stay for next time.
    pub fn run_macros(&mut self) {
        for m in &self.macros {
            self.apply(m);
        }
    }

    /// Moves the text into the clipboard (replacing what was there), leaving the text empty, and notes
    /// "cut <n> bytes".
    pub fn cut(&mut self) {
        self.clipboard = self.text;
        self.note(&format!("cut {} bytes", self.clipboard.len()));
    }

    /// Appends the clipboard to the text (the clipboard keeps it) and notes "paste <clipboard>".
    pub fn paste(&mut self) {
        let clip = &self.clipboard;
        self.note(&format!("paste {clip}"));
        self.text.push_str(clip);
    }
}
"""

EDITOR_SOLUTION = EDITOR_STARTER
for _old, _new in [
    ("        for m in &self.macros {\n            self.apply(m);\n        }\n",
     "        let macros = std::mem::take(&mut self.macros);\n        for m in &macros {\n            self.apply(m);\n        }\n        self.macros = macros;\n"),
    ("        self.clipboard = self.text;\n", "        self.clipboard = std::mem::take(&mut self.text);\n"),
    ("        let clip = &self.clipboard;\n        self.note(&format!(\"paste {clip}\"));\n        self.text.push_str(clip);\n",
     "        self.text.push_str(&self.clipboard);\n        let msg = format!(\"paste {}\", self.clipboard);\n        self.note(&msg);\n"),
]:
    EDITOR_SOLUTION = sub(EDITOR_SOLUTION, _old, _new)

ED_MACROS = 'e.macros = vec![Macro::Append("!".to_string()), Macro::Replace("a".to_string(), "o".to_string()), Macro::Upper];'
ED_DESC = 'macros [Append "!", Replace "a" with "o", Upper]'

P.append(fix(
    "fix-field-borrow-and-mut-method", "Fix: a field borrowed across a &mut self call", "medium", "split-borrows", ["E0502", "E0507", "mem::take", "take and restore"],
    """
        `Editor` doesn't compile: three methods hold a borrow of one field (or try to move one) while calling
        `note` or `apply`, which take all of `self`. Fix them without cloning anything and without changing
        `note` or `apply`.
    """,
    EDITOR_STARTER,
    EDITOR_SOLUTION,
    [T("run_macros_example", 'text "banana"; ' + ED_DESC + "; run twice", "(e.text.as_str(), e.log().to_vec(), e.macros.len())",
       '("BONONO!!", ["1. append !", "2. replace a with o", "3. upper", "4. append !", "5. replace a with o", "6. upper"].map(String::from).to_vec(), 3)',
       setup='let mut e = Editor::new("banana");\n' + ED_MACROS + "\ne.run_macros();\ne.run_macros();"),
     T("cut_then_paste_twice", 'text "hi"; cut; paste; paste', "(e.text.as_str(), e.clipboard.as_str(), e.log().to_vec())", '("hihi", "hi", ["1. cut 2 bytes", "2. paste hi", "3. paste hi"].map(String::from).to_vec())',
       setup='let mut e = Editor::new("hi");\ne.cut();\ne.paste();\ne.paste();'),
     T("cut_moves_the_text", 'text "moved"; cut', "(p == e.clipboard.as_ptr(), e.text.is_empty())", "(true, true)", setup='let mut e = Editor::new("moved");\nlet p = e.text.as_ptr();\ne.cut();'),
     T("no_macros", 'text "x"; run_macros', "(e.text.as_str(), e.log().len())", '("x", 0)', setup='let mut e = Editor::new("x");\ne.run_macros();'),
     T("paste_empty_clipboard", 'text "a"; paste', "(e.text.as_str(), e.log().to_vec())", '("a", vec!["1. paste ".to_string()])', setup='let mut e = Editor::new("a");\ne.paste();')],
    [T("cut_replaces_clipboard", 'text "one"; cut; text = "two"; cut', "(e.clipboard.as_str(), e.text.as_str())", '("two", "")', setup='let mut e = Editor::new("one");\ne.cut();\ne.text.push_str("two");\ne.cut();'),
     T("cut_empty", "text \"\"; cut", "e.log().to_vec()", 'vec!["1. cut 0 bytes".to_string()]', setup='let mut e = Editor::new("");\ne.cut();'),
     T("macros_see_earlier_macros", 'text "a"; [Append "a", Replace "aa" with "b"]', "e.text.as_str()", '"b"', setup='let mut e = Editor::new("a");\ne.macros = vec![Macro::Append("a".to_string()), Macro::Replace("aa".to_string(), "b".to_string())];\ne.run_macros();'),
     T("macros_kept_in_order", 'run once, then add a macro and run', "e.text.as_str()", '"XY!Z"',
       setup='let mut e = Editor::new("x");\ne.macros = vec![Macro::Upper, Macro::Append("Y".to_string())];\ne.run_macros();\ne.macros.push(Macro::Append("!".to_string()));\ne.text = "X".to_string();\ne.run_macros();\ne.text.push(\'Z\');'),
     T("unicode_cut", 'text "日本"; cut', "e.log().to_vec()", 'vec!["1. cut 6 bytes".to_string()]', setup='let mut e = Editor::new("日本");\ne.cut();'),
     T("paste_after_cut_restores", 'text "abc"; cut; paste', "(e.text.as_str(), e.clipboard.as_str())", '("abc", "abc")', setup='let mut e = Editor::new("abc");\ne.cut();\ne.paste();'),
     T("log_numbering_continues", "cut, paste, run [Upper]", "e.log().to_vec()", '["1. cut 1 bytes", "2. paste q", "3. upper"].map(String::from).to_vec()',
       setup='let mut e = Editor::new("q");\ne.cut();\ne.paste();\ne.macros = vec![Macro::Upper];\ne.run_macros();'),
     T("macro_pointer_stable", "the macros Vec is the same allocation after run_macros", "p == e.macros.as_ptr()", "true",
       setup='let mut e = Editor::new("q");\ne.macros = vec![Macro::Upper, Macro::Upper];\nlet p = e.macros.as_ptr();\ne.run_macros();'),
     r"""
     #[test]
     fn random_vs_model() {
         let mut rng = anneal_prelude::Rng::new(6224);
         for _ in 0..300 {
             let len = rng_len(&mut rng);
             let start = rng.string(len, "ab");
             let mut e = Editor::new(&start);
             let (mut text, mut clip, mut log) = (start.clone(), String::new(), Vec::<String>::new());
             let mut ops = Vec::new();
             for _ in 0..5 {
                 match rng.below(3) {
                     0 => {
                         e.cut();
                         clip = std::mem::take(&mut text);
                         log.push(format!("{}. cut {} bytes", log.len() + 1, clip.len()));
                         ops.push("cut");
                     }
                     1 => {
                         e.paste();
                         text.push_str(&clip);
                         log.push(format!("{}. paste {clip}", log.len() + 1));
                         ops.push("paste");
                     }
                     _ => {
                         e.macros = vec![Macro::Replace("a".to_string(), "b".to_string()), Macro::Append("a".to_string())];
                         e.run_macros();
                         text = text.replace('a', "b") + "a";
                         log.push(format!("{}. replace a with b", log.len() + 1));
                         log.push(format!("{}. append a", log.len() + 1));
                         ops.push("run [Replace a b, Append a]");
                     }
                 }
             }
             check!(format!("text {start:?}; {}", ops.join(", ")), (e.text.clone(), e.clipboard.clone(), e.log().to_vec()), (text, clip, log));
         }
     }

     fn rng_len(rng: &mut anneal_prelude::Rng) -> usize {
         rng.below(4)
     }
     """],
    [("rust", "`for m in &self.macros { self.apply(m) }` borrows `self.macros` for the loop while `apply` wants all of `self`. Move the macros out (`mem::take`), run them, put them back."),
     ("rust", "`self.clipboard = self.text` tries to move a field out of `&mut self` (E0507). `mem::take` moves it and leaves an empty `String`."),
     ("rust", "In `paste`, `clip` is still used after `note` needs `&mut self`. (Reading it inside `note`'s own arguments is fine: that's a two-phase borrow.) Finish with the clipboard before the call, or reach it through the field again afterwards.")],
    ("""A method that takes `&mut self` borrows every field, so it can't run while you hold a borrow of any one of them. Three ways out, one per method. Take and restore: `let macros = mem::take(&mut self.macros); ..; self.macros = macros;` detaches the field for the duration of the calls. (While it's out, `apply` would see an empty `macros`, and a panic in between would lose it; both are acceptable here.) Move with a replacement: you can't move a field out of `&mut self`, but `mem::take` (or `mem::replace`) swaps something in. Finish with the borrow before the call: compute the owned message, then call `note`.

Another fix for `run_macros` changes the helper's shape: make `apply` an associated function over the fields it touches (`fn apply(text: &mut String, log: &mut Vec<String>, m: &Macro)`), and the call borrows disjoint fields.

Syntax to remember: `let macros = std::mem::take(&mut self.macros);` · `self.clipboard = std::mem::take(&mut self.text);` · `std::mem::replace(&mut self.state, State::Idle)`.""", "O(total text) per call", "O(1) extra"),
    "`mem::take` needs `Default`. How would you take and restore a field whose type has no cheap default?",
    ["A `&mut self` call conflicts with any live borrow of a field.", "Take and restore: `mem::take` the field, use it, put it back.", "Finish the borrow before the call."],
    rules=dict(methods=["clone", "to_owned", "to_string", "cloned"]),
    wrong=dict(
        macros_not_restored=sub(EDITOR_SOLUTION, "        self.macros = macros;\n", ""),
        cut_counts_after_taking=sub(EDITOR_SOLUTION, 'self.note(&format!("cut {} bytes", self.clipboard.len()));', 'self.note(&format!("cut {} bytes", self.text.len()));'),
        paste_moves_the_clipboard=sub(EDITOR_SOLUTION, "        self.text.push_str(&self.clipboard);\n        let msg = format!(\"paste {}\", self.clipboard);\n",
                                      "        let clip = std::mem::take(&mut self.clipboard);\n        self.text.push_str(&clip);\n        let msg = format!(\"paste {clip}\");\n"),
    ),
))

PAIR_STARTER = r"""
/// Mutable references to `v[i]` and `v[j]`, in that order. `None` if `i == j` or either is out of bounds.
pub fn pair_mut<T>(v: &mut [T], i: usize, j: usize) -> Option<(&mut T, &mut T)> {
    todo!()
}

/// Calls `f(&mut v[k], &mut v[k + 1])` for every adjacent pair, left to right. Later calls see the changes
/// earlier calls made.
pub fn for_each_adjacent_mut<T>(v: &mut [T], mut f: impl FnMut(&mut T, &mut T)) {
    todo!()
}
"""

PAIR_SOLUTION = r"""
/// Mutable references to `v[i]` and `v[j]`, in that order. `None` if `i == j` or either is out of bounds.
pub fn pair_mut<T>(v: &mut [T], i: usize, j: usize) -> Option<(&mut T, &mut T)> {
    if i == j || i >= v.len() || j >= v.len() {
        return None;
    }
    let (lo, hi) = (i.min(j), i.max(j));
    let (left, right) = v.split_at_mut(hi);
    let (a, b) = (&mut left[lo], &mut right[0]);
    Some(if i < j { (a, b) } else { (b, a) })
}

/// Calls `f(&mut v[k], &mut v[k + 1])` for every adjacent pair, left to right. Later calls see the changes
/// earlier calls made.
pub fn for_each_adjacent_mut<T>(v: &mut [T], mut f: impl FnMut(&mut T, &mut T)) {
    for k in 1..v.len() {
        let (left, right) = v.split_at_mut(k);
        f(&mut left[k - 1], &mut right[0]);
    }
}
"""

CARRY = "|a: &mut u32, b: &mut u32| { *b += *a / 10; *a %= 10; }"
BUBBLE = "|a: &mut i32, b: &mut i32| if *a > *b { std::mem::swap(a, b) }"

P.append(write(
    "pair-mut", "Two &mut into one slice, and adjacent pairs", "medium", "split-borrows", ["split_at_mut", "no windows_mut", "FnMut(&mut T, &mut T)"],
    """
        Write `pair_mut`, which returns `&mut` to two different elements at once, and `for_each_adjacent_mut`,
        which hands a closure each adjacent pair mutably: the `windows_mut` that std doesn't have. No `unsafe`,
        and no `get_disjoint_mut` (write the split yourself).
    """,
    PAIR_STARTER,
    PAIR_SOLUTION,
    [T("pair_swap", "v = [1, 2, 3], pair (0, 2), swap", "{ let mut v = [1, 2, 3]; if let Some((a, b)) = pair_mut(&mut v, 0, 2) { std::mem::swap(a, b); } v }", "[3, 2, 1]"),
     T("pair_order_kept", "v = [10, 20, 30], pair (2, 0)", "{ let mut v = [10, 20, 30]; let (a, b) = pair_mut(&mut v, 2, 0).unwrap(); (*a, *b) }", "(30, 10)"),
     T("pair_none", "pair (1, 1) and (0, 5) of a 2-element slice", "(pair_mut(&mut [1, 2], 1, 1).is_none(), pair_mut(&mut [1, 2], 0, 5).is_none())", "(true, true)"),
     T("carry_digits", "digits [15, 9, 3] (least significant first), carry", "{ let mut v = [15u32, 9, 3]; for_each_adjacent_mut(&mut v, " + CARRY + "); v }", "[5, 0, 4]"),
     T("bubble_pass", "one bubble pass over [3, 1, 2, 0]", "{ let mut v = [3, 1, 2, 0]; for_each_adjacent_mut(&mut v, " + BUBBLE + "); v }", "[1, 2, 0, 3]"),
     T("short_slices", "for_each_adjacent_mut on [] and [7]: calls", "{ let mut n = 0; for_each_adjacent_mut(&mut [0u8; 0], |_, _| n += 1); for_each_adjacent_mut(&mut [7], |_, _| n += 1); n }", "0")],
    [T("pair_adjacent", "v = [1, 2], pair (1, 0), a += 10", "{ let mut v = [1, 2]; let (a, _) = pair_mut(&mut v, 1, 0).unwrap(); *a += 10; v }", "[1, 12]"),
     T("pair_at_end", "pair (0, len - 1) of 5", "{ let mut v = [0, 1, 2, 3, 4]; let (a, b) = pair_mut(&mut v, 0, 4).unwrap(); (*a, *b) }", "(0, 4)"),
     T("pair_empty", "pair (0, 1) of []", "pair_mut(&mut [0u8; 0], 0, 1).is_none()", "true"),
     T("pair_i_out", "pair (3, 0) of 3", "pair_mut(&mut [1, 2, 3], 3, 0).is_none()", "true"),
     T("pair_strings", "pair (1, 0) of [\"a\", \"b\"], push b onto a", '{ let mut v = ["a", "b"].map(String::from); let (b, a) = pair_mut(&mut v, 1, 0).unwrap(); a.push_str(b); v }', '["ab", "b"].map(String::from)'),
     T("prefix_sums", "[1, 2, 3, 4], b += a", "{ let mut v = [1, 2, 3, 4]; for_each_adjacent_mut(&mut v, |a, b| *b += *a); v }", "[1, 3, 6, 10]"),
     T("visits_in_order", "record pairs of [1, 2, 3]", "{ let mut seen = vec![]; for_each_adjacent_mut(&mut [1, 2, 3], |a, b| seen.push((*a, *b))); seen }", "vec![(1, 2), (2, 3)]"),
     T("carry_through_nines", "digits [10, 9, 9, 0]", "{ let mut v = [10u32, 9, 9, 0]; for_each_adjacent_mut(&mut v, " + CARRY + "); v }", "[0, 0, 0, 1]"),
     r"""
     #[test]
     fn random_vs_brute_force() {
         let mut rng = anneal_prelude::Rng::new(6225);
         for _ in 0..300 {
             let n = rng.below(7);
             let v: Vec<i32> = rng.vec(n, -9, 9);
             let (i, j) = (rng.below(n + 2), rng.below(n + 2));
             let mut got = v.clone();
             let pair = pair_mut(&mut got, i, j).map(|(a, b)| {
                 let seen = (*a, *b);
                 *a += 100;
                 *b -= 100;
                 seen
             });
             let mut want = v.clone();
             let want_pair = if i != j && i < n && j < n {
                 let seen = (v[i], v[j]);
                 want[i] += 100;
                 want[j] -= 100;
                 Some(seen)
             } else {
                 None
             };
             check!(format!("pair_mut({v:?}, {i}, {j})"), (pair, got), (want_pair, want));

             let mut got = v.clone();
             for_each_adjacent_mut(&mut got, |a, b| {
                 *b = *b * 2 - *a;
                 *a += 1;
             });
             let mut want = v.clone();
             for k in 1..n {
                 want[k] = want[k] * 2 - want[k - 1];
                 want[k - 1] += 1;
             }
             check!(format!("for_each_adjacent_mut({v:?}, b = 2b - a, a += 1)"), got, want);
         }
     }

     #[test]
     fn long_slice() {
         let mut v = vec![1u64; 200_000];
         for_each_adjacent_mut(&mut v, |a, b| *b += *a);
         check!("prefix sums of 200000 ones", (v[0], v[199_999]), (1, 200_000));
     }
     """],
    [("rust", "`split_at_mut(mid)` gives `(&mut v[..mid], &mut v[mid..])`, two borrows that can't overlap. Split at the larger index: the smaller one is in the left half, the larger is the first element of the right half."),
     ("rust", "Return the pair in the caller's order, `(v[i], v[j])`, even when `i > j`."),
     ("rust", "For adjacent pairs, split at each `k` in turn: `left[k - 1]` and `right[0]`. Each split's borrows end before the next one starts.")],
    ("""Two `&mut` into one slice need proof that they don't overlap, and `split_at_mut` is that proof: it checks `mid <= len` once and returns two disjoint halves (with `unsafe` inside std). A pair is the last element of one half and the first of the other. The adjacent-pairs helper is the loop version: split at each `k`, call `f`, and the borrows end before the next split, so later calls see earlier changes. That's also why std has `chunks_mut` but no `windows_mut`: an iterator's items can all be alive at once, and overlapping windows would alias. A callback that finishes before the next pair is made avoids the problem.

Syntax to remember: `let (left, right) = v.split_at_mut(hi);` · `Some(if i < j { (a, b) } else { (b, a) })` · `fn f<T>(v: &mut [T], mut f: impl FnMut(&mut T, &mut T))` · std's own: `v.get_disjoint_mut([i, j])`.""", "O(1) pair_mut; O(n) adjacent", "O(1)"),
    "Why can't `for_each_adjacent_mut` be an `Iterator<Item = (&mut T, &mut T)>`?",
    ["`split_at_mut` gives two disjoint `&mut` halves.", "Return pairs in the caller's order.", "A callback can visit overlapping windows; an iterator can't."],
    related=("L2", "S3"),
    wrong=dict(
        pair_sorted_order=sub(PAIR_SOLUTION, "Some(if i < j { (a, b) } else { (b, a) })", "Some((a, b))"),
        right_to_left=sub(PAIR_SOLUTION, "    for k in 1..v.len() {", "    for k in (1..v.len()).rev() {"),
        pair_bounds_off_by_one=sub(PAIR_SOLUTION, "if i == j || i >= v.len() || j >= v.len() {", "if i == j || i > v.len() || j > v.len() {"),
    ),
))

WORKER_HEAD = r"""
#[derive(Debug, PartialEq)]
pub enum State {
    Idle,
    Busy { job: String, tries: u32 },
    Done { job: String, result: u64 },
}

pub struct Worker {
    pub state: State,
    pub log: Vec<String>,
    pub max_tries: u32,
}
"""

WORKER_DOCS = dict(
    start="""    /// Idle → Busy with `job` and 0 tries, logging "start <job>". Anything else: no change, `false`.
    pub fn start(&mut self, job: &str) -> bool {
""",
    retry="""    /// Busy: counts a try and logs "retry <job> #<tries>". When tries reaches `max_tries` the worker gives up:
    /// back to Idle, logging "give up <job>" instead, and `None`. Otherwise returns the new tries count.
    /// Not busy: no change, `None`.
    pub fn retry(&mut self) -> Option<u32> {
""",
    finish="""    /// Busy → Done with the same job `String` (moved, not copied) and `result`, logging "done <job>".
    /// Anything else: no change, `false`.
    pub fn finish(&mut self, result: u64) -> bool {
""",
    collect="""    /// Done → Idle, handing back the job and result. Anything else: no change, `None`.
    pub fn collect(&mut self) -> Option<(String, u64)> {
""",
)

WORKER_BODIES = dict(
    start="""        if self.state != State::Idle {
            return false;
        }
        self.log.push(format!("start {job}"));
        self.state = State::Busy { job: job.to_string(), tries: 0 };
        true
""",
    retry="""        let Worker { state, log, max_tries } = self;
        let State::Busy { job, tries } = state else { return None };
        *tries += 1;
        if *tries < *max_tries {
            log.push(format!("retry {job} #{tries}"));
            return Some(*tries);
        }
        log.push(format!("give up {job}"));
        *state = State::Idle;
        None
""",
    finish="""        match std::mem::replace(&mut self.state, State::Idle) {
            State::Busy { job, .. } => {
                self.log.push(format!("done {job}"));
                self.state = State::Done { job, result };
                true
            }
            other => {
                self.state = other;
                false
            }
        }
""",
    collect="""        match std::mem::replace(&mut self.state, State::Idle) {
            State::Done { job, result } => Some((job, result)),
            other => {
                self.state = other;
                None
            }
        }
""",
)


def worker_src(bodies):
    out = WORKER_HEAD + "\nimpl Worker {\n"
    for k in ("start", "retry", "finish", "collect"):
        out += WORKER_DOCS[k] + bodies[k] + "    }\n\n"
    return out.rstrip("\n") + "\n}\n"


WORKER_SOLUTION = worker_src(WORKER_BODIES)
WORKER_STARTER = worker_src({k: "        todo!()\n" for k in WORKER_BODIES})
WK = "let mut w = Worker { state: State::Idle, log: vec![], max_tries: 3 };"

P.append(write(
    "destructure-self", "Destructure &mut self: binding modes and mem::replace", "medium", "split-borrows", ["patterns", "default binding modes", "let else", "mem::replace", "state machines"],
    """
        Write the transitions of a small worker state machine. The job name moves from state to state as the
        same `String`: no copies (a test compares pointers). Each method updates the state and the log in the
        same call, so you'll need borrows of both at once, and to move a field out of a `&mut self`.
    """,
    WORKER_STARTER,
    WORKER_SOLUTION,
    [T("full_cycle", "max 3; start a; retry; finish 7; collect", "(w.start(\"a\"), w.retry(), w.finish(7), w.collect(), w.state == State::Idle, w.log.clone())",
       '(true, Some(1), true, Some(("a".to_string(), 7)), true, ["start a", "retry a #1", "done a"].map(String::from).to_vec())', setup=WK),
     T("give_up", "max 3; start b; retry 3 times", "(w.retry(), w.retry(), w.retry(), w.state == State::Idle, w.log.last().cloned())", '(Some(1), Some(2), None, true, Some("give up b".to_string()))', setup=WK + '\nw.start("b");'),
     T("wrong_state_changes_nothing", "idle: retry, finish, collect", "(w.retry(), w.finish(1), w.collect(), w.log.len())", "(None, false, None, 0)", setup=WK),
     T("start_when_busy", "start a; start b", "(w.start(\"b\"), w.state == State::Busy { job: \"a\".to_string(), tries: 0 })", "(false, true)", setup=WK + '\nw.start("a");'),
     T("job_is_moved", "start j; finish; collect: the same String throughout", "(busy == done, done == out)", "(true, true)",
       setup=WK + '\nw.start("j");\nlet busy = match &w.state { State::Busy { job, .. } => job.as_ptr(), _ => std::ptr::null() };\nw.finish(1);\nlet done = match &w.state { State::Done { job, .. } => job.as_ptr(), _ => std::ptr::null() };\nlet out = w.collect().unwrap().0.as_ptr();')],
    [T("max_one", "max 1; start; retry", "(w.retry(), w.log.clone())", '(None, ["start x", "give up x"].map(String::from).to_vec())', setup='let mut w = Worker { state: State::Idle, log: vec![], max_tries: 1 };\nw.start("x");'),
     T("finish_when_done", "start; finish 1; finish 2", "(w.finish(2), w.collect())", '(false, Some(("x".to_string(), 1)))', setup=WK + '\nw.start("x");\nw.finish(1);'),
     T("start_when_done", "start; finish; start y", "(w.start(\"y\"), w.log.len())", "(false, 2)", setup=WK + '\nw.start("x");\nw.finish(1);'),
     T("collect_twice", "start; finish; collect twice", "(w.collect().is_some(), w.collect())", "(true, None)", setup=WK + '\nw.start("x");\nw.finish(1);'),
     T("restart_after_give_up", "max 3; start a; retry x3; start b; retry", "(w.start(\"b\"), w.retry(), w.log.len())", "(true, Some(1), 6)", setup=WK + '\nw.start("a");\nfor _ in 0..3 {\n    w.retry();\n}'),
     T("retry_after_finish", "start; finish; retry", "(w.retry(), w.state == State::Done { job: \"x\".to_string(), result: 5 })", "(None, true)", setup=WK + '\nw.start("x");\nw.finish(5);'),
     T("unicode_job", "start 日本; finish 0; collect", "(w.collect(), w.log.clone())", '(Some(("日本".to_string(), 0)), ["start 日本", "done 日本"].map(String::from).to_vec())', setup=WK + '\nw.start("日本");\nw.finish(0);'),
     T("tries_kept_in_state", "max 5; start; retry x2", "w.state", 'State::Busy { job: "t".to_string(), tries: 2 }', setup='let mut w = Worker { state: State::Idle, log: vec![], max_tries: 5 };\nw.start("t");\nw.retry();\nw.retry();'),
     r"""
     #[test]
     fn random_vs_model() {
         // 0 idle, 1 busy, 2 done
         let mut rng = anneal_prelude::Rng::new(6226);
         for _ in 0..300 {
             let max = 1 + rng.below(3) as u32;
             let mut w = Worker { state: State::Idle, log: vec![], max_tries: max };
             let (mut phase, mut job, mut tries, mut result) = (0, String::new(), 0u32, 0u64);
             let mut log: Vec<String> = Vec::new();
             let mut ops = Vec::new();
             for step in 0..8 {
                 match rng.below(4) {
                     0 => {
                         let name = format!("j{step}");
                         let want = phase == 0;
                         if want {
                             phase = 1;
                             job = name.clone();
                             tries = 0;
                             log.push(format!("start {name}"));
                         }
                         ops.push(format!("start {name}"));
                         check!(format!("max {max}; {}", ops.join(", ")), w.start(&name), want);
                     }
                     1 => {
                         let want = if phase == 1 {
                             tries += 1;
                             if tries < max {
                                 log.push(format!("retry {job} #{tries}"));
                                 Some(tries)
                             } else {
                                 log.push(format!("give up {job}"));
                                 phase = 0;
                                 None
                             }
                         } else {
                             None
                         };
                         ops.push("retry".to_string());
                         check!(format!("max {max}; {}", ops.join(", ")), w.retry(), want);
                     }
                     2 => {
                         let want = phase == 1;
                         if want {
                             phase = 2;
                             result = step as u64;
                             log.push(format!("done {job}"));
                         }
                         ops.push(format!("finish {step}"));
                         check!(format!("max {max}; {}", ops.join(", ")), w.finish(step as u64), want);
                     }
                     _ => {
                         let want = if phase == 2 {
                             phase = 0;
                             Some((job.clone(), result))
                         } else {
                             None
                         };
                         ops.push("collect".to_string());
                         check!(format!("max {max}; {}", ops.join(", ")), w.collect(), want);
                     }
                 }
             }
             check!(format!("max {max}; {}; log", ops.join(", ")), w.log.clone(), log);
         }
     }
     """],
    [("rust", "`let Worker { state, log, max_tries } = self;` on a `&mut Worker` binds each field as a `&mut` (default binding modes), all at once. Then `let State::Busy { job, tries } = state else { .. }` binds `job: &mut String` and `tries: &mut u32`."),
     ("rust", "To move `job` from `Busy` into `Done`, take the whole state out: `std::mem::replace(&mut self.state, State::Idle)` returns the old state by value, and you can destructure it and move its fields. Put it back if it wasn't the state you wanted."),
     ("rust", "Assigning `*state = State::Idle` while `job` is still borrowed from it won't compile; log first, then assign.")],
    ("""Matching on a `&mut` value binds its fields as `&mut` without writing `ref mut`: that's default binding modes, and destructuring `self` itself gives separate `&mut` borrows of every field at once, so a state's fields and `log` can be used together. Borrowing is enough while the state stays the same variant. Changing variant while keeping a field (the job `String`) needs ownership of the old state, and you only have `&mut self`: `mem::replace(&mut self.state, State::Idle)` moves the old state out and leaves a valid placeholder, the `match` moves `job` into the new variant, and a non-matching state is put back unchanged. `Option::take` is the same trick for `Option`.

Syntax to remember: `let Worker { state, log, max_tries } = self;` · `let State::Busy { job, tries } = state else { return None };` · `match std::mem::replace(&mut self.state, State::Idle) { State::Busy { job, .. } => .., other => self.state = other }`.""", "O(len of the job name) per log line", "O(1)"),
    "What would `retry` look like if `State::Busy` held a `Box<dyn Job>` instead of a `String`, and giving up had to hand the job to a callback?",
    ["Destructuring `&mut self` gives disjoint `&mut` fields.", "Default binding modes make pattern bindings `&mut` automatically.", "`mem::replace` moves an enum out of `&mut` to change variant without cloning."],
    wrong=dict(
        give_up_one_late=sub(WORKER_SOLUTION, "if *tries < *max_tries {", "if *tries <= *max_tries {"),
        finish_copies_the_job=sub(WORKER_SOLUTION, "                self.state = State::Done { job, result };", "                self.state = State::Done { job: job.as_str().to_string(), result };"),
        collect_leaves_done=worker_src(dict(WORKER_BODIES, collect="""        match &self.state {
            State::Done { job, result } => Some((job.to_string(), *result)),
            _ => None,
        }
""")),
    ),
))

SWAP_STARTER = r"""
use std::mem::take;

/// Swaps `v[i]` and `v[j]`. `i` may equal `j`.
pub fn swap_items(v: &mut [String], i: usize, j: usize) {
    let a = &mut v[i];
    let b = &mut v[j];
    let tmp = take(a);
    *a = take(b);
    *b = tmp;
}

/// Rotates three distinct slots: `v[i]` gets `v[j]`'s value, `v[j]` gets `v[k]`'s, `v[k]` gets `v[i]`'s.
pub fn rotate3(v: &mut [String], i: usize, j: usize, k: usize) {
    let (a, b, c) = (&mut v[i], &mut v[j], &mut v[k]);
    let first = take(a);
    *a = take(b);
    *b = take(c);
    *c = first;
}

/// Appends a copy of `v[j]` to `v[i]`. `i` may equal `j` (the string doubles).
pub fn append_copy(v: &mut [String], i: usize, j: usize) {
    v[i].push_str(&v[j]);
}
"""

SWAP_SOLUTION = r"""
use std::mem::take;

/// Swaps `v[i]` and `v[j]`. `i` may equal `j`.
pub fn swap_items(v: &mut [String], i: usize, j: usize) {
    if i == j {
        return;
    }
    let tmp = take(&mut v[i]);
    v[i] = std::mem::replace(&mut v[j], tmp);
}

/// Rotates three distinct slots: `v[i]` gets `v[j]`'s value, `v[j]` gets `v[k]`'s, `v[k]` gets `v[i]`'s.
pub fn rotate3(v: &mut [String], i: usize, j: usize, k: usize) {
    let first = take(&mut v[i]);
    v[i] = take(&mut v[j]);
    v[j] = take(&mut v[k]);
    v[k] = first;
}

/// Appends a copy of `v[j]` to `v[i]`. `i` may equal `j` (the string doubles).
pub fn append_copy(v: &mut [String], i: usize, j: usize) {
    if i == j {
        v[i].extend_from_within(..);
        return;
    }
    let (lo, hi) = (i.min(j), i.max(j));
    let (left, right) = v.split_at_mut(hi);
    let (a, b) = (&mut left[lo], &mut right[0]);
    if i < j {
        a.push_str(b);
    } else {
        b.push_str(a);
    }
}
"""


def sv_case(name, desc, v, call, want):
    lit = "[" + ", ".join(f'"{x}"' for x in v) + "].map(String::from)"
    w = "[" + ", ".join(f'"{x}"' for x in want) + "].map(String::from)"
    return T(name, f"{v}; {desc}".replace("'", '"'), f"{{ let mut v = {lit}; {call}; v }}", w)


P.append(fix(
    "fix-swap-without-swap", "Fix: move values between slots of one slice", "medium", "split-borrows", ["E0499", "E0502", "mem::take", "mem::replace", "split_at_mut", "extend_from_within"],
    """
        None of these compiles: each holds two borrows into `v` at once. Fix them without `swap`, `rotate_*`,
        `get_disjoint_mut`, or copying any string you don't have to. Two of them never need two live borrows at all. The third
        does, except in one case.
    """,
    SWAP_STARTER,
    SWAP_SOLUTION,
    [sv_case("swap_example", "swap_items(0, 2)", ["a", "b", "c"], "swap_items(&mut v, 0, 2)", ["c", "b", "a"]),
     sv_case("swap_same_slot", "swap_items(1, 1)", ["a", "b"], "swap_items(&mut v, 1, 1)", ["a", "b"]),
     sv_case("rotate3_example", "rotate3(0, 1, 2)", ["x", "y", "z"], "rotate3(&mut v, 0, 1, 2)", ["y", "z", "x"]),
     sv_case("append_copy_example", "append_copy(0, 1)", ["ab", "cd"], "append_copy(&mut v, 0, 1)", ["abcd", "cd"]),
     sv_case("append_copy_backwards", "append_copy(1, 0)", ["ab", "cd"], "append_copy(&mut v, 1, 0)", ["ab", "cdab"]),
     sv_case("append_to_itself", "append_copy(0, 0)", ["ab"], "append_copy(&mut v, 0, 0)", ["abab"])],
    [sv_case("swap_backwards", "swap_items(2, 0)", ["a", "b", "c"], "swap_items(&mut v, 2, 0)", ["c", "b", "a"]),
     sv_case("swap_adjacent", "swap_items(0, 1)", ["x", "y"], "swap_items(&mut v, 0, 1)", ["y", "x"]),
     sv_case("swap_single", "swap_items(0, 0)", ["only"], "swap_items(&mut v, 0, 0)", ["only"]),
     sv_case("rotate3_reversed_indices", "rotate3(2, 1, 0)", ["x", "y", "z"], "rotate3(&mut v, 2, 1, 0)", ["z", "x", "y"]),
     sv_case("rotate3_spread", "rotate3(0, 3, 1)", ["a", "b", "c", "d"], "rotate3(&mut v, 0, 3, 1)", ["d", "a", "c", "b"]),
     sv_case("append_empty", "append_copy(0, 1)", ["a", ""], "append_copy(&mut v, 0, 1)", ["a", ""]),
     sv_case("append_empty_self", "append_copy(1, 1)", ["a", ""], "append_copy(&mut v, 1, 1)", ["a", ""]),
     sv_case("append_unicode", "append_copy(2, 0)", ["日本", "-", "x"], "append_copy(&mut v, 2, 0)", ["日本", "-", "x日本"]),
     T("strings_moved_not_copied", "swap_items(0, 1) moves the Strings", "(v[0].as_ptr() == p1, v[1].as_ptr() == p0)", "(true, true)",
       setup='let mut v = ["first", "second"].map(String::from);\nlet (p0, p1) = (v[0].as_ptr(), v[1].as_ptr());\nswap_items(&mut v, 0, 1);'),
     r"""
     #[test]
     fn random_vs_model() {
         let mut rng = anneal_prelude::Rng::new(6227);
         for _ in 0..300 {
             let n = 3 + rng.below(3);
             let mut v: Vec<String> = Vec::new();
             for _ in 0..n {
                 let len = 1 + rng_len(&mut rng);
                 v.push(rng.string(len, "ab"));
             }
             let (i, j) = (rng.below(n), rng.below(n));
             let mut got = v.clone();
             swap_items(&mut got, i, j);
             let mut want = v.clone();
             want.swap(i, j);
             check!(format!("{v:?}; swap_items({i}, {j})"), got, want);
             let mut got = v.clone();
             append_copy(&mut got, i, j);
             let mut want = v.clone();
             let add = v[j].clone();
             want[i].push_str(&add);
             check!(format!("{v:?}; append_copy({i}, {j})"), got, want);
             let mut idx: Vec<usize> = (0..n).collect();
             rng.shuffle(&mut idx);
             let (a, b, c) = (idx[0], idx[1], idx[2]);
             let mut got = v.clone();
             rotate3(&mut got, a, b, c);
             let mut want = v.clone();
             want[a] = v[b].clone();
             want[b] = v[c].clone();
             want[c] = v[a].clone();
             check!(format!("{v:?}; rotate3({a}, {b}, {c})"), got, want);
         }
     }

     fn rng_len(rng: &mut anneal_prelude::Rng) -> usize {
         rng.below(3)
     }
     """],
    [("rust", "`let a = &mut v[i]; let b = &mut v[j];` is two live `&mut` into `v`, which is never allowed, whatever the indices. Do you need both at once? `mem::take(&mut v[i])` is one short borrow; `mem::replace(&mut v[j], x)` puts `x` in and hands back what was there."),
     ("rust", "Sequencing through `take` breaks when `i == j`: the first `take` empties the slot you're about to read. Test for it."),
     ("rust", "`append_copy` really does need `v[i]` and `v[j]` alive together (appending without copying `v[j]` first): split the slice. When `i == j` there's nothing to split; `String::extend_from_within(..)` appends a copy of the string to itself.")],
    ("""Moving values between slots doesn't need two live borrows: `take` moves a value out through a borrow that ends at the semicolon, and `replace` swaps a value in and returns the old one, so a swap is `let tmp = take(&mut v[i]); v[i] = replace(&mut v[j], tmp);` and a rotation is a chain of takes. The catch is aliasing by value: with `i == j` the first `take` empties the only slot, so the result is an empty string unless you return early. (`slice::swap` handles it with raw pointers inside std.) Reading one slot while growing another is different: both must be alive, so split the slice at the larger index. `i == j` is again special, since a `String` can't be borrowed shared and unique at once, and `extend_from_within(..)` copies within its own buffer.

Syntax to remember: `let tmp = std::mem::take(&mut v[i]);` · `v[i] = std::mem::replace(&mut v[j], tmp);` · `let (left, right) = v.split_at_mut(hi);` · `s.extend_from_within(..)`.""", "O(1) swaps; O(len) appends", "O(1) extra"),
    "`mem::swap(&mut a.x, &mut b.x)` compiles when `a` and `b` are different variables. When does the same code on `v[i].x` and `v[j].x` need a split instead?",
    ["`take` and `replace` move values through short, sequential borrows.", "Sequenced moves break when two indices are equal.", "Split only when two borrows must be alive together."],
    rules=dict(methods=["swap", "clone", "to_string", "to_owned", "rotate_left", "rotate_right", "swap_with_slice", "get_disjoint_mut", "cloned"]),
    wrong=dict(
        swap_loses_same_slot=sub(SWAP_SOLUTION, "    if i == j {\n        return;\n    }\n    let tmp = take(&mut v[i]);", "    let tmp = take(&mut v[i]);"),
        self_append_via_take=sub(SWAP_SOLUTION, "        v[i].extend_from_within(..);\n", "        let s = take(&mut v[j]);\n        v[i].push_str(&s);\n        v[j] = s;\n"),
        rotate3_backwards=sub(SWAP_SOLUTION, "    let first = take(&mut v[i]);\n    v[i] = take(&mut v[j]);\n    v[j] = take(&mut v[k]);\n    v[k] = first;",
                              "    let first = take(&mut v[k]);\n    v[k] = take(&mut v[j]);\n    v[j] = take(&mut v[i]);\n    v[i] = first;"),
    ),
))

DOC_HEAD = r"""
pub struct Document {
    pub title: String,
    pub tags: Vec<String>,
    pub lines: Vec<String>,
    pub words: usize,
}

/// The title and tags, borrowed mutably together.
pub struct Header<'a> {
    pub title: &'a mut String,
    pub tags: &'a mut Vec<String>,
}

/// The lines and word count, borrowed mutably together.
pub struct Body<'a> {
    pub lines: &'a mut Vec<String>,
    pub words: &'a mut usize,
}
"""

DOC_DOCS = dict(
    header="    pub fn header(&mut self) -> Header<'_> {\n",
    body="    pub fn body(&mut self) -> Body<'_> {\n",
    split="    /// Both views at once.\n    pub fn split(&mut self) -> (Header<'_>, Body<'_>) {\n",
    tag="    /// Adds `tag` unless it's already there. Returns whether it was added.\n    pub fn tag(&mut self, tag: &str) -> bool {\n",
    retitle="    /// Sets the title and adds the tag \"edited\" (once).\n    pub fn retitle(&mut self, title: &str) {\n",
    push="    /// Appends a line and adds its whitespace-separated words to the count.\n    pub fn push_line(&mut self, line: &str) {\n",
    hashtags="/// Adds every word of the body that starts with '#' as a tag, without the '#' (skipping a bare \"#\"), in order,\n/// unless it's already a tag. Returns how many tags were added.\npub fn hashtags(doc: &mut Document) -> usize {\n",
)

DOC_BODIES = dict(
    header="        Header { title: &mut self.title, tags: &mut self.tags }\n",
    body="        Body { lines: &mut self.lines, words: &mut self.words }\n",
    split="        let Document { title, tags, lines, words } = self;\n        (Header { title, tags }, Body { lines, words })\n",
    tag="        if self.tags.iter().any(|t| t == tag) {\n            return false;\n        }\n        self.tags.push(tag.to_string());\n        true\n",
    retitle="        self.title.clear();\n        self.title.push_str(title);\n        self.tag(\"edited\");\n",
    push="        *self.words += line.split_whitespace().count();\n        self.lines.push(line.to_string());\n",
    hashtags="""    let (mut header, body) = doc.split();
    let mut added = 0;
    for line in body.lines.iter() {
        for word in line.split_whitespace() {
            if let Some(tag) = word.strip_prefix('#').filter(|t| !t.is_empty()) {
                if header.tag(tag) {
                    added += 1;
                }
            }
        }
    }
    added
""",
)


def doc_src(bodies):
    out = DOC_HEAD + "\nimpl Document {\n"
    for k in ("header", "body", "split"):
        out += DOC_DOCS[k] + bodies[k] + "    }\n\n"
    out = out.rstrip("\n") + "\n}\n\nimpl Header<'_> {\n"
    for k in ("tag", "retitle"):
        out += DOC_DOCS[k] + bodies[k] + "    }\n\n"
    out = out.rstrip("\n") + "\n}\n\nimpl Body<'_> {\n" + DOC_DOCS["push"] + bodies["push"] + "    }\n}\n\n"
    return out + DOC_DOCS["hashtags"] + bodies["hashtags"] + "}\n"


DOC_SOLUTION = doc_src(DOC_BODIES)
DOC_STARTER = doc_src({k: ("    todo!()\n" if k == "hashtags" else "        todo!()\n") for k in DOC_BODIES})
DOC_NEW = 'let mut d = Document { title: "Draft".to_string(), tags: vec![], lines: vec![], words: 0 };'

P.append(write(
    "view-struct", "View structs over disjoint fields", "medium", "split-borrows", ["view structs", "lifetimes", "destructuring", "E0499"],
    """
        `Header` and `Body` are views: each bundles `&mut` borrows of some of `Document`'s fields. Write the
        methods that make them and use them, and `hashtags`, which reads the body while it adds tags to the
        header. Two views made by two `&mut self` calls can't be alive together; `split` has to make both.
    """,
    DOC_STARTER,
    DOC_SOLUTION,
    [T("views_together", "split; push a line and retitle while both are alive", "(d.title.as_str(), d.tags.clone(), d.lines.clone(), d.words)", '("Final", vec!["edited".to_string()], vec!["two words".to_string()], 2)',
       setup=DOC_NEW + '\n{\n    let (mut h, mut b) = d.split();\n    b.push_line("two words");\n    h.retitle("Final");\n}'),
     T("hashtags_example", "lines \"see #rust and #borrowck\", \"#rust again #\"", "(hashtags(&mut d), d.tags.clone())", '(2, vec!["rust".to_string(), "borrowck".to_string()])',
       setup=DOC_NEW + '\nd.body().push_line("see #rust and #borrowck");\nd.body().push_line("#rust again #");'),
     T("retitle_tags_once", "retitle twice", "(d.title.as_str(), d.tags.clone())", '("B", vec!["edited".to_string()])', setup=DOC_NEW + '\nd.header().retitle("A");\nd.header().retitle("B");'),
     T("tag_reports", "tag x, x, y", "{ let mut h = d.header(); (h.tag(\"x\"), h.tag(\"x\"), h.tag(\"y\")) }", "(true, false, true)", setup=DOC_NEW),
     T("word_count", "push \"a b  c\", \"\", \" d \"", "(d.words, d.lines.len())", "(4, 3)", setup=DOC_NEW + '\nlet mut b = d.body();\nb.push_line("a b  c");\nb.push_line("");\nb.push_line(" d ");')],
    [T("hashtags_existing_tag", "tags [rust]; line \"#rust #go\"", "(hashtags(&mut d), d.tags.clone())", '(1, vec!["rust".to_string(), "go".to_string()])', setup=DOC_NEW + '\nd.tags.push("rust".to_string());\nd.body().push_line("#rust #go");'),
     T("hashtags_none", "line \"no tags here\"", "(hashtags(&mut d), d.tags.len())", "(0, 0)", setup=DOC_NEW + '\nd.body().push_line("no tags here");'),
     T("hashtags_empty_doc", "empty document", "hashtags(&mut d)", "0", setup=DOC_NEW),
     T("hashtag_mid_word_ignored", "line \"a#b ##c\"", "(hashtags(&mut d), d.tags.clone())", '(1, vec!["#c".to_string()])', setup=DOC_NEW + '\nd.body().push_line("a#b ##c");'),
     T("hashtag_unicode", "line \"#日本 #é\"", "d.tags.clone()", '["日本", "é"].map(String::from).to_vec()', setup=DOC_NEW + '\nd.body().push_line("#日本 #é");\nhashtags(&mut d);'),
     T("retitle_empty", "retitle \"\"", "(d.title.as_str(), d.tags.len())", '("", 1)', setup=DOC_NEW + '\nd.header().retitle("");'),
     T("edited_already_there", "tags [edited]; retitle", "d.tags.clone()", 'vec!["edited".to_string()]', setup=DOC_NEW + '\nd.tags.push("edited".to_string());\nd.header().retitle("x");'),
     T("words_accumulate", "words 5; push \"x y\"", "d.words", "7", setup=DOC_NEW + '\nd.words = 5;\nd.body().push_line("x y");'),
     r"""
     #[test]
     fn random_vs_model() {
         let mut rng = anneal_prelude::Rng::new(6228);
         for _ in 0..300 {
             let mut d = Document { title: String::new(), tags: vec![], lines: vec![], words: 0 };
             let mut lines = Vec::new();
             for _ in 0..rng_len(&mut rng) {
                 let len = rng.below(8);
                 lines.push(rng.string(len, "#ab "));
             }
             for l in &lines {
                 d.body().push_line(l);
             }
             let mut tags: Vec<String> = Vec::new();
             let mut added = 0;
             for l in &lines {
                 for w in l.split_whitespace() {
                     if let Some(t) = w.strip_prefix('#') {
                         if !t.is_empty() && !tags.iter().any(|x| x == t) {
                             tags.push(t.to_string());
                             added += 1;
                         }
                     }
                 }
             }
             let words: usize = lines.iter().map(|l| l.split_whitespace().count()).sum();
             let got = hashtags(&mut d);
             check!(format!("lines {lines:?}"), (got, d.tags.clone(), d.words), (added, tags, words));
         }
     }

     fn rng_len(rng: &mut anneal_prelude::Rng) -> usize {
         rng.below(5)
     }
     """],
    [("rust", "`header` and `body` are one struct literal each, borrowing two fields: `Header { title: &mut self.title, tags: &mut self.tags }`."),
     ("rust", "`let h = doc.header(); let b = doc.body();` is two `&mut self` borrows alive together (E0499), even though the fields differ: a method borrows all of `self`. `split` makes both views from one borrow; destructure `self` into its fields first."),
     ("rust", "A view's methods reach the fields through its references: `self.title.push_str(..)` auto-derefs, `*self.words += n` needs the explicit `*`.")],
    ("""A view struct names a set of disjoint field borrows so they can travel together: into a function, into a method with its own name, back out of a method. A `&mut self` method that returns one view borrows all of `self`, though, so two such calls can't overlap. `split` is where the disjointness gets proven: `let Document { title, tags, lines, words } = self;` produces four independent `&mut` borrows, and they go into two views that live side by side. `hashtags` then reads `body.lines` while `header.tag(..)` pushes to the tags, which the compiler accepts because the two views share nothing. The views' lifetime `'_` ties them to the one `&mut self` borrow, so the document is unusable while either lives.

Syntax to remember: `pub fn split(&mut self) -> (Header<'_>, Body<'_>) { let Document { title, tags, lines, words } = self; (Header { title, tags }, Body { lines, words }) }` · `impl Header<'_> { .. }` · `*self.words += n`.""", "O(total words) for hashtags (O(tags) per check)", "O(1) extra"),
    "When is a view struct better than a method that takes several `&mut` field parameters?",
    ["A view struct bundles disjoint field borrows.", "Two `&mut self` calls can't hand out views that coexist; one `split` can.", "Destructure `self` to prove disjointness."],
    related=("L2", "L3"),
    wrong=dict(
        retitle_tags_every_time=sub(DOC_SOLUTION, '        self.tag("edited");\n', '        self.tags.push("edited".to_string());\n'),
        keeps_the_hash=sub(DOC_SOLUTION, "if let Some(tag) = word.strip_prefix('#').filter(|t| !t.is_empty()) {", "if let Some(tag) = Some(word).filter(|w| w.len() > 1 && w.starts_with('#')) {"),
        counts_lines_not_words=sub(DOC_SOLUTION, "*self.words += line.split_whitespace().count();", "*self.words += 1;"),
    ),
))

SHOP_STARTER = r"""
pub mod shop {
    pub struct Item {
        pub name: String,
        pub price: u32,
    }

    pub struct Shop {
        items: Vec<Item>,
        log: Vec<String>,
        discount: u32,
    }

    impl Shop {
        pub fn new(items: Vec<Item>, discount: u32) -> Self {
            Shop { items, log: Vec::new(), discount }
        }

        pub fn items(&self) -> &[Item] {
            &self.items
        }

        pub fn items_mut(&mut self) -> &mut [Item] {
            &mut self.items
        }

        pub fn log(&self) -> &[String] {
            &self.log
        }

        pub fn log_mut(&mut self) -> &mut Vec<String> {
            &mut self.log
        }

        pub fn discount(&self) -> u32 {
            self.discount
        }

        /// Takes `discount` percent (rounded down) off every item priced at least `min`, logging
        /// "<name>: <old> -> <new>". Returns how many items changed price.
        pub fn sale(&mut self, min: u32) -> usize {
            let mut n = 0;
            for it in self.items_mut() {
                if it.price >= min {
                    let new = it.price - it.price * self.discount() / 100;
                    self.log_mut().push(format!("{}: {} -> {new}", it.name, it.price));
                    if new != it.price {
                        n += 1;
                    }
                    it.price = new;
                }
            }
            n
        }
    }
}

pub use shop::Shop;

/// Logs "expensive: <name>" in the shop's log for every item priced above `limit`, in order. Returns how many.
pub fn flag_expensive(shop: &mut Shop, limit: u32) -> usize {
    let mut n = 0;
    for it in shop.items() {
        if it.price > limit {
            shop.log_mut().push(format!("expensive: {}", it.name));
            n += 1;
        }
    }
    n
}
"""

SHOP_SOLUTION = SHOP_STARTER
for _old, _new in [
    ("            for it in self.items_mut() {\n", "            for it in self.items.iter_mut() {\n"),
    ("                    let new = it.price - it.price * self.discount() / 100;\n                    self.log_mut().push(", "                    let new = it.price - it.price * self.discount / 100;\n                    self.log.push("),
    ("        pub fn discount(&self) -> u32 {\n            self.discount\n        }\n",
     "        pub fn discount(&self) -> u32 {\n            self.discount\n        }\n\n        /// The items and the log, borrowed together.\n        pub fn items_and_log(&mut self) -> (&[Item], &mut Vec<String>) {\n            (&self.items, &mut self.log)\n        }\n"),
    ("    let mut n = 0;\n    for it in shop.items() {\n        if it.price > limit {\n            shop.log_mut().push(", "    let mut n = 0;\n    let (items, log) = shop.items_and_log();\n    for it in items {\n        if it.price > limit {\n            log.push("),
]:
    SHOP_SOLUTION = sub(SHOP_SOLUTION, _old, _new)

SHOP_NEW = 'let mut s = Shop::new(vec![shop::Item { name: "pen".into(), price: 5 }, shop::Item { name: "lamp".into(), price: 40 }, shop::Item { name: "desk".into(), price: 300 }], 10);'
SHOP_DESC = "pen 5, lamp 40, desk 300; discount 10%"

P.append(fix(
    "fix-borrow-through-getter", "Fix: getters borrow all of self", "medium", "split-borrows", ["E0502", "E0499", "getters", "privacy", "split accessors"],
    """
        `sale` and `flag_expensive` don't compile: each borrows the shop through one accessor while calling
        another. `flag_expensive` lives outside the `shop` module, so it can't touch the private fields. Fix
        both without cloning or collecting anything. You may add one method to `Shop`.
    """,
    SHOP_STARTER,
    SHOP_SOLUTION,
    [T("flag_expensive_example", SHOP_DESC + "; flag above 30", "(flag_expensive(&mut s, 30), s.log().to_vec())", '(2, vec!["expensive: lamp".to_string(), "expensive: desk".to_string()])', setup=SHOP_NEW),
     T("sale_example", SHOP_DESC + "; sale from 40", "(s.sale(40), s.items().iter().map(|i| i.price).collect::<Vec<_>>(), s.log().to_vec())", '(2, vec![5, 36, 270], vec!["lamp: 40 -> 36".to_string(), "desk: 300 -> 270".to_string()])', setup=SHOP_NEW),
     T("sale_rounds_down", "price 15, discount 10%", "(s.sale(0), s.items()[0].price)", "(1, 14)", setup='let mut s = Shop::new(vec![shop::Item { name: "x".into(), price: 15 }], 10);'),
     T("unchanged_price_not_counted", "price 5, discount 10%", "(s.sale(0), s.log().to_vec())", '(0, vec!["x: 5 -> 5".to_string()])', setup='let mut s = Shop::new(vec![shop::Item { name: "x".into(), price: 5 }], 10);'),
     T("limit_is_strict", SHOP_DESC + "; flag above 40", "flag_expensive(&mut s, 40)", "1", setup=SHOP_NEW)],
    [T("empty_shop", "no items", "(flag_expensive(&mut s, 0), s.sale(0), s.log().len())", "(0, 0, 0)", setup="let mut s = Shop::new(vec![], 50);"),
     T("sale_then_flag", SHOP_DESC + "; sale from 0; flag above 35", "(flag_expensive(&mut s, 35), s.log().len())", "(2, 5)", setup=SHOP_NEW + "\ns.sale(0);"),
     T("zero_discount", "discount 0; sale", "(s.sale(0), s.items()[2].price)", "(0, 300)", setup=SHOP_NEW.replace("], 10);", "], 0);")),
     T("full_discount", "discount 100; sale from 40", "(s.sale(40), s.items().iter().map(|i| i.price).collect::<Vec<_>>())", "(2, vec![5, 0, 0])", setup=SHOP_NEW.replace("], 10);", "], 100);")),
     T("sale_twice", SHOP_DESC + "; sale from 300 twice", "(s.sale(300), s.sale(300), s.items()[2].price)", "(1, 0, 270)", setup=SHOP_NEW),
     T("big_price", "price u32::MAX / 100, discount 50", "(s.sale(0), s.items()[0].price)", "(1, 21474836)", setup='let mut s = Shop::new(vec![shop::Item { name: "x".into(), price: 42949672 }], 50);'),
     T("flag_none", SHOP_DESC + "; flag above 1000", "(flag_expensive(&mut s, 1000), s.log().len())", "(0, 0)", setup=SHOP_NEW),
     T("log_keeps_order", SHOP_DESC + "; flag above 0; flag above 100", "s.log().to_vec()", '["expensive: pen", "expensive: lamp", "expensive: desk", "expensive: desk"].map(String::from).to_vec()', setup=SHOP_NEW + "\nflag_expensive(&mut s, 0);\nflag_expensive(&mut s, 100);"),
     r"""
     #[test]
     fn random_vs_model() {
         let mut rng = anneal_prelude::Rng::new(6229);
         for _ in 0..300 {
             let n = rng.below(6);
             let prices: Vec<u32> = rng.vec(n, 0, 200);
             let disc = rng.below(101) as u32;
             let (min, limit) = (rng.below(200) as u32, rng.below(200) as u32);
             let mut s = Shop::new(prices.iter().enumerate().map(|(i, &p)| shop::Item { name: format!("i{i}"), price: p }).collect(), disc);
             let mut log = Vec::new();
             let mut changed = 0;
             let mut after = prices.clone();
             for (i, p) in after.iter_mut().enumerate() {
                 if *p >= min {
                     let new = *p - *p * disc / 100;
                     log.push(format!("i{i}: {p} -> {new}"));
                     if new != *p {
                         changed += 1;
                     }
                     *p = new;
                 }
             }
             let mut flagged = 0;
             for (i, p) in after.iter().enumerate() {
                 if *p > limit {
                     log.push(format!("expensive: i{i}"));
                     flagged += 1;
                 }
             }
             let got = (s.sale(min), flag_expensive(&mut s, limit));
             let got_prices: Vec<u32> = s.items().iter().map(|i| i.price).collect();
             check!(format!("prices {prices:?}, discount {disc}; sale({min}); flag_expensive({limit})"), (got, got_prices, s.log().to_vec()), ((changed, flagged), after, log));
         }
     }
     """],
    [("rust", "`self.items_mut()` borrows all of `self` for the whole loop, so `self.discount()` and `self.log_mut()` can't run inside it. The compiler doesn't look into a getter's body; its signature says \"all of `self`\"."),
     ("rust", "Inside the `impl`, use the fields: `self.items.iter_mut()`, `self.discount`, `self.log`. Field paths are disjoint."),
     ("rust", "Outside the module the fields are private, so the fix has to come from `Shop`: one method that returns `(&[Item], &mut Vec<String>)`, borrowing both fields in one call.")],
    ("""Borrow checking is per function and trusts signatures: `fn items(&self) -> &[Item]` means \"the result borrows all of `self`\", whatever the body touches. So a getter's result conflicts with every `&mut self` call, including `log_mut()`, even though the fields are disjoint. Inside the `impl`, field paths show the disjointness directly. Outside the module, privacy hides the fields, and the owner of the type has to export the split: a method returning several borrows from one `&mut self` call, (`&self.items`, `&mut self.log`), which the caller destructures. That is the pattern behind `slice::split_at_mut`, `HashMap::get_disjoint_mut` and view structs.

Syntax to remember: `pub fn items_and_log(&mut self) -> (&[Item], &mut Vec<String>) { (&self.items, &mut self.log) }` · `let (items, log) = shop.items_and_log();`.""", "O(n)", "O(1)"),
    "Could the compiler ever look inside `items_mut` to see that it only borrows one field? What would that cost?",
    ["A getter's signature borrows all of `self`.", "Inside the impl, borrow fields directly.", "Across a privacy boundary, export a method that splits the borrow."],
    rules=dict(methods=["clone", "collect", "to_vec", "to_owned", "cloned"]),
    wrong=dict(
        rounds_the_other_way=sub(SHOP_SOLUTION, "let new = it.price - it.price * self.discount / 100;", "let new = it.price * (100 - self.discount) / 100;"),
        flags_at_limit=sub(SHOP_SOLUTION, "        if it.price > limit {\n            log.push(", "        if it.price >= limit {\n            log.push("),
        counts_every_sale_item=sub(SHOP_SOLUTION, "                    if new != it.price {\n                        n += 1;\n                    }\n", "                    n += 1;\n"),
    ),
))

# ---------------------------------------------------------------- borrow-checker limits (hard)

CACHE_HEAD = r"""
use std::collections::HashMap;

pub struct Cache {
    map: HashMap<u32, String>,
    order: Vec<u32>,
    misses: usize,
}

impl Cache {
    pub fn new() -> Self {
        Cache { map: HashMap::new(), order: Vec::new(), misses: 0 }
    }

    /// Keys in the order they were first stored.
    pub fn order(&self) -> &[u32] {
        &self.order
    }

    pub fn misses(&self) -> usize {
        self.misses
    }
"""

CACHE_STARTER = CACHE_HEAD + r"""
    /// The value for `key`. If it's missing, counts a miss, records the key's order, and stores `make()`.
    /// `make` runs only on a miss.
    pub fn get_or_make(&mut self, key: u32, make: impl FnOnce() -> String) -> &String {
        if let Some(v) = self.map.get(&key) {
            return v;
        }
        self.misses += 1;
        self.order.push(key);
        self.map.insert(key, make());
        &self.map[&key]
    }

    /// Like `get_or_make`, for editing the value in place.
    pub fn get_or_make_mut(&mut self, key: u32, make: impl FnOnce() -> String) -> &mut String {
        match self.map.get_mut(&key) {
            Some(v) => v,
            None => {
                self.misses += 1;
                self.order.push(key);
                self.map.insert(key, make());
                self.map.get_mut(&key).unwrap()
            }
        }
    }
}
"""

CACHE_SOLUTION = CACHE_HEAD + r"""
    /// The value for `key`. If it's missing, counts a miss, records the key's order, and stores `make()`.
    /// `make` runs only on a miss.
    pub fn get_or_make(&mut self, key: u32, make: impl FnOnce() -> String) -> &String {
        if !self.map.contains_key(&key) {
            self.misses += 1;
            self.order.push(key);
            self.map.insert(key, make());
        }
        &self.map[&key]
    }

    /// Like `get_or_make`, for editing the value in place.
    pub fn get_or_make_mut(&mut self, key: u32, make: impl FnOnce() -> String) -> &mut String {
        if !self.map.contains_key(&key) {
            self.misses += 1;
            self.order.push(key);
            self.map.insert(key, make());
        }
        self.map.get_mut(&key).unwrap()
    }
}
"""

CA = "let mut c = Cache::new();"

P.append(fix(
    "fix-get-or-insert", "Fix: get-or-insert without the entry API", "hard", "borrow-checker-limits", ["NLL problem case 3", "E0502", "E0499", "Polonius"],
    """
        Neither method compiles, though both are correct: this is a known limit of today's borrow checker.
        Fix them without the entry API, without calling `make` on a hit, and without cloning any value.
    """,
    CACHE_STARTER,
    CACHE_SOLUTION,
    [T("hit_and_miss", "get_or_make 1 (\"one\"), then 1 again (\"uno\")", '(c.get_or_make(1, || "one".to_string()).clone(), c.get_or_make(1, || "uno".to_string()).clone(), c.misses())', '("one".to_string(), "one".to_string(), 1)', setup=CA),
     T("make_runs_only_on_a_miss", "get_or_make 5 twice, counting calls to make", "calls", "1",
       setup=CA + '\nlet mut calls = 0;\nc.get_or_make(5, || { calls += 1; "x".to_string() });\nc.get_or_make(5, || { calls += 1; "y".to_string() });'),
     T("edit_in_place", "get_or_make_mut 2 (\"a\") += \"b\", then += \"c\"", 'c.get_or_make(2, String::new).clone()', '"abc".to_string()',
       setup=CA + '\nc.get_or_make_mut(2, || "a".to_string()).push(\'b\');\nc.get_or_make_mut(2, || "z".to_string()).push(\'c\');'),
     T("order_of_first_store", "keys 3, 1, 3, 2, 1", "(c.order().to_vec(), c.misses())", "(vec![3, 1, 2], 3)",
       setup=CA + "\nfor k in [3, 1, 3, 2, 1] {\n    c.get_or_make(k, || k.to_string());\n}"),
     T("empty_value", "get_or_make 0 (\"\")", 'c.get_or_make(0, String::new).len()', "0", setup=CA)],
    [T("mut_then_shared", "get_or_make_mut 4 = \"w\", get_or_make 4", 'c.get_or_make(4, || "no".to_string()).clone()', '"w".to_string()', setup=CA + '\n*c.get_or_make_mut(4, String::new) = "w".to_string();'),
     T("mut_hit_keeps_value", "get_or_make 6 (\"k\"), get_or_make_mut 6 (\"no\")", 'c.get_or_make_mut(6, || "no".to_string()).clone()', '"k".to_string()', setup=CA + '\nc.get_or_make(6, || "k".to_string());'),
     T("mut_make_once", "get_or_make_mut 7 twice, counting make", "calls", "1", setup=CA + '\nlet mut calls = 0;\nc.get_or_make_mut(7, || { calls += 1; String::new() });\nc.get_or_make_mut(7, || { calls += 1; String::new() });'),
     T("misses_mixed", "keys 1, 2 via get_or_make, 2, 3 via get_or_make_mut", "(c.misses(), c.order().to_vec())", "(3, vec![1, 2, 3])",
       setup=CA + "\nc.get_or_make(1, String::new);\nc.get_or_make(2, String::new);\nc.get_or_make_mut(2, String::new);\nc.get_or_make_mut(3, String::new);"),
     T("key_zero_and_max", "keys 0 and u32::MAX", '(c.get_or_make(0, || "lo".to_string()).clone(), c.get_or_make(u32::MAX, || "hi".to_string()).clone())', '("lo".to_string(), "hi".to_string())', setup=CA),
     T("returned_value_is_stored", "the &String from get_or_make_mut is the stored one", "p == c.get_or_make(9, String::new).as_ptr()", "true",
       setup=CA + '\nlet p = c.get_or_make_mut(9, || "stored".to_string()).as_ptr();'),
     T("new_cache", "new cache", "(c.misses(), c.order().len())", "(0, 0)", setup=CA),
     T("unicode_value", "get_or_make 1 (\"日本\")", 'c.get_or_make(1, || "日本".to_string()).clone()', '"日本".to_string()', setup=CA),
     r"""
     #[test]
     fn random_vs_model() {
         let mut rng = anneal_prelude::Rng::new(6230);
         for _ in 0..300 {
             let mut c = Cache::new();
             let mut model: Vec<(u32, String)> = Vec::new();
             let mut misses = 0;
             let mut ops = Vec::new();
             for step in 0..10 {
                 let k = rng.below(4) as u32;
                 let made = format!("v{step}");
                 if model.iter().all(|e| e.0 != k) {
                     misses += 1;
                     model.push((k, made.clone()));
                 }
                 let want = model.iter().find(|e| e.0 == k).unwrap().1.clone();
                 if rng.bool() {
                     ops.push(format!("get_or_make({k})"));
                     check!(ops.join(", "), c.get_or_make(k, || made).clone(), want);
                 } else {
                     ops.push(format!("get_or_make_mut({k}) += \"!\""));
                     let v = c.get_or_make_mut(k, || made);
                     check!(ops.join(", "), v.clone(), want);
                     v.push('!');
                     model.iter_mut().find(|e| e.0 == k).unwrap().1.push('!');
                 }
             }
             let order: Vec<u32> = model.iter().map(|e| e.0).collect();
             check!(format!("{}; misses, order", ops.join(", ")), (c.misses(), c.order().to_vec()), (misses, order));
         }
     }

     #[test]
     fn many_keys() {
         let mut c = Cache::new();
         for i in 0..200_000u32 {
             c.get_or_make(i % 50_000, || i.to_string());
         }
         check!("200000 lookups over 50000 keys", (c.misses(), c.get_or_make(49_999, String::new).clone()), (50_000, "49999".to_string()));
     }
     """],
    [("rust", "Returning `v` from inside `if let Some(v) = self.map.get(&key)` makes that borrow last for the whole output lifetime. NLL doesn't track that it's only on the returning path, so the borrow counts as live on the insert path too."),
     ("rust", "The `match` version fails for the same reason: the `Some` arm returns the borrow, so the scrutinee's borrow is live in the `None` arm."),
     ("rust", "Decide with something that returns a `bool` (`contains_key`), mutate if needed, then look the key up once more and return that borrow.")],
    ("""This is NLL's \"problem case #3\": when a borrow is conditionally returned, the region of the returned reference must cover the whole rest of the function, and NLL applies that region to the borrow on every path, including the path that doesn't return. So the `insert` counts as a conflict. Polonius, the next borrow checker (`-Zpolonius` on nightly), reasons per path and accepts the original.

The standard workaround separates the decision from the borrow: `contains_key` returns a `bool`, so nothing is borrowed across the insert, and one final lookup produces the returned reference. It costs an extra hash lookup on hits. The entry API avoids that (`entry(key).or_insert_with(make)`) because the entry owns the decision, which is why it exists. Returning something that isn't a borrow (a `bool`, a copy, an index) from the early branch also compiles; only a returned *reference* triggers the problem.

Syntax to remember: `if !self.map.contains_key(&key) { self.map.insert(key, make()); } &self.map[&key]` · `self.map.get_mut(&key).unwrap()`.""", "O(1) expected", "O(1)"),
    "Rewrite `get_or_make` with the entry API. What does `or_insert_with` do that the two-lookup version can't?",
    ["NLL problem case 3: a conditionally returned borrow is live on every path.", "Decide with a `bool`, mutate, then borrow once for the return.", "Polonius accepts the original."],
    rules=dict(methods=["entry", "clone", "cloned", "to_owned"]),
    wrong=dict(
        counts_every_call=sub(CACHE_SOLUTION, "    pub fn get_or_make(&mut self, key: u32, make: impl FnOnce() -> String) -> &String {\n        if !self.map.contains_key(&key) {\n            self.misses += 1;",
                              "    pub fn get_or_make(&mut self, key: u32, make: impl FnOnce() -> String) -> &String {\n        self.misses += 1;\n        if !self.map.contains_key(&key) {"),
        mut_overwrites=sub(CACHE_SOLUTION, "        if !self.map.contains_key(&key) {\n            self.misses += 1;\n            self.order.push(key);\n            self.map.insert(key, make());\n        }\n        self.map.get_mut(&key).unwrap()",
                           "        let v = make();\n        if !self.map.contains_key(&key) {\n            self.misses += 1;\n            self.order.push(key);\n        }\n        self.map.insert(key, v);\n        self.map.get_mut(&key).unwrap()"),
        order_on_every_call=sub(CACHE_SOLUTION, "        if !self.map.contains_key(&key) {\n            self.misses += 1;\n            self.order.push(key);\n            self.map.insert(key, make());\n        }\n        &self.map[&key]",
                                "        self.order.push(key);\n        if !self.map.contains_key(&key) {\n            self.misses += 1;\n            self.map.insert(key, make());\n        }\n        &self.map[&key]"),
    ),
))

TRIE_HEAD = r"""
use std::collections::HashMap;

#[derive(Default)]
pub struct Node {
    pub count: u32,
    pub kids: HashMap<char, Node>,
}
"""

TRIE_STARTER = TRIE_HEAD + r"""
/// The node reached by following `path` from `root` as far as it exists (the root when even the first char
/// is missing).
pub fn deepest_mut<'a>(root: &'a mut Node, path: &str) -> &'a mut Node {
    let mut cur = root;
    for c in path.chars() {
        match cur.kids.get_mut(&c) {
            Some(next) => cur = next,
            None => return cur,
        }
    }
    cur
}

/// Extends the trie by one node along `path` from the deepest existing node, bumps that node's count, and
/// returns it. When the whole path already exists, bumps its end node.
pub fn grow<'a>(root: &'a mut Node, path: &str) -> &'a mut Node {
    let depth = depth_of(root, path);
    let node = deepest_mut(root, path);
    let node = match path.chars().nth(depth) {
        Some(c) => node.kids.entry(c).or_default(),
        None => node,
    };
    node.count += 1;
    node
}

fn depth_of(root: &Node, path: &str) -> usize {
    let mut cur = root;
    let mut depth = 0;
    for c in path.chars() {
        match cur.kids.get(&c) {
            Some(next) => cur = next,
            None => break,
        }
        depth += 1;
    }
    depth
}

/// The first word longer than `n` bytes; otherwise pushes "fallback" and returns that.
pub fn first_long_or_push(words: &mut Vec<String>, n: usize) -> &mut String {
    for w in words.iter_mut() {
        if w.len() > n {
            return w;
        }
    }
    words.push("fallback".to_string());
    words.last_mut().unwrap()
}
"""

TRIE_SOLUTION = sub(sub(TRIE_STARTER, r"""    let mut cur = root;
    for c in path.chars() {
        match cur.kids.get_mut(&c) {
            Some(next) => cur = next,
            None => return cur,
        }
    }
    cur
""", r"""    let mut cur = root;
    for c in path.chars() {
        if !cur.kids.contains_key(&c) {
            break;
        }
        cur = cur.kids.get_mut(&c).unwrap();
    }
    cur
"""), r"""    for w in words.iter_mut() {
        if w.len() > n {
            return w;
        }
    }
    words.push("fallback".to_string());
    words.last_mut().unwrap()
""", r"""    match words.iter().position(|w| w.len() > n) {
        Some(i) => &mut words[i],
        None => {
            words.push("fallback".to_string());
            words.last_mut().unwrap()
        }
    }
""")

TRIE_HELPER = r"""
fn build(words: &[&str]) -> Node {
    let mut root = Node::default();
    for w in words {
        let mut cur = &mut root;
        for c in w.chars() {
            cur = cur.kids.entry(c).or_default();
        }
        cur.count += 1;
    }
    root
}

fn depth(root: &mut Node, path: &str) -> usize {
    let target: *const Node = deepest_mut(root, path);
    let mut cur: &Node = root;
    let mut d = 0;
    for c in path.chars() {
        if std::ptr::eq(cur, target) {
            return d;
        }
        match cur.kids.get(&c) {
            Some(next) => cur = next,
            None => return usize::MAX,
        }
        d += 1;
    }
    if std::ptr::eq(cur, target) { d } else { usize::MAX }
}

fn words(xs: &[&str]) -> Vec<String> {
    xs.iter().map(|s| s.to_string()).collect()
}
"""

P.append(fix(
    "fix-conditional-return-of-borrow", "Fix: return a borrow on one path, mutate on another", "hard", "borrow-checker-limits", ["NLL problem case 3", "Polonius", "E0499", "trie descent"],
    """
        `deepest_mut` and `first_long_or_push` are correct and don't compile: each returns a mutable borrow from
        inside a loop on one path and keeps using the same data on another. Fix both without `unsafe`, without
        cloning, and without changing any signature. `grow` depends on `deepest_mut`.
    """,
    TRIE_STARTER,
    TRIE_SOLUTION,
    [TRIE_HELPER,
     T("deepest_example", "trie {car, cat}; path \"cart\"", 'depth(&mut root, "cart")', "3", setup='let mut root = build(&["car", "cat"]);'),
     T("deepest_missing_first", "trie {car}; path \"dog\"", 'depth(&mut root, "dog")', "0", setup='let mut root = build(&["car"]);'),
     T("grow_one_node_at_a_time", "trie {}; grow \"ab\" three times", '(grow(&mut root, "ab").count, grow(&mut root, "ab").count, grow(&mut root, "ab").count)', "(1, 1, 2)", setup="let mut root = Node::default();"),
     T("first_long", "[\"a\", \"long\", \"longer\"], n 2: append \"!\"", '{ first_long_or_push(&mut v, 2).push(\'!\'); v }', 'words(&["a", "long!", "longer"])', setup='let mut v = words(&["a", "long", "longer"]);'),
     T("fallback", "[\"a\"], n 5: append \"?\"", '{ first_long_or_push(&mut v, 5).push(\'?\'); v }', 'words(&["a", "fallback?"])', setup='let mut v = words(&["a"]);'),
     T("empty_path", "trie {a}; path \"\"", 'depth(&mut root, "")', "0", setup='let mut root = build(&["a"]);')],
    [TRIE_HELPER,
     T("full_path_exists", "trie {abc}; path \"abc\"", 'depth(&mut root, "abc")', "3", setup='let mut root = build(&["abc"]);'),
     T("deepest_is_mutable", "trie {ab}; deepest(\"abz\").count += 5; then count of \"ab\"", '{ deepest_mut(&mut root, "abz").count += 5; deepest_mut(&mut root, "ab").count }', "6", setup='let mut root = build(&["ab"]);'),
     T("grow_existing_path", "trie {ab}; grow \"ab\"", 'grow(&mut root, "ab").count', "2", setup='let mut root = build(&["ab"]);'),
     T("grow_empty_path", "trie {}; grow \"\"", '(grow(&mut root, "").count, root.kids.len())', "(1, 0)", setup="let mut root = Node::default();"),
     T("unicode_path", "trie {日本}; path \"日本語\"", 'depth(&mut root, "日本語")', "2", setup='let mut root = build(&["日本"]);'),
     T("first_long_empty", "[], n 0", '{ first_long_or_push(&mut v, 0); v }', 'words(&["fallback"])', setup="let mut v: Vec<String> = vec![];"),
     T("first_long_is_strict", "[\"ab\"], n 2", '{ first_long_or_push(&mut v, 2); v }', 'words(&["ab", "fallback"])', setup='let mut v = words(&["ab"]);'),
     T("first_long_is_first", "[\"xyz\", \"abc\"], n 1", '{ first_long_or_push(&mut v, 1).clear(); v }', 'words(&["", "abc"])', setup='let mut v = words(&["xyz", "abc"]);'),
     r"""
     #[test]
     fn random_vs_model() {
         let mut rng = anneal_prelude::Rng::new(6231);
         for _ in 0..300 {
             let mut owned = Vec::new();
             for _ in 0..rng.below(4) {
                 owned.push(String::new());
             }
             for w in owned.iter_mut() {
                 let len = 1 + (w.len() % 3);
                 w.push_str(&"ab"[..len.min(2)]);
             }
             let refs: Vec<&str> = owned.iter().map(|s| s.as_str()).collect();
             let mut root = build(&refs);
             let len = rng.below(5);
             let path = rng.string(len, "ab");
             let mut want = 0;
             {
                 let mut cur = &root;
                 for c in path.chars() {
                     match cur.kids.get(&c) {
                         Some(n) => {
                             cur = n;
                             want += 1;
                         }
                         None => break,
                     }
                 }
             }
             check!(format!("trie {refs:?}; path {path:?}"), depth(&mut root, &path), want);
             let before: u32 = grow(&mut root, &path).count;
             check!(format!("trie {refs:?}; grow {path:?}: count"), before >= 1, true);

             let n = rng.below(4);
             let mut v: Vec<String> = (0..rng.below(4)).map(|_| "x".repeat(rng.below(4))).collect();
             let start = v.clone();
             let mut want = v.clone();
             match want.iter().position(|w| w.len() > n) {
                 Some(i) => want[i].push('!'),
                 None => want.push("fallback!".to_string()),
             }
             first_long_or_push(&mut v, n).push('!');
             check!(format!("first_long_or_push({start:?}, {n})"), v, want);
         }
     }

     #[test]
     fn deep_trie() {
         let path: String = "ab".repeat(5_000);
         let mut root = Node::default();
         for _ in 0..3 {
             grow(&mut root, &path[..100]);
         }
         let mut v: Vec<String> = (0..100_000).map(|i| if i == 99_999 { "long".to_string() } else { String::new() }).collect();
         first_long_or_push(&mut v, 3).push('!');
         check!("grow a 100-char path 3 times; then 100000 words, the long one last", (depth(&mut root, &path), v[99_999].clone(), v.len()), (3, "long!".to_string(), 100_000));
     }
     """],
    [("rust", "In `deepest_mut`, `cur.kids.get_mut(&c)` either moves the cursor forward or is abandoned while `cur` itself is returned. NLL can't see that the borrow is dead on the `None` path, because on the other path it flows into `cur`, which is returned."),
     ("rust", "Check first with a method that returns no borrow (`contains_key`), then take the `&mut` only on the path that keeps it."),
     ("rust", "In `first_long_or_push`, find an index with a shared search (`position`); an index borrows nothing, so the push path is free, and `&mut words[i]` is taken only on the returning path.")],
    ("""Both functions hit the same NLL limit as the entry-less get-or-insert (problem case #3), in a loop. A mutable borrow that's returned (or flows into a variable that's returned) on one path is treated as live on every path, so the other path's use of the same data conflicts. The loop makes it worse: in `deepest_mut`, `get_mut`'s borrow flows into `cur`, and `cur` is returned, so the borrow is considered live even on the `None` arm, where `cur` itself is returned. Polonius accepts both originals.

The fix is always to decide without holding the borrow: `contains_key` (a `bool`) before `get_mut`, or `position` (an index) before indexing. The cost is a second lookup per step, which is why the entry API and `Vec::iter().position` exist. `grow` shows why the trie version matters: the returned `&mut Node` lets the caller keep extending from the deepest node.

Syntax to remember: `if !cur.kids.contains_key(&c) { break; } cur = cur.kids.get_mut(&c).unwrap();` · `match words.iter().position(|w| w.len() > n) { Some(i) => &mut words[i], None => { words.push(..); words.last_mut().unwrap() } }`.""", "O(len) descent; O(n) search", "O(1)"),
    "How would you write `deepest_mut` recursively, and why does recursion sidestep the problem?",
    ["A borrow returned on one path is live on all paths under NLL.", "Decide with a `bool` or an index, then borrow.", "Polonius (nightly `-Zpolonius`) accepts these."],
    rules=dict(methods=["clone", "entry_ref", "cloned"], unsafe=True),
    wrong=dict(
        deepest_stops_one_early=sub(TRIE_SOLUTION, "        if !cur.kids.contains_key(&c) {\n            break;\n        }\n        cur = cur.kids.get_mut(&c).unwrap();",
                                    "        if !cur.kids.get(&c).is_some_and(|k| !k.kids.is_empty()) {\n            break;\n        }\n        cur = cur.kids.get_mut(&c).unwrap();"),
        last_long_word=sub(TRIE_SOLUTION, "match words.iter().position(|w| w.len() > n) {", "match words.iter().rposition(|w| w.len() > n) {"),
        long_at_least=sub(TRIE_SOLUTION, "match words.iter().position(|w| w.len() > n) {", "match words.iter().position(|w| w.len() >= n) {"),
    ),
))

BANK_ENUM = r"""
use std::slice::GetDisjointMutError;

#[derive(Debug, PartialEq)]
pub enum SettleError {
    /// An index appears twice.
    SameAccount,
    /// An index is out of bounds.
    NoSuchAccount,
    /// The deltas don't add up to zero.
    Unbalanced,
    /// Some balance would go negative.
    Insufficient,
}
"""

SETTLE_DOCS = r"""
/// Applies `deltas[k]` to account `idx[k]` for every k, all or nothing. On error nothing changes. Index errors
/// are reported as std's `get_disjoint_mut` finds them, then `Unbalanced`, then `Insufficient`.
pub fn settle<const N: usize>(balances: &mut [i64], idx: [usize; N], deltas: [i64; N]) -> Result<(), SettleError> {
"""

TRANSFER_DOCS = r"""
/// Moves `amount` (>= 0) from account `from` to account `to`, with the same rules as `settle`.
pub fn transfer(balances: &mut [i64], from: usize, to: usize, amount: i64) -> Result<(), SettleError> {
"""

SETTLE_BODY = r"""    let accounts = balances.get_disjoint_mut(idx).map_err(|e| match e {
        GetDisjointMutError::IndexOutOfBounds => SettleError::NoSuchAccount,
        GetDisjointMutError::OverlappingIndices => SettleError::SameAccount,
    })?;
    if deltas.iter().sum::<i64>() != 0 {
        return Err(SettleError::Unbalanced);
    }
    if accounts.iter().zip(&deltas).any(|(a, d)| **a + d < 0) {
        return Err(SettleError::Insufficient);
    }
    for (a, d) in accounts.into_iter().zip(deltas) {
        *a += d;
    }
    Ok(())
}
"""

TRANSFER_BODY = "    settle(balances, [from, to], [-amount, amount])\n}\n"

SETTLE_SOLUTION = BANK_ENUM + SETTLE_DOCS + SETTLE_BODY + TRANSFER_DOCS + TRANSFER_BODY
SETTLE_STARTER = BANK_ENUM.replace("use std::slice::GetDisjointMutError;\n", "") + SETTLE_DOCS + "    todo!()\n}\n" + TRANSFER_DOCS + "    todo!()\n}\n"

P.append(write(
    "get-disjoint-mut", "All-or-nothing settlement with get_disjoint_mut", "hard", "borrow-checker-limits", ["get_disjoint_mut", "const generics", "GetDisjointMutError", "atomic updates"],
    """
        Write `settle`, which applies N balance changes at once or none at all, and `transfer` in terms of it.
        You need `&mut` to N accounts of one slice at the same time; std's `slice::get_disjoint_mut` checks
        the indices and hands them out. No `unsafe`, no cloning the balances.
    """,
    SETTLE_STARTER,
    SETTLE_SOLUTION,
    [T("transfer_example", "[10, 0], 0 -> 1, 4", "{ let mut b = [10, 0]; (transfer(&mut b, 0, 1, 4), b) }", "(Ok(()), [6, 4])"),
     T("settle_three_way", "[5, 5, 5], idx [0, 1, 2], deltas [-5, 2, 3]", "{ let mut b = [5, 5, 5]; (settle(&mut b, [0, 1, 2], [-5, 2, 3]), b) }", "(Ok(()), [0, 7, 8])"),
     T("same_account", "transfer 1 -> 1", "{ let mut b = [1, 1]; (transfer(&mut b, 1, 1, 1), b) }", "(Err(SettleError::SameAccount), [1, 1])"),
     T("no_such_account", "transfer 0 -> 7", "{ let mut b = [1, 1]; transfer(&mut b, 0, 7, 1) }", "Err(SettleError::NoSuchAccount)"),
     T("unbalanced", "settle [0, 1] by [5, -4]", "{ let mut b = [9, 9]; (settle(&mut b, [0, 1], [5, -4]), b) }", "(Err(SettleError::Unbalanced), [9, 9])"),
     T("insufficient_changes_nothing", "[3, 0, 1], settle [0, 1, 2] by [2, 1, -3]", "{ let mut b = [3, 0, 1]; (settle(&mut b, [0, 1, 2], [2, 1, -3]), b) }", "(Err(SettleError::Insufficient), [3, 0, 1])")],
    [T("settle_nothing", "settle with N = 0", "{ let mut b = [4]; (settle(&mut b, [], []), b) }", "(Ok(()), [4])"),
     T("settle_one_zero", "settle [0] by [0]", "{ let mut b = [4]; (settle(&mut b, [0], [0]), b) }", "(Ok(()), [4])"),
     T("settle_one_nonzero", "settle [0] by [3]", "{ let mut b = [4]; settle(&mut b, [0], [3]) }", "Err(SettleError::Unbalanced)"),
     T("drain_to_zero", "[7, 0], 0 -> 1, 7", "{ let mut b = [7, 0]; (transfer(&mut b, 0, 1, 7), b) }", "(Ok(()), [0, 7])"),
     T("unordered_indices", "[1, 2, 3, 4], idx [3, 0, 2], deltas [-4, 1, 3]", "{ let mut b = [1, 2, 3, 4]; (settle(&mut b, [3, 0, 2], [-4, 1, 3]), b) }", "(Ok(()), [2, 2, 6, 0])"),
     T("index_error_before_balance", "idx [0, 9] and unbalanced deltas", "{ let mut b = [1, 1]; settle(&mut b, [0, 9], [1, 1]) }", "Err(SettleError::NoSuchAccount)"),
     T("overlap_before_balance", "idx [1, 1] and unbalanced deltas", "{ let mut b = [1, 1]; settle(&mut b, [1, 1], [1, 1]) }", "Err(SettleError::SameAccount)"),
     T("empty_slice", "no accounts; transfer 0 -> 1", "transfer(&mut [], 0, 1, 0)", "Err(SettleError::NoSuchAccount)"),
     T("balance_before_funds", "[0, 0], settle by [-1, 2]", "{ let mut b = [0, 0]; settle(&mut b, [0, 1], [-1, 2]) }", "Err(SettleError::Unbalanced)"),
     r"""
     #[test]
     fn random_vs_model() {
         let mut rng = anneal_prelude::Rng::new(6232);
         for _ in 0..400 {
             let n = rng.below(5);
             let start: Vec<i64> = rng.vec(n, 0, 6);
             let idx = [rng.below(n + 1), rng.below(n + 1), rng.below(n + 1)];
             let mut deltas = [rng.int(-4, 4), rng.int(-4, 4), 0];
             deltas[2] = if rng.below(4) == 0 { rng.int(-3, 3) } else { -deltas[0] - deltas[1] };
             let mut b = start.clone();
             let got = settle(&mut b, idx, deltas);
             let mut want_b = start.clone();
             let mut idx_err = None;
             for k in 0..3 {
                 if idx[k] >= n {
                     idx_err = Some(SettleError::NoSuchAccount);
                     break;
                 }
                 if idx[..k].contains(&idx[k]) {
                     idx_err = Some(SettleError::SameAccount);
                     break;
                 }
             }
             let want = if let Some(e) = idx_err {
                 Err(e)
             } else if deltas.iter().sum::<i64>() != 0 {
                 Err(SettleError::Unbalanced)
             } else if (0..3).any(|k| start[idx[k]] + deltas[k] < 0) {
                 Err(SettleError::Insufficient)
             } else {
                 for k in 0..3 {
                     want_b[idx[k]] += deltas[k];
                 }
                 Ok(())
             };
             check!(format!("balances {start:?}, idx {idx:?}, deltas {deltas:?}"), (got, b), (want, want_b));
         }
     }

     #[test]
     fn many_transfers() {
         let mut b = vec![1i64; 100_000];
         let mut ok = 0;
         for i in 0..99_999 {
             let amount = b[i];
             if transfer(&mut b, i, i + 1, amount).is_ok() {
                 ok += 1;
             }
         }
         check!("100000 accounts of 1, each passing everything to the next", (ok, b[99_999], b[0]), (99_999, 100_000, 0));
     }
     """],
    [("rust", "`balances.get_disjoint_mut(idx)` takes `[usize; N]` and returns `Result<[&mut i64; N], GetDisjointMutError>`: every index checked for bounds and overlap once, then N live `&mut` into one slice."),
     ("rust", "All or nothing: do every check (sum, no negative result) before the first write. The `&mut`s you got can be read too."),
     ("rust", "`settle<const N: usize>` lets the caller pick N; `transfer` is `settle` with N = 2.")],
    ("""N simultaneous `&mut` into one slice need a proof that the indices are distinct and in bounds; `get_disjoint_mut` does it at run time (O(N²) comparisons, fine for small N) and returns an array of `&mut`, with the `unsafe` inside std. Everything after that is ordinary code, and the all-or-nothing rule is just ordering: validate through the borrows first, write last. A const generic keeps the array sizes in the types, so `idx` and `deltas` can't disagree in length.

Syntax to remember: `pub fn settle<const N: usize>(b: &mut [i64], idx: [usize; N], deltas: [i64; N])` · `let accounts = b.get_disjoint_mut(idx).map_err(|e| match e { GetDisjointMutError::IndexOutOfBounds => .., GetDisjointMutError::OverlappingIndices => .. })?;` · `for (a, d) in accounts.into_iter().zip(deltas) { *a += d; }`.""", "O(N²) checks + O(N)", "O(1)"),
    "How would you settle a batch whose size is only known at run time, with a `&[usize]` of indices?",
    ["`get_disjoint_mut` checks bounds and overlap, then hands out N `&mut`.", "Validate everything before the first write.", "Const generics tie array lengths together."],
    related=("L2", "S3"),
    wrong=dict(
        writes_before_checking_funds=sub(SETTLE_SOLUTION, "    if accounts.iter().zip(&deltas).any(|(a, d)| **a + d < 0) {\n        return Err(SettleError::Insufficient);\n    }\n    for (a, d) in accounts.into_iter().zip(deltas) {\n        *a += d;\n    }\n    Ok(())",
                                          "    let mut short = false;\n    for (a, d) in accounts.into_iter().zip(deltas) {\n        *a += d;\n        short |= *a < 0;\n    }\n    if short {\n        return Err(SettleError::Insufficient);\n    }\n    Ok(())"),
        zero_counts_as_insufficient=sub(SETTLE_SOLUTION, "**a + d < 0", "**a + d <= 0"),
        no_balance_check=sub(SETTLE_SOLUTION, "    if deltas.iter().sum::<i64>() != 0 {\n        return Err(SettleError::Unbalanced);\n    }\n", ""),
    ),
))

RING_HEAD = r"""
use std::cell::Cell;

/// A node in a doubly linked ring. The links are plain shared references, so every node can be reached from
/// every other one while all of them are only borrowed.
pub struct Node<'a> {
    pub name: String,
    visits: Cell<u32>,
    next: Cell<Option<&'a Node<'a>>>,
    prev: Cell<Option<&'a Node<'a>>>,
}
"""

RING_DOCS = dict(
    new="    pub fn new(name: &str) -> Self {\n",
    visits="    pub fn visits(&self) -> u32 {\n",
    next="    pub fn next(&self) -> Option<&'a Node<'a>> {\n",
    prev="    pub fn prev(&self) -> Option<&'a Node<'a>> {\n",
    link="    /// Makes `other` come right after `self`: `self.next` is `other` and `other.prev` is `self`.\n    pub fn link(&'a self, other: &'a Node<'a>) {\n",
    unlink="    /// Takes `self` out of its ring: its neighbours link to each other, and `self` links to nothing. A node\n    /// alone in its ring (linked to itself) just ends up unlinked.\n    pub fn unlink(&self) {\n",
    ring="/// Links `nodes` into one ring, in order, the last back to the first.\npub fn ring<'a>(nodes: &'a [Node<'a>]) {\n",
    walk="/// Starting at `start`, visits `steps` nodes along `next` links (counting each visit on the node), stopping\n/// early at a node with no `next`. Returns the names visited.\npub fn walk<'a>(start: &'a Node<'a>, steps: usize) -> Vec<&'a str> {\n",
)

RING_BODIES = dict(
    new="        Node { name: name.to_string(), visits: Cell::new(0), next: Cell::new(None), prev: Cell::new(None) }\n",
    visits="        self.visits.get()\n",
    next="        self.next.get()\n",
    prev="        self.prev.get()\n",
    link="        self.next.set(Some(other));\n        other.prev.set(Some(self));\n",
    unlink="""        let (prev, next) = (self.prev.take(), self.next.take());
        if let (Some(p), Some(n)) = (prev, next) {
            if !std::ptr::eq(p, self) {
                p.next.set(Some(n));
                n.prev.set(Some(p));
            }
        }
""",
    ring="""    for pair in nodes.windows(2) {
        pair[0].link(&pair[1]);
    }
    if let (Some(first), Some(last)) = (nodes.first(), nodes.last()) {
        last.link(first);
    }
""",
    walk="""    let mut out = Vec::with_capacity(steps);
    let mut cur = Some(start);
    while let Some(node) = cur {
        if out.len() == steps {
            break;
        }
        node.visits.set(node.visits.get() + 1);
        out.push(node.name.as_str());
        cur = node.next.get();
    }
    out
""",
)


def ring_src(bodies):
    out = RING_HEAD + "\nimpl<'a> Node<'a> {\n"
    for k in ("new", "visits", "next", "prev", "link", "unlink"):
        out += RING_DOCS[k] + bodies[k] + "    }\n\n"
    out = out.rstrip("\n") + "\n}\n\n"
    for k in ("ring", "walk"):
        out += RING_DOCS[k] + bodies[k] + "}\n\n"
    return out.rstrip("\n") + "\n"


RING_SOLUTION = ring_src(RING_BODIES)
RING_STARTER = ring_src({k: ("    todo!()\n" if k in ("ring", "walk") else "        todo!()\n") for k in RING_BODIES})
ABC = 'let nodes = [Node::new("a"), Node::new("b"), Node::new("c")];\nring(&nodes);'

P.append(write(
    "cell-for-copy", "Cell links between shared nodes", "hard", "borrow-checker-limits", ["Cell", "interior mutability", "arenas", "Option<&T> is Copy", "invariance"],
    """
        Write a doubly linked ring whose nodes live in an ordinary slice and point at each other with shared
        references. Everything goes through `&Node`: linking, unlinking and counting visits all mutate through
        shared borrows, with `Cell` and no `RefCell`, `Rc` or `unsafe`.
    """,
    RING_STARTER,
    RING_SOLUTION,
    [T("walk_the_ring", "ring a, b, c; walk 5 from a", '(walk(&nodes[0], 5), nodes.iter().map(|n| n.visits()).collect::<Vec<_>>())', '(vec!["a", "b", "c", "a", "b"], vec![2, 2, 1])', setup=ABC),
     T("backwards", "ring a, b, c; prev of a, prev of that", 'nodes[0].prev().and_then(|n| n.prev()).map(|n| n.name.as_str())', 'Some("b")', setup=ABC),
     T("unlink_middle", "ring a, b, c; unlink b; walk 4 from a", '(walk(&nodes[0], 4), nodes[1].next().is_none(), nodes[2].prev().map(|n| n.name.as_str()))', '(vec!["a", "c", "a", "c"], true, Some("a"))', setup=ABC + "\nnodes[1].unlink();"),
     T("single_node_ring", "ring [solo]; walk 3", 'walk(&nodes[0], 3)', 'vec!["solo", "solo", "solo"]', setup='let nodes = [Node::new("solo")];\nring(&nodes);'),
     T("unlinked_node_walks_once", "a lone node, never linked; walk 3", '(walk(&n, 3), n.visits())', '(vec!["x"], 1)', setup='let n = Node::new("x");'),
     T("link_by_hand", "a -> b by hand; walk 5 from a", '(walk(&a, 5), b.prev().map(|n| n.name.as_str()))', '(vec!["a", "b"], Some("a"))', setup='let (a, b) = (Node::new("a"), Node::new("b"));\na.link(&b);')],
    [T("walk_zero", "ring a, b, c; walk 0", '(walk(&nodes[0], 0).len(), nodes[0].visits())', "(0, 0)", setup=ABC),
     T("empty_ring", "ring of no nodes", '{ let nodes: [Node; 0] = []; ring(&nodes); 0 }', "0"),
     T("unlink_solo", "ring [solo]; unlink; next and prev", '(nodes[0].next().is_none(), nodes[0].prev().is_none(), walk(&nodes[0], 3))', '(true, true, vec!["solo"])', setup='let nodes = [Node::new("solo")];\nring(&nodes);\nnodes[0].unlink();'),
     T("unlink_two_of_three", "ring a, b, c; unlink a, then b; walk 3 from c", 'walk(&nodes[2], 3)', 'vec!["c", "c", "c"]', setup=ABC + "\nnodes[0].unlink();\nnodes[1].unlink();"),
     T("unlink_one_of_two", "ring a, b; unlink a; walk 3 from b", '(walk(&nodes[1], 3), nodes[1].prev().map(|n| n.name.as_str()))', '(vec!["b", "b", "b"], Some("b"))', setup='let nodes = [Node::new("a"), Node::new("b")];\nring(&nodes);\nnodes[0].unlink();'),
     T("relink_after_unlink", "ring a, b, c; unlink b; link c -> b, b -> a; walk 4 from a", 'walk(&nodes[0], 4)', 'vec!["a", "c", "b", "a"]', setup=ABC + "\nnodes[1].unlink();\nnodes[2].link(&nodes[1]);\nnodes[1].link(&nodes[0]);"),
     T("visits_accumulate", "ring a, b; walk 3 twice from a", '{ walk(&nodes[0], 3); walk(&nodes[0], 3); (nodes[0].visits(), nodes[1].visits()) }', "(4, 2)", setup='let nodes = [Node::new("a"), Node::new("b")];\nring(&nodes);'),
     T("names_borrow_the_nodes", "walk returns the nodes' own names", 'walk(&nodes[0], 1)[0].as_ptr() == nodes[0].name.as_ptr()', "true", setup=ABC),
     r"""
     #[test]
     fn random_vs_model() {
         let mut rng = anneal_prelude::Rng::new(6233);
         for _ in 0..300 {
             let n = 1 + rng.below(6);
             let nodes: Vec<Node> = (0..n).map(|i| Node::new(&format!("n{i}"))).collect();
             ring(&nodes);
             let mut model: Vec<usize> = (0..n).collect();
             let mut ops = Vec::new();
             for _ in 0..rng.below(n) {
                 let k = rng.below(n);
                 nodes[k].unlink();
                 model.retain(|&x| x != k);
                 ops.push(format!("unlink n{k}"));
             }
             let start = rng.below(n);
             let steps = rng.below(8);
             let want: Vec<String> = match model.iter().position(|&x| x == start) {
                 Some(p) => (0..steps).map(|s| format!("n{}", model[(p + s) % model.len()])).collect(),
                 None => (0..steps.min(1)).map(|_| format!("n{start}")).collect(),
             };
             let got: Vec<String> = walk(&nodes[start], steps).iter().map(|s| s.to_string()).collect();
             check!(format!("ring of {n}; {}; walk {steps} from n{start}", ops.join(", ")), got, want);
         }
     }

     #[test]
     fn big_ring() {
         let nodes: Vec<Node> = (0..100_000).map(|i| Node::new(&i.to_string())).collect();
         ring(&nodes);
         for k in (1..100_000).step_by(2) {
             nodes[k].unlink();
         }
         let w = walk(&nodes[0], 50_001);
         check!("ring of 100000, odd ones unlinked, walk 50001", (w.len(), w[49_999], w[50_000], nodes[0].visits()), (50_001, "99998", "0", 2));
     }
     """],
    [("rust", "`Option<&'a Node<'a>>` is `Copy`, so `Cell<Option<&Node>>` can hand out copies with `get` and change with `set`, through a shared `&Node`. `Cell::take` returns the value and leaves `None`."),
     ("rust", "`link(&'a self, ..)` needs `self` borrowed for `'a`, because it stores `&self` inside another node."),
     ("rust", "A node alone in its ring is its own neighbour: in `unlink`, don't relink it to itself.")],
    ("""`Cell<T>` allows mutation through `&` by never handing out a reference to its contents: `get` copies out (so `T: Copy`), `set` and `replace` swap in, `take` leaves `Default`. There's nothing to track at run time, unlike `RefCell`'s borrow counter. Shared references are `Copy`, so `Cell<Option<&'a Node<'a>>>` is the classic way to build cyclic structures over nodes that live in an arena (here, a slice): every node is only ever shared, and the links change in place. The price is invariance: a `Cell<&'a T>` fixes `'a` exactly, so all the nodes must share one lifetime, and they can't be moved or dropped while linked. `rustc` builds its type graphs this way. `Cell` isn't `Sync`, so none of this crosses threads.

Syntax to remember: `next: Cell<Option<&'a Node<'a>>>` · `self.next.set(Some(other))` · `let (p, n) = (self.prev.take(), self.next.take());` · `self.visits.set(self.visits.get() + 1)` (or `self.visits.update(|v| v + 1)`) · `pub fn link(&'a self, other: &'a Node<'a>)`.""", "O(1) link and unlink; O(steps) walk", "O(1) extra"),
    "Why must `Node<'a>` be invariant in `'a`, and what would go wrong if `Cell<&'a T>` were covariant?",
    ["`Cell` mutates `Copy` values through `&`: `get`, `set`, `replace`, `take`.", "Shared references are `Copy`, so they can live in a `Cell`.", "Arenas plus `Cell` links give cyclic graphs without `Rc`."],
    related=("L2", "L3", "S7"),
    wrong=dict(
        unlink_leaves_prev=sub(RING_SOLUTION, "                p.next.set(Some(n));\n                n.prev.set(Some(p));\n", "                p.next.set(Some(n));\n"),
        ring_not_closed=sub(RING_SOLUTION, "    if let (Some(first), Some(last)) = (nodes.first(), nodes.last()) {\n        last.link(first);\n    }\n", ""),
        walk_counts_start_twice=sub(RING_SOLUTION, "        node.visits.set(node.visits.get() + 1);\n", "        node.visits.set(node.visits.get() + if out.is_empty() { 2 } else { 1 });\n"),
    ),
))

SCHED_HEAD = r"""
use std::cell::RefCell;
use std::collections::HashMap;

pub struct Scheduler {
    queue: RefCell<Vec<u32>>,
    seen: RefCell<HashMap<u32, u32>>,
    done: RefCell<Vec<u32>>,
}
"""

SCHED_STARTER = SCHED_HEAD + r"""
impl Scheduler {
    pub fn new() -> Self {
        Scheduler { queue: RefCell::new(Vec::new()), seen: RefCell::new(HashMap::new()), done: RefCell::new(Vec::new()) }
    }

    /// Counts a submission of `job`, and queues it if the counts had no entry for it yet. Returns whether it
    /// queued.
    pub fn submit(&self, job: u32) -> bool {
        let seen = self.seen.borrow();
        let first = !seen.contains_key(&job);
        *self.seen.borrow_mut().entry(job).or_insert(0) += 1;
        if first {
            self.queue.borrow_mut().push(job);
        }
        first
    }

    /// How many times `job` was submitted. A job never submitted gets an entry with 0.
    pub fn count(&self, job: u32) -> u32 {
        if let Some(n) = self.seen.borrow().get(&job) {
            *n
        } else {
            self.seen.borrow_mut().insert(job, 0);
            0
        }
    }

    /// Runs queued jobs, newest first, until the queue is empty. Running an even job above 0 submits job / 2.
    /// Returns how many jobs ran.
    pub fn run(&self) -> usize {
        let mut ran = 0;
        while let Some(job) = self.queue.borrow_mut().pop() {
            self.done.borrow_mut().push(job);
            if job > 0 && job % 2 == 0 {
                self.submit(job / 2);
            }
            ran += 1;
        }
        ran
    }

    pub fn done(&self) -> Vec<u32> {
        self.done.borrow().to_vec()
    }

    /// How many jobs have an entry in the submission counts.
    pub fn known(&self) -> usize {
        self.seen.borrow().len()
    }
}
"""

SCHED_SOLUTION = SCHED_STARTER
for _old, _new in [
    ("        let seen = self.seen.borrow();\n        let first = !seen.contains_key(&job);\n", "        let first = !self.seen.borrow().contains_key(&job);\n"),
    ("        if let Some(n) = self.seen.borrow().get(&job) {\n            *n\n        } else {\n            self.seen.borrow_mut().insert(job, 0);\n            0\n        }\n",
     "        let known = self.seen.borrow().get(&job).copied();\n        match known {\n            Some(n) => n,\n            None => {\n                self.seen.borrow_mut().insert(job, 0);\n                0\n            }\n        }\n"),
    ("        while let Some(job) = self.queue.borrow_mut().pop() {\n", "        loop {\n            let next = self.queue.borrow_mut().pop();\n            let Some(job) = next else { break };\n"),
]:
    SCHED_SOLUTION = sub(SCHED_SOLUTION, _old, _new)

SC = "let s = Scheduler::new();"

P.append(fix(
    "fix-borrowmuterror", "Fix: BorrowMutError from guards you can't see", "hard", "borrow-checker-limits", ["RefCell", "BorrowMutError", "temporaries", "if let", "while let"],
    """
        `Scheduler` compiles, and three of its methods panic with `already borrowed`. Each holds a `RefCell`
        guard longer than it looks: one in a `let`, one in the condition of an `if let`, one in the condition of
        a `while let`. Fix them without `try_borrow*`, without cloning, and without changing what the methods do.
    """,
    SCHED_STARTER,
    SCHED_SOLUTION,
    [T("submit_once", "submit 3, 3, 5", "(s.submit(3), s.submit(3), s.submit(5), s.count(3))", "(true, false, true, 2)", setup=SC),
     T("count_unknown", "count(9) on a new scheduler", "(s.count(9), s.known())", "(0, 1)", setup=SC),
     T("run_follow_ups", "submit 12; run", "(s.run(), s.done())", "(3, vec![12, 6, 3])", setup=SC + "\ns.submit(12);"),
     T("run_newest_first", "submit 1, 5, 7; run", "(s.run(), s.done())", "(3, vec![7, 5, 1])", setup=SC + "\nfor j in [1, 5, 7] {\n    s.submit(j);\n}"),
     T("follow_up_already_seen", "submit 3, 6; run", "(s.run(), s.done(), s.count(3))", "(2, vec![6, 3], 2)", setup=SC + "\ns.submit(3);\ns.submit(6);")],
    [T("empty_run", "run on a new scheduler", "(s.run(), s.done())", "(0, vec![])", setup=SC),
     T("zero_job", "submit 0; run", "(s.run(), s.done())", "(1, vec![0])", setup=SC + "\ns.submit(0);"),
     T("power_of_two", "submit 16; run", "(s.run(), s.done())", "(5, vec![16, 8, 4, 2, 1])", setup=SC + "\ns.submit(16);"),
     T("run_twice", "submit 4; run; submit 4, 9; run", "{ s.run(); s.submit(4); s.submit(9); (s.run(), s.done()) }", "(1, vec![4, 2, 1, 9])", setup=SC + "\ns.submit(4);"),
     T("count_after_run", "submit 8; run; count 8, 4, 2, 1", "{ s.run(); (s.count(8), s.count(4), s.count(2), s.count(1)) }", "(1, 1, 1, 1)", setup=SC + "\ns.submit(8);"),
     T("count_seeds_once", "count 5 twice, then submit 5", "(s.count(5), s.count(5), s.submit(5), s.count(5))", "(0, 0, false, 1)", setup=SC),
     T("known_counts_entries", "submit 1, 1, 2; count 7", "{ s.submit(1); s.submit(1); s.submit(2); s.count(7); s.known() }", "3", setup=SC),
     T("big_job", "submit u32::MAX - 1 (even); run", "(s.run(), s.done()[1])", "(2, u32::MAX / 2)", setup=SC + "\ns.submit(u32::MAX - 1);"),
     r"""
     #[test]
     fn random_vs_model() {
         use std::collections::HashMap;
         let mut rng = anneal_prelude::Rng::new(6234);
         for _ in 0..300 {
             let s = Scheduler::new();
             let (mut queue, mut seen, mut done): (Vec<u32>, HashMap<u32, u32>, Vec<u32>) = (vec![], HashMap::new(), vec![]);
             let mut ops = Vec::new();
             fn submit(queue: &mut Vec<u32>, seen: &mut HashMap<u32, u32>, job: u32) -> bool {
                 let first = !seen.contains_key(&job);
                 *seen.entry(job).or_insert(0) += 1;
                 if first {
                     queue.push(job);
                 }
                 first
             }
             for _ in 0..6 {
                 match rng.below(3) {
                     0 => {
                         let j = rng.below(20) as u32;
                         ops.push(format!("submit {j}"));
                         check!(ops.join(", "), s.submit(j), submit(&mut queue, &mut seen, j));
                     }
                     1 => {
                         let j = rng.below(20) as u32;
                         let want = *seen.entry(j).or_insert(0);
                         ops.push(format!("count {j}"));
                         check!(ops.join(", "), s.count(j), want);
                     }
                     _ => {
                         let mut ran = 0;
                         while let Some(j) = queue.pop() {
                             done.push(j);
                             if j > 0 && j % 2 == 0 {
                                 submit(&mut queue, &mut seen, j / 2);
                             }
                             ran += 1;
                         }
                         ops.push("run".to_string());
                         check!(ops.join(", "), s.run(), ran);
                     }
                 }
             }
             check!(format!("{}; done, known", ops.join(", ")), (s.done(), s.known()), (done, seen.len()));
         }
     }
     """],
    [("rust", "`let seen = self.seen.borrow();` keeps a `Ref` guard until the end of `submit`, so the `borrow_mut` two lines later panics. End the shared borrow in the statement that needs it."),
     ("rust", "In edition 2021, a temporary in the condition of an `if let` (the `Ref` from `self.seen.borrow()`) lives until the end of the whole `if let ... else`, including the `else` block. Copy the answer out in a `let` first."),
     ("rust", "`while let Some(job) = self.queue.borrow_mut().pop()` keeps the `RefMut` alive for the whole loop body, where `submit` pushes to the same queue. Pop in its own statement.")],
    ("""`RefCell` enforces the borrow rules at run time through guards (`Ref`, `RefMut`), and a guard lives as long as the value holding it. That's easy to see for `let seen = self.seen.borrow();` (the end of the block) and easy to miss for temporaries. A temporary created in the scrutinee of `match`, `if let` or `while let` lives until the end of that whole expression, so the guard covers every arm, the loop body, and (in edition 2021) the `else` block. Edition 2024 drops `if let` temporaries before `else`, but `match` and `while let` scrutinees still hold theirs. The fix is always to finish with the guard in its own statement: copy the value out (`.copied()`, `.pop()` into a `let`), then act on it.

Syntax to remember: `let first = !self.seen.borrow().contains_key(&job);` · `let known = self.seen.borrow().get(&job).copied();` · `loop { let next = self.queue.borrow_mut().pop(); let Some(job) = next else { break }; .. }`.""", "O(1) per call; O(jobs) run", "O(1) extra"),
    "`clippy::await_holding_refcell_ref` exists for a related bug. What does an `.await` have in common with the `while let` body here?",
    ["A `RefCell` guard lives as long as the variable or temporary holding it.", "Scrutinee temporaries of `match`, `if let` (2021) and `while let` live through the whole expression.", "Copy out, then act."],
    rules=dict(methods=["try_borrow", "try_borrow_mut", "clone", "as_ptr", "get_mut"]),
    related=("L2", "S7"),
    wrong=dict(
        run_takes_the_queue=sub(SCHED_SOLUTION, "        loop {\n            let next = self.queue.borrow_mut().pop();\n            let Some(job) = next else { break };\n",
                                "        let jobs = self.queue.take();\n        for job in jobs.into_iter().rev() {\n"),
        count_forgets_to_seed=sub(SCHED_SOLUTION, "            None => {\n                self.seen.borrow_mut().insert(job, 0);\n                0\n            }\n", "            None => 0,\n"),
        counts_only_first=sub(SCHED_SOLUTION, "        *self.seen.borrow_mut().entry(job).or_insert(0) += 1;\n        if first {\n", "        if first {\n            self.seen.borrow_mut().insert(job, 1);\n"),
    ),
))

BUS_HEAD_STARTER = r"""
use std::cell::RefCell;

pub struct Bus {
    listeners: RefCell<Vec<Box<dyn Fn(&Bus, &str)>>>,
    log: RefCell<Vec<String>>,
}
"""

BUS_HEAD_SOLUTION = r"""
use std::cell::RefCell;
use std::rc::Rc;

pub struct Bus {
    listeners: RefCell<Vec<Rc<dyn Fn(&Bus, &str)>>>,
    log: RefCell<Vec<String>>,
}
"""

BUS_IMPL_STARTER = r"""
impl Bus {
    pub fn new() -> Self {
        Bus { listeners: RefCell::new(Vec::new()), log: RefCell::new(Vec::new()) }
    }

    pub fn subscribe(&self, f: impl Fn(&Bus, &str) + 'static) {
        self.listeners.borrow_mut().push(Box::new(f));
    }

    /// Logs `event`, then calls every listener subscribed so far, in order. A listener may emit more events
    /// (each handled completely, right away) and may subscribe new listeners, which hear only later events.
    pub fn emit(&self, event: &str) {
        self.log.borrow_mut().push(event.to_string());
        for l in self.listeners.borrow().iter() {
            l(self, event);
        }
    }

    /// Emits again, in order, every event logged before this call.
    pub fn replay(&self) {
        for e in self.log.borrow().iter() {
            self.emit(e);
        }
    }

    pub fn log(&self) -> Vec<String> {
        self.log.borrow().to_vec()
    }
}
"""

BUS_IMPL_SOLUTION = r"""
impl Bus {
    pub fn new() -> Self {
        Bus { listeners: RefCell::new(Vec::new()), log: RefCell::new(Vec::new()) }
    }

    pub fn subscribe(&self, f: impl Fn(&Bus, &str) + 'static) {
        self.listeners.borrow_mut().push(Rc::new(f));
    }

    /// Logs `event`, then calls every listener subscribed so far, in order. A listener may emit more events
    /// (each handled completely, right away) and may subscribe new listeners, which hear only later events.
    pub fn emit(&self, event: &str) {
        self.log.borrow_mut().push(event.to_string());
        let listeners = self.listeners.borrow().clone();
        for l in listeners {
            l(self, event);
        }
    }

    /// Emits again, in order, every event logged before this call.
    pub fn replay(&self) {
        let n = self.log.borrow().len();
        for i in 0..n {
            let e = self.log.borrow()[i].clone();
            self.emit(&e);
        }
    }

    pub fn log(&self) -> Vec<String> {
        self.log.borrow().to_vec()
    }
}
"""

BUS_SOLUTION = BUS_HEAD_SOLUTION + BUS_IMPL_SOLUTION
BUS_NEW = "let bus = Bus::new();"

P.append(fix(
    "fix-refcell-guard-across-call", "Fix: re-entrant callbacks and a RefCell guard", "hard", "borrow-checker-limits", ["RefCell", "re-entrancy", "Rc<dyn Fn>", "snapshots", "observer"],
    """
        `Bus` compiles, but a listener that subscribes another listener panics, and so does `replay`. Both call
        out while holding a `RefCell` guard that the callee needs. Fix them. You may change how listeners are
        stored. Listeners must stay free to emit and subscribe from inside a call, with the semantics in the
        doc comments.
    """,
    BUS_HEAD_STARTER + BUS_IMPL_STARTER,
    BUS_SOLUTION,
    [T("emit_in_order", "two listeners logging \"1:<e>\" and \"2:<e>\" into a shared Vec; emit x", "seen.borrow().clone()", 'vec!["1:x".to_string(), "2:x".to_string()]',
       setup=BUS_NEW + '\nlet seen = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));\nfor k in 1..=2 {\n    let seen = seen.clone();\n    bus.subscribe(move |_, e| seen.borrow_mut().push(format!("{k}:{e}")));\n}\nbus.emit("x");'),
     T("subscribe_from_a_listener", "a listener that subscribes a logger on \"add\"; emit add, then ping", "bus.log()", '["add", "ping", "heard ping"].map(String::from).to_vec()',
       setup=BUS_NEW + '\nbus.subscribe(|b, e| {\n    if e == "add" {\n        b.subscribe(|b, e| if !e.starts_with("heard") { b.emit(&format!("heard {e}")) });\n    }\n});\nbus.emit("add");\nbus.emit("ping");'),
     T("nested_emit", "a listener that answers \"ping\" with \"pong\"; emit ping", "bus.log()", '["ping", "pong"].map(String::from).to_vec()',
       setup=BUS_NEW + '\nbus.subscribe(|b, e| if e == "ping" { b.emit("pong") });\nbus.emit("ping");'),
     T("replay", "no listeners; emit a, b; replay", "bus.log()", '["a", "b", "a", "b"].map(String::from).to_vec()', setup=BUS_NEW + '\nbus.emit("a");\nbus.emit("b");\nbus.replay();'),
     T("replay_with_answers", "ping -> pong listener; emit ping; replay", "bus.log()", '["ping", "pong", "ping", "pong", "pong"].map(String::from).to_vec()',
       setup=BUS_NEW + '\nbus.subscribe(|b, e| if e == "ping" { b.emit("pong") });\nbus.emit("ping");\nbus.replay();')],
    [T("no_events", "new bus; replay", "bus.log().len()", "0", setup=BUS_NEW + "\nbus.replay();"),
     T("nested_reaches_all_listeners", "listener 1 answers a with b; listener 2 records everything; emit a", "seen.borrow().clone()", 'vec!["b".to_string(), "a".to_string()]',
       setup=BUS_NEW + '\nlet seen = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));\nbus.subscribe(|b, e| if e == "a" { b.emit("b") });\nlet s2 = seen.clone();\nbus.subscribe(move |_, e| s2.borrow_mut().push(e.to_string()));\nbus.emit("a");'),
     T("new_listener_misses_current_event", "listener subscribes a counter on every event; emit x, y", "count.get()", "1",
       setup=BUS_NEW + '\nlet count = std::rc::Rc::new(std::cell::Cell::new(0));\nlet c = count.clone();\nbus.subscribe(move |b, _| {\n    let c = c.clone();\n    b.subscribe(move |_, _| c.set(c.get() + 1));\n});\nbus.emit("x");\nbus.emit("y");'),
     T("chain", "listeners: a -> b, b -> c; emit a", "bus.log()", '["a", "b", "c"].map(String::from).to_vec()',
       setup=BUS_NEW + '\nbus.subscribe(|b, e| if e == "a" { b.emit("b") });\nbus.subscribe(|b, e| if e == "b" { b.emit("c") });\nbus.emit("a");'),
     T("depth_first", "a -> [b, c] from two listeners, b -> d; emit a", "bus.log()", '["a", "b", "d", "c"].map(String::from).to_vec()',
       setup=BUS_NEW + '\nbus.subscribe(|b, e| match e { "a" => b.emit("b"), "b" => b.emit("d"), _ => {} });\nbus.subscribe(|b, e| if e == "a" { b.emit("c") });\nbus.emit("a");'),
     T("replay_twice", "emit a; replay; replay", "bus.log()", '["a", "a", "a", "a"].map(String::from).to_vec()', setup=BUS_NEW + '\nbus.emit("a");\nbus.replay();\nbus.replay();'),
     T("replay_from_a_listener", "a listener that replays on \"again\" (once); emit x, again", "bus.log()", '["x", "again", "x", "again"].map(String::from).to_vec()',
       setup=BUS_NEW + '\nlet done = std::rc::Rc::new(std::cell::Cell::new(false));\nlet d = done.clone();\nbus.subscribe(move |b, e| if e == "again" && !d.get() {\n    d.set(true);\n    b.replay();\n});\nbus.emit("x");\nbus.emit("again");'),
     T("unicode_events", "emit 日本; replay", "bus.log()", '["日本", "日本"].map(String::from).to_vec()', setup=BUS_NEW + '\nbus.emit("日本");\nbus.replay();'),
     r"""
     #[test]
     fn many_listeners() {
         let bus = Bus::new();
         let hits = std::rc::Rc::new(std::cell::Cell::new(0u64));
         for _ in 0..1000 {
             let h = hits.clone();
             bus.subscribe(move |_, _| h.set(h.get() + 1));
         }
         for i in 0..100 {
             bus.emit(&i.to_string());
         }
         check!("1000 listeners, 100 events", (hits.get(), bus.log().len()), (100_000, 100));
     }
     """],
    [("rust", "`for l in self.listeners.borrow().iter()` holds a `Ref` for the whole loop. A listener that subscribes needs `borrow_mut` on the same cell: panic. Nested `emit` only needs `borrow`, so it seems fine until someone subscribes."),
     ("rust", "Taking the listeners out while calling them (`mem::take`) breaks nested emits: they'd find no listeners. Instead call a snapshot. With `Rc<dyn Fn>` instead of `Box<dyn Fn>`, copying the list clones pointers, not closures."),
     ("rust", "`replay` holds a `Ref` on `log` while `emit` pushes to it. Read the length first, then fetch each event in its own short borrow.")],
    ("""Callbacks make every `RefCell` guard dangerous: while you hold one, you've handed control to code that may come back into the same object (re-entrancy). The observer pattern hits this twice. Iterating the listeners with a guard alive fails as soon as a listener subscribes. Taking the list out with `mem::take` fixes the panic but breaks nested emits, which would find the list empty. Snapshotting is what works: store listeners as `Rc<dyn Fn>`, clone the `Vec` of `Rc`s (a refcount bump each), release the guard, then call. New subscribers go into the live list and hear only later events, and nested emits see every listener. `replay` has the same shape with the log; reading one event per short borrow (cloning that one `String`) avoids holding the guard while `emit` pushes.

Syntax to remember: `listeners: RefCell<Vec<Rc<dyn Fn(&Bus, &str)>>>` · `let listeners = self.listeners.borrow().clone(); for l in listeners { l(self, event); }` · `let n = self.log.borrow().len();`.""", "O(listeners) per emit", "O(listeners) per emit for the snapshot"),
    "What happens with the snapshot approach if a listener unsubscribes itself, and how would you support that?",
    ["Don't hold a `RefCell` guard across a callback.", "Snapshot a list of `Rc` handles instead of taking the list out.", "Take-and-restore breaks re-entrant reads."],
    rules=dict(methods=["try_borrow", "try_borrow_mut", "take", "replace"]),
    related=("L2", "S7"),
    wrong=dict(
        new_listeners_hear_current_event=BUS_HEAD_SOLUTION + sub(BUS_IMPL_SOLUTION, "        let listeners = self.listeners.borrow().clone();\n        for l in listeners {\n            l(self, event);\n        }\n",
                                                                 "        let mut i = 0;\n        loop {\n            let next = self.listeners.borrow().get(i).cloned();\n            let Some(l) = next else { break };\n            l(self, event);\n            i += 1;\n        }\n"),
        replay_backwards=BUS_HEAD_SOLUTION + sub(BUS_IMPL_SOLUTION, "        for i in 0..n {\n", "        for i in (0..n).rev() {\n"),
    ),
))

CART_STARTER = r"""
use std::cell::RefCell;

pub struct Cart {
    items: RefCell<Vec<u32>>,
    total: RefCell<u32>,
    log: RefCell<Vec<String>>,
}

impl Cart {
    pub fn new() -> Self {
        Cart { items: RefCell::new(Vec::new()), total: RefCell::new(0), log: RefCell::new(Vec::new()) }
    }

    /// Logs "[<n>] <msg>", where n is the number of items right now.
    fn note(&self, msg: String) {
        let n = self.items.borrow().len();
        self.log.borrow_mut().push(format!("[{n}] {msg}"));
    }

    /// Adds an item and logs "add <price>" (after adding it).
    pub fn add(&self, price: u32) {
        self.items.borrow_mut().push(price);
        *self.total.borrow_mut() += price;
        self.note(format!("add {price}"));
    }

    /// Doubles every price, keeping the total in step, and logs "double <old> -> <new>" for each item.
    pub fn double_all(&self) {
        for p in self.items.borrow_mut().iter_mut() {
            *self.total.borrow_mut() += *p;
            *p *= 2;
            self.note(format!("double {} -> {p}", *p / 2));
        }
    }

    /// Removes every item priced above `max`, keeping the total in step, and logs "drop <price>" for each
    /// (the count in the log line is the number of items before any were dropped). Returns how many.
    pub fn drop_above(&self, max: u32) -> usize {
        let mut dropped = 0;
        self.items.borrow_mut().retain(|&p| {
            if p <= max {
                return true;
            }
            *self.total.borrow_mut() -= p;
            self.note(format!("drop {p}"));
            dropped += 1;
            false
        });
        dropped
    }

    pub fn total(&self) -> u32 {
        *self.total.borrow()
    }

    pub fn items(&self) -> Vec<u32> {
        self.items.borrow().to_vec()
    }

    pub fn log(&self) -> Vec<String> {
        self.log.borrow().to_vec()
    }
}
"""

CART_SOLUTION = r"""
pub struct Cart {
    items: Vec<u32>,
    total: u32,
    log: Vec<String>,
}

impl Cart {
    pub fn new() -> Self {
        Cart { items: Vec::new(), total: 0, log: Vec::new() }
    }

    /// Logs "[<n>] <msg>".
    fn note(log: &mut Vec<String>, n: usize, msg: String) {
        log.push(format!("[{n}] {msg}"));
    }

    /// Adds an item and logs "add <price>" (after adding it).
    pub fn add(&mut self, price: u32) {
        self.items.push(price);
        self.total += price;
        Self::note(&mut self.log, self.items.len(), format!("add {price}"));
    }

    /// Doubles every price, keeping the total in step, and logs "double <old> -> <new>" for each item.
    pub fn double_all(&mut self) {
        let n = self.items.len();
        for p in self.items.iter_mut() {
            self.total += *p;
            *p *= 2;
            Self::note(&mut self.log, n, format!("double {} -> {p}", *p / 2));
        }
    }

    /// Removes every item priced above `max`, keeping the total in step, and logs "drop <price>" for each
    /// (the count in the log line is the number of items before any were dropped). Returns how many.
    pub fn drop_above(&mut self, max: u32) -> usize {
        let n = self.items.len();
        let mut dropped = 0;
        self.items.retain(|&p| {
            if p <= max {
                return true;
            }
            self.total -= p;
            Self::note(&mut self.log, n, format!("drop {p}"));
            dropped += 1;
            false
        });
        dropped
    }

    pub fn total(&self) -> u32 {
        self.total
    }

    pub fn items(&self) -> Vec<u32> {
        self.items.to_vec()
    }

    pub fn log(&self) -> Vec<String> {
        self.log.to_vec()
    }
}
"""

CT = "let mut c = Cart::new();"

P.append(fix(
    "fix-refcell-to-split-borrow", "Fix: RefCell where a split borrow suffices", "hard", "borrow-checker-limits", ["RefCell", "split borrows", "associated functions", "&mut self"],
    """
        `Cart` wraps every field in a `RefCell` to get around borrow errors, and still panics at run time in
        two of its methods. Remove every `RefCell` (and `Cell`): take `&mut self` where a method changes the
        cart, and let the compiler's split borrows do the work. The public methods keep their names, arguments
        and results.
    """,
    CART_STARTER,
    CART_SOLUTION,
    [T("add_and_double", "add 3, 4; double_all", "{ c.add(3); c.add(4); c.double_all(); (c.items(), c.total(), c.log()) }",
       '(vec![6, 8], 14, ["[1] add 3", "[2] add 4", "[2] double 3 -> 6", "[2] double 4 -> 8"].map(String::from).to_vec())', setup=CT),
     T("drop_above", "add 5, 50, 7, 70; drop_above 10", "{ for p in [5, 50, 7, 70] { c.add(p); } (c.drop_above(10), c.items(), c.total(), c.log()[4..].to_vec()) }",
       '(2, vec![5, 7], 12, ["[4] drop 50", "[4] drop 70"].map(String::from).to_vec())', setup=CT),
     T("empty_cart", "double and drop on an empty cart", "{ c.double_all(); (c.drop_above(0), c.total(), c.log().len()) }", "(0, 0, 0)", setup=CT),
     T("drop_is_strict", "add 10; drop_above 10", "{ c.add(10); (c.drop_above(10), c.items()) }", "(0, vec![10])", setup=CT),
     T("double_twice", "add 1; double twice", "{ c.add(1); c.double_all(); c.double_all(); (c.items(), c.total()) }", "(vec![4], 4)", setup=CT)],
    [T("drop_everything", "add 3, 4; drop_above 0", "{ c.add(3); c.add(4); (c.drop_above(0), c.items(), c.total()) }", "(2, vec![], 0)", setup=CT),
     T("zero_price", "add 0; double", "{ c.add(0); c.double_all(); (c.total(), c.log()) }", '(0, ["[1] add 0", "[1] double 0 -> 0"].map(String::from).to_vec())', setup=CT),
     T("add_after_drop", "add 9, 1; drop_above 5; add 2", "{ c.add(9); c.add(1); c.drop_above(5); c.add(2); c.log() }", '["[1] add 9", "[2] add 1", "[2] drop 9", "[2] add 2"].map(String::from).to_vec()', setup=CT),
     T("keeps_order", "add 1, 9, 2, 8, 3; drop_above 5", "{ for p in [1, 9, 2, 8, 3] { c.add(p); } c.drop_above(5); c.items() }", "vec![1, 2, 3]", setup=CT),
     T("large_prices", "add 1 << 30; double", "{ c.add(1 << 30); c.double_all(); (c.total(), c.items()) }", "(1 << 31, vec![1 << 31])", setup=CT),
     T("readers_side_by_side", "add 2; items, total and log read together", "{ c.add(2); let (i, t, l) = (c.items(), c.total(), c.log()); (i, t, l.len()) }", "(vec![2], 2, 1)", setup=CT),
     T("double_then_drop", "add 3, 6; double; drop_above 10", "{ c.add(3); c.add(6); c.double_all(); (c.drop_above(10), c.items(), c.total()) }", "(1, vec![6], 6)", setup=CT),
     T("log_count_is_before_dropping", "add 20, 30, 40; drop_above 25", "{ for p in [20, 30, 40] { c.add(p); } c.drop_above(25); c.log()[3..].to_vec() }", '["[3] drop 30", "[3] drop 40"].map(String::from).to_vec()', setup=CT),
     r"""
     #[test]
     fn random_vs_model() {
         let mut rng = anneal_prelude::Rng::new(6235);
         for _ in 0..300 {
             let mut c = Cart::new();
             let (mut items, mut log): (Vec<u32>, Vec<String>) = (vec![], vec![]);
             let mut ops = Vec::new();
             for _ in 0..8 {
                 match rng.below(3) {
                     0 => {
                         let p = rng.below(20) as u32;
                         c.add(p);
                         items.push(p);
                         log.push(format!("[{}] add {p}", items.len()));
                         ops.push(format!("add {p}"));
                     }
                     1 => {
                         c.double_all();
                         let n = items.len();
                         for p in items.iter_mut() {
                             log.push(format!("[{n}] double {p} -> {}", *p * 2));
                             *p *= 2;
                         }
                         ops.push("double_all".to_string());
                     }
                     _ => {
                         let max = rng.below(30) as u32;
                         let n = items.len();
                         let before = items.len();
                         for p in items.iter().filter(|&&p| p > max) {
                             log.push(format!("[{n}] drop {p}"));
                         }
                         items.retain(|&p| p <= max);
                         ops.push(format!("drop_above {max}"));
                         check!(ops.join(", "), c.drop_above(max), before - items.len());
                     }
                 }
             }
             let total: u32 = items.iter().sum();
             check!(format!("{}; state", ops.join(", ")), (c.items(), c.total(), c.log()), (items, total, log));
         }
     }

     #[test]
     fn big_cart() {
         let mut c = Cart::new();
         for i in 0..100_000 {
             c.add(i % 10);
         }
         c.double_all();
         let dropped = c.drop_above(10);
         check!("100000 items of 0..9, doubled, drop above 10", (dropped, c.total(), c.log().len()), (40_000, 300_000, 240_000));
     }
     """],
    [("rust", "Once the fields are plain, `for p in self.items.iter_mut()` and `self.total += *p` borrow different fields: that's allowed. What isn't allowed is calling `self.note(..)`, a `&mut self` method, inside that loop."),
     ("rust", "Make `note` an associated function over exactly what it touches: the log and the count. `Self::note(&mut self.log, n, msg)` borrows only `self.log`."),
     ("rust", "The item count for `double_all` and `drop_above` is the count at the start of the call, so read it before the loop.")],
    ("""`RefCell` turned compile-time guarantees into run-time checks, and the checks failed: `note` borrows `items` while `double_all` and `drop_above` hold it mutably, so both panic. Without the `RefCell`s the same mistake is a compile error (a `&mut self` call inside a loop over `self.items`), and the fix is the one the compiler points to: split the borrow. Field paths are disjoint (`self.items`, `self.total`, `self.log`), and edition-2021 closures, like `retain`'s, capture only the fields they name. The helper stops taking `self` and takes the fields it needs instead. Methods that change the cart take `&mut self`, which makes the mutation visible in the API.

Reach for `RefCell` when the mutation really happens through shared references you can't restructure (a graph with shared nodes, callbacks into the owner); inside one struct's own methods, split borrows almost always suffice.""", "O(n) per call", "O(1) extra"),
    "When is `RefCell` genuinely the right tool, and what would you check in review when you see one?",
    ["Reach for split borrows before interior mutability.", "Turn a `&mut self` helper into an associated function over the fields it touches.", "`RefCell` moves borrow errors from compile time to run time."],
    rules=dict(types=["RefCell", "Cell", "Rc"], methods=["clone"]),
    related=("L2", "S7"),
    wrong=dict(
        drop_keeps_total=sub(CART_SOLUTION, "            self.total -= p;\n", ""),
        double_adds_new_price=sub(CART_SOLUTION, "            self.total += *p;\n            *p *= 2;\n", "            *p *= 2;\n            self.total += *p;\n"),
        count_after_dropping=sub(CART_SOLUTION, "            Self::note(&mut self.log, n, format!(\"drop {p}\"));\n            dropped += 1;\n", "            dropped += 1;\n            Self::note(&mut self.log, n - dropped, format!(\"drop {p}\"));\n"),
    ),
))



STAGES = [
    ("shared-vs-unique", "Shared vs unique", "easy"),
    ("where-borrows-end", "Where borrows end", "easy"),
    ("reborrows", "Reborrows", "medium"),
    ("iterator-invalidation", "Iterator invalidation", "medium"),
    ("split-borrows", "Split borrows", "medium"),
    ("borrow-checker-limits", "Borrow-checker limits", "hard"),
]

if __name__ == "__main__":
    n = write_track("l2-borrowing", "L2", "Borrowing", "L", "core", 2,
                    "Aliasing XOR mutation: reason about what each borrow covers and how long it lives, instead of fighting the compiler.",
                    STAGES, P, keep={"two-mutable-borrows-of-self"})
    # The hand-written problem keeps its files, but its position follows the spec.
    import os
    import re
    from author import ROOT
    toml = os.path.join(ROOT, "l2-borrowing", "problems", "two-mutable-borrows-of-self", "problem.toml")
    pos = [p["slug"] for p in P].index("two-mutable-borrows-of-self") + 1
    if os.path.exists(toml):
        text = open(toml).read()
        open(toml, "w").write(re.sub(r"(?m)^order = \d+$", f"order = {pos}", text))
    print("L2", n)
