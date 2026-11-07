//! The native app's reading view-model (`crates/blyg-app/src/reading/vm.rs`),
//! compiled unchanged: the list's order and visibility, titles, hosts,
//! badges and dates, the reading search, reply stubs, quote labels and
//! partial quotes. It reaches its parent for `View` and `own::FORK_OWN`,
//! which are mirrored here.

/// The native screens (`crates/blyg-app/src/reading/mod.rs`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub enum View {
    Posts,
    Reading,
    Mentions,
    Subscriptions,
}

pub mod own {
    /// `crates/blyg-app/src/reading/own.rs`.
    pub const FORK_OWN: bool = true;
}

#[path = "../../../blyg-app/src/reading/vm.rs"]
#[allow(dead_code)]
pub mod vm;
