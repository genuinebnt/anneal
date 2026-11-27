use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Test {
    Above(i64),
    AtLeast(i64),
    Below(i64),
    AtMost(i64),
    Exactly(i64),
}

/// The value of `field` must pass `test`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rule {
    pub field: u32,
    pub test: Test,
}

/// For every field that has a rule, the smallest and the largest value that pass all its rules.
/// `None` if some field has no such value.
pub fn allowed(rules: &[Rule]) -> Option<BTreeMap<u32, (i64, i64)>> {
    todo!()
}

/// `entries` is sorted by value, then id. The entries whose value is in `low..=high`.
pub fn between(entries: &[(i64, u32)], low: i64, high: i64) -> &[(i64, u32)] {
    todo!()
}
