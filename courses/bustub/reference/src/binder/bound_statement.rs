//! The statements the binder produces.

use std::fmt;

use super::bound_expression::BoundExpression;
use super::bound_order_by::BoundOrderBy;
use super::bound_table_ref::{BoundSubqueryRef, BoundTableRef};
use crate::catalog::column::Column;

#[derive(Clone, Debug)]
pub struct SelectStatement {
    pub table: BoundTableRef,
    pub select_list: Vec<BoundExpression>,
    pub where_: BoundExpression,
    pub group_by: Vec<BoundExpression>,
    pub having: BoundExpression,
    pub limit_count: BoundExpression,
    pub limit_offset: BoundExpression,
    pub sort: Vec<BoundOrderBy>,
    pub ctes: Vec<BoundSubqueryRef>,
    pub is_distinct: bool,
}

impl fmt::Display for SelectStatement {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let list = |items: &[BoundExpression]| items.iter().map(|e| e.to_string()).collect::<Vec<_>>().join(", ");
        write!(
            f,
            "BoundSelect {{ table={}, columns=[{}], groupBy=[{}], having={}, where={}, limit={}, offset={}, order_by=[{}], is_distinct={} }}",
            self.table,
            list(&self.select_list),
            list(&self.group_by),
            self.having,
            self.where_,
            self.limit_count,
            self.limit_offset,
            self.sort.iter().map(|o| o.to_string()).collect::<Vec<_>>().join(", "),
            self.is_distinct
        )
    }
}

/// Which sections `EXPLAIN` prints.
pub mod explain_options {
    pub const INVALID: u8 = 0;
    pub const BINDER: u8 = 1;
    pub const PLANNER: u8 = 2;
    pub const OPTIMIZER: u8 = 4;
    pub const SCHEMA: u8 = 8;
    pub const ANALYZE: u8 = 16;
}

#[derive(Clone, Debug)]
pub enum BoundStatement {
    Select(Box<SelectStatement>),
    /// `table` is always a `BoundTableRef::Base`.
    Insert { table: BoundTableRef, select: Box<SelectStatement> },
    Delete { table: BoundTableRef, expr: BoundExpression },
    Update { table: BoundTableRef, filter_expr: BoundExpression, target_expr: Vec<(Vec<String>, BoundExpression)> },
    Create { table: String, columns: Vec<Column>, primary_key: Vec<String> },
    Index { index_name: String, table: BoundTableRef, cols: Vec<Vec<String>>, index_type: String },
    Explain { statement: Box<BoundStatement>, options: u8 },
    VariableSet { variable: String, value: String },
    VariableShow { variable: String },
    /// `"begin"`, `"commit"` or `"abort"`.
    Transaction(String),
}

impl fmt::Display for BoundStatement {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BoundStatement::Select(s) => write!(f, "{s}"),
            BoundStatement::Insert { table, select } => write!(f, "BoundInsert {{ table={table}, select={select} }}"),
            BoundStatement::Delete { table, expr } => write!(f, "BoundDelete {{ table={table}, expr={expr} }}"),
            BoundStatement::Update { table, filter_expr, target_expr } => {
                let targets: Vec<String> = target_expr.iter().map(|(c, e)| format!("{}={e}", c.join("."))).collect();
                write!(f, "BoundUpdate {{ table={table}, filter_expr={filter_expr}, target_exprs=[{}] }}", targets.join(", "))
            }
            BoundStatement::Create { table, columns, primary_key } => write!(
                f,
                "BoundCreate {{ table={table}, columns=[{}], primary_key=[{}] }}",
                columns.iter().map(|c| c.to_string(true)).collect::<Vec<_>>().join(", "),
                primary_key.join(", ")
            ),
            BoundStatement::Index { index_name, table, cols, index_type } => write!(
                f,
                "BoundIndex {{ index_name={index_name}, table={table}, cols=[{}], index_type={index_type} }}",
                cols.iter().map(|c| c.join(".")).collect::<Vec<_>>().join(", ")
            ),
            BoundStatement::Explain { statement, .. } => write!(f, "BoundExplain {{ statement={statement} }}"),
            BoundStatement::VariableSet { variable, value } => write!(f, "BoundVariableSet {{ variable={variable}, value={value} }}"),
            BoundStatement::VariableShow { variable } => write!(f, "BoundVariableShow {{ variable={variable} }}"),
            BoundStatement::Transaction(kind) => write!(f, "BoundTransaction {{ type={kind} }}"),
        }
    }
}
