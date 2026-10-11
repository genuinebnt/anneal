//! The optimizer rewrites a plan into one that produces the same result faster. Each *rule* is a function from a plan to a plan that looks for a
//! pattern (a filter on a nested loop join, a sort over a scan) and replaces it. Rules work bottom-up: optimise the children, rebuild the node,
//! then look at the node. Plans are never edited in place.

use std::sync::Arc;

use crate::binder::bound_order_by::OrderByType;
use crate::catalog::catalog::Catalog;
use crate::common::exception::Result;
use crate::execution::expressions::abstract_expression::{ExprRef, Expression};
use crate::execution::expressions::column_value_expression::ColumnValueExpression;
use crate::execution::expressions::comparison_expression::{ComparisonExpression, ComparisonType};
use crate::execution::expressions::constant_value_expression::ConstantValueExpression;
use crate::execution::expressions::logic_expression::{LogicExpression, LogicType};
use crate::execution::plans::plan_node::*;

pub struct Optimizer<'c, 'a> {
    pub catalog: &'c Catalog<'a>,
    force_starter_rule: bool,
}

impl<'c, 'a> Optimizer<'c, 'a> {
    pub fn new(catalog: &'c Catalog<'a>, force_starter_rule: bool) -> Optimizer<'c, 'a> {
        Optimizer { catalog, force_starter_rule }
    }

    /// Applies the rules in order. With `set force_optimizer_starter_rule=yes` a smaller rule set is used (a nested index join instead of a hash
    /// join, no top-N).
    pub fn optimize(&self, plan: &PlanRef) -> Result<PlanRef> {
        let mut p = plan.clone();
        if self.force_starter_rule {
            p = self.optimize_merge_projection(&p);
            p = self.optimize_merge_filter_nlj(&p);
            p = self.optimize_nlj_as_index_join(&p);
            p = self.optimize_order_by_as_index_scan(&p);
            p = self.optimize_merge_filter_scan(&p);
            p = self.optimize_seq_scan_as_index_scan(&p);
            return Ok(p);
        }
        p = self.optimize_merge_projection(&p);
        // @begin 3j-03
        p = self.optimize_outer_join_simplification(&p);
        // @end
        // @begin 3j-02
        p = self.optimize_filter_pushdown(&p);
        // @end
        p = self.optimize_merge_filter_nlj(&p);
        p = self.optimize_nlj_as_hash_join(&p);
        p = self.optimize_order_by_as_index_scan(&p);
        p = self.optimize_sort_limit_as_top_n(&p);
        p = self.optimize_merge_filter_scan(&p);
        p = self.optimize_seq_scan_as_index_scan(&p);
        Ok(p)
    }

    /// The number of rows assumed for a table, from the suffix of its name (`..._1m`, `..._100k`, ...), for choosing the order of a join.
    pub fn estimated_cardinality(table_name: &str) -> Option<usize> {
        [("_1m", 1_000_000), ("_100k", 100_000), ("_50k", 50_000), ("_10k", 10_000), ("_1k", 1_000), ("_100", 100)]
            .iter()
            .find(|(suffix, _)| table_name.ends_with(suffix))
            .map(|(_, n)| *n)
    }

    fn optimize_children(&self, plan: &PlanRef, rule: &dyn Fn(&Self, &PlanRef) -> PlanRef) -> PlanRef {
        let children: Vec<PlanRef> = plan.children.iter().map(|c| rule(self, c)).collect();
        plan.clone_with_children(children)
    }

    // ---- merge projection ---------------------------------------------------------------------------------------------------

    /// Merges a projection that does nothing but pass its child's columns through (what `SELECT *`, aggregation and renaming produce).
    pub fn optimize_merge_projection(&self, plan: &PlanRef) -> PlanRef {
        let optimized = self.optimize_children(plan, &Self::optimize_merge_projection);
        if let PlanKind::Projection { expressions } = &optimized.kind {
            assert_eq!(optimized.children.len(), 1, "Projection with multiple children?? That's weird!");
            let child = &optimized.children[0];
            let (child_cols, proj_cols) = (child.output_schema.columns(), optimized.output_schema.columns());
            if child_cols.len() == proj_cols.len() && child_cols.iter().zip(proj_cols).all(|(a, b)| a.type_id() == b.type_id()) {
                let identical = expressions.iter().enumerate().all(|(idx, e)| {
                    e.as_any().downcast_ref::<ColumnValueExpression>().is_some_and(|c| c.tuple_idx() == 0 && c.col_idx() as usize == idx)
                });
                if identical {
                    return child.with_schema(optimized.output_schema.clone());
                }
            }
        }
        optimized
    }

    // ---- merge filter into nested loop join ---------------------------------------------------------------------------------

    /// In `SELECT * FROM a, b WHERE a.x = b.y` the filter says `#0.x = #0.y`; in the join it must say `#0.x = #1.y`: work out which side
    /// each column belongs to.
    pub fn rewrite_expression_for_join(expr: &ExprRef, left_column_cnt: usize, right_column_cnt: usize) -> Result<ExprRef> {
        let children = expr.children().iter().map(|c| Self::rewrite_expression_for_join(c, left_column_cnt, right_column_cnt)).collect::<Result<Vec<_>>>()?;
        if let Some(cv) = expr.as_any().downcast_ref::<ColumnValueExpression>() {
            assert_eq!(cv.tuple_idx(), 0, "tuple_idx cannot be value other than 0 before this stage.");
            let col_idx = cv.col_idx() as usize;
            if col_idx < left_column_cnt {
                return Ok(Arc::new(ColumnValueExpression::new(0, cv.col_idx(), cv.return_type().clone())));
            }
            if col_idx < left_column_cnt + right_column_cnt {
                return Ok(Arc::new(ColumnValueExpression::new(1, (col_idx - left_column_cnt) as u32, cv.return_type().clone())));
            }
            return Err(crate::common::exception::Exception::new(crate::common::exception::ExceptionType::Invalid, "col_idx not in range"));
        }
        Ok(expr.clone_with_children(children))
    }

    /// Is the predicate the constant TRUE?
    pub fn is_predicate_true(expr: &ExprRef) -> bool {
        expr.as_any().downcast_ref::<ConstantValueExpression>().is_some_and(|c| c.val.as_bool() == Some(true))
    }

    /// The planner plans `FROM a, b WHERE ...` as a cross join (predicate TRUE) under a filter; put the filter into the join.
    pub fn optimize_merge_filter_nlj(&self, plan: &PlanRef) -> PlanRef {
        let optimized = self.optimize_children(plan, &Self::optimize_merge_filter_nlj);
        if let PlanKind::Filter { predicate } = &optimized.kind {
            let child = &optimized.children[0];
            if let PlanKind::NestedLoopJoin { predicate: join_predicate, join_type } = &child.kind {
                if Self::is_predicate_true(join_predicate) {
                    let (left, right) = (&child.children[0], &child.children[1]);
                    let rewritten = Self::rewrite_expression_for_join(predicate, left.output_schema.column_count() as usize, right.output_schema.column_count() as usize)
                        .expect("the filter's columns come from the join's inputs");
                    return PlanNode::new(
                        optimized.output_schema.clone(),
                        vec![left.clone(), right.clone()],
                        PlanKind::NestedLoopJoin { predicate: rewritten, join_type: *join_type },
                    );
                }
            }
        }
        optimized
    }

    // ---- merge filter into scan ---------------------------------------------------------------------------------------------

    /// A filter directly on a sequential scan becomes the scan's `filter_predicate`.
    pub fn optimize_merge_filter_scan(&self, plan: &PlanRef) -> PlanRef {
        let optimized = self.optimize_children(plan, &Self::optimize_merge_filter_scan);
        if let PlanKind::Filter { predicate } = &optimized.kind {
            let child = &optimized.children[0];
            if let PlanKind::SeqScan { table_oid, table_name, filter_predicate: None } = &child.kind {
                return PlanNode::new(
                    optimized.output_schema.clone(),
                    vec![],
                    PlanKind::SeqScan { table_oid: *table_oid, table_name: table_name.clone(), filter_predicate: Some(predicate.clone()) },
                );
            }
        }
        optimized
    }

    /// A filter whose predicate is always TRUE is dropped.
    pub fn optimize_eliminate_true_filter(&self, plan: &PlanRef) -> PlanRef {
        let optimized = self.optimize_children(plan, &Self::optimize_eliminate_true_filter);
        if let PlanKind::Filter { predicate } = &optimized.kind {
            if Self::is_predicate_true(predicate) {
                return optimized.children[0].clone();
            }
        }
        optimized
    }

    // ---- indexes ------------------------------------------------------------------------------------------------------------

    /// The index on exactly the column `index_key_idx` of `table_name`, if there is one: (its oid, its name).
    pub fn match_index(&self, table_name: &str, index_key_idx: u32) -> Option<(IndexOid, String)> {
        self.catalog
            .get_table_indexes(table_name)
            .iter()
            .find(|i| i.index.metadata().get_key_attrs() == [index_key_idx])
            .map(|i| (i.index_oid, i.name.clone()))
    }

    /// `a JOIN b ON a.x = b.y` where `b` is a scan with an index on `y` becomes a nested index join (probe the index for each `a`).
    pub fn optimize_nlj_as_index_join(&self, plan: &PlanRef) -> PlanRef {
        let optimized = self.optimize_children(plan, &Self::optimize_nlj_as_index_join);
        if let PlanKind::NestedLoopJoin { predicate, join_type } = &optimized.kind {
            assert_eq!(optimized.children.len(), 2, "NLJ should have exactly 2 children.");
            if let Some(cmp) = predicate.as_any().downcast_ref::<ComparisonExpression>() {
                if cmp.comp_type == ComparisonType::Equal {
                    let (l, r) = (cmp.children()[0].as_any().downcast_ref::<ColumnValueExpression>(), cmp.children()[1].as_any().downcast_ref::<ColumnValueExpression>());
                    if let (Some(left_expr), Some(right_expr)) = (l, r) {
                        let right_plan = &optimized.children[1];
                        if let PlanKind::SeqScan { table_oid, table_name, .. } = &right_plan.kind {
                            // (outer = the side that reads the left input, inner = the side that reads the right input)
                            let sides = if left_expr.tuple_idx() == 0 && right_expr.tuple_idx() == 1 {
                                Some((left_expr, right_expr))
                            } else if left_expr.tuple_idx() == 1 && right_expr.tuple_idx() == 0 {
                                Some((right_expr, left_expr))
                            } else {
                                None
                            };
                            if let Some((outer_expr, inner_expr)) = sides {
                                if let Some((index_oid, index_name)) = self.match_index(table_name, inner_expr.col_idx()) {
                                    let key: ExprRef = Arc::new(ColumnValueExpression::new(0, outer_expr.col_idx(), outer_expr.return_type().clone()));
                                    return PlanNode::new(
                                        optimized.output_schema.clone(),
                                        vec![optimized.children[0].clone()],
                                        PlanKind::NestedIndexJoin {
                                            key_predicate: key,
                                            inner_table_oid: *table_oid,
                                            index_oid,
                                            index_name,
                                            index_table_name: table_name.clone(),
                                            inner_table_schema: right_plan.output_schema.clone(),
                                            join_type: *join_type,
                                        },
                                    );
                                }
                            }
                        }
                    }
                }
            }
        }
        optimized
    }

    /// `ORDER BY` columns that an index on the same columns already delivers in order: a sort over a scan becomes an index scan.
    pub fn optimize_order_by_as_index_scan(&self, plan: &PlanRef) -> PlanRef {
        let optimized = self.optimize_children(plan, &Self::optimize_order_by_as_index_scan);
        if let PlanKind::Sort { order_bys } = &optimized.kind {
            let mut order_by_column_ids = vec![];
            for ob in order_bys {
                if ob.order_type != OrderByType::Asc && ob.order_type != OrderByType::Default {
                    return optimized;
                }
                match ob.expr.as_any().downcast_ref::<ColumnValueExpression>() {
                    Some(cv) => order_by_column_ids.push(cv.col_idx()),
                    None => return optimized,
                }
            }
            let child = &optimized.children[0];
            if let PlanKind::SeqScan { table_oid, .. } = &child.kind {
                let table_info = self.catalog.get_table_by_oid(*table_oid).expect("the plan names a table that exists");
                for index in self.catalog.get_table_indexes(&table_info.name) {
                    if order_by_column_ids == index.index.metadata().get_key_attrs() {
                        return PlanNode::new(
                            optimized.output_schema.clone(),
                            vec![],
                            PlanKind::IndexScan { table_oid: table_info.oid, index_oid: index.index_oid, filter_predicate: None, pred_keys: vec![] },
                        );
                    }
                }
            }
        }
        optimized
    }


    // ---- filters and outer joins --------------------------------------------------------------------------------

    /// The `col_idx` of every column the expression reads (all with tuple index 0: this is a filter, before it is merged into a join).
    pub fn columns_read(expr: &ExprRef, out: &mut Vec<u32>) {
        if let Some(c) = expr.as_any().downcast_ref::<ColumnValueExpression>() {
            out.push(c.col_idx());
        }
        for child in expr.children() {
            Self::columns_read(child, out);
        }
    }

    /// The same expression with every column index moved down by `by` (the columns of the right side of a join, as the right child sees them).
    pub fn shift_columns(expr: &ExprRef, by: u32) -> ExprRef {
        if let Some(c) = expr.as_any().downcast_ref::<ColumnValueExpression>() {
            return Arc::new(ColumnValueExpression::new(c.tuple_idx(), c.col_idx() - by, c.return_type().clone()));
        }
        let children = expr.children().iter().map(|c| Self::shift_columns(c, by)).collect();
        expr.clone_with_children(children)
    }

    /// `a AND b AND ...` (the constant TRUE for an empty list).
    pub fn and_all(parts: Vec<ExprRef>) -> ExprRef {
        let mut it = parts.into_iter();
        let Some(first) = it.next() else {
            return Arc::new(ConstantValueExpression::new(crate::types::value::Value::boolean(true)));
        };
        it.fold(first, |acc, e| Arc::new(LogicExpression::new(acc, e, LogicType::And).expect("conjuncts are booleans")))
    }

    /// A filter above a join, split so that the parts that can be applied to one input below the join are. Which parts may move depends on
    /// the join: an INNER join can take parts on either side; a LEFT join only parts on the left (a part on the right would remove
    /// right tuples before the join pads the unmatched left ones with NULLs, which a filter above the join would then have removed);
    /// a RIGHT join only parts on the right; a FULL join none. What cannot move stays in a filter above.
    pub fn optimize_filter_pushdown(&self, plan: &PlanRef) -> PlanRef {
        // @begin 3j-02
        if let PlanKind::Filter { predicate } = &plan.kind {
            let child = &plan.children[0];
            if let PlanKind::NestedLoopJoin { predicate: join_predicate, join_type } = &child.kind {
                if !Self::is_predicate_true(join_predicate) || *join_type == JoinType::Inner {
                    let (left, right) = (&child.children[0], &child.children[1]);
                    let nl = left.output_schema.column_count();
                    let (can_left, can_right) = match join_type {
                        JoinType::Inner => (true, true),
                        JoinType::Left => (true, false),
                        JoinType::Right => (false, true),
                        _ => (false, false),
                    };
                    let mut parts = vec![];
                    Self::conjuncts(predicate, &mut parts);
                    let (mut to_left, mut to_right, mut stay) = (vec![], vec![], vec![]);
                    for part in parts {
                        let mut cols = vec![];
                        Self::columns_read(&part, &mut cols);
                        if !cols.is_empty() && can_left && cols.iter().all(|c| *c < nl) {
                            to_left.push(part);
                        } else if !cols.is_empty() && can_right && cols.iter().all(|c| *c >= nl) {
                            to_right.push(Self::shift_columns(&part, nl));
                        } else {
                            stay.push(part);
                        }
                    }
                    if !to_left.is_empty() || !to_right.is_empty() {
                        let wrap = |input: &PlanRef, parts: Vec<ExprRef>| -> PlanRef {
                            if parts.is_empty() {
                                input.clone()
                            } else {
                                PlanNode::new(input.output_schema.clone(), vec![input.clone()], PlanKind::Filter { predicate: Self::and_all(parts) })
                            }
                        };
                        let join = child.clone_with_children(vec![wrap(left, to_left), wrap(right, to_right)]);
                        let rebuilt = if stay.is_empty() {
                            join
                        } else {
                            PlanNode::new(plan.output_schema.clone(), vec![join], PlanKind::Filter { predicate: Self::and_all(stay) })
                        };
                        // the pushed filters may sit on another join: go on below
                        return self.optimize_children(&rebuilt, &Self::optimize_filter_pushdown);
                    }
                }
            }
        }
        self.optimize_children(plan, &Self::optimize_filter_pushdown)
        //~ plan.clone() // 3j-02: a Filter whose child is a NestedLoopJoin: split the predicate into conjuncts (Optimizer::conjuncts), find which columns each reads (columns_read) and move the ones that only read one input below the join when the join type allows it (see the doc comment), shifting the right side's columns (shift_columns); keep the rest above (and_all); then apply the rule to the new children; any other node: apply it to the children
        // @end
    }

    /// Is `expr` certainly NULL when every one of the columns `cols` is NULL? (Arithmetic, comparisons, LIKE and `||` are: their operand is.)
    pub fn is_null_when(expr: &ExprRef, cols: &dyn Fn(u32) -> bool) -> bool {
        // @begin 3j-03
        use crate::execution::expressions::like_expression::LikeExpression;
        use crate::execution::expressions::operator_expression::{Operator, OperatorExpression};
        if let Some(c) = expr.as_any().downcast_ref::<ColumnValueExpression>() {
            return cols(c.col_idx());
        }
        let strict = expr.as_any().downcast_ref::<ComparisonExpression>().is_some()
            || expr.as_any().downcast_ref::<LikeExpression>().is_some()
            || expr.as_any().downcast_ref::<crate::execution::expressions::arithmetic_expression::ArithmeticExpression>().is_some()
            || expr.as_any().downcast_ref::<OperatorExpression>().is_some_and(|o| !matches!(o.op, Operator::IsNull | Operator::IsNotNull));
        strict && expr.children().iter().any(|c| Self::is_null_when(c, cols))
        //~ false // 3j-03: a column of the set is; a comparison, LIKE, arithmetic or `||` (but not IS NULL / IS NOT NULL) is when any of its operands is
        // @end
    }

    /// Does the condition fail (is not TRUE) whenever every column of `cols` is NULL? This is what lets a filter above an outer join remove
    /// the rows the join padded with NULLs.
    pub fn rejects_nulls(expr: &ExprRef, cols: &dyn Fn(u32) -> bool) -> bool {
        // @begin 3j-03
        use crate::execution::expressions::operator_expression::{Operator, OperatorExpression};
        if let Some(l) = expr.as_any().downcast_ref::<LogicExpression>() {
            let (a, b) = (Self::rejects_nulls(&expr.children()[0], cols), Self::rejects_nulls(&expr.children()[1], cols));
            return match l.logic_type {
                LogicType::And => a || b,
                LogicType::Or => a && b,
            };
        }
        if let Some(o) = expr.as_any().downcast_ref::<OperatorExpression>() {
            if o.op == Operator::IsNotNull {
                return Self::is_null_when(&expr.children()[0], cols);
            }
        }
        // a comparison, LIKE or arithmetic condition is NULL (not TRUE) when an operand is NULL
        Self::is_null_when(expr, cols)
        //~ false // 3j-03: AND rejects if either side does, OR if both do; `x IS NOT NULL` if x is NULL when the columns are; anything else if is_null_when says its value is NULL; NOT and IS NULL never (be careful: they can be TRUE for NULL)
        // @end
    }

    /// The type a join has once a filter above it rejects NULL-padded rows on the given sides.
    pub fn simplified_join_type(join_type: JoinType, rejects_left_nulls: bool, rejects_right_nulls: bool) -> JoinType {
        // @begin 3j-03
        match join_type {
            JoinType::Left if rejects_right_nulls => JoinType::Inner,
            JoinType::Right if rejects_left_nulls => JoinType::Inner,
            JoinType::Outer => match (rejects_left_nulls, rejects_right_nulls) {
                (true, true) => JoinType::Inner,
                (false, true) => JoinType::Right,
                (true, false) => JoinType::Left,
                (false, false) => JoinType::Outer,
            },
            other => other,
        }
        //~ join_type // 3j-03: LEFT pads the right with NULLs: if the filter rejects those rows it is an INNER join; RIGHT likewise for the left side; FULL pads both sides: each side the filter rejects is dropped (rejects the right padding: RIGHT join; the left: LEFT join; both: INNER)
        // @end
    }

    /// A filter above an outer join that fails on every NULL-padded row makes the padding pointless: the join is cheaper as an INNER (or a
    /// one-sided) join, and may be reordered or pushed into like one.
    pub fn optimize_outer_join_simplification(&self, plan: &PlanRef) -> PlanRef {
        // @begin 3j-03
        if let PlanKind::Filter { predicate } = &plan.kind {
            let child = &plan.children[0];
            if let PlanKind::NestedLoopJoin { predicate: join_predicate, join_type } = &child.kind {
                if *join_type != JoinType::Inner {
                    let nl = child.children[0].output_schema.column_count();
                    let mut parts = vec![];
                    Self::conjuncts(predicate, &mut parts);
                    let (is_left, is_right): (&dyn Fn(u32) -> bool, &dyn Fn(u32) -> bool) = (&|c| c < nl, &|c| c >= nl);
                    let rejects_left = parts.iter().any(|p| Self::rejects_nulls(p, is_left));
                    let rejects_right = parts.iter().any(|p| Self::rejects_nulls(p, is_right));
                    let new_type = Self::simplified_join_type(*join_type, rejects_left, rejects_right);
                    if new_type != *join_type {
                        let join = PlanNode::new(
                            child.output_schema.clone(),
                            child.children.clone(),
                            PlanKind::NestedLoopJoin { predicate: join_predicate.clone(), join_type: new_type },
                        );
                        let rebuilt = plan.clone_with_children(vec![join]);
                        return self.optimize_children(&rebuilt, &Self::optimize_outer_join_simplification);
                    }
                }
            }
        }
        self.optimize_children(plan, &Self::optimize_outer_join_simplification)
        //~ plan.clone() // 3j-03: a Filter over a NestedLoopJoin that is not INNER: does some conjunct reject NULLs on the left columns (col_idx < the left child's column count)? on the right? Then change the join type (simplified_join_type); apply the rule to the children
        // @end
    }

    // ---- the rules you write ------------------------------------------------------------------------------------------------

    /// Splits a predicate into its conjuncts: `a AND (b AND c)` is `[a, b, c]`; anything else (an `OR`, a comparison) is one conjunct.
    pub fn conjuncts(expr: &ExprRef, out: &mut Vec<ExprRef>) {
        match expr.as_any().downcast_ref::<LogicExpression>() {
            Some(l) if l.logic_type == LogicType::And => {
                for c in expr.children() {
                    Self::conjuncts(c, out);
                }
            }
            _ => out.push(expr.clone()),
        }
    }

    /// If **every** conjunct of a join predicate is `left column = right column` (written either way round), the key expressions of the
    /// two sides, in the same order: `(left keys, right keys)`. Anything else (another operator, an `OR`, both columns on one side, a
    /// constant) makes it `None`: the join cannot be a hash join.
    pub fn extract_equi_join_keys(predicate: &ExprRef) -> Option<(Vec<ExprRef>, Vec<ExprRef>)> {
        // @begin 3h-01
        let mut parts = vec![];
        Self::conjuncts(predicate, &mut parts);
        let (mut left_keys, mut right_keys): (Vec<ExprRef>, Vec<ExprRef>) = (vec![], vec![]);
        for part in &parts {
            let cmp = part.as_any().downcast_ref::<ComparisonExpression>()?;
            if cmp.comp_type != ComparisonType::Equal {
                return None;
            }
            let a = cmp.children()[0].as_any().downcast_ref::<ColumnValueExpression>()?;
            let b = cmp.children()[1].as_any().downcast_ref::<ColumnValueExpression>()?;
            let (left, right) = match (a.tuple_idx(), b.tuple_idx()) {
                (0, 1) => (a, b),
                (1, 0) => (b, a),
                _ => return None,
            };
            left_keys.push(Arc::new(ColumnValueExpression::new(0, left.col_idx(), left.return_type().clone())));
            right_keys.push(Arc::new(ColumnValueExpression::new(0, right.col_idx(), right.return_type().clone())));
        }
        if left_keys.is_empty() {
            return None;
        }
        Some((left_keys, right_keys))
        //~ None // 3h-01: split the predicate into conjuncts (Optimizer::conjuncts); each must be a ComparisonExpression of type Equal whose two children are ColumnValueExpressions, one reading tuple 0 (left) and the other tuple 1 (right); collect the left columns and the right columns (as ColumnValueExpression(0, col_idx, type)); None if any conjunct is anything else
        // @end
    }

    /// Turns a nested loop join whose predicate is equalities between the two sides into a hash join.
    pub fn optimize_nlj_as_hash_join(&self, plan: &PlanRef) -> PlanRef {
        // @begin 3h-01
        let optimized = self.optimize_children(plan, &Self::optimize_nlj_as_hash_join);
        if let PlanKind::NestedLoopJoin { predicate, join_type } = &optimized.kind {
            if matches!(join_type, JoinType::Inner | JoinType::Left) {
                if let Some((left_keys, right_keys)) = Self::extract_equi_join_keys(predicate) {
                    return PlanNode::new(
                        optimized.output_schema.clone(),
                        optimized.children.clone(),
                        PlanKind::HashJoin { left_key_expressions: left_keys, right_key_expressions: right_keys, join_type: *join_type },
                    );
                }
            }
        }
        optimized
        //~ plan.clone() // 3h-01: optimize the children first (self.optimize_children(plan, &Self::optimize_nlj_as_hash_join)); then, for an INNER or LEFT NestedLoopJoin whose predicate gives equi-join keys, a HashJoin with the same schema, children and join type; otherwise the node unchanged
        // @end
    }

    /// Turns `Limit(Sort(child))` into `TopN(child)`.
    pub fn optimize_sort_limit_as_top_n(&self, plan: &PlanRef) -> PlanRef {
        // @begin 3h-02
        let optimized = self.optimize_children(plan, &Self::optimize_sort_limit_as_top_n);
        if let PlanKind::Limit { limit } = &optimized.kind {
            let child = &optimized.children[0];
            if let PlanKind::Sort { order_bys } = &child.kind {
                return PlanNode::new(
                    optimized.output_schema.clone(),
                    child.children.clone(),
                    PlanKind::TopN { order_bys: order_bys.clone(), n: *limit },
                );
            }
        }
        optimized
        //~ plan.clone() // 3h-02: optimize the children first; then a Limit whose child is a Sort becomes a TopN with the limit's output schema, the sort's order-bys, n = the limit, and the sort's children
        // @end
    }

    /// If the predicate is `column = constant` (either way round) or an `OR` of such equalities on the **same** column: that column's
    /// index in the scan's output and the constant expressions, in order. Otherwise `None`.
    pub fn extract_point_lookup(predicate: &ExprRef) -> Option<(u32, Vec<ExprRef>)> {
        // @begin 3h-03
        if let Some(cmp) = predicate.as_any().downcast_ref::<ComparisonExpression>() {
            if cmp.comp_type != ComparisonType::Equal {
                return None;
            }
            let (a, b) = (&cmp.children()[0], &cmp.children()[1]);
            let split = |col: &ExprRef, constant: &ExprRef| -> Option<(u32, Vec<ExprRef>)> {
                let c = col.as_any().downcast_ref::<ColumnValueExpression>()?;
                constant.as_any().downcast_ref::<ConstantValueExpression>()?;
                Some((c.col_idx(), vec![constant.clone()]))
            };
            return split(a, b).or_else(|| split(b, a));
        }
        if let Some(l) = predicate.as_any().downcast_ref::<LogicExpression>() {
            if l.logic_type == LogicType::Or {
                let (c1, mut k1) = Self::extract_point_lookup(&predicate.children()[0])?;
                let (c2, k2) = Self::extract_point_lookup(&predicate.children()[1])?;
                if c1 != c2 {
                    return None;
                }
                k1.extend(k2);
                return Some((c1, k1));
            }
        }
        None
        //~ None // 3h-03: an Equal comparison between a ColumnValueExpression and a ConstantValueExpression (either order) gives (the column's index, [the constant]); an OR of two such lookups on the same column gives the same column and the keys of both, left first; anything else None
        // @end
    }

    /// Turns a scan with a point-lookup predicate on an indexed column into an index scan.
    pub fn optimize_seq_scan_as_index_scan(&self, plan: &PlanRef) -> PlanRef {
        // @begin 3h-03
        let optimized = self.optimize_children(plan, &Self::optimize_seq_scan_as_index_scan);
        if let PlanKind::SeqScan { table_oid, table_name, filter_predicate: Some(predicate) } = &optimized.kind {
            // the whole predicate, or the first conjunct of an AND that is a point lookup
            let mut candidates = vec![];
            Self::conjuncts(predicate, &mut candidates);
            if candidates.len() > 1 {
                candidates.insert(0, predicate.clone());
            }
            for candidate in candidates {
                if let Some((col, keys)) = Self::extract_point_lookup(&candidate) {
                    if let Some((index_oid, _)) = self.match_index(table_name, col) {
                        return PlanNode::new(
                            optimized.output_schema.clone(),
                            vec![],
                            PlanKind::IndexScan { table_oid: *table_oid, index_oid, filter_predicate: Some(predicate.clone()), pred_keys: keys },
                        );
                    }
                }
            }
        }
        optimized
        //~ plan.clone() // 3h-03: optimize the children first; then a SeqScan with a filter predicate that is (or has an AND-conjunct that is) a point lookup on a column with an index (self.match_index(table_name, column)) becomes an IndexScan with the scan's schema, that index, the keys as pred_keys, and the WHOLE predicate as filter_predicate; otherwise unchanged
        // @end
    }
}
