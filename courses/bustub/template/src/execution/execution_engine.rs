//! Runs a plan to completion.

use std::sync::atomic::Ordering;

use super::executor_context::ExecutorContext;
use super::executor_factory::create_executor;
use super::executors::abstract_executor::{ExecutorBox, BUSTUB_BATCH_SIZE};
use super::plans::plan_node::PlanRef;
use crate::common::exception::{ExceptionType, Result};
use crate::storage::table::tuple::Tuple;

pub struct ExecutionEngine;

impl ExecutionEngine {
    /// Runs `plan` and returns whether it succeeded and the tuples it produced.
    ///
    /// An `Execution` exception, a failure of this statement such as a transaction conflict, is a failed execution with no tuples; any other exception is passed on.
    pub fn execute<'e>(plan: &PlanRef, ctx: &'e ExecutorContext<'e>) -> Result<(bool, Vec<Tuple>)> {
        let mut executor = create_executor(ctx, plan)?;
        let mut result_set = vec![];
        match Self::run(&mut executor, &mut result_set).and_then(|_| Self::perform_checks(ctx)) {
            Ok(()) => Ok((true, result_set)),
            Err(e) if e.kind == ExceptionType::Execution => Ok((false, vec![])),
            Err(e) => Err(e),
        }
    }

    fn run(executor: &mut ExecutorBox<'_>, result_set: &mut Vec<Tuple>) -> Result<()> {
        executor.init()?;
        let (mut tuples, mut rids) = (vec![], vec![]);
        while executor.next(&mut tuples, &mut rids, BUSTUB_BATCH_SIZE)? {
            result_set.append(&mut tuples);
        }
        Ok(())
    }

    /// `+ensure:nlj_init_check`: the right child of a nested loop join was initialised once per left tuple (give or take one).
    fn perform_checks(ctx: &ExecutorContext<'_>) -> Result<()> {
        for (left, right) in ctx.nlj_check_exec_set() {
            assert!(
                right.n_init.load(Ordering::SeqCst) + 1 >= left.n_next.load(Ordering::SeqCst),
                "nlj check failed, are you initializing the right executor every time when there is a left tuple? (off-by-one is okay)"
            );
        }
        Ok(())
    }
}
