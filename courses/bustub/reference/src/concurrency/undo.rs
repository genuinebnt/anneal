//! Delta undo records for MVCC.

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UndoLog {
    /// The change created the tuple: undoing it removes it.
    Remove,
    /// The change deleted the tuple: undoing it restores the whole row.
    Restore(Vec<i64>),
    /// The change updated some columns: `mask[i]` says column `i` differs; `values` holds the old values of exactly those columns, in order.
    Columns { mask: Vec<bool>, values: Vec<i64> },
}

/// The undo log that turns `new` back into `old`, where `None` is no tuple.
///
/// An insert is undone by removal, a delete by restoring the row, and an update by the changed columns only.
pub fn make_undo(old: Option<&[i64]>, new: Option<&[i64]>) -> UndoLog {
    // @begin 4a-c4
    match (old, new) {
        (None, _) => UndoLog::Remove,
        (Some(o), None) => UndoLog::Restore(o.to_vec()),
        (Some(o), Some(n)) => {
            let mask: Vec<bool> = o.iter().zip(n).map(|(a, b)| a != b).collect();
            let values = o.iter().zip(&mask).filter(|(_, &m)| m).map(|(&v, _)| v).collect();
            UndoLog::Columns { mask, values }
        }
    }
    //~ todo!("4a-c4: an insert is undone by removing, a delete by restoring, an update by the changed columns only")
    // @end
}

/// Applies `log` to `tuple`, giving the version before the change; `None` is no tuple.
pub fn apply_undo(tuple: Option<&[i64]>, log: &UndoLog) -> Option<Vec<i64>> {
    // @begin 4a-c4
    match log {
        UndoLog::Remove => None,
        UndoLog::Restore(row) => Some(row.clone()),
        UndoLog::Columns { mask, values } => {
            let mut row = tuple?.to_vec();
            let mut vals = values.iter();
            for (c, &m) in row.iter_mut().zip(mask) {
                if m {
                    *c = *vals.next()?;
                }
            }
            Some(row)
        }
    }
    //~ todo!("4a-c4: remove, restore, or write the masked columns back")
    // @end
}
