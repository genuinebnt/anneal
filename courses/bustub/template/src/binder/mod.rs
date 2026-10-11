//! Name resolution: decides which table a name means, which column a reference is and what `*` expands to.
//!
//! Produces the bound tree the planner turns into a plan.

pub mod binder;
pub mod bound_expression;
pub mod bound_order_by;
pub mod bound_statement;
pub mod bound_table_ref;
