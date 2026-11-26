//! RECIPE 34 (language): the std traits you will implement or derive constantly.
//! LEVEL: simple
//! Use in: domain types (ids, states, errors), maps/sets, sorting, printing.   Run: cargo run --example common_traits
//! COMMON USES: domain types used as map keys, sorted collections, log output, CLI parsing, defaults for config structs.
//!
//! Debug/Display (formatting) | Clone/Copy | PartialEq/Eq/Hash (map keys) | PartialOrd/Ord (sorting, BinaryHeap)
//! | Default | FromStr | Deref | Iterator. Derive when the obvious behaviour is right; implement by hand when it is not.
//! Guideline: implement the common traits eagerly https://rust-lang.github.io/api-guidelines/interoperability.html
#![allow(dead_code)]
use std::cmp::Ordering;
use std::collections::{BTreeSet, HashMap};
use std::fmt;
use std::ops::Deref;
use std::str::FromStr;

// derive the usual bundle for an id type. `Copy` is fine because it is small and has no heap data.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
struct JobId(u32);

// Display = for users; Debug = for developers. Implement Display by hand.
impl fmt::Display for JobId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "job-{}", self.0)
    }
}
// FromStr makes `"job-7".parse::<JobId>()` work
impl FromStr for JobId {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, String> {
        s.strip_prefix("job-")
            .ok_or_else(|| format!("`{s}` must start with `job-`"))?
            .parse::<u32>()
            .map(JobId)
            .map_err(|e| e.to_string())
    }
}

// Custom ORDERING: highest priority first, then smallest id. (Derive would order by field order, ascending.)
#[derive(Debug, PartialEq, Eq)]
struct Job {
    id: JobId,
    priority: u8,
}
impl Ord for Job {
    fn cmp(&self, o: &Self) -> Ordering {
        o.priority.cmp(&self.priority).then(self.id.cmp(&o.id))
    }
}
impl PartialOrd for Job {
    fn partial_cmp(&self, o: &Self) -> Option<Ordering> {
        Some(self.cmp(o)) // always delegate to Ord when you have a total order
    }
}

// DEFAULT + struct update syntax
#[derive(Debug, Clone)]
struct Options {
    retries: u32,
    timeout_ms: u64,
    verbose: bool,
}
impl Default for Options {
    fn default() -> Self {
        Options {
            retries: 3,
            timeout_ms: 30_000,
            verbose: false,
        }
    }
}

// DEREF newtype: lets `&Name` act like `&str` (use sparingly; prefer explicit methods for non-wrapper types)
struct Name(String);
impl Deref for Name {
    type Target = str;
    fn deref(&self) -> &str {
        &self.0
    }
}

fn main() {
    let id: JobId = "job-7".parse().unwrap();
    println!("{id} / {id:?} / {:?}", "7".parse::<JobId>());
    let mut counts: HashMap<JobId, u32> = HashMap::new(); // Hash + Eq => usable as key
    *counts.entry(id).or_default() += 1;
    println!("{counts:?} default={:?}", JobId::default());

    let jobs: BTreeSet<Job> = [
        Job {
            id: JobId(2),
            priority: 1,
        },
        Job {
            id: JobId(1),
            priority: 9,
        },
        Job {
            id: JobId(3),
            priority: 9,
        },
    ]
    .into_iter()
    .collect();
    println!("{:?}", jobs.iter().map(|j| j.id.0).collect::<Vec<_>>()); // [1, 3, 2]

    let o = Options {
        retries: 5,
        ..Default::default()
    };
    println!("{o:?}");
    let n = Name("flowforge".into());
    println!("{} {}", n.len(), n.to_uppercase()); // str methods via Deref
    println!(
        "{:>8}|{:<8}|{:^8}|{:08.3}|{:#x}|{:#?}",
        "right",
        "left",
        "mid",
        3.14159,
        255,
        (1, "a")
    );
}
