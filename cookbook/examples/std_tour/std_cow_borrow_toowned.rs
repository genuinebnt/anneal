//! RECIPE 1003 (std): Cow<'a, B>, Borrow, ToOwned: allocate only when you must; Borrow vs AsRef vs Deref.
//! LEVEL: intermediate
//! Use in: escaping/normalising text, parsers returning slices, config structs that borrow or own, zero-copy APIs.   Run: cargo run --release --example std_cow_borrow_toowned
//! COMMON USES: HTML/URL/shell escaping that usually returns the input unchanged, `String::from_utf8_lossy`,
//! path normalisation, error messages that are mostly static, deserialisers borrowing from the input buffer (serde `Cow<'de, str>`).
//! METHODS: (everyday first) Cow::Borrowed, Cow::Owned, Cow::into_owned, Cow::to_mut, Cow::from, Cow::as_ref, matches!(Cow::Borrowed(_)),
//! METHODS: ToOwned::to_owned, ToOwned::clone_into, Borrow::borrow, BorrowMut::borrow_mut, AsRef::as_ref, String::from_utf8_lossy,
//! METHODS: str::replace, str::contains, Cow::clone, Cow::add_assign, Cow::eq, Cow::into
//! DOCS: https://doc.rust-lang.org/std/borrow/enum.Cow.html
//! DEFINED IN: Cow/ToOwned alloc (`alloc::borrow`); Borrow/BorrowMut core (`core::borrow`).
//! NOT COVERED (and why): Cow::is_borrowed/is_owned are unstable, so section 1 uses `matches!`; Cow<CStr>/Cow<OsStr>/Cow<Path> behave the same (see std_env_args_vars).
#![allow(dead_code)]
//!
//! CHEAT SHEET (I want to ... -> use ...)
//!   - return input unchanged when no work is needed -> `Cow<'a, str>` (Borrowed) else Owned
//!   - mutate lazily (clone on first write)          -> `to_mut()`         - get an owned value out -> `into_owned()`
//!   - function accepts either borrowed or owned     -> `impl Into<Cow<'a, str>>`
//!   - struct that sometimes borrows from input      -> `struct S<'a> { name: Cow<'a, str> }`
//!   - bytes of unknown encoding to text             -> `String::from_utf8_lossy` (Cow)
//!   - static strings + occasional dynamic ones      -> `Cow<'static, str>`
//!   - generic lookup with borrowed key              -> `K: Borrow<Q>`     - generic read-only view -> `AsRef<T>`
//!   - unsized type's owned form                     -> `ToOwned` (str->String, [T]->Vec<T>)
//! Read: https://doc.rust-lang.org/std/borrow/enum.Cow.html , https://doc.rust-lang.org/std/borrow/trait.Borrow.html , https://rust-lang.github.io/api-guidelines/flexibility.html
//! TRY NEXT:
//!   - [S] In `escape_html` count the allocations with a global counting allocator-free trick: compare `ptr` of input and output with `std::ptr::eq` for clean and dirty input.
//!   - [S] Predict what `Cow::to_mut` does to the original `&str` after the call; assert it is untouched.
//!   - [M] Write `normalize_path(&str) -> Cow<str>` collapsing `//` and `/./`; borrowed when already normal; differential test vs an always-allocating oracle on 2000 seeded random paths.
//!   - [M] Add a `Cow<'a, [u32]>` method `sorted()` that borrows when already sorted; measure the allocation difference.
//!   - [A] PROJECT: combine with std_convert_from_into_asref and std_fmt_display_debug_deep: a template renderer whose `render(&'a str, vars) -> Cow<'a, str>` returns Borrowed for templates without placeholders.
use std::borrow::{Borrow, Cow};
use std::collections::{BTreeSet, HashMap};

const COVERED: &[&str] = &[
    "Borrowed",
    "Owned",
    "into_owned",
    "to_mut",
    "from",
    "as_ref",
    "to_owned",
    "clone_into",
    "borrow",
    "borrow_mut",
    "deref",
    "from_utf8_lossy",
    "add",
    "add_assign",
    "clone",
];

fn is_borrowed<T: ?Sized + ToOwned>(c: &Cow<'_, T>) -> bool {
    matches!(c, Cow::Borrowed(_))
}

// ---- 0. Everyday core: escape that allocates only when needed -------------------------------------------------
fn escape_html(input: &str) -> Cow<'_, str> {
    // Fast path: scan once; if nothing to escape return the SAME slice (no allocation).
    if !input.contains(['<', '>', '&', '"']) {
        return Cow::Borrowed(input);
    }
    let mut out = String::with_capacity(input.len() + 8);
    for c in input.chars() {
        match c {
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '&' => out.push_str("&amp;"),
            '"' => out.push_str("&quot;"),
            c => out.push(c),
        }
    }
    Cow::Owned(out)
}

fn section_everyday() {
    let clean = "plain text";
    let r = escape_html(clean);
    assert!(is_borrowed(&r));
    assert!(
        std::ptr::eq(r.as_ref().as_ptr(), clean.as_ptr()),
        "same bytes, zero copy"
    );
    let dirty = escape_html("a<b & \"c\"");
    assert!(!is_borrowed(&dirty));
    assert_eq!(dirty, "a&lt;b &amp; &quot;c&quot;");
    // Cow<str> derefs to &str, so all str methods work and it compares with &str.
    assert_eq!(r.len(), 10);
    assert_eq!(r, "plain text");
    // into_owned: get a String; free if already Owned, one allocation if Borrowed.
    let s: String = dirty.into_owned();
    assert!(s.starts_with("a&lt;"));
    println!("0. escape_html ok");
}

// ---- 1. to_mut: clone-on-write inside a function --------------------------------------------------------------
fn strip_trailing_slashes(path: &str) -> Cow<'_, str> {
    let trimmed = path.trim_end_matches('/');
    if trimmed.len() == path.len() {
        Cow::Borrowed(path)
    } else {
        Cow::Owned(trimmed.to_owned())
    }
}
fn ensure_exclaim(s: &mut Cow<'_, str>) {
    // to_mut: if Borrowed, clones into Owned FIRST, then gives &mut String. Subsequent calls are free.
    if !s.ends_with('!') {
        s.to_mut().push('!');
    }
}
fn section_to_mut() {
    let original = String::from("hi");
    let mut c = Cow::Borrowed(original.as_str());
    ensure_exclaim(&mut c);
    assert!(!is_borrowed(&c));
    let mut e: Cow<str> = Cow::Borrowed("");
    e += "rhs";
    e += "";
    assert!(is_borrowed(&e) && e == "rhs");
    assert_eq!(c, "hi!");
    assert_eq!(original, "hi", "the source is never mutated");
    let mut already = Cow::Borrowed("ok!");
    ensure_exclaim(&mut already);
    assert!(is_borrowed(&already), "no write needed => no allocation");
    assert!(is_borrowed(&strip_trailing_slashes("a/b")));
    assert_eq!(strip_trailing_slashes("a/b///"), "a/b");
    // Constructing: Cow::from for &str and String; `.into()` works too.
    let a: Cow<str> = "lit".into();
    let b: Cow<str> = String::from("own").into();
    assert!(is_borrowed(&a) && !is_borrowed(&b));
    // + and += on Cow<str> (since 1.14). No allocation when one side is "": an empty rhs leaves self unchanged, and an
    // empty self simply becomes rhs (still Borrowed if rhs is).
    let mut c: Cow<str> = Cow::Borrowed("a");
    c += "b";
    assert_eq!(c, "ab");
    assert!(!is_borrowed(&c));
    println!("1. to_mut ok");
}

// ---- 2. Cow in function arguments and struct fields -----------------------------------------------------------
#[derive(Debug, Clone, PartialEq)]
struct Label<'a> {
    text: Cow<'a, str>,
}
impl<'a> Label<'a> {
    // Accept anything convertible into Cow: &'a str stays borrowed, String is moved in, no extra copy.
    fn new(text: impl Into<Cow<'a, str>>) -> Self {
        Label { text: text.into() }
    }
    fn into_static(self) -> Label<'static> {
        Label {
            text: Cow::Owned(self.text.into_owned()),
        } // detach from the input lifetime
    }
}
fn error_message(code: u32) -> Cow<'static, str> {
    // Static messages for known codes (no allocation), formatted only for the rare dynamic case.
    match code {
        404 => Cow::Borrowed("not found"),
        500 => Cow::Borrowed("internal error"),
        n => Cow::Owned(format!("unknown status {n}")),
    }
}
fn section_struct_and_static() {
    let buf = String::from("from buffer");
    let l = Label::new(buf.as_str());
    assert!(is_borrowed(&l.text));
    let owned_label = Label::new(format!("n={}", 5));
    assert!(!is_borrowed(&owned_label.text));
    let detached = l.clone().into_static(); // can outlive `buf`
    drop(buf);
    assert_eq!(detached.text, "from buffer");
    assert!(is_borrowed(&error_message(404)) && !is_borrowed(&error_message(418)));
    assert_eq!(error_message(418), "unknown status 418");
    println!("2. struct/static ok");
}

// ---- 3. from_utf8_lossy, Cow<[T]>, ToOwned for custom types ---------------------------------------------------
fn sorted_unique(xs: &[u32]) -> Cow<'_, [u32]> {
    if xs.windows(2).all(|w| w[0] < w[1]) {
        Cow::Borrowed(xs)
    } else {
        let mut v = xs.to_vec();
        v.sort_unstable();
        v.dedup();
        Cow::Owned(v)
    }
}
// ToOwned lets Cow work with YOUR unsized type: here a "path-like" str wrapper with a borrowed form and owned form.
#[derive(Debug, PartialEq)]
#[repr(transparent)]
struct Slug(str);
#[derive(Debug, PartialEq, Clone)]
struct SlugBuf(String);
impl Borrow<Slug> for SlugBuf {
    fn borrow(&self) -> &Slug {
        Slug::new(&self.0)
    }
}
impl Slug {
    fn new(s: &str) -> &Slug {
        // SAFETY: Slug is #[repr(transparent)] over str, so &str and &Slug have identical layout.
        unsafe { &*(s as *const str as *const Slug) }
    }
}
impl ToOwned for Slug {
    type Owned = SlugBuf;
    fn to_owned(&self) -> SlugBuf {
        SlugBuf(self.0.to_owned())
    }
}
fn section_slices_and_custom() {
    // from_utf8_lossy: valid input borrows; invalid input allocates with U+FFFD replacement.
    let ok = String::from_utf8_lossy(b"hello");
    assert!(is_borrowed(&ok));
    let bad = String::from_utf8_lossy(&[b'a', 0xFF, b'b']);
    assert!(!is_borrowed(&bad));
    assert_eq!(bad, "a\u{FFFD}b");
    // Cow<[T]>.
    assert!(is_borrowed(&sorted_unique(&[1, 2, 3])));
    assert_eq!(&*sorted_unique(&[3, 1, 3]), &[1, 3]);
    // Custom ToOwned.
    let s = Slug::new("hello-world");
    let c: Cow<Slug> = Cow::Borrowed(s);
    let owned: SlugBuf = c.into_owned();
    assert_eq!(owned, SlugBuf("hello-world".into()));
    // clone_into: reuse an existing allocation (String::clone_from semantics for ToOwned).
    let mut target = String::with_capacity(64);
    "reuse me".clone_into(&mut target);
    assert_eq!(target, "reuse me");
    assert!(target.capacity() >= 64);
    println!("3. Cow<[T]>/lossy/ToOwned ok");
}

// ---- 4. Borrow vs AsRef vs Deref comparison -------------------------------------------------------------------
// Borrow<T>   : "I am equivalent to T for Eq/Ord/Hash". Used by HashMap/BTreeMap/HashSet lookups. Has reflexive blanket impl.
// AsRef<T>    : cheap reference-to-reference conversion, NO promise about Eq/Hash. Used in generic fn params.
// Deref<Target>: ONE target type, applied automatically (coercion, method resolution). For smart pointers.
fn lookup<K: Borrow<Q> + Eq + std::hash::Hash, Q: ?Sized + Eq + std::hash::Hash>(
    m: &HashMap<K, i32>,
    k: &Q,
) -> Option<i32> {
    m.get(k).copied()
}
fn len_as_ref<T: AsRef<str>>(t: T) -> usize {
    t.as_ref().len()
}
struct CaseInsensitive(String); // AsRef<str> is fine, Borrow<str> would be WRONG: Eq differs from str's Eq.
impl AsRef<str> for CaseInsensitive {
    fn as_ref(&self) -> &str {
        &self.0
    }
}
impl PartialEq for CaseInsensitive {
    fn eq(&self, o: &Self) -> bool {
        self.0.eq_ignore_ascii_case(&o.0)
    }
}
fn section_borrow_vs_asref_vs_deref() {
    // BorrowMut::borrow_mut: String lends itself as &mut str.
    let mut owned = String::from("abc");
    std::borrow::BorrowMut::<str>::borrow_mut(&mut owned).make_ascii_uppercase();
    assert_eq!(owned, "ABC");
    let mut m: HashMap<String, i32> = HashMap::new();
    m.insert("a".into(), 1);
    assert_eq!(lookup(&m, "a"), Some(1)); // Q = str, K = String
    assert_eq!(lookup(&m, &"a".to_string()), Some(1)); // Q = String (reflexive Borrow)
    let mut vm: HashMap<Vec<u8>, i32> = HashMap::new();
    vm.insert(vec![1, 2], 5);
    assert_eq!(vm.get(&[1u8, 2][..]), Some(&5)); // Vec<u8>: Borrow<[u8]>
    let set: BTreeSet<String> = ["x".to_string()].into();
    assert!(set.contains("x"));
    // AsRef: wide, no semantic promise.
    assert_eq!(
        (
            len_as_ref("ab"),
            len_as_ref(String::from("abc")),
            len_as_ref(Cow::Borrowed("abcd"))
        ),
        (2, 3, 4)
    );
    let (a, b) = (CaseInsensitive("Hi".into()), CaseInsensitive("hI".into()));
    assert!(a == b && a.0 != b.0);
    assert_eq!(a.as_ref(), "Hi");
    // Deref: automatic. `&String` coerces to `&str` at call sites without AsRef.
    fn takes(s: &str) -> usize {
        s.len()
    }
    let owned = String::from("xyz");
    assert_eq!(takes(&owned), 3);
    let boxed: Box<String> = Box::new(owned);
    assert_eq!(takes(&boxed), 3); // Box<String> -> String -> str via two derefs
    // Table (asserted where observable):
    // trait    | promises Eq/Hash? | automatic? | typical bound position
    // Borrow   | yes               | no         | map key lookup K: Borrow<Q>
    // AsRef    | no                | no         | fn param T: AsRef<U>
    // Deref    | no                | yes        | smart pointers, coercion
    println!("4. Borrow/AsRef/Deref ok");
}

// ---- 5. Production tips and tricks ---------------------------------------------------------------------------
fn section_production_tips() {
    // 1. Cow shines when the common path needs NO change (escaping, trimming, normalising): measure the hit rate first.
    let inputs = ["abc", "a<b", "xyz", "q"];
    let allocs = inputs
        .iter()
        .filter(|s| !is_borrowed(&escape_html(s)))
        .count();
    assert_eq!(allocs, 1);
    // 2. Don't use Cow when you ALWAYS modify: just return String (clippy has no lint; reviewers do).
    // 3. `Cow<'static, str>` for names/labels (metrics, error codes) lets `&'static str` and `format!` coexist
    //    without leaking: used by tracing/metrics APIs.
    let name: Cow<'static, str> = if allocs > 0 {
        format!("dyn_{allocs}").into()
    } else {
        "static".into()
    };
    assert_eq!(name, "dyn_1");
    // 4. Returning Cow ties the output lifetime to the input: if callers need 'static, call into_owned at the boundary.
    // 5. `to_string()` / `String::from` on a Cow<str> is an extra copy when it is Owned; use into_owned (clippy::inefficient_to_string is for &&str).
    let c: Cow<str> = Cow::Owned("x".into());
    let s: String = c.into_owned();
    assert_eq!(s, "x");
    // 6. Compare a Cow with &str directly (PartialEq impls exist); don't convert first.
    assert!(Cow::Borrowed("a") == "a");
    // 7. Deserialisers: `#[serde(borrow)] Cow<'a, str>` borrows from the input buffer when there are no escapes (serde docs).
    // 8. Prefer `&str` params and `impl Into<Cow>` only for constructors that store; Cow params on plain functions are rarely needed.
    // 9. `Cow<[u8]>` for byte buffers that are usually mmapped slices but sometimes decoded copies.
    let data = [1u8, 2, 3];
    let bytes: Cow<[u8]> = Cow::Borrowed(&data);
    assert_eq!(bytes.len(), 3);
    // 10. Clone of a Borrowed Cow is free (copies the reference); clone of Owned allocates.
    let b = Cow::Borrowed("zz");
    assert!(is_borrowed(&b.clone()));
    // 11. Replace pattern: `str::replace` always allocates; guard with contains to skip when no match.
    fn replace_cow<'a>(s: &'a str, from: &str, to: &str) -> Cow<'a, str> {
        if s.contains(from) {
            Cow::Owned(s.replace(from, to))
        } else {
            Cow::Borrowed(s)
        }
    }
    assert!(is_borrowed(&replace_cow("abc", "x", "y")));
    assert_eq!(replace_cow("axc", "x", "y"), "ayc");
    println!("5. production tips ok");
}

fn main() {
    let mut seen = std::collections::HashSet::new();
    assert!(
        COVERED.iter().all(|m| seen.insert(*m)),
        "COVERED has duplicates"
    );
    section_everyday();
    section_to_mut();
    section_struct_and_static();
    section_slices_and_custom();
    section_borrow_vs_asref_vs_deref();
    section_production_tips();
    println!("all {} items exercised; done", COVERED.len());
}
