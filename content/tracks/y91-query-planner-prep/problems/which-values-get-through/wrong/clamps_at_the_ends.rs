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

/// The inclusive range a single test allows; `None` if it allows nothing.
fn range(test: Test) -> Option<(i64, i64)> {
    Some(match test {
        Test::Above(v) => (v.saturating_add(1), i64::MAX),
        Test::AtLeast(v) => (v, i64::MAX),
        Test::Below(v) => (i64::MIN, v.saturating_sub(1)),
        Test::AtMost(v) => (i64::MIN, v),
        Test::Exactly(v) => (v, v),
    })
}

/// For every field that has a rule, the smallest and the largest value that pass all its rules.
/// `None` if some field has no such value.
pub fn allowed(rules: &[Rule]) -> Option<BTreeMap<u32, (i64, i64)>> {
    let mut ranges = BTreeMap::new();
    for rule in rules {
        let (from, to) = range(rule.test)?;
        let kept = ranges.entry(rule.field).or_insert((i64::MIN, i64::MAX));
        *kept = (from.max(kept.0), to.min(kept.1));
        if kept.0 > kept.1 {
            return None;
        }
    }
    Some(ranges)
}

/// `entries` is sorted by value, then id. The entries whose value is in `low..=high`.
pub fn between(entries: &[(i64, u32)], low: i64, high: i64) -> &[(i64, u32)] {
    let from = entries.partition_point(|e| e.0 < low);
    let to = entries.partition_point(|e| e.0 <= high);
    &entries[from..to.max(from)]
}
