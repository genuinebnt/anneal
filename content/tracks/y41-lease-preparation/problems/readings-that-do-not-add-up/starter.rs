use std::collections::HashMap;
use std::ops::Range;

/// A meter reports its running total at a time.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Reading {
    pub meter: u32,
    pub at: u64,
    pub total: u64,
}

/// What counts as wrong, and as out of date.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rules {
    /// May a total stay the same from one reading to the next?
    pub allow_equal: bool,
    /// A total may fall by this much.
    pub tolerance: u64,
    /// A reading older than this says nothing about now.
    pub max_age: u64,
}

impl Rules {
    /// The same rules with no tolerance and no equal totals.
    pub fn strict(&self) -> Rules {
        todo!()
    }
}

/// Each meter's readings, in the order of the list.
pub fn by_meter(readings: &[Reading]) -> HashMap<u32, Vec<Reading>> {
    todo!()
}

/// The total of the meter's newest reading, if that reading is not out of date at `now`.
pub fn current_total(
    meters: &HashMap<u32, Vec<Reading>>,
    meter: u32,
    now: u64,
    rules: &Rules,
) -> Option<u64> {
    todo!()
}

/// The first two neighbours in one meter's readings that break the rules.
pub fn first_break(readings: &[Reading], rules: &Rules) -> Option<(Reading, Reading)> {
    todo!()
}

/// The longest start of a day's list in which nothing breaks the rules.
pub fn trusted_start<'a>(day: &'a [Reading], rules: &Rules) -> &'a [Reading] {
    todo!()
}

/// The readings of a day could not be had.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FeedError {
    pub day: u64,
}

/// Somewhere readings come from.
pub trait Feed {
    fn day(&mut self, day: u64) -> Result<Vec<Reading>, FeedError>;
}

/// A feed that asks another for each day only until it has had an answer.
pub struct Remembering<'a> {
    // TODO: the fields (add what you need)
    source: &'a mut dyn Feed,
}

impl<'a> Remembering<'a> {
    pub fn new(source: &'a mut dyn Feed) -> Remembering<'a> {
        todo!()
    }
}

impl Feed for Remembering<'_> {
    fn day(&mut self, day: u64) -> Result<Vec<Reading>, FeedError> {
        todo!()
    }
}

/// The first day with a break, and the two readings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BadDay {
    pub day: u64,
    pub before: Reading,
    pub after: Reading,
}

/// Looks through `days` in order and stops at the first one that breaks the rules.
pub fn first_bad_day(
    feed: &mut dyn Feed,
    rules: &Rules,
    days: Range<u64>,
) -> Result<Option<BadDay>, FeedError> {
    todo!()
}
