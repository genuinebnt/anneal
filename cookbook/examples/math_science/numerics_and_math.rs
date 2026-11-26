//! RECIPE 259 (numerics): numerical methods and floating-point wisdom, each checked against a closed form.
//! LEVEL: advanced
//! Use in: scientific code, signal processing, graphics, finance, anywhere results come from f64 arithmetic.   Run: cargo run --release --example numerics_and_math
//! COMMON USES: FFT for audio/image filtering and fast big-number multiplication, Newton/secant for solving equations, quadrature and ODE solvers in simulation, LU/CG for linear systems in engineering, Kahan/pairwise summation in statistics and ledgers, modular exponentiation and extended GCD in cryptography and competitive programming.
#![allow(dead_code)]
//!
//! Numerical code is wrong in quiet ways, so each method here is paired with a check that does not trust it:
//! FFT vs the O(n^2) DFT and a round trip; convolution vs schoolbook multiplication; root finders vs sqrt(2) with
//! their convergence ORDER measured (1 for bisection, ~1.62 secant, 2 Newton); quadrature and ODE solvers whose
//! observed order (halve the step, take log2 of the error ratio) must match theory (2, 4, 1, 4); linear solvers
//! judged by residual, with a 2x2 system where skipping pivoting destroys the answer; Kahan and pairwise summation
//! vs naive on 10 million terms, scored against an EXACT reference computed in integers.
//! Rules of thumb: never compare floats with ==, subtract nearly equal numbers last (or not at all), prefer
//! total_cmp for sorting floats, and know that f64 has ~15.95 decimal digits, no more.
//! Read: https://floating-point-gui.de/
//! Read: https://en.wikipedia.org/wiki/Fast_Fourier_transform
//! Read: https://en.wikipedia.org/wiki/Kahan_summation_algorithm

use std::ops::{Add, Mul, Sub};

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn unit(&mut self) -> f64 {
        (self.next() >> 11) as f64 / (1u64 << 53) as f64
    }
}
fn order(coarse: f64, fine: f64) -> f64 {
    (coarse / fine).log2()
}

// ---------------- 1. FFT ----------------
#[derive(Clone, Copy, Debug)]
struct C {
    re: f64,
    im: f64,
}
impl Add for C {
    type Output = C;
    fn add(self, o: C) -> C {
        C {
            re: self.re + o.re,
            im: self.im + o.im,
        }
    }
}
impl Sub for C {
    type Output = C;
    fn sub(self, o: C) -> C {
        C {
            re: self.re - o.re,
            im: self.im - o.im,
        }
    }
}
impl Mul for C {
    type Output = C;
    fn mul(self, o: C) -> C {
        C {
            re: self.re * o.re - self.im * o.im,
            im: self.re * o.im + self.im * o.re,
        }
    }
}
fn c(re: f64, im: f64) -> C {
    C { re, im }
}

/// Iterative radix-2 Cooley-Tukey. Twiddles come from sin/cos per stage (not repeated multiplication) to limit error growth.
fn fft(a: &mut [C], inverse: bool) {
    let n = a.len();
    assert!(n.is_power_of_two());
    let mut j = 0;
    for i in 1..n {
        let mut bit = n >> 1;
        while j & bit != 0 {
            j ^= bit;
            bit >>= 1;
        }
        j ^= bit;
        if i < j {
            a.swap(i, j);
        }
    }
    let mut len = 2;
    while len <= n {
        let sign = if inverse { 1.0 } else { -1.0 };
        let tw: Vec<C> = (0..len / 2)
            .map(|k| {
                let ang = sign * std::f64::consts::TAU * k as f64 / len as f64;
                c(ang.cos(), ang.sin())
            })
            .collect();
        for start in (0..n).step_by(len) {
            for k in 0..len / 2 {
                let (u, v) = (a[start + k], a[start + k + len / 2] * tw[k]);
                a[start + k] = u + v;
                a[start + k + len / 2] = u - v;
            }
        }
        len <<= 1;
    }
    if inverse {
        for x in a.iter_mut() {
            x.re /= n as f64;
            x.im /= n as f64;
        }
    }
}
fn dft_naive(x: &[C]) -> Vec<C> {
    let n = x.len();
    (0..n)
        .map(|k| {
            x.iter().enumerate().fold(c(0.0, 0.0), |s, (t, &v)| {
                let ang = -std::f64::consts::TAU * ((k * t) % n) as f64 / n as f64;
                s + v * c(ang.cos(), ang.sin())
            })
        })
        .collect()
}
fn conv_fft(a: &[i64], b: &[i64]) -> Vec<i64> {
    let n = (a.len() + b.len()).next_power_of_two();
    let mut fa: Vec<C> = a
        .iter()
        .map(|&x| c(x as f64, 0.0))
        .chain(std::iter::repeat(c(0.0, 0.0)))
        .take(n)
        .collect();
    let mut fb: Vec<C> = b
        .iter()
        .map(|&x| c(x as f64, 0.0))
        .chain(std::iter::repeat(c(0.0, 0.0)))
        .take(n)
        .collect();
    fft(&mut fa, false);
    fft(&mut fb, false);
    for i in 0..n {
        fa[i] = fa[i] * fb[i];
    }
    fft(&mut fa, true);
    fa.iter()
        .take(a.len() + b.len() - 1)
        .map(|x| x.re.round() as i64)
        .collect()
}
fn conv_school(a: &[i64], b: &[i64]) -> Vec<i64> {
    let mut r = vec![0; a.len() + b.len() - 1];
    for (i, &x) in a.iter().enumerate() {
        for (j, &y) in b.iter().enumerate() {
            r[i + j] += x * y;
        }
    }
    r
}
fn carry(mut d: Vec<i64>) -> Vec<i64> {
    let mut carry = 0;
    for x in d.iter_mut() {
        let t = *x + carry;
        *x = t % 10;
        carry = t / 10;
    }
    while carry > 0 {
        d.push(carry % 10);
        carry /= 10;
    }
    while d.len() > 1 && *d.last().unwrap() == 0 {
        d.pop();
    }
    d
}
fn fft_section(rng: &mut Rng) {
    let x: Vec<C> = (0..64)
        .map(|_| c(rng.unit() - 0.5, rng.unit() - 0.5))
        .collect();
    let mut f = x.clone();
    fft(&mut f, false);
    let dft = dft_naive(&x);
    let err = f
        .iter()
        .zip(&dft)
        .map(|(a, b)| (a.re - b.re).abs().max((a.im - b.im).abs()))
        .fold(0.0, f64::max);
    let mut back = f.clone();
    fft(&mut back, true);
    let rt = back
        .iter()
        .zip(&x)
        .map(|(a, b)| (a.re - b.re).abs().max((a.im - b.im).abs()))
        .fold(0.0, f64::max);
    assert!(err < 1e-12 && rt < 1e-14);
    // polynomial product and big-integer product (digits LSB first, base 10)
    let pa: Vec<i64> = (0..300).map(|_| (rng.next() % 19) as i64 - 9).collect();
    let pb: Vec<i64> = (0..257).map(|_| (rng.next() % 19) as i64 - 9).collect();
    assert_eq!(conv_fft(&pa, &pb), conv_school(&pa, &pb));
    let da: Vec<i64> = (0..3000).map(|_| (rng.next() % 10) as i64).collect();
    let db: Vec<i64> = (0..3000).map(|_| (rng.next() % 10) as i64).collect();
    let (fast, slow) = (carry(conv_fft(&da, &db)), carry(conv_school(&da, &db)));
    assert_eq!(fast, slow);
    let small = |d: &[i64]| d.iter().rev().fold(0u128, |s, &v| s * 10 + v as u128);
    let (xa, xb): (Vec<i64>, Vec<i64>) = (
        (0..19).map(|_| (rng.next() % 10) as i64).collect(),
        (0..19).map(|_| (rng.next() % 10) as i64).collect(),
    );
    assert_eq!(
        small(&carry(conv_fft(&xa, &xb))),
        small(&xa) * small(&xb),
        "matches native u128 multiplication"
    );
    println!(
        "1. FFT: max |FFT-DFT| = {err:.1e}, round trip {rt:.1e}; poly product exact; 3000x3000-digit product ({} digits) == schoolbook",
        fast.len()
    );
}

// ---------------- 2. root finding ----------------
fn local_order(e: &[f64]) -> f64 {
    let n = (2..e.len())
        .rev()
        .find(|&n| e[n] >= 1e-12 && e[n - 1] > e[n])
        .unwrap();
    (e[n] / e[n - 1]).ln() / (e[n - 1] / e[n - 2]).ln()
}
fn roots_section() {
    let f = |x: f64| x * x - 2.0;
    let truth = 2f64.sqrt();
    let (mut lo, mut hi) = (0.0, 2.0);
    let mut eb = Vec::new();
    for k in 1..=40 {
        let mid = (lo + hi) / 2.0;
        if f(mid) > 0.0 {
            hi = mid
        } else {
            lo = mid
        }
        eb.push((mid - truth).abs());
        assert!(
            hi - lo <= 2.0 / 2f64.powi(k) * 1.000001,
            "interval halves every step"
        );
    }
    let mut x = 2.0;
    let mut en = vec![(x - truth).abs()];
    for _ in 0..6 {
        x -= f(x) / (2.0 * x);
        en.push((x - truth).abs());
    }
    let (mut x0, mut x1) = (1.0, 2.0);
    let mut es = vec![(x0 - truth).abs(), (x1 - truth).abs()];
    for _ in 0..8 {
        let x2 = x1 - f(x1) * (x1 - x0) / (f(x1) - f(x0));
        if x2 == x1 {
            break;
        }
        (x0, x1) = (x1, x2);
        es.push((x1 - truth).abs());
    }
    let (on, os) = (local_order(&en), local_order(&es));
    println!(
        "2. roots of x^2-2: bisection 40 steps -> err {:.1e} (order 1); Newton order {on:.2}; secant order {os:.2} (golden ratio 1.618)",
        eb[39]
    );
    println!(
        "   Newton errors: {:?}",
        en.iter()
            .take(5)
            .map(|e| format!("{e:.1e}"))
            .collect::<Vec<_>>()
    );
    assert!((on - 2.0).abs() < 0.15 && (os - 1.618).abs() < 0.2);
    assert!(en[4] < 1e-11 && eb[39] < 1e-11);
}

// ---------------- 3. quadrature ----------------
fn trapezoid(f: &dyn Fn(f64) -> f64, a: f64, b: f64, n: usize) -> f64 {
    let h = (b - a) / n as f64;
    h * ((f(a) + f(b)) / 2.0 + (1..n).map(|i| f(a + i as f64 * h)).sum::<f64>())
}
fn simpson(f: &dyn Fn(f64) -> f64, a: f64, b: f64, n: usize) -> f64 {
    let h = (b - a) / n as f64; // n even
    let mut s = f(a) + f(b);
    for i in 1..n {
        s += f(a + i as f64 * h) * if i % 2 == 1 { 4.0 } else { 2.0 };
    }
    s * h / 3.0
}
/// Composite Gauss-Legendre with `nodes` points per panel (2 or 3).
fn gauss(f: &dyn Fn(f64) -> f64, a: f64, b: f64, panels: usize, nodes: usize) -> f64 {
    let (xs, ws): (&[f64], &[f64]) = if nodes == 2 {
        (
            &[-0.577_350_269_189_625_8, 0.577_350_269_189_625_8],
            &[1.0, 1.0],
        )
    } else {
        (
            &[-0.774_596_669_241_483_4, 0.0, 0.774_596_669_241_483_4],
            &[5.0 / 9.0, 8.0 / 9.0, 5.0 / 9.0],
        )
    };
    let h = (b - a) / panels as f64;
    let mut s = 0.0;
    for p in 0..panels {
        let (mid, half) = (a + (p as f64 + 0.5) * h, h / 2.0);
        for (x, w) in xs.iter().zip(ws) {
            s += w * half * f(mid + half * x);
        }
    }
    s
}
fn quadrature_section() {
    let (a, b) = (0.0, 2.0);
    let truth = 2f64.exp() - 1.0; // integral of e^x on [0,2]
    let f = |x: f64| x.exp();
    println!(
        "3. quadrature of e^x on [0,2]: error and observed order (log2 of error ratio when n doubles)"
    );
    println!(
        "   {:>4} {:>12} {:>12} {:>12} {:>12}",
        "n", "trapezoid", "Simpson", "GL 2-pt", "GL 3-pt"
    );
    let ns = [4usize, 8, 16, 32];
    let mut errs = vec![Vec::new(); 4];
    for &n in &ns {
        let e = [
            (trapezoid(&f, a, b, n) - truth).abs(),
            (simpson(&f, a, b, n) - truth).abs(),
            (gauss(&f, a, b, n / 2, 2) - truth).abs(),
            (gauss(&f, a, b, n / 2, 3) - truth).abs(),
        ];
        println!(
            "   {:>4} {:>12.3e} {:>12.3e} {:>12.3e} {:>12.3e}",
            n, e[0], e[1], e[2], e[3]
        );
        for k in 0..4 {
            errs[k].push(e[k]);
        }
    }
    let ord = |k: usize| order(errs[k][1], errs[k][2]);
    println!(
        "   observed order: trapezoid {:.2}, Simpson {:.2}, GL2 {:.2}, GL3 {:.2}",
        ord(0),
        ord(1),
        ord(2),
        ord(3)
    );
    assert!(
        (ord(0) - 2.0).abs() < 0.05
            && (ord(1) - 4.0).abs() < 0.1
            && (ord(2) - 4.0).abs() < 0.15
            && ord(3) > 5.5
    );
    assert!(
        errs[3][1] < errs[1][1] && errs[2][1] < errs[1][1],
        "Gauss beats Simpson at equal-ish cost"
    );
}

// ---------------- 4. ODE ----------------
fn ode_section() {
    // y' = y - t^2 + 1, y(0)=0.5; exact y = (t+1)^2 - 0.5 e^t
    let f = |t: f64, y: f64| y - t * t + 1.0;
    let exact = |t: f64| (t + 1.0).powi(2) - 0.5 * t.exp();
    let run = |n: usize, rk: bool| {
        let (h, mut t, mut y) = (2.0 / n as f64, 0.0, 0.5);
        for _ in 0..n {
            if rk {
                let (k1, k2) = (f(t, y), f(t + h / 2.0, y + h / 2.0 * f(t, y)));
                let k3 = f(t + h / 2.0, y + h / 2.0 * k2);
                let k4 = f(t + h, y + h * k3);
                y += h / 6.0 * (k1 + 2.0 * k2 + 2.0 * k3 + k4);
            } else {
                y += h * f(t, y);
            }
            t += h;
        }
        (y - exact(2.0)).abs()
    };
    println!("4. ODE y'=y-t^2+1 on [0,2]: error by step count");
    let ns = [10usize, 20, 40, 80];
    let (e, r): (Vec<f64>, Vec<f64>) = (
        ns.iter().map(|&n| run(n, false)).collect(),
        ns.iter().map(|&n| run(n, true)).collect(),
    );
    for i in 0..4 {
        println!("   n={:<3} Euler {:.3e}  RK4 {:.3e}", ns[i], e[i], r[i]);
    }
    let (oe, or) = (order(e[2], e[3]), order(r[1], r[2]));
    println!("   observed order: Euler {oe:.2}, RK4 {or:.2}");
    assert!((oe - 1.0).abs() < 0.1 && (or - 4.0).abs() < 0.3);
}

// ---------------- 5. linear algebra ----------------
type Mat = Vec<Vec<f64>>;
fn gauss_solve(mut a: Mat, mut b: Vec<f64>, pivot: bool) -> Vec<f64> {
    let n = b.len();
    for col in 0..n {
        if pivot {
            let best = (col..n)
                .max_by(|&i, &j| a[i][col].abs().total_cmp(&a[j][col].abs()))
                .unwrap();
            a.swap(col, best);
            b.swap(col, best);
        }
        for r in col + 1..n {
            let m = a[r][col] / a[col][col];
            for k in col..n {
                a[r][k] -= m * a[col][k];
            }
            b[r] -= m * b[col];
        }
    }
    let mut x = vec![0.0; n];
    for i in (0..n).rev() {
        x[i] = (b[i] - (i + 1..n).map(|k| a[i][k] * x[k]).sum::<f64>()) / a[i][i];
    }
    x
}
/// PA = LU, L (unit diagonal) and U packed in one matrix. Returns (lu, perm, sign).
fn lu(mut a: Mat) -> (Mat, Vec<usize>, f64) {
    let n = a.len();
    let (mut perm, mut sign): (Vec<usize>, f64) = ((0..n).collect(), 1.0);
    for col in 0..n {
        let best = (col..n)
            .max_by(|&i, &j| a[i][col].abs().total_cmp(&a[j][col].abs()))
            .unwrap();
        if best != col {
            a.swap(col, best);
            perm.swap(col, best);
            sign = -sign;
        }
        for r in col + 1..n {
            a[r][col] /= a[col][col];
            let m = a[r][col];
            for k in col + 1..n {
                a[r][k] -= m * a[col][k];
            }
        }
    }
    (a, perm, sign)
}
fn lu_solve(lu: &Mat, perm: &[usize], b: &[f64]) -> Vec<f64> {
    let n = b.len();
    let mut y: Vec<f64> = perm.iter().map(|&p| b[p]).collect();
    for i in 0..n {
        y[i] -= (0..i).map(|k| lu[i][k] * y[k]).sum::<f64>();
    }
    for i in (0..n).rev() {
        y[i] = (y[i] - (i + 1..n).map(|k| lu[i][k] * y[k]).sum::<f64>()) / lu[i][i];
    }
    y
}
fn matvec(a: &Mat, x: &[f64]) -> Vec<f64> {
    a.iter()
        .map(|row| row.iter().zip(x).map(|(p, q)| p * q).sum())
        .collect()
}
/// Conjugate gradient; `mv` is the (sparse) matrix-vector product. Returns (x, iterations).
fn cg(mv: &dyn Fn(&[f64]) -> Vec<f64>, b: &[f64], tol: f64) -> (Vec<f64>, usize) {
    let dot = |u: &[f64], v: &[f64]| u.iter().zip(v).map(|(p, q)| p * q).sum::<f64>();
    let mut x = vec![0.0; b.len()];
    let (mut r, mut p) = (b.to_vec(), b.to_vec());
    let mut rs = dot(&r, &r);
    for it in 1..=b.len() {
        let ap = mv(&p);
        let alpha = rs / dot(&p, &ap);
        for i in 0..x.len() {
            x[i] += alpha * p[i];
            r[i] -= alpha * ap[i];
        }
        let rs_new = dot(&r, &r);
        if rs_new.sqrt() < tol {
            return (x, it);
        }
        for i in 0..p.len() {
            p[i] = r[i] + rs_new / rs * p[i];
        }
        rs = rs_new;
    }
    (x, b.len())
}
fn linalg_section(rng: &mut Rng) {
    let n = 80;
    let a: Mat = (0..n)
        .map(|_| (0..n).map(|_| rng.unit() * 2.0 - 1.0).collect())
        .collect();
    let b: Vec<f64> = (0..n).map(|_| rng.unit()).collect();
    let x = gauss_solve(a.clone(), b.clone(), true);
    let res = matvec(&a, &x)
        .iter()
        .zip(&b)
        .map(|(p, q)| (p - q).abs())
        .fold(0.0, f64::max);
    let (l, perm, _) = lu(a.clone());
    let x2 = lu_solve(&l, &perm, &b);
    let diff = x
        .iter()
        .zip(&x2)
        .map(|(p, q)| (p - q).abs())
        .fold(0.0, f64::max);
    assert!(
        res < 1e-9 && diff < 1e-8,
        "residual {res:e}, gauss vs LU {diff:e}"
    );
    let det3 = lu(vec![
        vec![2.0, 0.0, 1.0],
        vec![1.0, 3.0, 2.0],
        vec![1.0, 1.0, 1.0],
    ]);
    let det = det3.2 * (0..3).map(|i| det3.0[i][i]).product::<f64>();
    assert!(
        (det - (2.0 * (3.0 - 2.0) - 0.0 + 1.0 * (1.0 - 3.0))).abs() < 1e-12,
        "det via LU"
    );
    // no pivoting: tiny pivot 1e-20 destroys the answer; the exact solution is x ~ (1, 1)
    let (am, bm) = (vec![vec![1e-20, 1.0], vec![1.0, 1.0]], vec![1.0, 2.0]);
    let (bad, good) = (
        gauss_solve(am.clone(), bm.clone(), false),
        gauss_solve(am, bm, true),
    );
    println!(
        "5. linalg: n={n} residual {res:.1e}; 2x2 with pivot 1e-20: no pivoting x={:?}  partial pivoting x={:?}",
        bad, good
    );
    assert!(
        (bad[0] - 1.0).abs() > 0.5
            && (good[0] - 1.0).abs() < 1e-12
            && (good[1] - 1.0).abs() < 1e-12
    );
    // conjugate gradient on the 2D Poisson matrix (5-point stencil, SPD, 4 nonzeros per row) never stored
    let m = 40;
    let mv = move |x: &[f64]| -> Vec<f64> {
        let mut y = vec![0.0; m * m];
        for i in 0..m {
            for j in 0..m {
                let k = i * m + j;
                y[k] = 4.0 * x[k];
                if i > 0 {
                    y[k] -= x[k - m];
                }
                if i + 1 < m {
                    y[k] -= x[k + m];
                }
                if j > 0 {
                    y[k] -= x[k - 1];
                }
                if j + 1 < m {
                    y[k] -= x[k + 1];
                }
            }
        }
        y
    };
    let xt: Vec<f64> = (0..m * m).map(|_| rng.unit()).collect();
    let rhs = mv(&xt);
    let (xc, it) = cg(&mv, &rhs, 1e-10);
    let err = xc
        .iter()
        .zip(&xt)
        .map(|(p, q)| (p - q).abs())
        .fold(0.0, f64::max);
    println!(
        "   CG on {}x{} sparse SPD system: {it} iterations, max error {err:.1e} (dense elimination would need {} flops ~ n^3)",
        m * m,
        m * m,
        (m * m).pow(3)
    );
    assert!(err < 1e-7 && it < m * m / 4);
    // dense vs CG on a small grid
    let ms: usize = 8;
    let dense: Mat = (0..ms * ms)
        .map(|r| {
            (0..ms * ms)
                .map(|cc| {
                    let (ri, rj, ci, cj) = (r / ms, r % ms, cc / ms, cc % ms);
                    if r == cc {
                        4.0
                    } else if (ri.abs_diff(ci) == 1 && rj == cj)
                        || (rj.abs_diff(cj) == 1 && ri == ci)
                    {
                        -1.0
                    } else {
                        0.0
                    }
                })
                .collect()
        })
        .collect();
    let bs: Vec<f64> = (0..ms * ms).map(|_| rng.unit()).collect();
    let xd = gauss_solve(dense.clone(), bs.clone(), true);
    let (xcg, _) = cg(&|v: &[f64]| matvec(&dense, v), &bs, 1e-12);
    assert!(xd.iter().zip(&xcg).all(|(p, q)| (p - q).abs() < 1e-9));
}

// ---------------- 6. floating-point pitfalls ----------------
fn ulp_distance(a: f64, b: f64) -> u64 {
    let key = |x: f64| {
        let bits = x.to_bits() as i64;
        if bits < 0 { i64::MIN - bits } else { bits }
    };
    (key(a) as i128 - key(b) as i128).unsigned_abs() as u64
}
fn approx_eq(a: f64, b: f64, rel: f64, abs: f64, max_ulps: u64) -> bool {
    (a - b).abs() <= abs
        || (a - b).abs() <= rel * a.abs().max(b.abs())
        || ulp_distance(a, b) <= max_ulps
}
fn kahan(xs: impl Iterator<Item = f64>) -> f64 {
    let (mut sum, mut comp) = (0.0f64, 0.0f64);
    for x in xs {
        let y = x - comp;
        let t = sum + y;
        comp = (t - sum) - y; // the part of y that was rounded away
        sum = t;
    }
    sum
}
fn pairwise(f: &dyn Fn(usize) -> f64, lo: usize, hi: usize) -> f64 {
    if hi - lo <= 128 {
        (lo..hi).map(f).fold(0.0, |s, x| s + x)
    } else {
        let mid = (lo + hi) / 2;
        pairwise(f, lo, mid) + pairwise(f, mid, hi)
    }
}
fn floats_section() {
    let s = 0.1 + 0.2;
    assert!(s != 0.3 && approx_eq(s, 0.3, 1e-12, 0.0, 0) && ulp_distance(s, 0.3) == 1);
    println!(
        "6. 0.1+0.2 = {s:.17} != 0.3 (distance {} ulp); EPSILON = {:e}; 1+EPS/2 == 1: {}",
        ulp_distance(s, 0.3),
        f64::EPSILON,
        1.0 + f64::EPSILON / 2.0 == 1.0
    );
    assert!(1.0 + f64::EPSILON > 1.0 && 1.0 + f64::EPSILON / 2.0 == 1.0);
    // quadratic x^2 + 1e8 x + 1: roots ~ -1e8 and ~ -1e-8
    let (a, b, cc) = (1.0f64, 1e8, 1.0);
    let d = (b * b - 4.0 * a * cc).sqrt();
    let naive = (-b + d) / (2.0 * a);
    let q = -(b + d) / 2.0;
    let stable = cc / q; // product of roots = c/a, so recover the small root from the big one
    let truth = -1.000000000000000e-8 * (1.0 + 1e-16); // -1e-8 (1 + 1e-16 + ...)
    let (en, es) = (
        ((naive - truth) / truth).abs(),
        ((stable - truth) / truth).abs(),
    );
    println!(
        "   quadratic small root: naive {naive:e} (rel err {en:.1e}) vs stable {stable:e} (rel err {es:.1e})"
    );
    assert!(en > 1e-2 && es < 1e-12, "catastrophic cancellation");
    // summation of 10M copies of 0.1, scored against an exact integer reference
    let n = 10_000_000usize;
    let bits = 0.1f64.to_bits();
    let mant = (bits & ((1u64 << 52) - 1)) | (1u64 << 52);
    let exp = ((bits >> 52) & 0x7ff) as i32 - 1075;
    let exact = (mant as u128 * n as u128) as f64 * 2f64.powi(exp);
    let naive_sum = (0..n).fold(0.0, |s, _| s + 0.1);
    let kahan_sum = kahan((0..n).map(|_| 0.1));
    let pair_sum = pairwise(&|_| 0.1, 0, n);
    let errs = [
        (naive_sum - exact).abs(),
        (pair_sum - exact).abs(),
        (kahan_sum - exact).abs(),
    ];
    println!(
        "   sum of 10M x 0.1 (exact {exact}): naive err {:.2e}, pairwise err {:.2e}, Kahan err {:.2e}",
        errs[0], errs[1], errs[2]
    );
    assert!(
        errs[2] <= errs[1] && errs[1] * 100.0 < errs[0],
        "Kahan <= pairwise << naive"
    );
    // subnormals and NaN ordering
    assert!(
        (f64::MIN_POSITIVE / 2.0).is_subnormal()
            && f64::MIN_POSITIVE / 2f64.powi(52) > 0.0
            && f64::MIN_POSITIVE / 2f64.powi(53) == 0.0
    );
    let nan = f64::NAN;
    assert!(nan != nan && nan.partial_cmp(&1.0).is_none());
    let mut v = [3.0, nan, -0.0, 0.0, f64::NEG_INFINITY, 1.0, f64::INFINITY];
    v.sort_by(|p, q| p.total_cmp(q));
    assert!(
        v[0] == f64::NEG_INFINITY
            && v[1].is_sign_negative()
            && v[1] == 0.0
            && v[2].is_sign_positive()
            && v[6].is_nan()
    );
    println!(
        "   subnormals reach {:e}; total_cmp sorts [3,NaN,-0,0,-inf,1,inf] as {:?}",
        f64::from_bits(1),
        v
    );
}

// ---------------- 7. Monte Carlo ----------------
fn mc_section(rng: &mut Rng) {
    let mut rms = Vec::new();
    println!("7. Monte Carlo pi: RMS error over 30 repeats vs theory 1.64/sqrt(N)");
    for &n in &[100usize, 10_000, 1_000_000] {
        let mut sq = 0.0;
        for _ in 0..30 {
            let hits = (0..n)
                .filter(|_| {
                    let (x, y) = (rng.unit(), rng.unit());
                    x * x + y * y < 1.0
                })
                .count();
            sq += (4.0 * hits as f64 / n as f64 - std::f64::consts::PI).powi(2);
        }
        let r = (sq / 30.0).sqrt();
        println!(
            "   N={n:<8} rms {r:.2e}   theory {:.2e}",
            1.642 / (n as f64).sqrt()
        );
        assert!(
            r < 3.0 * 1.642 / (n as f64).sqrt(),
            "within 3x of the 1/sqrt(N) law"
        );
        rms.push(r);
    }
    assert!(
        rms[1] < rms[0] / 3.0 && rms[2] < rms[1] / 3.0,
        "100x more samples -> ~10x less error"
    );
    let n = 2_000_000;
    let est = (0..n).map(|_| (-(rng.unit().powi(2))).exp()).sum::<f64>() / n as f64;
    println!("   integral of exp(-x^2) on [0,1]: {est:.5} (true 0.74682)");
    assert!((est - 0.746_824_132_812_427).abs() < 1e-3);
}

// ---------------- 8. number theory ----------------
fn pow_mod(mut b: u64, mut e: u64, m: u64) -> u64 {
    let mut r = 1 % m;
    b %= m;
    while e > 0 {
        if e & 1 == 1 {
            r = (r as u128 * b as u128 % m as u128) as u64;
        }
        b = (b as u128 * b as u128 % m as u128) as u64;
        e >>= 1;
    }
    r
}
type M2 = [[u64; 2]; 2];
fn mat_mul(a: &M2, b: &M2, m: u64) -> M2 {
    let mut r = [[0; 2]; 2];
    for i in 0..2 {
        for j in 0..2 {
            r[i][j] = ((a[i][0] as u128 * b[0][j] as u128 + a[i][1] as u128 * b[1][j] as u128)
                % m as u128) as u64;
        }
    }
    r
}
fn fib_matrix(mut n: u64, m: u64) -> u64 {
    let (mut r, mut b): (M2, M2) = ([[1, 0], [0, 1]], [[1, 1], [1, 0]]);
    while n > 0 {
        if n & 1 == 1 {
            r = mat_mul(&r, &b, m);
        }
        b = mat_mul(&b, &b, m);
        n >>= 1;
    }
    r[0][1]
}
/// Fast doubling: F(2k) = F(k)(2F(k+1)-F(k)), F(2k+1) = F(k)^2 + F(k+1)^2. Returns (F(n), F(n+1)).
fn fib_doubling(n: u64, m: u64) -> (u64, u64) {
    if n == 0 {
        return (0, 1 % m);
    }
    let (a, b) = fib_doubling(n / 2, m);
    let (a, b) = (a as u128, b as u128);
    let mm = m as u128;
    let c = a * ((2 * b + mm - a) % mm) % mm;
    let d = (a * a + b * b) % mm;
    if n % 2 == 0 {
        (c as u64, d as u64)
    } else {
        (d as u64, ((c + d) % mm) as u64)
    }
}
fn gcd(a: u64, b: u64) -> u64 {
    if b == 0 { a } else { gcd(b, a % b) }
}
/// Returns (g, x, y) with a*x + b*y = g.
fn ext_gcd(a: i128, b: i128) -> (i128, i128, i128) {
    if b == 0 {
        (a, 1, 0)
    } else {
        let (g, x, y) = ext_gcd(b, a % b);
        (g, y, x - (a / b) * y)
    }
}
fn number_theory_section(rng: &mut Rng) {
    const P: u64 = 1_000_000_007;
    for _ in 0..200 {
        let (b, e) = (rng.next() % P, rng.next() % 200);
        assert_eq!(pow_mod(b, e, P), (0..e).fold(1u64, |r, _| r * b % P));
    }
    for _ in 0..50 {
        let a = 1 + rng.next() % (P - 1);
        assert_eq!(pow_mod(a, P - 1, P), 1, "Fermat's little theorem");
    }
    let mut f = (0u64, 1u64);
    for n in 0..5000u64 {
        assert_eq!(fib_matrix(n, P), f.0);
        assert_eq!(fib_doubling(n, P).0, f.0);
        f = (f.1, (f.0 + f.1) % P);
    }
    let big = 1_000_000_000_000_000_000u64;
    assert_eq!(fib_matrix(big, P), fib_doubling(big, P).0);
    assert_eq!(
        fib_matrix(90, u64::MAX),
        2_880_067_194_370_816_120,
        "F(90) exact"
    );
    for _ in 0..2000 {
        let (a, b) = (rng.next() >> 2, rng.next() >> 2);
        let g = gcd(a, b);
        let (g2, x, y) = ext_gcd(a as i128, b as i128);
        assert_eq!(g as i128, g2);
        assert_eq!(a as i128 * x + b as i128 * y, g2, "Bezout identity");
        assert!(a % g == 0 && b % g == 0);
        assert_eq!(
            a as u128 / g as u128 * b as u128 % 7,
            (a as u128 * b as u128 / g as u128) % 7
        );
    }
    let (_, inv, _) = ext_gcd(123_456_789, P as i128);
    assert_eq!(
        123_456_789u64 as u128 * (inv.rem_euclid(P as i128)) as u128 % P as u128,
        1
    );
    println!(
        "8. pow_mod/matrix/doubling Fibonacci agree with iteration (F(10^18) mod p = {}); extended GCD Bezout verified, inverse of 123456789 mod p = {}",
        fib_matrix(big, P),
        inv.rem_euclid(P as i128)
    );
}

fn main() {
    let mut rng = Rng(0xD1B54A32D192ED03);
    fft_section(&mut rng);
    roots_section();
    quadrature_section();
    ode_section();
    linalg_section(&mut rng);
    floats_section();
    mc_section(&mut rng);
    number_theory_section(&mut rng);
    println!("all numerics checks passed");
}
