//! A relational database engine. `common` and `types` hold the shared vocabulary; `storage`, `buffer`, `container` and `recovery`
//! hold data on disk and in memory; `sql`, `binder`, `planner`, `optimizer` and `execution` turn a statement into rows; `catalog` and
//! `concurrency` tie them together.
#![allow(unused_variables, unused_mut, unused_imports, unreachable_code, dead_code)]

pub mod binder;
pub mod buffer;
pub mod catalog;
pub mod common;
pub mod concurrency;
pub mod container;
pub mod execution;
pub mod optimizer;
pub mod planner;
pub mod primer;
pub mod recovery;
pub mod rust_primer;
pub mod sql;
pub mod storage;
pub mod types;
