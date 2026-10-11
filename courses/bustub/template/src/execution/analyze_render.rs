//! The text of `EXPLAIN ANALYZE`: the plan tree with what each node did.

use super::analyze_stats::AnalyzeStats;
use super::plans::plan_node::PlanRef;
use std::sync::atomic::Ordering;

/// One line per plan node, the root first, children indented by two spaces under their parent:
///
/// ```text
/// Limit { limit=3 } (rows=3, batches=1, loops=1, time=0.042ms)
///   SeqScan { table=t } (rows=3, batches=1, loops=1, time=0.030ms)
/// ```
///
/// `time` is in milliseconds with three decimals. A node that was never executed prints `(never executed)` after its description.
pub fn render_analysis(plan: &PlanRef, stats: &AnalyzeStats) -> String {
    todo!("3i-06: the plan tree, root first, children indented, each line followed by its rows, batches, loops and time")
}
