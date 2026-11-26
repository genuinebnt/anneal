use std::ops::{Add, Index, IndexMut, Mul};

/// A rows × cols matrix, stored row by row in one Vec.
#[derive(Debug, Clone, PartialEq)]
pub struct Matrix<T> {
    rows: usize,
    cols: usize,
    data: Vec<T>,
}

impl<T: Copy + Default> Matrix<T> {
    /// Every entry `T::default()`.
    pub fn zeros(rows: usize, cols: usize) -> Self {
        Matrix {
            rows,
            cols,
            data: vec![T::default(); rows * cols],
        }
    }

    /// Panics if the rows have different lengths. No rows gives a 0 × 0 matrix.
    pub fn from_rows(rows: Vec<Vec<T>>) -> Self {
        let r = rows.len();
        let c = rows.first().map_or(0, Vec::len);
        assert!(
            rows.iter().all(|row| row.len() == c),
            "rows have different lengths"
        );
        Matrix {
            rows: r,
            cols: c,
            data: rows.into_iter().flatten().collect(),
        }
    }

    pub fn transpose(&self) -> Self {
        let mut t = Matrix::zeros(self.cols, self.rows);
        for r in 0..self.rows {
            for c in 0..self.cols {
                t[(c, r)] = self[(r, c)];
            }
        }
        t
    }
}

impl<T> Matrix<T> {
    pub fn rows(&self) -> usize {
        self.rows
    }

    pub fn cols(&self) -> usize {
        self.cols
    }

    /// Row `r` as a slice. Panics if `r` is out of range.
    pub fn row(&self, r: usize) -> &[T] {
        assert!(r < self.rows, "row {r} out of range");
        &self.data[r * self.cols..(r + 1) * self.cols]
    }

    /// Every row, top to bottom.
    pub fn rows_iter(&self) -> impl Iterator<Item = &[T]> {
        (0..self.rows).map(move |r| self.row(r))
    }
}

/// m[(r, c)]. Panics if `r` or `c` is out of range.
impl<T> Index<(usize, usize)> for Matrix<T> {
    type Output = T;

    fn index(&self, (r, c): (usize, usize)) -> &T {
        assert!(
            r < self.rows && c < self.cols,
            "({r}, {c}) is outside {}x{}",
            self.rows,
            self.cols
        );
        &self.data[r * self.cols + c]
    }
}

impl<T> IndexMut<(usize, usize)> for Matrix<T> {
    fn index_mut(&mut self, (r, c): (usize, usize)) -> &mut T {
        assert!(
            r < self.rows && c < self.cols,
            "({r}, {c}) is outside {}x{}",
            self.rows,
            self.cols
        );
        &mut self.data[r * self.cols + c]
    }
}

/// Element-wise sum. Panics if the shapes differ.
impl<T: Copy + Add<Output = T>> Add for Matrix<T> {
    type Output = Matrix<T>;

    fn add(mut self, rhs: Matrix<T>) -> Matrix<T> {
        assert!(
            self.rows == rhs.rows && self.cols == rhs.cols,
            "shapes differ"
        );
        for (a, &b) in self.data.iter_mut().zip(&rhs.data) {
            *a = *a + b;
        }
        self
    }
}

/// Matrix product. Panics unless `self.cols() == rhs.rows()`.
impl<'a, T: Copy + Default + Add<Output = T> + Mul<Output = T>> Mul for &'a Matrix<T> {
    type Output = Matrix<T>;

    fn mul(self, rhs: &'a Matrix<T>) -> Matrix<T> {
        assert_eq!(self.cols, rhs.rows, "inner dimensions differ");
        let mut out = Matrix::zeros(self.rows, rhs.cols);
        for i in 0..self.rows {
            for k in 0..self.cols {
                let a = self[(i, k)];
                for j in 0..rhs.cols {
                    out[(i, j)] = out[(i, j)] + a * rhs[(k, j)];
                }
            }
        }
        out
    }
}
