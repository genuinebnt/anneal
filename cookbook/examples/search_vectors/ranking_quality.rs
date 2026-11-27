//! RECIPE 1209 (vectors): precision, recall, reciprocal rank, average precision and nDCG, with ties, missing judgements and a paired comparison of two rankers.
//! LEVEL: intermediate
//! Use in: search, recommendations, retrieval for a language model, any change to a ranking.   Run: cargo run --release --example ranking_quality
//!
//! INPUT: for each query, a ranked list of document ids, and judgements that give some documents a grade. Grade 0 means
//! judged and not relevant. A document with no grade is UNJUDGED, which is not the same as 0. The metrics count it as
//! 0 unless you ask otherwise, so this file also reports how many results were unjudged.
//!
//! CONVENTIONS: precision@k always divides by k, also for a short list. Recall, average precision and nDCG have no value
//! for a query with no relevant document. nDCG builds its ideal list from every judged document, not only the returned
//! ones. A list that repeats a document is refused.
//!
//! TIES: when scores are equal, the order is an accident. For DCG the average over all tie orders has a short form: each
//! place in a tied group gets the group's mean gain.
//!
//! COMPARING TWO RANKERS: use the same queries and take the difference per query. A sign-flip randomisation test and a
//! bootstrap interval work on those differences. The simulations here measure their false alarms and coverage, and what
//! happens when the best of twenty equal rankers is reported.
//!
//! COMMON USES: offline tests of search and recommendation changes, retrieval tests for retrieval-augmented generation,
//! regression checks in a build.
//! Read: https://github.com/usnistgov/trec_eval
//! Read: https://tsapps.nist.gov/publication/get_pdf.cfm?pub_id=150469
//! Read: https://aclanthology.org/P18-1128/
#![allow(dead_code)]
use std::collections::{HashMap, HashSet};

/// Document id to grade. A missing id is unjudged.
type Judgements = HashMap<u32, u32>;

/// Grades from 1 up count as relevant for the metrics that only know yes and no.
const RELEVANT: u32 = 1;

fn is_relevant(judged: &Judgements, doc: u32) -> bool {
    judged.get(&doc).is_some_and(|&grade| grade >= RELEVANT)
}

fn relevant_total(judged: &Judgements) -> usize {
    judged.values().filter(|&&grade| grade >= RELEVANT).count()
}

/// The first document that appears twice. Counting it twice could push nDCG above 1.
fn repeated(ranked: &[u32]) -> Option<u32> {
    let mut seen = HashSet::new();
    ranked.iter().copied().find(|&doc| !seen.insert(doc))
}

fn check_ranking(ranked: &[u32]) {
    if let Some(doc) = repeated(ranked) {
        panic!("document {doc} is ranked twice");
    }
}

fn hits_in_top(ranked: &[u32], judged: &Judgements, k: usize) -> usize {
    check_ranking(ranked);
    ranked
        .iter()
        .take(k)
        .filter(|&&doc| is_relevant(judged, doc))
        .count()
}

/// Relevant results in the top k, divided by k even when fewer than k came back.
fn precision_at(ranked: &[u32], judged: &Judgements, k: usize) -> f64 {
    assert!(k > 0, "k starts at 1");
    hits_in_top(ranked, judged, k) as f64 / k as f64
}

/// The share of all relevant documents that is in the top k.
fn recall_at(ranked: &[u32], judged: &Judgements, k: usize) -> Option<f64> {
    let total = relevant_total(judged);
    (total > 0).then(|| hits_in_top(ranked, judged, k) as f64 / total as f64)
}

/// 1 divided by the place of the first relevant result, or 0 if there is none.
fn reciprocal_rank(ranked: &[u32], judged: &Judgements) -> f64 {
    check_ranking(ranked);
    ranked
        .iter()
        .position(|&doc| is_relevant(judged, doc))
        .map_or(0.0, |at| 1.0 / (at + 1) as f64)
}

/// The precision at each relevant result, summed and divided by ALL relevant documents.
fn average_precision(ranked: &[u32], judged: &Judgements) -> Option<f64> {
    check_ranking(ranked);
    let total = relevant_total(judged);
    let mut found = 0;
    let mut sum = 0.0;
    for (at, &doc) in ranked.iter().enumerate() {
        if is_relevant(judged, doc) {
            found += 1;
            sum += found as f64 / (at + 1) as f64;
        }
    }
    (total > 0).then(|| sum / total as f64)
}

#[derive(Clone, Copy, Debug)]
enum Gain {
    /// The grade itself. `trec_eval` does this by default.
    Linear,
    /// 2^grade - 1, which makes high grades count for much more.
    Exponential,
}

impl Gain {
    /// Stops if the gain is too large for an `f64`, as 2^1024 is.
    fn of(self, grade: u32) -> f64 {
        let gain = match self {
            Gain::Linear => grade as f64,
            Gain::Exponential => 2f64.powi(i32::try_from(grade).unwrap_or(i32::MAX)) - 1.0,
        };
        assert!(gain.is_finite(), "the gain of grade {grade} overflows");
        gain
    }
}

/// The weight of a place, counted from 0: 1 / log2(place + 2).
fn discount(at: usize) -> f64 {
    1.0 / ((at + 2) as f64).log2()
}

/// Discounted cumulative gain of gains already in ranked order.
fn dcg(gains: impl IntoIterator<Item = f64>, k: usize) -> f64 {
    let sum: f64 = gains
        .into_iter()
        .take(k)
        .enumerate()
        .map(|(at, gain)| gain * discount(at))
        .sum();
    // finite gains can still add up to infinity, and infinity / infinity is not a number
    assert!(
        sum.is_finite(),
        "the gains add up to more than an f64 holds"
    );
    sum
}

/// The best DCG any list could reach: every judged document, best first.
fn ideal_dcg(judged: &Judgements, k: usize, gain: Gain) -> f64 {
    let mut gains: Vec<f64> = judged.values().map(|&grade| gain.of(grade)).collect();
    gains.sort_by(|a, b| b.total_cmp(a));
    dcg(gains, k)
}

fn gain_of(judged: &Judgements, doc: u32, gain: Gain) -> f64 {
    judged.get(&doc).map_or(0.0, |&grade| gain.of(grade))
}

fn ndcg_at(ranked: &[u32], judged: &Judgements, k: usize, gain: Gain) -> Option<f64> {
    check_ranking(ranked);
    let ideal = ideal_dcg(judged, k, gain);
    let found = dcg(ranked.iter().map(|&doc| gain_of(judged, doc, gain)), k);
    (ideal > 0.0).then(|| found / ideal)
}

// ---- ties ----

/// Groups of equal score, highest score first. Inside a group the ids are ascending.
fn tie_groups(scored: &[(u32, f64)]) -> Vec<Vec<u32>> {
    assert!(
        scored.iter().all(|(_, score)| score.is_finite()),
        "a score is not a number"
    );
    check_ranking(&scored.iter().map(|&(doc, _)| doc).collect::<Vec<_>>());
    // adding 0.0 turns -0.0 into 0.0, so that the two stay tied
    let mut sorted: Vec<(u32, f64)> = scored.iter().map(|&(d, s)| (d, s + 0.0)).collect();
    sorted.sort_by(|a, b| b.1.total_cmp(&a.1).then(a.0.cmp(&b.0)));
    sorted
        .chunk_by(|a, b| a.1 == b.1)
        .map(|group| group.iter().map(|&(doc, _)| doc).collect())
        .collect()
}

/// nDCG averaged over every order of every tied group.
fn expected_ndcg_at(
    scored: &[(u32, f64)],
    judged: &Judgements,
    k: usize,
    gain: Gain,
) -> Option<f64> {
    let gains = tie_groups(scored).into_iter().flat_map(|group| {
        let total: f64 = group.iter().map(|&d| gain_of(judged, d, gain)).sum();
        vec![total / group.len() as f64; group.len()]
    });
    let ideal = ideal_dcg(judged, k, gain);
    let found = dcg(gains, k);
    (ideal > 0.0).then(|| found / ideal)
}

/// The lowest and highest nDCG that some order of the tied groups gives.
fn ndcg_range_at(
    scored: &[(u32, f64)],
    judged: &Judgements,
    k: usize,
    gain: Gain,
) -> Option<(f64, f64)> {
    let ordered = |best_first: bool| -> Vec<u32> {
        let mut groups = tie_groups(scored);
        for group in &mut groups {
            group.sort_by_key(|d| judged.get(d).copied().unwrap_or(0));
            if best_first {
                group.reverse();
            }
        }
        groups.concat()
    };
    Some((
        ndcg_at(&ordered(false), judged, k, gain)?,
        ndcg_at(&ordered(true), judged, k, gain)?,
    ))
}

// ---- missing judgements ----

fn unjudged_in_top(ranked: &[u32], judged: &Judgements, k: usize) -> usize {
    ranked
        .iter()
        .take(k)
        .filter(|d| !judged.contains_key(d))
        .count()
}

/// Precision@k if every unjudged result is not relevant, and if every one is.
fn precision_bounds(ranked: &[u32], judged: &Judgements, k: usize) -> (f64, f64) {
    let low = precision_at(ranked, judged, k);
    (
        low,
        low + unjudged_in_top(ranked, judged, k) as f64 / k as f64,
    )
}

/// The list with unjudged documents taken out (a "condensed list").
fn condensed(ranked: &[u32], judged: &Judgements) -> Vec<u32> {
    ranked
        .iter()
        .copied()
        .filter(|d| judged.contains_key(d))
        .collect()
}

// ---- comparing two rankers ----

/// xorshift64, for repeatable resampling.
struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
    /// A number from 0 up to, but not including, 1.
    fn unit(&mut self) -> f64 {
        (self.next() >> 11) as f64 / (1u64 << 53) as f64
    }
}

/// Stops on NaN, on infinity, and on values whose sum could overflow. Each would give a confident wrong answer.
fn check_finite(values: &[f64]) {
    let size: f64 = values.iter().map(|v| v.abs()).sum();
    assert!(size.is_finite(), "a value is not a finite number");
}

fn mean(values: &[f64]) -> f64 {
    assert!(!values.is_empty(), "no queries to average");
    check_finite(values);
    values.iter().sum::<f64>() / values.len() as f64
}

/// Per-query differences, second minus first. A query without a value for BOTH is a mistake, not a skip.
fn paired(first: &[Option<f64>], second: &[Option<f64>]) -> Vec<f64> {
    assert_eq!(first.len(), second.len(), "not the same queries");
    first
        .iter()
        .zip(second)
        .filter_map(|pair| match pair {
            (Some(a), Some(b)) => {
                check_finite(&[b - a]);
                Some(b - a)
            }
            (None, None) => None,
            _ => panic!("only one ranker has a value for this query"),
        })
        .collect()
}

/// Two-sided p-value of the sign-flip randomisation test. If the rankers were interchangeable, each difference
/// could as well have had the other sign. Count how often random signs give a mean at least as far from 0.
fn sign_flip_p(differences: &[f64], rounds: u32, rng: &mut Rng) -> f64 {
    let observed = mean(differences).abs();
    let extreme = (0..rounds)
        .filter(|_| {
            let sum: f64 = differences
                .iter()
                .map(|d| if rng.next() >> 63 == 0 { *d } else { -d })
                .sum();
            (sum / differences.len() as f64).abs() >= observed - 1e-12
        })
        .count();
    // the observed signs are one possible draw, so the result is never 0
    (extreme + 1) as f64 / (rounds + 1) as f64
}

/// A 95% percentile bootstrap interval for the mean difference: resample whole queries.
fn bootstrap_interval(differences: &[f64], rounds: usize, rng: &mut Rng) -> (f64, f64) {
    assert!(rounds >= 40, "too few rounds for a 95% interval");
    let n = differences.len();
    assert!(n > 0, "no queries to resample");
    check_finite(differences);
    let mut means: Vec<f64> = (0..rounds)
        .map(|_| (0..n).map(|_| differences[rng.below(n)]).sum::<f64>() / n as f64)
        .collect();
    means.sort_by(f64::total_cmp);
    (means[rounds / 40], means[rounds - 1 - rounds / 40])
}

// ---- oracles, written without the code above ----

/// DCG of a list of grades, using natural logarithms.
fn naive_ndcg(ranked: &[u32], judged: &Judgements, k: usize, exponential: bool) -> Option<f64> {
    let value = |g: u32| {
        if exponential {
            ((1u64 << g) - 1) as f64
        } else {
            g as f64
        }
    };
    let score = |grades: &[u32]| {
        let mut sum = 0.0;
        for place in 1..=grades.len().min(k) {
            sum += value(grades[place - 1]) * 2f64.ln() / (place as f64 + 1.0).ln();
        }
        sum
    };
    let got: Vec<u32> = ranked
        .iter()
        .map(|d| *judged.get(d).unwrap_or(&0))
        .collect();
    let mut best: Vec<u32> = judged.values().copied().collect();
    best.sort_unstable_by(|a, b| b.cmp(a));
    (score(&best) > 0.0).then(|| score(&got) / score(&best))
}

/// Every order of a list.
fn orders(items: &[u32]) -> Vec<Vec<u32>> {
    if items.is_empty() {
        return vec![Vec::new()];
    }
    let mut all = Vec::new();
    for (at, &first) in items.iter().enumerate() {
        let mut rest = items.to_vec();
        rest.remove(at);
        for mut tail in orders(&rest) {
            tail.insert(0, first);
            all.push(tail);
        }
    }
    all
}

/// Every ranking that respects the scores.
fn all_tie_orders(scored: &[(u32, f64)]) -> Vec<Vec<u32>> {
    orders(&scored.iter().map(|&(d, _)| d).collect::<Vec<_>>())
        .into_iter()
        .filter(|order| {
            let score = |d: u32| scored.iter().find(|(x, _)| *x == d).unwrap().1;
            order.windows(2).all(|w| score(w[0]) >= score(w[1]))
        })
        .collect()
}

/// The p-value from all 2^n sign patterns.
fn exact_sign_flip_p(differences: &[f64]) -> f64 {
    let n = differences.len();
    assert!(n <= 20, "too many patterns to list");
    let observed = differences.iter().sum::<f64>().abs();
    let extreme = (0u32..1 << n)
        .filter(|signs| {
            let sum: f64 = (0..n)
                .map(|i| {
                    if signs >> i & 1 == 0 {
                        differences[i]
                    } else {
                        -differences[i]
                    }
                })
                .sum();
            sum.abs() >= observed - 1e-9
        })
        .count();
    extreme as f64 / (1u64 << n) as f64
}

// ---- checks ----

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-9
}

fn same(a: Option<f64>, b: Option<f64>) -> bool {
    match (a, b) {
        (Some(a), Some(b)) => close(a, b),
        _ => a.is_none() && b.is_none(),
    }
}

/// Whether a call stops with a panic. The panic message is not printed.
fn panics<T>(call: impl FnOnce() -> T) -> bool {
    let hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));
    let failed = std::panic::catch_unwind(std::panic::AssertUnwindSafe(call)).is_err();
    std::panic::set_hook(hook);
    failed
}

/// One query, two rankers, worked by hand.
fn check_worked_example() {
    let judged: Judgements = [(1, 3), (2, 2), (3, 1), (4, 0), (7, 2)].into();
    let a = [3, 4, 1, 9, 2];
    let b = [1, 8, 4, 2, 9];
    println!("One query, k=5     A       B");
    let rows = [
        ("precision@5", [&a, &b].map(|r| precision_at(r, &judged, 5))),
        (
            "recall@5",
            [&a, &b].map(|r| recall_at(r, &judged, 5).unwrap()),
        ),
        (
            "reciprocal rank",
            [&a, &b].map(|r| reciprocal_rank(r, &judged)),
        ),
        (
            "average precision",
            [&a, &b].map(|r| average_precision(r, &judged).unwrap()),
        ),
        (
            "nDCG@5 linear",
            [&a, &b].map(|r| ndcg_at(r, &judged, 5, Gain::Linear).unwrap()),
        ),
        (
            "nDCG@5 exponential",
            [&a, &b].map(|r| ndcg_at(r, &judged, 5, Gain::Exponential).unwrap()),
        ),
    ];
    for (name, [x, y]) in rows {
        println!("  {name:<18} {x:.3}   {y:.3}");
    }
    for r in [&a, &b] {
        let (low, high) = precision_bounds(r, &judged, 5);
        println!(
            "  unjudged {}, precision@5 from {low:.1} to {high:.1}",
            unjudged_in_top(r, &judged, 5)
        );
    }
    assert!(close(rows[0].1[0], 0.6) && close(rows[0].1[1], 0.4));
    assert!(close(rows[1].1[0], 0.75) && close(rows[1].1[1], 0.5));
    assert!(close(rows[3].1[0], 17.0 / 30.0) && close(rows[3].1[1], 0.375));
    // A wins on average precision, B on nDCG
    assert!(rows[4].1[0] < rows[4].1[1]);
    assert!((rows[4].1[0] - 0.575).abs() < 5e-4 && (rows[4].1[1] - 0.678).abs() < 5e-4);
    assert_eq!(condensed(&b, &judged), [1, 4, 2]);
    println!(
        "  condensed: A average precision {:.3}, B {:.3}",
        average_precision(&condensed(&a, &judged), &judged).unwrap(),
        average_precision(&condensed(&b, &judged), &judged).unwrap()
    );

    // an ideal list built from the returned documents alone would call this list perfect
    assert!(ndcg_at(&[3], &judged, 5, Gain::Linear).unwrap() < 0.2);
    // a short list is still divided by k
    assert!(close(precision_at(&[1], &judged, 5), 0.2));
}

fn check_edges() {
    let none: Judgements = [(1, 0)].into();
    assert_eq!(recall_at(&[1], &none, 3), None);
    assert_eq!(average_precision(&[1], &none), None);
    assert_eq!(ndcg_at(&[1], &none, 3, Gain::Linear), None);
    assert_eq!(expected_ndcg_at(&[(1, 0.5)], &none, 3, Gain::Linear), None);
    assert_eq!(reciprocal_rank(&[], &none), 0.0);
    let one: Judgements = [(1, 2)].into();
    assert_eq!(ndcg_at(&[], &one, 3, Gain::Linear), Some(0.0));

    assert!(panics(|| ndcg_at(&[1, 1], &one, 2, Gain::Linear)), "twice");
    assert!(panics(|| precision_at(&[1, 1], &one, 2)));
    assert!(panics(|| average_precision(&[1, 1], &one)));
    assert!(panics(|| precision_at(&[1], &one, 0)));
    assert!(panics(|| tie_groups(&[(1, f64::NAN)])));
    assert!(panics(|| tie_groups(&[(1, 1.0), (1, 0.5)])));
    assert!(panics(|| paired(&[Some(1.0)], &[None])));
    assert!(panics(|| paired(&[Some(1.0)], &[])));
    assert!(panics(|| bootstrap_interval(&[], 100, &mut Rng(1))));
    assert!(
        panics(|| bootstrap_interval(&[0.1], 10, &mut Rng(1))),
        "10 rounds"
    );
    assert!(panics(|| mean(&[])));

    // numbers that are not finite stop every step, instead of giving a confident answer
    for bad in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert!(panics(|| mean(&[0.1, bad])), "mean of {bad}");
        assert!(
            panics(|| paired(&[Some(0.1)], &[Some(bad)])),
            "paired {bad}"
        );
        assert!(
            panics(|| sign_flip_p(&[0.1, bad], 499, &mut Rng(1))),
            "p {bad}"
        );
        assert!(panics(|| bootstrap_interval(&[0.1, bad], 100, &mut Rng(1))));
    }
    let huge = [f64::MAX, -f64::MAX];
    assert!(panics(|| sign_flip_p(&huge, 499, &mut Rng(1))), "overflow");
    assert!(panics(|| bootstrap_interval(&huge, 100, &mut Rng(1))));
    assert!(panics(|| paired(&[Some(-f64::MAX)], &[Some(f64::MAX)])));
    assert!(Gain::Exponential.of(1023).is_finite());
    for grade in [1024, u32::MAX] {
        assert!(panics(|| Gain::Exponential.of(grade)), "gain of {grade}");
        let big: Judgements = [(1, grade)].into();
        assert!(panics(|| ndcg_at(&[1], &big, 3, Gain::Exponential)));
    }
    // two gains that each fit, but not their sum
    let two: Judgements = [(1, 1023), (2, 1023), (3, 1023)].into();
    assert!(
        panics(|| ndcg_at(&[1, 2, 3], &two, 3, Gain::Exponential)),
        "sum"
    );
    // 20 equal gains: no random pattern is likely to match, yet p must not be 0
    let p = sign_flip_p(&[0.1; 20], 499, &mut Rng(2));
    assert!(p >= 1.0 / 500.0, "a sampled p-value of 0");
    assert_eq!(paired(&[Some(0.2), None], &[Some(0.5), None]).len(), 1);
    assert_eq!(tie_groups(&[(2, 0.0), (1, -0.0)]), [vec![1, 2]]);
}

fn check_against_naive() {
    let mut rng = Rng(11);
    let (mut with_ties, mut no_relevant) = (0, 0);
    for _ in 0..3_000 {
        let mut judged = Judgements::new();
        for doc in 0..8 {
            if rng.below(2) > 0 {
                judged.insert(doc, rng.below(4) as u32);
            }
        }
        let mut docs: Vec<u32> = (0..8).collect();
        for i in (1..docs.len()).rev() {
            docs.swap(i, rng.below(i + 1));
        }
        docs.truncate(rng.below(7));
        let k = 1 + rng.below(7);

        let top: Vec<bool> = docs.iter().map(|d| is_relevant(&judged, *d)).collect();
        let total = judged.values().filter(|g| **g > 0).count();
        let count = |n: usize| top.iter().take(n).filter(|hit| **hit).count() as f64;
        assert!(close(precision_at(&docs, &judged, k), count(k) / k as f64));
        assert!(same(
            recall_at(&docs, &judged, k),
            (total > 0).then(|| count(k) / total as f64)
        ));
        let mut first = 0.0;
        let mut sum = 0.0;
        for place in 1..=docs.len() {
            if top[place - 1] {
                sum += count(place) / place as f64;
                if first == 0.0 {
                    first = 1.0 / place as f64;
                }
            }
        }
        assert!(close(reciprocal_rank(&docs, &judged), first));
        assert!(same(
            average_precision(&docs, &judged),
            (total > 0).then(|| sum / total as f64)
        ));
        for (gain, exponential) in [(Gain::Linear, false), (Gain::Exponential, true)] {
            let got = ndcg_at(&docs, &judged, k, gain);
            assert!(same(got, naive_ndcg(&docs, &judged, k, exponential)));
            assert!(got.is_none_or(|v| (0.0..=1.0 + 1e-9).contains(&v)));
        }
        no_relevant += usize::from(total == 0);

        // ties: three score levels over at most 6 documents
        let scored: Vec<(u32, f64)> = docs.iter().map(|&d| (d, rng.below(3) as f64)).collect();
        let every: Vec<Option<f64>> = all_tie_orders(&scored)
            .iter()
            .map(|order| naive_ndcg(order, &judged, k, false))
            .collect();
        let expected = expected_ndcg_at(&scored, &judged, k, Gain::Linear);
        let range = ndcg_range_at(&scored, &judged, k, Gain::Linear);
        if let Some(values) = every.iter().copied().collect::<Option<Vec<f64>>>() {
            let low = values.iter().copied().fold(f64::INFINITY, f64::min);
            let high = values.iter().copied().fold(0.0, f64::max);
            assert!(close(expected.unwrap(), mean(&values)));
            let (a, b) = range.unwrap();
            assert!(close(a, low) && close(b, high));
            with_ties += usize::from(values.len() > 1);
        } else {
            assert_eq!(expected, None);
        }
    }
    assert!(with_ties > 500 && no_relevant > 20, "weak random cases");

    // the sampled p-value against the exact one
    for case in 0..20 {
        let differences: Vec<f64> = (0..4 + case % 9).map(|_| rng.unit() - 0.4).collect();
        let exact = exact_sign_flip_p(&differences);
        let sampled = sign_flip_p(&differences, 20_000, &mut rng);
        assert!((exact - sampled).abs() < 0.02, "{exact} {sampled}");
    }
}

fn demonstrate_ties() {
    // one relevant document among ten with the same score
    let judged: Judgements = [(7, 1)].into();
    let scored: Vec<(u32, f64)> = (0..10).map(|d| (d, 1.0)).collect();
    let by_id = tie_groups(&scored).concat();
    let (low, high) = ndcg_range_at(&scored, &judged, 10, Gain::Linear).unwrap();
    println!(
        "Ten tied documents: by id {:.3}, average {:.3}, range {low:.3} to {high:.3}",
        ndcg_at(&by_id, &judged, 10, Gain::Linear).unwrap(),
        expected_ndcg_at(&scored, &judged, 10, Gain::Linear).unwrap()
    );
}

// ---- simulations ----

/// Per-query scores of a ranker: the query's difficulty, plus the ranker's own luck on it, plus `lift`.
fn scores(rng: &mut Rng, difficulty: &[f64], lift: f64) -> Vec<f64> {
    difficulty
        .iter()
        .map(|d| d + lift + 0.3 * (rng.unit() - 0.5))
        .collect()
}

fn difficulties(rng: &mut Rng, queries: usize) -> Vec<f64> {
    (0..queries).map(|_| 0.2 + 0.6 * rng.unit()).collect()
}

fn minus(a: &[f64], b: &[f64]) -> Vec<f64> {
    a.iter().zip(b).map(|(a, b)| a - b).collect()
}

const EXPERIMENTS: usize = 1_000;
const ROUNDS: u32 = 499;

fn share(count: usize) -> f64 {
    count as f64 / EXPERIMENTS as f64
}

/// False alarms between equal rankers, and detection of a real gain with and without pairing.
fn simulate_one_comparison(rng: &mut Rng) {
    let mut false_alarms = 0;
    let (mut paired_found, mut unpaired_found) = (0, 0);
    for _ in 0..EXPERIMENTS {
        let queries = difficulties(rng, 50);
        let base = scores(rng, &queries, 0.0);
        let equal = scores(rng, &queries, 0.0);
        false_alarms += usize::from(sign_flip_p(&minus(&equal, &base), ROUNDS, rng) < 0.05);

        let better = scores(rng, &queries, 0.05);
        paired_found += usize::from(sign_flip_p(&minus(&better, &base), ROUNDS, rng) < 0.05);
        // thrown-away pairing: compare each query with the next one
        let mut shifted = base.clone();
        shifted.rotate_left(1);
        unpaired_found += usize::from(sign_flip_p(&minus(&better, &shifted), ROUNDS, rng) < 0.05);
    }
    println!(
        "Equal rankers, 50 queries: p < 0.05 in {:.1}% of experiments",
        100.0 * share(false_alarms)
    );
    println!(
        "Real gain of 0.05: found in {:.1}% paired, {:.1}% with the pairs broken",
        100.0 * share(paired_found),
        100.0 * share(unpaired_found)
    );
    assert!((0.03..0.075).contains(&share(false_alarms)));
    assert!(share(paired_found) > 0.75 && share(unpaired_found) < 0.15);
}

/// Equal rankers, tested after every 10 queries up to 200, stopping at the first p < 0.05.
fn simulate_peeking(rng: &mut Rng) {
    const RUNS: usize = 600;
    let (mut ever, mut at_the_end) = (0, 0);
    for _ in 0..RUNS {
        let queries = difficulties(rng, 200);
        let (base, equal) = (scores(rng, &queries, 0.0), scores(rng, &queries, 0.0));
        let differences = minus(&equal, &base);
        let looks: Vec<bool> = (1..=20)
            .map(|look| sign_flip_p(&differences[..10 * look], 199, rng) < 0.05)
            .collect();
        ever += usize::from(looks.contains(&true));
        at_the_end += usize::from(looks[19]);
    }
    let (ever, once) = (ever as f64 / RUNS as f64, at_the_end as f64 / RUNS as f64);
    println!(
        "Peeking every 10 queries up to 200: false alarm in {:.1}%, against {:.1}% with one test at 200",
        100.0 * ever,
        100.0 * once
    );
    assert!(ever > 0.18 && once < 0.08);
}

/// How often the 95% interval holds the true difference, which is 0.05 here.
fn simulate_coverage(rng: &mut Rng) {
    for queries in [8, 50] {
        let covered = (0..EXPERIMENTS)
            .filter(|_| {
                let set = difficulties(rng, queries);
                let base = scores(rng, &set, 0.0);
                let better = scores(rng, &set, 0.05);
                let (low, high) = bootstrap_interval(&minus(&better, &base), 1_000, rng);
                (low..=high).contains(&0.05)
            })
            .count();
        println!(
            "95% bootstrap interval, {queries} queries: holds the truth in {:.1}%",
            100.0 * share(covered)
        );
        if queries == 8 {
            assert!((0.85..0.935).contains(&share(covered)));
        } else {
            assert!((0.925..0.97).contains(&share(covered)));
        }
    }
}

/// Twenty variants, none better than the baseline. Report the one with the highest mean.
fn simulate_many_variants(rng: &mut Rng) {
    const VARIANTS: usize = 20;
    let (mut naive, mut corrected, mut fresh_alarms) = (0, 0, 0);
    let (mut gain_seen, mut gain_fresh) = (0.0, 0.0);
    for _ in 0..EXPERIMENTS {
        let queries = difficulties(rng, 50);
        let base = scores(rng, &queries, 0.0);
        let best = (0..VARIANTS)
            .map(|_| minus(&scores(rng, &queries, 0.0), &base))
            .max_by(|a, b| mean(a).total_cmp(&mean(b)))
            .unwrap();
        // 999 rounds, so that p can go below 0.05 / 20
        let p = sign_flip_p(&best, 999, rng);
        naive += usize::from(mean(&best) > 0.0 && p < 0.05);
        corrected += usize::from(mean(&best) > 0.0 && p < 0.05 / VARIANTS as f64);
        gain_seen += mean(&best);

        // the chosen variant on queries that played no part in choosing it
        let later = difficulties(rng, 50);
        let (base, chosen) = (scores(rng, &later, 0.0), scores(rng, &later, 0.0));
        let difference = minus(&chosen, &base);
        gain_fresh += mean(&difference);
        fresh_alarms +=
            usize::from(mean(&difference) > 0.0 && sign_flip_p(&difference, ROUNDS, rng) < 0.05);
    }
    let (gain_seen, gain_fresh) = (
        gain_seen / EXPERIMENTS as f64,
        gain_fresh / EXPERIMENTS as f64,
    );
    println!("Best of {VARIANTS} equal variants:");
    println!(
        "  'significantly better' in {:.1}%, and {:.1}% when p must be under 0.05/{VARIANTS}",
        100.0 * share(naive),
        100.0 * share(corrected)
    );
    println!(
        "  mean gain {gain_seen:+.3} on the queries used to choose, {gain_fresh:+.3} on new queries"
    );
    println!(
        "  'significantly better' on new queries in {:.1}%",
        100.0 * share(fresh_alarms)
    );
    assert!(share(naive) > 0.12 && share(corrected) < 0.025);
    assert!(gain_seen > 0.015 && gain_fresh.abs() < 0.003);
    assert!(share(fresh_alarms) < 0.05);
}

fn main() {
    check_worked_example();
    check_edges();
    check_against_naive();
    demonstrate_ties();
    let mut rng = Rng(0x5EED_CAFE);
    simulate_one_comparison(&mut rng);
    simulate_coverage(&mut rng);
    simulate_many_variants(&mut rng);
    simulate_peeking(&mut rng);
    println!("All checks passed.");
}
