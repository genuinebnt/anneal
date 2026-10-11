//! An exact decimal with two fractional digits, stored as hundredths.

use std::fmt;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Decimal {
    cents: i128,
}

/// `n / d` rounded half away from zero (`d != 0`).
fn div_round(n: i128, d: i128) -> i128 {
    let (q, r) = (n / d, n % d);
    if r.abs() * 2 >= d.abs() {
        q + if (n < 0) != (d < 0) { -1 } else { 1 }
    } else {
        q
    }
}

impl Decimal {
    pub fn from_cents(cents: i128) -> Decimal {
        Decimal { cents }
    }

    pub fn cents(&self) -> i128 {
        self.cents
    }

    /// `[+-]digits[.d[d]]`, nothing else.
    pub fn parse(s: &str) -> Option<Decimal> {
        // @begin 3a-c1
        let (neg, body) = match s.as_bytes().first()? {
            b'-' => (true, &s[1..]),
            b'+' => (false, &s[1..]),
            _ => (false, s),
        };
        let (int, frac) = match body.split_once('.') {
            Some((i, f)) => (i, f),
            None => (body, ""),
        };
        if int.is_empty() || !int.bytes().all(|b| b.is_ascii_digit()) || frac.len() > 2 || !frac.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        if body.contains('.') && frac.is_empty() {
            return None;
        }
        let mut cents = int.parse::<i128>().ok()?.checked_mul(100)?;
        let f: i128 = if frac.is_empty() { 0 } else { frac.parse::<i128>().ok()? * if frac.len() == 1 { 10 } else { 1 } };
        cents = cents.checked_add(f)?;
        Some(Decimal { cents: if neg { -cents } else { cents } })
        //~ todo!("3a-c1: sign, digits, at most two fractional digits")
        // @end
    }

    /// `self + other`; `None` on overflow.
    pub fn add(self, other: Decimal) -> Option<Decimal> {
        // @begin 3a-c1
        self.cents.checked_add(other.cents).map(Decimal::from_cents)
        //~ todo!("3a-c1: checked addition of hundredths")
        // @end
    }

    /// `self - other`; `None` on overflow.
    pub fn sub(self, other: Decimal) -> Option<Decimal> {
        // @begin 3a-c1
        self.cents.checked_sub(other.cents).map(Decimal::from_cents)
        //~ todo!("3a-c1: checked subtraction")
        // @end
    }

    /// `self * other`, rounded half away from zero; `None` on overflow.
    pub fn mul(self, other: Decimal) -> Option<Decimal> {
        // @begin 3a-c1
        let p = self.cents.checked_mul(other.cents)?;
        Some(Decimal::from_cents(div_round(p, 100)))
        //~ todo!("3a-c1: multiply, then scale back by 100 rounding half away from zero")
        // @end
    }

    /// `self / other`, rounded half away from zero; `None` for a zero divisor or on overflow.
    pub fn div(self, other: Decimal) -> Option<Decimal> {
        // @begin 3a-c1
        if other.cents == 0 {
            return None;
        }
        let n = self.cents.checked_mul(100)?;
        Some(Decimal::from_cents(div_round(n, other.cents)))
        //~ todo!("3a-c1: scale up by 100, divide, rounding half away from zero; zero is None")
        // @end
    }
}

impl fmt::Display for Decimal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // @begin 3a-c1
        let a = self.cents.unsigned_abs();
        write!(f, "{}{}.{:02}", if self.cents < 0 { "-" } else { "" }, a / 100, a % 100)
        //~ todo!("3a-c1: [-]int.dd")
        // @end
    }
}
