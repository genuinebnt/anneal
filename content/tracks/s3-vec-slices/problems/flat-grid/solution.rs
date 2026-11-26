pub struct Grid {
    w: usize,
    h: usize,
    cells: Vec<u8>,
}

impl Grid {
    pub fn new(w: usize, h: usize) -> Self {
        Grid {
            w,
            h,
            cells: vec![0; w * h],
        }
    }

    fn index(&self, x: usize, y: usize) -> Option<usize> {
        (x < self.w && y < self.h).then(|| y * self.w + x)
    }

    pub fn get(&self, x: usize, y: usize) -> Option<u8> {
        self.index(x, y).map(|i| self.cells[i])
    }

    pub fn set(&mut self, x: usize, y: usize, value: u8) -> bool {
        match self.index(x, y) {
            Some(i) => {
                self.cells[i] = value;
                true
            }
            None => false,
        }
    }

    pub fn row(&self, y: usize) -> Option<&[u8]> {
        (y < self.h).then(|| &self.cells[y * self.w..(y + 1) * self.w])
    }
}
