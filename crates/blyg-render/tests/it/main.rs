//! The crate's integration tests in one binary (one link instead of one
//! per file): one module per suite.

#[path = "../common/mod.rs"]
mod common;

mod engine_parity;
mod math_shape;
mod parity;
mod preview;
mod spec102;
