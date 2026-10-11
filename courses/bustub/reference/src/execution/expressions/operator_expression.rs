//! `*`, `/`, `%`, `||`, unary `-`, `NOT` and `IS [NOT] NULL`, and `+` and `-` on every numeric type. One expression type with an [`Operator`] tag.

use std::any::Any;
use std::sync::Arc;

use super::abstract_expression::{ExprRef, Expression};
use crate::catalog::column::Column;
use crate::catalog::schema::Schema;
use crate::common::exception::{Exception, ExceptionType, Result};
use crate::storage::table::tuple::Tuple;
use crate::types::type_id::TypeId;
use crate::types::value::Value;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Operator {
    Add,
    Subtract,
    Multiply,
    Divide,
    Modulo,
    /// `a || b` on strings.
    Concat,
    /// `-a`.
    Negate,
    Not,
    IsNull,
    IsNotNull,
}

impl Operator {
    /// How many operands the operator takes.
    pub fn arity(self) -> usize {
        match self {
            Operator::Negate | Operator::Not | Operator::IsNull | Operator::IsNotNull => 1,
            _ => 2,
        }
    }

    /// The operator as `EXPLAIN` prints it.
    pub fn symbol(self) -> &'static str {
        match self {
            Operator::Add => "+",
            Operator::Subtract => "-",
            Operator::Multiply => "*",
            Operator::Divide => "/",
            Operator::Modulo => "%",
            Operator::Concat => "||",
            Operator::Negate => "-",
            Operator::Not => "not ",
            Operator::IsNull => " is null",
            Operator::IsNotNull => " is not null",
        }
    }
}

#[derive(Clone, Debug)]
pub struct OperatorExpression {
    pub op: Operator,
    children: Vec<ExprRef>,
    ret_type: Column,
}

impl OperatorExpression {
    /// Checks the operand count and the operand types and works out the result type: the arithmetic operators need numbers (DECIMAL if
    /// either side is, else the wider integer type), `||` needs two strings, `NOT` a boolean; `IS [NOT] NULL` takes anything and gives a
    /// boolean. Anything else is a `NotImplemented` error that names the operator.
    pub fn new(op: Operator, children: Vec<ExprRef>) -> Result<OperatorExpression> {
        // @begin 3i-01
        if children.len() != op.arity() {
            return Err(Exception::new(ExceptionType::Invalid, format!("operator {} takes {} operand(s), got {}", op.symbol().trim(), op.arity(), children.len())));
        }
        let types: Vec<TypeId> = children.iter().map(|c| c.return_type().type_id()).collect();
        let bad = |what: &str| Exception::new(ExceptionType::NotImplemented, format!("operator {} needs {what}", op.symbol().trim()));
        let result = match op {
            Operator::Add | Operator::Subtract | Operator::Multiply | Operator::Divide | Operator::Modulo => {
                if !types.iter().all(|t| t.is_numeric()) {
                    return Err(bad("numbers on both sides"));
                }
                widest(types[0], types[1])
            }
            Operator::Negate => {
                if !types[0].is_numeric() {
                    return Err(bad("a number"));
                }
                types[0]
            }
            Operator::Concat => {
                if types != [TypeId::Varchar, TypeId::Varchar] {
                    return Err(bad("strings on both sides"));
                }
                TypeId::Varchar
            }
            Operator::Not => {
                if types[0] != TypeId::Boolean {
                    return Err(bad("a boolean"));
                }
                TypeId::Boolean
            }
            Operator::IsNull | Operator::IsNotNull => TypeId::Boolean,
        };
        let ret_type = if result == TypeId::Varchar { Column::new_varchar("<val>", 256) } else { Column::new("<val>", result) };
        Ok(OperatorExpression { op, children, ret_type })
        //~ todo!("3i-01: operand count (arity), operand types, the result type; see the doc comment")
        // @end
    }

    /// The value of the operator on already evaluated operands. A NULL operand gives a NULL of the result type, except for
    /// `IS [NOT] NULL`, which never gives NULL.
    fn compute(&self, args: &[Value]) -> Result<Value> {
        // @begin 3i-01
        let ret = self.ret_type.type_id();
        match self.op {
            Operator::IsNull => return Ok(Value::boolean(args[0].is_null())),
            Operator::IsNotNull => return Ok(Value::boolean(!args[0].is_null())),
            _ => {}
        }
        if args.iter().any(|a| a.is_null()) {
            return Ok(Value::null(ret));
        }
        match self.op {
            Operator::Add => args[0].add(&args[1]),
            Operator::Subtract => args[0].subtract(&args[1]),
            Operator::Multiply => args[0].multiply(&args[1]),
            Operator::Divide => args[0].divide(&args[1]),
            Operator::Modulo => args[0].modulo(&args[1]),
            Operator::Concat => Ok(Value::varchar(&format!("{}{}", args[0].as_str().unwrap_or(""), args[1].as_str().unwrap_or("")))),
            Operator::Negate => Value::integer(0).cast_as(ret)?.subtract(&args[0]),
            Operator::Not => Ok(Value::boolean(!args[0].as_bool().unwrap_or(false))),
            Operator::IsNull | Operator::IsNotNull => unreachable!("answered above"),
        }
        //~ todo!("3i-01: a NULL operand gives NULL (except IS [NOT] NULL); arithmetic through Value's add, subtract, multiply, divide and modulo; || concatenates; NOT flips a boolean")
        // @end
    }
}

/// The type of an arithmetic result: DECIMAL if either side is, else the wider integer type.
fn widest(a: TypeId, b: TypeId) -> TypeId {
    // @begin 3i-01
    if a == TypeId::Decimal || b == TypeId::Decimal {
        return TypeId::Decimal;
    }
    let rank = |t: TypeId| match t {
        TypeId::TinyInt => 0,
        TypeId::SmallInt => 1,
        TypeId::Integer => 2,
        _ => 3,
    };
    if rank(a) >= rank(b) {
        a
    } else {
        b
    }
    //~ todo!("3i-01: DECIMAL if either is DECIMAL, else the wider of the two integer types")
    // @end
}

impl Expression for OperatorExpression {
    fn evaluate(&self, tuple: &Tuple, schema: &Schema) -> Result<Value> {
        // @begin 3i-01
        let args = self.children.iter().map(|c| c.evaluate(tuple, schema)).collect::<Result<Vec<_>>>()?;
        self.compute(&args)
        //~ todo!("3i-01: evaluate the children and compute")
        // @end
    }

    fn evaluate_join(&self, left_tuple: &Tuple, left_schema: &Schema, right_tuple: &Tuple, right_schema: &Schema) -> Result<Value> {
        // @begin 3i-01
        let args =
            self.children.iter().map(|c| c.evaluate_join(left_tuple, left_schema, right_tuple, right_schema)).collect::<Result<Vec<_>>>()?;
        self.compute(&args)
        //~ todo!("3i-01: the same with evaluate_join")
        // @end
    }

    fn children(&self) -> &[ExprRef] {
        &self.children
    }

    fn return_type(&self) -> &Column {
        &self.ret_type
    }

    fn to_string(&self) -> String {
        match self.op.arity() {
            1 => match self.op {
                Operator::IsNull | Operator::IsNotNull => format!("({}{})", self.children[0], self.op.symbol()),
                _ => format!("({}{})", self.op.symbol(), self.children[0]),
            },
            _ => format!("({}{}{})", self.children[0], self.op.symbol(), self.children[1]),
        }
    }

    fn clone_with_children(&self, children: Vec<ExprRef>) -> ExprRef {
        Arc::new(OperatorExpression { children, ..self.clone() })
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}
