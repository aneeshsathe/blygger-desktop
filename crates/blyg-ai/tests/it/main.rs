//! The crate's integration tests in one binary (one link instead of one
//! per file): one module per suite.

#[path = "../common/mod.rs"]
mod common;

mod accounts;
mod agentic_prompt; // --- agentic writer ---
mod chatgpt;
mod cli_bridges;
mod http_providers;
mod hydrate;
mod prompts; // --- studio 0.71 --- [TK] link reading
