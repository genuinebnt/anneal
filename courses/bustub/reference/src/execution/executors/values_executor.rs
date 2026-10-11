//! `VALUES (...), (...)`: one row per list of expressions.

use super::abstract_executor::Executor;
use crate::catalog::schema::Schema;
use crate::common::exception::Result;
use crate::common::rid::Rid;
use crate::execution::expressions::abstract_expression::ExprRef;
use crate::execution::plans::plan_node::{PlanKind, PlanRef};
use crate::storage::table::tuple::Tuple;

pub struct ValuesExecutor {
    plan: PlanRef,
    dummy_schema: Schema,
    cursor: usize,
}

impl ValuesExecutor {
    pub fn new(plan: PlanRef) -> ValuesExecutor {
        ValuesExecutor { plan, dummy_schema: Schema::new(vec![]), cursor: 0 }
    }

    fn rows(&self) -> &[Vec<ExprRef>] {
        match &self.plan.kind {
            PlanKind::Values { values } => values,
            _ => unreachable!("a ValuesExecutor needs a Values plan"),
        }
    }
}

impl Executor for ValuesExecutor {
    fn init(&mut self) -> Result<()> {
        self.cursor = 0;
        Ok(())
    }

    fn next(&mut self, tuple_batch: &mut Vec<Tuple>, rid_batch: &mut Vec<Rid>, batch_size: usize) -> Result<bool> {
        tuple_batch.clear();
        rid_batch.clear();
        let empty = Tuple::empty();
        while tuple_batch.len() < batch_size && self.cursor < self.rows().len() {
            let mut values = Vec::with_capacity(self.plan.output_schema.column_count() as usize);
            for col in &self.rows()[self.cursor] {
                values.push(col.evaluate(&empty, &self.dummy_schema)?);
            }
            tuple_batch.push(Tuple::new(&values, &self.plan.output_schema));
            rid_batch.push(Rid::default());
            self.cursor += 1;
        }
        Ok(!tuple_batch.is_empty())
    }

    fn output_schema(&self) -> &Schema {
        &self.plan.output_schema
    }
}
