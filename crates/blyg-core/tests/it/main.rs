//! The crate's integration tests in one binary (one link instead of one
//! per file): one module per suite.

#![expect(
    clippy::disallowed_types,
    reason = "tests run git, cargo, scripts and the e2e studio"
)]

#[path = "../common/mod.rs"]
mod common;

mod auth;
mod config;
mod config_docs;
mod contacts;
mod contract_coverage;
mod crate_graph;
mod e2e;
mod profiles;
mod read_sync;
mod reading;
mod save_if_base;
mod scratch;
mod sync;
mod test_first_script;
mod typing;
mod versions;
