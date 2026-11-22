//! The crate's integration tests in one binary (one link instead of one
//! per file): one module per suite.

#![expect(
    clippy::disallowed_types,
    reason = "tests run git, cargo, scripts and the e2e studio"
)]

#[path = "../common/mod.rs"]
mod common;

mod agentic;
mod auth;
mod capability_gates;
mod changelog_stitch;
mod config;
mod config_docs;
mod contacts;
mod contract_coverage;
mod crate_graph;
mod dictionary;
mod doc_paths;
mod e2e;
mod helpers;
mod lint_exceptions;
mod list_prune_scan;
mod newer_contract;
mod no_git;
mod personal_data;
mod profiles;
mod publish;
mod read_sync;
mod reader_order;
mod reading;
mod save_if_base;
mod scratch;
mod sync;
mod syndication_contract;
mod test_first_script;
mod timing_scan;
mod typing;
mod versions;

mod version_gates;
