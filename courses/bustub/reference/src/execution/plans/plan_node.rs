//! A plan is a tree of operators. One struct holds what every plan node has (an output schema and children) and an enum, [`PlanKind`], holds what
//! is particular to the operator. The optimizer builds new trees instead of changing old ones, so nodes are shared as `Arc<PlanNode>`
//! ([`PlanRef`]).

use std::collections::BTreeMap;
use std::fmt;
use std::sync::Arc;

use crate::binder::bound_order_by::OrderBy;
use crate::catalog::schema::{Schema, SchemaRef};
use crate::execution::expressions::abstract_expression::ExprRef;

pub type TableOid = u32;
pub type IndexOid = u32;
pub type PlanRef = Arc<PlanNode>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum JoinType {
    Invalid,
    Left,
    Right,
    Inner,
    Outer,
}

impl fmt::Display for JoinType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            JoinType::Invalid => "Invalid",
            JoinType::Left => "Left",
            JoinType::Right => "Right",
            JoinType::Inner => "Inner",
            JoinType::Outer => "Outer",
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum AggregationType {
    CountStarAggregate,
    CountAggregate,
    SumAggregate,
    MinAggregate,
    MaxAggregate,
}

impl fmt::Display for AggregationType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            AggregationType::CountStarAggregate => "count_star",
            AggregationType::CountAggregate => "count",
            AggregationType::SumAggregate => "sum",
            AggregationType::MinAggregate => "min",
            AggregationType::MaxAggregate => "max",
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum WindowFunctionType {
    CountStarAggregate,
    CountAggregate,
    SumAggregate,
    MinAggregate,
    MaxAggregate,
    Rank,
}

impl fmt::Display for WindowFunctionType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            WindowFunctionType::CountStarAggregate => "count_star",
            WindowFunctionType::CountAggregate => "count",
            WindowFunctionType::SumAggregate => "sum",
            WindowFunctionType::MinAggregate => "min",
            WindowFunctionType::MaxAggregate => "max",
            WindowFunctionType::Rank => "rank",
        })
    }
}

/// One window function of a `WindowFunction` plan: what to compute, over which partitions, in which order.
#[derive(Clone, Debug)]
pub struct WindowFunction {
    pub function: ExprRef,
    pub func_type: WindowFunctionType,
    pub partition_by: Vec<ExprRef>,
    pub order_by: Vec<OrderBy>,
}

/// Which operator a plan node is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlanType {
    SeqScan,
    IndexScan,
    Insert,
    Update,
    Delete,
    Aggregation,
    Limit,
    Offset,
    SetOp,
    NestedLoopJoin,
    NestedIndexJoin,
    HashJoin,
    Filter,
    Values,
    Projection,
    Sort,
    TopN,
    TopNPerGroup,
    MockScan,
    InitCheck,
    Window,
}

#[derive(Clone, Debug)]
pub enum PlanKind {
    /// Reads a whole table. `filter_predicate` is set by the optimizer when a filter is merged into the scan.
    SeqScan { table_oid: TableOid, table_name: String, filter_predicate: Option<ExprRef> },
    /// Reads a table through an index: a point lookup (`pred_keys`, one constant per lookup) or a full ordered scan (none).
    IndexScan { table_oid: TableOid, index_oid: IndexOid, filter_predicate: Option<ExprRef>, pred_keys: Vec<ExprRef> },
    /// Inserts the rows of its one child into the table; outputs one row: the count.
    Insert { table_oid: TableOid },
    /// Replaces each row of its one child with `target_expressions` evaluated on it; outputs the count.
    Update { table_oid: TableOid, target_expressions: Vec<ExprRef> },
    /// Deletes the rows of its one child from the table; outputs the count.
    Delete { table_oid: TableOid },
    /// `GROUP BY group_bys` computing `aggregates` (an expression and a type for each).
    Aggregation { group_bys: Vec<ExprRef>, aggregates: Vec<ExprRef>, agg_types: Vec<AggregationType> },
    Limit { limit: usize },
    /// Drops the first `offset` rows of its one child.
    Offset { offset: usize },
    /// Two children with the same columns: `UNION [ALL]`, `INTERSECT [ALL]` or `EXCEPT [ALL]` of their rows.
    SetOp { op: crate::sql::ast::SetOperator, all: bool },
    /// Two children (left, right) and a predicate over a pair of tuples.
    NestedLoopJoin { predicate: ExprRef, join_type: JoinType },
    /// One child (the outer side); the inner side is looked up in `index_oid` with `key_predicate`.
    NestedIndexJoin {
        key_predicate: ExprRef,
        inner_table_oid: TableOid,
        index_oid: IndexOid,
        index_name: String,
        index_table_name: String,
        inner_table_schema: SchemaRef,
        join_type: JoinType,
    },
    /// Two children and the equality keys of each side.
    HashJoin { left_key_expressions: Vec<ExprRef>, right_key_expressions: Vec<ExprRef>, join_type: JoinType },
    Filter { predicate: ExprRef },
    /// Rows of expressions evaluated without any input (`VALUES (1, 'a'), (2, 'b')`).
    Values { values: Vec<Vec<ExprRef>> },
    Projection { expressions: Vec<ExprRef> },
    Sort { order_bys: Vec<OrderBy> },
    TopN { order_bys: Vec<OrderBy>, n: usize },
    TopNPerGroup { group_bys: Vec<ExprRef>, order_bys: Vec<OrderBy>, n: usize },
    /// Reads one of the built-in `__mock_*` tables, which have no storage.
    MockScan { table: String },
    /// `columns` are the output expressions; a window function's slot holds a placeholder (`col_idx` = `u32::MAX`) and
    /// `window_functions` says, by output index, what to compute there.
    Window { columns: Vec<ExprRef>, window_functions: BTreeMap<u32, WindowFunction> },
}

#[derive(Clone, Debug)]
pub struct PlanNode {
    pub output_schema: SchemaRef,
    pub children: Vec<PlanRef>,
    pub kind: PlanKind,
}

fn list<T: fmt::Display>(items: &[T]) -> String {
    format!("[{}]", items.iter().map(|i| i.to_string()).collect::<Vec<_>>().join(", "))
}

impl PlanNode {
    pub fn new(output_schema: SchemaRef, children: Vec<PlanRef>, kind: PlanKind) -> PlanRef {
        Arc::new(PlanNode { output_schema, children, kind })
    }

    pub fn plan_type(&self) -> PlanType {
        match &self.kind {
            PlanKind::SeqScan { .. } => PlanType::SeqScan,
            PlanKind::IndexScan { .. } => PlanType::IndexScan,
            PlanKind::Insert { .. } => PlanType::Insert,
            PlanKind::Update { .. } => PlanType::Update,
            PlanKind::Delete { .. } => PlanType::Delete,
            PlanKind::Aggregation { .. } => PlanType::Aggregation,
            PlanKind::Limit { .. } => PlanType::Limit,
            PlanKind::Offset { .. } => PlanType::Offset,
            PlanKind::SetOp { .. } => PlanType::SetOp,
            PlanKind::NestedLoopJoin { .. } => PlanType::NestedLoopJoin,
            PlanKind::NestedIndexJoin { .. } => PlanType::NestedIndexJoin,
            PlanKind::HashJoin { .. } => PlanType::HashJoin,
            PlanKind::Filter { .. } => PlanType::Filter,
            PlanKind::Values { .. } => PlanType::Values,
            PlanKind::Projection { .. } => PlanType::Projection,
            PlanKind::Sort { .. } => PlanType::Sort,
            PlanKind::TopN { .. } => PlanType::TopN,
            PlanKind::TopNPerGroup { .. } => PlanType::TopNPerGroup,
            PlanKind::MockScan { .. } => PlanType::MockScan,
            PlanKind::Window { .. } => PlanType::Window,
        }
    }

    pub fn output_schema(&self) -> &Schema {
        &self.output_schema
    }

    /// The `idx`th child.
    pub fn child_at(&self, idx: usize) -> &PlanRef {
        &self.children[idx]
    }

    /// The same node with other children (the optimizer's `CloneWithChildren`).
    pub fn clone_with_children(&self, children: Vec<PlanRef>) -> PlanRef {
        Arc::new(PlanNode { children, ..self.clone() })
    }

    /// The same children and operator with another output schema.
    pub fn with_schema(&self, output_schema: SchemaRef) -> PlanRef {
        Arc::new(PlanNode { output_schema, ..self.clone() })
    }

    /// The one-line description of this operator (without its children), as `EXPLAIN` prints it.
    pub fn node_to_string(&self) -> String {
        self.plan_node_to_string()
    }

    /// The one-line description of this operator, as `EXPLAIN` prints it.
    fn plan_node_to_string(&self) -> String {
        match &self.kind {
            PlanKind::SeqScan { table_name, filter_predicate, .. } => match filter_predicate {
                Some(p) => format!("SeqScan {{ table={table_name}, filter={p} }}"),
                None => format!("SeqScan {{ table={table_name} }}"),
            },
            PlanKind::IndexScan { index_oid, filter_predicate, .. } => match filter_predicate {
                Some(p) => format!("IndexScan {{ index_oid={index_oid}, filter={p} }}"),
                None => format!("IndexScan {{ index_oid={index_oid} }}"),
            },
            PlanKind::Insert { table_oid } => format!("Insert {{ table_oid={table_oid} }}"),
            PlanKind::Update { table_oid, target_expressions } => {
                format!("Update {{ table_oid={table_oid}, target_exprs={} }}", list(target_expressions))
            }
            PlanKind::Delete { table_oid } => format!("Delete {{ table_oid={table_oid} }}"),
            PlanKind::Aggregation { group_bys, aggregates, agg_types } => {
                format!("Agg {{ types={}, aggregates={}, group_by={} }}", list(agg_types), list(aggregates), list(group_bys))
            }
            PlanKind::Limit { limit } => format!("Limit {{ limit={limit} }}"),
            PlanKind::Offset { offset } => format!("Offset {{ offset={offset} }}"),
            PlanKind::SetOp { op, all } => format!("SetOp {{ op={op:?}, all={all} }}"),
            PlanKind::NestedLoopJoin { predicate, join_type } => format!("NestedLoopJoin {{ type={join_type}, predicate={predicate} }}"),
            PlanKind::NestedIndexJoin { key_predicate, index_name, index_table_name, join_type, .. } => {
                format!("NestedIndexJoin {{ type={join_type}, key_predicate={key_predicate}, index={index_name}, index_table={index_table_name} }}")
            }
            PlanKind::HashJoin { left_key_expressions, right_key_expressions, join_type } => {
                format!("HashJoin {{ type={join_type}, left_key={}, right_key={} }}", list(left_key_expressions), list(right_key_expressions))
            }
            PlanKind::Filter { predicate } => format!("Filter {{ predicate={predicate} }}"),
            PlanKind::Values { values } => format!("Values {{ rows={} }}", values.len()),
            PlanKind::Projection { expressions } => format!("Projection {{ exprs={} }}", list(expressions)),
            // BusTub: "A sort plan node will be converted to an external merge sort executor", so that is its name.
            PlanKind::Sort { order_bys } => format!("ExternalMergeSort {{ order_bys={} }}", list(order_bys)),
            PlanKind::TopN { order_bys, n } => format!("TopN {{ n={n}, order_bys={}}}", list(order_bys)),
            PlanKind::TopNPerGroup { .. } => "TopNPerGroupPlan PlanNodeToString Not Implemented".to_string(),
            PlanKind::MockScan { table } => format!("MockScan {{ table={table} }}"),
            PlanKind::Window { columns, window_functions } => {
                let mut columns_str = String::new();
                for col in columns {
                    match col.as_any().downcast_ref::<crate::execution::expressions::column_value_expression::ColumnValueExpression>() {
                        Some(c) if c.col_idx() == u32::MAX => columns_str.push_str("placeholder, "),
                        _ => columns_str.push_str(&format!("{col}, ")),
                    }
                }
                let funcs: Vec<String> = window_functions
                    .iter()
                    .map(|(k, v)| {
                        format!(
                            "    {k}=>{{ function_arg={}, type={}, partition_by={}, order_by={} }}",
                            v.function,
                            v.func_type,
                            list(&v.partition_by),
                            list(&v.order_by)
                        )
                    })
                    .collect();
                format!("WindowFunc {{\n  columns={columns_str},\n  window_functions={{\n{}\n  }}\n}}", funcs.join(",\n"))
            }
        }
    }

    fn children_to_string(&self, indent: usize, with_schema: bool) -> String {
        if self.children.is_empty() {
            return String::new();
        }
        let pad = " ".repeat(indent);
        let mut lines = vec![];
        for child in &self.children {
            for line in child.to_string_with(with_schema).split('\n') {
                lines.push(format!("{pad}{line}"));
            }
        }
        format!("\n{}", lines.join("\n"))
    }

    /// The plan as text, `EXPLAIN`'s format: each operator on a line (`Op { details } | (schema)`), children indented by two.
    pub fn to_string_with(&self, with_schema: bool) -> String {
        if with_schema {
            format!("{} | {}{}", self.plan_node_to_string(), self.output_schema.to_string(true), self.children_to_string(2, with_schema))
        } else {
            format!("{}{}", self.plan_node_to_string(), self.children_to_string(2, with_schema))
        }
    }
}

impl fmt::Display for PlanNode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_string_with(true))
    }
}
