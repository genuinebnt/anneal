//! RECIPE 187 (numerics): arbitrary-precision unsigned integers: limbs, Karatsuba, Knuth division, gcd.
//! LEVEL: advanced
//! Use in: cryptography, exact arithmetic, big factorials/Fibonacci, rational/decimal libraries.   Run: cargo run --release --example bigint_arithmetic
//!
//! WHAT: `BigUint` stores a number as little-endian base-2^32 "limbs" in a Vec<u32>, normalized so the top limb
//! is non-zero (zero = empty vec). Every operation is the grade-school algorithm in base 2^32, with u64 as the
//! carry-holding intermediate type: that is the whole trick for add/sub/mul. u32 limbs (not u64) keep the
//! intermediates in native u64 so no u128 is needed.
//! HOW: add/sub are O(n) carry/borrow loops. Multiplication is O(n^2) schoolbook, switching to Karatsuba (three
//! half-size products instead of four, ~O(n^1.58)) once both operands have >= 32 limbs; below that the
//! recursion overhead loses. Division by a single limb is a simple loop; general division is Knuth's Algorithm D
//! (TAOCP 4.3.1): normalize so the divisor's top bit is set, estimate each quotient limb from the top two
//! limbs (the estimate is at most 2 too big, fixed by a cheap correction and a rare add-back). Decimal output
//! repeatedly divides by 10^9. Correctness is pinned by differential tests vs u128 and by algebraic identities
//! on huge random values (including limbs full of 0 and 0xFFFFFFFF, which trigger the rare Algorithm D paths).
//!
//! COMMON USES: RSA/Diffie-Hellman/elliptic-curve arithmetic (real libraries use fixed-width, constant-time
//! limbs, never a variable-length one like this), exact factorials/binomials/Fibonacci, rational and decimal
//! types, computing digits of pi/e, big-ID and money math without overflow, checking u128 overflow logic.
//! Real code: the `num-bigint` crate https://github.com/rust-num/num-bigint (or `rug`/`crypto-bigint`).
//! Read: https://en.wikipedia.org/wiki/Karatsuba_algorithm   https://github.com/rust-num/num-bigint
#![allow(dead_code)]
use std::cmp::Ordering;
use std::fmt;

const KARATSUBA_THRESHOLD: usize = 32; // limbs

#[derive(Clone, Debug, PartialEq, Eq, Default)]
struct BigUint {
    limbs: Vec<u32>, // little endian, no trailing zero limbs
}

impl BigUint {
    fn from_limbs(mut v: Vec<u32>) -> Self {
        while v.last() == Some(&0) {
            v.pop(); // keep the canonical form so == and cmp can compare limb vectors directly
        }
        BigUint { limbs: v }
    }
    fn from_u128(mut n: u128) -> Self {
        let mut v = Vec::new();
        while n > 0 {
            v.push(n as u32);
            n >>= 32;
        }
        BigUint { limbs: v }
    }
    fn to_u128(&self) -> Option<u128> {
        (self.limbs.len() <= 4).then(|| {
            self.limbs
                .iter()
                .rev()
                .fold(0u128, |acc, &l| acc << 32 | l as u128)
        })
    }
    fn is_zero(&self) -> bool {
        self.limbs.is_empty()
    }
    fn parse(s: &str) -> Result<Self, String> {
        if s.is_empty() || !s.bytes().all(|b| b.is_ascii_digit()) {
            return Err(format!("not a decimal number: {s:?}"));
        }
        let mut n = BigUint::default();
        // Consume up to 9 digits at a time: n = n * 10^k + chunk.
        let first = s.len() % 9;
        let (head, rest) = s.split_at(first);
        let chunks = std::iter::once(head).filter(|h| !h.is_empty()).chain(
            rest.as_bytes()
                .chunks(9)
                .map(|c| std::str::from_utf8(c).unwrap()),
        );
        for chunk in chunks {
            n.mul_small_add(10u32.pow(chunk.len() as u32), chunk.parse().unwrap());
        }
        Ok(n)
    }
    fn mul_small_add(&mut self, m: u32, a: u32) {
        let mut carry = a as u64;
        for l in &mut self.limbs {
            let t = *l as u64 * m as u64 + carry;
            *l = t as u32;
            carry = t >> 32;
        }
        if carry > 0 {
            self.limbs.push(carry as u32);
        }
    }

    fn add(&self, o: &BigUint) -> BigUint {
        let (a, b) = if self.limbs.len() >= o.limbs.len() {
            (self, o)
        } else {
            (o, self)
        };
        let mut out = Vec::with_capacity(a.limbs.len() + 1);
        let mut carry = 0u64;
        for (i, &x) in a.limbs.iter().enumerate() {
            let t = x as u64 + *b.limbs.get(i).unwrap_or(&0) as u64 + carry;
            out.push(t as u32);
            carry = t >> 32;
        }
        out.push(carry as u32);
        BigUint::from_limbs(out)
    }
    /// `None` when the result would be negative (unsigned numbers can't represent it).
    fn checked_sub(&self, o: &BigUint) -> Option<BigUint> {
        if self < o {
            return None;
        }
        let mut out = Vec::with_capacity(self.limbs.len());
        let mut borrow = 0i64;
        for (i, &x) in self.limbs.iter().enumerate() {
            let t = x as i64 - *o.limbs.get(i).unwrap_or(&0) as i64 - borrow;
            out.push(t as u32); // wraps mod 2^32 when negative
            borrow = (t < 0) as i64;
        }
        Some(BigUint::from_limbs(out))
    }
    fn shl_limbs(&self, k: usize) -> BigUint {
        if self.is_zero() {
            return self.clone();
        }
        let mut v = vec![0; k];
        v.extend_from_slice(&self.limbs);
        BigUint { limbs: v }
    }

    fn mul_schoolbook(&self, o: &BigUint) -> BigUint {
        if self.is_zero() || o.is_zero() {
            return BigUint::default();
        }
        let mut out = vec![0u32; self.limbs.len() + o.limbs.len()];
        for (i, &a) in self.limbs.iter().enumerate() {
            let mut carry = 0u64;
            for (j, &b) in o.limbs.iter().enumerate() {
                // max value: (2^32-1)^2 + 2*(2^32-1) = 2^64-1, so this never overflows u64
                let t = a as u64 * b as u64 + out[i + j] as u64 + carry;
                out[i + j] = t as u32;
                carry = t >> 32;
            }
            out[i + o.limbs.len()] = carry as u32;
        }
        BigUint::from_limbs(out)
    }
    /// Karatsuba: with x = x1*B^m + x0 and y likewise, x*y = z2*B^2m + z1*B^m + z0 where
    /// z0 = x0*y0, z2 = x1*y1, and z1 = (x0+x1)(y0+y1) - z0 - z2: three recursive products, not four.
    fn mul_karatsuba(&self, o: &BigUint) -> BigUint {
        let (n, k) = (self.limbs.len(), o.limbs.len());
        if n.min(k) < KARATSUBA_THRESHOLD {
            return self.mul_schoolbook(o);
        }
        let m = n.max(k) / 2;
        let split = |x: &BigUint| {
            let cut = m.min(x.limbs.len());
            (
                BigUint::from_limbs(x.limbs[..cut].to_vec()),
                BigUint::from_limbs(x.limbs[cut..].to_vec()),
            )
        };
        let ((x0, x1), (y0, y1)) = (split(self), split(o));
        let z0 = x0.mul_karatsuba(&y0);
        let z2 = x1.mul_karatsuba(&y1);
        let z1 = x0
            .add(&x1)
            .mul_karatsuba(&y0.add(&y1))
            .checked_sub(&z0)
            .unwrap()
            .checked_sub(&z2)
            .unwrap();
        z0.add(&z1.shl_limbs(m)).add(&z2.shl_limbs(2 * m))
    }

    fn divmod_small(&self, d: u32) -> (BigUint, u32) {
        assert!(d != 0, "division by zero");
        let mut q = vec![0u32; self.limbs.len()];
        let mut rem = 0u64;
        for i in (0..self.limbs.len()).rev() {
            let cur = rem << 32 | self.limbs[i] as u64;
            q[i] = (cur / d as u64) as u32;
            rem = cur % d as u64;
        }
        (BigUint::from_limbs(q), rem as u32)
    }
    /// Knuth Algorithm D. Returns (quotient, remainder); `None` on division by zero.
    fn divmod(&self, d: &BigUint) -> Option<(BigUint, BigUint)> {
        if d.is_zero() {
            return None;
        }
        if self < d {
            return Some((BigUint::default(), self.clone()));
        }
        if d.limbs.len() == 1 {
            let (q, r) = self.divmod_small(d.limbs[0]);
            return Some((q, BigUint::from_u128(r as u128)));
        }
        let (n, m) = (d.limbs.len(), self.limbs.len() - d.limbs.len());
        let s = d.limbs[n - 1].leading_zeros(); // normalize: divisor's top bit set => estimates are tight
        let vn = shl_bits(&d.limbs, s)[..n].to_vec();
        let mut un = shl_bits(&self.limbs, s); // one extra limb
        let mut q = vec![0u32; m + 1];
        let b = 1u64 << 32;
        for j in (0..=m).rev() {
            let num = (un[j + n] as u64) << 32 | un[j + n - 1] as u64;
            let (mut qhat, mut rhat) = (num / vn[n - 1] as u64, num % vn[n - 1] as u64);
            while qhat >= b || qhat * vn[n - 2] as u64 > (rhat << 32 | un[j + n - 2] as u64) {
                qhat -= 1;
                rhat += vn[n - 1] as u64;
                if rhat >= b {
                    break;
                }
            }
            // multiply-and-subtract: un[j..j+n+1] -= qhat * vn
            let mut k = 0i64;
            for i in 0..n {
                let p = qhat * vn[i] as u64;
                let t = un[i + j] as i64 - k - (p & 0xFFFF_FFFF) as i64;
                un[i + j] = t as u32;
                k = (p >> 32) as i64 - (t >> 32);
            }
            let t = un[j + n] as i64 - k;
            un[j + n] = t as u32;
            q[j] = qhat as u32;
            if t < 0 {
                // qhat was one too large (probability ~2/2^32 per digit): add the divisor back
                q[j] = q[j].wrapping_sub(1);
                let mut c = 0u64;
                for i in 0..n {
                    let s2 = un[i + j] as u64 + vn[i] as u64 + c;
                    un[i + j] = s2 as u32;
                    c = s2 >> 32;
                }
                un[j + n] = un[j + n].wrapping_add(c as u32);
            }
        }
        let rem: Vec<u32> = (0..n)
            .map(|i| {
                if s == 0 {
                    un[i]
                } else {
                    un[i] >> s | un[i + 1] << (32 - s)
                }
            }) // undo the normalization shift
            .collect();
        Some((BigUint::from_limbs(q), BigUint::from_limbs(rem)))
    }

    fn pow(&self, mut e: u32) -> BigUint {
        let (mut base, mut acc) = (self.clone(), BigUint::from_u128(1));
        while e > 0 {
            if e & 1 == 1 {
                acc = &acc * &base;
            }
            base = &base * &base;
            e >>= 1;
        }
        acc
    }
    fn factorial(n: u32) -> BigUint {
        let mut acc = BigUint::from_u128(1);
        (2..=n).for_each(|k| acc.mul_small_add(k, 0));
        acc
    }
    fn fibonacci(n: u32) -> BigUint {
        let (mut a, mut b) = (BigUint::default(), BigUint::from_u128(1));
        for _ in 0..n {
            (a, b) = (b.clone(), a.add(&b));
        }
        a
    }
    fn gcd(&self, o: &BigUint) -> BigUint {
        let (mut a, mut b) = (self.clone(), o.clone());
        while !b.is_zero() {
            let (_, r) = a.divmod(&b).unwrap();
            (a, b) = (b, r);
        }
        a
    }
}

/// Shift limbs left by `s` bits (0..32); result has one extra limb for the bits shifted out.
fn shl_bits(limbs: &[u32], s: u32) -> Vec<u32> {
    let mut out = vec![0u32; limbs.len() + 1];
    for (i, &l) in limbs.iter().enumerate() {
        out[i] |= l << s;
        if s > 0 {
            out[i + 1] = l >> (32 - s);
        }
    }
    out
}

impl PartialOrd for BigUint {
    fn partial_cmp(&self, o: &Self) -> Option<Ordering> {
        Some(self.cmp(o))
    }
}
impl Ord for BigUint {
    fn cmp(&self, o: &Self) -> Ordering {
        // more limbs = bigger (canonical form); same length: compare from the most significant limb down
        self.limbs
            .len()
            .cmp(&o.limbs.len())
            .then_with(|| self.limbs.iter().rev().cmp(o.limbs.iter().rev()))
    }
}
impl std::ops::Add for &BigUint {
    type Output = BigUint;
    fn add(self, o: &BigUint) -> BigUint {
        BigUint::add(self, o)
    }
}
impl std::ops::Mul for &BigUint {
    type Output = BigUint;
    fn mul(self, o: &BigUint) -> BigUint {
        self.mul_karatsuba(o)
    }
}
impl fmt::Display for BigUint {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        if self.is_zero() {
            return f.write_str("0");
        }
        let mut chunks = Vec::new();
        let mut cur = self.clone();
        while !cur.is_zero() {
            let (q, r) = cur.divmod_small(1_000_000_000);
            chunks.push(r);
            cur = q;
        }
        write!(f, "{}", chunks.pop().unwrap())?; // most significant chunk has no leading zeros
        chunks.iter().rev().try_for_each(|c| write!(f, "{c:09}"))
    }
}

// ---------- tests ----------
struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn u128(&mut self) -> u128 {
        let bits = self.next() % 129; // vary magnitude so small and huge operands both appear
        if bits == 0 {
            0
        } else {
            ((self.next() as u128) << 64 | self.next() as u128) >> (128 - bits)
        }
    }
    /// Random limbs, biased toward 0 / 1 / 0x7FFFFFFF / 0x80000000 / 0xFFFFFFFF to stress carries and Algorithm D.
    fn big(&mut self, max_limbs: u64) -> BigUint {
        const SPICY: [u32; 5] = [0, 1, 0x7FFF_FFFF, 0x8000_0000, 0xFFFF_FFFF];
        let n = self.next() % max_limbs + 1;
        BigUint::from_limbs(
            (0..n)
                .map(|_| {
                    if self.next() % 3 == 0 {
                        SPICY[(self.next() % 5) as usize]
                    } else {
                        self.next() as u32
                    }
                })
                .collect(),
        )
    }
}

fn gcd_u128(a: u128, b: u128) -> u128 {
    if b == 0 { a } else { gcd_u128(b, a % b) }
}

fn differential_vs_u128(rng: &mut Rng) {
    for _ in 0..20_000 {
        let (x, y) = (rng.u128(), rng.u128());
        let (bx, by) = (BigUint::from_u128(x), BigUint::from_u128(y));
        assert_eq!(bx.to_u128(), Some(x));
        assert_eq!(bx.to_string(), x.to_string());
        assert_eq!(BigUint::parse(&x.to_string()).unwrap(), bx);
        assert_eq!(bx.cmp(&by), x.cmp(&y));
        assert_eq!(bx.add(&by).to_u128(), x.checked_add(y)); // None only on u128 overflow (> 4 limbs)
        if x.checked_add(y).is_none() {
            assert_eq!(bx.add(&by).limbs.len(), 5);
        }
        assert_eq!(
            bx.checked_sub(&by).map(|r| r.to_u128().unwrap()),
            x.checked_sub(y)
        );
        let (xs, ys) = (x >> 64, y >> 64); // keep the product inside u128
        let prod = BigUint::from_u128(xs).mul_schoolbook(&BigUint::from_u128(ys));
        assert_eq!(prod.to_u128(), Some(xs * ys));
        assert_eq!(
            BigUint::from_u128(xs).mul_karatsuba(&BigUint::from_u128(ys)),
            prod
        );
        match (bx.divmod(&by), y) {
            (None, 0) => {}
            (Some((q, r)), _) => assert_eq!((q.to_u128(), r.to_u128()), (Some(x / y), Some(x % y))),
            _ => panic!("divmod disagreement"),
        }
        let d = (rng.next() as u32).max(1);
        let (q, r) = bx.divmod_small(d);
        assert_eq!(
            (q.to_u128(), r as u128),
            (Some(x / d as u128), x % d as u128)
        );
        assert_eq!(bx.gcd(&by).to_u128(), Some(gcd_u128(x, y)));
        let (base, e) = (rng.next() as u32 as u128 & 0xFFFF, (rng.next() % 8) as u32);
        assert_eq!(BigUint::from_u128(base).pow(e).to_u128(), Some(base.pow(e)));
    }
}

fn identities_on_big_values(rng: &mut Rng) {
    for round in 0..400 {
        let (a, b, c) = (rng.big(120), rng.big(120), rng.big(20));
        assert_eq!(a.add(&b).checked_sub(&b), Some(a.clone())); // (a+b)-b == a
        assert_eq!(a.checked_sub(&a.add(&BigUint::from_u128(1))), None);
        assert_eq!(a.mul_karatsuba(&b), a.mul_schoolbook(&b), "round {round}"); // two algorithms must agree
        assert_eq!(a.mul_karatsuba(&b), b.mul_karatsuba(&a)); // commutative
        assert_eq!(&a * &b.add(&c), (&a * &b).add(&(&a * &c))); // distributive
        if !b.is_zero() {
            let p = &a * &b;
            assert_eq!(p.divmod(&b), Some((a.clone(), BigUint::default()))); // (a*b)/b == a
            let (q, r) = a.divmod(&b).unwrap();
            assert!(r < b);
            assert_eq!(&(&q * &b).add(&r), &a); // a == q*b + r
        }
        assert_eq!(BigUint::parse(&a.to_string()).unwrap(), a);
    }
    assert_eq!(
        BigUint::default().divmod(&BigUint::from_u128(7)).unwrap().0,
        BigUint::default()
    );
    assert!(BigUint::from_u128(5).divmod(&BigUint::default()).is_none());
    assert!(BigUint::parse("12x").is_err() && BigUint::parse("").is_err());
    assert_eq!(
        BigUint::parse("000000000000000000123").unwrap().to_u128(),
        Some(123)
    );
    assert_eq!(BigUint::default().to_string(), "0");
}

fn main() {
    let mut rng = Rng(0xDEAD_BEEF_CAFE_F00D);
    differential_vs_u128(&mut rng);
    identities_on_big_values(&mut rng);

    let f100 = BigUint::factorial(100);
    println!("100! = {f100}");
    assert_eq!(f100.to_string().len(), 158);
    assert!(f100.to_string().starts_with("93326215443944152681699238856266700490715968264381621468592963895217599993229915608941463976156518286253697920827223758251185210916864"));
    assert_eq!(
        f100.to_string()
            .bytes()
            .map(|b| (b - b'0') as u32)
            .sum::<u32>(),
        648
    ); // known digit sum
    assert_eq!(
        BigUint::factorial(25).to_string(),
        "15511210043330985984000000"
    );
    assert_eq!(
        BigUint::factorial(30).to_u128(),
        Some(265252859812191058636308480000000)
    );

    let f1000 = BigUint::fibonacci(1000);
    let s = f1000.to_string();
    println!("fib(1000) = {s}");
    assert_eq!(s.len(), 209);
    assert!(s.starts_with("4346655768693745643568852767504062580256466051737178040248172908953655541794905189040387984007925516929592259308032263477520968962323987332247116164299644090653318793829896964992851600370447613779516684922887"));
    // Identity cross-check with the multiplier: F(2n) = F(n) * (2F(n+1) - F(n)).
    let (f500, f501) = (BigUint::fibonacci(500), BigUint::fibonacci(501));
    let inner = f501.add(&f501).checked_sub(&f500).unwrap();
    assert_eq!(&f500 * &inner, f1000);
    // And the famous gcd property: gcd(F(m), F(n)) = F(gcd(m, n)).
    assert_eq!(
        BigUint::fibonacci(300).gcd(&BigUint::fibonacci(450)),
        BigUint::fibonacci(150)
    );
    assert_eq!(
        BigUint::from_u128(2).pow(200).to_string(),
        "1606938044258990275541962092341162602522202993782792835301376"
    );

    // Karatsuba pays off for big operands (timing is informational only).
    let (a, b) = (rng.big(1500), rng.big(1500));
    let a = BigUint::from_limbs([a.limbs.clone(), vec![1; 1500]].concat());
    let b = BigUint::from_limbs([b.limbs.clone(), vec![1; 1500]].concat());
    let t = std::time::Instant::now();
    let slow = a.mul_schoolbook(&b);
    let t_school = t.elapsed();
    let t = std::time::Instant::now();
    let fast = a.mul_karatsuba(&b);
    let t_kara = t.elapsed();
    assert_eq!(slow, fast);
    println!(
        "{}-limb multiply: schoolbook {:?}, karatsuba {:?}",
        a.limbs.len(),
        t_school,
        t_kara
    );
    println!("ok");
}
