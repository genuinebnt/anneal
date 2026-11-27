//! RECIPE 1208 (database): keeping the evidence behind each answer: observations, coverage, corrections and as-of questions.
//! LEVEL: intermediate
//! Use in: stock and shelf checks, sensor readings, data merged from several sources.   Run: cargo run --release --example evidence_log
//! COMMON USES: inventory, asset registers, reports that must be explained or reproduced later.
#![allow(dead_code)]
//!
//! The log holds what each source reported and is only ever added to. An answer such as "on shelf F2" is worked out
//! from it each time, together with the numbers of the records behind it. Every record has two times: when it was
//! seen and when the log learned of it. A sweep says which shelf it covered and whether it finished, so only a
//! finished sweep of the right shelf can count as "looked and did not find". A retraction cancels a record from the
//! moment the retraction arrives; questions about earlier knowledge still get the earlier answer.
//! Each record is recorded later than the one before (whole hours, so one record per hour here). An answer "as known
//! at K" is fixed once the last record is at K or later, and provisional until then. A retraction cannot be retracted: if it was wrong, add the observation again. A finished sweep that
//! misses the copy in the same hour as a sighting is not counted against it, and the answer does not mention it.
//! NOT COVERED: wrong or drifting clocks, how much to trust each source, indexes (every question reads the whole log).
//! Read: https://www.w3.org/TR/prov-overview/

use std::collections::{BTreeMap, BTreeSet};

/// Hours since the start of the example.
type Time = u32;

fn at(day: u32, hour: u32) -> Time {
    day * 24 + hour
}

#[derive(Debug, Clone, PartialEq)]
enum Body {
    /// One copy seen in one place.
    Sighting {
        copy: String,
        shelf: String,
        seen: Time,
    },
    /// Someone read one shelf. `finished` is false if they did not get to the end.
    Sweep {
        shelf: String,
        seen: Time,
        found: BTreeSet<String>,
        finished: bool,
    },
    /// An earlier record was wrong.
    Retraction { target: usize, why: String },
}

#[derive(Debug, Clone)]
struct Record {
    source: String,
    recorded: Time,
    body: Body,
}

#[derive(Debug, PartialEq)]
enum Refused {
    NotRecordedAfterTheLastRecord,
    SeenAfterItWasRecorded,
    NothingToRetract,
}

/// A record's number is its position, and it never changes.
#[derive(Default, Clone)]
struct Log {
    records: Vec<Record>,
}

impl Log {
    fn add(&mut self, source: &str, recorded: Time, body: Body) -> Result<usize, Refused> {
        if self
            .records
            .last()
            .is_some_and(|last| recorded <= last.recorded)
        {
            return Err(Refused::NotRecordedAfterTheLastRecord);
        }
        match &body {
            Body::Sighting { seen, .. } | Body::Sweep { seen, .. } if *seen > recorded => {
                return Err(Refused::SeenAfterItWasRecorded);
            }
            Body::Retraction { target, .. } => match self.records.get(*target).map(|r| &r.body) {
                Some(Body::Sighting { .. } | Body::Sweep { .. }) => {}
                _ => return Err(Refused::NothingToRetract),
            },
            _ => {}
        }
        self.records.push(Record {
            source: source.into(),
            recorded,
            body,
        });
        Ok(self.records.len() - 1)
    }

    /// The observations that count when we use only what the log held at `known`.
    fn usable(&self, known: Time) -> Vec<(usize, &Record)> {
        let held = || {
            self.records
                .iter()
                .enumerate()
                .filter(move |(_, r)| r.recorded <= known)
        };
        let retracted: BTreeSet<usize> = held()
            .filter_map(|(_, r)| match r.body {
                Body::Retraction { target, .. } => Some(target),
                _ => None,
            })
            .collect();
        held()
            .filter(|(id, r)| !matches!(r.body, Body::Retraction { .. }) && !retracted.contains(id))
            .collect()
    }

    /// What can we say about where `copy` was at `when`, using only what the log held at `known`?
    fn ask(&self, copy: &str, when: Time, known: Time) -> Answer {
        let usable = self.usable(known);

        // every report of seeing the copy, whether it came from a sighting or a sweep
        let sightings: Vec<(Time, &str, usize)> = usable
            .iter()
            .filter_map(|&(id, r)| match &r.body {
                Body::Sighting {
                    copy: c,
                    shelf,
                    seen,
                } if c == copy => Some((*seen, shelf.as_str(), id)),
                Body::Sweep {
                    shelf, seen, found, ..
                } if found.contains(copy) => Some((*seen, shelf.as_str(), id)),
                _ => None,
            })
            .filter(|s| s.0 <= when)
            .collect();
        let Some(last) = sightings.iter().map(|s| s.0).max() else {
            return Answer {
                state: State::NoSighting,
                support: vec![],
                against: vec![],
            };
        };
        let newest: Vec<_> = sightings.iter().filter(|s| s.0 == last).collect();
        let shelves: BTreeSet<String> = newest.iter().map(|s| s.1.to_string()).collect();
        let support: Vec<usize> = newest.iter().map(|s| s.2).collect();
        if shelves.len() > 1 {
            return Answer {
                state: State::Conflict {
                    shelves,
                    seen: last,
                },
                support,
                against: vec![],
            };
        }
        let shelf = shelves.into_iter().next().unwrap_or_default();

        // "looked and did not find": only a finished, later sweep of that very shelf
        let against: Vec<usize> = usable
            .iter()
            .filter(|(_, r)| match &r.body {
                Body::Sweep {
                    shelf: s,
                    seen,
                    found,
                    finished,
                } => {
                    *finished
                        && *s == shelf
                        && *seen > last
                        && *seen <= when
                        && !found.contains(copy)
                }
                _ => false,
            })
            .map(|&(id, _)| id)
            .collect();
        let state = if against.is_empty() {
            State::LastSeen { shelf, seen: last }
        } else {
            State::NotFoundSince {
                shelf,
                last_seen: last,
            }
        };
        Answer {
            state,
            support,
            against,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
enum State {
    /// The log has no sighting. This says nothing about whether the copy exists.
    NoSighting,
    LastSeen {
        shelf: String,
        seen: Time,
    },
    /// Looked for where it was last seen, and not found. This is not proof that it is gone.
    NotFoundSince {
        shelf: String,
        last_seen: Time,
    },
    Conflict {
        shelves: BTreeSet<String>,
        seen: Time,
    },
}

/// A conclusion, with the numbers of the records for and against it.
#[derive(Debug, PartialEq)]
struct Answer {
    state: State,
    support: Vec<usize>,
    against: Vec<usize>,
}

// --- checks --------------------------------------------------------------------------------------

/// A second way to reach the same answer: walk through time and update a belief.
fn replay(log: &Log, copy: &str, when: Time, known: Time) -> Answer {
    let mut by_time: BTreeMap<Time, Vec<(usize, &Body)>> = BTreeMap::new();
    for (id, r) in log.usable(known) {
        if let Body::Sighting { seen, .. } | Body::Sweep { seen, .. } = r.body {
            by_time.entry(seen).or_default().push((id, &r.body));
        }
    }
    let mut belief = Answer {
        state: State::NoSighting,
        support: vec![],
        against: vec![],
    };
    for (&time, bodies) in by_time.range(..=when) {
        let here: Vec<(usize, &String)> = bodies
            .iter()
            .filter_map(|&(id, b)| match b {
                Body::Sighting { copy: c, shelf, .. } if c == copy => Some((id, shelf)),
                Body::Sweep { shelf, found, .. } if found.contains(copy) => Some((id, shelf)),
                _ => None,
            })
            .collect();
        if !here.is_empty() {
            // new sightings replace everything believed before
            let shelves: BTreeSet<String> = here.iter().map(|h| h.1.clone()).collect();
            belief.support = here.iter().map(|h| h.0).collect();
            belief.against.clear();
            belief.state = if shelves.len() > 1 {
                State::Conflict {
                    shelves,
                    seen: time,
                }
            } else {
                State::LastSeen {
                    shelf: here[0].1.clone(),
                    seen: time,
                }
            };
            continue;
        }
        let (shelf, last_seen) = match &belief.state {
            State::LastSeen { shelf, seen } => (shelf.clone(), *seen),
            State::NotFoundSince { shelf, last_seen } => (shelf.clone(), *last_seen),
            _ => continue,
        };
        let misses = bodies.iter().filter(
            |(_, b)| matches!(b, Body::Sweep { shelf: s, finished: true, .. } if *s == shelf),
        );
        belief.against.extend(misses.map(|m| m.0));
        if !belief.against.is_empty() {
            belief.state = State::NotFoundSince { shelf, last_seen };
        }
    }
    belief.support.sort();
    belief.against.sort();
    belief
}

struct Rng(u64);
impl Rng {
    fn below(&mut self, n: u32) -> u32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 % n as u64) as u32
    }
}

const SHELVES: [&str; 3] = ["F1", "F2", "F3"];

fn random_log(rng: &mut Rng) -> Log {
    let mut log = Log::default();
    let mut now = 0;
    for _ in 0..rng.below(16) {
        now += rng.below(3);
        let seen = now - rng.below(5).min(now);
        let shelf = SHELVES[rng.below(3) as usize].to_string();
        let body = match rng.below(5) {
            0 => Body::Sighting {
                copy: "A".into(),
                shelf,
                seen,
            },
            1 if !log.records.is_empty() => Body::Retraction {
                target: rng.below(log.records.len() as u32) as usize,
                why: "test".into(),
            },
            _ => {
                let found = if rng.below(3) == 0 {
                    BTreeSet::from(["A".to_string()])
                } else {
                    BTreeSet::new()
                };
                Body::Sweep {
                    shelf,
                    seen,
                    found,
                    finished: rng.below(2) == 0,
                }
            }
        };
        // retracting a retraction is refused; the log is unchanged then
        let _ = log.add("random", now, body);
    }
    log
}

/// The same log with some observations blanked out: each becomes an unfinished sweep of an unused shelf.
fn without(log: &Log, drop: impl Fn(usize, &Body) -> bool) -> Log {
    let mut out = log.clone();
    for (id, r) in out.records.iter_mut().enumerate() {
        if let Body::Sighting { seen, .. } | Body::Sweep { seen, .. } = r.body
            && drop(id, &r.body)
        {
            r.body = Body::Sweep {
                shelf: "unused".into(),
                seen,
                found: BTreeSet::new(),
                finished: false,
            };
        }
    }
    out
}

fn check_random_logs() {
    let mut rng = Rng(0xFACE);
    for _ in 0..20_000 {
        let log = random_log(&mut rng);
        let (when, known) = (rng.below(30), rng.below(30));
        let answer = log.ask("A", when, known);
        assert_eq!(answer, replay(&log, "A", when, known));

        // the past stays put: records that arrive after `known` change nothing
        let mut held = log.clone();
        held.records.retain(|r| r.recorded <= known);
        assert_eq!(held.ask("A", when, known), answer);

        // an unfinished sweep that did not find the copy is no evidence
        let found_a = |b: &Body| matches!(b, Body::Sweep { found, .. } if found.contains("A"));
        let silent = without(&log, |_, b| {
            matches!(
                b,
                Body::Sweep {
                    finished: false,
                    ..
                }
            ) && !found_a(b)
        });
        assert_eq!(silent.ask("A", when, known), answer);

        // neither is a sweep of some other shelf
        if let State::LastSeen { shelf, .. } | State::NotFoundSince { shelf, .. } = &answer.state {
            let elsewhere = without(&log, |_, b| {
                matches!(b, Body::Sweep { shelf: s, .. } if s != shelf) && !found_a(b)
            });
            assert_eq!(elsewhere.ask("A", when, known), answer);
        }

        // once a retraction is known, its target might as well not be there
        let retracted: BTreeSet<usize> = log
            .records
            .iter()
            .filter(|r| r.recorded <= known)
            .filter_map(|r| match r.body {
                Body::Retraction { target, .. } => Some(target),
                _ => None,
            })
            .collect();
        assert_eq!(
            without(&log, |id, _| retracted.contains(&id)).ask("A", when, known),
            answer
        );
        for id in answer.support.iter().chain(&answer.against) {
            assert!(!retracted.contains(id) && log.records[*id].recorded <= known);
        }
    }
}

fn main() {
    let sweep = |shelf: &str, seen, found: &[&str], finished| Body::Sweep {
        shelf: shelf.into(),
        seen,
        found: found.iter().map(|s| s.to_string()).collect(),
        finished,
    };
    let sighting = |copy: &str, shelf: &str, seen| Body::Sighting {
        copy: copy.into(),
        shelf: shelf.into(),
        seen,
    };

    // Copy C17 of one library book
    let mut log = Log::default();
    let steps = [
        (
            "scanner A",
            at(1, 10),
            sweep("F2", at(1, 9), &["C17", "C20"], true),
        ),
        (
            "scanner A",
            at(2, 10),
            sweep("F2", at(2, 9), &["C20"], false),
        ), // battery died half-way
        ("scanner B", at(3, 10), sweep("F3", at(3, 9), &[], true)),
        (
            "scanner A",
            at(4, 10),
            sweep("F2", at(4, 9), &["C20"], true),
        ),
    ];
    for (source, recorded, body) in steps {
        log.add(source, recorded, body).expect("accepted");
        let a = log.ask("C17", recorded, recorded);
        println!(
            "day {}: {:?} for={:?} against={:?}",
            recorded / 24,
            a.state,
            a.support,
            a.against
        );
    }
    assert_eq!(
        log.ask("C17", at(3, 12), at(3, 12)).state,
        State::LastSeen {
            shelf: "F2".into(),
            seen: at(1, 9)
        }
    );
    assert!(matches!(
        log.ask("C17", at(4, 12), at(4, 12)).state,
        State::NotFoundSince { .. }
    ));

    // on day 6 a desk uploads a note it made on day 5
    log.add("desk", at(6, 8), sighting("C17", "returns cart", at(5, 15)))
        .expect("accepted");
    let then = log.ask("C17", at(5, 18), at(5, 18));
    let now = log.ask("C17", at(5, 18), at(6, 9));
    println!("day 5 18:00, as known then: {:?}", then.state);
    println!("day 5 18:00, as known on day 6: {:?}", now.state);
    assert!(matches!(then.state, State::NotFoundSince { .. }));
    assert!(matches!(now.state, State::LastSeen { .. }));

    // on day 7 it turns out that the day 4 sweep read the wrong shelf
    log.add(
        "supervisor",
        at(7, 9),
        Body::Retraction {
            target: 3,
            why: "read shelf F1 under an F2 label".into(),
        },
    )
    .expect("accepted");
    let before = log.ask("C17", at(4, 12), at(6, 9));
    let after = log.ask("C17", at(4, 12), at(7, 10));
    println!("day 4, before the retraction: {:?}", before.state);
    println!("day 4, after it: {:?}", after.state);
    assert!(matches!(before.state, State::NotFoundSince { .. }));
    assert!(matches!(after.state, State::LastSeen { .. }));

    // two sources, one hour, two places
    log.add("desk", at(8, 12), sighting("C17", "loan desk", at(8, 11)))
        .expect("accepted");
    log.add(
        "scanner B",
        at(8, 13),
        sweep("F3", at(8, 11), &["C17"], true),
    )
    .expect("accepted");
    let torn = log.ask("C17", at(8, 13), at(8, 13));
    println!("day 8: {:?} for={:?}", torn.state, torn.support);
    assert!(matches!(torn.state, State::Conflict { .. }));
    assert_eq!(
        log.ask("C99", at(8, 13), at(8, 13)).state,
        State::NoSighting
    );

    // an answer as of the last record's hour stays put: nothing more can be filed under that hour
    let fixed = log.ask("C17", at(8, 13), at(8, 13));
    assert_eq!(
        log.add("late", at(8, 13), sighting("C17", "F1", at(8, 13))),
        Err(Refused::NotRecordedAfterTheLastRecord)
    );
    assert_eq!(log.ask("C17", at(8, 13), at(8, 13)), fixed);

    let size = log.records.len();
    assert_eq!(
        log.add("x", at(1, 0), sighting("C17", "F1", at(1, 0))),
        Err(Refused::NotRecordedAfterTheLastRecord)
    );
    assert_eq!(
        log.add("x", at(9, 0), sighting("C17", "F1", at(9, 1))),
        Err(Refused::SeenAfterItWasRecorded)
    );
    // 5 is the retraction itself; the other two do not exist
    for target in [5, size, usize::MAX] {
        assert_eq!(
            log.add(
                "x",
                at(9, 0),
                Body::Retraction {
                    target,
                    why: String::new()
                }
            ),
            Err(Refused::NothingToRetract)
        );
    }
    assert_eq!(log.records.len(), size);

    check_random_logs();
    println!("all checks passed");
}
