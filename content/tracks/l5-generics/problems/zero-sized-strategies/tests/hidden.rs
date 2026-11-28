use solution::*;

/// Ordered by priority only, so equal-priority jobs are distinguishable.
#[derive(Debug, PartialEq, Eq)]
struct Job(u8, &'static str);

impl PartialOrd for Job {
    fn partial_cmp(&self, o: &Job) -> Option<std::cmp::Ordering> {
        Some(self.cmp(o))
    }
}

impl Ord for Job {
    fn cmp(&self, o: &Job) -> std::cmp::Ordering {
        self.0.cmp(&o.0)
    }
}

/// A test-defined strategy: everything is equal.
#[allow(dead_code)]
struct Unordered;

impl Order for Unordered {
    fn cmp<T: Ord>(_: &T, _: &T) -> std::cmp::Ordering {
        std::cmp::Ordering::Equal
    }
}

fn names<O: Order>(v: &SortedVec<Job, O>) -> Vec<&'static str> {
    v.as_slice().iter().map(|j| j.1).collect()
}

#[test]
fn contains() {
    let v: SortedVec<i32> = vec![9, 1, 5].into_iter().collect();
    check!(r#"[1, 5, 9] contains 5 and 6"#, (v.contains(&5), v.contains(&6)), (true, false));
}

#[test]
fn contains_desc() {
    let v: SortedVec<i32, Desc> = vec![9, 1, 5].into_iter().collect();
    check!(r#"Desc [1, 5, 9] contains 1 and 0"#, (v.contains(&1), v.contains(&0)), (true, false));
}

#[test]
fn empty() {
    let v: SortedVec<String> = SortedVec::new();
    check!(r#"new SortedVec<String>"#, (v.as_slice().len(), v.contains(&String::new())), (0, false));
}

#[test]
fn strings() {
    let mut v = SortedVec::<&str>::new();
    for w in ["pear", "Apple", "fig"] {
        v.insert(w);
    }
    check!(r#"insert "pear", "Apple", "fig""#, v.as_slice(), ["Apple", "fig", "pear"]);
}

#[test]
fn your_own_strategy() {
    let v: SortedVec<Job, Unordered> = vec![Job(2, "x"), Job(0, "y"), Job(1, "z")].into_iter().collect();
    check!(r#"Unordered keeps insertion order"#, names(&v), vec!["x", "y", "z"]);
}

#[test]
fn reorder_back_and_forth() {
    let v: SortedVec<i32> = vec![2, 1, 3].into_iter().collect();
    check!(r#"[2, 1, 3] asc -> desc -> asc"#, v.reorder::<Desc>().reorder::<Asc>().as_slice().to_vec(), vec![1, 2, 3]);
}

#[test]
fn duplicates_in_desc() {
    let v: SortedVec<Job, Desc> = vec![Job(0, "a"), Job(1, "b"), Job(0, "c")].into_iter().collect();
    check!(r#"Desc jobs (0, a), (1, b), (0, c)"#, names(&v), vec!["b", "a", "c"]);
}

#[test]
fn zero_sized_strategies() {
    check!(r#"size_of Asc, Desc, Unordered"#, (std::mem::size_of::<Asc>(), std::mem::size_of::<Desc>(), std::mem::size_of::<Unordered>()), (0, 0, 0));
}

#[test]
fn extremes() {
    let v: SortedVec<i64> = vec![i64::MAX, i64::MIN, 0].into_iter().collect();
    check!(r#"i64::MIN, i64::MAX, 0"#, v.as_slice(), [i64::MIN, 0, i64::MAX]);
}

#[test]
fn random_vs_stable_sort() {
    let mut rng = anneal_prelude::Rng::new(4513);
    let tags = ["a", "b", "c", "d", "e", "f", "g", "h"];
    for _ in 0..300 {
        let n = rng.below(8);
        let pris: Vec<u8> = rng.vec(n, 0, 3);
        let jobs = || pris.iter().zip(tags).map(|(&p, t)| Job(p, t));
        let asc: SortedVec<Job> = jobs().collect();
        let desc: SortedVec<Job, Desc> = jobs().collect();
        let mut want_asc: Vec<Job> = jobs().collect();
        want_asc.sort_by(|a, b| a.0.cmp(&b.0));
        let mut want_desc: Vec<Job> = jobs().collect();
        want_desc.sort_by(|a, b| b.0.cmp(&a.0));
        let probe = Job(rng.int(0, 4) as u8, "?");
        let has = pris.contains(&probe.0);
        check!(format!("priorities = {pris:?}, probe = {}", probe.0), (names(&asc), names(&desc), asc.contains(&probe), desc.contains(&probe)),
               (want_asc.iter().map(|j| j.1).collect::<Vec<_>>(), want_desc.iter().map(|j| j.1).collect::<Vec<_>>(), has, has));
    }
}

#[test]
fn scale_insert_and_lookup() {
    let n = 200_000u64;
    let v: SortedVec<u64> = (0..n).map(|x| x * 2).collect();
    let found = (0..2 * n).filter(|x| v.contains(x)).count();
    check!("200000 sorted inserts, 400000 lookups", (v.as_slice().len(), found), (200_000, 200_000));
}

std::thread_local! {
    static COMPARISONS: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}

/// Ascending, and counts every comparison the SortedVec asks for.
struct Counting;

impl Order for Counting {
    fn cmp<T: Ord>(a: &T, b: &T) -> std::cmp::Ordering {
        COMPARISONS.with(|c| c.set(c.get() + 1));
        a.cmp(b)
    }
}

#[test]
fn comparisons_grow_with_log_n() {
    let mut rng = anneal_prelude::Rng::new(4520);
    let n = 2048u64;
    let xs: Vec<i64> = (0..n).map(|_| rng.int(0, 1_000_000)).collect();
    COMPARISONS.with(|c| c.set(0));
    let v: SortedVec<i64, Counting> = xs.iter().copied().collect();
    let inserts = COMPARISONS.with(|c| c.replace(0));
    let found = xs.iter().filter(|x| v.contains(x)).count() as u64;
    let lookups = COMPARISONS.with(|c| c.get());
    check!(
        format!("2048 random inserts then 2048 lookups: {} and {} comparisons per call, at most 40 (log2 2048 is 11)", inserts / n, lookups / n),
        (found, inserts <= 40 * n, lookups <= 40 * n),
        (n, true, true)
    );
}
