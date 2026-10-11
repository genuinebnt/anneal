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
    // @begin 3j-04
    /// The result, made when the executor is initialised and handed out in batches.
    rows: VecDeque<Tuple>,
    //~ _set: (),
    // @end
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
            // @begin 3j-04
            rows: VecDeque::new(),
            //~ _set: (),
            // @end
        }
    }

    /// The rows of the result, from the rows of both sides (in the order the sides produced them).
    fn compute(&self, left_rows: Vec<Tuple>, right_rows: Vec<Tuple>) -> Vec<Tuple> {
        let (ls, rs) = (self.left.output_schema(), self.right.output_schema());
        match (self.op, self.all) {
            // @begin 3j-04
            (SetOperator::Union, true) => left_rows.into_iter().chain(right_rows).collect(),
            //~ (SetOperator::Union, true) => todo!("3j-04: every row of the left side, then every row of the right side"),
            // @end
            // @begin 3j-05
            (SetOperator::Union, false) => {
                let mut seen = HashSet::new();
                let mut out = Vec::new();
                for (t, schema) in left_rows.into_iter().map(|t| (t, ls)).chain(right_rows.into_iter().map(|t| (t, rs))) {
                    if seen.insert(row_key(&t, schema)) {
                        out.push(t);
                    }
                }
                out
            }
            (SetOperator::Intersect, all) => {
                let mut available: HashMap<String, usize> = HashMap::new();
                for t in &right_rows {
                    *available.entry(row_key(t, rs)).or_insert(0) += 1;
                }
                let mut emitted = HashSet::new();
                let mut out = Vec::new();
                for t in left_rows {
                    let key = row_key(&t, ls);
                    match available.get_mut(&key) {
                        Some(n) if *n > 0 && (all || emitted.insert(key.clone())) => {
                            if all {
                                *n -= 1;
                            }
                            out.push(t);
                        }
                        _ => {}
                    }
                }
                out
            }
            (SetOperator::Except, all) => {
                let mut counts: HashMap<String, usize> = HashMap::new();
                for t in &right_rows {
                    *counts.entry(row_key(t, rs)).or_insert(0) += 1;
                }
                let mut emitted = HashSet::new();
                let mut out = Vec::new();
                for t in left_rows {
                    let key = row_key(&t, ls);
                    let blocked = match counts.get_mut(&key) {
                        Some(n) if *n > 0 => {
                            if all {
                                *n -= 1;
                            }
                            true
                        }
                        _ => false,
                    };
                    if !blocked && (all || emitted.insert(key)) {
                        out.push(t);
                    }
                }
                out
            }
            //~ _ => todo!("3j-05: UNION without ALL (every distinct row once), INTERSECT [ALL] and EXCEPT [ALL] by counting rows"),
            // @end
        }
    }
}

impl Executor for SetOpExecutor<'_> {
    fn init(&mut self) -> Result<()> {
        // @begin 3j-04
        let left_rows = drain(&mut self.left)?;
        let right_rows = drain(&mut self.right)?;
        let schema = self.plan.output_schema.clone();
        let (ls, _) = (self.left.output_schema().clone(), 0);
        // the result is written in this node's own schema (the sides may declare different string lengths)
        self.rows = self
            .compute(left_rows, right_rows)
            .into_iter()
            .map(|t| Tuple::new(&values_of(&t, &ls), &schema))
            .collect();
        Ok(())
        //~ todo!("3j-04: read both children completely, compute the result (compute), and keep it as tuples of this node's schema")
        // @end
    }

    fn next(&mut self, tuple_batch: &mut Vec<Tuple>, rid_batch: &mut Vec<Rid>, batch_size: usize) -> Result<bool> {
        tuple_batch.clear();
        rid_batch.clear();
        // @begin 3j-04
        while tuple_batch.len() < batch_size {
            match self.rows.pop_front() {
                Some(t) => {
                    tuple_batch.push(t);
                    rid_batch.push(Rid::default());
                }
                None => break,
            }
        }
        Ok(!tuple_batch.is_empty())
        //~ todo!("3j-04: hand out up to batch_size rows of the result")
        // @end
    }

    fn output_schema(&self) -> &Schema {
        &self.plan.output_schema
    }
}
