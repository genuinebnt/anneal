//! Reference backend: a multi-tenant "projects and tasks" API built from one module per building block.
pub mod auth;
pub mod cache;
pub mod config;
pub mod db;
pub mod error;
pub mod http;
pub mod idempotency;
pub mod jobs;
pub mod metrics;
pub mod objstore;
pub mod ports;
pub mod query;
pub mod ratelimit;
pub mod server;
pub mod state;
pub mod testkit;
