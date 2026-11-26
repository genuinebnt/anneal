/// Conway's Game of Life on a `w` × `h` grid. Cells outside the grid are dead.
pub struct Life {
    w: usize,
    h: usize,
    cells: Vec<bool>,
    next: Vec<bool>,
}

impl Life {
    pub fn new(w: usize, h: usize, alive: &[(usize, usize)]) -> Self {
        let mut cells = vec![false; w * h];
        for &(x, y) in alive {
            cells[y * w + x] = true;
        }
        Life {
            w,
            h,
            cells,
            next: vec![false; w * h],
        }
    }

    /// Live cells as (x, y), row by row.
    pub fn alive(&self) -> Vec<(usize, usize)> {
        (0..self.w * self.h)
            .filter(|&i| self.cells[i])
            .map(|i| (i % self.w, i / self.w))
            .collect()
    }

    fn live_neighbors(&self, x: usize, y: usize) -> usize {
        let mut n = 0;
        for ny in y.saturating_sub(1)..=(y + 1).min(self.h - 1) {
            for nx in x.saturating_sub(1)..=(x + 1).min(self.w - 1) {
                if (nx, ny) != (x, y) && self.cells[ny * self.w + nx] {
                    n += 1;
                }
            }
        }
        n
    }

    /// Advances one generation: a live cell with 2 or 3 live neighbors lives on, a dead cell with exactly 3
    /// becomes alive, every other cell is dead. Every cell's fate depends on the old generation only. Must not
    /// allocate.
    pub fn step(&mut self) {
        todo!()
    }
}
