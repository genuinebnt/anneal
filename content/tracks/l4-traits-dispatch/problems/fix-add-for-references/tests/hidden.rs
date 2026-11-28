use solution::*;

#[test]
fn sum_all_empty() {
    check!(r#"sum_all(&[])"#, sum_all(&[]), Poly(vec![]));
}

#[test]
fn all_cancel() {
    check!(r#"&[1, -1] + &[-1, 1]"#, &Poly(vec![1, -1]) + &Poly(vec![-1, 1]), Poly(vec![]));
}

#[test]
fn zero_plus_zero() {
    check!(r#"Poly [] + &Poly []"#, Poly(vec![]) + &Poly(vec![]), Poly(vec![]));
}

#[test]
fn longer_right() {
    check!(r#"[1] + &[0, 0, 0, 4]"#, Poly(vec![1]) + &Poly(vec![0, 0, 0, 4]), Poly(vec![1, 0, 0, 4]));
}

#[test]
fn add_assign_trims() {
    let mut p = Poly(vec![1, 2, 3]);
    p += &Poly(vec![0, 0, -3]);
    check!(r#"p = [1, 2, 3]; p += &[0, 0, -3]"#, p, Poly(vec![1, 2]));
}

#[test]
fn add_assign_self_copy() {
    let mut p = Poly(vec![]);
    let q = Poly(vec![1, 2]);
    p += &q;
    p += &q;
    check!(r#"p += &q twice"#, p, Poly(vec![2, 4]));
}

#[test]
fn owned_by_value_still_works() {
    check!(r#"Poly [1] + Poly [2]"#, Poly(vec![1]) + Poly(vec![2]), Poly(vec![3]));
}

#[test]
fn chained() {
    let (a, b, c) = (Poly(vec![1]), Poly(vec![0, 1]), Poly(vec![0, 0, 1]));
    check!(r#"&a + &b + &c"#, &a + &b + &c, Poly(vec![1, 1, 1]));
}

#[test]
fn sum_all_many() {
    let ps: Vec<Poly> = (0..1000).map(|_| Poly(vec![1, -1, 2])).collect();
    check!(r#"sum_all of 1000 copies of [1, -1, 2]"#, sum_all(&ps), Poly(vec![1000, -1000, 2000]));
}

#[test]
fn random_vs_brute_force() {
    let mut rng = anneal_prelude::Rng::new(4415);
    for _ in 0..300 {
        let n = rng.below(5);
        let mut ps = Vec::new();
        for _ in 0..n {
            let len = rng.below(4);
            let mut c: Vec<i64> = rng.vec(len, -2, 2);
            while c.last() == Some(&0) {
                c.pop();
            }
            ps.push(Poly(c));
        }
        let width = ps.iter().map(|p| p.0.len()).max().unwrap_or(0);
        let mut want = vec![0i64; width];
        for p in &ps {
            for (i, c) in p.0.iter().enumerate() {
                want[i] += c;
            }
        }
        while want.last() == Some(&0) {
            want.pop();
        }
        check!(format!("sum_all({ps:?})"), sum_all(&ps), Poly(want.clone()));
        if n >= 2 {
            let mut acc = Poly(vec![]);
            for p in &ps {
                acc += p;
            }
            check!(format!("+= over {ps:?}"), acc, Poly(want.clone()));
            let pair = &ps[0] + &ps[1];
            let mut w2 = vec![0i64; ps[0].0.len().max(ps[1].0.len())];
            for p in &ps[..2] {
                for (i, c) in p.0.iter().enumerate() {
                    w2[i] += c;
                }
            }
            while w2.last() == Some(&0) {
                w2.pop();
            }
            check!(format!("{:?} + {:?}", ps[0], ps[1]), pair, Poly(w2));
        }
    }
}

#[test]
fn owned_plus_ref_reuses_the_buffer() {
    let p = Poly(vec![1, 2, 3]);
    let before = p.0.as_ptr();
    let sum = p + &Poly(vec![1, 1]);
    check!(r#"[1, 2, 3] + &[1, 1]: same buffer as the left side"#, (sum.0.as_ptr() == before, sum), (true, Poly(vec![2, 3, 3])));
}
