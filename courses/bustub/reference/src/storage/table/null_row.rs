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
    // @begin 3b-c1
    let mut out = vec![0u8; row.len().div_ceil(8)];
    for (i, c) in row.iter().enumerate() {
        if c.is_none() {
            out[i / 8] |= 1 << (i % 8);
        }
    }
    for c in row.iter().flatten() {
        match c {
            Cell::Int(v) => out.extend_from_slice(&v.to_le_bytes()),
            Cell::Text(s) => {
                out.extend_from_slice(&(s.len() as u32).to_le_bytes());
                out.extend_from_slice(s.as_bytes());
            }
        }
    }
    out
    //~ todo!("3b-c1: the bitmap, then each non-NULL value")
    // @end
}

/// The row [`encode_row`] wrote, given the column types; `None` for anything inconsistent.
pub fn decode_row(bytes: &[u8], types: &[Ty]) -> Option<Vec<Option<Cell>>> {
    // @begin 3b-c1
    let bm = types.len().div_ceil(8);
    if bytes.len() < bm {
        return None;
    }
    let (bitmap, mut rest) = bytes.split_at(bm);
    let mut row = Vec::with_capacity(types.len());
    for (i, ty) in types.iter().enumerate() {
        if bitmap[i / 8] >> (i % 8) & 1 == 1 {
            row.push(None);
            continue;
        }
        match ty {
            Ty::Int => {
                let (v, r) = rest.split_first_chunk::<8>()?;
                row.push(Some(Cell::Int(i64::from_le_bytes(*v))));
                rest = r;
            }
            Ty::Text => {
                let (len, r) = rest.split_first_chunk::<4>()?;
                let len = u32::from_le_bytes(*len) as usize;
                if r.len() < len {
                    return None;
                }
                let s = std::str::from_utf8(&r[..len]).ok()?;
                row.push(Some(Cell::Text(s.to_owned())));
                rest = &r[len..];
            }
        }
    }
    // bits past the last column must be clear, and nothing may follow
    if rest.is_empty() && (types.len() % 8 == 0 || bitmap[bm - 1] >> (types.len() % 8) == 0) {
        Some(row)
    } else {
        None
    }
    //~ todo!("3b-c1: read the bitmap, then the value of each column that is not NULL; refuse anything inconsistent")
    // @end
}
