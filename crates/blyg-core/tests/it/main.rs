//! The crate's integration tests in one binary (one link instead of one
//! per file): one module per suite.

#[path = "../common/mod.rs"]
mod common;

mod auth;
mod config;
mod config_docs;
mod contacts;
mod crate_graph;
mod e2e;
mod profiles;
mod read_sync;
mod reading;
mod save_if_base;
mod scratch;
mod sync;
mod typing;
mod versions;
