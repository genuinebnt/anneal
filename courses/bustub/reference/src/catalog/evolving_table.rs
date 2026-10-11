//! A table whose columns can be added and dropped without rewriting its rows.

pub struct EvolvingTable {
    // @begin 3c-c5
    /// rows as stored; a row may be shorter than the number of columns ever added to it.
    rows: Vec<Vec<i64>>,
    /// For each *current* column: which stored position it reads, and the default for rows too short to have it.
    columns: Vec<(usize, i64)>,
    /// How many stored positions exist in total (dropped ones stay as holes).
    stored_width: usize,
    //~ _evolving: (),
    // @end
}

impl EvolvingTable {
    /// An empty table with `columns` columns.
    pub fn new(columns: usize) -> EvolvingTable {
        // @begin 3c-c5
        EvolvingTable { rows: Vec::new(), columns: (0..columns).map(|i| (i, 0)).collect(), stored_width: columns }
        //~ todo!("3c-c5: an empty table with `columns` columns")
        // @end
    }

    /// The number of columns now.
    pub fn columns(&self) -> usize {
        // @begin 3c-c5
        self.columns.len()
        //~ todo!("3c-c5: the number of columns now")
        // @end
    }

    /// Appends a row of one value per column and returns its row id; `None` if the row has the wrong number of values.
    pub fn insert(&mut self, row: &[i64]) -> Option<usize> {
        // @begin 3c-c5
        if row.len() != self.columns.len() {
            return None;
        }
        let mut stored = vec![0; self.stored_width];
        for (&(pos, _), &v) in self.columns.iter().zip(row) {
            stored[pos] = v;
        }
        self.rows.push(stored);
        Some(self.rows.len() - 1)
        //~ todo!("3c-c5: store the row in the stored layout")
        // @end
    }

    /// Adds a column at the end. Rows stored earlier read it as `default`; no stored row is rewritten.
    pub fn add_column(&mut self, default: i64) {
        // @begin 3c-c5
        self.columns.push((self.stored_width, default));
        self.stored_width += 1;
        //~ todo!("3c-c5: a new column that old rows read as the default")
        // @end
    }

    /// Hides column `i` from every row and shifts the later columns down by one; returns whether there was such a column.
    ///
    /// No stored row is rewritten.
    pub fn drop_column(&mut self, i: usize) -> bool {
        // @begin 3c-c5
        if i >= self.columns.len() {
            return false;
        }
        self.columns.remove(i);
        true
        //~ todo!("3c-c5: hide the column from every row")
        // @end
    }

    /// Row `rid` as the current schema sees it: dropped columns are gone, added ones read their default. `None` for an unknown row.
    pub fn get(&self, rid: usize) -> Option<Vec<i64>> {
        // @begin 3c-c5
        let stored = self.rows.get(rid)?;
        Some(self.columns.iter().map(|&(pos, default)| stored.get(pos).copied().unwrap_or(default)).collect())
        //~ todo!("3c-c5: the row as the current schema sees it")
        // @end
    }
}
