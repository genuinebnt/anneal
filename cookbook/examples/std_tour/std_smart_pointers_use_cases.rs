//! RECIPE 1004 (std): Box, Rc/Weak, Arc, Cell/RefCell/OnceCell/LazyCell/OnceLock/LazyLock: one real use case each.
//! LEVEL: intermediate
//! Use in: recursive types, shared ownership, trees with parent pointers, caches, lazy statics, interior mutability.   Run: cargo run --release --example std_smart_pointers_use_cases
//! COMMON USES: AST nodes (`Box<Expr>`), DOM/UI trees with parent links (Rc/Weak), config snapshots swapped atomically (Arc),
//! memoisation (RefCell/OnceCell), process-wide regex/config (LazyLock/OnceLock), trait objects (`Box<dyn Trait>`).
//! METHODS: (everyday first) Box::new, Rc::new, Rc::clone, Rc::strong_count, Arc::new, Arc::clone, RefCell::borrow, RefCell::borrow_mut, Cell::get, Cell::set,
//! METHODS: Rc::downgrade, Weak::upgrade, Rc::weak_count, Rc::ptr_eq, Rc::try_unwrap, Rc::get_mut, Rc::make_mut, Arc::make_mut, Arc::strong_count, Arc::try_unwrap,
//! METHODS: Box::leak, Box::into_raw, Box::from_raw, Box::into_boxed_slice, Vec::into_boxed_slice, Cell::replace, Cell::take, RefCell::try_borrow, RefCell::try_borrow_mut,
//! METHODS: RefCell::replace, RefCell::take, RefCell::into_inner, OnceCell::get_or_init, OnceCell::set, OnceCell::get, LazyCell::new, LazyCell::force, OnceLock::get_or_init,
//! METHODS: OnceLock::set, LazyLock::new, LazyLock::force, Mutex::lock, Mutex::into_inner, Arc::from, Rc::from, Pin::new, Box::pin, Weak::new, Weak::strong_count
//! DOCS: https://doc.rust-lang.org/std/boxed/struct.Box.html , https://doc.rust-lang.org/std/rc/struct.Rc.html , https://doc.rust-lang.org/std/sync/struct.Arc.html , https://doc.rust-lang.org/std/cell/index.html
//! DEFINED IN: Box/Rc/Weak/Arc alloc; Cell/RefCell/OnceCell/LazyCell/Pin core; OnceLock/LazyLock/Mutex std (`std::sync`).
//! NOT COVERED (and why): Rc::new_cyclic / Arc::new_cyclic beyond mention, UniqueRc and allocator-aware APIs (nightly), RefCell::map/Ref::map projections, Rc::into_raw/from_raw (unsafe, same shape as Box).
#![allow(dead_code)]
//!
//! CHEAT SHEET (I want to ... -> use ...)
//!   - recursive type / big value on the heap     -> Box<T>              - heterogeneous list -> Vec<Box<dyn Trait>>
//!   - shared ownership, single thread            -> Rc<T>               - ... across threads -> Arc<T>
//!   - parent pointer without a leak              -> Weak<T> (downgrade/upgrade)
//!   - mutate through &self, Copy types           -> Cell<T>             - ... non-Copy, runtime-checked -> RefCell<T>
//!   - shared AND mutable across threads          -> Arc<Mutex<T>> / Arc<RwLock<T>>
//!   - copy-on-write shared value                 -> Rc::make_mut / Arc::make_mut
//!   - compute once, then read-only               -> OnceCell / OnceLock (set once) or LazyCell / LazyLock (init closure)
//!   - global lazily built table                  -> static X: LazyLock<T>
//!   - cheap-to-clone immutable string/slice      -> Arc<str> / Rc<[T]>
//! Read: https://doc.rust-lang.org/book/ch15-00-smart-pointers.html , https://doc.rust-lang.org/nomicon/ (ownership chapters) , https://doc.rust-lang.org/std/cell/index.html
//! TRY NEXT:
//!   - [S] In the cycle section, remove the `Weak` and keep a strong parent pointer; use the drop counter to confirm the leak, then restore Weak.
//!   - [S] Predict what `RefCell::borrow_mut` does while a `Ref` is alive in the same scope: first with try_borrow_mut, then (in catch_unwind) with borrow_mut.
//!   - [M] Add `Tree::depth(&self) -> usize` walking up with `parent.upgrade()`; assert on a 1000-deep chain without recursion.
//!   - [M] Implement an observer list `Vec<Weak<RefCell<dyn Observer>>>` that prunes dead entries on notify; test with drops in the middle.
//!   - [A] PROJECT: combine with std_any_error_marker_traits and std_ops_traits_overloading: a typed event bus: `HashMap<TypeId, Vec<Box<dyn Fn(&dyn Any)>>>` plus an `Rc<RefCell<..>>` state, subscriptions removed by RAII guards.
use std::cell::{Cell, LazyCell, OnceCell, RefCell};
use std::collections::HashMap;
use std::pin::Pin;
use std::rc::{Rc, Weak};
use std::sync::{Arc, LazyLock, Mutex, OnceLock};

const COVERED: &[&str] = &[
    "Box::new",
    "Rc::new",
    "Rc::clone",
    "strong_count",
    "Arc::new",
    "RefCell::borrow",
    "borrow_mut",
    "Cell::get",
    "Cell::set",
    "downgrade",
    "upgrade",
    "weak_count",
    "ptr_eq",
    "try_unwrap",
    "get_mut",
    "make_mut",
    "Box::leak",
    "into_raw",
    "from_raw",
    "into_boxed_slice",
    "Cell::replace",
    "Cell::take",
    "try_borrow",
    "try_borrow_mut",
    "RefCell::replace",
    "RefCell::take",
    "into_inner",
    "OnceCell::get_or_init",
    "OnceCell::set",
    "LazyCell",
    "OnceLock",
    "LazyLock",
    "Mutex::lock",
    "Box::pin",
];

// ---- 0. Everyday core: Box --------------------------------------------------------------------------------
#[derive(Debug)]
enum Expr {
    Num(i64),
    Add(Box<Expr>, Box<Expr>), // recursion needs indirection to have a finite size
    Neg(Box<Expr>),
}
fn eval(e: &Expr) -> i64 {
    match e {
        Expr::Num(n) => *n,
        Expr::Add(a, b) => eval(a) + eval(b),
        Expr::Neg(a) => -eval(a),
    }
}
trait Shape {
    fn area(&self) -> f64;
}
struct Sq(f64);
struct Circ(f64);
impl Shape for Sq {
    fn area(&self) -> f64 {
        self.0 * self.0
    }
}
impl Shape for Circ {
    fn area(&self) -> f64 {
        3.0 * self.0 * self.0
    }
}

fn section_box() {
    let e = Expr::Add(
        Box::new(Expr::Num(2)),
        Box::new(Expr::Neg(Box::new(Expr::Num(5)))),
    );
    assert_eq!(eval(&e), -3);
    // Trait objects: different concrete types in one Vec.
    let shapes: Vec<Box<dyn Shape>> = vec![Box::new(Sq(2.0)), Box::new(Circ(1.0))];
    assert_eq!(shapes.iter().map(|s| s.area()).sum::<f64>(), 7.0);
    // Large values: moving a Box moves 8 bytes, not the payload.
    let big = Box::new([0u8; 1 << 20]);
    let moved = big; // pointer copy only
    assert_eq!(moved.len(), 1 << 20);
    // Box<[T]> / Box<str>: fixed-size owned slice, drops the Vec's capacity word.
    let bs: Box<[i32]> = vec![1, 2, 3].into_boxed_slice();
    assert_eq!(
        (bs.len(), size_of::<Box<[i32]>>()),
        (3, 2 * size_of::<usize>())
    );
    // Box::leak: &'static mut for process-lifetime data (config loaded once). Never free.
    let cfg: &'static mut String = Box::leak(Box::new(String::from("cfg")));
    cfg.push('!');
    assert_eq!(cfg, "cfg!");
    // into_raw / from_raw: hand ownership to C and take it back. Pair them exactly once.
    let raw = Box::into_raw(Box::new(41));
    // SAFETY: raw came from Box::into_raw, is unique and not used after from_raw.
    let back = unsafe { Box::from_raw(raw) };
    assert_eq!(*back + 1, 42);
    // Recursive Drop pitfall: a 1e6-long Box list overflows the stack on drop; see std_option_methods section 9.
    println!("0. Box ok");
}

// ---- 1. Rc / Weak: tree with parent pointers, cycles ----------------------------------------------------------
struct TreeNode {
    name: String,
    parent: RefCell<Weak<TreeNode>>,      // weak up: no cycle
    children: RefCell<Vec<Rc<TreeNode>>>, // strong down: parent owns children
    drops: Rc<Cell<u32>>,
}
impl Drop for TreeNode {
    fn drop(&mut self) {
        self.drops.set(self.drops.get() + 1);
    }
}
fn node(name: &str, drops: &Rc<Cell<u32>>) -> Rc<TreeNode> {
    Rc::new(TreeNode {
        name: name.into(),
        parent: RefCell::new(Weak::new()),
        children: RefCell::new(vec![]),
        drops: drops.clone(),
    })
}
fn add_child(p: &Rc<TreeNode>, c: Rc<TreeNode>) {
    *c.parent.borrow_mut() = Rc::downgrade(p);
    p.children.borrow_mut().push(c);
}
fn path(n: &Rc<TreeNode>) -> String {
    match n.parent.borrow().upgrade() {
        Some(p) => format!("{}/{}", path(&p), n.name),
        None => n.name.clone(),
    }
}
fn section_rc_weak() {
    let drops = Rc::new(Cell::new(0));
    {
        let root = node("root", &drops);
        let leaf = node("leaf", &drops);
        add_child(&root, leaf.clone());
        assert_eq!(path(&leaf), "root/leaf");
        assert_eq!((Rc::strong_count(&root), Rc::weak_count(&root)), (1, 1));
        assert_eq!(Rc::strong_count(&leaf), 2);
        assert!(Rc::ptr_eq(&leaf, &root.children.borrow()[0]));
        drop(root); // leaf survives (we hold a clone) but its parent is gone
        assert!(leaf.parent.borrow().upgrade().is_none());
    }
    assert_eq!(
        drops.get(),
        2,
        "both nodes freed: no leak with Weak parents"
    );
    // Cycle leak demo: A -> B -> A with strong pointers never drops.
    struct Leaky {
        next: RefCell<Option<Rc<Leaky>>>,
        drops: Rc<Cell<u32>>,
    }
    impl Drop for Leaky {
        fn drop(&mut self) {
            self.drops.set(self.drops.get() + 1);
        }
    }
    let d = Rc::new(Cell::new(0));
    {
        let a = Rc::new(Leaky {
            next: RefCell::new(None),
            drops: d.clone(),
        });
        let b = Rc::new(Leaky {
            next: RefCell::new(Some(a.clone())),
            drops: d.clone(),
        });
        *a.next.borrow_mut() = Some(b.clone());
    }
    assert_eq!(d.get(), 0, "leaked: strong cycle keeps both alive");
    // Rc::get_mut / make_mut / try_unwrap.
    let mut x = Rc::new(vec![1]);
    Rc::get_mut(&mut x).unwrap().push(2); // unique => &mut allowed
    let y = x.clone();
    assert!(Rc::get_mut(&mut x).is_none()); // shared => None
    Rc::make_mut(&mut x).push(3); // clones the Vec (copy-on-write), x now unique
    assert_eq!((&*x, &*y), (&vec![1, 2, 3], &vec![1, 2]));
    assert_eq!(Rc::try_unwrap(y), Ok(vec![1, 2]));
    let z = Rc::new(5);
    let z2 = z.clone();
    assert!(Rc::try_unwrap(z).is_err()); // still shared, returns the Rc back
    drop(z2);
    println!("1. Rc/Weak ok");
}

// ---- 2. Observer lists with Weak -----------------------------------------------------------------------------
trait Observer {
    fn notify(&mut self, ev: &str);
}
struct Recorder(Vec<String>);
impl Observer for Recorder {
    fn notify(&mut self, ev: &str) {
        self.0.push(ev.to_string());
    }
}
struct Subject {
    observers: Vec<Weak<RefCell<dyn Observer>>>,
}
impl Subject {
    fn emit(&mut self, ev: &str) -> usize {
        // retain + upgrade: prune dead observers while notifying.
        self.observers.retain(|w| match w.upgrade() {
            Some(o) => {
                o.borrow_mut().notify(ev);
                true
            }
            None => false,
        });
        self.observers.len()
    }
}
fn section_observers() {
    let a = Rc::new(RefCell::new(Recorder(vec![])));
    let b = Rc::new(RefCell::new(Recorder(vec![])));
    let mut s = Subject { observers: vec![] };
    let wa: Weak<RefCell<dyn Observer>> = Rc::downgrade(&(a.clone() as Rc<RefCell<dyn Observer>>));
    s.observers.push(wa);
    s.observers
        .push(Rc::downgrade(&(b.clone() as Rc<RefCell<dyn Observer>>)));
    assert_eq!(s.emit("one"), 2);
    drop(b);
    assert_eq!(s.emit("two"), 1, "dead observer pruned");
    assert_eq!(a.borrow().0, vec!["one", "two"]);
    println!("2. observers ok");
}

// ---- 3. Arc: shared config snapshots, make_mut, Arc<str> -----------------------------------------------------
#[derive(Clone, Debug, PartialEq)]
struct Config {
    retries: u32,
    name: String,
}
fn section_arc() {
    // Pattern: readers clone the Arc (cheap), writers publish a new Arc. Readers keep a consistent snapshot.
    let current = Mutex::new(Arc::new(Config {
        retries: 1,
        name: "a".into(),
    }));
    let snapshot = current.lock().unwrap().clone(); // lock held only for the clone
    {
        let mut guard = current.lock().unwrap();
        let mut next = (**guard).clone();
        next.retries = 5;
        *guard = Arc::new(next);
    }
    assert_eq!(snapshot.retries, 1, "old reader unaffected");
    assert_eq!(current.lock().unwrap().retries, 5);
    // Arc::make_mut: copy-on-write only if shared.
    let mut a = Arc::new(Config {
        retries: 0,
        name: "x".into(),
    });
    let b = a.clone();
    Arc::make_mut(&mut a).retries = 9;
    assert_eq!((a.retries, b.retries, Arc::strong_count(&b)), (9, 0, 1));
    // Arc<str> / Arc<[T]>: one allocation, cheap clone, no capacity word. Great for interned names.
    let name: Arc<str> = Arc::from("shared-name");
    let n2 = Arc::clone(&name);
    assert_eq!((Arc::strong_count(&name), &*n2), (2, "shared-name"));
    let items: Arc<[u32]> = Arc::from(vec![1, 2, 3]);
    let t = {
        let items = items.clone();
        std::thread::spawn(move || items.iter().sum::<u32>())
    };
    assert_eq!(t.join().unwrap(), 6);
    // Arc::try_unwrap after all threads finished: get the inner value back.
    let solo = Arc::new(String::from("solo"));
    assert_eq!(Arc::try_unwrap(solo).unwrap(), "solo");
    println!("3. Arc ok");
}

// ---- 4. Cell / RefCell / OnceCell / LazyCell ------------------------------------------------------------------
struct Counter {
    hits: Cell<u32>,
    memo: RefCell<HashMap<u64, u64>>,
    calls: Cell<u32>,
}
impl Counter {
    // &self but mutates: interior mutability. Cell for Copy values, RefCell when you need references into it.
    fn fib(&self, n: u64) -> u64 {
        self.calls.set(self.calls.get() + 1);
        if n < 2 {
            return n;
        }
        if let Some(&v) = self.memo.borrow().get(&n) {
            self.hits.set(self.hits.get() + 1);
            return v;
        }
        // IMPORTANT: the borrow() above is dropped at the end of the `if let` statement, so recursion may borrow_mut.
        let v = self.fib(n - 1) + self.fib(n - 2);
        self.memo.borrow_mut().insert(n, v);
        v
    }
}
fn section_cells() {
    let c = Counter {
        hits: Cell::new(0),
        memo: RefCell::new(HashMap::new()),
        calls: Cell::new(0),
    };
    assert_eq!(c.fib(50), 12_586_269_025);
    assert!(c.calls.get() < 200, "memoised: linear, not exponential");
    // Cell: get/set/replace/take. take needs Default.
    let cell = Cell::new(5);
    assert_eq!(cell.replace(6), 5);
    assert_eq!(cell.take(), 6);
    assert_eq!(cell.get(), 0);
    // RefCell: runtime borrow rules. try_borrow* returns Err instead of panicking.
    let rc = RefCell::new(vec![1]);
    {
        let r1 = rc.borrow();
        assert!(rc.try_borrow().is_ok(), "many shared borrows are fine");
        assert!(rc.try_borrow_mut().is_err(), "mutable while shared: error");
        drop(r1);
    }
    let m = rc.borrow_mut();
    assert!(rc.try_borrow().is_err());
    drop(m);
    let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _a = rc.borrow_mut();
        let _b = rc.borrow_mut(); // BorrowMutError panic
    }));
    assert!(r.is_err());
    assert_eq!(rc.replace(vec![9]), vec![1]);
    assert_eq!(rc.take(), vec![9]);
    assert_eq!(rc.into_inner(), Vec::<i32>::new());
    // OnceCell: set once, then shared reads. get_or_init is the memo pattern for ONE value.
    let once: OnceCell<String> = OnceCell::new();
    assert!(once.get().is_none());
    assert_eq!(once.get_or_init(|| "first".into()), "first");
    assert_eq!(once.get_or_init(|| "second".into()), "first");
    assert!(once.set("third".into()).is_err());
    // LazyCell: init closure stored with the cell; runs on first deref.
    let ran = Cell::new(0);
    let lazy = LazyCell::new(|| {
        ran.set(ran.get() + 1);
        40 + 2
    });
    assert_eq!(ran.get(), 0);
    assert_eq!(*LazyCell::force(&lazy), 42); // explicit initialisation; deref does the same implicitly
    assert_eq!((*lazy, *lazy, ran.get()), (42, 42, 1));
    println!("4. cells ok");
}

// ---- 5. OnceLock / LazyLock / Mutex vs RefCell ----------------------------------------------------------------
static TABLE: LazyLock<HashMap<&'static str, u32>> = LazyLock::new(|| {
    // Runs once, thread-safe. Replaces lazy_static!/once_cell::Lazy for new code (std since 1.80).
    HashMap::from([("a", 1), ("b", 2)])
});
static SETTINGS: OnceLock<String> = OnceLock::new();
fn init_settings(s: &str) -> bool {
    SETTINGS.set(s.to_string()).is_ok() // first caller wins; Err(value) back if already set
}
fn section_statics_and_threads() {
    assert_eq!(TABLE["b"], 2);
    assert!(init_settings("prod"));
    assert!(!init_settings("dev"));
    assert_eq!(SETTINGS.get().map(String::as_str), Some("prod"));
    assert_eq!(SETTINGS.get_or_init(|| "ignored".into()), "prod");
    // Mutex<T> vs RefCell<T>: both give runtime-checked exclusive access; Mutex is Sync (threads), blocks instead of panicking.
    let counter = Arc::new(Mutex::new(0u32));
    let hs: Vec<_> = (0..4)
        .map(|_| {
            let c = counter.clone();
            std::thread::spawn(move || {
                for _ in 0..1000 {
                    *c.lock().unwrap() += 1;
                }
            })
        })
        .collect();
    hs.into_iter().for_each(|h| h.join().unwrap());
    assert_eq!(*counter.lock().unwrap(), 4000);
    assert_eq!(
        Arc::try_unwrap(counter).unwrap().into_inner().unwrap(),
        4000
    );
    // Rc<RefCell<_>> is !Send: `thread::spawn(move || rc)` does not compile (see std_any_error_marker_traits).
    println!("5. statics/Mutex ok");
}

// ---- 6. Pin<Box<T>> in short -----------------------------------------------------------------------------------
// A pinned value cannot move, so self-referential structs (async state machines) can hold pointers into themselves.
struct SelfRef {
    data: String,
    ptr: *const String, // points to `data`; valid only if the struct never moves
    _pin: std::marker::PhantomPinned,
}
fn section_pin() {
    let mut b = Box::pin(SelfRef {
        data: "pinned".into(),
        ptr: std::ptr::null(),
        _pin: std::marker::PhantomPinned,
    });
    let p: *const String = &b.data;
    // SAFETY: we only set a field; we do not move the pinned struct.
    unsafe { b.as_mut().get_unchecked_mut().ptr = p };
    // SAFETY: b is pinned in the heap, so `data` has not moved since we took the address.
    let read = unsafe { &*b.ptr };
    assert_eq!(read, "pinned");
    let pinned_i32: Pin<Box<i32>> = Box::pin(5); // i32: Unpin, so Pin adds no restriction
    assert_eq!(*pinned_i32 + 1, 6);
    println!("6. Pin ok");
}

// ---- 7. Production tips and tricks ---------------------------------------------------------------------------
fn section_production_tips() {
    // 1. Rc::clone(&x) rather than x.clone() makes refcount bumps explicit in review (clippy::clone_on_ref_ptr is opt-in).
    let r = Rc::new(1);
    let r2 = Rc::clone(&r);
    assert_eq!(Rc::strong_count(&r2), 2);
    // 2. Arc<Mutex<T>> is often replaced by message passing or by Arc<T> snapshots; lock for the shortest scope.
    // 3. Never hold a MutexGuard across .await or I/O (clippy::await_holding_lock); copy out what you need, drop, then work.
    let m = Mutex::new(vec![1, 2, 3]);
    let first = { m.lock().unwrap()[0] }; // guard dropped at end of the block
    assert_eq!(first, 1);
    // 4. A poisoned Mutex returns Err; services usually `lock().unwrap_or_else(|e| e.into_inner())` to keep going.
    let m2 = Arc::new(Mutex::new(0));
    let m3 = m2.clone();
    let _ = std::thread::spawn(move || {
        let _g = m3.lock().unwrap();
        panic!("poison")
    })
    .join();
    assert!(m2.is_poisoned());
    let v = *m2.lock().unwrap_or_else(|e| e.into_inner());
    assert_eq!(v, 0);
    // 5. Prefer `Rc<RefCell<T>>` only when you truly have shared mutation; usually arena + indices is simpler and faster.
    // 6. RefCell: never keep a `Ref` across a call that might borrow_mut the same cell; copy values out early.
    // 7. Box<dyn Trait> needs `+ Send` to cross threads; spell `Box<dyn Fn() + Send + 'static>` in job queues.
    let jobs: Vec<Box<dyn FnOnce() -> u32 + Send + 'static>> = vec![Box::new(|| 1), Box::new(|| 2)];
    assert_eq!(
        jobs.into_iter()
            .map(|j| std::thread::spawn(j).join().unwrap())
            .sum::<u32>(),
        3
    );
    // 8. Replace lazy_static/once_cell with LazyLock/OnceLock; use LazyLock for regexes/tables built once.
    // 9. Arc<str> beats String in structs cloned often (cache keys, log fields): clone = refcount bump.
    let key: Arc<str> = "tenant-1".into();
    let copies: Vec<Arc<str>> = (0..3).map(|_| key.clone()).collect();
    assert_eq!(Arc::strong_count(&key), 4);
    drop(copies);
    // 10. Weak for caches: Weak<T> lets entries die when nobody else holds them.
    let strong = Arc::new(5);
    let weak = Arc::downgrade(&strong);
    assert_eq!(weak.upgrade().map(|a| *a), Some(5));
    drop(strong);
    assert!(weak.upgrade().is_none());
    // 11. `Box<T>` field in a hot enum shrinks the enum (clippy::large_enum_variant): box the rare big variant.
    enum Msg {
        Small(u8),
        Big(Box<[u8; 1024]>),
    }
    assert!(size_of::<Msg>() <= 16);
    println!("7. production tips ok");
}

fn main() {
    let mut seen = std::collections::HashSet::new();
    assert!(
        COVERED.iter().all(|m| seen.insert(*m)),
        "COVERED has duplicates"
    );
    section_box();
    section_rc_weak();
    section_observers();
    section_arc();
    section_cells();
    section_statics_and_threads();
    section_pin();
    section_production_tips();
    println!("all {} smart-pointer APIs exercised; done", COVERED.len());
}
