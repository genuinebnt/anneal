use solution::*;

#[derive(Clone, Debug, PartialEq)]
struct Text(String);
impl Summary for Text {
    fn empty() -> Self {
        Text(String::new())
    }
    fn join(a: &Self, b: &Self) -> Self {
        Text(format!("{}{}", a.0, b.0))
    }
}

#[derive(Clone, Copy)]
struct Max(i64);
impl Summary for Max {
    fn empty() -> Self {
        Max(i64::MIN)
    }
    fn join(a: &Self, b: &Self) -> Self {
        Max(a.0.max(b.0))
    }
}

#[test]
fn core_random_sums_match_a_slice() {
    let mut rng = anneal_prelude::Rng::new(5501);
    for _ in 0..400 {
        let n = 1 + rng.below(20);
        let mut v: Vec<i64> = rng.vec(n, -20, 20);
        let mut s = PrefixSums::new(&v);
        for _ in 0..20 {
            if rng.bool() {
                let (i, d) = (rng.below(n), rng.int(-20, 20));
                v[i] += d;
                s.add(i, d);
            }
            let r = rng.below(n + 1);
            let l = rng.below(r + 1);
            check!(format!("{v:?}: sum({l}, {r})"), s.sum(l, r), v[l..r].iter().sum());
        }
    }
}

#[test]
fn core_large_values() {
    let big = 1_000_000_000_000i64;
    let s = PrefixSums::new(&[big, big, -big]);
    check!("prefix(3)", s.prefix(3), big);
}

#[test]
fn part2_targets_at_or_below_zero() {
    check!("no slots", PrefixSums::new(&[]).first_reaching(0), None);
    let s = PrefixSums::new(&[0, 0, 4]);
    check!("(0, -5, 1, 4, 5)", [0, -5, 1, 4, 5].map(|t| s.first_reaching(t)), [Some(0), Some(0), Some(2), Some(2), None]);
}

#[test]
fn part2_random_targets_match_a_walk() {
    let mut rng = anneal_prelude::Rng::new(5502);
    for _ in 0..400 {
        let n = 1 + rng.below(20);
        let mut v: Vec<i64> = rng.vec(n, 0, 4);
        let mut s = PrefixSums::new(&v);
        for _ in 0..10 {
            let (i, extra) = (rng.below(n), rng.int(0, 4));
            v[i] += extra;
            s.add(i, extra);
            let total: i64 = v.iter().sum();
            let target = rng.int(0, total + 2);
            let mut running = 0;
            let want = v.iter().position(|&x| {
                running += x;
                running >= target
            });
            check!(format!("{v:?}: first_reaching({target})"), s.first_reaching(target), want);
        }
    }
}

#[test]
fn part3_random_ranges_match_concatenation() {
    let mut rng = anneal_prelude::Rng::new(5503);
    for _ in 0..300 {
        let n = rng.below(17);
        let mut v: Vec<Text> = (0..n).map(|_| Text(rng.string(1, "abcdef"))).collect();
        let mut t = Combine::new(&v);
        for _ in 0..20 {
            if n > 0 && rng.bool() {
                let i = rng.below(n);
                let value = Text(rng.string(1, "uvwxyz"));
                v[i] = value.clone();
                t.set(i, value);
            }
            let r = rng.below(n + 1);
            let l = rng.below(r + 1);
            let want: String = v[l..r].iter().map(|x| x.0.as_str()).collect();
            check!(format!("{v:?}: query({l}, {r})"), t.query(l, r).0, want);
        }
    }
}

#[test]
fn part3_another_operation() {
    let t = Combine::new(&[Max(-3), Max(-9), Max(-1)]);
    check!("(0..2, 1..3, empty)", (t.query(0, 2).0, t.query(1, 3).0, t.query(2, 2).0), (-3, -1, i64::MIN));
}

#[test]
fn part4_pending_work_reaches_the_slots() {
    let mut a = RangeAdder::new(&[0; 8]);
    a.add_range(0, 8, 5);
    check!("one slot after a full-range add", a.sum(3, 4), 5);
    a.add_range(2, 6, -1);
    check!("(0..8, 5..7)", (a.sum(0, 8), a.sum(5, 7)), (36, 9));
}

#[test]
fn part4_empty() {
    let mut a = RangeAdder::new(&[]);
    a.add_range(0, 0, 9);
    check!("sum", a.sum(0, 0), 0);
    let mut b = RangeAdder::new(&[7]);
    b.add_range(1, 1, 9);
    check!("empty range changes nothing", b.sum(0, 1), 7);
}

#[test]
fn part4_random_updates_match_a_slice() {
    let mut rng = anneal_prelude::Rng::new(5504);
    for _ in 0..400 {
        let n = 1 + rng.below(18);
        let mut v: Vec<i64> = rng.vec(n, -9, 9);
        let mut a = RangeAdder::new(&v);
        let mut trace = vec![format!("{v:?}")];
        for _ in 0..30 {
            let r = rng.below(n + 1);
            let l = rng.below(r + 1);
            if rng.bool() {
                let d = rng.int(-9, 9);
                trace.push(format!("add_range({l}, {r}, {d})"));
                for x in &mut v[l..r] {
                    *x += d;
                }
                a.add_range(l, r, d);
            } else {
                check!(format!("{trace:?}: sum({l}, {r})"), a.sum(l, r), v[l..r].iter().sum());
            }
        }
    }
}

const N: usize = 200_000;

#[test]
fn core_scale_updates_and_sums() {
    let mut s = PrefixSums::new(&vec![1; N]);
    let mut total = 0i64;
    for i in 0..N {
        s.add((i * 7919) % N, 1);
        total += s.sum(i / 2, N - i / 3);
    }
    check!("final total", s.prefix(N), 2 * N as i64);
    assert!(total > 0);
}

#[test]
fn part2_scale_many_searches() {
    let s = PrefixSums::new(&vec![2; N]);
    let mut reached = 0;
    for i in 0..N {
        reached += s.first_reaching(2 * i as i64 + 1).unwrap_or(0);
    }
    check!("first_reaching over the whole range", reached > N, true);
}

#[test]
fn part3_scale() {
    let mut t = Combine::new(&vec![Max(0); N]);
    let mut seen = 0i64;
    for i in 0..N {
        t.set((i * 7919) % N, Max(i as i64));
        seen += i64::from(t.query(i / 2, N - i / 3).0 >= 0);
    }
    check!("queries that saw a value", seen, N as i64);
}

#[test]
fn part4_scale_wide_updates() {
    let mut a = RangeAdder::new(&vec![0; N]);
    let mut want = 0i64;
    for i in 0..N / 2 {
        let (l, r) = (i % 1000, N - (i % 777));
        a.add_range(l, r, 1);
        want += (r - l) as i64;
        if i % 4 == 0 {
            std::hint::black_box(a.sum(i / 2, N - i / 3));
        }
    }
    check!("total", a.sum(0, N), want);
}
