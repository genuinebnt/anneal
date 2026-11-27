---
title: Name resolution: what does `v1` mean here?
summary: How a binder maps the names in a query to columns of tables in scope, with aliases, qualified names, ambiguity errors, subqueries and case rules, and why the result is a position.
minutes: 8
---
`select v1 from t1, t2` is fine if only one of the tables has a `v1`, an error if both do, and fine again as `select t1.v1 ...`. In `select a.v1 from t1 a` the table is called `a` and `t1.v1` is no longer a valid name. Turning those names into *which column of which input* is **binding**, and it is the stage where SQL's scoping rules live.

## Scope: the tables a name can refer to

The `FROM` clause defines the **scope** of a select: the list of tables (and subqueries) whose columns can be named. Each entry is visible under one *bound name*: its alias if it has one, else its table name. Looking up a column asks every table in scope:

| written | rule |
|---|---|
| `v1` | any column called `v1` in any table in scope; exactly one is fine, two is **ambiguous**, none is **not found** |
| `t1.v1` | the column `v1` of the table whose bound name is `t1` |
| `a.v1` with `from t1 a` | the same; `t1.v1` would now be *not found* |
| `x.v1` with `from (select v1 from t1) x` | a column of the subquery's select list |

After resolving, BusTub names the column by its full path, `[bound name, column]`, i.e. `t1.v1`, so that two columns called `v1` in a join stay distinguishable in the plan's schemas.

## Case

SQL folds unquoted names to one case (PostgreSQL: lower case), so `V1`, `v1` and `v1` are the same name, and `"V1"` is a different one. BusTub's parser lower-cases unquoted identifiers, and the binder compares case-insensitively with the declared column names (`colA` is declared with a capital, matched by `cola`).

## Subqueries and `SELECT *`

A subquery in `FROM` has its own select list; the outer query sees those names, prefixed by the subquery's alias. `select *` is also resolved here: the binder expands it to one column reference per column of every table in scope, in order, so later stages never see a star.

## Why a position comes out

A plan operator works with tuples of values, not names: its expressions say *column 2 of my input*. The planner (the stage after) takes the binder's `t1.v1` path and finds its index in the child operator's output schema. The binder's job is to make the name unambiguous; the planner's job is to make it a number.

## C++ comparison

| C / C++ | Rust |
|---|---|
| `std::unique_ptr<BoundColumnRef>` that is `nullptr` when not found | `Option<Vec<String>>` |
| `throw Exception("... is ambiguous")` in the middle of a loop | `Err(..)` returned by `?` |
| `StringUtil::Lower(a) == b` | `a.to_lowercase() == b` (Unicode-aware) |
| visitor over `BoundTableRef` subclasses with `dynamic_cast` | `match` on an `enum BoundTableRef` |

## In real code

### Using it: resolving names in a scope

```rust test
#[derive(Debug, Clone, PartialEq)]
struct Table { name: &'static str, alias: Option<&'static str>, columns: Vec<&'static str> }

impl Table {
    fn bound_name(&self) -> &str { self.alias.unwrap_or(self.name) }
}

/// `v1` or `t.v1`: the path `[bound table name, column]`, or an error.
fn resolve(scope: &[Table], name: &[&str]) -> Result<Vec<String>, String> {
    let mut found: Vec<Vec<String>> = vec![];
    for t in scope {
        let (qualifier_ok, column) = match name {
            [c] => (true, *c),
            [q, c] => (q.eq_ignore_ascii_case(t.bound_name()), *c),
            _ => return Err("unsupported name".into()),
        };
        if !qualifier_ok {
            continue;
        }
        for c in &t.columns {
            if c.eq_ignore_ascii_case(column) {
                found.push(vec![t.bound_name().to_string(), c.to_string()]);
            }
        }
    }
    match found.len() {
        0 => Err(format!("column {} not found", name.join("."))),
        1 => Ok(found.pop().unwrap()),
        _ => Err(format!("{} is ambiguous", name.join("."))),
    }
}

fn scope() -> Vec<Table> {
    vec![
        Table { name: "t1", alias: None, columns: vec!["v1", "v2"] },
        Table { name: "t2", alias: Some("b"), columns: vec!["v1", "colA"] },
    ]
}

#[test]
fn an_unqualified_name_must_match_exactly_one_column() {
    assert_eq!(resolve(&scope(), &["v2"]).unwrap(), ["t1", "v2"]);
    assert_eq!(resolve(&scope(), &["cola"]).unwrap(), ["b", "colA"], "case-insensitive, declared case kept");
    assert_eq!(resolve(&scope(), &["v1"]).unwrap_err(), "v1 is ambiguous");
    assert_eq!(resolve(&scope(), &["nope"]).unwrap_err(), "column nope not found");
}

#[test]
fn a_qualified_name_picks_the_table_by_its_bound_name() {
    assert_eq!(resolve(&scope(), &["t1", "v1"]).unwrap(), ["t1", "v1"]);
    assert_eq!(resolve(&scope(), &["b", "v1"]).unwrap(), ["b", "v1"]);
}

#[test]
fn an_alias_hides_the_table_name() {
    assert_eq!(resolve(&scope(), &["t2", "v1"]).unwrap_err(), "column t2.v1 not found");
}
```

### In the exercises

- **3d-02:** the binder (given) does this before your factory is called: `lower(day_of_week)` is bound to `__mock_table_schedule.day_of_week`.
- **Module 3f (joins):** `select * from t1 inner join t1 temp on ...` works because of aliases; the plan's schemas carry the bound names.

### Where it is used

- **PostgreSQL**: `parse_relation.c` (`colNameToVar`, "column reference is ambiguous").
- **SQLite**: `resolveExprNames` in `resolve.c`, with the same ambiguity error.
- **DuckDB / DataFusion**: a *binder* and a *schema* with qualified names (`DFSchema` fields carry a table qualifier).
- **Compilers**: scope resolution of variable names is the same problem with block scopes.
