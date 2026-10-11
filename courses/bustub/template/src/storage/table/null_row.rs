//! A row of nullable cells with a null bitmap in front.

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Cell {
    Int(i64),
    Text(String),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ty {
    Int,
    Text,
}

/// A row as bytes: a bitmap of the NULL columns (bit `i` set for a NULL column `i`), then each non-NULL value in column order.
pub fn encode_row(row: &[Option<Cell>]) -> Vec<u8> {
    todo!("3b-c1: the bitmap, then each non-NULL value")
}

/// The row [`encode_row`] wrote, given the column types; `None` for anything inconsistent.
pub fn decode_row(bytes: &[u8], types: &[Ty]) -> Option<Vec<Option<Cell>>> {
    todo!("3b-c1: read the bitmap, then the value of each column that is not NULL; refuse anything inconsistent")
}
