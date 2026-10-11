//! RIGHT and FULL OUTER joins as a nested loop join that remembers which right tuples found a partner.
//!
//! A nested loop join drives from the left side, which is enough for a LEFT join: a left tuple that matched nothing is padded with NULLs. A right
//! tuple that no left tuple matched is known only once **every** left tuple has been seen, so this join keeps the right side in memory with a flag
//! per tuple and reports the unmatched ones at the end.

use std::collections::VecDeque;

use super::abstract_executor::{Executor, ExecutorBox, TupleStream};
use super::nested_loop_join_executor::{nulls_for, values_of};
use crate::catalog::schema::Schema;
use crate::common::exception::Result;
use crate::common::rid::Rid;
use crate::execution::expressions::abstract_expression::ExprRef;
use crate::execution::plans::plan_node::{JoinType, PlanKind, PlanRef};
use crate::storage::table::tuple::Tuple;

pub struct OuterJoinExecutor<'e> {
    plan: PlanRef,
    predicate: ExprRef,
    /// `Right` or `Outer`.
    join_type: JoinType,
    left: TupleStream<'e>,
    right: ExecutorBox<'e>,
    // @begin 3j-01
    /// The whole right side, and for each tuple whether some left tuple matched it.
    right_rows: Vec<Tuple>,
    right_matched: Vec<bool>,
    /// Output made but not yet handed out.
    pending: VecDeque<Tuple>,
    left_done: bool,
    //~ _outer: (),
    // @end
}

impl<'e> OuterJoinExecutor<'e> {
    pub fn new(plan: PlanRef, left: ExecutorBox<'e>, right: ExecutorBox<'e>) -> OuterJoinExecutor<'e> {
        let PlanKind::NestedLoopJoin { predicate, join_type } = &plan.kind else { unreachable!("an OuterJoinExecutor needs a NestedLoopJoin plan") };
        let (predicate, join_type) = (predicate.clone(), *join_type);
        OuterJoinExecutor {
            plan,
            predicate,
            join_type,
            left: TupleStream::new(left),
            right,
            // @begin 3j-01
            right_rows: Vec::new(),
            right_matched: Vec::new(),
            pending: VecDeque::new(),
            left_done: false,
            //~ _outer: (),
            // @end
        }
    }

    fn joined(&self, left_values: &[crate::types::value::Value], right: &Tuple, right_schema: &Schema) -> Tuple {
        let mut values = left_values.to_vec();
        values.extend(values_of(right, right_schema));
        Tuple::new(&values, &self.plan.output_schema)
    }
}

impl Executor for OuterJoinExecutor<'_> {
    fn init(&mut self) -> Result<()> {
        // @begin 3j-01
        self.left.init()?;
        self.right.init()?;
        self.right_rows.clear();
        let (mut tuples, mut rids) = (Vec::new(), Vec::new());
        while self.right.next(&mut tuples, &mut rids, super::abstract_executor::BUSTUB_BATCH_SIZE)? {
            self.right_rows.append(&mut tuples);
        }
        self.right_matched = vec![false; self.right_rows.len()];
        self.pending.clear();
        self.left_done = false;
        Ok(())
        //~ todo!("3j-01: initialise both children, read the whole right side into memory with a flag per tuple, forget pending output")
        // @end
    }

    fn next(&mut self, tuple_batch: &mut Vec<Tuple>, rid_batch: &mut Vec<Rid>, batch_size: usize) -> Result<bool> {
        tuple_batch.clear();
        rid_batch.clear();
        // @begin 3j-01
        let (left_schema, right_schema) = (self.left.output_schema().clone(), self.right.output_schema().clone());
        while tuple_batch.len() < batch_size {
            if let Some(t) = self.pending.pop_front() {
                tuple_batch.push(t);
                rid_batch.push(Rid::default());
                continue;
            }
            if self.left_done {
                break;
            }
            match self.left.next()? {
                Some((l, _)) => {
                    let lv = values_of(&l, &left_schema);
                    let mut found = false;
                    for i in 0..self.right_rows.len() {
                        let r = &self.right_rows[i];
                        if self.predicate.evaluate_join(&l, &left_schema, r, &right_schema)?.as_bool() == Some(true) {
                            found = true;
                            self.right_matched[i] = true;
                            let t = self.joined(&lv, r, &right_schema);
                            self.pending.push_back(t);
                        }
                    }
                    if !found && self.join_type == JoinType::Outer {
                        let mut values = lv;
                        values.extend(nulls_for(&right_schema));
                        self.pending.push_back(Tuple::new(&values, &self.plan.output_schema));
                    }
                }
                None => {
                    // every left tuple has been seen: the right tuples that nobody matched, padded with NULLs on the left
                    self.left_done = true;
                    let nulls = nulls_for(&left_schema);
                    for i in 0..self.right_rows.len() {
                        if !self.right_matched[i] {
                            let t = self.joined(&nulls, &self.right_rows[i], &right_schema);
                            self.pending.push_back(t);
                        }
                    }
                }
            }
        }
        Ok(!tuple_batch.is_empty())
        //~ todo!("3j-01: until the batch is full: hand out pending output; take the next left tuple and compare it with every right tuple (predicate.evaluate_join), queueing the pairs and flagging the right tuples; a FULL join also queues a left tuple that matched nothing with NULLs on the right; when the left side ends, queue every unflagged right tuple with NULLs on the left")
        // @end
    }

    fn output_schema(&self) -> &Schema {
        &self.plan.output_schema
    }
}
