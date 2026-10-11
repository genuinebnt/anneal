//! Expressions after name resolution: every column reference names its table and column.

use std::fmt;

use super::bound_order_by::BoundOrderBy;
use crate::types::value::Value;

/// Where a window frame starts or ends.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WindowBoundary {
    Invalid,
    UnboundedPreceding,
    UnboundedFollowing,
    CurrentRowRange,
    CurrentRowRows,
    ExprPrecedingRows,
    ExprFollowingRows,
    ExprPrecedingRange,
    ExprFollowingRange,
}

#[derive(Clone, Debug)]
pub struct BoundWindow {
    pub func_name: String,
    pub args: Vec<BoundExpression>,
    pub partition_by: Vec<BoundExpression>,
    pub order_bys: Vec<BoundOrderBy>,
    pub start: WindowBoundary,
    pub end: WindowBoundary,
}

#[derive(Clone, Debug)]
pub enum BoundExpression {
    /// No expression: a missing `WHERE`, `LIMIT` and so on.
    Invalid,
    Constant(Value),
    /// `[table, column]`, `[alias, column]`, or just `[column]`.
    ColumnRef(Vec<String>),
    BinaryOp { op: String, left: Box<BoundExpression>, right: Box<BoundExpression> },
    UnaryOp { op: String, arg: Box<BoundExpression> },
    FuncCall { func_name: String, args: Vec<BoundExpression> },
    AggCall { func_name: String, is_distinct: bool, args: Vec<BoundExpression> },
    /// `CASE WHEN c THEN r ... [ELSE e] END`.
    Case { branches: Vec<(BoundExpression, BoundExpression)>, otherwise: Option<Box<BoundExpression>> },
    Alias { alias: String, child: Box<BoundExpression> },
    Window(Box<BoundWindow>),
    Star,
}

impl BoundExpression {
    pub fn is_invalid(&self) -> bool {
        matches!(self, BoundExpression::Invalid)
    }

    pub fn has_aggregation(&self) -> bool {
        match self {
            BoundExpression::AggCall { .. } => true,
            BoundExpression::Alias { child, .. } => child.has_aggregation(),
            BoundExpression::BinaryOp { left, right, .. } => left.has_aggregation() || right.has_aggregation(),
            BoundExpression::UnaryOp { arg, .. } => arg.has_aggregation(),
            BoundExpression::Case { branches, otherwise } => {
                branches.iter().any(|(c, r)| c.has_aggregation() || r.has_aggregation()) || otherwise.as_ref().is_some_and(|e| e.has_aggregation())
            }
            _ => false,
        }
    }

    pub fn has_window_function(&self) -> bool {
        match self {
            BoundExpression::Window(_) => true,
            BoundExpression::Alias { child, .. } => child.has_window_function(),
            _ => false,
        }
    }
}

fn join(items: &[BoundExpression]) -> String {
    items.iter().map(|e| e.to_string()).collect::<Vec<_>>().join(", ")
}

impl fmt::Display for BoundExpression {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BoundExpression::Invalid => Ok(()),
            BoundExpression::Constant(v) => write!(f, "{v}"),
            BoundExpression::ColumnRef(name) => write!(f, "{}", name.join(".")),
            BoundExpression::BinaryOp { op, left, right } => write!(f, "({left}{op}{right})"),
            // an operator written in words (`not`, `is null`) needs a space before its operand
            BoundExpression::UnaryOp { op, arg } if op.chars().all(|c| c.is_alphabetic() || c == ' ') => write!(f, "({op} {arg})"),
            BoundExpression::UnaryOp { op, arg } => write!(f, "({op}{arg})"),
            BoundExpression::Case { branches, otherwise } => {
                write!(f, "(case")?;
                for (c, r) in branches {
                    write!(f, " when {c} then {r}")?;
                }
                if let Some(e) = otherwise {
                    write!(f, " else {e}")?;
                }
                write!(f, " end)")
            }
            BoundExpression::FuncCall { func_name, args } => write!(f, "{func_name}({})", join(args)),
            BoundExpression::AggCall { func_name, is_distinct, args } => {
                write!(f, "{func_name}({}{})", if *is_distinct { "distinct " } else { "" }, join(args))
            }
            BoundExpression::Alias { alias, child } => write!(f, "({child} as {alias})"),
            BoundExpression::Window(w) => write!(
                f,
                "{}({}) over (partition by [{}] order by [{}] start={:?} end={:?})",
                w.func_name,
                join(&w.args),
                join(&w.partition_by),
                w.order_bys.iter().map(|o| o.to_string()).collect::<Vec<_>>().join(", "),
                w.start,
                w.end
            ),
            BoundExpression::Star => write!(f, "*"),
        }
    }
}
