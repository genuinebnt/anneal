use std::ops::{Add, Mul};

#[derive(Debug, Clone, PartialEq)]
pub struct Matrix<T, const R: usize, const C: usize> {
    rows: [[T; C]; R],
}

impl<T, const R: usize, const C: usize> Matrix<T, R, C> {
    pub const ROWS: usize = R;
    pub const COLS: usize = C;

    pub fn from_rows(rows: [[T; C]; R]) -> Self {
        Matrix { rows }
    }

    pub fn into_rows(self) -> [[T; C]; R] {
        self.rows
    }

    pub fn get(&self, r: usize, c: usize) -> Option<&T> {
        self.rows.get(r)?.get(c)
    }

    /// Moves every element; needs nothing from T.
    pub fn transpose(self) -> Matrix<T, C, R> {
        let mut rows = self.rows.map(|row| row.into_iter());
        Matrix {
            rows: std::array::from_fn(|_| std::array::from_fn(|r| rows[r].next().unwrap())),
        }
    }
}

/// Only square matrices have an identity.
impl<T: Clone + Default, const N: usize> Matrix<T, N, N> {
    pub fn identity(one: T) -> Self {
        Matrix {
            rows: std::array::from_fn(|r| {
                std::array::from_fn(|c| if r == c { one.clone() } else { T::default() })
            }),
        }
    }
}

impl<T: Add<Output = T>, const R: usize, const C: usize> Add for Matrix<T, R, C> {
    type Output = Matrix<T, R, C>;

    fn add(self, rhs: Self) -> Self::Output {
        let mut right = rhs.rows.map(|row| row.into_iter());
        let mut r = 0;
        Matrix {
            rows: self.rows.map(|row| {
                let out = row.map(|x| x + right[r].next().unwrap());
                r += 1;
                out
            }),
        }
    }
}

/// (R x K) * (K x C) = (R x C). A shape mismatch is a type error.
impl<T, const R: usize, const K: usize, const C: usize> Mul<Matrix<T, K, C>> for Matrix<T, R, K>
where
    T: Clone + Default + Add<Output = T> + Mul<Output = T>,
{
    type Output = Matrix<T, R, C>;

    fn mul(self, rhs: Matrix<T, K, C>) -> Self::Output {
        Matrix {
            rows: std::array::from_fn(|i| {
                std::array::from_fn(|j| {
                    let mut sum = T::default();
                    for k in 0..K {
                        sum = sum + self.rows[i][k].clone() * rhs.rows[k][j].clone();
                    }
                    sum
                })
            }),
        }
    }
}
