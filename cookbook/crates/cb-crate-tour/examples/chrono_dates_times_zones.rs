//! RECIPE 1100 (crates): chrono: dates, times, durations, formatting, time zones, DST and schedules
//! LEVEL: intermediate
//! Use in: billing periods, schedulers, log/event timestamps, report windows, APIs that exchange RFC 3339.   Run: cargo run --release -p cb-crate-tour --example chrono_dates_times_zones
//! COMMON USES: storing `DateTime<Utc>` in databases, rendering local times for users, business-day SLAs, cron-like recurring jobs,
//! age/anniversary maths, JSON APIs (serde), parsing log timestamps in odd formats.
//! METHODS: NaiveDate::from_ymd_opt, NaiveDate::checked_add_months, NaiveDate::checked_add_days, NaiveDate::weekday, NaiveDate::iso_week, NaiveDate::signed_duration_since, NaiveDate::succ_opt, NaiveDate::with_day, NaiveTime::from_hms_opt, NaiveDateTime::and_utc, NaiveDate::and_hms_opt, DateTime::parse_from_rfc3339, DateTime::parse_from_rfc2822, DateTime::to_rfc3339, DateTime::with_timezone, DateTime::timestamp, DateTime::from_timestamp, Utc::now, Utc::with_ymd_and_hms, TimeZone::from_local_datetime, LocalResult, Duration::hours, TimeDelta::num_minutes, NaiveDateTime::parse_from_str, format, Datelike::month, Timelike::hour, FixedOffset::east_opt, Months, Weekday::succ, DateTime::signed_duration_since, DurationRound::duration_trunc
//! DOCS: https://docs.rs/chrono/0.4/chrono/
//! CRATES: chrono = "0.4" (0.4.45 is current; this chapter pins ^0.4), chrono-tz = "0.10"
//! NOT COVERED (and why): `Local` assertions (machine dependent; only constructed), `NaiveWeek` details, deprecated `Duration::seconds` family panics, `serde::ts_*` helpers beyond one example.
//!
//! CHEAT SHEET (I want to ... -> use ...)
//!   store an instant                      -> DateTime<Utc> (or i64 epoch seconds); convert at the edge
//!   a calendar date without a zone        -> NaiveDate (birthday, due date, billing day)
//!   parse "2024-03-05T10:00:00+02:00"     -> DateTime::parse_from_rfc3339 (keeps the offset: DateTime<FixedOffset>)
//!   parse a custom log format             -> NaiveDateTime::parse_from_str(s, "%d/%b/%Y:%H:%M:%S") then .and_utc()
//!   add 1 month safely                    -> date.checked_add_months(Months::new(1)) (clamps Jan 31 -> Feb 28/29)
//!   add 24h vs "same time tomorrow"       -> Duration::hours(24) on UTC vs add Days on the LOCAL naive time
//!   difference between two instants       -> a.signed_duration_since(b) -> TimeDelta, then num_seconds()/num_days()
//!   weekday / ISO week                    -> date.weekday(), date.iso_week().week()
//!   convert zone                          -> dt.with_timezone(&Tz) (never re-interpret the wall clock!)
//!   a local wall time in a real zone      -> tz.from_local_datetime(&naive) -> LocalResult (None in gaps, Ambiguous in overlaps)
//!   epoch seconds -> instant              -> DateTime::from_timestamp(secs, nanos) -> Option
//!   round to the hour/day                 -> DurationRound::duration_trunc / DateTime::date_naive
//!   first/last day of month               -> with_day(1) ; next month first day minus 1 day
//!   JSON                                  -> chrono's `serde` feature: RFC 3339 strings; ts_seconds for numbers
//!
//! Read: chrono docs https://docs.rs/chrono ; IANA tz notes https://data.iana.org/time-zones/tz-link.html ; "Falsehoods programmers believe about time" (Noah Sussman, title only)
//! TRY NEXT:
//!   - [S] Change `add_months_clamped` to use `Months::new(12)` on 2024-02-29 and predict the result before running.
//!   - [S] What does `parse_from_rfc3339("2024-03-05 10:00:00Z")` do (space instead of T)? Predict, then try `parse_from_str`.
//!   - [M] Extend `business_days_between` with a holiday set (`HashSet<NaiveDate>`) and a half-day rule.
//!   - [M] Port `RecurringSchedule` to "last Friday of every month" and assert 12 outputs for 2025.
//!   - [A] PROJECT: combine with `time_crate_dates_formatting` (same table in `time`) and `sqlx_sqlite_queries_transactions` (store the schedule's next_run as TEXT, query due jobs with keyset pagination).
#![allow(dead_code)]
use chrono::{
    DateTime, Datelike, Days, Duration, DurationRound, FixedOffset, IsoWeek, LocalResult, Months,
    NaiveDate, NaiveDateTime, NaiveTime, TimeDelta, TimeZone, Timelike, Utc, Weekday,
};
use chrono_tz::Tz;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

fn d(y: i32, m: u32, day: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(y, m, day).expect("valid date")
}

// ---------------------------------------------------------------------------------------------------------------
// 1. Everyday core: construct, inspect, compare, format, parse
// ---------------------------------------------------------------------------------------------------------------
fn everyday_core() {
    // Constructors return Option: invalid dates are values, not panics. (Pitfall: `from_ymd` without _opt is deprecated.)
    assert!(
        NaiveDate::from_ymd_opt(2023, 2, 29).is_none(),
        "2023 is not a leap year"
    );
    assert!(NaiveDate::from_ymd_opt(2024, 2, 29).is_some());
    let date = d(2024, 3, 5);
    assert_eq!((date.year(), date.month(), date.day()), (2024, 3, 5));
    assert_eq!(date.weekday(), Weekday::Tue);
    assert_eq!(date.ordinal(), 65); // 31 + 29 + 5
    assert_eq!(date.iso_week().week(), 10);

    let time = NaiveTime::from_hms_opt(14, 30, 15).unwrap();
    assert_eq!((time.hour(), time.minute(), time.second()), (14, 30, 15));
    let ndt = date.and_time(time);
    assert_eq!(ndt.to_string(), "2024-03-05 14:30:15");
    assert_eq!(date.and_hms_opt(25, 0, 0), None);

    // Instants: DateTime<Utc>. `with_ymd_and_hms` returns LocalResult (single() for UTC always works).
    let t = Utc
        .with_ymd_and_hms(2024, 3, 5, 14, 30, 15)
        .single()
        .unwrap();
    assert_eq!(t.to_rfc3339(), "2024-03-05T14:30:15+00:00");
    assert_eq!(t.timestamp(), 1_709_649_015);
    assert_eq!(DateTime::from_timestamp(1_709_649_015, 0).unwrap(), t);
    assert_eq!(t.timestamp_millis(), 1_709_649_015_000);
    assert_eq!(ndt.and_utc(), t); // naive + "this is UTC" claim

    // Formatting with strftime specs.
    assert_eq!(
        t.format("%Y-%m-%d %H:%M:%S").to_string(),
        "2024-03-05 14:30:15"
    );
    assert_eq!(t.format("%a %d %b %Y").to_string(), "Tue 05 Mar 2024");
    assert_eq!(
        t.format("%A, %B %-d, %Y %I:%M %p").to_string(),
        "Tuesday, March 5, 2024 02:30 PM"
    );
    assert_eq!(t.format("%j|%U|%V|%u").to_string(), "065|09|10|2"); // ordinal, week-of-year (Sun), ISO week, Mon=1
    assert_eq!(t.format("%s").to_string(), "1709649015");
    assert_eq!(t.format("%.3f").to_string(), ".000");
    assert_eq!(t.to_rfc2822(), "Tue, 5 Mar 2024 14:30:15 +0000");

    // Comparison and ordering work on instants regardless of zone.
    let plus2 = FixedOffset::east_opt(2 * 3600).unwrap();
    let same = t.with_timezone(&plus2);
    assert_eq!(same, t);
    assert_eq!(same.to_rfc3339(), "2024-03-05T16:30:15+02:00");
    assert_eq!(same.hour(), 16);

    // Parsing.
    let p = DateTime::parse_from_rfc3339("2024-03-05T16:30:15+02:00").unwrap();
    assert_eq!(p, t);
    assert_eq!(p.offset().local_minus_utc(), 7200);
    let p2 = DateTime::parse_from_rfc2822("Tue, 5 Mar 2024 16:30:15 +0200").unwrap();
    assert_eq!(p2, t);
    let n = NaiveDateTime::parse_from_str("05/Mar/2024:14:30:15", "%d/%b/%Y:%H:%M:%S").unwrap();
    assert_eq!(n.and_utc(), t);
    // PITFALL: parse_from_str does not accept trailing text; and a format with %z gives a DateTime, not naive.
    assert!(
        NaiveDateTime::parse_from_str("05/Mar/2024:14:30:15 extra", "%d/%b/%Y:%H:%M:%S").is_err()
    );
    let with_off =
        DateTime::parse_from_str("2024-03-05 14:30:15 +0000", "%Y-%m-%d %H:%M:%S %z").unwrap();
    assert_eq!(with_off, t);
    // FromStr for DateTime<Utc> accepts several ISO-8601 shapes.
    let from_str: DateTime<Utc> = "2024-03-05T14:30:15Z".parse().unwrap();
    assert_eq!(from_str, t);
}

// ---------------------------------------------------------------------------------------------------------------
// 2. Durations (TimeDelta) and arithmetic
// ---------------------------------------------------------------------------------------------------------------
fn durations() {
    let a = Utc.with_ymd_and_hms(2024, 3, 5, 14, 0, 0).unwrap();
    let b = Utc.with_ymd_and_hms(2024, 3, 7, 9, 30, 0).unwrap();
    let delta: TimeDelta = b.signed_duration_since(a);
    assert_eq!(delta.num_hours(), 43);
    assert_eq!(delta.num_minutes(), 43 * 60 + 30);
    assert_eq!(delta.num_days(), 1); // truncates toward zero
    assert_eq!(a.signed_duration_since(b).num_days(), -1);
    assert_eq!(b - a, delta);
    assert_eq!(a + delta, b);
    assert_eq!(
        a + Duration::hours(1) + Duration::minutes(30),
        a + Duration::minutes(90)
    );
    // checked arithmetic where inputs are untrusted (huge user value) instead of panicking `+`.
    assert!(a.checked_add_signed(Duration::days(1)).is_some());
    assert!(TimeDelta::try_days(i64::MAX).is_none());
    assert!(a.checked_add_signed(TimeDelta::MAX).is_none());
    // Convert to std::time::Duration only if non-negative.
    assert!(delta.to_std().is_ok());
    assert!((-delta).to_std().is_err());
    // Truncating/rounding instants.
    let t = Utc.with_ymd_and_hms(2024, 3, 5, 14, 37, 59).unwrap();
    assert_eq!(t.duration_trunc(Duration::hours(1)).unwrap().hour(), 14);
    assert_eq!(
        t.duration_trunc(Duration::minutes(15)).unwrap().minute(),
        30
    );
    assert_eq!(t.duration_round(Duration::hours(1)).unwrap().hour(), 15);
    assert_eq!(t.date_naive(), d(2024, 3, 5));
    // Human readable: num_* helpers, not a built-in formatter.
    fn human(dt: TimeDelta) -> String {
        let s = dt.num_seconds();
        format!("{}h{:02}m{:02}s", s / 3600, s / 60 % 60, s % 60)
    }
    assert_eq!(human(delta), "43h30m00s");
}

// ---------------------------------------------------------------------------------------------------------------
// 3. Month-end, leap-year and "one month later" pitfalls
// ---------------------------------------------------------------------------------------------------------------
fn add_months_clamped(date: NaiveDate, n: u32) -> NaiveDate {
    date.checked_add_months(Months::new(n)).expect("in range")
}

fn month_end_pitfalls() {
    // chrono clamps to the last valid day instead of overflowing into the next month.
    assert_eq!(add_months_clamped(d(2024, 1, 31), 1), d(2024, 2, 29));
    assert_eq!(add_months_clamped(d(2023, 1, 31), 1), d(2023, 2, 28));
    assert_eq!(add_months_clamped(d(2024, 2, 29), 12), d(2025, 2, 28));
    // PITFALL: month addition is NOT associative: +1 month +1 month != +2 months from Jan 31.
    let two_steps = add_months_clamped(add_months_clamped(d(2023, 1, 31), 1), 1);
    let at_once = add_months_clamped(d(2023, 1, 31), 2);
    assert_eq!(two_steps, d(2023, 3, 28));
    assert_eq!(at_once, d(2023, 3, 31));
    // Fix for monthly billing: always add to the ANCHOR date, never to the previous result.
    let anchor = d(2023, 1, 31);
    let bills: Vec<NaiveDate> = (0..4).map(|k| add_months_clamped(anchor, k)).collect();
    assert_eq!(
        bills,
        vec![
            d(2023, 1, 31),
            d(2023, 2, 28),
            d(2023, 3, 31),
            d(2023, 4, 30)
        ]
    );
    // Subtracting months, days.
    assert_eq!(
        d(2024, 3, 31).checked_sub_months(Months::new(1)),
        Some(d(2024, 2, 29))
    );
    assert_eq!(
        d(2024, 3, 1).checked_sub_days(Days::new(1)),
        Some(d(2024, 2, 29))
    );
    // Last day of a month: first of next month minus one day.
    fn last_day(y: i32, m: u32) -> NaiveDate {
        d(y, m, 1)
            .checked_add_months(Months::new(1))
            .unwrap()
            .pred_opt()
            .unwrap()
    }
    assert_eq!(last_day(2024, 2).day(), 29);
    assert_eq!(last_day(2100, 2).day(), 28); // century rule: 2100 is not a leap year
    assert_eq!(last_day(2000, 2).day(), 29);
    assert_eq!(d(2024, 12, 31).succ_opt().unwrap(), d(2025, 1, 1));
    // with_day returns None for invalid combos.
    assert_eq!(d(2024, 1, 31).with_day(30), Some(d(2024, 1, 30)));
    assert_eq!(d(2024, 2, 1).with_day(30), None);
    assert!(NaiveDate::from_ymd_opt(2024, 1, 1).unwrap().leap_year());
}

// ---------------------------------------------------------------------------------------------------------------
// 4. Weekdays, ISO weeks, business days
// ---------------------------------------------------------------------------------------------------------------
fn is_business_day(date: NaiveDate, holidays: &HashSet<NaiveDate>) -> bool {
    !matches!(date.weekday(), Weekday::Sat | Weekday::Sun) && !holidays.contains(&date)
}

fn add_business_days(start: NaiveDate, mut n: u32, holidays: &HashSet<NaiveDate>) -> NaiveDate {
    let mut cur = start;
    while n > 0 {
        cur = cur.succ_opt().unwrap();
        if is_business_day(cur, holidays) {
            n -= 1;
        }
    }
    cur
}

fn business_days_between(a: NaiveDate, b: NaiveDate, holidays: &HashSet<NaiveDate>) -> usize {
    a.iter_days()
        .take_while(|x| *x < b)
        .filter(|x| is_business_day(*x, holidays))
        .count()
}

fn weekdays_and_business_days() {
    let fri = d(2024, 3, 1);
    assert_eq!(fri.weekday(), Weekday::Fri);
    assert_eq!(fri.weekday().succ(), Weekday::Sat);
    assert_eq!(Weekday::Mon.number_from_monday(), 1);
    assert_eq!(Weekday::Sun.num_days_from_sunday(), 0);
    assert_eq!("Wed".parse::<Weekday>().unwrap(), Weekday::Wed);
    let hol: HashSet<NaiveDate> = [d(2024, 3, 4)].into_iter().collect(); // a Monday holiday
    assert_eq!(add_business_days(fri, 1, &hol), d(2024, 3, 5));
    assert_eq!(add_business_days(fri, 5, &HashSet::new()), d(2024, 3, 8));
    assert_eq!(
        business_days_between(d(2024, 3, 1), d(2024, 3, 11), &HashSet::new()),
        6
    );
    assert_eq!(
        business_days_between(d(2024, 3, 1), d(2024, 3, 11), &hol),
        5
    );
    // ISO week: Dec 30 2024 belongs to ISO year 2025 week 1: never mix %Y with %V, use %G.
    let w: IsoWeek = d(2024, 12, 30).iso_week();
    assert_eq!((w.year(), w.week()), (2025, 1));
    assert_eq!(
        d(2024, 12, 30).format("%G-W%V-%u").to_string(),
        "2025-W01-1"
    );
    assert_eq!(d(2024, 12, 30).format("%Y-W%V").to_string(), "2024-W01"); // the classic bug
    // from_isoywd_opt builds a date from ISO year/week/weekday.
    assert_eq!(
        NaiveDate::from_isoywd_opt(2025, 1, Weekday::Mon),
        Some(d(2024, 12, 30))
    );
}

// ---------------------------------------------------------------------------------------------------------------
// 5. Age, anniversaries, recurring schedules
// ---------------------------------------------------------------------------------------------------------------
fn age_on(birth: NaiveDate, today: NaiveDate) -> u32 {
    // checked_years_since does the "has the birthday happened yet" logic (Feb 29 handled by chrono).
    today.years_since(birth).expect("birth <= today")
}

#[derive(Debug, Clone, Copy)]
enum Recurrence {
    EveryNDays(u64),
    MonthlyOnDay(u32),
    WeeklyOn(Weekday),
}

struct RecurringSchedule {
    rule: Recurrence,
    anchor: NaiveDate,
}

impl RecurringSchedule {
    fn occurrences(&self, count: usize) -> Vec<NaiveDate> {
        match self.rule {
            Recurrence::EveryNDays(n) => (0..count as u64)
                .map(|k| self.anchor.checked_add_days(Days::new(k * n)).unwrap())
                .collect(),
            Recurrence::MonthlyOnDay(day) => (0..count as u32)
                .map(|k| {
                    let first = self
                        .anchor
                        .with_day(1)
                        .unwrap()
                        .checked_add_months(Months::new(k))
                        .unwrap();
                    // clamp day-of-month to the length of that month
                    let len = first
                        .checked_add_months(Months::new(1))
                        .unwrap()
                        .pred_opt()
                        .unwrap()
                        .day();
                    first.with_day(day.min(len)).unwrap()
                })
                .collect(),
            Recurrence::WeeklyOn(wd) => {
                let mut cur = self.anchor;
                while cur.weekday() != wd {
                    cur = cur.succ_opt().unwrap();
                }
                (0..count as u64).map(|k| cur + Days::new(7 * k)).collect()
            }
        }
    }
}

fn age_and_schedules() {
    assert_eq!(age_on(d(1990, 6, 15), d(2024, 6, 14)), 33);
    assert_eq!(age_on(d(1990, 6, 15), d(2024, 6, 15)), 34);
    assert_eq!(age_on(d(2000, 2, 29), d(2023, 2, 28)), 22);
    assert_eq!(age_on(d(2000, 2, 29), d(2023, 3, 1)), 23);
    let monthly = RecurringSchedule {
        rule: Recurrence::MonthlyOnDay(31),
        anchor: d(2024, 1, 15),
    };
    assert_eq!(
        monthly.occurrences(4),
        vec![
            d(2024, 1, 31),
            d(2024, 2, 29),
            d(2024, 3, 31),
            d(2024, 4, 30)
        ]
    );
    let weekly = RecurringSchedule {
        rule: Recurrence::WeeklyOn(Weekday::Mon),
        anchor: d(2024, 3, 5),
    };
    let w = weekly.occurrences(3);
    assert_eq!(w, vec![d(2024, 3, 11), d(2024, 3, 18), d(2024, 3, 25)]);
    assert!(w.iter().all(|x| x.weekday() == Weekday::Mon));
    let every = RecurringSchedule {
        rule: Recurrence::EveryNDays(10),
        anchor: d(2024, 2, 25),
    };
    assert_eq!(every.occurrences(3)[2], d(2024, 3, 16));
}

// ---------------------------------------------------------------------------------------------------------------
// 6. Time zones: FixedOffset, real zones (chrono-tz), DST gaps/overlaps
// ---------------------------------------------------------------------------------------------------------------
fn zone_conversion() {
    let utc = Utc.with_ymd_and_hms(2024, 7, 1, 12, 0, 0).unwrap();
    let ny: Tz = "America/New_York".parse().unwrap();
    let tokyo: Tz = chrono_tz::Asia::Tokyo;
    assert_eq!(
        utc.with_timezone(&ny).to_rfc3339(),
        "2024-07-01T08:00:00-04:00"
    ); // EDT
    assert_eq!(utc.with_timezone(&tokyo).hour(), 21);
    let winter = Utc.with_ymd_and_hms(2024, 1, 1, 12, 0, 0).unwrap();
    assert_eq!(
        winter.with_timezone(&ny).to_rfc3339(),
        "2024-01-01T07:00:00-05:00"
    ); // EST
    assert_eq!("Not/AZone".parse::<Tz>().is_err(), true);
    // Day-boundary: "what date is it in Tokyo" differs from the UTC date.
    let late = Utc.with_ymd_and_hms(2024, 7, 1, 20, 0, 0).unwrap();
    assert_eq!(late.date_naive(), d(2024, 7, 1));
    assert_eq!(late.with_timezone(&tokyo).date_naive(), d(2024, 7, 2));
}

/// Resolve a local wall-clock time in a zone, with an explicit policy for gaps and overlaps.
fn resolve_local(tz: Tz, naive: NaiveDateTime) -> DateTime<Utc> {
    match tz.from_local_datetime(&naive) {
        LocalResult::Single(t) => t.with_timezone(&Utc),
        // Overlap (clocks go back): the wall time happens twice. Policy: take the earlier (first) one.
        LocalResult::Ambiguous(first, _second) => first.with_timezone(&Utc),
        // Gap (clocks go forward): the wall time never exists. Policy: shift forward by the gap (1h) -> run late, not skipped.
        LocalResult::None => {
            let shifted = naive + Duration::hours(1);
            tz.from_local_datetime(&shifted)
                .earliest()
                .expect("1h after a gap exists")
                .with_timezone(&Utc)
        }
    }
}

fn dst_gaps_and_overlaps() {
    let ny: Tz = chrono_tz::America::New_York;
    // US 2024: spring forward on Mar 10 (02:00 -> 03:00), fall back on Nov 3 (02:00 -> 01:00).
    let gap = d(2024, 3, 10).and_hms_opt(2, 30, 0).unwrap();
    assert_eq!(ny.from_local_datetime(&gap), LocalResult::None);
    let overlap = d(2024, 11, 3).and_hms_opt(1, 30, 0).unwrap();
    match ny.from_local_datetime(&overlap) {
        LocalResult::Ambiguous(a, b) => {
            assert_eq!(b.signed_duration_since(a), Duration::hours(1));
            assert_eq!(a.offset().to_string(), "EDT");
            assert_eq!(b.offset().to_string(), "EST");
        }
        other => panic!("expected ambiguous, got {other:?}"),
    }
    assert_eq!(
        resolve_local(ny, gap).to_rfc3339(),
        "2024-03-10T07:30:00+00:00"
    ); // 03:30 EDT = 07:30Z
    assert_eq!(
        resolve_local(ny, overlap).to_rfc3339(),
        "2024-11-03T05:30:00+00:00"
    ); // first 01:30 = EDT
    let normal = d(2024, 6, 1).and_hms_opt(9, 0, 0).unwrap();
    assert_eq!(
        resolve_local(ny, normal).to_rfc3339(),
        "2024-06-01T13:00:00+00:00"
    );
    // "+24h" vs "same wall clock tomorrow" differ across a DST change: pick deliberately.
    let before = ny.with_ymd_and_hms(2024, 3, 9, 12, 0, 0).unwrap();
    let plus_24h = before + Duration::hours(24);
    let same_wall = before.checked_add_days(Days::new(1)).unwrap();
    assert_eq!(plus_24h.hour(), 13); // 24 real hours later is 13:00 on the wall clock
    assert_eq!(same_wall.hour(), 12);
    assert_eq!(same_wall.signed_duration_since(before), Duration::hours(23));
    // `.earliest()` / `.latest()` / `.single()` are the one-liners for the common policies.
    assert!(ny.from_local_datetime(&gap).earliest().is_none());
    assert!(
        ny.from_local_datetime(&overlap).latest().unwrap()
            > ny.from_local_datetime(&overlap).earliest().unwrap()
    );
    assert!(ny.from_local_datetime(&overlap).single().is_none());
}

// ---------------------------------------------------------------------------------------------------------------
// 7. serde
// ---------------------------------------------------------------------------------------------------------------
#[derive(Serialize, Deserialize, Debug, PartialEq)]
struct Event {
    at: DateTime<Utc>,
    #[serde(with = "chrono::serde::ts_seconds")]
    epoch: DateTime<Utc>,
    day: NaiveDate,
    #[serde(default, with = "chrono::serde::ts_milliseconds_option")]
    maybe: Option<DateTime<Utc>>,
}

fn serde_roundtrip() {
    let t = Utc.with_ymd_and_hms(2024, 3, 5, 14, 30, 15).unwrap();
    let ev = Event {
        at: t,
        epoch: t,
        day: d(2024, 3, 5),
        maybe: None,
    };
    let json = serde_json::to_string(&ev).unwrap();
    assert_eq!(
        json,
        r#"{"at":"2024-03-05T14:30:15Z","epoch":1709649015,"day":"2024-03-05","maybe":null}"#
    );
    assert_eq!(serde_json::from_str::<Event>(&json).unwrap(), ev);
    // Offsets are normalised to UTC when deserialising into DateTime<Utc>.
    let back: Event =
        serde_json::from_str(r#"{"at":"2024-03-05T16:30:15+02:00","epoch":0,"day":"2024-03-05"}"#)
            .unwrap();
    assert_eq!(back.at, t);
    // Bad input produces a located error.
    let err = serde_json::from_str::<Event>(r#"{"at":"yesterday","epoch":0,"day":"2024-03-05"}"#)
        .unwrap_err();
    assert!(err.to_string().contains("invalid"), "{err}");
}

// ---------------------------------------------------------------------------------------------------------------
// 8. Production tips and tricks
// ---------------------------------------------------------------------------------------------------------------

/// A clock you can inject: production uses Utc::now, tests use a fixed instant.
trait Clock {
    fn now(&self) -> DateTime<Utc>;
}
struct SystemClock;
impl Clock for SystemClock {
    fn now(&self) -> DateTime<Utc> {
        Utc::now()
    }
}
struct FixedClock(DateTime<Utc>);
impl Clock for FixedClock {
    fn now(&self) -> DateTime<Utc> {
        self.0
    }
}
fn is_expired(clock: &dyn Clock, expires: DateTime<Utc>) -> bool {
    clock.now() >= expires
}

fn production_tips() {
    // 1. Store UTC (or epoch), convert to a zone only when rendering: the same instant must compare equal everywhere.
    let ny: Tz = chrono_tz::America::New_York;
    let stored = Utc.with_ymd_and_hms(2024, 11, 3, 5, 30, 0).unwrap();
    assert_eq!(stored.with_timezone(&ny), stored);
    // 2. Inject the clock (tip: no Utc::now() in business logic).
    let fixed = FixedClock(Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap());
    assert!(is_expired(&fixed, fixed.0));
    assert!(!is_expired(&fixed, fixed.0 + Duration::seconds(1)));
    let _ = SystemClock.now();
    // 3. Never use Local in logic or tests: its offset depends on the machine. Construct but do not assert.
    let _local = chrono::Local::now();
    // 4. Parse at the boundary into typed values; reject ambiguous input instead of guessing a zone.
    assert!(
        DateTime::parse_from_rfc3339("2024-03-05T10:00:00").is_err(),
        "no offset => not RFC 3339"
    );
    // 5. Use checked_* on untrusted values; plain + panics on overflow (clippy has no lint here, a fuzz test finds it).
    assert!(
        Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0)
            .unwrap()
            .checked_add_signed(TimeDelta::MAX)
            .is_none()
    );
    // 6. Compare instants, not strings: "+02:00" and "Z" strings of the same instant differ textually.
    let a = DateTime::parse_from_rfc3339("2024-03-05T16:30:15+02:00").unwrap();
    let b = DateTime::parse_from_rfc3339("2024-03-05T14:30:15Z").unwrap();
    assert_ne!(a.to_rfc3339(), b.to_rfc3339());
    assert_eq!(a, b);
    // 7. Measure durations with std::time::Instant (monotonic), not by subtracting wall-clock instants (NTP can step back).
    let start = std::time::Instant::now();
    assert!(start.elapsed() < std::time::Duration::from_secs(5));
    // 8. Use a half-open interval [start, end) for windows so adjacent windows do not double count.
    let day_start = d(2024, 3, 5).and_hms_opt(0, 0, 0).unwrap().and_utc();
    let day_end = day_start + Duration::days(1);
    let events = [day_start, day_end - Duration::nanoseconds(1), day_end];
    assert_eq!(
        events
            .iter()
            .filter(|e| **e >= day_start && **e < day_end)
            .count(),
        2
    );
    // 9. Store the IANA zone NAME (not the offset) for recurring local events ("every day 09:00 New York").
    let zone_name = "America/New_York";
    let tz: Tz = zone_name.parse().unwrap();
    let nine_am = |day: NaiveDate| resolve_local(tz, day.and_hms_opt(9, 0, 0).unwrap());
    assert_ne!(
        nine_am(d(2024, 3, 9)).hour(),
        nine_am(d(2024, 3, 11)).hour(),
        "UTC hour shifts with DST"
    );
    // 10. Prefer `DateTime::from_timestamp` (Option) over the removed/deprecated NaiveDateTime::from_timestamp.
    assert!(DateTime::from_timestamp(i64::MAX, 0).is_none());
    // 11. Truncate to the second before using as a cache/DB key to avoid sub-ms mismatches.
    let noisy =
        Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap() + Duration::nanoseconds(123_456_789);
    assert_eq!(
        noisy.with_nanosecond(0).unwrap().timestamp_subsec_nanos(),
        0
    );
    // 12. RFC 3339 with explicit precision for stable output: use to_rfc3339_opts.
    assert_eq!(
        noisy.to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
        "2024-01-01T00:00:00.123Z"
    );
    // 13. Naive types in public APIs hide the zone assumption: if you take NaiveDateTime, name the param `_utc`/`_local`.
    let ndt: NaiveDateTime = d(2024, 1, 1).and_time(NaiveTime::MIN);
    assert_eq!(ndt.and_utc().timestamp(), 1_704_067_200);
}

pub fn main() {
    everyday_core();
    durations();
    month_end_pitfalls();
    weekdays_and_business_days();
    age_and_schedules();
    zone_conversion();
    dst_gaps_and_overlaps();
    serde_roundtrip();
    production_tips();
    println!(
        "chrono: all sections passed; sample: {}",
        Utc.with_ymd_and_hms(2024, 3, 5, 14, 30, 15)
            .unwrap()
            .to_rfc3339()
    );
}
