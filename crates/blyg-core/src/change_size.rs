//! How much a text changed between two versions (blygger-studio 0.68's
//! `src/change-size.ts`, ported exactly): the measure behind Background's
//! `near-same` rule ("Small edits") and the pace-layered order's
//! maintenance versions (spec §13.2: decision #77's same hash, #83's
//! reader-local near-sameness).
//!
//! The measure is the **edit span**: the stretch between the longest common
//! prefix and the longest common suffix, counted in the longer text. It's an
//! upper bound on the characters changed, never an underestimate, so a rule
//! built on it errs towards "this changed". Lengths are UTF-16 code units,
//! as the studio's `charCodeAt` counts them, so both sides agree on every
//! text (an emoji is two).
//!
//! Burrow reads the studio's verdict for other people's posts (it keeps no
//! unpinned old version of them to measure, spec §8.4); the sample data
//! (`BLYGGER_FAKE`) works its reasons out with this.

/// Reader-local constants (#83): never on the wire, never advertised.
pub const NEAR_SAME_MAX_CHARS: usize = 40;
pub const NEAR_SAME_MAX_FRACTION: f64 = 0.02;

/// The edit span between `a` and `b`, in UTF-16 code units.
pub fn edit_span(a: &str, b: &str) -> usize {
    let a: Vec<u16> = a.encode_utf16().collect();
    let b: Vec<u16> = b.encode_utf16().collect();
    let max = a.len().min(b.len());
    let mut prefix = 0;
    while prefix < max && a[prefix] == b[prefix] {
        prefix += 1;
    }
    let mut suffix = 0;
    while suffix < max - prefix && a[a.len() - 1 - suffix] == b[b.len() - 1 - suffix] {
        suffix += 1;
    }
    a.len().max(b.len()) - prefix - suffix
}

/// Changed at most [`NEAR_SAME_MAX_CHARS`] characters and at most
/// [`NEAR_SAME_MAX_FRACTION`] of the text (studio `isNearSame`).
pub fn is_near_same(a: &str, b: &str) -> bool {
    let span = edit_span(a, b);
    let longest = a
        .encode_utf16()
        .count()
        .max(b.encode_utf16().count())
        .max(1);
    span <= NEAR_SAME_MAX_CHARS && span as f64 <= NEAR_SAME_MAX_FRACTION * longest as f64
}

#[cfg(test)]
#[path = "change_size_tests.rs"]
mod tests;
