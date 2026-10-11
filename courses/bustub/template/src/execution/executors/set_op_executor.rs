//! `UNION`, `INTERSECT` and `EXCEPT`, each with and without `ALL`.
//!
//! Rows are compared as *values*, and two NULLs are the same value here (unlike in `WHERE a = b`): `select null union select null` is one row.
//! Without `ALL` the result has no duplicates; with `ALL` rows are counted: `INTERSECT ALL` keeps a row as often as both sides have it (the smaller
//! count), `EXCEPT ALL` as often as the left has it more than the right.

use std::collections::{HashMap, HashSet, VecDeque};

use super::abstract_executor::{Executor, ExecutorBox, BUSTUB_BATCH_SIZE};
use super::nested_loop_join_executor::values_of;
use crate::catalog::schema::Schema;
use crate::common::exception::Result;
use crate::common::rid::Rid;
use crate::execution::plans::plan_node::{PlanKind, PlanRef};
use crate::sql::ast::SetOperator;
use crate::storage::table::tuple::Tuple;

pub struct SetOpExecutor<'e> {
    plan: PlanRef,
    op: SetOperator,
    all: bool,
    left: ExecutorBox<'e>,
    right: ExecutorBox<'e>,
    _set: (),
}

/// A key that is equal for equal rows (NULLs equal): the values with their types.
fn row_key(tuple: &Tuple, schema: &Schema) -> String {
    format!("{:?}", values_of(tuple, schema))
}

fn drain(child: &mut ExecutorBox<'_>) -> Result<Vec<Tuple>> {
    child.init()?;
    let (mut out, mut tuples, mut rids) = (Vec::new(), Vec::new(), Vec::new());
    while child.next(&mut tuples, &mut rids, BUSTUB_BATCH_SIZE)? {
        out.append(&mut tuples);
    }
    Ok(out)
}

impl<'e> SetOpExecutor<'e> {
    pub fn new(plan: PlanRef, left: ExecutorBox<'e>, right: ExecutorBox<'e>) -> SetOpExecutor<'e> {
        let PlanKind::SetOp { op, all } = &plan.kind else { unreachable!("a SetOpExecutor needs a SetOp plan") };
        let (op, all) = (*op, *all);
        SetOpExecutor {
            plan,
            op,
            all,
            left,
            right,
            _set: (),
        }
    }

    /// The rows of the result, from the rows of both sides (in the order the sides produced them).
    fn compute(&self, left_rows: Vec<Tuple>, right_rows: Vec<Tuple>) -> Vec<Tuple> {
        let (ls, rs) = (self.left.output_schema(), self.right.output_schema());
        match (self.op, self.all) {
            (SetOperator::Union, true) => todo!("3j-04: every row of the left side, then every row of the right side"),
            _ => todo!("3j-05: UNION without ALL (every distinct row once), INTERSECT [ALL] and EXCEPT [ALL] by counting rows"),
        }
    }
}

impl Executor for SetOpExecutor<'_> {
    fn init(&mut self) -> Result<()> {
        todo!("3j-04: read both children completely, compute the result (compute), and keep it as tuples of this node's schema")
    }

    fn next(&mut self, tuple_batch: &mut Vec<Tuple>, rid_batch: &mut Vec<Rid>, batch_size: usize) -> Result<bool> {
        tuple_batch.clear();
        rid_batch.clear();
        todo!("3j-04: hand out up to batch_size rows of the result")
    }

    fn output_schema(&self) -> &Schema {
        &self.plan.output_schema
    }
}
