//! RECIPE 1101 (crates): the `time` crate: Date/Time/OffsetDateTime, format descriptions, well-known formats, chrono comparison
//! LEVEL: intermediate
//! Use in: services that want a small, panic-free, no-implicit-local-time date library; embedded/`no_std` friendly code; strict RFC 3339 handling.   Run: cargo run --release -p cb-crate-tour --example time_crate_dates_formatting
//! COMMON USES: log/trace timestamps (tracing-subscriber uses `time`), JWT expiry, certificate validity windows, API payloads in RFC 3339,
//! compile-time-checked literals (`datetime!`) in tests and config defaults.
//! METHODS: time::Date::from_calendar_date, time::Date::weekday, time::Date::iso_week, time::Date::ordinal, time::Date::checked_add, time::Date::replace_day, time::Date::to_julian_day, time::Time::from_hms, time::Time::hour, time::PrimitiveDateTime::new, time::PrimitiveDateTime::assume_utc, time::PrimitiveDateTime::assume_offset, time::OffsetDateTime::now_utc, time::OffsetDateTime::from_unix_timestamp, time::OffsetDateTime::unix_timestamp, time::OffsetDateTime::to_offset, time::OffsetDateTime::parse, time::OffsetDateTime::format, time::OffsetDateTime::date, time::OffsetDateTime::replace_time, time::Duration::hours, time::Duration::whole_minutes, time::Duration::checked_add, time::Duration::saturating_add, time::UtcOffset::from_hms, time::Month::next, time::Month::try_from, time::Weekday::next, time::format_description::parse_borrowed, time::format_description::well_known::Rfc3339, time::format_description::well_known::Rfc2822, time::macros::date, time::macros::datetime, time::macros::format_description, time::format_description::parse_owned, time::format_description::well_known::Iso8601, time::serde::rfc3339 (and ::option), time::serde::timestamp, time::serde::timestamp::milliseconds, time::serde::format_description!, OffsetDateTime::{from_unix_timestamp_nanos, unix_timestamp_nanos, checked_to_offset}, Date::{next_occurrence, nth_next_occurrence, prev_occurrence, to_iso_week_date, monday_based_week, sunday_based_week}, Time::duration_until, UtcDateTime, OffsetDateTime::{parse_with_defaults, truncate_to_hour, truncate_to_day}, time::parsing::Parsed
//! DOCS: https://docs.rs/time/0.3/time/
//! CRATES: time = "0.3" (0.3.55 current; features formatting, parsing, macros, serde, serde-well-known, local-offset)
//! NOT COVERED (and why): `UtcOffset::current_local_offset` (unsound-adjacent on multithreaded Unix: shown only as a note), `quickcheck`/`rand` integrations, `Instant` (deprecated in favour of std).
//!
//! CHEAT SHEET (I want to ... -> use ...)
//!   a compile-time checked literal        -> date!(2024-03-05), time!(14:30), datetime!(2024-03-05 14:30 UTC)
//!   current instant                       -> OffsetDateTime::now_utc()
//!   parse RFC 3339                        -> OffsetDateTime::parse(s, &Rfc3339)
//!   format RFC 3339                       -> dt.format(&Rfc3339)?
//!   custom format (compile time)          -> format_description!("[year]-[month]-[day] [hour]:[minute]:[second]")
//!   custom format (from config/runtime)   -> format_description::parse_borrowed::<2>(&str)?  (returns Vec<FormatItem>)
//!   epoch seconds <-> instant             -> from_unix_timestamp(i64)? / unix_timestamp()
//!   change zone                           -> to_offset(UtcOffset) (same instant) vs replace_offset (reinterpret!)
//!   add days/hours                        -> dt + Duration::days(1) ; checked_add for untrusted; saturating_add for clamping
//!   month arithmetic                      -> NO built-in `add months`: use Month::next + replace_day, shown below
//!   weekday / ISO week                    -> date.weekday(), date.iso_week()
//!   continuous day number (Julian day)    -> date.to_julian_day()
//!   JSON field as RFC 3339 / epoch        -> #[serde(with = "time::serde::rfc3339")] / "time::serde::timestamp::milliseconds"
//!   epoch MILLISECONDS (JavaScript, Java) -> from_unix_timestamp_nanos(ms as i128 * 1_000_000)
//!   next Friday                           -> date.next_occurrence(Weekday::Friday)  (strictly after `date`)
//!
//! WHEN TO CHOOSE WHICH: chrono has `Months`, chrono-tz (real IANA zones), a bigger ecosystem and lax-ish ergonomics; `time` has no
//! built-in IANA database (offsets only; zones need an add-on such as `time-tz`), compile-time literals, fully checked constructors, and `no_std`.
//! `jiff` (BurntSushi) is the newer all-in-one with zones. For services that store UTC and display fixed offsets, both work.
//! MSRV/SOUNDNESS: `time` raises its MSRV quickly (track the Cargo.lock); `UtcOffset::current_local_offset` returns Err when other threads
//! exist on Unix (because `localtime_r` reads `TZ` unsafely), which is why `local-offset` is opt-in: determine the offset once at startup.
//!
//! Read: https://docs.rs/time ; https://time-rs.github.io/book/ (the time book) ; RUSTSEC-2020-0071 (the chrono/time localtime advisory, title only)
//! TRY NEXT:
//!   - [S] Parse "2024-02-30" with `Date::parse` and a `[year]-[month]-[day]` description: which error variant do you get?
//!   - [S] Predict `date!(2024-01-31)` + 1 month using `add_months` below, then change it to anchor-based addition.
//!   - [M] Write `fn parse_flexible(s) -> Result<OffsetDateTime>` trying Rfc3339, then Rfc2822, then a custom description, and test with three inputs.
//!   - [M] Translate chrono_dates_times_zones section 4 (business days) to `time` and diff the outputs.
//!   - [A] PROJECT: combine with `serde_attributes_enum_representations` (a `#[serde(with = ...)]` module that serialises OffsetDateTime as RFC 3339) and `tokio_runtime_tasks_channels_timers`.
#![allow(dead_code)]
use time::format_description::{
    self,
    well_known::{Rfc2822, Rfc3339},
};
use time::macros::{date, datetime, format_description as fd, time};
use time::{Date, Duration, Month, OffsetDateTime, PrimitiveDateTime, Time, UtcOffset, Weekday};

// ---------------------------------------------------------------------------------------------------------------
// 1. Everyday core
// ---------------------------------------------------------------------------------------------------------------
fn everyday_core() {
    // Macros validate at COMPILE time: `date!(2023-02-29)` would not build.
    let d = date!(2024 - 03 - 05);
    assert_eq!((d.year(), d.month(), d.day()), (2024, Month::March, 5));
    assert_eq!(d.weekday(), Weekday::Tuesday);
    assert_eq!(d.ordinal(), 65);
    assert_eq!(d.iso_week(), 10);
    assert_eq!(d.to_julian_day(), 2_460_375);
    // Runtime constructors return Result (never panic).
    assert!(Date::from_calendar_date(2023, Month::February, 29).is_err());
    assert!(Date::from_calendar_date(2024, Month::February, 29).is_ok());
    assert!(Date::from_ordinal_date(2024, 366).is_ok());
    assert!(Date::from_iso_week_date(2024, 53, Weekday::Monday).is_err());
    assert!(Time::from_hms(24, 0, 0).is_err());
    let t = Time::from_hms(14, 30, 15).unwrap();
    assert_eq!(t, time!(14:30:15));
    assert_eq!((t.hour(), t.minute(), t.second()), (14, 30, 15));
    // Three layers: PrimitiveDateTime (no offset), OffsetDateTime (instant + offset), UtcOffset.
    let p = PrimitiveDateTime::new(d, t);
    let utc = p.assume_utc();
    assert_eq!(utc, datetime!(2024-03-05 14:30:15 UTC));
    assert_eq!(utc.unix_timestamp(), 1_709_649_015);
    assert_eq!(
        OffsetDateTime::from_unix_timestamp(1_709_649_015).unwrap(),
        utc
    );
    assert!(OffsetDateTime::from_unix_timestamp(i64::MAX).is_err());
    let plus2 = UtcOffset::from_hms(2, 0, 0).unwrap();
    let local = utc.to_offset(plus2); // same instant, different wall clock
    assert_eq!(local, utc);
    assert_eq!(local.hour(), 16);
    assert_eq!(local.offset(), plus2);
    // PITFALL: `replace_offset`/assume_offset REINTERPRET the wall clock and change the instant.
    let reinterpret = p.assume_offset(plus2);
    assert_ne!(reinterpret, utc);
    assert_eq!(reinterpret.unix_timestamp(), utc.unix_timestamp() - 7200);
    assert_eq!(utc.date(), d);
    assert_eq!(
        utc.replace_time(time!(0:00)),
        datetime!(2024-03-05 0:00 UTC)
    );
    let _now = OffsetDateTime::now_utc(); // never assert on it
}

// ---------------------------------------------------------------------------------------------------------------
// 2. Duration arithmetic
// ---------------------------------------------------------------------------------------------------------------
fn durations() {
    let a = datetime!(2024-03-05 14:00 UTC);
    let b = datetime!(2024-03-07 09:30 UTC);
    let delta = b - a;
    assert_eq!(delta.whole_hours(), 43);
    assert_eq!(delta.whole_minutes(), 43 * 60 + 30);
    assert_eq!(delta.whole_days(), 1);
    assert_eq!(a + delta, b);
    assert_eq!(a + Duration::hours(1), datetime!(2024-03-05 15:00 UTC));
    assert_eq!(a - Duration::days(5), datetime!(2024-02-29 14:00 UTC));
    assert!(Duration::seconds(-5).is_negative());
    assert_eq!(Duration::seconds(-5).abs(), Duration::seconds(5));
    assert_eq!(Duration::seconds_f64(1.5).whole_milliseconds(), 1500);
    // Checked / saturating for untrusted inputs: plain + panics on overflow in debug (and in time it panics in release too).
    assert!(a.checked_add(Duration::MAX).is_none());
    assert_eq!(
        Duration::MAX.saturating_add(Duration::seconds(1)),
        Duration::MAX
    );
    assert!(
        date!(2024 - 03 - 05)
            .checked_add(Duration::days(i64::MAX / 100_000))
            .is_none()
    );
    // Conversion with std.
    let std: std::time::Duration = Duration::seconds(3).try_into().unwrap();
    assert_eq!(std.as_secs(), 3);
    assert!(std::time::Duration::try_from(Duration::seconds(-3)).is_err());
    // Sub-day truncation: replace_* methods return Result because the replacement may be invalid.
    let t = datetime!(2024-03-05 14:37:59.5 UTC);
    assert_eq!(
        t.replace_minute(0)
            .unwrap()
            .replace_second(0)
            .unwrap()
            .replace_nanosecond(0)
            .unwrap(),
        datetime!(2024-03-05 14:00 UTC)
    );
}

// ---------------------------------------------------------------------------------------------------------------
// 3. Month arithmetic (not built in) and weekday helpers
// ---------------------------------------------------------------------------------------------------------------
fn add_months(date: Date, n: u32) -> Date {
    let total = date.year() * 12 + (date.month() as i32 - 1) + n as i32;
    let (year, month0) = (total.div_euclid(12), total.rem_euclid(12) as u8);
    let month = Month::try_from(month0 + 1).unwrap();
    let day = date.day().min(time::util::days_in_month(month, year)); // clamp like chrono
    Date::from_calendar_date(year, month, day).unwrap()
}

fn months_and_weekdays() {
    assert_eq!(add_months(date!(2024 - 01 - 31), 1), date!(2024 - 02 - 29));
    assert_eq!(add_months(date!(2023 - 11 - 30), 3), date!(2024 - 02 - 29));
    assert_eq!(add_months(date!(2024 - 12 - 15), 1), date!(2025 - 01 - 15));
    assert_eq!(Month::December.next(), Month::January);
    assert_eq!(Month::January.previous(), Month::December);
    assert!(Month::try_from(13).is_err());
    assert_eq!(Weekday::Sunday.next(), Weekday::Monday);
    assert_eq!(Weekday::Monday.number_from_monday(), 1);
    assert_eq!(Weekday::Sunday.number_days_from_sunday(), 0);
    assert_eq!(
        date!(2024 - 03 - 05).next_day(),
        Some(date!(2024 - 03 - 06))
    );
    assert_eq!(
        date!(2024 - 03 - 01).previous_day(),
        Some(date!(2024 - 02 - 29))
    );
    assert_eq!(
        date!(2024 - 03 - 05).replace_day(31).unwrap(),
        date!(2024 - 03 - 31)
    );
    assert!(date!(2024 - 02 - 05).replace_day(30).is_err());
    assert_eq!(
        date!(2024 - 03 - 05).saturating_add(Duration::days(1)),
        date!(2024 - 03 - 06)
    );
    assert_eq!(time::util::is_leap_year(2100), false);
    assert_eq!(time::util::days_in_year(2024), 366);
}

// ---------------------------------------------------------------------------------------------------------------
// 4. Formatting and parsing: macros, runtime descriptions, well-known formats
// ---------------------------------------------------------------------------------------------------------------
fn formatting_and_parsing() {
    let dt = datetime!(2024-03-05 14:30:15.25 +2);
    // Well-known formats are consts: no allocation to build, zero config.
    assert_eq!(dt.format(&Rfc3339).unwrap(), "2024-03-05T14:30:15.25+02:00");
    assert_eq!(
        dt.format(&Rfc2822).unwrap(),
        "Tue, 05 Mar 2024 14:30:15 +0200"
    );
    // Compile-time format description.
    let f = fd!("[year]-[month]-[day] [hour]:[minute]:[second]");
    assert_eq!(dt.format(&f).unwrap(), "2024-03-05 14:30:15");
    let pretty = fd!(
        "[weekday repr:short] [day padding:none] [month repr:long] [year] [hour repr:12]:[minute] [period]"
    );
    assert_eq!(dt.format(&pretty).unwrap(), "Tue 5 March 2024 02:30 PM");
    let sub = fd!("[subsecond digits:3]");
    assert_eq!(dt.format(&sub).unwrap(), "250");
    let off = fd!("[offset_hour sign:mandatory]:[offset_minute]");
    assert_eq!(dt.format(&off).unwrap(), "+02:00");

    // Runtime description (from config/CLI): parse the description string, then reuse the compiled Vec<FormatItem>.
    let rt =
        format_description::parse_borrowed::<2>("[day]/[month repr:short]/[year] [hour]:[minute]")
            .unwrap();
    assert_eq!(dt.format(&rt).unwrap(), "05/Mar/2024 14:30");
    assert!(format_description::parse_borrowed::<2>("[nonsense]").is_err());
    // Parsing: PrimitiveDateTime::parse when the string has no offset; OffsetDateTime::parse needs one.
    let p = PrimitiveDateTime::parse("05/Mar/2024 14:30", &rt).unwrap();
    assert_eq!(p, datetime!(2024-03-05 14:30));
    assert!(
        OffsetDateTime::parse("05/Mar/2024 14:30", &rt).is_err(),
        "no offset component in the description"
    );
    assert_eq!(
        OffsetDateTime::parse("2024-03-05T14:30:15.25+02:00", &Rfc3339).unwrap(),
        dt
    );
    assert_eq!(
        OffsetDateTime::parse("2024-03-05T12:30:15.25Z", &Rfc3339).unwrap(),
        dt
    );
    // RFC 3339 section 5.6 NOTE allows a space instead of 'T'; `time` accepts it. Missing offset is still rejected:
    assert!(OffsetDateTime::parse("2024-03-05 12:30:15Z", &Rfc3339).is_ok());
    assert!(OffsetDateTime::parse("2024-03-05T12:30:15", &Rfc3339).is_err());
    // Date-only and time-only parsing.
    assert_eq!(
        Date::parse("2024-03-05", &fd!("[year]-[month]-[day]")).unwrap(),
        date!(2024 - 03 - 05)
    );
    assert!(Date::parse("2024-02-30", &fd!("[year]-[month]-[day]")).is_err());
    assert_eq!(
        Time::parse("23:59:59", &fd!("[hour]:[minute]:[second]")).unwrap(),
        time!(23:59:59)
    );
    // Parse error values are matchable (good for API error mapping).
    let err = Date::parse("2024-02-30", &fd!("[year]-[month]-[day]")).unwrap_err();
    assert!(
        matches!(
            err,
            time::error::Parse::ParseFromDescription(_) | time::error::Parse::TryFromParsed(_)
        ),
        "{err:?}"
    );
    // Display / Debug: Display is stable human form.
    assert_eq!(date!(2024 - 03 - 05).to_string(), "2024-03-05");
    assert_eq!(
        datetime!(2024-03-05 14:30 UTC).to_string(),
        "2024-03-05 14:30:00.0 +00:00:00"
    );
}

// ---------------------------------------------------------------------------------------------------------------
// 5. A small scenario: token expiry and a retention window, with an injected clock
// ---------------------------------------------------------------------------------------------------------------
struct Token {
    issued: OffsetDateTime,
    ttl: Duration,
}
impl Token {
    fn valid_at(&self, now: OffsetDateTime) -> bool {
        // saturating arithmetic so a huge ttl cannot panic
        now >= self.issued
            && now
                < self
                    .issued
                    .checked_add(self.ttl)
                    .unwrap_or(OffsetDateTime::new_utc(
                        date!(9999 - 12 - 31),
                        Time::MIDNIGHT,
                    ))
    }
}

fn expiry_scenario() {
    let tok = Token {
        issued: datetime!(2024-03-05 10:00 UTC),
        ttl: Duration::minutes(15),
    };
    assert!(tok.valid_at(datetime!(2024-03-05 10:00 UTC)));
    assert!(tok.valid_at(datetime!(2024-03-05 10:14:59 UTC)));
    assert!(
        !tok.valid_at(datetime!(2024-03-05 10:15 UTC)),
        "half-open interval"
    );
    assert!(
        !tok.valid_at(datetime!(2024-03-05 09:59:59 UTC)),
        "not valid before issue"
    );
    let forever = Token {
        issued: datetime!(2024-03-05 10:00 UTC),
        ttl: Duration::MAX,
    };
    assert!(forever.valid_at(datetime!(2030-01-01 0:00 UTC)));
    // Retention: delete rows older than 30 days (compare unix seconds when storing as INTEGER).
    let now = datetime!(2024-03-05 10:00 UTC);
    let cutoff = (now - Duration::days(30)).unix_timestamp();
    assert_eq!(cutoff, datetime!(2024-02-04 10:00 UTC).unix_timestamp());
}

// ---------------------------------------------------------------------------------------------------------------
// 6. Production tips and tricks
// ---------------------------------------------------------------------------------------------------------------
fn production_tips() {
    // 1. Use literals macros in tests/consts: typos fail at compile time (a bad literal is a build error, not a runtime panic).
    const EPOCH_2000: OffsetDateTime = datetime!(2000-01-01 0:00 UTC);
    assert_eq!(EPOCH_2000.unix_timestamp(), 946_684_800);
    // 2. Build FormatItem lists once (LazyLock) when the description comes from config; re-parsing per call is the slow way.
    static CFG_FORMAT: std::sync::LazyLock<Vec<format_description::BorrowedFormatItem<'static>>> =
        std::sync::LazyLock::new(|| {
            format_description::parse_borrowed::<2>("[year]/[month]/[day]").unwrap()
        });
    assert_eq!(
        date!(2024 - 03 - 05).format(&CFG_FORMAT).unwrap(),
        "2024/03/05"
    );
    // 3. Keep OffsetDateTime in UTC internally; to_offset only when rendering.
    let stored = datetime!(2024-03-05 23:30 UTC);
    let shown = stored.to_offset(UtcOffset::from_hms(5, 30, 0).unwrap());
    assert_eq!(shown.date(), date!(2024 - 03 - 06));
    assert_eq!(stored.date(), date!(2024 - 03 - 05));
    // 4. Offsets are not zones: an offset cannot know about DST. Store the IANA name next to the instant when you need future local times.
    assert_eq!(UtcOffset::from_hms(-5, 0, 0).unwrap().whole_hours(), -5);
    assert!(
        UtcOffset::from_hms(26, 0, 0).is_err(),
        "offset range is +-25:59:59"
    );
    // 5. Use checked_/saturating_ on user-supplied Duration (see section 2).
    assert!(
        datetime!(2024-03-05 0:00 UTC)
            .checked_sub(Duration::MAX)
            .is_none()
    );
    // 6. For a stable log/DB string use Rfc3339 on UTC, which sorts lexicographically if precision is constant.
    let mut v = [
        datetime!(2024-03-05 10:00 UTC),
        datetime!(2023-12-31 23:59:59 UTC),
        datetime!(2024-03-05 9:59:59 UTC),
    ];
    let mut s: Vec<String> = v.iter().map(|x| x.format(&Rfc3339).unwrap()).collect();
    v.sort();
    s.sort();
    let back: Vec<String> = v.iter().map(|x| x.format(&Rfc3339).unwrap()).collect();
    assert_eq!(s, back);
    // 7. The `local-offset` feature: call UtcOffset::current_local_offset() once at startup (single-threaded) and cache it. It returns Err when unsound.
    let _maybe = UtcOffset::current_local_offset(); // may be Ok or Err: do not assert
    // 8. Prefer std::time::Instant for elapsed time; `time` is for calendar instants.
    let i = std::time::Instant::now();
    assert!(i.elapsed().as_secs() < 5);
    // 9. Leap seconds: `time` cannot REPRESENT second 60. Parsing RFC 3339, it accepts :60 only where a leap second could be
    // (23:59:60 UTC at a month end; it does not consult a leap-second table) and clamps to the last nanosecond of :59.
    let parse = |s| OffsetDateTime::parse(s, &Rfc3339);
    assert_eq!(
        parse("2016-12-31T23:59:60Z").unwrap(),
        datetime!(2016-12-31 23:59:59.999999999 UTC)
    );
    assert_eq!(
        parse("2024-03-31T23:59:60Z").unwrap(),
        datetime!(2024-03-31 23:59:59.999999999 UTC),
        "no real leap second then"
    );
    assert!(parse("2024-03-05T23:59:60Z").is_err(), "not a month end");
    assert!(parse("2016-12-31T12:00:60Z").is_err(), "not 23:59 UTC");
    // 10. Differences vs chrono worth remembering in review: time::Date year range is +-9999 by default; Month/Weekday are enums, not numbers.
    assert_eq!(Date::MAX.year(), 9999);
    assert_eq!(u8::from(Month::March), 3);
}

// ---------------------------------------------------------------------------------------------------------------
// 7. Arithmetic traps and calendar helpers
// ---------------------------------------------------------------------------------------------------------------
fn arithmetic_traps() {
    // PITFALL: Date + Duration uses only the WHOLE days of the duration. The remaining hours are dropped, not rounded,
    // and nothing accumulates across calls.
    let d = date!(2024 - 03 - 05);
    assert_eq!(d + Duration::hours(47), date!(2024 - 03 - 06));
    assert_eq!(d + Duration::hours(23) + Duration::hours(23), d);
    assert_eq!(d - Duration::hours(23), d);
    assert_eq!(
        (date!(2024 - 03 - 05) - date!(2023 - 03 - 05)).whole_days(),
        366
    );
    // PITFALL: Time is a clock face. Addition wraps at midnight with no carry anywhere.
    assert_eq!(time!(23:00) + Duration::hours(2), time!(1:00));
    assert_eq!(
        time!(23:00).duration_until(time!(1:00)),
        Duration::hours(2),
        "forward around the clock"
    );
    assert_eq!(
        time!(1:00) - time!(23:00),
        Duration::hours(-22),
        "plain subtraction assumes the same day"
    );
    // If dates matter, do the arithmetic on a date-time.
    assert_eq!(
        datetime!(2024-03-05 23:00) + Duration::hours(2),
        datetime!(2024-03-06 1:00)
    );

    // to_offset panics when the shifted wall clock leaves the supported range; checked_to_offset reports it.
    let plus1 = UtcOffset::from_hms(1, 0, 0).unwrap();
    let last = PrimitiveDateTime::MAX.assume_utc();
    assert!(last.checked_to_offset(plus1).is_none());

    // "Next Friday" is strictly after the starting day, even if that day is a Friday.
    let friday = date!(2024 - 03 - 08);
    assert_eq!(d.next_occurrence(Weekday::Friday), friday);
    assert_eq!(
        friday.next_occurrence(Weekday::Friday),
        date!(2024 - 03 - 15)
    );
    assert_eq!(
        friday.prev_occurrence(Weekday::Friday),
        date!(2024 - 03 - 01)
    );
    // Scenario: US Thanksgiving is the fourth Thursday of November.
    let thanksgiving = |year| {
        let before = Date::from_calendar_date(year, Month::November, 1)
            .unwrap()
            .previous_day()
            .unwrap();
        before.nth_next_occurrence(Weekday::Thursday, 4)
    };
    assert_eq!(
        (thanksgiving(2024), thanksgiving(2026)),
        (date!(2024 - 11 - 28), date!(2026 - 11 - 26))
    );

    // Three different "week numbers". ISO weeks have their own YEAR: never pair iso_week() with year().
    let edge = date!(2024 - 12 - 30);
    assert_eq!(edge.to_iso_week_date(), (2025, 1, Weekday::Monday));
    assert_eq!(
        (edge.year(), edge.iso_week()),
        (2024, 1),
        "formatting this pair would be a year off"
    );
    assert_eq!(
        (edge.monday_based_week(), edge.sunday_based_week()),
        (53, 52)
    );
}

// ---------------------------------------------------------------------------------------------------------------
// 8. Wire formats: epoch units, ISO 8601, strict custom parsing and serde
// ---------------------------------------------------------------------------------------------------------------
#[derive(Debug, PartialEq, serde::Serialize, serde::Deserialize)]
struct ApiEvent {
    #[serde(with = "time::serde::rfc3339")]
    at: OffsetDateTime,
    /// Option needs the `::option` variant of the module, plus `default` if the key may be absent.
    #[serde(default, with = "time::serde::rfc3339::option")]
    ended: Option<OffsetDateTime>,
    #[serde(with = "time::serde::timestamp")]
    created_s: OffsetDateTime,
    #[serde(with = "time::serde::timestamp::milliseconds")]
    seen_ms: OffsetDateTime,
    #[serde(with = "birthday_format")]
    birthday: Date,
}
time::serde::format_description!(birthday_format, Date, "[day].[month].[year]");

fn wire_formats() {
    // PITFALL: milliseconds passed as seconds. `time` rejects the absurd year instead of returning a date in year 56000.
    let millis: i64 = 1_709_649_015_250;
    assert!(OffsetDateTime::from_unix_timestamp(millis).is_err());
    let t = OffsetDateTime::from_unix_timestamp_nanos(millis as i128 * 1_000_000).unwrap();
    assert_eq!(t, datetime!(2024-03-05 14:30:15.25 UTC));
    assert_eq!(
        (t.unix_timestamp(), t.unix_timestamp_nanos() / 1_000_000),
        (1_709_649_015, millis as i128)
    );
    // Before 1970 the timestamp is negative; whole seconds are floored toward the past.
    let before = datetime!(1969-12-31 23:59:59.5 UTC);
    assert_eq!(
        (before.unix_timestamp(), before.unix_timestamp_nanos()),
        (-1, -500_000_000)
    );

    use time::format_description::well_known::Iso8601;
    let dt = datetime!(2024-03-05 14:30:15.25 +2);
    let iso = dt.format(&Iso8601::DEFAULT).unwrap();
    assert_eq!(iso, "2024-03-05T14:30:15.250000000+02:00");
    assert_eq!(OffsetDateTime::parse(&iso, &Iso8601::DEFAULT).unwrap(), dt);
    // ISO 8601 allows forms that RFC 3339 does not, such as the "basic" format without separators.
    assert_eq!(
        OffsetDateTime::parse("20240305T143015+0200", &Iso8601::PARSING).unwrap(),
        datetime!(2024-03-05 14:30:15 +2)
    );
    assert!(OffsetDateTime::parse("20240305T143015+0200", &Rfc3339).is_err());

    // Custom descriptions are strict about padding, and must consume the whole input.
    let strict = fd!("[year]-[month]-[day]");
    assert!(Date::parse("2024-3-5", &strict).is_err());
    assert_eq!(
        Date::parse(
            "2024-3-5",
            &fd!("[year]-[month padding:none]-[day padding:none]")
        )
        .unwrap(),
        date!(2024 - 03 - 05)
    );
    assert!(Date::parse("2024-03-05 trailing", &strict).is_err());
    // A description loaded from configuration must outlive the config string: parse_owned.
    let owned = {
        let from_config = String::from("[year]/[ordinal]");
        format_description::parse_owned::<2>(&from_config).unwrap()
    };
    assert_eq!(date!(2024 - 03 - 05).format(&owned).unwrap(), "2024/065");
    // Formatting a component the value does not have is a RUNTIME error.
    assert!(
        date!(2024 - 03 - 05)
            .format(&fd!("[hour]:[minute]"))
            .is_err()
    );

    let event = ApiEvent {
        at: dt,
        ended: None,
        created_s: datetime!(2024-03-05 0:00 UTC),
        seen_ms: t,
        birthday: date!(1990 - 07 - 04),
    };
    let json = serde_json::to_string(&event).unwrap();
    assert_eq!(
        json,
        r#"{"at":"2024-03-05T14:30:15.25+02:00","ended":null,"created_s":1709596800,"seen_ms":1709649015250,"birthday":"04.07.1990"}"#
    );
    assert_eq!(serde_json::from_str::<ApiEvent>(&json).unwrap(), event);
    assert!(
        serde_json::from_str::<ApiEvent>(&json.replace(r#""ended":null,"#, ""))
            .unwrap()
            .ended
            .is_none()
    );
    assert!(serde_json::from_str::<ApiEvent>(&json.replace("04.07.1990", "31.02.1990")).is_err());
    // An epoch timestamp forgets the offset: it comes back as UTC (same instant, different wall clock).
    let mut shifted = event;
    shifted.created_s = shifted
        .created_s
        .to_offset(UtcOffset::from_hms(5, 0, 0).unwrap());
    let back: ApiEvent = serde_json::from_str(&serde_json::to_string(&shifted).unwrap()).unwrap();
    assert_eq!(back.created_s, shifted.created_s);
    assert_eq!(
        (
            back.created_s.offset(),
            shifted.created_s.hour(),
            back.created_s.hour()
        ),
        (UtcOffset::UTC, 5, 0)
    );

    // UtcDateTime states "always UTC" in the type, so no offset can sneak in.
    let u = time::UtcDateTime::from_unix_timestamp(1_709_649_015).unwrap();
    assert_eq!(
        u.to_offset(UtcOffset::UTC),
        datetime!(2024-03-05 14:30:15 UTC)
    );
}

/// Scenario: classic syslog lines carry no year and no offset. Supply them as defaults instead of editing the string.
fn partial_inputs_and_truncation() {
    use time::parsing::Parsed;
    let syslog = fd!("[month repr:short] [day padding:space] [hour]:[minute]:[second]");
    let parse = |line: &str, year: i32| {
        let defaults = Parsed::new()
            .with_year(year)
            .unwrap()
            .with_offset_hour(0)
            .unwrap();
        OffsetDateTime::parse_with_defaults(line.as_bytes(), &syslog, defaults)
    };
    assert_eq!(
        parse("Mar  5 14:30:15", 2024).unwrap(),
        datetime!(2024-03-05 14:30:15 UTC)
    );
    assert_eq!(
        parse("Feb 29 00:00:00", 2024).unwrap(),
        datetime!(2024-02-29 0:00 UTC)
    );
    assert!(
        parse("Feb 29 00:00:00", 2023).is_err(),
        "the combination is validated, defaults included"
    );
    assert!(
        OffsetDateTime::parse("Mar  5 14:30:15", &syslog).is_err(),
        "without defaults the data is incomplete"
    );

    // Bucketing for reports: truncate_to_* keeps the offset and zeroes everything below the named unit.
    let t = datetime!(2024-03-05 14:37:59.5 +5:30);
    assert_eq!(t.truncate_to_hour(), datetime!(2024-03-05 14:00 +5:30));
    assert_eq!(t.truncate_to_day(), datetime!(2024-03-05 0:00 +5:30));
    // PITFALL: truncating in a local offset and in UTC gives DIFFERENT buckets. Choose the offset first.
    assert_ne!(
        t.truncate_to_day(),
        t.to_offset(UtcOffset::UTC).truncate_to_day()
    );
    assert_ne!(
        t.truncate_to_hour(),
        t.to_offset(UtcOffset::UTC).truncate_to_hour(),
        "even hours, with a :30 offset"
    );
}

pub fn main() {
    everyday_core();
    durations();
    months_and_weekdays();
    formatting_and_parsing();
    expiry_scenario();
    production_tips();
    arithmetic_traps();
    wire_formats();
    partial_inputs_and_truncation();
    println!("time crate: all sections passed");
}
