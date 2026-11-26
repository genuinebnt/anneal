//! RECIPE 1107 (crates): uuid and rand: random, time-ordered and name-based ids; seeded simulation; distributions; secret tokens
//! LEVEL: intermediate
//! Use in: database keys, request ids, idempotency keys, simulations and tests, session tokens.   Run: cargo run --release -p cb-crate-tour --example uuid_rand_ids_tokens
//! COMMON USES: v7 primary keys (index-friendly), v5 for deterministic ids from natural keys, seeded RNG for reproducible
//! tests and load generators, OS randomness for anything an attacker must not guess.
//! METHODS: Uuid::new_v4, Uuid::now_v7, Uuid::new_v7, Uuid::new_v5, uuid::Timestamp::from_unix, uuid::NoContext, uuid::ContextV7, uuid::Builder::from_random_bytes, Uuid::parse_str, Uuid::get_version_num, Uuid::get_variant, Uuid::get_timestamp, Uuid::as_bytes, Uuid::as_u128, Uuid::from_u128, Uuid::nil, Uuid::max, Uuid::is_nil, Uuid::hyphenated, Uuid::simple, Uuid::urn, rand::rngs::StdRng, rand::SeedableRng::seed_from_u64, rand::Rng::random, rand::Rng::random_range, rand::Rng::random_bool, rand::seq::SliceRandom::shuffle, rand::seq::IndexedRandom::choose, rand::seq::IndexedRandom::choose_multiple, rand::seq::IndexedRandom::choose_weighted, rand::seq::index::sample, rand::seq::IteratorRandom::choose, rand::distr::Uniform, rand::distr::weighted::WeightedIndex, rand::distr::Alphanumeric, rand::distr::Distribution::{sample, sample_iter}, rand_distr::Normal, rand_distr::Exp, rand::rngs::OsRng, rand::TryRngCore::try_fill_bytes, Uuid::{from_slice, try_parse, as_fields, from_fields, as_u64_pair, from_u64_pair, to_bytes_le, from_bytes_le, get_version, is_max}, Hyphenated::{encode_lower, encode_upper}, uuid::Builder::from_unix_timestamp_millis, uuid::serde::{compact, simple}, rand::rng, rand::RngCore, rand::Rng::{fill, random_ratio, random_iter}, SeedableRng::from_rng, SliceRandom::partial_shuffle, IndexedMutRandom::choose_mut, IndexedRandom::{choose_multiple_array, choose_multiple_weighted}, IteratorRandom::{choose_multiple, choose_stable}, rand::distr::{Bernoulli, StandardUniform}, custom Distribution impl
//! DOCS: https://docs.rs/uuid/1/uuid/ https://docs.rs/rand/0.9/rand/ https://docs.rs/rand_distr/0.5/rand_distr/
//! CRATES: uuid = "1" (features v4, v5, v7, serde), rand = "0.9" (not the newest major line; method names differ from 0.8), rand_distr = "0.5"
//!
//! CHEAT SHEET
//!   opaque random id                          -> Uuid::new_v4
//!   primary key that sorts by creation time   -> Uuid::now_v7
//!   same input => same id                     -> Uuid::new_v5(&namespace, name)
//!   reproducible test data                    -> a seeded generator, with the seed printed on failure
//!   integer in a range                        -> rng.random_range(a..b)   (never `x % n`)
//!   weighted choice, many draws               -> WeightedIndex
//!   weighted sample WITHOUT replacement       -> choose_multiple_weighted
//!   k random items to the front, in place     -> partial_shuffle
//!   "rows created between t1 and t2" on v7    -> id range from the smallest id of t1 to the largest id of t2
//!   Windows / .NET / SQL Server GUID bytes    -> from_bytes_le / to_bytes_le (first three fields are byte-swapped)
//!   session token / API key / reset link      -> >= 128 bits from the OS (OsRng), then encode
//!
//! REPRODUCIBILITY WARNING. rand documents StdRng as NOT value-stable: its algorithm may change between versions. If a seed
//! must give the same stream forever (golden files, replayable simulations), pick a named algorithm crate (rand_chacha,
//! rand_pcg, rand_xoshiro) and pin it. This file only compares two StdRng streams within one build.
//! SECURITY WARNING. Never derive secrets from seed_from_u64, a timestamp, or any small seed: the whole stream follows from
//! the seed. A UUIDv7 is NOT a secret (it leaks its creation time). v4 uuids come from the OS-backed getrandom, but when
//! you need a secret, say so in code: take raw bytes from OsRng and size them yourself.
//!
//! Read: https://www.rfc-editor.org/rfc/rfc9562.html ; https://rust-random.github.io/book/
//!
//! TRY NEXT:
//!   - [S] Predict how many of 10_000 v7 ids generated in one millisecond with NoContext are out of order; measure.
//!   - [M] Implement `random_range` by `next_u64() % n` for n = 2^63 + 2^62 and show the bias in a histogram.
//!   - [A] PROJECT: with `sqlx_sqlite_queries_transactions`, insert v4 and v7 keys and compare database file sizes.
#![allow(dead_code)]
use rand::TryRngCore;
use rand::distr::{Alphanumeric, Uniform, weighted::WeightedIndex};
use rand::prelude::*;
use rand::rngs::{OsRng, StdRng};
use rand::seq::index::sample;
use rand_distr::{Exp, Normal};
use std::collections::HashSet;
use uuid::{Builder, ContextV7, NoContext, Timestamp, Uuid, Variant};

fn v4_parsing_and_formats() {
    let id = Uuid::new_v4();
    assert_eq!(
        (id.get_version_num(), id.get_variant()),
        (4, Variant::RFC4122)
    );
    assert_ne!(id, Uuid::new_v4());
    let text = "67e55044-10b1-426f-9247-bb680e5fe0c8";
    let u = Uuid::parse_str(text).unwrap();
    // Accepted spellings all parse to the same value.
    for form in [
        text.to_uppercase(),
        text.replace('-', ""),
        format!("urn:uuid:{text}"),
        format!("{{{text}}}"),
    ] {
        assert_eq!(Uuid::parse_str(&form).unwrap(), u, "{form}");
    }
    for bad in [
        "",
        "67e55044",
        "zze55044-10b1-426f-9247-bb680e5fe0c8",
        "67e55044-10b1-426f-9247-bb680e5fe0c8-00",
    ] {
        assert!(Uuid::parse_str(bad).is_err(), "{bad:?}");
    }
    assert_eq!(u.to_string(), text); // Display = hyphenated lowercase
    assert_eq!(u.simple().to_string(), text.replace('-', ""));
    assert_eq!(u.urn().to_string(), format!("urn:uuid:{text}"));
    // Store the 16 bytes, not the 36-character string.
    assert_eq!(Uuid::from_bytes(*u.as_bytes()), u);
    assert_eq!(Uuid::from_u128(u.as_u128()), u);
    assert!(Uuid::nil().is_nil() && Uuid::max().as_u128() == u128::MAX);
    assert!(Uuid::nil() < u && u < Uuid::max());
    // Builder: stamp version and variant bits onto bytes YOU generated (e.g. from a seeded RNG in tests).
    let mut rng = StdRng::seed_from_u64(1);
    let built = Builder::from_random_bytes(rng.random()).into_uuid();
    assert_eq!(built.get_version_num(), 4);
    assert_eq!(
        built,
        Builder::from_random_bytes(StdRng::seed_from_u64(1).random()).into_uuid(),
        "deterministic test ids"
    );
    // serde: hyphenated string in human-readable formats, 16 bytes in binary ones.
    assert_eq!(serde_json::to_string(&u).unwrap(), format!("\"{text}\""));
    assert_eq!(
        postcard::to_allocvec(&u).unwrap().len(),
        17,
        "length byte + 16"
    );
}

fn v7_time_ordering() {
    // The first 48 bits are Unix milliseconds, big-endian, so byte order == string order == time order across milliseconds.
    let at = |ms: u64| {
        Uuid::new_v7(Timestamp::from_unix(
            NoContext,
            ms / 1000,
            ((ms % 1000) * 1_000_000) as u32,
        ))
    };
    let mut rng = StdRng::seed_from_u64(7);
    let times: Vec<u64> = (0..2000)
        .map(|_| 1_700_000_000_000 + rng.random_range(0..10_000_000))
        .collect();
    let ids: Vec<Uuid> = times.iter().map(|&t| at(t)).collect();
    for (id, &t) in ids.iter().zip(&times) {
        assert_eq!(id.get_version_num(), 7);
        assert_eq!(
            u64::from_be_bytes(id.as_bytes()[..8].try_into().unwrap()) >> 16,
            t
        );
        let (secs, nanos) = id.get_timestamp().unwrap().to_unix();
        assert_eq!(
            secs * 1000 + nanos as u64 / 1_000_000,
            t,
            "anyone can read the creation time"
        );
    }
    for i in 1..ids.len() {
        if times[i] != times[i - 1] {
            assert_eq!(ids[i] > ids[i - 1], times[i] > times[i - 1]);
            assert_eq!(
                ids[i].to_string() > ids[i - 1].to_string(),
                times[i] > times[i - 1]
            );
        }
    }
    // WITHIN one millisecond, order needs a counter. ContextV7 provides one.
    let ctx = ContextV7::new();
    let same_ms: Vec<Uuid> = (0..1000)
        .map(|_| Uuid::new_v7(Timestamp::from_unix(&ctx, 1_700_000_000, 0)))
        .collect();
    assert!(same_ms.windows(2).all(|w| w[0] < w[1]));
    // now_v7 uses a shared context: ids from one process are increasing.
    let now: Vec<Uuid> = (0..1000).map(|_| Uuid::now_v7()).collect();
    assert!(now.windows(2).all(|w| w[0] < w[1]));
}

fn v5_name_based() {
    let a = Uuid::new_v5(&Uuid::NAMESPACE_DNS, b"www.example.com");
    // Known value from RFC 9562, Appendix A.4.
    assert_eq!(a.to_string(), "2ed6657d-e927-568b-95e1-2665a8aea6a2");
    assert_eq!(a.get_version_num(), 5);
    assert_ne!(
        a,
        Uuid::new_v5(&Uuid::NAMESPACE_URL, b"www.example.com"),
        "the namespace is part of the input"
    );
    // Make your own namespace once, then derive stable ids from natural keys (idempotent imports).
    let customers = Uuid::new_v5(&Uuid::NAMESPACE_DNS, b"customers.example.com");
    assert_eq!(
        Uuid::new_v5(&customers, b"42"),
        Uuid::new_v5(&customers, b"42")
    );
    // It is a HASH of guessable input: not a secret.
}

fn seeded_generation() {
    let draw = |seed| {
        let mut r = StdRng::seed_from_u64(seed);
        (0..20).map(|_| r.random::<u64>()).collect::<Vec<_>>()
    };
    assert_eq!(draw(42), draw(42));
    assert_ne!(draw(42), draw(43));
    let mut r = StdRng::seed_from_u64(5);
    let mut counts = [0u32; 10];
    for _ in 0..100_000 {
        let x = r.random_range(0..10usize);
        counts[x] += 1;
        let f: f64 = r.random(); // [0, 1)
        assert!((0.0..1.0).contains(&f));
        assert!((5..=6).contains(&r.random_range(5..=6)));
    }
    assert!(
        counts.iter().all(|&c| (9_400..10_600).contains(&c)),
        "{counts:?}"
    );
    let yes = (0..100_000).filter(|_| r.random_bool(0.2)).count();
    assert!((19_000..21_000).contains(&yes));
    // For many draws from the same range, build the distribution once.
    let die = Uniform::new_inclusive(1, 6).unwrap();
    assert!((0..1000).all(|_| (1..=6).contains(&die.sample(&mut r))));
    assert!(
        Uniform::new(5, 5).is_err(),
        "empty range is an error here, but random_range(5..5) PANICS"
    );
    // Write generic code over `Rng` so tests can inject a seeded generator.
    fn roll(rng: &mut (impl Rng + ?Sized)) -> u8 {
        rng.random_range(1..=6)
    }
    assert_eq!(
        roll(&mut StdRng::seed_from_u64(9)),
        roll(&mut StdRng::seed_from_u64(9))
    );
}

fn sequences_and_weights() {
    let mut r = StdRng::seed_from_u64(11);
    let mut v: Vec<u32> = (0..50).collect();
    v.shuffle(&mut r);
    assert_ne!(v, (0..50).collect::<Vec<_>>());
    let mut s = v.clone();
    s.sort_unstable();
    assert_eq!(s, (0..50).collect::<Vec<_>>(), "a permutation");
    // All 6 orders of 3 items appear about equally often (Fisher-Yates is unbiased).
    let mut seen = std::collections::HashMap::new();
    for _ in 0..60_000 {
        let mut a = [1, 2, 3];
        a.shuffle(&mut r);
        *seen.entry(a).or_insert(0) += 1;
    }
    assert!(
        seen.len() == 6 && seen.values().all(|&c| (9_400..10_600).contains(&c)),
        "{seen:?}"
    );

    assert!(v.choose(&mut r).is_some());
    assert!(<[u32]>::choose(&[], &mut r).is_none());
    let three: Vec<u32> = v.choose_multiple(&mut r, 3).copied().collect(); // without replacement
    assert_eq!(three.iter().collect::<HashSet<_>>().len(), 3);
    assert_eq!(
        v.choose_multiple(&mut r, 500).count(),
        50,
        "asking for too many returns all of them"
    );
    let idx = sample(&mut r, 1_000_000, 100).into_vec(); // distinct indices without allocating the population
    assert_eq!(idx.iter().collect::<HashSet<_>>().len(), 100);
    // From an iterator of unknown length (reservoir sampling).
    assert!(
        (0..100)
            .filter(|x| x % 2 == 0)
            .choose(&mut r)
            .is_some_and(|x| x % 2 == 0)
    );

    let backends = [("big", 6u32), ("mid", 3), ("small", 1), ("drained", 0)];
    assert!(backends.choose_weighted(&mut r, |b| b.1).is_ok());
    let index = WeightedIndex::new(backends.iter().map(|b| b.1)).unwrap(); // build once, sample many times
    let mut hits = [0u32; 4];
    for _ in 0..100_000 {
        hits[index.sample(&mut r)] += 1;
    }
    assert_eq!(hits[3], 0);
    for (h, expect) in hits.iter().zip([60_000u32, 30_000, 10_000]) {
        assert!(h.abs_diff(expect) < 1_000, "{hits:?}");
    }
    assert!(WeightedIndex::new([0u32, 0]).is_err());
}

fn continuous_distributions() {
    let mut r = StdRng::seed_from_u64(13);
    let stats = |xs: &[f64]| {
        let mean = xs.iter().sum::<f64>() / xs.len() as f64;
        (
            mean,
            (xs.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / xs.len() as f64).sqrt(),
        )
    };
    // Normal(mean, STANDARD DEVIATION), not variance.
    let xs: Vec<f64> = Normal::new(100.0, 15.0)
        .unwrap()
        .sample_iter(&mut r)
        .take(100_000)
        .collect();
    let (m, sd) = stats(&xs);
    assert!(
        (m - 100.0).abs() < 0.3 && (sd - 15.0).abs() < 0.3,
        "{m} {sd}"
    );
    // Exp(lambda) models gaps between Poisson arrivals: mean 1/lambda. Use it for realistic load generation.
    let gaps: Vec<f64> = Exp::new(4.0)
        .unwrap()
        .sample_iter(&mut r)
        .take(100_000)
        .collect();
    assert!((stats(&gaps).0 - 0.25).abs() < 0.01);
    assert!(Normal::new(0.0, f64::NAN).is_err() && Exp::new(-1.0).is_err());
}

fn uuid_interop_and_layouts() {
    let u = Uuid::parse_str("a1a2a3a4-b1b2-c1c2-d1d2-d3d4d5d6d7d8").unwrap();
    // PITFALL: "the 16 bytes of a GUID" is ambiguous. RFC order is big-endian throughout; Microsoft's GUID layout stores
    // the first three fields little-endian. Mixing them gives a DIFFERENT, equally valid-looking id.
    let guid = u.to_bytes_le();
    assert_eq!(
        guid,
        [
            0xa4, 0xa3, 0xa2, 0xa1, 0xb2, 0xb1, 0xc2, 0xc1, 0xd1, 0xd2, 0xd3, 0xd4, 0xd5, 0xd6,
            0xd7, 0xd8
        ]
    );
    assert_eq!(Uuid::from_bytes_le(guid), u);
    assert_eq!(
        Uuid::from_bytes(guid).to_string(),
        "a4a3a2a1-b2b1-c2c1-d1d2-d3d4d5d6d7d8",
        "silently wrong, no error"
    );
    // Fields and halves, for columns or protocols that split the value.
    let (d1, d2, d3, d4) = u.as_fields();
    assert_eq!((d1, d2, d3), (0xa1a2_a3a4, 0xb1b2, 0xc1c2));
    assert_eq!(Uuid::from_fields(d1, d2, d3, d4), u);
    let (hi, lo) = u.as_u64_pair();
    assert_eq!(Uuid::from_u64_pair(hi, lo), u);
    // from_slice is the checked way in from a database blob or a packet.
    assert_eq!(Uuid::from_slice(u.as_bytes()), Ok(u));
    assert!(Uuid::from_slice(&u.as_bytes()[..15]).is_err());
    assert_eq!(Uuid::try_parse(&u.to_string()), Ok(u));
    // Parsing accepts any 128 bits: version and variant are NOT validated. Check them if the version matters.
    assert_eq!(
        u.get_version(),
        None,
        "version nibble 0xc is not a known version"
    );
    assert_eq!(Uuid::new_v4().get_version(), Some(uuid::Version::Random));
    assert!(Uuid::max().is_max());
    // Format into a stack buffer on hot paths instead of allocating a String.
    let mut buf = Uuid::encode_buffer();
    assert_eq!(
        u.hyphenated().encode_upper(&mut buf),
        "A1A2A3A4-B1B2-C1C2-D1D2-D3D4D5D6D7D8"
    );
    assert_eq!(u.simple().encode_lower(&mut buf).len(), 32);
    // Per-field serde representation: pick it where the struct is declared.
    #[derive(serde::Serialize, serde::Deserialize, PartialEq, Debug)]
    struct Row {
        #[serde(with = "uuid::serde::compact")]
        raw: Uuid, // 16 numbers in JSON: for binary formats
        #[serde(with = "uuid::serde::simple")]
        short: Uuid, // 32 hex digits, no hyphens
    }
    let row = Row {
        raw: Uuid::from_u128(1),
        short: u,
    };
    let json = serde_json::to_string(&row).unwrap();
    assert_eq!(
        json,
        r#"{"raw":[0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,1],"short":"a1a2a3a4b1b2c1c2d1d2d3d4d5d6d7d8"}"#
    );
    assert_eq!(serde_json::from_str::<Row>(&json).unwrap(), row);
}

/// Scenario: v7 primary keys make "created between" a plain index range scan, with no created_at index.
fn v7_range_queries() {
    let bound =
        |ms: u64, fill: u8| Builder::from_unix_timestamp_millis(ms, &[fill; 10]).into_uuid();
    let mut r = StdRng::seed_from_u64(0x7A);
    let base = 1_700_000_000_000u64;
    let mut index = std::collections::BTreeMap::new();
    for _ in 0..3000 {
        let ms = base + r.random_range(0..2_000);
        index.insert(
            Builder::from_unix_timestamp_millis(ms, &r.random()).into_uuid(),
            ms,
        );
    }
    for _ in 0..200 {
        let from = base + r.random_range(0..1_900);
        let to = from + r.random_range(0..100);
        // Smallest and largest ids that can exist in those milliseconds (version and variant bits are forced either way).
        let hits: Vec<u64> = index
            .range(bound(from, 0)..=bound(to, 0xFF))
            .map(|(_, &ms)| ms)
            .collect();
        let mut want: Vec<u64> = index
            .values()
            .copied()
            .filter(|ms| (from..=to).contains(ms))
            .collect();
        want.sort_unstable();
        assert_eq!(hits, want);
    }
}

/// A type of our own that can be drawn with any Rng: implement Distribution.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Fault {
    None,
    Delay(u8),
    Drop,
}
struct FaultPlan {
    delay: rand::distr::Bernoulli,
    drop: rand::distr::Bernoulli,
}
impl Distribution<Fault> for FaultPlan {
    fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> Fault {
        if self.drop.sample(rng) {
            Fault::Drop
        } else if self.delay.sample(rng) {
            Fault::Delay(rng.random_range(1..=50))
        } else {
            Fault::None
        }
    }
}

fn rand_more_surface() {
    use rand::distr::{Bernoulli, StandardUniform};
    use rand::seq::IndexedMutRandom;
    let mut r = StdRng::seed_from_u64(21);
    // Scenario: reproducible fault injection for a simulation test.
    let plan = FaultPlan {
        delay: Bernoulli::new(0.25).unwrap(),
        drop: Bernoulli::from_ratio(1, 10).unwrap(),
    };
    assert!(Bernoulli::new(1.5).is_err());
    let faults: Vec<Fault> = (&plan).sample_iter(&mut r).take(100_000).collect();
    let dropped = faults.iter().filter(|f| **f == Fault::Drop).count();
    let delayed = faults
        .iter()
        .filter(|f| matches!(f, Fault::Delay(1..=50)))
        .count();
    assert!((9_500..10_500).contains(&dropped), "{dropped}");
    assert!(
        (21_800..23_200).contains(&delayed),
        "25% of the remaining 90%: {delayed}"
    );
    let again: Vec<Fault> = plan
        .sample_iter(StdRng::seed_from_u64(21))
        .take(100_000)
        .collect();
    assert_eq!(
        again, faults,
        "same seed, same schedule (within one build, see the header)"
    );

    // random_ratio panics for numerator > denominator or a zero denominator; extremes are exact.
    assert!((0..1000).all(|_| r.random_ratio(5, 5)) && !(0..1000).any(|_| r.random_ratio(0, 5)));
    // fill whole buffers and arrays.
    let mut block = [0u32; 64];
    r.fill(&mut block);
    assert!(block.iter().collect::<HashSet<_>>().len() > 60);
    let firsts: Vec<u8> = StdRng::seed_from_u64(1).random_iter().take(8).collect();
    assert_eq!(
        firsts,
        StdRng::seed_from_u64(1)
            .sample_iter(StandardUniform)
            .take(8)
            .collect::<Vec<u8>>()
    );

    // Independent child generators: give each worker its own stream derived from one parent seed, so that adding draws in
    // one worker does not shift the values another one sees.
    let streams = |parent: u64| {
        let mut p = StdRng::seed_from_u64(parent);
        let kids: Vec<StdRng> = (0..3).map(|_| StdRng::from_rng(&mut p)).collect();
        kids.into_iter()
            .map(|mut k| k.random::<u64>())
            .collect::<Vec<_>>()
    };
    assert_eq!(streams(9), streams(9));
    assert_eq!(streams(9).iter().collect::<HashSet<_>>().len(), 3);

    // Object safety: `Rng` has generic methods, so `dyn Rng` does not exist. Use &mut dyn RngCore; it implements Rng.
    fn through_dyn(rng: &mut dyn rand::RngCore) -> u8 {
        rng.random_range(0..10)
    }
    assert!(through_dyn(&mut r) < 10);
    // rand::rng() is the lazily seeded per-thread generator: right for "any random number", useless for replay.
    assert!(rand::rng().random_range(0..3) < 3);

    // partial_shuffle: pick k random elements in O(k) by moving them aside. WHERE they end up inside the slice is not part
    // of the contract: use the two returned slices.
    let mut deck: Vec<u8> = (0..52).collect();
    let (hand, rest) = deck.partial_shuffle(&mut r, 5);
    assert_eq!((hand.len(), rest.len()), (5, 47));
    let all: HashSet<u8> = hand.iter().chain(rest.iter()).copied().collect();
    assert_eq!(all.len(), 52);
    let mut stock = [0u32; 4];
    *stock.choose_mut(&mut r).unwrap() += 1;
    assert_eq!(stock.iter().sum::<u32>(), 1);
    let five: [u8; 5] = deck.choose_multiple_array(&mut r).unwrap();
    assert_eq!(five.iter().collect::<HashSet<_>>().len(), 5);
    assert!(deck.choose_multiple_array::<_, 53>(&mut r).is_none());

    // Weighted sampling WITHOUT replacement, e.g. choosing 2 distinct replicas by capacity.
    let nodes = [("a", 8.0), ("b", 1.0), ("c", 1.0), ("off", 0.0)];
    let mut seen = std::collections::HashMap::new();
    for _ in 0..5000 {
        let picked: Vec<&str> = nodes
            .choose_multiple_weighted(&mut r, 2, |n| n.1)
            .unwrap()
            .map(|n| n.0)
            .collect();
        assert!(
            picked.len() == 2 && picked[0] != picked[1] && !picked.contains(&"off"),
            "{picked:?}"
        );
        for p in picked {
            *seen.entry(p).or_insert(0u32) += 1;
        }
    }
    assert!(
        seen["a"] > 4800 && seen["b"] > 400 && seen["c"] > 400,
        "{seen:?}"
    );

    // PITFALL: IteratorRandom::choose_multiple picks random elements, but its documentation says their ORDER is not fully
    // random (it is a reservoir). Shuffle if order matters. Asking for the whole input returns it in the ORIGINAL order.
    let mut everything = (0..10).choose_multiple(&mut r, 10);
    assert_eq!(everything, (0..10).collect::<Vec<_>>());
    everything.shuffle(&mut r);
    assert_ne!(everything, (0..10).collect::<Vec<_>>());
    // choose() may take different draws depending on size_hint; choose_stable() does not depend on it, so a refactor from
    // `a.iter()` to `a.iter().filter(..)` that keeps the same items keeps the same pick for a given seed.
    let pick = |hinted: bool| {
        let mut r = StdRng::seed_from_u64(4);
        let items = 0..1000;
        if hinted {
            items.choose_stable(&mut r)
        } else {
            items.filter(|_| true).choose_stable(&mut r)
        }
    };
    assert_eq!(pick(true), pick(false));
}

/// 256 bits from the operating system, hex encoded. OsRng is fallible, so it implements TryRngCore.
fn secret_token() -> String {
    let mut bytes = [0u8; 32];
    OsRng
        .try_fill_bytes(&mut bytes)
        .expect("the OS random source failed");
    hex::encode(bytes)
}
/// Compare secrets without an early exit. (In production use an audited crate such as `subtle`; an optimiser gives no
/// guarantee about hand-written constant-time code.)
fn same_secret(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}
fn tokens() {
    let set: HashSet<String> = (0..1000).map(|_| secret_token()).collect();
    assert_eq!(set.len(), 1000);
    assert!(set.iter().all(|t| t.len() == 64));
    let t = secret_token();
    assert!(
        same_secret(t.as_bytes(), t.clone().as_bytes())
            && !same_secret(t.as_bytes(), secret_token().as_bytes())
    );
    // Alphanumeric: about 5.95 bits per character, so 22 characters exceed 128 bits. The SOURCE decides the security.
    let code: String = (&mut StdRng::from_os_rng())
        .sample_iter(Alphanumeric)
        .take(22)
        .map(char::from)
        .collect();
    assert!(code.len() == 22 && code.bytes().all(|b| b.is_ascii_alphanumeric()));
    assert!(22.0 * 62f64.log2() > 128.0);
    // Why a small seed is fatal: an attacker simply tries them all.
    let weak_token = StdRng::seed_from_u64(31_337).random::<u128>();
    let cracked =
        (0..100_000u64).find(|&s| StdRng::seed_from_u64(s).random::<u128>() == weak_token);
    assert_eq!(cracked, Some(31_337));
}

pub fn main() {
    v4_parsing_and_formats();
    v7_time_ordering();
    v5_name_based();
    uuid_interop_and_layouts();
    v7_range_queries();
    rand_more_surface();
    seeded_generation();
    sequences_and_weights();
    continuous_distributions();
    tokens();
    println!("uuid + rand: all sections passed");
}
