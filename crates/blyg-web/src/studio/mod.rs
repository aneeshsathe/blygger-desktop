//! The native app's quote resolver and snapshot sanitiser, compiled
//! unchanged from `crates/blyg-app/src/studio/` (`resolver.rs` reaches its
//! sibling as `super::sanitize`).

#![allow(
    dead_code,
    reason = "blyg-app's modules, compiled unchanged; each build uses a different part"
)]

#[path = "../../../blyg-app/src/studio/resolver.rs"]
pub mod resolver;
#[path = "../../../blyg-app/src/studio/sanitize.rs"]
pub mod sanitize;
