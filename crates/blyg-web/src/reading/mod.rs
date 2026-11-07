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
    /// The experimental #thicket tab (native only; never shown here).
    Chat,
}

pub mod own {
    /// `crates/blyg-app/src/reading/own.rs`.
    pub const FORK_OWN: bool = true;
    // --- web-reading --- what `sources_vm.rs` reads of `own.rs`.
    pub const OWN: &str = "~own";
    pub const OWN_LABEL: &str = "Your blyg";
    pub fn is_own_row(r: &blyg_core::ReadingItem) -> bool {
        r.subscription_id == OWN
    }
    // --- end web-reading ---
}

#[path = "../../../blyg-app/src/reading/vm.rs"]
#[allow(dead_code)]
pub mod vm;

// --- web-reading --- the Reader's sources pane rules (smart feeds,
// Background, hoppers, subscriptions), compiled unchanged.
#[path = "../../../blyg-app/src/reading/sources_vm.rs"]
#[allow(dead_code)]
pub mod sources_vm;
// --- end web-reading ---
