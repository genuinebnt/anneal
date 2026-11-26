use std::borrow::Borrow;
use std::cmp::Ordering;
use std::fmt;
use std::hash::{Hash, Hasher};
use std::ops::Deref;

/// Up to 22 bytes live inside the value; longer strings go to the heap. `Box<str>` (16 bytes) keeps the
/// heap variant as small as the inline one: tag, length and 22 bytes is 24, and so is tag, padding and
/// the box. A `String` there (24 bytes) would make every value 32.
#[derive(Clone)]
enum Repr {
    Inline {
        len: u8,
        buf: [u8; SmallStr::INLINE_CAP],
    },
    Heap(Box<str>),
}

/// An immutable string for a column of mostly short values (country codes, user names, enum-like tags).
#[derive(Clone)]
pub struct SmallStr(Repr);

impl SmallStr {
    /// The longest string kept inline, in bytes.
    pub const INLINE_CAP: usize = 22;

    pub fn new(s: &str) -> SmallStr {
        if s.len() <= SmallStr::INLINE_CAP {
            let mut buf = [0; SmallStr::INLINE_CAP];
            buf[..s.len()].copy_from_slice(s.as_bytes());
            SmallStr(Repr::Inline {
                len: s.len() as u8,
                buf,
            })
        } else {
            SmallStr(Repr::Heap(Box::from(s)))
        }
    }

    pub fn as_str(&self) -> &str {
        match &self.0 {
            // Always a whole `&str`'s bytes, so this can't fail (compact_str skips the check with unsafe).
            Repr::Inline { len, buf } => {
                std::str::from_utf8(&buf[..*len as usize]).expect("copied from a &str")
            }
            Repr::Heap(s) => s,
        }
    }

    /// Whether the bytes live inside the value rather than on the heap.
    pub fn is_inline(&self) -> bool {
        matches!(self.0, Repr::Inline { .. })
    }
}

/// Equality, hashing and ordering all go through `as_str`, so they agree with `str`'s, as `Borrow<str>`
/// requires.
impl PartialEq for SmallStr {
    fn eq(&self, other: &SmallStr) -> bool {
        self.as_str() == other.as_str()
    }
}

impl Eq for SmallStr {}

/// Must hash exactly like the `str` it holds: `Borrow<str>` promises that, and `HashSet<SmallStr>` looks
/// values up by `&str`.
impl Hash for SmallStr {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.as_str().hash(state);
    }
}

impl Ord for SmallStr {
    fn cmp(&self, other: &SmallStr) -> Ordering {
        self.as_str().cmp(other.as_str())
    }
}

/// Prints like a `&str`: `"abc"`.
impl fmt::Debug for SmallStr {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        fmt::Debug::fmt(self.as_str(), f)
    }
}

impl fmt::Display for SmallStr {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        fmt::Display::fmt(self.as_str(), f)
    }
}

impl Deref for SmallStr {
    type Target = str;
    fn deref(&self) -> &str {
        self.as_str()
    }
}

impl Borrow<str> for SmallStr {
    fn borrow(&self) -> &str {
        self.as_str()
    }
}

impl From<&str> for SmallStr {
    fn from(s: &str) -> SmallStr {
        SmallStr::new(s)
    }
}

impl PartialOrd for SmallStr {
    fn partial_cmp(&self, other: &SmallStr) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
