//! The native app's reading view-model (`crates/blyg-app/src/reading/vm.rs`),
//! compiled unchanged: the list's order and visibility, titles, hosts,
//! badges and dates, the reading search, reply stubs, quote labels and
//! partial quotes. It reaches its parent for `View` and `own::FORK_OWN`,
//! which are mirrored here.

// These modules are blyg-app's, compiled unchanged: each build (wasm, the
// native tests) uses a different part of them, so no `expect` holds on both.
#![allow(
    dead_code,
    reason = "blyg-app's modules, compiled unchanged; each build uses a different part"
)]

/// The native screens (`crates/blyg-app/src/reading/mod.rs`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum View {
    Posts,
    Reading,
    Mentions,
    Subscriptions,
}

pub mod own {
    /// `crates/blyg-app/src/reading/own.rs`.
    pub const FORK_OWN: bool = true;
    // --- web-reading --- what `sources_vm.rs` reads of `own.rs`.
    pub use blyg_core::OWN_SUBSCRIPTION as OWN;
    pub use blyg_core::own::is_own_row;
    pub const OWN_LABEL: &str = "Your blyg";
    // --- end web-reading ---
}

// --- contacts --- why a post is in the Mentions feed, compiled unchanged.
#[path = "../../../blyg-app/src/reading/mentions_vm.rs"]
pub mod mentions_vm;

#[path = "../../../blyg-app/src/reading/vm.rs"]
pub mod vm;

// --- web-reading --- the Reader's sources pane rules (smart feeds,
// Background, hoppers, subscriptions), compiled unchanged.
#[path = "../../../blyg-app/src/reading/sources_vm.rs"]
pub mod sources_vm;
// --- end web-reading ---

// --- reader order --- studio 0.68's tabs: the Feed's order (pace-layered
// or chronological), Raw's order and the tabs' texts, compiled unchanged
// (`feed_vm.rs`; the app's `feed.rs` is its GPUI half). It reads
// `stream_vm::ReadMode`, so the stream's view-model comes too.
#[path = "../../../blyg-app/src/reading/feed_vm.rs"]
pub mod feed;
#[path = "../../../blyg-app/src/reading/stream_vm.rs"]
pub mod stream_vm;
