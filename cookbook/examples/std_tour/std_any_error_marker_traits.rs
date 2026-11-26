//! RECIPE 1005 (std): std::any (Any, TypeId, downcast), std::error::Error (source chains, boxing), marker and auto traits.
//! LEVEL: intermediate
//! Use in: error design, type-keyed registries, plugin payloads, thread-safety bounds, layout checks.   Run: cargo run --release --example std_any_error_marker_traits
//! COMMON USES: request "extensions" maps in web frameworks, event buses, logging a whole error chain, finding an
//! io::Error inside a wrapped error to decide whether to retry, `Send + Sync + 'static` bounds on spawned work.
//! METHODS: (everyday first) std::error::Error, Error::source, std::fmt::Display, Box<dyn Error + Send + Sync>, From::from,
//! METHODS: <dyn Error>::downcast_ref, <dyn Error>::downcast_mut, <dyn Error>::is, Box<dyn Error>::downcast, std::io::Error::new,
//! METHODS: std::io::Error::other, std::io::Error::kind, std::io::Error::get_ref, std::io::Error::into_inner, std::io::Error::downcast,
//! METHODS: std::any::Any, std::any::TypeId::of, Any::type_id, <dyn Any>::is, <dyn Any>::downcast_ref, <dyn Any>::downcast_mut,
//! METHODS: Box<dyn Any>::downcast, Rc<dyn Any>::downcast, Arc<dyn Any + Send + Sync>::downcast, std::any::type_name, std::any::type_name_of_val,
//! METHODS: Send, Sync, Sized, Unpin, Copy, std::marker::PhantomData, std::marker::PhantomPinned, std::mem::size_of
//! DOCS: https://doc.rust-lang.org/std/any/index.html
//! DEFINED IN: core: `core::any`, `core::error::Error` (in core since 1.81), `core::marker`. alloc: `Box`, the `From<E> for Box<dyn Error>`
//! impls and `Box::downcast`; Rc/Arc downcasts. std: `std::io::Error`, `std::thread`.
//! Also: https://doc.rust-lang.org/std/error/trait.Error.html , https://doc.rust-lang.org/std/marker/index.html
//! NOT COVERED (and why): Error::description/cause (deprecated), Error::provide/request_ref and Error::sources (unstable in 1.98.1),
//! downcast_unchecked_* (unsafe), writing `unsafe impl Send/Sync` (see the unsafe chapter), negative impls `impl !Send` (nightly).
#![allow(dead_code)]
//!
//! CHEAT SHEET (I want to ... -> use ...)
//!   make my type an error                     -> derive Debug, impl Display, `impl Error` (override source if wrapping)
//!   let `?` convert a lower error             -> impl From<Lower> for Mine
//!   "any error" in an application             -> Box<dyn Error + Send + Sync + 'static>
//!   an error from a string                    -> `Err("msg".into())` or io::Error::other("msg")
//!   print the whole cause chain               -> loop over source()
//!   was the cause an io::Error?               -> walk the chain with downcast_ref::<io::Error>()
//!   get the concrete error back by value      -> Box<dyn Error>::downcast::<T>() (Err returns the box)
//!   value of unknown type                     -> Box<dyn Any> + downcast
//!   one value per type                        -> HashMap<TypeId, Box<dyn Any + Send + Sync>>
//!   print a type for debugging                -> type_name::<T>()  (never parse or compare it)
//!   prove a type is thread-safe               -> `const _: () = assert_send_sync::<T>();`
//!   zero-cost type tag                        -> PhantomData<T>
//!
//! DISPLAY CONVENTION (from the Error docs): messages are typically lowercase, no trailing punctuation; a wrapping error
//! returns the cause from source() OR includes it in its Display output, NOT both (otherwise chain printers repeat it).
//!
//! Read: https://doc.rust-lang.org/std/error/trait.Error.html , https://doc.rust-lang.org/std/any/index.html ,
//!       https://doc.rust-lang.org/nomicon/send-and-sync.html
//! TRY NEXT:
//!   - [S] Make `ConfigError::Read`'s Display include `{source}` as well and print `chain_string`. What gets duplicated?
//!   - [S] Remove `+ Send + Sync` from `load_boxed`'s return type and try to send its error through `thread::spawn`. Read the error.
//!   - [M] Add `fn is_retryable(&dyn Error) -> bool` covering TimedOut and ConnectionReset at any depth; test 3 levels deep.
//!   - [M] Give `Extensions` a `get_or_insert_with::<T>` method. Which bounds must T have, and which one comes from Any?
//!   - [A] PROJECT: an event bus keyed by TypeId whose handlers return Result with this page's chain printer for failures,
//!         delivered over channels from std_smart_pointers_use_cases-style Arc state.
use std::any::{Any, TypeId, type_name, type_name_of_val};
use std::collections::{HashMap, HashSet};
use std::error::Error;
use std::fmt;
use std::io;
use std::marker::{PhantomData, PhantomPinned};
use std::num::{NonZeroU32, ParseIntError};
use std::rc::Rc;
use std::sync::Arc;

const COVERED: &[&str] = &[
    "Error",
    "source",
    "Display",
    "From",
    "downcast_ref",
    "downcast_mut",
    "is",
    "downcast",
    "new",
    "other",
    "kind",
    "get_ref",
    "into_inner",
    "Any",
    "TypeId",
    "type_id",
    "type_name",
    "type_name_of_val",
    "Send",
    "Sync",
    "Sized",
    "Unpin",
    "Copy",
    "PhantomData",
    "PhantomPinned",
    "size_of",
];

type BoxError = Box<dyn Error + Send + Sync + 'static>;

// ---- 1. Everyday core: a typed library error -------------------------------------------------------------------
#[derive(Debug)]
#[non_exhaustive]
enum ConfigError {
    Read {
        path: String,
        source: io::Error,
    },
    BadPort {
        value: String,
        source: ParseIntError,
    },
    Missing(&'static str),
}
impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Describe THIS layer only. The cause is reached through source().
        match self {
            ConfigError::Read { path, .. } => write!(f, "cannot read config {path}"),
            ConfigError::BadPort { value, .. } => write!(f, "invalid port {value:?}"),
            ConfigError::Missing(k) => write!(f, "missing key {k}"),
        }
    }
}
impl Error for ConfigError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            ConfigError::Read { source, .. } => Some(source),
            ConfigError::BadPort { source, .. } => Some(source),
            ConfigError::Missing(_) => None,
        }
    }
}

/// `read` is injected so the example needs no filesystem.
fn load(path: &str, read: impl Fn(&str) -> io::Result<String>) -> Result<u16, ConfigError> {
    let text = read(path).map_err(|source| ConfigError::Read {
        path: path.into(),
        source,
    })?;
    let value = text
        .trim()
        .strip_prefix("port=")
        .ok_or(ConfigError::Missing("port"))?;
    value.parse().map_err(|source| ConfigError::BadPort {
        value: value.into(),
        source,
    })
}

/// "outer: middle: root": the usual one-line log format.
fn chain_string(e: &dyn Error) -> String {
    let mut out = e.to_string();
    let mut cur = e.source();
    while let Some(c) = cur {
        out.push_str(": ");
        out.push_str(&c.to_string());
        cur = c.source();
    }
    out
}

/// First error of type T anywhere in the chain.
fn find_cause<'a, T: Error + 'static>(e: &'a (dyn Error + 'static)) -> Option<&'a T> {
    std::iter::successors(Some(e), |c| (*c).source()).find_map(|c| c.downcast_ref::<T>())
}

fn section_errors() {
    assert_eq!(load("a", |_| Ok("port=80\n".into())).unwrap(), 80);
    let bad = load("a", |_| Ok("port=eighty".into())).unwrap_err();
    assert_eq!(
        chain_string(&bad),
        "invalid port \"eighty\": invalid digit found in string"
    );
    let miss = load("a", |_| Ok("host=x".into())).unwrap_err();
    assert!(miss.source().is_none() && miss.to_string() == "missing key port");
    let denied = load("/etc/x", |_| {
        Err(io::Error::new(io::ErrorKind::PermissionDenied, "denied"))
    })
    .unwrap_err();
    assert_eq!(chain_string(&denied), "cannot read config /etc/x: denied");
    let io_cause = find_cause::<io::Error>(&denied).expect("io error is in the chain");
    assert_eq!(io_cause.kind(), io::ErrorKind::PermissionDenied);
    assert!(find_cause::<ParseIntError>(&denied).is_none());
    // PITFALL: Debug is for developers (and what `main() -> Result` prints); Display is for users.
    assert!(format!("{denied:?}").starts_with("Read {") && !denied.to_string().contains("Read"));
    println!("1. typed errors and chains ok");
}

// ---- 2. Boxed errors at application boundaries -----------------------------------------------------------------
fn load_boxed(text: &str) -> Result<u16, BoxError> {
    if text.is_empty() {
        return Err("empty config".into()); // &str -> Box<dyn Error + Send + Sync>
    }
    let n: u32 = text.parse()?; // any Error + Send + Sync converts through From
    Ok(u16::try_from(n).map_err(|e| format!("port {n}: {e}"))?) // String converts too
}

fn section_boxed() {
    assert_eq!(load_boxed("8080").unwrap(), 8080);
    let from_str: BoxError = From::from("plain message"); // the conversion `?` and `.into()` use
    assert_eq!(from_str.to_string(), "plain message");
    assert_eq!(load_boxed("").unwrap_err().to_string(), "empty config");
    let e = load_boxed("x").unwrap_err();
    // is / downcast_ref: borrow the concrete type back.
    assert!(e.is::<ParseIntError>());
    assert!(e.downcast_ref::<ParseIntError>().is_some() && e.downcast_ref::<io::Error>().is_none());
    // downcast: take it back BY VALUE. On failure you get the box back unchanged.
    let e: BoxError = match e.downcast::<io::Error>() {
        Ok(_) => unreachable!("it is a ParseIntError"),
        Err(original) => original,
    };
    let concrete: Box<ParseIntError> = e.downcast::<ParseIntError>().unwrap();
    assert_eq!(*concrete.kind(), std::num::IntErrorKind::InvalidDigit);
    // downcast_mut edits in place.
    #[derive(Debug)]
    struct Counted(u32);
    impl fmt::Display for Counted {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(f, "failed {} times", self.0)
        }
    }
    impl Error for Counted {}
    let mut c: Box<dyn Error> = Box::new(Counted(1));
    c.downcast_mut::<Counted>().unwrap().0 += 1;
    assert_eq!(c.to_string(), "failed 2 times");
    // Send + Sync + 'static lets the error cross threads.
    let joined = std::thread::spawn(|| load_boxed("nope")).join().unwrap();
    assert!(joined.is_err());
    println!("2. boxed errors ok");
}

// ---- 3. io::Error as a carrier ---------------------------------------------------------------------------------
fn section_io_error() {
    let timeout = io::Error::from(io::ErrorKind::TimedOut);
    assert!(timeout.get_ref().is_none()); // a bare kind has no payload
    let custom = io::Error::other(ConfigError::Missing("host")); // other() since 1.74: kind Other
    assert_eq!(
        (custom.kind(), custom.to_string()),
        (io::ErrorKind::Other, "missing key host".to_string())
    );
    // get_ref borrows the wrapped error. NOTE: source() of an io::Error does not return the payload itself,
    // it forwards to the PAYLOAD's source(), so a chain walker will not "see" the payload's type.
    assert!(custom.get_ref().is_some_and(|p| p.is::<ConfigError>()));
    assert!(custom.source().is_none());
    assert!(find_cause::<ConfigError>(&custom).is_none());
    // into_inner / downcast take the payload out.
    let inner = io::Error::other(ConfigError::Missing("a"))
        .into_inner()
        .unwrap();
    assert!(inner.is::<ConfigError>());
    let typed = custom.downcast::<ConfigError>().expect("payload type"); // since 1.79
    assert!(matches!(typed, ConfigError::Missing("host")));
    println!("3. io::Error payloads ok");
}

// ---- 4. Any: runtime type checks -------------------------------------------------------------------------------
/// One value per type, like the "extensions" map on an HTTP request.
#[derive(Default)]
struct Extensions(HashMap<TypeId, Box<dyn Any + Send + Sync>>);
impl Extensions {
    fn insert<T: Any + Send + Sync>(&mut self, v: T) -> Option<T> {
        self.0
            .insert(TypeId::of::<T>(), Box::new(v))
            .and_then(|old| old.downcast::<T>().ok())
            .map(|b| *b)
    }
    fn get<T: Any>(&self) -> Option<&T> {
        self.0.get(&TypeId::of::<T>())?.downcast_ref::<T>()
    }
    fn get_mut<T: Any>(&mut self) -> Option<&mut T> {
        self.0.get_mut(&TypeId::of::<T>())?.downcast_mut::<T>()
    }
}
#[derive(Debug, PartialEq)]
struct UserId(u64);
#[derive(Debug, PartialEq)]
struct TraceId(u64);

fn render(v: &dyn Any) -> String {
    if let Some(n) = v.downcast_ref::<i64>() {
        format!("int {n}")
    } else if let Some(s) = v.downcast_ref::<String>() {
        format!("string {s}")
    } else if v.is::<&str>() {
        "str".into()
    } else {
        "unknown".into()
    }
}

/// Trait objects cannot be downcast unless the trait exposes Any.
trait Shape: Any {
    fn area(&self) -> f64;
}
struct Square(f64);
impl Shape for Square {
    fn area(&self) -> f64 {
        self.0 * self.0
    }
}

fn section_any() {
    let mut ext = Extensions::default();
    assert_eq!(ext.insert(UserId(1)), None);
    ext.insert(TraceId(9)); // same inner type u64, but a DIFFERENT newtype, so a different slot
    assert_eq!(ext.insert(UserId(2)), Some(UserId(1)));
    assert_eq!(
        (ext.get::<UserId>(), ext.get::<TraceId>(), ext.get::<u64>()),
        (Some(&UserId(2)), Some(&TraceId(9)), None)
    );
    ext.get_mut::<UserId>().unwrap().0 = 3;
    assert_eq!(ext.get::<UserId>(), Some(&UserId(3)));

    assert_eq!(render(&5i64), "int 5");
    assert_eq!(render(&String::from("a")), "string a");
    assert_eq!(render(&"lit"), "str");
    assert_eq!(render(&5i32), "unknown"); // exact type only: no numeric conversion, no subtyping

    // PITFALL (documented in std::any): Box<dyn Any> is itself Any. Method resolution on the box finds the BOX's id.
    let boxed: Box<dyn Any> = Box::new(3_i32);
    assert_eq!((&*boxed).type_id(), TypeId::of::<i32>());
    assert_eq!(boxed.type_id(), TypeId::of::<Box<dyn Any>>());
    assert_eq!(render(&boxed), "unknown"); // &Box<dyn Any> coerced to &dyn Any: the box, not the i32
    assert_eq!(render(boxed.as_ref()), "unknown"); // i32 has no arm above
    // Owning downcasts exist on Box, Rc and Arc.
    let b: Box<i32> = boxed.downcast::<i32>().unwrap();
    assert_eq!(*b, 3);
    let rc: Rc<dyn Any> = Rc::new(1u8);
    assert_eq!(rc.downcast::<u8>().map(|v| *v).ok(), Some(1));
    let arc: Arc<dyn Any + Send + Sync> = Arc::new(2u8);
    assert!(arc.clone().downcast::<u16>().is_err());
    assert_eq!(arc.downcast::<u8>().map(|v| *v).ok(), Some(2));
    // Since Rust 1.86 a &dyn Shape upcasts to &dyn Any because Any is a supertrait.
    let s: Box<dyn Shape> = Box::new(Square(2.0));
    let any: &dyn Any = s.as_ref();
    assert_eq!(any.downcast_ref::<Square>().map(|q| q.0), Some(2.0));
    assert_eq!(s.area(), 4.0);
    println!("4. Any and downcasting ok");
}

// ---- 5. type_name: diagnostics only ----------------------------------------------------------------------------
fn section_type_name() {
    // The exact text is unspecified and may change between compiler versions: assert only loosely, never persist it.
    assert!(type_name::<UserId>().contains("UserId"));
    assert!(type_name_of_val(&vec![1u8]).contains("Vec")); // since 1.76
    println!("   example: {}", type_name::<Option<Vec<UserId>>>());
    // TypeId is the thing to compare; it is also unstable across builds, so never serialise it.
    assert_ne!(TypeId::of::<i32>(), TypeId::of::<u32>());
    type Alias = i32;
    assert_eq!(TypeId::of::<Alias>(), TypeId::of::<i32>());
    println!("5. type_name ok");
}

// ---- 6. Marker and auto traits ---------------------------------------------------------------------------------
const fn assert_send<T: Send + ?Sized>() {}
const fn assert_sync<T: Sync + ?Sized>() {}
const fn assert_unpin<T: Unpin + ?Sized>() {}
const fn assert_copy<T: Copy>() {}
// Evaluated at COMPILE time: if a field change makes Extensions !Send, the build breaks here, not in a user's code.
const _: () = {
    assert_send::<Extensions>();
    assert_sync::<Extensions>();
    assert_send::<ConfigError>();
    assert_sync::<ConfigError>();
    assert_send::<BoxError>();
};

/// PhantomData: the type remembers a unit without storing anything.
struct Meters;
struct Feet;
#[derive(Clone, Copy)]
struct Distance<U> {
    value: f64,
    _unit: PhantomData<U>,
}
impl<U> Distance<U> {
    fn new(value: f64) -> Self {
        Distance {
            value,
            _unit: PhantomData,
        }
    }
    fn add(self, o: Self) -> Self {
        Self::new(self.value + o.value)
    }
}
struct SelfReferentialMarker {
    _pin: PhantomPinned, // opts OUT of Unpin
}

fn section_markers() {
    // Send: ownership may move to another thread. Sync: &T may be shared between threads (T: Sync iff &T: Send).
    assert_send::<std::cell::Cell<u8>>(); // movable, but...
    // assert_sync::<std::cell::Cell<u8>>();   // does not compile: Cell allows unsynchronised mutation through &
    // assert_send::<Rc<u8>>();                // does not compile: Rc's reference count is not atomic
    // assert_send::<std::sync::MutexGuard<'static, u8>>(); // does not compile: unlock must happen on the locking thread
    assert_sync::<std::sync::MutexGuard<'static, u8>>();
    assert_send::<Arc<std::sync::Mutex<u8>>>();
    assert_sync::<Arc<std::sync::Mutex<u8>>>();
    // Arc<T> needs T: Send + Sync to be Send; Arc<Cell<u8>> is neither.
    // Raw pointers are neither Send nor Sync, so a struct holding one loses both automatically (auto traits are structural).
    // PhantomData<T> inherits T's auto traits: PhantomData<*const ()> is the stable way to make a type !Send + !Sync.

    // Sized: generics have an implicit `T: Sized`. Opt out with ?Sized to accept str, [T] and dyn Trait behind a reference.
    fn len_of<T: AsRef<[u8]> + ?Sized>(t: &T) -> usize {
        t.as_ref().len()
    }
    assert_eq!(len_of("abc"), 3); // T = str, which is unsized
    // Unpin: "moving me after pinning is fine". Nearly everything is Unpin; PhantomPinned removes it.
    assert_unpin::<Vec<u8>>();
    assert_unpin::<Box<SelfReferentialMarker>>(); // the Box is Unpin even when its content is not
    // assert_unpin::<SelfReferentialMarker>();   // does not compile
    // Copy: bitwise duplication on assignment. It requires all fields Copy and no Drop impl.
    assert_copy::<(u8, &str)>();
    // assert_copy::<String>();                   // does not compile

    let d = Distance::<Meters>::new(1.0).add(Distance::new(2.0));
    assert_eq!(d.value, 3.0);
    // d.add(Distance::<Feet>::new(1.0));         // does not compile: mismatched types
    assert_eq!(size_of::<Distance<Feet>>(), size_of::<f64>());
    assert_eq!(
        (
            size_of::<PhantomData<[u8; 100]>>(),
            size_of::<PhantomPinned>()
        ),
        (0, 0)
    );
    println!("6. marker and auto traits ok");
}

// ---- 7. Niche optimisation -------------------------------------------------------------------------------------
fn section_niche() {
    // Documented guarantees (std::option "Representation"): no extra space for these.
    assert_eq!(size_of::<Option<Box<u64>>>(), size_of::<Box<u64>>());
    assert_eq!(size_of::<Option<&u64>>(), size_of::<&u64>());
    assert_eq!(size_of::<Option<NonZeroU32>>(), 4);
    assert_eq!(size_of::<Option<fn()>>(), size_of::<fn()>());
    assert!(size_of::<Option<u32>>() > 4);
    // Observed on this compiler but NOT a language guarantee: bool and char have spare values too.
    println!(
        "   Option<bool>={}B Option<char>={}B",
        size_of::<Option<bool>>(),
        size_of::<Option<char>>()
    );
    println!("7. niche sizes ok");
}

// ---- 8. Production tips and tricks -----------------------------------------------------------------------------
fn section_production_tips() {
    // 1. Libraries expose a concrete error type (callers can match); applications use a boxed error.
    // 2. Mark public error enums #[non_exhaustive] so a new variant is not a breaking change.
    // 3. Always bound boxed errors with Send + Sync + 'static: without them they cannot cross threads or tasks.
    const _: () = assert_send::<BoxError>();
    // 4. source() OR Display, not both (see header).
    let e = load("a", |_| Ok("port=z".into())).unwrap_err();
    assert!(!e.to_string().contains("invalid digit"));
    // 5. Never return Result<T, String> from a library: no source(), no matching (clippy::result_unit_err flags the `()` cousin).
    // 6. Keep error types small: a large Err bloats every Result (clippy::result_large_err); box the big variant.
    assert!(size_of::<Result<(), Box<ConfigError>>>() <= size_of::<Result<(), ConfigError>>());
    // 7. Decide retries on io::ErrorKind, never on message text.
    let retry = |e: &io::Error| {
        matches!(
            e.kind(),
            io::ErrorKind::TimedOut | io::ErrorKind::Interrupted
        )
    };
    assert!(retry(&io::ErrorKind::TimedOut.into()) && !retry(&io::Error::other("timed out")));
    // 8. Log the whole chain once, at the layer that handles the error; not at every layer it passes.
    assert!(chain_string(&e).contains("invalid digit"));
    // 9. Prefer an enum or a trait object of YOUR trait over dyn Any: Any loses all compile-time checking.
    // 10. Add compile-time Send/Sync assertions for public types, especially ones used across .await.
    // 11. A Drop-carrying or growing type should not be Copy: removing Copy later is a breaking change.
    println!("8. production tips ok");
}

fn main() {
    let mut seen = HashSet::new();
    assert!(
        COVERED.iter().all(|m| seen.insert(*m)),
        "COVERED has duplicates"
    );
    section_errors();
    section_boxed();
    section_io_error();
    section_any();
    section_type_name();
    section_markers();
    section_niche();
    section_production_tips();
    println!(
        "covered {} any/error/marker names; all asserts passed",
        COVERED.len()
    );
}
