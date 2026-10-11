//! The planner turns a bound statement into a plan: a tree of scan, filter, projection, aggregation, join, sort and limit nodes, with every column
//! reference resolved to *"column i of child j"*.

use std::collections::BTreeMap;
use std::rc::Rc;
use std::sync::Arc;

use crate::binder::bound_expression::{BoundExpression, BoundWindow, WindowBoundary};
use crate::binder::bound_order_by::OrderBy;
use crate::binder::bound_statement::{BoundStatement, SelectStatement};
use crate::binder::bound_table_ref::{BoundSubqueryRef, BoundTableRef};
use crate::sql::ast::SetOperator;
use crate::catalog::catalog::Catalog;
use crate::catalog::column::Column;
use crate::catalog::schema::{Schema, SchemaRef};
use crate::common::exception::{Exception, ExceptionType, Result};
use crate::execution::expressions::abstract_expression::{ExprRef, Expression};
use crate::execution::expressions::arithmetic_expression::{ArithmeticExpression, ArithmeticType};
use crate::execution::expressions::column_value_expression::ColumnValueExpression;
use crate::execution::expressions::comparison_expression::{ComparisonExpression, ComparisonType};
use crate::execution::expressions::constant_value_expression::ConstantValueExpression;
use crate::execution::expressions::logic_expression::{LogicExpression, LogicType};
use crate::execution::expressions::string_expression::{StringExpression, StringExpressionType};
use crate::execution::plans::plan_node::*;
use crate::types::type_id::TypeId;
use crate::types::value::Value;

/// The name of a planned expression that has none (`1 + 2`, `count(*)`).
pub const UNNAMED_COLUMN: &str = "<unnamed>";

fn exception(msg: impl Into<String>) -> Exception {
    Exception::new(ExceptionType::Invalid, msg)
}

fn not_implemented(msg: impl Into<String>) -> Exception {
    Exception::new(ExceptionType::NotImplemented, msg)
}

/// State that is private to one SELECT: whether aggregates are allowed, the aggregates found, and the `WITH` queries in scope.
#[derive(Default)]
struct PlannerContext {
    allow_aggregation: bool,
    next_aggregation: usize,
    /// The expressions that stand for the aggregates' results: "column `group_bys + i` of the aggregation's output".
    expr_in_agg: Vec<ExprRef>,
    cte_list: Option<Rc<Vec<BoundSubqueryRef>>>,
}

pub struct Planner<'c, 'a> {
    catalog: &'c Catalog<'a>,
    ctx: Vec<PlannerContext>,
    universal_id: usize,
    pub plan: Option<PlanRef>,
}

type Named = (String, ExprRef);

impl<'c, 'a> Planner<'c, 'a> {
    pub fn new(catalog: &'c Catalog<'a>) -> Planner<'c, 'a> {
        Planner { catalog, ctx: vec![PlannerContext::default()], universal_id: 0, plan: None }
    }

    fn ctx(&mut self) -> &mut PlannerContext {
        self.ctx.last_mut().unwrap()
    }

    /// Runs `f` in a fresh context that sees the `WITH` queries of the enclosing one.
    fn with_context<T>(&mut self, f: impl FnOnce(&mut Self) -> Result<T>) -> Result<T> {
        let ctes = self.ctx.last().and_then(|c| c.cte_list.clone());
        self.ctx.push(PlannerContext { cte_list: ctes, ..Default::default() });
        let result = f(self);
        self.ctx.pop();
        result
    }

    fn next_id(&mut self) -> usize {
        self.universal_id += 1;
        self.universal_id - 1
    }

    pub fn plan_query(&mut self, statement: &BoundStatement) -> Result<()> {
        self.plan = Some(match statement {
            BoundStatement::Select(s) => self.plan_select(s)?,
            BoundStatement::Insert { table, select } => self.plan_insert(table, select)?,
            BoundStatement::Delete { table, expr } => self.plan_delete(table, expr)?,
            BoundStatement::Update { table, filter_expr, target_expr } => self.plan_update(table, filter_expr, target_expr)?,
            other => return Err(exception(format!("the statement {other} is not supported in planner yet"))),
        });
        Ok(())
    }

    // ---- schemas ------------------------------------------------------------------------------------------------------------

    fn rename_schema(schema: &Schema, names: &[String]) -> Result<Schema> {
        if names.len() != schema.columns().len() {
            return Err(exception("mismatched number of columns"));
        }
        Ok(Schema::new(schema.columns().iter().zip(names).map(|(c, n)| c.with_column_name(n)).collect()))
    }

    fn infer_projection_schema(exprs: &[ExprRef]) -> Schema {
        Schema::new(exprs.iter().map(|e| e.return_type().with_column_name("<unnamed>")).collect())
    }

    fn infer_scan_schema(table: &BoundTableRef) -> Schema {
        let BoundTableRef::Base { schema, .. } = table else { unreachable!() };
        let bound = table.bound_table_name().unwrap();
        Schema::new(schema.columns().iter().map(|c| c.with_column_name(&format!("{bound}.{}", c.name()))).collect())
    }

    fn infer_join_schema(left: &PlanNode, right: &PlanNode) -> Schema {
        let mut cols: Vec<Column> = left.output_schema.columns().to_vec();
        cols.extend(right.output_schema.columns().iter().cloned());
        Schema::new(cols)
    }

    fn infer_agg_schema(group_bys: &[ExprRef], aggregates: &[ExprRef]) -> Schema {
        let mut cols: Vec<Column> = group_bys.iter().map(|e| e.return_type().with_column_name("<unnamed>")).collect();
        cols.extend(aggregates.iter().map(|_| Column::new("<unnamed>", TypeId::Integer)));
        Schema::new(cols)
    }

    // ---- SELECT -------------------------------------------------------------------------------------------------------------

    pub fn plan_select(&mut self, statement: &SelectStatement) -> Result<PlanRef> {
        self.with_context(|p| {
            if !statement.ctes.is_empty() {
                p.ctx().cte_list = Some(Rc::new(statement.ctes.clone()));
            }
            let mut plan = match &statement.table {
                BoundTableRef::Empty => PlanNode::new(Arc::new(Schema::new(vec![])), vec![], PlanKind::Values { values: vec![vec![]] }),
                table => p.plan_table_ref(table)?,
            };
            if !statement.where_.is_invalid() {
                let schema = plan.output_schema.clone();
                let (_, expr) = p.plan_expression(&statement.where_, &[plan.clone()])?;
                plan = PlanNode::new(schema, vec![plan], PlanKind::Filter { predicate: expr });
            }

            let mut has_agg = false;
            let mut has_window_agg = false;
            for item in &statement.select_list {
                if item.has_aggregation() {
                    has_agg = true;
                    break;
                }
                if item.has_window_function() {
                    has_window_agg = true;
                    break;
                }
            }

            if has_window_agg {
                if !statement.having.is_invalid() {
                    return Err(exception("HAVING on window function is not supported yet."));
                }
                if !statement.group_by.is_empty() {
                    return Err(exception("Group by is not allowed to use with window function."));
                }
                plan = p.plan_select_window(statement, plan)?;
            } else if !statement.having.is_invalid() || !statement.group_by.is_empty() || has_agg {
                plan = p.plan_select_agg(statement, plan)?;
            } else {
                let mut exprs = vec![];
                let mut column_names = vec![];
                for item in &statement.select_list {
                    let (mut name, expr) = p.plan_expression(item, &[plan.clone()])?;
                    if name == UNNAMED_COLUMN {
                        name = format!("__unnamed#{}", p.next_id());
                    }
                    exprs.push(expr);
                    column_names.push(name);
                }
                let schema = Self::rename_schema(&Self::infer_projection_schema(&exprs), &column_names)?;
                plan = PlanNode::new(Arc::new(schema), vec![plan], PlanKind::Projection { expressions: exprs });
            }

            if statement.is_distinct {
                let child = plan;
                let distinct_exprs: Vec<ExprRef> = child
                    .output_schema
                    .columns()
                    .iter()
                    .enumerate()
                    .map(|(i, col)| Arc::new(ColumnValueExpression::new(0, i as u32, col.clone())) as ExprRef)
                    .collect();
                plan = PlanNode::new(
                    child.output_schema.clone(),
                    vec![child],
                    PlanKind::Aggregation { group_bys: distinct_exprs, aggregates: vec![], agg_types: vec![] },
                );
            }

            if !statement.sort.is_empty() {
                let mut order_bys = vec![];
                for ob in &statement.sort {
                    let (_, expr) = p.plan_expression(&ob.expr, &[plan.clone()])?;
                    order_bys.push(OrderBy::new(ob.order_type, ob.null_order, expr));
                }
                plan = PlanNode::new(plan.output_schema.clone(), vec![plan], PlanKind::Sort { order_bys });
            }

            if !statement.limit_count.is_invalid() || !statement.limit_offset.is_invalid() {
                let constant_int = |e: &BoundExpression, what: &str| -> Result<Option<usize>> {
                    match e {
                        BoundExpression::Invalid => Ok(None),
                        BoundExpression::Constant(Value::Integer(v)) if *v >= 0 => Ok(Some(*v as usize)),
                        _ => Err(not_implemented(format!("{what} clause must be an integer constant."))),
                    }
                };
                let limit = constant_int(&statement.limit_count, "LIMIT")?;
                let offset = constant_int(&statement.limit_offset, "OFFSET")?;
                // @begin 3i-05
                // the rows are dropped before the limit counts: `LIMIT n OFFSET m` is Limit(n) over Offset(m)
                if let Some(offset) = offset {
                    plan = PlanNode::new(plan.output_schema.clone(), vec![plan], PlanKind::Offset { offset });
                }
                //~ if offset.is_some() {
                //~     return Err(not_implemented("OFFSET clause is not supported yet."));
                //~ }
                // @end
                if let Some(limit) = limit {
                    plan = PlanNode::new(plan.output_schema.clone(), vec![plan], PlanKind::Limit { limit });
                }
            }
            Ok(plan)
        })
    }

    pub fn plan_table_ref(&mut self, table_ref: &BoundTableRef) -> Result<PlanRef> {
        match table_ref {
            BoundTableRef::Base { .. } => self.plan_base_table_ref(table_ref),
            BoundTableRef::CrossProduct { left, right } => {
                let left = self.plan_table_ref(left)?;
                let right = self.plan_table_ref(right)?;
                let schema = Arc::new(Self::infer_join_schema(&left, &right));
                let predicate: ExprRef = Arc::new(ConstantValueExpression::new(Value::boolean(true)));
                Ok(PlanNode::new(schema, vec![left, right], PlanKind::NestedLoopJoin { predicate, join_type: JoinType::Inner }))
            }
            BoundTableRef::Join { join_type, left, right, condition } => {
                let left = self.plan_table_ref(left)?;
                let right = self.plan_table_ref(right)?;
                let (_, predicate) = self.plan_expression(condition, &[left.clone(), right.clone()])?;
                let schema = Arc::new(Self::infer_join_schema(&left, &right));
                Ok(PlanNode::new(schema, vec![left, right], PlanKind::NestedLoopJoin { predicate, join_type: *join_type }))
            }
            BoundTableRef::ExpressionList { values, identifier } => self.plan_expression_list_ref(values, identifier),
            BoundTableRef::Subquery(s) => self.plan_subquery(s, &s.alias),
            BoundTableRef::SetOp { op, all, left, right, alias, select_list_name } => self.plan_set_op(*op, *all, left, right, alias, select_list_name),
            BoundTableRef::Cte { cte_name, alias, .. } => {
                let ctes = self.ctx.last().and_then(|c| c.cte_list.clone());
                if let Some(ctes) = ctes {
                    for cte in ctes.iter() {
                        if &cte.alias == cte_name {
                            return self.plan_subquery(cte, alias);
                        }
                    }
                }
                Err(exception("CTE not found"))
            }
            other => Err(exception(format!("the table ref type {other} is not supported in planner yet"))),
        }
    }

    /// `left UNION [ALL] right` and its relatives. Both sides must have the same number of columns, of the same types in the same order
    /// (PostgreSQL would convert an INTEGER to a DECIMAL; this engine says no). The result's columns are named after the left side, under
    /// `alias`.
    fn plan_set_op(&mut self, op: SetOperator, all: bool, left: &BoundSubqueryRef, right: &BoundSubqueryRef, alias: &str, names: &[Vec<String>]) -> Result<PlanRef> {
        // @begin 3j-04
        let l = self.plan_subquery(left, &left.alias)?;
        let r = self.plan_subquery(right, &right.alias)?;
        let word = match op {
            SetOperator::Union => "UNION",
            SetOperator::Intersect => "INTERSECT",
            SetOperator::Except => "EXCEPT",
        };
        let (lc, rc) = (l.output_schema.columns(), r.output_schema.columns());
        if lc.len() != rc.len() {
            return Err(exception(format!("each {word} query must have the same number of columns ({} and {})", lc.len(), rc.len())));
        }
        for (a, b) in lc.iter().zip(rc) {
            if a.type_id() != b.type_id() {
                return Err(Exception::new(ExceptionType::MismatchType, format!("{word} types {:?} and {:?} cannot be matched", a.type_id(), b.type_id())));
            }
        }
        let renamed: Vec<String> = names.iter().map(|n| format!("{alias}.{}", n.join("."))).collect();
        let schema = Self::rename_schema(&Schema::new(lc.to_vec()), &renamed)?;
        Ok(PlanNode::new(Arc::new(schema), vec![l, r], PlanKind::SetOp { op, all }))
        //~ let _ = (op, all, left, right, alias, names);
        //~ Err(not_implemented("set operations are not planned yet"))
        // @end
    }

    fn plan_subquery(&mut self, table_ref: &BoundSubqueryRef, alias: &str) -> Result<PlanRef> {
        let select_node = self.plan_select(&table_ref.subquery)?;
        let mut names = vec![];
        let mut exprs: Vec<ExprRef> = vec![];
        for (idx, col) in select_node.output_schema.columns().iter().enumerate() {
            exprs.push(Arc::new(ColumnValueExpression::new(0, idx as u32, col.clone())));
            names.push(format!("{alias}.{}", table_ref.select_list_name[idx].join(".")));
        }
        let schema = Self::rename_schema(&Self::infer_projection_schema(&exprs), &names)?;
        Ok(PlanNode::new(Arc::new(schema), vec![select_node], PlanKind::Projection { expressions: exprs }))
    }

    fn plan_base_table_ref(&mut self, table_ref: &BoundTableRef) -> Result<PlanRef> {
        let BoundTableRef::Base { table, .. } = table_ref else { unreachable!() };
        let info = self.catalog.get_table(table).ok_or_else(|| exception("table not found"))?;
        if info.name.starts_with("__") {
            if info.name.starts_with("__mock") {
                return Ok(PlanNode::new(Arc::new(Self::infer_scan_schema(table_ref)), vec![], PlanKind::MockScan { table: info.name.clone() }));
            }
            return Err(exception(format!("unsupported internal table: {}", info.name)));
        }
        Ok(PlanNode::new(
            Arc::new(Self::infer_scan_schema(table_ref)),
            vec![],
            PlanKind::SeqScan { table_oid: info.oid, table_name: info.name.clone(), filter_predicate: None },
        ))
    }

    fn plan_expression_list_ref(&mut self, values: &[Vec<BoundExpression>], identifier: &str) -> Result<PlanRef> {
        let mut all_exprs = vec![];
        for row in values {
            let mut row_exprs = vec![];
            for col in row {
                row_exprs.push(self.plan_expression(col, &[])?.1);
            }
            all_exprs.push(row_exprs);
        }
        let cols: Vec<Column> =
            all_exprs[0].iter().enumerate().map(|(idx, c)| c.return_type().with_column_name(&format!("{identifier}.{idx}"))).collect();
        Ok(PlanNode::new(Arc::new(Schema::new(cols)), vec![], PlanKind::Values { values: all_exprs }))
    }

    // ---- expressions --------------------------------------------------------------------------------------------------------

    pub fn plan_expression(&mut self, expr: &BoundExpression, children: &[PlanRef]) -> Result<Named> {
        match expr {
            BoundExpression::AggCall { .. } => {
                let ctx = self.ctx();
                if ctx.next_aggregation >= ctx.expr_in_agg.len() {
                    return Err(exception("unexpected agg call"));
                }
                ctx.next_aggregation += 1;
                Ok((UNNAMED_COLUMN.to_string(), ctx.expr_in_agg[ctx.next_aggregation - 1].clone()))
            }
            BoundExpression::ColumnRef(names) => {
                let (name, col) = self.plan_column_ref(names, children)?;
                Ok((name, col))
            }
            BoundExpression::BinaryOp { op, left, right } => {
                let (_, l) = self.plan_expression(left, children)?;
                let (_, r) = self.plan_expression(right, children)?;
                Ok((UNNAMED_COLUMN.to_string(), Self::get_binary_expression_from_factory(op, l, r)?))
            }
            BoundExpression::FuncCall { func_name, args } => {
                let mut planned = vec![];
                for a in args {
                    planned.push(self.plan_expression(a, children)?.1);
                }
                Ok((UNNAMED_COLUMN.to_string(), Self::get_func_call_from_factory(func_name, planned)?))
            }
            BoundExpression::Constant(v) => Ok((UNNAMED_COLUMN.to_string(), Arc::new(ConstantValueExpression::new(v.clone())))),
            BoundExpression::Alias { alias, child } => {
                let (_, e) = self.plan_expression(child, children)?;
                Ok((alias.clone(), e))
            }
            BoundExpression::Window(_) => Err(exception("should not parse window expressions here")),
            // @begin 3i-01
            BoundExpression::UnaryOp { op, arg } => {
                use crate::execution::expressions::operator_expression::{Operator, OperatorExpression};
                let (_, a) = self.plan_expression(arg, children)?;
                let operator = match op.as_str() {
                    "-" => Operator::Negate,
                    "not" => Operator::Not,
                    "is null" => Operator::IsNull,
                    "is not null" => Operator::IsNotNull,
                    other => return Err(not_implemented(format!("unary operator {other} not supported in planner yet"))),
                };
                Ok((UNNAMED_COLUMN.to_string(), Arc::new(OperatorExpression::new(operator, vec![a])?)))
            }
            //~ // TODO(3i-01): unary `-`, `not`, `is null` and `is not null` become an OperatorExpression over the planned operand
            // @end
            // @begin 3i-03
            BoundExpression::Case { branches, otherwise } => {
                use crate::execution::expressions::case_expression::CaseExpression;
                let mut planned = Vec::new();
                // the order matters: aggregate calls are met again in the order `collect_aggregates` found them
                for (cond, result) in branches {
                    planned.push((self.plan_expression(cond, children)?.1, self.plan_expression(result, children)?.1));
                }
                let otherwise = match otherwise {
                    Some(e) => Some(self.plan_expression(e, children)?.1),
                    None => None,
                };
                Ok((UNNAMED_COLUMN.to_string(), Arc::new(CaseExpression::new(planned, otherwise)?)))
            }
            //~ // TODO(3i-03): a Case becomes a CaseExpression over its planned conditions, results and else
            // @end
            other => Err(exception(format!("expression type {other} not supported in planner yet"))),
        }
    }

    /// A reference to column `names` of the children's output. One child: "column i of my input"; two children (a join): "column i of
    /// the left (tuple 0) or right (tuple 1) input".
    fn plan_column_ref(&mut self, names: &[String], children: &[PlanRef]) -> Result<(String, ExprRef)> {
        if children.is_empty() {
            return Err(exception("column ref should have at least one child"));
        }
        let col_name = names.join(".");
        if children.len() == 1 {
            let schema = &children[0].output_schema;
            let mut found = false;
            for col in schema.columns() {
                if col.name() == col_name {
                    if found {
                        return Err(exception("duplicated column found in schema"));
                    }
                    found = true;
                }
            }
            let idx = schema.try_col_idx(&col_name).ok_or_else(|| exception(format!("column name {col_name} not found")))?;
            let col_type = schema.column(idx).clone();
            return Ok((col_name, Arc::new(ColumnValueExpression::new(0, idx, col_type))));
        }
        if children.len() == 2 {
            let (left, right) = (&children[0].output_schema, &children[1].output_schema);
            return match (left.try_col_idx(&col_name), right.try_col_idx(&col_name)) {
                (Some(_), Some(_)) => Err(exception(format!("ambiguous column name {col_name}"))),
                (Some(i), None) => Ok((col_name, Arc::new(ColumnValueExpression::new(0, i, left.column(i).clone())))),
                (None, Some(i)) => Ok((col_name, Arc::new(ColumnValueExpression::new(1, i, right.column(i).clone())))),
                (None, None) => Err(exception(format!("column name {col_name} not found"))),
            };
        }
        Err(exception("no executor with expression has more than two children for now"))
    }

    pub fn get_binary_expression_from_factory(op_name: &str, left: ExprRef, right: ExprRef) -> Result<ExprRef> {
        // @begin 3i-01
        // the operators BusTub's expression layer left out, and `+` and `-` on every numeric type
        use crate::execution::expressions::operator_expression::{Operator, OperatorExpression};
        let operator = match op_name {
            "+" => Some(Operator::Add),
            "-" => Some(Operator::Subtract),
            "*" => Some(Operator::Multiply),
            "/" => Some(Operator::Divide),
            "%" => Some(Operator::Modulo),
            "||" => Some(Operator::Concat),
            _ => None,
        };
        if let Some(op) = operator {
            let integers = left.return_type().type_id() == TypeId::Integer && right.return_type().type_id() == TypeId::Integer;
            if !(matches!(op, Operator::Add | Operator::Subtract) && integers) {
                return Ok(Arc::new(OperatorExpression::new(op, vec![left, right])?));
            }
        }
        //~ // TODO(3i-01): `*`, `/`, `%`, `||`, and `+` and `-` on non-integers: build an OperatorExpression
        // @end
        // @begin 3i-04
        if op_name == "like" {
            use crate::execution::expressions::like_expression::LikeExpression;
            return Ok(Arc::new(LikeExpression::new(left, right)?));
        }
        //~ // TODO(3i-04): `like` becomes a LikeExpression
        // @end
        let cmp = |t| -> Result<ExprRef> { Ok(Arc::new(ComparisonExpression::new(left.clone(), right.clone(), t))) };
        match op_name {
            "=" | "==" => cmp(ComparisonType::Equal),
            "!=" | "<>" => cmp(ComparisonType::NotEqual),
            "<" => cmp(ComparisonType::LessThan),
            "<=" => cmp(ComparisonType::LessThanOrEqual),
            ">" => cmp(ComparisonType::GreaterThan),
            ">=" => cmp(ComparisonType::GreaterThanOrEqual),
            "+" => Ok(Arc::new(ArithmeticExpression::new(left, right, ArithmeticType::Plus)?)),
            "-" => Ok(Arc::new(ArithmeticExpression::new(left, right, ArithmeticType::Minus)?)),
            "and" => Ok(Arc::new(LogicExpression::new(left, right, LogicType::And)?)),
            "or" => Ok(Arc::new(LogicExpression::new(left, right, LogicType::Or)?)),
            other => Err(exception(format!("binary op {other} not supported in planner yet"))),
        }
    }

    /// Builds the expression for a call of the function `func_name`.
    pub fn get_func_call_from_factory(func_name: &str, mut args: Vec<ExprRef>) -> Result<ExprRef> {
        // @begin 3d-06
        let kind = match func_name {
            "lower" => StringExpressionType::Lower,
            "upper" => StringExpressionType::Upper,
            _ => return Err(exception(format!("func call {func_name} not supported in planner yet"))),
        };
        if args.len() != 1 {
            return Err(exception(format!("{func_name} takes exactly one argument, got {}", args.len())));
        }
        Ok(Arc::new(StringExpression::new(args.remove(0), kind)?))
        //~ todo!("3d-06: `lower` and `upper` with exactly one argument become a StringExpression; any other name or argument count is an error")
        // @end
    }

    // ---- aggregation --------------------------------------------------------------------------------------------------------

    /// The aggregate calls of `expr`, in the order the planner will meet them again.
    fn collect_aggregates(expr: &BoundExpression, out: &mut Vec<BoundExpression>) -> Result<()> {
        match expr {
            BoundExpression::AggCall { .. } => out.push(expr.clone()),
            BoundExpression::ColumnRef(_) | BoundExpression::Constant(_) => {}
            BoundExpression::BinaryOp { left, right, .. } => {
                Self::collect_aggregates(left, out)?;
                Self::collect_aggregates(right, out)?;
            }
            BoundExpression::UnaryOp { arg, .. } => Self::collect_aggregates(arg, out)?,
            BoundExpression::Case { branches, otherwise } => {
                for (c, r) in branches {
                    Self::collect_aggregates(c, out)?;
                    Self::collect_aggregates(r, out)?;
                }
                if let Some(e) = otherwise {
                    Self::collect_aggregates(e, out)?;
                }
            }
            BoundExpression::FuncCall { args, .. } => {
                for a in args {
                    Self::collect_aggregates(a, out)?;
                }
            }
            BoundExpression::Alias { child, .. } => Self::collect_aggregates(child, out)?,
            other => return Err(exception(format!("expression type {other} not supported in planner yet"))),
        }
        Ok(())
    }

    fn get_agg_call_from_factory(func_name: &str, mut args: Vec<ExprRef>) -> Result<(AggregationType, Vec<ExprRef>)> {
        if args.is_empty() && func_name == "count_star" {
            return Ok((AggregationType::CountStarAggregate, vec![]));
        }
        if args.len() == 1 {
            let t = match func_name {
                "min" => Some(AggregationType::MinAggregate),
                "max" => Some(AggregationType::MaxAggregate),
                "sum" => Some(AggregationType::SumAggregate),
                "count" => Some(AggregationType::CountAggregate),
                _ => None,
            };
            if let Some(t) = t {
                return Ok((t, vec![args.remove(0)]));
            }
        }
        Err(exception(format!("unsupported agg_call {func_name} with {} args", args.len())))
    }

    fn get_window_agg_call_from_factory(func_name: &str, mut args: Vec<ExprRef>) -> Result<(WindowFunctionType, Vec<ExprRef>)> {
        if args.is_empty() {
            if func_name == "count_star" {
                return Ok((WindowFunctionType::CountStarAggregate, vec![]));
            }
            if func_name == "rank" {
                return Ok((WindowFunctionType::Rank, vec![]));
            }
        }
        if args.len() == 1 {
            let t = match func_name {
                "min" => Some(WindowFunctionType::MinAggregate),
                "max" => Some(WindowFunctionType::MaxAggregate),
                "sum" => Some(WindowFunctionType::SumAggregate),
                "count" => Some(WindowFunctionType::CountAggregate),
                _ => None,
            };
            if let Some(t) = t {
                return Ok((t, vec![args.remove(0)]));
            }
        }
        Err(exception(format!("unsupported window_call {func_name} with {} args", args.len())))
    }

    fn plan_agg_call(&mut self, agg_call: &BoundExpression, children: &[PlanRef]) -> Result<(AggregationType, Vec<ExprRef>)> {
        let BoundExpression::AggCall { func_name, is_distinct, args } = agg_call else {
            return Err(not_implemented("alias for agg call is not supported for now"));
        };
        if *is_distinct {
            return Err(not_implemented("distinct agg is not implemented yet"));
        }
        let exprs = self.with_context(|p| {
            let mut exprs = vec![];
            for arg in args {
                exprs.push(p.plan_expression(arg, children)?.1);
            }
            Ok(exprs)
        })?;
        Self::get_agg_call_from_factory(func_name, exprs)
    }

    /// `select v3, max(v1) + max(v2) from t where c group by v3 having count(v4) > count(v5)` becomes
    /// `Projection(v3, max + max) <- Filter(count > count) <- Aggregation(group_by v3; max v1, max v2, count v4, count v5) <- Filter/Scan`.
    fn plan_select_agg(&mut self, statement: &SelectStatement, child: PlanRef) -> Result<PlanRef> {
        self.with_context(|p| {
            p.ctx().allow_aggregation = true;
            let mut group_by_exprs = vec![];
            let mut output_col_names = vec![];
            for expr in &statement.group_by {
                let (name, e) = p.plan_expression(expr, &[child.clone()])?;
                group_by_exprs.push(e);
                output_col_names.push(name);
            }

            let mut aggregations = vec![];
            if !statement.having.is_invalid() {
                Self::collect_aggregates(&statement.having, &mut aggregations)?;
            }
            for item in &statement.select_list {
                Self::collect_aggregates(item, &mut aggregations)?;
            }

            // @begin 3i-05
            // `count(distinct x)`: first one row per (group, x) by an aggregation without aggregates, then the plain aggregate over those
            // rows; NULLs of x form a group of their own and are ignored by count and sum as ever
            let mut distinct_input: Option<ExprRef> = None;
            let (child, group_by_exprs) = if aggregations.iter().any(|a| matches!(a, BoundExpression::AggCall { is_distinct: true, .. })) {
                let mut arg: Option<&BoundExpression> = None;
                for a in &aggregations {
                    let BoundExpression::AggCall { is_distinct, args, .. } = a else { unreachable!("collect_aggregates keeps only aggregate calls") };
                    if !*is_distinct || args.len() != 1 {
                        return Err(not_implemented("DISTINCT and plain aggregates in one query are not supported yet"));
                    }
                    match arg {
                        None => arg = Some(&args[0]),
                        Some(first) if first.to_string() == args[0].to_string() => {}
                        Some(_) => return Err(not_implemented("DISTINCT aggregates over different expressions are not supported yet")),
                    }
                }
                let arg_expr = p.plan_expression(arg.expect("at least one aggregate"), &[child.clone()])?.1;
                let mut inner_groups = group_by_exprs.clone();
                inner_groups.push(arg_expr);
                let inner_schema = Schema::new(inner_groups.iter().map(|e| e.return_type().with_column_name("<unnamed>")).collect());
                let g = group_by_exprs.len();
                let inner = PlanNode::new(
                    Arc::new(inner_schema),
                    vec![child],
                    PlanKind::Aggregation { group_bys: inner_groups, aggregates: vec![], agg_types: vec![] },
                );
                let column = |i: usize| -> ExprRef { Arc::new(ColumnValueExpression::new(0, i as u32, inner.output_schema.column(i as u32).clone())) };
                distinct_input = Some(column(g));
                let outer_groups: Vec<ExprRef> = (0..g).map(column).collect();
                (inner.clone(), outer_groups)
            } else {
                (child, group_by_exprs)
            };
            //~ let distinct_input: Option<ExprRef> = None;
            // @end

            let mut input_exprs: Vec<ExprRef> = vec![];
            let mut agg_types = vec![];
            let agg_begin_idx = group_by_exprs.len();
            for (term_idx, agg_call) in aggregations.iter().enumerate() {
                let (agg_type, mut exprs) = match &distinct_input {
                    Some(column) => {
                        let BoundExpression::AggCall { func_name, .. } = agg_call else { unreachable!() };
                        Self::get_agg_call_from_factory(func_name, vec![column.clone()])?
                    }
                    None => p.plan_agg_call(agg_call, &[child.clone()])?,
                };
                if exprs.len() > 1 {
                    return Err(not_implemented("only agg call of zero/one arg is supported"));
                }
                input_exprs.push(match exprs.pop() {
                    Some(e) => e,
                    None => Arc::new(ConstantValueExpression::new(Value::integer(1))),
                });
                agg_types.push(agg_type);
                output_col_names.push(format!("agg#{term_idx}"));
                p.ctx().expr_in_agg.push(Arc::new(ColumnValueExpression::new(
                    0,
                    (agg_begin_idx + term_idx) as u32,
                    Column::new("<agg_result>", TypeId::Integer),
                )));
            }

            let agg_output_schema = Self::infer_agg_schema(&group_by_exprs, &input_exprs);
            let mut plan = PlanNode::new(
                Arc::new(Self::rename_schema(&agg_output_schema, &output_col_names)?),
                vec![child],
                PlanKind::Aggregation { group_bys: group_by_exprs, aggregates: input_exprs, agg_types },
            );

            if !statement.having.is_invalid() {
                let (_, expr) = p.plan_expression(&statement.having, &[plan.clone()])?;
                plan = PlanNode::new(plan.output_schema.clone(), vec![plan], PlanKind::Filter { predicate: expr });
            }

            let mut exprs = vec![];
            let mut final_names = vec![];
            for item in &statement.select_list {
                let (name, e) = p.plan_expression(item, &[plan.clone()])?;
                exprs.push(e);
                final_names.push(name);
            }
            let schema = Self::rename_schema(&Self::infer_projection_schema(&exprs), &final_names)?;
            Ok(PlanNode::new(Arc::new(schema), vec![plan], PlanKind::Projection { expressions: exprs }))
        })
    }

    // ---- window functions ---------------------------------------------------------------------------------------------------

    fn check_order_by_compatible(order_by_exprs: &[Vec<OrderBy>]) -> Result<()> {
        let Some(first) = order_by_exprs.first() else {
            return Ok(());
        };
        for order_by in order_by_exprs {
            if order_by.len() != first.len() {
                return Err(exception("order by clause of window functions are not compatible"));
            }
            for (a, b) in order_by.iter().zip(first) {
                if a.order_type != b.order_type || a.null_order != b.null_order || a.expr.to_string() != b.expr.to_string() {
                    return Err(exception("order by clause of window functions are not compatible"));
                }
            }
        }
        Ok(())
    }

    fn plan_select_window(&mut self, statement: &SelectStatement, child: PlanRef) -> Result<PlanRef> {
        let mut columns: Vec<ExprRef> = vec![];
        let mut column_names = vec![];
        let mut window_functions: BTreeMap<u32, WindowFunction> = BTreeMap::new();
        let mut order_by_exprs: Vec<Vec<OrderBy>> = vec![];
        for (i, item) in statement.select_list.iter().enumerate() {
            if !item.has_window_function() {
                let (mut name, expr) = self.plan_expression(item, &[child.clone()])?;
                if name == UNNAMED_COLUMN {
                    name = format!("__unnamed#{}", self.next_id());
                }
                columns.push(expr);
                column_names.push(name);
                continue;
            }
            columns.push(Arc::new(ColumnValueExpression::new(0, u32::MAX, Column::new("<placeholder>", TypeId::Integer))));
            let window_item = match item {
                BoundExpression::Alias { alias, child } => {
                    column_names.push(alias.clone());
                    child.as_ref()
                }
                other => {
                    column_names.push(format!("__unnamed#{}", self.next_id()));
                    other
                }
            };
            let BoundExpression::Window(w) = window_item else {
                return Err(exception("Invalid expression type has window function"));
            };
            let BoundWindow { func_name, args, partition_by, order_bys, start, end } = w.as_ref();
            if *start != WindowBoundary::UnboundedPreceding || (*end != WindowBoundary::CurrentRowRows && *end != WindowBoundary::CurrentRowRange) {
                return Err(exception("BusTub currently only support window function with default window frame settings"));
            }
            let mut planned_partition_by = vec![];
            for p in partition_by {
                planned_partition_by.push(self.plan_expression(p, &[child.clone()])?.1);
            }
            if func_name == "rank" && order_bys.is_empty() {
                return Err(exception("order by clause is mandatory for rank function"));
            }
            let mut order_by = vec![];
            for ob in order_bys {
                order_by.push(OrderBy::new(ob.order_type, ob.null_order, self.plan_expression(&ob.expr, &[child.clone()])?.1));
            }
            order_by_exprs.push(order_by.clone());
            let mut raw_args = vec![];
            for a in args {
                raw_args.push(self.plan_expression(a, &[child.clone()])?.1);
            }
            let (window_func_type, mut clean_args) = Self::get_window_agg_call_from_factory(func_name, raw_args)?;
            if clean_args.len() > 1 {
                return Err(not_implemented("only agg call of zero/one arg is supported"));
            }
            let function: ExprRef = match clean_args.pop() {
                Some(e) => e,
                None => Arc::new(ConstantValueExpression::new(Value::integer(1))),
            };
            window_functions.insert(i as u32, WindowFunction { function, func_type: window_func_type, partition_by: planned_partition_by, order_by });
        }
        Self::check_order_by_compatible(&order_by_exprs)?;
        let output = Schema::new(columns.iter().map(|c| c.return_type().with_column_name("<unnamed>")).collect());
        let schema = Self::rename_schema(&output, &column_names)?;
        Ok(PlanNode::new(Arc::new(schema), vec![child], PlanKind::Window { columns, window_functions }))
    }

    // ---- INSERT / DELETE / UPDATE -------------------------------------------------------------------------------------------

    fn plan_insert(&mut self, table: &BoundTableRef, select: &SelectStatement) -> Result<PlanRef> {
        let child = self.plan_select(select)?;
        let BoundTableRef::Base { schema: table_schema, oid, .. } = table else { unreachable!() };
        let table_cols = table_schema.columns();
        let child_cols = child.output_schema.columns();
        if table_cols.len() != child_cols.len() || table_cols.iter().zip(child_cols).any(|(a, b)| a.type_id() != b.type_id()) {
            return Err(exception("table schema mismatch"));
        }
        let schema: SchemaRef = Arc::new(Schema::new(vec![Column::new("__bustub_internal.insert_rows", TypeId::Integer)]));
        Ok(PlanNode::new(schema, vec![child], PlanKind::Insert { table_oid: *oid }))
    }

    fn plan_delete(&mut self, table: &BoundTableRef, expr: &BoundExpression) -> Result<PlanRef> {
        let BoundTableRef::Base { oid, .. } = table else { unreachable!() };
        let scan = self.plan_table_ref(table)?;
        let (_, condition) = self.plan_expression(expr, &[scan.clone()])?;
        let filter = PlanNode::new(scan.output_schema.clone(), vec![scan], PlanKind::Filter { predicate: condition });
        let schema: SchemaRef = Arc::new(Schema::new(vec![Column::new("__bustub_internal.delete_rows", TypeId::Integer)]));
        Ok(PlanNode::new(schema, vec![filter], PlanKind::Delete { table_oid: *oid }))
    }

    fn plan_update(&mut self, table: &BoundTableRef, filter_expr: &BoundExpression, target_expr: &[(Vec<String>, BoundExpression)]) -> Result<PlanRef> {
        let BoundTableRef::Base { oid, .. } = table else { unreachable!() };
        let scan = self.plan_table_ref(table)?;
        let (_, condition) = self.plan_expression(filter_expr, &[scan.clone()])?;
        let filter = PlanNode::new(scan.output_schema.clone(), vec![scan], PlanKind::Filter { predicate: condition });
        let scope = [filter.clone()];
        let mut target_exprs: Vec<Option<ExprRef>> = vec![None; filter.output_schema.columns().len()];
        for (col, target) in target_expr {
            let (_, target_abstract) = self.plan_expression(target, &scope)?;
            let (_, col_expr) = self.plan_column_ref(col, &scope)?;
            let idx = col_expr.as_any().downcast_ref::<ColumnValueExpression>().unwrap().col_idx();
            target_exprs[idx as usize] = Some(target_abstract);
        }
        let target_exprs: Vec<ExprRef> = target_exprs
            .into_iter()
            .enumerate()
            .map(|(idx, e)| e.unwrap_or_else(|| Arc::new(ColumnValueExpression::new(0, idx as u32, filter.output_schema.column(idx as u32).clone()))))
            .collect();
        let schema: SchemaRef = Arc::new(Schema::new(vec![Column::new("__bustub_internal.update_rows", TypeId::Integer)]));
        Ok(PlanNode::new(schema, vec![filter], PlanKind::Update { table_oid: *oid, target_expressions: target_exprs }))
    }
}
