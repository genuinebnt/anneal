//! Name resolution: turns the syntax tree into a bound tree.
//!
//! Names match case-insensitively. A column reference resolves to the table (or alias) and column it names, `*` expands to every column
//! in scope, and a column present in two tables is ambiguous.

use super::bound_expression::{BoundExpression, BoundWindow, WindowBoundary};
use super::bound_order_by::{BoundOrderBy, OrderByNullType, OrderByType};
use super::bound_statement::{explain_options, BoundStatement, SelectStatement};
use super::bound_table_ref::{BoundSubqueryRef, BoundTableRef};
use crate::catalog::catalog::Catalog;
use crate::catalog::column::Column;
use crate::catalog::schema::Schema;
use crate::common::exception::{Exception, ExceptionType, Result};
use crate::execution::plans::plan_node::JoinType;
use crate::sql::ast::*;
use crate::types::type_id::TypeId;
use crate::types::value::Value;

fn exception(msg: impl Into<String>) -> Exception {
    Exception::new(ExceptionType::Invalid, msg)
}

fn not_implemented(msg: impl Into<String>) -> Exception {
    Exception::new(ExceptionType::NotImplemented, msg)
}

/// A `WITH` query that is in scope: its name and the names of its columns.
struct CteInfo {
    name: String,
    select_list_name: Vec<Vec<String>>,
}

pub struct Binder<'c, 'a> {
    catalog: &'c Catalog<'a>,
    cte_scope: Vec<CteInfo>,
    universal_id: usize,
}

fn lower(s: &str) -> String {
    s.to_lowercase()
}

fn prepend(prefix: &str, name: &[String]) -> Vec<String> {
    std::iter::once(prefix.to_string()).chain(name.iter().cloned()).collect()
}

impl<'c, 'a> Binder<'c, 'a> {
    pub fn new(catalog: &'c Catalog<'a>) -> Binder<'c, 'a> {
        Binder { catalog, cte_scope: vec![], universal_id: 0 }
    }

    fn next_id(&mut self) -> usize {
        self.universal_id += 1;
        self.universal_id - 1
    }

    pub fn bind_statement(&mut self, stmt: &Statement) -> Result<BoundStatement> {
        match stmt {
            Statement::Select(q) => Ok(BoundStatement::Select(Box::new(self.bind_select(q)?))),
            Statement::CreateTable { name, columns, primary_key } => self.bind_create(name, columns, primary_key),
            Statement::CreateIndex { name, table, columns, using } => self.bind_index(name, table, columns, using.as_deref()),
            Statement::Insert { table, columns, query } => {
                if columns.is_some() {
                    return Err(not_implemented("insert only supports all columns, don't specify columns"));
                }
                let table_ref = self.bind_base_table_ref(table, None)?;
                if table.starts_with("__") {
                    return Err(exception(format!("invalid table for insert: {table}")));
                }
                let select = self.bind_select(query)?;
                Ok(BoundStatement::Insert { table: table_ref, select: Box::new(select) })
            }
            Statement::Delete { table, filter } => {
                let table_ref = self.bind_base_table_ref(table, None)?;
                let expr = match filter {
                    Some(f) => self.bind_expression(f, &table_ref)?,
                    None => BoundExpression::Constant(Value::boolean(true)),
                };
                Ok(BoundStatement::Delete { table: table_ref, expr })
            }
            Statement::Update { table, assignments, filter, has_from } => {
                if *has_from {
                    return Err(not_implemented("update from clause not supported yet"));
                }
                let table_ref = self.bind_base_table_ref(table, None)?;
                let filter_expr = match filter {
                    Some(f) => self.bind_expression(f, &table_ref)?,
                    None => BoundExpression::Constant(Value::boolean(true)),
                };
                let mut target_expr = vec![];
                for (name, value) in assignments {
                    let column = self
                        .resolve_column_ref_from_base_table_ref(&table_ref, std::slice::from_ref(name))?
                        .ok_or_else(|| exception(format!("column {name} not found")))?;
                    target_expr.push((column, self.bind_expression(value, &table_ref)?));
                }
                Ok(BoundStatement::Update { table: table_ref, filter_expr, target_expr })
            }
            Statement::Explain { options, statement } => {
                let mut flags = explain_options::PLANNER | explain_options::OPTIMIZER | explain_options::BINDER | explain_options::SCHEMA;
                if !options.is_empty() {
                    flags = explain_options::INVALID;
                    for o in options {
                        match o.as_str() {
                            "planner" | "p" => flags |= explain_options::PLANNER,
                            "binder" | "b" => flags |= explain_options::BINDER,
                            "optimizer" | "o" => flags |= explain_options::OPTIMIZER,
                            "schema" | "s" => flags |= explain_options::SCHEMA,
                            "analyze" | "a" => flags |= explain_options::ANALYZE,
                            _ => {}
                        }
                    }
                }
                Ok(BoundStatement::Explain { statement: Box::new(self.bind_statement(statement)?), options: flags })
            }
            Statement::Set { name, value } => {
                let empty = BoundTableRef::Empty;
                match self.bind_expression(value, &empty)? {
                    BoundExpression::Constant(v) => Ok(BoundStatement::VariableSet { variable: name.clone(), value: v.to_string() }),
                    _ => Err(not_implemented("Only constant is supported")),
                }
            }
            Statement::Show { name } => Ok(BoundStatement::VariableShow { variable: name.clone() }),
            Statement::Begin { .. } => Ok(BoundStatement::Transaction("begin".into())),
            Statement::Commit => Ok(BoundStatement::Transaction("commit".into())),
            Statement::Rollback => Ok(BoundStatement::Transaction("abort".into())),
        }
    }

    // ---- DDL ----------------------------------------------------------------------------------------------------------------

    fn bind_column_definition(&mut self, def: &ColumnDef) -> Result<Column> {
        let empty = BoundTableRef::Empty;
        match def.type_name.as_str() {
            "int" | "int4" | "integer" => Ok(Column::new(&def.name, TypeId::Integer)),
            "double" | "float8" => Ok(Column::new(&def.name, TypeId::Decimal)),
            "bool" | "boolean" => Ok(Column::new(&def.name, TypeId::Boolean)),
            "varchar" => {
                if def.type_args.len() != 1 {
                    return Err(exception("should specify max length for varchar field"));
                }
                match self.bind_expression(&def.type_args[0], &empty)? {
                    BoundExpression::Constant(v) => {
                        let len = v.to_string().parse::<u32>().map_err(|_| exception("bad varchar length"))?;
                        Ok(Column::new_varchar(&def.name, len))
                    }
                    _ => Err(exception("bad varchar length")),
                }
            }
            other => Err(not_implemented(format!("unsupported type: {other}"))),
        }
    }

    fn bind_create(&mut self, table: &str, defs: &[ColumnDef], table_pk: &[String]) -> Result<BoundStatement> {
        let mut columns = vec![];
        let mut pk: Vec<String> = vec![];
        for def in defs {
            let column = self.bind_column_definition(def)?;
            if def.primary_key {
                if !pk.is_empty() {
                    return Err(not_implemented("cannot have two primary keys"));
                }
                pk = vec![column.name().to_string()];
            }
            columns.push(column);
        }
        if !table_pk.is_empty() {
            if !pk.is_empty() {
                return Err(not_implemented("cannot have two primary keys"));
            }
            pk = table_pk.to_vec();
        }
        if columns.is_empty() {
            return Err(exception("should have at least 1 column"));
        }
        Ok(BoundStatement::Create { table: table.to_string(), columns, primary_key: pk })
    }

    fn bind_index(&mut self, name: &str, table: &str, columns: &[String], using: Option<&str>) -> Result<BoundStatement> {
        let table_ref = self.bind_base_table_ref(table, None)?;
        let mut cols = vec![];
        for c in columns {
            match self.resolve_column(&table_ref, std::slice::from_ref(c))? {
                BoundExpression::ColumnRef(path) => cols.push(path),
                _ => return Err(exception("bad index column")),
            }
        }
        let mut index_type = using.unwrap_or("").to_string();
        if index_type == "art" {
            index_type = String::new();
        }
        Ok(BoundStatement::Index { index_name: name.to_string(), table: table_ref, cols, index_type })
    }

    // ---- SELECT -------------------------------------------------------------------------------------------------------------

    fn bind_values_list(&mut self, rows: &[Vec<Expr>]) -> Result<BoundTableRef> {
        let empty = BoundTableRef::Empty;
        let mut all_values: Vec<Vec<BoundExpression>> = vec![];
        for row in rows {
            let mut values = vec![];
            for e in row {
                if matches!(e, Expr::Star) {
                    return Err(exception("unsupported * in expression list"));
                }
                values.push(self.bind_expression(e, &empty)?);
            }
            if let Some(first) = all_values.first() {
                if first.len() != values.len() {
                    return Err(exception("values must have the same length"));
                }
            }
            all_values.push(values);
        }
        if all_values.is_empty() {
            return Err(exception("at least one row of values should be provided"));
        }
        Ok(BoundTableRef::ExpressionList { values: all_values, identifier: "<unnamed>".into() })
    }

    fn bind_subquery(&mut self, query: &Query, alias: &str) -> Result<BoundSubqueryRef> {
        let subquery = self.bind_select(query)?;
        let mut select_list_name = vec![];
        for col in &subquery.select_list {
            select_list_name.push(match col {
                BoundExpression::ColumnRef(name) => name.clone(),
                BoundExpression::Alias { alias, .. } => vec![alias.clone()],
                _ => vec![format!("__item#{}", self.next_id())],
            });
        }
        Ok(BoundSubqueryRef { subquery: Box::new(subquery), select_list_name, alias: alias.to_string() })
    }

    pub fn bind_select(&mut self, query: &Query) -> Result<SelectStatement> {
        let saved_cte_scope = self.cte_scope.len();
        let result = self.bind_select_inner(query);
        self.cte_scope.truncate(saved_cte_scope);
        result
    }

    fn bind_select_inner(&mut self, query: &Query) -> Result<SelectStatement> {
        let core = match &query.body {
            QueryBody::Values(rows) => {
                let values_list_name = format!("__values#{}", self.next_id());
                let mut list = self.bind_values_list(rows)?;
                let width = match &mut list {
                    BoundTableRef::ExpressionList { values, identifier } => {
                        *identifier = values_list_name.clone();
                        values[0].len()
                    }
                    _ => unreachable!(),
                };
                let exprs = (0..width).map(|i| BoundExpression::ColumnRef(vec![values_list_name.clone(), i.to_string()])).collect();
                return Ok(SelectStatement {
                    table: list,
                    select_list: exprs,
                    where_: BoundExpression::Invalid,
                    group_by: vec![],
                    having: BoundExpression::Invalid,
                    limit_count: BoundExpression::Invalid,
                    limit_offset: BoundExpression::Invalid,
                    sort: vec![],
                    ctes: vec![],
                    is_distinct: false,
                });
            }
            QueryBody::Select(core) => core,
            QueryBody::SetOp { op, all, left, right } => return self.bind_set_op(query, *op, *all, left, right),
        };

        let mut ctes = vec![];
        if !query.ctes.is_empty() {
            for cte in &query.ctes {
                if cte.recursive {
                    return Err(not_implemented("recursive CTE not supported"));
                }
                ctes.push(self.bind_subquery(&cte.query, &cte.name)?);
            }
            for cte in &ctes {
                self.cte_scope.push(CteInfo { name: cte.alias.clone(), select_list_name: cte.select_list_name.clone() });
            }
        }

        let table = self.bind_from(&core.from)?;
        if core.distinct_on {
            return Err(not_implemented("DISTINCT ON is not supported"));
        }
        let select_list = self.bind_select_list(&core.items, &table)?;
        let where_ = match &core.filter {
            Some(f) => self.bind_expression(f, &table)?,
            None => BoundExpression::Invalid,
        };
        let mut group_by = vec![];
        for g in &core.group_by {
            group_by.push(self.bind_expression_no_star(g, &table)?);
        }
        let having = match &core.having {
            Some(h) => self.bind_expression(h, &table)?,
            None => BoundExpression::Invalid,
        };
        let limit_count = match &query.limit {
            Some(e) => self.bind_expression(e, &table)?,
            None => BoundExpression::Invalid,
        };
        let limit_offset = match &query.offset {
            Some(e) => self.bind_expression(e, &table)?,
            None => BoundExpression::Invalid,
        };
        let sort = self.bind_sort(&query.order_by, &table)?;
        Ok(SelectStatement {
            table,
            select_list,
            where_,
            group_by,
            having,
            limit_count,
            limit_offset,
            sort,
            ctes,
            is_distinct: core.distinct,
        })
    }

    /// `left UNION right ORDER BY ... LIMIT ...` is a select of everything from a table that is the union: the clauses of the whole query
    /// refer to its columns, which are named like the left query's.
    fn bind_set_op(&mut self, query: &Query, op: SetOperator, all: bool, left: &Query, right: &Query) -> Result<SelectStatement> {
        let mut ctes = vec![];
        for cte in &query.ctes {
            if cte.recursive {
                return Err(not_implemented("recursive CTE not supported"));
            }
            ctes.push(self.bind_subquery(&cte.query, &cte.name)?);
        }
        for cte in &ctes {
            self.cte_scope.push(CteInfo { name: cte.alias.clone(), select_list_name: cte.select_list_name.clone() });
        }
        let alias = format!("__setop#{}", self.next_id());
        let l = self.bind_subquery(left, &format!("{alias}.left"))?;
        let r = self.bind_subquery(right, &format!("{alias}.right"))?;
        let select_list_name = l.select_list_name.clone();
        let table = BoundTableRef::SetOp { op, all, left: Box::new(l), right: Box::new(r), alias: alias.clone(), select_list_name: select_list_name.clone() };
        let select_list = select_list_name
            .iter()
            .map(|n| BoundExpression::ColumnRef(std::iter::once(alias.clone()).chain(n.iter().cloned()).collect()))
            .collect();
        let limit_count = match &query.limit {
            Some(e) => self.bind_expression(e, &table)?,
            None => BoundExpression::Invalid,
        };
        let limit_offset = match &query.offset {
            Some(e) => self.bind_expression(e, &table)?,
            None => BoundExpression::Invalid,
        };
        let sort = self.bind_sort(&query.order_by, &table)?;
        Ok(SelectStatement { table, select_list, where_: BoundExpression::Invalid, group_by: vec![], having: BoundExpression::Invalid, limit_count, limit_offset, sort, ctes, is_distinct: false })
    }

    fn bind_sort(&mut self, items: &[OrderItem], scope: &BoundTableRef) -> Result<Vec<BoundOrderBy>> {
        let mut out = vec![];
        for item in items {
            let order_type = match item.dir {
                SortDir::Default => OrderByType::Default,
                SortDir::Asc => OrderByType::Asc,
                SortDir::Desc => OrderByType::Desc,
            };
            let null_order = match item.nulls {
                SortNulls::Default => OrderByNullType::Default,
                SortNulls::First => OrderByNullType::NullsFirst,
                SortNulls::Last => OrderByNullType::NullsLast,
            };
            out.push(BoundOrderBy { order_type, null_order, expr: self.bind_expression(&item.expr, scope)? });
        }
        Ok(out)
    }

    fn bind_from(&mut self, list: &[TableRef]) -> Result<BoundTableRef> {
        match list {
            [] => Ok(BoundTableRef::Empty),
            [only] => self.bind_table_ref(only),
            [first, second, rest @ ..] => {
                let l = self.bind_table_ref(first)?;
                let r = self.bind_table_ref(second)?;
                let mut result = BoundTableRef::CrossProduct { left: Box::new(l), right: Box::new(r) };
                for t in rest {
                    let t = self.bind_table_ref(t)?;
                    result = BoundTableRef::CrossProduct { left: Box::new(result), right: Box::new(t) };
                }
                Ok(result)
            }
        }
    }

    fn bind_table_ref(&mut self, table: &TableRef) -> Result<BoundTableRef> {
        match table {
            TableRef::Named { name, alias } => {
                if let Some(cte) = self.cte_scope.iter().find(|c| &c.name == name) {
                    return Ok(BoundTableRef::Cte {
                        cte_name: cte.name.clone(),
                        alias: alias.clone().unwrap_or_else(|| name.clone()),
                        select_list_name: cte.select_list_name.clone(),
                    });
                }
                self.bind_base_table_ref(name, alias.clone())
            }
            TableRef::Subquery { query, alias } => {
                let alias = match alias {
                    Some(a) => a.clone(),
                    None => format!("__subquery#{}", self.next_id()),
                };
                Ok(BoundTableRef::Subquery(Box::new(self.bind_subquery(query, &alias)?)))
            }
            TableRef::Join { kind, left, right, on } => {
                let join_type = match kind {
                    JoinKind::Inner => JoinType::Inner,
                    JoinKind::Left => JoinType::Left,
                    JoinKind::Full => JoinType::Outer,
                    JoinKind::Right => JoinType::Right,
                    JoinKind::Cross => return Err(exception("Join type CROSS not supported")),
                };
                let left = self.bind_table_ref(left)?;
                let right = self.bind_table_ref(right)?;
                let on = on.as_ref().ok_or_else(|| exception("a join needs an ON condition"))?;
                let mut join = BoundTableRef::Join { join_type, left: Box::new(left), right: Box::new(right), condition: BoundExpression::Invalid };
                let condition = self.bind_expression(on, &join)?;
                if let BoundTableRef::Join { condition: c, .. } = &mut join {
                    *c = condition;
                }
                Ok(join)
            }
        }
    }

    fn bind_base_table_ref(&mut self, table_name: &str, alias: Option<String>) -> Result<BoundTableRef> {
        let info = self.catalog.get_table(table_name).ok_or_else(|| exception(format!("invalid table {table_name}")))?;
        Ok(BoundTableRef::Base { table: table_name.to_string(), oid: info.oid, alias, schema: info.schema.clone() })
    }

    /// `SELECT *`: a column reference for every column of every table in scope.
    fn get_all_columns(&self, scope: &BoundTableRef) -> Result<Vec<BoundExpression>> {
        match scope {
            BoundTableRef::Base { schema, .. } => {
                let bound_name = scope.bound_table_name().unwrap();
                Ok(schema.columns().iter().map(|c| BoundExpression::ColumnRef(vec![bound_name.to_string(), c.name().to_string()])).collect())
            }
            BoundTableRef::CrossProduct { left, right } | BoundTableRef::Join { left, right, .. } => {
                let mut columns = self.get_all_columns(left)?;
                columns.extend(self.get_all_columns(right)?);
                Ok(columns)
            }
            BoundTableRef::Subquery(s) => Ok(s.select_list_name.iter().map(|n| BoundExpression::ColumnRef(prepend(&s.alias, n))).collect()),
            BoundTableRef::Cte { alias, select_list_name, .. } => {
                Ok(select_list_name.iter().map(|n| BoundExpression::ColumnRef(prepend(alias, n))).collect())
            }
            _ => Err(exception("select * cannot be used with this TableReferenceType")),
        }
    }

    fn bind_select_list(&mut self, items: &[SelectItem], scope: &BoundTableRef) -> Result<Vec<BoundExpression>> {
        let mut select_list: Vec<BoundExpression> = vec![];
        let mut is_select_star = false;
        let (mut has_agg, mut has_window) = (false, false);
        for item in items {
            match item {
                SelectItem::Wildcard => {
                    if !select_list.is_empty() {
                        return Err(exception("select * cannot have other expressions in list"));
                    }
                    select_list = self.get_all_columns(scope)?;
                    is_select_star = true;
                }
                SelectItem::Expr { expr, alias } => {
                    if is_select_star {
                        return Err(exception("select * cannot have other expressions in list"));
                    }
                    let mut bound = self.bind_expression(expr, scope)?;
                    if let Some(a) = alias {
                        bound = BoundExpression::Alias { alias: a.clone(), child: Box::new(bound) };
                    }
                    has_agg |= bound.has_aggregation();
                    has_window |= bound.has_window_function();
                    select_list.push(bound);
                }
            }
        }
        if has_agg && has_window {
            return Err(exception("cannot have both normal agg and window agg in same query"));
        }
        Ok(select_list)
    }

    // ---- expressions --------------------------------------------------------------------------------------------------------

    fn bind_expression_no_star(&mut self, e: &Expr, scope: &BoundTableRef) -> Result<BoundExpression> {
        if matches!(e, Expr::Star) {
            return Err(exception("unsupported * in expression list"));
        }
        self.bind_expression(e, scope)
    }

    pub fn bind_expression(&mut self, e: &Expr, scope: &BoundTableRef) -> Result<BoundExpression> {
        match e {
            Expr::Integer(v) => {
                if *v > i32::MAX as i64 {
                    return Err(exception("value out of range"));
                }
                Ok(BoundExpression::Constant(Value::integer(*v as i32)))
            }
            Expr::Float(s) => {
                let parsed = s.parse::<f64>().map_err(|_| exception(format!("bad number {s}")))?;
                Ok(BoundExpression::Constant(Value::decimal(parsed)))
            }
            Expr::Str(s) => Ok(BoundExpression::Constant(Value::varchar(s))),
            Expr::Bool(b) => Ok(BoundExpression::Constant(Value::boolean(*b))),
            Expr::Null => Ok(BoundExpression::Constant(Value::null(TypeId::Integer))),
            Expr::Column(names) => self.resolve_column(scope, names),
            Expr::Star => Ok(BoundExpression::Star),
            Expr::Binary { op, left, right } => {
                let l = self.bind_expression_no_star(left, scope)?;
                let r = self.bind_expression_no_star(right, scope)?;
                Ok(BoundExpression::BinaryOp { op: op.clone(), left: Box::new(l), right: Box::new(r) })
            }
            Expr::Unary { op, expr } => {
                let arg = self.bind_expression_no_star(expr, scope)?;
                Ok(BoundExpression::UnaryOp { op: op.clone(), arg: Box::new(arg) })
            }
            Expr::Param(i) => Err(exception(format!("there is no value for parameter ${}: prepare the statement and execute it with values", i + 1))),
            Expr::Case { branches, otherwise } => {
                let _ = (branches, otherwise);
                Err(not_implemented("CASE is not supported yet"))
            }
            Expr::IsNull { expr, negated } => {
                let _ = (expr, negated);
                Err(exception("Expr of type PGNullTest not implemented"))
            }
            Expr::Function { name, args, distinct, over } => {
                let mut children = vec![];
                for a in args {
                    children.push(self.bind_expression(a, scope)?);
                }
                let mut function_name = lower(name);
                // TODO(3i-03): `coalesce(a, b, ...)` and `nullif(a, b)` rewritten to a Case
                if ["min", "max", "first", "last", "sum", "count", "rank", "row_number"].contains(&function_name.as_str()) {
                    if (function_name == "count" && children.is_empty()) || function_name == "row_number" {
                        function_name = "count_star".into();
                    }
                    if let Some(spec) = over {
                        if *distinct {
                            return Err(exception("DISTINCT is not supported in window functions"));
                        }
                        return self.bind_window_expression(function_name, children, spec, scope);
                    }
                    return Ok(BoundExpression::AggCall { func_name: function_name, is_distinct: *distinct, args: children });
                }
                Ok(BoundExpression::FuncCall { func_name: function_name, args: children })
            }
        }
    }

    fn bind_window_expression(&mut self, func_name: String, args: Vec<BoundExpression>, spec: &WindowSpec, scope: &BoundTableRef) -> Result<BoundExpression> {
        let mut partition_by = vec![];
        for p in &spec.partition_by {
            partition_by.push(self.bind_expression_no_star(p, scope)?);
        }
        let order_bys = self.bind_sort(&spec.order_by, scope)?;
        let (start, end) = match &spec.frame {
            // PostgreSQL's default frame: RANGE BETWEEN UNBOUNDED PRECEDING AND CURRENT ROW
            None => (WindowBoundary::UnboundedPreceding, WindowBoundary::CurrentRowRange),
            Some(frame) => {
                let boundary = |b: &FrameBound, is_start: bool| -> Result<WindowBoundary> {
                    Ok(match b {
                        FrameBound::UnboundedPreceding => WindowBoundary::UnboundedPreceding,
                        FrameBound::UnboundedFollowing => WindowBoundary::UnboundedFollowing,
                        FrameBound::CurrentRow if frame.range => WindowBoundary::CurrentRowRange,
                        FrameBound::CurrentRow => WindowBoundary::CurrentRowRows,
                        FrameBound::Preceding(_) if frame.range => WindowBoundary::ExprPrecedingRange,
                        FrameBound::Preceding(_) => WindowBoundary::ExprPrecedingRows,
                        FrameBound::Following(_) if frame.range => WindowBoundary::ExprFollowingRange,
                        FrameBound::Following(_) => WindowBoundary::ExprFollowingRows,
                    })
                    .and_then(|w| {
                        if (is_start && w == WindowBoundary::UnboundedFollowing) || (!is_start && w == WindowBoundary::UnboundedPreceding) {
                            Err(exception("Window frames starting with unbounded following or ending in unbounded preceding make no sense"))
                        } else {
                            Ok(w)
                        }
                    })
                };
                (boundary(&frame.start, true)?, boundary(&frame.end, false)?)
            }
        };
        Ok(BoundExpression::Window(Box::new(BoundWindow { func_name, args, partition_by, order_bys, start, end })))
    }

    // ---- name resolution ----------------------------------------------------------------------------------------------------

    /// Finds the column `col_name` in the tables of `scope` and returns it as a column reference `[table or alias, column]`.
    pub fn resolve_column(&self, scope: &BoundTableRef, col_name: &[String]) -> Result<BoundExpression> {
        if scope.is_invalid() {
            return Err(exception("invalid scope"));
        }
        match self.resolve_column_internal(scope, col_name)? {
            Some(path) => Ok(BoundExpression::ColumnRef(path)),
            None => Err(exception(format!("column {} not found", col_name.join(".")))),
        }
    }

    fn resolve_column_internal(&self, table_ref: &BoundTableRef, col_name: &[String]) -> Result<Option<Vec<String>>> {
        match table_ref {
            BoundTableRef::Base { .. } => self.resolve_column_ref_from_base_table_ref(table_ref, col_name),
            BoundTableRef::CrossProduct { left, right } | BoundTableRef::Join { left, right, .. } => {
                let l = self.resolve_column_internal(left, col_name)?;
                let r = self.resolve_column_internal(right, col_name)?;
                match (l, r) {
                    (Some(_), Some(_)) => Err(exception(format!("{} is ambiguous", col_name.join(".")))),
                    (Some(l), None) => Ok(Some(l)),
                    (None, r) => Ok(r),
                }
            }
            BoundTableRef::Subquery(s) => Self::resolve_column_ref_from_subquery(&s.select_list_name, &s.alias, col_name),
            BoundTableRef::Cte { alias, select_list_name, .. } => Self::resolve_column_ref_from_subquery(select_list_name, alias, col_name),
            BoundTableRef::SetOp { alias, select_list_name, .. } => Self::resolve_column_ref_from_subquery(select_list_name, alias, col_name),
            _ => Err(exception("unsupported TableReferenceType")),
        }
    }

    fn resolve_column_ref_from_schema(schema: &Schema, col_name: &[String]) -> Result<Option<Vec<String>>> {
        if col_name.len() != 1 {
            return Ok(None);
        }
        let mut found: Option<Vec<String>> = None;
        for column in schema.columns() {
            if lower(column.name()) == col_name[0] {
                if found.is_some() {
                    return Err(exception(format!("{} is ambiguous in schema", col_name.join("."))));
                }
                found = Some(vec![column.name().to_string()]);
            }
        }
        Ok(found)
    }

    /// `alias.column` or `table.column` for a column of a base table (`column` alone also works).
    fn resolve_column_ref_from_base_table_ref(&self, table_ref: &BoundTableRef, col_name: &[String]) -> Result<Option<Vec<String>>> {
        let BoundTableRef::Base { table, schema, .. } = table_ref else {
            return Ok(None);
        };
        let bound_name = table_ref.bound_table_name().unwrap();
        let direct = Self::resolve_column_ref_from_schema(schema, col_name)?.map(|n| prepend(bound_name, &n));
        let mut strip = None;
        if col_name.len() > 1 && col_name[0] == bound_name {
            strip = Self::resolve_column_ref_from_schema(schema, &col_name[1..])?.map(|n| prepend(bound_name, &n));
        }
        if strip.is_some() && direct.is_some() {
            return Err(exception(format!("{} is ambiguous in table {}", col_name.join("."), table)));
        }
        Ok(strip.or(direct))
    }

    fn match_suffix(suffix: &[String], full_name: &[String]) -> bool {
        let full: Vec<String> = full_name.iter().map(|c| lower(c)).collect();
        suffix.len() <= full.len() && suffix.iter().rev().zip(full.iter().rev()).all(|(a, b)| a == b)
    }

    fn resolve_column_ref_from_select_list(select_list: &[Vec<String>], col_name: &[String]) -> Result<Option<Vec<String>>> {
        let mut found: Option<Vec<String>> = None;
        for full in select_list {
            if Self::match_suffix(col_name, full) {
                if found.is_some() {
                    return Err(exception(format!("{} is ambiguous in subquery select list", col_name.join("."))));
                }
                found = Some(full.clone());
            }
        }
        Ok(found)
    }

    fn resolve_column_ref_from_subquery(select_list: &[Vec<String>], alias: &str, col_name: &[String]) -> Result<Option<Vec<String>>> {
        let direct = Self::resolve_column_ref_from_select_list(select_list, col_name)?.map(|n| prepend(alias, &n));
        let mut strip = None;
        if col_name.len() > 1 && col_name[0] == alias {
            strip = Self::resolve_column_ref_from_select_list(select_list, &col_name[1..])?.map(|n| prepend(alias, &n));
        }
        if strip.is_some() && direct.is_some() {
            return Err(exception(format!("{} is ambiguous in subquery {}", col_name.join("."), alias)));
        }
        Ok(strip.or(direct))
    }
}
