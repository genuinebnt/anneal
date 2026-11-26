use std::ops::{Add, AddAssign};

/// Coefficients from the constant term up: Poly(vec![1, 0, 3]) is 1 + 3x².
/// No trailing zeros: the zero polynomial is Poly(vec![]).
#[derive(Debug, PartialEq, Eq)]
pub struct Poly(pub Vec<i64>);

impl Add for Poly {
    type Output = Poly;

    fn add(self, rhs: Poly) -> Poly {
        let (mut long, short) = if self.0.len() >= rhs.0.len() {
            (self.0, rhs.0)
        } else {
            (rhs.0, self.0)
        };
        for (i, c) in short.into_iter().enumerate() {
            long[i] += c;
        }
        while long.last() == Some(&0) {
            long.pop();
        }
        Poly(long)
    }
}

impl AddAssign<&Poly> for Poly {
    fn add_assign(&mut self, rhs: &Poly) {
        if self.0.len() < rhs.0.len() {
            self.0.resize(rhs.0.len(), 0);
        }
        for (a, b) in self.0.iter_mut().zip(&rhs.0) {
            *a += b;
        }
        while self.0.last() == Some(&0) {
            self.0.pop();
        }
    }
}

/// Reuses the left side's buffer.
impl Add<&Poly> for Poly {
    type Output = Poly;

    fn add(mut self, rhs: &Poly) -> Poly {
        self += rhs;
        self
    }
}

impl Add<&Poly> for &Poly {
    type Output = Poly;

    fn add(self, rhs: &Poly) -> Poly {
        Poly(Vec::with_capacity(self.0.len().max(rhs.0.len()))) + self + rhs
    }
}

/// The sum of all the polynomials.
pub fn sum_all(ps: &[Poly]) -> Poly {
    ps.iter().fold(Poly(vec![]), |acc, p| acc + p)
}
