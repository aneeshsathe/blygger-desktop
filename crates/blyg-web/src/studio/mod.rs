//! The native app's quote resolver and snapshot sanitiser, compiled
//! unchanged from `crates/blyg-app/src/studio/` (`resolver.rs` reaches its
//! sibling as `super::sanitize`).

#[path = "../../../blyg-app/src/studio/resolver.rs"]
pub mod resolver;
#[path = "../../../blyg-app/src/studio/sanitize.rs"]
#[allow(dead_code)]
pub mod sanitize;
