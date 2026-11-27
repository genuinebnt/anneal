---
title: The SQL pipeline: from text to rows
summary: The five stages every SQL engine has (parse, bind, plan, optimize, execute), what each one adds, where errors come from at each stage, and why EXPLAIN is the debugger of a query engine.
minutes: 10
---
SQL says *what* you want, not *how* to get it. Between the text and the rows sits a pipeline, and nearly every relational database has the same five stages, because each one needs information the previous one did not have.

```svg
caption: The path of a query in BusTub. Each stage turns one representation into the next; errors found early (a typo, an unknown table, upper(1)) never reach a row. EXPLAIN prints the plan between the planner and the engine.
<svg viewBox="0 0 780 190" role="img" aria-label="Five boxes: parser, binder, planner, optimizer, execution engine">
<defs><marker id="sp2-a" markerWidth="9" markerHeight="9" refX="7" refY="4.5" orient="auto"><path d="M0 0 L9 4.5 L0 9 z" style="fill:var(--dim)"/></marker></defs>
<rect class="live" x="10" y="50" width="128" height="62" rx="3"/><text class="mid fg" x="74" y="76">parser</text><text class="mid dim sm" x="74" y="96">text → syntax tree</text>
<rect class="live" x="162" y="50" width="128" height="62" rx="3"/><text class="mid fg" x="226" y="76">binder</text><text class="mid dim sm" x="226" y="96">names → objects</text>
<rect class="blue" x="314" y="50" width="128" height="62" rx="3"/><text class="mid t-b" x="378" y="76">planner</text><text class="mid dim sm" x="378" y="96">→ operator tree</text>
<rect class="live" x="466" y="50" width="128" height="62" rx="3"/><text class="mid fg" x="530" y="76">optimizer</text><text class="mid dim sm" x="530" y="96">plan → better plan</text>
<rect class="live" x="618" y="50" width="150" height="62" rx="3"/><text class="mid fg" x="693" y="76">engine</text><text class="mid dim sm" x="693" y="96">executors pull rows</text>
<path class="ln" d="M138 81 L160 81" marker-end="url(#sp2-a)"/><path class="ln" d="M290 81 L312 81" marker-end="url(#sp2-a)"/><path class="ln" d="M442 81 L464 81" marker-end="url(#sp2-a)"/><path class="ln" d="M594 81 L616 81" marker-end="url(#sp2-a)"/>
<text class="dim sm" x="74" y="140" style="text-anchor:middle">syntax errors</text><text class="dim sm" x="226" y="140" style="text-anchor:middle">unknown table / column,</text><text class="dim sm" x="226" y="156" style="text-anchor:middle">ambiguous name</text><text class="dim sm" x="378" y="140" style="text-anchor:middle">wrong types, upper(1),</text><text class="dim sm" x="378" y="156" style="text-anchor:middle">unsupported operator</text><text class="dim sm" x="693" y="140" style="text-anchor:middle">overflow, bad cast,</text><text class="dim sm" x="693" y="156" style="text-anchor:middle">conflict</text>
<text class="t-b sm" x="378" y="30" style="text-anchor:middle">EXPLAIN shows this</text>
</svg>
```

## What each stage knows

| stage | input → output | what it needs | what it can reject |
|---|---|---|---|
| **parser** | text → *syntax tree* | the grammar only | `selct 1;` |
| **binder** | syntax tree → *bound tree* | the catalog (which tables and columns exist) | an unknown table, an ambiguous column |
| **planner** | bound tree → *plan* | operator shapes, expression types | `upper(1)`, `'a' + 1`, `*` (unsupported) |
| **optimizer** | plan → *equivalent plan* | the catalog again (indexes, sizes) | nothing: it must keep the answer the same |
| **engine** | plan → *rows* | the data | overflow, division by zero, conflicts |

The split is not accidental. The **parser** can run without a database at all. The **binder** is where the same text means different things in different databases (which `t1`?). The **planner** decides *shape*: a filter above a scan, a join of two scans. The **optimizer** is allowed to change the shape, never the result. The **engine** is the only stage that reads data, which is why a mistake that can be found earlier should be.

## Plans are trees of operators

A plan for `select name from people where age > 30 order by name` is a tree read from the leaves up: `Scan(people)` → `Filter(age > 30)` → `Projection(name)` → `Sort(name)`. Each node has an output schema, the columns of the tuples it produces; each node's expressions refer to columns of its *child's* output by position. `EXPLAIN` prints exactly that, and a plan you do not understand is almost always a plan you have not `EXPLAIN`ed.

## Why separate "bind" from "plan"?

A real planner needs more than names resolved: it chooses join orders, picks access methods, decides on sorting. BusTub keeps the stage boundary because its binder is shared by every statement type (`INSERT`, `UPDATE`, `CREATE INDEX`), while the planner is specific to queries. In PostgreSQL the same split is "analyze" (parse tree → query tree) and "plan".

## C++ comparison

| C / C++ | Rust |
|---|---|
| a parser generated from a grammar (`libpg_query`, a copy of PostgreSQL's) | a hand-written recursive-descent parser in `src/sql/` |
| `BoundStatement` class hierarchy and `dynamic_cast` | an `enum BoundStatement` and `match` |
| `AbstractPlanNode` subclasses | one `PlanNode` struct plus an `enum PlanKind` |
| exceptions thrown from any stage | `Result<_, Exception>` returned up to `execute_sql` |

## In real code

### Using it: a four-stage toy

```rust test
use std::collections::HashMap;

// ---- parse: text -> syntax tree ---------------------------------------------------------------------------------------------
#[derive(Debug, PartialEq, Clone)]
enum Ast { Col(String), Num(i64), Add(Box<Ast>, Box<Ast>) }
#[derive(Debug, PartialEq)]
struct Select { items: Vec<Ast>, table: String }

fn parse_expr(s: &str) -> Result<Ast, String> {
    let s = s.trim();
    if let Some((l, r)) = s.split_once('+') {
        return Ok(Ast::Add(Box::new(parse_expr(l)?), Box::new(parse_expr(r)?)));
    }
    match s.parse::<i64>() {
        Ok(n) => Ok(Ast::Num(n)),
        Err(_) if !s.is_empty() && s.chars().all(|c| c.is_alphanumeric() || c == '_') => Ok(Ast::Col(s.to_lowercase())),
        Err(_) => Err(format!("syntax error at or near {s:?}")),
    }
}

fn parse(sql: &str) -> Result<Select, String> {
    let sql = sql.trim().trim_end_matches(';');
    let rest = sql.strip_prefix("select ").ok_or("syntax error: expected SELECT")?;
    let (items, table) = rest.split_once(" from ").ok_or("syntax error: expected FROM")?;
    Ok(Select { items: items.split(',').map(parse_expr).collect::<Result<_, _>>()?, table: table.trim().to_string() })
}

// ---- bind: names -> positions in the catalog ---------------------------------------------------------------------------------
#[derive(Debug, PartialEq, Clone)]
enum Bound { Col(usize), Num(i64), Add(Box<Bound>, Box<Bound>) }

fn bind(select: &Select, catalog: &HashMap<&str, Vec<&str>>) -> Result<Vec<Bound>, String> {
    let columns = catalog.get(select.table.as_str()).ok_or(format!("invalid table {}", select.table))?;
    fn go(a: &Ast, columns: &[&str]) -> Result<Bound, String> {
        Ok(match a {
            Ast::Num(n) => Bound::Num(*n),
            Ast::Col(c) => Bound::Col(columns.iter().position(|x| x == c).ok_or(format!("column {c} not found"))?),
            Ast::Add(l, r) => Bound::Add(Box::new(go(l, columns)?), Box::new(go(r, columns)?)),
        })
    }
    select.items.iter().map(|a| go(a, columns)).collect()
}

// ---- execute: bound tree + rows -> values -------------------------------------------------------------------------------------
fn eval(b: &Bound, row: &[i64]) -> i64 {
    match b { Bound::Num(n) => *n, Bound::Col(i) => row[*i], Bound::Add(l, r) => eval(l, row) + eval(r, row) }
}

fn run(sql: &str, catalog: &HashMap<&str, Vec<&str>>, rows: &[Vec<i64>]) -> Result<Vec<Vec<i64>>, String> {
    let bound = bind(&parse(sql)?, catalog)?;
    Ok(rows.iter().map(|row| bound.iter().map(|b| eval(b, row)).collect()).collect())
}

fn catalog() -> HashMap<&'static str, Vec<&'static str>> {
    HashMap::from([("t", vec!["a", "b"])])
}

#[test]
fn text_becomes_values_through_the_stages() {
    let rows = vec![vec![1, 10], vec![2, 20]];
    assert_eq!(run("select a + 1, b from t;", &catalog(), &rows).unwrap(), vec![vec![2, 10], vec![3, 20]]);
    assert_eq!(parse("select a from t").unwrap(), Select { items: vec![Ast::Col("a".into())], table: "t".into() });
}

#[test]
fn each_stage_rejects_what_only_it_can_see() {
    let rows = vec![];
    assert!(run("selec a from t", &catalog(), &rows).unwrap_err().contains("syntax"), "the parser: grammar");
    assert_eq!(run("select a from nope", &catalog(), &rows).unwrap_err(), "invalid table nope", "the binder: the catalog");
    assert_eq!(run("select zzz from t", &catalog(), &rows).unwrap_err(), "column zzz not found", "the binder: columns");
}

#[test]
fn binding_replaces_names_by_positions() {
    let bound = bind(&parse("select b + a from t").unwrap(), &catalog()).unwrap();
    assert_eq!(bound, vec![Bound::Add(Box::new(Bound::Col(1)), Box::new(Bound::Col(0)))]);
    assert_eq!(eval(&bound[0], &[3, 4]), 7);
}
```

### In the exercises

- **3d-01 and 3d-02:** you write the nodes the *engine* evaluates, and the function factory the *planner* calls; `explain` shows the result.
- **3d-03 and 3d-04:** you write the front of the pipeline for expressions: the lexer and the expression parser.
- **3d-05:** `execute_sql` runs all five stages for each record of the `.slt` files.
- **Modules 3e to 3h:** executors (the engine), then the optimizer rules.

### Where it is used

- **PostgreSQL**: `raw_parser` → `parse_analyze` (binding) → `pg_plan_query` (planner and optimizer in one) → the executor. `EXPLAIN` prints the plan.
- **SQLite**: the parser builds an AST; code generation turns it into bytecode for the VDBE (a different shape of the same pipeline). `EXPLAIN` prints the bytecode.
- **DuckDB**: parser (PostgreSQL's, as BusTub), binder, logical planner, optimizer, physical planner, vectorised executors.
- **DataFusion** (Rust): `sqlparser` → logical plan → optimizer → physical plan → streams of record batches.
