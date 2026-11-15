//! --- web-parity --- What the studio does that the web lacked (the
//! studio-parity audit, 2026-11-15), as the studio does it:
//!
//! - **Mark all read** for the chosen Reading source and lens
//!   (`mark_all.rs`).
//! - **Status filters** over Posts (`filters.rs`).
//!
//! Every server feature is gated on the operation names the blyg
//! advertises in `GET /api/capabilities` (`wr_can`), never on its
//! `contractVersion`: the studio may reuse a contract number without the
//! feature.

use wasm_bindgen::prelude::*;

use crate::Burrow;

pub mod filters;
pub mod mark_all;

/// The web's state for these screens. Nothing here is persisted.
#[derive(Default)]
pub struct Parity {
    /// The Posts list's filter.
    pub filter: filters::PostFilter,
}

impl Burrow {
    pub(crate) fn can(&self, op: &str) -> bool {
        self.wr.ops.as_ref().is_some_and(|o| o.contains(op))
    }
}

#[wasm_bindgen]
impl Burrow {
    /// Whether the blyg advertises operation `op` (`/api/capabilities`):
    /// `false` until capabilities are read, and in the demo (no server).
    pub fn wr_can(&self, op: &str) -> bool {
        self.is_live() && self.can(op)
    }

    /// Whether capabilities have been read, so `wr_can` is final.
    pub fn wr_ops_known(&self) -> bool {
        self.wr.ops.is_some()
    }
}

#[cfg(test)]
mod tests;
