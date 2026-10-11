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
    // @begin 3i-06
    fn walk(plan: &PlanRef, stats: &AnalyzeStats, depth: usize, out: &mut String) {
        out.push_str(&"  ".repeat(depth));
        out.push_str(&plan.node_to_string());
        match stats.get(plan) {
            Some(n) => out.push_str(&format!(
                " (rows={}, batches={}, loops={}, time={:.3}ms)",
                n.rows.load(Ordering::Relaxed),
                n.batches.load(Ordering::Relaxed),
                n.loops.load(Ordering::Relaxed),
                n.nanos.load(Ordering::Relaxed) as f64 / 1e6
            )),
            None => out.push_str(" (never executed)"),
        }
        out.push('\n');
        for child in &plan.children {
            walk(child, stats, depth + 1, out);
        }
    }
    let mut out = String::new();
    walk(plan, stats, 0, &mut out);
    out
    //~ todo!("3i-06: the plan tree, root first, children indented, each line followed by its rows, batches, loops and time")
    // @end
}
