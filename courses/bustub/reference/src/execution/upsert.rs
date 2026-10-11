//! INSERT ... ON CONFLICT over a keyed table.

use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Conflict {
    DoNothing,
    Replace,
    Add,
}

#[derive(Debug, PartialEq, Eq, Default)]
pub struct Counts {
    pub inserted: usize,
    pub updated: usize,
    pub ignored: usize,
}

/// Applies `rows` in order to `table`, counting what happened to each. A new key is inserted; for a key already present `policy` decides:
/// `DoNothing` ignores the row, `Replace` overwrites the value, `Add` adds the new value to the old one.
pub fn upsert(table: &mut BTreeMap<i64, i64>, rows: &[(i64, i64)], policy: Conflict) -> Counts {
    // @begin 3e-c2
    let mut c = Counts::default();
    for &(k, v) in rows {
        match table.get_mut(&k) {
            None => {
                table.insert(k, v);
                c.inserted += 1;
            }
            Some(old) => match policy {
                Conflict::DoNothing => c.ignored += 1,
                Conflict::Replace => {
                    *old = v;
                    c.updated += 1;
                }
                Conflict::Add => {
                    *old = old.wrapping_add(v);
                    c.updated += 1;
                }
            },
        }
    }
    c
    //~ todo!("3e-c2: apply the rows in order, counting what happened to each")
    // @end
}
