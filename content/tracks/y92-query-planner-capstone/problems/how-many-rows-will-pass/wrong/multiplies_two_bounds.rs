use std::collections::BTreeMap;

// ---- column summaries ----

/// A summary cuts a column's sorted values into this many slices of (nearly) equal size.
pub const SLICES: usize = 32;
/// At most this many values are listed with their own counts.
pub const COMMON: usize = 8;

/// All the planner remembers about one column.
#[derive(Debug, Clone, PartialEq)]
pub struct ColumnStats {
    /// All rows, those with a missing value included.
    pub rows: usize,
    pub missing: usize,
    /// How many different values are present.
    pub distinct: usize,
    /// `SLICES + 1` values that bound the slices; empty if no value is present.
    pub cuts: Vec<i64>,
    /// Unusually frequent values with their exact counts, most frequent first.
    pub common: Vec<(i64, usize)>,
}

pub fn summarise(values: &[Option<i64>]) -> ColumnStats {
    let mut sorted: Vec<i64> = values.iter().flatten().copied().collect();
    sorted.sort_unstable();
    let present = sorted.len();
    let mut counts: Vec<(i64, usize)> = Vec::new();
    for &v in &sorted {
        match counts.last_mut() {
            Some((value, count)) if *value == v => *count += 1,
            _ => counts.push((v, 1)),
        }
    }
    let distinct = counts.len();
    let cuts = if present == 0 {
        Vec::new()
    } else {
        (0..=SLICES)
            .map(|i| sorted[i * (present - 1) / SLICES])
            .collect()
    };
    counts.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    counts.truncate(COMMON);
    // more than 1.25 times the average count, in whole numbers
    counts.retain(|&(_, count)| 4 * count * distinct > 5 * present);
    ColumnStats {
        rows: values.len(),
        missing: values.len() - present,
        distinct,
        cuts,
        common: counts,
    }
}

impl ColumnStats {
    pub fn present(&self) -> usize {
        self.rows - self.missing
    }

    /// The estimated number of rows whose value is `value`.
    pub fn equal_to(&self, value: i64) -> f64 {
        let (Some(&low), Some(&high)) = (self.cuts.first(), self.cuts.last()) else {
            return 0.0;
        };
        if value < low || value > high {
            return 0.0;
        }
        if let Some(&(_, count)) = self.common.iter().find(|c| c.0 == value) {
            return count as f64;
        }
        let listed: usize = self.common.iter().map(|c| c.1).sum();
        match self.distinct - self.common.len() {
            0 => 0.0,
            others => (self.present() - listed) as f64 / others as f64,
        }
    }

    /// The estimated number of present values that are at most `value`.
    fn at_most(&self, value: i64) -> f64 {
        let (Some(&low), Some(&high)) = (self.cuts.first(), self.cuts.last()) else {
            return 0.0;
        };
        if value < low {
            return 0.0;
        }
        if value >= high {
            return self.present() as f64;
        }
        let slice = self.cuts[..SLICES].partition_point(|&c| c <= value) - 1;
        let (a, b) = (self.cuts[slice] as i128, self.cuts[slice + 1] as i128);
        let inside = (value as i128 - a + 1) as f64 / (b - a + 1) as f64;
        (slice as f64 + inside) / SLICES as f64 * self.present() as f64
    }

    /// The estimated number of rows whose value is in `low..=high`.
    pub fn between(&self, low: i64, high: i64) -> f64 {
        if low > high {
            return 0.0;
        }
        if low == high {
            return self.equal_to(low);
        }
        let below = low.checked_sub(1).map_or(0.0, |v| self.at_most(v));
        (self.at_most(high) - below).max(0.0)
    }
}

// ---- conditions and how much they keep ----

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Column {
    /// The table's number in the catalog.
    pub table: usize,
    /// The column's number in that table.
    pub column: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cmp {
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Cond {
    /// `column <cmp> number`
    Compare(Column, Cmp, i64),
    /// `column = column`, for two different columns
    Same(Column, Column),
    /// `column in (numbers)`
    OneOf(Column, Vec<i64>),
    /// Every part holds. An empty list always holds.
    All(Vec<Cond>),
    /// At least one part holds. An empty list never holds.
    Any(Vec<Cond>),
}

#[derive(Debug, Clone)]
pub struct Table {
    pub name: String,
    pub column_names: Vec<String>,
    pub rows: usize,
    /// One summary per column.
    pub stats: Vec<ColumnStats>,
    /// The columns that have an index.
    pub indexed: Vec<usize>,
}

#[derive(Debug, Clone)]
pub struct Catalog {
    pub tables: Vec<Table>,
}

/// The inclusive range of numbers that `x <cmp> value` allows; `low > high` if there are none.
/// `None` for `Ne`, which is not one range.
pub fn range_of(cmp: Cmp, value: i64) -> Option<(i64, i64)> {
    const NOTHING: (i64, i64) = (1, 0);
    Some(match cmp {
        Cmp::Eq => (value, value),
        Cmp::Lt => value.checked_sub(1).map_or(NOTHING, |h| (i64::MIN, h)),
        Cmp::Le => (i64::MIN, value),
        Cmp::Gt => value.checked_add(1).map_or(NOTHING, |l| (l, i64::MAX)),
        Cmp::Ge => (value, i64::MAX),
        Cmp::Ne => return None,
    })
}

/// The estimated fraction of rows for which the condition holds, from 0 to 1.
pub fn selectivity(condition: &Cond, catalog: &Catalog) -> f64 {
    let stats = |c: &Column| &catalog.tables[c.table].stats[c.column];
    let share = |c: &Column, matching: f64| match catalog.tables[c.table].rows {
        0 => 0.0,
        rows => (matching / rows as f64).clamp(0.0, 1.0),
    };
    match condition {
        Cond::Compare(c, cmp, value) => match cmp {
            Cmp::Eq => share(c, stats(c).equal_to(*value)),
            Cmp::Ne => share(c, stats(c).present() as f64 - stats(c).equal_to(*value)),
            _ => {
                let (low, high) = range_of(*cmp, *value).expect("an ordering");
                share(c, stats(c).between(low, high))
            }
        },
        Cond::Same(a, b) => match (stats(a).distinct, stats(b).distinct) {
            (0, _) | (_, 0) => 0.0,
            (x, y) => 1.0 / x.max(y) as f64,
        },
        Cond::OneOf(c, values) => {
            let mut values = values.clone();
            values.sort_unstable();
            values.dedup();
            let sum: f64 = values.iter().map(|&v| share(c, stats(c).equal_to(v))).sum();
            sum.min(1.0)
        }
        Cond::All(_) => {
            let mut parts = Vec::new();
            conjuncts(condition, &mut parts);
            let mut ranges: BTreeMap<Column, (i64, i64)> = BTreeMap::new();
            let mut kept = 1.0;
            for part in &parts {
                match part {
                    Cond::Compare(c, cmp @ (Cmp::Lt | Cmp::Le | Cmp::Gt | Cmp::Ge), value)
                        if false =>
                    {
                        let (low, high) = range_of(*cmp, *value).expect("an ordering");
                        let range = ranges.entry(*c).or_insert((i64::MIN, i64::MAX));
                        *range = (range.0.max(low), range.1.min(high));
                    }
                    other => kept *= selectivity(other, catalog),
                }
            }
            for (c, (low, high)) in &ranges {
                kept *= share(c, stats(c).between(*low, *high));
            }
            kept
        }
        Cond::Any(parts) => {
            let missed: f64 = parts
                .iter()
                .map(|p| 1.0 - selectivity(p, catalog))
                .product();
            1.0 - missed
        }
    }
}

/// The parts that must all hold: nested `All` lists are opened up, everything else is one part.
pub fn conjuncts(condition: &Cond, out: &mut Vec<Cond>) {
    match condition {
        Cond::All(parts) => parts.iter().for_each(|p| conjuncts(p, out)),
        other => out.push(other.clone()),
    }
}

/// How far an estimate is off, as a factor of at least 1. Both numbers count as at least 1 row.
pub fn q_error(estimate: f64, actual: f64) -> f64 {
    let (e, a) = (estimate.max(1.0), actual.max(1.0));
    (e / a).max(a / e)
}
