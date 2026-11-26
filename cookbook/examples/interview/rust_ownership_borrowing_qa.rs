//! RECIPE 600 (interview): Ownership, borrowing, lifetimes and Drop: the questions every Rust interview opens with.
//! LEVEL: intermediate
//! Use in: interview preparation, onboarding reviews, explaining borrow-checker errors to teammates.   Run: cargo run --release --example rust_ownership_borrowing_qa
//! COMMON USES: screening interviews, code-review vocabulary, debugging E0382/E0499/E0502/E0597/E0505 errors.
#![allow(dead_code)]
//! Read: https://www.techinterview.org/post/3233474457/rust-interview-questions-2025-ownership-borrowing-lifetimes-traits-async-await-error-handling-smart-pointers-concurrency/
//! Read: https://www.techinterview.org/post/3233460429/rust-interview-questions/ and https://codeforgeek.com/rust-interview-questions/
//! Read: https://github.com/devinterview-io/rust-interview-questions and https://flakm.github.io/posts/rust_interview_questions
//!
//! QUESTION BANK
//! Q1. What is the difference between move, Copy and Clone? [Easy] [asked: very common]
//! Q2. Why does `let a = b;` invalidate `b` when `b` is a String but not when it is an i32? [Easy] [asked: very common]
//! Q3. State the borrowing rules. Why "one &mut XOR many &"? [Easy] [asked: very common]
//! Q4. What are non-lexical lifetimes (NLL)? Show code that compiles only because of NLL. [Medium] [asked: very common]
//! Q5. How does Rust prevent dangling references? [Easy] [asked: very common]
//! Q6. What are the lifetime elision rules? When must you write lifetimes explicitly? [Medium] [asked: very common]
//! Q7. Does `'static` mean "lives forever"? Difference between `T: 'static` and `&'static T`? [Hard] [asked: very common]
//! Q8. String vs &str vs &String: which should a function parameter take? [Easy] [asked: very common]
//! Q9. What are slices, and how does `split_at_mut` give two `&mut` into one buffer? [Medium] [asked: common]
//! Q10. Shadowing vs mutation: what is the difference? [Easy] [asked: common]
//! Q11. In what order are values dropped? [Medium] [asked: very common]
//! Q12. What happens on a partial move out of a struct? What is still usable? [Medium] [asked: common]
//! Q13. How do `mem::take`, `mem::replace` and `mem::swap` help move out of a `&mut`? [Medium] [asked: common]
//! Q14. How do you mutate a collection while iterating it? [Medium] [asked: common]
//! Q15. What do `Option<&mut T>`, `as_mut`, `as_ref` and `take` do? [Medium] [asked: common]
//! Q16. Why does `v.push(v.len())` compile? (two-phase borrows) [Hard] [asked: occasional]
//! Q17. Is `clone()` always the right fix for a borrow-checker error? [Medium] [asked: very common]

use std::cell::RefCell;
use std::collections::HashMap;
use std::mem;
use std::rc::Rc;

// ---------------------------------------------------------------------------
// Q1. move vs Copy vs Clone
// ---------------------------------------------------------------------------
/// ANSWER: A *move* is a bitwise transfer of ownership; the source is statically dead afterwards.
/// `Copy` marks types whose bitwise duplicate is a complete, valid value (no heap, no Drop), so
/// assignment copies and both stay usable. `Clone` is an explicit, possibly expensive, user-defined
/// duplicate (`.clone()`); every `Copy` type is `Clone`, but `Copy` cannot be implemented by hand
/// logic: it is always a memcpy. A type that implements `Drop` can never be `Copy`.
///
/// REALLY TESTING: do you know that move is the default and that it is a memcpy, not a deep copy?
/// WRONG ANSWERS: "move copies the heap data" (it copies the 24-byte String header only);
/// "Clone is automatic" (it is not, you must call it).
/// FOLLOW-UP: why can `&T` be Copy but `&mut T` not? (aliasing: two live `&mut` would break XOR).
#[derive(Debug, Clone, Copy, PartialEq)]
struct Point {
    x: i32,
    y: i32,
}
#[derive(Debug, Clone, PartialEq)]
struct Named {
    name: String,
}
fn q1_move_copy_clone() {
    let p = Point { x: 1, y: 2 };
    let q = p; // Copy: p is still usable
    assert_eq!(p, q);

    let n = Named { name: "a".into() };
    let m = n.clone(); // explicit deep copy
    assert_eq!(n, m);
    assert_ne!(
        n.name.as_ptr(),
        m.name.as_ptr(),
        "clone allocated a new buffer"
    );

    let moved = n; // move: header copied, heap buffer NOT copied
    assert_eq!(moved.name, "a");
    // println!("{:?}", n); // error[E0382]: borrow of moved value: `n`
    // Compile-time property: `Point: Copy` is checked by this helper.
    fn is_copy<T: Copy>() {}
    is_copy::<Point>();
    is_copy::<&Named>(); // shared refs are Copy
    // is_copy::<Named>();   // error[E0277]: the trait bound `String: Copy` is not satisfied
    // is_copy::<&mut i32>(); // error[E0277]: `&mut i32` is not Copy
}

// ---------------------------------------------------------------------------
// Q2. why `let a = b` invalidates b for String
// ---------------------------------------------------------------------------
/// ANSWER: A String is {ptr, len, cap} owning a heap buffer. Copying that header would create two
/// owners and a double free at scope end. Rust instead moves: the compiler marks the source
/// uninitialised and runs the destructor only once, for the destination. i32 has no destructor and
/// no owned resource, so it is Copy and both bindings are independent values.
///
/// REALLY TESTING: the link between ownership and RAII/Drop (single owner => single free).
/// WRONG: "because Strings are big" (a `[u8; 1_000_000]` is Copy and stays usable; the reason is
/// ownership of a resource, not size).
/// FOLLOW-UP: what does the compiler do for conditional moves? (drop flags at runtime).
fn q2_let_a_equals_b() {
    let b = String::from("hello");
    let heap_ptr = b.as_ptr();
    let a = b; // move
    assert_eq!(
        a.as_ptr(),
        heap_ptr,
        "the same heap buffer: only the header moved"
    );

    let big = [7u8; 4096];
    let big2 = big; // Copy despite being large
    assert_eq!(big[0], big2[0]);

    let x = 5;
    let y = x;
    assert_eq!(x + y, 10);

    // Conditional move => the compiler tracks it with a hidden drop flag.
    let s = String::from("maybe");
    let keep = std::env::args().count() > 1000; // false, but unknown at compile time
    if keep {
        drop(s);
    } else {
        assert_eq!(s, "maybe");
    }
    // println!("{}", s); // error[E0382]: borrow of possibly-moved value: `s`
}

// ---------------------------------------------------------------------------
// Q3. borrowing rules
// ---------------------------------------------------------------------------
/// ANSWER: At any time you may have EITHER any number of shared references `&T` OR exactly one
/// exclusive reference `&mut T`, and every reference must not outlive its referent. This forbids
/// aliasing + mutation, which is the root of data races, iterator invalidation and use-after-free.
/// Shared refs may read, `&mut` may read and write, and while `&mut` is live nothing else (even
/// the owner) may touch the value.
///
/// REALLY TESTING: can you connect the rule to a *bug class*, not recite it.
/// WRONG: "you can have one &mut and many & at once" (no), or "the rules only matter across threads"
/// (single-threaded iterator invalidation is the classic example).
/// FOLLOW-UP: what about `Cell`/`RefCell`? They move the check to runtime (interior mutability).
fn q3_borrow_rules() {
    let mut v = vec![1, 2, 3];
    let a = &v;
    let b = &v; // many shared: fine
    assert_eq!(a.len() + b.len(), 6);

    let m = &mut v; // exclusive: a and b are dead (NLL)
    m.push(4);
    assert_eq!(v, [1, 2, 3, 4]);

    // FAILING (E0499): two simultaneous &mut
    //   let m1 = &mut v;
    //   let m2 = &mut v;   // error[E0499]: cannot borrow `v` as mutable more than once at a time
    //   m1.push(1); m2.push(2);
    // FAILING (E0502): & and &mut overlap
    //   let first = &v[0];
    //   v.push(9);          // error[E0502]: cannot borrow `v` as mutable because it is also borrowed as immutable
    //   println!("{first}"); // (the push may reallocate and leave `first` dangling)
    // FIXED: finish using the shared borrow first, or copy the value out.
    let first = v[0]; // i32 is Copy: no borrow survives
    v.push(9);
    assert_eq!((first, v.len()), (1, 5));
}

// ---------------------------------------------------------------------------
// Q4. NLL
// ---------------------------------------------------------------------------
/// ANSWER: Since Rust 2018 a borrow lasts from its creation to its LAST USE on any control-flow path,
/// not to the end of the lexical block. Before NLL the borrow lived to the closing brace, so many
/// obviously fine programs were rejected. NLL is computed on the MIR control-flow graph.
///
/// REALLY TESTING: whether you think scopes or liveness decide borrow validity (liveness).
/// WRONG: "a reference lives until the end of its block" (that is a *variable's* scope, not the borrow).
/// FOLLOW-UP: what is the remaining limitation? (Polonius cases: conditional return of a borrow,
/// `get_or_insert` patterns) - see the last function below.
fn q4_nll() {
    let mut s = String::from("a");
    let r = &s; // shared borrow starts
    let n = r.len(); // ... and ENDS here (last use of r)
    s.push('b'); // fine under NLL
    assert_eq!((n, s.as_str()), (1, "ab"));

    // The variant that does NOT compile: use of the reference after the mutation.
    //   let r = &s;
    //   s.push('c');             // error[E0502]
    //   println!("{}", r);       // r is still live here
    // Fix: move the use before the mutation, or clone what you need:
    let r = &s;
    let snapshot = r.clone();
    s.push('c');
    assert_eq!((snapshot.as_str(), s.as_str()), ("ab", "abc"));

    // Liveness is per path: this compiles because `r` is dead on the branch that mutates.
    let mut data = vec![1, 2, 3];
    let r = &data;
    if r.len() > 100 {
        println!("{:?}", r);
    } else {
        data.push(4); // r is not used after this point on this path
    }
    assert_eq!(data.len(), 4);

    // Known NLL gap (needs Polonius): returning a borrow from one branch and using `map` in another.
    fn get_or_insert(map: &mut HashMap<u32, String>, k: u32) -> &String {
        // The commonly-written `if let Some(v) = map.get(&k) { return v; } map.insert(..)`
        // is rejected today (E0502). The entry API expresses it without the problem:
        map.entry(k).or_insert_with(|| format!("v{k}"))
    }
    let mut m = HashMap::new();
    assert_eq!(get_or_insert(&mut m, 3), "v3");
}

// ---------------------------------------------------------------------------
// Q5. dangling references
// ---------------------------------------------------------------------------
/// ANSWER: Every reference carries a lifetime, and the compiler proves that the referent outlives
/// every use of the reference. A function cannot return a reference to a local because the local is
/// dropped at the closing brace; the fix is to return the owned value (or borrow from an argument).
///
/// REALLY TESTING: do you understand that lifetimes are a compile-time-only proof, zero runtime cost.
/// WRONG: "Rust uses a garbage collector / reference counting for this" (no runtime involved).
/// FOLLOW-UP: how does C++ differ? (it compiles and is undefined behaviour).
fn q5_dangling() {
    // FAILING (E0106 / E0515):
    //   fn dangle() -> &String { let s = String::new(); &s }            // E0106 missing lifetime specifier
    //   fn dangle<'a>() -> &'a String { let s = String::new(); &s }     // E0515 returns a reference to local
    // FAILING (E0597): inner value dropped while still borrowed
    //   let r; { let x = 5; r = &x; } println!("{r}");   // error[E0597]: `x` does not live long enough
    // FIXED 1: return ownership.
    fn no_dangle() -> String {
        let s = String::from("owned");
        s
    }
    // FIXED 2: borrow from the caller, whose data outlives the call.
    fn first_word(s: &str) -> &str {
        s.split(' ').next().unwrap_or("")
    }
    let owner = no_dangle();
    let w = first_word(&owner);
    assert_eq!(w, "owned");
    // FIXED 3: extend the owner's scope.
    let x = 5;
    let r = &x;
    assert_eq!(*r, 5);
}

// ---------------------------------------------------------------------------
// Q6. elision rules
// ---------------------------------------------------------------------------
/// ANSWER: (1) each elided input lifetime becomes a distinct parameter; (2) if there is exactly one
/// input lifetime it is assigned to all elided outputs; (3) if there is `&self`/`&mut self`, its
/// lifetime is assigned to all elided outputs. If none applies the output lifetime is ambiguous
/// and you must annotate (E0106). Annotations never change how long data lives, they only tell the
/// checker how the inputs and outputs are related.
///
/// REALLY TESTING: can you expand `fn f(&self, x: &str) -> &str` by hand (output tied to self).
/// WRONG: "lifetime annotations extend a lifetime".
/// FOLLOW-UP: what does `fn longest<'a>(a: &'a str, b: &'a str) -> &'a str` promise? The result is
/// valid only as long as BOTH inputs are (the intersection).
struct Parser<'src> {
    src: &'src str,
}
impl<'src> Parser<'src> {
    // Rule 3: output tied to &self, written out: fn name<'s>(&'s self, other: &str) -> &'s str
    fn name(&self, _other: &str) -> &str {
        self.src
    }
    // Explicit: output tied to the *source*, not to the parser borrow, so it outlives the parser.
    fn source(&self) -> &'src str {
        self.src
    }
}
fn rule1_rule2(s: &str) -> &str {
    // expands to fn<'a>(s: &'a str) -> &'a str
    &s[..1]
}
fn longest<'a>(a: &'a str, b: &'a str) -> &'a str {
    // Rule 2 cannot apply (two inputs), so annotation is mandatory.
    if a.len() >= b.len() { a } else { b }
}
fn q6_elision() {
    assert_eq!(rule1_rule2("xyz"), "x");
    let text = String::from("source text");
    let leaked: &str;
    {
        let p = Parser { src: &text };
        assert_eq!(p.name("ignored"), "source text");
        leaked = p.source(); // allowed: tied to `text`, not to `p`
    }
    assert_eq!(leaked, "source text");

    let a = String::from("longer one");
    let result;
    {
        let b = String::from("short");
        result = longest(&a, &b).len(); // use inside the scope of b
    }
    assert_eq!(result, 10);
    // FAILING (E0597): using the result after `b` died.
    //   let r; { let b = String::from("s"); r = longest(&a, &b); } println!("{r}");
    // FAILING (E0106): fn pick(a: &str, b: &str) -> &str   // two inputs and no &self: ambiguous
}

// ---------------------------------------------------------------------------
// Q7. 'static
// ---------------------------------------------------------------------------
/// ANSWER: `&'static T` is a reference that is valid for the whole program (string literals,
/// leaked boxes, statics). `T: 'static` only means T contains no non-'static borrows: an owned
/// `String` or `Vec<i32>` is `'static` even though it is created and dropped at runtime. So
/// `thread::spawn` requiring `F: 'static` means "owns all its data", not "lives forever". Data can
/// be made `&'static` deliberately with `Box::leak` (memory is never freed until exit).
///
/// REALLY TESTING: the most common lifetime misconception.
/// WRONG: "'static values are never dropped" (an owned String: 'static is dropped normally) and
/// "the compiler suggests 'static so I should add it" (usually the real fix is `move` or owning).
/// FOLLOW-UP: how to borrow local data in threads without 'static? `thread::scope`.
fn q7_static() {
    fn requires_static<T: 'static>(_t: T) {}
    requires_static(String::from("owned data is 'static"));
    requires_static(vec![1, 2, 3]);
    requires_static("literal");
    let local = String::from("borrowed");
    // requires_static(&local); // error[E0597]: `local` does not live long enough (a short borrow is not 'static)
    let _ = &local;

    // an owned 'static type is dropped like anything else
    let dropped = Rc::new(RefCell::new(false));
    struct Flag(Rc<RefCell<bool>>);
    impl Drop for Flag {
        fn drop(&mut self) {
            *self.0.borrow_mut() = true;
        }
    }
    requires_static(Flag(dropped.clone())); // moved in and dropped at end of the call
    assert!(*dropped.borrow(), "T: 'static types are still dropped");

    // thread::spawn needs F: 'static. Borrowing a local fails (E0373); `move` fixes it.
    //   let v = vec![1]; let h = std::thread::spawn(|| println!("{:?}", v));
    //   // error[E0373]: closure may outlive the current function, but it borrows `v`
    let v = vec![1, 2, 3];
    let h = std::thread::spawn(move || v.iter().sum::<i32>());
    assert_eq!(h.join().unwrap(), 6);
    // Alternative: scoped threads may borrow.
    let data = vec![10, 20];
    let total = std::thread::scope(|s| s.spawn(|| data.iter().sum::<i32>()).join().unwrap());
    assert_eq!(total, 30);

    // Deliberately leaking to obtain &'static (memory is never reclaimed).
    let leaked: &'static mut Vec<i32> = Box::leak(Box::new(vec![1, 2]));
    leaked.push(3);
    let as_static: &'static [i32] = leaked;
    assert_eq!(as_static, [1, 2, 3]);
}

// ---------------------------------------------------------------------------
// Q8. String vs &str vs &String
// ---------------------------------------------------------------------------
/// ANSWER: `String` is an owned, growable, heap UTF-8 buffer (ptr,len,cap). `&str` is a borrowed
/// view (ptr,len) into any UTF-8 bytes: heap, static data or a stack buffer. `&String` is a
/// pointer to the String header and adds nothing over `&str`; it auto-derefs to `&str` (deref
/// coercion). Parameters should take `&str` so callers can pass literals, `&String`, `&Box<str>`,
/// and slices of other strings without allocation.
///
/// REALLY TESTING: API design sense and deref coercion.
/// WRONG: "&str is a smaller String" / "use String params to be safe" (forces callers to allocate).
/// FOLLOW-UP: when should a parameter take `String`? When the function needs to store/own it.
/// `impl Into<String>` / `impl AsRef<str>` generalise further.
fn takes_str(s: &str) -> usize {
    s.len()
}
fn takes_string_ref(s: &String) -> usize {
    s.len()
}
fn takes_owned(s: String) -> String {
    s
}
fn q8_string_vs_str() {
    let owned = String::from("hello world");
    assert_eq!(takes_str("literal"), 7); // &'static str
    assert_eq!(takes_str(&owned), 11); // &String -> &str by deref coercion
    assert_eq!(takes_str(&owned[..5]), 5); // sub-slice, no allocation
    assert_eq!(takes_string_ref(&owned), 11);
    // takes_string_ref("literal"); // error[E0308]: expected `&String`, found `&str`
    assert_eq!(takes_owned(owned.clone()), owned);

    assert_eq!(
        std::mem::size_of::<&str>(),
        2 * std::mem::size_of::<usize>()
    );
    assert_eq!(
        std::mem::size_of::<String>(),
        3 * std::mem::size_of::<usize>()
    );
    // A &str slice points INTO the owner's buffer:
    let sub = &owned[6..];
    assert_eq!(sub.as_ptr(), unsafe_free_offset(&owned, 6));
    fn unsafe_free_offset(s: &str, off: usize) -> *const u8 {
        s.as_bytes()[off..].as_ptr()
    }
}

// ---------------------------------------------------------------------------
// Q9. slices and split_at_mut
// ---------------------------------------------------------------------------
/// ANSWER: A slice `&[T]`/`&mut [T]` is a fat pointer (ptr,len) borrowing a contiguous region.
/// The borrow checker sees `&mut v[0]` and `&mut v[1]` as two borrows of all of `v` (it does not
/// reason about indices), so it rejects them (E0499). `split_at_mut` hands out two non-overlapping
/// `&mut` halves; it is implemented with a small `unsafe` block that is sound because the halves
/// are disjoint. Also see `chunks_mut`, `iter_mut`, `swap`.
///
/// REALLY TESTING: do you know the checker is conservative and how std encapsulates unsafe.
/// WRONG: "it is impossible to get two &mut into one Vec".
/// FOLLOW-UP: how would you swap two elements? `v.swap(i, j)`.
fn q9_slices() {
    let mut v = vec![1, 2, 3, 4, 5];
    // FAILING: let (a, b) = (&mut v[0], &mut v[4]);   // error[E0499]
    let (left, right) = v.split_at_mut(2);
    left[0] += right[0]; // 1 + 3
    right[2] *= 10;
    assert_eq!(v, [4, 2, 3, 4, 50]);
    v.swap(0, 4);
    assert_eq!(v, [50, 2, 3, 4, 4]);
    for chunk in v.chunks_mut(2) {
        chunk[0] = 0;
    }
    assert_eq!(v, [0, 2, 0, 4, 0]);
    assert_eq!(
        std::mem::size_of::<&[u8]>(),
        2 * std::mem::size_of::<usize>()
    );
}

// ---------------------------------------------------------------------------
// Q10. shadowing vs mutation
// ---------------------------------------------------------------------------
/// ANSWER: `let x = ...; let x = ...;` creates a NEW binding (possibly of a different type); the old
/// one still exists until scope end (and its Drop still runs then). `let mut x; x = ...` changes the
/// value in place and keeps the type. Shadowing is for transformations (`let s = s.trim();`), `mut`
/// for state that evolves.
///
/// REALLY TESTING: binding vs value, and that shadowing does not drop the old value early.
/// WRONG: "shadowing frees the old value" (it does not; it just becomes unnameable).
/// FOLLOW-UP: is shadowing inside an inner block undone at its end? Yes.
fn q10_shadowing() {
    let x = "42";
    let x: i32 = x.parse().unwrap(); // different type: only possible by shadowing
    assert_eq!(x, 42);
    {
        let x = x * 2;
        assert_eq!(x, 84);
    }
    assert_eq!(x, 42); // the inner shadow ended with its block

    let mut y = 1;
    y += 1; // same binding mutated
    assert_eq!(y, 2);

    // Shadowing does NOT drop early; proven in Q11's log style:
    let log: Log = Rc::new(RefCell::new(Vec::new()));
    {
        let _t = Tracer::new("first", &log);
        let t = Tracer::new("second", &log); // old `t` still alive, unnameable
        log.borrow_mut().push("body".to_string());
        let _ = &t;
    }
    assert_eq!(*log.borrow(), ["body", "drop second", "drop first"]);
}

// ---------------------------------------------------------------------------
// Q11. drop order
// ---------------------------------------------------------------------------
/// ANSWER: Local variables drop in REVERSE order of declaration. Struct (and tuple, array) fields
/// drop in DECLARATION order, after the struct's own `Drop::drop` ran. Temporaries drop at the end
/// of the enclosing statement (so `foo().bar();` frees foo's result right after the semicolon).
/// Function arguments drop after locals, in reverse order of declaration. `let _ = expr;` drops at
/// once, `let _x = expr;` keeps it to scope end. `mem::drop(x)` just moves x into a function.
///
/// REALLY TESTING: RAII reasoning (lock guards, file handles, transaction rollback).
/// WRONG: "fields drop in reverse like locals", "`let _ = guard` keeps the lock".
/// FOLLOW-UP: what is the drop order when a panic unwinds? The same, via landing pads.
type Log = Rc<RefCell<Vec<String>>>;
struct Tracer {
    name: &'static str,
    log: Log,
}
impl Tracer {
    fn new(name: &'static str, log: &Log) -> Self {
        Tracer {
            name,
            log: log.clone(),
        }
    }
    fn len(&self) -> usize {
        self.name.len()
    }
}
impl Drop for Tracer {
    fn drop(&mut self) {
        self.log.borrow_mut().push(format!("drop {}", self.name));
    }
}
struct Pair {
    a: Tracer,
    b: Tracer,
}
impl Drop for Pair {
    fn drop(&mut self) {
        self.a.log.borrow_mut().push("drop Pair".into());
    }
}
fn q11_drop_order() {
    let log: Log = Rc::new(RefCell::new(Vec::new()));
    {
        let _l1 = Tracer::new("l1", &log);
        let _l2 = Tracer::new("l2", &log);
        let _pair = Pair {
            a: Tracer::new("fa", &log),
            b: Tracer::new("fb", &log),
        };
        let n = Tracer::new("temp", &log).len(); // temporary dropped at the semicolon
        log.borrow_mut().push(format!("after temp {n}"));
        let _ = Tracer::new("underscore", &log); // dropped immediately
        log.borrow_mut().push("end of scope".into());
    }
    let expected = [
        "drop temp",
        "after temp 4",
        "drop underscore",
        "end of scope",
        "drop Pair", // the struct's own Drop first ...
        "drop fa",   // ... then fields in declaration order
        "drop fb",
        "drop l2", // locals in reverse
        "drop l1",
    ];
    assert_eq!(*log.borrow(), expected);
}

// ---------------------------------------------------------------------------
// Q12. partial moves
// ---------------------------------------------------------------------------
/// ANSWER: You may move one field out of a struct (that has no `Drop` impl); the struct as a whole
/// is then unusable, but the OTHER fields can still be used or moved individually, and the remaining
/// fields are dropped at scope end. Moving out of a field of a type that implements `Drop` is an
/// error (E0509), as is moving out from behind a reference (E0507).
///
/// REALLY TESTING: do you understand that the borrow checker tracks paths, not just variables.
/// WRONG: "after moving a field the whole struct is gone".
/// FOLLOW-UP: how to take a field out of `&mut self`? `mem::take`/`replace`/`Option::take` (Q13).
struct Person {
    name: String,
    email: String,
    age: u32,
}
fn q12_partial_moves() {
    let p = Person {
        name: "Ada".into(),
        email: "ada@x.io".into(),
        age: 36,
    };
    let name = p.name; // partial move
    assert_eq!(name, "Ada");
    assert_eq!(p.email, "ada@x.io"); // other fields remain usable
    assert_eq!(p.age, 36);
    // let whole = p;   // error[E0382]: use of partially moved value: `p`
    // let r = &p;      // error[E0382]: borrow of partially moved value
    let email = p.email; // moving the rest is fine
    assert_eq!(email.len(), 8);

    // Moving out of a reference: E0507.
    //   fn take(p: &Person) -> String { p.name }    // error[E0507]: cannot move out of `p.name` which is behind a shared reference
    // Fixes: clone, or return a borrow.
    fn name_ref(p: &Person) -> &str {
        &p.name
    }
    let q = Person {
        name: "Bo".into(),
        email: String::new(),
        age: 1,
    };
    assert_eq!(name_ref(&q), "Bo");

    // Destructuring moves selectively too, and `ref` borrows instead of moving.
    let Person { ref name, age, .. } = q;
    assert_eq!((name.as_str(), age), ("Bo", 1));
    let _still_whole = &q;
}

// ---------------------------------------------------------------------------
// Q13. mem::take / replace / swap
// ---------------------------------------------------------------------------
/// ANSWER: Behind `&mut T` you cannot move out (that would leave a hole), but you may swap
/// something in. `mem::replace(&mut x, new)` returns the old value; `mem::take(&mut x)` replaces
/// with `Default::default()`; `mem::swap(&mut a, &mut b)` exchanges. All are safe, O(size_of::<T>())
/// and never run Drop on the moved values.
///
/// REALLY TESTING: the fix for E0507 "cannot move out of borrowed content" without clone.
/// WRONG: "just clone" (allocates) or "use unsafe ptr::read" (UB risk).
/// FOLLOW-UP: `Option::take` is `mem::replace(self, None)`. State machines use this pattern.
#[derive(Debug, Default, PartialEq)]
enum State {
    #[default]
    Idle,
    Running(String),
    Done(String),
}
struct Machine {
    state: State,
    buf: Vec<u8>,
}
impl Machine {
    fn advance(&mut self) {
        // E0507 if written as `match self.state { State::Running(s) => State::Done(s), .. }`
        self.state = match mem::take(&mut self.state) {
            State::Idle => State::Running("job".into()),
            State::Running(s) => State::Done(s),
            State::Done(s) => State::Done(s),
        };
    }
    fn flush(&mut self) -> Vec<u8> {
        mem::take(&mut self.buf)
    }
}
fn q13_mem_helpers() {
    let mut m = Machine {
        state: State::Idle,
        buf: vec![1, 2, 3],
    };
    m.advance();
    assert_eq!(m.state, State::Running("job".into()));
    m.advance();
    assert_eq!(m.state, State::Done("job".into()));
    let out = m.flush();
    assert_eq!((out.len(), m.buf.capacity()), (3, 0)); // buf is a fresh empty Vec

    let mut a = String::from("a");
    let old = mem::replace(&mut a, "b".into());
    assert_eq!((old.as_str(), a.as_str()), ("a", "b"));
    let mut b = String::from("z");
    mem::swap(&mut a, &mut b);
    assert_eq!((a.as_str(), b.as_str()), ("z", "b"));
}

// ---------------------------------------------------------------------------
// Q14. mutate while iterating
// ---------------------------------------------------------------------------
/// ANSWER: `for x in &v { v.push(..) }` is rejected (E0502): the iterator holds a shared borrow of
/// `v` for the whole loop. That is "iterator invalidation" caught at compile time. Options:
/// `retain`/`retain_mut` for filtering in place, `iter_mut` for element mutation, `drain(..)` or
/// `into_iter` + collect for rebuilding, index loops (`for i in 0..v.len()`) when growth is needed
/// and the semantics are clear, or collect the changes first and apply afterwards.
///
/// REALLY TESTING: whether you know why C++ needs the "do not push during range-for" folklore and
/// Rust does not.
/// WRONG: "wrap it in RefCell/unsafe" (the iterator would still be invalidated at runtime).
/// FOLLOW-UP: what does `HashMap::retain` give you? Removal without a second lookup.
fn q14_mutate_while_iterating() {
    let mut v = vec![1, 2, 3, 4, 5, 6];
    // FAILING: for x in &v { if x % 2 == 0 { v.push(*x); } }   // error[E0502]
    v.retain(|x| x % 2 == 0);
    assert_eq!(v, [2, 4, 6]);
    for x in v.iter_mut() {
        *x *= 10;
    }
    assert_eq!(v, [20, 40, 60]);
    // Growth while looping: snapshot the length, index, push after reading.
    let n = v.len();
    for i in 0..n {
        let doubled = v[i] * 2;
        v.push(doubled);
    }
    assert_eq!(v, [20, 40, 60, 40, 80, 120]);
    // Collect the changes first, apply afterwards.
    let additions: Vec<i32> = v.iter().filter(|x| **x > 100).map(|x| x + 1).collect();
    v.extend(additions);
    assert_eq!(v.last(), Some(&121));

    let mut hm: HashMap<&str, i32> = HashMap::from([("a", 1), ("b", 2), ("c", 3)]);
    hm.retain(|_, val| *val != 2);
    let mut keys: Vec<_> = hm.keys().copied().collect();
    keys.sort();
    assert_eq!(keys, ["a", "c"]);
}

// ---------------------------------------------------------------------------
// Q15. Option<&mut T>, as_mut, take
// ---------------------------------------------------------------------------
/// ANSWER: `Option<T>` OWNS its payload; `opt.as_ref()` gives `Option<&T>` and `opt.as_mut()` gives
/// `Option<&mut T>` without moving, so you can inspect or mutate through `&mut self`. `opt.take()`
/// moves the value out leaving `None`. `&mut Option<Box<Node>>` is the idiom for linked-list
/// cursors. `Option<&T>` and `Option<&mut T>` are pointer-sized thanks to the null niche.
///
/// REALLY TESTING: linked list manipulation, E0507 avoidance.
/// WRONG: "match on `self.head` directly" (tries to move out of a borrow).
/// FOLLOW-UP: `Option<Box<T>>` size == `Box<T>` size (niche optimisation).
struct Node {
    val: i32,
    next: Option<Box<Node>>,
}
struct Stack {
    head: Option<Box<Node>>,
}
impl Stack {
    fn push(&mut self, val: i32) {
        let old = self.head.take(); // move the old head out, leaving None
        self.head = Some(Box::new(Node { val, next: old }));
    }
    fn pop(&mut self) -> Option<i32> {
        self.head.take().map(|n| {
            self.head = n.next;
            n.val
        })
    }
    fn peek(&self) -> Option<&i32> {
        self.head.as_ref().map(|n| &n.val)
    }
    fn peek_mut(&mut self) -> Option<&mut i32> {
        self.head.as_mut().map(|n| &mut n.val)
    }
    fn reverse(&mut self) {
        let mut prev: Option<Box<Node>> = None;
        let mut cur = self.head.take();
        while let Some(mut n) = cur {
            cur = n.next.take();
            n.next = prev;
            prev = Some(n);
        }
        self.head = prev;
    }
}
fn q15_option_mut() {
    let mut s = Stack { head: None };
    for i in 1..=3 {
        s.push(i);
    }
    assert_eq!(s.peek(), Some(&3));
    if let Some(top) = s.peek_mut() {
        *top = 30;
    }
    s.reverse();
    assert_eq!(s.pop(), Some(1));
    assert_eq!(s.pop(), Some(2));
    assert_eq!(s.pop(), Some(30));
    assert_eq!(s.pop(), None);
    assert_eq!(mem::size_of::<Option<Box<Node>>>(), mem::size_of::<usize>());
    assert_eq!(mem::size_of::<Option<&mut i32>>(), mem::size_of::<usize>());
}

// ---------------------------------------------------------------------------
// Q16. two-phase borrows
// ---------------------------------------------------------------------------
/// ANSWER: `v.push(v.len())` desugars to `Vec::push(&mut v, v.len())`. A naive reading takes `&mut v`
/// first and then needs `&v` for `len()`: a conflict. Two-phase borrows split the `&mut` autoref into
/// a *reservation* (acts like a shared borrow, so reads are allowed) and an *activation* at the call.
/// Between them only reads are allowed. It applies to autoref method receivers and compound
/// assignment, not to an explicit `let m = &mut v;` (that activates immediately).
///
/// REALLY TESTING: depth of borrow-checker knowledge; most candidates cannot explain it.
/// WRONG: "it compiles because len() is evaluated first" (order is the same; the borrow model changed).
/// FOLLOW-UP: show a variant that fails.
fn q16_two_phase() {
    let mut v = vec![0usize];
    v.push(v.len()); // OK: two-phase borrow
    v.push(v.len());
    assert_eq!(v, [0, 1, 2]);

    // FAILING (E0502): explicit &mut activates immediately.
    //   let r = &mut v;
    //   r.push(v.len());   // error[E0502]: cannot borrow `v` as immutable because it is also borrowed as mutable
    // Equivalent via a temporary: fine.
    let n = v.len();
    let r = &mut v;
    r.push(n);
    assert_eq!(v.len(), 4);

    let mut total = 10;
    total += total; // compound assignment also uses the two-phase idea for overloaded ops on primitives
    assert_eq!(total, 20);
}

// ---------------------------------------------------------------------------
// Q17. clone is not always the fix
// ---------------------------------------------------------------------------
/// ANSWER: Cloning silences the checker by giving you an independent copy, but it costs allocation
/// and copying, and it can change semantics: mutations to the clone are NOT seen by the original
/// (a classic bug when you "fix" E0499 and then wonder why updates vanish). Prefer: restructure so
/// borrows do not overlap (NLL), borrow `&`/`&mut` properly, pass `&str`, use `mem::take`, split
/// the struct into disjoint fields, index/ids instead of references, or `Rc`/`Arc` when sharing is
/// the real requirement (clone of an Rc is a counter bump, not a deep copy).
///
/// REALLY TESTING: judgement. "Does the candidate fight the checker or listen to it?"
/// WRONG: "clone everywhere until it compiles".
/// FOLLOW-UP: when is clone right? Small Copy-like data, snapshots, crossing thread/'static boundaries.
struct Inventory {
    items: Vec<String>,
    log: Vec<String>,
}
impl Inventory {
    // FAILING (E0502): for it in &self.items { self.record(it); }  // self borrowed twice via the method
    // A method taking &mut self borrows ALL of self. Touching the fields directly is a disjoint borrow.
    fn log_all(&mut self) {
        for it in &self.items {
            self.log.push(format!("saw {it}")); // fields are disjoint: compiles, no clone
        }
    }
}
fn q17_clone_not_always() {
    // Semantic trap: the clone diverges.
    let original = vec![1, 2, 3];
    let mut copy = original.clone();
    copy.push(4);
    assert_ne!(original, copy);

    let mut inv = Inventory {
        items: vec!["a".into(), "b".into()],
        log: vec![],
    };
    inv.log_all();
    assert_eq!(inv.log, ["saw a", "saw b"]);

    // Cheap sharing when sharing IS the requirement: Rc::clone bumps a counter, copies nothing.
    let shared = Rc::new(vec![0u8; 1024]);
    let also = Rc::clone(&shared);
    assert_eq!(Rc::strong_count(&shared), 2);
    assert!(std::ptr::eq(shared.as_ptr(), also.as_ptr()));
}

fn main() {
    q1_move_copy_clone();
    println!("Q1: ok");
    q2_let_a_equals_b();
    println!("Q2: ok");
    q3_borrow_rules();
    println!("Q3: ok");
    q4_nll();
    println!("Q4: ok");
    q5_dangling();
    println!("Q5: ok");
    q6_elision();
    println!("Q6: ok");
    q7_static();
    println!("Q7: ok");
    q8_string_vs_str();
    println!("Q8: ok");
    q9_slices();
    println!("Q9: ok");
    q10_shadowing();
    println!("Q10: ok");
    q11_drop_order();
    println!("Q11: ok");
    q12_partial_moves();
    println!("Q12: ok");
    q13_mem_helpers();
    println!("Q13: ok");
    q14_mutate_while_iterating();
    println!("Q14: ok");
    q15_option_mut();
    println!("Q15: ok");
    q16_two_phase();
    println!("Q16: ok");
    q17_clone_not_always();
    println!("Q17: ok");
}
