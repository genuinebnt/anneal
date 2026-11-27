use std::collections::hash_map::Entry;
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
        Rules {
            allow_equal: false,
            tolerance: 0,
            ..*self
        }
    }

    fn breaks(&self, before: Reading, after: Reading) -> bool {
        after.total.saturating_add(self.tolerance) < before.total
            || (!self.allow_equal && after.total == before.total)
    }
}

/// Each meter's readings, in the order of the list.
pub fn by_meter(readings: &[Reading]) -> HashMap<u32, Vec<Reading>> {
    let mut meters: HashMap<u32, Vec<Reading>> = HashMap::new();
    for &reading in readings {
        meters.entry(reading.meter).or_default().push(reading);
    }
    meters
}

/// The total of the meter's newest reading, if that reading is not out of date at `now`.
pub fn current_total(
    meters: &HashMap<u32, Vec<Reading>>,
    meter: u32,
    now: u64,
    rules: &Rules,
) -> Option<u64> {
    let newest = meters.get(&meter)?.iter().max_by_key(|r| (r.at, r.total))?;
    now.checked_sub(newest.at)
        .filter(|&age| age <= rules.max_age)?;
    Some(newest.total)
}

/// The first two neighbours in one meter's readings that break the rules.
pub fn first_break(readings: &[Reading], rules: &Rules) -> Option<(Reading, Reading)> {
    readings
        .windows(2)
        .find(|pair| rules.breaks(pair[0], pair[1]))
        .map(|pair| (pair[0], pair[1]))
}

/// The first reading in a day's list that breaks the rules against the one before it from the same meter.
fn first_bad_reading(day: &[Reading], rules: &Rules) -> Option<(Reading, Reading)> {
    let mut last: HashMap<u32, Reading> = HashMap::new();
    day.iter().find_map(|&reading| {
        let before = last.insert(reading.meter, reading)?;
        rules.breaks(before, reading).then_some((before, reading))
    })
}

/// The longest start of a day's list in which nothing breaks the rules.
pub fn trusted_start<'a>(day: &'a [Reading], rules: &Rules) -> &'a [Reading] {
    let mut last: HashMap<u32, Reading> = HashMap::new();
    let end = day.iter().position(|&reading| {
        let before = last.insert(reading.meter, reading);
        before.is_some_and(|before| rules.breaks(before, reading))
    });
    &day[..end.unwrap_or(day.len())]
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
    source: &'a mut dyn Feed,
    days: HashMap<u64, Vec<Reading>>,
}

impl<'a> Remembering<'a> {
    pub fn new(source: &'a mut dyn Feed) -> Remembering<'a> {
        Remembering {
            source,
            days: HashMap::new(),
        }
    }
}

impl Feed for Remembering<'_> {
    fn day(&mut self, day: u64) -> Result<Vec<Reading>, FeedError> {
        let readings = match self.days.entry(day) {
            Entry::Occupied(known) => known.into_mut(),
            Entry::Vacant(slot) => match self.source.day(day) {
                Ok(readings) => slot.insert(readings),
                Err(error) => {
                    slot.insert(Vec::new());
                    return Err(error);
                }
            },
        };
        Ok(readings.clone())
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
    for day in days {
        if let Some((before, after)) = first_bad_reading(&feed.day(day)?, rules) {
            return Ok(Some(BadDay { day, before, after }));
        }
    }
    Ok(None)
}
