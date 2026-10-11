//! What a `FROM` clause refers to, once names are resolved.

use std::fmt;

use super::bound_expression::BoundExpression;
use super::bound_statement::SelectStatement;
use crate::catalog::schema::Schema;
use crate::execution::plans::plan_node::{JoinType, TableOid};
use crate::sql::ast::SetOperator;

#[derive(Clone, Debug)]
pub struct BoundSubqueryRef {
    pub subquery: Box<SelectStatement>,
    /// The name of each output column of the subquery, as a path (`["t1", "v1"]`, `["total"]`).
    pub select_list_name: Vec<Vec<String>>,
    pub alias: String,
}

#[derive(Clone, Debug)]
pub enum BoundTableRef {
    Invalid,
    /// `SELECT 1` has no FROM clause.
    Empty,
    Base { table: String, oid: TableOid, alias: Option<String>, schema: Schema },
    CrossProduct { left: Box<BoundTableRef>, right: Box<BoundTableRef> },
    Join { join_type: JoinType, left: Box<BoundTableRef>, right: Box<BoundTableRef>, condition: BoundExpression },
    Subquery(Box<BoundSubqueryRef>),
    /// `VALUES (...), (...)`.
    ExpressionList { values: Vec<Vec<BoundExpression>>, identifier: String },
    /// A reference to a `WITH` query: the name it was defined with, the name it is used under, and its columns.
    Cte { cte_name: String, alias: String, select_list_name: Vec<Vec<String>> },
    /// `left UNION [ALL] right` (or INTERSECT, EXCEPT) used as a table: its columns are named like the left query's, under `alias`.
    SetOp { op: SetOperator, all: bool, left: Box<BoundSubqueryRef>, right: Box<BoundSubqueryRef>, alias: String, select_list_name: Vec<Vec<String>> },
}

impl BoundTableRef {
    pub fn is_invalid(&self) -> bool {
        matches!(self, BoundTableRef::Invalid)
    }

    /// For a base table, the name its columns are qualified with: the alias if there is one, else the table name.
    pub fn bound_table_name(&self) -> Option<&str> {
        match self {
            BoundTableRef::Base { table, alias, .. } => Some(alias.as_deref().unwrap_or(table)),
            _ => None,
        }
    }
}

impl fmt::Display for BoundTableRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BoundTableRef::Invalid => write!(f, "Invalid"),
            BoundTableRef::Empty => write!(f, "<empty>"),
            BoundTableRef::Base { table, oid, alias, .. } => match alias {
                Some(a) => write!(f, "BoundBaseTableRef {{ table={table}, oid={oid}, alias={a} }}"),
                None => write!(f, "BoundBaseTableRef {{ table={table}, oid={oid} }}"),
            },
            BoundTableRef::CrossProduct { left, right } => write!(f, "BoundCrossProductRef {{ left={left}, right={right} }}"),
            BoundTableRef::Join { join_type, left, right, condition } => {
                write!(f, "BoundJoin {{ type={join_type}, left={left}, right={right}, condition={condition} }}")
            }
            BoundTableRef::Subquery(s) => write!(f, "BoundSubqueryRef {{ alias={}, subquery={} }}", s.alias, s.subquery),
            BoundTableRef::ExpressionList { values, identifier } => {
                let rows: Vec<String> =
                    values.iter().map(|row| format!("[{}]", row.iter().map(|e| e.to_string()).collect::<Vec<_>>().join(", "))).collect();
                write!(f, "BoundExpressionListRef {{ identifier={identifier}, values=[{}] }}", rows.join(", "))
            }
            BoundTableRef::Cte { cte_name, alias, .. } => write!(f, "BoundCTERef {{ cte_name={cte_name}, alias={alias} }}"),
            BoundTableRef::SetOp { op, all, left, right, .. } => {
                write!(f, "BoundSetOp {{ op={op:?}, all={all}, left={}, right={} }}", left.subquery, right.subquery)
            }
        }
    }
}
