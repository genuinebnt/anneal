from _c import C
M3J = "19c-outer-joins-and-set-operations"
CH = []

CH.append(C("3j-c1", M3J, "90-challenge-a-hash-outer-join", "build", "Challenge: a hash outer join", "medium", "stages_3j::s3j_c1",
  ["an outer join in linear time", "remembering which build rows were hit", "NULL keys in a hash table"],
  ["outer-joins-and-null-padding", "join-algorithms", "property-testing-and-fuzzing"],
  "`hash_join` in `src/execution/hash_outer_join.rs`: the four join kinds (`Inner`, `Left`, `Right`, `Full`) on an equality of keys, computed with a hash table instead of a nested loop. Rows are `(Option<i64>, i64)`: a join key that may be NULL, and a payload. The answer is a list of `(left payload, right payload)` pairs where `None` is the NULL padding of an unmatched row.",
  "The nested loop of stage 3j-01 does `|left| x |right|` comparisons and is the only choice for an arbitrary condition. For an equality it is the wrong tool: every real engine builds a hash table of one side and probes it with the other. An outer join adds the same bookkeeping as before (a flag per build row), and a NULL key adds the trap that a hash table happily stores a NULL as a key like any other while SQL says it matches nothing.",
  ["Two rows match when both keys are present and equal. A row with a NULL key matches nothing.", "`Inner`: every matching pair. `Left` adds each left row with no partner as `(Some(left), None)`. `Right` adds each right row with no partner as `(None, Some(right))`. `Full` adds both.", "The order of the answer does not matter; duplicates do (a row with three partners appears three times).", "The work is linear in the sizes of the inputs plus the size of the answer: 200 000 rows on each side must finish in a few seconds, which a nested loop cannot do."],
  ["The number of pairs with both sides present is the same for all four kinds.", "A left row appears in the answer at least once for `Left` and `Full`, and exactly as often as it has partners for `Inner` and `Right` (once, padded, if it has none and the kind keeps it)."],
  ["`Left` = `Inner` + the unmatched left rows; `Right` = the pairs of `Left` with the sides swapped; `Full` = `Left` + the unmatched right rows.", "Swapping the two inputs and `Left` for `Right` gives the same pairs with the components swapped."],
  ["left [(1,10),(2,20),(NULL,30)], right [(2,200),(3,300),(NULL,400)], Full -> (20,200) (10,None) (30,None) (None,300) (None,400)", "duplicate keys: left [(1,1)], right [(1,5),(1,6)] -> (1,5) (1,6) for every kind"],
  ["The four kinds against a nested-loop model on random inputs with NULLs and duplicates.", "The relations above.", "Empty inputs.", "A large input under a time limit."],
  src=("src/execution/hash_outer_join.rs", '''
//! Joins on equal keys with a hash table.

use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JoinKind {
    Inner,
    Left,
    Right,
    Full,
}

/// A row: a join key that may be NULL, and a payload.
pub type Row = (Option<i64>, i64);

/// The pairs `(left payload, right payload)` of `left JOIN right ON left.key = right.key`; `None` is NULL padding.
pub fn hash_join(left: &[Row], right: &[Row], kind: JoinKind) -> Vec<(Option<i64>, Option<i64>)> {
    // @begin 3j-c1
    let mut table: HashMap<i64, Vec<usize>> = HashMap::new();
    for (i, (key, _)) in right.iter().enumerate() {
        if let Some(k) = key {
            table.entry(*k).or_default().push(i);
        }
    }
    let mut matched = vec![false; right.len()];
    let mut out = Vec::new();
    for (key, payload) in left {
        match (*key).and_then(|k| table.get(&k)) {
            Some(partners) => {
                for &i in partners {
                    matched[i] = true;
                    out.push((Some(*payload), Some(right[i].1)));
                }
            }
            None => {
                if matches!(kind, JoinKind::Left | JoinKind::Full) {
                    out.push((Some(*payload), None));
                }
            }
        }
    }
    if matches!(kind, JoinKind::Right | JoinKind::Full) {
        for (i, (_, payload)) in right.iter().enumerate() {
            if !matched[i] {
                out.push((None, Some(*payload)));
            }
        }
    }
    out
    //~ let _ = (left, right, kind, HashMap::<i64, usize>::new());
    //~ todo!("3j-c1: build a hash table on the right keys, probe it with the left rows, flag the right rows that were hit")
    // @end
}
'''),
  test=("tests/stages_3j.rs", '''
use super::*;
use bustub::execution::hash_outer_join::{hash_join, JoinKind, Row};

type Pair = (Option<i64>, Option<i64>);

fn model(left: &[Row], right: &[Row], kind: JoinKind) -> Vec<Pair> {
    let mut out = vec![];
    let mut used = vec![false; right.len()];
    for l in left {
        let mut found = false;
        for (i, r) in right.iter().enumerate() {
            if l.0.is_some() && l.0 == r.0 {
                found = true;
                used[i] = true;
                out.push((Some(l.1), Some(r.1)));
            }
        }
        if !found && matches!(kind, JoinKind::Left | JoinKind::Full) {
            out.push((Some(l.1), None));
        }
    }
    if matches!(kind, JoinKind::Right | JoinKind::Full) {
        for (i, r) in right.iter().enumerate() {
            if !used[i] {
                out.push((None, Some(r.1)));
            }
        }
    }
    out.sort();
    out
}

fn sorted(mut v: Vec<Pair>) -> Vec<Pair> {
    v.sort();
    v
}

#[test]
fn s3j_c1_the_worked_example_for_every_kind() {
    let l = [(Some(1), 10), (Some(2), 20), (None, 30)];
    let r = [(Some(2), 200), (Some(3), 300), (None, 400)];
    assert_eq!(sorted(hash_join(&l, &r, JoinKind::Inner)), [(Some(20), Some(200))], "inner");
    assert_eq!(sorted(hash_join(&l, &r, JoinKind::Left)), sorted(vec![(Some(20), Some(200)), (Some(10), None), (Some(30), None)]), "left");
    assert_eq!(sorted(hash_join(&l, &r, JoinKind::Right)), sorted(vec![(Some(20), Some(200)), (None, Some(300)), (None, Some(400))]), "right");
    assert_eq!(hash_join(&l, &r, JoinKind::Full).len(), 5, "full: one pair, two padded left rows, two padded right rows");
}

#[test]
fn s3j_c1_null_keys_match_nothing_but_are_kept() {
    let l = [(None, 1)];
    let r = [(None, 2)];
    assert!(hash_join(&l, &r, JoinKind::Inner).is_empty(), "NULL = NULL is not a match");
    assert_eq!(sorted(hash_join(&l, &r, JoinKind::Full)), sorted(vec![(Some(1), None), (None, Some(2))]), "a FULL join keeps both rows, padded");
}

#[test]
fn s3j_c1_a_row_with_several_partners_appears_once_per_partner() {
    let l = [(Some(1), 1), (Some(1), 2)];
    let r = [(Some(1), 5), (Some(1), 6)];
    for kind in [JoinKind::Inner, JoinKind::Left, JoinKind::Right, JoinKind::Full] {
        assert_eq!(hash_join(&l, &r, kind).len(), 4, "{kind:?}: two left rows times two right rows, nothing padded");
    }
}

#[test]
fn s3j_c1_empty_inputs() {
    let r = [(Some(1), 5)];
    assert!(hash_join(&[], &[], JoinKind::Full).is_empty(), "nothing joined with nothing");
    assert_eq!(hash_join(&[], &r, JoinKind::Full), [(None, Some(5))], "an empty left side: the right rows, padded");
    assert!(hash_join(&[], &r, JoinKind::Left).is_empty(), "a LEFT join of nothing is nothing");
    assert_eq!(hash_join(&r, &[], JoinKind::Left), [(Some(5), None)], "an empty right side: the left rows, padded");
    assert!(hash_join(&r, &[], JoinKind::Inner).is_empty(), "an inner join with nothing is nothing");
}

#[test]
fn s3j_c1_linear_time() {
    let n = 200_000i64;
    let l: Vec<Row> = (0..n).map(|i| (Some(i), i)).collect();
    let r: Vec<Row> = (n / 2..n + n / 2).map(|i| (Some(i), i)).collect();
    let start = std::time::Instant::now();
    let out = hash_join(&l, &r, JoinKind::Full);
    assert_eq!(out.len() as i64, n + n / 2, "half overlap: n/2 pairs, n/2 padded on each side");
    assert!(start.elapsed().as_secs() < 10, "200 000 rows on each side took {:?}: a nested loop cannot do this", start.elapsed());
}

fn arb_rows() -> impl Strategy<Value = Vec<Row>> {
    proptest::collection::vec((prop_oneof![3 => (0i64..5).prop_map(Some), 1 => Just(None)], 0i64..1000), 0..12)
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 80, failure_persistence: None, ..ProptestConfig::default() })]

    #[test]
    fn s3j_c1_property_every_kind_equals_the_nested_loop(l in arb_rows(), r in arb_rows()) {
        for kind in [JoinKind::Inner, JoinKind::Left, JoinKind::Right, JoinKind::Full] {
            prop_assert_eq!(sorted(hash_join(&l, &r, kind)), model(&l, &r, kind), "{:?}", kind);
        }
    }

    #[test]
    fn s3j_c1_property_swapping_the_inputs_swaps_left_and_right(l in arb_rows(), r in arb_rows()) {
        let swapped: Vec<Pair> = hash_join(&r, &l, JoinKind::Left).into_iter().map(|(a, b)| (b, a)).collect();
        prop_assert_eq!(sorted(hash_join(&l, &r, JoinKind::Right)), sorted(swapped));
    }
}
''')))

CH.append(C("3j-c2", M3J, "91-challenge-the-customer-with-one-order", "debug", "Challenge: the customer with one order", "easy", "stages_3j::s3j_c2",
  ["what count(*) counts after an outer join", "count of a column versus count of rows", "reading a query as a table of cases"],
  ["outer-joins-and-null-padding", "sql-types-and-three-valued-logic", "property-testing-and-fuzzing"],
  "`partner_counts` in `src/execution/outer_join_counts.rs` answers the report \"for every customer (left row), how many orders (right rows) have the same key?\" as `select a.id, count(...) from a left join b on a.k = b.k group by a.id`. It uses the right outer join, so customers without orders are listed, and the report shows them with the wrong number. Find the bug and fix it.",
  "The report is the reason outer joins exist, and the wrong number is the most common bug written with them. After a LEFT join a customer with no orders is one row whose order columns are NULL. `count(*)` counts rows, so it says 1; `count(b.id)` counts non-NULL values, so it says 0. Nothing crashes and every customer with at least one order is right, which is why the bug survives review.",
  ["One `(payload, count)` per left row, in the order of the left rows. A left row's count is the number of right rows with an equal key.", "A NULL key has no partners: its count is 0, and it is still listed.", "The right payload is never read for its value, only for its presence."],
  ["Every left row is listed exactly once, whatever its count.", "The counts add up to the number of matching pairs of the inner join."],
  ["Adding a right row with a key no left row has changes nothing.", "Adding a right row with the key of a left row raises that row's count by one and no other."],
  ["left [(1, id 10), (2, id 20)], right [(1, a), (1, b)] -> (10, 2) (20, 0)", "left [(NULL, id 30)], right [(NULL, a)] -> (30, 0)"],
  ["Customers without orders have count 0.", "Customers with several orders.", "NULL keys.", "The invariants and relations on random inputs."],
  src=("src/execution/outer_join_counts.rs", '''
//! The number of partners of each row of the left side, through an outer join.

/// A row: a join key that may be NULL, and a payload.
pub type Row = (Option<i64>, i64);

/// For each left row, `(its payload, the number of right rows with an equal key)`, in the order of the left rows.
pub fn partner_counts(left: &[Row], right: &[Row]) -> Vec<(i64, usize)> {
    // the LEFT join: (index of the left row, payload of its partner or None for the padding)
    let mut joined: Vec<(usize, Option<i64>)> = Vec::new();
    for (i, (key, _)) in left.iter().enumerate() {
        let mut found = false;
        for (rk, rp) in right {
            if key.is_some() && key == rk {
                found = true;
                joined.push((i, Some(*rp)));
            }
        }
        if !found {
            joined.push((i, None));
        }
    }
    let mut counts = vec![0usize; left.len()];
    for (i, partner) in &joined {
        // @begin 3j-c2
        if partner.is_some() {
            counts[*i] += 1;
        }
        //~ let _ = partner;
        //~ counts[*i] += 1;
        // @end
    }
    left.iter().zip(counts).map(|(&(_, payload), n)| (payload, n)).collect()
}
'''),
  test=("tests/stages_3j.rs", '''
use super::*;
use bustub::execution::outer_join_counts::{partner_counts, Row};

fn model(left: &[Row], right: &[Row]) -> Vec<(i64, usize)> {
    left.iter().map(|l| (l.1, right.iter().filter(|r| l.0.is_some() && l.0 == r.0).count())).collect()
}

#[test]
fn s3j_c2_a_customer_without_orders_has_none() {
    let l = [(Some(1), 10), (Some(2), 20)];
    let r = [(Some(1), 1), (Some(1), 2)];
    assert_eq!(partner_counts(&l, &r), [(10, 2), (20, 0)], "customer 20 has no order: 0, not 1");
}

#[test]
fn s3j_c2_every_customer_is_listed_in_order() {
    let l = [(Some(3), 1), (Some(1), 2), (Some(2), 3), (Some(3), 4)];
    let r = [(Some(3), 9), (Some(1), 8)];
    assert_eq!(partner_counts(&l, &r), [(1, 1), (2, 1), (3, 0), (4, 1)], "one entry per left row, in the left order");
}

#[test]
fn s3j_c2_null_keys_have_no_partners() {
    assert_eq!(partner_counts(&[(None, 30)], &[(None, 1)]), [(30, 0)], "NULL matches nothing, not even a NULL");
}

#[test]
fn s3j_c2_no_orders_at_all() {
    let l = [(Some(1), 1), (Some(2), 2)];
    assert_eq!(partner_counts(&l, &[]), [(1, 0), (2, 0)], "an empty right side");
    assert!(partner_counts(&[], &[(Some(1), 1)]).is_empty(), "an empty left side");
}

#[test]
fn s3j_c2_many_orders_for_one_customer() {
    let l = [(Some(7), 1)];
    let r: Vec<Row> = (0..40).map(|i| (Some(7), i)).collect();
    assert_eq!(partner_counts(&l, &r), [(1, 40)], "forty orders");
}

fn arb_rows() -> impl Strategy<Value = Vec<Row>> {
    proptest::collection::vec((prop_oneof![3 => (0i64..4).prop_map(Some), 1 => Just(None)], 0i64..1000), 0..10)
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 80, failure_persistence: None, ..ProptestConfig::default() })]

    #[test]
    fn s3j_c2_property_counts_equal_the_model(l in arb_rows(), r in arb_rows()) {
        prop_assert_eq!(partner_counts(&l, &r), model(&l, &r));
    }

    #[test]
    fn s3j_c2_property_a_new_order_raises_one_count(l in arb_rows(), r in arb_rows(), k in 0i64..4) {
        let before = partner_counts(&l, &r);
        let mut r2 = r.clone();
        r2.push((Some(k), 5000));
        let after = partner_counts(&l, &r2);
        for (i, (b, a)) in before.iter().zip(&after).enumerate() {
            let expected = b.1 + usize::from(l[i].0 == Some(k));
            prop_assert_eq!(a.1, expected, "left row {}", i);
        }
    }
}
''')))

CH.append(C("3j-c3", M3J, "92-challenge-the-union-that-kept-a-duplicate", "debug", "Challenge: the union that kept a duplicate", "easy", "stages_3j::s3j_c3",
  ["the six set operations as functions on lists", "reading code for the one that is wrong", "why dedup is not distinct"],
  ["set-operations-and-bag-arithmetic", "property-testing-and-fuzzing", "model-based-testing"],
  "`src/execution/set_laws.rs` has the six set operations on lists of integers (`union`, `union_all`, `intersect`, `intersect_all`, `except`, `except_all`), each returning a sorted list. Five of them are right. One returns a duplicate for some inputs and passes most examples. Find the bug and fix it.",
  "Set operations are specified by counts, and an implementation by sorting and scanning is short enough to look obviously right. The bugs that survive are the ones that are right for the inputs you thought of: most small hand-picked examples come in sorted, or have duplicates next to each other, and the failing case is the one that does not. A property that states the law (\"the result has no duplicates\") finds it in a second.",
  ["`union_all`: every element of both lists. `union`: every distinct element of either.", "`intersect_all`: each element as often as the smaller of its counts; `intersect`: each element that is in both, once.", "`except_all`: each element as often as its count on the left exceeds its count on the right; `except`: each element that is on the left and not on the right, once.", "Every result is sorted ascending."],
  ["`union`, `intersect` and `except` never contain a duplicate.", "No element appears in a result more often than the operation allows (the counts of the contract)."],
  ["`union(l, r)` equals the distinct elements of `union_all(l, r)`.", "`union(l, r) == union(r, l)`, and the same for `intersect`.", "`intersect_all` and `except_all` of the same pair add up to the left list."],
  ["union([1, 2, 1], [2]) -> [1, 2]", "union([3, 1], [2, 3, 1]) -> [1, 2, 3]", "except_all([1, 1, 1, 2], [1, 1, 3]) -> [1, 2]"],
  ["Hand-picked cases, including unsorted inputs.", "The laws above on random lists."],
  src=("src/execution/set_laws.rs", '''
//! The six set operations on lists of integers; the results are sorted.

use std::collections::BTreeMap;

fn counts(rows: &[i64]) -> BTreeMap<i64, usize> {
    let mut m = BTreeMap::new();
    for r in rows {
        *m.entry(*r).or_insert(0) += 1;
    }
    m
}

fn combine(l: &[i64], r: &[i64], f: impl Fn(usize, usize) -> usize) -> Vec<i64> {
    let (a, b) = (counts(l), counts(r));
    let keys: std::collections::BTreeSet<i64> = a.keys().chain(b.keys()).copied().collect();
    let mut out = Vec::new();
    for k in keys {
        let n = f(a.get(&k).copied().unwrap_or(0), b.get(&k).copied().unwrap_or(0));
        out.extend(std::iter::repeat(k).take(n));
    }
    out
}

pub fn union_all(l: &[i64], r: &[i64]) -> Vec<i64> {
    let mut v: Vec<i64> = l.iter().chain(r).copied().collect();
    v.sort();
    v
}

pub fn union(l: &[i64], r: &[i64]) -> Vec<i64> {
    // @begin 3j-c3
    let mut v: Vec<i64> = l.iter().chain(r).copied().collect();
    v.sort();
    v.dedup();
    v
    //~ let mut v: Vec<i64> = l.iter().chain(r).copied().collect();
    //~ v.dedup();
    //~ v.sort();
    //~ v
    // @end
}

pub fn intersect_all(l: &[i64], r: &[i64]) -> Vec<i64> {
    combine(l, r, |a, b| a.min(b))
}

pub fn intersect(l: &[i64], r: &[i64]) -> Vec<i64> {
    combine(l, r, |a, b| usize::from(a > 0 && b > 0))
}

pub fn except_all(l: &[i64], r: &[i64]) -> Vec<i64> {
    combine(l, r, |a, b| a.saturating_sub(b))
}

pub fn except(l: &[i64], r: &[i64]) -> Vec<i64> {
    combine(l, r, |a, b| usize::from(a > 0 && b == 0))
}
'''),
  test=("tests/stages_3j.rs", '''
use super::*;
use bustub::execution::set_laws::*;

fn distinct(mut v: Vec<i64>) -> Vec<i64> {
    v.sort();
    v.dedup();
    v
}

#[test]
fn s3j_c3_union_of_sorted_input() {
    assert_eq!(union(&[1, 2, 3], &[2, 3, 4]), [1, 2, 3, 4], "sorted inputs");
    assert_eq!(union(&[], &[]), Vec::<i64>::new(), "nothing");
}

#[test]
fn s3j_c3_union_of_unsorted_input() {
    assert_eq!(union(&[1, 2, 1], &[2]), [1, 2], "a duplicate that is not next to its twin");
    assert_eq!(union(&[3, 1], &[2, 3, 1]), [1, 2, 3], "after the concatenation the equal values are far apart");
}

#[test]
fn s3j_c3_the_other_five_operations() {
    let (l, r) = ([1, 1, 1, 2], [1, 1, 3]);
    assert_eq!(union_all(&l, &r).len(), 7);
    assert_eq!(intersect(&l, &r), [1]);
    assert_eq!(intersect_all(&l, &r), [1, 1]);
    assert_eq!(except(&l, &r), [2]);
    assert_eq!(except_all(&l, &r), [1, 2]);
}

#[test]
fn s3j_c3_union_with_itself_is_the_distinct_rows() {
    assert_eq!(union(&[5, 3, 5, 3, 1], &[5, 3, 5, 3, 1]), [1, 3, 5]);
}

#[test]
fn s3j_c3_results_are_sorted_and_duplicates_free() {
    let u = union(&[9, 7, 9, 8, 7], &[8, 9, 6, 6]);
    assert_eq!(u, [6, 7, 8, 9]);
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 120, failure_persistence: None, ..ProptestConfig::default() })]

    #[test]
    fn s3j_c3_property_union_is_the_distinct_union_all(l in proptest::collection::vec(0i64..6, 0..10), r in proptest::collection::vec(0i64..6, 0..10)) {
        prop_assert_eq!(union(&l, &r), distinct(union_all(&l, &r)));
        prop_assert_eq!(union(&l, &r), union(&r, &l));
    }

    #[test]
    fn s3j_c3_property_intersect_and_except_split_the_left(l in proptest::collection::vec(0i64..6, 0..10), r in proptest::collection::vec(0i64..6, 0..10)) {
        let mut both = intersect_all(&l, &r);
        both.extend(except_all(&l, &r));
        both.sort();
        let mut want = l.clone();
        want.sort();
        prop_assert_eq!(both, want);
        let mut s = intersect(&l, &r);
        s.extend(except(&l, &r));
        s.sort();
        prop_assert_eq!(s, distinct(l.clone()));
        prop_assert_eq!(intersect(&l, &r), intersect(&r, &l));
    }
}
''')))

CH.append(C("3j-c4", M3J, "93-challenge-set-operations-by-merging", "extend", "Challenge: set operations by merging", "hard", "stages_3j::s3j_c4",
  ["a streaming set operation on sorted input", "constant memory and laziness", "run-length counting instead of a hash table"],
  ["set-operations-and-bag-arithmetic", "iterators-and-closures", "external-merge-sort"],
  "`merge_set_op` in `src/execution/merge_set_ops.rs`: `UNION`, `INTERSECT` and `EXCEPT`, with and without `ALL`, on two **sorted** streams of integers, returning a lazy iterator that is also sorted. Where stage 3j-05 hashed whole rows, this one walks both inputs once, counting each run of equal values.",
  "A hash table of every distinct row is the right answer when the input is unordered and fits in memory. When both inputs are already sorted (they came from an index, or from an external sort that spilled to disk), the same operations need no table at all: look at the heads, take the smaller value, count its run on each side, and decide with the six formulas how many copies to emit. Memory is constant and the first row arrives before the inputs are read, which is what makes `LIMIT 10` on a billion-row union cheap.",
  ["`merge_set_op(left, right, op, all)` takes two iterators over `i64`, each in ascending order (duplicates adjacent), and returns an iterator.", "The output is in ascending order and has, for each value, the number of copies the operation and `all` give from the counts `na` and `nb` of that value.", "The returned iterator is lazy: asking for the first item reads a bounded number of items from the inputs, and it works on infinite inputs.", "It stores no more than a few numbers: no `Vec` of the inputs."],
  ["The output is sorted.", "An input is read at most once, and never beyond the run of the value being counted plus one item."],
  ["For `all = false` the output equals the output with `all = true` with duplicates removed (for `Union`, `Intersect`, and `Except` after taking `min(na, 1)`-style logic, as in stage 3j-05).", "`Union` with `all = true` is the merge of the two inputs."],
  ["Union all of [1, 1, 3] and [1, 2] -> [1, 1, 1, 2, 3]", "Intersect all of [1, 1, 1, 2] and [1, 1, 3] -> [1, 1]", "Except all of the same pair -> [1, 2]; Except of the same pair -> [2]"],
  ["The six operations against a counting model on random sorted inputs.", "Laziness on infinite inputs.", "A bound on how many items are pulled to produce the first output."],
  src=("src/execution/merge_set_ops.rs", '''
//! Set operations on sorted streams, by merging.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Op {
    Union,
    Intersect,
    Except,
}

/// `left op [ALL] right` for two ascending iterators; the result is ascending and lazy.
pub fn merge_set_op<'a>(left: impl Iterator<Item = i64> + 'a, right: impl Iterator<Item = i64> + 'a, op: Op, all: bool) -> Box<dyn Iterator<Item = i64> + 'a> {
    // @begin 3j-c4
    struct Merge<L: Iterator<Item = i64>, R: Iterator<Item = i64>> {
        l: std::iter::Peekable<L>,
        r: std::iter::Peekable<R>,
        op: Op,
        all: bool,
        /// A value still to be handed out, and how many more copies.
        pending: (i64, usize),
    }
    impl<L: Iterator<Item = i64>, R: Iterator<Item = i64>> Iterator for Merge<L, R> {
        type Item = i64;
        fn next(&mut self) -> Option<i64> {
            loop {
                if self.pending.1 > 0 {
                    self.pending.1 -= 1;
                    return Some(self.pending.0);
                }
                let key = match (self.l.peek().copied(), self.r.peek().copied()) {
                    (None, None) => return None,
                    (Some(a), None) => a,
                    (None, Some(b)) => b,
                    (Some(a), Some(b)) => a.min(b),
                };
                let (mut na, mut nb) = (0usize, 0usize);
                while self.l.peek() == Some(&key) {
                    self.l.next();
                    na += 1;
                }
                while self.r.peek() == Some(&key) {
                    self.r.next();
                    nb += 1;
                }
                let n = match (self.op, self.all) {
                    (Op::Union, true) => na + nb,
                    (Op::Union, false) => 1,
                    (Op::Intersect, true) => na.min(nb),
                    (Op::Intersect, false) => usize::from(na > 0 && nb > 0),
                    (Op::Except, true) => na.saturating_sub(nb),
                    (Op::Except, false) => usize::from(na > 0 && nb == 0),
                };
                self.pending = (key, n);
            }
        }
    }
    Box::new(Merge { l: left.peekable(), r: right.peekable(), op, all, pending: (0, 0) })
    //~ let _ = (left, right, op, all);
    //~ todo!("3j-c4: peek both heads, take the smaller value, count its run on each side, emit as many copies as the operation says")
    // @end
}
'''),
  test=("tests/stages_3j.rs", '''
use super::*;
use bustub::execution::merge_set_ops::{merge_set_op, Op};
use std::cell::Cell as Counter;
use std::rc::Rc;

fn run(l: &[i64], r: &[i64], op: Op, all: bool) -> Vec<i64> {
    merge_set_op(l.to_vec().into_iter(), r.to_vec().into_iter(), op, all).collect()
}

fn model(l: &[i64], r: &[i64], op: Op, all: bool) -> Vec<i64> {
    let mut keys: Vec<i64> = l.iter().chain(r).copied().collect();
    keys.sort();
    keys.dedup();
    let mut out = vec![];
    for k in keys {
        let (na, nb) = (l.iter().filter(|x| **x == k).count(), r.iter().filter(|x| **x == k).count());
        let n = match (op, all) {
            (Op::Union, true) => na + nb,
            (Op::Union, false) => 1,
            (Op::Intersect, true) => na.min(nb),
            (Op::Intersect, false) => usize::from(na > 0 && nb > 0),
            (Op::Except, true) => na.saturating_sub(nb),
            (Op::Except, false) => usize::from(na > 0 && nb == 0),
        };
        out.extend(std::iter::repeat(k).take(n));
    }
    out
}

#[test]
fn s3j_c4_the_worked_examples() {
    assert_eq!(run(&[1, 1, 3], &[1, 2], Op::Union, true), [1, 1, 1, 2, 3]);
    assert_eq!(run(&[1, 1, 1, 2], &[1, 1, 3], Op::Intersect, true), [1, 1]);
    assert_eq!(run(&[1, 1, 1, 2], &[1, 1, 3], Op::Except, true), [1, 2]);
    assert_eq!(run(&[1, 1, 1, 2], &[1, 1, 3], Op::Except, false), [2]);
    assert_eq!(run(&[1, 1, 1, 2], &[1, 1, 3], Op::Union, false), [1, 2, 3]);
}

#[test]
fn s3j_c4_empty_sides() {
    for op in [Op::Union, Op::Intersect, Op::Except] {
        for all in [false, true] {
            assert_eq!(run(&[], &[], op, all), Vec::<i64>::new(), "{op:?} {all}");
            assert_eq!(run(&[1, 2, 2], &[], op, all), model(&[1, 2, 2], &[], op, all), "{op:?} {all}: empty right");
            assert_eq!(run(&[], &[1, 2, 2], op, all), model(&[], &[1, 2, 2], op, all), "{op:?} {all}: empty left");
        }
    }
}

#[test]
fn s3j_c4_it_works_on_infinite_inputs_because_it_is_lazy() {
    let evens = (0i64..).map(|x| x * 2);
    let odds = (0i64..).map(|x| x * 2 + 1);
    let first: Vec<i64> = merge_set_op(evens, odds, Op::Union, true).take(6).collect();
    assert_eq!(first, [0, 1, 2, 3, 4, 5], "the first six of an infinite union, without reading to the end");
    let threes = (0i64..).map(|x| x * 3);
    let twos = (0i64..).map(|x| x * 2);
    let both: Vec<i64> = merge_set_op(threes, twos, Op::Intersect, false).take(4).collect();
    assert_eq!(both, [0, 6, 12, 18], "multiples of both 2 and 3");
}

#[test]
fn s3j_c4_the_first_item_reads_a_bounded_number_of_inputs() {
    let (pulled_l, pulled_r) = (Rc::new(Counter::new(0usize)), Rc::new(Counter::new(0usize)));
    let (a, b) = (pulled_l.clone(), pulled_r.clone());
    let left = (0i64..1_000_000).inspect(move |_| a.set(a.get() + 1));
    let right = (0i64..1_000_000).inspect(move |_| b.set(b.get() + 1));
    let mut it = merge_set_op(left, right, Op::Union, true);
    assert_eq!(it.next(), Some(0));
    assert!(pulled_l.get() <= 3 && pulled_r.get() <= 3, "one output item pulled {} and {} input items: it must not read ahead", pulled_l.get(), pulled_r.get());
}

#[test]
fn s3j_c4_long_runs_are_counted_not_stored() {
    let l = std::iter::repeat(5i64).take(100_000);
    let r = std::iter::repeat(5i64).take(99_990);
    let diff: Vec<i64> = merge_set_op(l, r, Op::Except, true).collect();
    assert_eq!(diff.len(), 10, "100000 - 99990 copies");
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 100, failure_persistence: None, ..ProptestConfig::default() })]

    #[test]
    fn s3j_c4_property_all_six_equal_the_counting_model(mut l in proptest::collection::vec(0i64..6, 0..12), mut r in proptest::collection::vec(0i64..6, 0..12)) {
        l.sort();
        r.sort();
        for op in [Op::Union, Op::Intersect, Op::Except] {
            for all in [false, true] {
                prop_assert_eq!(run(&l, &r, op, all), model(&l, &r, op, all), "{:?} all={}", op, all);
            }
        }
    }
}
''')))

CH.append(C("3j-c5", M3J, "94-challenge-which-filters-reject-nulls", "extend", "Challenge: which filters reject NULLs", "hard", "stages_3j::s3j_c5",
  ["an analysis that is sound and as exact as it can be", "three-valued logic through NOT, AND and OR", "abstract interpretation with a set of possible values"],
  ["filters-above-and-below-outer-joins", "sql-types-and-three-valued-logic", "expression-trees"],
  "`rejects_nulls` in `src/execution/null_rejection.rs`, on a small expression language (columns, integer literals, `COALESCE`, `>`, `=`, `IS NULL`, `IS NOT NULL`, `AND`, `OR`, `NOT`): given the columns that are NULL on a padded row, say whether the condition is **certainly not TRUE** for it, whatever the other columns hold. Stage 3j-03 answered \"no\" for `NOT`, `COALESCE` and constants; this one answers as precisely as the language allows.",
  "A \"yes\" turns an outer join into an inner join, so it must never be wrong: that is *soundness*, and a brute-force check over small values proves it. A \"no\" only costs an optimization, and the more conditions that get a correct \"yes\", the more joins are simplified: that is *precision*. The way to get both is to evaluate the condition not on values but on **sets of possible outcomes**: the set of truth values a sub-condition can take, given that some columns are NULL and the others are unknown.",
  ["`eval` (given) evaluates a condition on a row of `Option<i64>`, with three-valued logic: the result is `Some(true)`, `Some(false)` or `None` for unknown.", "`rejects_nulls(cond, nulls)`: `nulls` lists the columns that are NULL; every other column may hold any integer or NULL.", "Return `true` only if `eval` is never `Some(true)` for any such row (soundness).", "Return `true` for every condition where this holds and no column appears twice in it (precision on the common case); a condition that repeats a column may get a cautious `false`."],
  ["Soundness: if `rejects_nulls` is `true`, no assignment of the other columns makes the condition true.", "The answer depends only on the condition and on which columns are NULL."],
  ["`NOT` flips true and false and keeps unknown: `NOT (c0 > 3)` rejects when c0 is NULL, `NOT (c0 IS NULL)` rejects when c0 is NULL, `NOT (c0 IS NOT NULL)` does not.", "`AND` rejects if either side does; `OR` only if both do; `COALESCE(c0, 5) > 3` does not reject when c0 is NULL (5 > 3), but `COALESCE(c0, 1) > 3` does.", "Making more columns NULL never turns a `true` into `false` for a condition without `IS NULL`."],
  ["c0 > 3, nulls [0] -> true", "NOT (c0 > 3), nulls [0] -> true", "c0 IS NULL, nulls [0] -> false", "COALESCE(c0, 5) > 3, nulls [0] -> false; COALESCE(c0, 1) > 3 -> true", "(c0 > 3) OR (c1 > 3), nulls [0] -> false; nulls [0, 1] -> true"],
  ["A list of conditions with the expected answers.", "Soundness against a brute-force evaluation, on random conditions.", "Precision on random conditions with no repeated column."],
  src=("src/execution/null_rejection.rs", '''
//! Does a condition fail on a row that an outer join padded with NULLs?

/// An integer-valued expression.
#[derive(Debug, Clone, PartialEq)]
pub enum S {
    Col(usize),
    Lit(i64),
    Coalesce(Box<S>, Box<S>),
}

/// A condition.
#[derive(Debug, Clone, PartialEq)]
pub enum B {
    Gt(S, S),
    Eq(S, S),
    IsNull(S),
    IsNotNull(S),
    And(Box<B>, Box<B>),
    Or(Box<B>, Box<B>),
    Not(Box<B>),
}

pub fn eval_s(s: &S, row: &[Option<i64>]) -> Option<i64> {
    match s {
        S::Col(i) => row[*i],
        S::Lit(v) => Some(*v),
        S::Coalesce(a, b) => eval_s(a, row).or_else(|| eval_s(b, row)),
    }
}

/// Three-valued evaluation: `None` is unknown.
pub fn eval(b: &B, row: &[Option<i64>]) -> Option<bool> {
    match b {
        B::Gt(x, y) => Some(eval_s(x, row)? > eval_s(y, row)?),
        B::Eq(x, y) => Some(eval_s(x, row)? == eval_s(y, row)?),
        B::IsNull(x) => Some(eval_s(x, row).is_none()),
        B::IsNotNull(x) => Some(eval_s(x, row).is_some()),
        B::And(x, y) => match (eval(x, row), eval(y, row)) {
            (Some(false), _) | (_, Some(false)) => Some(false),
            (Some(true), Some(true)) => Some(true),
            _ => None,
        },
        B::Or(x, y) => match (eval(x, row), eval(y, row)) {
            (Some(true), _) | (_, Some(true)) => Some(true),
            (Some(false), Some(false)) => Some(false),
            _ => None,
        },
        B::Not(x) => eval(x, row).map(|v| !v),
    }
}

/// Is `cond` certainly not TRUE when the columns in `nulls` are NULL, whatever the other columns hold?
pub fn rejects_nulls(cond: &B, nulls: &[usize]) -> bool {
    // @begin 3j-c5
    // what is known about an integer expression
    #[derive(Clone, Copy, PartialEq)]
    enum A {
        Null,
        Const(i64),
        Any,
        AnyOrNull,
    }
    // truth values that may occur: bit 1 = true, 2 = false, 4 = unknown
    fn abs_s(s: &S, nulls: &[usize]) -> A {
        match s {
            S::Col(i) => {
                if nulls.contains(i) {
                    A::Null
                } else {
                    A::AnyOrNull
                }
            }
            S::Lit(v) => A::Const(*v),
            S::Coalesce(a, b) => match (abs_s(a, nulls), abs_s(b, nulls)) {
                (A::Null, b) => b,
                (a @ (A::Const(_) | A::Any), _) => a,
                (A::AnyOrNull, A::AnyOrNull | A::Null) => A::AnyOrNull,
                (A::AnyOrNull, _) => A::Any,
            },
        }
    }
    fn cmp(a: A, b: A, eq: bool) -> u8 {
        match (a, b) {
            (A::Null, _) | (_, A::Null) => 4,
            (A::Const(x), A::Const(y)) => {
                if (eq && x == y) || (!eq && x > y) {
                    1
                } else {
                    2
                }
            }
            _ => 3 | if a == A::AnyOrNull || b == A::AnyOrNull { 4 } else { 0 },
        }
    }
    fn lift(x: u8, y: u8, f: impl Fn(u8, u8) -> u8) -> u8 {
        let mut out = 0;
        for a in [1u8, 2, 4] {
            for b in [1u8, 2, 4] {
                if x & a != 0 && y & b != 0 {
                    out |= f(a, b);
                }
            }
        }
        out
    }
    fn go(b: &B, nulls: &[usize]) -> u8 {
        match b {
            B::Gt(x, y) => cmp(abs_s(x, nulls), abs_s(y, nulls), false),
            B::Eq(x, y) => cmp(abs_s(x, nulls), abs_s(y, nulls), true),
            B::IsNull(x) => match abs_s(x, nulls) {
                A::Null => 1,
                A::AnyOrNull => 3,
                _ => 2,
            },
            B::IsNotNull(x) => match abs_s(x, nulls) {
                A::Null => 2,
                A::AnyOrNull => 3,
                _ => 1,
            },
            B::And(x, y) => lift(go(x, nulls), go(y, nulls), |a, b| if a == 2 || b == 2 { 2 } else if a == 4 || b == 4 { 4 } else { 1 }),
            B::Or(x, y) => lift(go(x, nulls), go(y, nulls), |a, b| if a == 1 || b == 1 { 1 } else if a == 4 || b == 4 { 4 } else { 2 }),
            B::Not(x) => {
                let v = go(x, nulls);
                (if v & 1 != 0 { 2 } else { 0 }) | (if v & 2 != 0 { 1 } else { 0 }) | (v & 4)
            }
        }
    }
    go(cond, nulls) & 1 == 0
    //~ let _ = (cond, nulls);
    //~ false
    // @end
}
'''),
  test=("tests/stages_3j.rs", '''
use super::*;
use bustub::execution::null_rejection::{eval, rejects_nulls, B, S};

fn c(i: usize) -> S {
    S::Col(i)
}
fn l(v: i64) -> S {
    S::Lit(v)
}
fn co(a: S, b: S) -> S {
    S::Coalesce(Box::new(a), Box::new(b))
}
fn not(b: B) -> B {
    B::Not(Box::new(b))
}
fn and(a: B, b: B) -> B {
    B::And(Box::new(a), Box::new(b))
}
fn or(a: B, b: B) -> B {
    B::Or(Box::new(a), Box::new(b))
}

/// Can the condition be TRUE for some values of the columns that are not in `nulls`? (Brute force over a small domain.)
fn can_be_true(b: &B, nulls: &[usize], ncols: usize) -> bool {
    let domain = [None, Some(-1), Some(0), Some(1), Some(2), Some(3), Some(4)];
    let free: Vec<usize> = (0..ncols).filter(|i| !nulls.contains(i)).collect();
    let total = domain.len().pow(free.len() as u32);
    for mut n in 0..total {
        let mut row = vec![None; ncols];
        for &i in &free {
            row[i] = domain[n % domain.len()];
            n /= domain.len();
        }
        if eval(b, &row) == Some(true) {
            return true;
        }
    }
    false
}

#[test]
fn s3j_c5_comparisons_and_null_tests() {
    assert!(rejects_nulls(&B::Gt(c(0), l(3)), &[0]), "NULL > 3 is unknown");
    assert!(!rejects_nulls(&B::Gt(c(0), l(3)), &[1]), "c0 is not NULL: it can be 5");
    assert!(rejects_nulls(&B::Eq(c(0), c(1)), &[1]), "x = NULL is unknown");
    assert!(!rejects_nulls(&B::IsNull(c(0)), &[0]), "NULL IS NULL is true");
    assert!(rejects_nulls(&B::IsNotNull(c(0)), &[0]), "NULL IS NOT NULL is false");
}

#[test]
fn s3j_c5_not_and_or() {
    assert!(rejects_nulls(&not(B::Gt(c(0), l(3))), &[0]), "NOT unknown is unknown");
    assert!(rejects_nulls(&not(B::IsNull(c(0))), &[0]), "NOT (c0 IS NULL) is c0 IS NOT NULL");
    assert!(!rejects_nulls(&not(B::IsNotNull(c(0))), &[0]), "NOT (c0 IS NOT NULL) is c0 IS NULL: true");
    let p = or(B::Gt(c(0), l(3)), B::Gt(c(1), l(3)));
    assert!(!rejects_nulls(&p, &[0]), "OR: the other side can be true");
    assert!(rejects_nulls(&p, &[0, 1]), "OR: both sides unknown");
    assert!(rejects_nulls(&and(B::Gt(c(0), l(3)), B::Gt(c(1), l(3))), &[0]), "AND: one unknown side is enough");
    assert!(rejects_nulls(&not(and(B::Gt(c(0), l(3)), B::Gt(c(1), l(3)))), &[0, 1]), "NOT (unknown AND unknown)");
    assert!(!rejects_nulls(&not(and(B::Gt(c(0), l(3)), B::Gt(c(1), l(3)))), &[0]), "NOT (unknown AND c1 > 3): when c1 <= 3 the AND is false and the NOT true");
}

#[test]
fn s3j_c5_coalesce_and_constants() {
    assert!(!rejects_nulls(&B::Gt(co(c(0), l(5)), l(3)), &[0]), "COALESCE(NULL, 5) > 3 is true");
    assert!(rejects_nulls(&B::Gt(co(c(0), l(1)), l(3)), &[0]), "COALESCE(NULL, 1) > 3 is false");
    assert!(!rejects_nulls(&B::Gt(co(c(0), c(1)), l(3)), &[0]), "the second column can be big");
    assert!(rejects_nulls(&B::Gt(co(c(0), c(1)), l(3)), &[0, 1]), "both NULL");
    assert!(rejects_nulls(&B::Eq(l(1), l(2)), &[]), "a constant condition that is false rejects every row");
    assert!(!rejects_nulls(&B::Eq(l(2), l(2)), &[]), "and a true one rejects none");
}

fn arb_s(ncols: usize) -> impl Strategy<Value = S> {
    let leaf = prop_oneof![(0..ncols).prop_map(S::Col), (0i64..3).prop_map(S::Lit)];
    leaf.prop_recursive(2, 6, 2, |inner| (inner.clone(), inner).prop_map(|(a, b)| S::Coalesce(Box::new(a), Box::new(b))))
}

fn arb_b(ncols: usize) -> impl Strategy<Value = B> {
    let leaf = prop_oneof![
        (arb_s(ncols), arb_s(ncols)).prop_map(|(a, b)| B::Gt(a, b)),
        (arb_s(ncols), arb_s(ncols)).prop_map(|(a, b)| B::Eq(a, b)),
        arb_s(ncols).prop_map(B::IsNull),
        arb_s(ncols).prop_map(B::IsNotNull),
    ];
    leaf.prop_recursive(3, 12, 2, |inner| {
        prop_oneof![
            (inner.clone(), inner.clone()).prop_map(|(a, b)| B::And(Box::new(a), Box::new(b))),
            (inner.clone(), inner.clone()).prop_map(|(a, b)| B::Or(Box::new(a), Box::new(b))),
            inner.prop_map(|a| B::Not(Box::new(a))),
        ]
    })
}

fn columns(b: &B, out: &mut Vec<usize>) {
    fn s(x: &S, out: &mut Vec<usize>) {
        match x {
            S::Col(i) => out.push(*i),
            S::Lit(_) => {}
            S::Coalesce(a, b) => {
                s(a, out);
                s(b, out);
            }
        }
    }
    match b {
        B::Gt(x, y) | B::Eq(x, y) => {
            s(x, out);
            s(y, out);
        }
        B::IsNull(x) | B::IsNotNull(x) => s(x, out),
        B::And(x, y) | B::Or(x, y) => {
            columns(x, out);
            columns(y, out);
        }
        B::Not(x) => columns(x, out),
    }
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 300, max_global_rejects: 100_000, failure_persistence: None, ..ProptestConfig::default() })]

    #[test]
    fn s3j_c5_property_a_yes_is_always_right(cond in arb_b(3), mask in 0u8..8) {
        let nulls: Vec<usize> = (0..3).filter(|i| mask & (1 << i) != 0).collect();
        if rejects_nulls(&cond, &nulls) {
            prop_assert!(!can_be_true(&cond, &nulls, 3), "{:?} with NULL columns {:?} can be true", cond, nulls);
        }
    }

    #[test]
    fn s3j_c5_property_it_finds_every_rejection_when_no_column_repeats(cond in arb_b(3), mask in 0u8..8) {
        let mut cols = vec![];
        columns(&cond, &mut cols);
        let mut uniq = cols.clone();
        uniq.sort();
        uniq.dedup();
        prop_assume!(uniq.len() == cols.len());
        let nulls: Vec<usize> = (0..3).filter(|i| mask & (1 << i) != 0).collect();
        prop_assert_eq!(rejects_nulls(&cond, &nulls), !can_be_true(&cond, &nulls, 3), "{:?} with NULL columns {:?}", cond, nulls);
    }
}
''')))
