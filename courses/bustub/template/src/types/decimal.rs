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
        todo!("3a-c1: sign, digits, at most two fractional digits")
    }

    /// `self + other`; `None` on overflow.
    pub fn add(self, other: Decimal) -> Option<Decimal> {
        todo!("3a-c1: checked addition of hundredths")
    }

    /// `self - other`; `None` on overflow.
    pub fn sub(self, other: Decimal) -> Option<Decimal> {
        todo!("3a-c1: checked subtraction")
    }

    /// `self * other`, rounded half away from zero; `None` on overflow.
    pub fn mul(self, other: Decimal) -> Option<Decimal> {
        todo!("3a-c1: multiply, then scale back by 100 rounding half away from zero")
    }

    /// `self / other`, rounded half away from zero; `None` for a zero divisor or on overflow.
    pub fn div(self, other: Decimal) -> Option<Decimal> {
        todo!("3a-c1: scale up by 100, divide, rounding half away from zero; zero is None")
    }
}

impl fmt::Display for Decimal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        todo!("3a-c1: [-]int.dd")
    }
}
