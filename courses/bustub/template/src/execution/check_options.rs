//! Extra checks a test run can switch on, such as `+ensure:nlj_init_check`.

use std::collections::HashSet;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum CheckOption {
    /// The right child of a nested loop join must be re-initialised once per left tuple.
    EnableNljCheck,
    /// A TopN executor must not keep more than N tuples.
    EnableTopnCheck,
}

#[derive(Clone, Debug, Default)]
pub struct CheckOptions {
    pub check_options_set: HashSet<CheckOption>,
}
