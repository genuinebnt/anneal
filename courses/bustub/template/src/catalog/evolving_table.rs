//! A table whose columns can be added and dropped without rewriting its rows.

pub struct EvolvingTable {
    _evolving: (),
}

impl EvolvingTable {
    /// An empty table with `columns` columns.
    pub fn new(columns: usize) -> EvolvingTable {
        todo!("3c-c5: an empty table with `columns` columns")
    }

    /// The number of columns now.
    pub fn columns(&self) -> usize {
        todo!("3c-c5: the number of columns now")
    }

    /// Appends a row of one value per column and returns its row id; `None` if the row has the wrong number of values.
    pub fn insert(&mut self, row: &[i64]) -> Option<usize> {
        todo!("3c-c5: store the row in the stored layout")
    }

    /// Adds a column at the end. Rows stored earlier read it as `default`; no stored row is rewritten.
    pub fn add_column(&mut self, default: i64) {
        todo!("3c-c5: a new column that old rows read as the default")
    }

    /// Hides column `i` from every row and shifts the later columns down by one; returns whether there was such a column.
    ///
    /// No stored row is rewritten.
    pub fn drop_column(&mut self, i: usize) -> bool {
        todo!("3c-c5: hide the column from every row")
    }

    /// Row `rid` as the current schema sees it: dropped columns are gone, added ones read their default. `None` for an unknown row.
    pub fn get(&self, rid: usize) -> Option<Vec<i64>> {
        todo!("3c-c5: the row as the current schema sees it")
    }
}
