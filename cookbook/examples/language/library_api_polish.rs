//! RECIPE 113 (api-design): the polish high-quality libraries apply to a public API.
//! LEVEL: intermediate
//! Use in: any crate you publish or any internal crate other teams depend on.   Run: cargo run --release --example library_api_polish
#![allow(dead_code)]
//!
//! The mini module `shelf` below is written as if it were a library crate; `main` plays the downstream user.
//!  - `#[non_exhaustive]` on public enums/structs: adding a variant/field later is NOT a breaking change, because outside the crate
//!    users must write `_ =>` in matches and cannot build the struct with a literal. (Inside the defining crate it has no effect.)
//!    API Guidelines, future-proofing: https://rust-lang.github.io/api-guidelines/future-proofing.html (C-STRUCT-PRIVATE, C-SEALED).
//!  - `#[must_use]` on builders and Results-like values: ignoring them is almost certainly a bug.
//!  - SEALED trait: public trait that outsiders can call but not implement (supertrait in a private module), so you may add methods.
//!  - EXTENSION trait: add methods to foreign types (`StrExt for str`) without newtype wrapping.
//!  - BUILDER with `impl Into<String>` (accepts &str and String) and `impl AsRef<str>` (borrow only): flexible, allocation-honest.
//!  - `#[deprecated(since, note)]` keeps old names compiling while steering users to the new one.
//!  - `Default`, `From`/`TryFrom` for conversions (C-CONV-TRAIT, C-COMMON-TRAITS), errors implementing `Display` + `std::error::Error`
//!    with `source()` so chains work with `?`, `anyhow`, and logging.  Error messages: lowercase, no trailing punctuation (C-GOOD-ERR).
//!  - `pub(crate)` for internals: everything `pub` is a promise. Return `impl Iterator<Item = &str> + '_` instead of exposing a Vec field.
//!  - FEATURE FLAGS: in a real crate, `#[cfg(feature = "serde")] impl Serialize for Shelf {..}` with `[features] serde = ["dep:serde"]` in
//!    Cargo.toml makes dependencies opt-in; features must be ADDITIVE (enabling one must never remove API). Not enabled here.
//!  - DOC TESTS: code blocks in `///` comments are compiled and run by `cargo test --doc`, so examples cannot rot, e.g.
//!    `/// let s = shelf::Shelf::builder().name("a").build().unwrap();` (they only run for lib crates, not examples).
//! Checklist: https://rust-lang.github.io/api-guidelines/checklist.html
//! COMMON USES: reqwest/clap-style builders, `std::io::ErrorKind` (non_exhaustive), `tokio` extension traits (`AsyncReadExt`),
//! sealed traits in `futures`/`tower`-family crates, `cargo-semver-checks` to catch accidental breaking changes.
//! Read: https://rust-lang.github.io/api-guidelines/ , https://doc.rust-lang.org/cargo/reference/features.html
mod shelf {
    use std::convert::TryFrom;
    use std::fmt;

    mod sealed {
        pub trait Sealed {}
    }

    /// Sealed: downstream crates can use `Storage` bounds but cannot implement it.
    pub trait Storage: sealed::Sealed {
        fn label(&self) -> &'static str;
    }
    pub struct Memory;
    pub struct Disk;
    impl sealed::Sealed for Memory {}
    impl sealed::Sealed for Disk {}
    impl Storage for Memory {
        fn label(&self) -> &'static str {
            "memory"
        }
    }
    impl Storage for Disk {
        fn label(&self) -> &'static str {
            "disk"
        }
    }

    #[derive(Debug)]
    #[non_exhaustive]
    pub enum Error {
        EmptyName,
        TooManyTags { max: usize },
        BadLevel(ParseLevelError),
    }
    #[derive(Debug, PartialEq)]
    pub struct ParseLevelError(pub i32);

    impl fmt::Display for ParseLevelError {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(f, "level {} is out of range 0..=2", self.0)
        }
    }
    impl std::error::Error for ParseLevelError {}
    impl fmt::Display for Error {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            match self {
                Error::EmptyName => f.write_str("shelf name must not be empty"),
                Error::TooManyTags { max } => write!(f, "at most {max} tags are allowed"),
                Error::BadLevel(_) => f.write_str("invalid level"),
            }
        }
    }
    impl std::error::Error for Error {
        fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
            match self {
                Error::BadLevel(e) => Some(e),
                _ => None,
            }
        }
    }
    impl From<ParseLevelError> for Error {
        fn from(e: ParseLevelError) -> Self {
            Error::BadLevel(e)
        }
    }

    #[derive(Debug, Clone, Copy, PartialEq, Default)]
    pub enum Level {
        #[default]
        Low,
        Mid,
        High,
    }
    impl TryFrom<i32> for Level {
        type Error = ParseLevelError;
        fn try_from(n: i32) -> Result<Self, ParseLevelError> {
            match n {
                0 => Ok(Level::Low),
                1 => Ok(Level::Mid),
                2 => Ok(Level::High),
                _ => Err(ParseLevelError(n)),
            }
        }
    }
    impl From<Level> for i32 {
        fn from(l: Level) -> i32 {
            l as i32
        }
    }

    #[derive(Debug, Clone, PartialEq)]
    #[non_exhaustive]
    pub struct Shelf {
        pub name: String,
        pub level: Level,
        tags: Vec<String>,
    }
    pub(crate) const MAX_TAGS: usize = 3;

    impl Shelf {
        pub fn builder() -> ShelfBuilder {
            ShelfBuilder::default()
        }
        /// Borrowing iterator: callers cannot see or depend on the `Vec`.
        pub fn tags(&self) -> impl Iterator<Item = &str> + '_ {
            self.tags.iter().map(String::as_str)
        }
        #[deprecated(since = "0.2.0", note = "use `Shelf::builder().name(..)` instead")]
        pub fn new(name: &str) -> Shelf {
            Shelf {
                name: name.to_string(),
                level: Level::default(),
                tags: vec![],
            }
        }
    }

    #[must_use = "a builder does nothing until `build()` is called"]
    #[derive(Default, Debug)]
    pub struct ShelfBuilder {
        name: String,
        level: Level,
        tags: Vec<String>,
    }
    impl ShelfBuilder {
        pub fn name(mut self, n: impl Into<String>) -> Self {
            self.name = n.into();
            self
        }
        pub fn tag(mut self, t: impl AsRef<str>) -> Self {
            self.tags.push(t.as_ref().to_lowercase());
            self
        }
        pub fn level(mut self, n: i32) -> Result<Self, Error> {
            self.level = Level::try_from(n)?; // ParseLevelError -> Error via From
            Ok(self)
        }
        pub fn build(self) -> Result<Shelf, Error> {
            if self.name.is_empty() {
                return Err(Error::EmptyName);
            }
            if self.tags.len() > MAX_TAGS {
                return Err(Error::TooManyTags { max: MAX_TAGS });
            }
            Ok(Shelf {
                name: self.name,
                level: self.level,
                tags: self.tags,
            })
        }
    }

    /// Extension trait on a foreign type.
    pub trait StrExt {
        fn shelf_name(&self) -> Result<Shelf, Error>;
    }
    impl StrExt for str {
        fn shelf_name(&self) -> Result<Shelf, Error> {
            Shelf::builder().name(self).build()
        }
    }
}

use shelf::{Disk, Error, Level, Memory, Shelf, Storage, StrExt};
use std::error::Error as StdError;

fn describe(s: &impl Storage) -> &'static str {
    s.label()
}

fn main() {
    // builder: &str, String and &String all work thanks to Into/AsRef
    let s = Shelf::builder()
        .name("books")
        .tag("Rust")
        .tag(String::from("API"))
        .build()
        .unwrap();
    assert_eq!(s.name, "books");
    assert_eq!(s.tags().collect::<Vec<_>>(), vec!["rust", "api"]);
    assert_eq!(
        Shelf::builder()
            .name(String::from("x"))
            .build()
            .unwrap()
            .level,
        Level::default()
    );

    // errors, Display, source chain
    let e = Shelf::builder().build().unwrap_err();
    assert_eq!(e.to_string(), "shelf name must not be empty");
    let e = Shelf::builder()
        .name("a")
        .tag("1")
        .tag("2")
        .tag("3")
        .tag("4")
        .build()
        .unwrap_err();
    assert_eq!(e.to_string(), "at most 3 tags are allowed");
    let e = Shelf::builder().level(7).map(|_| ()).unwrap_err();
    assert_eq!(e.to_string(), "invalid level");
    assert_eq!(
        e.source().unwrap().to_string(),
        "level 7 is out of range 0..=2"
    );

    // non_exhaustive: downstream code must keep a wildcard arm
    let msg = match e {
        Error::EmptyName => "empty",
        Error::BadLevel(_) => "level",
        _ => "other / future variant",
    };
    assert_eq!(msg, "level");

    // conversions
    assert_eq!(Level::try_from(2), Ok(Level::High));
    assert_eq!(i32::from(Level::Mid), 1);
    let lvl: Result<Level, _> = 9.try_into();
    assert!(lvl.is_err());

    // extension trait + sealed trait usable as a bound
    assert_eq!("hi".shelf_name().unwrap().name, "hi");
    assert!("".shelf_name().is_err());
    assert_eq!((describe(&Memory), describe(&Disk)), ("memory", "disk"));

    // deprecated API still works but warns for users, so opt in explicitly here
    #[allow(deprecated)]
    let old = Shelf::new("legacy");
    assert_eq!(old.name, "legacy");
    println!("API polish OK: {:?}", s);
}
